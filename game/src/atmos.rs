//! Ambient life: fireflies at night, dust motes in daylight, falling leaves
//! near trees, and glow halos around torches. Cheap, drawn on the FX layer.

use crate::camera::Camera;
use crate::gfx::{rgb, with_alpha, Canvas};
use crate::screens::Screens;
use crate::world::{hash2, Tile, World, TILE};

#[derive(Clone, Copy)]
enum Kind {
    Firefly,
    Mote,
    Leaf,
}

#[derive(Clone, Copy)]
struct Mote {
    kind: Kind,
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    age: f32,
    life: f32,
    phase: f32,
}

pub struct Atmos {
    list: Vec<Mote>,
    rng: u32,
    spawn_acc: f32,
}

impl Atmos {
    pub fn new() -> Self {
        Atmos { list: Vec::with_capacity(128), rng: 0x9E37_79B9, spawn_acc: 0.0 }
    }

    fn rand(&mut self) -> f32 {
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng = x;
        (x >> 8) as f32 / (1u32 << 24) as f32
    }

    pub fn update(&mut self, dt: f32, world: &World, cam: &Camera, daylight: f32, screens: &Screens) {
        let night = 1.0 - daylight;
        self.spawn_acc += dt;
        while self.spawn_acc > 0.05 {
            self.spawn_acc -= 0.05;
            let count = |k: fn(&Kind) -> bool, l: &Vec<Mote>| l.iter().filter(|m| k(&m.kind)).count();
            let fireflies = count(|k| matches!(k, Kind::Firefly), &self.list);
            let motes = count(|k| matches!(k, Kind::Mote), &self.list);
            let leaves = count(|k| matches!(k, Kind::Leaf), &self.list);

            // Fireflies: hover just above the grass at night.
            if fireflies < (night * 46.0) as usize {
                let x = cam.x + self.rand() * cam.vw;
                let tx = (x / TILE as f32) as i32;
                let gy = world.ground_y(tx) as f32 * TILE as f32;
                let y = gy - 6.0 - self.rand() * 54.0;
                if y > cam.y && y < cam.y + cam.vh && !in_screen(screens, x, y) {
                    let life = 4.0 + self.rand() * 5.0;
                    let phase = self.rand() * 10.0;
                    self.list.push(Mote { kind: Kind::Firefly, x, y, vx: 0.0, vy: 0.0, age: 0.0, life, phase });
                }
            }
            // Dust motes drifting in the sunlight (above ground only).
            if motes < (daylight * 26.0) as usize {
                let x = cam.x + self.rand() * cam.vw;
                let y = cam.y + self.rand() * cam.vh;
                let tx = (x / TILE as f32) as i32;
                if y < world.ground_y(tx) as f32 * TILE as f32 - 8.0 && !in_screen(screens, x, y) {
                    let life = 5.0 + self.rand() * 6.0;
                    let phase = self.rand() * 10.0;
                    let vx = 3.0 + self.rand() * 5.0;
                    self.list.push(Mote { kind: Kind::Mote, x, y, vx, vy: -1.0, age: 0.0, life, phase });
                }
            }
            // Falling leaves from tree canopies in view.
            if leaves < 14 && self.rand() < 0.35 {
                if let Some((x, y)) = canopy_point(world, cam, self.rand(), self.rand()) {
                    let life = 6.0;
                    let phase = self.rand() * 10.0;
                    let vx = 6.0 + self.rand() * 10.0;
                    self.list.push(Mote { kind: Kind::Leaf, x, y, vx, vy: 10.0, age: 0.0, life, phase });
                }
            }
        }

        for m in self.list.iter_mut() {
            m.age += dt;
            let t = m.age + m.phase;
            match m.kind {
                Kind::Firefly => {
                    m.vx = (t * 0.9).sin() * 10.0 + (t * 2.3).cos() * 4.0;
                    m.vy = (t * 1.3).cos() * 7.0;
                }
                Kind::Mote => {
                    m.vy = (t * 0.7).sin() * 2.5 - 0.8;
                }
                Kind::Leaf => {
                    m.vx = 8.0 + (t * 2.2).sin() * 16.0;
                    m.vy = 14.0 + (t * 3.1).cos() * 6.0;
                }
            }
            m.x += m.vx * dt;
            m.y += m.vy * dt;
            if matches!(m.kind, Kind::Leaf) && world.solid_px(m.x, m.y) {
                m.age = m.life; // landed
            }
        }
        let (x0, x1, y0, y1) = (cam.x - 60.0, cam.x + cam.vw + 60.0, cam.y - 60.0, cam.y + cam.vh + 60.0);
        self.list.retain(|m| m.age < m.life && m.x > x0 && m.x < x1 && m.y > y0 && m.y < y1);
        // Fireflies leave at dawn, motes at dusk.
        if night < 0.2 {
            self.list.retain(|m| !matches!(m.kind, Kind::Firefly) || m.age < m.life - 1.0);
        }
    }

    pub fn draw(&self, c: &mut Canvas, cam: &Camera, time: f32) {
        for m in &self.list {
            let (x, y) = cam.to_view(m.x, m.y);
            let fade_in = (m.age / 0.8).min(1.0);
            let fade_out = ((m.life - m.age) / 1.0).clamp(0.0, 1.0);
            let a = fade_in * fade_out;
            match m.kind {
                Kind::Firefly => {
                    let blink = (0.5 + 0.5 * ((time + m.phase) * 2.6).sin()).powf(2.0);
                    let k = a * blink;
                    if k > 0.03 {
                        c.glow(x, y, 6, with_alpha(rgb(190, 255, 120), (90.0 * k) as u8));
                        c.blend(x, y, with_alpha(rgb(240, 255, 180), (255.0 * k) as u8));
                    }
                }
                Kind::Mote => {
                    c.blend(x, y, with_alpha(rgb(255, 250, 220), (70.0 * a) as u8));
                }
                Kind::Leaf => {
                    let flip = ((time * 4.0 + m.phase).sin() > 0.0) as i32;
                    let col = if (m.phase * 10.0) as i32 % 3 == 0 { rgb(214, 160, 60) } else { rgb(96, 170, 72) };
                    let col = with_alpha(col, (230.0 * a) as u8);
                    c.blend(x, y, col);
                    c.blend(x + 1 - 2 * flip, y + flip, col);
                }
            }
        }
    }

    /// Warm halos around visible torches (stronger in the dark).
    pub fn draw_torch_glows(&self, c: &mut Canvas, cam: &Camera, world: &World, daylight: f32, time: f32) {
        let strength = 0.35 + 0.65 * (1.0 - daylight);
        let (ox, oy) = cam.origin();
        let (tx0, ty0) = (ox.div_euclid(TILE) - 1, oy.div_euclid(TILE) - 1);
        let (tx1, ty1) = ((ox + c.w).div_euclid(TILE) + 1, (oy + c.h).div_euclid(TILE) + 1);
        for ty in ty0..=ty1 {
            for tx in tx0..=tx1 {
                if world.get(tx, ty) != Tile::Torch {
                    continue;
                }
                let ph = (tx * 7 + ty * 13) as f32;
                let f = 1.0 + 0.12 * (time * 9.0 + ph).sin();
                let (x, y) = cam.to_view((tx * TILE + 8) as f32, (ty * TILE + 4) as f32);
                c.glow(x, y, (22.0 * f) as i32, with_alpha(rgb(255, 170, 70), (60.0 * strength) as u8));
                c.glow(x, y, 7, with_alpha(rgb(255, 230, 160), (110.0 * strength) as u8));
            }
        }
    }
}

impl Default for Atmos {
    fn default() -> Self {
        Self::new()
    }
}

fn in_screen(screens: &Screens, x: f32, y: f32) -> bool {
    screens.list.iter().any(|s| {
        let (fx, fy, fw, fh) = s.frame_px();
        x >= fx && x < fx + fw && y >= fy && y < fy + fh
    })
}

/// A random point inside a tree canopy in view (leaf tiles), if any.
fn canopy_point(world: &World, cam: &Camera, r1: f32, r2: f32) -> Option<(f32, f32)> {
    // Sample a few columns; leaves sit in the top few rows of each tree.
    for k in 0..6 {
        let x = cam.x + ((r1 + k as f32 * 0.17).fract()) * cam.vw;
        let tx = (x / TILE as f32) as i32;
        let top = world.ground_y(tx);
        for ty in (top - 12).max(0)..top {
            let t = world.get(tx, ty);
            if t == Tile::Leaves || (t == Tile::Log && world.get(tx, ty - 1) != Tile::Log && hash2(3, tx, ty) < 0.9) {
                let y = (ty * TILE) as f32 - if t == Tile::Log { 16.0 + r2 * 24.0 } else { 0.0 };
                return Some((x, y + r2 * 8.0));
            }
        }
    }
    None
}
