#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mat4 {
    pub data: [f32; 16],
}

impl Mat4 {
    pub fn identity() -> Self {
        let mut data = [0.0; 16];
        data[0] = 1.0;
        data[5] = 1.0;
        data[10] = 1.0;
        data[15] = 1.0;
        Self { data }
    }

    pub fn translation(x: f32, y: f32, z: f32) -> Self {
        let mut m = Self::identity();
        m.data[3] = x;
        m.data[7] = y;
        m.data[11] = z;
        m
    }

    pub fn rotation_x(angle: f32) -> Self {
        let mut m = Self::identity();
        let cos = angle.cos();
        let sin = angle.sin();
        m.data[5] = cos;
        m.data[6] = -sin;
        m.data[9] = sin;
        m.data[10] = cos;
        m
    }

    pub fn rotation_y(angle: f32) -> Self {
        let mut m = Self::identity();
        let cos = angle.cos();
        let sin = angle.sin();
        m.data[0] = cos;
        m.data[2] = sin;
        m.data[8] = -sin;
        m.data[10] = cos;
        m
    }

    pub fn rotation_z(angle: f32) -> Self {
        let mut m = Self::identity();
        let cos = angle.cos();
        let sin = angle.sin();
        m.data[0] = cos;
        m.data[1] = -sin;
        m.data[4] = sin;
        m.data[5] = cos;
        m
    }

    pub fn perspective(fov: f32, aspect: f32, near: f32, far: f32) -> Self {
        let mut m = Self { data: [0.0; 16] };
        let f = 1.0 / (fov / 2.0).tan();

        m.data[0] = f / aspect;
        m.data[5] = f;
        m.data[10] = (far + near) / (near - far);
        m.data[11] = (2.0 * far * near) / (near - far);
        m.data[14] = -1.0;
        m
    }

    pub fn multiply(a: Mat4, b: Mat4) -> Mat4 {
        let mut res = [0.0; 16];
        for i in 0..4 {
            for j in 0..4 {
                res[i * 4 + j] = a.data[i * 4 + 0] * b.data[0 * 4 + j]
                    + a.data[i * 4 + 1] * b.data[1 * 4 + j]
                    + a.data[i * 4 + 2] * b.data[2 * 4 + j]
                    + a.data[i * 4 + 3] * b.data[3 * 4 + j];
            }
        }
        Mat4 { data: res }
    }

    pub fn transform_vec(&self, v: crate::math::vec::Vec3) -> [f32; 4] {
        let x = self.data[0] * v.x + self.data[1] * v.y + self.data[2] * v.z + self.data[3];
        let y = self.data[4] * v.x + self.data[5] * v.y + self.data[6] * v.z + self.data[7];
        let z = self.data[8] * v.x + self.data[9] * v.y + self.data[10] * v.z + self.data[11];
        let w = self.data[12] * v.x + self.data[13] * v.y + self.data[14] * v.z + self.data[15];
        [x, y, z, w]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::vec::Vec3;

    #[test]
    fn test_identity() {
        let m = Mat4::identity();
        let v = Vec3::new(1.0, 2.0, 3.0);
        let res = m.transform_vec(v);
        assert_eq!(res, [1.0, 2.0, 3.0, 1.0]);
    }

    #[test]
    fn test_translation() {
        let m = Mat4::translation(10.0, -5.0, 2.0);
        let v = Vec3::new(0.0, 0.0, 0.0);
        let res = m.transform_vec(v);
        assert_eq!(res, [10.0, -5.0, 2.0, 1.0]);
    }

    #[test]
    fn test_multiply() {
        let m1 = Mat4::translation(1.0, 0.0, 0.0);
        let m2 = Mat4::translation(2.0, 0.0, 0.0);
        let m3 = Mat4::multiply(m1, m2);
        let v = Vec3::new(0.0, 0.0, 0.0);
        let res = m3.transform_vec(v);
        assert_eq!(res, [3.0, 0.0, 0.0, 1.0]);
    }
}
