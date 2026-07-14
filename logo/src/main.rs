use std::{
    io::{self, stdout},
    time::{Duration as StdDuration, Instant},
};

use ratatui::{
    crossterm::{
        event::{self, Event, KeyCode},
        execute,
        terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
    },
    prelude::*,
};
use tachyonfx::{Effect, EffectRenderer, Interpolation, Motion, fx};

// ── palette lifted from the source image ────────────────────────────────
const MAUVE: Color = Color::Rgb(0xA9, 0x99, 0xAF); // muted lilac field
const PURPLE: Color = Color::Rgb(0x95, 0x53, 0x9A); // saturated plum
const PAPER: Color = Color::Rgb(0xFF, 0xFF, 0xFF); // the counter / bowl

// 0 = mauve, 1 = purple, 2 = white
const GRID: [[u8; 5]; 6] = [
    [0, 0, 0, 0, 0],
    [0, 2, 2, 2, 1],
    [0, 2, 1, 2, 1],
    [0, 2, 2, 2, 1],
    [0, 2, 1, 1, 1],
    [1, 1, 1, 1, 1],
];

/// One logical pixel = CELL_W columns × CELL_H rows, which compensates for the
/// ~1:2 aspect ratio of a terminal cell and keeps the mark roughly square.
const CELL_W: u16 = 6;
const CELL_H: u16 = 3;
const BLOCK: &str = "█";

const LOGO_W: u16 = 5 * CELL_W;
const LOGO_H: u16 = 6 * CELL_H;

struct Logo;

impl Widget for Logo {
    fn render(self, area: Rect, buf: &mut Buffer) {
        for (gy, row) in GRID.iter().enumerate() {
            for (gx, code) in row.iter().enumerate() {
                let color = match code {
                    0 => MAUVE,
                    1 => PURPLE,
                    _ => PAPER,
                };

                let x0 = area.x + gx as u16 * CELL_W;
                let y0 = area.y + gy as u16 * CELL_H;

                for dy in 0..CELL_H {
                    for dx in 0..CELL_W {
                        let (x, y) = (x0 + dx, y0 + dy);
                        if x < area.right() && y < area.bottom() {
                            buf[(x, y)]
                                .set_symbol(BLOCK)
                                .set_fg(color)
                                .set_bg(Color::Reset);
                        }
                    }
                }
            }
        }
    }
}

/// Wipe the mark in from the top, then let it breathe with a slow luminance
/// pulse so it never sits completely static.
fn build_effect() -> Effect {
    fx::sequence(&[
        fx::sweep_in(
            Motion::UpToDown,
            12,
            0,
            Color::Black,
            (1400, Interpolation::QuadOut),
        ),
        fx::repeating(fx::ping_pong(fx::hsl_shift_fg(
            [0.0, 0.0, 14.0],
            (2600, Interpolation::SineInOut),
        ))),
    ])
}

fn centered(area: Rect, w: u16, h: u16) -> Rect {
    let x = area.x + area.width.saturating_sub(w) / 2;
    let y = area.y + area.height.saturating_sub(h) / 2;
    Rect::new(x, y, w.min(area.width), h.min(area.height))
}

fn main() -> io::Result<()> {
    enable_raw_mode()?;
    execute!(stdout(), EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;

    let mut effect = build_effect();
    let mut last = Instant::now();

    loop {
        let elapsed = last.elapsed();
        last = Instant::now();

        terminal.draw(|f| {
            let area = centered(f.area(), LOGO_W, LOGO_H);
            f.render_widget(Logo, area);
            f.render_effect(&mut effect, area, elapsed.into());
        })?;

        if event::poll(StdDuration::from_millis(16))? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => break,
                    KeyCode::Char('r') => effect = build_effect(),
                    _ => {}
                }
            }
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}
