# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands
- Build: `cargo build`
- Run: `cargo run`
- Test: `cargo test`
- Check: `cargo check`

## Architecture
The project is a terminal-based engine written in Rust, designed to render ASCII graphics.

### Core Components
- `main.rs`: Entry point. Implements the main game loop: polling input, updating the framebuffer, and presenting it to the terminal.
- `framebuffer.rs`: A simple 2D buffer of characters (`Vec<char>`) used to compose a frame before rendering.
- `terminal.rs`: Handles low-level terminal manipulation using `crossterm`.
    - `TerminalGuard`: RAII guard that enables raw mode, enters the alternate screen, and hides the cursor on creation, restoring them on drop.
    - `Terminal`: Implements an optimized `present` method that only updates characters in the terminal that have changed since the last frame (dirty-rect-like optimization).
