//! The game. Pure Rust, no Smithay. Owner: person B.
//! Placeholder implementation so the workspace builds; replace freely.

use shared::*;

#[derive(Default)]
pub struct Game {
    time: f32,
    events: Vec<GameEvent>,
}

impl Game {
    pub fn new(_seed: u64) -> Self {
        Self::default()
    }
}

impl GameApi for Game {
    fn update(&mut self, dt: f32, _input: &InputState) {
        self.time += dt;
    }

    fn draw_world(&mut self, rgba: &mut [u8]) {
        // Placeholder sky gradient.
        for y in 0..VIEW_H as usize {
            let t = y as f32 / VIEW_H as f32;
            let c = [(40.0 + 60.0 * t) as u8, (90.0 + 80.0 * t) as u8, (180.0 + 40.0 * t) as u8, 255];
            for x in 0..VIEW_W as usize {
                let i = (y * VIEW_W as usize + x) * 4;
                rgba[i..i + 4].copy_from_slice(&c);
            }
        }
    }

    fn draw_fx(&mut self, rgba: &mut [u8]) {
        rgba.fill(0);
    }

    fn screens(&self) -> Vec<ScreenInfo> {
        Vec::new()
    }

    fn set_screen_occupied(&mut self, _id: ScreenId, _occupied: bool) {}

    fn drain_events(&mut self) -> Vec<GameEvent> {
        std::mem::take(&mut self.events)
    }
}
