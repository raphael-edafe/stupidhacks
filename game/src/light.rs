//! Lighting: a per-tile colour light map around the view. Owner: person B1.
//!
//! Sources:
//! - sky light (scaled by daylight, a little blue at night) down every open
//!   column until the first solid tile,
//! - torches (warm, reach about 8 tiles),
//! - crystal ore (faint cyan),
//! - online screens (a cool glow around their frames).
//!
//! Light spreads to neighbouring tiles, losing a little per air tile and a lot
//! per solid one (forward and backward sweeps, twice). `apply` shades the world
//! layer per pixel, interpolating between tile centres in fixed point.
//! `level_at` samples the same map for sprites on the FX layer, so mobs and the
//! player light up near torches too.

use crate::camera::Camera;
use crate::gfx::Canvas;
use crate::screens::Screens;
use crate::world::{Tile, World, TILE};

/// Sky brightness at midnight (1.0 at noon).
pub const AMBIENT_NIGHT: f32 = 0.28;
/// Extra tiles of light map around the view, so lights just off screen still count.
const MARGIN: i32 = 12;
/// Light lost per tile entered, through air and through solid blocks.
const FALL_AIR: f32 = 0.11;
const FALL_SOLID: f32 = 0.30;
const TORCH: [f32; 3] = [1.0, 0.80, 0.50];
const CRYSTAL: [f32; 3] = [0.30, 0.62, 0.75];
const SCREEN: [f32; 3] = [0.52, 0.68, 0.88];

pub struct Light {
    /// Sky level (0..1) from daylight.
    level: f32,
    /// Light map origin (tile) and size (tiles).
    gx: i32,
    gy: i32,
    gw: i32,
    gh: i32,
    /// World px of the view's top-left, as the tiles were drawn (`Camera::origin`).
    origin: (i32, i32),
    /// RGB light per cell, 0..1.
    map: Vec<[f32; 3]>,
    solid: Vec<bool>,
    /// Scratch for `apply`.
    row: Vec<[u32; 3]>,
}

impl Light {
    pub fn new() -> Self {
        Light { level: 1.0, gx: 0, gy: 0, gw: 0, gh: 0, origin: (0, 0), map: Vec::new(), solid: Vec::new(), row: Vec::new() }
    }

    /// Recompute light for the visible area. Called once per frame before drawing.
    pub fn update(&mut self, world: &World, cam: &Camera, daylight: f32, screens: &Screens) {
        self.level = AMBIENT_NIGHT + (1.0 - AMBIENT_NIGHT) * daylight;
        let night = 1.0 - daylight;
        let sky = [self.level * (1.0 - 0.20 * night), self.level * (1.0 - 0.08 * night), self.level];

        let (ox, oy) = cam.origin();
        self.origin = (ox, oy);
        self.gx = ox.div_euclid(TILE) - MARGIN;
        self.gy = oy.div_euclid(TILE) - MARGIN;
        self.gw = cam.vw as i32 / TILE + 2 * MARGIN + 2;
        self.gh = cam.vh as i32 / TILE + 2 * MARGIN + 2;
        let (gx, gy, gw, gh) = (self.gx, self.gy, self.gw, self.gh);
        let n = (gw * gh) as usize;
        self.map.clear();
        self.map.resize(n, [0.0; 3]);
        self.solid.clear();
        self.solid.resize(n, false);

        // Tiles: solidity, sky down open columns, torches and crystals.
        for i in 0..gw {
            let x = gx + i;
            let open_to = world.ground_y(x.clamp(0, world.w - 1));
            for j in 0..gh {
                let y = gy + j;
                let k = (j * gw + i) as usize;
                let t = world.get(x, y);
                self.solid[k] = t.solid();
                let mut l = if y < open_to { sky } else { [0.0; 3] };
                let src = match t {
                    Tile::Torch => Some(TORCH),
                    Tile::Crystal => Some(CRYSTAL),
                    _ => None,
                };
                if let Some(s) = src {
                    l = max3(l, s);
                }
                self.map[k] = l;
            }
        }

        // Online screens glow around their frames.
        for s in screens.list.iter().filter(|s| s.attackable()) {
            let mut emit = |x: i32, y: i32| {
                let (i, j) = (x - gx, y - gy);
                if i >= 0 && j >= 0 && i < gw && j < gh {
                    let k = (j * gw + i) as usize;
                    self.map[k] = max3(self.map[k], SCREEN);
                }
            };
            for x in s.tx..s.tx + s.tw {
                emit(x, s.ty);
                emit(x, s.ty + s.th - 1);
            }
            for y in s.ty..s.ty + s.th {
                emit(s.tx, y);
                emit(s.tx + s.tw - 1, y);
            }
        }

        // Spread: forward (from left/up) then backward (from right/down), twice.
        for _ in 0..2 {
            for j in 0..gh {
                for i in 0..gw {
                    let k = (j * gw + i) as usize;
                    let f = if self.solid[k] { FALL_SOLID } else { FALL_AIR };
                    let mut l = self.map[k];
                    if i > 0 {
                        l = max3(l, sub3(self.map[k - 1], f));
                    }
                    if j > 0 {
                        l = max3(l, sub3(self.map[k - gw as usize], f));
                    }
                    self.map[k] = l;
                }
            }
            for j in (0..gh).rev() {
                for i in (0..gw).rev() {
                    let k = (j * gw + i) as usize;
                    let f = if self.solid[k] { FALL_SOLID } else { FALL_AIR };
                    let mut l = self.map[k];
                    if i + 1 < gw {
                        l = max3(l, sub3(self.map[k + 1], f));
                    }
                    if j + 1 < gh {
                        l = max3(l, sub3(self.map[k + gw as usize], f));
                    }
                    self.map[k] = l;
                }
            }
        }
    }

    /// Shade the already-drawn world layer with the light map.
    pub fn apply(&mut self, c: &mut Canvas) {
        if self.map.is_empty() {
            return;
        }
        let (ox, oy) = self.origin;
        let (gx, gy, gw, gh) = (self.gx, self.gy, self.gw, self.gh);
        let half = TILE / 2;
        // Per column: grid column of the left sample and the blend weight (0..256).
        let cols: Vec<(usize, u32)> = (0..c.w)
            .map(|px| {
                let wx = ox + px - half;
                let i = (wx.div_euclid(TILE) - gx).clamp(0, gw - 2) as usize;
                (i, (wx.rem_euclid(TILE) * 256 / TILE) as u32)
            })
            .collect();
        let (i_lo, i_hi) = (cols.first().map_or(0, |c| c.0), cols.last().map_or(0, |c| c.0) + 1);
        self.row.resize(gw as usize, [0; 3]);
        let to_fix = |v: f32| (v.clamp(0.0, 1.0) * 256.0) as u32;
        for py in 0..c.h {
            let wy = oy + py - half;
            let j = (wy.div_euclid(TILE) - gy).clamp(0, gh - 2) as usize;
            let fy = (wy.rem_euclid(TILE) * 256 / TILE) as u32;
            // Vertical blend for the columns we need.
            for i in i_lo..=i_hi.min(gw as usize - 1) {
                let a = self.map[j * gw as usize + i];
                let b = self.map[(j + 1) * gw as usize + i];
                for ch in 0..3 {
                    self.row[i][ch] = (to_fix(a[ch]) * (256 - fy) + to_fix(b[ch]) * fy) >> 8;
                }
            }
            let line = &mut c.px[(py * c.w * 4) as usize..((py + 1) * c.w * 4) as usize];
            for (px, p) in line.chunks_exact_mut(4).enumerate() {
                let (i, fx) = cols[px];
                let (l, r) = (self.row[i], self.row[(i + 1).min(gw as usize - 1)]);
                for ch in 0..3 {
                    let k = (l[ch] * (256 - fx) + r[ch] * fx) >> 8;
                    p[ch] = ((p[ch] as u32 * k) >> 8).min(255) as u8;
                }
            }
        }
    }

    /// Brightness (0..1) at a world-px position, for shading sprites on the FX layer.
    pub fn level_at(&self, wx: f32, wy: f32) -> f32 {
        if self.map.is_empty() {
            return self.level;
        }
        let t = TILE as f32;
        let fx = (wx - t / 2.0) / t - self.gx as f32;
        let fy = (wy - t / 2.0) / t - self.gy as f32;
        if fx < 0.0 || fy < 0.0 || fx >= (self.gw - 1) as f32 || fy >= (self.gh - 1) as f32 {
            return self.level;
        }
        let (i, j) = (fx as usize, fy as usize);
        let (u, v) = (fx.fract(), fy.fract());
        let gw = self.gw as usize;
        let at = |i: usize, j: usize| {
            let l = self.map[j * gw + i];
            l[0].max(l[1]).max(l[2])
        };
        let top = at(i, j) * (1.0 - u) + at(i + 1, j) * u;
        let bot = at(i, j + 1) * (1.0 - u) + at(i + 1, j + 1) * u;
        top * (1.0 - v) + bot * v
    }
}

impl Default for Light {
    fn default() -> Self {
        Self::new()
    }
}

fn max3(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0].max(b[0]), a[1].max(b[1]), a[2].max(b[2])]
}

fn sub3(a: [f32; 3], f: f32) -> [f32; 3] {
    [a[0] - f, a[1] - f, a[2] - f]
}

#[cfg(test)]
mod tests {
    use crate::sky::DAY_LEN;
    use crate::world::{Tile, TILE};
    use crate::{Game, DEFAULT_SEED};
    use shared::{GameApi, InputState};

    #[test]
    fn torches_light_up_the_night() {
        let mut g = Game::new(DEFAULT_SEED);
        let mut input = InputState::default();
        input.game_focused = true;
        for _ in 0..60 {
            g.update(1.0 / 60.0, &input);
        }
        g.sky.clock = DAY_LEN + 10.0;
        // A torch in the air a few tiles above the player.
        let (px, py) = g.player.center();
        let (tx, ty) = ((px / TILE as f32) as i32 + 3, (py / TILE as f32) as i32 - 3);
        g.world.set(tx, ty, Tile::Torch);
        let (w, h) = g.view_size();
        let mut buf = vec![0u8; (w * h * 4) as usize];
        g.draw_world(&mut buf);
        let at_torch = g.light.level_at((tx * TILE + 8) as f32, (ty * TILE + 8) as f32);
        let ambient = g.light.level_at((tx * TILE + 8) as f32 + 400.0, (ty * TILE + 8) as f32 - 64.0);
        assert!(at_torch > 0.9, "torch tile should be bright, got {at_torch}");
        assert!(ambient < 0.45, "open sky far away should be night-dark, got {ambient}");
        assert!(at_torch > ambient + 0.4);
    }
}
