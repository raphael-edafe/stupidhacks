//! Standalone preview: the game without the compositor.
//!
//!   cargo run -p game --bin preview --release              # interactive window
//!   cargo run -p game --bin preview --release -- --scale 3 # 1920x1080
//!   cargo run -p game --bin preview --release -- --shots out/   # headless scripted demo -> PNGs
//!
//! A fake compositor stands in for the real one: `LaunchApp` maps a fake
//! window for `APPS[app]` 0.8 s later, `CloseApp`/`ScreenDestroyed` remove it,
//! `FocusScreen` takes keyboard focus away from the game (Esc gives it back
//! and sends Key::Escape for one tick). Quit with Ctrl+Q or by closing the window.

use game::{Game, DEFAULT_SEED};
use shared::{GameApi, GameEvent, InputState, Key, ScreenId, ScreenInfo, APPS, LAYER_BYTES, VIEW_H, VIEW_W};
use std::collections::HashMap;
use std::time::{Duration, Instant};

const DT: f32 = 1.0 / 60.0;
const MAP_DELAY: f32 = 0.8;

/// Simulates the compositor's side: which screens have a (fake) window, and who has focus.
#[derive(Default)]
struct FakeCompositor {
    pending: HashMap<ScreenId, (usize, f32)>, // id -> (app, time until the window "maps")
    windows: HashMap<ScreenId, usize>,        // id -> app index
    /// Screen whose window has keyboard focus (None = the game has it).
    focus: Option<ScreenId>,
}

impl FakeCompositor {
    fn handle(&mut self, g: &mut Game, events: Vec<GameEvent>, dt: f32) {
        for e in events {
            match e {
                GameEvent::LaunchApp { id, app } => {
                    let name = APPS.get(app).map(|a| a.name).unwrap_or("?");
                    println!("[compositor] launch {name} into screen {id}");
                    self.pending.insert(id, (app, MAP_DELAY));
                }
                GameEvent::CloseApp { id } | GameEvent::ScreenDestroyed { id } => {
                    self.pending.remove(&id);
                    if let Some(app) = self.windows.remove(&id) {
                        let name = APPS.get(app).map(|a| a.name).unwrap_or("?");
                        println!("[compositor] screen {id}: close {name}");
                    }
                    if self.focus == Some(id) {
                        self.focus = None;
                    }
                    g.set_screen_occupied(id, false);
                }
                GameEvent::FocusScreen { id } => {
                    if self.windows.contains_key(&id) {
                        self.focus = Some(id);
                    }
                }
                _ => {}
            }
        }
        let mut ready = Vec::new();
        for (id, (app, t)) in self.pending.iter_mut() {
            *t -= dt;
            if *t <= 0.0 {
                ready.push((*id, *app));
            }
        }
        for (id, app) in ready {
            self.pending.remove(&id);
            self.windows.insert(id, app);
            g.set_screen_occupied(id, true);
        }
    }

    /// Esc while an app has focus: the game gets focus back. True if it was taken.
    fn give_back_focus(&mut self) -> bool {
        self.focus.take().is_some()
    }
}

/// Fake window contents per APPS index.
fn fake_lines(app: usize, t: f32) -> Vec<String> {
    let cpu = 30 + ((t * 2.3).sin() * 12.0) as i32;
    let bar = |p: i32| format!("[{:<14}{:>3}%]", "|".repeat((p / 7).clamp(0, 14) as usize), p);
    match app {
        0 => ["user@siege:~$ ls", "Desktop  Documents  world.sav", "user@siege:~$ cargo run --release", "   Compiling siege v0.1.0", "    Finished release in 4.2s", "     Running target/release/siege", "user@siege:~$ _"]
            .iter()
            .map(|s| s.to_string())
            .collect(),
        1 => ["< > HOME", "", "[DIR] DESKTOP", "[DIR] DOCUMENTS", "[DIR] SCREENSHOTS", "[DIR] WORLD SAVES", "      NOTES.TXT", "      SIEGE.PNG"].iter().map(|s| s.to_string()).collect(),
        2 => ["NOTES.TXT", "", "TODO", "- BUILD MORE SCREENS", "- TORCHES BEFORE NIGHT", "- DO NOT LET THE BATS", "  NEAR THE TERMINAL", "_"].iter().map(|s| s.to_string()).collect(),
        4 => vec![
            format!("CPU {}", bar(cpu)),
            format!("MEM {}", bar(61)),
            format!("SWP {}", bar(4)),
            "  PID USER   CPU% CMD".to_string(),
            format!(" 1337 user  {:>5.1} compositor", 10.0 + (t * 3.0).sin() * 4.0),
            " 2048 user    4.1 foot".to_string(),
            " 4096 user    0.7 mobs".to_string(),
            " 8192 root    0.1 systemd".to_string(),
        ],
        _ => Vec::new(),
    }
}

/// Draw a fake app window into the composited RGBA buffer (view resolution), at `s.rect`.
fn draw_fake_window(buf: &mut [u8], vw: u32, vh: u32, s: &ScreenInfo, app: usize, t: f32) {
    let mut c = game::gfx::Canvas::new(buf, vw, vh);
    let r = s.rect;
    if r.w < 8 || r.h < 8 {
        return;
    }
    let name = APPS.get(app).map(|a| a.name).unwrap_or("App");
    // Zoomed windows get bigger text, like a real app after the resize configure.
    let sc = if r.w >= 400 { 2 } else { 1 };
    let bg = match app {
        5 => [0, 10, 0, 255],
        3 => [36, 38, 48, 255],
        _ => [24, 26, 36, 255],
    };
    c.fill_rect(r.x, r.y, r.w, r.h, bg);
    // Title bar.
    let tb = 4 + 5 * sc;
    c.fill_rect(r.x, r.y, r.w, tb, [48, 52, 68, 255]);
    c.text(r.x + 3, r.y + 2, name, sc, [220, 220, 230, 255]);
    c.fill_rect(r.x + r.w - 4 - 3 * sc, r.y + 2, 3 * sc, 3 * sc, [230, 90, 90, 255]);
    let line_h = 8 * sc;
    let max_chars = ((r.w - 8) / (4 * sc)).max(0) as usize;
    match app {
        3 => {
            // Calculator: display + button grid.
            let pad = 4 * sc;
            let dh = 12 * sc;
            c.fill_rect(r.x + pad, r.y + tb + pad, r.w - 2 * pad, dh, [200, 220, 200, 255]);
            let v = "1337";
            c.text(r.x + r.w - pad - 2 - game::gfx::text_width(v, sc), r.y + tb + pad + 3 * sc, v, sc, [30, 40, 30, 255]);
            let keys = ["7", "8", "9", "/", "4", "5", "6", "X", "1", "2", "3", "-", "0", ".", "=", "+"];
            let gy = r.y + tb + 2 * pad + dh;
            let (bw, bh) = ((r.w - 2 * pad) / 4, (r.y + r.h - pad - gy) / 4);
            if bw > 4 && bh > 4 {
                for (i, k) in keys.iter().enumerate() {
                    let (bx, by) = (r.x + pad + (i as i32 % 4) * bw, gy + (i as i32 / 4) * bh);
                    let col = if i % 4 == 3 { [230, 150, 60, 255] } else { [70, 74, 90, 255] };
                    c.fill_rect(bx + 1, by + 1, bw - 2, bh - 2, col);
                    c.text(bx + bw / 2 - sc, by + bh / 2 - 2 * sc, k, sc, [240, 240, 250, 255]);
                }
            }
        }
        5 => {
            // Matrix rain.
            let cols = ((r.w - 4) / (4 * sc)).max(0);
            let rows = ((r.h - tb - 2) / (6 * sc)).max(0);
            for cx in 0..cols {
                let speed = 4.0 + (cx * 7 % 5) as f32;
                let head = ((t * speed) as i32 + cx * 13) % (rows + 8);
                for ry in 0..rows {
                    let d = head - ry;
                    if (0..8).contains(&d) {
                        let g = 255 - d * 28;
                        let ch = if (cx * 31 + ry * 17 + (t * 6.0) as i32) % 2 == 0 { "1" } else { "0" };
                        let col = if d == 0 { [200, 255, 200, 255] } else { [40, g.clamp(60, 255) as u8, 60, 255] };
                        c.text(r.x + 2 + cx * 4 * sc, r.y + tb + 2 + ry * 6 * sc, ch, sc, col);
                    }
                }
            }
        }
        _ => {
            let fg = match app {
                4 => [120, 220, 255, 255],
                1 => [240, 210, 140, 255],
                2 => [220, 220, 240, 255],
                _ => [200, 230, 200, 255],
            };
            for (i, l) in fake_lines(app, t).iter().enumerate() {
                let y = r.y + tb + 4 + i as i32 * line_h;
                if y + 5 * sc < r.y + r.h {
                    let l: String = l.chars().take(max_chars).collect();
                    c.text(r.x + 4, y, &l, sc, fg);
                }
            }
        }
    }
    if s.glitch {
        // What the compositor's glitch effect will roughly look like.
        let off = ((t * 50.0).sin() * 4.0) as i32;
        c.fill_rect(r.x + off, r.y + r.h / 3, r.w, 4, [255, 40, 200, 140]);
    }
}

/// Composite world + windows + fx into one RGBA image (view resolution).
/// Buffers are resized to the game's current view size.
fn composite(g: &mut Game, fake: &FakeCompositor, world: &mut Vec<u8>, fx: &mut Vec<u8>, out: &mut Vec<u8>, t: f32) {
    let (vw, vh) = g.view_size();
    let n = (vw * vh * 4) as usize;
    for b in [&mut *world, &mut *fx, &mut *out] {
        b.resize(n, 0);
    }
    g.draw_world(world);
    g.draw_fx(fx);
    out.copy_from_slice(world);
    let mut infos = g.screens();
    // Zoomed window on top.
    infos.sort_by(|a, b| a.zoom.total_cmp(&b.zoom));
    for s in infos {
        if let Some(&app) = fake.windows.get(&s.id) {
            draw_fake_window(out, vw, vh, &s, app, t);
        }
    }
    let mut c = game::gfx::Canvas::new(out, vw, vh);
    for (i, p) in fx.chunks_exact(4).enumerate() {
        if p[3] > 0 {
            let x = (i % vw as usize) as i32;
            let y = (i / vw as usize) as i32;
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
    use minifb::{Key as K, KeyRepeat, MouseButton, MouseMode, Window, WindowOptions};
    let (w, h) = (VIEW_W as usize * scale, VIEW_H as usize * scale);
    let mut window = match Window::new("Desktop Siege - preview (Ctrl+Q quits)", w, h, WindowOptions::default()) {
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
    let mut last = Instant::now();
    let mut acc = 0.0f32;
    let mut t = 0.0f32;
    let mut fps_timer = Instant::now();
    let mut frames = 0;
    // Key::Escape is held for exactly one tick after focus comes back.
    let mut escape_tick = false;

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
        (Key::Interact, &[K::E][..]),
        (Key::CloseApp, &[K::X][..]),
        (Key::Zoom, &[K::Z][..]),
        (Key::Escape, &[K::Escape][..]),
        (Key::Hotbar1, &[K::Key1][..]),
        (Key::Hotbar2, &[K::Key2][..]),
        (Key::Hotbar3, &[K::Key3][..]),
        (Key::Hotbar4, &[K::Key4][..]),
        (Key::Hotbar5, &[K::Key5][..]),
        (Key::Hotbar6, &[K::Key6][..]),
    ];

    while window.is_open() {
        let ctrl = window.is_key_down(K::LeftCtrl) || window.is_key_down(K::RightCtrl);
        if ctrl && window.is_key_down(K::Q) {
            break;
        }
        if fake.focus.is_some() {
            // An app "has focus": keys would go to it. Esc hands focus back.
            if window.is_key_pressed(K::Escape, KeyRepeat::No) && fake.give_back_focus() {
                escape_tick = true;
            }
        }
        let app_focused = fake.focus.is_some();
        input.game_focused = !app_focused;
        for (k, keys) in map.iter() {
            input.set(*k, !app_focused && keys.iter().any(|kk| window.is_key_down(*kk)));
        }
        if escape_tick {
            input.set(Key::Escape, true);
        }
        let (vw, vh) = g.view_size();
        if let Some((mx, my)) = window.get_mouse_pos(MouseMode::Pass) {
            input.mouse_x = mx * vw as f32 / w as f32;
            input.mouse_y = my * vh as f32 / h as f32;
        }
        // Mouse wheel: notches this frame (consumed by one tick below).
        input.wheel = window.get_scroll_wheel().map(|(_, dy)| dy.signum()).unwrap_or(0.0);
        input.lmb = window.get_mouse_down(MouseButton::Left);
        input.rmb = window.get_mouse_down(MouseButton::Right);

        let now = Instant::now();
        acc += (now - last).as_secs_f32().min(0.25);
        last = now;
        while acc >= DT {
            g.update(DT, &input);
            input.wheel = 0.0;
            if escape_tick {
                escape_tick = false;
                input.set(Key::Escape, false);
            }
            let ev = g.drain_events();
            fake.handle(&mut g, ev, DT);
            acc -= DT;
            t += DT;
        }

        composite(&mut g, &fake, &mut world, &mut fx, &mut out, t);
        // Nearest-neighbour scale of the current view into the window buffer.
        let (vw, vh) = g.view_size();
        let (vw, vh) = (vw as usize, vh as usize);
        for y in 0..h {
            let sy = (y * vh / h).min(vh - 1);
            let row = &out[sy * vw * 4..(sy + 1) * vw * 4];
            let dst = &mut frame[y * w..(y + 1) * w];
            for (x, d) in dst.iter_mut().enumerate() {
                let sx = (x * vw / w).min(vw - 1);
                let p = &row[sx * 4..sx * 4 + 4];
                *d = (p[0] as u32) << 16 | (p[1] as u32) << 8 | p[2] as u32;
            }
        }
        if window.update_with_buffer(&frame, w, h).is_err() {
            break;
        }

        frames += 1;
        if fps_timer.elapsed() >= Duration::from_secs(1) {
            g.fps = frames as f32 / fps_timer.elapsed().as_secs_f32();
            window.set_title(&format!("Desktop Siege - preview - {:.0} fps (Ctrl+Q quits)", g.fps));
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
    let mut t = 0.0f32;
    let mut draw_time = Duration::ZERO;
    let mut draws = 0u32;
    let mut screen_id: Option<ScreenId> = None;
    // Build origin (tiles), picked when B is pressed: just right of the player,
    // tall enough that the bottom rows dig into the ground.
    let mut origin = (0i32, 0i32);

    let shots: &[(f32, &str)] = &[
        (0.5, "01_spawn"),
        (2.0, "02_walk"),
        (2.75, "03_mining"),
        (4.62, "04a_rejected"),
        (5.35, "04_build_drag"),
        (6.0, "05_building"),
        (6.62, "06a_chooser_pop"),
        (7.3, "06_chooser"),
        (8.3, "07_launching"),
        (9.6, "08_online"),
        (12.0, "09_prompt"),
        (12.37, "10_zoom_mid"),
        (12.9, "11_zoom_full"),
        (14.2, "12_zoomed_out"),
        (14.85, "13_crt_close"),
        (15.45, "13a_empty"),
        (16.0, "14_chooser_again"),
        (18.0, "15_second_app"),
        (21.0, "16_dusk"),
        (24.5, "17_night"),
        (26.0, "18_cracked"),
        (27.6, "19_dying"),
        (28.33, "20_explosion"),
        (30.5, "21_after"),
    ];
    let mut next_shot = 0;
    let end = 31.0;

    let tile_to_mouse = |g: &Game, x: i32, y: i32| ((x * 16 + 8) as f32 - g.cam.x, (y * 16 + 8) as f32 - g.cam.y);
    let during = |a: f32, b: f32, t: f32| (a..b).contains(&t);

    while t < end {
        // --- script ---
        input.set(Key::Escape, false);
        input.set(Key::Right, during(1.0, 2.2, t) || during(10.8, 11.5, t));
        input.set(Key::Left, during(9.7, 10.5, t));
        input.set(Key::Jump, during(1.6, 1.8, t));
        let (pcx, pcy) = g.player.center();
        input.lmb = false;
        // Mine the tile just below-right of the player.
        if during(2.6, 3.9, t) {
            let (wx, wy) = (pcx + 20.0, pcy + 28.0);
            input.mouse_x = wx - g.cam.x;
            input.mouse_y = wy - g.cam.y;
            input.lmb = true;
        }
        // Build: press B, drag an 18x11 rect whose bottom rows cover terrain.
        input.set(Key::Build, during(4.2, 4.3, t));
        if during(4.2, 4.22, t) {
            let tx = (pcx / 16.0) as i32 + 2;
            let ty = g.world.surface.get(tx as usize).copied().unwrap_or(48) - 9;
            origin = (tx, ty);
        }
        // A too-small first attempt shows the rejection label.
        if during(4.3, 4.5, t) {
            let f = ((t - 4.3) / 0.15).min(1.0);
            let (ax, ay) = tile_to_mouse(&g, origin.0, origin.1);
            let (bx, by) = tile_to_mouse(&g, origin.0 + 4, origin.1 + 3);
            input.mouse_x = ax + (bx - ax) * f;
            input.mouse_y = ay + (by - ay) * f;
            input.lmb = true;
        }
        if during(4.7, 5.5, t) {
            let f = ((t - 4.7) / 0.7).min(1.0);
            let (ax, ay) = tile_to_mouse(&g, origin.0, origin.1);
            let (bx, by) = tile_to_mouse(&g, origin.0 + 17, origin.1 + 10);
            input.mouse_x = ax + (bx - ax) * f;
            input.mouse_y = ay + (by - ay) * f;
            input.lmb = true;
        }
        // Chooser: glide to "Terminal", click.
        if during(7.4, 8.0, t) {
            if let Some((rx, ry)) = g.screens.chooser_row_center(0, &g.cam) {
                let f = ((t - 7.4) / 0.4).min(1.0);
                input.mouse_x += (rx - input.mouse_x) * f;
                input.mouse_y += (ry - input.mouse_y) * f;
            }
            input.lmb = during(7.9, 7.95, t);
        }
        // Zoom in, then Esc out (the fake compositor holds focus in between).
        input.set(Key::Zoom, during(12.2, 12.25, t));
        if during(13.6, 13.62, t) && fake.give_back_focus() {
            input.set(Key::Escape, true);
        }
        // Close with X, reopen the chooser with E, pick System Monitor with 5.
        input.set(Key::CloseApp, during(14.6, 14.65, t));
        input.set(Key::Interact, during(15.6, 15.65, t));
        input.set(Key::Hotbar5, during(16.3, 16.35, t));
        input.set(Key::Night, during(17.5, 17.6, t));
        input.game_focused = fake.focus.is_none();
        // Mobs are stubbed: simulate attacks from the left side at night.
        if let Some(id) = screen_id {
            if t > 23.5 && ((t * 60.0) as i32) % 14 == 0 {
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
            let (vw, vh) = g.view_size();
            let sc = 2;
            match write_png(&path, &out, vw as usize, vh as usize, sc) {
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

fn write_png(path: &str, rgba: &[u8], w: usize, h: usize, scale: usize) -> Result<(), Box<dyn std::error::Error>> {
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
