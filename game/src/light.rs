//! Lighting: darkens the world at night, torches/screens/crystals glow. Owner: person B2.
//!
//! STUB: uniform darkness. TODO(B2): per-tile light map (flood fill from torches,
//! crystals, screens, sky light from above), smooth per-pixel interpolation, glow halos.

use crate::camera::Camera;
use crate::gfx::Canvas;
use crate::screens::Screens;
use crate::world::World;

pub const AMBIENT_NIGHT: f32 = 0.28;

pub struct Light {
    level: f32,
}

impl Light {
    pub fn new() -> Self {
        Light { level: 1.0 }
    }

    /// Recompute light for the visible area. Called once per frame before drawing.
    pub fn update(&mut self, _world: &World, _cam: &Camera, daylight: f32, _screens: &Screens) {
        self.level = AMBIENT_NIGHT + (1.0 - AMBIENT_NIGHT) * daylight;
    }

    /// Darken the already-drawn world layer.
    pub fn apply(&self, c: &mut Canvas) {
        if self.level >= 0.999 {
            return;
        }
        let k = (self.level * 256.0) as u32;
        for p in c.px.chunks_exact_mut(4) {
            p[0] = ((p[0] as u32 * k) >> 8) as u8;
            p[1] = ((p[1] as u32 * k) >> 8) as u8;
            p[2] = ((p[2] as u32 * k) >> 8) as u8;
        }
    }

    /// Brightness (0..1) at a world-px position, for shading sprites on the FX layer.
    pub fn level_at(&self, _wx: f32, _wy: f32) -> f32 {
        self.level
    }
}

impl Default for Light {
    fn default() -> Self {
        Self::new()
    }
}
