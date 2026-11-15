//! Passive mobs: bunnies, pigs, cows, a cat, birds and fireflies. Owner: person B2.
//!
//! None of them hurt the player or touch screens, and they can't be killed: a
//! sword hit (or, for the shy ones, the player walking up) sends them running
//! with a little puff. By day a few roam near the player, spawning just off
//! camera. At dusk bunnies burrow into the ground, birds fly off, the cat
//! leaves, and fireflies come out. Pigs and cows don't mind the dark.
//!
//! The cat walks to an online screen, leaps onto its frame and naps there
//! until the screen starts dying, night falls, or the player pokes it. Birds
//! perch on screen frames and treetops and scatter when anything comes close.

use super::critter_art as art;
use super::{draw_rows, overlaps, MobCtx, Rng};
use crate::camera::Camera;
use crate::gfx::{rgb, with_alpha, Canvas, Sprite};
use crate::light::Light;
use crate::world::{Tile, World, TILE};
use shared::ScreenId;

// ---------------- tuning ----------------

const GRAVITY: f32 = 1100.0;
const MAX_FALL: f32 = 520.0;
/// Animals live within this many view widths of the player.
const RANGE_VIEWS: f32 = 1.5;
/// Below this daylight it's dusk: bunnies burrow, birds and the cat leave, no new spawns.
const DUSK: f32 = 0.45;
/// Seconds between spawn attempts (before jitter).
const SPAWN_EVERY: f32 = 2.0;
const BIRDS: usize = 5;
const FIREFLIES: usize = 18;
/// Cat leap time (s) onto / off a screen.
const LEAP_TIME: f32 = 0.6;
/// Birds scatter when the player (or a zombie / bat) gets this close (px).
const BIRD_SHY: f32 = 48.0;
const BIRD_THREAT: f32 = 70.0;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Bunny,
    Pig,
    Cow,
    Cat,
}

impl Kind {
    const ALL: [Kind; 4] = [Kind::Bunny, Kind::Pig, Kind::Cow, Kind::Cat];

    /// Hitbox (w, h), world px.
    fn size(self) -> (f32, f32) {
        match self {
            Kind::Bunny => (10.0, 10.0),
            Kind::Pig => (20.0, 12.0),
            Kind::Cow => (28.0, 16.0),
            Kind::Cat => (12.0, 9.0),
        }
    }

    fn cap(self) -> usize {
        match self {
            Kind::Bunny => 3,
            Kind::Pig | Kind::Cow => 2,
            Kind::Cat => 1,
        }
    }

    /// Walking speed (px/s).
    fn speed(self) -> f32 {
        match self {
            Kind::Bunny => 46.0,
            Kind::Pig => 18.0,
            Kind::Cow => 14.0,
            Kind::Cat => 30.0,
        }
    }

    /// Player distance that makes it run off (None: only a sword hit does).
    fn shy(self) -> Option<f32> {
        match self {
            Kind::Bunny => Some(56.0),
            Kind::Cat => Some(40.0),
            Kind::Pig | Kind::Cow => None,
        }
    }
}

// ---------------- physics (sized box; same rules as the zombies) ----------------

#[derive(Clone, Copy, Debug)]
struct Body {
    /// Top-left, world px.
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    vx: f32,
    vy: f32,
    on_ground: bool,
}

impl Body {
    fn center(&self) -> (f32, f32) {
        (self.x + self.w / 2.0, self.y + self.h / 2.0)
    }

    fn rect(&self) -> (f32, f32, f32, f32) {
        (self.x, self.y, self.w, self.h)
    }

    fn collides(&self, x: f32, y: f32, world: &World) -> bool {
        let t = TILE as f32;
        let x0 = (x / t).floor() as i32;
        let x1 = ((x + self.w - 0.01) / t).floor() as i32;
        let y0 = (y / t).floor() as i32;
        let y1 = ((y + self.h - 0.01) / t).floor() as i32;
        (y0..=y1).any(|ty| (x0..=x1).any(|tx| world.solid(tx, ty)))
    }

    /// Gravity and movement. Returns true if a wall too tall to step up stopped it.
    fn step(&mut self, dt: f32, world: &World) -> bool {
        self.vy = (self.vy + GRAVITY * dt).min(MAX_FALL);
        let blocked = self.move_x(self.vx * dt, world);
        self.move_y(self.vy * dt, world);
        blocked
    }

    fn move_x(&mut self, dx: f32, world: &World) -> bool {
        if dx == 0.0 {
            return false;
        }
        let nx = self.x + dx;
        if !self.collides(nx, self.y, world) {
            self.x = nx;
            return false;
        }
        let up = TILE as f32 + 0.5;
        if self.on_ground && !self.collides(nx, self.y - up, world) && !self.collides(self.x, self.y - up, world) {
            self.y -= up;
            self.x = nx;
            return false;
        }
        let step = dx.signum() * 0.5;
        let mut n = 0;
        while !self.collides(self.x + step, self.y, world) && n < 64 {
            self.x += step;
            n += 1;
        }
        self.vx = 0.0;
        true
    }

    fn move_y(&mut self, dy: f32, world: &World) {
        let ny = self.y + dy;
        if !self.collides(self.x, ny, world) {
            self.y = ny;
            self.on_ground = false;
            return;
        }
        let step = dy.signum() * 0.5;
        let mut n = 0;
        while !self.collides(self.x, self.y + step, world) && n < 64 {
            self.y += step;
            n += 1;
        }
        if dy > 0.0 {
            self.on_ground = true;
        }
        self.vy = 0.0;
    }
}

// ---------------- ground critters ----------------

#[derive(Clone, Copy, Debug, PartialEq)]
enum Act {
    Idle { t: f32, graze: bool },
    Walk { dir: f32, t: f32 },
    Flee { dir: f32, t: f32 },
    /// Bunny sinking into the ground, 0..1.
    Burrow { p: f32 },
    /// Cat heading for a screen to nap on.
    ToScreen { id: ScreenId },
    /// Cat jumping from (x0, y0) to (x1, y1) (top-left); lands napping on `nap` = (screen, x offset).
    Leap { x0: f32, y0: f32, x1: f32, y1: f32, t: f32, nap: Option<(ScreenId, f32)> },
    /// Cat asleep on a screen frame, `dx` from its left edge.
    Nap { id: ScreenId, dx: f32, t: f32 },
}

#[derive(Clone, Debug)]
struct Critter {
    kind: Kind,
    body: Body,
    act: Act,
    facing: f32,
    anim: f32,
    hop_cd: f32,
    poke_cd: f32,
    gone: bool,
}

// ---------------- birds ----------------

#[derive(Clone, Copy, Debug, PartialEq)]
enum Perch {
    /// On a screen frame's top edge, `dx` px from its left.
    Screen(ScreenId, f32),
    /// On top of a leaf tile.
    Leaf(i32, i32),
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum BirdAct {
    Fly { perch: Perch },
    Sit { perch: Perch, t: f32 },
    Scatter,
}

#[derive(Clone, Debug)]
struct Bird {
    /// Feet, world px.
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    act: BirdAct,
    flap: f32,
    facing: f32,
    gone: bool,
}

#[derive(Clone, Debug)]
struct Firefly {
    x: f32,
    y: f32,
    phase: f32,
}

struct Art {
    bunny: [Sprite; 2],
    pig: [Sprite; 3],
    cow: [Sprite; 3],
    cat: [Sprite; 3],
    bird: [Sprite; 3],
}

pub struct Critters {
    list: Vec<Critter>,
    birds: Vec<Bird>,
    flies: Vec<Firefly>,
    rng: Rng,
    spawn_t: f32,
    bird_t: f32,
    seeded: bool,
    t: f32,
    /// 0..1 firefly visibility (set in `update`).
    glow: f32,
    art: Art,
}

impl Default for Critters {
    fn default() -> Self {
        Self::new()
    }
}

impl Critters {
    pub fn new() -> Self {
        let s = |rows: &[&str]| Sprite::from_ascii(rows, &art::PALETTE);
        Critters {
            list: Vec::new(),
            birds: Vec::new(),
            flies: Vec::new(),
            rng: Rng(0xC0FF_EE01),
            spawn_t: 0.5,
            bird_t: 0.5,
            seeded: false,
            t: 0.0,
            glow: 0.0,
            art: Art {
                bunny: [s(&art::BUNNY_SIT), s(&art::BUNNY_HOP)],
                pig: [s(&art::PIG_A), s(&art::PIG_B), s(&art::PIG_GRAZE)],
                cow: [s(&art::COW_A), s(&art::COW_B), s(&art::COW_GRAZE)],
                cat: [s(&art::CAT_A), s(&art::CAT_B), s(&art::CAT_SLEEP)],
                bird: [s(&art::BIRD_PERCH), s(&art::BIRD_UP), s(&art::BIRD_DOWN)],
            },
        }
    }

    fn count(&self, kind: Kind) -> usize {
        self.list.iter().filter(|c| c.kind == kind && !c.gone).count()
    }

    /// Put a critter on the ground at world x (centre). False if there's no
    /// open grass there (or a screen is in the way).
    fn spawn_at(&mut self, kind: Kind, x: f32, ctx: &MobCtx) -> bool {
        let world = ctx.world;
        let (w, h) = kind.size();
        let col = (x / TILE as f32).floor() as i32;
        if col < 2 || col >= world.w - 2 {
            return false;
        }
        let gy = world.ground_y(col);
        if !matches!(world.get(col, gy), Tile::Grass | Tile::Dirt) {
            return false;
        }
        let body = Body { x: x - w / 2.0, y: (gy * TILE) as f32 - h - 0.5, w, h, vx: 0.0, vy: 0.0, on_ground: true };
        if body.collides(body.x, body.y, world) {
            return false;
        }
        if ctx.screens.list.iter().any(|s| overlaps(body.rect(), s.frame_px())) {
            return false;
        }
        let r = &mut self.rng;
        let facing = if r.next() < 0.5 { -1.0 } else { 1.0 };
        self.list.push(Critter {
            kind,
            body,
            act: Act::Idle { t: 0.5 + 2.0 * r.next(), graze: false },
            facing,
            anim: r.next() * 2.0,
            hop_cd: 0.0,
            poke_cd: 0.0,
            gone: false,
        });
        true
    }

    /// `threats`: zombie and bat centres (birds scatter from them).
    pub fn update(&mut self, dt: f32, ctx: &mut MobCtx, threats: &[(f32, f32)]) {
        self.t += dt;
        let daylight = ctx.sky.daylight();
        let dusk = daylight < DUSK;
        let (pcx, _) = ctx.player.center();
        let vw = ctx.cam.vw;

        // --- populate ---
        if !dusk {
            if !self.seeded {
                // A few animals already in view for the opening shot.
                self.seeded = true;
                for kind in [Kind::Bunny, Kind::Bunny, Kind::Pig, Kind::Cow, Kind::Cat] {
                    for _ in 0..6 {
                        let side = if self.rng.next() < 0.5 { -1.0 } else { 1.0 };
                        let x = pcx + side * (70.0 + (vw * 0.45 - 70.0).max(10.0) * self.rng.next());
                        if self.spawn_at(kind, x, ctx) {
                            break;
                        }
                    }
                }
            }
            self.spawn_t -= dt;
            if self.spawn_t <= 0.0 {
                self.spawn_t = SPAWN_EVERY * (0.6 + 0.8 * self.rng.next());
                let start = (self.rng.next() * 4.0) as usize;
                for i in 0..4 {
                    let kind = Kind::ALL[(start + i) % 4];
                    if self.count(kind) < kind.cap() {
                        // Just off camera, so they walk into view.
                        let side = if self.rng.next() < 0.5 { -1.0 } else { 1.0 };
                        let x = pcx + side * (vw * 0.5 + 30.0 + vw * 0.4 * self.rng.next());
                        self.spawn_at(kind, x, ctx);
                        break;
                    }
                }
            }
        }

        // --- ground critters ---
        let online = ctx.screens.targets();
        // Every screen frame (building, online or dying) is a wall to animals,
        // so they never wander into a screen's dug-out pit or over its window.
        let walls: Vec<(f32, f32, f32, f32)> = ctx.screens.list.iter().map(|s| s.frame_px()).collect();
        let Critters { list, rng, .. } = self;
        for cr in list.iter_mut() {
            update_critter(cr, dt, ctx, &online, &walls, dusk, rng);
            let (cx, _) = cr.body.center();
            if (cx - pcx).abs() > RANGE_VIEWS * vw || cr.body.y > ctx.world.height_px() {
                cr.gone = true;
            }
            // The cat leaving at dusk disappears once it's off camera.
            if dusk && cr.kind == Kind::Cat && (cx - pcx).abs() > vw * 0.6 {
                cr.gone = true;
            }
        }
        list.retain(|c| !c.gone);

        // --- birds ---
        self.update_birds(dt, ctx, &online, dusk, threats);

        // --- fireflies ---
        self.glow = ((DUSK - daylight) / 0.3).clamp(0.0, 1.0);
        self.update_fireflies(dt, ctx, &walls);
    }

    fn update_birds(&mut self, dt: f32, ctx: &mut MobCtx, online: &[(ScreenId, (f32, f32, f32, f32))], dusk: bool, threats: &[(f32, f32)]) {
        let cam = (ctx.cam.x, ctx.cam.y, ctx.cam.vw, ctx.cam.vh);
        if !dusk {
            self.bird_t -= dt;
            if self.bird_t <= 0.0 {
                self.bird_t = 1.5 * (0.6 + 0.8 * self.rng.next());
                if self.birds.len() < BIRDS {
                    if let Some(perch) = self.pick_perch(ctx.world, online, cam) {
                        let side = if self.rng.next() < 0.5 { -1.0 } else { 1.0 };
                        let x = if side < 0.0 { cam.0 - 20.0 } else { cam.0 + cam.2 + 20.0 };
                        let y = cam.1 + 10.0 + 40.0 * self.rng.next();
                        self.birds.push(Bird { x, y, vx: 0.0, vy: 0.0, act: BirdAct::Fly { perch }, flap: 0.0, facing: -side, gone: false });
                    }
                }
            }
        }

        let (pcx, pcy) = ctx.player.center();
        let sword = ctx.player.attack_box();
        let world = ctx.world;
        // Re-perching needs `pick_perch` (&self), so collect the birds that want a new spot.
        let mut wants_new: Vec<usize> = Vec::new();
        for (i, b) in self.birds.iter_mut().enumerate() {
            b.flap += dt;
            let near = |px: f32, py: f32, r: f32| (px - b.x).hypot(py - (b.y - 3.0)) < r;
            let spooked = dusk
                || near(pcx, pcy, BIRD_SHY)
                || threats.iter().any(|&(tx, ty)| near(tx, ty, BIRD_THREAT))
                || sword.is_some_and(|s| overlaps(s, (b.x - 4.0, b.y - 7.0, 8.0, 7.0)));
            match b.act {
                BirdAct::Fly { perch } | BirdAct::Sit { perch, .. } if spooked || perch_pos(perch, world, online).is_none() => {
                    b.act = BirdAct::Scatter;
                    b.vy = -60.0;
                    b.vx = if b.x < pcx { -90.0 } else { 90.0 };
                }
                BirdAct::Fly { perch } => {
                    let Some((tx, ty)) = perch_pos(perch, world, online) else { continue };
                    let (dx, dy) = (tx - b.x, ty - b.y);
                    let d = dx.hypot(dy);
                    if d < 3.0 {
                        b.x = tx;
                        b.y = ty;
                        b.vx = 0.0;
                        b.vy = 0.0;
                        b.act = BirdAct::Sit { perch, t: 6.0 + 12.0 * self.rng.next() };
                    } else {
                        let s = 110.0 * (d / 30.0).min(1.0).max(0.3);
                        let k = 1.0 - (-5.0 * dt).exp();
                        b.vx += (dx / d * s - b.vx) * k;
                        b.vy += (dy / d * s - b.vy) * k;
                        if b.vx.abs() > 5.0 {
                            b.facing = b.vx.signum();
                        }
                    }
                }
                BirdAct::Sit { perch, t } => {
                    if let Some((tx, ty)) = perch_pos(perch, world, online) {
                        b.x = tx;
                        b.y = ty;
                    }
                    let nt = t - dt;
                    if nt <= 0.0 {
                        wants_new.push(i);
                    } else {
                        b.act = BirdAct::Sit { perch, t: nt };
                    }
                }
                BirdAct::Scatter => {
                    b.vy = (b.vy - 260.0 * dt).max(-150.0);
                    b.vx = b.vx.signum() * (b.vx.abs() + 80.0 * dt).min(120.0);
                    if b.vx.abs() > 5.0 {
                        b.facing = b.vx.signum();
                    }
                    let (cx, cy, cw, _) = cam;
                    if b.y < cy - 20.0 || b.x < cx - 40.0 || b.x > cx + cw + 40.0 {
                        b.gone = true;
                    }
                }
            }
            if !matches!(b.act, BirdAct::Sit { .. }) {
                b.x += b.vx * dt;
                b.y += b.vy * dt;
            }
        }
        for i in wants_new {
            let next = if self.rng.next() < 0.5 { self.pick_perch(world, online, cam) } else { None };
            let b = &mut self.birds[i];
            match next {
                Some(perch) => b.act = BirdAct::Fly { perch },
                None => {
                    b.act = BirdAct::Scatter;
                    b.vy = -60.0;
                    b.vx = if b.facing < 0.0 { -90.0 } else { 90.0 };
                }
            }
        }
        self.birds.retain(|b| !b.gone);
    }

    /// A free perch in or near the view: screen frame tops (preferred) or treetops.
    fn pick_perch(&mut self, world: &World, online: &[(ScreenId, (f32, f32, f32, f32))], cam: (f32, f32, f32, f32)) -> Option<Perch> {
        let mut spots: Vec<Perch> = Vec::new();
        for &(id, (fx, _, fw, _)) in online {
            if fx + fw < cam.0 - 40.0 || fx > cam.0 + cam.2 + 40.0 {
                continue;
            }
            let mut dx = 10.0;
            while dx < fw - 10.0 {
                spots.push(Perch::Screen(id, dx));
                dx += 22.0;
            }
        }
        let screens = spots.len();
        let t = TILE as f32;
        let c0 = ((cam.0 / t).floor() as i32).max(1);
        let c1 = (((cam.0 + cam.2) / t).ceil() as i32).min(world.w - 2);
        for col in c0..=c1 {
            let ground = world.ground_y(col);
            if let Some(row) = (1..ground).find(|&r| world.get(col, r) == Tile::Leaves) {
                if world.get(col, row - 1) == Tile::Air {
                    spots.push(Perch::Leaf(col, row));
                }
            }
        }
        let taken = |p: &Perch| {
            self.birds.iter().any(|b| match b.act {
                BirdAct::Fly { perch } | BirdAct::Sit { perch, .. } => perch == *p,
                BirdAct::Scatter => false,
            })
        };
        let free: Vec<Perch> = spots.iter().copied().filter(|p| !taken(p)).collect();
        if free.is_empty() {
            return None;
        }
        // Prefer screens (they're the stars of the show) when there are any free.
        let free_screens: Vec<Perch> = free.iter().copied().filter(|p| matches!(p, Perch::Screen(..))).collect();
        let pool = if screens > 0 && !free_screens.is_empty() && self.rng.next() < 0.7 { free_screens } else { free };
        let i = ((self.rng.next() * pool.len() as f32) as usize).min(pool.len() - 1);
        Some(pool[i])
    }

    fn update_fireflies(&mut self, dt: f32, ctx: &mut MobCtx, walls: &[(f32, f32, f32, f32)]) {
        if self.glow <= 0.0 {
            self.flies.clear();
            return;
        }
        let (cx, cy, cw, ch) = (ctx.cam.x, ctx.cam.y, ctx.cam.vw, ctx.cam.vh);
        let world = ctx.world;
        let t = self.t;
        for f in self.flies.iter_mut() {
            f.x += (t * 0.8 + f.phase).sin() * 12.0 * dt;
            f.y += (t * 1.1 + f.phase * 1.7).cos() * 7.0 * dt;
        }
        let in_frame = |x: f32, y: f32| walls.iter().any(|&(fx, fy, fw, fh)| x >= fx && x < fx + fw && y >= fy && y < fy + fh);
        self.flies.retain(|f| f.x > cx - 30.0 && f.x < cx + cw + 30.0 && f.y > cy - 30.0 && f.y < cy + ch + 30.0 && !world.solid_px(f.x, f.y) && !in_frame(f.x, f.y));
        let mut tries = 0;
        while self.flies.len() < FIREFLIES && tries < 40 {
            tries += 1;
            let x = cx + cw * self.rng.next();
            let col = (x / TILE as f32).floor() as i32;
            if col < 0 || col >= world.w {
                continue;
            }
            let ground = (world.ground_y(col) * TILE) as f32;
            let y = ground - 6.0 - 48.0 * self.rng.next();
            if y < cy || y > cy + ch || world.solid_px(x, y) || in_frame(x, y) {
                continue;
            }
            self.flies.push(Firefly { x, y, phase: self.rng.next() * 6.28 });
        }
    }

    pub fn draw(&self, c: &mut Canvas, cam: &Camera, light: &Light) {
        for cr in &self.list {
            let spr = self.frame(cr);
            let b = &cr.body;
            // Centred on the hitbox, feet on its bottom edge.
            let (vx, vy) = cam.to_view(b.x + b.w / 2.0 - (spr.w / 2) as f32, b.y + b.h - spr.h as f32);
            if vx < -spr.w - 8 || vx > c.w + 8 || vy < -spr.h - 8 || vy > c.h + 8 {
                continue;
            }
            let (px, py) = b.center();
            let k = light.level_at(px, py).clamp(0.45, 1.0);
            let flip = cr.facing < 0.0;
            match cr.act {
                Act::Burrow { p } => {
                    let rows = ((1.0 - p) * spr.h as f32).ceil() as i32;
                    draw_rows(c, spr, vx, vy + spr.h - rows, flip, k, rows);
                }
                Act::Nap { t, .. } => {
                    c.sprite_shaded(spr, vx, vy, flip, k);
                    // Little "z"s drifting up.
                    let p = (t * 0.6).fract();
                    let a = (1.0 - p) * 220.0;
                    let zx = vx + if flip { 2 } else { spr.w - 4 } + (p * 6.0) as i32;
                    c.text(zx, vy - 4 - (p * 12.0) as i32, "z", 1, with_alpha(rgb(230, 236, 255), a as u8));
                }
                _ => c.sprite_shaded(spr, vx, vy, flip, k),
            }
        }

        for bd in &self.birds {
            let spr = match bd.act {
                BirdAct::Sit { .. } => &self.art.bird[0],
                _ => &self.art.bird[if (bd.flap * 10.0) as i32 % 2 == 0 { 1 } else { 2 }],
            };
            // Peck: bob the head down now and then while perched.
            let peck = matches!(bd.act, BirdAct::Sit { .. }) && (bd.flap * 1.7).sin() > 0.93;
            let (vx, vy) = cam.to_view(bd.x - (spr.w / 2) as f32, bd.y - (spr.h - 1) as f32);
            if vx < -16 || vx > c.w + 16 || vy < -16 || vy > c.h + 16 {
                continue;
            }
            let k = light.level_at(bd.x, bd.y).clamp(0.45, 1.0);
            c.sprite_shaded(spr, vx, vy + peck as i32, bd.facing < 0.0, k);
        }

        if self.glow > 0.0 {
            for f in &self.flies {
                let (vx, vy) = cam.to_view(f.x, f.y);
                let pulse = ((self.t * 2.6 + f.phase).sin() * 0.5 + 0.5).powi(2);
                let a = self.glow * (0.3 + 0.7 * pulse);
                c.glow(vx, vy, 5, with_alpha(rgb(190, 255, 110), (a * 90.0) as u8));
                c.blend(vx, vy, with_alpha(rgb(240, 255, 170), (a * 255.0) as u8));
            }
        }
    }

    fn frame(&self, cr: &Critter) -> &Sprite {
        let step = (cr.anim as usize) % 2;
        let moving = cr.body.vx.abs() > 1.0;
        match cr.kind {
            Kind::Bunny => &self.art.bunny[if cr.body.on_ground { 0 } else { 1 }],
            Kind::Pig => match cr.act {
                Act::Idle { graze: true, .. } => &self.art.pig[2],
                _ => &self.art.pig[if moving { step } else { 0 }],
            },
            Kind::Cow => match cr.act {
                Act::Idle { graze: true, .. } => &self.art.cow[2],
                _ => &self.art.cow[if moving { step } else { 0 }],
            },
            Kind::Cat => match cr.act {
                Act::Nap { .. } => &self.art.cat[2],
                _ => &self.art.cat[if moving { step } else { 0 }],
            },
        }
    }
}

/// Where a perch is now (feet position), or None if it's gone.
fn perch_pos(p: Perch, world: &World, online: &[(ScreenId, (f32, f32, f32, f32))]) -> Option<(f32, f32)> {
    match p {
        Perch::Screen(id, dx) => online.iter().find(|(i, _)| *i == id).map(|(_, (fx, fy, _, _))| (fx + dx, *fy)),
        Perch::Leaf(tx, ty) => {
            (world.get(tx, ty) == Tile::Leaves && world.get(tx, ty - 1) == Tile::Air).then(|| ((tx * TILE + 8) as f32, (ty * TILE) as f32))
        }
    }
}

fn update_critter(
    cr: &mut Critter,
    dt: f32,
    ctx: &mut MobCtx,
    online: &[(ScreenId, (f32, f32, f32, f32))],
    walls: &[(f32, f32, f32, f32)],
    dusk: bool,
    rng: &mut Rng,
) {
    cr.hop_cd = (cr.hop_cd - dt).max(0.0);
    cr.poke_cd = (cr.poke_cd - dt).max(0.0);
    let (pcx, pcy) = ctx.player.center();
    let (cx, cy) = cr.body.center();
    let away = if cx >= pcx { 1.0 } else { -1.0 };
    let world = ctx.world;
    let ground_top = |x: f32, h: f32| (world.ground_y((x / TILE as f32).floor() as i32) * TILE) as f32 - h - 0.5;

    // Poked with the sword: a puff, then run (the cat jumps down first).
    if cr.poke_cd <= 0.0 {
        if let Some(sword) = ctx.player.attack_box() {
            if overlaps(sword, cr.body.rect()) {
                cr.poke_cd = 0.5;
                ctx.fx.sparkle(cx, cy, rgb(255, 255, 255), 5);
                cr.act = match cr.act {
                    Act::Nap { .. } | Act::Leap { .. } => leap_down(cr, ground_top(cx, cr.body.h)),
                    _ => {
                        cr.body.vy = -140.0;
                        cr.body.on_ground = false;
                        Act::Flee { dir: away, t: 1.4 }
                    }
                };
            }
        }
    }

    // Shy ones run from the player.
    if let Some(r) = cr.kind.shy() {
        let close = (pcx - cx).abs() < r && (pcy - cy).abs() < 40.0;
        if close && matches!(cr.act, Act::Idle { .. } | Act::Walk { .. } | Act::ToScreen { .. }) {
            cr.act = Act::Flee { dir: away, t: 1.2 };
        }
    }

    // Dusk: bunnies burrow, the cat heads off; pigs and cows carry on.
    if dusk {
        match (cr.kind, cr.act) {
            (Kind::Bunny, Act::Burrow { .. }) => {}
            (Kind::Bunny, _) if cr.body.on_ground => cr.act = Act::Burrow { p: 0.0 },
            (Kind::Cat, Act::Nap { .. }) => cr.act = leap_down(cr, ground_top(cx, cr.body.h)),
            (Kind::Cat, Act::Idle { .. } | Act::Walk { .. } | Act::ToScreen { .. }) => cr.act = Act::Flee { dir: away, t: 60.0 },
            _ => {}
        }
    }

    let speed = cr.kind.speed();
    let mut physics = true;
    match cr.act {
        Act::Idle { t, graze } => {
            cr.body.vx = 0.0;
            let nt = t - dt;
            cr.act = if nt > 0.0 {
                Act::Idle { t: nt, graze }
            } else if cr.kind == Kind::Cat && !dusk && rng.next() < 0.6 {
                match nearest_screen(online, cx, 420.0) {
                    Some(id) => Act::ToScreen { id },
                    None => Act::Walk { dir: if rng.next() < 0.5 { -1.0 } else { 1.0 }, t: 1.0 + 3.0 * rng.next() },
                }
            } else {
                Act::Walk { dir: if rng.next() < 0.5 { -1.0 } else { 1.0 }, t: 1.0 + 3.0 * rng.next() }
            };
        }
        Act::Walk { dir, t } => {
            cr.body.vx = dir * speed;
            let nt = t - dt;
            let graze = matches!(cr.kind, Kind::Pig | Kind::Cow) && rng.next() < 0.6;
            cr.act = if nt > 0.0 { Act::Walk { dir, t: nt } } else { Act::Idle { t: 1.0 + 2.5 * rng.next(), graze } };
        }
        Act::Flee { dir, t } => {
            cr.body.vx = dir * speed * 1.9;
            let nt = t - dt;
            cr.act = if nt > 0.0 { Act::Flee { dir, t: nt } } else { Act::Idle { t: 0.5 + rng.next(), graze: false } };
        }
        Act::Burrow { p } => {
            physics = false;
            cr.body.vx = 0.0;
            let np = p + dt / 0.9;
            if (np * 10.0) as i32 != (p * 10.0) as i32 {
                ctx.fx.debris(cx, cr.body.y + cr.body.h, rgb(134, 88, 58), 2);
            }
            if np >= 1.0 {
                cr.gone = true;
            }
            cr.act = Act::Burrow { p: np };
        }
        Act::ToScreen { id } => match online.iter().find(|(i, _)| *i == id) {
            None => cr.act = Act::Idle { t: 1.0, graze: false },
            Some(&(_, (fx, fy, fw, _))) => {
                let target = fx + fw / 2.0;
                let dx = target - cx;
                // The frame is a wall, so leap from beside it (or from under a floating one).
                let at_edge = cr.body.x + cr.body.w > fx - 4.0 && cr.body.x < fx + fw + 4.0;
                if at_edge && cr.body.on_ground {
                    // Leap up onto the frame, somewhere near the middle.
                    let x1 = cr.body.x.clamp(fx + 4.0, fx + fw - cr.body.w - 4.0);
                    let y1 = fy - cr.body.h;
                    cr.act = Act::Leap { x0: cr.body.x, y0: cr.body.y, x1, y1, t: 0.0, nap: Some((id, x1 - fx)) };
                } else {
                    cr.body.vx = dx.signum() * speed;
                }
            }
        },
        Act::Leap { x0, y0, x1, y1, t, nap } => {
            physics = false;
            let nt = t + dt;
            let p = (nt / LEAP_TIME).min(1.0);
            let arc = 26.0 + (y0 - y1).max(0.0) * 0.3;
            cr.body.x = x0 + (x1 - x0) * p;
            cr.body.y = y0 + (y1 - y0) * p - arc * 4.0 * p * (1.0 - p);
            cr.body.vx = 0.0;
            cr.body.vy = 0.0;
            if (x1 - x0).abs() > 0.5 {
                cr.facing = (x1 - x0).signum();
            }
            cr.act = if p < 1.0 {
                Act::Leap { x0, y0, x1, y1, t: nt, nap }
            } else {
                match nap {
                    Some((id, dx)) => Act::Nap { id, dx, t: 0.0 },
                    None => {
                        cr.body.on_ground = false;
                        if dusk {
                            Act::Flee { dir: away, t: 60.0 }
                        } else {
                            Act::Flee { dir: away, t: 1.2 }
                        }
                    }
                }
            };
        }
        Act::Nap { id, dx, t } => {
            physics = false;
            match online.iter().find(|(i, _)| *i == id) {
                // The screen is dying (or gone): jump off.
                None => {
                    cr.body.vy = -150.0;
                    cr.body.on_ground = false;
                    cr.act = Act::Flee { dir: if rng.next() < 0.5 { -1.0 } else { 1.0 }, t: 1.2 };
                    physics = true;
                }
                Some(&(_, (fx, fy, _, _))) => {
                    cr.body.x = fx + dx;
                    cr.body.y = fy - cr.body.h;
                    cr.body.vx = 0.0;
                    cr.body.vy = 0.0;
                    cr.body.on_ground = true;
                    cr.act = Act::Nap { id, dx, t: t + dt };
                }
            }
        }
    }

    if physics {
        let (ox, oy) = (cr.body.x, cr.body.y);
        let was_inside = walls.iter().any(|w| overlaps(cr.body.rect(), *w));
        let mut blocked = cr.body.step(dt, world);
        if !was_inside && walls.iter().any(|w| overlaps(cr.body.rect(), *w)) {
            // Bumped into a screen: undo the move and turn round.
            cr.body.x = ox;
            if walls.iter().any(|w| overlaps(cr.body.rect(), *w)) {
                cr.body.y = oy;
                cr.body.vy = 0.0;
            }
            cr.body.vx = 0.0;
            blocked = false;
            cr.act = match cr.act {
                Act::Walk { dir, t } => Act::Walk { dir: -dir, t },
                Act::Flee { dir, t } => Act::Flee { dir: -dir, t },
                a => a,
            };
        }
        let moving = cr.body.vx.abs() > 1.0;
        if moving {
            cr.facing = cr.body.vx.signum();
        }
        // Bunnies hop along; a wall too tall makes bunnies and the cat jump and the rest turn round.
        if cr.kind == Kind::Bunny && moving && cr.body.on_ground && cr.hop_cd <= 0.0 {
            cr.body.vy = -190.0;
            cr.body.on_ground = false;
            cr.hop_cd = if matches!(cr.act, Act::Flee { .. }) { 0.22 } else { 0.35 };
        }
        if blocked && cr.body.on_ground {
            match cr.kind {
                Kind::Bunny | Kind::Cat => {
                    cr.body.vy = -300.0;
                    cr.body.on_ground = false;
                }
                Kind::Pig | Kind::Cow => {
                    if let Act::Walk { dir, t } = cr.act {
                        cr.act = Act::Walk { dir: -dir, t };
                    }
                }
            }
        }
        if cr.body.on_ground && moving {
            cr.anim += dt * cr.body.vx.abs() / 10.0;
        }
    }
}

/// Jump from wherever the cat is down to the ground below.
fn leap_down(cr: &Critter, ground_y: f32) -> Act {
    Act::Leap { x0: cr.body.x, y0: cr.body.y, x1: cr.body.x + cr.facing * 14.0, y1: ground_y, t: 0.0, nap: None }
}

fn nearest_screen(online: &[(ScreenId, (f32, f32, f32, f32))], x: f32, max: f32) -> Option<ScreenId> {
    online
        .iter()
        .map(|(id, (fx, _, fw, _))| (*id, (fx + fw / 2.0 - x).abs()))
        .filter(|(_, d)| *d < max)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(id, _)| id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sky::DAY_LEN;
    use crate::{Game, DEFAULT_SEED};
    use shared::{GameApi, GameEvent, InputState};

    fn tick(g: &mut Game, seconds: f32) {
        let mut input = InputState::default();
        input.game_focused = true;
        for _ in 0..(seconds * 60.0).round() as usize {
            g.update(1.0 / 60.0, &input);
        }
    }

    #[test]
    fn sprites_are_well_formed() {
        for rows in [
            &art::BUNNY_SIT[..], &art::BUNNY_HOP[..], &art::PIG_A[..], &art::PIG_B[..], &art::PIG_GRAZE[..],
            &art::COW_A[..], &art::COW_B[..], &art::COW_GRAZE[..], &art::CAT_A[..], &art::CAT_B[..],
            &art::CAT_SLEEP[..], &art::BIRD_PERCH[..], &art::BIRD_UP[..], &art::BIRD_DOWN[..],
        ] {
            let w = rows[0].chars().count();
            assert!(rows.iter().all(|r| r.chars().count() == w), "ragged sprite");
            for r in rows {
                assert!(r.chars().all(|ch| ch == '.' || art::PALETTE.iter().any(|(k, _)| *k == ch)), "unknown colour in {r:?}");
            }
        }
    }

    #[test]
    fn animals_roam_by_day() {
        let mut g = Game::new(DEFAULT_SEED);
        tick(&mut g, 3.0);
        assert!(!g.mobs.critters.list.is_empty(), "some animals should be around by day");
        assert!(g.mobs.critters.flies.is_empty(), "no fireflies by day");
    }

    #[test]
    fn bunnies_and_birds_leave_at_night_fireflies_come_out() {
        let mut g = Game::new(DEFAULT_SEED);
        tick(&mut g, 3.0);
        g.sky.clock = DAY_LEN + 0.1;
        tick(&mut g, 6.0);
        let c = &g.mobs.critters;
        assert!(!c.list.iter().any(|cr| cr.kind == Kind::Bunny), "bunnies should burrow at dusk");
        assert!(c.birds.is_empty(), "birds should fly off at dusk");
        assert!(!c.flies.is_empty(), "fireflies should come out");
    }

    #[test]
    fn animals_are_harmless() {
        let mut g = Game::new(DEFAULT_SEED);
        let tx = g.world.w / 2 - 2;
        g.screens.spawn(tx, g.world.surface[tx as usize] - 12, 22, 10);
        let hp = g.player.hp;
        tick(&mut g, 30.0);
        assert_eq!(g.player.hp, hp);
        assert!(!g.drain_events().iter().any(|e| matches!(e, GameEvent::ScreenDamaged { .. })));
    }

    #[test]
    fn cat_naps_on_a_screen() {
        let mut g = Game::new(DEFAULT_SEED);
        g.player.god = true;
        let tx = g.world.w / 2 + 6;
        let ty = g.world.surface[tx as usize] - 10;
        let id = g.screens.spawn(tx, ty, 12, 8);
        tick(&mut g, 2.0);
        // Put the cat on the ground just past the screen, out of the player's way.
        g.mobs.critters.list.clear();
        let (fx, _, fw, _) = g.screens.list[0].frame_px();
        let mut placed = false;
        let mut x = fx + fw + 24.0;
        let mut ctxless = std::mem::take(&mut g.mobs.critters);
        {
            let ctx = MobCtx {
                world: &g.world,
                screens: &mut g.screens,
                player: &mut g.player,
                fx: &mut g.particles,
                cam: &mut g.cam,
                sky: &g.sky,
                events: &mut g.events,
            };
            for _ in 0..20 {
                if ctxless.spawn_at(Kind::Cat, x, &ctx) {
                    placed = true;
                    break;
                }
                x += 16.0;
            }
        }
        assert!(placed, "no room for the cat");
        ctxless.list[0].act = Act::ToScreen { id };
        ctxless.seeded = true;
        g.mobs.critters = ctxless;
        // Keep the player well away so the cat isn't spooked.
        g.player.x = fx - 200.0;
        tick(&mut g, 10.0);
        let cat = g.mobs.critters.list.iter().find(|c| c.kind == Kind::Cat).expect("cat still around");
        assert!(matches!(cat.act, Act::Nap { id: i, .. } if i == id), "cat should be napping on the screen, is {:?}", cat.act);
    }
}
