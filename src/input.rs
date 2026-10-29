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
        self.mouse_delta.0 += dx;
        self.mouse_delta.1 += dy;
    }

    pub fn flush_mouse(&mut self) -> (f32, f32) {
        let delta = self.mouse_delta;
        self.mouse_delta = (0.0, 0.0);
        delta
    }
}
