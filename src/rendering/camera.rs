use crate::math::mat::Mat4;
use crate::math::vec::Vec3;

pub struct Camera {
    pub position: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub fov: f32,
}

impl Camera {
    pub fn new(position: Vec3, yaw: f32, pitch: f32, fov: f32) -> Self {
        Self {
            position,
            yaw,
            pitch,
            fov,
        }
    }

    pub fn get_view_matrix(&self) -> Mat4 {
        let forward = Vec3::new(
            self.yaw.sin() * self.pitch.cos(),
            self.pitch.sin(),
            -self.yaw.cos() * self.pitch.cos(),
        )
        .normalize();

        let world_up = Vec3::new(0.0, 1.0, 0.0);
        let right = Vec3::cross(forward, world_up).normalize();
        let up = Vec3::cross(right, forward).normalize();

        let mut m = Mat4::identity();

        // Orientation part
        m.data[0] = right.x;
        m.data[1] = right.y;
        m.data[2] = right.z;

        m.data[4] = up.x;
        m.data[5] = up.y;
        m.data[6] = up.z;

        m.data[8] = -forward.x;
        m.data[9] = -forward.y;
        m.data[10] = -forward.z;

        // Translation part
        let pos = self.position;
        m.data[3] = -Vec3::dot(right, pos);
        m.data[7] = -Vec3::dot(up, pos);
        m.data[11] = Vec3::dot(forward, pos);

        m
    }
}
