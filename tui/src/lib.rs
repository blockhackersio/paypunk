#![allow(dead_code)]
mod api;
mod app;
mod components;
mod screens;
mod ui;

use crate::api::WalletApi;
use app::App;
use api::mock::MockWalletApi;
use screens::setup::SetupScreen;
use screens::Screen;

use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::event::{EnableBracketedPaste, DisableBracketedPaste};
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::widgets::{Block, Paragraph};
use ratatui::Frame;
use std::io;

#[tokio::main]
pub async fn run_tui() -> io::Result<()> {
    let api = MockWalletApi::new();

    let mut app = App::new(Box::new(api));
    let mut setup = Box::new(SetupScreen::new());
    setup.init(&*app.api).await;
    app.push_screen(setup);

    let prev_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = ratatui::restore();
        prev_hook(info);
    }));

    let mut terminal = ratatui::init();
    terminal.clear()?;
    crossterm::execute!(std::io::stdout(), EnableBracketedPaste)?;

    let res = run_app(&mut terminal, &mut app).await;

    crossterm::execute!(std::io::stdout(), DisableBracketedPaste)?;
    ratatui::restore();
    terminal.show_cursor()?;

    if let Err(e) = res {
        eprintln!("Error: {}", e);
    }

    Ok(())
}

async fn run_app(
    terminal: &mut ratatui::DefaultTerminal,
    app: &mut App,
) -> io::Result<()> {
    while !app.should_quit {
        terminal.draw(|frame| {
            render(frame, app);
        })?;

        if event::poll(std::time::Duration::from_millis(100))? {
            let evt = event::read()?;
            match evt {
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    if key.code == KeyCode::Char('q') && app.screen_stack.len() <= 1 {
                        app.should_quit = true;
                    } else if key.modifiers.contains(event::KeyModifiers::CONTROL)
                        && key.code == KeyCode::Char('c')
                    {
                        app.should_quit = true;
                    } else {
                        app.handle_input(key).await?;
                        if app.screen_stack.is_empty() {
                            app.should_quit = true;
                        }
                    }
                }
                Event::Paste(text) => {
                    app.handle_paste(&text).await;
                }
                Event::Resize(_, _) => {
                }
                _ => {}
            }
        }
    }
    Ok(())
}

fn render(frame: &mut Frame, app: &mut App) {
    let api: &dyn WalletApi = &*app.api;

    let bg_block = Block::new().style(Style::new().bg(ui::BG));
    frame.render_widget(bg_block, frame.area());

    if let Some(screen) = app.screen_stack.last_mut() {
        screen.render(frame, api);
    } else {
        let block = Block::new()
            .style(Style::new().bg(ui::BG));
        frame.render_widget(block, frame.area());
        let msg = Paragraph::new(Line::from("No screen loaded").centered())
            .style(Style::new().fg(ui::palette().error));
        frame.render_widget(msg, frame.area());
    }
}
