use std::time::Duration;

use winit::{
    dpi::PhysicalPosition,
    event::{ElementState, MouseButton, MouseScrollDelta},
    keyboard::KeyCode,
};

pub struct Camera {
    position: glam::Vec3,
    orientation: glam::Quat,
}

impl Camera {
    pub fn new(position: impl Into<glam::Vec3>, yaw: f32, pitch: f32) -> Self {
        let position = position.into();
        let yaw_quat = glam::Quat::from_rotation_y(yaw);
        let pitch_quat = glam::Quat::from_rotation_x(pitch);
        let orientation = (yaw_quat * pitch_quat).normalize();

        Self { position, orientation }
    }

    pub fn position(&self) -> glam::Vec3 {
        self.position
    }

    pub fn view_matrix(&self) -> glam::Mat4 {
        glam::Mat4::from_rotation_translation(self.orientation, self.position).inverse()
    }

    fn forward(&self) -> glam::Vec3 {
        self.orientation * -glam::Vec3::Z
    }

    fn right(&self) -> glam::Vec3 {
        self.orientation * glam::Vec3::X
    }

    fn up(&self) -> glam::Vec3 {
        self.orientation * glam::Vec3::Y
    }
}

pub struct Projection {
    aspect: f32,
    fov_y: f32,
    z_near: f32,
    z_far: f32,
    matrix: glam::Mat4,
}

impl Projection {
    pub fn new(width: u32, height: u32, fov_y_radians: f32, z_near: f32, z_far: f32) -> Self {
        let aspect = width as f32 / height as f32;
        let matrix = glam::Mat4::perspective_rh(fov_y_radians, aspect, z_near, z_far);

        Self {
            aspect,
            fov_y: fov_y_radians,
            z_near,
            z_far,
            matrix,
        }
    }

    pub fn matrix(&self) -> glam::Mat4 {
        self.matrix
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.aspect = width as f32 / height as f32;
        self.matrix = glam::Mat4::perspective_rh(self.fov_y, self.aspect, self.z_near, self.z_far);
    }
}

pub struct CameraController {
    velocity: glam::Vec3,
    rotation: glam::Vec2,
    mouse_pressed: bool,
    scroll: f32,
    speed: f32,
    sensitivity: f32,
}

impl CameraController {
    pub fn new(speed: f32, sensitivity: f32) -> Self {
        Self {
            velocity: glam::Vec3::ZERO,
            rotation: glam::Vec2::ZERO,
            scroll: 0.0,
            mouse_pressed: false,
            speed,
            sensitivity,
        }
    }

    pub fn is_mouse_pressed(&self) -> bool {
        self.mouse_pressed
    }

    pub fn handle_key(&mut self, key: KeyCode, state: ElementState) -> bool {
        let increment = if state.is_pressed() { 1.0 } else { 0.0 };
        match key {
            KeyCode::KeyW => {
                self.velocity.z = increment;
                true
            }
            KeyCode::KeyA => {
                self.velocity.x = -increment;
                true
            }
            KeyCode::KeyS => {
                self.velocity.z = -increment;
                true
            }
            KeyCode::KeyD => {
                self.velocity.x = increment;
                true
            }
            KeyCode::Space => {
                self.velocity.y = increment;
                true
            }
            KeyCode::ControlLeft => {
                self.velocity.y = -increment;
                true
            }
            _ => false,
        }
    }

    pub fn handle_mouse(&mut self, mouse_dx: f64, mouse_dy: f64) {
        if mouse_dx == 0.0 && mouse_dy == 0.0 {
            return;
        }

        self.rotation.x = mouse_dx as f32;
        self.rotation.y = mouse_dy as f32;
    }

    pub fn handle_scroll(&mut self, delta: &MouseScrollDelta) {
        self.scroll = match delta {
            MouseScrollDelta::LineDelta(_, scroll) => scroll * 100.0,
            MouseScrollDelta::PixelDelta(PhysicalPosition { y: scroll, .. }) => *scroll as f32,
        }
    }

    pub fn handle_mouse_button(&mut self, button: MouseButton, pressed: bool) {
        match button {
            MouseButton::Left => self.mouse_pressed = pressed,
            _ => (),
        }
    }

    pub fn update_camera(&mut self, camera: &mut Camera, dt: Duration) {
        let dt = dt.as_secs_f32();

        let yaw = glam::Quat::from_rotation_y(self.rotation.x * self.sensitivity);
        let pitch = glam::Quat::from_axis_angle(camera.right(), self.rotation.y * self.sensitivity);
        camera.orientation = ((yaw * pitch) * camera.orientation).normalize();
        self.rotation = glam::Vec2::ZERO;

        let translation =
            camera.forward() * self.velocity.z + camera.right() * self.velocity.x + camera.up() * self.velocity.y;

        if translation != glam::Vec3::ZERO {
            camera.position += translation.normalize() * self.speed * dt;
        }

        camera.position += camera.forward() * self.scroll * self.speed * self.sensitivity * dt;
        self.scroll = 0.0;
    }
}
