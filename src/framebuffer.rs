#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerminalColor {
    /// An xterm-256 palette index.  This keeps materials distinct on capable terminals.
    Ansi256(u8),
}

pub struct Framebuffer {
    pub width: usize,
    pub height: usize,
    pub cells: Vec<char>,
    pub colors: Vec<TerminalColor>,
    pub depth: Vec<f32>,
}

impl Framebuffer {
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            cells: vec![' '; width * height],
            colors: vec![TerminalColor::Ansi256(250); width * height],
            depth: vec![f32::INFINITY; width * height],
        }
    }

    pub fn clear(&mut self) {
        for cell in self.cells.iter_mut() {
            *cell = ' ';
        }
        for depth in self.depth.iter_mut() {
            *depth = f32::INFINITY;
        }
    }

    pub fn set(&mut self, x: usize, y: usize, ch: char) {
        if x < self.width && y < self.height {
            self.cells[y * self.width + x] = ch;
        }
    }

    pub fn set_with_depth(
        &mut self,
        x: usize,
        y: usize,
        depth: f32,
        ch: char,
        color: TerminalColor,
    ) -> bool {
        if x >= self.width || y >= self.height {
            return false;
        }

        let idx = y * self.width + x;
        if depth < self.depth[idx] {
            self.depth[idx] = depth;
            self.cells[idx] = ch;
            self.colors[idx] = color;
            return true;
        }

        false
    }
}
