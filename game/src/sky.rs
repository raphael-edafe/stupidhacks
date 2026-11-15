//! Day/night clock and sky rendering. Owner: person B2.
//!
//! STUB: working clock + plain gradient. TODO(B2): stars, sun/moon arc, clouds,
//! parallax hills, dusk/dawn colour bands, "Night is coming" timing hooks.

use crate::camera::Camera;
use crate::gfx::{lerp_color, rgb, Canvas};
use crate::world::World;

pub const DAY_LEN: f32 = 40.0;
pub const NIGHT_LEN: f32 = 30.0;
/// Seconds of dusk/dawn fade at each end of the night.
pub const FADE: f32 = 4.0;

pub struct Sky {
    /// Seconds into the current day+night cycle. 0 = sunrise.
    pub clock: f32,
    /// How many nights have started (difficulty scaling).
    pub nights: u32,
}

impl Sky {
    pub fn new() -> Self {
        Sky { clock: 3.0, nights: 0 }
    }

    pub fn update(&mut self, dt: f32) {
        let was_night = self.is_night();
        self.clock += dt;
        if self.clock >= DAY_LEN + NIGHT_LEN {
            self.clock -= DAY_LEN + NIGHT_LEN;
        }
        if !was_night && self.is_night() {
            self.nights += 1;
        }
    }

    /// Demo knob: jump to a few seconds before nightfall.
    pub fn skip_to_dusk(&mut self) {
        if !self.is_night() {
            self.clock = DAY_LEN - FADE - 2.0;
        }
    }

    pub fn is_night(&self) -> bool {
        self.clock >= DAY_LEN
    }

    /// Seconds until night starts (0 during the night).
    pub fn until_night(&self) -> f32 {
        (DAY_LEN - self.clock).max(0.0)
    }

    /// 1.0 = full day, 0.0 = full night, smooth at dusk/dawn.
    pub fn daylight(&self) -> f32 {
        let t = self.clock;
        let k = if t < DAY_LEN - FADE {
            1.0
        } else if t < DAY_LEN {
            (DAY_LEN - t) / FADE
        } else if t < DAY_LEN + NIGHT_LEN - FADE {
            0.0
        } else {
            1.0 - (DAY_LEN + NIGHT_LEN - t) / FADE
        };
        k * k * (3.0 - 2.0 * k)
    }

    pub fn draw(&self, c: &mut Canvas, _cam: &Camera, _world: &World) {
        let d = self.daylight();
        let top = lerp_color(rgb(10, 12, 34), rgb(70, 140, 230), d);
        let bottom = lerp_color(rgb(40, 30, 70), rgb(170, 215, 250), d);
        for y in 0..c.h {
            let col = lerp_color(top, bottom, y as f32 / c.h as f32);
            c.fill_rect(0, y, c.w, 1, col);
        }
    }
}

impl Default for Sky {
    fn default() -> Self {
        Self::new()
    }
}
