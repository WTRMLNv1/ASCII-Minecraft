#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Vec3 {
    pub fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    pub fn add(a: Vec3, b: Vec3) -> Vec3 {
        Vec3::new(a.x + b.x, a.y + b.y, a.z + b.z)
    }

    pub fn sub(a: Vec3, b: Vec3) -> Vec3 {
        Vec3::new(a.x - b.x, a.y - b.y, a.z - b.z)
    }

    pub fn mul_scalar(v: Vec3, s: f32) -> Vec3 {
        Vec3::new(v.x * s, v.y * s, v.z * s)
    }

    pub fn dot(a: Vec3, b: Vec3) -> f32 {
        a.x * b.x + a.y * b.y + a.z * b.z
    }

    pub fn cross(a: Vec3, b: Vec3) -> Vec3 {
        Vec3::new(
            a.y * b.z - a.z * b.y,
            a.z * b.x - a.x * b.z,
            a.x * b.y - a.y * b.x,
        )
    }

    pub fn length(&self) -> f32 {
        (self.x * self.x + self.y * self.y + self.z * self.z).sqrt()
    }

    pub fn normalize(&self) -> Vec3 {
        let len = self.length();
        if len > 0.0 {
            Vec3::mul_scalar(*self, 1.0 / len)
        } else {
            Vec3::new(0.0, 0.0, 0.0)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct IVec3 {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

impl IVec3 {
    pub fn new(x: i32, y: i32, z: i32) -> Self {
        Self { x, y, z }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vec3_ops() {
        let v1 = Vec3::new(1.0, 2.0, 3.0);
        let v2 = Vec3::new(4.0, 5.0, 6.0);

        assert_eq!(Vec3::add(v1, v2), Vec3::new(5.0, 7.0, 9.0));
        assert_eq!(Vec3::sub(v2, v1), Vec3::new(3.0, 3.0, 3.0));
        assert_eq!(Vec3::mul_scalar(v1, 2.0), Vec3::new(2.0, 4.0, 6.0));
        assert_eq!(Vec3::dot(v1, v2), 4.0 + 10.0 + 18.0);
    }

    #[test]
    fn test_vec3_cross() {
        let v1 = Vec3::new(1.0, 0.0, 0.0);
        let v2 = Vec3::new(0.0, 1.0, 0.0);
        assert_eq!(Vec3::cross(v1, v2), Vec3::new(0.0, 0.0, 1.0));
    }

    #[test]
    fn test_vec3_normalize() {
        let v = Vec3::new(5.0, 0.0, 0.0);
        assert_eq!(v.normalize(), Vec3::new(1.0, 0.0, 0.0));
    }
}
