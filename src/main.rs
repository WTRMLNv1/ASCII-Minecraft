mod framebuffer;
mod math;
mod rendering;
mod terminal;

use framebuffer::Framebuffer;
use terminal::{Terminal, TerminalGuard};

use crossterm::event::{self, Event, KeyCode};

use std::fs::File;
use std::io::Write;
use std::thread;
use std::time::{Duration, Instant};

use crate::math::mat::Mat4;
use crate::math::vec::Vec3;
use crate::rendering::camera::Camera;
use crate::rendering::pipeline::{project_vertex_with_depth, rasterize_triangle, shade_char};

#[derive(Clone, Copy)]
struct Triangle {
    vertices: [usize; 3],
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    eprintln!("[debug] main started");

    let mut debug_log = File::create("debug.log")?;
    writeln!(debug_log, "[debug] main started")?;
    writeln!(debug_log, "[debug] press q to quit")?;

    let guard = TerminalGuard::new()?;

    let width = 120;
    let height = 40;

    let mut terminal = Terminal::new(width, height)?;
    let mut fb = Framebuffer::new(width, height);
    writeln!(debug_log, "[debug] terminal created: {}x{}", width, height)?;

    let target_fps = 30;
    let target_frame_time = Duration::from_secs_f64(1.0 / target_fps as f64);

    let mut running = true;
    let start_time = Instant::now();
    let mut frame_count = 0_u64;
    let mut last_debug_frame = Instant::now();

    // Cube definition
    let vertices = [
        Vec3::new(-1.0, -1.0, -1.0),
        Vec3::new(1.0, -1.0, -1.0),
        Vec3::new(1.0, 1.0, -1.0),
        Vec3::new(-1.0, 1.0, -1.0),
        Vec3::new(-1.0, -1.0, 1.0),
        Vec3::new(1.0, -1.0, 1.0),
        Vec3::new(1.0, 1.0, 1.0),
        Vec3::new(-1.0, 1.0, 1.0),
    ];

    let triangles = [
        Triangle {
            vertices: [4, 5, 6],
        },
        Triangle {
            vertices: [4, 6, 7],
        },
        Triangle {
            vertices: [1, 0, 3],
        },
        Triangle {
            vertices: [1, 3, 2],
        },
        Triangle {
            vertices: [5, 1, 2],
        },
        Triangle {
            vertices: [5, 2, 6],
        },
        Triangle {
            vertices: [0, 4, 7],
        },
        Triangle {
            vertices: [0, 7, 3],
        },
        Triangle {
            vertices: [3, 7, 6],
        },
        Triangle {
            vertices: [3, 6, 2],
        },
        Triangle {
            vertices: [0, 1, 5],
        },
        Triangle {
            vertices: [0, 5, 4],
        },
    ];

    let camera = Camera::new(
        Vec3::new(0.0, 0.0, 5.0),
        0.0, // yaw
        0.0, // pitch
        60.0f32.to_radians(),
    );

    while running {
        let frame_start = Instant::now();

        // 1. Poll input
        if event::poll(Duration::from_millis(1))? {
            if let Event::Key(key) = event::read()? {
                if let KeyCode::Char('q') = key.code {
                    writeln!(debug_log, "[debug] q pressed; exiting main loop")?;
                    running = false;
                }
            }
        }

        // 2. Update
        fb.clear();

        let elapsed = start_time.elapsed().as_secs_f32();

        // Model matrix: rotate the cube over time
        let model = Mat4::multiply(
            Mat4::rotation_x(elapsed * 0.5),
            Mat4::rotation_y(elapsed * 0.8),
        );

        let view = camera.get_view_matrix();

        let projection =
            Mat4::perspective(camera.fov, width as f32 / (height as f32 * 2.0), 0.1, 100.0);

        let model_view = Mat4::multiply(view, model);
        let mvp = Mat4::multiply(Mat4::multiply(projection, view), model);
        let light_dir = Vec3::new(-0.35, 0.65, 0.68).normalize();

        // Project vertices
        let mut projected_vertices = Vec::with_capacity(vertices.len());
        let mut view_vertices = Vec::with_capacity(vertices.len());
        let mut world_vertices = Vec::with_capacity(vertices.len());
        let mut visible_vertices = 0;

        for &vertex in &vertices {
            let world = vec3_from_homogeneous(model.transform_vec(vertex));
            let view_space = vec3_from_homogeneous(model_view.transform_vec(vertex));
            let projected = project_vertex_with_depth(vertex, &mvp, width, height);

            if projected.is_some() {
                visible_vertices += 1;
            }

            world_vertices.push(world);
            view_vertices.push(view_space);
            projected_vertices.push(projected);
        }

        let mut culled_triangles = 0;
        let mut rasterized_triangles = 0;
        let mut shaded_cells = 0;

        for triangle in &triangles {
            let [i0, i1, i2] = triangle.vertices;
            let view_normal =
                triangle_normal(view_vertices[i0], view_vertices[i1], view_vertices[i2]);

            if Vec3::dot(view_normal, Vec3::new(0.0, 0.0, -1.0)) >= 0.0 {
                culled_triangles += 1;
                continue;
            }

            let Some(p0) = projected_vertices[i0] else {
                continue;
            };
            let Some(p1) = projected_vertices[i1] else {
                continue;
            };
            let Some(p2) = projected_vertices[i2] else {
                continue;
            };

            let world_normal =
                triangle_normal(world_vertices[i0], world_vertices[i1], world_vertices[i2]);
            let brightness = 0.18 + Vec3::dot(world_normal, light_dir).max(0.0) * 0.82;
            let ch = shade_char(brightness);
            let written = rasterize_triangle(&mut fb, p0, p1, p2, ch);

            if written > 0 {
                rasterized_triangles += 1;
                shaded_cells += written;
            }
        }

        let status = format!(
            "ASCII Minecraft | frame {frame_count} | verts {visible_vertices}/{} | tris {rasterized_triangles}/{} | cells {shaded_cells} | culled {culled_triangles} | q quits",
            vertices.len(),
            triangles.len(),
        );
        for (x, ch) in status.chars().take(width).enumerate() {
            fb.set(x, height - 1, ch);
        }

        if frame_count == 0 || last_debug_frame.elapsed() >= Duration::from_secs(1) {
            writeln!(
                debug_log,
                "[debug] frame={frame_count} elapsed={elapsed:.2}s visible_vertices={visible_vertices}/{} rasterized_triangles={rasterized_triangles}/{} shaded_cells={shaded_cells} culled_triangles={culled_triangles}",
                vertices.len(),
                triangles.len(),
            )?;
            debug_log.flush()?;
            last_debug_frame = Instant::now();
        }

        // 3. Present
        terminal.present(&fb)?;
        frame_count += 1;

        // 4. Frame timing
        let frame_elapsed = frame_start.elapsed();

        if frame_elapsed < target_frame_time {
            thread::sleep(target_frame_time - frame_elapsed);
        }
    }

    drop(guard);
    eprintln!("[debug] exited cleanly after {frame_count} frames; see debug.log");

    Ok(())
}

fn vec3_from_homogeneous(v: [f32; 4]) -> Vec3 {
    if v[3].abs() > f32::EPSILON {
        Vec3::new(v[0] / v[3], v[1] / v[3], v[2] / v[3])
    } else {
        Vec3::new(v[0], v[1], v[2])
    }
}

fn triangle_normal(a: Vec3, b: Vec3, c: Vec3) -> Vec3 {
    Vec3::cross(Vec3::sub(b, a), Vec3::sub(c, a)).normalize()
}
