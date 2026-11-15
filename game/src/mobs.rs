//! Night mobs: spawning, AI, attacking screens, combat with the player. Owner: person B2.
//!
//! STUB: no mobs yet. TODO(B2):
//! - spawn at night from the world edges / off-screen, scaling with `sky.nights`
//! - Crawler: walks + hops toward the nearest target in `ctx.screens.targets()`
//! - Bat: flies straight at a screen, swoops
//! - on reaching a frame: `ctx.screens.damage(id, amount, impact_point, ctx.fx, ctx.events)`
//! - player sword: `ctx.player.attack_box()` -> knockback, hit flash, death pop (`ctx.fx`)
//! - touching the player: `ctx.player.hurt(dmg, mob_x, ctx.fx)`
//! - retreat/burn up at dawn
//! Sprites: original pixel art (see sprites.rs for the ASCII-art approach).

use crate::camera::Camera;
use crate::gfx::Canvas;
use crate::light::Light;
use crate::particles::Particles;
use crate::player::Player;
use crate::screens::Screens;
use crate::sky::Sky;
use crate::world::World;
use shared::GameEvent;

pub struct MobCtx<'a> {
    pub world: &'a World,
    pub screens: &'a mut Screens,
    pub player: &'a mut Player,
    pub fx: &'a mut Particles,
    pub cam: &'a mut Camera,
    pub sky: &'a Sky,
    pub events: &'a mut Vec<GameEvent>,
}

#[derive(Clone, Debug)]
pub struct Mob {
    pub x: f32,
    pub y: f32,
    pub vx: f32,
    pub vy: f32,
    pub hp: i32,
}

#[derive(Default)]
pub struct Mobs {
    pub list: Vec<Mob>,
}

impl Mobs {
    pub fn new() -> Self {
        Mobs::default()
    }

    pub fn update(&mut self, _dt: f32, _ctx: &mut MobCtx) {}

    /// Draw on the FX layer (over windows).
    pub fn draw(&self, _c: &mut Canvas, _cam: &Camera, _light: &Light) {}
}
