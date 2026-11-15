//! Bit bats: night flyers that dive-bomb screens. Owner: person B2.
//!
//! A bat flaps in from above the view, hovers over the nearest online screen,
//! then dives at its frame (a bigger hit than a zombie claw) and bounces back
//! up to hover again. Bats reach screens floating too high for zombies. With no
//! screen to attack (or during the siege cooldown) they dive at the player.
//! One sword hit kills a bat; at dawn they flee upwards.
//!
//! Art: `assets/sprites/mob_bat.png` (3 flap frames, 18x12, symmetric).

use super::{overlaps, rect_dist, MobCtx, Rng, BURN_LIGHT, SPAWN_LIGHT};
use crate::camera::Camera;
use crate::gfx::{rgb, rgba, with_alpha, Canvas, Rgba, Sprite};
use crate::light::Light;
use shared::{GameEvent, ScreenId};

// ---------------- tuning ----------------

/// Sprite frame size (view px).
const FW: i32 = 18;
const FH: i32 = 12;
/// Hitbox (world px), centred on the bat.
const BW: f32 = 12.0;
const BH: f32 = 8.0;

/// Screen HP (0..1) a dive takes off (a zombie claw is 0.05).
const DIVE_DAMAGE: f32 = 0.06;
/// Player damage when a dive connects.
const PLAYER_DAMAGE: i32 = 1;
const HP: i32 = 1;

/// Cruising and diving speeds (px/s).
const CRUISE: f32 = 95.0;
const DIVE_SPEED: f32 = 290.0;
/// How high above its target a bat hovers (px).
const HOVER_ABOVE: f32 = 44.0;
/// A dive that hasn't connected after this long gives up.
const DIVE_MAX: f32 = 1.2;

/// Seconds after dark before the first bat, then between bats (before jitter).
const FIRST_BAT: f32 = 5.0;
const SPAWN_EVERY: f32 = 7.0;

/// Max bats at once on night `n`: 2, then +1 every 2 nights, up to 4.
fn max_alive(nights: u32) -> usize {
    (2 + nights.saturating_sub(1) as usize / 2).min(4)
}

const BODY: Rgba = rgb(138, 48, 180);
const GLITCH: Rgba = rgb(255, 46, 200);

// ---------------- state ----------------

#[derive(Clone, Copy, Debug, PartialEq)]
enum State {
    /// Flying in from above the view.
    Arrive,
    /// Bobbing above the target; dives when `t` runs out.
    Hover { t: f32 },
    /// Diving at a point; `t` counts up.
    Dive { tx: f32, ty: f32, t: f32 },
    /// Bouncing back up after a hit (or a sword).
    Recoil { t: f32 },
    /// Dawn: flying up and away.
    Flee,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Target {
    Screen(ScreenId),
    Player,
}

#[derive(Clone, Debug)]
pub struct Bat {
    /// Centre, world px.
    pub x: f32,
    pub y: f32,
    vx: f32,
    vy: f32,
    state: State,
    target: Target,
    /// Where along the target screen's top edge to hover (0..1).
    side: f32,
    flap: f32,
    /// Hit flash; also stops one swing from hitting twice.
    hurt: f32,
    pub hp: i32,
    /// Flew off the top of the view at dawn: remove without a death pop.
    gone: bool,
    seed: f32,
}

impl Bat {
    fn hitbox(&self) -> (f32, f32, f32, f32) {
        (self.x - BW / 2.0, self.y - BH / 2.0, BW, BH)
    }
}

pub struct Bats {
    pub list: Vec<Bat>,
    rng: Rng,
    timer: f32,
    was_dark: bool,
    frames: Vec<Sprite>,
}

impl Default for Bats {
    fn default() -> Self {
        Self::new()
    }
}

impl Bats {
    pub fn new() -> Self {
        Bats { list: Vec::new(), rng: Rng(0xBA75_0001), timer: FIRST_BAT, was_dark: false, frames: bat_frames() }
    }

    /// Add a bat at (x, y) (world px, centre), flying in.
    pub fn spawn(&mut self, x: f32, y: f32) {
        let r = &mut self.rng;
        self.list.push(Bat {
            x,
            y,
            vx: 0.0,
            vy: 40.0,
            state: State::Arrive,
            target: Target::Player,
            side: 0.2 + 0.6 * r.next(),
            flap: r.next() * 3.0,
            hurt: 0.0,
            hp: HP,
            gone: false,
            seed: r.next() * 6.28,
        });
    }

    /// Centres of all bats (birds scatter from them).
    pub fn centers(&self) -> impl Iterator<Item = (f32, f32)> + '_ {
        self.list.iter().map(|b| (b.x, b.y))
    }

    /// `calm`: the shared siege cooldown. While > 0 bats leave screens alone;
    /// a dive that destroys a screen starts it. `safe(x, y)`: false where a
    /// torch keeps mobs from spawning.
    pub fn update(&mut self, dt: f32, ctx: &mut MobCtx, calm: &mut f32, safe: &dyn Fn(f32, f32) -> bool) {
        let daylight = ctx.sky.daylight();
        let dark = ctx.sky.is_night() && daylight < SPAWN_LIGHT;
        if dark && !self.was_dark {
            self.timer = FIRST_BAT;
        }
        self.was_dark = dark;
        if dark {
            self.timer -= dt;
            if self.timer <= 0.0 {
                self.timer = SPAWN_EVERY * (0.7 + 0.6 * self.rng.next());
                if self.list.len() < max_alive(ctx.sky.nights.max(1)) {
                    // Just above the top edge of the view, somewhere across it.
                    for _ in 0..6 {
                        let x = ctx.cam.x + 24.0 + (ctx.cam.vw - 48.0) * self.rng.next();
                        let y = ctx.cam.y - 20.0;
                        if safe(x, y) {
                            self.spawn(x, y);
                            break;
                        }
                    }
                }
            }
        }

        let dawn = daylight > BURN_LIGHT;
        let targets = ctx.screens.targets();
        let Bats { list, rng, .. } = self;
        for b in list.iter_mut() {
            if dawn && b.state != State::Flee {
                b.state = State::Flee;
            }
            let t: &[(ScreenId, (f32, f32, f32, f32))] = if *calm > 0.0 { &[] } else { targets.as_slice() };
            let seen = ctx.events.len();
            update_bat(b, dt, ctx, t, rng);
            if ctx.events[seen..].iter().any(|e| matches!(e, GameEvent::ScreenDamaged { hp, .. } if *hp <= 0.0)) {
                *calm = super::SIEGE_COOLDOWN;
            }
        }
        for b in list.iter().filter(|b| b.hp <= 0 && !b.gone) {
            death_pop(b, ctx);
        }
        list.retain(|b| b.hp > 0 && !b.gone);
    }

    pub fn draw(&self, c: &mut Canvas, cam: &Camera, light: &Light, eye_glow: f32) {
        for b in &self.list {
            let f = match b.state {
                State::Dive { .. } => 0, // wings tucked up
                _ => [0, 1, 2, 1][(b.flap * 12.0) as usize % 4],
            };
            let Some(spr) = self.frames.get(f) else { continue };
            let (vx, vy) = cam.to_view(b.x - FW as f32 / 2.0, b.y - FH as f32 / 2.0);
            if vx < -FW - 8 || vx > c.w + 8 || vy < -FH - 8 || vy > c.h + 8 {
                continue;
            }
            let k = light.level_at(b.x, b.y).clamp(0.55, 1.0);
            c.sprite_shaded(spr, vx, vy, false, k);
            if b.hurt > 0.12 {
                c.sprite_flat(spr, vx, vy, false, rgba(255, 255, 255, 210));
            }
            if eye_glow > 0.0 {
                c.glow(vx + FW / 2, vy + 5, 6, with_alpha(GLITCH, (55.0 * eye_glow) as u8));
            }
        }
    }
}

// ---------------- behaviour ----------------

fn update_bat(b: &mut Bat, dt: f32, ctx: &mut MobCtx, targets: &[(ScreenId, (f32, f32, f32, f32))], rng: &mut Rng) {
    b.hurt = (b.hurt - dt).max(0.0);
    b.flap += dt;
    let (pcx, pcy) = ctx.player.center();

    // Pick a target (keep the current screen while it's still standing).
    if !matches!(b.state, State::Dive { .. } | State::Flee) {
        let keep = matches!(b.target, Target::Screen(id) if targets.iter().any(|(i, _)| *i == id));
        if !keep {
            b.target = targets
                .iter()
                .min_by(|a, c| rect_dist(&a.1, b.x, b.y).total_cmp(&rect_dist(&c.1, b.x, b.y)))
                .map(|(id, _)| Target::Screen(*id))
                .unwrap_or(Target::Player);
        }
    }
    let rect = match b.target {
        Target::Screen(id) => targets.iter().find(|(i, _)| *i == id).map(|(_, r)| *r),
        Target::Player => None,
    };
    let bob = (b.flap * 2.3 + b.seed).sin() * 6.0;
    let (gx, gy) = match rect {
        Some((fx, fy, fw, _)) => (fx + fw * b.side, fy - HOVER_ABOVE + bob),
        None => (pcx + if b.x < pcx { -34.0 } else { 34.0 }, pcy - HOVER_ABOVE + bob),
    };

    match b.state {
        State::Arrive => {
            steer(b, gx, gy, CRUISE, dt);
            if (gx - b.x).hypot(gy - b.y) < 22.0 {
                b.state = State::Hover { t: 1.5 + 1.5 * rng.next() };
            }
        }
        State::Hover { t } => {
            steer(b, gx, gy, CRUISE, dt);
            let nt = t - dt;
            b.state = if nt > 0.0 {
                State::Hover { t: nt }
            } else {
                let (tx, ty) = match rect {
                    Some((fx, fy, fw, _)) => (b.x.clamp(fx + 6.0, fx + fw - 6.0), fy + 8.0),
                    None => (pcx, pcy),
                };
                State::Dive { tx, ty, t: 0.0 }
            };
        }
        State::Dive { tx, ty, t } => {
            let (dx, dy) = (tx - b.x, ty - b.y);
            let d = dx.hypot(dy).max(0.001);
            let k = (10.0 * dt).min(1.0);
            b.vx += (dx / d * DIVE_SPEED - b.vx) * k;
            b.vy += (dy / d * DIVE_SPEED - b.vy) * k;
            let nt = t + dt;
            b.state = State::Dive { tx, ty, t: nt };
            let hit = match (b.target, rect) {
                (Target::Screen(id), Some((fx, fy, fw, fh))) => {
                    if b.x >= fx && b.x <= fx + fw && b.y >= fy && b.y <= fy + fh {
                        ctx.screens.damage(id, DIVE_DAMAGE, (b.x, b.y), ctx.fx, ctx.events);
                        ctx.cam.kick(0.06);
                        true
                    } else {
                        false
                    }
                }
                (Target::Player, _) => {
                    if overlaps(b.hitbox(), (ctx.player.x, ctx.player.y, crate::player::W, crate::player::H)) {
                        ctx.player.hurt(PLAYER_DAMAGE, b.x, ctx.fx);
                        true
                    } else {
                        false
                    }
                }
                // The screen went down mid-dive.
                (Target::Screen(_), None) => true,
            };
            if hit || nt > DIVE_MAX || d < 3.0 {
                b.vy = -170.0;
                b.vx = (if b.x < tx { -1.0 } else { 1.0 }) * (60.0 + 60.0 * rng.next());
                b.state = State::Recoil { t: 0.5 };
            }
        }
        State::Recoil { t } => {
            let damp = (1.0 - 2.5 * dt).max(0.0);
            b.vx *= damp;
            b.vy *= damp;
            let nt = t - dt;
            b.state = if nt > 0.0 { State::Recoil { t: nt } } else { State::Hover { t: 1.2 + 1.3 * rng.next() } };
        }
        State::Flee => {
            b.vy = (b.vy - 320.0 * dt).max(-170.0);
            b.vx *= (1.0 - dt).max(0.0);
            if b.y < ctx.cam.y - 40.0 {
                b.gone = true;
            }
        }
    }

    // A little flutter, then move.
    let flutter = (b.flap * 9.0 + b.seed).sin() * 18.0;
    b.x += b.vx * dt;
    b.y += (b.vy + flutter) * dt;
    b.x = b.x.clamp(8.0, ctx.world.width_px() - 8.0);
    // Never inside terrain: pop up out of it.
    let mut n = 0;
    while ctx.world.solid_px(b.x, b.y + BH / 2.0) && n < 32 {
        b.y -= 1.0;
        b.vy = b.vy.min(0.0);
        n += 1;
    }

    // Sword.
    if let Some(sword) = ctx.player.attack_box() {
        if b.hurt <= 0.0 && overlaps(sword, b.hitbox()) {
            b.hp -= 1;
            b.hurt = 0.25;
            let away = if b.x >= pcx { 1.0 } else { -1.0 };
            b.vx = away * 200.0;
            b.vy = -120.0;
            if !matches!(b.state, State::Flee) {
                b.state = State::Recoil { t: 0.4 };
            }
            ctx.fx.debris(b.x, b.y, BODY, 5);
            ctx.cam.kick(if b.hp <= 0 { 0.14 } else { 0.06 });
        }
    }
}

/// Ease velocity towards flying at `speed` to (gx, gy), slowing near it.
fn steer(b: &mut Bat, gx: f32, gy: f32, speed: f32, dt: f32) {
    let (dx, dy) = (gx - b.x, gy - b.y);
    let d = dx.hypot(dy).max(0.001);
    let s = speed * (d / 40.0).min(1.0);
    let k = 1.0 - (-4.0 * dt).exp();
    b.vx += (dx / d * s - b.vx) * k;
    b.vy += (dy / d * s - b.vy) * k;
}

fn death_pop(b: &Bat, ctx: &mut MobCtx) {
    ctx.fx.debris(b.x, b.y, BODY, 10);
    ctx.fx.sparkle(b.x, b.y, GLITCH, 8);
    ctx.fx.sparkle(b.x, b.y, rgb(78, 240, 255), 4);
}

/// Slice the 3-frame strip. Blank frames if the PNG won't decode.
fn bat_frames() -> Vec<Sprite> {
    let bytes = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../assets/sprites/mob_bat.png"));
    let Ok(strip) = crate::sprites::decode_png(bytes) else {
        return vec![Sprite::new(FW, FH); 3];
    };
    (0..3)
        .map(|i| {
            let mut s = Sprite::new(FW, FH);
            for y in 0..FH {
                for x in 0..FW {
                    s.set(x, y, strip.get(i * FW + x, y));
                }
            }
            s
        })
        .collect()
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
        for _ in 0..(seconds * 60.0).round() as usize {
            g.update(1.0 / 60.0, &input);
        }
    }

    #[test]
    fn frames_decode() {
        let f = bat_frames();
        assert_eq!(f.len(), 3);
        assert!(f.iter().all(|s| s.px.iter().any(|p| p[3] > 0)), "bat frames should not be blank");
    }

    #[test]
    fn no_bats_by_day() {
        let mut g = Game::new(DEFAULT_SEED);
        tick(&mut g, 20.0);
        assert!(g.mobs.bats.list.is_empty());
    }

    #[test]
    fn bats_come_at_night_and_leave_by_morning() {
        let mut g = Game::new(DEFAULT_SEED);
        g.player.god = true;
        g.sky.clock = DAY_LEN + 0.1;
        tick(&mut g, 14.0);
        assert!(!g.mobs.bats.list.is_empty(), "bats should arrive at night");
        assert!(g.mobs.bats.list.len() <= max_alive(1));
        tick(&mut g, NIGHT_LEN - 14.0 + 8.0);
        assert!(!g.sky.is_night());
        assert!(g.mobs.bats.list.is_empty(), "bats should be gone by morning");
    }

    #[test]
    fn bat_dives_a_screen_too_high_for_zombies() {
        let mut g = Game::new(DEFAULT_SEED);
        g.player.god = true;
        let tx = g.world.w / 2 + 2;
        let ty = g.world.surface[tx as usize] - 26; // ~19 tiles above the ground
        let id = g.screens.spawn(tx, ty, 10, 6);
        tick(&mut g, 2.0);
        g.drain_events();
        g.sky.clock = DAY_LEN + 0.1;
        let (fx, fy, fw, _) = g.screens.list[0].frame_px();
        g.mobs.bats.spawn(fx + fw / 2.0, fy - 80.0);
        tick(&mut g, 8.0);
        let hits = g.drain_events().iter().filter(|e| matches!(e, GameEvent::ScreenDamaged { id: i, .. } if *i == id)).count();
        assert!(hits > 0, "the bat never dived the high screen");
    }

    #[test]
    fn one_sword_hit_kills_a_bat() {
        let mut g = Game::new(DEFAULT_SEED);
        tick(&mut g, 2.0); // land
        g.sky.clock = DAY_LEN + 5.0;
        g.mobs.bats.was_dark = true;
        g.mobs.bats.timer = 1000.0;
        g.player.god = true;
        g.player.facing = 1.0;
        let (pcx, pcy) = g.player.center();
        g.mobs.bats.spawn(pcx + 14.0, pcy - 10.0);
        g.player.swing = 0.28;
        tick(&mut g, 0.3);
        assert!(g.mobs.bats.list.is_empty(), "one swing should kill the bat");
    }

    #[test]
    fn bats_leave_screens_alone_during_the_cooldown() {
        let mut g = Game::new(DEFAULT_SEED);
        g.player.god = true;
        let tx = g.world.w / 2 + 2;
        let ty = g.world.surface[tx as usize] - 26;
        g.screens.spawn(tx, ty, 10, 6);
        tick(&mut g, 2.0);
        g.drain_events();
        g.sky.clock = DAY_LEN + 0.1;
        g.mobs.calm = 30.0;
        let (fx, fy, fw, _) = g.screens.list[0].frame_px();
        g.mobs.bats.spawn(fx + fw / 2.0, fy - 80.0);
        tick(&mut g, 8.0);
        assert!(!g.drain_events().iter().any(|e| matches!(e, GameEvent::ScreenDamaged { .. })));
    }
}
