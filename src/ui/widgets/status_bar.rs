use ratatui::{
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};
use crate::app::state::AppState;
use crate::ui::theme::CATPPUCCIN_MOCHA as T;

pub fn render(f: &mut Frame, area: Rect, state: &AppState) {
    let connected_icon = if state.connected { "connected" } else { "disconnected" };
    let connected_color = if state.connected { T.green } else { T.red };
    let left = Line::from(vec![
        Span::styled(format!(" {} ", connected_icon), Style::default().fg(connected_color)),
        Span::styled(format!("| mihomo {} ", state.version), Style::default().fg(T.text_secondary)),
    ]);
    let right = Line::from(vec![
        Span::styled(
            format!("{} | 1-5 views | :cmd | q quit ", state.last_updated),
            Style::default().fg(T.text_secondary),
        ),
    ]);
    let bar = Paragraph::new(vec![left, right]).style(Style::default().bg(T.surface));
    f.render_widget(bar, area);
}
