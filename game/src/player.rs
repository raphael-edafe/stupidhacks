//! Player: movement, mining, placing, sword swing. Owner: person B1.

use crate::camera::Camera;
use crate::gfx::{rgb, rgba, with_alpha, Canvas};
use crate::input::Controls;
use crate::items::{self, Item, Slot, Tier};
use crate::particles::Particles;
use crate::sprites::{Sprites, PLAYER_H, PLAYER_W, SWORD_GRIPS};
use crate::world::{Tile, World, TILE};
use shared::Key;

/// Hitbox size, world px. The 16x40 sprite is centred on it with its feet on the bottom edge
/// (the top of the hard hat pokes 3 px above the box).
pub const W: f32 = 12.0;
pub const H: f32 = 37.0;
/// Jump tuning: ~54 px apex (3 tiles plus margin) in ~0.3 s.
const JUMP_V: f32 = 380.0;
const GRAVITY: f32 = 1250.0;
const MAX_FALL: f32 = 560.0;
/// Hand (front glove centre) in sprite px, facing right.
const HAND: (i32, i32) = (9, 24);
/// Shoulder in sprite px (sword arc centre).
const SHOULDER: (i32, i32) = (9, 15);
const REACH: f32 = 6.0 * TILE as f32;
const SWING_TIME: f32 = 0.28;

/// Hotbar slots (keys 1-6).
pub const HOTBAR_SLOTS: usize = 6;

pub struct Player {
    /// Top-left of the hitbox, world px.
    pub x: f32,
    pub y: f32,
    pub vx: f32,
    pub vy: f32,
    pub on_ground: bool,
    pub facing: f32,
    pub hp: i32,
    pub max_hp: i32,
    pub invuln: f32,
    pub swing: f32,
    pub god: bool,
    /// Item counts, indexed by `Item as usize`.
    pub inventory: [u32; items::COUNT],
    /// The hotbar: any item kind per slot (blocks, tools, weapons).
    pub hotbar: [Option<Item>; HOTBAR_SLOTS],
    pub selected: usize,
    /// Seconds since the selection changed (the HUD shows the item name for a moment).
    pub select_age: f32,
    /// Worn armour.
    pub helmet: Option<Tier>,
    pub chest: Option<Tier>,
    pub spawn: (f32, f32),
    anim: f32,
    /// Seconds since spawn (idle breathing).
    t: f32,
    /// Seconds airborne (short drops while walking downhill keep the walk cycle).
    air: f32,
    coyote: f32,
    jump_buffer: f32,
    mining: Option<(i32, i32, f32)>,
    place_cooldown: f32,
    /// The tile under the cursor if it's within reach (for the highlight).
    pub hover: Option<(i32, i32)>,
}

impl Player {
    pub fn new(world: &World) -> Self {
        let tx = world.w / 2 - 4;
        let gy = world.ground_y(tx);
        let spawn = ((tx * TILE) as f32 + 3.0, (gy * TILE) as f32 - H - 1.0);
        let mut p = Player {
            x: spawn.0,
            y: spawn.1,
            vx: 0.0,
            vy: 0.0,
            on_ground: false,
            facing: 1.0,
            hp: 10,
            max_hp: 10,
            invuln: 0.0,
            swing: 0.0,
            god: false,
            inventory: [0; items::COUNT],
            hotbar: [None; HOTBAR_SLOTS],
            selected: 0,
            select_age: 10.0,
            helmet: None,
            chest: None,
            spawn,
            anim: 0.0,
            t: 0.0,
            air: 0.0,
            coyote: 0.0,
            jump_buffer: 0.0,
            mining: None,
            place_cooldown: 0.0,
            hover: None,
        };
        // Starting kit.
        for (it, n) in [(Item::WoodSword, 1), (Item::Dirt, 30), (Item::Plank, 20), (Item::Stone, 10), (Item::Torch, 12), (Item::Workbench, 1)] {
            p.give(it, n);
        }
        p
    }

    pub fn count(&self, it: Item) -> u32 {
        self.inventory[it as usize]
    }

    /// The item in the selected hotbar slot.
    pub fn held(&self) -> Option<Item> {
        self.hotbar[self.selected]
    }

    /// Add items. A kind you didn't have goes into the first free hotbar slot.
    pub fn give(&mut self, it: Item, n: u32) {
        self.inventory[it as usize] += n;
        if it.hotbar() && !self.hotbar.contains(&Some(it)) {
            if let Some(slot) = self.hotbar.iter_mut().find(|s| s.is_none()) {
                *slot = Some(it);
            }
        }
    }

    /// Use up `n` of an item (free in god mode). False if there aren't enough.
    /// A stack that runs out leaves the hotbar; tools and weapons never run out.
    pub fn take(&mut self, it: Item, n: u32) -> bool {
        if self.god {
            return true;
        }
        if self.inventory[it as usize] < n {
            return false;
        }
        self.inventory[it as usize] -= n;
        if self.inventory[it as usize] == 0 && !it.is_tool() {
            for s in self.hotbar.iter_mut().filter(|s| **s == Some(it)) {
                *s = None;
            }
        }
        true
    }

    /// Total armour defence (each hit does `max(1, dmg - defense)`).
    pub fn defense(&self) -> i32 {
        self.helmet.map_or(0, |t| t.defense(Slot::Head)) + self.chest.map_or(0, |t| t.defense(Slot::Body))
    }

    /// Wear a piece of armour if it's better than what's in that slot.
    pub fn equip_if_better(&mut self, it: Item) {
        if let Some((slot, tier)) = it.armour() {
            let cur = match slot {
                Slot::Head => &mut self.helmet,
                Slot::Body => &mut self.chest,
            };
            if cur.is_none_or(|c| tier > c) {
                *cur = Some(tier);
            }
        }
    }

    /// Put an owned item into hotbar slot `slot` (moving it from any other slot).
    pub fn assign(&mut self, slot: usize, it: Item) {
        if slot >= HOTBAR_SLOTS || self.count(it) == 0 || !it.hotbar() {
            return;
        }
        for s in self.hotbar.iter_mut().filter(|s| **s == Some(it)) {
            *s = None;
        }
        self.hotbar[slot] = Some(it);
        self.selected = slot;
        self.select_age = 0.0;
    }

    pub fn center(&self) -> (f32, f32) {
        (self.x + W / 2.0, self.y + H / 2.0)
    }

    /// `busy` = another system (screen building) owns the mouse this frame.
    pub fn update(&mut self, dt: f32, ctl: &Controls, cam: &mut Camera, world: &mut World, fx: &mut Particles, busy: bool) {
        self.t += dt;
        self.invuln = (self.invuln - dt).max(0.0);
        self.swing = (self.swing - dt).max(0.0);
        self.place_cooldown = (self.place_cooldown - dt).max(0.0);

        self.select_age += dt;
        for (i, k) in [Key::Hotbar1, Key::Hotbar2, Key::Hotbar3, Key::Hotbar4, Key::Hotbar5, Key::Hotbar6].iter().enumerate() {
            if ctl.pressed(*k) {
                self.selected = i;
                self.select_age = 0.0;
            }
        }

        // --- horizontal movement ---
        let dir = ctl.down(Key::Right) as i32 as f32 - ctl.down(Key::Left) as i32 as f32;
        let max_speed = 125.0;
        let accel = if self.on_ground { 1500.0 } else { 900.0 };
        if dir != 0.0 {
            self.vx += dir * accel * dt;
            self.vx = self.vx.clamp(-max_speed, max_speed);
            self.facing = dir;
        } else {
            let friction = if self.on_ground { 1800.0 } else { 300.0 };
            let s = self.vx.signum();
            self.vx -= s * friction * dt;
            if self.vx.signum() != s {
                self.vx = 0.0;
            }
        }

        // --- jumping (coyote time + buffer + variable height) ---
        self.coyote = if self.on_ground { 0.1 } else { self.coyote - dt };
        if ctl.pressed(Key::Jump) || ctl.pressed(Key::Up) {
            self.jump_buffer = 0.12;
        } else {
            self.jump_buffer -= dt;
        }
        if self.jump_buffer > 0.0 && self.coyote > 0.0 {
            self.vy = -JUMP_V;
            self.jump_buffer = 0.0;
            self.coyote = 0.0;
            fx.debris(self.x + W / 2.0, self.y + H, rgb(200, 190, 170), 4);
        }
        if !(ctl.down(Key::Jump) || ctl.down(Key::Up)) && self.vy < -130.0 {
            self.vy = -130.0;
        }
        self.vy = (self.vy + GRAVITY * dt).min(MAX_FALL);

        // --- collide ---
        let was_on_ground = self.on_ground;
        let fall_speed = self.vy;
        self.move_x(self.vx * dt, world);
        self.move_y(self.vy * dt, world);
        if self.on_ground && !was_on_ground && fall_speed > 300.0 {
            fx.debris(self.x + W / 2.0, self.y + H, rgb(200, 190, 170), 6);
            cam.kick(0.15);
        }
        if self.y > world.height_px() {
            self.respawn(fx);
        }

        self.air = if self.on_ground { 0.0 } else { self.air + dt };
        let walking_frames = self.on_ground || self.air < 0.1;
        if walking_frames && self.vx.abs() > 5.0 {
            // ~18 frames/s at full speed: 3 strides per second.
            self.anim += dt * self.vx.abs() / 7.0;
        } else if walking_frames {
            self.anim = 0.0;
        }

        // --- mouse: mine / swing / place ---
        let (mx, my) = ctl.mouse();
        let (wx, wy) = cam.to_world(mx, my);
        let (tx, ty) = ((wx / TILE as f32).floor() as i32, (wy / TILE as f32).floor() as i32);
        let (cx, cy) = self.center();
        let in_reach = ((tx * TILE + TILE / 2) as f32 - cx).hypot((ty * TILE + TILE / 2) as f32 - cy) < REACH;
        self.hover = if in_reach && !busy { Some((tx, ty)) } else { None };

        if busy {
            self.mining = None;
            return;
        }

        let target = world.get(tx, ty);
        if ctl.lmb() {
            if (wx - cx).abs() > 2.0 {
                self.facing = (wx - cx).signum();
            }
            if in_reach && target.hardness().is_some() {
                let progress = match self.mining {
                    Some((mx_, my_, p)) if mx_ == tx && my_ == ty => p,
                    _ => 0.0,
                };
                let hardness = target.hardness().unwrap_or(1.0);
                let speed = if self.god { 3.0 } else { 1.0 };
                let p = progress + dt * speed / hardness;
                if p >= 1.0 {
                    world.set(tx, ty, Tile::Air);
                    if let Some(it) = Item::from_tile(target) {
                        self.give(it, 1);
                    }
                    let (px, py) = ((tx * TILE + 8) as f32, (ty * TILE + 8) as f32);
                    fx.debris(px, py, target.color(), 14);
                    if target == Tile::Crystal {
                        fx.sparkle(px, py, rgb(140, 240, 255), 10);
                    }
                    self.mining = None;
                } else {
                    // Chip particles while digging.
                    if ((p * 8.0) as i32) != ((progress * 8.0) as i32) {
                        fx.debris((tx * TILE + 8) as f32, (ty * TILE + 8) as f32, target.color(), 2);
                    }
                    self.mining = Some((tx, ty, p));
                }
            } else {
                self.mining = None;
                if self.swing <= 0.0 {
                    self.swing = SWING_TIME;
                }
            }
        } else {
            self.mining = None;
        }

        // T always places a torch; right click places the held item if it's a block.
        let place = if ctl.pressed(Key::Torch) {
            Some(Item::Torch)
        } else if ctl.rmb() {
            self.held().filter(|it| it.tile().is_some())
        } else {
            None
        };
        if let Some((it, t)) = place.and_then(|it| it.tile().map(|t| (it, t))) {
            if self.place_cooldown <= 0.0 && in_reach && target == Tile::Air && self.can_place(tx, ty, world, t) {
                if self.take(it, 1) {
                    world.set(tx, ty, t);
                    fx.debris((tx * TILE + 8) as f32, (ty * TILE + 8) as f32, t.color(), 4);
                    self.place_cooldown = 0.12;
                }
            }
        }
    }

    fn can_place(&self, tx: i32, ty: i32, world: &World, t: Tile) -> bool {
        // Not inside the player.
        let (x0, y0, x1, y1) = ((tx * TILE) as f32, (ty * TILE) as f32, ((tx + 1) * TILE) as f32, ((ty + 1) * TILE) as f32);
        if t.solid() && x0 < self.x + W && x1 > self.x && y0 < self.y + H && y1 > self.y {
            return false;
        }
        // Must touch something (or be a wall-mounted torch underground).
        [(0, 1), (0, -1), (1, 0), (-1, 0)].iter().any(|(dx, dy)| world.get(tx + dx, ty + dy) != Tile::Air)
            || world.is_underground(tx, ty)
    }

    fn collides(&self, x: f32, y: f32, world: &World) -> bool {
        let x0 = (x / TILE as f32).floor() as i32;
        let x1 = ((x + W - 0.01) / TILE as f32).floor() as i32;
        let y0 = (y / TILE as f32).floor() as i32;
        let y1 = ((y + H - 0.01) / TILE as f32).floor() as i32;
        (y0..=y1).any(|ty| (x0..=x1).any(|tx| world.solid(tx, ty)))
    }

    fn move_x(&mut self, dx: f32, world: &World) {
        if dx == 0.0 {
            return;
        }
        let nx = self.x + dx;
        if !self.collides(nx, self.y, world) {
            self.x = nx;
            return;
        }
        // Auto step-up onto 1-tile ledges while walking on the ground.
        if self.on_ground && !self.collides(nx, self.y - TILE as f32 - 0.5, world) && !self.collides(self.x, self.y - TILE as f32 - 0.5, world) {
            self.y -= TILE as f32 + 0.5;
            self.x = nx;
            return;
        }
        // Slide up against the wall.
        let step = dx.signum() * 0.5;
        while !self.collides(self.x + step, self.y, world) {
            self.x += step;
        }
        self.vx = 0.0;
    }

    fn move_y(&mut self, dy: f32, world: &World) {
        let ny = self.y + dy;
        if !self.collides(self.x, ny, world) {
            self.y = ny;
            self.on_ground = false;
            return;
        }
        let step = dy.signum() * 0.5;
        while !self.collides(self.x, self.y + step, world) {
            self.y += step;
        }
        if dy > 0.0 {
            self.on_ground = true;
            // Snap the feet exactly onto the tile top so the sprite never floats half a pixel.
            let top = ((self.y + H) / TILE as f32).ceil() * TILE as f32;
            if top - (self.y + H) < 0.75 && !self.collides(self.x, top - H, world) {
                self.y = top - H;
            }
        }
        self.vy = 0.0;
    }

    /// Hitbox of the current sword swing, if active: (x, y, w, h) in world px.
    pub fn attack_box(&self) -> Option<(f32, f32, f32, f32)> {
        if self.swing > SWING_TIME * 0.25 {
            let (cx, sy) = self.shoulder();
            let w = 32.0;
            let x = if self.facing > 0.0 { cx - 2.0 } else { cx + 2.0 - w };
            Some((x, sy - 24.0, w, 40.0))
        } else {
            None
        }
    }

    /// Take damage from `from_x` (for knockback). Returns true if it landed.
    pub fn hurt(&mut self, dmg: i32, from_x: f32, fx: &mut Particles) -> bool {
        if self.invuln > 0.0 || self.god {
            return false;
        }
        self.hp -= dmg;
        self.invuln = 0.8;
        let dir = if self.center().0 < from_x { -1.0 } else { 1.0 };
        self.vx = dir * 200.0;
        self.vy = -200.0;
        let (cx, cy) = self.center();
        fx.debris(cx, cy, rgb(220, 60, 60), 8);
        if self.hp <= 0 {
            self.respawn(fx);
        }
        true
    }

    fn respawn(&mut self, fx: &mut Particles) {
        let (cx, cy) = self.center();
        fx.sparkle(cx, cy, rgb(255, 255, 255), 20);
        self.x = self.spawn.0;
        self.y = self.spawn.1;
        self.vx = 0.0;
        self.vy = 0.0;
        self.hp = self.max_hp;
        self.invuln = 1.5;
    }

    /// Sprite top-left in world px (the sprite is centred on the hitbox, feet on its bottom).
    fn sprite_origin(&self) -> (f32, f32) {
        (self.x + W / 2.0 - PLAYER_W as f32 / 2.0, self.y + H - PLAYER_H as f32)
    }

    /// Shoulder of the front arm, world px (sword arc centre).
    fn shoulder(&self) -> (f32, f32) {
        let (ox, oy) = self.sprite_origin();
        let sx = if self.facing < 0.0 { PLAYER_W - 1 - SHOULDER.0 } else { SHOULDER.0 };
        (ox + sx as f32 + 0.5, oy + SHOULDER.1 as f32)
    }

    fn frame<'s>(&self, sprites: &'s Sprites) -> &'s crate::gfx::Sprite {
        let walking_frames = self.on_ground || self.air < 0.1;
        if !walking_frames {
            if self.vy < 0.0 {
                &sprites.player_jump
            } else {
                &sprites.player_fall
            }
        } else if self.anim > 0.0 {
            let n = sprites.player_walk.len().max(1);
            &sprites.player_walk[(self.anim as usize) % n]
        } else {
            // Breathing: long inhale, shorter exhale.
            let n = sprites.player_idle.len().max(1);
            let phase = (self.t % 1.6) / 1.6;
            &sprites.player_idle[if phase < 0.6 { 0 } else { 1 % n }]
        }
    }

    /// Draw on the FX layer. `light` scales brightness (night darkness).
    pub fn draw(&self, c: &mut Canvas, cam: &Camera, sprites: &Sprites, light: f32) {
        if self.invuln > 0.0 && ((self.invuln * 14.0) as i32) % 2 == 1 {
            return;
        }
        let spr = self.frame(sprites);
        let flip = self.facing < 0.0;
        let (ox, oy) = self.sprite_origin();
        let (vx, vy) = cam.to_view(ox, oy);
        let k = light.clamp(0.6, 1.0); // keep the hero readable at night

        // Wind-up: the raised sword is behind the head, so it goes under the body.
        if self.swing > 0.0 {
            self.draw_slash(c, cam, sprites, k, true);
        }

        c.sprite_shaded(spr, vx, vy, flip, k);

        // Pickaxe in the front hand while mining: raised, then struck, a few times a second.
        if let Some((_, _, p)) = self.mining {
            let struck = (p * 30.0).sin() > 0.0;
            let (tool, grip) = if struck { (&sprites.pickaxe_down, (1, 1)) } else { (&sprites.pickaxe, (1, 12)) };
            let (hx, hy) = (if flip { spr.w - 1 - HAND.0 } else { HAND.0 }, HAND.1);
            let gx = if flip { tool.w - 1 - grip.0 } else { grip.0 };
            c.sprite_shaded(tool, vx + hx - gx, vy + hy - grip.1, flip, k);
        }

        if self.swing > 0.0 {
            self.draw_slash(c, cam, sprites, k, false);
        }
    }

    /// Two passes: `behind` (under the body) draws the sword only while it's raised behind
    /// the head; the other pass draws the trail and the sword once it comes over.
    fn draw_slash(&self, c: &mut Canvas, cam: &Camera, sprites: &Sprites, k: f32, behind: bool) {
        let t = 1.0 - self.swing / SWING_TIME; // 0 -> 1
        let (sx, sy) = self.shoulder();
        let (vx, vy) = cam.to_view(sx, sy);
        let f = self.facing;
        // Arc from up-back (-110deg) to down-front (+50deg).
        let a0 = -110f32.to_radians();
        let a1 = 50f32.to_radians();
        let head = a0 + (a1 - a0) * (t * 1.4).min(1.0);
        let deg = head.to_degrees();
        if behind != (deg < -75.0) {
            return;
        }
        if !behind {
            self.draw_trail(c, vx, vy, head, t);
        }
        // Sword sprite nearest the arc angle, grip in the hand at arm's length.
        let i = (((deg + 90.0) / 45.0).round().clamp(0.0, 3.0)) as usize;
        let (Some(s), Some(&(px, py))) = (sprites.sword_swing.get(i), SWORD_GRIPS.get(i)) else { return };
        let (gx, gy) = (vx + (head.cos() * 6.0 * f).round() as i32, vy + (head.sin() * 6.0).round() as i32);
        let x = if f < 0.0 { gx - (s.w - 1 - px) } else { gx - px };
        c.sprite_shaded(s, x, gy - py, f < 0.0, k.max(0.6));
    }

    fn draw_trail(&self, c: &mut Canvas, vx: i32, vy: i32, head: f32, t: f32) {
        let f = self.facing;
        let a0 = -110f32.to_radians();
        let trail = 10;
        for i in 0..trail {
            let a = head - i as f32 * 0.12;
            if a < a0 {
                break;
            }
            let fade = 1.0 - i as f32 / trail as f32;
            let alpha = (220.0 * fade * (1.0 - t * 0.6)) as u8;
            for r in 9..24 {
                let x = vx + (a.cos() * r as f32 * f) as i32;
                let y = vy + (a.sin() * r as f32) as i32;
                let col = if r > 20 { rgba(255, 255, 255, alpha) } else { rgba(150, 236, 255, alpha / 2) };
                c.blend(x, y, col);
            }
        }
    }

    /// Mining crack + hover outline (FX layer).
    pub fn draw_cursor(&self, c: &mut Canvas, cam: &Camera, world: &World) {
        if let Some((tx, ty)) = self.hover {
            let (vx, vy) = cam.to_view((tx * TILE) as f32, (ty * TILE) as f32);
            let t = world.get(tx, ty);
            let col = if t == Tile::Air { rgba(255, 255, 255, 50) } else { rgba(255, 255, 255, 110) };
            c.rect_outline(vx, vy, TILE, TILE, col);
        }
        if let Some((tx, ty, p)) = self.mining {
            let (vx, vy) = cam.to_view((tx * TILE) as f32, (ty * TILE) as f32);
            let stages = (p * 5.0) as i32;
            let dark = rgba(20, 16, 24, 180);
            // Crack lines radiating from the centre, more with progress.
            let rays = [(8, 8, 2, 3), (8, 8, 14, 5), (8, 8, 4, 13), (8, 8, 13, 14), (8, 8, 1, 9)];
            for (i, &(x0, y0, x1, y1)) in rays.iter().enumerate() {
                if (i as i32) < stages + 1 {
                    c.line(vx + x0, vy + y0, vx + x1, vy + y1, dark);
                }
            }
            c.fill_rect(vx, vy, TILE, TILE, with_alpha(rgb(255, 255, 255), (p * 40.0) as u8));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::World;

    #[test]
    fn starting_kit_fills_the_hotbar() {
        let p = Player::new(&World::generate(1));
        assert_eq!(p.hotbar[0], Some(Item::WoodSword));
        assert!(p.hotbar.iter().all(|s| s.is_some()));
        assert_eq!(p.count(Item::Dirt), 30);
    }

    #[test]
    fn stacks_leave_the_hotbar_when_used_up_and_come_back() {
        let mut p = Player::new(&World::generate(1));
        let slot = p.hotbar.iter().position(|s| *s == Some(Item::Workbench)).unwrap();
        assert!(p.take(Item::Workbench, 1));
        assert_eq!(p.hotbar[slot], None);
        assert!(!p.take(Item::Workbench, 1), "can't use what you don't have");
        p.give(Item::Copper, 3);
        assert_eq!(p.hotbar[slot], Some(Item::Copper), "a new kind fills the free slot");
        p.take(Item::WoodSword, 1);
        assert_eq!(p.hotbar[0], Some(Item::WoodSword), "weapons never leave the hotbar");
    }

    #[test]
    fn assign_moves_an_item_between_slots() {
        let mut p = Player::new(&World::generate(1));
        p.assign(5, Item::WoodSword);
        assert_eq!(p.hotbar[5], Some(Item::WoodSword));
        assert_eq!(p.hotbar[0], None);
        assert_eq!(p.selected, 5);
    }
}
