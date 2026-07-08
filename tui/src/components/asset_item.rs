use crate::api::types::AssetRow;
use crate::components::Component;
use crate::ui;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

pub enum AssetAction {
    Send,
}

pub struct AssetItem {
    asset: AssetRow,
    focused: bool,
}

impl AssetItem {
    pub fn new(asset: AssetRow) -> Self {
        Self {
            asset,
            focused: false,
        }
    }
}

impl Component<AssetAction> for AssetItem {
    fn render(&mut self, frame: &mut Frame, area: Rect) {
        let row_bg = if self.focused { ui::SURFACE } else { ui::BG };

        let name_style = if self.focused {
            Style::new().fg(ui::palette().foreground).bold()
        } else {
            Style::new().fg(ui::palette().foreground)
        };
        let amount_style = Style::new().fg(ui::palette().foreground);

        let name_width = (area.width as usize).saturating_sub(32);

        let line = Line::from(vec![
            Span::styled(
                format!(" {:width$} ", self.asset.name, width = name_width),
                name_style,
            ),
            Span::styled(
                format!(" {:>14} ", self.asset.holdings_amount),
                amount_style,
            ),
            Span::styled(format!(" {:>14} ", ""), Style::new()),
        ]);
        frame.render_widget(Paragraph::new(line).style(Style::new().bg(row_bg)), area);
    }

    fn handle_event(&mut self, key: KeyEvent) -> Option<AssetAction> {
        if !self.focused {
            return None;
        }
        match key.code {
            KeyCode::Enter => Some(AssetAction::Send),
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
