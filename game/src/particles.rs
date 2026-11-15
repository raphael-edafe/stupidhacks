//! Particle system (debris, sparkles, explosions). Shared utility: anyone may call
//! the burst helpers; owner of the module itself: person B1.

use crate::camera::Camera;
use crate::gfx::{rgb, with_alpha, Canvas, Rgba};
use crate::world::World;

#[derive(Clone, Copy, Debug)]
pub struct Particle {
    pub x: f32,
    pub y: f32,
    pub vx: f32,
    pub vy: f32,
    pub life: f32,
    pub max_life: f32,
    pub color: Rgba,
    pub size: i32,
    pub gravity: f32,
    /// Additive glow halo radius (0 = none). Sparkles/embers.
    pub glow: i32,
    /// Bounce off tiles instead of passing through.
    pub collide: bool,
}

#[derive(Default)]
pub struct Particles {
    pub list: Vec<Particle>,
    rng: u32,
}

impl Particles {
    pub fn new() -> Self {
        Particles { list: Vec::with_capacity(2048), rng: 0x1234_5678 }
    }

    /// Cheap xorshift in [0,1).
    pub fn rand(&mut self) -> f32 {
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng = x;
        (x >> 8) as f32 / (1u32 << 24) as f32
    }

    pub fn spawn(&mut self, p: Particle) {
        if self.list.len() < 4000 {
            self.list.push(p);
        }
    }

    /// Chunky debris when a tile breaks or something gets hit.
    pub fn debris(&mut self, x: f32, y: f32, color: Rgba, n: usize) {
        for _ in 0..n {
            let a = self.rand() * std::f32::consts::TAU;
            let s = 30.0 + self.rand() * 90.0;
            let life = 0.4 + self.rand() * 0.5;
            let shade = 0.75 + self.rand() * 0.5;
            let c = crate::gfx::shade(color, shade);
            let size = 1 + (self.rand() * 2.0) as i32;
            self.spawn(Particle {
                x,
                y,
                vx: a.cos() * s,
                vy: a.sin() * s - 60.0,
                life,
                max_life: life,
                color: c,
                size,
                gravity: 420.0,
                glow: 0,
                collide: true,
            });
        }
    }

    /// Floating glowing sparkles (building, magic, power-up).
    pub fn sparkle(&mut self, x: f32, y: f32, color: Rgba, n: usize) {
        for _ in 0..n {
            let a = self.rand() * std::f32::consts::TAU;
            let s = 10.0 + self.rand() * 40.0;
            let life = 0.5 + self.rand() * 0.6;
            let ox = (self.rand() - 0.5) * 8.0;
            let oy = (self.rand() - 0.5) * 8.0;
            self.spawn(Particle {
                x: x + ox,
                y: y + oy,
                vx: a.cos() * s,
                vy: a.sin() * s - 20.0,
                life,
                max_life: life,
                color,
                size: 1,
                gravity: -10.0,
                glow: 3,
                collide: false,
            });
        }
    }

    /// Big explosion: fireball sparks + smoke + debris.
    pub fn explosion(&mut self, x: f32, y: f32, w: f32, h: f32) {
        let n = ((w * h) / 60.0).clamp(80.0, 500.0) as usize;
        let fire = [rgb(255, 240, 180), rgb(255, 190, 70), rgb(255, 110, 40), rgb(200, 60, 40)];
        for i in 0..n {
            let px = x + self.rand() * w;
            let py = y + self.rand() * h;
            let cx = x + w / 2.0;
            let cy = y + h / 2.0;
            let (dx, dy) = (px - cx, py - cy);
            let len = (dx * dx + dy * dy).sqrt().max(1.0);
            let s = 60.0 + self.rand() * 220.0;
            let life = 0.5 + self.rand() * 1.0;
            let c = fire[i % fire.len()];
            let size = 1 + (self.rand() * 3.0) as i32;
            self.spawn(Particle {
                x: px,
                y: py,
                vx: dx / len * s,
                vy: dy / len * s - 80.0,
                life,
                max_life: life,
                color: c,
                size,
                gravity: 260.0,
                glow: if i % 3 == 0 { 4 } else { 0 },
                collide: i % 2 == 0,
            });
        }
        // Smoke puffs.
        for _ in 0..n / 4 {
            let life = 1.0 + self.rand() * 1.2;
            let g = (60.0 + self.rand() * 50.0) as u8;
            let px = x + self.rand() * w;
            let py = y + self.rand() * h;
            let vx = (self.rand() - 0.5) * 40.0;
            let vy = -20.0 - self.rand() * 40.0;
            let size = 3 + (self.rand() * 3.0) as i32;
            self.spawn(Particle {
                x: px,
                y: py,
                vx,
                vy,
                life,
                max_life: life,
                color: [g, g, g + 10, 170],
                size,
                gravity: -15.0,
                glow: 0,
                collide: false,
            });
        }
    }

    pub fn update(&mut self, dt: f32, world: &World) {
        for p in self.list.iter_mut() {
            p.life -= dt;
            p.vy += p.gravity * dt;
            let nx = p.x + p.vx * dt;
            let ny = p.y + p.vy * dt;
            if p.collide {
                if world.solid_px(nx, p.y) {
                    p.vx *= -0.4;
                } else {
                    p.x = nx;
                }
                if world.solid_px(p.x, ny) {
                    p.vy *= -0.35;
                    p.vx *= 0.7;
                } else {
                    p.y = ny;
                }
            } else {
                p.x = nx;
                p.y = ny;
            }
        }
        self.list.retain(|p| p.life > 0.0);
    }

    pub fn draw(&self, c: &mut Canvas, cam: &Camera) {
        for p in &self.list {
            let (vx, vy) = cam.to_view(p.x, p.y);
            if vx < -8 || vy < -8 || vx > c.w + 8 || vy > c.h + 8 {
                continue;
            }
            let t = (p.life / p.max_life).clamp(0.0, 1.0);
            let a = (p.color[3] as f32 * (t * 1.5).min(1.0)) as u8;
            if p.glow > 0 {
                c.glow(vx, vy, p.glow, with_alpha(p.color, (a as f32 * 0.5) as u8));
            }
            c.fill_rect(vx - p.size / 2, vy - p.size / 2, p.size, p.size, with_alpha(p.color, a));
        }
    }
}
