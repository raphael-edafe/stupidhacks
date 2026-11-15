//! Day/night clock and sky rendering: gradient with dusk/dawn warmth, sun and
//! moon arcs, twinkling stars, drifting clouds, three parallax backdrop layers.
//!
//! The sky is drawn into its own buffer and composited *behind* the terrain
//! (see lib.rs), so lighting never darkens it twice.

use crate::camera::Camera;
use crate::gfx::{lerp_color, rgb, shade, with_alpha, Canvas, Rgba, Sprite};
use crate::world::{fbm1, hash2, World, SURFACE, TILE};

/// About 5 minutes of day and 2 of night (N skips straight to dusk for demos).
pub const DAY_LEN: f32 = 300.0;
pub const NIGHT_LEN: f32 = 120.0;
/// Seconds of dusk/dawn fade at each end of the night.
pub const FADE: f32 = 12.0;

pub struct Sky {
    /// Seconds into the current day+night cycle. 0 = sunrise.
    pub clock: f32,
    /// How many nights have started (difficulty scaling).
    pub nights: u32,
    /// Free-running time (cloud drift, twinkle).
    pub time: f32,
}

/// Optional pxl backdrop art (filled in from `Sprites` when available).
#[derive(Default)]
pub struct SkyArt<'a> {
    pub parallax: [Option<&'a Sprite>; 3],
    pub clouds: &'a [Sprite],
    pub sun: Option<&'a Sprite>,
    pub moon: Option<&'a Sprite>,
}

impl Sky {
    pub fn new() -> Self {
        Sky { clock: 3.0, nights: 0, time: 0.0 }
    }

    pub fn update(&mut self, dt: f32) {
        let was_night = self.is_night();
        self.clock += dt;
        self.time += dt;
        if self.clock >= DAY_LEN + NIGHT_LEN {
            self.clock -= DAY_LEN + NIGHT_LEN;
        }
        if !was_night && self.is_night() {
            self.nights += 1;
        }
    }

    /// Demo knob: jump to a few seconds before nightfall (so dusk is visible).
    pub fn skip_to_dusk(&mut self) {
        if !self.is_night() {
            // A short golden hour, then the light fades into night.
            self.clock = (DAY_LEN - FADE - 4.0).max(self.clock);
        }
    }

    pub fn is_night(&self) -> bool {
        self.clock >= DAY_LEN
    }

    /// Seconds until night starts (0 during the night).
    pub fn until_night(&self) -> f32 {
        (DAY_LEN - self.clock).max(0.0)
    }

    /// 1.0 = full day, 0.0 = full night, smooth at dusk/dawn.
    pub fn daylight(&self) -> f32 {
        let t = self.clock;
        let k = if t < DAY_LEN - FADE {
            1.0
        } else if t < DAY_LEN {
            (DAY_LEN - t) / FADE
        } else if t < DAY_LEN + NIGHT_LEN - FADE {
            0.0
        } else {
            1.0 - (DAY_LEN + NIGHT_LEN - t) / FADE
        };
        smooth(k)
    }

    /// 0..1 golden-hour warmth: rises before sunset, peaks at dusk, fades into
    /// the night; mirrored (shorter) at dawn.
    pub fn warmth(&self) -> f32 {
        let t = self.clock;
        let dusk = {
            let a = DAY_LEN - FADE - 18.0;
            let peak = DAY_LEN - FADE * 0.4;
            let end = DAY_LEN + 6.0;
            if t < a || t > end {
                0.0
            } else if t < peak {
                smooth((t - a) / (peak - a))
            } else {
                1.0 - smooth((t - peak) / (end - peak))
            }
        };
        let cycle = DAY_LEN + NIGHT_LEN;
        let dawn = {
            // around clock = cycle (== 0)
            let d = if t > cycle / 2.0 { t - cycle } else { t };
            let x = 1.0 - ((d + 2.0).abs() / 10.0).min(1.0);
            smooth(x)
        };
        dusk.max(dawn * 0.8)
    }

    /// Screen y of the horizon (where the sky meets the world's average surface).
    fn horizon_y(cam: &Camera) -> f32 {
        (SURFACE * TILE) as f32 - cam.y
    }

    pub fn draw(&self, c: &mut Canvas, cam: &Camera, world: &World, art: &SkyArt) {
        let d = self.daylight();
        let w = self.warmth();
        let (vw, vh) = (c.w, c.h);
        let horizon = Self::horizon_y(cam);

        // --- gradient ---
        let top = lerp_color(lerp_color(rgb(8, 10, 32), rgb(64, 132, 222), d), rgb(62, 54, 128), w * 0.55);
        let mid = lerp_color(lerp_color(rgb(20, 24, 62), rgb(120, 178, 240), d), rgb(214, 112, 120), w * 0.7);
        let bot = lerp_color(lerp_color(rgb(34, 36, 80), rgb(190, 224, 252), d), rgb(255, 170, 100), w * 0.9);
        let hz = horizon.clamp(40.0, vh as f32 + 200.0);
        for y in 0..vh {
            let t = (y as f32 / hz).clamp(0.0, 1.0);
            let col = if t < 0.6 { lerp_color(top, mid, t / 0.6) } else { lerp_color(mid, bot, (t - 0.6) / 0.4) };
            // Ordered 2-tone dithering on the band edges for a smooth pixel-art ramp.
            c.fill_rect(0, y, vw, 1, col);
        }

        // --- stars ---
        let star_a = ((1.0 - d) * (1.0 - w * 0.6)).powf(1.4);
        if star_a > 0.01 {
            let px_off = cam.x * 0.03;
            for i in 0..170 {
                let sx = (hash2(51, i, 0) * 1400.0 - px_off).rem_euclid(1400.0) * vw as f32 / 1400.0;
                let sy = hash2(51, i, 1) * hz * 0.85;
                let tw = 0.55 + 0.45 * (self.time * (1.0 + hash2(51, i, 2) * 3.0) + i as f32).sin();
                let a = (star_a * tw * 255.0) as u8;
                let tint = match i % 7 {
                    0 => rgb(190, 210, 255),
                    1 => rgb(255, 236, 200),
                    _ => rgb(255, 255, 255),
                };
                let (x, y) = (sx as i32, sy as i32);
                c.blend(x, y, with_alpha(tint, a));
                if i % 23 == 0 {
                    // A few bright stars with a little cross.
                    let a2 = a / 2;
                    for (dx, dy) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
                        c.blend(x + dx, y + dy, with_alpha(tint, a2));
                    }
                }
            }
        }

        // --- sun & moon ---
        let cycle = DAY_LEN + NIGHT_LEN;
        if self.clock < DAY_LEN + 2.0 || self.clock > cycle - 2.0 {
            let t = ((self.clock + 2.0) / (DAY_LEN + 4.0)).rem_euclid(1.0);
            let (sx, sy) = arc(t, vw, hz);
            let warm = lerp_color(rgb(255, 246, 210), rgb(255, 150, 80), w);
            c.glow(sx, sy, 70, with_alpha(lerp_color(rgb(255, 240, 200), rgb(255, 140, 80), w), 70));
            c.glow(sx, sy, 26, with_alpha(warm, 120));
            match art.sun {
                Some(s) => c.sprite(s, sx - s.w / 2, sy - s.h / 2, false),
                None => disc(c, sx, sy, 9, warm, shade(warm, 0.92)),
            }
        }
        if self.clock > DAY_LEN - 2.0 {
            let t = ((self.clock - DAY_LEN + 2.0) / (NIGHT_LEN + 4.0)).clamp(0.0, 1.0);
            let (mx, my) = arc(t, vw, hz);
            let a = (1.0 - d).clamp(0.0, 1.0);
            c.glow(mx, my, 44, with_alpha(rgb(150, 180, 255), (50.0 * a) as u8));
            match art.moon {
                Some(s) => c.sprite(s, mx - s.w / 2, my - s.h / 2, false),
                None => {
                    disc(c, mx, my, 7, rgb(232, 236, 250), rgb(200, 206, 228));
                    // Craters.
                    c.fill_rect(mx - 3, my - 2, 2, 2, rgb(190, 196, 220));
                    c.fill_rect(mx + 2, my + 1, 2, 2, rgb(190, 196, 220));
                    c.put(mx, my + 3, rgb(196, 202, 224));
                }
            }
        }

        // --- clouds ---
        let cloud_tint = lerp_color(lerp_color(rgb(70, 76, 120), rgb(255, 255, 255), d), rgb(255, 190, 160), w * 0.7);
        let span = (vw + 400) as f32;
        for i in 0..7 {
            let speed = 4.0 + hash2(61, i, 0) * 6.0;
            let par = 0.06 + hash2(61, i, 1) * 0.06;
            let x = (hash2(61, i, 2) * span + self.time * speed - cam.x * par).rem_euclid(span) - 200.0;
            let y = 12.0 + hash2(61, i, 3) * (hz * 0.45).max(20.0);
            if let Some(s) = art.clouds.get(i as usize % art.clouds.len().max(1)) {
                c.sprite_tinted(s, x as i32, y as i32, cloud_tint);
            } else {
                cloud(c, x as i32, y as i32, 22 + (hash2(61, i, 4) * 30.0) as i32, cloud_tint, i);
            }
        }

        // --- parallax backdrop (far -> near) ---
        let layers = [
            // (parallax factor, day colour, height, noise scale, base offset)
            (0.12f32, rgb(140, 160, 204), 74.0f32, 90.0f32, 46.0f32),
            (0.25, rgb(96, 142, 136), 52.0, 60.0, 26.0),
            (0.42, rgb(62, 108, 84), 36.0, 34.0, 8.0),
        ];
        let haze = lerp_color(bot, mid, 0.2);
        let tint = lerp_color(lerp_color(rgb(44, 52, 96), rgb(255, 255, 255), d), rgb(255, 196, 170), w * 0.6);
        let mut bodies = [rgb(0, 0, 0); 3];
        let mut bases = [0.0f32; 3];
        for (li, &(f, col, _, _, base)) in layers.iter().enumerate() {
            // The layer's horizon moves with the camera at a fraction of world speed.
            bases[li] = horizon * (0.35 + f) + vh as f32 * (0.65 - f) * 0.62 - base * 0.3;
            let atmos = 0.55 - li as f32 * 0.22; // far layers fade into the haze
            bodies[li] = multiply(lerp_color(col, haze, atmos), tint);
        }

        // Art strips (when provided) are drawn layer by layer.
        if art.parallax.iter().all(|p| p.is_some()) {
            for (li, &(f, ..)) in layers.iter().enumerate() {
                if let Some(strip) = art.parallax[li] {
                    draw_strip(c, strip, cam.x * f, bases[li] as i32, tint, bodies[li]);
                }
            }
            return;
        }

        // Procedural ridges: compute each layer's ridge line per column, then
        // fill every column once (each layer only down to the next layer's top).
        let snow = multiply(rgb(236, 240, 252), tint);
        let rims = [shade(bodies[0], 1.18), shade(bodies[1], 1.18), shade(bodies[2], 1.18)];
        let seeds = [world.seed ^ 0x5A1, world.seed ^ (0x5A1 + 977), world.seed ^ (0x5A1 + 2 * 977)];
        let w_us = vw as usize;
        for x in 0..vw {
            let mut tops = [0i32; 3];
            let mut ns = [0f32; 3];
            for (li, &(f, _, amp, scale, _)) in layers.iter().enumerate() {
                let wx = cam.x * f + x as f32;
                let n = fbm1(seeds[li], wx / scale, 4) * 0.5 + 0.5;
                ns[li] = n;
                tops[li] = (bases[li] - amp * (0.35 + 0.65 * n)) as i32;
            }
            for li in 0..3 {
                let top = tops[li].max(0);
                // Visible until a nearer layer starts.
                let mut end = vh;
                for &t in &tops[li + 1..] {
                    end = end.min(t.max(0));
                }
                if top >= end {
                    continue;
                }
                let body = bodies[li];
                for y in top..end {
                    let i = (y as usize * w_us + x as usize) * 4;
                    c.px[i..i + 4].copy_from_slice(&body);
                }
                if tops[li] >= 0 {
                    c.put(x, tops[li], rims[li]);
                    if li == 0 && ns[0] > 0.78 {
                        // Snow caps on the tallest far peaks.
                        let cap = (((ns[0] - 0.78) * 40.0) as i32).min(6).min(end - tops[0]);
                        c.fill_rect(x, tops[0], 1, cap, snow);
                    }
                    if li == 2 && hash2(seeds[2], x + (cam.x * 0.42) as i32 * 3, 7) > 0.9 {
                        // Treeline speckle on the nearest hills.
                        c.put(x, tops[2] - 1, rims[2]);
                    }
                }
            }
        }
    }
}

impl Default for Sky {
    fn default() -> Self {
        Self::new()
    }
}

fn smooth(k: f32) -> f32 {
    let k = k.clamp(0.0, 1.0);
    k * k * (3.0 - 2.0 * k)
}

/// Position on a sky arc for t in 0..1 (rises left, sets right).
fn arc(t: f32, vw: i32, horizon: f32) -> (i32, i32) {
    let x = vw as f32 * (0.08 + 0.84 * t);
    let peak = (horizon * 0.82).max(60.0);
    let y = horizon + 10.0 - (t * std::f32::consts::PI).sin() * peak;
    (x as i32, y as i32)
}

fn multiply(a: Rgba, b: Rgba) -> Rgba {
    [((a[0] as u32 * b[0] as u32) / 255) as u8, ((a[1] as u32 * b[1] as u32) / 255) as u8, ((a[2] as u32 * b[2] as u32) / 255) as u8, a[3]]
}

fn disc(c: &mut Canvas, cx: i32, cy: i32, r: i32, col: Rgba, edge: Rgba) {
    for y in -r..=r {
        for x in -r..=r {
            let d2 = x * x + y * y;
            if d2 <= r * r {
                c.put(cx + x, cy + y, if d2 > (r - 1) * (r - 1) { edge } else { col });
            }
        }
    }
}

/// Chunky procedural cloud (placeholder until pxl cloud art is wired).
fn cloud(c: &mut Canvas, x: i32, y: i32, w: i32, tint: Rgba, seed: i32) {
    let shadow = shade(tint, 0.86);
    let puffs = 4 + (seed % 3);
    for p in 0..puffs {
        let px = x + p * w / puffs + (hash2(71, seed, p) * 6.0) as i32;
        let r = 5 + (hash2(72, seed, p) * 7.0) as i32;
        let py = y - (hash2(73, seed, p) * 4.0) as i32;
        for yy in -r..=0 {
            for xx in -r..=r {
                if xx * xx + yy * yy <= r * r {
                    c.blend(px + xx, py + yy, with_alpha(tint, 235));
                }
            }
        }
    }
    // Flat, shaded underside.
    c.fill_rect(x - 4, y + 1, w + 12, 2, with_alpha(shadow, 220));
}

/// Tile a horizontally seamless strip across the view, bottom at `base_y`,
/// tinted; fills below it with `fill` so nothing shows through.
fn draw_strip(c: &mut Canvas, s: &Sprite, scroll: f32, base_y: i32, tint: Rgba, fill: Rgba) {
    let top = base_y - s.h;
    let off = (scroll as i32).rem_euclid(s.w.max(1));
    let mut x = -off;
    while x < c.w {
        c.sprite_tinted(s, x, top, tint);
        x += s.w;
    }
    if base_y < c.h {
        // Continue the strip's own bottom colour below it.
        let b = s.get(s.w / 2, s.h - 1);
        let fill = if b[3] > 0 { multiply(b, tint) } else { fill };
        c.fill_rect(0, base_y, c.w, c.h - base_y, fill);
    }
}
