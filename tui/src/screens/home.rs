use crate::api::types::*;
use crate::api::WalletApi;
use crate::app::Nav;
use crate::components::list::{LabelItem, List};
use crate::components::Component;
use crate::screens::assets::AssetsScreen;
use crate::screens::help::HelpScreen;
use crate::screens::receive::ReceiveScreen;
use crate::screens::send::SendScreen;
use crate::screens::Screen;
use crate::ui;
use async_trait::async_trait;
use ratatui::layout::{Constraint, Layout, Margin, Rect};
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::widgets::{Block, Paragraph};
use ratatui::Frame;

pub struct HomeScreen {
    list: List<()>,
    state: ApiState<HomeData>,
}

impl HomeScreen {
    pub fn new() -> Self {
        Self {
            list: List::new(vec![]),
            state: ApiState::Loading,
        }
    }

    fn rebuild_list(&mut self, data: &HomeData) {
        let items: Vec<Box<dyn Component<()>>> = data
            .accounts
            .iter()
            .map(|a| {
                let addr = if a.address.len() > 12 {
                    format!("{}...{}", &a.address[..6], &a.address[a.address.len() - 5..])
                } else {
                    a.address.clone()
                };
                let label = format!("{} — {}", a.name, addr);
                Box::new(LabelItem::new(label)) as Box<dyn Component<()>>
            })
            .collect();
        self.list = List::new(items);
        self.list.set_focused(true);
    }
}

#[async_trait(?Send)]
impl Screen for HomeScreen {
    fn name(&self) -> &str {
        "Home"
    }

    async fn on_reactivate(&mut self, api: &mut dyn WalletApi) {
        api.refresh_home().await;
        self.state = api.home_state().await;
    }

    async fn init(&mut self, api: &dyn WalletApi) {
        self.state = api.home_state().await;
        if let ApiState::Loaded(ref data) = self.state {
            let data = data.clone();
            self.rebuild_list(&data);
        }
    }

    fn render(&mut self, frame: &mut Frame, _api: &dyn WalletApi) {
        if let ApiState::Loaded(ref data) = self.state {
            if self.list.selected().is_none() {
                let data = data.clone();
                self.rebuild_list(&data);
            }
        }

        let area = frame.area();
        let chunks = Layout::vertical([
            Constraint::Length(3),
            Constraint::Min(5),
            Constraint::Length(3),
        ])
        .split(area);
        let header = chunks[0];
        let body = chunks[1];
        let footer = chunks[2];

        let theme = ui::theme();
        let title = theme.title(" PayPunk Wallet ").centered();
        frame.render_widget(Paragraph::new(title).style(Style::new().bg(ui::BG)), header);

        self.render_body(frame, body);

        let footer_text = theme.help_line([
            ("↑↓", "Select"),
            ("Enter", "Assets"),
            ("s", "Send"),
            ("o", "Receive"),
            ("a", "Add Account"),
            ("r", "Refresh"),
            ("q", "Quit"),
        ]);
        let fb = Block::new().style(Style::new().bg(ui::SURFACE));
        frame.render_widget(fb, footer);
        frame.render_widget(
            Paragraph::new(footer_text).style(Style::new().bg(ui::SURFACE)),
            footer.inner(Margin {
                vertical: 0,
                horizontal: 1,
            }),
        );
    }

    async fn handle_input(
        &mut self,
        key: crossterm::event::KeyEvent,
        api: &mut dyn WalletApi,
    ) -> Nav {
        use crossterm::event::KeyCode;

        match key.code {
            KeyCode::Up | KeyCode::Down => {
                let _ = self.list.handle_event(key);
            }
            KeyCode::Enter => {
                if let Some(idx) = self.list.selected() {
                    if let ApiState::Loaded(ref data) = self.state {
                        if let Some(acc) = data.accounts.get(idx) {
                            return Nav::Push(Box::new(AssetsScreen::new(
                                acc.clone(),
                            )));
                        }
                    }
                }
            }
            KeyCode::Char('s') => {
                if let Some(idx) = self.list.selected() {
                    if let ApiState::Loaded(ref data) = self.state {
                        if let Some(acc) = data.accounts.get(idx) {
                            return Nav::Push(Box::new(SendScreen::new(&acc.chain_id)));
                        }
                    }
                }
            }
            KeyCode::Char('o') => {
                if let Some(idx) = self.list.selected() {
                    if let ApiState::Loaded(ref data) = self.state {
                        if let Some(acc) = data.accounts.get(idx) {
                            return Nav::Push(Box::new(ReceiveScreen::new(&acc.chain_id)));
                        }
                    }
                }
            }
            KeyCode::Char('a') => {
                let _ = api.add_account().await;
                api.refresh_home().await;
                self.state = api.home_state().await;
                if let ApiState::Loaded(ref data) = self.state {
                    let data = data.clone();
                    self.rebuild_list(&data);
                }
            }
            KeyCode::Char('r') => {
                api.refresh_home().await;
                self.state = api.home_state().await;
                if let ApiState::Loaded(ref data) = self.state {
                    let data = data.clone();
                    self.rebuild_list(&data);
                }
            }
            KeyCode::Char('q') => return Nav::Quit,
            KeyCode::Char('?') => return Nav::Push(Box::new(HelpScreen::new(self.name()))),
            _ => {}
        }
        Nav::None
    }
}

impl HomeScreen {
    fn render_body(&mut self, frame: &mut Frame, area: Rect) {
        let theme = ui::theme();
        match &self.state {
            ApiState::Loading => {
                let block = theme.titled_block("Accounts");
                let inner = block.inner(area);
                frame.render_widget(block, area);
                let msg = Paragraph::new(Line::from(vec![theme.muted(" Loading...")]))
                    .centered()
                    .style(Style::new().bg(ui::BG));
                frame.render_widget(
                    msg,
                    inner.inner(Margin {
                        vertical: 3,
                        horizontal: 2,
                    }),
                );
            }
            ApiState::Error(err) => {
                let block = theme.titled_block("Accounts");
                let inner = block.inner(area);
                frame.render_widget(block, area);
                ui::render_error_banner(frame, area, err);
                let msg = Paragraph::new(Line::from(vec![
                    theme.error(" Could not load accounts. "),
                    theme.muted("Press "),
                    theme.accent("r"),
                    theme.muted(" to retry."),
                ]))
                .centered()
                .style(Style::new().bg(ui::BG));
                frame.render_widget(
                    msg,
                    inner.inner(Margin {
                        vertical: 4,
                        horizontal: 2,
                    }),
                );
            }
            ApiState::Loaded(data) => {
                if data.accounts.is_empty() {
                    let block = theme.titled_block("Accounts");
                    let inner = block.inner(area);
                    frame.render_widget(block, area);
                    let msg = Paragraph::new(Line::from(vec![
                        theme.muted("No accounts yet. "),
                        theme.accent("Press `a`"),
                        theme.muted(" to add one."),
                    ]))
                    .centered()
                    .style(Style::new().bg(ui::BG));
                    frame.render_widget(
                        msg,
                        inner.inner(Margin {
                            vertical: 3,
                            horizontal: 2,
                        }),
                    );
                    return;
                }

                let block = theme.titled_block("Accounts");
                let inner = block.inner(area);
                frame.render_widget(block, area);

                self.list.render(
                    frame,
                    inner.inner(Margin {
                        horizontal: 1,
                        vertical: 1,
                    }),
                );
            }
        }
    }
}
