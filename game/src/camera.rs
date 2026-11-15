//! Camera: world pixels -> view pixels, smooth follow, trauma-based shake.

use crate::world::World;
use shared::{VIEW_H, VIEW_W};

pub struct Camera {
    /// Top-left of the view in world pixels.
    pub x: f32,
    pub y: f32,
    /// 0..1 shake intensity; decays over time. Add with `kick`.
    pub trauma: f32,
    time: f32,
    /// Integer shake offset for this frame (view px).
    off: (i32, i32),
}

impl Camera {
    pub fn new(cx: f32, cy: f32) -> Self {
        Camera {
            x: cx - VIEW_W as f32 / 2.0,
            y: cy - VIEW_H as f32 / 2.0,
            trauma: 0.0,
            time: 0.0,
            off: (0, 0),
        }
    }

    pub fn kick(&mut self, amount: f32) {
        self.trauma = (self.trauma + amount).min(1.0);
    }

    /// Ease towards centring (cx, cy), clamped to the world.
    pub fn follow(&mut self, cx: f32, cy: f32, dt: f32, world: &World) {
        let tx = cx - VIEW_W as f32 / 2.0;
        let ty = cy - VIEW_H as f32 / 2.0 - 20.0; // see a bit more above the player
        let k = 1.0 - (-6.0 * dt).exp();
        self.x += (tx - self.x) * k;
        self.y += (ty - self.y) * k;
        self.x = self.x.clamp(0.0, world.width_px() - VIEW_W as f32);
        self.y = self.y.clamp(0.0, world.height_px() - VIEW_H as f32);

        self.time += dt;
        self.trauma = (self.trauma - dt * 1.5).max(0.0);
        let s = self.trauma * self.trauma * 6.0;
        let t = self.time * 47.0;
        self.off = (((t.sin() + (t * 1.7).cos()) * 0.5 * s) as i32, (((t * 1.3).cos() + (t * 2.1).sin()) * 0.5 * s) as i32);
    }

    /// World px -> view px (with shake).
    #[inline]
    pub fn to_view(&self, wx: f32, wy: f32) -> (i32, i32) {
        ((wx - self.x).round() as i32 + self.off.0, (wy - self.y).round() as i32 + self.off.1)
    }

    /// View px -> world px (no shake, for mouse picking).
    pub fn to_world(&self, vx: f32, vy: f32) -> (f32, f32) {
        (vx + self.x, vy + self.y)
    }

    /// Integer top-left used for tile rendering (rounded, with shake).
    pub fn origin(&self) -> (i32, i32) {
        (self.x.round() as i32 - self.off.0, self.y.round() as i32 - self.off.1)
    }
}
