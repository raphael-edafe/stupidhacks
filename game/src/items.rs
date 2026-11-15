//! Items: everything that can sit in the inventory and the 6-slot hotbar
//! (blocks, torches, the workbench, swords, the bow and arrows, armour).

use crate::gfx::{rgb, Rgba};
use crate::world::Tile;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Item {
    Dirt,
    Stone,
    Plank,
    Copper,
    Iron,
    Crystal,
    Torch,
    Workbench,
    WoodSword,
    CopperSword,
    IronSword,
    CrystalSword,
    Bow,
    Arrow,
    CopperHelmet,
    IronHelmet,
    CrystalHelmet,
    CopperChest,
    IronChest,
    CrystalChest,
}

pub const COUNT: usize = 20;

pub const ALL: [Item; COUNT] = [
    Item::Dirt,
    Item::Stone,
    Item::Plank,
    Item::Copper,
    Item::Iron,
    Item::Crystal,
    Item::Torch,
    Item::Workbench,
    Item::WoodSword,
    Item::CopperSword,
    Item::IronSword,
    Item::CrystalSword,
    Item::Bow,
    Item::Arrow,
    Item::CopperHelmet,
    Item::IronHelmet,
    Item::CrystalHelmet,
    Item::CopperChest,
    Item::IronChest,
    Item::CrystalChest,
];

/// Material tier for swords (Wood..Crystal) and armour (Copper..Crystal).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Tier {
    Wood,
    Copper,
    Iron,
    Crystal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Slot {
    Head,
    Body,
}

impl Item {
    pub fn name(self) -> &'static str {
        match self {
            Item::Dirt => "DIRT",
            Item::Stone => "STONE",
            Item::Plank => "PLANKS",
            Item::Copper => "COPPER",
            Item::Iron => "IRON",
            Item::Crystal => "CRYSTAL",
            Item::Torch => "TORCH",
            Item::Workbench => "WORKBENCH",
            Item::WoodSword => "WOOD SWORD",
            Item::CopperSword => "COPPER SWORD",
            Item::IronSword => "IRON SWORD",
            Item::CrystalSword => "CRYSTAL SWORD",
            Item::Bow => "BOW",
            Item::Arrow => "ARROWS",
            Item::CopperHelmet => "COPPER HELMET",
            Item::IronHelmet => "IRON HELMET",
            Item::CrystalHelmet => "CRYSTAL HELMET",
            Item::CopperChest => "COPPER CHESTPLATE",
            Item::IronChest => "IRON CHESTPLATE",
            Item::CrystalChest => "CRYSTAL CHESTPLATE",
        }
    }

    /// The block this item places, if it's placeable.
    pub fn tile(self) -> Option<Tile> {
        match self {
            Item::Dirt => Some(Tile::Dirt),
            Item::Stone => Some(Tile::Stone),
            Item::Plank => Some(Tile::Plank),
            Item::Copper => Some(Tile::Copper),
            Item::Iron => Some(Tile::Iron),
            Item::Crystal => Some(Tile::Crystal),
            Item::Torch => Some(Tile::Torch),
            Item::Workbench => Some(Tile::Workbench),
            _ => None,
        }
    }

    /// What mining a tile gives you (after `Tile::drop`, so logs give planks).
    pub fn from_tile(t: Tile) -> Option<Item> {
        match t.drop() {
            Tile::Dirt | Tile::Grass => Some(Item::Dirt),
            Tile::Stone => Some(Item::Stone),
            Tile::Plank | Tile::Log => Some(Item::Plank),
            Tile::Copper => Some(Item::Copper),
            Tile::Iron => Some(Item::Iron),
            Tile::Crystal => Some(Item::Crystal),
            Tile::Torch => Some(Item::Torch),
            Tile::Workbench => Some(Item::Workbench),
            Tile::Air | Tile::Leaves | Tile::Bedrock => None,
        }
    }

    pub fn sword(self) -> Option<Tier> {
        match self {
            Item::WoodSword => Some(Tier::Wood),
            Item::CopperSword => Some(Tier::Copper),
            Item::IronSword => Some(Tier::Iron),
            Item::CrystalSword => Some(Tier::Crystal),
            _ => None,
        }
    }

    pub fn armour(self) -> Option<(Slot, Tier)> {
        match self {
            Item::CopperHelmet => Some((Slot::Head, Tier::Copper)),
            Item::IronHelmet => Some((Slot::Head, Tier::Iron)),
            Item::CrystalHelmet => Some((Slot::Head, Tier::Crystal)),
            Item::CopperChest => Some((Slot::Body, Tier::Copper)),
            Item::IronChest => Some((Slot::Body, Tier::Iron)),
            Item::CrystalChest => Some((Slot::Body, Tier::Crystal)),
            _ => None,
        }
    }

    /// Tools and weapons are never used up (the hotbar slot stays when you have one).
    pub fn is_tool(self) -> bool {
        self.sword().is_some() || self == Item::Bow
    }

    /// Belongs in the hotbar (armour is worn instead).
    pub fn hotbar(self) -> bool {
        self.armour().is_none()
    }
}

impl Tier {
    pub const SWORDS: [Tier; 4] = [Tier::Wood, Tier::Copper, Tier::Iron, Tier::Crystal];

    pub fn sword_item(self) -> Item {
        match self {
            Tier::Wood => Item::WoodSword,
            Tier::Copper => Item::CopperSword,
            Tier::Iron => Item::IronSword,
            Tier::Crystal => Item::CrystalSword,
        }
    }

    /// Sword damage per hit (zombies have 3+ HP, bats 1).
    pub fn damage(self) -> i32 {
        match self {
            Tier::Wood => 1,
            Tier::Copper => 2,
            Tier::Iron => 3,
            Tier::Crystal => 4,
        }
    }

    /// Width of the sword's hitbox (px).
    pub fn reach(self) -> f32 {
        match self {
            Tier::Wood => 32.0,
            Tier::Copper => 36.0,
            Tier::Iron => 40.0,
            Tier::Crystal => 46.0,
        }
    }

    /// Seconds per swing.
    pub fn swing_time(self) -> f32 {
        match self {
            Tier::Wood => 0.28,
            Tier::Copper => 0.26,
            Tier::Iron => 0.24,
            Tier::Crystal => 0.22,
        }
    }

    /// Armour defence for a piece in `slot` (damage taken = max(1, dmg - total)).
    pub fn defense(self, slot: Slot) -> i32 {
        match (self, slot) {
            (Tier::Wood, _) => 0,
            (Tier::Copper, _) => 1,
            (Tier::Iron, Slot::Head) => 1,
            (Tier::Iron, Slot::Body) => 2,
            (Tier::Crystal, Slot::Head) => 2,
            (Tier::Crystal, Slot::Body) => 3,
        }
    }

    /// Metal shades, light to dark: [highlight, light, mid, dark].
    pub fn metal(self) -> [Rgba; 4] {
        match self {
            Tier::Wood => [rgb(226, 186, 128), rgb(196, 150, 96), rgb(160, 114, 70), rgb(110, 72, 44)],
            Tier::Copper => [rgb(255, 206, 150), rgb(240, 166, 108), rgb(204, 118, 70), rgb(130, 64, 34)],
            Tier::Iron => [rgb(244, 246, 255), rgb(212, 220, 240), rgb(138, 146, 180), rgb(74, 78, 104)],
            Tier::Crystal => [rgb(220, 255, 255), rgb(130, 232, 255), rgb(70, 170, 230), rgb(36, 92, 160)],
        }
    }

    /// Colour of the sword's swing trail.
    pub fn trail(self) -> Rgba {
        match self {
            Tier::Wood => rgb(240, 214, 160),
            Tier::Copper => rgb(255, 170, 100),
            Tier::Iron => rgb(220, 232, 255),
            Tier::Crystal => rgb(110, 240, 255),
        }
    }
}

/// A crafting recipe: `out` × `n` for the listed ingredients.
#[derive(Clone, Copy, Debug)]
pub struct Recipe {
    pub out: Item,
    pub n: u32,
    pub cost: &'static [(Item, u32)],
}

/// Everything the workbench can make, in menu order.
pub const RECIPES: &[Recipe] = &[
    Recipe { out: Item::Torch, n: 3, cost: &[(Item::Plank, 1)] },
    Recipe { out: Item::Workbench, n: 1, cost: &[(Item::Plank, 8)] },
    Recipe { out: Item::Arrow, n: 10, cost: &[(Item::Plank, 2), (Item::Stone, 1)] },
    Recipe { out: Item::Bow, n: 1, cost: &[(Item::Plank, 10), (Item::Copper, 2)] },
    Recipe { out: Item::CopperSword, n: 1, cost: &[(Item::Copper, 8), (Item::Plank, 2)] },
    Recipe { out: Item::IronSword, n: 1, cost: &[(Item::Iron, 8), (Item::Plank, 2)] },
    Recipe { out: Item::CrystalSword, n: 1, cost: &[(Item::Crystal, 6), (Item::Iron, 4)] },
    Recipe { out: Item::CopperHelmet, n: 1, cost: &[(Item::Copper, 6)] },
    Recipe { out: Item::CopperChest, n: 1, cost: &[(Item::Copper, 10)] },
    Recipe { out: Item::IronHelmet, n: 1, cost: &[(Item::Iron, 6)] },
    Recipe { out: Item::IronChest, n: 1, cost: &[(Item::Iron, 10)] },
    Recipe { out: Item::CrystalHelmet, n: 1, cost: &[(Item::Crystal, 5), (Item::Iron, 2)] },
    Recipe { out: Item::CrystalChest, n: 1, cost: &[(Item::Crystal, 8), (Item::Iron, 4)] },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_is_complete_and_ordered() {
        for (i, it) in ALL.iter().enumerate() {
            assert_eq!(*it as usize, i);
        }
    }

    #[test]
    fn placeable_items_round_trip_through_tiles() {
        for it in ALL {
            if let Some(t) = it.tile() {
                assert_eq!(Item::from_tile(t), Some(it), "{it:?}");
            }
        }
    }
}
