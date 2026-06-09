use ratatui::{
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, Paragraph, Wrap},
    Frame,
};
use crate::app::state::AppState;
use crate::ui::theme::CATPPUCCIN_MOCHA as T;

pub fn render(f: &mut Frame, area: Rect, state: &AppState) {
    let filtered: Vec<&crate::api::types::LogEntry> = match state.ui.log_level_filter.as_deref() {
        Some(level) => state.logs.iter().filter(|e| e.level == level).collect(),
        None => state.logs.iter().collect(),
    };
    let log_lines: Vec<Line> = filtered.iter().map(|entry| {
        let color = match entry.level.as_str() {
            "error" => T.red, "warning" => T.yellow, "debug" => T.text_secondary, _ => T.green,
        };
        Line::from(vec![
            Span::styled(format!("{:5} ", entry.level.to_uppercase()), Style::default().fg(color)),
            Span::styled(&entry.payload, Style::default().fg(T.text)),
        ])
    }).collect();
    let paused = if state.ui.log_paused { " [PAUSED]" } else { "" };
    let level = state.ui.log_level_filter.as_deref().unwrap_or("all");
    let block = Block::default().title(format!("Logs ({}){} | level: {} | s:switch space:pause", filtered.len(), paused, level));
    let para = Paragraph::new(log_lines).block(block).wrap(Wrap { trim: true })
        .scroll(((filtered.len().saturating_sub(1).min(u16::MAX as usize)) as u16, 0));
    f.render_widget(para, area);
}
