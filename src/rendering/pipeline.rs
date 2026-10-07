use crate::framebuffer::{Framebuffer, TerminalColor};
use crate::math::vec::Vec3;

#[derive(Debug, Clone, Copy)]
pub struct ScreenVertex {
    pub x: f32,
    pub y: f32,
    pub depth: f32,
}

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

pub fn shade_char(brightness: f32) -> char {
    const GRADIENT: &[u8] = b".:-=+*#%@";
    let clamped = brightness.clamp(0.0, 1.0);
    let idx = (clamped * (GRADIENT.len() - 1) as f32).round() as usize;
    GRADIENT[idx] as char
}

pub fn project_vertex_with_depth(
    v: Vec3,
    mvp: &crate::math::mat::Mat4,
    width: usize,
    height: usize,
) -> Option<ScreenVertex> {
    let clip = mvp.transform_vec(v);
    let w = clip[3];

    if w <= 0.0 {
        return None;
    }

    let ndc_x = clip[0] / w;
    let ndc_y = clip[1] / w;
    let ndc_z = clip[2] / w;

    if !(-1.0..=1.0).contains(&ndc_z) {
        return None;
    }

    let screen_x = (ndc_x + 1.0) * 0.5 * width as f32;
    let screen_y = (1.0 - ndc_y) * 0.5 * height as f32;
    let depth = (ndc_z + 1.0) * 0.5;

    Some(ScreenVertex {
        x: screen_x,
        y: screen_y,
        depth,
    })
}

pub fn rasterize_triangle(
    fb: &mut Framebuffer,
    a: ScreenVertex,
    b: ScreenVertex,
    c: ScreenVertex,
    ch: char,
    color: TerminalColor,
) -> usize {
    let min_x = a.x.min(b.x).min(c.x).floor().max(0.0) as i32;
    let max_x = a.x.max(b.x).max(c.x).ceil().min((fb.width - 1) as f32) as i32;
    let min_y = a.y.min(b.y).min(c.y).floor().max(0.0) as i32;
    let max_y = a.y.max(b.y).max(c.y).ceil().min((fb.height - 1) as f32) as i32;

    let area = edge_function(a.x, a.y, b.x, b.y, c.x, c.y);
    if area.abs() < f32::EPSILON {
        return 0;
    }

    let mut written = 0;
    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let px = x as f32 + 0.5;
            let py = y as f32 + 0.5;

            let w0 = edge_function(b.x, b.y, c.x, c.y, px, py);
            let w1 = edge_function(c.x, c.y, a.x, a.y, px, py);
            let w2 = edge_function(a.x, a.y, b.x, b.y, px, py);

            let inside = if area > 0.0 {
                w0 >= 0.0 && w1 >= 0.0 && w2 >= 0.0
            } else {
                w0 <= 0.0 && w1 <= 0.0 && w2 <= 0.0
            };

            if inside {
                let alpha = w0 / area;
                let beta = w1 / area;
                let gamma = w2 / area;
                let depth = alpha * a.depth + beta * b.depth + gamma * c.depth;

                if fb.set_with_depth(x as usize, y as usize, depth, ch, color) {
                    written += 1;
                }
            }
        }
    }

    written
}

pub fn rasterize_transparent_triangle(
    fb: &mut Framebuffer,
    a: ScreenVertex,
    b: ScreenVertex,
    c: ScreenVertex,
    ch: char,
    color: TerminalColor,
    opacity: f32,
) -> usize {
    let min_x = a.x.min(b.x).min(c.x).floor().max(0.0) as i32;
    let max_x = a.x.max(b.x).max(c.x).ceil().min((fb.width - 1) as f32) as i32;
    let min_y = a.y.min(b.y).min(c.y).floor().max(0.0) as i32;
    let max_y = a.y.max(b.y).max(c.y).ceil().min((fb.height - 1) as f32) as i32;
    let area = edge_function(a.x, a.y, b.x, b.y, c.x, c.y);
    if area.abs() < f32::EPSILON {
        return 0;
    }

    let mut written = 0;
    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let px = x as f32 + 0.5;
            let py = y as f32 + 0.5;
            let w0 = edge_function(b.x, b.y, c.x, c.y, px, py);
            let w1 = edge_function(c.x, c.y, a.x, a.y, px, py);
            let w2 = edge_function(a.x, a.y, b.x, b.y, px, py);
            let inside = if area > 0.0 {
                w0 >= 0.0 && w1 >= 0.0 && w2 >= 0.0
            } else {
                w0 <= 0.0 && w1 <= 0.0 && w2 <= 0.0
            };
            if inside {
                let depth = (w0 * a.depth + w1 * b.depth + w2 * c.depth) / area;
                if fb
                    .blend_transparent_with_depth(x as usize, y as usize, depth, ch, color, opacity)
                {
                    written += 1;
                }
            }
        }
    }
    written
}

fn edge_function(ax: f32, ay: f32, bx: f32, by: f32, px: f32, py: f32) -> f32 {
    (px - ax) * (by - ay) - (py - ay) * (bx - ax)
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
