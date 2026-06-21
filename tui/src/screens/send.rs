use crate::api::types::*;
use crate::api::WalletApi;
use crate::app::Nav;
use crate::components::text_field::{TextField, TextFieldConfig};
use crate::components::Component;
use crate::screens::help::HelpScreen;
use crate::screens::Screen;
use crate::ui;
use async_trait::async_trait;
use ratatui::layout::{Constraint, Layout, Margin, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Text};
use ratatui::widgets::{Block, Paragraph};
use ratatui::Frame;
use ratatui_bubbletea_components::{ListItem, Progress, SelectList};

enum SendStep {
    Form,
    Review,
    ConfirmSend,
    Sending,
    Confirm,
}

pub struct SendScreen {
    chain_id: String,
    step: SendStep,
    to_field: TextField,
    amount_field: TextField,
    fee_tiers: SelectList,
    review_data: Option<SendReviewData>,
    result: Option<SendResult>,
    focus: usize,
    confirm_choice: SelectList,
    copied_feedback: Option<String>,
    send_data: ApiState<SendData>,
}

impl SendScreen {
    pub fn new(chain_id: &str) -> Self {
        let fee_tiers = SelectList::new([
            ListItem::new("slow"),
            ListItem::new("medium"),
            ListItem::new("fast"),
        ])
        .theme(ui::theme());
        let confirm_choice =
            SelectList::new([ListItem::new("Yes, send it"), ListItem::new("No, go back")])
                .theme(ui::theme());
        Self {
            chain_id: chain_id.to_string(),
            step: SendStep::Form,
            to_field: TextField::new(TextFieldConfig {
                label: "To".into(),
                placeholder: "Enter recipient address...".into(),
                password_mode: false,
                initial_value: String::new(),
                feedback: None,
            }),
            amount_field: TextField::new(TextFieldConfig {
                label: "Amount".into(),
                placeholder: "Enter amount...".into(),
                password_mode: false,
                initial_value: String::new(),
                feedback: None,
            }),
            fee_tiers,
            review_data: None,
            result: None,
            focus: 0,
            confirm_choice,
            copied_feedback: None,
            send_data: ApiState::Loading,
        }
    }
}

#[async_trait(?Send)]
impl Screen for SendScreen {
    fn name(&self) -> &str {
        "Send"
    }

    async fn on_reactivate(&mut self, api: &mut dyn WalletApi) {
        api.refresh_send(&self.chain_id).await;
        self.send_data = api.send_state(&self.chain_id).await;
    }

    async fn init(&mut self, api: &dyn WalletApi) {
        self.send_data = api.send_state(&self.chain_id).await;
    }

    fn render(&mut self, frame: &mut Frame, _api: &dyn WalletApi) {
        let theme = ui::theme();
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

        let step_name = match self.step {
            SendStep::Form => "Send — Enter Details",
            SendStep::Review => "Send — Review",
            SendStep::ConfirmSend => "Send — Confirm",
            SendStep::Sending => "Send — Broadcasting",
            SendStep::Confirm => "Send — Confirmed",
        };

        let title = theme.title(format!(" {} ", step_name)).centered();
        frame.render_widget(Paragraph::new(title).style(Style::new().bg(ui::BG)), header);

        match self.step {
            SendStep::Form => self.render_form(frame, body),
            SendStep::Review => self.render_review(frame, body),
            SendStep::ConfirmSend => self.render_confirm_send(frame, body),
            SendStep::Sending => self.render_sending(frame, body),
            SendStep::Confirm => self.render_confirm(frame, body),
        }

        let footer_text = match self.step {
            SendStep::Form => theme.help_line([
                ("Tab/↓", "Focus"),
                ("←/→", "Fee"),
                ("Enter", "Review"),
                ("Esc", "Back"),
                ("?", "Help"),
            ]),
            SendStep::Review => {
                theme.help_line([("Enter", "Confirm"), ("Esc", "Edit"), ("?", "Help")])
            }
            SendStep::ConfirmSend => {
                theme.help_line([("←/→", "Choose"), ("Enter", "Send"), ("Esc", "Cancel")])
            }
            SendStep::Sending => theme.help_line([("", "Sending...")]),
            SendStep::Confirm => {
                theme.help_line([("c", "Copy TX Hash"), ("Enter", "Done"), ("?", "Help")])
            }
        };
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
            KeyCode::Char('?') => return Nav::Push(Box::new(HelpScreen::new(self.name()))),
            _ => {}
        }
        match self.step {
            SendStep::Form => {
                let is_ethereum = self.chain_id.contains("eip155");
                let max_focus = if is_ethereum { 1 } else { 3 };
                match key.code {
                    KeyCode::Tab | KeyCode::Down => {
                        self.focus = (self.focus + 1).min(max_focus);
                    }
                    KeyCode::Up => {
                        self.focus = self.focus.saturating_sub(1);
                    }
                    KeyCode::Left => {
                        if self.focus == 3 {
                            self.fee_tiers.previous();
                        }
                    }
                    KeyCode::Right => {
                        if self.focus == 3 {
                            self.fee_tiers.next();
                        }
                    }
                    _ => {
                        if key.code == KeyCode::Enter {
                            let review = api
                                .submit_send_review(SendReviewInput {
                                    to_address: if self.to_field.value().is_empty() {
                                        "0x1f9840a85d5aF5bf1D1762F925BDADdC4201F984".into()
                                    } else {
                                        self.to_field.value().into()
                                    },
                                    amount: if self.amount_field.value().is_empty() {
                                        "250000000000000000".into()
                                    } else {
                                        self.amount_field.value().into()
                                    },
                                    token_id: "eth-native".into(),
                                    chain_id: self.chain_id.clone(),
                                    fee_selection: FeeSelection {
                                        tier: self
                                            .fee_tiers
                                            .selected_item()
                                            .map(|i| i.label().to_string())
                                            .unwrap_or_else(|| "medium".into()),
                                    },
                                })
                                .await;
                            self.review_data = Some(review);
                            self.step = SendStep::Review;
                        } else if key.code == KeyCode::Esc {
                            return Nav::Pop;
                        } else {
                            match self.focus {
                                0 => {
                                    if is_ethereum
                                        && matches!(key.code, KeyCode::Char(c) if !c.is_ascii_hexdigit() && c != 'x')
                                    {
                                    } else {
                                        self.to_field.handle_event(key);
                                    }
                                }
                                1 => {
                                    if is_ethereum
                                        && matches!(key.code, KeyCode::Char(c) if !c.is_ascii_digit() && c != '.')
                                    {
                                    } else {
                                        self.amount_field.handle_event(key);
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }
            SendStep::Review => match key.code {
                KeyCode::Enter => {
                    self.step = SendStep::ConfirmSend;
                    self.confirm_choice.first();
                }
                KeyCode::Esc => {
                    self.step = SendStep::Form;
                }
                _ => {}
            },
            SendStep::ConfirmSend => match key.code {
                KeyCode::Left | KeyCode::Up => {
                    self.confirm_choice.previous();
                }
                KeyCode::Right | KeyCode::Down => {
                    self.confirm_choice.next();
                }
                KeyCode::Enter => {
                    if self.confirm_choice.selected() == Some(0) {
                        self.step = SendStep::Sending;
                        if let Some(ref review) = self.review_data {
                            let result = api
                                .submit_send_confirm(SendConfirmInput {
                                    reviewed: ReviewedDetails {
                                        to_address: review.to_address.clone(),
                                        amount: review.amount.clone(),
                                        fee_estimate: review.fee_estimate.clone(),
                                        total_amount: review.total_amount.clone(),
                                    },
                                    auth_confirmation: AuthConfirmation {
                                        auth_type: "biometric".into(),
                                        value: "face-id-assertion-token".into(),
                                    },
                                    signed_tx: "0x02f8b00182002a8459682f00851b572f4e...".into(),
                                })
                                .await;
                            self.result = Some(result);
                            self.step = SendStep::Confirm;
                        }
                    } else {
                        self.step = SendStep::Review;
                    }
                }
                KeyCode::Esc => {
                    self.step = SendStep::Review;
                }
                _ => {}
            },
            SendStep::Sending => {}
            SendStep::Confirm => match key.code {
                KeyCode::Enter | KeyCode::Esc => return Nav::Pop,
                KeyCode::Char('c') => {
                    if let Some(ref result) = self.result {
                        let mut cb = arboard::Clipboard::new().ok();
                        if let Some(ref mut clipboard) = cb {
                            let _ = clipboard.set_text(result.tx_hash.clone());
                        }
                        self.copied_feedback = Some("Copied!".into());
                    }
                }
                _ => {}
            },
        }
        Nav::None
    }

    async fn handle_paste(&mut self, text: &str, _api: &mut dyn WalletApi) -> Nav {
        match self.step {
            SendStep::Form => match self.focus {
                0 => self.to_field.handle_paste(text),
                1 => self.amount_field.handle_paste(text),
                _ => {}
            },
            _ => {}
        }
        Nav::None
    }
}

impl SendScreen {
    fn render_form(&mut self, frame: &mut Frame, area: Rect) {
        let theme = ui::theme();
        let block = theme.titled_block("Transaction Details");
        let inner = block.inner(area);
        frame.render_widget(block, area);

        match &self.send_data {
            ApiState::Loading => {
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
                ui::render_error_banner(frame, area, err);
                let msg = Paragraph::new(Line::from(vec![
                    theme.error(" Could not load send data. "),
                    theme.muted("Press "),
                    theme.accent("Esc"),
                    theme.muted(" to go back."),
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
            ApiState::Loaded(ref data) => {
                let divisor = 10u128.pow(data.decimals as u32) as f64;
                let bal = data.spendable_balance.parse::<f64>().unwrap_or(0.0) / divisor;
                let bal_str = format!("{:.8}", bal);
                let symbol = if data.chain_id.contains("eip155") {
                    "ETH"
                } else {
                    "ZEC"
                };
                let is_ethereum = data.chain_id.contains("eip155");

                self.to_field.set_focused(self.focus == 0);
                self.to_field.render(
                    frame,
                    inner.inner(Margin {
                        vertical: 3,
                        horizontal: 2,
                    }),
                );

                let amt_placeholder = format!("Enter amount ({})...", symbol);
                self.amount_field.set_placeholder(&amt_placeholder);
                self.amount_field.set_focused(self.focus == 1);
                self.amount_field.render(
                    frame,
                    inner.inner(Margin {
                        vertical: 6,
                        horizontal: 2,
                    }),
                );

                let mut y_offset = 9;

                let balance_line = Line::from(vec![
                    theme.muted("Balance: "),
                    theme.span(format!("{} {}", bal_str, symbol)),
                ]);
                let bal_para = Paragraph::new(balance_line).style(Style::new().bg(ui::BG));
                frame.render_widget(
                    bal_para,
                    inner.inner(Margin {
                        vertical: y_offset,
                        horizontal: 2,
                    }),
                );

                if !is_ethereum {
                    y_offset += 2;

                    let fee_str = match &data.fee_data {
                        FeeData::Eth(f) => {
                            format!(
                                "Base: {} Gwei | Priority: {} Gwei | Gas: {}",
                                &f.base_fee_per_gas[..std::cmp::min(f.base_fee_per_gas.len(), 6)],
                                &f.max_priority_fee_per_gas
                                    [..std::cmp::min(f.max_priority_fee_per_gas.len(), 6)],
                                f.gas_limit_estimate
                            )
                        }
                        FeeData::Zec(r) => {
                            format!(
                                "Slow: {}  Medium: {}  Fast: {} zat/byte",
                                r.slow, r.medium, r.fast
                            )
                        }
                    };

                    let fee_line = Line::from(vec![theme.muted("Fee:     "), theme.span(fee_str)]);
                    let fee_para = Paragraph::new(fee_line).style(Style::new().bg(ui::BG));
                    frame.render_widget(
                        fee_para,
                        inner.inner(Margin {
                            vertical: y_offset,
                            horizontal: 2,
                        }),
                    );

                    y_offset += 1;
                    let tier_line = Line::from(vec![theme.muted("Tier:    ")]);
                    let tier_para = Paragraph::new(tier_line).style(Style::new().bg(ui::BG));
                    frame.render_widget(
                        tier_para,
                        inner.inner(Margin {
                            vertical: y_offset,
                            horizontal: 2,
                        }),
                    );

                    frame.render_widget(
                        &self.fee_tiers,
                        inner.inner(Margin {
                            vertical: y_offset + 1,
                            horizontal: 4,
                        }),
                    );

                    if let Some(nonce) = data.nonce {
                        let nonce_line = Line::from(vec![
                            theme.muted("Nonce:   "),
                            theme.span(nonce.to_string()),
                        ]);
                        let nonce_para = Paragraph::new(nonce_line).style(Style::new().bg(ui::BG));
                        frame.render_widget(
                            nonce_para,
                            inner.inner(Margin {
                                vertical: y_offset + 2,
                                horizontal: 2,
                            }),
                        );
                    }
                }
            }
        }
    }

    fn render_review(&self, frame: &mut Frame, area: Rect) {
        let theme = ui::theme();
        let block = theme.titled_block("Review Transaction");
        let inner = block.inner(area);
        frame.render_widget(block, area);

        if let Some(ref review) = self.review_data {
            let is_ethereum = self.chain_id.contains("eip155");

            let decimals = if let ApiState::Loaded(ref data) = &self.send_data {
                data.decimals
            } else {
                18
            };
            let from_address = if let ApiState::Loaded(ref data) = &self.send_data {
                data.from_address.clone()
            } else {
                String::new()
            };

            let amount_display = if is_ethereum {
                format_eth_amount(&review.amount, decimals)
            } else {
                review.amount.clone()
            };

            let fee_display = if is_ethereum {
                format_eth_amount(&review.fee_estimate, decimals)
            } else {
                review.fee_estimate.clone()
            };

            let total_display = if is_ethereum {
                format_eth_amount(&review.total_amount, decimals)
            } else {
                review.total_amount.clone()
            };

            let chain_display = if is_ethereum {
                "Ethereum Mainnet".to_string()
            } else {
                review.chain_id.clone()
            };

            let lines = vec![
                Line::from(vec![theme.muted("From:      "), theme.span(&from_address)]),
                Line::from(""),
                Line::from(vec![
                    theme.muted("To:        "),
                    theme.span(&review.to_address),
                ]),
                Line::from(""),
                Line::from(vec![
                    theme.muted("Amount:    "),
                    theme.span(&amount_display),
                ]),
                Line::from(vec![
                    theme.muted("Fee:       "),
                    theme.warning(&fee_display),
                ]),
                Line::from(vec![
                    theme.muted("Total:     "),
                    theme.accent(&total_display),
                ]),
                Line::from(""),
                Line::from(vec![theme.muted("Chain:     "), theme.span(&chain_display)]),
                Line::from(""),
                Line::from(vec![theme.success("Press ENTER to confirm")]),
            ];
            let para = Paragraph::new(Text::from(lines)).style(Style::new().bg(ui::BG));
            frame.render_widget(
                para,
                inner.inner(Margin {
                    vertical: 2,
                    horizontal: 4,
                }),
            );
        }
    }

    fn render_confirm_send(&self, frame: &mut Frame, area: Rect) {
        let theme = ui::theme();
        let block = theme.titled_block("Confirm Transaction");
        let inner = block.inner(area);
        frame.render_widget(block, area);

        if let Some(ref review) = self.review_data {
            let lines = vec![
                Line::from(vec![
                    theme.muted("Send "),
                    theme.span(&review.amount),
                    theme.muted(" to"),
                ]),
                Line::from(vec![theme.accent(&review.to_address)]),
                Line::from(""),
                Line::from(vec![
                    theme.muted("Fee: "),
                    theme.warning(&review.fee_estimate),
                ]),
                Line::from(vec![
                    theme.muted("Total: "),
                    theme.span(&review.total_amount),
                ]),
                Line::from(""),
                Line::from(""),
            ];
            let para = Paragraph::new(Text::from(lines)).style(Style::new().bg(ui::BG));
            frame.render_widget(
                para,
                inner.inner(Margin {
                    vertical: 2,
                    horizontal: 4,
                }),
            );

            frame.render_widget(
                &self.confirm_choice,
                inner.inner(Margin {
                    vertical: 8,
                    horizontal: 4,
                }),
            );
        }
    }

    fn render_sending(&self, frame: &mut Frame, area: Rect) {
        let theme = ui::theme();
        let block = theme.titled_block("Broadcasting");
        let inner = block.inner(area);
        frame.render_widget(block, area);

        let progress = Progress::from_percent(65)
            .width(20)
            .label("Broadcasting")
            .theme(theme);
        frame.render_widget(
            &progress,
            inner.inner(Margin {
                vertical: 2,
                horizontal: 4,
            }),
        );

        let lines = vec![
            Line::from(vec![theme.accent(" Sending transaction... ")]).centered(),
            Line::from(""),
            Line::from(vec![
                theme.muted("Please wait while your transaction is broadcast")
            ])
            .centered(),
        ];
        let para = Paragraph::new(Text::from(lines)).style(Style::new().bg(ui::BG));
        frame.render_widget(
            para,
            inner.inner(Margin {
                vertical: 4,
                horizontal: 2,
            }),
        );
    }

    fn render_confirm(&self, frame: &mut Frame, area: Rect) {
        let theme = ui::theme();
        let block = theme.titled_block("Transaction Sent");
        let inner = block.inner(area);
        frame.render_widget(block, area);

        if let Some(ref result) = self.result {
            let mut lines = vec![
                Line::from(vec![theme.success(" ✓ Transaction Broadcasted ")]),
                Line::from(""),
                Line::from(vec![theme.muted("TX Hash:")]),
                Line::from(vec![theme.accent(&result.tx_hash)]),
                Line::from(""),
                Line::from(vec![theme.muted("Status: "), theme.success(&result.status)]),
                Line::from(""),
                Line::from(vec![theme.muted("View on block explorer:")]),
                Line::from(vec![theme.span(&result.block_explorer_url)]),
                Line::from(""),
            ];
            if let Some(ref feedback) = self.copied_feedback {
                lines.push(Line::from(vec![theme.success(feedback)]));
                lines.push(Line::from(""));
            }
            lines.push(Line::from(vec![theme.muted("Press ENTER to return")]));
            let para = Paragraph::new(Text::from(lines)).style(Style::new().bg(ui::BG));
            frame.render_widget(
                para,
                inner.inner(Margin {
                    vertical: 2,
                    horizontal: 4,
                }),
            );
        }
    }
}

fn format_eth_amount(amount: &str, decimals: u8) -> String {
    let divisor = 10u128.pow(decimals as u32) as f64;
    let value = amount.parse::<f64>().unwrap_or(0.0) / divisor;
    format!("{:.6} ETH", value)
}
