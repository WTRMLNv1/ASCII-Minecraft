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
use crate::rendering::pipeline::{draw_line, project_vertex};

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

    let edges = [
        (0, 1),
        (1, 2),
        (2, 3),
        (3, 0), // Back face
        (4, 5),
        (5, 6),
        (6, 7),
        (7, 4), // Front face
        (0, 4),
        (1, 5),
        (2, 6),
        (3, 7), // Connecting edges
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

        let mvp = Mat4::multiply(Mat4::multiply(projection, view), model);

        // Project vertices
        let mut projected_vertices = Vec::with_capacity(vertices.len());
        let mut visible_vertices = 0;

        for &vertex in &vertices {
            let projected = project_vertex(vertex, &mvp, width, height);
            if projected.is_some() {
                visible_vertices += 1;
            }
            projected_vertices.push(projected);
        }

        // Draw edges
        let mut drawn_edges = 0;
        for &(i, j) in &edges {
            if let (Some(p1), Some(p2)) = (projected_vertices[i], projected_vertices[j]) {
                draw_line(&mut fb, p1, p2, '#');
                drawn_edges += 1;
            }
        }

        let status = format!(
            "ASCII Minecraft | frame {frame_count} | visible vertices {visible_vertices}/{} | drawn edges {drawn_edges}/{} | q quits",
            vertices.len(),
            edges.len(),
        );
        for (x, ch) in status.chars().take(width).enumerate() {
            fb.set(x, height - 1, ch);
        }

        if frame_count == 0 || last_debug_frame.elapsed() >= Duration::from_secs(1) {
            writeln!(
                debug_log,
                "[debug] frame={frame_count} elapsed={elapsed:.2}s visible_vertices={visible_vertices}/{} drawn_edges={drawn_edges}/{}",
                vertices.len(),
                edges.len(),
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
