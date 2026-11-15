//! HUD: hotbar, hearts, armour, clock. Owner: person B2.
//!
//! Top left: the 6-slot hotbar (keys 1-6) with item icons and stack counts,
//! hearts (2 HP each) and, when wearing armour, a shield with the total
//! defence. The held item's name shows for a moment after switching.
//! Top right: the day/night clock. Hidden with F1.

use crate::gfx::{rgb, rgba, text_width, with_alpha, Canvas, Rgba, Sprite};
use crate::player::HOTBAR_SLOTS;
use crate::Game;
use std::sync::OnceLock;

/// Slot size and spacing (view px).
const SLOT: i32 = 22;
const GAP: i32 = 2;

const HEART: [&str; 8] = [
    "..oo.oo..",
    ".orrorro.",
    "orhrrrrro",
    "orrrrrrRo",
    ".orrrrRo.",
    "..orrRo..",
    "...oRo...",
    "....o....",
];
const SHIELD: [&str; 8] = [
    ".oooooo.",
    "osssssso",
    "osSsssso",
    "osSsssso",
    "osssssSo",
    ".osssSo.",
    "..osSo..",
    "...oo...",
];

struct Art {
    heart: Sprite,
    heart_empty: Sprite,
    shield: Sprite,
}

fn art() -> &'static Art {
    static ART: OnceLock<Art> = OnceLock::new();
    ART.get_or_init(|| {
        let o: (char, Rgba) = ('o', rgb(20, 18, 28));
        Art {
            heart: Sprite::from_ascii(&HEART, &[o, ('r', rgb(232, 64, 72)), ('R', rgb(160, 32, 48)), ('h', rgb(255, 196, 196))]),
            heart_empty: Sprite::from_ascii(&HEART, &[o, ('r', rgb(58, 52, 70)), ('R', rgb(40, 36, 50)), ('h', rgb(80, 74, 94))]),
            shield: Sprite::from_ascii(&SHIELD, &[o, ('s', rgb(176, 186, 214)), ('S', rgb(244, 246, 255))]),
        }
    })
}

pub fn draw(c: &mut Canvas, g: &Game) {
    let p = &g.player;
    let a = art();
    let (x0, y0) = (8, 8);

    // --- hotbar ---
    for i in 0..HOTBAR_SLOTS {
        let sel = i == p.selected;
        let x = x0 + i as i32 * (SLOT + GAP);
        let y = if sel { y0 - 1 } else { y0 };
        c.fill_rect(x, y, SLOT, SLOT, if sel { rgba(52, 60, 98, 225) } else { rgba(16, 20, 34, 190) });
        if sel {
            c.rect_outline(x - 1, y - 1, SLOT + 2, SLOT + 2, rgb(140, 100, 28));
        }
        c.rect_outline(x, y, SLOT, SLOT, if sel { rgb(255, 214, 90) } else { rgb(70, 80, 112) });
        c.text(x + 2, y + 2, &(i + 1).to_string(), 1, rgba(150, 160, 190, 150));
        if let Some(it) = p.hotbar[i] {
            let spr = g.sprites.icon(it);
            c.sprite(spr, x + SLOT / 2 - spr.w / 2, y + SLOT / 2 - spr.h / 2, false);
            let n = p.count(it);
            if !it.is_tool() && !it.unlimited() && (n > 1 || p.god) {
                let s = if p.god { "*".to_string() } else { n.to_string() };
                c.text_shadow(x + SLOT - 1 - text_width(&s, 1), y + SLOT - 7, &s, 1, rgb(255, 255, 255));
            }
        }
    }

    // --- hearts (2 HP each) and armour ---
    let hy = y0 + SLOT + 5;
    let hearts = (p.max_hp + 1) / 2;
    for i in 0..hearts {
        let hx = x0 + i * 10;
        c.sprite(&a.heart_empty, hx, hy, false);
        match (p.hp - i * 2).clamp(0, 2) {
            2 => c.sprite(&a.heart, hx, hy, false),
            1 => {
                // Left half of a full heart.
                for sy in 0..a.heart.h {
                    for sx in 0..a.heart.w / 2 + 1 {
                        let px = a.heart.get(sx, sy);
                        if px[3] > 0 {
                            c.blend(hx + sx, hy + sy, px);
                        }
                    }
                }
            }
            _ => {}
        }
    }
    let def = p.defense();
    if def > 0 {
        let sx = x0 + hearts * 10 + 4;
        c.sprite(&a.shield, sx, hy, false);
        c.text_shadow(sx + 11, hy + 1, &def.to_string(), 1, rgb(220, 230, 255));
    }

    // --- held item name, fading out ---
    if let Some(it) = p.held() {
        let fade = if p.select_age < 1.4 { 1.0 } else { (1.0 - (p.select_age - 1.4) / 0.5).max(0.0) };
        if fade > 0.0 {
            c.text_shadow(x0, hy + 12, it.name(), 1, with_alpha(rgb(255, 240, 200), (fade * 255.0) as u8));
        }
    }

    // --- clock ---
    let label = if g.sky.is_night() {
        format!("NIGHT {}", g.sky.nights)
    } else {
        format!("DAY  NIGHT IN {:.0}S", g.sky.until_night())
    };
    c.text_shadow(c.w - text_width(&label, 1) - 8, 8, &label, 1, rgb(255, 255, 255));
    if g.god {
        c.text_shadow(c.w - 40, 18, "GOD", 1, rgb(255, 220, 90));
    }
}
