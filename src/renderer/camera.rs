use glam::{Mat4, Vec3};

pub struct Camera {
    pub position: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub fovy: f32,
    pub aspect: f32,
    pub znear: f32,
    pub zfar: f32,
}

impl Camera {
    pub fn new(aspect: f32) -> Self {
        Camera {
            position: Vec3::new(0.0, 5.0, 10.0),
            yaw: -std::f32::consts::FRAC_PI_2,
            pitch: -0.3,
            fovy: 45_f32.to_radians(),
            aspect,
            znear: 0.1,
            zfar: 1000.0,
        }
    }

    pub fn view_proj(&self) -> Mat4 {
        let direction = Vec3::new(
            self.yaw.cos() * self.pitch.cos(),
            self.pitch.sin(),
            self.yaw.sin() * self.pitch.cos(),
        );
        let view = Mat4::look_at_rh(self.position, self.position + direction, Vec3::Y);
        let proj = Mat4::perspective_rh(self.fovy, self.aspect, self.znear, self.zfar);
        proj * view
    }
}

pub struct CameraController {
    speed: f32,
    sensitivity: f32,
    move_forward: bool,
    move_backward: bool,
    move_left: bool,
    move_right: bool,
    move_up: bool,
    move_down: bool,
}

impl CameraController {
    pub fn new() -> Self {
        CameraController {
            speed: 5.0,
            sensitivity: 0.002,
            move_forward: false,
            move_backward: false,
            move_left: false,
            move_right: false,
            move_up: false,
            move_down: false,
        }
    }

    pub fn process_key(&mut self, key: winit::keyboard::KeyCode, pressed: bool) {
        use winit::keyboard::KeyCode;
        match key {
            KeyCode::KeyW => self.move_forward = pressed,
            KeyCode::KeyS => self.move_backward = pressed,
            KeyCode::KeyA => self.move_left = pressed,
            KeyCode::KeyD => self.move_right = pressed,
            KeyCode::Space => self.move_up = pressed,
            KeyCode::ShiftLeft | KeyCode::ShiftRight => self.move_down = pressed,
            _ => {}
        }
    }

    pub fn process_mouse(&self, camera: &mut Camera, dx: f32, dy: f32) {
        camera.yaw += dx * self.sensitivity;
        camera.pitch = (camera.pitch - dy * self.sensitivity)
            .clamp(-1.5, 1.5);
    }

    pub fn update(&self, camera: &mut Camera, dt: f32) {
        let forward = Vec3::new(camera.yaw.cos(), 0.0, camera.yaw.sin()).normalize();
        let right = forward.cross(Vec3::Y).normalize();

        let mut delta = Vec3::ZERO;
        if self.move_forward  { delta += forward; }
        if self.move_backward { delta -= forward; }
        if self.move_right    { delta += right; }
        if self.move_left     { delta -= right; }
        if self.move_up       { delta += Vec3::Y; }
        if self.move_down     { delta -= Vec3::Y; }

        if delta.length_squared() > 0.0 {
            camera.position += delta.normalize() * self.speed * dt;
        }
    }
}