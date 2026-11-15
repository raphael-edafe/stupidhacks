//! Tile rendering with autotiled edges. Owner: person B1.

use crate::camera::Camera;
use crate::gfx::{shade, Canvas};
use crate::sprites::{grass_overlay, Sprites};
use crate::world::{Tile, World, TILE};

/// Draw all visible tiles (and underground back walls) onto the world layer.
/// Sky must already be drawn underneath.
pub fn draw_tiles(c: &mut Canvas, world: &World, sprites: &Sprites, cam: &Camera) {
    let (ox, oy) = cam.origin();
    let tx0 = ox.div_euclid(TILE) - 1;
    let ty0 = oy.div_euclid(TILE) - 1;
    let tx1 = (ox + c.w).div_euclid(TILE) + 1;
    let ty1 = (oy + c.h).div_euclid(TILE) + 1;

    for ty in ty0..=ty1 {
        for tx in tx0..=tx1 {
            let t = world.get(tx, ty);
            let sx = tx * TILE - ox;
            let sy = ty * TILE - oy;

            // Back wall behind anything that isn't a full solid block.
            if !t.solid() && world.is_underground(tx, ty) {
                let deep = ty > world.surface[tx.clamp(0, world.w - 1) as usize] + 6;
                let v = (crate::world::hash2(0xBAC, tx, ty) * 4.0) as usize % 4;
                let wall = if deep { &sprites.back_stone[v] } else { &sprites.back_dirt[v] };
                c.sprite(wall, sx, sy, false);
            }

            match t {
                Tile::Air => {}
                t if t.solid() => draw_solid(c, world, sprites, t, tx, ty, sx, sy),
                t => c.sprite(sprites.tile(t, tx, ty), sx, sy, false),
            }
        }
    }

    // Grass tops last so blades overlap the tile above.
    for ty in ty0..=ty1 {
        for tx in tx0..=tx1 {
            if world.get(tx, ty) == Tile::Grass && !world.solid(tx, ty - 1) {
                let sx = tx * TILE - ox;
                let sy = ty * TILE - oy;
                let left_open = !world.solid(tx - 1, ty);
                let right_open = !world.solid(tx + 1, ty);
                for (dx, dy, col) in grass_overlay(tx, ty) {
                    // Round off the grass at exposed corners.
                    if dy >= 0 && ((left_open && dx == 0) || (right_open && dx == TILE - 1)) && dy < 1 {
                        continue;
                    }
                    c.put(sx + dx, sy + dy, col);
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_solid(c: &mut Canvas, world: &World, sprites: &Sprites, t: Tile, tx: i32, ty: i32, sx: i32, sy: i32) {
    let s = sprites.tile(t, tx, ty);
    let open = |dx: i32, dy: i32| !world.solid(tx + dx, ty + dy);
    let (up, down, left, right) = (open(0, -1), open(0, 1), open(-1, 0), open(1, 0));
    let last = TILE - 1;

    for y in 0..TILE {
        for x in 0..TILE {
            // Rounded corners where two exposed sides meet.
            let corner = (up && left && x + y < 2)
                || (up && right && (last - x) + y < 2)
                || (down && left && x + (last - y) < 2)
                || (down && right && (last - x) + (last - y) < 2);
            if corner {
                continue;
            }
            let mut col = s.get(x, y);
            // Dark outline on exposed edges, soft highlight just inside the top.
            if (up && y == 0) || (down && y == last) || (left && x == 0) || (right && x == last) {
                col = shade(col, 0.5);
            } else if up && y == 1 {
                col = shade(col, 1.18);
            } else if (down && y == last - 1) || (right && x == last - 1) {
                col = shade(col, 0.8);
            }
            c.put(sx + x, sy + y, col);
        }
    }
}
