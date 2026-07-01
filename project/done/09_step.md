# Step 9: TUI History Screen

## Goal
Create a new generic transaction history screen with a minimal table (date, type, amount, status).

## Changes

### 1. `tui/src/screens/history.rs` (NEW FILE)

```rust
use crate::api::types::*;
use crate::api::WalletApi;
use crate::app::Nav;
use crate::screens::help::HelpScreen;
use crate::screens::Screen;
use crate::ui;
use async_trait::async_trait;
use ratatui::layout::{Constraint, Layout, Margin, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};
use ratatui::Frame;

/// A single entry in the transaction history.
#[derive(Debug, Clone)]
struct HistoryRow {
    date: String,
    tx_type: String, // "Sent" or "Received"
    amount: String,
    status: String,
}

pub struct HistoryScreen {
    account_id: String,
    account_name: String,
    rows: Vec<HistoryRow>,
    selected: usize,
}

impl HistoryScreen {
    pub fn new(account_id: String, account_name: String) -> Self {
        Self {
            account_id,
            account_name,
            rows: Vec::new(),
            selected: 0,
        }
    }
}

#[async_trait(?Send)]
impl Screen for HistoryScreen {
    fn name(&self) -> &str {
        "History"
    }

    async fn init(&mut self, _api: &dyn WalletApi) {
        // TODO: Load history from API when available
        // For now, show empty state
        self.rows = Vec::new();
    }

    async fn on_reactivate(&mut self, _api: &mut dyn WalletApi) {
        // Refresh history when returning to this screen
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

        // Header
        let title_text = format!(" Transaction History — {} ", self.account_name);
        let title = theme.title(&title_text).centered();
        frame.render_widget(Paragraph::new(title).style(Style::new().bg(ui::BG)), header);

        // Body
        let block = theme.titled_block("History");
        let inner = block.inner(body);
        frame.render_widget(block, body);

        if self.rows.is_empty() {
            let empty_msg = Paragraph::new(
                Line::from(vec![theme.muted(" No transactions yet ")])
            ).centered().style(Style::new().bg(ui::BG));
            frame.render_widget(
                empty_msg,
                inner.inner(Margin { vertical: 3, horizontal: 2 }),
            );
        } else {
            // Column headers
            let header_style = Style::new().fg(ui::palette().muted);
            let header_line = Line::from(vec![
                Span::styled(" Date          ", header_style),
                Span::styled(" Type     ", header_style),
                Span::styled(" Amount           ", header_style),
                Span::styled(" Status    ", header_style),
            ]);
            frame.render_widget(
                Paragraph::new(header_line).style(Style::new().bg(ui::BG)),
                inner.inner(Margin { vertical: 1, horizontal: 2 }),
            );

            // Rows
            for (i, row) in self.rows.iter().enumerate() {
                let y = 3 + i as u16;
                if y > inner.height.saturating_sub(2) {
                    break;
                }
                let row_style = if i == self.selected {
                    ui::selected_style()
                } else {
                    Style::new().fg(ui::palette().foreground).bg(ui::BG)
                };
                let row_line = Line::from(vec![
                    Span::styled(format!(" {:<13} ", row.date), row_style),
                    Span::styled(format!(" {:<8} ", row.tx_type), row_style),
                    Span::styled(format!(" {:<16} ", row.amount), row_style),
                    Span::styled(format!(" {:<8} ", row.status), row_style),
                ]);
                frame.render_widget(
                    Paragraph::new(row_line).style(Style::new().bg(ui::BG)),
                    inner.inner(Margin {
                        vertical: y,
                        horizontal: 2,
                    }),
                );
            }
        }

        // Footer
        let footer_text = theme.help_line([
            ("↑↓", "Navigate"),
            ("Esc", "Back"),
            ("?", "Help"),
        ]);
        let fb = Block::new().style(Style::new().bg(ui::SURFACE));
        frame.render_widget(fb, footer);
        frame.render_widget(
            Paragraph::new(footer_text).style(Style::new().bg(ui::SURFACE)),
            footer.inner(Margin { vertical: 0, horizontal: 1 }),
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
            KeyCode::Up => {
                self.selected = self.selected.saturating_sub(1);
            }
            KeyCode::Down => {
                if !self.rows.is_empty() {
                    self.selected = (self.selected + 1).min(self.rows.len() - 1);
                }
            }
            KeyCode::Esc => return Nav::Pop,
            _ => {}
        }
        Nav::None
    }
}
```

### 2. `tui/src/screens/mod.rs`

Add:
```rust
pub mod history;
```

## Verification
- `cargo build -p paypunk-tui` succeeds
