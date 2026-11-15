//! Screens: block-built frames that host real app windows. Building mode,
//! app chooser, proximity prompts (open / zoom / close), zoom, lifecycle,
//! damage, cracks, glitch, explosion. Owner: person B1.
//!
//! Mobs (person B2) interact only through `Screens::targets()` and `Screens::damage()`.
//!
//! Interaction flow (compositor side in brackets):
//!   B + drag -> frame builds -> `ScreenBuilt` -> "OPEN APP" chooser pops up
//!   pick (click / 1-5 / arrows+E) -> `LaunchApp` -> "LAUNCHING ..." panel
//!   [window maps -> set_screen_occupied(id, true)] -> CRT power-on
//!   near a screen: E opens the chooser (empty), Z zooms (occupied), X closes
//!   Z -> `FocusScreen`, rect grows to ZOOM_RECT over ZOOM_TIME
//!   Esc / focus handed back / screen gone -> zoom out
//!   X -> `CloseApp`, screen marked empty at once, CRT power-off

use crate::camera::Camera;
use crate::gfx::{lerp_color, rgb, rgba, shade, text_width, with_alpha, Canvas, Rgba};
use crate::input::Controls;
use crate::particles::Particles;
use crate::ui;
use crate::world::{hash2, Tile, World, TILE};
use shared::{GameEvent, Key, Rect, ScreenId, ScreenInfo, APPS, VIEW_H, VIEW_W};

pub const MIN_W: i32 = 8;
pub const MIN_H: i32 = 6;
pub const MAX_W: i32 = 36;
pub const MAX_H: i32 = 22;
const BUILD_RATE: f32 = 55.0; // frame tiles per second
const DYING_TIME: f32 = 0.9;

/// Seconds for a full zoom in (or out).
pub const ZOOM_TIME: f32 = 0.35;
/// Window rect when fully zoomed: 8 px margin, 16 px at the bottom for the hint bar.
pub const ZOOM_RECT: Rect = Rect { x: 8, y: 8, w: VIEW_W as i32 - 16, h: VIEW_H as i32 - 24 };
/// Prompts appear within this distance (world px) of a screen's frame.
pub const PROMPT_RANGE: f32 = 3.0 * TILE as f32;
const CHOOSER_POP: f32 = 0.26;
const CHOOSER_CLOSE: f32 = 0.14;
pub const CRT_OFF_TIME: f32 = 0.55;
pub const CRT_ON_TIME: f32 = 0.4;
/// Give up on a launch that never produced a window (screen goes back to empty).
const LAUNCH_TIMEOUT: f32 = 20.0;

pub const REASON_SMALL: &str = "TOO SMALL - MIN 8x6";
pub const REASON_OVERLAP: &str = "OVERLAPS SCREEN";

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum State {
    Building { t: f32 },
    Online,
    Dying { t: f32 },
}

pub struct Screen {
    pub id: ScreenId,
    /// Frame rect in tiles (includes the 1-tile border).
    pub tx: i32,
    pub ty: i32,
    pub tw: i32,
    pub th: i32,
    /// 0..1
    pub hp: f32,
    pub state: State,
    /// Whether a real window is attached.
    pub occupied: bool,
    /// App picked for this screen (running or launching).
    pub app: Option<usize>,
    /// Seconds since `LaunchApp` while waiting for the window.
    pub launching: Option<f32>,
    /// CRT effect in progress: (seconds, powering on?).
    crt: Option<(f32, bool)>,
    shake: f32,
    flash: f32,
    age: f32,
    /// Impact points (frame-relative px) that seed cracks.
    impacts: Vec<(f32, f32)>,
}

impl Screen {
    /// Frame rect in world px.
    pub fn frame_px(&self) -> (f32, f32, f32, f32) {
        ((self.tx * TILE) as f32, (self.ty * TILE) as f32, (self.tw * TILE) as f32, (self.th * TILE) as f32)
    }

    /// Interior rect in world px.
    pub fn interior_px(&self) -> (f32, f32, f32, f32) {
        let (x, y, w, h) = self.frame_px();
        let t = TILE as f32;
        (x + t, y + t, w - 2.0 * t, h - 2.0 * t)
    }

    pub fn attackable(&self) -> bool {
        self.state == State::Online
    }

    /// Online, no window and not waiting for one.
    pub fn is_empty(&self) -> bool {
        self.state == State::Online && !self.occupied && self.launching.is_none()
    }

    fn perimeter(&self) -> Vec<(i32, i32)> {
        // Both sides rise from the bottom simultaneously, then the top closes in the middle.
        let mut out = Vec::new();
        for i in 0..self.th {
            out.push((self.tx, self.ty + self.th - 1 - i));
            out.push((self.tx + self.tw - 1, self.ty + self.th - 1 - i));
        }
        for row in [self.ty, self.ty + self.th - 1] {
            let (mut a, mut b) = (self.tx + 1, self.tx + self.tw - 2);
            while a <= b {
                out.push((a, row));
                if a != b {
                    out.push((b, row));
                }
                a += 1;
                b -= 1;
            }
        }
        out
    }

    fn placed_count(&self) -> usize {
        match self.state {
            State::Building { t } => (t * BUILD_RATE) as usize,
            _ => usize::MAX,
        }
    }

    /// Shake offset in view px for this frame.
    fn shake_offset(&self) -> (i32, i32) {
        let t = self.age;
        match self.state {
            State::Dying { t: d } => {
                let s = 2.0 + d * 6.0;
                (((hash2(7, (t * 60.0) as i32, 0) - 0.5) * 2.0 * s) as i32, ((hash2(9, (t * 60.0) as i32, 1) - 0.5) * 2.0 * s) as i32)
            }
            _ if self.shake > 0.0 => {
                let s = self.shake / 0.3 * 3.0;
                (((t * 95.0).sin() * s) as i32, ((t * 71.0).cos() * s * 0.6) as i32)
            }
            _ => (0, 0),
        }
    }

    /// Interior rect in view px (camera + shake applied).
    fn interior_view(&self, cam: &Camera) -> (i32, i32, i32, i32) {
        let (x, y, w, h) = self.interior_px();
        let (vx, vy) = cam.to_view(x, y);
        let (ox, oy) = self.shake_offset();
        (vx + ox, vy + oy, w as i32, h as i32)
    }

    fn power_color(&self, time: f32) -> Rgba {
        if matches!(self.state, State::Building { .. }) {
            rgb(80, 140, 170)
        } else if matches!(self.state, State::Dying { .. }) || self.hp < 0.3 {
            if (time * 8.0).sin() > 0.0 {
                rgb(255, 70, 70)
            } else {
                rgb(110, 30, 30)
            }
        } else if !self.occupied {
            lerp_color(rgb(255, 180, 50), rgb(255, 230, 140), (time * 4.0).sin() * 0.5 + 0.5)
        } else {
            rgb(90, 240, 255)
        }
    }
}

/// The "OPEN APP" pop-up (one at a time).
#[derive(Clone, Copy, Debug)]
pub struct Chooser {
    pub id: ScreenId,
    t: f32,
    pub hover: usize,
    closing: Option<f32>,
}

#[derive(Clone, Copy, Debug)]
pub struct Zoom {
    pub id: ScreenId,
    /// Linear progress 0..1 (eased for the rect).
    v: f32,
    out: bool,
}

/// What `Screens::update` tells the player this tick.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Busy {
    /// The build tool / chooser owns the mouse: no mining, swinging or placing.
    pub mouse: bool,
    /// A modal UI is up (chooser, zoom): the player must not move either.
    pub frozen: bool,
}

#[derive(Default)]
pub struct Screens {
    pub list: Vec<Screen>,
    next_id: ScreenId,
    pub build_mode: bool,
    drag_start: Option<(i32, i32)>,
    cursor: (i32, i32),
    /// Last rejected drag and why (shown while `reject_flash` > 0).
    pub reject: Option<((i32, i32, i32, i32), &'static str)>,
    reject_flash: f32,
    time: f32,
    /// World size in tiles (drag rects are clipped to it; 0 = unknown).
    bounds: (i32, i32),
    pub chooser: Option<Chooser>,
    pub zoom: Option<Zoom>,
    /// Nearest screen in prompt range.
    pub near: Option<ScreenId>,
    prompt_id: Option<ScreenId>,
    prompt_a: f32,
    /// Ignore mouse buttons until both are released (after a UI click).
    swallow: bool,
}

impl Screens {
    pub fn new() -> Self {
        Screens { next_id: 1, ..Default::default() }
    }

    /// Ghost rect being dragged, normalised: (tx, ty, tw, th). It grows from the
    /// drag anchor towards the cursor (capped at MAX_W x MAX_H) and is clipped
    /// to the world, so the ghost always starts where the drag started.
    fn drag_rect(&self) -> Option<(i32, i32, i32, i32)> {
        let (ax, ay) = self.drag_start?;
        let (bx, by) = self.cursor;
        let axis = |a: i32, b: i32, max: i32| {
            let len = ((b - a).abs() + 1).min(max);
            if b >= a {
                (a, len)
            } else {
                (a - len + 1, len)
            }
        };
        let (mut x, mut w) = axis(ax, bx, MAX_W);
        let (mut y, mut h) = axis(ay, by, MAX_H);
        if self.bounds.0 > 0 {
            // Clip to the world; keep the bottom bedrock row out.
            let (x1, y1) = ((x + w).min(self.bounds.0), (y + h).min(self.bounds.1 - 1));
            x = x.max(0);
            y = y.max(0);
            w = (x1 - x).max(0);
            h = (y1 - y).max(0);
        }
        Some((x, y, w, h))
    }

    /// Ok(number of terrain tiles that will be cleared) or the rejection reason.
    pub fn check(&self, r: (i32, i32, i32, i32), world: &World) -> Result<usize, &'static str> {
        let (x, y, w, h) = r;
        if w < MIN_W || h < MIN_H {
            return Err(REASON_SMALL);
        }
        if self.list.iter().any(|s| x < s.tx + s.tw && x + w > s.tx && y < s.ty + s.th && y + h > s.ty) {
            return Err(REASON_OVERLAP);
        }
        Ok((y..y + h).flat_map(|yy| (x..x + w).map(move |xx| (xx, yy))).filter(|&(xx, yy)| clearable(world.get(xx, yy))).count())
    }

    /// Create a screen directly (demo scripting / tests). Returns its id.
    /// Does not touch terrain; the build tool uses `build`.
    pub fn spawn(&mut self, tx: i32, ty: i32, tw: i32, th: i32) -> ScreenId {
        let id = self.next_id;
        self.next_id += 1;
        self.list.push(Screen {
            id,
            tx,
            ty,
            tw: tw.clamp(MIN_W, MAX_W),
            th: th.clamp(MIN_H, MAX_H),
            hp: 1.0,
            state: State::Building { t: 0.0 },
            occupied: false,
            app: None,
            launching: None,
            crt: None,
            shake: 0.0,
            flash: 0.0,
            age: 0.0,
            impacts: Vec::new(),
        });
        id
    }

    /// Clear the terrain inside `r` (with debris, never bedrock) and start building there.
    pub fn build(&mut self, r: (i32, i32, i32, i32), world: &mut World, fx: &mut Particles, cam: &mut Camera) -> ScreenId {
        let (x, y, w, h) = r;
        let cleared: Vec<(i32, i32, Tile)> = (y..y + h)
            .flat_map(|yy| (x..x + w).map(move |xx| (xx, yy)))
            .map(|(xx, yy)| (xx, yy, world.get(xx, yy)))
            .filter(|&(_, _, t)| clearable(t))
            .collect();
        let per = if cleared.len() > 250 {
            1
        } else if cleared.len() > 80 {
            2
        } else {
            4
        };
        for &(xx, yy, t) in &cleared {
            world.set(xx, yy, Tile::Air);
            fx.debris((xx * TILE + 8) as f32, (yy * TILE + 8) as f32, t.color(), per);
        }
        if !cleared.is_empty() {
            cam.kick(0.3);
        }
        self.spawn(x, y, w, h)
    }

    pub fn find(&self, id: ScreenId) -> Option<&Screen> {
        self.list.iter().find(|s| s.id == id)
    }

    fn find_mut(&mut self, id: ScreenId) -> Option<&mut Screen> {
        self.list.iter_mut().find(|s| s.id == id)
    }

    /// Open the app chooser over a screen.
    pub fn open_chooser(&mut self, id: ScreenId) {
        if self.find(id).is_some_and(|s| s.is_empty()) {
            self.chooser = Some(Chooser { id, t: 0.0, hover: 0, closing: None });
            self.build_mode = false;
            self.drag_start = None;
        }
    }

    /// Player closes the app on a screen by hand.
    pub fn close_app(&mut self, id: ScreenId, events: &mut Vec<GameEvent>) {
        let Some(s) = self.find_mut(id) else { return };
        if !(s.occupied || s.launching.is_some()) {
            return;
        }
        let was_visible = s.occupied;
        s.occupied = false;
        s.launching = None;
        s.app = None;
        if was_visible {
            s.crt = Some((0.0, false));
        }
        events.push(GameEvent::CloseApp { id });
    }

    /// Current zoom of a screen, eased (0 = in the world, 1 = fills the view).
    pub fn zoom_of(&self, id: ScreenId) -> f32 {
        match self.zoom {
            Some(z) if z.id == id => ui::ease_in_out_cubic(z.v),
            _ => 0.0,
        }
    }

    /// Eased zoom of whichever screen is zoomed.
    pub fn zoom_amount(&self) -> f32 {
        self.zoom.map(|z| ui::ease_in_out_cubic(z.v)).unwrap_or(0.0)
    }

    /// The window rect of a screen this frame (view px), zoom applied.
    fn zoom_rect(&self, s: &Screen, cam: &Camera) -> Rect {
        let (x, y, w, h) = s.interior_view(cam);
        let z = self.zoom_of(s.id);
        if z <= 0.0 {
            return Rect { x, y, w, h };
        }
        let l = |a: i32, b: i32| (a as f32 + (b - a) as f32 * z).round() as i32;
        let (ox, oy) = if z >= 1.0 { s.shake_offset() } else { (0, 0) };
        Rect { x: l(x, ZOOM_RECT.x) + ox, y: l(y, ZOOM_RECT.y) + oy, w: l(w, ZOOM_RECT.w), h: l(h, ZOOM_RECT.h) }
    }

    /// Chooser panel rect (view px): centred on the screen's interior, kept in view.
    fn chooser_rect(&self, s: &Screen, cam: &Camera) -> (i32, i32, i32, i32) {
        let (x, y, w, h) = s.interior_view(cam);
        let (pw, ph) = (ui::PANEL_W, ui::panel_h());
        let px = (x + w / 2 - pw / 2).clamp(4, VIEW_W as i32 - 4 - pw);
        let py = (y + h / 2 - ph / 2).clamp(4, VIEW_H as i32 - 4 - ph);
        (px, py, pw, ph)
    }

    /// View-px centre of chooser row `i` (scripting / tests).
    pub fn chooser_row_center(&self, i: usize, cam: &Camera) -> Option<(f32, f32)> {
        let ch = self.chooser?;
        let s = self.find(ch.id)?;
        let (x, y, w, h) = ui::row_rect(self.chooser_rect(s, cam), i);
        Some(((x + w / 2) as f32, (y + h / 2) as f32))
    }

    fn nearest(&self, p: (f32, f32)) -> Option<ScreenId> {
        self.list
            .iter()
            .filter(|s| s.state == State::Online)
            .map(|s| {
                let (x, y, w, h) = s.frame_px();
                let dx = (x - p.0).max(p.0 - (x + w)).max(0.0);
                let dy = (y - p.1).max(p.1 - (y + h)).max(0.0);
                (s.id, dx.hypot(dy))
            })
            .filter(|&(_, d)| d <= PROMPT_RANGE)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(id, _)| id)
    }

    /// One tick: build tool, chooser, prompts, zoom, lifecycle.
    /// `player` = player centre in world px (for proximity prompts).
    #[allow(clippy::too_many_arguments)]
    pub fn update(
        &mut self,
        dt: f32,
        ctl: &Controls,
        cam: &mut Camera,
        world: &mut World,
        fx: &mut Particles,
        events: &mut Vec<GameEvent>,
        player: (f32, f32),
    ) -> Busy {
        self.time += dt;
        self.reject_flash = (self.reject_flash - dt).max(0.0);
        self.bounds = (world.w, world.h);
        if self.swallow && !ctl.lmb() && !ctl.rmb() {
            self.swallow = false;
        }
        let (mx, my) = ctl.mouse();
        let (wx, wy) = cam.to_world(mx, my);
        self.cursor = ((wx / TILE as f32).floor() as i32, (wy / TILE as f32).floor() as i32);
        self.near = self.nearest(player);

        self.update_zoom(dt, ctl);
        if self.zoom.is_some() {
            self.build_mode = false;
            self.drag_start = None;
        } else if self.chooser.is_some_and(|ch| ch.closing.is_none()) {
            self.update_chooser(ctl, cam, fx, events);
        } else if self.chooser.is_none() {
            if ctl.pressed(Key::Build) {
                self.build_mode = !self.build_mode;
                self.drag_start = None;
            }
            if self.build_mode {
                self.update_build_tool(ctl, cam, world, fx);
            } else if let Some(id) = self.near {
                self.update_prompts(id, ctl, fx, events);
            }
        }

        // Chooser animation.
        if let Some(ch) = &mut self.chooser {
            ch.t += dt;
            if let Some(c) = &mut ch.closing {
                *c += dt;
                if *c >= CHOOSER_CLOSE {
                    self.chooser = None;
                }
            }
        }

        // Prompt fade: fade out the old one before showing a new one.
        let want = if self.build_mode || self.chooser.is_some() || self.zoom.is_some() { None } else { self.near };
        let has_prompt = want.and_then(|id| self.find(id)).is_some_and(|s| s.crt.is_none() && (s.is_empty() || s.occupied));
        if want == self.prompt_id && has_prompt {
            self.prompt_a = (self.prompt_a + dt * 7.0).min(1.0);
        } else {
            self.prompt_a = (self.prompt_a - dt * 9.0).max(0.0);
            if self.prompt_a <= 0.0 {
                self.prompt_id = want;
            }
        }

        self.update_lifecycle(dt, cam, fx, events);

        Busy {
            mouse: self.build_mode || self.chooser.is_some() || self.swallow,
            frozen: self.chooser.is_some() || self.zoom.is_some(),
        }
    }

    fn update_zoom(&mut self, dt: f32, ctl: &Controls) {
        let Some(mut z) = self.zoom else { return };
        let gone = self.find(z.id).is_none_or(|s| !s.occupied || s.state != State::Online);
        if !z.out && (ctl.pressed(Key::Escape) || ctl.focus_regained() || gone) {
            z.out = true;
        }
        if z.out {
            z.v -= dt / ZOOM_TIME;
        } else {
            z.v = (z.v + dt / ZOOM_TIME).min(1.0);
        }
        self.zoom = if z.v <= 0.0 { None } else { Some(z) };
    }

    fn update_chooser(&mut self, ctl: &Controls, cam: &Camera, fx: &mut Particles, events: &mut Vec<GameEvent>) {
        let Some(ch) = self.chooser else { return };
        let Some(s) = self.find(ch.id).filter(|s| s.is_empty()) else {
            self.chooser = None;
            return;
        };
        let rect = self.chooser_rect(s, cam);
        let n = APPS.len();
        let (mx, my) = ctl.mouse();
        let mouse_row = (0..n).find(|&i| {
            let (x, y, w, h) = ui::row_rect(rect, i);
            mx >= x as f32 && my >= y as f32 && mx < (x + w) as f32 && my < (y + h + 2) as f32
        });
        let moved = ctl.cur.mouse_x != ctl.prev.mouse_x || ctl.cur.mouse_y != ctl.prev.mouse_y;
        let mut hover = ch.hover;
        if moved {
            if let Some(i) = mouse_row {
                hover = i;
            }
        }
        if ctl.pressed(Key::Up) {
            hover = (hover + n - 1) % n;
        }
        if ctl.pressed(Key::Down) {
            hover = (hover + 1) % n;
        }
        let mut pick = None;
        for (i, k) in [Key::Hotbar1, Key::Hotbar2, Key::Hotbar3, Key::Hotbar4, Key::Hotbar5].iter().enumerate() {
            if ctl.pressed(*k) && i < n {
                pick = Some(i);
            }
        }
        if ctl.pressed(Key::Interact) {
            pick = Some(hover);
        }
        if ctl.lmb_pressed() && !self.swallow {
            if let Some(i) = mouse_row {
                pick = Some(i);
            }
        }
        if let Some(c) = &mut self.chooser {
            c.hover = pick.unwrap_or(hover);
        }
        if let Some(app) = pick {
            let id = ch.id;
            if let Some(s) = self.find_mut(id) {
                s.app = Some(app);
                s.launching = Some(0.0);
                let (ix, iy, iw, ih) = s.interior_px();
                for k in 0..24 {
                    fx.sparkle(ix + iw * hash2(21, k, id as i32), iy + ih * hash2(22, k, id as i32), rgb(140, 250, 255), 1);
                }
            }
            events.push(GameEvent::LaunchApp { id, app });
            self.close_chooser();
        } else if ctl.pressed(Key::Escape) || ctl.rmb_pressed() {
            self.close_chooser();
        }
    }

    fn close_chooser(&mut self) {
        if let Some(c) = &mut self.chooser {
            c.closing = Some(0.0);
        }
        self.swallow = true;
    }

    fn update_build_tool(&mut self, ctl: &Controls, cam: &mut Camera, world: &mut World, fx: &mut Particles) {
        if ctl.pressed(Key::Escape) || (ctl.rmb_pressed() && self.drag_start.is_none()) {
            self.build_mode = false;
            self.drag_start = None;
            self.swallow = true;
            return;
        }
        if ctl.rmb_pressed() {
            // Cancel just the drag, stay in build mode.
            self.drag_start = None;
            self.swallow = true;
            return;
        }
        if self.drag_start.is_none() {
            // Level-triggered: also starts if LMB was already held when B was pressed.
            if ctl.lmb() && !self.swallow {
                self.drag_start = Some(self.cursor);
                self.reject_flash = 0.0;
            }
        } else if !ctl.lmb() {
            if let Some(r) = self.drag_rect() {
                match self.check(r, world) {
                    Ok(_) => {
                        self.build(r, world, fx, cam);
                        self.build_mode = false;
                        self.reject = None;
                    }
                    Err(reason) => {
                        self.reject = Some((r, reason));
                        self.reject_flash = 1.6;
                        cam.kick(0.12);
                    }
                }
            }
            self.drag_start = None;
        }
    }

    fn update_prompts(&mut self, id: ScreenId, ctl: &Controls, fx: &mut Particles, events: &mut Vec<GameEvent>) {
        let Some(s) = self.find(id) else { return };
        if s.is_empty() && ctl.pressed(Key::Interact) {
            self.open_chooser(id);
        } else if s.occupied && s.state == State::Online {
            if ctl.pressed(Key::Zoom) {
                self.zoom = Some(Zoom { id, v: 0.0, out: false });
                events.push(GameEvent::FocusScreen { id });
            } else if ctl.pressed(Key::CloseApp) {
                let (ix, iy, iw, ih) = s.interior_px();
                fx.sparkle(ix + iw / 2.0, iy + ih / 2.0, rgb(200, 245, 255), 10);
                self.close_app(id, events);
            }
        }
    }

    fn update_lifecycle(&mut self, dt: f32, cam: &mut Camera, fx: &mut Particles, events: &mut Vec<GameEvent>) {
        let mut destroyed = Vec::new();
        let mut built = Vec::new();
        for s in self.list.iter_mut() {
            s.age += dt;
            s.shake = (s.shake - dt).max(0.0);
            s.flash = (s.flash - dt * 4.0).max(0.0);
            if let Some((t, on)) = s.crt {
                let t = t + dt;
                let dur = if on { CRT_ON_TIME } else { CRT_OFF_TIME };
                s.crt = if t >= dur { None } else { Some((t, on)) };
            }
            if let Some(t) = s.launching {
                let t = t + dt;
                s.launching = if t > LAUNCH_TIMEOUT { None } else { Some(t) };
                if s.launching.is_none() {
                    s.app = None;
                }
            }
            match s.state {
                State::Building { t } => {
                    let before = s.placed_count();
                    let nt = t + dt;
                    s.state = State::Building { t: nt };
                    let after = s.placed_count();
                    let per = s.perimeter();
                    for &(px, py) in per.iter().take(after.min(per.len())).skip(before) {
                        let (cx, cy) = ((px * TILE + 8) as f32, (py * TILE + 8) as f32);
                        fx.sparkle(cx, cy, rgb(120, 240, 255), 2);
                        fx.debris(cx, cy, rgb(150, 160, 180), 1);
                    }
                    if after >= per.len() {
                        s.state = State::Online;
                        let (ix, iy, iw, ih) = s.interior_px();
                        for k in 0..40 {
                            let f = k as f32 / 40.0;
                            fx.sparkle(ix + f * iw, iy + ih * hash2(3, k, s.id as i32), rgb(140, 250, 255), 1);
                        }
                        cam.kick(0.25);
                        events.push(GameEvent::ScreenBuilt { id: s.id });
                        built.push(s.id);
                    }
                }
                State::Online => {}
                State::Dying { t } => {
                    let nt = t + dt;
                    s.state = State::Dying { t: nt };
                    if (nt * 20.0) as i32 != (t * 20.0) as i32 {
                        let (x, y, w, h) = s.frame_px();
                        let r = hash2(5, (nt * 100.0) as i32, s.id as i32);
                        fx.debris(x + r * w, y + h * hash2(6, (nt * 100.0) as i32, 0), rgb(120, 230, 255), 3);
                    }
                    if nt >= DYING_TIME {
                        destroyed.push(s.id);
                        let (x, y, w, h) = s.frame_px();
                        fx.explosion(x, y, w, h);
                        cam.kick(1.0);
                        events.push(GameEvent::ScreenDestroyed { id: s.id });
                    }
                }
            }
        }
        self.list.retain(|s| !destroyed.contains(&s.id));
        if self.chooser.is_some_and(|c| destroyed.contains(&c.id)) {
            self.chooser = None;
        }
        if self.zoom.is_some_and(|z| destroyed.contains(&z.id)) {
            self.zoom = None;
        }
        // Freshly built screens pop the chooser (unless the player is in a zoom).
        if self.zoom.is_none() {
            if let Some(&id) = built.last() {
                self.open_chooser(id);
            }
        }
    }

    /// Screens mobs may attack: (id, frame rect in world px).
    pub fn targets(&self) -> Vec<(ScreenId, (f32, f32, f32, f32))> {
        self.list.iter().filter(|s| s.attackable()).map(|s| (s.id, s.frame_px())).collect()
    }

    /// Damage a screen (amount in HP fraction, e.g. 0.05). `at` = world px of the impact.
    pub fn damage(&mut self, id: ScreenId, amount: f32, at: (f32, f32), fx: &mut Particles, events: &mut Vec<GameEvent>) {
        let Some(s) = self.list.iter_mut().find(|s| s.id == id) else { return };
        if !s.attackable() {
            return;
        }
        s.hp = (s.hp - amount).max(0.0);
        s.shake = 0.3;
        s.flash = 1.0;
        let (fx0, fy0, fw, fh) = s.frame_px();
        // Cracks live on the window, so seed them just inside the interior.
        let t = TILE as f32;
        let rel = ((at.0 - fx0).clamp(t + 2.0, fw - t - 2.0), (at.1 - fy0).clamp(t + 2.0, fh - t - 2.0));
        if s.impacts.len() < 24 {
            s.impacts.push(rel);
        }
        fx.debris(at.0, at.1, rgb(160, 230, 255), 5);
        events.push(GameEvent::ScreenDamaged { id, hp: s.hp });
        if s.hp <= 0.0 {
            s.state = State::Dying { t: 0.0 };
        }
    }

    pub fn set_occupied(&mut self, id: ScreenId, occupied: bool) {
        if let Some(s) = self.find_mut(id) {
            if occupied && !s.occupied {
                s.crt = Some((0.0, true));
                s.launching = None;
            } else if !occupied && s.occupied {
                s.crt = Some((0.0, false));
                s.app = None;
            }
            s.occupied = occupied;
        }
    }

    pub fn infos(&self, cam: &Camera) -> Vec<ScreenInfo> {
        self.list
            .iter()
            .filter(|s| !matches!(s.state, State::Building { .. }))
            .map(|s| ScreenInfo {
                id: s.id,
                rect: self.zoom_rect(s, cam),
                hp: s.hp,
                zoom: self.zoom_of(s.id),
                glitch: s.flash > 0.5 || matches!(s.state, State::Dying { .. }),
            })
            .collect()
    }

    // ---------------- drawing ----------------

    /// Frames + interior backing on the world layer (under the windows).
    pub fn draw_world(&self, c: &mut Canvas, cam: &Camera) {
        for s in &self.list {
            let (ox, oy) = s.shake_offset();
            let per = s.perimeter();
            let placed = s.placed_count().min(per.len());
            let building = matches!(s.state, State::Building { .. });

            // Interior.
            let (vx, vy, iw, ih) = s.interior_view(cam);
            if building {
                // Holographic blueprint grid while constructing.
                let a = (40.0 + 20.0 * (self.time * 6.0).sin()) as u8;
                c.fill_rect(vx, vy, iw, ih, rgba(40, 160, 220, a / 2));
                for gx in (0..iw).step_by(8) {
                    c.fill_rect(vx + gx, vy, 1, ih, rgba(120, 220, 255, a));
                }
                for gy in (0..ih).step_by(8) {
                    c.fill_rect(vx, vy + gy, iw, 1, rgba(120, 220, 255, a));
                }
            } else {
                c.fill_rect(vx, vy, iw, ih, rgb(10, 14, 24));
            }

            // Frame tiles.
            for (i, &(tx, ty)) in per.iter().enumerate().take(placed) {
                let (fx, fy) = cam.to_view((tx * TILE) as f32, (ty * TILE) as f32);
                // Newly placed tiles pop in with a white flash.
                let fresh = building && placed.saturating_sub(i) < 3;
                draw_frame_tile(c, s, tx, ty, fx + ox, fy + oy, fresh, self.time);
            }

            // Name plate on the top bar.
            if !building {
                let label = match s.app {
                    Some(a) if s.occupied => ui::app_label(a),
                    _ => format!("SCR-{:02}", s.id),
                };
                let (fx, fy) = cam.to_view((s.tx * TILE) as f32, (s.ty * TILE) as f32);
                let w = text_width(&label, 1);
                let cx = fx + ox + s.tw * TILE / 2;
                c.fill_rect(cx - w / 2 - 3, fy + oy + 4, w + 6, 9, rgb(16, 20, 30));
                c.text(cx - w / 2, fy + oy + 6, &label, 1, s.power_color(self.time));
            }
        }
    }

    /// Glowing interior content while there is no window (FX layer, so it
    /// stays bright at night): the empty "press E" state or the launch panel.
    fn draw_panel(&self, c: &mut Canvas, s: &Screen, x: i32, y: i32, w: i32, h: i32) {
        if s.occupied {
            return; // the real window covers it
        }
        let t = s.age;
        // Scanlines + a slow rolling scan band.
        for yy in (0..h).step_by(3) {
            c.fill_rect(x, y + yy, w, 1, rgba(120, 200, 255, 9));
        }
        let band = ((self.time * 30.0) as i32).rem_euclid(h + 20) - 10;
        let band_h = 6.min(h - band.max(0)).max(0);
        c.fill_rect(x, y + band.max(0), w, band_h, rgba(150, 220, 255, 12));

        let (cx, cy) = (x + w / 2, y + h / 2);
        if let Some(lt) = s.launching {
            let green = rgb(90, 240, 160);
            c.text(x + 5, y + 5, "DESKTOP SIEGE OS", 1, with_alpha(green, 150));
            let app = s.app.unwrap_or(0);
            let bob = ((lt * 4.0).sin() * 1.5).round() as i32;
            let top = (cy - 22).max(y + 13);
            c.glow(cx, top + 8, 14, rgba(90, 240, 160, 50));
            ui::draw_app_icon(c, app, cx - 8, top + bob);
            let dots = ".".repeat(1 + ((lt * 3.0) as usize % 3));
            c.text_centered(cx, top + 21, "LAUNCHING", 1, green);
            let label = ui::app_label(app);
            let nw = text_width(&label, 1) + 11;
            c.text_shadow(cx - nw / 2, top + 29, &format!("{label}{dots}"), 1, rgb(220, 255, 235));
            // Spinner bar.
            let bw = (w - 24).clamp(10, 120);
            let bx = cx - bw / 2;
            let by = (y + h - 10).max(top + 38);
            c.rect_outline(bx, by, bw, 5, rgb(60, 140, 120));
            let seg = bw / 4;
            let pos = ((lt * 70.0) as i32) % (bw + seg) - seg;
            let x0 = (bx + pos).max(bx + 1);
            let x1 = (bx + pos + seg).min(bx + bw - 1);
            if x1 > x0 {
                c.fill_rect(x0, by + 1, x1 - x0, 3, green);
            }
        } else {
            let amber = s.power_color(self.time);
            // Corner brackets.
            let (bl, inset) = (6, 4);
            let corners = [
                (x + inset, y + inset, 1, 1),
                (x + w - 1 - inset, y + inset, -1, 1),
                (x + inset, y + h - 1 - inset, 1, -1),
                (x + w - 1 - inset, y + h - 1 - inset, -1, -1),
            ];
            for (bx, by, dx, dy) in corners {
                c.fill_rect(bx.min(bx + dx * (bl - 1)), by, bl, 1, with_alpha(amber, 170));
                c.fill_rect(bx, by.min(by + dy * (bl - 1)), 1, bl, with_alpha(amber, 170));
            }
            let blink = (t * 2.0).sin() > -0.6;
            let sc = if w >= 150 && h >= 80 { 2 } else { 1 };
            let row1_w = text_width("PRESS", sc) + 4 * sc + 9 * sc;
            let rx = cx - row1_w / 2;
            let ry = cy - 10 * sc;
            c.text_shadow(rx, ry + 3 * sc, "PRESS", sc, rgb(255, 240, 200));
            if blink {
                ui::draw_keycap_s(c, rx + text_width("PRESS", sc) + 4 * sc, ry, "E", 1.0, sc);
            }
            c.text_centered(cx, ry + 15 * sc, "TO OPEN AN APP", sc, amber);
        }
    }

    /// Interior content, cracks, hit flash, glitch bars, HP bar, CRT effects
    /// (FX layer, over the windows), plus the build ghost.
    pub fn draw_fx(&self, c: &mut Canvas, cam: &Camera, world: &World, show_bars: bool) {
        for s in &self.list {
            if matches!(s.state, State::Building { .. }) {
                continue;
            }
            let (ox, oy) = s.shake_offset();
            let (fx0, fy0, fw, _fh) = s.frame_px();
            let (vfx, vfy) = cam.to_view(fx0, fy0);
            let (vfx, vfy) = (vfx + ox, vfy + oy);
            let (vx, vy, iw, ih) = s.interior_view(cam);

            if s.state == State::Online {
                self.draw_panel(c, s, vx, vy, iw, ih);
            }
            if let Some((t, on)) = s.crt {
                let p = t / if on { CRT_ON_TIME } else { CRT_OFF_TIME };
                ui::draw_crt(c, vx, vy, iw, ih, p, on);
            }

            // Cracks: deterministic jagged lines from each impact point.
            for (k, &(px, py)) in s.impacts.iter().enumerate() {
                let sx = vfx + px as i32;
                let sy = vfy + py as i32;
                let branches = 3 + (k % 3);
                for b in 0..branches {
                    let mut ang = hash2(s.id as u64 * 31 + k as u64, b as i32, 0) * std::f32::consts::TAU;
                    let (mut x, mut y) = (sx as f32, sy as f32);
                    let len = 14 + (hash2(s.id as u64, k as i32, b as i32) * 40.0) as i32;
                    for step in 0..len / 5 {
                        ang += (hash2(k as u64, b as i32, step) - 0.5) * 1.2;
                        let nx = x + ang.cos() * 5.0;
                        let ny = y + ang.sin() * 5.0;
                        // Stop at the window edge (clamping would draw straight lines along it).
                        let inside = |px: f32, py: f32| px >= vx as f32 && py >= vy as f32 && px < (vx + iw - 1) as f32 && py < (vy + ih - 1) as f32;
                        if !inside(nx, ny) {
                            break;
                        }
                        c.line(x as i32 + 1, y as i32 + 1, nx as i32 + 1, ny as i32 + 1, rgba(0, 0, 0, 140));
                        c.line(x as i32, y as i32, nx as i32, ny as i32, rgba(235, 250, 255, 220));
                        x = nx;
                        y = ny;
                    }
                }
            }

            // Hit flash.
            if s.flash > 0.0 {
                c.fill_rect(vx, vy, iw, ih, rgba(255, 255, 255, (s.flash * 90.0) as u8));
            }

            // Dying: chromatic glitch bars + static.
            if let State::Dying { t } = s.state {
                let frame = (t * 30.0) as i32;
                let bars = 4 + (t * 14.0) as i32;
                for b in 0..bars {
                    let by = vy + (hash2(11, frame, b) * ih as f32) as i32;
                    let bh = 2 + (hash2(12, frame, b) * 8.0) as i32;
                    let col = if b % 2 == 0 { rgba(255, 40, 200, 150) } else { rgba(40, 255, 240, 150) };
                    let shift = ((hash2(13, frame, b) - 0.5) * 30.0) as i32;
                    c.fill_rect(vx + shift.max(0), by, iw - shift.abs(), bh.min(vy + ih - by), col);
                }
                let density = (t / DYING_TIME * 0.5 * 255.0) as u8;
                for k in 0..(iw * ih / 30) {
                    let px = vx + (hash2(14, frame, k) * iw as f32) as i32;
                    let py = vy + (hash2(15, frame, k) * ih as f32) as i32;
                    let g = (hash2(16, frame, k) * 255.0) as u8;
                    c.blend(px, py, rgba(g, g, g, density));
                }
            }

            // HP bar over the frame.
            if show_bars && s.hp < 1.0 {
                let bw = (fw as i32 - 16).min(120);
                let bx = vfx + (fw as i32 - bw) / 2;
                let by = vfy - 8;
                c.fill_rect(bx - 1, by - 1, bw + 2, 5, rgba(10, 10, 16, 200));
                let col = lerp_color(rgb(240, 60, 60), rgb(90, 240, 120), s.hp);
                c.fill_rect(bx, by, (bw as f32 * s.hp) as i32, 3, col);
                c.fill_rect(bx, by, (bw as f32 * s.hp) as i32, 1, shade(col, 1.3));
            }
        }

        if self.build_mode || self.reject_flash > 0.0 {
            self.draw_ghost(c, cam, world);
        }
    }

    fn draw_ghost(&self, c: &mut Canvas, cam: &Camera, world: &World) {
        let (r, label, ok) = match self.drag_rect() {
            Some(r) => match self.check(r, world) {
                Ok(0) => (r, format!("{}x{}", r.2, r.3), true),
                Ok(_) => (r, format!("{}x{} - CLEARS TERRAIN", r.2, r.3), true),
                Err(reason) => (r, reason.to_string(), false),
            },
            None => match self.reject {
                Some((r, reason)) if self.reject_flash > 0.0 => (r, reason.to_string(), false),
                _ if self.build_mode => ((self.cursor.0, self.cursor.1, 1, 1), "DRAG TO BUILD A SCREEN".to_string(), true),
                _ => return,
            },
        };
        let (vx, vy) = cam.to_view((r.0 * TILE) as f32, (r.1 * TILE) as f32);
        let (w, h) = (r.2 * TILE, r.3 * TILE);
        // A rejected rect fades out after release.
        let fade = if self.drag_start.is_none() && !ok { (self.reject_flash / 0.4).min(1.0) } else { 1.0 };
        let base = if ok { rgb(110, 230, 255) } else { rgb(255, 90, 90) };
        c.fill_rect(vx, vy, w, h, with_alpha(base, (40.0 * fade) as u8));
        // Marching-ants outline.
        let phase = (self.time * 20.0) as i32;
        let ant = ui::fade(base, fade);
        let mut dash = |x: i32, y: i32, i: i32| {
            if (i + phase) % 6 < 3 {
                c.put(x, y, ant);
            }
        };
        for i in 0..w {
            dash(vx + i, vy, i);
            dash(vx + w - 1 - i, vy + h - 1, i);
        }
        for i in 0..h {
            dash(vx + w - 1, vy + i, i);
            dash(vx, vy + h - 1 - i, i);
        }
        let ly = if vy - 10 < 2 { vy + 3 } else { vy - 10 };
        let lw = text_width(&label, 1);
        let lx = vx.clamp(3, (VIEW_W as i32 - lw - 4).max(3));
        c.fill_rect(lx - 2, ly - 2, lw + 5, 9, rgba(10, 12, 20, (150.0 * fade) as u8));
        c.text_shadow(lx, ly, &label, 1, ui::fade(base, fade));
    }

    /// Prompts and the chooser pop-up: drawn last on the FX layer (over the player).
    pub fn draw_overlay(&self, c: &mut Canvas, cam: &Camera) {
        // Proximity prompt.
        if let Some(s) = self.prompt_id.and_then(|id| self.find(id)) {
            if self.prompt_a > 0.0 {
                let a = ui::ease_out_cubic(self.prompt_a);
                let (fx0, fy0, fw, _) = s.frame_px();
                let (vfx, vfy) = cam.to_view(fx0, fy0);
                let cx = vfx + fw as i32 / 2;
                let lift = if s.hp < 1.0 { 30 } else { 24 };
                let bob = ((self.time * 3.0).sin() * 1.2).round() as i32;
                let y = (vfy - lift + bob + ((1.0 - a) * 6.0) as i32).max(3);
                if s.occupied {
                    ui::draw_prompt(c, cx, y, &[("Z", "ZOOM"), ("X", "CLOSE")], rgb(90, 240, 255), a);
                } else if s.is_empty() {
                    ui::draw_prompt(c, cx, y, &[("E", "OPEN APP")], rgb(255, 200, 90), a);
                }
            }
        }

        // Chooser.
        if let Some(ch) = self.chooser {
            let Some(s) = self.find(ch.id) else { return };
            let rect = self.chooser_rect(s, cam);
            let p = ch.t / CHOOSER_POP;
            let (scale, alpha, content) = match ch.closing {
                Some(k) => {
                    let q = (k / CHOOSER_CLOSE).min(1.0);
                    (1.0 - 0.15 * q, 1.0 - q, 1.0 - q)
                }
                None => (0.4 + 0.6 * ui::ease_out_back(p), (ch.t / 0.1).min(1.0), ui::clamp01((p - 0.45) / 0.9)),
            };
            // Dim the screen behind the panel a touch so it reads as modal.
            let (vx, vy, iw, ih) = s.interior_view(cam);
            c.fill_rect(vx, vy, iw, ih, rgba(0, 0, 0, (90.0 * alpha) as u8));
            ui::draw_chooser(c, rect, scale, alpha, content, Some(ch.hover), self.time);
        }
    }

    /// End of the world layer: dim + vignette the world while zoomed.
    pub fn draw_zoom_world(&self, c: &mut Canvas) {
        let z = self.zoom_amount();
        if z <= 0.0 {
            return;
        }
        let (w, h) = (c.w as usize, c.h as usize);
        let (hw, hh) = (w as f32 / 2.0, h as f32 / 2.0);
        let col: Vec<f32> = (0..w).map(|x| ((x as f32 - hw) / hw).powi(2)).collect();
        for y in 0..h {
            let dy = ((y as f32 - hh) / hh).powi(2);
            let Some(row) = c.px.get_mut(y * w * 4..(y + 1) * w * 4) else { break };
            for (p, cx) in row.chunks_exact_mut(4).zip(col.iter()) {
                let v = ((cx + dy) * 0.5).min(1.0);
                let k = ((1.0 - z * (0.55 + 0.4 * v)).max(0.0) * 256.0) as u32;
                p[0] = ((p[0] as u32 * k) >> 8) as u8;
                p[1] = ((p[1] as u32 * k) >> 8) as u8;
                p[2] = ((p[2] as u32 * k) >> 8) as u8;
            }
        }
    }

    /// End of the FX layer while zoomed: fade out everything over the window
    /// (player, HUD, prompts), then draw the bezel and the "ESC" hint.
    pub fn draw_zoom_fx(&self, c: &mut Canvas, cam: &Camera) {
        let Some(zm) = self.zoom else { return };
        let Some(s) = self.find(zm.id) else { return };
        let z = self.zoom_amount();
        if z <= 0.0 {
            return;
        }
        let r = self.zoom_rect(s, cam);
        // Fade fast so the player/HUD are gone well before the window settles.
        let keep = ((1.0 - (z * 2.5).min(1.0)) * 256.0) as u32;
        let (x0, y0) = (r.x.max(0), r.y.max(0));
        let (x1, y1) = ((r.x + r.w).min(c.w), (r.y + r.h).min(c.h));
        for y in y0..y1 {
            for x in x0..x1 {
                let i = ((y * c.w + x) * 4 + 3) as usize;
                c.px[i] = ((c.px[i] as u32 * keep) >> 8) as u8;
            }
        }
        // Bezel around the window.
        let a = z;
        let accent = s.power_color(self.time);
        c.rect_outline(r.x - 3, r.y - 3, r.w + 6, r.h + 6, ui::fade(rgb(10, 12, 20), a));
        c.rect_outline(r.x - 2, r.y - 2, r.w + 4, r.h + 4, ui::fade(rgb(70, 80, 104), a));
        c.rect_outline(r.x - 1, r.y - 1, r.w + 2, r.h + 2, ui::fade(accent, a));
        // Hint bar in the bottom margin.
        let ha = ui::clamp01((z - 0.5) * 2.0);
        if ha > 0.0 {
            let y = r.y + r.h + 3;
            let label = s.app.map(ui::app_label).unwrap_or_else(|| format!("SCR-{:02}", s.id));
            c.text_shadow(r.x + 1, y + 3, &label, 1, ui::fade(accent, ha));
            let hint = "BACK TO WORLD";
            let total = text_width("ESC", 1) + 6 + 4 + text_width(hint, 1);
            let hx = r.x + r.w - total - 1;
            let kw = ui::draw_keycap(c, hx, y, "ESC", ha);
            c.text_shadow(hx + kw + 4, y + 3, hint, 1, ui::fade(rgb(230, 236, 250), ha));
        }
    }
}

/// Terrain the build tool clears (everything but air and bedrock).
fn clearable(t: Tile) -> bool {
    !matches!(t, Tile::Air | Tile::Bedrock)
}

/// One steel frame tile with bevels, rivets and a glowing inner power strip.
#[allow(clippy::too_many_arguments)]
fn draw_frame_tile(c: &mut Canvas, s: &Screen, tx: i32, ty: i32, x: i32, y: i32, fresh: bool, time: f32) {
    let left = tx == s.tx;
    let right = tx == s.tx + s.tw - 1;
    let top = ty == s.ty;
    let bottom = ty == s.ty + s.th - 1;
    let corner = (left || right) && (top || bottom);

    let body = rgb(64, 72, 92);
    let hi = rgb(118, 128, 152);
    let lo = rgb(34, 38, 52);
    let outline = rgb(16, 18, 26);
    let t = TILE;
    let noise = |px: i32, py: i32| hash2(0xF4A, tx * t + px, ty * t + py);

    for py in 0..t {
        for px in 0..t {
            let mut col = shade(body, 0.92 + noise(px, py) * 0.16);
            // Outer edges dark, bevel highlight on the outer top/left of the frame.
            let outer = (left && px == 0) || (right && px == t - 1) || (top && py == 0) || (bottom && py == t - 1);
            let bevel_hi = (top && py == 1) || (left && px == 1);
            let bevel_lo = (bottom && py == t - 2) || (right && px == t - 2);
            if outer {
                col = outline;
            } else if bevel_hi {
                col = hi;
            } else if bevel_lo {
                col = lo;
            }
            // Panel seams between tiles along the bar.
            if (top || bottom) && !corner && px == 0 {
                col = shade(col, 0.75);
            }
            if (left || right) && !corner && py == 0 {
                col = shade(col, 0.75);
            }
            c.put(x + px, y + py, col);
        }
    }
    // Inner power strip facing the interior.
    let power = if matches!(s.state, State::Building { .. }) {
        rgb(80, 140, 170)
    } else if matches!(s.state, State::Dying { .. }) || s.hp < 0.3 {
        if (time * 8.0).sin() > 0.0 {
            rgb(255, 70, 70)
        } else {
            rgb(110, 30, 30)
        }
    } else if !s.occupied {
        lerp_color(rgb(255, 180, 50), rgb(255, 230, 140), (time * 4.0).sin() * 0.5 + 0.5)
    } else {
        rgb(90, 240, 255)
    };
    let dim = shade(power, 0.55);
    if top && !corner {
        c.fill_rect(x, y + t - 3, t, 1, dim);
        c.fill_rect(x, y + t - 2, t, 2, power);
    }
    if bottom && !corner {
        c.fill_rect(x, y, t, 2, power);
        c.fill_rect(x, y + 2, t, 1, dim);
    }
    if left && !corner {
        c.fill_rect(x + t - 3, y, 1, t, dim);
        c.fill_rect(x + t - 2, y, 2, t, power);
    }
    if right && !corner {
        c.fill_rect(x, y, 2, t, power);
        c.fill_rect(x + 2, y, 1, t, dim);
    }
    // Rivets.
    if corner {
        c.fill_rect(x + 5, y + 5, 6, 6, outline);
        c.fill_rect(x + 6, y + 6, 4, 4, hi);
        c.fill_rect(x + 7, y + 7, 2, 2, rgb(200, 210, 230));
    } else {
        let (rx, ry) = if top || bottom { (7, if top { 5 } else { 8 }) } else { (if left { 5 } else { 8 }, 7) };
        c.put(x + rx, y + ry, hi);
        c.put(x + rx + 1, y + ry + 1, outline);
    }
    if fresh {
        c.fill_rect(x, y, t, t, rgba(220, 250, 255, 170));
    }
}
