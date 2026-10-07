#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerminalColor {
    /// An xterm-256 palette index.  This keeps materials distinct on capable terminals.
    Ansi256(u8),
    /// A true-colour value used for effects such as water and underwater tinting.
    Rgb(u8, u8, u8),
}

pub struct Framebuffer {
    pub width: usize,
    pub height: usize,
    pub cells: Vec<char>,
    pub colors: Vec<TerminalColor>,
    pub depth: Vec<f32>,
    /// Depth written by translucent geometry.  Kept separate from `depth` so
    /// transparent surfaces do not hide the opaque scene behind them.
    transparent_depth: Vec<f32>,
}

impl Framebuffer {
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            cells: vec![' '; width * height],
            colors: vec![TerminalColor::Ansi256(250); width * height],
            depth: vec![f32::INFINITY; width * height],
            transparent_depth: vec![f32::INFINITY; width * height],
        }
    }

    pub fn clear(&mut self) {
        for cell in self.cells.iter_mut() {
            *cell = ' ';
        }
        for depth in self.depth.iter_mut() {
            *depth = f32::INFINITY;
        }
        for depth in self.transparent_depth.iter_mut() {
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

    pub fn blend_transparent_with_depth(
        &mut self,
        x: usize,
        y: usize,
        depth: f32,
        ch: char,
        color: TerminalColor,
        opacity: f32,
    ) -> bool {
        if x >= self.width || y >= self.height {
            return false;
        }

        let idx = y * self.width + x;
        if depth >= self.depth[idx] || depth >= self.transparent_depth[idx] {
            return false;
        }

        self.transparent_depth[idx] = depth;
        self.colors[idx] = blend_colors(color, self.colors[idx], opacity);
        // Preserve geometry detail visible through water.  Empty cells still
        // receive the water glyph so open water has a readable surface.
        if self.cells[idx] == ' ' {
            self.cells[idx] = ch;
        }
        true
    }

    pub fn tint_blue(&mut self, amount: f32) {
        let tint = TerminalColor::Rgb(45, 122, 220);
        for color in &mut self.colors {
            *color = blend_colors(tint, *color, amount);
        }
    }
}

fn blend_colors(
    foreground: TerminalColor,
    background: TerminalColor,
    opacity: f32,
) -> TerminalColor {
    let opacity = opacity.clamp(0.0, 1.0);
    let (fr, fg, fb) = foreground.rgb();
    let (br, bg, bb) = background.rgb();
    TerminalColor::Rgb(
        (fr as f32 * opacity + br as f32 * (1.0 - opacity)).round() as u8,
        (fg as f32 * opacity + bg as f32 * (1.0 - opacity)).round() as u8,
        (fb as f32 * opacity + bb as f32 * (1.0 - opacity)).round() as u8,
    )
}

impl TerminalColor {
    fn rgb(self) -> (u8, u8, u8) {
        match self {
            Self::Rgb(r, g, b) => (r, g, b),
            Self::Ansi256(index @ 16..=231) => {
                let value = index - 16;
                let component = |n| [0, 95, 135, 175, 215, 255][n as usize];
                (
                    component(value / 36),
                    component((value / 6) % 6),
                    component(value % 6),
                )
            }
            Self::Ansi256(index @ 232..=255) => {
                let gray = 8 + (index - 232) * 10;
                (gray, gray, gray)
            }
            Self::Ansi256(index) => {
                const BASIC: [(u8, u8, u8); 16] = [
                    (0, 0, 0),
                    (205, 0, 0),
                    (0, 205, 0),
                    (205, 205, 0),
                    (0, 0, 238),
                    (205, 0, 205),
                    (0, 205, 205),
                    (229, 229, 229),
                    (127, 127, 127),
                    (255, 0, 0),
                    (0, 255, 0),
                    (255, 255, 0),
                    (92, 92, 255),
                    (255, 0, 255),
                    (0, 255, 255),
                    (255, 255, 255),
                ];
                BASIC[index as usize]
            }
        }
    }
}
