//! Night mobs: zombies. Owner: person B2.
//!
//! At night zombies claw their way out of the ground near the player, shamble
//! to the nearest online screen and claw at its frame. They go for the player
//! instead when the player is closer than any screen, take sword hits
//! (knockback, flash, death pop), and catch fire at dawn.
//!
//! All tuning lives in the consts below. The sprites are original pixel art
//! (office-worker zombies in torn shirts and ties), built with `Sprite::from_ascii`.
//!
//! TODO(B2): more mob kinds (bats that swoop at screens), loot drops.

use crate::camera::Camera;
use crate::gfx::{rgb, rgba, shade, with_alpha, Canvas, Rgba, Sprite};
use crate::light::Light;
use crate::particles::{Particle, Particles};
use crate::player::{self, Player};
use crate::screens::Screens;
use crate::sky::Sky;
use crate::world::{World, TILE};
use shared::{GameEvent, ScreenId};

pub struct MobCtx<'a> {
    pub world: &'a World,
    pub screens: &'a mut Screens,
    pub player: &'a mut Player,
    pub fx: &'a mut Particles,
    pub cam: &'a mut Camera,
    pub sky: &'a Sky,
    pub events: &'a mut Vec<GameEvent>,
}

// ---------------- tuning ----------------

/// Zombie hitbox in world px (same footprint as the player).
pub const W: f32 = 10.0;
pub const H: f32 = 19.0;

/// Base shamble speed (px/s). Each zombie gets a 0.8..1.25 multiplier.
const WALK_SPEED: f32 = 34.0;
const GRAVITY: f32 = 1100.0;
const MAX_FALL: f32 = 520.0;
const JUMP_VY: f32 = -310.0;

/// Seconds to climb out of the ground.
const RISE_TIME: f32 = 0.9;
/// Sword hits to kill one on the first night (+1 every 3 nights).
const BASE_HP: i32 = 3;

/// Screen HP (0..1) removed per claw, and seconds between claws.
const CLAW_DAMAGE: f32 = 0.035;
const CLAW_COOLDOWN: f32 = 1.0;
/// How far past its hitbox a zombie can claw a frame, sideways and upwards.
/// REACH_UP = 16 means a frame floating up to one tile above a zombie's head is
/// reachable standing; higher ones it has to leap at.
const REACH_X: f32 = 5.0;
const REACH_UP: f32 = 16.0;

/// Player damage per touch (the player has 10 HP and 0.8 s of i-frames).
const TOUCH_DAMAGE: i32 = 1;
/// A zombie goes for the player instead of a screen when the player is this
/// close (px) *and* closer than the nearest screen.
const AGGRO: f32 = 48.0;

/// Spawn distance from the player (px): far enough to be fair, close enough
/// that they rise on camera (the view is 640 px wide).
const SPAWN_MIN: f32 = 150.0;
const SPAWN_MAX: f32 = 290.0;
/// Delay after nightfall before the first zombie.
const FIRST_SPAWN: f32 = 0.8;
const MAX_ALIVE: usize = 12;

/// Zombies spawn while daylight is below SPAWN_LIGHT and burn once it's above BURN_LIGHT.
const SPAWN_LIGHT: f32 = 0.05;
const BURN_LIGHT: f32 = 0.5;
/// Seconds a zombie burns before it crumbles.
const BURN_TIME: f32 = 1.6;

/// Seconds between spawns on night `n` (1-based), before jitter.
fn spawn_interval(nights: u32) -> f32 {
    (2.3 - 0.25 * nights.saturating_sub(1) as f32).max(0.9)
}

/// Max zombies alive at once on night `n`.
fn max_alive(nights: u32) -> usize {
    (3 + 2 * nights as usize).min(MAX_ALIVE)
}

// ---------------- colours ----------------

const OUTLINE: Rgba = rgb(20, 18, 28);
const SKIN: Rgba = rgb(138, 170, 120);
const SKIN_LO: Rgba = rgb(100, 130, 92);
const EYE: Rgba = rgb(210, 255, 120);
const MOUTH: Rgba = rgb(64, 26, 34);
const SHOES: Rgba = rgb(44, 34, 32);
const DIRT: Rgba = rgb(134, 88, 58);
const FIRE: Rgba = rgb(255, 150, 50);
const ASH: Rgba = rgb(70, 66, 72);

/// One outfit. Every zombie wears one of these.
struct Look {
    hair: Rgba,
    shirt: Rgba,
    shirt_lo: Rgba,
    tie: Rgba,
    tie_lo: Rgba,
    pants: Rgba,
    pants_lo: Rgba,
}

const LOOKS: [Look; 3] = [
    // White shirt, red tie, grey slacks.
    Look {
        hair: rgb(48, 40, 38),
        shirt: rgb(214, 212, 200),
        shirt_lo: rgb(158, 156, 150),
        tie: rgb(190, 44, 52),
        tie_lo: rgb(132, 26, 36),
        pants: rgb(76, 80, 94),
        pants_lo: rgb(54, 56, 68),
    },
    // Pale blue shirt, navy tie, brown trousers.
    Look {
        hair: rgb(96, 72, 50),
        shirt: rgb(150, 182, 214),
        shirt_lo: rgb(104, 134, 170),
        tie: rgb(40, 48, 96),
        tie_lo: rgb(26, 30, 64),
        pants: rgb(96, 74, 60),
        pants_lo: rgb(70, 52, 42),
    },
    // Mustard sweater, dark jeans.
    Look {
        hair: rgb(30, 30, 36),
        shirt: rgb(204, 170, 72),
        shirt_lo: rgb(150, 122, 46),
        tie: rgb(170, 140, 56),
        tie_lo: rgb(120, 96, 36),
        pants: rgb(50, 54, 78),
        pants_lo: rgb(36, 38, 56),
    },
];

// ---------------- state ----------------

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MobState {
    /// Clawing out of the ground; `t` runs 0..RISE_TIME.
    Rising { t: f32 },
    Walking,
    /// Caught by daylight; crumbles when `t` reaches BURN_TIME.
    Burning { t: f32 },
}

#[derive(Clone, Debug)]
pub struct Mob {
    /// Top-left of the hitbox, world px.
    pub x: f32,
    pub y: f32,
    pub vx: f32,
    pub vy: f32,
    pub hp: i32,
    pub state: MobState,
    pub facing: f32,
    pub on_ground: bool,
    /// Speed multiplier, so a horde spreads out instead of moving in lockstep.
    speed: f32,
    /// Outfit (index into LOOKS and the sprite sets).
    variant: usize,
    anim: f32,
    claw_cd: f32,
    /// Lunge animation time left after a claw.
    lunge: f32,
    /// Knockback: no steering while > 0.
    stun: f32,
    /// Hit flash. Also stops one sword swing from hitting twice.
    hurt: f32,
    jump_cd: f32,
    /// Random delay before catching fire, so they don't all ignite at once.
    burn_delay: f32,
}

impl Mob {
    pub fn center(&self) -> (f32, f32) {
        (self.x + W / 2.0, self.y + H / 2.0)
    }

    fn hitbox(&self) -> (f32, f32, f32, f32) {
        (self.x, self.y, W, H)
    }
}

/// xorshift32. Fixed seed, so a demo run plays out the same way every time.
#[derive(Clone, Copy)]
struct Rng(u32);

impl Rng {
    /// Uniform in [0, 1).
    fn next(&mut self) -> f32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        (x >> 8) as f32 / (1u32 << 24) as f32
    }
}

struct ZombieFrames {
    idle: Sprite,
    walk: [Sprite; 4],
}

pub struct Mobs {
    pub list: Vec<Mob>,
    rng: Rng,
    spawn_timer: f32,
    was_night: bool,
    /// 0..1: how brightly eyes glow. Set from the sky in `update`, used by `draw`.
    eye_glow: f32,
    sprites: Vec<ZombieFrames>,
}

impl Default for Mobs {
    fn default() -> Self {
        Self::new()
    }
}

impl Mobs {
    pub fn new() -> Self {
        Mobs {
            list: Vec::new(),
            rng: Rng(0x5EED_2B0B),
            spawn_timer: 0.0,
            was_night: false,
            eye_glow: 0.0,
            sprites: zombie_sprites(),
        }
    }

    /// Spawn a zombie that rises out of the ground with its feet at `feet_y` (world px).
    pub fn spawn_zombie(&mut self, x: f32, feet_y: f32, facing: f32, nights: u32) {
        let n = self.sprites.len();
        let r = &mut self.rng;
        let variant = (r.next() * n as f32) as usize % n;
        self.list.push(Mob {
            x,
            y: feet_y - H,
            vx: 0.0,
            vy: 0.0,
            hp: BASE_HP + nights as i32 / 3,
            state: MobState::Rising { t: 0.0 },
            facing,
            on_ground: true,
            speed: 0.8 + 0.45 * r.next(),
            variant,
            anim: r.next() * 4.0,
            claw_cd: 0.3 + r.next() * 0.6,
            lunge: 0.0,
            stun: 0.0,
            hurt: 0.0,
            jump_cd: 0.0,
            burn_delay: r.next() * 1.5,
        });
    }

    pub fn update(&mut self, dt: f32, ctx: &mut MobCtx) {
        let daylight = ctx.sky.daylight();
        self.eye_glow = (1.0 - daylight * 1.6).clamp(0.0, 1.0);

        // --- spawning ---
        let night = ctx.sky.is_night();
        if night && !self.was_night {
            self.spawn_timer = FIRST_SPAWN;
        }
        self.was_night = night;
        if night && daylight < SPAWN_LIGHT {
            self.spawn_timer -= dt;
            if self.spawn_timer <= 0.0 {
                let nights = ctx.sky.nights.max(1);
                self.spawn_timer = spawn_interval(nights) * (0.7 + 0.6 * self.rng.next());
                if self.list.len() < max_alive(nights) {
                    self.try_spawn(ctx, nights);
                }
            }
        }

        // --- AI, physics, combat ---
        let targets = ctx.screens.targets();
        let burn = daylight > BURN_LIGHT;
        let Mobs { list, rng, .. } = self;
        for z in list.iter_mut() {
            update_zombie(z, dt, ctx, &targets, burn, rng);
        }
        separate(list, ctx.world);

        // --- deaths ---
        for z in list.iter().filter(|z| z.hp <= 0) {
            death_pop(z, ctx.fx, rng);
        }
        list.retain(|z| z.hp > 0);
    }

    /// Pick a patch of ground near (but not too near) the player and raise a zombie there.
    fn try_spawn(&mut self, ctx: &MobCtx, nights: u32) {
        let world = ctx.world;
        let t = TILE as f32;
        let (pcx, _) = ctx.player.center();
        for _ in 0..8 {
            let side = if self.rng.next() < 0.5 { -1.0 } else { 1.0 };
            let dist = SPAWN_MIN + (SPAWN_MAX - SPAWN_MIN) * self.rng.next();
            let x = (pcx + side * dist - W / 2.0).clamp(2.0 * t, world.width_px() - 2.0 * t - W);
            if (x + W / 2.0 - pcx).abs() < SPAWN_MIN * 0.6 {
                continue; // squeezed against the world edge, too close to the player
            }
            let gy = world.ground_y(((x + W / 2.0) / t).floor() as i32);
            let feet = (gy * TILE) as f32 - 0.5;
            if collides(x, feet - H, world) {
                continue; // uneven ground: part of the body would be inside a wall
            }
            let body = (x - 6.0, feet - H - 6.0, W + 12.0, H + 12.0);
            if ctx.screens.list.iter().any(|s| overlaps(body, s.frame_px())) {
                continue; // don't rise inside a screen
            }
            let facing = if pcx < x { -1.0 } else { 1.0 };
            self.spawn_zombie(x, feet, facing, nights);
            return;
        }
    }

    /// Draw on the FX layer (over windows).
    pub fn draw(&self, c: &mut Canvas, cam: &Camera, light: &Light) {
        for z in &self.list {
            let frames = &self.sprites[z.variant];
            let spr = if matches!(z.state, MobState::Rising { .. }) {
                &frames.idle
            } else if !z.on_ground {
                &frames.walk[0]
            } else if z.vx.abs() > 2.0 {
                &frames.walk[(z.anim as usize) % 4]
            } else {
                &frames.idle
            };
            let flip = z.facing < 0.0;
            // Keep the body (not the outstretched arms) on the hitbox.
            let left = if flip { z.x - (spr.w - 1 - BODY_R) as f32 } else { z.x - BODY_L as f32 };
            let (mut vx, vy) = cam.to_view(left, z.y + H - spr.h as f32);
            if vx < -spr.w - 8 || vx > c.w + 8 || vy < -spr.h - 8 || vy > c.h + 8 {
                continue;
            }
            if z.lunge > 0.09 {
                vx += (z.facing * 2.0) as i32;
            }
            let (cx, cy) = z.center();
            // Darker than the player at night (spooky) but still readable.
            let k = light.level_at(cx, cy).clamp(0.5, 1.0);

            // While rising, only the top rows have cleared the ground.
            let rows = match z.state {
                MobState::Rising { t } => ((t / RISE_TIME) * spr.h as f32).ceil() as i32,
                _ => spr.h,
            };
            let top = vy + spr.h - rows;
            draw_rows(c, spr, vx, top, flip, k, rows);

            match z.state {
                MobState::Burning { t } => {
                    let flicker = 110.0 + 70.0 * (t * 37.0).sin();
                    c.sprite_flat(spr, vx, vy, flip, with_alpha(FIRE, flicker as u8));
                    c.glow(vx + spr.w / 2, vy + spr.h / 2, 12, with_alpha(FIRE, 60));
                }
                _ if z.hurt > 0.12 => c.sprite_flat(spr, vx, vy, flip, rgba(255, 255, 255, 210)),
                _ => {}
            }

            // Glowing eyes in the dark.
            if self.eye_glow > 0.0 && rows > EYE_Y && !matches!(z.state, MobState::Burning { .. }) {
                let ex = vx + if flip { spr.w - 1 - EYE_X } else { EYE_X };
                let ey = top + EYE_Y;
                c.glow(ex, ey, 4, with_alpha(EYE, (110.0 * self.eye_glow) as u8));
                c.put(ex, ey, rgb(240, 255, 190));
            }
        }
    }
}

// ---------------- behaviour ----------------

fn update_zombie(z: &mut Mob, dt: f32, ctx: &mut MobCtx, targets: &[(ScreenId, (f32, f32, f32, f32))], burn: bool, rng: &mut Rng) {
    for t in [&mut z.claw_cd, &mut z.lunge, &mut z.stun, &mut z.hurt, &mut z.jump_cd] {
        *t = (*t - dt).max(0.0);
    }
    let (cx, cy) = z.center();
    let feet = z.y + H;

    match z.state {
        MobState::Rising { t } => {
            let nt = t + dt;
            if (nt * 14.0) as i32 != (t * 14.0) as i32 {
                ctx.fx.debris(cx + (rng.next() - 0.5) * 8.0, feet, DIRT, 2);
            }
            if nt >= RISE_TIME {
                z.state = MobState::Walking;
                ctx.fx.debris(cx, feet, DIRT, 6);
            } else {
                z.state = MobState::Rising { t: nt };
            }
            return; // half-buried: doesn't move, can't be hit
        }
        MobState::Walking => {
            if burn {
                z.burn_delay -= dt;
                if z.burn_delay <= 0.0 {
                    z.state = MobState::Burning { t: 0.0 };
                    ctx.fx.sparkle(cx, cy, FIRE, 6);
                }
            }
        }
        MobState::Burning { t } => {
            let nt = t + dt;
            z.state = MobState::Burning { t: nt };
            if (nt * 24.0) as i32 != (t * 24.0) as i32 {
                ctx.fx.sparkle(z.x + rng.next() * W, z.y + rng.next() * H * 0.7, FIRE, 1);
                smoke(ctx.fx, cx, z.y, rng);
            }
            if nt >= BURN_TIME {
                z.hp = 0;
                return;
            }
        }
    }
    let burning = matches!(z.state, MobState::Burning { .. });

    // --- decide where to go ---
    let (pcx, pcy) = ctx.player.center();
    let mut dir = 0.0;
    let mut claw: Option<(ScreenId, (f32, f32))> = None;
    if burning {
        dir = z.facing; // flail onwards until it crumbles
    } else {
        let pdist = (pcx - cx).abs().max((pcy - cy).abs() * 1.5);
        let screen = nearest(targets, cx, cy).filter(|(_, r)| !(pdist < AGGRO && pdist < rect_dist(r, cx, cy)));
        match screen {
            None => {
                // No screens to wreck, or the player is right here: go for the player.
                if (pcx - cx).abs() > 3.0 {
                    dir = (pcx - cx).signum();
                }
            }
            Some((id, (fx0, fy0, fw, fh))) => {
                let in_x = z.x + W + REACH_X > fx0 && z.x - REACH_X < fx0 + fw;
                let in_y = z.y - REACH_UP < fy0 + fh && feet > fy0;
                if in_x && in_y {
                    if cx < fx0 {
                        z.facing = 1.0;
                    } else if cx > fx0 + fw {
                        z.facing = -1.0;
                    }
                    let at = ((cx + z.facing * (W / 2.0 + 3.0)).clamp(fx0, fx0 + fw), (z.y + 5.0).clamp(fy0, fy0 + fh));
                    claw = Some((id, at));
                } else if in_x {
                    if fy0 + fh <= z.y {
                        // Frame is overhead but out of reach: leap at it.
                        if z.on_ground && z.jump_cd <= 0.0 {
                            z.vy = JUMP_VY;
                            z.on_ground = false;
                            z.jump_cd = 0.9 + rng.next() * 0.8;
                        }
                    } else {
                        // Frame is below (we're up on a ledge): head for its middle.
                        dir = (fx0 + fw / 2.0 - cx).signum();
                    }
                } else {
                    dir = if cx < fx0 { 1.0 } else { -1.0 };
                }
            }
        }
    }

    // --- claw the screen ---
    if let Some((id, at)) = claw {
        if z.claw_cd <= 0.0 {
            ctx.screens.damage(id, CLAW_DAMAGE, at, ctx.fx, ctx.events);
            z.claw_cd = CLAW_COOLDOWN * (0.75 + 0.5 * rng.next());
            z.lunge = 0.18;
        }
    }

    // --- move ---
    if z.stun > 0.0 {
        z.vx *= (1.0 - 6.0 * dt).max(0.0); // knockback slides out
    } else {
        // Lurching gait: speed pulses with the step cycle.
        let gait = 0.5 + 0.5 * (z.anim * std::f32::consts::PI).sin().abs();
        let speed = WALK_SPEED * z.speed * gait * if burning { 1.7 } else { 1.0 };
        z.vx = dir * speed;
        if dir != 0.0 {
            z.facing = dir;
        }
    }
    z.vy = (z.vy + GRAVITY * dt).min(MAX_FALL);
    let blocked = move_x(z, z.vx * dt, ctx.world);
    move_y(z, z.vy * dt, ctx.world);
    if blocked && dir != 0.0 && z.on_ground && z.jump_cd <= 0.0 {
        z.vy = JUMP_VY; // wall too tall to step up: hop it
        z.on_ground = false;
        z.jump_cd = 0.6;
    }
    if z.on_ground && z.vx.abs() > 2.0 {
        z.anim += dt * z.vx.abs() / 9.0;
    }
    if z.y > ctx.world.height_px() {
        z.hp = 0;
        return;
    }

    // --- combat with the player ---
    let (cx, cy) = z.center();
    if let Some(sword) = ctx.player.attack_box() {
        if z.hurt <= 0.0 && overlaps(sword, z.hitbox()) {
            z.hp -= 1;
            z.hurt = 0.25;
            z.stun = 0.3;
            let away = if cx >= pcx { 1.0 } else { -1.0 };
            z.vx = away * 170.0;
            z.vy = -150.0;
            z.on_ground = false;
            ctx.fx.debris(cx, cy, SKIN, 6);
            ctx.cam.kick(if z.hp <= 0 { 0.2 } else { 0.08 });
        }
    }
    if !burning {
        let body = (z.x + 1.0, z.y + 2.0, W - 2.0, H - 2.0);
        if overlaps(body, (ctx.player.x, ctx.player.y, player::W, player::H)) {
            ctx.player.hurt(TOUCH_DAMAGE, cx, ctx.fx);
        }
    }
}

/// Distance from a point to a rect (0 inside).
fn rect_dist(r: &(f32, f32, f32, f32), x: f32, y: f32) -> f32 {
    let dx = (r.0 - x).max(x - (r.0 + r.2)).max(0.0);
    let dy = (r.1 - y).max(y - (r.1 + r.3)).max(0.0);
    dx.hypot(dy)
}

fn nearest(targets: &[(ScreenId, (f32, f32, f32, f32))], x: f32, y: f32) -> Option<(ScreenId, (f32, f32, f32, f32))> {
    targets.iter().copied().min_by(|a, b| rect_dist(&a.1, x, y).total_cmp(&rect_dist(&b.1, x, y)))
}

fn overlaps(a: (f32, f32, f32, f32), b: (f32, f32, f32, f32)) -> bool {
    a.0 < b.0 + b.2 && a.0 + a.2 > b.0 && a.1 < b.1 + b.3 && a.1 + a.3 > b.1
}

/// Nudge overlapping zombies apart so a horde reads as a crowd, not one sprite.
fn separate(list: &mut [Mob], world: &World) {
    const GAP: f32 = 7.0;
    for i in 0..list.len() {
        let (head, tail) = list.split_at_mut(i + 1);
        let a = &mut head[i];
        if matches!(a.state, MobState::Rising { .. }) {
            continue;
        }
        for b in tail.iter_mut() {
            if matches!(b.state, MobState::Rising { .. }) {
                continue;
            }
            let dx = b.x - a.x;
            if dx.abs() >= GAP || (a.y - b.y).abs() >= 12.0 {
                continue;
            }
            let push = (GAP - dx.abs()) * 0.25;
            let dir = if dx >= 0.0 { 1.0 } else { -1.0 };
            if !collides(a.x - dir * push, a.y, world) {
                a.x -= dir * push;
            }
            if !collides(b.x + dir * push, b.y, world) {
                b.x += dir * push;
            }
        }
    }
}

fn death_pop(z: &Mob, fx: &mut Particles, rng: &mut Rng) {
    let (cx, cy) = z.center();
    if matches!(z.state, MobState::Burning { .. }) {
        fx.debris(cx, cy, ASH, 12);
        fx.sparkle(cx, cy, FIRE, 8);
    } else {
        fx.debris(cx, cy, SKIN, 10);
        fx.debris(cx, cy, LOOKS[z.variant].shirt, 6);
        fx.sparkle(cx, cy, EYE, 6);
    }
    for _ in 0..6 {
        smoke(fx, cx + (rng.next() - 0.5) * W, cy, rng);
    }
}

fn smoke(fx: &mut Particles, x: f32, y: f32, rng: &mut Rng) {
    let life = 0.6 + rng.next() * 0.5;
    fx.spawn(Particle {
        x,
        y,
        vx: (rng.next() - 0.5) * 14.0,
        vy: -18.0 - rng.next() * 16.0,
        life,
        max_life: life,
        color: rgba(58, 54, 62, 150),
        size: 2,
        gravity: -12.0,
        glow: 0,
        collide: false,
    });
}

// ---------------- physics (same rules as the player) ----------------

fn collides(x: f32, y: f32, world: &World) -> bool {
    let t = TILE as f32;
    let x0 = (x / t).floor() as i32;
    let x1 = ((x + W - 0.01) / t).floor() as i32;
    let y0 = (y / t).floor() as i32;
    let y1 = ((y + H - 0.01) / t).floor() as i32;
    (y0..=y1).any(|ty| (x0..=x1).any(|tx| world.solid(tx, ty)))
}

/// Returns true if a wall too tall to step up stopped the move.
fn move_x(z: &mut Mob, dx: f32, world: &World) -> bool {
    if dx == 0.0 {
        return false;
    }
    let nx = z.x + dx;
    if !collides(nx, z.y, world) {
        z.x = nx;
        return false;
    }
    // Step up onto 1-tile ledges.
    let up = TILE as f32 + 0.5;
    if z.on_ground && !collides(nx, z.y - up, world) && !collides(z.x, z.y - up, world) {
        z.y -= up;
        z.x = nx;
        return false;
    }
    let step = dx.signum() * 0.5;
    while !collides(z.x + step, z.y, world) {
        z.x += step;
    }
    z.vx = 0.0;
    true
}

fn move_y(z: &mut Mob, dy: f32, world: &World) {
    let ny = z.y + dy;
    if !collides(z.x, ny, world) {
        z.y = ny;
        z.on_ground = false;
        return;
    }
    let step = dy.signum() * 0.5;
    while !collides(z.x, z.y + step, world) {
        z.y += step;
    }
    if dy > 0.0 {
        z.on_ground = true;
    }
    z.vy = 0.0;
}

// ---------------- sprites ----------------

/// Body columns (outline to outline) in the unflipped sprite; the hitbox sits on these.
const BODY_L: i32 = 1;
const BODY_R: i32 = 10;
/// Eye pixel in the unflipped sprite.
const EYE_X: i32 = 9;
const EYE_Y: i32 = 4;

/// Head and torso, facing right with arms stretched out in front.
const TOP: [&str; 15] = [
    "...oooooo.......",
    "..ohhhhhhoo.....",
    ".ohhhhhhhhho....",
    ".ohhkkkkkkko....",
    ".ohkkkkkoeko....",
    "..okkkkkkkko....",
    "..oKkkkmmmko....",
    "...oKkkkkko.....",
    "..owwwtwwoooooo.",
    ".owwwwtTwwwkkkko",
    ".owWwwtTwWWKKKko",
    ".owWwwtTwoooooo.",
    ".oWwwwwtwwo.....",
    ".oWWwKwwwWo.....",
    "..opppppppo.....",
];
const LEGS_IDLE: [&str; 4] = ["..opPo.opPo.....", "..opPo.opPo.....", "..opPo.opPo.....", "..osso.osso....."];
const LEGS_W1: [&str; 4] = ["..opPo.opPo.....", ".opPo...opPo....", ".opPo....opo....", ".osso....osso..."];
const LEGS_W2: [&str; 4] = ["..opPoopPo......", "...opPopPo......", "...opPoPo.......", "...ossoso......."];
const LEGS_W3: [&str; 4] = [".opPo..opPo.....", ".opPo...opPo....", "opPo.....opo....", "osso.....osso..."];

fn zombie_sprites() -> Vec<ZombieFrames> {
    LOOKS
        .iter()
        .map(|look| {
            let pal: [(char, Rgba); 13] = [
                ('o', OUTLINE),
                ('h', look.hair),
                ('k', SKIN),
                ('K', SKIN_LO),
                ('e', EYE),
                ('m', MOUTH),
                ('w', look.shirt),
                ('W', look.shirt_lo),
                ('t', look.tie),
                ('T', look.tie_lo),
                ('p', look.pants),
                ('P', look.pants_lo),
                ('s', SHOES),
            ];
            let build = |legs: &[&str]| {
                let rows: Vec<&str> = TOP.iter().chain(legs.iter()).copied().collect();
                Sprite::from_ascii(&rows, &pal)
            };
            ZombieFrames {
                idle: build(&LEGS_IDLE),
                walk: [build(&LEGS_W1), build(&LEGS_W2), build(&LEGS_W3), build(&LEGS_W2)],
            }
        })
        .collect()
}

/// Draw only the first `rows` rows of a sprite (for zombies half out of the ground).
fn draw_rows(c: &mut Canvas, s: &Sprite, x: i32, y: i32, flip: bool, k: f32, rows: i32) {
    for sy in 0..rows.clamp(0, s.h) {
        for sx in 0..s.w {
            let src = if flip { s.w - 1 - sx } else { sx };
            let col = s.px[(sy * s.w + src) as usize];
            if col[3] > 0 {
                c.blend(x + sx, y + sy, shade(col, k));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sky::{DAY_LEN, NIGHT_LEN};
    use crate::{Game, DEFAULT_SEED};
    use shared::{GameApi, InputState};

    fn tick(g: &mut Game, seconds: f32) {
        let mut input = InputState::default();
        input.game_focused = true;
        for _ in 0..(seconds * 60.0) as usize {
            g.update(1.0 / 60.0, &input);
        }
    }

    #[test]
    fn sprites_are_well_formed() {
        for rows in [&TOP[..], &LEGS_IDLE[..], &LEGS_W1[..], &LEGS_W2[..], &LEGS_W3[..]] {
            for r in rows {
                assert_eq!(r.chars().count(), 16, "sprite row {r:?}");
            }
        }
        let m = Mobs::new();
        assert_eq!(m.sprites[0].idle.h, H as i32);
        assert!(m.sprites[0].idle.get(EYE_X, EYE_Y)[3] > 0, "eye pixel must be on the sprite");
    }

    #[test]
    fn no_zombies_by_day() {
        let mut g = Game::new(DEFAULT_SEED);
        tick(&mut g, 20.0);
        assert!(!g.sky.is_night());
        assert!(g.mobs.list.is_empty());
    }

    #[test]
    fn zombies_rise_at_night_and_burn_at_dawn() {
        let mut g = Game::new(DEFAULT_SEED);
        g.player.god = true;
        g.sky.clock = DAY_LEN + 0.1;
        tick(&mut g, 8.0);
        assert!(!g.mobs.list.is_empty(), "zombies should spawn at night");
        assert!(g.mobs.list.len() <= max_alive(1));
        tick(&mut g, NIGHT_LEN);
        assert!(!g.sky.is_night());
        assert!(g.mobs.list.is_empty(), "zombies should burn up by morning");
    }

    #[test]
    fn zombies_attack_an_undefended_screen() {
        let mut g = Game::new(DEFAULT_SEED);
        g.player.god = true;
        // Same spot the preview's scripted demo builds on.
        let tx = g.world.w / 2 - 2;
        let ty = g.world.surface[tx as usize] - 12;
        let id = g.screens.spawn(tx, ty, 22, 10);
        tick(&mut g, 2.0); // let it finish building
        g.drain_events();
        g.sky.clock = DAY_LEN + 0.1;
        tick(&mut g, NIGHT_LEN - 5.0);
        let ev = g.drain_events();
        let hits = ev.iter().filter(|e| matches!(e, GameEvent::ScreenDamaged { id: i, .. } if *i == id)).count();
        assert!(hits > 0, "zombies never reached the screen");
    }

    #[test]
    fn sword_hits_once_per_swing_and_kills() {
        let mut g = Game::new(DEFAULT_SEED);
        tick(&mut g, 2.0); // land on the ground
        g.sky.clock = DAY_LEN + 5.0;
        g.mobs.was_night = true;
        g.mobs.spawn_timer = 1000.0; // no extra spawns during the test
        g.player.god = true;
        g.player.facing = 1.0;
        let (pcx, _) = g.player.center();
        let py = g.player.y;
        g.mobs.spawn_zombie(pcx + 4.0, py + player::H, -1.0, 1);
        g.mobs.list[0].state = MobState::Walking;
        let hp0 = g.mobs.list[0].hp;

        g.player.swing = 0.28;
        tick(&mut g, 0.3);
        assert_eq!(g.mobs.list[0].hp, hp0 - 1, "one swing should land exactly one hit");

        for _ in 1..hp0 {
            let z = &mut g.mobs.list[0];
            z.x = pcx + 4.0;
            z.y = py;
            z.vx = 0.0;
            z.vy = 0.0;
            g.player.swing = 0.28;
            tick(&mut g, 0.3);
        }
        assert!(g.mobs.list.is_empty(), "zombie should die after {hp0} hits");
    }
}
