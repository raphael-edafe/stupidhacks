//! Screens: block-built frames that host real app windows. Building mode,
//! lifecycle, damage, cracks, glitch, explosion. Owner: person B1.
//!
//! Mobs (person B2) interact only through `Screens::targets()` and `Screens::damage()`.

use crate::camera::Camera;
use crate::gfx::{lerp_color, rgb, rgba, shade, text_width, with_alpha, Canvas, Rgba};
use crate::input::Controls;
use crate::particles::Particles;
use crate::world::{hash2, World, TILE};
use shared::{GameEvent, Key, Rect, ScreenId, ScreenInfo};

pub const MIN_W: i32 = 8;
pub const MIN_H: i32 = 6;
pub const MAX_W: i32 = 36;
pub const MAX_H: i32 = 22;
const BUILD_RATE: f32 = 55.0; // frame tiles per second
const DYING_TIME: f32 = 0.9;

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

    fn perimeter(&self) -> Vec<(i32, i32)> {
        // Both sides rise from the bottom simultaneously, then the top closes in the middle.
        let mut left = Vec::new();
        let mut right = Vec::new();
        for i in 0..self.th {
            left.push((self.tx, self.ty + self.th - 1 - i));
            right.push((self.tx + self.tw - 1, self.ty + self.th - 1 - i));
        }
        let mut out = Vec::new();
        for i in 0..self.th as usize {
            out.push(left[i]);
            out.push(right[i]);
        }
        // Top row from both ends inward.
        let (mut a, mut b) = (self.tx + 1, self.tx + self.tw - 2);
        while a <= b {
            out.push((a, self.ty));
            if a != b {
                out.push((b, self.ty));
            }
            a += 1;
            b -= 1;
        }
        // Bottom row from both ends inward.
        let (mut a, mut b) = (self.tx + 1, self.tx + self.tw - 2);
        while a <= b {
            out.push((a, self.ty + self.th - 1));
            if a != b {
                out.push((b, self.ty + self.th - 1));
            }
            a += 1;
            b -= 1;
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
}

#[derive(Default)]
pub struct Screens {
    pub list: Vec<Screen>,
    next_id: ScreenId,
    pub build_mode: bool,
    drag_start: Option<(i32, i32)>,
    cursor: (i32, i32),
    reject_flash: f32,
    time: f32,
}

impl Screens {
    pub fn new() -> Self {
        Screens { next_id: 1, ..Default::default() }
    }

    /// Ghost rect being dragged, normalised: (tx, ty, tw, th).
    fn drag_rect(&self) -> Option<(i32, i32, i32, i32)> {
        let (ax, ay) = self.drag_start?;
        let (bx, by) = self.cursor;
        let (x0, x1) = (ax.min(bx), ax.max(bx));
        let (y0, y1) = (ay.min(by), ay.max(by));
        Some((x0, y0, (x1 - x0 + 1).min(MAX_W), (y1 - y0 + 1).min(MAX_H)))
    }

    fn valid(&self, r: (i32, i32, i32, i32), world: &World) -> bool {
        let (x, y, w, h) = r;
        if w < MIN_W || h < MIN_H {
            return false;
        }
        let overlaps = self.list.iter().any(|s| x < s.tx + s.tw && x + w > s.tx && y < s.ty + s.th && y + h > s.ty);
        if overlaps {
            return false;
        }
        let solid = (y..y + h).flat_map(|yy| (x..x + w).map(move |xx| (xx, yy))).filter(|&(xx, yy)| world.solid(xx, yy)).count();
        (solid as f32) < (w * h) as f32 * 0.25
    }

    /// Create a screen directly (demo scripting / tests). Returns its id.
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
            shake: 0.0,
            flash: 0.0,
            age: 0.0,
            impacts: Vec::new(),
        });
        id
    }

    /// Returns true if the build tool owns the mouse this frame.
    pub fn update(&mut self, dt: f32, ctl: &Controls, cam: &mut Camera, world: &World, fx: &mut Particles, events: &mut Vec<GameEvent>) -> bool {
        self.time += dt;
        self.reject_flash = (self.reject_flash - dt).max(0.0);

        // --- build tool ---
        if ctl.pressed(Key::Build) {
            self.build_mode = !self.build_mode;
            self.drag_start = None;
        }
        let (mx, my) = ctl.mouse();
        let (wx, wy) = cam.to_world(mx, my);
        self.cursor = ((wx / TILE as f32).floor() as i32, (wy / TILE as f32).floor() as i32);
        if self.build_mode {
            if ctl.rmb_pressed() {
                self.build_mode = false;
                self.drag_start = None;
            } else if ctl.lmb_pressed() {
                self.drag_start = Some(self.cursor);
            } else if ctl.lmb_released() {
                if let Some(r) = self.drag_rect() {
                    if self.valid(r, world) {
                        self.spawn(r.0, r.1, r.2, r.3);
                        self.build_mode = false;
                    } else {
                        self.reject_flash = 0.4;
                        cam.kick(0.1);
                    }
                }
                self.drag_start = None;
            }
        }

        // --- lifecycle ---
        let mut destroyed = Vec::new();
        for s in self.list.iter_mut() {
            s.age += dt;
            s.shake = (s.shake - dt).max(0.0);
            s.flash = (s.flash - dt * 4.0).max(0.0);
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

        self.build_mode
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
        if let Some(s) = self.list.iter_mut().find(|s| s.id == id) {
            s.occupied = occupied;
        }
    }

    pub fn infos(&self, cam: &Camera) -> Vec<ScreenInfo> {
        self.list
            .iter()
            .filter(|s| !matches!(s.state, State::Building { .. }))
            .map(|s| {
                let (x, y, w, h) = s.interior_px();
                let (vx, vy) = cam.to_view(x, y);
                let (ox, oy) = s.shake_offset();
                ScreenInfo {
                    id: s.id,
                    rect: Rect { x: vx + ox, y: vy + oy, w: w as i32, h: h as i32 },
                    hp: s.hp,
                    zoom: 0.0,
                    glitch: s.flash > 0.5 || matches!(s.state, State::Dying { .. }),
                }
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
            let (ix, iy, iw, ih) = s.interior_px();
            let (vx, vy) = cam.to_view(ix, iy);
            let (vx, vy) = (vx + ox, vy + oy);
            if building {
                // Holographic blueprint grid while constructing.
                let a = (40.0 + 20.0 * (self.time * 6.0).sin()) as u8;
                c.fill_rect(vx, vy, iw as i32, ih as i32, rgba(40, 160, 220, a / 2));
                for gx in (0..iw as i32).step_by(8) {
                    c.fill_rect(vx + gx, vy, 1, ih as i32, rgba(120, 220, 255, a));
                }
                for gy in (0..ih as i32).step_by(8) {
                    c.fill_rect(vx, vy + gy, iw as i32, 1, rgba(120, 220, 255, a));
                }
            } else {
                self.draw_panel(c, s, vx, vy, iw as i32, ih as i32);
            }

            // Frame tiles.
            for (i, &(tx, ty)) in per.iter().enumerate().take(placed) {
                let (fx, fy) = cam.to_view((tx * TILE) as f32, (ty * TILE) as f32);
                // Newly placed tiles pop in with a white flash.
                let fresh = if building { placed.saturating_sub(i) < 3 } else { false };
                draw_frame_tile(c, s, tx, ty, fx + ox, fy + oy, fresh, self.time);
            }

            // Name plate on the top bar.
            if !building {
                let label = format!("SCR-{:02}", s.id);
                let (fx, fy) = cam.to_view((s.tx * TILE) as f32, (s.ty * TILE) as f32);
                let w = text_width(&label, 1);
                let cx = fx + ox + s.tw * TILE / 2;
                c.fill_rect(cx - w / 2 - 3, fy + oy + 4, w + 6, 9, rgb(16, 20, 30));
                c.text(cx - w / 2, fy + oy + 6, &label, 1, self.power_color(s));
            }
        }
    }

    fn power_color(&self, s: &Screen) -> Rgba {
        if matches!(s.state, State::Dying { .. }) || s.hp < 0.3 {
            let on = (self.time * 8.0).sin() > 0.0;
            if on {
                rgb(255, 70, 70)
            } else {
                rgb(120, 30, 30)
            }
        } else if !s.occupied {
            lerp_color(rgb(255, 190, 60), rgb(255, 230, 140), (self.time * 4.0).sin() * 0.5 + 0.5)
        } else {
            rgb(90, 240, 255)
        }
    }

    /// Interior backing: a "booting" terminal look until a window arrives.
    fn draw_panel(&self, c: &mut Canvas, s: &Screen, x: i32, y: i32, w: i32, h: i32) {
        c.fill_rect(x, y, w, h, rgb(10, 14, 24));
        if s.occupied {
            return; // the real window covers it
        }
        // Scanlines + soft vignette glow.
        for yy in (0..h).step_by(2) {
            c.fill_rect(x, y + yy, w, 1, rgb(14, 22, 34));
        }
        let t = s.age;
        let lines = ["DESKTOP SIEGE OS", "MEM CHECK.... OK", "LINKING WINDOW..."];
        let shown = ((t * 3.0) as usize).min(lines.len());
        for (i, l) in lines.iter().take(shown).enumerate() {
            c.text(x + 6, y + 6 + i as i32 * 8, l, 1, rgb(90, 240, 160));
        }
        if (t * 3.0) as i32 % 2 == 0 {
            c.fill_rect(x + 6, y + 6 + shown as i32 * 8, 4, 5, rgb(90, 240, 160));
        }
        // Spinner bar.
        let bw = (w - 24).max(10);
        let bx = x + (w - bw) / 2;
        let by = y + h - 14;
        c.rect_outline(bx, by, bw, 6, rgb(60, 140, 120));
        let seg = bw / 4;
        let pos = ((t * 60.0) as i32) % (bw + seg) - seg;
        c.fill_rect((bx + pos).max(bx + 1), by + 1, seg.min(bx + bw - 1 - (bx + pos).max(bx + 1)), 4, rgb(90, 240, 160));
    }

    /// Cracks, hit flash, glitch bars, HP bar (FX layer, over the windows), plus the build ghost.
    pub fn draw_fx(&self, c: &mut Canvas, cam: &Camera, world: &World, show_bars: bool) {
        for s in &self.list {
            if matches!(s.state, State::Building { .. }) {
                continue;
            }
            let (ox, oy) = s.shake_offset();
            let (fx0, fy0, fw, _fh) = s.frame_px();
            let (vfx, vfy) = cam.to_view(fx0, fy0);
            let (vfx, vfy) = (vfx + ox, vfy + oy);
            let (ix, iy, iw, ih) = s.interior_px();
            let (vx, vy) = cam.to_view(ix, iy);
            let (vx, vy) = (vx + ox, vy + oy);
            let (iw, ih) = (iw as i32, ih as i32);

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

        // Build ghost.
        if self.build_mode {
            let (r, valid) = match self.drag_rect() {
                Some(r) => (r, self.valid(r, world)),
                None => ((self.cursor.0, self.cursor.1, 1, 1), true),
            };
            let (vx, vy) = cam.to_view((r.0 * TILE) as f32, (r.1 * TILE) as f32);
            let (w, h) = (r.2 * TILE, r.3 * TILE);
            let base = if valid && self.reject_flash <= 0.0 { rgb(110, 230, 255) } else { rgb(255, 90, 90) };
            c.fill_rect(vx, vy, w, h, with_alpha(base, 40));
            // Marching-ants outline.
            let phase = (self.time * 20.0) as i32;
            let mut dash = |x: i32, y: i32, i: i32| {
                if (i + phase) % 6 < 3 {
                    c.put(x, y, base);
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
            let label = if self.drag_start.is_some() {
                format!("{}x{}", r.2, r.3)
            } else {
                "DRAG TO BUILD A SCREEN".to_string()
            };
            c.text_shadow(vx + 2, vy - 9, &label, 1, base);
        }
    }
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
