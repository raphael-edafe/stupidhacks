//! Compositor state: Wayland globals, windows bound to game screens, focus, the game itself.

use std::{
    collections::VecDeque,
    ffi::OsString,
    process::Child,
    sync::Arc,
    time::{Duration, Instant},
};

use smithay::{
    desktop::{PopupManager, Window, WindowSurfaceType},
    input::{Seat, SeatState},
    reexports::{
        calloop::{generic::Generic, EventLoop, Interest, LoopSignal, Mode, PostAction},
        wayland_protocols::xdg::shell::server::xdg_toplevel,
        wayland_server::{
            backend::{ClientData, ClientId, DisconnectReason},
            protocol::wl_surface::WlSurface,
            Display, DisplayHandle, Resource,
        },
    },
    utils::{Logical, Point, Rectangle, SERIAL_COUNTER},
    wayland::{
        compositor::{CompositorClientState, CompositorState},
        output::OutputManagerState,
        selection::data_device::DataDeviceState,
        shell::xdg::{decoration::XdgDecorationState, XdgShellState},
        shm::ShmState,
        socket::ListeningSocketSource,
    },
};
use tracing::{info, warn};

use shared::{GameApi, GameEvent, InputState, Key, ScreenId, ScreenInfo, APPS, VIEW_H, VIEW_W};

use crate::CalloopData;

/// A toplevel window. `screen` is the game screen it lives in (None = floating dialog).
pub struct WinEntry {
    pub window: Window,
    pub screen: Option<ScreenId>,
    /// Last size we configured (output px), to avoid configure spam.
    pub configured: Option<(i32, i32)>,
}

/// Maps the game's VIEW_W x VIEW_H view into the output (uniform scale + letterbox offset).
#[derive(Clone, Copy, Debug)]
pub struct ViewXform {
    pub scale: f64,
    pub ox: f64,
    pub oy: f64,
}

impl ViewXform {
    pub fn for_output(w: i32, h: i32) -> Self {
        let scale = (w as f64 / VIEW_W as f64).min(h as f64 / VIEW_H as f64).max(0.1);
        let ox = (w as f64 - VIEW_W as f64 * scale) / 2.0;
        let oy = (h as f64 - VIEW_H as f64 * scale) / 2.0;
        ViewXform { scale, ox, oy }
    }

    /// View rect -> output rect (logical == physical, scale 1).
    pub fn rect(&self, r: shared::Rect) -> Rectangle<i32, Logical> {
        let x = (self.ox + r.x as f64 * self.scale).round() as i32;
        let y = (self.oy + r.y as f64 * self.scale).round() as i32;
        let w = (r.w as f64 * self.scale).round() as i32;
        let h = (r.h as f64 * self.scale).round() as i32;
        Rectangle::new((x, y).into(), (w.max(1), h.max(1)).into())
    }

    pub fn to_view(&self, p: Point<f64, Logical>) -> (f32, f32) {
        (((p.x - self.ox) / self.scale) as f32, ((p.y - self.oy) / self.scale) as f32)
    }
}

pub struct Options {
    pub no_kill: bool,
    /// Test hook: build a screen and launch this app automatically.
    pub autolaunch: Option<usize>,
    /// Test hook: write a PNG of the output at these times (seconds) and quit after the last.
    pub shots: Vec<(f32, String)>,
}

pub struct Siege {
    pub start_time: Instant,
    pub socket_name: OsString,
    pub display_handle: DisplayHandle,
    pub loop_signal: LoopSignal,

    pub compositor_state: CompositorState,
    pub xdg_shell_state: XdgShellState,
    pub xdg_decoration_state: XdgDecorationState,
    pub shm_state: ShmState,
    pub output_manager_state: OutputManagerState,
    pub seat_state: SeatState<Siege>,
    pub data_device_state: DataDeviceState,
    pub popups: PopupManager,
    pub seat: Seat<Self>,

    pub windows: Vec<WinEntry>,
    /// Screens waiting for their app's first window, oldest first.
    pub pending: VecDeque<(ScreenId, Instant)>,
    /// Windows we asked to close; if still alive after the deadline, disconnect the client.
    pub closing: Vec<(Window, Instant)>,
    pub children: Vec<Child>,

    /// Keyboard focus: Some(window surface) = an app has it, None = the game.
    pub app_focus: Option<WlSurface>,
    /// LMB/RMB went down on the world: keep pointer events away from windows until release.
    pub game_drag: bool,

    pub game: game::Game,
    pub input: InputState,
    /// Send Key::Escape to the game on the next tick (focus handed back from an app).
    pub escape_pulse: bool,
    pub screens: Vec<ScreenInfo>,
    pub view: ViewXform,
    pub opts: Options,
    pub frame: u64,
    /// Test hook: press this key on the game's chooser at this time (autolaunch).
    pub autopick: Option<(Instant, Key, u8)>,
}

impl Siege {
    pub fn new(event_loop: &mut EventLoop<CalloopData>, display: Display<Self>, opts: Options) -> Self {
        let dh = display.handle();
        let compositor_state = CompositorState::new::<Self>(&dh);
        let xdg_shell_state = XdgShellState::new::<Self>(&dh);
        let xdg_decoration_state = XdgDecorationState::new::<Self>(&dh);
        let shm_state = ShmState::new::<Self>(&dh, vec![]);
        let output_manager_state = OutputManagerState::new_with_xdg_output::<Self>(&dh);
        let mut seat_state = SeatState::new();
        let data_device_state = DataDeviceState::new::<Self>(&dh);
        let mut seat: Seat<Self> = seat_state.new_wl_seat(&dh, "siege");
        if let Err(e) = seat.add_keyboard(Default::default(), 300, 30) {
            warn!("keyboard init failed: {e:?}");
        }
        seat.add_pointer();

        let socket_name = Self::init_wayland_listener(display, event_loop);
        let mut input = InputState::default();
        input.game_focused = true;

        Siege {
            start_time: Instant::now(),
            display_handle: dh,
            loop_signal: event_loop.get_signal(),
            socket_name,
            compositor_state,
            xdg_shell_state,
            xdg_decoration_state,
            shm_state,
            output_manager_state,
            seat_state,
            data_device_state,
            popups: PopupManager::default(),
            seat,
            windows: Vec::new(),
            pending: VecDeque::new(),
            closing: Vec::new(),
            children: Vec::new(),
            app_focus: None,
            game_drag: false,
            game: game::Game::new(game::DEFAULT_SEED),
            input,
            escape_pulse: false,
            screens: Vec::new(),
            view: ViewXform::for_output(1920, 1080),
            opts,
            frame: 0,
            autopick: None,
        }
    }

    fn init_wayland_listener(display: Display<Siege>, event_loop: &mut EventLoop<CalloopData>) -> OsString {
        let listening_socket = match ListeningSocketSource::new_auto() {
            Ok(s) => s,
            Err(e) => {
                eprintln!("cannot create wayland socket: {e}");
                std::process::exit(1);
            }
        };
        let socket_name = listening_socket.socket_name().to_os_string();
        let handle = event_loop.handle();
        let r = handle.insert_source(listening_socket, move |client_stream, _, data| {
            if let Err(e) = data.state.display_handle.insert_client(client_stream, Arc::new(ClientState::default())) {
                warn!("insert_client failed: {e}");
            }
        });
        if let Err(e) = r {
            eprintln!("cannot listen: {e}");
            std::process::exit(1);
        }
        let r = handle.insert_source(Generic::new(display, Interest::READ, Mode::Level), |_, display, data| {
            // Safety: the display is never dropped while the loop runs.
            unsafe {
                if let Err(e) = display.get_mut().dispatch_clients(&mut data.state) {
                    warn!("dispatch_clients: {e}");
                }
            }
            Ok(PostAction::Continue)
        });
        if let Err(e) = r {
            eprintln!("cannot add display source: {e}");
            std::process::exit(1);
        }
        socket_name
    }

    // ---------------- game tick ----------------

    pub fn tick_game(&mut self, dt: f32) {
        let mut input = self.input;
        input.game_focused = self.app_focus.is_none();
        if self.escape_pulse {
            input.set(Key::Escape, true);
            self.escape_pulse = false;
        }
        if let Some((at, k, n)) = self.autopick {
            if Instant::now() >= at {
                input.set(k, n < 2); // hold for two ticks, then release
                self.autopick = if n < 3 { Some((at, k, n + 1)) } else { None };
            }
        }
        self.game.update(dt, &input);

        for ev in self.game.drain_events() {
            self.handle_game_event(ev);
        }

        // Test hook: auto-build a screen and launch an app into it.
        if let Some(app) = self.opts.autolaunch {
            if self.start_time.elapsed() > Duration::from_millis(1500) && self.game.screens.list.is_empty() && self.frame < 100_000 {
                let w = &self.game.world;
                let tx = w.w / 2 - 2;
                let ty = w.surface[tx as usize] - 13;
                self.game.screens.spawn(tx, ty, 22, 11);
                self.opts.autolaunch = Some(app | 0x1000); // mark as done
            }
        }

        self.screens = self.game.screens();
        self.reap_closing();
    }

    fn handle_game_event(&mut self, ev: GameEvent) {
        match ev {
            GameEvent::ScreenBuilt { id } => {
                info!("screen {id} built");
                if let Some(app) = self.opts.autolaunch {
                    let keys = [Key::Hotbar1, Key::Hotbar2, Key::Hotbar3, Key::Hotbar4, Key::Hotbar5];
                    if let Some(k) = keys.get(app & 0xFFF) {
                        self.autopick = Some((Instant::now() + Duration::from_millis(700), *k, 0));
                    }
                }
            }
            GameEvent::LaunchApp { id, app } => self.launch(id, app),
            GameEvent::CloseApp { id } => {
                self.close_screen_window(id, false);
            }
            GameEvent::FocusScreen { id } => {
                if let Some(surface) = self.windows.iter().find(|w| w.screen == Some(id)).and_then(|w| w.window.toplevel().map(|t| t.wl_surface().clone())) {
                    self.focus_app(Some(surface));
                }
            }
            GameEvent::ScreenDamaged { .. } => {}
            GameEvent::ScreenDestroyed { id } => {
                info!("screen {id} destroyed");
                self.pending.retain(|(s, _)| *s != id);
                if self.opts.no_kill {
                    // Keep the app alive but unbind it: it becomes a floating window.
                    for w in self.windows.iter_mut().filter(|w| w.screen == Some(id)) {
                        w.screen = None;
                        w.configured = None;
                    }
                } else {
                    self.close_screen_window(id, true);
                }
            }
        }
    }

    /// Spawn APPS[app] for a screen. Its first new toplevel gets bound to the screen.
    pub fn launch(&mut self, screen: ScreenId, app: usize) {
        let Some(entry) = APPS.get(app) else {
            warn!("unknown app index {app}");
            return;
        };
        // GTK apps are single-instance over D-Bus: a private session bus makes them
        // start a fresh instance inside this compositor instead of the host desktop's.
        let use_dbus = !entry.command.first().is_some_and(|c| *c == "foot") && which("dbus-run-session");
        let mut cmd = if use_dbus {
            let mut c = std::process::Command::new("dbus-run-session");
            c.arg("--").args(entry.command);
            c
        } else {
            let mut c = std::process::Command::new(entry.command[0]);
            c.args(&entry.command[1..]);
            c
        };
        cmd.env("WAYLAND_DISPLAY", &self.socket_name)
            .env_remove("DISPLAY")
            .env("GDK_BACKEND", "wayland")
            .env("GSK_RENDERER", "cairo")
            .env("XDG_CURRENT_DESKTOP", "DesktopSiege")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        match cmd.spawn() {
            Ok(child) => {
                info!("launched {} for screen {screen} (pid {})", entry.name, child.id());
                self.children.push(child);
                self.pending.push_back((screen, Instant::now()));
            }
            Err(e) => warn!("failed to launch {}: {e}", entry.name),
        }
    }

    fn close_screen_window(&mut self, id: ScreenId, force_later: bool) {
        let mut i = 0;
        while i < self.windows.len() {
            if self.windows[i].screen == Some(id) {
                let entry = self.windows.remove(i);
                if let Some(t) = entry.window.toplevel() {
                    if self.app_focus.as_ref() == Some(t.wl_surface()) {
                        self.focus_app(None);
                    }
                    t.send_close();
                }
                if force_later {
                    self.closing.push((entry.window, Instant::now() + Duration::from_millis(1500)));
                }
                self.game.set_screen_occupied(id, false);
            } else {
                i += 1;
            }
        }
    }

    /// Disconnect clients that ignored a close request from a destroyed screen.
    fn reap_closing(&mut self) {
        let now = Instant::now();
        let dh = self.display_handle.clone();
        self.closing.retain(|(w, deadline)| {
            let Some(t) = w.toplevel() else { return false };
            if !t.alive() {
                return false;
            }
            if now >= *deadline {
                if let Ok(client) = dh.get_client(t.wl_surface().id()) {
                    info!("client ignored close; disconnecting it");
                    dh.backend_handle().kill_client(client.id(), DisconnectReason::ConnectionClosed);
                }
                return false;
            }
            true
        });
        // Reap exited children so they don't linger as zombies.
        self.children.retain_mut(|c| matches!(c.try_wait(), Ok(None)));
    }

    // ---------------- windows ----------------

    pub fn on_new_toplevel(&mut self, window: Window) {
        // Bind to the oldest pending screen that still exists and has no window.
        let mut screen = None;
        while let Some((id, at)) = self.pending.pop_front() {
            let exists = self.game.screens.list.iter().any(|s| s.id == id);
            let taken = self.windows.iter().any(|w| w.screen == Some(id));
            if exists && !taken && at.elapsed() < Duration::from_secs(30) {
                screen = Some(id);
                break;
            }
        }
        if let Some(t) = window.toplevel() {
            t.with_pending_state(|s| {
                s.states.set(xdg_toplevel::State::TiledLeft);
                s.states.set(xdg_toplevel::State::TiledRight);
                s.states.set(xdg_toplevel::State::TiledTop);
                s.states.set(xdg_toplevel::State::TiledBottom);
                if let Some(id) = screen {
                    if let Some(info) = self.screens.iter().find(|i| i.id == id) {
                        let r = self.view.rect(info.rect);
                        s.size = Some(r.size);
                    }
                }
            });
        }
        info!("new toplevel -> screen {screen:?}");
        if let Some(id) = screen {
            self.game.set_screen_occupied(id, true);
        }
        self.windows.push(WinEntry { window, screen, configured: None });
    }

    pub fn on_toplevel_destroyed(&mut self, surface: &WlSurface) {
        if let Some(pos) = self.windows.iter().position(|w| w.window.toplevel().is_some_and(|t| t.wl_surface() == surface)) {
            let entry = self.windows.remove(pos);
            if let Some(id) = entry.screen {
                // Only mark empty if no other window is bound to it.
                if !self.windows.iter().any(|w| w.screen == Some(id)) {
                    self.game.set_screen_occupied(id, false);
                }
            }
        }
        if self.app_focus.as_ref() == Some(surface) {
            self.focus_app(None);
        }
    }

    /// Resize bound windows to their screen rect once the rect has settled (not mid-zoom).
    pub fn configure_windows(&mut self) {
        for w in self.windows.iter_mut() {
            let Some(id) = w.screen else { continue };
            let Some(info) = self.screens.iter().find(|i| i.id == id) else { continue };
            let settled = info.zoom < 0.001 || info.zoom > 0.999;
            if !settled {
                continue;
            }
            let r = self.view.rect(info.rect);
            let size = (r.size.w, r.size.h);
            if w.configured != Some(size) {
                if let Some(t) = w.window.toplevel() {
                    t.with_pending_state(|s| s.size = Some(r.size));
                    if t.is_initial_configure_sent() {
                        t.send_pending_configure();
                    }
                }
                w.configured = Some(size);
            }
        }
    }

    /// Where a window is drawn on the output this frame (None = hidden).
    pub fn window_rect(&self, w: &WinEntry, out_w: i32, out_h: i32) -> Option<Rectangle<i32, Logical>> {
        match w.screen {
            Some(id) => self.screens.iter().find(|i| i.id == id).map(|i| self.view.rect(i.rect)),
            None => {
                let g = w.window.geometry();
                let (gw, gh) = (g.size.w.max(1), g.size.h.max(1));
                Some(Rectangle::new(((out_w - gw) / 2, (out_h - gh) / 2).into(), (gw, gh).into()))
            }
        }
    }

    /// Topmost window under an output point: (window index, surface, surface origin in output coords).
    pub fn surface_under(&self, pos: Point<f64, Logical>, out_w: i32, out_h: i32) -> Option<(usize, WlSurface, Point<f64, Logical>)> {
        // Draw order is: bound windows (zoomed last), then floating. Hit-test in reverse.
        let order = self.draw_order();
        for &i in order.iter().rev() {
            let w = &self.windows[i];
            let Some(r) = self.window_rect(w, out_w, out_h) else { continue };
            let geo = w.window.geometry();
            let sx = r.size.w as f64 / geo.size.w.max(1) as f64;
            let sy = r.size.h as f64 / geo.size.h.max(1) as f64;
            // Point in window-local (geometry) coordinates.
            let local = Point::<f64, Logical>::from(((pos.x - r.loc.x as f64) / sx, (pos.y - r.loc.y as f64) / sy));
            let in_rect = r.to_f64().contains(pos);
            let local_surface = local + geo.loc.to_f64();
            if let Some((surface, sloc)) = w.window.surface_under(local_surface, WindowSurfaceType::ALL) {
                // Only accept toplevel hits inside the visible rect; popups may stick out.
                let is_popup = w.window.toplevel().is_some_and(|t| t.wl_surface() != &surface);
                if in_rect || is_popup {
                    let origin = Point::<f64, Logical>::from((
                        r.loc.x as f64 + (sloc.x - geo.loc.x) as f64 * sx,
                        r.loc.y as f64 + (sloc.y - geo.loc.y) as f64 * sy,
                    ));
                    return Some((i, surface, origin));
                }
            }
        }
        None
    }

    /// Indices into `windows`, back to front.
    pub fn draw_order(&self) -> Vec<usize> {
        let zoom_of = |w: &WinEntry| w.screen.and_then(|id| self.screens.iter().find(|i| i.id == id)).map(|i| i.zoom).unwrap_or(0.0);
        let mut bound: Vec<usize> = (0..self.windows.len()).filter(|&i| self.windows[i].screen.is_some()).collect();
        bound.sort_by(|&a, &b| zoom_of(&self.windows[a]).partial_cmp(&zoom_of(&self.windows[b])).unwrap_or(std::cmp::Ordering::Equal));
        bound.extend((0..self.windows.len()).filter(|&i| self.windows[i].screen.is_none()));
        bound
    }

    pub fn focus_app(&mut self, surface: Option<WlSurface>) {
        let serial = SERIAL_COUNTER.next_serial();
        for w in &self.windows {
            let active = surface.as_ref().is_some_and(|s| w.window.toplevel().is_some_and(|t| t.wl_surface() == s));
            w.window.set_activated(active);
            if let Some(t) = w.window.toplevel() {
                if t.is_initial_configure_sent() {
                    t.send_pending_configure();
                }
            }
        }
        if surface.is_none() && self.app_focus.is_some() {
            // Handing focus back to the game: the game zooms out on this.
            self.escape_pulse = true;
        }
        // Drop any keys the game thought were held.
        for k in ALL_KEYS {
            self.input.set(k, false);
        }
        self.app_focus = surface.clone();
        if let Some(kb) = self.seat.get_keyboard() {
            kb.set_focus(self, surface, serial);
        }
    }
}

pub const ALL_KEYS: [Key; 19] = [
    Key::Left,
    Key::Right,
    Key::Up,
    Key::Down,
    Key::Jump,
    Key::Build,
    Key::Night,
    Key::God,
    Key::ToggleHud,
    Key::Torch,
    Key::Interact,
    Key::CloseApp,
    Key::Zoom,
    Key::Escape,
    Key::Hotbar1,
    Key::Hotbar2,
    Key::Hotbar3,
    Key::Hotbar4,
    Key::Hotbar5,
];

fn which(bin: &str) -> bool {
    std::env::var_os("PATH").is_some_and(|p| std::env::split_paths(&p).any(|d| d.join(bin).is_file()))
}

#[derive(Default)]
pub struct ClientState {
    pub compositor_state: CompositorClientState,
}

impl ClientData for ClientState {
    fn initialized(&self, _client_id: ClientId) {}
    fn disconnected(&self, _client_id: ClientId, _reason: DisconnectReason) {}
}
