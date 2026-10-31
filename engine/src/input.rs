use std::collections::HashSet;
use winit::keyboard::KeyCode;

pub struct InputState {
    pub keys_pressed: HashSet<KeyCode>,
    pub mouse_delta: (f32, f32),
    pub mouse_captured: bool,
}

impl InputState {
    pub fn new() -> Self {
        InputState {
            keys_pressed: HashSet::new(),
            mouse_delta: (0.0, 0.0),
            mouse_captured: false,
        }
    }

    pub fn press(&mut self, key: KeyCode) {
        self.keys_pressed.insert(key);
    }

    pub fn release(&mut self, key: KeyCode) {
        self.keys_pressed.remove(&key);
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
