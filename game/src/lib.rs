//! Desktop Siege game. Pure Rust, no Smithay.
//!
//! Module ownership (see CLAUDE.md):
//!   B1: world, sprites, render, player, screens, particles, camera, input, gfx, lib (this file)
//!   B2: sky, light, mobs, hud

pub mod camera;
pub mod gfx;
pub mod hud;
pub mod input;
pub mod light;
pub mod mobs;
pub mod particles;
pub mod player;
pub mod render;
pub mod screens;
pub mod sky;
pub mod sprites;
pub mod ui;
pub mod world;

use camera::Camera;
use gfx::{rgba, Canvas};
use input::Controls;
use light::Light;
use mobs::{MobCtx, Mobs};
use particles::Particles;
use player::Player;
use screens::Screens;
use shared::{GameApi, GameEvent, InputState, Key, ScreenId, ScreenInfo, FAR_H, FAR_W, VIEW_H, VIEW_W};
use sky::Sky;
use sprites::Sprites;
use world::World;

pub const DEFAULT_SEED: u64 = 0xD35C_0001;

pub struct Game {
    pub world: World,
    pub sprites: Sprites,
    pub cam: Camera,
    pub player: Player,
    pub screens: Screens,
    pub particles: Particles,
    pub sky: Sky,
    pub light: Light,
    pub mobs: Mobs,
    pub controls: Controls,
    pub events: Vec<GameEvent>,
    pub time: f32,
    pub god: bool,
    pub hud_visible: bool,
    /// Smoothed simulation rate (for the debug FPS readout).
    pub fps: f32,
}

impl Game {
    pub fn new(seed: u64) -> Self {
        let world = World::generate(seed);
        let sprites = Sprites::new();
        ui::set_icons(sprites.app_icons.clone());
        let player = Player::new(&world);
        let (cx, cy) = player.center();
        let mut cam = Camera::new(cx, cy);
        cam.follow(cx, cy, 10.0, &world); // snap into bounds
        Game {
            world,
            sprites,
            cam,
            player,
            screens: Screens::new(),
            particles: Particles::new(),
            sky: Sky::new(),
            light: Light::new(),
            mobs: Mobs::new(),
            controls: Controls::default(),
            events: Vec::new(),
            time: 0.0,
            god: false,
            hud_visible: true,
            fps: 60.0,
        }
    }
}

impl GameApi for Game {
    fn update(&mut self, dt: f32, input: &InputState) {
        self.time += dt;
        self.controls.advance(input);
        let ctl = self.controls;

        // Demo knobs.
        if ctl.pressed(Key::Night) {
            self.sky.skip_to_dusk();
        }
        if ctl.pressed(Key::God) {
            self.god = !self.god;
            self.player.god = self.god;
        }
        if ctl.pressed(Key::ToggleHud) {
            self.hud_visible = !self.hud_visible;
        }

        self.sky.update(dt);
        let busy = self.screens.update(dt, &ctl, &mut self.cam, &mut self.world, &mut self.particles, &mut self.events, self.player.center());
        // Modal UI (chooser, zoom): the player stands still and can't mine/place.
        let player_ctl = if busy.frozen { ctl.frozen() } else { ctl };

        // World zoom: wheel / `-` `=` switch between the close and far view.
        // (While a modal UI is up the wheel belongs to it.)
        if !busy.frozen {
            let far = ctl.cur.wheel < 0.0 || ctl.pressed(Key::ZoomOut);
            let close = ctl.cur.wheel > 0.0 || ctl.pressed(Key::ZoomIn);
            if far && self.cam.vw < FAR_W as f32 {
                self.cam.set_view(FAR_W, FAR_H);
            } else if close && self.cam.vw > VIEW_W as f32 {
                self.cam.set_view(VIEW_W, VIEW_H);
            }
        }
        self.player.update(dt, &player_ctl, &mut self.cam, &mut self.world, &mut self.particles, busy.mouse || busy.frozen);
        self.player.break_screens(
            dt,
            &player_ctl,
            &mut self.cam,
            &mut self.screens,
            &mut self.particles,
            &mut self.events,
            busy.mouse || busy.frozen,
        );

        let mut ctx = MobCtx {
            world: &self.world,
            screens: &mut self.screens,
            player: &mut self.player,
            fx: &mut self.particles,
            cam: &mut self.cam,
            sky: &self.sky,
            events: &mut self.events,
        };
        self.mobs.update(dt, &mut ctx);

        self.particles.update(dt, &self.world);
        let (cx, cy) = self.player.center();
        self.cam.follow(cx, cy, dt, &self.world);
    }

    fn draw_world(&mut self, rgba_buf: &mut [u8]) {
        let (w, h) = self.view_size();
        let mut c = Canvas::new(rgba_buf, w, h);
        self.sky.draw(&mut c, &self.cam, &self.world);
        render::draw_tiles(&mut c, &self.world, &self.sprites, &self.cam);
        self.screens.draw_world(&mut c, &self.cam);
        self.light.update(&self.world, &self.cam, self.sky.daylight(), &self.screens);
        self.light.apply(&mut c);
        self.screens.draw_zoom_world(&mut c);
    }

    fn draw_fx(&mut self, rgba_buf: &mut [u8]) {
        let (w, h) = self.view_size();
        let mut c = Canvas::new(rgba_buf, w, h);
        c.clear(rgba(0, 0, 0, 0));
        self.screens.draw_fx(&mut c, &self.cam, &self.world, self.hud_visible || self.sky.is_night());
        self.mobs.draw(&mut c, &self.cam, &self.light);
        let (px, py) = self.player.center();
        self.player.draw(&mut c, &self.cam, &self.sprites, self.light.level_at(px, py));
        self.player.draw_smash(&mut c, &self.cam, self.time);
        self.particles.draw(&mut c, &self.cam);
        if self.hud_visible {
            self.player.draw_cursor(&mut c, &self.cam, &self.world);
            hud::draw(&mut c, self);
        }
        self.screens.draw_overlay(&mut c, &self.cam);
        self.screens.draw_zoom_fx(&mut c, &self.cam);
    }

    fn screens(&self) -> Vec<ScreenInfo> {
        self.screens.infos(&self.cam)
    }

    fn set_screen_occupied(&mut self, id: ScreenId, occupied: bool) {
        self.screens.set_occupied(id, occupied);
    }

    fn drain_events(&mut self) -> Vec<GameEvent> {
        std::mem::take(&mut self.events)
    }

    fn view_size(&self) -> (u32, u32) {
        (self.cam.vw as u32, self.cam.vh as u32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::LAYER_BYTES;

    fn tick(g: &mut Game, n: usize) {
        let mut input = InputState::default();
        input.game_focused = true;
        for _ in 0..n {
            g.update(1.0 / 60.0, &input);
        }
    }

    #[test]
    fn wheel_and_keys_switch_world_zoom() {
        let mut g = Game::new(DEFAULT_SEED);
        assert_eq!(g.view_size(), (VIEW_W, VIEW_H));
        let mut input = InputState::default();
        input.game_focused = true;
        input.wheel = -1.0;
        g.update(1.0 / 60.0, &input);
        assert_eq!(g.view_size(), (FAR_W, FAR_H));
        // Drawing at the far size uses the bigger buffers.
        let mut a = vec![0u8; (FAR_W * FAR_H * 4) as usize];
        g.draw_world(&mut a);
        g.draw_fx(&mut a);
        input.wheel = 0.0;
        input.set(Key::ZoomIn, true);
        g.update(1.0 / 60.0, &input);
        assert_eq!(g.view_size(), (VIEW_W, VIEW_H));
    }

    #[test]
    fn player_lands_on_ground() {
        let mut g = Game::new(DEFAULT_SEED);
        tick(&mut g, 120);
        assert!(g.player.on_ground);
    }

    #[test]
    fn screen_lifecycle_emits_events_once() {
        let mut g = Game::new(DEFAULT_SEED);
        let tx = g.world.w / 2 - 6;
        let ty = g.world.surface[tx as usize] - 14;
        let id = g.screens.spawn(tx, ty, 14, 9);
        tick(&mut g, 120);
        let ev = g.drain_events();
        assert_eq!(ev.iter().filter(|e| matches!(e, GameEvent::ScreenBuilt { id: i } if *i == id)).count(), 1);
        assert_eq!(g.screens().len(), 1);

        for _ in 0..30 {
            g.screens.damage(id, 0.05, (0.0, 0.0), &mut g.particles, &mut g.events);
        }
        tick(&mut g, 120);
        let ev = g.drain_events();
        assert_eq!(ev.iter().filter(|e| matches!(e, GameEvent::ScreenDestroyed { .. })).count(), 1);
        assert!(g.screens().is_empty());
    }

    #[test]
    fn draws_without_panicking_day_and_night() {
        let mut g = Game::new(DEFAULT_SEED);
        let mut a = vec![0u8; LAYER_BYTES];
        let mut b = vec![0u8; LAYER_BYTES];
        g.draw_world(&mut a);
        g.draw_fx(&mut b);
        assert!(a.chunks_exact(4).all(|p| p[3] == 255), "world layer must be opaque");
        g.sky.skip_to_dusk();
        tick(&mut g, 600);
        assert!(g.sky.is_night());
        g.draw_world(&mut a);
        g.draw_fx(&mut b);
    }

    // ---------- input-driven interaction tests ----------

    use crate::screens::{REASON_OVERLAP, REASON_SMALL, ZOOM_RECT};
    use crate::world::TILE;

    /// Drives the game through `InputState` like a front-end would.
    struct Driver {
        g: Game,
        input: InputState,
        events: Vec<GameEvent>,
    }

    impl Driver {
        fn new() -> Self {
            let mut input = InputState::default();
            input.game_focused = true;
            let mut d = Driver { g: Game::new(DEFAULT_SEED), input, events: Vec::new() };
            d.steps(60); // land
            d
        }
        fn step(&mut self) {
            self.g.update(1.0 / 60.0, &self.input);
            self.events.extend(self.g.drain_events());
        }
        fn steps(&mut self, n: usize) {
            for _ in 0..n {
                self.step();
            }
        }
        fn tap(&mut self, k: Key) {
            self.input.set(k, true);
            self.step();
            self.input.set(k, false);
            self.step();
        }
        fn mouse_at_tile(&mut self, tx: i32, ty: i32) {
            self.input.mouse_x = (tx * TILE + 8) as f32 - self.g.cam.x;
            self.input.mouse_y = (ty * TILE + 8) as f32 - self.g.cam.y;
        }
        /// B, then drag from tile `a` to tile `b` with LMB, release.
        fn build_drag(&mut self, a: (i32, i32), b: (i32, i32)) {
            if !self.g.screens.build_mode {
                self.tap(Key::Build);
            }
            assert!(self.g.screens.build_mode);
            self.mouse_at_tile(a.0, a.1);
            self.input.lmb = true;
            self.step();
            for i in 1..=10 {
                let f = i as f32 / 10.0;
                let x = a.0 + ((b.0 - a.0) as f32 * f).round() as i32;
                let y = a.1 + ((b.1 - a.1) as f32 * f).round() as i32;
                self.mouse_at_tile(x, y);
                self.step();
            }
            self.input.lmb = false;
            self.step();
        }
        fn player_tile(&self) -> i32 {
            (self.g.player.center().0 / TILE as f32) as i32
        }
        fn count(&self, f: impl Fn(&GameEvent) -> bool) -> usize {
            self.events.iter().filter(|e| f(e)).count()
        }
        /// A built, empty screen right next to the player, chooser open.
        fn screen_next_to_player(&mut self) -> ScreenId {
            let px = self.player_tile();
            let ty = self.g.world.surface[px as usize] - 9;
            let id = self.g.screens.spawn(px + 1, ty, 14, 8);
            self.steps(90);
            assert_eq!(self.g.screens.chooser.map(|c| c.id), Some(id), "chooser pops up after building");
            id
        }
        fn info(&self, id: ScreenId) -> ScreenInfo {
            self.g.screens().into_iter().find(|s| s.id == id).expect("screen info")
        }
    }

    /// A built screen next to the player with the chooser dismissed.
    fn online_screen(d: &mut Driver) -> ScreenId {
        let id = d.screen_next_to_player();
        d.tap(Key::Escape);
        d.steps(20); // close animation
        assert!(d.g.screens.chooser.is_none());
        id
    }

    #[test]
    fn smash_key_breaks_nearby_screen() {
        let mut d = Driver::new();
        let id = online_screen(&mut d);
        assert_eq!(d.g.screens.near_online(), Some(id));
        d.input.set(Key::Smash, true);
        d.steps((crate::player::SMASH_TIME * 60.0) as usize + 5);
        d.input.set(Key::Smash, false);
        d.steps(90);
        assert_eq!(d.count(|e| matches!(e, GameEvent::ScreenDestroyed { id: i } if *i == id)), 1);
        assert!(d.g.screens.find(id).is_none());
    }

    #[test]
    fn short_smash_press_does_nothing() {
        let mut d = Driver::new();
        let id = online_screen(&mut d);
        d.input.set(Key::Smash, true);
        d.steps(20);
        d.input.set(Key::Smash, false);
        d.steps(60);
        assert!(d.g.screens.find(id).is_some_and(|s| s.hp >= 1.0));
    }

    #[test]
    fn pickaxe_and_sword_wear_down_frames() {
        let mut d = Driver::new();
        let id = online_screen(&mut d);
        let s = d.g.screens.find(id).map(|s| (s.tx, s.ty, s.th)).expect("screen");
        // Pickaxe: hold LMB on the bottom-left frame block for one second.
        d.mouse_at_tile(s.0, s.1 + s.2 - 1);
        d.input.lmb = true;
        d.steps(60);
        d.input.lmb = false;
        d.steps(2);
        let hp1 = d.info(id).hp;
        assert!(hp1 < 0.8 && hp1 > 0.5, "pickaxe ~30%/s, got hp {hp1}");
        // Sword: swing at the air just above the frame's bottom edge.
        let px = d.player_tile();
        d.mouse_at_tile(px + 1, s.1 + s.2 - 3);
        d.input.lmb = true;
        d.steps(60);
        d.input.lmb = false;
        d.steps(2);
        let hp2 = d.info(id).hp;
        assert!(hp2 < hp1 - 0.05, "sword hits should land: {hp1} -> {hp2}");
    }

    #[test]
    fn drag_build_over_terrain_clears_it() {
        let mut d = Driver::new();
        let x0 = d.player_tile() + 2;
        let y0 = d.g.world.surface[x0 as usize] - 8;
        let (w, h) = (14, 12); // bottom rows are underground: >25% terrain
        let solid_before = (y0..y0 + h).flat_map(|y| (x0..x0 + w).map(move |x| (x, y))).filter(|&(x, y)| d.g.world.solid(x, y)).count();
        assert!(solid_before as f32 > (w * h) as f32 * 0.25, "test rect should be >25% terrain ({solid_before})");

        d.build_drag((x0, y0), (x0 + w - 1, y0 + h - 1));
        assert_eq!(d.g.screens.list.len(), 1, "build must not be rejected for terrain");
        let s = &d.g.screens.list[0];
        assert_eq!((s.tx, s.ty, s.tw, s.th), (x0, y0, w, h));
        assert!(!d.g.screens.build_mode);
        for y in y0..y0 + h {
            for x in x0..x0 + w {
                assert_eq!(d.g.world.get(x, y), crate::world::Tile::Air, "tile {x},{y} not cleared");
            }
        }
        d.steps(120);
        let id = d.g.screens.list[0].id;
        assert_eq!(d.count(|e| matches!(e, GameEvent::ScreenBuilt { id: i } if *i == id)), 1);
        assert_eq!(d.g.screens.chooser.map(|c| c.id), Some(id));
    }

    #[test]
    fn too_small_drag_is_rejected_with_reason() {
        let mut d = Driver::new();
        let x0 = d.player_tile() + 2;
        let y0 = d.g.world.surface[x0 as usize] - 7;
        d.build_drag((x0, y0), (x0 + 3, y0 + 2));
        assert!(d.g.screens.list.is_empty());
        assert_eq!(d.g.screens.reject.map(|r| r.1), Some(REASON_SMALL));
        assert!(d.g.screens.build_mode, "stays in build mode for another try");
        // Second try (still in build mode) succeeds.
        d.build_drag((x0, y0), (x0 + 9, y0 + 6));
        assert_eq!(d.g.screens.list.len(), 1);
    }

    #[test]
    fn overlapping_drag_is_rejected_with_reason() {
        let mut d = Driver::new();
        let x0 = d.player_tile() + 2;
        let y0 = d.g.world.surface[x0 as usize] - 10;
        d.g.screens.spawn(x0, y0, 10, 7);
        d.build_drag((x0 + 5, y0 + 3), (x0 + 16, y0 + 10));
        assert_eq!(d.g.screens.list.len(), 1);
        assert_eq!(d.g.screens.reject.map(|r| r.1), Some(REASON_OVERLAP));
    }

    #[test]
    fn oversized_drag_stays_anchored_at_start() {
        let mut d = Driver::new();
        let ax = d.player_tile() + 10;
        let ay = d.g.world.surface[ax as usize];
        // Drag up-left far past the max size: the rect grows from the anchor.
        d.build_drag((ax, ay), (ax - 60, ay - 40));
        let s = &d.g.screens.list[0];
        assert_eq!((s.tw, s.th), (crate::screens::MAX_W, crate::screens::MAX_H));
        assert_eq!((s.tx + s.tw - 1, s.ty + s.th - 1), (ax, ay));
    }

    #[test]
    fn chooser_launch_zoom_close_flow() {
        let mut d = Driver::new();
        let id = d.screen_next_to_player();

        // Chooser is modal: the player doesn't move.
        let x_before = d.g.player.x;
        d.input.set(Key::Right, true);
        d.steps(30);
        d.input.set(Key::Right, false);
        assert!((d.g.player.x - x_before).abs() < 0.5, "player moved while chooser open");

        // Pick app 2 with the number key.
        d.tap(Key::Hotbar2);
        assert_eq!(d.count(|e| *e == GameEvent::LaunchApp { id, app: 1 }), 1);
        assert!(d.g.screens.find(id).is_some_and(|s| s.launching.is_some()));
        d.steps(20);
        assert!(d.g.screens.chooser.is_none());
        d.g.set_screen_occupied(id, true);
        d.steps(30);

        // Zoom in: FocusScreen, then the rect grows to ZOOM_RECT.
        assert_eq!(d.g.screens.near, Some(id));
        d.tap(Key::Zoom);
        assert_eq!(d.count(|e| *e == GameEvent::FocusScreen { id }), 1);
        d.input.game_focused = false; // compositor gave the window focus
        d.steps(30);
        let info = d.info(id);
        assert_eq!(info.zoom, 1.0);
        assert_eq!(info.rect, ZOOM_RECT);
        let x_before = d.g.player.x;

        // Compositor hands focus back with Esc for one tick.
        d.input.game_focused = true;
        d.input.set(Key::Escape, true);
        d.step();
        d.input.set(Key::Escape, false);
        d.steps(30);
        let info = d.info(id);
        assert_eq!(info.zoom, 0.0);
        let s = d.g.screens.find(id).unwrap();
        assert_eq!((info.rect.w, info.rect.h), ((s.tw - 2) * TILE, (s.th - 2) * TILE));
        assert!((d.g.player.x - x_before).abs() < 0.5);

        // Close with X: event + empty at once.
        d.tap(Key::CloseApp);
        assert_eq!(d.count(|e| *e == GameEvent::CloseApp { id }), 1);
        assert!(!d.g.screens.find(id).unwrap().occupied);
        d.steps(40);

        // Reopen with E, cancel with Esc: nothing launches.
        d.tap(Key::Interact);
        assert_eq!(d.g.screens.chooser.map(|c| c.id), Some(id));
        d.tap(Key::Escape);
        d.steps(20);
        assert!(d.g.screens.chooser.is_none());
        assert_eq!(d.count(|e| matches!(e, GameEvent::LaunchApp { .. })), 1);
        assert!(d.g.screens.find(id).unwrap().is_empty());
    }

    #[test]
    fn chooser_click_selects_row_and_blocks_mining() {
        let mut d = Driver::new();
        let id = d.screen_next_to_player();
        // Hold LMB on the ground under the player: must not mine while the chooser is up.
        let (pcx, pcy) = d.g.player.center();
        let (gx, gy) = ((pcx / TILE as f32) as i32, (pcy / TILE as f32) as i32 + 1);
        let before = d.g.world.get(gx, gy);
        d.mouse_at_tile(gx, gy);
        d.input.lmb = true;
        d.steps(90);
        d.input.lmb = false;
        d.step();
        assert_eq!(d.g.world.get(gx, gy), before);
        // Click row 3 ("Text Editor").
        let (rx, ry) = d.g.screens.chooser_row_center(2, &d.g.cam).unwrap();
        d.input.mouse_x = rx;
        d.input.mouse_y = ry;
        d.step();
        d.input.lmb = true;
        d.step();
        d.input.lmb = false;
        d.step();
        assert_eq!(d.count(|e| *e == GameEvent::LaunchApp { id, app: 2 }), 1);
        assert_eq!(d.count(|e| matches!(e, GameEvent::LaunchApp { .. })), 1);
    }

    #[test]
    fn focus_return_or_app_exit_zooms_out() {
        let mut d = Driver::new();
        let id = d.screen_next_to_player();
        d.tap(Key::Hotbar1);
        d.steps(20);
        d.g.set_screen_occupied(id, true);
        d.steps(30);
        d.tap(Key::Zoom);
        d.input.game_focused = false;
        d.steps(30);
        // Focus comes back without an Escape key: still zooms out.
        d.input.game_focused = true;
        d.steps(30);
        assert!(d.g.screens.zoom.is_none());
        // App exits on its own while zoomed: zoom out too.
        d.tap(Key::Zoom);
        d.steps(10);
        d.g.set_screen_occupied(id, false);
        d.steps(30);
        assert!(d.g.screens.zoom.is_none());
        assert_eq!(d.count(|e| matches!(e, GameEvent::FocusScreen { .. })), 2);
    }
}
