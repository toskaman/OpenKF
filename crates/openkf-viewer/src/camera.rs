//! Free-fly camera in the converted right-handed Y-up space.
use glam::{Mat4, Vec3};

pub struct Camera {
    pub pos: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub fov_y: f32,
}

impl Camera {
    pub fn forward(&self) -> Vec3 {
        Vec3::new(self.pitch.cos() * self.yaw.cos(), self.pitch.sin(), self.pitch.cos() * self.yaw.sin())
    }
    pub fn right(&self) -> Vec3 {
        self.forward().cross(Vec3::Y).normalize()
    }
    pub fn view_proj(&self, aspect: f32) -> Mat4 {
        let view = Mat4::look_to_rh(self.pos, self.forward(), Vec3::Y);
        let proj = Mat4::perspective_rh(self.fov_y, aspect, 4.0, 120_000.0);
        proj * view
    }
}
