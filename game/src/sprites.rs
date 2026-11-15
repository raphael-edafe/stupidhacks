//! Pixel art: procedural tile textures and back walls, plus PNG sprites (player, tools,
//! torch, app icons, mobs) drawn with pxl (sources in `assets/sprites/src/*.px.js`).
//! The PNGs are embedded with `include_bytes!` and decoded once at startup.
//! All original art; nothing is copied from any existing game. Owner: person B1.

use crate::gfx::{lerp_color, rgb, rgba, shade, Rgba, Sprite};
use crate::items::{Item, Tier, ALL as ALL_ITEMS};
use crate::world::{hash2, Tile, TILE};

pub const VARIANTS: usize = 4;

pub struct Sprites {
    /// [tile as usize][variant]
    pub tiles: Vec<Vec<Sprite>>,
    /// Underground back wall variants (already darkened).
    pub back_dirt: Vec<Sprite>,
    pub back_stone: Vec<Sprite>,
    /// Player frames, 16x40, facing right (flip for left). Feet on the bottom row.
    /// Idle: 2 frames (breathing). Walk: 6 frames.
    pub player_idle: Vec<Sprite>,
    pub player_walk: Vec<Sprite>,
    pub player_jump: Sprite,
    pub player_fall: Sprite,
    /// 14x14, pointing up-right. Grip at the bottom-left.
    pub sword: Sprite,
    /// The sword at the four swing angles [up, up-right, right, down-right] (facing right;
    /// flip for left). Grip pixel of each in `SWORD_GRIPS`.
    pub sword_swing: Vec<Sprite>,
    /// 14x14, raised: handle from bottom-left (grip) to the head at the top-right.
    pub pickaxe: Sprite,
    /// The pickaxe rotated 90deg clockwise (struck): grip top-left, head bottom-right.
    pub pickaxe_down: Sprite,
    /// 16x16 app-chooser icons, same order as `shared::APPS`.
    pub app_icons: Vec<Sprite>,
    /// Ground "glitch bug": 4 walk frames, 20x14 each, facing RIGHT (flip for left).
    pub mob_crawler: Vec<Sprite>,
    /// "Bit bat": 3 flap frames (up, level, down), 18x12 each, symmetric.
    pub mob_bat: Vec<Sprite>,
    /// Hotbar / crafting icon per item, indexed by `Item as usize` (at most 16x16).
    pub icons: Vec<Sprite>,
    /// `sword_swing` recoloured per sword tier, indexed by `Tier as usize`.
    pub sword_tiers: Vec<Vec<Sprite>>,
}

// --- embedded PNGs (rebuild with assets/sprites/src/build.sh) ---
macro_rules! asset {
    ($name:literal) => {
        ($name, include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../assets/sprites/", $name, ".png")) as &[u8])
    };
}
type Asset = (&'static str, &'static [u8]);
const PLAYER_IDLE: Asset = asset!("player_idle");
const PLAYER_WALK: Asset = asset!("player_walk");
const PLAYER_JUMP: Asset = asset!("player_jump");
const PLAYER_FALL: Asset = asset!("player_fall");
const SWORD: Asset = asset!("sword");
const SWORD_UP: Asset = asset!("sword_up");
const PICKAXE: Asset = asset!("pickaxe");
const TORCH: Asset = asset!("torch");
const MOB_CRAWLER: Asset = asset!("mob_crawler");
const MOB_BAT: Asset = asset!("mob_bat");
const APP_ICONS: [Asset; 6] = [
    asset!("icon_terminal"),
    asset!("icon_files"),
    asset!("icon_editor"),
    asset!("icon_calculator"),
    asset!("icon_monitor"),
    asset!("icon_matrix"),
];

/// Hand position inside each `sword_swing` sprite.
pub const SWORD_GRIPS: [(i32, i32); 4] = [(3, 10), (3, 10), (3, 3), (3, 3)];
pub const PLAYER_W: i32 = 16;
pub const PLAYER_H: i32 = 40;
pub const CRAWLER_W: i32 = 20;
pub const CRAWLER_H: i32 = 14;
pub const BAT_W: i32 = 18;
pub const BAT_H: i32 = 12;

/// Decode an 8-bit PNG (any colour type) into a straight-alpha RGBA sprite.
pub fn decode_png(bytes: &[u8]) -> Result<Sprite, String> {
    let mut dec = png::Decoder::new(std::io::Cursor::new(bytes));
    dec.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = dec.read_info().map_err(|e| e.to_string())?;
    let size = reader.output_buffer_size().ok_or("png too large")?;
    let mut buf = vec![0u8; size];
    let info = reader.next_frame(&mut buf).map_err(|e| e.to_string())?;
    if info.bit_depth != png::BitDepth::Eight {
        return Err(format!("unsupported bit depth {:?}", info.bit_depth));
    }
    let (w, h) = (info.width as i32, info.height as i32);
    let ch = match info.color_type {
        png::ColorType::Grayscale => 1,
        png::ColorType::GrayscaleAlpha => 2,
        png::ColorType::Rgb => 3,
        png::ColorType::Rgba => 4,
        png::ColorType::Indexed => return Err("indexed png not expanded".into()),
    };
    let mut s = Sprite::new(w, h);
    for y in 0..h {
        let row = &buf[y as usize * info.line_size..];
        for x in 0..w {
            let p = &row[x as usize * ch..x as usize * ch + ch];
            let c = match ch {
                1 => [p[0], p[0], p[0], 255],
                2 => [p[0], p[0], p[0], p[1]],
                3 => [p[0], p[1], p[2], 255],
                _ => [p[0], p[1], p[2], p[3]],
            };
            s.px[(y * w + x) as usize] = if c[3] == 0 { [0, 0, 0, 0] } else { c };
        }
    }
    Ok(s)
}

/// Magenta/black checker, so a missing sprite is obvious but never crashes the game.
fn placeholder(w: i32, h: i32) -> Sprite {
    let mut s = Sprite::new(w, h);
    for y in 0..h {
        for x in 0..w {
            s.set(x, y, if (x / 2 + y / 2) % 2 == 0 { rgb(255, 0, 255) } else { rgb(20, 0, 20) });
        }
    }
    s
}

/// Decode one sprite; on failure log and return a `w`x`h` placeholder.
fn load(a: Asset, w: i32, h: i32) -> Sprite {
    match decode_png(a.1) {
        Ok(s) if s.w == w && s.h == h => s,
        Ok(s) => {
            eprintln!("sprites: {}.png is {}x{}, expected {}x{}", a.0, s.w, s.h, w, h);
            s
        }
        Err(e) => {
            eprintln!("sprites: failed to decode {}.png: {e}; using a placeholder", a.0);
            placeholder(w, h)
        }
    }
}

/// Decode a horizontal strip of `n` frames of `fw`x`fh`.
fn load_strip(a: Asset, fw: i32, fh: i32, n: i32) -> Vec<Sprite> {
    let strip = load(a, fw * n, fh);
    if strip.w < fw * n || strip.h < fh {
        eprintln!("sprites: {}.png too small for {n} frames of {fw}x{fh}; using placeholders", a.0);
        return vec![placeholder(fw, fh); n as usize];
    }
    (0..n)
        .map(|i| {
            let mut s = Sprite::new(fw, fh);
            for y in 0..fh {
                for x in 0..fw {
                    s.set(x, y, strip.get(i * fw + x, y));
                }
            }
            s
        })
        .collect()
}

/// Rotate 90 degrees clockwise.
fn rot90(s: &Sprite) -> Sprite {
    let mut r = Sprite::new(s.h, s.w);
    for y in 0..s.h {
        for x in 0..s.w {
            r.set(s.h - 1 - y, x, s.get(x, y));
        }
    }
    r
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
            Tile::Workbench,
        ];
        let tiles = all
            .iter()
            .map(|&t| (0..VARIANTS).map(|v| tile_texture(t, v as i32)).collect())
            .collect();
        let darken = |s: Sprite, k: f32| Sprite { px: s.px.iter().map(|&c| shade(c, k)).collect(), ..s };
        let back_dirt = (0..VARIANTS).map(|v| darken(tile_texture(Tile::Dirt, v as i32 + 10), 0.38)).collect();
        let back_stone = (0..VARIANTS).map(|v| darken(tile_texture(Tile::Stone, v as i32 + 10), 0.36)).collect();
        let pickaxe = load(PICKAXE, 14, 14);
        let sword = load(SWORD, 14, 14);
        let sword_up = load(SWORD_UP, 7, 14);
        let swing = vec![sword_up.clone(), sword.clone(), rot90(&sword_up), rot90(&sword)];
        let sword_tiers: Vec<Vec<Sprite>> = Tier::SWORDS.iter().map(|&t| swing.iter().map(|s| tint_sword(s, t)).collect()).collect();
        let torch = load(TORCH, TILE, TILE);
        let icons = ALL_ITEMS
            .iter()
            .map(|&it| {
                if let Some(t) = it.sword() {
                    return tint_sword(&sword, t);
                }
                match it {
                    Item::Torch => torch.clone(),
                    Item::Bow => Sprite::from_ascii(&ICON_BOW, &ICON_PAL),
                    Item::Arrow => Sprite::from_ascii(&ICON_ARROW, &ICON_PAL),
                    _ => match (it.tile(), it.armour()) {
                        (Some(t), _) => tile_texture(t, 0),
                        (None, Some((slot, tier))) => {
                            let rows: &[&str] = if slot == crate::items::Slot::Head { &ICON_HELMET } else { &ICON_CHEST };
                            let m = tier.metal();
                            let mut pal = ICON_PAL.to_vec();
                            pal.extend([('1', m[0]), ('2', m[1]), ('3', m[2]), ('4', m[3])]);
                            Sprite::from_ascii(rows, &pal)
                        }
                        _ => placeholder(12, 12),
                    },
                }
            })
            .collect();
        Sprites {
            tiles,
            back_dirt,
            back_stone,
            player_idle: load_strip(PLAYER_IDLE, PLAYER_W, PLAYER_H, 2),
            player_walk: load_strip(PLAYER_WALK, PLAYER_W, PLAYER_H, 6),
            player_jump: load(PLAYER_JUMP, PLAYER_W, PLAYER_H),
            player_fall: load(PLAYER_FALL, PLAYER_W, PLAYER_H),
            sword_swing: swing,
            sword,
            pickaxe_down: rot90(&pickaxe),
            pickaxe,
            app_icons: APP_ICONS.iter().map(|&a| load(a, 16, 16)).collect(),
            mob_crawler: load_strip(MOB_CRAWLER, CRAWLER_W, CRAWLER_H, 4),
            mob_bat: load_strip(MOB_BAT, BAT_W, BAT_H, 3),
            icons,
            sword_tiers,
        }
    }

    pub fn icon(&self, it: Item) -> &Sprite {
        &self.icons[it as usize]
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
        Tile::Torch => load(TORCH, TILE, TILE),
        Tile::Bedrock => speckled(rgb(40, 38, 48), rgb(58, 56, 68), rgb(26, 24, 32), 11, v),
        Tile::Workbench => workbench(),
    }
}

/// A little carpenter's bench: plank top, two legs, a crossbar and a hammer on top.
fn workbench() -> Sprite {
    let mut s = Sprite::new(TILE, TILE);
    let (hi, top, dark, leg) = (rgb(214, 164, 104), rgb(186, 136, 82), rgb(110, 74, 42), rgb(126, 86, 54));
    let outline = rgb(24, 20, 36);
    // Hammer lying on the top.
    for x in 4..8 {
        s.set(x, 4, rgb(138, 86, 52));
    }
    for (x, y) in [(8, 3), (9, 3), (8, 4), (9, 4), (10, 4)] {
        s.set(x, y, rgb(138, 146, 180));
    }
    s.set(8, 3, rgb(212, 220, 240));
    // Top: outline, highlight, plank, shadow.
    for x in 0..TILE {
        s.set(x, 5, outline);
        s.set(x, 6, hi);
        s.set(x, 7, top);
        s.set(x, 8, top);
        s.set(x, 9, dark);
    }
    s.set(0, 6, outline);
    s.set(15, 6, outline);
    s.set(0, 7, outline);
    s.set(15, 7, outline);
    s.set(0, 8, outline);
    s.set(15, 8, outline);
    for x in [5, 11] {
        s.set(x, 7, dark);
    }
    // Legs and the crossbar.
    for y in 10..TILE {
        for (x, c) in [(1, outline), (2, leg), (3, dark), (4, outline), (11, outline), (12, leg), (13, dark), (14, outline)] {
            s.set(x, y, c);
        }
    }
    for x in 4..12 {
        s.set(x, 12, dark);
        s.set(x, 13, outline);
    }
    s.set(4, 12, dark);
    s
}

/// Recolour the sword blade (greys + cyan edge in the PNG) to a tier's metal.
fn tint_sword(s: &Sprite, t: Tier) -> Sprite {
    let m = t.metal();
    let map = [
        (rgb(90, 240, 255), m[0]),
        (rgb(212, 220, 240), m[1]),
        (rgb(138, 146, 180), m[2]),
        (rgb(74, 78, 104), m[3]),
    ];
    let mut out = s.clone();
    for p in out.px.iter_mut() {
        if p[3] > 0 {
            if let Some(&(_, to)) = map.iter().find(|(from, _)| from[..3] == p[..3]) {
                *p = to;
            }
        }
    }
    out
}

// --- item icons (original pixel art; 1-4 = the armour tier's metal, light to dark) ---
const ICON_PAL: [(char, Rgba); 10] = [
    ('o', rgb(20, 18, 28)),
    ('w', rgb(196, 150, 96)),
    ('W', rgb(130, 90, 54)),
    ('g', rgb(90, 60, 40)),
    ('G', rgb(60, 40, 28)),
    ('s', rgb(232, 230, 220)),
    ('m', rgb(212, 220, 240)),
    ('M', rgb(138, 146, 180)),
    ('f', rgb(236, 84, 70)),
    ('F', rgb(170, 50, 50)),
];

pub const ICON_BOW: [&str; 14] = [
    "......ooo.....",
    ".....owWso....",
    "....owWoso....",
    "...owWooso....",
    "..owWo.oso....",
    "..owWo.oso....",
    ".ogGo..oso....",
    ".ogGo..oso....",
    "..owWo.oso....",
    "..owWo.oso....",
    "...owWooso....",
    "....owWoso....",
    ".....owWso....",
    "......ooo.....",
];
pub const ICON_ARROW: [&str; 14] = [
    "...........oo.",
    "..........oMmo",
    ".........ommMo",
    ".........omMo.",
    "........owoo..",
    ".......owo....",
    "......owo.....",
    ".....owo......",
    "....owo.......",
    ".ooowo........",
    "offwo.........",
    "ofFfo.........",
    "oFfFo.........",
    ".ooo..........",
];
pub const ICON_HELMET: [&str; 11] = [
    "....oooooo....",
    "...o222222o...",
    "..o21122223o..",
    ".o2112222233o.",
    ".o2122222233o.",
    "o222222222334o",
    "o333333333344o",
    "o34oooooooo44o",
    "o34o......o44o",
    "o34o......o4o.",
    ".oo........o..",
];
pub const ICON_CHEST: [&str; 13] = [
    ".ooo......ooo.",
    "o222o....o333o",
    "o2122oooo2334o",
    "o21122222233o.",
    ".o2122122333o.",
    ".o2222122333o.",
    ".o2222122333o.",
    ".o2222122334o.",
    ".o2222122334o.",
    ".o3333333444o.",
    ".o3333333444o.",
    "..o33333344o..",
    "...oooooooo...",
];


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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_sprites_decode_with_expected_sizes() {
        for (a, w, h) in [
            (PLAYER_IDLE, 32, 40),
            (PLAYER_WALK, 96, 40),
            (PLAYER_JUMP, 16, 40),
            (PLAYER_FALL, 16, 40),
            (SWORD, 14, 14),
            (SWORD_UP, 7, 14),
            (PICKAXE, 14, 14),
            (TORCH, 16, 16),
            (MOB_CRAWLER, 80, 14),
            (MOB_BAT, 54, 12),
        ]
        .into_iter()
        .chain(APP_ICONS.iter().map(|&a| (a, 16, 16)))
        {
            let s = decode_png(a.1).unwrap_or_else(|e| panic!("{}: {e}", a.0));
            assert_eq!((s.w, s.h), (w, h), "{}", a.0);
            assert!(s.px.iter().any(|p| p[3] == 255), "{} is empty", a.0);
        }
    }

    #[test]
    fn bad_png_falls_back_to_placeholder() {
        let s = load(("broken", b"not a png"), 5, 7);
        assert_eq!((s.w, s.h), (5, 7));
        let frames = load_strip(("broken", b"nope"), 4, 4, 3);
        assert_eq!(frames.len(), 3);
    }

    #[test]
    fn player_feet_touch_bottom_row() {
        let sp = Sprites::new();
        for f in sp.player_idle.iter().chain(&sp.player_walk).chain([&sp.player_jump, &sp.player_fall]) {
            assert_eq!((f.w, f.h), (PLAYER_W, PLAYER_H));
        }
        for f in sp.player_idle.iter().chain(&sp.player_walk) {
            let bottom = (0..f.w).any(|x| f.get(x, f.h - 1)[3] > 0);
            assert!(bottom, "grounded frames must reach the bottom row");
        }
        assert_eq!(sp.app_icons.len(), shared::APPS.len());
    }
}
