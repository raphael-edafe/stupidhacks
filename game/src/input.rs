//! Input helper: current + previous snapshot for edge detection.

use shared::{InputState, Key};

#[derive(Default, Clone, Copy)]
pub struct Controls {
    pub cur: InputState,
    pub prev: InputState,
}

impl Controls {
    pub fn advance(&mut self, next: &InputState) {
        self.prev = self.cur;
        self.cur = *next;
        if !next.game_focused {
            // Ignore keys and buttons while an app has focus, but keep the mouse position.
            let (mx, my) = (next.mouse_x, next.mouse_y);
            let mut blank = InputState::default();
            blank.mouse_x = mx;
            blank.mouse_y = my;
            self.cur = blank;
        }
    }

    /// The game got keyboard focus back this tick (the compositor took it away
    /// from an app). `game_focused` survives the blanking above, so this sees
    /// the raw focus transition.
    pub fn focus_regained(&self) -> bool {
        self.cur.game_focused && !self.prev.game_focused
    }

    /// A copy with every key and button released (mouse position and focus
    /// kept): what a frozen player sees while a modal UI is up.
    pub fn frozen(&self) -> Controls {
        let blank = |s: &InputState| {
            let mut b = InputState::default();
            b.mouse_x = s.mouse_x;
            b.mouse_y = s.mouse_y;
            b.game_focused = s.game_focused;
            b
        };
        Controls { cur: blank(&self.cur), prev: blank(&self.prev) }
    }

    pub fn down(&self, k: Key) -> bool {
        self.cur.down(k)
    }
    pub fn pressed(&self, k: Key) -> bool {
        self.cur.down(k) && !self.prev.down(k)
    }
    pub fn lmb(&self) -> bool {
        self.cur.lmb
    }
    pub fn lmb_pressed(&self) -> bool {
        self.cur.lmb && !self.prev.lmb
    }
    pub fn lmb_released(&self) -> bool {
        !self.cur.lmb && self.prev.lmb
    }
    pub fn rmb(&self) -> bool {
        self.cur.rmb
    }
    pub fn rmb_pressed(&self) -> bool {
        self.cur.rmb && !self.prev.rmb
    }
    pub fn mouse(&self) -> (f32, f32) {
        (self.cur.mouse_x, self.cur.mouse_y)
    }
}
