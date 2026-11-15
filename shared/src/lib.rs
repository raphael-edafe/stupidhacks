//! The contract between the compositor (person A) and the game (person B).
//!
//! Keep this crate tiny and dependency-free. Changing it breaks the other
//! person's build, so agree on changes first.
//!
//! Coordinates: everything here is in *view pixels*. The game renders an image
//! of `GameApi::view_size()` (640x360 "close" or 960x540 "far" zoom); the
//! compositor scales it to the output with nearest filtering.

/// "Close" view size (3x pixels at 1080p).
pub const VIEW_W: u32 = 640;
pub const VIEW_H: u32 = 360;
/// "Far" (zoomed-out) view size (2x pixels at 1080p).
pub const FAR_W: u32 = 960;
pub const FAR_H: u32 = 540;
pub const SCALE: u32 = 3;
pub const OUTPUT_W: u32 = VIEW_W * SCALE; // 1920
pub const OUTPUT_H: u32 = VIEW_H * SCALE; // 1080

/// Bytes in one RGBA8 layer buffer at the close view size
/// (row-major, top-left origin, straight alpha). Use `view_size()` for the live size.
pub const LAYER_BYTES: usize = (VIEW_W * VIEW_H * 4) as usize;

pub type ScreenId = u32;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    pub fn contains(&self, px: f32, py: f32) -> bool {
        px >= self.x as f32
            && py >= self.y as f32
            && px < (self.x + self.w) as f32
            && py < (self.y + self.h) as f32
    }
}

/// Logical keys the game cares about. Front-ends map their physical keys onto these.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Key {
    Left = 0,
    Right,
    Up,
    Down,
    Jump,
    /// B: start screen-building mode (then drag a rectangle with LMB).
    Build,
    /// N: skip to dusk (demo knob).
    Night,
    /// G: god mode (demo knob).
    God,
    /// F1: hide HUD (demo knob).
    ToggleHud,
    /// T: place a torch.
    Torch,
    /// E: interact (open the app chooser on an empty screen you're standing near).
    Interact,
    /// X: close the app on the nearby screen (the screen itself stays).
    CloseApp,
    /// Z: zoom into the nearby screen (window grows to fill the view and gets focus).
    Zoom,
    /// Esc: back out of menus / zoom. The compositor also sends this when it
    /// takes keyboard focus away from an app.
    Escape,
    Hotbar1,
    Hotbar2,
    Hotbar3,
    Hotbar4,
    Hotbar5,
    Hotbar6,
    /// Enter: confirm (launcher).
    Enter,
    /// Backspace: delete a character (launcher search).
    Backspace,
    /// Q: hold near a screen to smash it.
    Smash,
    /// `=`: zoom the world in (close view).
    ZoomIn,
    /// `-`: zoom the world out (far view).
    ZoomOut,
}

/// Snapshot of input for one tick. Edge detection ("pressed this frame") is
/// the game's job: it keeps the previous snapshot.
#[derive(Clone, Copy, Debug, Default)]
pub struct InputState {
    keys: u32,
    /// Pointer position in view pixels (may be outside the view).
    pub mouse_x: f32,
    pub mouse_y: f32,
    pub lmb: bool,
    pub rmb: bool,
    /// Mouse wheel notches this tick (positive = up / away from the user).
    /// Only sent while the pointer is over the world, not over a window.
    pub wheel: f32,
    /// False while an app window has keyboard focus: the game must ignore
    /// keys and mouse buttons (but keep simulating).
    pub game_focused: bool,
}

impl InputState {
    pub fn set(&mut self, key: Key, down: bool) {
        let bit = 1u32 << key as u8;
        if down {
            self.keys |= bit;
        } else {
            self.keys &= !bit;
        }
    }

    pub fn down(&self, key: Key) -> bool {
        self.keys & (1u32 << key as u8) != 0
    }
}

/// A built screen, as the compositor needs to see it each frame.
#[derive(Clone, Copy, Debug)]
pub struct ScreenInfo {
    pub id: ScreenId,
    /// Where to draw the app window this frame, camera, shake and zoom applied.
    /// When zooming, this grows towards the full view. The compositor scales the
    /// window's buffer to fit and sends a configure once the size settles.
    pub rect: Rect,
    /// 0.0 = in the world, 1.0 = fully zoomed in (fills the view).
    pub zoom: f32,
    /// 0.0..=1.0
    pub hp: f32,
    /// Window should flicker/glitch this frame (being hit or dying).
    pub glitch: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GameEvent {
    /// Frame finished building. The game now shows its app chooser; nothing to launch yet.
    ScreenBuilt { id: ScreenId },
    /// Player picked an app for a screen: launch `APPS[app]` and bind its first window to `id`.
    LaunchApp { id: ScreenId, app: usize },
    /// Player closed the app by hand: send xdg_toplevel.close. The screen stays (empty).
    CloseApp { id: ScreenId },
    /// Give keyboard focus to the window on this screen (zoom-in finished starting).
    FocusScreen { id: ScreenId },
    /// Screen took a hit; hp is what's left (0..1).
    ScreenDamaged { id: ScreenId, hp: f32 },
    /// Screen destroyed: compositor closes the app bound to it.
    ScreenDestroyed { id: ScreenId },
}

/// One launchable app, as the game sees it. The compositor keeps the matching
/// command line; `GameEvent::LaunchApp { app }` is an index into the catalog
/// passed to `GameApi::set_app_catalog`.
#[derive(Clone, Debug, Default)]
pub struct AppInfo {
    /// Desktop file id, e.g. "firefox_firefox" or "org.gnome.Nautilus".
    pub id: String,
    pub name: String,
    /// 16x16 straight-alpha RGBA icon, if one was found.
    pub icon: Option<Vec<u8>>,
    /// Pinned favourite (shown when the search box is empty), in this order.
    pub favourite: Option<u8>,
}

/// Fallback app list until a catalog is supplied (and for tests).
/// Apps offered in the in-game chooser. `command` is run by the compositor
/// with WAYLAND_DISPLAY pointing at the nested compositor.
#[derive(Clone, Copy, Debug)]
pub struct AppEntry {
    pub name: &'static str,
    pub command: &'static [&'static str],
}

pub const APPS: &[AppEntry] = &[
    AppEntry { name: "Terminal", command: &["foot"] },
    AppEntry { name: "Files", command: &["nautilus", "--new-window"] },
    AppEntry { name: "Text Editor", command: &["gnome-text-editor", "--new-window"] },
    AppEntry { name: "Calculator", command: &["gnome-calculator"] },
    AppEntry { name: "System Monitor", command: &["foot", "-e", "htop"] },
    AppEntry { name: "Matrix", command: &["foot", "-e", "cmatrix"] },
];

pub trait GameApi {
    /// Advance the simulation. Called at a fixed 60 Hz (dt = 1/60).
    fn update(&mut self, dt: f32, input: &InputState);
    /// Draw the layer under the windows. `rgba` is LAYER_BYTES long; fully overwrite it.
    fn draw_world(&mut self, rgba: &mut [u8]);
    /// Draw the layer over the windows. `rgba` is LAYER_BYTES long; overwrite it,
    /// transparent (alpha 0) where nothing is drawn.
    fn draw_fx(&mut self, rgba: &mut [u8]);
    /// All live screens (including ones still waiting for a window).
    fn screens(&self) -> Vec<ScreenInfo>;
    /// Tell the game whether a window is attached to a screen (shows the
    /// "booting" panel when false). Also call with false when the app exits
    /// on its own.
    fn set_screen_occupied(&mut self, id: ScreenId, occupied: bool);
    /// Events since the last call.
    fn drain_events(&mut self) -> Vec<GameEvent>;

    /// Current view size in view pixels: (VIEW_W, VIEW_H) or (FAR_W, FAR_H).
    /// Layer buffers passed to draw_* must be `w * h * 4` bytes.
    fn view_size(&self) -> (u32, u32) {
        (VIEW_W, VIEW_H)
    }
    /// The installed apps the launcher offers. Indices are used by `LaunchApp`.
    fn set_app_catalog(&mut self, _apps: Vec<AppInfo>) {}
    /// True while a text field (launcher search) wants typed characters.
    /// The compositor then sends text via `text_input` instead of game keys
    /// (Esc/Enter/Backspace/arrows still arrive as keys).
    fn wants_text(&self) -> bool {
        false
    }
    fn text_input(&mut self, _text: &str) {}
}
