use std::collections::HashSet;
use winit::event::MouseButton;
use winit::keyboard::KeyCode;

pub struct InputState {
    pub keys_pressed: HashSet<KeyCode>,
    pub mouse_delta: (f32, f32),
    pub mouse_captured: bool,
    pub mouse_buttons: HashSet<MouseButton>,
    pub mouse_buttons_pressed: HashSet<MouseButton>,
}

impl InputState {
    pub fn new() -> Self {
        InputState {
            keys_pressed: HashSet::new(),
            mouse_delta: (0.0, 0.0),
            mouse_captured: false,
            mouse_buttons: HashSet::new(),
            mouse_buttons_pressed: HashSet::new(),
        }
    }

    pub fn press(&mut self, key: KeyCode) {
        self.keys_pressed.insert(key);
    }

    pub fn release(&mut self, key: KeyCode) {
        self.keys_pressed.remove(&key);
    }

    pub fn mouse_button(&mut self, button: MouseButton, pressed: bool) {
        if pressed && self.mouse_captured {
            if self.mouse_buttons.insert(button) {
                self.mouse_buttons_pressed.insert(button);
            }
        } else {
            self.mouse_buttons.remove(&button);
        }
    }

    pub fn clear_mouse_buttons(&mut self) {
        self.mouse_buttons.clear();
        self.mouse_buttons_pressed.clear();
    }

    pub fn button_name(button: &MouseButton) -> String {
        match button {
            MouseButton::Other(number) => format!("Other{number}"),
            button => format!("{button:?}"),
        }
    }

    pub fn accumulate_mouse(&mut self, dx: f32, dy: f32) {
        if !self.mouse_captured {
            return;
        }
        self.mouse_delta.0 += dx;
        self.mouse_delta.1 += dy;
    }

    pub fn flush_mouse(&mut self) -> (f32, f32) {
        let delta = self.mouse_delta;
        self.mouse_delta = (0.0, 0.0);
        if self.mouse_captured {
            delta
        } else {
            (0.0, 0.0)
        }
    }
}

impl Default for InputState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn quick_click_survives_release_but_capture_loss_discards_it() {
        let mut input = InputState::new();
        input.mouse_button(MouseButton::Left, true);
        assert!(input.mouse_buttons_pressed.is_empty());
        input.mouse_captured = true;
        input.mouse_button(MouseButton::Left, true);
        input.mouse_button(MouseButton::Left, false);
        assert!(input.mouse_buttons.is_empty());
        assert!(input.mouse_buttons_pressed.contains(&MouseButton::Left));
        input.clear_mouse_buttons();
        assert!(input.mouse_buttons_pressed.is_empty());
    }
    #[test]
    fn motion_requires_capture_and_does_not_leak_after_release() {
        let mut input = InputState::new();
        input.accumulate_mouse(20.0, 30.0);
        assert_eq!(input.flush_mouse(), (0.0, 0.0));
        input.mouse_captured = true;
        input.accumulate_mouse(2.0, 3.0);
        assert_eq!(input.flush_mouse(), (2.0, 3.0));
        input.accumulate_mouse(5.0, 6.0);
        input.mouse_captured = false;
        assert_eq!(input.flush_mouse(), (0.0, 0.0));
        input.mouse_captured = true;
        assert_eq!(input.flush_mouse(), (0.0, 0.0));
    }
}
