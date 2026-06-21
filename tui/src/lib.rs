#![allow(dead_code)]
mod api;
mod app;
mod components;
mod screens;
mod ui;

use crate::api::WalletApi;
use api::mock::MockWalletApi;
use api::real::RealWalletApi;
use app::App;
use screens::greeting::GreetingScreen;
use screens::setup::SetupScreen;
use screens::Screen;

use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::event::{DisableBracketedPaste, EnableBracketedPaste};
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::widgets::{Block, Paragraph};
use ratatui::Frame;
use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;

pub async fn run_tui(socket_path: &str, shutdown: Option<Arc<AtomicBool>>) -> io::Result<()> {
    let api: Box<dyn WalletApi> = match RealWalletApi::connect(socket_path).await {
        Ok(real) => Box::new(real),
        Err(e) => {
            eprintln!("Failed to connect to paypunkd at {socket_path}: {e}");
            eprintln!("Falling back to mock API");
            Box::new(MockWalletApi::new())
        }
    };

    let mut app = App::new(api);

    let wallet_exists = app.api.check_wallet_exists().await;
    if wallet_exists {
        let mut greeting = Box::new(GreetingScreen::new());
        greeting.init(&*app.api).await;
        app.push_screen(greeting);
    } else {
        let mut setup = Box::new(SetupScreen::new());
        setup.init(&*app.api).await;
        app.push_screen(setup);
    }

    let prev_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = ratatui::restore();
        prev_hook(info);
    }));

    let mut terminal = ratatui::init();
    terminal.clear()?;
    crossterm::execute!(std::io::stdout(), EnableBracketedPaste)?;

    let (event_tx, mut event_rx) = mpsc::channel::<Event>(256);
    let event_tx_clone = event_tx.clone();

    tokio::task::spawn_blocking(move || loop {
        if event::poll(std::time::Duration::from_millis(50)).unwrap_or(false) {
            let evt = event::read().unwrap_or(Event::Resize(0, 0));
            if event_tx_clone.blocking_send(evt).is_err() {
                break;
            }
        } else {
            if event_tx_clone.blocking_send(Event::Resize(0, 0)).is_err() {
                break;
            }
        }
    });

    while !app.should_quit {
        if let Some(ref flag) = shutdown {
            if flag.load(Ordering::SeqCst) {
                app.should_quit = true;
                break;
            }
        }

        terminal.draw(|frame| render(frame, &mut app))?;

        if let Some(evt) = event_rx.recv().await {
            match evt {
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    if key.code == KeyCode::Char('q') && app.screen_stack.len() <= 1 {
                        app.should_quit = true;
                    } else if key.modifiers.contains(KeyModifiers::CONTROL)
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
                Event::Resize(_, _) => {}
                _ => {}
            }
        }
    }

    crossterm::execute!(std::io::stdout(), DisableBracketedPaste)?;
    ratatui::restore();
    terminal.show_cursor()?;

    Ok(())
}

fn render(frame: &mut Frame, app: &mut App) {
    let api: &dyn WalletApi = &*app.api;

    let bg_block = Block::new().style(Style::new().bg(ui::BG));
    frame.render_widget(bg_block, frame.area());

    if let Some(screen) = app.screen_stack.last_mut() {
        screen.render(frame, api);
    } else {
        let block = Block::new().style(Style::new().bg(ui::BG));
        frame.render_widget(block, frame.area());
        let msg = Paragraph::new(Line::from("No screen loaded").centered())
            .style(Style::new().fg(ui::palette().error));
        frame.render_widget(msg, frame.area());
    }
}
