//! HUD: hotbar, hearts, clock, banners. Owner: person B2.
//!
//! STUB: plain hotbar + clock text. TODO(B2): pixel-art hotbar slots with item
//! icons, hearts, day/night clock dial, big "NIGHT IS COMING" / "DAWN" banners,
//! build-mode hint, FPS counter (hidden by F1).

use crate::gfx::{rgb, rgba, Canvas};
use crate::player::HOTBAR;
use crate::Game;

pub fn draw(c: &mut Canvas, g: &Game) {
    // Hotbar.
    for (i, t) in HOTBAR.iter().enumerate() {
        let x = 8 + i as i32 * 22;
        let y = 8;
        let sel = i == g.player.selected;
        c.fill_rect(x, y, 20, 20, rgba(20, 24, 40, 190));
        c.rect_outline(x, y, 20, 20, if sel { rgb(255, 220, 90) } else { rgb(90, 100, 130) });
        c.fill_rect(x + 5, y + 4, 10, 10, t.color());
        c.text_shadow(x + 2, y + 14, &g.player.inventory[*t as usize].to_string(), 1, rgb(255, 255, 255));
    }
    // HP.
    c.text_shadow(8, 32, &format!("HP {}/{}", g.player.hp, g.player.max_hp), 1, rgb(255, 120, 120));
    // Clock.
    let label = if g.sky.is_night() {
        format!("NIGHT {}", g.sky.nights)
    } else {
        format!("DAY  NIGHT IN {:.0}S", g.sky.until_night())
    };
    c.text_shadow(c.w - crate::gfx::text_width(&label, 1) - 8, 8, &label, 1, rgb(255, 255, 255));
    if g.god {
        c.text_shadow(c.w - 40, 18, "GOD", 1, rgb(255, 220, 90));
    }
}
