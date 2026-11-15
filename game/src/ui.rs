//! In-world UI widgets: app chooser panel, key-cap prompts, app icons, CRT
//! power effects, easing. Owner: person B1. Everything draws in view px.

use crate::gfx::{lerp_color, rgb, rgba, shade, text_width, Canvas, Rgba};
use shared::APPS;

pub const PANEL_W: i32 = 152;
pub const TITLE_H: i32 = 20;
pub const ROW_H: i32 = 20;
pub const FOOTER_H: i32 = 14;
pub const ROWS_TOP: i32 = TITLE_H + 5;

pub fn panel_h() -> i32 {
    ROWS_TOP + APPS.len() as i32 * ROW_H + 3 + FOOTER_H
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

/// Display name in pixel-font caps.
pub fn app_label(index: usize) -> String {
    APPS.get(index).map(|a| a.name.to_ascii_uppercase()).unwrap_or_else(|| "APP".to_string())
}

/// Placeholder app icon: a 16x16 bevelled coloured tile with a letter.
/// (Pixel-art icons from the sprite pass will replace the body of this function.)
pub fn draw_app_icon(c: &mut Canvas, index: usize, x: i32, y: i32) {
    draw_app_icon_a(c, index, x, y, 1.0);
}

pub fn draw_app_icon_a(c: &mut Canvas, index: usize, x: i32, y: i32, a: f32) {
    const STYLE: [(Rgba, char); 6] = [
        (rgb(34, 150, 96), 'T'),
        (rgb(222, 160, 56), 'F'),
        (rgb(82, 132, 232), 'E'),
        (rgb(150, 98, 222), 'C'),
        (rgb(228, 86, 86), 'S'),
        (rgb(30, 190, 70), 'M'),
    ];
    let (body, letter) = STYLE.get(index).copied().unwrap_or((
        rgb(120, 120, 140),
        APPS.get(index).and_then(|ap| ap.name.chars().next()).unwrap_or('?'),
    ));
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

/// The "OPEN APP" chooser panel. `rect` is the final panel rect; `scale`
/// animates the frame size around its centre; `alpha` fades everything;
/// `content` fades the rows in after the frame pops.
#[allow(clippy::too_many_arguments)]
pub fn draw_chooser(c: &mut Canvas, rect: (i32, i32, i32, i32), scale: f32, alpha: f32, content: f32, hover: Option<usize>, time: f32) {
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
    }

    // Rows.
    let ra = a * content;
    if ra > 0.0 {
        for i in 0..APPS.len() {
            let (rx, ry, rw, rh) = row_rect(rect, i);
            // Staggered slide-in.
            let k = clamp01(content * 1.6 - i as f32 * 0.12);
            if k <= 0.0 {
                continue;
            }
            let off = ((1.0 - ease_out_cubic(k)) * 10.0) as i32;
            let rx = rx + off;
            let a2 = ra * k;
            let hovered = hover == Some(i);
            if hovered {
                let pulse = 0.75 + 0.25 * (time * 6.0).sin();
                c.fill_rect(rx, ry, rw, rh, fade(rgba(110, 228, 255, 46), a2 * pulse));
                c.fill_rect(rx, ry, rw, 1, fade(accent, a2));
                c.fill_rect(rx, ry + rh - 1, rw, 1, fade(accent, a2));
                c.fill_rect(rx, ry + 1, 1, rh - 2, fade(accent, a2));
                c.fill_rect(rx + rw - 1, ry + 1, 1, rh - 2, fade(accent, a2));
            } else if i % 2 == 1 {
                c.fill_rect(rx, ry, rw, rh, fade(rgba(255, 255, 255, 8), a2));
            }
            let key = (i + 1).to_string();
            draw_keycap(c, rx + 3, ry + (rh - 11) / 2, &key, a2);
            let bob = if hovered { ((time * 8.0).sin() * 1.0).round() as i32 } else { 0 };
            draw_app_icon_a(c, i, rx + 17, ry + (rh - 16) / 2 + bob, a2);
            let name = app_label(i);
            let col = if hovered { rgb(255, 255, 255) } else { rgb(178, 190, 214) };
            c.text_shadow(rx + 38, ry + (rh - 5) / 2, &name, 1, fade(col, a2));
            if hovered {
                let nudge = ((time * 7.0).sin() * 1.5) as i32;
                c.text(rx + rw - 9 + nudge, ry + (rh - 5) / 2, ">", 1, fade(accent, a2));
            }
        }
        let foot = "RMB/ESC: CANCEL";
        c.text_centered(cx, y + h - FOOTER_H + 3, foot, 1, fade(rgb(110, 124, 156), ra));
    }

    // Pop flash: bright outline that fades as the panel settles.
    let flash = clamp01(1.0 - content * 2.0) * a;
    if flash > 0.0 {
        c.rect_outline(fx, fy, sw, sh, fade(rgb(200, 245, 255), flash));
    }
}

/// Row hit box inside a chooser panel rect.
pub fn row_rect(rect: (i32, i32, i32, i32), i: usize) -> (i32, i32, i32, i32) {
    let (x, y, w, _) = rect;
    (x + 6, y + ROWS_TOP + i as i32 * ROW_H, w - 12, ROW_H - 2)
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
