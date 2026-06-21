use crate::api::types::AssetRow;
use crate::components::Component;
use crate::ui;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Layout, Rect};
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

        let chunks = Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).split(area);

        let line1_area = chunks[0];
        let line2_area = chunks[1];

        let name_style = if self.focused {
            Style::new().fg(ui::palette().foreground).bold()
        } else {
            Style::new().fg(ui::palette().foreground)
        };
        let price_style = Style::new().fg(ui::palette().foreground);
        let value_style = Style::new().fg(ui::palette().success);

        let name_width = (area.width as usize).saturating_sub(32);

        let line1 = Line::from(vec![
            Span::styled(
                format!(" {:width$} ", self.asset.name, width = name_width),
                name_style,
            ),
            Span::styled(format!(" {:>14} ", self.asset.price), price_style),
            Span::styled(format!(" {:>14} ", self.asset.holdings_value), value_style),
        ]);
        frame.render_widget(
            Paragraph::new(line1).style(Style::new().bg(row_bg)),
            line1_area,
        );

        let ticker_style = Style::new().fg(ui::palette().muted);
        let change_style = if self.asset.price_change_up {
            Style::new().fg(ui::palette().success)
        } else {
            Style::new().fg(ui::palette().error)
        };
        let amount_style = Style::new().fg(ui::palette().foreground);

        let line2 = Line::from(vec![
            Span::styled(
                format!(" {:width$} ", self.asset.ticker, width = name_width),
                ticker_style,
            ),
            Span::styled(format!(" {:>14} ", self.asset.price_change), change_style),
            Span::styled(
                format!(" {:>14} ", self.asset.holdings_amount),
                amount_style,
            ),
        ]);
        frame.render_widget(
            Paragraph::new(line2).style(Style::new().bg(row_bg)),
            line2_area,
        );
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
