use crossterm::{
    cursor,
    execute,
    terminal::{self, disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand,
};
use std::io::{stdout, Write, BufWriter};
use crate::framebuffer::Framebuffer;

pub struct TerminalGuard;

impl TerminalGuard {
    pub fn new() -> Result<Self, std::io::Error> {
        enable_raw_mode()?;
        execute!(stdout(), EnterAlternateScreen, cursor::Hide)?;
        // Clear the screen once at startup
        execute!(stdout(), terminal::Clear(terminal::ClearType::All));
        Ok(Self)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let mut stdout = stdout();
        let _ = execute!(stdout, cursor::Show, LeaveAlternateScreen);
        let _ = disable_raw_mode();
    }
}

pub struct Terminal {
    stdout: BufWriter<std::io::Stdout>,
    previous_frame: Vec<char>,
}

impl Terminal {
    pub fn new(width: usize, height: usize) -> Result<Self, std::io::Error> {
        Ok(Self {
            stdout: BufWriter::new(stdout()),
            previous_frame: vec![' '; width * height],
        })
    }

    pub fn present(&mut self, fb: &Framebuffer) -> Result<(), std::io::Error> {
        for y in 0..fb.height {
            for x in 0..fb.width {
                let idx = y * fb.width + x;
                let current_char = fb.cells[idx];

                if current_char != self.previous_frame[idx] {
                    self.stdout.execute(cursor::MoveTo(x as u16, y as u16))?;
                    write!(self.stdout, "{}", current_char)?;
                    self.previous_frame[idx] = current_char;
                }
            }
        }
        self.stdout.flush()?;
        Ok(())
    }
}
