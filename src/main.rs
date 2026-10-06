mod framebuffer;
mod math;
mod rendering;
mod terminal;
mod world;

use framebuffer::{Framebuffer, TerminalColor};
use terminal::{Terminal, TerminalGuard};

use crossterm::event::{
    self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseEventKind,
};

use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

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

    fn direction(&self, camera: &Camera, is_flying: bool) -> Vec3 {
        let mut direction = Vec3::new(0.0, 0.0, 0.0);

        if self.forward {
            let fwd = camera.get_forward();
            direction = Vec3::add(direction, Vec3::new(fwd.x, 0.0, fwd.z));
        }
        if self.backward {
            let fwd = camera.get_forward();
            direction = Vec3::sub(direction, Vec3::new(fwd.x, 0.0, fwd.z));
        }
        if self.left {
            let right = camera.get_right();
            direction = Vec3::sub(direction, Vec3::new(right.x, 0.0, right.z));
        }
        if self.right {
            let right = camera.get_right();
            direction = Vec3::add(direction, Vec3::new(right.x, 0.0, right.z));
        }
        if is_flying && self.up {
            direction = Vec3::add(direction, Vec3::new(0.0, 1.0, 0.0));
        }
        if is_flying && self.down {
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

enum GameState {
    StartScreen {
        selected_index: usize,
        seed_text: String,
        cursor_pos: usize,
    },
    Playing {
        world: World,
        camera: Camera,
        input: MovementInput,
        mouse_look: RelativeMouseLook,
        is_flying: bool,
        velocity_y: f32,
        last_space_press: Option<Instant>,
        is_grounded: bool,
    },
}

fn hash_seed(text: &str) -> u64 {
    if text.is_empty() {
        return SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
    }
    let mut hash: u64 = 0xcbf29ce484222325;
    let prime: u64 = 0x100000001b3;
    for byte in text.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(prime);
    }
    hash
}

fn render_start_screen(
    fb: &mut Framebuffer,
    selected_index: usize,
    seed_text: &str,
    cursor_pos: usize,
) {
    let width = fb.width;
    let height = fb.height;

    // Fill the entire terminal with a subtle framed title screen.
    for y in 0..height {
        for x in 0..width {
            let ch = if y == 0 || y == height - 1 || x == 0 || x == width - 1 {
                '#'
            } else if y == 1 || y == height - 2 || x == 1 || x == width - 2 {
                '.'
            } else {
                ' '
            };
            fb.set(x, y, ch);
        }
    }

    // Large block/ASCII "AsciiMLN" logo.
    // Each character is 5 columns wide, with a single-column gap.
    // Large block/ASCII "AsciiMLN" logo.
    const LOGO: [&str; 6] = [
        " █████╗ ███████╗ ██████╗██╗██╗   ███╗   ███╗██╗     ███╗   ██╗",
        "██╔══██╗██╔════╝██╔════╝██║██║   ████╗ ████║██║     ████╗  ██║",
        "███████║███████╗██║     ██║██║   ██╔████╔██║██║     ██╔██╗ ██║",
        "██╔══██║╚════██║██║     ██║██║   ██║╚██╔╝██║██║     ██║╚██╗██║",
        "██║  ██║███████║╚██████╗██║██║   ██║ ╚═╝ ██║███████╗██║ ╚████║",
        "╚═╝  ╚═╝╚══════╝ ╚═════╝╚═╝╚═╝   ╚═╝     ╚═╝╚══════╝╚═╝  ╚═══╝",
    ];
    let subtitle = "TERMINAL WORLD ENGINE";

    const BOX_WIDTH: usize = 46;
    const BOX_HEIGHT: usize = 3;
    let items = ["PLAY", "WORLD SEED", "QUIT"];
    // Total height of just the stacked boxes, no trailing gap after the last one.
    let menu_block_height = items.len() * BOX_HEIGHT + (items.len() - 1);

    // Gaps between the sections, in rows.
    let gap_logo_subtitle = 1;
    let gap_subtitle_menu = 2;

    let content_height =
        LOGO.len() + gap_logo_subtitle + 1 /*subtitle*/ + gap_subtitle_menu + menu_block_height;
    let content_start_y = (height.saturating_sub(content_height)) / 2;

    // --- Logo ---
    let logo_width = LOGO
        .iter()
        .map(|line| line.chars().count())
        .max()
        .unwrap_or(0);
    let logo_x = width.saturating_sub(logo_width) / 2;
    let logo_y = content_start_y;

    for (row, line) in LOGO.iter().enumerate() {
        for (col, ch) in line.chars().enumerate() {
            if logo_x + col < width && logo_y + row < height {
                fb.set(logo_x + col, logo_y + row, ch);
            }
        }
    }

    // --- Subtitle ---
    let sx = width.saturating_sub(subtitle.len()) / 2;
    let sy = logo_y + LOGO.len() + gap_logo_subtitle;
    for (i, ch) in subtitle.chars().enumerate() {
        if sx + i < width && sy < height {
            fb.set(sx + i, sy, ch);
        }
    }

    // --- Menu ---
    let menu_x = width.saturating_sub(BOX_WIDTH) / 2;
    let menu_y = sy + 1 + gap_subtitle_menu;

    for (i, item) in items.iter().enumerate() {
        let y = menu_y + i * BOX_HEIGHT; // boxes are flush-stacked, no per-item gap
        if y + BOX_HEIGHT > height.saturating_sub(3) {
            continue;
        }

        let selected = i == selected_index;

        // Box.
        let left = if selected { '>' } else { '|' };
        let right = if selected { '<' } else { '|' };

        for x in 0..BOX_WIDTH {
            let ch = if x == 0 {
                '╔'
            } else if x == BOX_WIDTH - 1 {
                '╗'
            } else {
                '═'
            };
            fb.set(menu_x + x, y, ch);

            let bottom = if x == 0 {
                '╚'
            } else if x == BOX_WIDTH - 1 {
                '╝'
            } else {
                '═'
            };
            fb.set(menu_x + x, y + BOX_HEIGHT - 1, bottom);
        }

        for yy in 1..BOX_HEIGHT - 1 {
            fb.set(menu_x, y + yy, '║');
            fb.set(menu_x + BOX_WIDTH - 1, y + yy, '║');
        }

        let label = if i == 1 {
            format!("WORLD SEED: {}", seed_text)
        } else {
            (*item).to_string()
        };

        let label_width = label.chars().count();
        let label_x = menu_x + (BOX_WIDTH.saturating_sub(label_width)) / 2;

        for (j, ch) in label.chars().enumerate() {
            if label_x + j < menu_x + BOX_WIDTH - 1 {
                fb.set(label_x + j, y + 1, ch);
            }
        }

        // Selection arrows make the active control much more obvious.
        if selected {
            fb.set(menu_x + 2, y + 1, left);
            fb.set(menu_x + BOX_WIDTH - 3, y + 1, right);
        }

        // Seed cursor.
        if i == 1 && selected {
            let prefix_width = "WORLD SEED: ".chars().count();
            let cursor_x = label_x + prefix_width + cursor_pos;
            if cursor_x < menu_x + BOX_WIDTH - 2 {
                fb.set(cursor_x, y + 1, '_');
            }
        }
    }

    // Controls hint.
    let hint = "↑ / ↓  SELECT     ENTER  CONFIRM     Q  QUIT";
    let hx = width.saturating_sub(hint.len()) / 2;
    let hy = height.saturating_sub(3);
    for (i, ch) in hint.chars().enumerate() {
        if hx + i < width {
            fb.set(hx + i, hy, ch);
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let guard = TerminalGuard::new()?;

    let (width, height) = crossterm::terminal::size()
        .map(|(w, h)| (w as usize, h as usize))
        .unwrap_or((240, 80));

    let mut width = width;
    let mut height = height;

    let mut terminal = Terminal::new(width, height)?;
    let mut fb = Framebuffer::new(width, height);

    let target_fps = 30;
    let target_frame_time = Duration::from_secs_f64(1.0 / target_fps as f64);

    let mut running = true;
    let mut last_frame_start = Instant::now();

    let mut state = GameState::StartScreen {
        selected_index: 0,
        seed_text: String::new(),
        cursor_pos: 0,
    };

    while running {
        let frame_start = Instant::now();
        let delta_time = frame_start.duration_since(last_frame_start).as_secs_f32();
        last_frame_start = frame_start;

        // 1. Poll input
        while event::poll(Duration::from_millis(1))? {
            let ev = event::read()?;
            match ev {
                Event::Key(key) => {
                    match &mut state {
                        GameState::StartScreen {
                            selected_index,
                            seed_text,
                            cursor_pos,
                        } => {
                            if key.kind == KeyEventKind::Press {
                                match key.code {
                                    KeyCode::Up => {
                                        *selected_index = if *selected_index == 0 {
                                            2
                                        } else {
                                            *selected_index - 1
                                        };
                                    }
                                    KeyCode::Down => {
                                        *selected_index = (*selected_index + 1) % 3;
                                    }
                                    KeyCode::Enter => {
                                        if *selected_index == 0 {
                                            let seed = hash_seed(seed_text);
                                            let mut mouse_look = RelativeMouseLook::new()?;
                                            state = GameState::Playing {
                                                world: World::new(seed),
                                                camera: Camera::new(
                                                    Vec3::new(0.5, 40.0, 5.0),
                                                    0.0,
                                                    -0.35,
                                                    60.0f32.to_radians(),
                                                ),
                                                input: MovementInput::default(),
                                                mouse_look,
                                                is_flying: true,
                                                velocity_y: 0.0,
                                                last_space_press: None,
                                                is_grounded: false,
                                            };
                                        } else if *selected_index == 2 {
                                            running = false;
                                        }
                                    }
                                    KeyCode::Char(c) if *selected_index == 1 => {
                                        seed_text.insert(*cursor_pos, c);
                                        *cursor_pos += 1;
                                    }
                                    KeyCode::Backspace if *selected_index == 1 => {
                                        if *cursor_pos > 0 {
                                            *cursor_pos -= 1;
                                            seed_text.remove(*cursor_pos);
                                        }
                                    }
                                    KeyCode::Left if *selected_index == 1 => {
                                        if *cursor_pos > 0 {
                                            *cursor_pos -= 1;
                                        }
                                    }
                                    KeyCode::Right if *selected_index == 1 => {
                                        if *cursor_pos < seed_text.len() {
                                            *cursor_pos += 1;
                                        }
                                    }
                                    KeyCode::Char('q') => running = false,
                                    _ => {}
                                }
                            }
                        }
                        GameState::Playing {
                            world,
                            camera,
                            input,
                            mouse_look,
                            is_flying,
                            velocity_y,
                            last_space_press,
                            is_grounded,
                        } => {
                            let unlock_requested = key.kind == KeyEventKind::Press
                                && (key.code == KeyCode::Esc
                                    || (key.code == KeyCode::Char('c')
                                        && key.modifiers.contains(KeyModifiers::CONTROL)));

                            if unlock_requested {
                                mouse_look.unlock();
                                input.clear();
                            } else if key.code == KeyCode::Char('q')
                                && key.kind == KeyEventKind::Press
                            {
                                running = false;
                            } else if key.code == KeyCode::Char(' ') {
                                // Windows sends repeated Press events while a key is held.  Only the
                                // transition from released to pressed is a tap; repeats must not count
                                // toward the double-tap flight toggle.
                                let is_new_space_press =
                                    key.kind == KeyEventKind::Press && !input.up;

                                if is_new_space_press {
                                    let now = Instant::now();
                                    if let Some(last_press) = *last_space_press {
                                        if now.duration_since(last_press).as_millis() < 300 {
                                            *is_flying = !*is_flying;
                                            *velocity_y = 0.0;
                                            *last_space_press = None;
                                        } else {
                                            *last_space_press = Some(now);
                                            if !*is_flying && *is_grounded {
                                                *velocity_y = 7.75;
                                            }
                                        }
                                    } else {
                                        *last_space_press = Some(now);
                                        if !*is_flying && *is_grounded {
                                            *velocity_y = 7.75;
                                        }
                                    }
                                }

                                input.update(key);
                            } else if key.kind == KeyEventKind::Press {
                                input.update(key);
                            } else {
                                input.update(key);
                            }
                        }
                    }
                }
                Event::FocusGained => {
                    if let GameState::Playing { mouse_look, .. } = &mut state {
                        mouse_look.lock()?;
                    }
                }
                Event::FocusLost => {
                    if let GameState::Playing {
                        mouse_look, input, ..
                    } = &mut state
                    {
                        mouse_look.unlock();
                        input.clear();
                    }
                }
                Event::Mouse(mouse_event)
                    if matches!(mouse_event.kind, MouseEventKind::Down(_)) =>
                {
                    if let GameState::Playing {
                        mouse_look,
                        world,
                        camera,
                        ..
                    } = &mut state
                    {
                        mouse_look.lock()?;

                        if let crossterm::event::MouseEventKind::Down(button) = mouse_event.kind {
                            if button == crossterm::event::MouseButton::Left {
                                let eye_position = Vec3::new(
                                    camera.position.x + 0.5,
                                    camera.position.y + 1.5,
                                    camera.position.z + 0.5,
                                );
                                let direction = camera.get_forward();

                                if let Some((hit_pos, _, _)) =
                                    world.raycast(eye_position, direction, 4.5)
                                {
                                    world.set_block(hit_pos.x, hit_pos.y, hit_pos.z, Block::Air);
                                }
                            } else if button == crossterm::event::MouseButton::Right {
                                let eye_position = Vec3::new(
                                    camera.position.x + 0.5,
                                    camera.position.y + 1.5,
                                    camera.position.z + 0.5,
                                );
                                let direction = camera.get_forward();

                                if let Some((hit_pos, face, _)) =
                                    world.raycast(eye_position, direction, 4.5)
                                {
                                    let (px, py, pz) = match face {
                                        crate::world::FaceDirection::West if direction.x > 0.0 => {
                                            (hit_pos.x - 1, hit_pos.y, hit_pos.z)
                                        }
                                        crate::world::FaceDirection::East if direction.x < 0.0 => {
                                            (hit_pos.x + 1, hit_pos.y, hit_pos.z)
                                        }
                                        crate::world::FaceDirection::Top if direction.y < 0.0 => {
                                            (hit_pos.x, hit_pos.y + 1, hit_pos.z)
                                        }
                                        crate::world::FaceDirection::Top if direction.y > 0.0 => {
                                            (hit_pos.x, hit_pos.y - 1, hit_pos.z)
                                        }
                                        crate::world::FaceDirection::North if direction.z > 0.0 => {
                                            (hit_pos.x, hit_pos.y, hit_pos.z - 1)
                                        }
                                        crate::world::FaceDirection::South if direction.z < 0.0 => {
                                            (hit_pos.x, hit_pos.y, hit_pos.z + 1)
                                        }
                                        _ => (hit_pos.x, hit_pos.y, hit_pos.z), // Fallback
                                    };

                                    // Hitbox clipping check
                                    let p_min_x = camera.position.x;
                                    let p_max_x = camera.position.x + 0.6;
                                    let p_min_y = camera.position.y;
                                    let p_max_y = camera.position.y + 2.0;
                                    let p_min_z = camera.position.z;
                                    let p_max_z = camera.position.z + 0.6;

                                    let b_min_x = px as f32;
                                    let b_max_x = px as f32 + 1.0;
                                    let b_min_y = py as f32;
                                    let b_max_y = py as f32 + 1.0;
                                    let b_min_z = pz as f32;
                                    let b_max_z = pz as f32 + 1.0;

                                    let intersects = (p_min_x < b_max_x && p_max_x > b_min_x)
                                        && (p_min_y < b_max_y && p_max_y > b_min_y)
                                        && (p_min_z < b_max_z && p_max_z > b_min_z);

                                    if !intersects {
                                        world.set_block(px, py, pz, Block::Stone);
                                    }
                                }
                            }
                        }
                    }
                }
                Event::Resize(new_w, new_h) => {
                    width = new_w as usize;
                    height = new_h as usize;
                    fb = Framebuffer::new(width, height);
                    terminal.resize(width, height);
                }
                _ => {}
            }
        }

        fb.clear();

        match &mut state {
            GameState::StartScreen {
                selected_index,
                seed_text,
                cursor_pos,
            } => {
                render_start_screen(&mut fb, *selected_index, seed_text, *cursor_pos);
            }
            GameState::Playing {
                world,
                camera,
                input,
                mouse_look,
                is_flying,
                velocity_y,
                last_space_press,
                is_grounded,
            } => {
                mouse_look.update_camera(camera)?;

                let speed = 5.0;
                let gravity = -20.0;
                let delta_time = delta_time;

                // Apply gravity if not flying
                if !*is_flying {
                    *velocity_y += gravity * delta_time;
                }

                // Ground detection
                let mut grounded = false;
                let hitbox_width = 0.6;
                let hitbox_depth = 0.6;
                let hitbox_height = 2.0;
                // `camera.position` is the bottom of the player's hitbox.  Check the
                // block immediately below it, not the (normally empty) feet cell.
                let ground_y = (camera.position.y - 0.05).floor() as i32;

                let ground_offsets = [
                    (0.0, 0.0),
                    (hitbox_width, 0.0),
                    (0.0, hitbox_depth),
                    (hitbox_width, hitbox_depth),
                ];
                for (ox, oz) in ground_offsets {
                    let wx = (camera.position.x + ox).floor() as i32;
                    let wz = (camera.position.z + oz).floor() as i32;
                    if world.get_block(wx, ground_y, wz) != Block::Air {
                        grounded = true;
                        break;
                    }
                }
                *is_grounded = grounded;

                let move_vec_dir = input.direction(camera, *is_flying);
                let mut move_vec = Vec3::mul_scalar(move_vec_dir.normalize(), speed * delta_time);

                // Apply vertical velocity
                if !*is_flying {
                    move_vec.y = *velocity_y * delta_time;
                }

                let mut next_position = camera.position;

                // Resolve X-axis movement
                let next_pos_x = Vec3::add(camera.position, Vec3::new(move_vec.x, 0.0, 0.0));
                let mut collided_x = false;
                let x_offsets = [
                    (0.0, 0.0),
                    (hitbox_width, 0.0),
                    (0.0, hitbox_depth),
                    (hitbox_width, hitbox_depth),
                ];
                for (ox, oz) in x_offsets {
                    for py in [0.0, hitbox_height - 0.1] {
                        let world_x = (next_pos_x.x + ox).floor() as i32;
                        let world_y = (next_pos_x.y + py).floor() as i32;
                        let world_z = (next_pos_x.z + oz).floor() as i32;
                        if world.get_block(world_x, world_y, world_z) != Block::Air {
                            collided_x = true;
                            break;
                        }
                    }
                    if collided_x {
                        break;
                    }
                }
                if !collided_x {
                    next_position.x = next_pos_x.x;
                }

                // Resolve Z-axis movement
                let next_pos_z = Vec3::add(next_position, Vec3::new(0.0, 0.0, move_vec.z));
                let mut collided_z = false;
                let z_offsets = [
                    (0.0, 0.0),
                    (hitbox_width, 0.0),
                    (0.0, hitbox_depth),
                    (hitbox_width, hitbox_depth),
                ];
                for (ox, oz) in z_offsets {
                    for py in [0.0, hitbox_height - 0.1] {
                        let world_x = (next_pos_z.x + ox).floor() as i32;
                        let world_y = (next_pos_z.y + py).floor() as i32;
                        let world_z = (next_pos_z.z + oz).floor() as i32;
                        if world.get_block(world_x, world_y, world_z) != Block::Air {
                            collided_z = true;
                            break;
                        }
                    }
                    if collided_z {
                        break;
                    }
                }
                if !collided_z {
                    next_position.z = next_pos_z.z;
                }

                // Resolve Y-axis movement
                let next_pos_y = Vec3::add(next_position, Vec3::new(0.0, move_vec.y, 0.0));
                let mut collided_y = false;
                let y_offsets = [
                    (0.0, 0.0),
                    (hitbox_width, 0.0),
                    (0.0, hitbox_depth),
                    (hitbox_width, hitbox_depth),
                ];
                for (ox, oz) in y_offsets {
                    for py in [0.0, hitbox_height - 0.1] {
                        let world_x = (next_pos_y.x + ox).floor() as i32;
                        let world_y = (next_pos_y.y + py).floor() as i32;
                        let world_z = (next_pos_y.z + oz).floor() as i32;
                        if world.get_block(world_x, world_y, world_z) != Block::Air {
                            collided_y = true;
                            break;
                        }
                    }
                    if collided_y {
                        break;
                    }
                }
                if !collided_y {
                    next_position.y = next_pos_y.y;
                } else {
                    // Handle vertical velocity reset on collision
                    if *velocity_y < 0.0 {
                        *velocity_y = 0.0;
                    } else if *velocity_y > 0.0 {
                        *velocity_y = 0.0;
                    }
                }

                camera.position = next_position;

                let eye_position = Vec3::new(
                    camera.position.x + 0.5,
                    camera.position.y + 1.5,
                    camera.position.z + 0.5,
                );
                let view = camera.get_view_matrix_at(eye_position);

                let projection =
                    Mat4::perspective(camera.fov, width as f32 / (height as f32 * 2.0), 0.1, 100.0);

                let mvp = Mat4::multiply(projection, view);
                let light_dir = Vec3::new(-0.35, 0.65, 0.68).normalize();
                let camera_chunk_x = (camera.position.x.floor() as i32).div_euclid(CHUNK_SIZE);
                let camera_chunk_z = (camera.position.z.floor() as i32).div_euclid(CHUNK_SIZE);
                world.ensure_render_distance(camera_chunk_x, camera_chunk_z);

                for face in world.visible_faces(camera_chunk_x, camera_chunk_z) {
                    let vertices = face.corners.map(|[x, y, z]| Vec3::new(x, y, z));
                    let normal = triangle_normal(vertices[0], vertices[1], vertices[2]);
                    if Vec3::dot(normal, Vec3::sub(eye_position, vertices[0])) <= 0.0 {
                        continue;
                    }
                    let projected = vertices
                        .map(|vertex| project_vertex_with_depth(vertex, &mvp, width, height));
                    let (Some(p0), Some(p1), Some(p2), Some(p3)) =
                        (projected[0], projected[1], projected[2], projected[3])
                    else {
                        continue;
                    };
                    let (ch, color) = block_shade(
                        face.block,
                        0.18 + Vec3::dot(normal, light_dir).max(0.0) * 0.82,
                    );
                    for (a, b, c) in [(p0, p1, p2), (p0, p2, p3)] {
                        rasterize_triangle(&mut fb, a, b, c, ch, color);
                    }
                }

                let status = "ASCII Minecraft | WASD move | Space jump/up | double Space fly | C down | Esc/Ctrl+C release mouse | q quits";
                for (x, ch) in status.chars().take(width).enumerate() {
                    fb.set(x, height - 1, ch);
                }
            }
        }

        terminal.render(&fb)?;

        let frame_elapsed = frame_start.elapsed();
        if frame_elapsed < target_frame_time {
            thread::sleep(target_frame_time - frame_elapsed);
        }
    }

    drop(guard);

    Ok(())
}

fn triangle_normal(a: Vec3, b: Vec3, c: Vec3) -> Vec3 {
    Vec3::cross(Vec3::sub(b, a), Vec3::sub(c, a)).normalize()
}

fn block_shade(block: Block, brightness: f32) -> (char, TerminalColor) {
    let (material_brightness, color) = match block {
        // xterm's 256-colour palette: natural greens, warm soil, neutral stone,
        // and a dark bark that remain readable under the lighting gradient.
        Block::Grass => (1.0, TerminalColor::Ansi256(71)),
        Block::Dirt => (0.72, TerminalColor::Ansi256(137)),
        Block::Stone => (0.48, TerminalColor::Ansi256(246)),
        Block::Log => (0.66, TerminalColor::Ansi256(94)),
        Block::Leaves => (0.88, TerminalColor::Ansi256(34)),
        Block::Air => (0.0, TerminalColor::Ansi256(250)),
    };
    (shade_char(brightness * material_brightness), color)
}
