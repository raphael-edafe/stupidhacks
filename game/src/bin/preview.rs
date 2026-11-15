//! Standalone preview: the game without the compositor.
//!
//!   cargo run -p game --bin preview --release              # interactive window
//!   cargo run -p game --bin preview --release -- --scale 3 # 1920x1080
//!   cargo run -p game --bin preview --release -- --shots out/   # headless scripted demo -> PNGs
//!
//! The launcher offers the real installed apps (`apps::scan()`), like the
//! compositor. A fake compositor stands in for the real one: `LaunchApp` maps a
//! fake window for that catalog entry 0.8 s later, `CloseApp`/`ScreenDestroyed` remove it,
//! `FocusScreen` takes keyboard focus away from the game (Esc gives it back
//! and sends Key::Escape for one tick). Quit with Ctrl+Q or by closing the window.

use game::{Game, DEFAULT_SEED};
use shared::{AppInfo, GameApi, GameEvent, InputState, Key, ScreenId, ScreenInfo, LAYER_BYTES, VIEW_H, VIEW_W};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
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
    /// The catalog given to the game (LaunchApp indexes into it).
    apps: Vec<AppInfo>,
}

/// Fake window styles.
const K_TERMINAL: usize = 0;
const K_FILES: usize = 1;
const K_EDITOR: usize = 2;
const K_CALC: usize = 3;
const K_MONITOR: usize = 4;
const K_MATRIX: usize = 5;
const K_BROWSER: usize = 6;
const K_GENERIC: usize = 7;

/// Which fake window to draw for a catalog app (by desktop id / name).
fn fake_kind(app: Option<&AppInfo>) -> usize {
    let Some(a) = app else { return K_GENERIC };
    let id = a.id.to_ascii_lowercase();
    let name = a.name.to_ascii_lowercase();
    if id.contains("firefox") || id.contains("chrom") || name.contains("browser") {
        K_BROWSER
    } else if id == "foot" || id.contains("terminal") || id.contains("ptyxis") {
        K_TERMINAL
    } else if id.contains("nautilus") {
        K_FILES
    } else if id.contains("texteditor") {
        K_EDITOR
    } else if id.contains("calculator") {
        K_CALC
    } else if id.contains("system-monitor") || id.contains("systemmonitor") || id == "htop" {
        K_MONITOR
    } else if id.contains("cmatrix") {
        K_MATRIX
    } else {
        K_GENERIC
    }
}

/// Load the installed-app catalog into the game (falls back to the static list).
fn load_catalog(g: &mut Game) -> Vec<AppInfo> {
    let infos: Vec<AppInfo> = apps::scan()
        .into_iter()
        .map(|a| AppInfo { id: a.id, name: a.name, icon: a.icon, favourite: a.favourite })
        .collect();
    g.set_app_catalog(infos);
    println!("[preview] app catalog: {} apps", g.screens.catalog.len());
    g.screens.catalog.clone()
}

/// minifb text input: typed characters, drained into `Game::text_input`.
struct TextSink(Rc<RefCell<String>>);

impl minifb::InputCallback for TextSink {
    fn add_char(&mut self, uni_char: u32) {
        if let Some(ch) = char::from_u32(uni_char).filter(|c| !c.is_control()) {
            self.0.borrow_mut().push(ch);
        }
    }
}

impl FakeCompositor {
    fn handle(&mut self, g: &mut Game, events: Vec<GameEvent>, dt: f32) {
        for e in events {
            match e {
                GameEvent::LaunchApp { id, app } => {
                    let name = self.apps.get(app).map(|a| a.name.as_str()).unwrap_or("?");
                    println!("[compositor] launch {name} into screen {id}");
                    self.pending.insert(id, (app, MAP_DELAY));
                }
                GameEvent::CloseApp { id } | GameEvent::ScreenDestroyed { id } => {
                    self.pending.remove(&id);
                    if let Some(app) = self.windows.remove(&id) {
                        let name = self.apps.get(app).map(|a| a.name.as_str()).unwrap_or("?");
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

/// Fake window contents per fake kind.
fn fake_lines(app: usize, name: &str, t: f32) -> Vec<String> {
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
        K_BROWSER => Vec::new(),
        _ => vec![name.to_uppercase(), String::new(), "THIS WOULD BE THE".into(), "REAL APP WINDOW".into(), "IN THE COMPOSITOR".into()],
    }
}

/// Draw a fake app window into the composited RGBA buffer (view resolution), at `s.rect`.
fn draw_fake_window(buf: &mut [u8], vw: u32, vh: u32, s: &ScreenInfo, name: &str, app: usize, t: f32) {
    let mut c = game::gfx::Canvas::new(buf, vw, vh);
    let r = s.rect;
    if r.w < 8 || r.h < 8 {
        return;
    }
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
        K_BROWSER => {
            // Browser: URL bar + a page of blocks.
            let pad = 3 * sc;
            c.fill_rect(r.x + pad, r.y + tb + pad, r.w - 2 * pad, 7 * sc, [235, 238, 245, 255]);
            c.text(r.x + pad + 3, r.y + tb + pad + sc, "HTTPS://DESKTOP-SIEGE.GAME", sc, [60, 70, 100, 255]);
            let py = r.y + tb + 2 * pad + 7 * sc;
            c.fill_rect(r.x, py, r.w, r.y + r.h - py, [246, 246, 250, 255]);
            c.fill_rect(r.x + 8 * sc, py + 6 * sc, (r.w / 2).max(4), 6 * sc, [255, 120, 40, 255]);
            for i in 0..6 {
                let y = py + 18 * sc + i * 7 * sc;
                if y + 3 * sc < r.y + r.h {
                    c.fill_rect(r.x + 8 * sc, y, (r.w - 16 * sc) * (6 + i % 3) / 9, 3 * sc, [190, 196, 210, 255]);
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
            for (i, l) in fake_lines(app, name, t).iter().enumerate() {
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
            let info = fake.apps.get(app);
            let name = info.map(|a| a.name.as_str()).unwrap_or("App");
            draw_fake_window(out, vw, vh, &s, name, fake_kind(info), t);
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
    let mut fake = FakeCompositor { apps: load_catalog(&mut g), ..Default::default() };
    let typed = Rc::new(RefCell::new(String::new()));
    window.set_input_callback(Box::new(TextSink(typed.clone())));
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
        (Key::Enter, &[K::Enter, K::NumPadEnter][..]),
        (Key::Backspace, &[K::Backspace][..]),
        (Key::Smash, &[K::Q][..]),
        (Key::ZoomOut, &[K::Minus][..]),
        (Key::ZoomIn, &[K::Equal][..]),
    ];
    // Keys that still work while the launcher search takes text (letters,
    // space and number row become text instead; digits also pick favourites).
    let text_safe = |k: K| {
        matches!(
            k,
            K::Left | K::Right | K::Up | K::Down | K::Escape | K::Enter | K::NumPadEnter | K::Backspace | K::F1 | K::Key1 | K::Key2 | K::Key3 | K::Key4 | K::Key5 | K::Key6
        )
    };

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
        let text_mode = g.wants_text() && !app_focused;
        {
            let mut buf = typed.borrow_mut();
            if text_mode && !buf.is_empty() && !ctrl {
                g.text_input(&buf);
            }
            buf.clear();
        }
        for (k, keys) in map.iter() {
            input.set(*k, !app_focused && keys.iter().any(|kk| (!text_mode || text_safe(*kk)) && window.is_key_down(*kk)));
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
    let mut fake = FakeCompositor { apps: load_catalog(&mut g), ..Default::default() };
    let (mut world, mut fx, mut out) = (vec![0u8; LAYER_BYTES], vec![0u8; LAYER_BYTES], vec![0u8; LAYER_BYTES]);
    let mut input = InputState::default();
    let mut t = 0.0f32;
    // Everything after the launcher search is shifted by this much (s).
    const S: f32 = 1.6;
    // Launcher script: favourites shot, type "fire", Backspace x4, type "calc", Enter.
    let typing: &[(f32, &str)] = &[(7.5, "f"), (7.56, "i"), (7.62, "r"), (7.68, "e"), (8.45, "c"), (8.51, "a"), (8.57, "l"), (8.63, "c")];
    let mut next_typed = 0;
    let taps: &[(f32, Key)] = &[(8.0, Key::Backspace), (8.08, Key::Backspace), (8.16, Key::Backspace), (8.24, Key::Backspace), (8.85, Key::Enter)];
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
        (7.3, "06_launcher_favourites"),
        (7.9, "06b_search_fire"),
        (8.75, "06c_search_calc"),
        (9.25, "07_launching"),
        (9.6 + S, "08_online"),
        (12.0 + S, "09_prompt"),
        (12.37 + S, "10_zoom_mid"),
        (12.9 + S, "11_zoom_full"),
        (14.2 + S, "12_zoomed_out"),
        (14.85 + S, "13_crt_close"),
        (15.45 + S, "13a_empty"),
        (16.0 + S, "14_chooser_again"),
        (18.0 + S, "15_second_app"),
        (21.0 + S, "16_dusk"),
        (24.5 + S, "17_night"),
        (26.0 + S, "18_cracked"),
        (27.6 + S, "19_dying"),
        (28.33 + S, "20_explosion"),
        (30.5 + S, "21_after"),
        (33.05 + S, "22_crafting"),
        (34.6 + S, "23_torchlight_armour"),
        (35.75 + S, "24_bow"),
    ];
    let mut next_shot = 0;
    let end = 36.0 + S;

    let tile_to_mouse = |g: &Game, x: i32, y: i32| ((x * 16 + 8) as f32 - g.cam.x, (y * 16 + 8) as f32 - g.cam.y);
    let during = |a: f32, b: f32, t: f32| (a..b).contains(&t);

    while t < end {
        // --- script ---
        input.set(Key::Escape, false);
        input.set(Key::Right, during(1.0, 2.2, t) || during(10.8 + S, 11.5 + S, t));
        input.set(Key::Left, during(9.7 + S, 10.5 + S, t));
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
        // Launcher: type into the search (like the compositor's text routing),
        // Backspace / Enter as keys.
        while next_typed < typing.len() && t >= typing[next_typed].0 {
            if g.wants_text() {
                g.text_input(typing[next_typed].1);
            }
            next_typed += 1;
        }
        for &(at, k) in taps {
            if during(at, at + 0.04, t) {
                input.set(k, true);
            } else if during(at + 0.04, at + 0.08, t) {
                input.set(k, false);
            }
        }
        // Zoom in, then Esc out (the fake compositor holds focus in between).
        input.set(Key::Zoom, during(12.2 + S, 12.25 + S, t));
        if during(13.6 + S, 13.62 + S, t) && fake.give_back_focus() {
            input.set(Key::Escape, true);
        }
        // Close with X, reopen the launcher with E, pick favourite 6 (System Monitor).
        input.set(Key::CloseApp, during(14.6 + S, 14.65 + S, t));
        input.set(Key::Night, during(17.5 + S, 17.6 + S, t));
        // After the siege: god mode, place the workbench, craft copper armour, a bow
        // and arrows, light the night with torches, then shoot the bow.
        input.set(Key::God, during(31.0 + S, 31.05 + S, t));
        input.set(Key::Hotbar6, during(16.3 + S, 16.35 + S, t) || during(31.2 + S, 31.25 + S, t) || during(35.0 + S, 35.05 + S, t));
        let ptx = (pcx / 16.0) as i32;
        let bench = (ptx - 2, g.world.ground_y(ptx - 2) - 1);
        input.rmb = during(31.4 + S, 31.5 + S, t);
        if during(31.3 + S, 31.6 + S, t) {
            (input.mouse_x, input.mouse_y) = tile_to_mouse(&g, bench.0, bench.1);
        }
        input.set(Key::Interact, during(15.6 + S, 15.65 + S, t) || during(31.8 + S, 31.85 + S, t));
        let view = (g.cam.vw as i32, g.cam.vh as i32);
        for (at, item) in [(32.2 + S, game::items::Item::CopperHelmet), (32.5 + S, game::items::Item::CopperChest), (32.8 + S, game::items::Item::Bow), (33.0 + S, game::items::Item::Arrow)] {
            let row = game::items::RECIPES.iter().position(|r| r.out == item).unwrap_or(0);
            if during(at - 0.15, at + 0.05, t) {
                (input.mouse_x, input.mouse_y) = game::crafting::Crafting::row_center(row, view);
                input.lmb = during(at, at + 0.02, t);
            }
        }
        input.set(Key::Escape, during(33.4 + S, 33.45 + S, t));
        // Torches: T at a tile to the left and one at the player's feet.
        let mut torch = false;
        for (at, dx) in [(33.8 + S, -5), (34.0 + S, 1)] {
            if during(at - 0.05, at + 0.05, t) {
                let tx = ptx + dx;
                (input.mouse_x, input.mouse_y) = tile_to_mouse(&g, tx, g.world.ground_y(tx) - 1);
            }
            torch |= during(at, at + 0.02, t);
        }
        input.set(Key::Torch, torch);
        if during(35.2 + S, 36.0 + S, t) {
            let aim = g.mobs.list.first().map(|z| z.center()).unwrap_or((pcx + 160.0, pcy - 70.0));
            (input.mouse_x, input.mouse_y) = (aim.0 - g.cam.x, aim.1 - g.cam.y);
            input.lmb = true;
        }
        input.game_focused = fake.focus.is_none();
        // Mobs are stubbed: simulate attacks from the left side at night.
        if let Some(id) = screen_id {
            if t > 23.5 + S && ((t * 60.0) as i32) % 14 == 0 {
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
