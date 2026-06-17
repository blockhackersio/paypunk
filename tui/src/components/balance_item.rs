use crate::api::types::BalanceInfo;
use crate::components::Component;
use crate::ui;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

pub enum BalanceAction {
    Select,
}

pub struct BalanceItem {
    balance: BalanceInfo,
    focused: bool,
}

impl BalanceItem {
    pub fn new(balance: BalanceInfo) -> Self {
        Self { balance, focused: false }
    }
}

impl Component<BalanceAction> for BalanceItem {
    fn render(&mut self, frame: &mut Frame, area: Rect) {
        let raw = self.balance.raw_balance.parse::<f64>().unwrap_or(0.0);
        let divisor = 10u128.pow(self.balance.decimals as u32) as f64;
        let human = raw / divisor;
        let formatted = if self.balance.decimals >= 8 {
            format!("{:.8}", human)
        } else {
            format!("{:.4}", human)
        };
        let chain_name = if self.balance.chain_id.contains("eip155") { "Ethereum" } else { "Zcash" };
        let desc = format!("{}  ${:.2} ({})", formatted, self.balance.fiat_value, chain_name);

        let name_style = if self.focused {
            Style::new().fg(ui::palette().foreground).bold()
        } else {
            Style::new().fg(ui::palette().foreground)
        };
        let desc_style = Style::new().fg(ui::palette().muted);

        let content = Paragraph::new(Line::from(vec![
            Span::styled(format!(" {}", self.balance.symbol), name_style),
            Span::raw("  "),
            Span::styled(desc, desc_style),
        ]));
        frame.render_widget(content, area);
    }

    fn handle_event(&mut self, key: KeyEvent) -> Option<BalanceAction> {
        if !self.focused {
            return None;
        }
        match key.code {
            KeyCode::Enter => Some(BalanceAction::Select),
            _ => None,
        }
    }

    fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
    }

    fn is_focused(&self) -> bool {
        self.focused
    }
}
