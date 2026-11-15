//! World rendering: terrain from pxl edge sets, back walls with ambient occlusion, grass caps,
//! trees (trunk tiles + canopy sprites) and swaying surface decorations. Owner: person B1.
//!
//! Draw order: back walls + tiles (row by row) -> decorations -> root flares -> grass caps and
//! side wraps (their blades overlap what stands on the grass) -> canopies.

use std::sync::OnceLock;
use std::time::Instant;

use crate::camera::Camera;
use crate::gfx::{Canvas, Sprite};
use crate::sprites::{
    Sprites, CANOPY_ANCHORS, EDGE_DOWN, EDGE_LEFT, EDGE_RIGHT, EDGE_UP, GRASS_CAP_TOP, GRASS_CAP_VARIANTS, ROOTS_W,
    TRUNK_VARIANTS, VARIANTS,
};
use crate::world::{hash2, Tile, World, TILE};

/// Draw all visible tiles, back walls, trees and decorations onto the world layer, animated
/// with a process clock. Prefer `draw_tiles_t` with the game's own time.
pub fn draw_tiles(c: &mut Canvas, world: &World, sprites: &Sprites, cam: &Camera) {
    static START: OnceLock<Instant> = OnceLock::new();
    let t = START.get_or_init(Instant::now).elapsed().as_secs_f32();
    draw_tiles_t(c, world, sprites, cam, t);
}

/// Materials that blend into each other without a seam.
fn family(t: Tile) -> u8 {
    match t {
        Tile::Dirt | Tile::Grass => 1,
        Tile::Stone | Tile::Copper | Tile::Iron | Tile::Crystal => 2,
        Tile::Plank => 3,
        Tile::Bedrock => 4,
        _ => 0,
    }
}

/// Wind sway in whole pixels (-1, 0, 1) for something at world column `x` with a phase.
fn sway(time: f32, x: i32, phase: f32) -> i32 {
    // A slow gust travelling right, plus a gentle per-plant flutter.
    let gust = (time * 1.3 - x as f32 * 0.35).sin();
    let flutter = (time * 2.7 + phase).sin() * 0.35;
    let v = gust * 0.9 + flutter;
    if v > 0.75 {
        1
    } else if v < -0.95 {
        -1
    } else {
        0
    }
}

/// Draw everything with an explicit time (seconds) for the sway animation.
pub fn draw_tiles_t(c: &mut Canvas, world: &World, sprites: &Sprites, cam: &Camera, time: f32) {
    let (ox, oy) = cam.origin();
    let tx0 = ox.div_euclid(TILE) - 1;
    let ty0 = oy.div_euclid(TILE) - 1;
    let tx1 = (ox + c.w).div_euclid(TILE) + 1;
    let ty1 = (oy + c.h).div_euclid(TILE) + 1;
    let ao = ao_table();

    // Trees whose trunks are in view (canopies are wider: look a few tiles further).
    let trees: Vec<_> = world.trees.iter().filter(|t| t.x >= tx0 - 3 && t.x <= tx1 + 3).collect();
    let tree_at = |x: i32| trees.iter().find(|t| t.x == x);

    for ty in ty0..=ty1 {
        for tx in tx0..=tx1 {
            let t = world.get(tx, ty);
            let sx = tx * TILE - ox;
            let sy = ty * TILE - oy;

            // Doors: a 3-tile column; pick the top/middle/bottom slice.
            if t.is_door() {
                if world.is_underground(tx, ty) {
                    draw_wall(c, world, sprites, ao, tx, ty, sx, sy);
                }
                let (top, bottom) = world.door_span(tx, ty).unwrap_or((ty, ty));
                let seg = if ty == top { 0 } else if ty == bottom { 2 } else { 1 };
                let set = if t == Tile::Door { &sprites.doors_closed } else { &sprites.doors_open };
                if let Some(slice) = set.get(seg) {
                    blit(c, slice, sx, sy, t == Tile::Door);
                }
                continue;
            }

            if !t.solid() {
                if world.is_underground(tx, ty) {
                    draw_wall(c, world, sprites, ao, tx, ty, sx, sy);
                }
                match t {
                    Tile::Air => {}
                    Tile::Log => {
                        let tree = tree_at(tx).filter(|tr| ty >= tr.top_y && ty < tr.base_y);
                        match tree {
                            // The bottom log also gets the root flare on top (drawn later).
                            Some(tr) => {
                                let v = (hash2(0x7A1C, tx, ty) * TRUNK_VARIANTS as f32) as usize % TRUNK_VARIANTS;
                                blit(c, &sprites.trunks[tr.kind as usize][v], sx, sy, false);
                            }
                            None => blit(c, sprites.tile(Tile::Log, tx, ty), sx, sy, false),
                        }
                    }
                    Tile::Torch => blit(c, &sprites.torch, sx, sy, false),
                    _ => blit(c, sprites.tile(t, tx, ty), sx, sy, false),
                }
                continue;
            }

            let Some(set) = sprites.edge_set(t) else { continue };
            let n_up = world.get(tx, ty - 1);
            let n_left = world.get(tx - 1, ty);
            let mut mask = 0;
            if !n_up.solid() {
                mask |= EDGE_UP;
            }
            if !world.solid(tx + 1, ty) {
                mask |= EDGE_RIGHT;
            }
            if !world.solid(tx, ty + 1) {
                mask |= EDGE_DOWN;
            }
            if !n_left.solid() {
                mask |= EDGE_LEFT;
            }
            let v = (hash2(0x7117, tx, ty) * VARIANTS as f32) as usize % VARIANTS;
            let opaque = set.opaque(v, mask);
            // Rounded corners are transparent: underground, the back wall shows through them
            // (above ground they stay clear so the sky does).
            if !opaque && world.is_underground(tx, ty) {
                draw_wall(c, world, sprites, ao, tx, ty, sx, sy);
            }
            blit(c, set.get(v, mask), sx, sy, opaque);

            // Subtle 1px seam where two different materials meet (drawn on the top/left side
            // only, so each boundary gets exactly one line).
            let f = family(t);
            if n_up.solid() && family(n_up) != f {
                darken_span(c, sx, sy, TILE, 1, 190);
            }
            if n_left.solid() && family(n_left) != f {
                darken_span(c, sx, sy, 1, TILE, 190);
            }
            // Concave corner: both neighbours solid but the diagonal open -> one dark pixel.
            if mask & (EDGE_UP | EDGE_LEFT) == 0 && !world.solid(tx - 1, ty - 1) {
                darken_span(c, sx, sy, 1, 1, 150);
            }
            if mask & (EDGE_UP | EDGE_RIGHT) == 0 && !world.solid(tx + 1, ty - 1) {
                darken_span(c, sx + TILE - 1, sy, 1, 1, 150);
            }
        }
    }

    // Decorations standing on grass.
    for d in world.decor.iter().filter(|d| d.x >= tx0 && d.x <= tx1 && d.y >= ty0 && d.y <= ty1) {
        if world.get(d.x, d.y) != Tile::Air || world.get(d.x, d.y + 1) != Tile::Grass {
            continue;
        }
        let s = &sprites.decor[d.kind as usize];
        let sx = d.x * TILE - ox;
        let sy = d.y * TILE - oy + 2; // sink into the grass so the blades hide the base
        let flip = d.seed & 1 == 1;
        let off = if d.kind.sways() { sway(time, d.x, d.seed as f32 * 0.05) } else { 0 };
        draw_swaying(c, s, sx, sy, flip, off);
    }

    // Root flares at the base of standing trees.
    for tr in &trees {
        if world.get(tr.x, tr.base_y - 1) != Tile::Log {
            continue;
        }
        let sx = tr.x * TILE - ox + TILE / 2 - ROOTS_W / 2;
        let sy = (tr.base_y - 1) * TILE - oy + 2;
        blit(c, &sprites.roots[tr.kind as usize], sx, sy, false);
    }

    // Grass caps (blades overhang the tile above) and side wraps.
    for ty in ty0..=ty1 {
        for tx in tx0..=tx1 {
            if world.get(tx, ty) != Tile::Grass || world.solid(tx, ty - 1) {
                continue;
            }
            let sx = tx * TILE - ox;
            let sy = ty * TILE - oy;
            let lo = !world.solid(tx - 1, ty);
            let ro = !world.solid(tx + 1, ty);
            let r = hash2(0x6A55, tx, ty);
            let v = if r < 0.1 {
                3
            } else if r < 0.18 {
                4
            } else {
                (r * 97.0) as usize % 3
            };
            let ends = lo as usize | (ro as usize) << 1;
            let cap = &sprites.grass_caps[(v % GRASS_CAP_VARIANTS) * 4 + ends];
            draw_cap(c, cap, sx, sy - GRASS_CAP_TOP, sway(time, tx, r * 6.0));
            if lo {
                blit(c, &sprites.grass_sides[0], sx, sy, false);
            }
            if ro {
                blit(c, &sprites.grass_sides[1], sx, sy, false);
            }
        }
    }

    // Canopies, while the top log is still there.
    for tr in &trees {
        if world.get(tr.x, tr.top_y) != Tile::Log {
            continue;
        }
        let s = &sprites.canopies[tr.kind as usize];
        let (ax, ay) = CANOPY_ANCHORS[tr.kind as usize];
        let sx = tr.x * TILE - ox + TILE / 2 - ax;
        let sy = tr.top_y * TILE - oy - ay;
        if sx + s.w < 0 || sy + s.h < 0 || sx >= c.w || sy >= c.h {
            continue;
        }
        blit(c, s, sx, sy, false);
    }
}

/// Back wall behind a non-solid underground cell, darkened towards solid neighbours.
#[allow(clippy::too_many_arguments)]
fn draw_wall(c: &mut Canvas, world: &World, sprites: &Sprites, ao: &[[u8; 256]; 256], tx: i32, ty: i32, sx: i32, sy: i32) {
    // Same dirt/stone boundary as worldgen, so the wall matches the terrain around it.
    let col = tx.clamp(0, world.w - 1);
    let dirt_depth = 4 + (hash2(world.seed, col, 7) * 4.0) as i32;
    let deep = ty >= world.surface[col as usize] + dirt_depth;
    let v = (hash2(0xBAC, tx, ty) * 4.0) as usize % 4;
    let wall = if deep { &sprites.back_stone[v] } else { &sprites.back_dirt[v] };
    let s = |dx, dy| world.solid(tx + dx, ty + dy) as usize;
    let m = s(0, -1) | s(1, 0) << 1 | s(0, 1) << 2 | s(-1, 0) << 3 | s(1, -1) << 4 | s(1, 1) << 5 | s(-1, 1) << 6 | s(-1, -1) << 7;
    let k = &ao[m];
    if sx >= 0 && sy >= 0 && sx + TILE <= c.w && sy + TILE <= c.h {
        for y in 0..TILE as usize {
            let d = ((sy as usize + y) * c.w as usize + sx as usize) * 4;
            let dst = &mut c.px[d..d + TILE as usize * 4];
            let src = &wall.px[y * TILE as usize..(y + 1) * TILE as usize];
            let kr = &k[y * TILE as usize..(y + 1) * TILE as usize];
            for ((o, p), &f) in dst.chunks_exact_mut(4).zip(src).zip(kr) {
                let f = f as u32;
                o[0] = ((p[0] as u32 * f) >> 8) as u8;
                o[1] = ((p[1] as u32 * f) >> 8) as u8;
                o[2] = ((p[2] as u32 * f) >> 8) as u8;
                o[3] = 255;
            }
        }
        return;
    }
    for y in 0..TILE {
        let py = sy + y;
        if py < 0 || py >= c.h {
            continue;
        }
        for x in 0..TILE {
            let px = sx + x;
            if px < 0 || px >= c.w {
                continue;
            }
            let p = wall.px[(y * TILE + x) as usize];
            let f = k[(y * TILE + x) as usize] as u32;
            let i = ((py * c.w + px) * 4) as usize;
            c.px[i] = ((p[0] as u32 * f) >> 8) as u8;
            c.px[i + 1] = ((p[1] as u32 * f) >> 8) as u8;
            c.px[i + 2] = ((p[2] as u32 * f) >> 8) as u8;
            c.px[i + 3] = 255;
        }
    }
}

/// Per 8-neighbour solidity mask (N, E, S, W, NE, SE, SW, NW), a 16x16 table of brightness
/// factors (0..=256 scaled to u8): walls get darker close to the solid blocks around the
/// opening, in a few soft bands.
fn ao_table() -> &'static [[u8; 256]; 256] {
    static T: OnceLock<Box<[[u8; 256]; 256]>> = OnceLock::new();
    T.get_or_init(|| {
        let mut t = Box::new([[255u8; 256]; 256]);
        const R: f32 = 7.0;
        for (m, tab) in t.iter_mut().enumerate() {
            let b = |i: usize| m & (1 << i) != 0;
            for y in 0..16 {
                for x in 0..16 {
                    let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
                    let mut occ: f32 = 0.0;
                    let mut add = |d: f32| occ = occ.max(1.0 - (d / R).min(1.0));
                    if b(0) {
                        add(fy);
                    }
                    if b(1) {
                        add(16.0 - fx);
                    }
                    if b(2) {
                        add(16.0 - fy);
                    }
                    if b(3) {
                        add(fx);
                    }
                    let corner = |cx: f32, cy: f32| ((fx - cx).powi(2) + (fy - cy).powi(2)).sqrt();
                    if b(4) {
                        add(corner(16.0, 0.0));
                    }
                    if b(5) {
                        add(corner(16.0, 16.0));
                    }
                    if b(6) {
                        add(corner(0.0, 16.0));
                    }
                    if b(7) {
                        add(corner(0.0, 0.0));
                    }
                    // Quantise into bands (pixel-art friendly), darkest right at the edge.
                    let band = (occ * occ * 4.0).round() / 4.0;
                    tab[y * 16 + x] = (255.0 * (1.0 - 0.6 * band)) as u8;
                }
            }
        }
        t
    })
}

/// Blit a sprite: straight copy for opaque sprites fully on screen, alpha-tested otherwise.
fn blit(c: &mut Canvas, s: &Sprite, x: i32, y: i32, opaque: bool) {
    let inside = x >= 0 && y >= 0 && x + s.w <= c.w && y + s.h <= c.h;
    if inside && opaque {
        let w = s.w as usize;
        for row in 0..s.h {
            let src = &s.px[row as usize * w..row as usize * w + w];
            let d = (((y + row) * c.w + x) * 4) as usize;
            c.px[d..d + w * 4].copy_from_slice(src.as_flattened());
        }
        return;
    }
    for row in 0..s.h {
        let py = y + row;
        if py < 0 || py >= c.h {
            continue;
        }
        for col in 0..s.w {
            let px = x + col;
            if px < 0 || px >= c.w {
                continue;
            }
            let p = s.px[(row * s.w + col) as usize];
            if p[3] == 255 {
                let i = ((py * c.w + px) * 4) as usize;
                c.px[i..i + 4].copy_from_slice(&p);
            } else if p[3] > 0 {
                c.blend(px, py, p);
            }
        }
    }
}

/// Multiply a rectangle of the canvas by `k`/256.
fn darken_span(c: &mut Canvas, x: i32, y: i32, w: i32, h: i32, k: u32) {
    for py in y.max(0)..(y + h).min(c.h) {
        for px in x.max(0)..(x + w).min(c.w) {
            let i = ((py * c.w + px) * 4) as usize;
            for ch in 0..3 {
                c.px[i + ch] = ((c.px[i + ch] as u32 * k) >> 8) as u8;
            }
        }
    }
}

/// Draw a decoration whose upper half leans by `off` px.
fn draw_swaying(c: &mut Canvas, s: &Sprite, x: i32, y: i32, flip: bool, off: i32) {
    // First opaque row, so "upper half" means the plant's, not the cell's.
    let top = (0..s.h).find(|&r| (0..s.w).any(|q| s.px[(r * s.w + q) as usize][3] > 0)).unwrap_or(0);
    let bend = top + (s.h - top) / 2;
    for row in 0..s.h {
        let dx = if row < bend { off } else { 0 };
        for col in 0..s.w {
            let src = if flip { s.w - 1 - col } else { col };
            let p = s.px[(row * s.w + src) as usize];
            if p[3] > 0 {
                c.blend(x + col + dx, y + row, p);
            }
        }
    }
}

/// Grass cap: blade tips (top rows) lean with the wind.
fn draw_cap(c: &mut Canvas, s: &Sprite, x: i32, y: i32, off: i32) {
    for row in 0..s.h {
        let dx = if row < 3 { off } else { 0 };
        for col in 0..s.w {
            let p = s.px[(row * s.w + col) as usize];
            if p[3] > 0 {
                c.blend(x + col + dx, y + row, p);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::camera::Camera;

    /// Draw the world at both zoom levels and print the cost (budget: <= 3 ms at 960x540 in
    /// release builds; debug builds are much slower, so only a loose bound is asserted).
    #[test]
    #[ignore = "timing; run with --release -- --ignored --nocapture"]
    fn world_draw_perf() {
        let world = World::generate(42);
        let sprites = Sprites::new();
        for (w, h, cx, dy) in [(640u32, 360u32, world.w / 2, 4), (960, 540, world.w / 2, 4), (960, 540, 60, 45)] {
            let (px, py) = ((cx * TILE) as f32, ((world.surface[cx as usize] + dy) * TILE) as f32);
            let mut cam = Camera::new(px, py);
            cam.set_view(w, h);
            for _ in 0..600 {
                cam.follow(px, py, 1.0 / 60.0, &world);
            }
            let mut buf = vec![0u8; (w * h * 4) as usize];
            // Report the median and best frame: the machine may be busy with other builds.
            let mut times: Vec<f64> = (0..200)
                .map(|i| {
                    let start = Instant::now();
                    let mut c = Canvas::new(&mut buf, w, h);
                    draw_tiles_t(&mut c, &world, &sprites, &cam, i as f32 * 0.016);
                    start.elapsed().as_secs_f64() * 1000.0
                })
                .collect();
            times.sort_by(f64::total_cmp);
            let (ms, best) = (times[times.len() / 2], times[0]);
            println!("world draw {w}x{h} (column {cx}, {dy} below surface): median {ms:.2} ms, best {best:.2} ms");
            assert!(ms < 200.0);
        }
    }

    /// Render a few 960x540 world views over a flat sky colour into `$WORLD_SHOTS` (a dir).
    #[test]
    #[ignore = "writes PNGs; WORLD_SHOTS=dir cargo test -p game world_shots -- --ignored"]
    fn world_shots() {
        let Ok(dir) = std::env::var("WORLD_SHOTS") else { return };
        let world = World::generate(42);
        let sprites = Sprites::new();
        let (w, h) = (960u32, 540u32);
        for (name, tx, dy) in [("surface", world.w / 2 - 20, 6), ("caves", world.w / 2 + 30, 24), ("deep", 60, 45)] {
            let (px, py) = ((tx * TILE) as f32, ((world.surface[tx as usize] + dy) * TILE) as f32);
            let mut cam = Camera::new(px, py);
            cam.set_view(w, h);
            for _ in 0..600 {
                cam.follow(px, py, 1.0 / 60.0, &world);
            }
            let mut buf = vec![0u8; (w * h * 4) as usize];
            let mut c = Canvas::new(&mut buf, w, h);
            draw_tiles_t(&mut c, &world, &sprites, &cam, 1.0);
            // Composite over a sky blue so transparent air is visible.
            for p in buf.chunks_exact_mut(4) {
                let a = p[3] as u32;
                for (ch, sky) in p.iter_mut().take(3).zip([116u32, 170, 230]) {
                    *ch = ((*ch as u32 * a + sky * (255 - a)) / 255) as u8;
                }
                p[3] = 255;
            }
            let f = std::fs::File::create(format!("{dir}/world_{name}.png")).unwrap();
            let mut e = png::Encoder::new(std::io::BufWriter::new(f), w, h);
            e.set_color(png::ColorType::Rgba);
            e.set_depth(png::BitDepth::Eight);
            e.write_header().unwrap().write_image_data(&buf).unwrap();
        }
    }

    #[test]
    fn ao_is_bright_in_open_space_and_dark_against_walls() {
        let t = ao_table();
        assert!(t[0].iter().all(|&k| k == 255));
        let north = &t[1];
        assert!(north[8] < north[15 * 16 + 8]);
    }
}
