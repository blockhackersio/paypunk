use crate::api::types::WalletDerivation;
use crate::components::Component;
use crate::ui;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

pub enum WalletAction {
    Select,
}

pub struct WalletItem {
    wallet: WalletDerivation,
    focused: bool,
}

impl WalletItem {
    pub fn new(wallet: WalletDerivation) -> Self {
        Self {
            wallet,
            focused: false,
        }
    }
}

impl Component<WalletAction> for WalletItem {
    fn render(&mut self, frame: &mut Frame, area: Rect) {
        let is_selected = self.focused;
        let idx_style = if is_selected {
            Style::new().fg(ui::palette().primary).bold()
        } else {
            Style::new().fg(ui::palette().muted)
        };
        let addr_style = if is_selected {
            Style::new().fg(ui::palette().foreground).bold()
        } else {
            Style::new().fg(ui::palette().foreground)
        };
        let chain_style = if is_selected {
            Style::new().fg(ui::palette().primary).bold()
        } else {
            Style::new().fg(ui::palette().primary)
        };

        let short_addr = if self.wallet.address.len() > 20 {
            format!(
                "{}...{}",
                &self.wallet.address[..10],
                &self.wallet.address[self.wallet.address.len() - 8..]
            )
        } else {
            self.wallet.address.clone()
        };

        let content = Paragraph::new(Line::from(vec![
            Span::styled(format!(" {:2}. ", self.wallet.index + 1), idx_style),
            Span::styled(short_addr, addr_style),
            Span::raw("  "),
            Span::styled(format!("[{}]", self.wallet.chain_name), chain_style),
        ]));
        frame.render_widget(content, area);
    }

    fn handle_event(&mut self, key: KeyEvent) -> Option<WalletAction> {
        if !self.focused {
            return None;
        }
        match key.code {
            KeyCode::Enter => Some(WalletAction::Select),
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
