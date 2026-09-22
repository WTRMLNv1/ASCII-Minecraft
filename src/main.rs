mod framebuffer;
mod terminal;

use framebuffer::Framebuffer;
use terminal::{Terminal, TerminalGuard};
use crossterm::event::{self, Event, KeyCode};
use std::time::{Duration, Instant};
use std::thread;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _guard = TerminalGuard::new()?;

    let width = 120;
    let height = 40;
    let mut terminal = Terminal::new(width, height)?;
    let mut fb = Framebuffer::new(width, height);

    let target_fps = 30;
    let target_frame_time = Duration::from_secs_f64(1.0 / target_fps as f64);

    let mut running = true;

    while running {
        let frame_start = Instant::now();

        // 1. Poll Input
        if event::poll(Duration::from_millis(0))? {
            if let Event::Key(key) = event::read()? {
                if let KeyCode::Char('q') = key.code {
                    running = false;
                }
            }
        }

        // 2. Update (Test Pattern)
        fb.clear();

        // Draw Border
        for x in 0..width {
            fb.set(x, 0, '-');
            fb.set(x, height - 1, '-');
        }
        for y in 0..height {
            fb.set(0, y, '|');
            fb.set(width - 1, y, '|');
        }
        fb.set(0, 0, '+');
        fb.set(width - 1, 0, '+');
        fb.set(0, height - 1, '+');
        fb.set(width - 1, height - 1, '+');

        // Draw Text
        let title = "ASCII MINECRAFT";
        let title_x = (width - title.len()) / 2;
        for (i, c) in title.chars().enumerate() {
            fb.set(title_x + i, height / 2 - 1, c);
        }

        let subtitle = "TERMINAL ENGINE";
        let subtitle_x = (width - subtitle.len()) / 2;
        for (i, c) in subtitle.chars().enumerate() {
            fb.set(subtitle_x + i, height / 2, c);
        }

        let fps_text = format!("FPS: {}", target_fps);
        let fps_x = (width - fps_text.len()) / 2;
        for (i, c) in fps_text.chars().enumerate() {
            fb.set(fps_x + i, height / 2 + 2, c);
        }

        // 3. Present
        terminal.present(&fb)?;

        // 4. Timing
        let elapsed = frame_start.elapsed();
        if elapsed < target_frame_time {
            thread::sleep(target_frame_time - elapsed);
        }
    }

    Ok(())
}
