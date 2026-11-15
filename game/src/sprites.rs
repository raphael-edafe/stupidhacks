//! Procedurally generated pixel art: tile textures, back walls, player sprites.
//! All original art; nothing is copied from any existing game. Owner: person B1.

use crate::gfx::{lerp_color, rgb, rgba, shade, Rgba, Sprite};
use crate::world::{hash2, Tile, TILE};

pub const VARIANTS: usize = 4;

pub struct Sprites {
    /// [tile as usize][variant]
    pub tiles: Vec<Vec<Sprite>>,
    /// Underground back wall variants (already darkened).
    pub back_dirt: Vec<Sprite>,
    pub back_stone: Vec<Sprite>,
    pub player_idle: Sprite,
    pub player_walk: [Sprite; 4],
    pub player_jump: Sprite,
    pub sword: Sprite,
    pub pickaxe: Sprite,
}

impl Sprites {
    pub fn new() -> Self {
        let all = [
            Tile::Air,
            Tile::Dirt,
            Tile::Grass,
            Tile::Stone,
            Tile::Copper,
            Tile::Iron,
            Tile::Crystal,
            Tile::Log,
            Tile::Leaves,
            Tile::Plank,
            Tile::Torch,
            Tile::Bedrock,
        ];
        let tiles = all
            .iter()
            .map(|&t| (0..VARIANTS).map(|v| tile_texture(t, v as i32)).collect())
            .collect();
        let darken = |s: Sprite, k: f32| Sprite { px: s.px.iter().map(|&c| shade(c, k)).collect(), ..s };
        let back_dirt = (0..VARIANTS).map(|v| darken(tile_texture(Tile::Dirt, v as i32 + 10), 0.38)).collect();
        let back_stone = (0..VARIANTS).map(|v| darken(tile_texture(Tile::Stone, v as i32 + 10), 0.36)).collect();
        let (player_idle, player_walk, player_jump) = player_sprites();
        Sprites {
            tiles,
            back_dirt,
            back_stone,
            player_idle,
            player_walk,
            player_jump,
            sword: sword_sprite(),
            pickaxe: pickaxe_sprite(),
        }
    }

    pub fn tile(&self, t: Tile, x: i32, y: i32) -> &Sprite {
        let v = (hash2(0x7117, x, y) * VARIANTS as f32) as usize % VARIANTS;
        &self.tiles[t as usize][v]
    }
}

impl Default for Sprites {
    fn default() -> Self {
        Self::new()
    }
}

/// Per-pixel noise in [0,1) for a texture.
fn n(salt: i32, v: i32, x: i32, y: i32) -> f32 {
    hash2(0xA11CE ^ (salt as u64) << 8 ^ v as u64, x, y)
}

fn speckled(base: Rgba, light: Rgba, dark: Rgba, salt: i32, v: i32) -> Sprite {
    let mut s = Sprite::new(TILE, TILE);
    for y in 0..TILE {
        for x in 0..TILE {
            // Low-frequency blotches + per-pixel speckle.
            let blot = n(salt, v, x / 4, y / 4);
            let sp = n(salt + 1, v, x, y);
            let mut c = lerp_color(base, if blot > 0.5 { light } else { dark }, (blot - 0.5).abs() * 0.6);
            if sp > 0.93 {
                c = light;
            } else if sp < 0.07 {
                c = dark;
            }
            s.set(x, y, c);
        }
    }
    s
}

fn tile_texture(t: Tile, v: i32) -> Sprite {
    match t {
        Tile::Air => Sprite::new(TILE, TILE),
        Tile::Dirt => {
            let mut s = speckled(rgb(134, 88, 58), rgb(160, 110, 74), rgb(100, 64, 44), 1, v);
            // A couple of pebbles.
            for k in 0..2 {
                let px = (n(3, v, k, 0) * 13.0) as i32 + 1;
                let py = (n(3, v, k, 1) * 13.0) as i32 + 1;
                s.set(px, py, rgb(150, 140, 130));
                s.set(px + 1, py, rgb(120, 112, 104));
            }
            s
        }
        Tile::Grass => tile_texture(Tile::Dirt, v), // grass top is an edge overlay (see render)
        Tile::Stone => {
            let mut s = speckled(rgb(122, 124, 138), rgb(146, 148, 162), rgb(96, 98, 112), 4, v);
            // Cracks / mortar-ish lines.
            let cy = 4 + (n(5, v, 0, 0) * 8.0) as i32;
            for x in 0..TILE {
                if n(6, v, x, 0) > 0.25 {
                    s.set(x, cy + (x / 5) % 2, rgb(88, 90, 104));
                }
            }
            s
        }
        Tile::Copper | Tile::Iron | Tile::Crystal => {
            let mut s = tile_texture(Tile::Stone, v);
            let (c, hi) = match t {
                Tile::Copper => (rgb(206, 116, 60), rgb(255, 178, 110)),
                Tile::Iron => (rgb(184, 176, 196), rgb(240, 236, 250)),
                _ => (rgb(90, 210, 250), rgb(210, 250, 255)),
            };
            for k in 0..4 {
                let px = (n(7, v, k, 0) * 12.0) as i32 + 1;
                let py = (n(7, v, k, 1) * 12.0) as i32 + 1;
                for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1), (2, 1)] {
                    s.set(px + dx, py + dy, c);
                }
                s.set(px, py, hi);
                s.set(px + 2, py + 2, shade(c, 0.6));
            }
            s
        }
        Tile::Log => {
            let mut s = Sprite::new(TILE, TILE);
            for y in 0..TILE {
                for x in 0..TILE {
                    let edge = x <= 2 || x >= 13;
                    let stripe = (x + (n(8, v, x, y / 3) * 2.0) as i32) % 4 == 0;
                    let c = if x <= 1 || x >= 14 {
                        rgba(0, 0, 0, 0)
                    } else if edge {
                        rgb(88, 58, 36)
                    } else if stripe {
                        rgb(98, 66, 40)
                    } else {
                        rgb(126, 86, 54)
                    };
                    s.set(x, y, c);
                }
            }
            s
        }
        Tile::Leaves => {
            let mut s = Sprite::new(TILE, TILE);
            for y in 0..TILE {
                for x in 0..TILE {
                    let r = n(9, v, x / 2, y / 2);
                    let c = if r < 0.12 {
                        rgba(0, 0, 0, 0)
                    } else if r < 0.4 {
                        rgb(40, 112, 56)
                    } else if r < 0.85 {
                        rgb(60, 146, 70)
                    } else {
                        rgb(108, 190, 92)
                    };
                    s.set(x, y, c);
                }
            }
            s
        }
        Tile::Plank => {
            let mut s = Sprite::new(TILE, TILE);
            for y in 0..TILE {
                let board = y / 4;
                let seam = y % 4 == 3;
                let offset = (board * 7 + v * 3) % TILE;
                for x in 0..TILE {
                    let grain = n(10, v, x / 3, y);
                    let mut c = lerp_color(rgb(186, 136, 82), rgb(160, 112, 66), grain * 0.7);
                    if seam || (x + offset) % TILE == 0 {
                        c = rgb(110, 74, 42);
                    }
                    s.set(x, y, c);
                }
            }
            s
        }
        Tile::Torch => {
            Sprite::from_ascii(
                &[
                    "................",
                    "................",
                    "................",
                    ".......y........",
                    "......yYy.......",
                    "......yWy.......",
                    ".......o........",
                    ".......s........",
                    ".......s........",
                    ".......S........",
                    ".......s........",
                    ".......S........",
                    ".......s........",
                    ".......S........",
                    "................",
                    "................",
                ],
                &[
                    ('y', rgb(255, 170, 40)),
                    ('Y', rgb(255, 230, 120)),
                    ('W', rgb(255, 255, 220)),
                    ('o', rgb(230, 110, 30)),
                    ('s', rgb(140, 96, 56)),
                    ('S', rgb(110, 74, 42)),
                ],
            )
        }
        Tile::Bedrock => speckled(rgb(40, 38, 48), rgb(58, 56, 68), rgb(26, 24, 32), 11, v),
    }
}

/// Grass overlay drawn on top of an exposed grass tile: top band + blades poking
/// up into the tile above (negative y). Returns (dx, dy, colour) pixels.
pub fn grass_overlay(x: i32, y: i32) -> Vec<(i32, i32, Rgba)> {
    let mut out = Vec::with_capacity(80);
    let dark = rgb(58, 132, 54);
    let mid = rgb(92, 178, 74);
    let light = rgb(150, 220, 100);
    for dx in 0..TILE {
        let depth = 3 + (hash2(0x6A55, x * TILE + dx, y) * 3.0) as i32;
        for dy in 0..depth {
            out.push((dx, dy, if dy == 0 { light } else if dy == depth - 1 { dark } else { mid }));
        }
        let r = hash2(0xB1AD, x * TILE + dx, y);
        if r > 0.6 {
            out.push((dx, -1, mid));
            if r > 0.85 {
                out.push((dx, -2, light));
            }
        }
    }
    out
}

fn player_sprites() -> (Sprite, [Sprite; 4], Sprite) {
    // An original little engineer: amber hard hat, teal jacket, dark trousers.
    let pal: &[(char, Rgba)] = &[
        ('h', rgb(255, 184, 48)),  // hat
        ('H', rgb(206, 132, 24)),  // hat shade
        ('k', rgb(246, 200, 160)), // skin
        ('K', rgb(212, 158, 120)), // skin shade
        ('e', rgb(30, 30, 46)),    // eye
        ('j', rgb(46, 170, 168)),  // jacket
        ('J', rgb(30, 118, 124)),  // jacket shade
        ('b', rgb(250, 220, 90)),  // belt buckle / visor stripe
        ('p', rgb(54, 58, 92)),    // trousers
        ('P', rgb(36, 38, 64)),    // trousers shade
        ('s', rgb(70, 46, 34)),    // boots
        ('o', rgb(20, 18, 28)),    // outline
    ];
    let top = [
        "...oooooo...",
        "..ohhhhhho..",
        ".ohhhhbhhho.",
        ".oHHHHHHHHo.",
        "..okkkkkeo..",
        "..okkkkkko..",
        "..oKkkkkKo..",
        "...okkkko...",
        "..ojjjjjjo..",
        ".ojjjjjjjjo.",
        ".ojjjjjjJjo.",
        ".okjjjjjJko.",
        ".oKjjjjjJKo.",
        "..obbbbbbo..",
    ];
    let legs_idle = ["..oppppppo..", "..opPo.opPo.", "..opPo.opPo.", "..opPo.opPo.", "..osso.osso.", "............"];
    let legs_w1 = ["..oppppppo..", "..opPo.opPo.", ".opPo..opPo.", ".opPo...opo.", ".osso...osso", "............"];
    let legs_w2 = ["..oppppppo..", "..opPoopPo..", "...opPopPo..", "...opPoPo...", "...ossoso...", "............"];
    let legs_w3 = ["..oppppppo..", ".opPo..opPo.", ".opPo...opPo", "opPo.....opo", "osso.....oss", "............"];
    let legs_jump = ["..oppppppo..", ".opPo..opPo.", ".opo....opo.", ".oss....oss.", "............", "............"];
    let build = |legs: &[&str]| {
        let rows: Vec<&str> = top.iter().copied().chain(legs.iter().copied()).collect();
        Sprite::from_ascii(&rows, pal)
    };
    (
        build(&legs_idle),
        [build(&legs_w1), build(&legs_w2), build(&legs_w3), build(&legs_w2)],
        build(&legs_jump),
    )
}

fn sword_sprite() -> Sprite {
    Sprite::from_ascii(
        &[
            "..........ow",
            ".........owW",
            "........owWo",
            ".......owWo.",
            "......owWo..",
            ".....owWo...",
            "..o.owWo....",
            "..oowWo.....",
            "...gWo......",
            "..gogo......",
            ".bo..oo.....",
            "bb..........",
        ],
        &[
            ('w', rgb(230, 240, 255)),
            ('W', rgb(160, 186, 220)),
            ('o', rgb(30, 30, 50)),
            ('g', rgb(255, 196, 60)),
            ('b', rgb(120, 70, 40)),
        ],
    )
}

fn pickaxe_sprite() -> Sprite {
    Sprite::from_ascii(
        &[
            "...oooooo...",
            "..oiiiiiio..",
            ".oiIo..oIio.",
            ".oio.ob.oio.",
            ".oo..ob..oo.",
            ".....ob.....",
            ".....ob.....",
            ".....ob.....",
            ".....ob.....",
            ".....ob.....",
            ".....oo.....",
        ],
        &[('o', rgb(30, 30, 50)), ('i', rgb(170, 176, 196)), ('I', rgb(230, 234, 246)), ('b', rgb(140, 92, 52))],
    )
}
