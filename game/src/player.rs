//! Player: movement, mining, placing, sword swing. Owner: person B1.

use crate::camera::Camera;
use crate::gfx::{rgb, rgba, with_alpha, Canvas};
use crate::input::Controls;
use crate::particles::Particles;
use crate::sprites::Sprites;
use crate::world::{Tile, World, TILE};
use shared::Key;

pub const W: f32 = 10.0;
pub const H: f32 = 19.0;
const REACH: f32 = 6.0 * TILE as f32;
const SWING_TIME: f32 = 0.28;

pub const HOTBAR: [Tile; 5] = [Tile::Dirt, Tile::Stone, Tile::Plank, Tile::Torch, Tile::Crystal];

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
    pub inventory: [u32; 12],
    pub selected: usize,
    pub spawn: (f32, f32),
    anim: f32,
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
        let mut inventory = [0u32; 12];
        inventory[Tile::Dirt as usize] = 30;
        inventory[Tile::Plank as usize] = 20;
        inventory[Tile::Torch as usize] = 12;
        Player {
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
            inventory,
            selected: 0,
            spawn,
            anim: 0.0,
            coyote: 0.0,
            jump_buffer: 0.0,
            mining: None,
            place_cooldown: 0.0,
            hover: None,
        }
    }

    pub fn center(&self) -> (f32, f32) {
        (self.x + W / 2.0, self.y + H / 2.0)
    }

    /// `busy` = another system (screen building) owns the mouse this frame.
    pub fn update(&mut self, dt: f32, ctl: &Controls, cam: &mut Camera, world: &mut World, fx: &mut Particles, busy: bool) {
        self.invuln = (self.invuln - dt).max(0.0);
        self.swing = (self.swing - dt).max(0.0);
        self.place_cooldown = (self.place_cooldown - dt).max(0.0);

        for (i, k) in [Key::Hotbar1, Key::Hotbar2, Key::Hotbar3, Key::Hotbar4, Key::Hotbar5].iter().enumerate() {
            if ctl.pressed(*k) {
                self.selected = i;
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
            self.vy = -330.0;
            self.jump_buffer = 0.0;
            self.coyote = 0.0;
            fx.debris(self.x + W / 2.0, self.y + H, rgb(200, 190, 170), 4);
        }
        if !(ctl.down(Key::Jump) || ctl.down(Key::Up)) && self.vy < -120.0 {
            self.vy = -120.0;
        }
        self.vy = (self.vy + 1100.0 * dt).min(520.0);

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

        if self.on_ground && self.vx.abs() > 5.0 {
            self.anim += dt * self.vx.abs() / 14.0;
        } else {
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
                    let d = target.drop();
                    self.inventory[d as usize] += 1;
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

        let place_tile = if ctl.pressed(Key::Torch) { Some(Tile::Torch) } else if ctl.rmb() { Some(HOTBAR[self.selected]) } else { None };
        if let Some(t) = place_tile {
            if self.place_cooldown <= 0.0 && in_reach && target == Tile::Air && self.can_place(tx, ty, world, t) {
                if self.god || self.inventory[t as usize] > 0 {
                    if !self.god {
                        self.inventory[t as usize] -= 1;
                    }
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
        }
        self.vy = 0.0;
    }

    /// Hitbox of the current sword swing, if active: (x, y, w, h) in world px.
    pub fn attack_box(&self) -> Option<(f32, f32, f32, f32)> {
        if self.swing > SWING_TIME * 0.25 {
            let (cx, cy) = self.center();
            let w = 28.0;
            let x = if self.facing > 0.0 { cx } else { cx - w };
            Some((x, cy - 18.0, w, 30.0))
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

    /// Draw on the FX layer. `light` scales brightness (night darkness).
    pub fn draw(&self, c: &mut Canvas, cam: &Camera, sprites: &Sprites, light: f32) {
        if self.invuln > 0.0 && ((self.invuln * 14.0) as i32) % 2 == 1 {
            return;
        }
        let spr = if !self.on_ground {
            &sprites.player_jump
        } else if self.anim > 0.0 {
            &sprites.player_walk[(self.anim as usize) % 4]
        } else {
            &sprites.player_idle
        };
        let flip = self.facing < 0.0;
        let (vx, vy) = cam.to_view(self.x - 1.0, self.y + H - spr.h as f32 + 1.0);
        let k = light.clamp(0.6, 1.0); // keep the hero readable at night

        // Pickaxe bobbing while mining.
        if let Some((_, _, p)) = self.mining {
            let bob = ((p * 30.0).sin() * 3.0) as i32;
            let px = if flip { vx - 7 } else { vx + 7 };
            c.sprite_shaded(&sprites.pickaxe, px, vy + 2 + bob, flip, k);
        }

        c.sprite_shaded(spr, vx, vy, flip, k);

        if self.swing > 0.0 {
            self.draw_slash(c, cam, sprites, k);
        }
    }

    fn draw_slash(&self, c: &mut Canvas, cam: &Camera, sprites: &Sprites, k: f32) {
        let t = 1.0 - self.swing / SWING_TIME; // 0 -> 1
        let (cx, cy) = self.center();
        let (vx, vy) = cam.to_view(cx, cy - 2.0);
        let f = self.facing;
        // Arc from up-back (-110deg) to down-front (+50deg).
        let a0 = -110f32.to_radians();
        let a1 = 50f32.to_radians();
        let head = a0 + (a1 - a0) * (t * 1.4).min(1.0);
        let trail = 9;
        for i in 0..trail {
            let a = head - i as f32 * 0.13;
            if a < a0 {
                break;
            }
            let fade = 1.0 - i as f32 / trail as f32;
            let alpha = (220.0 * fade * (1.0 - t * 0.6)) as u8;
            for r in 12..24 {
                let x = vx + (a.cos() * r as f32 * f) as i32;
                let y = vy + (a.sin() * r as f32) as i32;
                let col = if r > 20 { rgba(255, 255, 255, alpha) } else { rgba(170, 220, 255, alpha / 2) };
                c.blend(x, y, col);
            }
        }
        // Sword sprite at the arc head.
        let sx = vx + (head.cos() * 14.0 * f) as i32 - 6;
        let sy = vy + (head.sin() * 14.0) as i32 - 6;
        c.sprite_shaded(&sprites.sword, sx, sy, f < 0.0, k.max(0.6));
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
