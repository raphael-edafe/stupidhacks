//! Tile world: storage, procedural generation, queries. Owner: person B1.

pub const TILE: i32 = 16; // view pixels per tile
pub const WORLD_W: i32 = 320; // tiles
pub const WORLD_H: i32 = 110; // tiles
pub const SURFACE: i32 = 48; // average surface row

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Tile {
    Air = 0,
    Dirt,
    Grass,
    Stone,
    Copper,
    Iron,
    Crystal,
    Log,
    Leaves,
    Plank,
    Torch,
    Bedrock,
    /// Crafting station (furniture: you walk through it). Press E near it to craft.
    Workbench,
    /// Closed door segment (doors are 3 tiles tall). Blocks the player and mobs.
    Door,
    /// Open door segment: walk-through.
    DoorOpen,
}

impl Tile {
    /// Blocks movement.
    pub fn solid(self) -> bool {
        !matches!(self, Tile::Air | Tile::Log | Tile::Leaves | Tile::Torch | Tile::Workbench | Tile::DoorOpen)
    }

    /// Seconds to mine with bare hands. `None` = unbreakable.
    pub fn hardness(self) -> Option<f32> {
        match self {
            Tile::Air | Tile::Bedrock => None,
            Tile::Torch => Some(0.05),
            Tile::Workbench | Tile::Door | Tile::DoorOpen => Some(0.3),
            Tile::Leaves => Some(0.12),
            Tile::Dirt | Tile::Grass => Some(0.25),
            Tile::Log | Tile::Plank => Some(0.35),
            Tile::Stone => Some(0.5),
            Tile::Copper => Some(0.6),
            Tile::Iron => Some(0.75),
            Tile::Crystal => Some(0.9),
        }
    }

    /// What you get when you mine it.
    pub fn drop(self) -> Tile {
        match self {
            Tile::Grass => Tile::Dirt,
            Tile::Log => Tile::Plank,
            Tile::DoorOpen => Tile::Door,
            t => t,
        }
    }

    /// Representative colour (particles, minimap, debris).
    pub fn color(self) -> crate::gfx::Rgba {
        use crate::gfx::rgb;
        match self {
            Tile::Air => rgb(0, 0, 0),
            Tile::Dirt => rgb(134, 88, 58),
            Tile::Grass => rgb(92, 178, 74),
            Tile::Stone => rgb(122, 124, 138),
            Tile::Copper => rgb(214, 128, 72),
            Tile::Iron => rgb(196, 190, 204),
            Tile::Crystal => rgb(120, 230, 255),
            Tile::Log => rgb(118, 80, 50),
            Tile::Leaves => rgb(58, 140, 70),
            Tile::Plank => rgb(186, 136, 82),
            Tile::Torch => rgb(255, 200, 90),
            Tile::Bedrock => rgb(40, 38, 48),
            Tile::Workbench => rgb(160, 112, 66),
            Tile::Door | Tile::DoorOpen => rgb(150, 100, 60),
        }
    }

    pub fn is_door(self) -> bool {
        matches!(self, Tile::Door | Tile::DoorOpen)
    }

    /// Light emitted (0..1).
    pub fn light(self) -> f32 {
        match self {
            Tile::Torch => 1.0,
            Tile::Crystal => 0.45,
            _ => 0.0,
        }
    }
}

/// Tree designs. Each has a canopy sprite, trunk segments and a root flare (see `Sprites`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum TreeKind {
    Oak = 0,
    Pine,
    Blossom,
    Autumn,
}

impl TreeKind {
    pub const COUNT: usize = 4;
}

/// A tree placed by worldgen. Its trunk is `Tile::Log` cells from `base_y - 1` up to `top_y`
/// in column `x` (choppable); the canopy is a sprite drawn while the top log still exists.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tree {
    pub x: i32,
    /// Row of the grass tile the tree stands on.
    pub base_y: i32,
    /// Row of the top log (canopy anchor).
    pub top_y: i32,
    pub kind: TreeKind,
}

/// Surface decoration kinds (no collision). Order = cells of `decor.png`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum DecorKind {
    TuftA = 0,
    TuftB,
    TuftC,
    FlowerRed,
    FlowerYellow,
    FlowerBlue,
    Pebbles,
    MushroomRed,
    MushroomBrown,
    Bush,
    BushBerry,
}

impl DecorKind {
    pub const COUNT: usize = 11;
    /// Plants that sway in the wind.
    pub fn sways(self) -> bool {
        matches!(
            self,
            DecorKind::TuftA
                | DecorKind::TuftB
                | DecorKind::TuftC
                | DecorKind::FlowerRed
                | DecorKind::FlowerYellow
                | DecorKind::FlowerBlue
        )
    }
}

/// A decoration standing in air cell (x, y) on top of the grass tile at (x, y + 1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Decor {
    pub x: i32,
    pub y: i32,
    pub kind: DecorKind,
    /// Per-decor randomness (sway phase, flip).
    pub seed: u8,
}

pub struct World {
    pub w: i32,
    pub h: i32,
    pub seed: u64,
    tiles: Vec<Tile>,
    /// Row of the original surface per column (for background walls / sky).
    pub surface: Vec<i32>,
    /// Bumped on every edit; renderers/light maps can cache on it.
    pub revision: u64,
    /// Trees (sorted by x). Their canopies are drawn while the top log exists.
    pub trees: Vec<Tree>,
    /// Surface decorations (sorted by x). Removed when their cell or the ground under them changes.
    pub decor: Vec<Decor>,
}

impl World {
    pub fn get(&self, x: i32, y: i32) -> Tile {
        if x < 0 || x >= self.w || y >= self.h {
            return Tile::Bedrock;
        }
        if y < 0 {
            return Tile::Air;
        }
        self.tiles[(y * self.w + x) as usize]
    }

    pub fn set(&mut self, x: i32, y: i32, t: Tile) {
        if x >= 0 && y >= 0 && x < self.w && y < self.h {
            self.tiles[(y * self.w + x) as usize] = t;
            self.revision += 1;
            // Decor in this cell, or standing on it, goes away for good.
            if !self.decor.is_empty() {
                self.decor.retain(|d| !(d.x == x && (d.y == y || d.y + 1 == y)));
            }
        }
    }

    /// The door column containing (x, y): (top row, bottom row), if it's a door tile.
    pub fn door_span(&self, x: i32, y: i32) -> Option<(i32, i32)> {
        if !self.get(x, y).is_door() {
            return None;
        }
        let mut top = y;
        while self.get(x, top - 1).is_door() && y - top < 8 {
            top -= 1;
        }
        let mut bottom = y;
        while self.get(x, bottom + 1).is_door() && bottom - y < 8 {
            bottom += 1;
        }
        Some((top, bottom))
    }

    /// Open a closed door / close an open one (the whole column). Returns the new state
    /// (true = open), or None if (x, y) isn't a door.
    pub fn toggle_door(&mut self, x: i32, y: i32) -> Option<bool> {
        let (top, bottom) = self.door_span(x, y)?;
        let open = self.get(x, y) == Tile::Door;
        for yy in top..=bottom {
            self.set(x, yy, if open { Tile::DoorOpen } else { Tile::Door });
        }
        Some(open)
    }

    /// Remove a whole door column. Returns true if there was one.
    pub fn remove_door(&mut self, x: i32, y: i32) -> bool {
        match self.door_span(x, y) {
            Some((top, bottom)) => {
                for yy in top..=bottom {
                    self.set(x, yy, Tile::Air);
                }
                true
            }
            None => false,
        }
    }

    pub fn solid(&self, x: i32, y: i32) -> bool {
        self.get(x, y).solid()
    }

    /// Solid at a world-pixel position.
    pub fn solid_px(&self, px: f32, py: f32) -> bool {
        self.solid((px / TILE as f32).floor() as i32, (py / TILE as f32).floor() as i32)
    }

    /// True if the tile is underground background (behind the original surface).
    pub fn is_underground(&self, x: i32, y: i32) -> bool {
        x >= 0 && x < self.w && y > self.surface[x as usize] + 1
    }

    /// Top solid tile row in a column (for spawning things on the ground).
    pub fn ground_y(&self, x: i32) -> i32 {
        (0..self.h).find(|&y| self.solid(x, y)).unwrap_or(self.h - 1)
    }

    pub fn width_px(&self) -> f32 {
        (self.w * TILE) as f32
    }
    pub fn height_px(&self) -> f32 {
        (self.h * TILE) as f32
    }

    pub fn generate(seed: u64) -> World {
        let (w, h) = (WORLD_W, WORLD_H);
        let mut tiles = vec![Tile::Air; (w * h) as usize];
        let mut surface = vec![SURFACE; w as usize];
        let center = w / 2;

        for x in 0..w {
            let fx = x as f32;
            let mut s = SURFACE as f32
                + fbm1(seed, fx / 48.0, 4) * 14.0
                + fbm1(seed ^ 0x55, fx / 14.0, 2) * 3.0;
            // Flatten a wide plateau around spawn so screens have room to be built.
            let d = (((x - center).abs() - 14) as f32 / 22.0).clamp(0.0, 1.0);
            let plateau = SURFACE as f32 - 2.0;
            s = plateau + (s - plateau) * smoothstep(d);
            surface[x as usize] = s.round() as i32;
        }

        for x in 0..w {
            let s = surface[x as usize];
            let dirt_depth = 4 + (hash2(seed, x, 7) * 4.0) as i32;
            for y in 0..h {
                let t = if y < s {
                    Tile::Air
                } else if y == h - 1 {
                    Tile::Bedrock
                } else if y == s {
                    Tile::Grass
                } else if y < s + dirt_depth {
                    Tile::Dirt
                } else {
                    Tile::Stone
                };
                tiles[(y * w + x) as usize] = t;
            }
        }

        let mut world = World { w, h, seed, tiles, surface, revision: 0, trees: Vec::new(), decor: Vec::new() };

        // Caves: worm-ish noise channels below the dirt, kept away from spawn's top.
        for y in 0..h - 1 {
            for x in 0..w {
                let s = world.surface[x as usize];
                if y < s + 6 {
                    continue;
                }
                let n = fbm2(seed ^ 0xC4FE, x as f32 / 22.0, y as f32 / 13.0, 3);
                let band = (n - 0.5).abs();
                let depth = ((y - s) as f32 / 30.0).min(1.0);
                if band < 0.035 + 0.03 * depth {
                    world.tiles[(y * w + x) as usize] = Tile::Air;
                }
                // Occasional big caverns deeper down.
                let big = fbm2(seed ^ 0xBEEF, x as f32 / 16.0, y as f32 / 10.0, 2);
                if y > s + 18 && big > 0.72 {
                    world.tiles[(y * w + x) as usize] = Tile::Air;
                }
            }
        }

        // Ore veins, deeper = rarer.
        let ores = [
            (Tile::Copper, 4, 0.74, 0x01),
            (Tile::Iron, 14, 0.78, 0x02),
            (Tile::Crystal, 26, 0.84, 0x03),
        ];
        for (ore, min_depth, thresh, salt) in ores {
            for y in 0..h - 1 {
                for x in 0..w {
                    let i = (y * w + x) as usize;
                    if world.tiles[i] != Tile::Stone {
                        continue;
                    }
                    if y < world.surface[x as usize] + min_depth {
                        continue;
                    }
                    let n = fbm2(seed ^ (salt * 0x9E37), x as f32 / 5.0, y as f32 / 5.0, 2);
                    if n > thresh {
                        world.tiles[i] = ore;
                    }
                }
            }
        }

        // Trees on grass, spaced out, none right at spawn.
        let mut x = 3;
        while x < w - 3 {
            let r = hash2(seed, x, 99);
            if (x - center).abs() > 6 && r > 0.55 {
                let gy = world.surface[x as usize];
                if world.get(x, gy) == Tile::Grass {
                    let k = hash2(seed, x, 11);
                    let kind = if k < 0.36 {
                        TreeKind::Oak
                    } else if k < 0.64 {
                        TreeKind::Pine
                    } else if k < 0.84 {
                        TreeKind::Blossom
                    } else {
                        TreeKind::Autumn
                    };
                    let (min_h, span) = match kind {
                        TreeKind::Blossom => (2, 2.0),
                        TreeKind::Pine => (3, 3.0),
                        _ => (3, 2.0),
                    };
                    let height = min_h + (hash2(seed, x, 3) * span) as i32;
                    for t in 1..=height {
                        world.set(x, gy - t, Tile::Log);
                    }
                    world.trees.push(Tree { x, base_y: gy, top_y: gy - height, kind });
                }
                x += 6 + (hash2(seed, x, 5) * 6.0) as i32;
            } else {
                x += 2;
            }
        }

        // Surface decorations on open grass tops (not on or right next to a trunk).
        for x in 1..w - 1 {
            let gy = world.surface[x as usize];
            if world.get(x, gy) != Tile::Grass || world.get(x, gy - 1) != Tile::Air {
                continue;
            }
            let tree_d = world.trees.iter().map(|t| (t.x - x).abs()).min().unwrap_or(99);
            if tree_d == 0 {
                continue;
            }
            let r = hash2(seed ^ 0xDEC0, x, gy);
            let pick = hash2(seed ^ 0xDEC1, x, gy);
            let kind = if r < 0.30 {
                [DecorKind::TuftA, DecorKind::TuftB, DecorKind::TuftC][(pick * 3.0) as usize % 3]
            } else if r < 0.42 {
                [DecorKind::FlowerRed, DecorKind::FlowerYellow, DecorKind::FlowerBlue][(pick * 3.0) as usize % 3]
            } else if r < 0.46 {
                DecorKind::Pebbles
            } else if r < 0.49 || (tree_d <= 2 && r < 0.60) {
                if pick < 0.5 {
                    DecorKind::MushroomRed
                } else {
                    DecorKind::MushroomBrown
                }
            } else if r < 0.55 && tree_d > 1 {
                if pick < 0.6 {
                    DecorKind::Bush
                } else {
                    DecorKind::BushBerry
                }
            } else {
                continue;
            };
            let s = (hash2(seed ^ 0xDEC2, x, gy) * 256.0) as u8;
            world.decor.push(Decor { x, y: gy - 1, kind, seed: s });
        }

        world.revision = 0;
        world
    }
}

// ---------- deterministic noise ----------

/// Hash (seed, x, y) -> [0, 1).
pub fn hash2(seed: u64, x: i32, y: i32) -> f32 {
    let mut h = seed
        ^ (x as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ (y as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F);
    h ^= h >> 33;
    h = h.wrapping_mul(0xFF51_AFD7_ED55_8CCD);
    h ^= h >> 33;
    h = h.wrapping_mul(0xC4CE_B9FE_1A85_EC53);
    h ^= h >> 33;
    (h >> 40) as f32 / (1u64 << 24) as f32
}

fn smoothstep(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn value1(seed: u64, x: f32) -> f32 {
    let i = x.floor() as i32;
    let f = smoothstep(x - i as f32);
    let a = hash2(seed, i, 0);
    let b = hash2(seed, i + 1, 0);
    a + (b - a) * f
}

/// 1D fractal noise in roughly [-1, 1].
pub fn fbm1(seed: u64, x: f32, octaves: u32) -> f32 {
    let (mut sum, mut amp, mut freq, mut norm) = (0.0, 1.0, 1.0, 0.0);
    for o in 0..octaves {
        sum += (value1(seed.wrapping_add(o as u64 * 1013), x * freq) * 2.0 - 1.0) * amp;
        norm += amp;
        amp *= 0.5;
        freq *= 2.0;
    }
    sum / norm
}

fn value2(seed: u64, x: f32, y: f32) -> f32 {
    let (ix, iy) = (x.floor() as i32, y.floor() as i32);
    let (fx, fy) = (smoothstep(x - ix as f32), smoothstep(y - iy as f32));
    let a = hash2(seed, ix, iy);
    let b = hash2(seed, ix + 1, iy);
    let c = hash2(seed, ix, iy + 1);
    let d = hash2(seed, ix + 1, iy + 1);
    let top = a + (b - a) * fx;
    let bot = c + (d - c) * fx;
    top + (bot - top) * fy
}

/// 2D fractal noise in [0, 1].
pub fn fbm2(seed: u64, x: f32, y: f32, octaves: u32) -> f32 {
    let (mut sum, mut amp, mut freq, mut norm) = (0.0, 1.0, 1.0, 0.0);
    for o in 0..octaves {
        sum += value2(seed.wrapping_add(o as u64 * 7919), x * freq, y * freq) * amp;
        norm += amp;
        amp *= 0.5;
        freq *= 2.0;
    }
    sum / norm
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generation_is_deterministic() {
        let a = World::generate(42);
        let b = World::generate(42);
        assert_eq!(a.tiles, b.tiles);
        assert_eq!(a.trees, b.trees);
        assert_eq!(a.decor, b.decor);
        let c = World::generate(43);
        assert_ne!(a.tiles, c.tiles);
    }

    #[test]
    fn trees_have_logs_and_no_leaf_blocks() {
        let w = World::generate(42);
        assert!(w.trees.len() >= 10, "trees: {}", w.trees.len());
        assert!(!w.tiles.contains(&Tile::Leaves));
        for t in &w.trees {
            assert_eq!(w.get(t.x, t.base_y), Tile::Grass);
            for y in t.top_y..t.base_y {
                assert_eq!(w.get(t.x, y), Tile::Log);
            }
            assert_ne!(w.get(t.x, t.top_y - 1), Tile::Log);
        }
        assert!(w.trees.windows(2).all(|p| p[0].x < p[1].x));
    }

    #[test]
    fn decor_sits_on_grass_and_goes_when_ground_is_mined() {
        let mut w = World::generate(42);
        assert!(w.decor.len() > 40, "decor: {}", w.decor.len());
        for d in &w.decor {
            assert_eq!(w.get(d.x, d.y), Tile::Air);
            assert_eq!(w.get(d.x, d.y + 1), Tile::Grass);
        }
        let d = w.decor[0];
        w.set(d.x, d.y + 1, Tile::Air);
        assert!(!w.decor.iter().any(|e| e.x == d.x && e.y == d.y));
    }

    #[test]
    fn spawn_area_is_flat_and_open() {
        let w = World::generate(42);
        let c = (w.w / 2) as usize;
        for x in c - 10..c + 10 {
            assert!((w.surface[x] - w.surface[c]).abs() <= 1, "column {x} not flat");
        }
        // Plenty of sky above spawn for building screens.
        for y in 0..w.surface[c] - 1 {
            assert_eq!(w.get(c as i32, y), Tile::Air);
        }
    }

    #[test]
    fn has_ores_and_caves() {
        let w = World::generate(42);
        let count = |t| w.tiles.iter().filter(|&&x| x == t).count();
        assert!(count(Tile::Copper) > 50);
        assert!(count(Tile::Iron) > 20);
        assert!(count(Tile::Crystal) > 5);
        let underground_air = (0..w.h)
            .flat_map(|y| (0..w.w).map(move |x| (x, y)))
            .filter(|&(x, y)| w.is_underground(x, y) && w.get(x, y) == Tile::Air)
            .count();
        assert!(underground_air > 200, "caves: {underground_air}");
    }
}
