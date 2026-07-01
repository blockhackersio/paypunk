use crate::api::types::*;
use crate::api::WalletApi;
use crate::app::Nav;
use crate::components::asset_item::{AssetAction, AssetItem};
use crate::components::button::{Button, ButtonSize};
use crate::components::flex_box::FlexBox;
use crate::components::list::List;
use crate::components::Component;
use crate::screens::help::HelpScreen;
use crate::screens::history::HistoryScreen;
use crate::screens::receive::ReceiveScreen;
use crate::screens::send::SendScreen;
use crate::screens::Screen;
use crate::ui;
use async_trait::async_trait;
use ratatui::layout::{Constraint, Layout, Margin};
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::widgets::{Block, Padding, Paragraph};
use ratatui::Frame;

enum AssetsFocus {
    Buttons(usize),
    Table,
}

struct OptimisticDeduction {
    amount_raw: String,
    address: String,
}

pub struct AssetsScreen {
    account: AccountInfo,
    data: Option<AssetsData>,
    list: List<AssetAction>,
    focus: AssetsFocus,
    protocol: String,
    sync_status: SyncStatus,
    optimistic_deduction: Option<OptimisticDeduction>,
}

impl AssetsScreen {
    pub fn new(account: AccountInfo) -> Self {
        let protocol = if account.chain_id.contains("eip155") {
            "Ethereum".to_string()
        } else {
            "Zcash".to_string()
        };
        Self {
            account,
            data: None,
            list: List::new(vec![]).row_height(2),
            focus: AssetsFocus::Buttons(0),
            protocol,
            sync_status: SyncStatus::default(),
            optimistic_deduction: None,
        }
    }
}

#[async_trait(?Send)]
impl Screen for AssetsScreen {
    fn name(&self) -> &str {
        "Assets"
    }

    async fn init(&mut self, api: &dyn WalletApi) {
        let data = api.get_assets(&self.account.account_id).await;
        let items: Vec<Box<dyn Component<AssetAction>>> = data
            .assets
            .iter()
            .map(|a| Box::new(AssetItem::new(a.clone())) as Box<dyn Component<AssetAction>>)
            .collect();
        self.list = List::new(items).row_height(2);
        self.data = Some(data);
    }

    async fn on_reactivate(&mut self, api: &mut dyn WalletApi) {
        let mut data = api.get_assets(&self.account.account_id).await;

        // Check for pending deduction from a send
        if let Some((amount_raw, address)) = api.take_pending_deduction().await {
            self.optimistic_deduction = Some(OptimisticDeduction {
                amount_raw,
                address,
            });
        } else {
            self.optimistic_deduction = None;
        }

        // Apply optimistic deduction to balance display
        if let Some(ref deduction) = self.optimistic_deduction {
            for asset in &mut data.assets {
                let parts: Vec<&str> = asset.holdings_amount.split(' ').collect();
                if parts.len() >= 2 {
                    let value: f64 = parts[0].parse().unwrap_or(0.0);
                    let ticker = parts[1];
                    let decimals = if ticker == "ZEC" { 8 } else { 18 };
                    let divisor = 10u128.pow(decimals) as f64;
                    let deduction_val = deduction.amount_raw.parse::<f64>().unwrap_or(0.0) / divisor;
                    let new_val = value - deduction_val;
                    asset.holdings_amount = format!("{:.8} {} (pending)", new_val, ticker);
                }
            }
        }

        let items: Vec<Box<dyn Component<AssetAction>>> = data
            .assets
            .iter()
            .map(|a| Box::new(AssetItem::new(a.clone())) as Box<dyn Component<AssetAction>>)
            .collect();
        self.list = List::new(items).row_height(2);
        self.data = Some(data);
    }

    async fn tick(&mut self, api: &mut dyn WalletApi) {
        self.sync_status = api.get_sync_status(&self.protocol).await;
    }

    fn render(&mut self, frame: &mut Frame, _api: &dyn WalletApi) {
        let theme = ui::theme();
        let area = frame.area();
        let chunks = Layout::vertical([
            Constraint::Length(4),
            Constraint::Length(3),
            Constraint::Min(5),
            Constraint::Length(3),
        ])
        .split(area);
        let header = chunks[0];
        let buttons = chunks[1];
        let body = chunks[2];
        let footer = chunks[3];

        let title = theme.title(" PayPunk Wallet ").centered();
        frame.render_widget(Paragraph::new(title).style(Style::new().bg(ui::BG)), header);

        let chain_label = if self.account.chain_id.contains("eip155") {
            "Ethereum"
        } else {
            "Zcash"
        };
        let subtitle = Paragraph::new(
            Line::from(format!(
                "{} — {} ({}) — {}",
                self.account.name, chain_label, self.account.chain_id, self.account.address
            ))
            .centered(),
        )
        .style(theme.text);
        frame.render_widget(
            subtitle,
            header.inner(Margin {
                vertical: 2,
                horizontal: 0,
            }),
        );

        if self.sync_status.is_syncing {
            let sync_line = Paragraph::new(
                Line::from(vec![theme.warning(format!(
                    " Syncing: {} / {} blocks ",
                    self.sync_status.current_height,
                    self.sync_status.target_height,
                ))]),
            ).style(Style::new().bg(ui::BG));
            frame.render_widget(sync_line, header.inner(Margin {
                vertical: 3,
                horizontal: 0,
            }));
        }

        let on_buttons = matches!(self.focus, AssetsFocus::Buttons(_));
        let mut send_btn = Button::new(" \u{2191} Send ").size(ButtonSize::Sm);
        send_btn.set_focused(on_buttons && matches!(self.focus, AssetsFocus::Buttons(0)));
        let mut recv_btn = Button::new(" \u{2193} Receive ").size(ButtonSize::Sm);
        recv_btn.set_focused(on_buttons && matches!(self.focus, AssetsFocus::Buttons(1)));
        let mut hist_btn = Button::new(" \u{2191} History ").size(ButtonSize::Sm);
        hist_btn.set_focused(on_buttons && matches!(self.focus, AssetsFocus::Buttons(2)));

        let mut btn_bar = FlexBox::horizontal()
            .bg(ui::BG)
            .margin(Padding {
                top: 1,
                bottom: 1,
                left: 2,
                right: 2,
            })
            .gap(2)
            .child_with(Constraint::Length(10), send_btn)
            .child_with(Constraint::Length(13), recv_btn)
            .child_with(Constraint::Length(12), hist_btn);
        btn_bar.render(frame, buttons);

        let block = theme.titled_block("");
        let inner = block.inner(body);
        frame.render_widget(block, body);

        let on_table = matches!(self.focus, AssetsFocus::Table);
        self.list.set_focused(on_table);

        let table_area = inner.inner(Margin {
            vertical: 0,
            horizontal: 1,
        });
        let header_style = Style::new().fg(ui::palette().muted);
        let name_width = (table_area.width as usize).saturating_sub(32);
        let header_line = Line::from(vec![
            ratatui::text::Span::styled(
                format!(" {:width$} ", "Asset", width = name_width),
                header_style,
            ),
            ratatui::text::Span::styled(format!(" {:>14} ", "Balance"), header_style),
            ratatui::text::Span::styled(format!(" {:>14} ", " "), header_style),
        ]);
        frame.render_widget(
            Paragraph::new(header_line).style(Style::new().bg(ui::BG)),
            table_area.inner(Margin {
                vertical: 0,
                horizontal: 0,
            }),
        );

        let sep_style = Style::new().fg(ui::palette().border);
        let sep_line = Line::from(vec![
            ratatui::text::Span::styled(
                format!(" {:-<width$} ", "", width = name_width),
                sep_style,
            ),
            ratatui::text::Span::styled(format!(" {:->14} ", ""), sep_style),
            ratatui::text::Span::styled(format!(" {:->14} ", ""), sep_style),
        ]);
        frame.render_widget(
            Paragraph::new(sep_line).style(Style::new().bg(ui::BG)),
            table_area.inner(Margin {
                vertical: 1,
                horizontal: 0,
            }),
        );

        self.list.render(
            frame,
            table_area.inner(Margin {
                vertical: 2,
                horizontal: 0,
            }),
        );

        let footer_text = theme.help_line([
            ("\u{2191}\u{2193}", "Navigate"),
            ("\u{2190}/\u{2192}", "Buttons"),
            ("Enter", "Select action"),
            ("r", "Refresh/Sync"),
            ("Esc", "Back to wallets"),
            ("?", "Help"),
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
        _api: &mut dyn WalletApi,
    ) -> Nav {
        use crossterm::event::KeyCode;
        match key.code {
            KeyCode::Char('?') => return Nav::Push(Box::new(HelpScreen::new(self.name()))),
            _ => {}
        }

        match self.focus {
            AssetsFocus::Buttons(ref mut sel) => match key.code {
                KeyCode::Left | KeyCode::Right => {
                    *sel = if *sel == 0 { 1 } else if *sel == 1 { 2 } else { 0 };
                }
                KeyCode::Down => {
                    if self.data.as_ref().map_or(false, |d| !d.assets.is_empty()) {
                        self.focus = AssetsFocus::Table;
                        self.list.set_focused(true);
                    }
                }
                KeyCode::Enter => {
                    return match *sel {
                        0 => Nav::Push(Box::new(SendScreen::new(self.account.clone()))),
                        1 => Nav::Push(Box::new(ReceiveScreen::new(self.account.clone()))),
                        2 => Nav::Push(Box::new(HistoryScreen::new(
                            self.account.account_id.clone(),
                            self.account.name.clone(),
                        ))),
                        _ => Nav::None,
                    };
                }
                KeyCode::Esc => return Nav::Pop,
                KeyCode::Char('r') => {
                    // Trigger sync — handled in tick
                }
                _ => {}
            },
            AssetsFocus::Table => match key.code {
                KeyCode::Up => {
                    if self.list.selected().map_or(true, |i| i == 0) {
                        self.focus = AssetsFocus::Buttons(0);
                        self.list.set_focused(false);
                    } else {
                        let _ = self.list.handle_event(key);
                    }
                }
                KeyCode::Down => {
                    let _ = self.list.handle_event(key);
                }
                KeyCode::Left => {
                    self.focus = AssetsFocus::Buttons(0);
                    self.list.set_focused(false);
                }
                KeyCode::Right => {
                    self.focus = AssetsFocus::Buttons(2);
                    self.list.set_focused(false);
                }
                KeyCode::Enter => {
                    if let Some(idx) = self.list.selected() {
                        if let Some(ref data) = self.data {
                            if idx < data.assets.len() {
                                return Nav::Push(Box::new(SendScreen::new(self.account.clone())));
                            }
                        }
                    }
                }
                KeyCode::Esc => return Nav::Pop,
                KeyCode::Char('r') => {
                    // Trigger sync — handled in tick
                }
                _ => {}
            },
        }
        Nav::None
    }
}
