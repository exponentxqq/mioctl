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

    let mut spans = vec![
        Span::styled(format!(" {} ", connected_icon), Style::default().fg(connected_color)),
        Span::styled(format!("| mihomo {} ", state.version), Style::default().fg(T.text_secondary)),
        Span::styled(format!("| {} ", state.last_updated), Style::default().fg(T.text_secondary)),
    ];

    if let Some(ref status) = state.ui.update_status {
        spans.push(Span::styled(format!("| {} ", status), Style::default().fg(T.yellow)));
    }

    spans.push(Span::styled(
        "| 1-5 views | ? help | q quit ",
        Style::default().fg(T.text_secondary),
    ));

    let bar = Paragraph::new(Line::from(spans)).style(Style::default().bg(T.surface));
    f.render_widget(bar, area);
}
