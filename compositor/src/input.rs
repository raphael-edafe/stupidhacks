//! Input routing between the game and app windows.
//!
//! - Keyboard: if an app has focus, keys go to it, except Esc, which hands focus
//!   back to the game. Otherwise keys become game input and never reach apps.
//! - Pointer: hovering a window sends motion to it; clicking a window focuses it
//!   and the click goes to the app; clicking the world focuses the game.

use smithay::{
    backend::input::{
        AbsolutePositionEvent, Axis, AxisSource, ButtonState, Event, InputBackend, InputEvent, KeyState, KeyboardKeyEvent,
        PointerAxisEvent, PointerButtonEvent,
    },
    input::{
        keyboard::FilterResult,
        pointer::{AxisFrame, ButtonEvent, MotionEvent},
    },
    reexports::wayland_server::protocol::wl_surface::WlSurface,
    utils::{Logical, Point, SERIAL_COUNTER},
};

use shared::Key;

use crate::state::Siege;

const BTN_LEFT: u32 = 0x110;
const BTN_RIGHT: u32 = 0x111;
const KEY_ESC: u32 = 1;

/// evdev keycode -> game key.
fn map_key(code: u32) -> Option<Key> {
    Some(match code {
        30 | 105 => Key::Left,  // A, Left
        32 | 106 => Key::Right, // D, Right
        17 | 103 => Key::Up,    // W, Up
        31 | 108 => Key::Down,  // S, Down
        57 => Key::Jump,        // Space
        48 => Key::Build,       // B
        49 => Key::Night,       // N
        34 => Key::God,         // G
        59 => Key::ToggleHud,   // F1
        20 => Key::Torch,       // T
        18 => Key::Interact,    // E
        45 => Key::CloseApp,    // X
        44 => Key::Zoom,        // Z
        1 => Key::Escape,       // Esc
        2 => Key::Hotbar1,
        3 => Key::Hotbar2,
        4 => Key::Hotbar3,
        5 => Key::Hotbar4,
        6 => Key::Hotbar5,
        7 => Key::Hotbar6,
        _ => return None,
    })
}

impl Siege {
    pub fn process_input_event<I: InputBackend>(&mut self, event: InputEvent<I>, out_size: (i32, i32)) {
        match event {
            InputEvent::Keyboard { event, .. } => {
                let serial = SERIAL_COUNTER.next_serial();
                let time = Event::time_msec(&event);
                let code = event.key_code().raw().saturating_sub(8);
                let pressed = event.state() == KeyState::Pressed;
                let Some(kb) = self.seat.get_keyboard() else { return };

                let to_app = self.app_focus.is_some() && !(code == KEY_ESC && pressed);
                kb.input::<(), _>(self, event.key_code(), event.state(), serial, time, |_, _, _| {
                    if to_app {
                        FilterResult::Forward
                    } else {
                        FilterResult::Intercept(())
                    }
                });
                if to_app {
                    return;
                }
                if self.app_focus.is_some() && code == KEY_ESC && pressed {
                    // Esc while an app has focus: give focus back to the game.
                    self.focus_app(None);
                    return;
                }
                if let Some(k) = map_key(code) {
                    self.input.set(k, pressed);
                }
            }
            InputEvent::PointerMotionAbsolute { event, .. } => {
                let pos = event.position_transformed((out_size.0, out_size.1).into());
                self.pointer_moved(pos, event.time_msec(), out_size);
            }
            InputEvent::PointerButton { event, .. } => {
                let Some(pointer) = self.seat.get_pointer() else { return };
                let serial = SERIAL_COUNTER.next_serial();
                let button = event.button_code();
                let state = event.state();
                let pos = pointer.current_location();

                if state == ButtonState::Pressed {
                    let game_owns_mouse = self.game_drag || self.game.screens.build_mode;
                    let under = if game_owns_mouse { None } else { self.surface_under(pos, out_size.0, out_size.1) };
                    match under {
                        Some((idx, _surface, _)) => {
                            // Click on a window: focus it (unless it already has focus).
                            let top = self.windows[idx].window.toplevel().map(|t| t.wl_surface().clone());
                            if top.is_some() && self.app_focus != top {
                                self.focus_app(top);
                            }
                        }
                        None => {
                            if self.app_focus.is_some() {
                                self.focus_app(None);
                            }
                            self.game_drag = true;
                            match button {
                                BTN_LEFT => self.input.lmb = true,
                                BTN_RIGHT => self.input.rmb = true,
                                _ => {}
                            }
                        }
                    }
                } else {
                    match button {
                        BTN_LEFT => self.input.lmb = false,
                        BTN_RIGHT => self.input.rmb = false,
                        _ => {}
                    }
                    if !self.input.lmb && !self.input.rmb {
                        self.game_drag = false;
                    }
                }

                pointer.button(self, &ButtonEvent { button, state, serial, time: event.time_msec() });
                pointer.frame(self);
            }
            InputEvent::PointerAxis { event, .. } => {
                let Some(pointer) = self.seat.get_pointer() else { return };
                let source = event.source();
                let h = event.amount(Axis::Horizontal).unwrap_or_else(|| event.amount_v120(Axis::Horizontal).unwrap_or(0.0) * 15.0 / 120.);
                let v = event.amount(Axis::Vertical).unwrap_or_else(|| event.amount_v120(Axis::Vertical).unwrap_or(0.0) * 15.0 / 120.);
                let mut frame = AxisFrame::new(event.time_msec()).source(source);
                if h != 0.0 {
                    frame = frame.value(Axis::Horizontal, h);
                    if let Some(d) = event.amount_v120(Axis::Horizontal) {
                        frame = frame.v120(Axis::Horizontal, d as i32);
                    }
                }
                if v != 0.0 {
                    frame = frame.value(Axis::Vertical, v);
                    if let Some(d) = event.amount_v120(Axis::Vertical) {
                        frame = frame.v120(Axis::Vertical, d as i32);
                    }
                }
                if source == AxisSource::Finger {
                    if event.amount(Axis::Horizontal) == Some(0.0) {
                        frame = frame.stop(Axis::Horizontal);
                    }
                    if event.amount(Axis::Vertical) == Some(0.0) {
                        frame = frame.stop(Axis::Vertical);
                    }
                }
                pointer.axis(self, frame);
                pointer.frame(self);
            }
            _ => {}
        }
    }

    pub fn pointer_moved(&mut self, pos: Point<f64, Logical>, time: u32, out_size: (i32, i32)) {
        let (vx, vy) = self.view.to_view(pos);
        self.input.mouse_x = vx;
        self.input.mouse_y = vy;
        let Some(pointer) = self.seat.get_pointer() else { return };
        let under: Option<(WlSurface, Point<f64, Logical>)> = if self.game_drag || self.game.screens.build_mode {
            None
        } else {
            self.surface_under(pos, out_size.0, out_size.1).map(|(_, s, o)| (s, o))
        };
        let serial = SERIAL_COUNTER.next_serial();
        pointer.motion(self, under, &MotionEvent { location: pos, serial, time });
        pointer.frame(self);
    }

    /// Re-send pointer focus after windows moved under a stationary pointer.
    pub fn refresh_pointer(&mut self, out_size: (i32, i32)) {
        if let Some(pointer) = self.seat.get_pointer() {
            let pos = pointer.current_location();
            let time = self.start_time.elapsed().as_millis() as u32;
            self.pointer_moved(pos, time, out_size);
        }
    }
}
