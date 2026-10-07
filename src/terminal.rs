use crate::framebuffer::{Framebuffer, TerminalColor};
use crossterm::{
    ExecutableCommand, cursor,
    event::{DisableFocusChange, DisableMouseCapture, EnableFocusChange, EnableMouseCapture},
    execute,
    style::{Color, ResetColor, SetBackgroundColor, SetForegroundColor},
    terminal::{
        self, EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
    },
};
use std::io::{BufWriter, Write, stdout};

pub struct TerminalGuard;

impl TerminalGuard {
    pub fn new() -> Result<Self, std::io::Error> {
        enable_raw_mode()?;
        execute!(
            stdout(),
            EnterAlternateScreen,
            cursor::Hide,
            SetBackgroundColor(Color::Black),
            SetForegroundColor(Color::AnsiValue(250)),
            EnableFocusChange,
            EnableMouseCapture
        )?;
        execute!(stdout(), terminal::Clear(terminal::ClearType::All))?;
        Ok(Self)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let mut stdout = stdout();
        let _ = execute!(
            stdout,
            cursor::Show,
            ResetColor,
            DisableFocusChange,
            DisableMouseCapture,
            LeaveAlternateScreen
        );
        let _ = disable_raw_mode();
    }
}

pub struct Terminal {
    stdout: BufWriter<std::io::Stdout>,
    previous_frame: Vec<char>,
    previous_colors: Vec<TerminalColor>,
    active_color: TerminalColor,
}

impl Terminal {
    pub fn new(width: usize, height: usize) -> Result<Self, std::io::Error> {
        Ok(Self {
            stdout: BufWriter::new(stdout()),
            previous_frame: vec![' '; width * height],
            previous_colors: vec![TerminalColor::Ansi256(250); width * height],
            active_color: TerminalColor::Ansi256(250),
        })
    }

    pub fn resize(&mut self, width: usize, height: usize) {
        self.previous_frame = vec![' '; width * height];
        self.previous_colors = vec![TerminalColor::Ansi256(250); width * height];
    }

    pub fn render(&mut self, fb: &Framebuffer) -> Result<(), std::io::Error> {
        for y in 0..fb.height {
            for x in 0..fb.width {
                let idx = y * fb.width + x;
                let current_char = fb.cells[idx];
                let current_color = fb.colors[idx];

                if current_char != self.previous_frame[idx]
                    || current_color != self.previous_colors[idx]
                {
                    self.stdout.execute(cursor::MoveTo(x as u16, y as u16))?;

                    if current_color != self.active_color {
                        self.stdout
                            .execute(SetForegroundColor(color_to_crossterm(current_color)))?;

                        self.active_color = current_color;
                    }

                    write!(self.stdout, "{}", current_char)?;

                    self.previous_frame[idx] = current_char;
                    self.previous_colors[idx] = current_color;
                }
            }
        }

        self.stdout.flush()?;
        Ok(())
    }
}

fn color_to_crossterm(color: TerminalColor) -> Color {
    match color {
        TerminalColor::Ansi256(index) => Color::AnsiValue(index),
        TerminalColor::Rgb(r, g, b) => Color::Rgb { r, g, b },
    }
}
