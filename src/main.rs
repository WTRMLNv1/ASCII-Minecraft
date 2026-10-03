mod framebuffer;
mod math;
mod rendering;
mod terminal;
mod world;

use framebuffer::Framebuffer;
use framebuffer::TerminalColor;
use terminal::{Terminal, TerminalGuard};

use crossterm::event::{
    self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseEventKind,
};

use std::fs::File;
use std::io::Write;
use std::thread;
use std::time::{Duration, Instant};

use crate::math::mat::Mat4;
use crate::math::vec::Vec3;
use crate::rendering::camera::Camera;
use crate::rendering::pipeline::{project_vertex_with_depth, rasterize_triangle, shade_char};
use crate::world::{Block, CHUNK_SIZE, World};
use windows_sys::Win32::Foundation::POINT;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetCursorPos, GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN, SetCursorPos,
};

#[derive(Default)]
struct MovementInput {
    forward: bool,
    backward: bool,
    left: bool,
    right: bool,
    up: bool,
    down: bool,
}

impl MovementInput {
    fn clear(&mut self) {
        *self = Self::default();
    }

    fn update(&mut self, key: KeyEvent) {
        let pressed = key.kind != KeyEventKind::Release;

        match key.code {
            KeyCode::Char('w' | 'W') => self.forward = pressed,
            KeyCode::Char('s' | 'S') => self.backward = pressed,
            KeyCode::Char('a' | 'A') => self.left = pressed,
            KeyCode::Char('d' | 'D') => self.right = pressed,
            KeyCode::Char(' ') => self.up = pressed,
            KeyCode::Char('c' | 'C') => self.down = pressed,
            _ => {}
        }
    }

    fn direction(&self, camera: &Camera) -> Vec3 {
        let mut direction = Vec3::new(0.0, 0.0, 0.0);

        if self.forward {
            direction = Vec3::add(direction, camera.get_forward());
        }
        if self.backward {
            direction = Vec3::sub(direction, camera.get_forward());
        }
        if self.left {
            direction = Vec3::sub(direction, camera.get_right());
        }
        if self.right {
            direction = Vec3::add(direction, camera.get_right());
        }
        if self.up {
            direction = Vec3::add(direction, Vec3::new(0.0, 1.0, 0.0));
        }
        if self.down {
            direction = Vec3::sub(direction, Vec3::new(0.0, 1.0, 0.0));
        }

        direction
    }
}

struct RelativeMouseLook {
    locked: bool,
}

impl RelativeMouseLook {
    fn new() -> std::io::Result<Self> {
        let mut mouse_look = Self { locked: false };
        mouse_look.lock()?;
        Ok(mouse_look)
    }

    fn lock(&mut self) -> std::io::Result<()> {
        self.locked = true;
        self.recenter()
    }

    fn unlock(&mut self) {
        self.locked = false;
    }

    fn update_camera(&self, camera: &mut Camera) -> std::io::Result<()> {
        if !self.locked {
            return Ok(());
        }

        let center = screen_center()?;
        let mut cursor = POINT { x: 0, y: 0 };

        // SAFETY: `cursor` is valid writable storage for the Win32 API.
        if unsafe { GetCursorPos(&mut cursor) } == 0 {
            return Err(std::io::Error::last_os_error());
        }

        let dx = (cursor.x - center.x) as f32;
        let dy = (cursor.y - center.y) as f32;
        if dx != 0.0 || dy != 0.0 {
            // Screen Y increases downwards; a positive pitch looks up.
            camera.update_rotation(dx, -dy, 0.0025);
        }

        // SAFETY: the coordinates came from the primary display's screen-space center.
        if unsafe { SetCursorPos(center.x, center.y) } == 0 {
            return Err(std::io::Error::last_os_error());
        }

        Ok(())
    }

    fn recenter(&self) -> std::io::Result<()> {
        let center = screen_center()?;

        // SAFETY: the coordinates came from the primary display's screen-space center.
        if unsafe { SetCursorPos(center.x, center.y) } == 0 {
            return Err(std::io::Error::last_os_error());
        }

        Ok(())
    }
}

fn screen_center() -> std::io::Result<POINT> {
    // SAFETY: GetSystemMetrics has no pointer arguments and is safe to query repeatedly.
    let width = unsafe { GetSystemMetrics(SM_CXSCREEN) };
    // SAFETY: GetSystemMetrics has no pointer arguments and is safe to query repeatedly.
    let height = unsafe { GetSystemMetrics(SM_CYSCREEN) };
    if width <= 0 || height <= 0 {
        return Err(std::io::Error::other(
            "could not determine the display size",
        ));
    }

    Ok(POINT {
        x: width / 2,
        y: height / 2,
    })
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    eprintln!("[debug] main started");

    let mut debug_log = File::create("debug.log")?;
    writeln!(debug_log, "[debug] main started")?;
    writeln!(debug_log, "[debug] press q to quit")?;

    let guard = TerminalGuard::new()?;

    let width = 240;
    let height = 80;

    let mut terminal = Terminal::new(width, height)?;
    let mut fb = Framebuffer::new(width, height);
    writeln!(debug_log, "[debug] terminal created: {}x{}", width, height)?;

    let target_fps = 30;
    let target_frame_time = Duration::from_secs_f64(1.0 / target_fps as f64);

    let mut running = true;
    let mut frame_count = 0_u64;
    let mut last_debug_frame = Instant::now();
    let mut last_frame_start = Instant::now();
    let mut input = MovementInput::default();
    let mut mouse_look = RelativeMouseLook::new()?;

    let mut camera = Camera::new(
        Vec3::new(0.5, 40.0, 5.0),
        0.0,   // yaw
        -0.35, // pitch: start looking slightly down at the terrain
        60.0f32.to_radians(),
    );
    let mut world = World::new(0xA5C1_1A55_C11A_5511);

    while running {
        let frame_start = Instant::now();

        let delta_time = frame_start.duration_since(last_frame_start).as_secs_f32();
        last_frame_start = frame_start;

        // 1. Poll input. Windows reports press, repeat, and release events, so a
        // movement key stays active for its entire hold rather than one frame.
        while event::poll(Duration::from_millis(1))? {
            let ev = event::read()?;
            match ev {
                Event::FocusGained => mouse_look.lock()?,
                Event::FocusLost => {
                    mouse_look.unlock();
                    input.clear();
                }
                Event::Mouse(mouse_event)
                    if matches!(mouse_event.kind, MouseEventKind::Down(_)) =>
                {
                    mouse_look.lock()?;
                }
                Event::Key(key) => {
                    let unlock_requested = key.kind == KeyEventKind::Press
                        && (key.code == KeyCode::Esc
                            || (key.code == KeyCode::Char('c')
                                && key.modifiers.contains(KeyModifiers::CONTROL)));

                    if unlock_requested {
                        mouse_look.unlock();
                        input.clear();
                    } else if key.code == KeyCode::Char('q') && key.kind == KeyEventKind::Press {
                        writeln!(debug_log, "[debug] q pressed; exiting main loop")?;
                        running = false;
                    } else {
                        input.update(key);
                    }
                }
                _ => {}
            }
        }

        mouse_look.update_camera(&mut camera)?;

        // 2. Update
        fb.clear();

        // Update camera position. Space rises; Shift descends. Collision is
        // intentionally absent, so the camera can move freely through the scene.
        let speed = 5.0;
        let move_vec = Vec3::mul_scalar(input.direction(&camera).normalize(), speed * delta_time);
        camera.position = Vec3::add(camera.position, move_vec);

        let view = camera.get_view_matrix();

        let projection =
            Mat4::perspective(camera.fov, width as f32 / (height as f32 * 2.0), 0.1, 100.0);

        let mvp = Mat4::multiply(projection, view);
        let light_dir = Vec3::new(-0.35, 0.65, 0.68).normalize();
        let camera_chunk_x = (camera.position.x.floor() as i32).div_euclid(CHUNK_SIZE);
        let camera_chunk_z = (camera.position.z.floor() as i32).div_euclid(CHUNK_SIZE);
        world.ensure_render_distance(camera_chunk_x, camera_chunk_z);

        let mut visible_faces = 0;
        let mut rasterized_triangles = 0;
        let mut shaded_cells = 0;
        for face in world.visible_faces(camera_chunk_x, camera_chunk_z) {
            let vertices = face.corners.map(|[x, y, z]| Vec3::new(x, y, z));
            let normal = triangle_normal(vertices[0], vertices[1], vertices[2]);
            if Vec3::dot(normal, Vec3::sub(camera.position, vertices[0])) <= 0.0 {
                continue;
            }
            let projected =
                vertices.map(|vertex| project_vertex_with_depth(vertex, &mvp, width, height));
            let (Some(p0), Some(p1), Some(p2), Some(p3)) =
                (projected[0], projected[1], projected[2], projected[3])
            else {
                continue;
            };
            visible_faces += 1;
            let (ch, color) = block_shade(
                face.block,
                0.18 + Vec3::dot(normal, light_dir).max(0.0) * 0.82,
            );
            for (a, b, c) in [(p0, p1, p2), (p0, p2, p3)] {
                let written = rasterize_triangle(&mut fb, a, b, c, ch, color);
                if written > 0 {
                    rasterized_triangles += 1;
                    shaded_cells += written;
                }
            }
        }

        let status =
            "ASCII Minecraft | WASD move | Space up | C down | Esc/Ctrl+C release mouse | q quits";
        for (x, ch) in status.chars().take(width).enumerate() {
            fb.set(x, height - 1, ch);
        }

        if frame_count == 0 || last_debug_frame.elapsed() >= Duration::from_secs(1) {
            writeln!(
                debug_log,
                "[debug] frame={frame_count} position=({:.2}, {:.2}, {:.2}) visible_faces={visible_faces} rasterized_triangles={rasterized_triangles} shaded_cells={shaded_cells}",
                camera.position.x, camera.position.y, camera.position.z,
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

fn triangle_normal(a: Vec3, b: Vec3, c: Vec3) -> Vec3 {
    Vec3::cross(Vec3::sub(b, a), Vec3::sub(c, a)).normalize()
}

fn block_shade(block: Block, brightness: f32) -> (char, TerminalColor) {
    let (material_brightness, color) = match block {
        Block::Grass => (1.0, TerminalColor::Green),
        Block::Dirt => (0.72, TerminalColor::Yellow),
        Block::Stone => (0.48, TerminalColor::Grey),
        Block::Air => (0.0, TerminalColor::Grey),
    };
    (shade_char(brightness * material_brightness), color)
}
