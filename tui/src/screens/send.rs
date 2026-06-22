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
use ratatui_bubbletea_components::Progress;

enum SendStep {
    Form,
    Review,
    Sending,
    Confirm,
}

pub struct SendScreen {
    account_id: String,
    account_name: String,
    account_address: String,
    chain_id: String,
    step: SendStep,
    to_field: TextField,
    amount_field: TextField,
    password_field: TextField,
    review_data: Option<SendReviewData>,
    result: Option<SendResult>,
    focus: usize,
    copied_feedback: Option<String>,
    send_data: ApiState<SendData>,
}

impl SendScreen {
    pub fn new(account: AccountInfo) -> Self {
        Self {
            account_id: account.account_id,
            account_name: account.name,
            account_address: account.address,
            chain_id: account.chain_id,
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
            password_field: TextField::new(TextFieldConfig {
                label: "Password".into(),
                placeholder: "Enter password...".into(),
                password_mode: true,
                initial_value: String::new(),
                feedback: None,
            }),
            review_data: None,
            result: None,
            focus: 0,
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
            Constraint::Length(5),
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
            SendStep::Sending => "Send — Broadcasting",
            SendStep::Confirm => "Send — Confirmed",
        };

        let addr_short = if self.account_address.len() > 12 {
            format!("{}...{}", &self.account_address[..6], &self.account_address[self.account_address.len() - 5..])
        } else {
            self.account_address.clone()
        };
        let title_text = format!(" {} — {} ", step_name, self.account_name);
        let title = theme.title(&title_text).centered();
        frame.render_widget(Paragraph::new(title).style(Style::new().bg(ui::BG)), header);

        let addr_line = Paragraph::new(
            Line::from(vec![theme.muted(format!("{}", addr_short))]).centered(),
        )
        .style(Style::new().bg(ui::BG));
        frame.render_widget(
            addr_line,
            header.inner(Margin {
                vertical: 2,
                horizontal: 0,
            }),
        );

        match self.step {
            SendStep::Form => self.render_form(frame, body),
            SendStep::Review => self.render_review(frame, body),
            SendStep::Sending => self.render_sending(frame, body),
            SendStep::Confirm => self.render_confirm(frame, body),
        }

        let footer_text = match self.step {
            SendStep::Form => theme.help_line([
                ("Tab/↓", "Focus"),
                ("Enter", "Review"),
                ("Esc", "Back"),
                ("?", "Help"),
            ]),
            SendStep::Review => {
                theme.help_line([("Enter", "Send"), ("Esc", "Edit"), ("?", "Help")])
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
                let max_focus = 1;
                match key.code {
                    KeyCode::Tab | KeyCode::Down => {
                        self.focus = (self.focus + 1).min(max_focus);
                    }
                    KeyCode::BackTab | KeyCode::Up => {
                        self.focus = self.focus.saturating_sub(1);
                    }
                    _ => {
                        if key.code == KeyCode::Enter {
                            let review = api
                                .submit_send_review(SendReviewInput {
                                    to_address: self.to_field.value().into(),
                                    amount: self.amount_field.value().into(),
                                    token_id: "eth-native".into(),
                                    chain_id: self.chain_id.clone(),
                                    account_id: self.account_id.clone(),
                                })
                                .await;
                            self.review_data = Some(review);
                            self.step = SendStep::Review;
                        } else if key.code == KeyCode::Esc {
                            return Nav::Pop;
                        } else {
                            match self.focus {
                                0 => {
                                    self.to_field.handle_event(key);
                                }
                                1 => {
                                    self.amount_field.handle_event(key);
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }
            SendStep::Review => match key.code {
                KeyCode::Enter => {
                    self.step = SendStep::Sending;
                    if let Some(ref review) = self.review_data {
                        let password = self.password_field.value().to_string();
                        let result = api
                            .submit_send_confirm(SendConfirmInput {
                                reviewed: ReviewedDetails {
                                    to_address: review.to_address.clone(),
                                    amount: review.amount.clone(),
                                    fee_estimate: review.fee_estimate.clone(),
                                    total_amount: review.total_amount.clone(),
                                },
                                auth_confirmation: AuthConfirmation {
                                    auth_type: "password".into(),
                                    value: password,
                                },
                                signed_tx: String::new(),
                            })
                            .await;
                        self.result = Some(result);
                        self.step = SendStep::Confirm;
                    }
                }
                KeyCode::Esc => {
                    self.step = SendStep::Form;
                }
                _ => {
                    self.password_field.handle_event(key);
                }
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
            SendStep::Review => self.password_field.handle_paste(text),
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

                let y_offset = 9;

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
            }
        }
    }

    fn render_review(&mut self, frame: &mut Frame, area: Rect) {
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

            let nonce_display = format!("{}", review.nonce);

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
                    theme.muted("Nonce:     "),
                    theme.span(&nonce_display),
                ]),
                Line::from(vec![
                    theme.muted("Total:     "),
                    theme.accent(&total_display),
                ]),
                Line::from(""),
                Line::from(vec![theme.muted("Chain:     "), theme.span(&chain_display)]),
                Line::from(""),
                Line::from(vec![theme.muted("Enter password and press ENTER to send")]),
            ];
            let para = Paragraph::new(Text::from(lines)).style(Style::new().bg(ui::BG));
            frame.render_widget(
                para,
                inner.inner(Margin {
                    vertical: 2,
                    horizontal: 4,
                }),
            );

            self.password_field.set_focused(true);
            self.password_field.render(
                frame,
                inner.inner(Margin {
                    vertical: 14,
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
