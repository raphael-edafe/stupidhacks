//! In-world UI widgets: the app launcher panel (search field + scrolling list
//! over the installed-app catalog), key-cap prompts, app icons, CRT power
//! effects, easing. Owner: person B1. Everything draws in view px.

use crate::gfx::{lerp_color, rgb, rgba, shade, text_width, Canvas, Rgba};
use shared::{AppInfo, APPS};

pub const PANEL_W: i32 = 176;
pub const TITLE_H: i32 = 20;
/// Search field, relative to the panel top.
pub const SEARCH_TOP: i32 = TITLE_H + 5;
pub const SEARCH_H: i32 = 13;
pub const ROWS_TOP: i32 = SEARCH_TOP + SEARCH_H + 4;
pub const ROW_H: i32 = 18;
pub const VISIBLE_ROWS: usize = 6;
pub const FOOTER_H: i32 = 14;
/// Longest search query (chars).
pub const MAX_QUERY: usize = 24;

pub fn panel_h() -> i32 {
    ROWS_TOP + VISIBLE_ROWS as i32 * ROW_H + 3 + FOOTER_H
}

pub fn clamp01(t: f32) -> f32 {
    t.clamp(0.0, 1.0)
}

pub fn ease_out_back(t: f32) -> f32 {
    let t = clamp01(t);
    let c1 = 1.70158;
    let c3 = c1 + 1.0;
    1.0 + c3 * (t - 1.0).powi(3) + c1 * (t - 1.0).powi(2)
}

pub fn ease_in_out_cubic(t: f32) -> f32 {
    let t = clamp01(t);
    if t < 0.5 {
        4.0 * t * t * t
    } else {
        1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
    }
}

pub fn ease_out_cubic(t: f32) -> f32 {
    1.0 - (1.0 - clamp01(t)).powi(3)
}

/// Scale a colour's alpha by `a` (0..=1).
pub fn fade(c: Rgba, a: f32) -> Rgba {
    [c[0], c[1], c[2], (c[3] as f32 * clamp01(a)) as u8]
}

// ---------------------------------------------------------------------------
// App catalog helpers
// ---------------------------------------------------------------------------

/// Catalog ids drawn with the pxl icons, in `shared::APPS` order (which is the
/// order of `Sprites::app_icons`): Terminal, Files, Text Editor, Calculator,
/// System Monitor, Matrix.
const PXL_IDS: [&[&str]; 6] = [
    &["foot", "desktop-siege.terminal"],
    &["org.gnome.Nautilus"],
    &["org.gnome.TextEditor"],
    &["org.gnome.Calculator"],
    &["desktop-siege.system-monitor"],
    &["cmatrix"],
];

/// The catalog used until the compositor supplies one (and in tests): the
/// static `shared::APPS`, all favourites, in order.
pub fn fallback_catalog() -> Vec<AppInfo> {
    APPS.iter()
        .enumerate()
        .map(|(i, a)| AppInfo {
            id: PXL_IDS.get(i).map(|ids| ids[0].to_string()).unwrap_or_else(|| format!("app-{i}")),
            name: a.name.to_string(),
            icon: None,
            favourite: Some(i as u8),
        })
        .collect()
}

fn pxl_index(app: &AppInfo) -> Option<usize> {
    PXL_IDS.iter().position(|ids| ids.contains(&app.id.as_str()))
}

/// Display name in pixel-font caps.
pub fn app_label(apps: &[AppInfo], index: usize) -> String {
    apps.get(index).map(|a| a.name.to_uppercase()).unwrap_or_else(|| "APP".to_string())
}

/// Match quality of `query` (lowercase) against `name` (lowercase):
/// 0 = name prefix, 1 = start of a word, 2 = substring.
fn rank(name: &str, query: &str) -> Option<u8> {
    if name.starts_with(query) {
        return Some(0);
    }
    let mut prev_alnum = true;
    for (i, ch) in name.char_indices() {
        if !prev_alnum && ch.is_alphanumeric() && name[i..].starts_with(query) {
            return Some(1);
        }
        prev_alnum = ch.is_alphanumeric();
    }
    name.contains(query).then_some(2)
}

/// Launcher results (catalog indices). Empty query: favourites in order, then
/// everything else alphabetically. Otherwise ranked prefix > word start >
/// substring (case-insensitive), ties by name.
pub fn search(apps: &[AppInfo], query: &str) -> Vec<usize> {
    let q = query.trim().to_lowercase();
    let mut v: Vec<(u8, u8, String, usize)> = apps
        .iter()
        .enumerate()
        .filter_map(|(i, a)| {
            let name = a.name.to_lowercase();
            if q.is_empty() {
                Some((a.favourite.is_none() as u8, a.favourite.unwrap_or(0), name, i))
            } else {
                rank(&name, &q).map(|r| (r, 0, name, i))
            }
        })
        .collect();
    v.sort();
    v.into_iter().map(|t| t.3).collect()
}

// ---------------------------------------------------------------------------
// Icons
// ---------------------------------------------------------------------------

/// App icon (16x16) at full opacity.
pub fn draw_app_icon(c: &mut Canvas, apps: &[AppInfo], index: usize, x: i32, y: i32) {
    draw_app_icon_a(c, apps, index, x, y, 1.0);
}

/// pxl-made app icons (set once from `Sprites` at startup), in `shared::APPS` order.
static ICONS: std::sync::OnceLock<Vec<crate::gfx::Sprite>> = std::sync::OnceLock::new();

pub fn set_icons(icons: Vec<crate::gfx::Sprite>) {
    let _ = ICONS.set(icons);
}

/// App icon: the pxl icon for the favourites that have one, else the
/// catalog's own (system theme) icon, else a bevelled letter tile.
pub fn draw_app_icon_a(c: &mut Canvas, apps: &[AppInfo], index: usize, x: i32, y: i32, a: f32) {
    let a = clamp01(a);
    let app = apps.get(index);
    let pxl = app.and_then(pxl_index);
    if let Some(icon) = pxl.and_then(|p| ICONS.get().and_then(|v| v.get(p))) {
        for sy in 0..icon.h {
            for sx in 0..icon.w {
                let p = icon.get(sx, sy);
                if p[3] > 0 {
                    c.blend(x + sx, y + sy, [p[0], p[1], p[2], (p[3] as f32 * a) as u8]);
                }
            }
        }
        return;
    }
    if let Some(px) = app.and_then(|ap| ap.icon.as_deref()).filter(|b| b.len() >= 16 * 16 * 4) {
        for (i, p) in px.chunks_exact(4).take(256).enumerate() {
            if p[3] > 0 {
                c.blend(x + (i % 16) as i32, y + (i / 16) as i32, [p[0], p[1], p[2], (p[3] as f32 * a) as u8]);
            }
        }
        return;
    }
    const STYLE: [Rgba; 6] = [rgb(34, 150, 96), rgb(222, 160, 56), rgb(82, 132, 232), rgb(150, 98, 222), rgb(228, 86, 86), rgb(30, 190, 70)];
    const PALETTE: [Rgba; 8] = [
        rgb(52, 140, 200),
        rgb(214, 120, 60),
        rgb(70, 160, 110),
        rgb(170, 90, 200),
        rgb(200, 70, 100),
        rgb(60, 170, 170),
        rgb(190, 160, 50),
        rgb(110, 110, 210),
    ];
    let name = app.map(|ap| ap.name.as_str()).unwrap_or("?");
    let hash = name.bytes().fold(2166136261u32, |h, b| (h ^ b as u32).wrapping_mul(16777619));
    let body = pxl.and_then(|p| STYLE.get(p).copied()).unwrap_or(PALETTE[hash as usize % PALETTE.len()]);
    let letter = name.chars().find(|ch| ch.is_ascii_alphanumeric()).unwrap_or('?').to_ascii_uppercase();
    let outline = rgb(14, 16, 24);
    c.fill_rect(x + 1, y, 14, 16, fade(outline, a));
    c.fill_rect(x, y + 1, 16, 14, fade(outline, a));
    c.fill_rect(x + 1, y + 1, 14, 14, fade(body, a));
    // Bevel: light top-left, dark bottom-right, glossy top half.
    c.fill_rect(x + 1, y + 1, 14, 1, fade(shade(body, 1.45), a));
    c.fill_rect(x + 1, y + 2, 1, 12, fade(shade(body, 1.25), a));
    c.fill_rect(x + 1, y + 14, 14, 1, fade(shade(body, 0.6), a));
    c.fill_rect(x + 14, y + 2, 1, 12, fade(shade(body, 0.72), a));
    c.fill_rect(x + 2, y + 2, 12, 5, fade(rgba(255, 255, 255, 34), a));
    let mut buf = [0u8; 4];
    let s = letter.encode_utf8(&mut buf);
    c.text(x + 6, y + 4, s, 2, fade(rgba(10, 12, 20, 150), a));
    c.text(x + 5, y + 3, s, 2, fade(rgb(250, 252, 255), a));
}

/// `s` cut down (with "..") to fit `max_w` pixels at scale 1.
pub fn ellipsize(s: &str, max_w: i32) -> String {
    if text_width(s, 1) <= max_w {
        return s.to_string();
    }
    let mut out: String = s.to_string();
    while !out.is_empty() && text_width(&format!("{}..", out.trim_end()), 1) > max_w {
        out.pop();
    }
    format!("{}..", out.trim_end())
}

/// Keyboard key cap with a label (e.g. "E", "ESC"). Returns its width.
pub fn draw_keycap(c: &mut Canvas, x: i32, y: i32, key: &str, a: f32) -> i32 {
    draw_keycap_s(c, x, y, key, a, 1)
}

/// Key cap at pixel scale `sc` (1 = 11 px tall).
pub fn draw_keycap_s(c: &mut Canvas, x: i32, y: i32, key: &str, a: f32, sc: i32) -> i32 {
    let sc = sc.max(1);
    let w = (text_width(key, sc) + 6 * sc).max(9 * sc);
    let h = 11 * sc;
    let outline = rgb(16, 18, 28);
    c.fill_rect(x + sc, y, w - 2 * sc, h, fade(outline, a));
    c.fill_rect(x, y + sc, w, h - 2 * sc, fade(outline, a));
    c.fill_rect(x + sc, y + sc, w - 2 * sc, h - 2 * sc, fade(rgb(132, 140, 164), a)); // lip
    c.fill_rect(x + sc, y + sc, w - 2 * sc, h - 4 * sc, fade(rgb(232, 236, 246), a)); // top face
    c.fill_rect(x + sc, y + sc, w - 2 * sc, sc, fade(rgb(255, 255, 255), a));
    c.text(x + (w - text_width(key, sc)) / 2, y + 2 * sc, key, sc, fade(rgb(30, 34, 52), a));
    w
}

/// Pill-shaped prompt: [KEY] LABEL  [KEY] LABEL, centred on `cx`.
pub fn draw_prompt(c: &mut Canvas, cx: i32, y: i32, items: &[(&str, &str)], accent: Rgba, a: f32) {
    if a <= 0.01 || items.is_empty() {
        return;
    }
    let gap = 10;
    let item_w = |(k, l): &(&str, &str)| (text_width(k, 1) + 6).max(9) + 4 + text_width(l, 1);
    let total: i32 = items.iter().map(item_w).sum::<i32>() + gap * (items.len() as i32 - 1);
    let (pw, ph) = (total + 12, 17);
    let px = cx - pw / 2;
    // Shadow, body, outline.
    c.fill_rect(px + 2, y + 2, pw, ph, fade(rgba(0, 0, 0, 90), a));
    c.fill_rect(px + 1, y, pw - 2, ph, fade(rgba(14, 16, 28, 225), a));
    c.fill_rect(px, y + 1, pw, ph - 2, fade(rgba(14, 16, 28, 225), a));
    c.fill_rect(px + 1, y, pw - 2, 1, fade(accent, a * 0.8));
    c.fill_rect(px + 1, y + ph - 1, pw - 2, 1, fade(shade(accent, 0.5), a * 0.8));
    c.fill_rect(px, y + 1, 1, ph - 2, fade(shade(accent, 0.7), a * 0.8));
    c.fill_rect(px + pw - 1, y + 1, 1, ph - 2, fade(shade(accent, 0.7), a * 0.8));
    // Little pointer under the pill.
    c.fill_rect(cx - 2, y + ph, 5, 1, fade(rgba(14, 16, 28, 225), a));
    c.fill_rect(cx - 1, y + ph + 1, 3, 1, fade(rgba(14, 16, 28, 225), a));
    c.put(cx, y + ph + 2, fade(rgba(14, 16, 28, 225), a));
    let mut x = px + 6;
    for it in items {
        let kw = draw_keycap(c, x, y + 3, it.0, a);
        c.text_shadow(x + kw + 4, y + 6, it.1, 1, fade(rgb(240, 244, 255), a));
        x += item_w(it) + gap;
    }
}

/// What the launcher panel shows this frame.
pub struct LauncherView<'a> {
    pub apps: &'a [AppInfo],
    /// Catalog indices, in display order (see `search`).
    pub results: &'a [usize],
    pub query: &'a str,
    /// Highlighted position in `results`.
    pub hover: usize,
    /// First visible position in `results`.
    pub scroll: usize,
}

impl LauncherView<'_> {
    fn has_bar(&self) -> bool {
        self.results.len() > VISIBLE_ROWS
    }
}

/// The "OPEN APP" launcher panel. `rect` is the final panel rect; `scale`
/// animates the frame size around its centre; `alpha` fades everything;
/// `content` fades the search field and rows in after the frame pops.
#[allow(clippy::too_many_arguments)]
pub fn draw_chooser(c: &mut Canvas, rect: (i32, i32, i32, i32), scale: f32, alpha: f32, content: f32, view: &LauncherView, time: f32) {
    let (x, y, w, h) = rect;
    let (cx, cy) = (x + w / 2, y + h / 2);
    let sw = ((w as f32 * scale) as i32).max(8);
    let sh = ((h as f32 * scale) as i32).max(8);
    let (fx, fy) = (cx - sw / 2, cy - sh / 2);
    let a = alpha;

    let outline = rgb(10, 12, 20);
    let accent = rgb(110, 228, 255);
    // Drop shadow.
    c.fill_rect(fx + 4, fy + 4, sw, sh, fade(rgba(0, 0, 0, 100), a));
    // Outline with clipped corners.
    c.fill_rect(fx + 1, fy, sw - 2, sh, fade(outline, a));
    c.fill_rect(fx, fy + 1, sw, sh - 2, fade(outline, a));
    // Body gradient.
    let title_h = ((TITLE_H as f32) * scale) as i32;
    for yy in 1..sh - 1 {
        let t = yy as f32 / sh as f32;
        let col = if yy < title_h {
            lerp_color(rgb(60, 112, 176), rgb(38, 76, 132), yy as f32 / title_h.max(1) as f32)
        } else {
            lerp_color(rgb(32, 38, 58), rgb(20, 23, 36), t)
        };
        c.fill_rect(fx + 1, fy + yy, sw - 2, 1, fade(col, a));
    }
    // Bevels.
    c.fill_rect(fx + 1, fy + 1, sw - 2, 1, fade(rgb(132, 186, 240), a));
    c.fill_rect(fx + 1, fy + 2, 1, sh - 4, fade(rgb(70, 84, 118), a));
    c.fill_rect(fx + sw - 2, fy + 2, 1, sh - 4, fade(rgb(14, 16, 26), a));
    c.fill_rect(fx + 1, fy + sh - 2, sw - 2, 1, fade(rgb(14, 16, 26), a));
    // Accent line under the title bar (glows).
    c.fill_rect(fx + 2, fy + title_h, sw - 4, 1, fade(accent, a));
    c.fill_rect(fx + 2, fy + title_h + 1, sw - 4, 1, fade(rgba(110, 228, 255, 70), a));
    // Corner rivets.
    for (rx, ry) in [(fx + 4, fy + 4), (fx + sw - 6, fy + 4), (fx + 4, fy + sh - 6), (fx + sw - 6, fy + sh - 6)] {
        c.fill_rect(rx, ry, 2, 2, fade(rgb(16, 18, 28), a));
        c.put(rx, ry, fade(rgb(170, 190, 220), a));
    }

    // Title.
    let ca = a * clamp01(content * 1.5);
    if ca > 0.0 {
        let title = "OPEN APP";
        let tw = text_width(title, 2);
        c.text(cx - tw / 2 + 1, fy + 5 + 1, title, 2, fade(rgba(8, 14, 30, 200), ca));
        c.text(cx - tw / 2, fy + 5, title, 2, fade(rgb(250, 252, 255), ca));
        draw_search_field(c, rect, view, ca, time);
    }

    // Rows.
    let ra = a * content;
    if ra > 0.0 {
        let bar = view.has_bar();
        for vis in 0..VISIBLE_ROWS {
            let pos = view.scroll + vis;
            let Some(&idx) = view.results.get(pos) else { break };
            let (rx, ry, rw, rh) = row_rect(rect, vis, bar);
            // Staggered slide-in.
            let k = clamp01(content * 1.6 - vis as f32 * 0.12);
            if k <= 0.0 {
                continue;
            }
            let off = ((1.0 - ease_out_cubic(k)) * 10.0) as i32;
            let rx = rx + off;
            let a2 = ra * k;
            let hovered = view.hover == pos;
            if hovered {
                let pulse = 0.75 + 0.25 * (time * 6.0).sin();
                c.fill_rect(rx, ry, rw, rh, fade(rgba(110, 228, 255, 46), a2 * pulse));
                c.fill_rect(rx, ry, rw, 1, fade(accent, a2));
                c.fill_rect(rx, ry + rh - 1, rw, 1, fade(accent, a2));
                c.fill_rect(rx, ry + 1, 1, rh - 2, fade(accent, a2));
                c.fill_rect(rx + rw - 1, ry + 1, 1, rh - 2, fade(accent, a2));
            } else if pos % 2 == 1 {
                c.fill_rect(rx, ry, rw, rh, fade(rgba(255, 255, 255, 8), a2));
            }
            // Number keys pick the favourites while the search is empty.
            let fav = view.apps.get(idx).and_then(|ap| ap.favourite).filter(|&f| (f as usize) < 6 && view.query.is_empty());
            if let Some(f) = fav {
                draw_keycap(c, rx + 3, ry + (rh - 11) / 2, &(f + 1).to_string(), a2);
            }
            let bob = if hovered { ((time * 8.0).sin() * 1.0).round() as i32 } else { 0 };
            draw_app_icon_a(c, view.apps, idx, rx + 16, ry + (rh - 16) / 2 + bob, a2);
            let name = ellipsize(&app_label(view.apps, idx), rw - 36 - 10);
            let col = if hovered { rgb(255, 255, 255) } else { rgb(178, 190, 214) };
            c.text_shadow(rx + 36, ry + (rh - 5) / 2, &name, 1, fade(col, a2));
            if hovered {
                let nudge = ((time * 7.0).sin() * 1.5) as i32;
                c.text(rx + rw - 8 + nudge, ry + (rh - 5) / 2, ">", 1, fade(accent, a2));
            }
        }
        if view.results.is_empty() {
            let my = y + ROWS_TOP + (VISIBLE_ROWS as i32 * ROW_H) / 2 - 8;
            c.text_centered(cx, my, "NO APPS MATCH", 1, fade(rgb(255, 196, 120), ra));
            c.text_centered(cx, my + 10, "BACKSPACE TO EDIT", 1, fade(rgb(110, 124, 156), ra));
        }
        // Scrollbar.
        if bar {
            let n = view.results.len() as i32;
            let (tx, ty, th) = (x + w - 9, y + ROWS_TOP, VISIBLE_ROWS as i32 * ROW_H - 2);
            c.fill_rect(tx, ty, 3, th, fade(rgba(0, 0, 0, 110), ra));
            let kh = (th * VISIBLE_ROWS as i32 / n).max(6);
            let ky = ty + (th - kh) * view.scroll as i32 / (n - VISIBLE_ROWS as i32).max(1);
            c.fill_rect(tx, ky, 3, kh, fade(rgb(120, 170, 220), ra));
            c.fill_rect(tx, ky, 3, 1, fade(rgb(200, 235, 255), ra));
        }
        let foot = "ENTER: OPEN   ESC: CANCEL";
        c.text_centered(cx, y + h - FOOTER_H + 3, foot, 1, fade(rgb(110, 124, 156), ra));
    }

    // Pop flash: bright outline that fades as the panel settles.
    let flash = clamp01(1.0 - content * 2.0) * a;
    if flash > 0.0 {
        c.rect_outline(fx, fy, sw, sh, fade(rgb(200, 245, 255), flash));
    }
}

/// Search field: inset box, magnifier, query (or placeholder) and a blinking caret.
fn draw_search_field(c: &mut Canvas, rect: (i32, i32, i32, i32), view: &LauncherView, a: f32, time: f32) {
    let (x, y, w, _) = rect;
    let (sx, sy, sw, sh) = (x + 6, y + SEARCH_TOP, w - 12, SEARCH_H);
    let accent = rgb(110, 228, 255);
    c.fill_rect(sx, sy, sw, sh, fade(rgb(9, 11, 20), a));
    c.fill_rect(sx, sy, sw, 1, fade(rgb(4, 5, 10), a));
    c.fill_rect(sx, sy + sh - 1, sw, 1, fade(rgb(70, 84, 118), a));
    let glow = 0.35 + 0.15 * (time * 3.0).sin();
    c.rect_outline(sx - 1, sy - 1, sw + 2, sh + 2, fade(accent, a * glow));
    // Magnifier: 5x5 ring + handle.
    let (mx, my) = (sx + 3, sy + 3);
    let ring = fade(rgb(150, 200, 235), a);
    c.fill_rect(mx + 1, my, 3, 1, ring);
    c.fill_rect(mx + 1, my + 4, 3, 1, ring);
    c.fill_rect(mx, my + 1, 1, 3, ring);
    c.fill_rect(mx + 4, my + 1, 1, 3, ring);
    c.put(mx + 5, my + 5, ring);
    c.put(mx + 6, my + 6, ring);
    let tx = sx + 13;
    let ty = sy + (sh - 5) / 2;
    let caret_on = (time * 2.2).fract() < 0.6;
    let count = if view.query.is_empty() { String::new() } else { view.results.len().to_string() };
    let count_w = if count.is_empty() { 0 } else { text_width(&count, 1) + 4 };
    if view.query.is_empty() {
        c.text(tx + 3, ty, "TYPE TO SEARCH", 1, fade(rgb(92, 104, 136), a));
        if caret_on {
            c.fill_rect(tx, ty - 1, 1, 7, fade(accent, a));
        }
    } else {
        let avail = sw - 13 - 6 - count_w;
        let mut shown = view.query.to_uppercase();
        while !shown.is_empty() && text_width(&shown, 1) > avail {
            shown.remove(0);
        }
        c.text(tx, ty, &shown, 1, fade(rgb(245, 250, 255), a));
        let cx = tx + text_width(&shown, 1) + 1;
        if caret_on {
            c.fill_rect(cx, ty - 1, 1, 7, fade(accent, a));
        }
        c.text(sx + sw - count_w, ty, &count, 1, fade(rgb(92, 104, 136), a));
    }
}

/// Row hit box inside a launcher panel rect (`vis` = visible row 0..6).
/// Rows leave room on the right for the scrollbar when there is one.
pub fn row_rect(rect: (i32, i32, i32, i32), vis: usize, bar: bool) -> (i32, i32, i32, i32) {
    let (x, y, w, _) = rect;
    (x + 6, y + ROWS_TOP + vis as i32 * ROW_H, w - 12 - if bar { 6 } else { 0 }, ROW_H - 2)
}

/// CRT power effect over an interior rect. `p` = 0..1 progress.
/// `on == false`: picture collapses to a line, then a dot, then fades.
/// `on == true`: a dot stretches into a line and opens up to reveal the window.
pub fn draw_crt(c: &mut Canvas, x: i32, y: i32, w: i32, h: i32, p: f32, on: bool) {
    let p = clamp01(p);
    let (cx, cy) = (x + w / 2, y + h / 2);
    let black = rgb(4, 6, 10);
    let white = rgb(240, 252, 255);
    if !on {
        if p < 0.45 {
            let k = p / 0.45;
            let bh = ((h as f32) * (1.0 - k * k)).max(2.0) as i32;
            let by = cy - bh / 2;
            c.fill_rect(x, y, w, by - y, black);
            c.fill_rect(x, by + bh, w, y + h - by - bh, black);
            // The squashed picture: brightening wash.
            c.fill_rect(x, by, w, bh, fade(lerp_color(rgb(120, 200, 230), white, k), 0.35 + 0.65 * k));
            c.fill_rect(x, by, w, 1, white);
            c.fill_rect(x, by + bh - 1, w, 1, white);
        } else if p < 0.75 {
            let k = (p - 0.45) / 0.3;
            c.fill_rect(x, y, w, h, black);
            let lw = ((w as f32) * (1.0 - k * k)).max(2.0) as i32;
            c.fill_rect(cx - lw / 2, cy - 1, lw, 2, white);
            c.fill_rect(cx - lw / 2, cy - 2, lw, 1, rgba(150, 230, 255, 110));
            c.fill_rect(cx - lw / 2, cy + 1, lw, 1, rgba(150, 230, 255, 110));
            c.glow(cx, cy, 6, rgba(160, 235, 255, 160));
        } else {
            let k = (p - 0.75) / 0.25;
            c.fill_rect(x, y, w, h, fade(black, 1.0 - k));
            let r = (5.0 * (1.0 - k)) as i32 + 1;
            c.glow(cx, cy, r + 4, rgba(170, 240, 255, (200.0 * (1.0 - k)) as u8));
            c.fill_rect(cx - 1, cy - 1, 2, 2, fade(white, 1.0 - k));
        }
    } else if p < 0.3 {
        let k = p / 0.3;
        c.fill_rect(x, y, w, h, black);
        let lw = ((w as f32) * ease_out_cubic(k)).max(2.0) as i32;
        c.fill_rect(cx - lw / 2, cy - 1, lw, 2, white);
        c.glow(cx, cy, 5, rgba(160, 235, 255, 150));
    } else {
        let k = ease_out_cubic((p - 0.3) / 0.7);
        let bh = ((h as f32) * k).max(2.0) as i32;
        let by = cy - bh / 2;
        c.fill_rect(x, y, w, by - y, black);
        c.fill_rect(x, by + bh, w, y + h - by - bh, black);
        c.fill_rect(x, by, w, bh, fade(white, (1.0 - k) * 0.6));
        let edge = fade(white, 1.0 - k * 0.7);
        c.fill_rect(x, by, w, 1, edge);
        c.fill_rect(x, by + bh - 1, w, 1, edge);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Game;
    use shared::{GameApi, GameEvent, InputState, Key};

    fn app(id: &str, name: &str, fav: Option<u8>) -> AppInfo {
        AppInfo { id: id.into(), name: name.into(), icon: None, favourite: fav }
    }

    fn catalog() -> Vec<AppInfo> {
        vec![
            app("firefox_firefox", "Firefox", Some(0)),
            app("foot", "Terminal (foot)", Some(1)),
            app("org.gnome.Calculator", "Calculator", Some(2)),
            app("libreoffice-calc", "LibreOffice Calc", None),
            app("org.gnome.Settings", "Settings", None),
            app("a.firewall", "Firewall Config", None),
            app("x.wildfire", "Wildfire", None),
            app("b.backup", "Backups", None),
            app("e.app", "Easy Fire Starter", None),
        ]
    }

    fn names(apps: &[AppInfo], r: &[usize]) -> Vec<String> {
        r.iter().map(|&i| apps[i].name.clone()).collect()
    }

    #[test]
    fn empty_query_lists_favourites_then_alphabetical() {
        let c = catalog();
        let r = search(&c, "");
        assert_eq!(r.len(), c.len());
        assert_eq!(names(&c, &r[..4]), ["Firefox", "Terminal (foot)", "Calculator", "Backups"]);
        assert_eq!(names(&c, &r[r.len() - 1..]), ["Wildfire"]);
    }

    #[test]
    fn query_ranks_prefix_then_word_start_then_substring() {
        let c = catalog();
        // prefix: Firefox, Firewall Config (by name); word start: Easy Fire Starter; substring: Wildfire.
        assert_eq!(names(&c, &search(&c, "FiRe")), ["Firefox", "Firewall Config", "Easy Fire Starter", "Wildfire"]);
        assert_eq!(names(&c, &search(&c, "calc")), ["Calculator", "LibreOffice Calc"]);
        assert_eq!(names(&c, &search(&c, "foot")), ["Terminal (foot)"]);
        assert!(search(&c, "zzz").is_empty());
    }

    #[test]
    fn ellipsize_fits() {
        let s = ellipsize("A VERY LONG APPLICATION NAME INDEED", 60);
        assert!(text_width(&s, 1) <= 60 && s.ends_with(".."));
        assert_eq!(ellipsize("SHORT", 60), "SHORT");
    }

    struct D {
        g: Game,
        input: InputState,
        events: Vec<GameEvent>,
    }
    impl D {
        fn new() -> D {
            let mut g = Game::new(crate::DEFAULT_SEED);
            g.set_app_catalog(catalog());
            let mut input = InputState::default();
            input.game_focused = true;
            let mut d = D { g, input, events: Vec::new() };
            let px = (d.g.player.center().0 / crate::world::TILE as f32) as i32;
            let ty = d.g.world.surface[px as usize] - 9;
            d.g.screens.spawn(px + 1, ty, 14, 8);
            d.steps(90);
            d
        }
        fn step(&mut self) {
            self.g.update(1.0 / 60.0, &self.input);
            self.input.wheel = 0.0;
            self.events.extend(self.g.drain_events());
        }
        fn steps(&mut self, n: usize) {
            for _ in 0..n {
                self.step();
            }
        }
        fn tap(&mut self, k: Key) {
            self.input.set(k, true);
            self.step();
            self.input.set(k, false);
            self.step();
        }
        fn launched(&self) -> Vec<usize> {
            self.events.iter().filter_map(|e| if let GameEvent::LaunchApp { app, .. } = e { Some(*app) } else { None }).collect()
        }
    }

    #[test]
    fn launcher_search_type_backspace_enter() {
        let mut d = D::new();
        assert!(d.g.wants_text(), "launcher open after building");
        d.g.text_input("fire");
        assert_eq!(d.g.screens.launcher_query(), "fire");
        let first = d.g.screens.launcher_results()[0];
        assert_eq!(catalog()[first].name, "Firefox");
        // Number keys don't pick favourites while there's a query.
        d.tap(Key::Hotbar2);
        assert!(d.launched().is_empty());
        for _ in 0..4 {
            d.tap(Key::Backspace);
        }
        assert_eq!(d.g.screens.launcher_query(), "");
        d.g.text_input("calc");
        d.tap(Key::Down); // -> LibreOffice Calc
        d.tap(Key::Enter);
        assert_eq!(d.launched(), [3]);
        d.steps(20);
        assert!(!d.g.wants_text(), "launcher closed");
        // The launching panel names the app.
        let s = d.g.screens.list.iter().find(|s| s.launching.is_some()).unwrap();
        assert_eq!(app_label(&d.g.screens.catalog, s.app.unwrap()), "LIBREOFFICE CALC");
    }

    #[test]
    fn launcher_hotbar_and_digit_text_on_empty_query() {
        let mut d = D::new();
        // A digit typed into an empty query is left to the Hotbar key.
        d.g.text_input("3");
        assert_eq!(d.g.screens.launcher_query(), "");
        d.tap(Key::Hotbar3);
        assert_eq!(d.launched(), [2]); // Calculator (favourite 3)
    }

    #[test]
    fn launcher_wheel_scrolls_and_esc_cancels() {
        let mut d = D::new();
        for _ in 0..8 {
            d.input.wheel = -1.0;
            d.step();
        }
        let ch = d.g.screens.chooser.unwrap();
        assert_eq!(ch.hover, 8); // clamped to the last result
        assert!(d.g.screens.launcher_scroll() >= 3, "scrolled to keep the highlight visible");
        d.g.text_input("zzz");
        assert!(d.g.screens.launcher_results().is_empty());
        d.tap(Key::Enter); // nothing to launch
        assert!(d.launched().is_empty());
        d.tap(Key::Escape);
        d.steps(20);
        assert!(d.g.screens.chooser.is_none() && !d.g.wants_text());
        assert!(d.launched().is_empty());
        // Typing after the launcher closed does nothing.
        d.g.text_input("x");
        assert_eq!(d.g.screens.launcher_query(), "zzz");
    }
}
