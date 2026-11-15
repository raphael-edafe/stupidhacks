//! Crafting at a workbench.
//!
//! Stand near a workbench and press E: a panel (styled like the app chooser)
//! lists every recipe with its cost. Click a row, or pick one with Up/Down and
//! press Enter, to craft it. Under the recipes is everything you own: hover an
//! item and press 1-6 to put it in that hotbar slot. Esc or E closes the panel.
//! While it's open the player stands still. In god mode crafting is free.
//! E only opens crafting when no screen prompt wants it.

use crate::camera::Camera;
use crate::gfx::{lerp_color, rgb, rgba, text_width, Canvas};
use crate::input::Controls;
use crate::items::{Item, Recipe, ALL as ALL_ITEMS, RECIPES};
use crate::particles::Particles;
use crate::player::Player;
use crate::sprites::Sprites;
use crate::ui::{self, fade};
use crate::world::{Tile, World, TILE};
use shared::Key;

/// How close (tiles, from the player's centre) a workbench must be.
const REACH: i32 = 3;
const PANEL_W: i32 = 212;
const TITLE_H: i32 = 20;
const ROW_H: i32 = 17;
const STRIP_CELL: i32 = 19;
const FOOTER_H: i32 = 16;
/// Seconds to open / close.
const ANIM_TIME: f32 = 0.18;

pub struct Crafting {
    pub open: bool,
    /// 0 closed .. 1 fully open.
    anim: f32,
    /// Keyboard cursor row.
    cursor: usize,
    /// Row under the mouse.
    hover: Option<usize>,
    /// Owned item under the mouse in the inventory strip.
    inv_hover: Option<Item>,
    /// Nearest workbench within reach (tile).
    pub near: Option<(i32, i32)>,
    /// Row that was just crafted, and how long ago (feedback flash).
    flash: Option<(usize, f32)>,
    /// Panel rect from the last draw (view px), for mouse picking.
    rect: (i32, i32, i32, i32),
}

impl Default for Crafting {
    fn default() -> Self {
        Self::new()
    }
}

impl Crafting {
    pub fn new() -> Self {
        Crafting { open: false, anim: 0.0, cursor: 0, hover: None, inv_hover: None, near: None, flash: None, rect: (0, 0, 0, 0) }
    }

    /// Whether the player can make `r` right now.
    pub fn affordable(r: &Recipe, p: &Player) -> bool {
        p.god || r.cost.iter().all(|&(it, n)| p.count(it) >= n)
    }

    /// Craft recipe `i`. False if it can't be afforded. New armour is worn if it's better.
    pub fn craft(i: usize, p: &mut Player) -> bool {
        let Some(r) = RECIPES.get(i) else { return false };
        if !Self::affordable(r, p) {
            return false;
        }
        for &(it, n) in r.cost {
            p.take(it, n);
        }
        p.give(r.out, r.n);
        p.equip_if_better(r.out);
        true
    }

    /// The panel rect for a view of `view` px (also fits the 640x360 close view).
    fn layout(view: (i32, i32)) -> (i32, i32, i32, i32) {
        let rows = RECIPES.len() as i32;
        let h = TITLE_H + 4 + rows * ROW_H + 6 + STRIP_CELL * 2 + 4 + FOOTER_H;
        ((view.0 - PANEL_W) / 2, ((view.1 - h) / 2).max(4), PANEL_W, h)
    }

    /// One tick. `blocked`: a screen prompt or another modal UI owns E / the
    /// mouse. Returns true while the panel is open (the player is frozen).
    pub fn update(&mut self, dt: f32, ctl: &Controls, p: &mut Player, world: &World, fx: &mut Particles, view: (i32, i32), blocked: bool) -> bool {
        self.near = nearest_bench(p, world);
        self.anim = if self.open { (self.anim + dt / ANIM_TIME).min(1.0) } else { (self.anim - dt / ANIM_TIME).max(0.0) };
        if let Some((row, t)) = self.flash {
            self.flash = if t + dt < 0.4 { Some((row, t + dt)) } else { None };
        }
        if !self.open {
            if self.near.is_some() && !blocked && ctl.pressed(Key::Interact) {
                self.open = true;
                self.cursor = self.cursor.min(RECIPES.len() - 1);
            }
            return false;
        }
        if self.near.is_none() || ctl.pressed(Key::Escape) || ctl.pressed(Key::Interact) {
            self.open = false;
            return false;
        }

        self.rect = Self::layout(view);
        let (x, y, w, _) = self.rect;
        let (mx, my) = ctl.mouse();
        let (mx, my) = (mx as i32, my as i32);
        let rows_top = y + TITLE_H + 4;
        self.hover = None;
        if mx >= x && mx < x + w && my >= rows_top && my < rows_top + RECIPES.len() as i32 * ROW_H {
            self.hover = Some(((my - rows_top) / ROW_H) as usize);
        }
        if ctl.pressed(Key::Up) {
            self.cursor = (self.cursor + RECIPES.len() - 1) % RECIPES.len();
        }
        if ctl.pressed(Key::Down) {
            self.cursor = (self.cursor + 1) % RECIPES.len();
        }
        if let Some(h) = self.hover {
            self.cursor = h;
        }
        let pick = if ctl.lmb_pressed() { self.hover } else if ctl.pressed(Key::Enter) { Some(self.cursor) } else { None };
        if let Some(i) = pick {
            if Self::craft(i, p) {
                self.flash = Some((i, 0.0));
                let (cx, cy) = p.center();
                fx.sparkle(cx, cy - 8.0, rgb(255, 230, 140), 10);
            }
        }

        // Inventory strip: hover an item, press 1-6 to put it in the hotbar.
        self.inv_hover = None;
        let strip_top = rows_top + RECIPES.len() as i32 * ROW_H + 6;
        for (i, it) in owned(p).enumerate() {
            let (cx, cy) = strip_cell(x, strip_top, i);
            if mx >= cx && mx < cx + STRIP_CELL && my >= cy && my < cy + STRIP_CELL {
                self.inv_hover = Some(it);
            }
        }
        if let Some(it) = self.inv_hover {
            let keys = [Key::Hotbar1, Key::Hotbar2, Key::Hotbar3, Key::Hotbar4, Key::Hotbar5, Key::Hotbar6];
            if let Some(slot) = keys.iter().position(|k| ctl.pressed(*k)) {
                p.assign(slot, it);
            }
        }
        true
    }

    /// "E CRAFT" over a nearby workbench (world-space prompt).
    pub fn draw_prompt(&self, c: &mut Canvas, cam: &Camera, blocked: bool) {
        if self.open || blocked {
            return;
        }
        if let Some((tx, ty)) = self.near {
            let (vx, vy) = cam.to_view((tx * TILE + TILE / 2) as f32, (ty * TILE) as f32);
            ui::draw_prompt(c, vx, vy - 24, &[("E", "CRAFT")], rgb(255, 200, 110), 1.0);
        }
    }

    pub fn draw(&self, c: &mut Canvas, p: &Player, sprites: &Sprites) {
        if self.anim <= 0.0 {
            return;
        }
        let e = ui::ease_out_cubic(self.anim);
        let a = e;
        let (x, y0, w, h) = Self::layout((c.w, c.h));
        let y = y0 + ((1.0 - e) * 14.0) as i32;

        // Frame: shadow, outline with clipped corners, title gradient, body.
        let outline = rgb(10, 12, 20);
        c.fill_rect(x + 4, y + 4, w, h, fade(rgba(0, 0, 0, 100), a));
        c.fill_rect(x + 1, y, w - 2, h, fade(outline, a));
        c.fill_rect(x, y + 1, w, h - 2, fade(outline, a));
        for yy in 1..h - 1 {
            let col = if yy < TITLE_H {
                lerp_color(rgb(176, 112, 52), rgb(130, 76, 34), yy as f32 / TITLE_H as f32)
            } else {
                lerp_color(rgb(34, 36, 54), rgb(20, 22, 34), yy as f32 / h as f32)
            };
            c.fill_rect(x + 1, y + yy, w - 2, 1, fade(col, a));
        }
        c.fill_rect(x + 1, y + 1, w - 2, 1, fade(rgb(240, 190, 120), a));
        c.text_shadow(x + 8, y + 7, "WORKBENCH", 1, fade(rgb(255, 246, 226), a));
        if p.god {
            c.text_shadow(x + w - 8 - text_width("FREE (GOD)", 1), y + 7, "FREE (GOD)", 1, fade(rgb(255, 220, 90), a));
        }
        if self.anim < 0.6 {
            return;
        }

        // Recipes.
        let rows_top = y + TITLE_H + 4;
        for (i, r) in RECIPES.iter().enumerate() {
            let ry = rows_top + i as i32 * ROW_H;
            let ok = Self::affordable(r, p);
            if i == self.cursor {
                c.fill_rect(x + 3, ry, w - 6, ROW_H - 1, fade(rgba(255, 200, 110, 44), a));
                c.fill_rect(x + 3, ry, 2, ROW_H - 1, fade(rgb(255, 200, 110), a));
            }
            if let Some((row, t)) = self.flash {
                if row == i {
                    c.fill_rect(x + 3, ry, w - 6, ROW_H - 1, rgba(255, 255, 255, ((1.0 - t / 0.4) * 120.0) as u8));
                }
            }
            let icon = sprites.icon(r.out);
            c.sprite(icon, x + 8 + 8 - icon.w / 2, ry + ROW_H / 2 - icon.h / 2, false);
            let name = if r.n > 1 { format!("{} X{}", r.out.name(), r.n) } else { r.out.name().to_string() };
            let name_col = if ok { rgb(240, 244, 255) } else { rgb(120, 124, 144) };
            c.text_shadow(x + 28, ry + 6, &name, 1, fade(name_col, a));
            // Cost, right-aligned: [icon]n [icon]n
            let mut cx = x + w - 6;
            for &(it, n) in r.cost.iter().rev() {
                let label = n.to_string();
                let short = !p.god && p.count(it) < n;
                cx -= text_width(&label, 1);
                c.text_shadow(cx, ry + 6, &label, 1, fade(if short { rgb(255, 96, 96) } else { rgb(200, 210, 230) }, a));
                let ic = sprites.icon(it);
                cx -= 13;
                draw_small(c, ic, cx, ry + 2);
                cx -= 4;
            }
        }

        // Inventory strip.
        let strip_top = rows_top + RECIPES.len() as i32 * ROW_H + 6;
        c.fill_rect(x + 3, strip_top - 3, w - 6, 1, fade(rgb(60, 66, 92), a));
        for (i, it) in owned(p).enumerate() {
            let (cx, cy) = strip_cell(x, strip_top, i);
            let hov = self.inv_hover == Some(it);
            c.fill_rect(cx, cy, STRIP_CELL - 1, STRIP_CELL - 1, fade(if hov { rgba(255, 200, 110, 70) } else { rgba(255, 255, 255, 14) }, a));
            draw_small(c, sprites.icon(it), cx + 3, cy + 3);
            if !it.is_tool() && p.count(it) > 1 {
                let s = p.count(it).to_string();
                c.text_shadow(cx + STRIP_CELL - 2 - text_width(&s, 1), cy + STRIP_CELL - 7, &s, 1, fade(rgb(255, 255, 255), a));
            }
        }

        // Footer: what the keys do (or the hovered item's name).
        let fy = y + h - FOOTER_H + 4;
        if let Some(it) = self.inv_hover {
            c.text_shadow(x + 8, fy, it.name(), 1, fade(rgb(255, 230, 170), a));
            let hint = "PRESS 1-6: HOTBAR";
            c.text_shadow(x + w - 8 - text_width(hint, 1), fy, hint, 1, fade(rgb(170, 180, 205), a));
        } else {
            let mut fx = x + 8;
            for (k, label) in [("ENTER", "CRAFT"), ("1-6", "HOTBAR"), ("ESC", "CLOSE")] {
                fx += ui::draw_keycap(c, fx, fy - 3, k, a) + 3;
                fx += c.text_shadow(fx, fy, label, 1, fade(rgb(200, 208, 228), a)) - fx + 8;
            }
        }
    }
}

/// Items the player owns, in item order.
fn owned(p: &Player) -> impl Iterator<Item = Item> + '_ {
    ALL_ITEMS.iter().copied().filter(move |&it| p.count(it) > 0)
}

/// Top-left of inventory strip cell `i` (two rows of 11).
fn strip_cell(px: i32, top: i32, i: usize) -> (i32, i32) {
    let per_row = 11;
    (px + 4 + (i % per_row) as i32 * STRIP_CELL, top + (i / per_row) as i32 * STRIP_CELL)
}

/// Draw an icon shrunk to at most 13x13 (every other pixel of bigger ones).
fn draw_small(c: &mut Canvas, s: &crate::gfx::Sprite, x: i32, y: i32) {
    if s.w <= 13 && s.h <= 13 {
        c.sprite(s, x + (13 - s.w) / 2, y + (13 - s.h) / 2, false);
        return;
    }
    // 16 -> 13: drop 3 evenly spaced rows/columns.
    for dy in 0..13 {
        for dx in 0..13 {
            let sx = dx * s.w / 13;
            let sy = dy * s.h / 13;
            let px = s.get(sx, sy);
            if px[3] > 0 {
                c.blend(x + dx, y + dy, px);
            }
        }
    }
}

/// The closest workbench tile within REACH of the player, if any.
fn nearest_bench(p: &Player, world: &World) -> Option<(i32, i32)> {
    let (cx, cy) = p.center();
    let (ptx, pty) = ((cx / TILE as f32).floor() as i32, (cy / TILE as f32).floor() as i32);
    let mut best: Option<((i32, i32), i32)> = None;
    for ty in pty - REACH..=pty + REACH {
        for tx in ptx - REACH..=ptx + REACH {
            if world.get(tx, ty) == Tile::Workbench {
                let d = (tx - ptx).abs() + (ty - pty).abs();
                if best.is_none_or(|(_, bd)| d < bd) {
                    best = Some(((tx, ty), d));
                }
            }
        }
    }
    best.map(|(t, _)| t)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::World;

    fn row(out: Item) -> usize {
        RECIPES.iter().position(|r| r.out == out).unwrap()
    }

    #[test]
    fn crafting_spends_ingredients_and_fails_when_short() {
        let mut p = Player::new(&World::generate(1));
        let planks = p.count(Item::Plank);
        let torches = p.count(Item::Torch);
        assert!(Crafting::craft(row(Item::Torch), &mut p));
        assert_eq!(p.count(Item::Plank), planks - 1);
        assert_eq!(p.count(Item::Torch), torches + 3);
        assert!(!Crafting::craft(row(Item::CrystalSword), &mut p), "no crystal yet");
        assert_eq!(p.count(Item::CrystalSword), 0);
    }

    #[test]
    fn god_mode_crafts_for_free() {
        let mut p = Player::new(&World::generate(1));
        p.god = true;
        assert!(Crafting::craft(row(Item::CrystalSword), &mut p));
        assert_eq!(p.count(Item::CrystalSword), 1);
        assert!(p.hotbar.contains(&Some(Item::CrystalSword)) || p.hotbar.iter().all(|s| s.is_some()));
    }

    #[test]
    fn crafted_armour_is_worn() {
        let mut p = Player::new(&World::generate(1));
        p.give(Item::Copper, 6);
        assert!(Crafting::craft(row(Item::CopperHelmet), &mut p));
        assert_eq!(p.helmet, Some(crate::items::Tier::Copper));
        assert!(p.defense() > 0);
    }
}
