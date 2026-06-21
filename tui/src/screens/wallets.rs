use crate::api::types::*;
use crate::api::WalletApi;
use crate::app::Nav;
use crate::components::list::{List, ListAction};
use crate::components::wallet_item::{WalletAction, WalletItem};
use crate::components::Component;
use crate::screens::assets::AssetsScreen;
use crate::screens::help::HelpScreen;
use crate::screens::Screen;
use crate::ui;
use async_trait::async_trait;
use ratatui::layout::{Constraint, Layout, Margin};
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::widgets::{Block, Paragraph};
use ratatui::Frame;

pub struct WalletsScreen {
    data: Option<Vec<AccountInfo>>,
    list: List<WalletAction>,
}

impl WalletsScreen {
    pub fn new() -> Self {
        Self {
            data: None,
            list: List::new(vec![]).row_height(3),
        }
    }
}

#[async_trait(?Send)]
impl Screen for WalletsScreen {
    fn name(&self) -> &str {
        "Wallets"
    }

    async fn init(&mut self, api: &dyn WalletApi) {
        let accounts = api.list_accounts().await.unwrap_or_default();
        let wallets: Vec<WalletDerivation> = accounts
            .iter()
            .enumerate()
            .map(|(i, a)| WalletDerivation {
                index: i,
                address: a.address.clone(),
                chain_id: a.chain_id.clone(),
                chain_name: a.name.clone(),
            })
            .collect();
        let items: Vec<Box<dyn Component<WalletAction>>> = wallets
            .iter()
            .map(|w| Box::new(WalletItem::new(w.clone())) as Box<dyn Component<WalletAction>>)
            .collect();
        self.list = List::new(items).row_height(3);
        self.list.set_focused(true);
        self.data = Some(accounts);
    }

    fn render(&mut self, frame: &mut Frame, _api: &dyn WalletApi) {
        let theme = ui::theme();
        let area = frame.area();
        let chunks = Layout::vertical([
            Constraint::Length(4),
            Constraint::Min(5),
            Constraint::Length(3),
        ])
        .split(area);
        let header = chunks[0];
        let body = chunks[1];
        let footer = chunks[2];

        let title_block = Block::new()
            .style(Style::new().bg(ui::BG))
            .title(Line::from(" PayPunk Wallet ").centered())
            .title_style(Style::new().fg(ui::palette().primary));
        frame.render_widget(title_block, header);

        let subtitle = Paragraph::new(Line::from("Wallets").centered()).style(theme.text);
        frame.render_widget(
            subtitle,
            header.inner(Margin {
                vertical: 2,
                horizontal: 0,
            }),
        );

        let block = theme.titled_block("Your Wallets");
        let inner = block.inner(body);
        frame.render_widget(block, body);

        let list_area = inner.inner(Margin {
            vertical: 1,
            horizontal: 2,
        });
        self.list.render(frame, list_area);

        let footer_text = theme.help_line([
            ("↑↓", "Select wallet"),
            ("Enter", "View assets"),
            ("q", "Quit"),
            ("?", "Help"),
        ]);
        let footer_block = Block::new().style(Style::new().bg(ui::SURFACE));
        frame.render_widget(footer_block, footer);
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
        _api: &mut dyn WalletApi,
    ) -> Nav {
        use crossterm::event::KeyCode;
        match key.code {
            KeyCode::Char('?') => return Nav::Push(Box::new(HelpScreen::new(self.name()))),
            KeyCode::Char('q') => return Nav::Quit,
            KeyCode::Esc => return Nav::Pop,
            _ => {}
        }

        if let Some(action) = self.list.handle_event(key) {
            match action {
                ListAction::Selected(idx) => {
                    if let Some(ref accounts) = self.data {
                        if let Some(account) = accounts.get(idx) {
                            return Nav::Push(Box::new(AssetsScreen::new(
                                &account.chain_id,
                                &account.name,
                            )));
                        }
                    }
                }
                _ => {}
            }
        }
        Nav::None
    }
}
