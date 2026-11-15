//! Lighting: a coloured per-tile light map over the visible area, smoothed per
//! pixel and multiplied into the terrain (never the sky, never the windows).
//!
//! Sources: sky light flooding down open columns (white by day, cool blue
//! moonlight by night, warm at dusk), torches (warm, flickering), crystal ore
//! (cyan), screens (cyan frame glow, soft white from the window) and the
//! player's lantern at night. Light spreads with distance falloff through air
//! and dies quickly inside solid blocks, so caves are dark.

use crate::camera::Camera;
use crate::gfx::Canvas;
use crate::screens::Screens;
use crate::world::{Tile, World, TILE};

/// Light map margin around the view (tiles), so sources just off-screen still light it.
const MARGIN: i32 = 9;
/// Per-tile attenuation through air / solid blocks.
const FALL_AIR: f32 = 0.82;
const FALL_SOLID: f32 = 0.76;
/// Darkest it gets (deep caves): keeps silhouettes barely readable.
const MIN_LIGHT: f32 = 0.05;

pub struct Light {
    gx0: i32,
    gy0: i32,
    gw: usize,
    gh: usize,
    /// RGB light per cell, row-major.
    cells: Vec<[f32; 3]>,
    /// Overall sky brightness used for `level_at` fallbacks.
    sky: [f32; 3],
}

impl Light {
    pub fn new() -> Self {
        Light { gx0: 0, gy0: 0, gw: 0, gh: 0, cells: Vec::new(), sky: [1.0; 3] }
    }

    /// Colour of sky light for a given daylight (1 day .. 0 night) and dusk warmth (0..1).
    pub fn sky_color(daylight: f32, warmth: f32) -> [f32; 3] {
        let night = [0.20, 0.25, 0.42];
        let day = [1.0, 1.0, 1.0];
        let dusk = [1.0, 0.72, 0.55];
        let mut c = [0.0; 3];
        for k in 0..3 {
            let base = night[k] + (day[k] - night[k]) * daylight;
            c[k] = base + (dusk[k] * base.max(0.55) - base) * warmth * 0.8;
        }
        c
    }

    /// Recompute the light map for the visible area.
    #[allow(clippy::too_many_arguments)]
    pub fn update(&mut self, world: &World, cam: &Camera, daylight: f32, warmth: f32, screens: &Screens, player: (f32, f32), time: f32) {
        let (ox, oy) = cam.origin();
        self.gx0 = ox.div_euclid(TILE) - MARGIN;
        self.gy0 = oy.div_euclid(TILE) - MARGIN;
        self.gw = (cam.vw as i32 / TILE + 2 * MARGIN + 2) as usize;
        self.gh = (cam.vh as i32 / TILE + 2 * MARGIN + 2) as usize;
        let n = self.gw * self.gh;
        self.cells.clear();
        self.cells.resize(n, [0.0; 3]);
        self.sky = Self::sky_color(daylight, warmth);
        let (gw, gh, gx0, gy0) = (self.gw, self.gh, self.gx0, self.gy0);

        // Solidity grid (so propagation doesn't hit World::get repeatedly).
        let mut solid = vec![false; n];
        for gy in 0..gh {
            for gx in 0..gw {
                solid[gy * gw + gx] = world.solid(gx0 + gx as i32, gy0 + gy as i32);
            }
        }

        // Sky light: down each column until (and including) the first solid block.
        for gx in 0..gw {
            let wx = gx0 + gx as i32;
            // Find the first solid row in this column from the top of the world.
            let mut first_solid = world.h;
            for y in 0..world.h {
                if world.solid(wx, y) {
                    first_solid = y;
                    break;
                }
            }
            for gy in 0..gh {
                let wy = gy0 + gy as i32;
                if wy <= first_solid {
                    self.cells[gy * gw + gx] = self.sky;
                }
            }
        }

        // Point sources.
        let add = |cells: &mut Vec<[f32; 3]>, wx: i32, wy: i32, c: [f32; 3]| {
            let (gx, gy) = (wx - gx0, wy - gy0);
            if gx >= 0 && gy >= 0 && (gx as usize) < gw && (gy as usize) < gh {
                let cell = &mut cells[gy as usize * gw + gx as usize];
                for k in 0..3 {
                    cell[k] = cell[k].max(c[k]);
                }
            }
        };
        for gy in 0..gh {
            for gx in 0..gw {
                let (wx, wy) = (gx0 + gx as i32, gy0 + gy as i32);
                match world.get(wx, wy) {
                    Tile::Torch => {
                        // Gentle flicker, different per torch.
                        let ph = (wx * 7 + wy * 13) as f32;
                        let f = 1.12 + 0.07 * (time * 9.0 + ph).sin() + 0.04 * (time * 23.0 + ph * 0.7).sin();
                        add(&mut self.cells, wx, wy, [1.0 * f, 0.70 * f, 0.38 * f]);
                    }
                    Tile::Crystal => add(&mut self.cells, wx, wy, [0.30, 0.68, 0.85]),
                    _ => {}
                }
            }
        }
        for s in &screens.list {
            if !s.attackable() {
                continue;
            }
            let glow = if s.occupied { [0.42, 0.85, 1.0] } else { [0.95, 0.70, 0.30] };
            for ty in s.ty..s.ty + s.th {
                for tx in s.tx..s.tx + s.tw {
                    let border = tx == s.tx || tx == s.tx + s.tw - 1 || ty == s.ty || ty == s.ty + s.th - 1;
                    let c = if border { [glow[0] * 0.75, glow[1] * 0.75, glow[2] * 0.75] } else { [0.85, 0.9, 1.0] };
                    add(&mut self.cells, tx, ty, c);
                }
            }
        }
        // Player lantern: only noticeable when it's dark.
        let lantern = (1.0 - daylight) * 0.75;
        if lantern > 0.05 {
            let (px, py) = ((player.0 / TILE as f32).floor() as i32, (player.1 / TILE as f32).floor() as i32);
            add(&mut self.cells, px, py, [lantern, lantern * 0.86, lantern * 0.62]);
        }

        // Propagate: max-of-neighbour with falloff, four directional sweeps, twice.
        let fall: Vec<f32> = solid.iter().map(|&s| if s { FALL_SOLID } else { FALL_AIR }).collect();
        let cells = &mut self.cells;
        let mut relax = |i: usize, j: usize| {
            // light flows from j into i
            let f = fall[i];
            let src = cells[j];
            let dst = &mut cells[i];
            for k in 0..3 {
                let v = src[k] * f;
                if v > dst[k] {
                    dst[k] = v;
                }
            }
        };
        for _ in 0..2 {
            for gy in 0..gh {
                for gx in 1..gw {
                    relax(gy * gw + gx, gy * gw + gx - 1);
                }
                for gx in (0..gw - 1).rev() {
                    relax(gy * gw + gx, gy * gw + gx + 1);
                }
            }
            for gx in 0..gw {
                for gy in 1..gh {
                    relax(gy * gw + gx, (gy - 1) * gw + gx);
                }
                for gy in (0..gh - 1).rev() {
                    relax(gy * gw + gx, (gy + 1) * gw + gx);
                }
            }
        }
    }

    /// Multiply the terrain pixels by the smoothed light map. Pixels with
    /// alpha 0 (sky, drawn later behind the terrain) are left alone.
    pub fn apply(&self, c: &mut Canvas, cam: &Camera) {
        if self.gw < 2 || self.gh < 2 {
            return;
        }
        let (ox, oy) = cam.origin();
        let (w, h) = (c.w as usize, c.h as usize);
        let gw = self.gw;
        let t = TILE as i32;

        // Per pixel column: left cell index and 0..256 weight of the right cell.
        let mut col_i = vec![0usize; w];
        let mut col_f = vec![0u32; w];
        for px in 0..w {
            // Cell centres are at gx0*16 + 8 + k*16 in world px.
            let wx = ox + px as i32 - (self.gx0 * t + t / 2);
            let i = wx.div_euclid(t).clamp(0, gw as i32 - 2) as usize;
            let f = (wx - i as i32 * t).clamp(0, t) as u32 * 256 / t as u32;
            col_i[px] = i;
            col_f[px] = f;
        }

        // Light as fixed point (x256), allowing a little over-brightening near torches.
        let to_fx = |v: f32| (v.clamp(MIN_LIGHT, 1.35) * 256.0) as u32;
        let mut row = vec![[0u32; 3]; gw];
        for py in 0..h {
            let wy = oy + py as i32 - (self.gy0 * t + t / 2);
            let j = wy.div_euclid(t).clamp(0, self.gh as i32 - 2) as usize;
            let fy = (wy - j as i32 * t).clamp(0, t) as u32 * 256 / t as u32;
            let (r0, r1) = (&self.cells[j * gw..(j + 1) * gw], &self.cells[(j + 1) * gw..(j + 2) * gw]);
            for gx in 0..gw {
                for k in 0..3 {
                    let a = to_fx(r0[gx][k]);
                    let b = to_fx(r1[gx][k]);
                    row[gx][k] = (a * (256 - fy) + b * fy) >> 8;
                }
            }
            let line = &mut c.px[py * w * 4..(py + 1) * w * 4];
            for px in 0..w {
                let p = &mut line[px * 4..px * 4 + 4];
                if p[3] == 0 {
                    continue;
                }
                let (i, f) = (col_i[px], col_f[px]);
                for k in 0..3 {
                    let l = (row[i][k] * (256 - f) + row[i + 1][k] * f) >> 8;
                    p[k] = ((p[k] as u32 * l) >> 8).min(255) as u8;
                }
            }
        }
    }

    /// Brightness (0..1) at a world-px position, for shading sprites on the FX layer.
    pub fn level_at(&self, wx: f32, wy: f32) -> f32 {
        let (gx, gy) = ((wx / TILE as f32).floor() as i32 - self.gx0, (wy / TILE as f32).floor() as i32 - self.gy0);
        let c = if gx >= 0 && gy >= 0 && (gx as usize) < self.gw && (gy as usize) < self.gh {
            self.cells[gy as usize * self.gw + gx as usize]
        } else {
            self.sky
        };
        (0.3 * c[0] + 0.55 * c[1] + 0.15 * c[2]).clamp(0.0, 1.0)
    }

    /// RGB light at a world-px position (for tinting sprites).
    pub fn color_at(&self, wx: f32, wy: f32) -> [f32; 3] {
        let (gx, gy) = ((wx / TILE as f32).floor() as i32 - self.gx0, (wy / TILE as f32).floor() as i32 - self.gy0);
        if gx >= 0 && gy >= 0 && (gx as usize) < self.gw && (gy as usize) < self.gh {
            self.cells[gy as usize * self.gw + gx as usize]
        } else {
            self.sky
        }
    }
}

impl Default for Light {
    fn default() -> Self {
        Self::new()
    }
}
