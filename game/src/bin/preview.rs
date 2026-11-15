//! Standalone preview: the game without the compositor.
//!
//!   cargo run -p game --bin preview --release              # interactive window
//!   cargo run -p game --bin preview --release -- --scale 3 # 1920x1080
//!   cargo run -p game --bin preview --release -- --shots out/   # headless scripted demo -> PNGs
//!
//! Fake "app windows" stand in for the compositor: a screen gets a fake
//! terminal 0.8 s after it's built, and destroyed screens print what the
//! compositor would close.

use game::{Game, DEFAULT_SEED};
use shared::{GameApi, GameEvent, InputState, Key, ScreenId, ScreenInfo, LAYER_BYTES, VIEW_H, VIEW_W};
use std::collections::HashMap;
use std::time::{Duration, Instant};

const DT: f32 = 1.0 / 60.0;

/// Simulates the compositor's side: which screens have a (fake) window.
#[derive(Default)]
struct FakeCompositor {
    pending: HashMap<ScreenId, f32>, // id -> time until the window "maps"
    windows: HashMap<ScreenId, usize>, // id -> which fake app
    next_app: usize,
}

impl FakeCompositor {
    fn handle(&mut self, g: &mut Game, events: Vec<GameEvent>, dt: f32) {
        for e in events {
            match e {
                GameEvent::ScreenBuilt { id } => {
                    self.pending.insert(id, 0.8);
                }
                GameEvent::ScreenDestroyed { id } => {
                    self.pending.remove(&id);
                    if let Some(app) = self.windows.remove(&id) {
                        println!("[compositor] screen {id} destroyed -> would close app {}", APPS[app % APPS.len()].0);
                    }
                }
                GameEvent::ScreenDamaged { .. } => {}
            }
        }
        let mut ready = Vec::new();
        for (id, t) in self.pending.iter_mut() {
            *t -= dt;
            if *t <= 0.0 {
                ready.push(*id);
            }
        }
        for id in ready {
            self.pending.remove(&id);
            self.windows.insert(id, self.next_app);
            self.next_app += 1;
            g.set_screen_occupied(id, true);
        }
    }
}

const APPS: &[(&str, &[&str])] = &[
    ("foot", &["user@siege:~$ ls", "Desktop  Documents  world.sav", "user@siege:~$ cargo run", "   Compiling siege v0.1.0", "    Finished release", "user@siege:~$ _"]),
    ("htop", &["CPU [|||||||      38%]", "MEM [||||||||||   61%]", "  PID USER   CPU% CMD", " 1337 user   12.0 compositor", " 2048 user    4.1 foot", " 4096 user    0.7 mobs"]),
    ("cmatrix", &["1 0 1 1 0 0 1 0 1 1", "0 1 0 0 1 1 0 1 0 0", "1 1 0 1 0 1 1 0 0 1", "0 0 1 0 1 0 0 1 1 0", "1 0 0 1 1 0 1 0 1 1", "0 1 1 0 0 1 0 1 0 0"]),
];

/// Draw a fake app window into the composited RGBA buffer (view resolution).
fn draw_fake_window(buf: &mut [u8], s: &ScreenInfo, app: usize, t: f32) {
    let mut c = game::gfx::Canvas::new(buf);
    let r = s.rect;
    let (name, lines) = APPS[app % APPS.len()];
    let bg = if name == "cmatrix" { [0, 12, 0, 255] } else { [24, 26, 36, 255] };
    c.fill_rect(r.x, r.y, r.w, r.h, bg);
    // Title bar.
    c.fill_rect(r.x, r.y, r.w, 9, [48, 52, 68, 255]);
    c.text(r.x + 3, r.y + 2, name, 1, [220, 220, 230, 255]);
    c.fill_rect(r.x + r.w - 8, r.y + 2, 5, 5, [230, 90, 90, 255]);
    let fg = match name {
        "cmatrix" => [80, 255, 120, 255],
        "htop" => [120, 220, 255, 255],
        _ => [200, 230, 200, 255],
    };
    let scroll = if name == "cmatrix" { (t * 4.0) as usize } else { 0 };
    for (i, _) in lines.iter().enumerate() {
        let l = lines[(i + scroll) % lines.len()];
        let y = r.y + 13 + i as i32 * 8;
        if y + 6 < r.y + r.h {
            c.text(r.x + 4, y, l, 1, fg);
        }
    }
    if s.glitch {
        // What the compositor's glitch effect will roughly look like.
        let off = ((t * 50.0).sin() * 4.0) as i32;
        c.fill_rect(r.x + off, r.y + r.h / 3, r.w, 4, [255, 40, 200, 140]);
    }
}

/// Composite world + windows + fx into one RGBA image (view resolution).
fn composite(g: &mut Game, fake: &FakeCompositor, world: &mut [u8], fx: &mut [u8], out: &mut [u8], t: f32) {
    g.draw_world(world);
    g.draw_fx(fx);
    out.copy_from_slice(world);
    for s in g.screens() {
        if let Some(&app) = fake.windows.get(&s.id) {
            draw_fake_window(out, &s, app, t);
        }
    }
    let mut c = game::gfx::Canvas::new(out);
    for (i, p) in fx.chunks_exact(4).enumerate() {
        if p[3] > 0 {
            let x = (i % VIEW_W as usize) as i32;
            let y = (i / VIEW_W as usize) as i32;
            c.blend(x, y, [p[0], p[1], p[2], p[3]]);
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let arg = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
    let seed = arg("--seed").and_then(|s| s.parse().ok()).unwrap_or(DEFAULT_SEED);
    if let Some(dir) = arg("--shots") {
        headless(seed, &dir);
        return;
    }
    let scale = arg("--scale").and_then(|s| s.parse().ok()).unwrap_or(2usize);
    interactive(seed, scale);
}

fn interactive(seed: u64, scale: usize) {
    use minifb::{Key as K, MouseButton, MouseMode, Window, WindowOptions};
    let (w, h) = (VIEW_W as usize * scale, VIEW_H as usize * scale);
    let mut window = match Window::new("Desktop Siege - preview", w, h, WindowOptions::default()) {
        Ok(win) => win,
        Err(e) => {
            eprintln!("could not open window: {e}");
            return;
        }
    };
    window.set_target_fps(0); // we pace ourselves

    let mut g = Game::new(seed);
    let mut fake = FakeCompositor::default();
    let (mut world, mut fx, mut out) = (vec![0u8; LAYER_BYTES], vec![0u8; LAYER_BYTES], vec![0u8; LAYER_BYTES]);
    let mut frame = vec![0u32; w * h];
    let mut input = InputState::default();
    input.game_focused = true;
    let mut last = Instant::now();
    let mut acc = 0.0f32;
    let mut t = 0.0f32;
    let mut fps_timer = Instant::now();
    let mut frames = 0;

    let map = [
        (Key::Left, &[K::A, K::Left][..]),
        (Key::Right, &[K::D, K::Right][..]),
        (Key::Up, &[K::W, K::Up][..]),
        (Key::Down, &[K::S, K::Down][..]),
        (Key::Jump, &[K::Space][..]),
        (Key::Build, &[K::B][..]),
        (Key::Night, &[K::N][..]),
        (Key::God, &[K::G][..]),
        (Key::ToggleHud, &[K::F1][..]),
        (Key::Torch, &[K::T][..]),
        (Key::Hotbar1, &[K::Key1][..]),
        (Key::Hotbar2, &[K::Key2][..]),
        (Key::Hotbar3, &[K::Key3][..]),
        (Key::Hotbar4, &[K::Key4][..]),
        (Key::Hotbar5, &[K::Key5][..]),
    ];

    while window.is_open() && !window.is_key_down(K::Escape) {
        for (k, keys) in map.iter() {
            input.set(*k, keys.iter().any(|kk| window.is_key_down(*kk)));
        }
        if let Some((mx, my)) = window.get_mouse_pos(MouseMode::Pass) {
            input.mouse_x = mx / scale as f32;
            input.mouse_y = my / scale as f32;
        }
        input.lmb = window.get_mouse_down(MouseButton::Left);
        input.rmb = window.get_mouse_down(MouseButton::Right);

        let now = Instant::now();
        acc += (now - last).as_secs_f32().min(0.25);
        last = now;
        while acc >= DT {
            g.update(DT, &input);
            let ev = g.drain_events();
            fake.handle(&mut g, ev, DT);
            acc -= DT;
            t += DT;
        }

        composite(&mut g, &fake, &mut world, &mut fx, &mut out, t);
        // Nearest-neighbour upscale into the window buffer.
        for y in 0..h {
            let sy = y / scale;
            let row = &out[sy * VIEW_W as usize * 4..(sy + 1) * VIEW_W as usize * 4];
            let dst = &mut frame[y * w..(y + 1) * w];
            for (x, d) in dst.iter_mut().enumerate() {
                let p = &row[(x / scale) * 4..(x / scale) * 4 + 4];
                *d = (p[0] as u32) << 16 | (p[1] as u32) << 8 | p[2] as u32;
            }
        }
        if window.update_with_buffer(&frame, w, h).is_err() {
            break;
        }

        frames += 1;
        if fps_timer.elapsed() >= Duration::from_secs(1) {
            g.fps = frames as f32 / fps_timer.elapsed().as_secs_f32();
            window.set_title(&format!("Desktop Siege - preview - {:.0} fps", g.fps));
            frames = 0;
            fps_timer = Instant::now();
        }
        // Sleep off the rest of the frame.
        let spent = now.elapsed();
        if spent < Duration::from_micros(16_000) {
            std::thread::sleep(Duration::from_micros(16_000) - spent);
        }
    }
}

/// Scripted demo, no window: writes PNGs of key moments into `dir`.
/// Used by tests/agents to check visuals. Also prints timing.
fn headless(seed: u64, dir: &str) {
    if let Err(e) = std::fs::create_dir_all(dir) {
        eprintln!("cannot create {dir}: {e}");
        return;
    }
    let mut g = Game::new(seed);
    let mut fake = FakeCompositor::default();
    let (mut world, mut fx, mut out) = (vec![0u8; LAYER_BYTES], vec![0u8; LAYER_BYTES], vec![0u8; LAYER_BYTES]);
    let mut input = InputState::default();
    input.game_focused = true;
    let mut t = 0.0f32;
    let mut draw_time = Duration::ZERO;
    let mut draws = 0u32;
    let mut screen_id: Option<ScreenId> = None;

    // Where to build: a 22x10 frame up and to the right of spawn, low enough
    // that the whole frame (and the HP bar above it) stays in view.
    let tx = g.world.w / 2 - 2;
    let ty = g.world.surface[tx as usize] - 12;
    let shots: &[(f32, &str)] = &[
        (0.5, "01_spawn"),
        (2.0, "02_walk"),
        (2.75, "03_mining"),
        (5.2, "04_build_drag"),
        (5.9, "05_building"),
        (7.0, "06_booting"),
        (8.5, "07_online"),
        (12.5, "08_dusk"),
        (16.0, "09_night"),
        (17.5, "10_cracked"),
        (19.4, "11_dying"),
        (19.95, "12_explosion"),
        (22.0, "13_after"),
    ];
    let mut next_shot = 0;
    let end = 23.0;

    while t < end {
        // --- script ---
        input.set(Key::Right, (1.0..2.2).contains(&t));
        input.set(Key::Jump, (1.6..1.8).contains(&t));
        let (pcx, pcy) = g.player.center();
        // Mine the tile just below-right of the player.
        if (2.6..3.9).contains(&t) {
            let (wx, wy) = (pcx + 20.0, pcy + 18.0);
            input.mouse_x = wx - g.cam.x;
            input.mouse_y = wy - g.cam.y;
            input.lmb = true;
        } else {
            input.lmb = false;
        }
        // Build: press B, drag from (tx,ty) to (tx+21, ty+9).
        input.set(Key::Build, (4.2..4.3).contains(&t));
        let tile_to_mouse = |g: &Game, x: i32, y: i32| ((x * 16 + 8) as f32 - g.cam.x, (y * 16 + 8) as f32 - g.cam.y);
        if (4.5..5.4).contains(&t) {
            let f = ((t - 4.5) / 0.7).min(1.0);
            let (ax, ay) = tile_to_mouse(&g, tx, ty);
            let (bx, by) = tile_to_mouse(&g, tx + 21, ty + 9);
            input.mouse_x = ax + (bx - ax) * f;
            input.mouse_y = ay + (by - ay) * f;
            input.lmb = true;
        } else if (5.4..5.5).contains(&t) {
            input.lmb = false;
        }
        input.set(Key::Night, (9.0..9.1).contains(&t));
        // Mobs are stubbed: simulate attacks from the left side at night.
        if let Some(id) = screen_id {
            if t > 15.0 && ((t * 60.0) as i32) % 14 == 0 {
                let r = g.screens.list.iter().find(|s| s.id == id).map(|s| s.frame_px());
                if let Some((x, y, _, h)) = r {
                    let hy = y + h * (0.3 + 0.5 * ((t * 3.7).sin() * 0.5 + 0.5));
                    g.screens.damage(id, 0.06, (x + 8.0, hy), &mut g.particles, &mut g.events);
                }
            }
        }

        g.update(DT, &input);
        let ev = g.drain_events();
        for e in &ev {
            if let GameEvent::ScreenBuilt { id } = e {
                screen_id = Some(*id);
            }
            println!("[{t:5.2}s] {e:?}");
        }
        fake.handle(&mut g, ev, DT);
        t += DT;

        if next_shot < shots.len() && t >= shots[next_shot].0 {
            let start = Instant::now();
            composite(&mut g, &fake, &mut world, &mut fx, &mut out, t);
            draw_time += start.elapsed();
            draws += 1;
            let path = format!("{dir}/{}.png", shots[next_shot].1);
            match write_png(&path, &out, 2) {
                Ok(()) => println!("wrote {path}"),
                Err(e) => eprintln!("failed {path}: {e}"),
            }
            next_shot += 1;
        }
    }

    // Measure steady-state frame cost (draw both layers + composite) for the perf budget.
    let start = Instant::now();
    for _ in 0..60 {
        g.update(DT, &input);
        composite(&mut g, &fake, &mut world, &mut fx, &mut out, t);
    }
    let per = start.elapsed() / 60;
    println!(
        "avg frame (update+draw+composite): {:.2} ms; screenshot draws avg {:.2} ms",
        per.as_secs_f64() * 1000.0,
        draw_time.as_secs_f64() * 1000.0 / draws.max(1) as f64
    );
}

fn write_png(path: &str, rgba: &[u8], scale: usize) -> Result<(), Box<dyn std::error::Error>> {
    let (w, h) = (VIEW_W as usize, VIEW_H as usize);
    let mut big = vec![0u8; w * h * 4 * scale * scale];
    for y in 0..h * scale {
        for x in 0..w * scale {
            let s = ((y / scale) * w + x / scale) * 4;
            let d = (y * w * scale + x) * 4;
            big[d..d + 4].copy_from_slice(&rgba[s..s + 4]);
        }
    }
    let file = std::fs::File::create(path)?;
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), (w * scale) as u32, (h * scale) as u32);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut writer = enc.write_header()?;
    writer.write_image_data(&big)?;
    Ok(())
}
