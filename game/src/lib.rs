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
pub mod world;

use camera::Camera;
use gfx::{rgba, Canvas};
use input::Controls;
use light::Light;
use mobs::{MobCtx, Mobs};
use particles::Particles;
use player::Player;
use screens::Screens;
use shared::{GameApi, GameEvent, InputState, Key, ScreenId, ScreenInfo};
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
        let player = Player::new(&world);
        let (cx, cy) = player.center();
        let mut cam = Camera::new(cx, cy);
        cam.follow(cx, cy, 10.0, &world); // snap into bounds
        Game {
            world,
            sprites: Sprites::new(),
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
        let busy = self.screens.update(dt, &ctl, &mut self.cam, &self.world, &mut self.particles, &mut self.events);
        self.player.update(dt, &ctl, &mut self.cam, &mut self.world, &mut self.particles, busy);

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
        let mut c = Canvas::new(rgba_buf);
        self.sky.draw(&mut c, &self.cam, &self.world);
        render::draw_tiles(&mut c, &self.world, &self.sprites, &self.cam);
        self.screens.draw_world(&mut c, &self.cam);
        self.light.update(&self.world, &self.cam, self.sky.daylight(), &self.screens);
        self.light.apply(&mut c);
    }

    fn draw_fx(&mut self, rgba_buf: &mut [u8]) {
        let mut c = Canvas::new(rgba_buf);
        c.clear(rgba(0, 0, 0, 0));
        self.screens.draw_fx(&mut c, &self.cam, &self.world, self.hud_visible || self.sky.is_night());
        self.mobs.draw(&mut c, &self.cam, &self.light);
        let (px, py) = self.player.center();
        self.player.draw(&mut c, &self.cam, &self.sprites, self.light.level_at(px, py));
        self.particles.draw(&mut c, &self.cam);
        if self.hud_visible {
            self.player.draw_cursor(&mut c, &self.cam, &self.world);
            hud::draw(&mut c, self);
        }
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
}
