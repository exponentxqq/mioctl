use crate::app::state::AppState;
use crate::ui::theme::CATPPUCCIN_MOCHA as T;
use ratatui::{
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, Paragraph, Wrap},
    Frame,
};

pub fn render(f: &mut Frame, area: Rect, state: &AppState) {
    // Collect filtered entries with their absolute index in state.logs
    let filtered: Vec<(usize, &crate::api::types::LogEntry)> = state
        .logs
        .iter()
        .enumerate()
        .filter(|(_, e)| match state.ui.log_level_filter.as_deref() {
            Some(level) => e.level == level,
            None => true,
        })
        .collect();

    let total = filtered.len();
    let visible = (area.height.saturating_sub(3) as usize).max(1);

    // Find cursor position in filtered list
    let cursor_abs = state.ui.log_cursor.min(state.logs.len().saturating_sub(1));
    let cursor_filt = if total > 0 {
        filtered
            .iter()
            .position(|(abs, _)| *abs >= cursor_abs)
            .unwrap_or(total.saturating_sub(1))
    } else {
        0
    };

    // Compute scroll to keep cursor visible
    let scroll = if total == 0 {
        0
    } else if cursor_filt >= total.saturating_sub(1) && total > visible {
        total.saturating_sub(visible)
    } else if total > visible {
        let pos = (cursor_filt + visible / 3).saturating_sub(visible / 3);
        pos.min(total.saturating_sub(visible))
    } else {
        0
    };

    // Selection range (absolute indices)
    let sel_abs = if state.ui.log_visual {
        let s = state.ui.log_select_start.min(state.ui.log_select_end);
        let e = state.ui.log_select_start.max(state.ui.log_select_end);
        (s, e)
    } else {
        (0, 0)
    };

    let log_lines: Vec<Line> = filtered
        .iter()
        .enumerate()
        .map(|(filt_idx, (abs_idx, entry))| {
            let color = match entry.level.as_str() {
                "error" => T.red,
                "warning" => T.yellow,
                "debug" => T.text_secondary,
                _ => T.green,
            };

            let in_selection =
                state.ui.log_visual && *abs_idx >= sel_abs.0 && *abs_idx <= sel_abs.1;
            let is_cursor = *abs_idx == cursor_abs;
            let highlighted = in_selection || is_cursor;
            let _ = filt_idx; // unused — for clarity that we have both indices

            Line::from(vec![
                Span::styled(
                    format!("{:5} ", entry.level.to_uppercase()),
                    if highlighted {
                        Style::default().fg(color).bg(T.surface)
                    } else {
                        Style::default().fg(color)
                    },
                ),
                Span::styled(
                    &entry.payload,
                    if highlighted {
                        Style::default().fg(T.text).bg(T.surface)
                    } else {
                        Style::default().fg(T.text)
                    },
                ),
            ])
        })
        .collect();

    let paused = if state.ui.log_paused { " [PAUSED]" } else { "" };
    let visual = if state.ui.log_visual { " [VISUAL]" } else { "" };
    let level = state.ui.log_level_filter.as_deref().unwrap_or("all");
    let cursor_info = if total > 0 {
        format!(" | {}/{}", cursor_filt.saturating_add(1).min(total), total)
    } else {
        String::new()
    };
    let title = format!(
        "Logs ({}){} | level: {} | s:switch space:pause{}{}",
        total, paused, level, cursor_info, visual,
    );
    let block = Block::default().title(title);
    let para = Paragraph::new(log_lines)
        .block(block)
        .wrap(Wrap { trim: true })
        .scroll((scroll as u16, 0));
    f.render_widget(para, area);
}
