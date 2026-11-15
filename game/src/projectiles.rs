//! Arrows from the bow: they fly with light gravity, stick into blocks (and fade
//! out a few seconds later), and hit zombies and bats (mobs call `hit`).

use crate::camera::Camera;
use crate::gfx::{rgb, with_alpha, Canvas, Rgba};
use crate::particles::Particles;
use crate::world::World;

const GRAVITY: f32 = 380.0;
/// Seconds an arrow stays stuck in a block before it's gone.
const STICK_TIME: f32 = 3.0;
const MAX_FLIGHT: f32 = 4.0;
/// Damage to a zombie or bat.
pub const ARROW_DAMAGE: i32 = 2;
/// Launch speed (px/s).
pub const ARROW_SPEED: f32 = 430.0;
const LEN: f32 = 9.0;

const SHAFT: Rgba = rgb(196, 150, 96);
const HEAD: Rgba = rgb(212, 220, 240);
const FLETCH: Rgba = rgb(236, 84, 70);

#[derive(Clone, Debug)]
pub struct Arrow {
    /// Tip, world px.
    pub x: f32,
    pub y: f32,
    pub vx: f32,
    pub vy: f32,
    angle: f32,
    t: f32,
    stuck: bool,
    pub dead: bool,
}

impl Arrow {
    pub fn flying(&self) -> bool {
        !self.stuck && !self.dead
    }
}

#[derive(Default)]
pub struct Projectiles {
    pub arrows: Vec<Arrow>,
}

impl Projectiles {
    pub fn new() -> Self {
        Self::default()
    }

    /// Loose an arrow with its tip at (x, y).
    pub fn fire(&mut self, x: f32, y: f32, vx: f32, vy: f32) {
        self.arrows.push(Arrow { x, y, vx, vy, angle: vy.atan2(vx), t: 0.0, stuck: false, dead: false });
    }

    pub fn update(&mut self, dt: f32, world: &World, fx: &mut Particles) {
        for a in self.arrows.iter_mut() {
            a.t += dt;
            if a.stuck {
                a.dead |= a.t > STICK_TIME;
                continue;
            }
            a.vy += GRAVITY * dt;
            // Small steps so a fast arrow can't skip through a block.
            let steps = ((a.vx.abs().max(a.vy.abs()) * dt) / 4.0).ceil().max(1.0) as i32;
            for _ in 0..steps {
                a.x += a.vx * dt / steps as f32;
                a.y += a.vy * dt / steps as f32;
                if world.solid_px(a.x, a.y) {
                    a.stuck = true;
                    a.t = 0.0;
                    fx.debris(a.x, a.y, SHAFT, 2);
                    break;
                }
            }
            if !a.stuck {
                a.angle = a.vy.atan2(a.vx);
            }
            if a.t > MAX_FLIGHT || a.y > world.height_px() || a.x < 0.0 || a.x > world.width_px() {
                a.dead = true;
            }
        }
        self.arrows.retain(|a| !a.dead);
    }

    /// If a flying arrow's tip is inside `rect`, use it up and return its horizontal direction.
    pub fn hit(&mut self, rect: (f32, f32, f32, f32)) -> Option<f32> {
        let (x, y, w, h) = rect;
        let a = self.arrows.iter_mut().find(|a| a.flying() && a.x >= x && a.x < x + w && a.y >= y && a.y < y + h)?;
        a.dead = true;
        Some(if a.vx >= 0.0 { 1.0 } else { -1.0 })
    }

    pub fn draw(&self, c: &mut Canvas, cam: &Camera) {
        for a in &self.arrows {
            let alpha = if a.stuck { ((STICK_TIME - a.t) / 0.5).clamp(0.0, 1.0) } else { 1.0 };
            let k = (alpha * 255.0) as u8;
            let (dx, dy) = (a.angle.cos(), a.angle.sin());
            let (x0, y0) = cam.to_view(a.x - dx * LEN, a.y - dy * LEN);
            let (x1, y1) = cam.to_view(a.x, a.y);
            if x1 < -16 || x1 > c.w + 16 || y1 < -16 || y1 > c.h + 16 {
                continue;
            }
            c.line(x0, y0, x1, y1, with_alpha(SHAFT, k));
            let (hx, hy) = cam.to_view(a.x - dx * 2.0, a.y - dy * 2.0);
            c.blend(hx, hy, with_alpha(HEAD, k));
            c.blend(x1, y1, with_alpha(HEAD, k));
            let (fx, fy) = cam.to_view(a.x - dx * (LEN - 1.0), a.y - dy * (LEN - 1.0));
            c.blend(fx + (-dy).round() as i32, fy + dx.round() as i32, with_alpha(FLETCH, k));
            c.blend(fx - (-dy).round() as i32, fy - dx.round() as i32, with_alpha(FLETCH, k));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::World;

    #[test]
    fn arrows_fall_and_stick_into_the_ground() {
        let world = World::generate(1);
        let mut p = Projectiles::new();
        let mut fx = Particles::new();
        let x = (world.w / 2 * 16) as f32;
        let y = (world.ground_y(world.w / 2) * 16) as f32 - 40.0;
        p.fire(x, y, 200.0, 0.0);
        for _ in 0..120 {
            p.update(1.0 / 60.0, &world, &mut fx);
        }
        assert_eq!(p.arrows.len(), 1);
        assert!(!p.arrows[0].flying(), "the arrow should have stuck in the ground");
        assert!(p.hit((x - 1000.0, y - 1000.0, 3000.0, 3000.0)).is_none(), "stuck arrows don't hit");
    }
}
