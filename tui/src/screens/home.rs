use crate::api::types::*;
use crate::api::WalletApi;
use crate::app::Nav;
use crate::components::balance_item::{BalanceAction, BalanceItem};
use crate::components::list::List;
use crate::components::Component;
use crate::screens::help::HelpScreen;
use crate::screens::lock::LockScreen;
use crate::screens::receive::ReceiveScreen;
use crate::screens::send::SendScreen;
use crate::screens::settings::SettingsScreen;
use crate::screens::Screen;
use crate::ui;
use async_trait::async_trait;
use ratatui::layout::{Constraint, Layout, Margin, Rect};
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::widgets::{Block, Paragraph};
use ratatui::Frame;
use ratatui_bubbletea_components::SelectList;

pub struct HomeScreen {
    list: List<BalanceAction>,
    menu_open: bool,
    menu: SelectList,
    state: ApiState<HomeData>,
}

impl HomeScreen {
    pub fn new() -> Self {
        Self {
            list: List::new(vec![]),
            menu_open: false,
            menu: SelectList::new([" Send ", " Receive "]),
            state: ApiState::Loading,
        }
    }

    fn rebuild_list(&mut self, data: &HomeData) {
        let items: Vec<Box<dyn Component<BalanceAction>>> = data
            .balances
            .iter()
            .map(|b| Box::new(BalanceItem::new(b.clone())) as Box<dyn Component<BalanceAction>>)
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
            Constraint::Length(5),
            Constraint::Min(5),
            Constraint::Length(3),
        ])
        .split(area);
        let header = chunks[0];
        let body = chunks[1];
        let footer = chunks[2];

        self.render_header(frame, header);
        self.render_body(frame, body);
        self.render_footer(frame, footer);
    }

    async fn handle_input(
        &mut self,
        key: crossterm::event::KeyEvent,
        api: &mut dyn WalletApi,
    ) -> Nav {
        use crossterm::event::KeyCode;

        if self.menu_open {
            match key.code {
                KeyCode::Left => {
                    self.menu.first();
                }
                KeyCode::Right => {
                    self.menu.last();
                }
                KeyCode::Enter => {
                    self.menu_open = false;
                    let sel = self.menu.selected().unwrap_or(0);
                    if let Some(idx) = self.list.selected() {
                        if let ApiState::Loaded(ref data) = self.state {
                            if let Some(bal) = data.balances.get(idx) {
                                return match sel {
                                    0 => Nav::Push(Box::new(SendScreen::new(&bal.chain_id))),
                                    _ => Nav::Push(Box::new(ReceiveScreen::new(&bal.chain_id))),
                                };
                            }
                        }
                    }
                }
                KeyCode::Esc => {
                    self.menu_open = false;
                }
                _ => {}
            }
            return Nav::None;
        }

        match key.code {
            KeyCode::Up | KeyCode::Down => {
                let _ = self.list.handle_event(key);
            }
            KeyCode::Enter => {
                if let ApiState::Loaded(ref data) = self.state {
                    if !data.balances.is_empty() {
                        self.menu_open = true;
                        self.menu.first();
                    }
                }
            }
            KeyCode::Char('s') => {
                if let Some(idx) = self.list.selected() {
                    if let ApiState::Loaded(ref data) = self.state {
                        if let Some(bal) = data.balances.get(idx) {
                            return Nav::Push(Box::new(SendScreen::new(&bal.chain_id)));
                        }
                    }
                }
            }
            KeyCode::Char('o') => {
                if let Some(idx) = self.list.selected() {
                    if let ApiState::Loaded(ref data) = self.state {
                        if let Some(bal) = data.balances.get(idx) {
                            return Nav::Push(Box::new(ReceiveScreen::new(&bal.chain_id)));
                        }
                    }
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
            KeyCode::Char('t') => return Nav::Push(Box::new(SettingsScreen::new())),
            KeyCode::Char('l') => return Nav::Push(Box::new(LockScreen::new())),
            KeyCode::Char('?') => return Nav::Push(Box::new(HelpScreen::new(self.name()))),
            _ => {}
        }
        Nav::None
    }
}

impl HomeScreen {
    fn render_header(&self, frame: &mut Frame, area: Rect) {
        let theme = ui::theme();
        let block = Block::new().style(Style::new().bg(ui::BG));
        frame.render_widget(block, area);

        let title = theme.title(" PayPunk Wallet ").centered();
        frame.render_widget(Paragraph::new(title).style(Style::new().bg(ui::BG)), area);

        if let ApiState::Loaded(ref data) = self.state {
            let total = format!("${:.2} {}", data.total_fiat_value, data.fiat_currency);
            let total_line = theme.accent(&total).into_centered_line();
            frame.render_widget(
                Paragraph::new(total_line).style(Style::new().bg(ui::BG)),
                area.inner(Margin {
                    vertical: 2,
                    horizontal: 0,
                }),
            );
        }
    }

    fn render_body(&mut self, frame: &mut Frame, area: Rect) {
        let theme = ui::theme();
        match &self.state {
            ApiState::Loading => {
                let block = theme.titled_block("Assets");
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
                let block = theme.titled_block("Assets");
                let inner = block.inner(area);
                frame.render_widget(block, area);
                ui::render_error_banner(frame, area, err);
                let msg = Paragraph::new(Line::from(vec![
                    theme.error(" Could not load assets. "),
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
                if data.balances.is_empty() {
                    let block = theme.titled_block("Assets");
                    let inner = block.inner(area);
                    frame.render_widget(block, area);
                    let msg = Paragraph::new(Line::from(vec![
                        theme.muted("No assets yet. "),
                        theme.accent("Press `o`"),
                        theme.muted(" to receive funds."),
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

                let block = theme.titled_block("Assets");
                let inner = block.inner(area);
                frame.render_widget(block, area);

                if self.menu_open {
                    let body_chunks =
                        Layout::vertical([Constraint::Min(3), Constraint::Length(1)]).split(inner);
                    let list_area = body_chunks[0];
                    let menu_area = body_chunks[1];

                    self.list.render(
                        frame,
                        list_area.inner(Margin {
                            horizontal: 1,
                            vertical: 1,
                        }),
                    );

                    let menu_items = [" Send ", " Receive "];
                    let sel = self.menu.selected().unwrap_or(0);
                    let spans: Vec<_> = menu_items
                        .iter()
                        .enumerate()
                        .map(|(i, label)| {
                            if i == sel {
                                theme.accent(format!("▸{}◂", label))
                            } else {
                                theme.muted(label.to_string())
                            }
                        })
                        .collect();
                    let menu_para =
                        Paragraph::new(Line::from(spans)).style(Style::new().bg(ui::SURFACE));
                    frame.render_widget(
                        menu_para,
                        menu_area.inner(Margin {
                            horizontal: 2,
                            vertical: 0,
                        }),
                    );
                } else {
                    self.list.render(
                        frame,
                        area.inner(Margin {
                            horizontal: 2,
                            vertical: 1,
                        }),
                    );
                }
            }
        }
    }

    fn render_footer(&self, frame: &mut Frame, area: Rect) {
        let theme = ui::theme();
        let line = theme.help_line([
            ("↑↓", "Select"),
            ("s", "Send"),
            ("o", "Receive"),
            ("r", "Refresh"),
            ("t", "Settings"),
            ("l", "Lock"),
            ("q", "Quit"),
            ("?", "Help"),
        ]);
        let block = Block::new().style(Style::new().bg(ui::SURFACE));
        frame.render_widget(block, area);
        frame.render_widget(
            Paragraph::new(line).style(Style::new().bg(ui::SURFACE)),
            area.inner(Margin {
                vertical: 0,
                horizontal: 1,
            }),
        );
    }
}
