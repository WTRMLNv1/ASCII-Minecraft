use crate::framebuffer::Framebuffer;
use crate::math::vec::Vec3;

pub fn draw_line(fb: &mut Framebuffer, p1: (f32, f32), p2: (f32, f32), ch: char) {
    let (x1, y1) = (p1.0 as i32, p1.1 as i32);
    let (x2, y2) = (p2.0 as i32, p2.1 as i32);

    let dx = (x2 - x1).abs();
    let dy = -(y2 - y1).abs();
    let sx = if x1 < x2 { 1 } else { -1 };
    let sy = if y1 < y2 { 1 } else { -1 };
    let mut err = dx + dy;

    let mut curr_x = x1;
    let mut curr_y = y1;

    loop {
        fb.set(curr_x as usize, curr_y as usize, ch);
        if curr_x == x2 && curr_y == y2 {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            curr_x += sx;
        }
        if e2 <= dx {
            err += dx;
            curr_y += sy;
        }
    }
}

pub fn project_vertex(
    v: Vec3,
    mvp: &crate::math::mat::Mat4,
    width: usize,
    height: usize,
) -> Option<(f32, f32)> {
    let clip = mvp.transform_vec(v);
    let w = clip[3];

    if w <= 0.0 {
        return None;
    }

    let ndc_x = clip[0] / w;
    let ndc_y = clip[1] / w;

    let screen_x = (ndc_x + 1.0) * 0.5 * width as f32;
    let screen_y = (1.0 - ndc_y) * 0.5 * height as f32; // Invert Y for screen space

    Some((screen_x, screen_y))
}
