use crate::app::state::AppState;
use crate::ui::theme::CATPPUCCIN_MOCHA as T;
use ratatui::{
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame,
};

pub fn render(f: &mut Frame, area: Rect, state: &AppState) {
    let items = &state.config.subscriptions.items;
    let active = state.config.subscriptions.active.as_deref();

    let mut lines: Vec<Line> = Vec::new();
    for (i, item) in items.iter().enumerate() {
        let is_active = active == Some(item.name.as_str());
        let is_selected = i == state.ui.selected_sub_idx;
        let mark = if is_active { "* " } else { "  " };
        let label = format!(
            "{}{}  {} nodes  {}",
            mark,
            item.name,
            item.node_count.unwrap_or(0),
            item.last_updated.as_deref().unwrap_or("(never)")
        );
        let style = if is_selected {
            Style::default().fg(T.primary).bg(T.surface)
        } else if is_active {
            Style::default().fg(T.green)
        } else {
            Style::default().fg(T.text)
        };
        lines.push(Line::from(Span::styled(label, style)));
    }

    if items.is_empty() {
        lines.push(Line::from(Span::styled(
            "No subscriptions — press 'a' to add, or run: mioctl sub add <url>",
            Style::default().fg(T.text_secondary),
        )));
    }

    let input_line = if state.ui.sub_input_mode {
        Line::from(Span::styled(
            format!("URL: {}_", state.ui.sub_input),
            Style::default().fg(T.yellow),
        ))
    } else {
        Line::from(Span::styled(
            "Enter 激活 · u 更新 · a 添加 · d 删除",
            Style::default().fg(T.text_secondary),
        ))
    };
    lines.push(Line::from(""));
    lines.push(input_line);

    let p = Paragraph::new(lines).wrap(Wrap { trim: false });
    f.render_widget(p, area);

    if let Some(ref name) = state.ui.confirm_remove {
        let popup = centered_rect(46, 25, f.area());
        let block = Block::default()
            .title(" Remove subscription ")
            .borders(Borders::ALL)
            .style(Style::default().bg(T.surface));
        let inner = block.inner(popup);
        f.render_widget(Clear, popup);
        f.render_widget(block, popup);
        let text = Paragraph::new(vec![
            Line::from(Span::styled(
                format!("Remove '{}'?", name),
                Style::default().fg(T.text),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "y confirm · Esc cancel",
                Style::default().fg(T.text_secondary),
            )),
        ])
        .wrap(Wrap { trim: true });
        f.render_widget(text, inner);
    }
}

fn centered_rect(px: u16, py: u16, area: Rect) -> Rect {
    let w = area.width * px / 100;
    let h = area.height * py / 100;
    let x = (area.width.saturating_sub(w)) / 2;
    let y = (area.height.saturating_sub(h)) / 2;
    Rect {
        x,
        y,
        width: w,
        height: h,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::state::ActiveView;
    use crate::config::mioctl_config::{SubscriptionItem, Subscriptions};
    use ratatui::{backend::TestBackend, Terminal};

    fn state_with_subs(subs: Subscriptions) -> AppState {
        let mut state = AppState::new();
        state.ui.loading = None;
        state.ui.active_view = ActiveView::Subscriptions;
        state.config.subscriptions = subs;
        state
    }

    fn draw(state: &AppState) -> ratatui::Terminal<TestBackend> {
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        terminal
            .draw(|f| {
                let area = f.area();
                render(f, area, state)
            })
            .unwrap();
        terminal
    }

    fn buffer_text(terminal: &Terminal<TestBackend>) -> String {
        let buffer = terminal.backend().buffer();
        let mut out = String::new();
        for y in 0..buffer.area.height {
            let mut x = 0u16;
            while x < buffer.area.width {
                let cell = &buffer[(x, y)];
                out.push_str(cell.symbol());
                let c = cell.symbol().chars().next().unwrap_or(' ');
                x += if (c as u32) >= 0x2E80 { 2 } else { 1 };
            }
            out.push('\n');
        }
        out
    }

    fn item(
        name: &str,
        url: &str,
        last_updated: Option<&str>,
        node_count: Option<usize>,
    ) -> SubscriptionItem {
        SubscriptionItem {
            name: name.into(),
            url: url.into(),
            last_updated: last_updated.map(|s| s.into()),
            node_count,
        }
    }

    #[test]
    fn test_render_empty_list_hint() {
        let state = state_with_subs(Subscriptions::default());
        let terminal = draw(&state);
        let text = buffer_text(&terminal);
        assert!(text.contains("No subscriptions — press 'a' to add"));
        assert!(text.contains("mioctl sub add <url>"));
        assert!(text.contains("Enter 激活 · u 更新 · a 添加 · d 删除"));
    }

    #[test]
    fn test_render_items_with_active_marker() {
        let mut subs = Subscriptions::default();
        subs.items
            .push(item("sub1", "https://a", Some("2026-01-01"), Some(42)));
        subs.items.push(item("sub2", "https://b", None, None));
        subs.active = Some("sub1".into());
        let state = state_with_subs(subs);

        let terminal = draw(&state);
        let text = buffer_text(&terminal);
        assert!(text.contains("* sub1  42 nodes  2026-01-01"));
        assert!(text.contains("  sub2  0 nodes  (never)"));
    }

    #[test]
    fn test_render_item_styles() {
        let mut subs = Subscriptions::default();
        subs.items.push(item("sel", "https://a", None, None));
        subs.items.push(item("act", "https://b", None, None));
        subs.items.push(item("plain", "https://c", None, None));
        subs.active = Some("act".into());
        let mut state = state_with_subs(subs);
        state.ui.selected_sub_idx = 0;

        let terminal = draw(&state);
        let buffer = terminal.backend().buffer();
        assert_eq!(buffer[(0, 0)].fg, T.primary);
        assert_eq!(buffer[(0, 0)].bg, T.surface);
        assert_eq!(buffer[(0, 1)].fg, T.green);
        assert_eq!(buffer[(0, 2)].fg, T.text);
    }

    #[test]
    fn test_render_input_mode_overlay() {
        let mut state = state_with_subs(Subscriptions::default());
        state.ui.sub_input_mode = true;
        state.ui.sub_input = "https://example.com/sub".into();

        let terminal = draw(&state);
        let text = buffer_text(&terminal);
        assert!(text.contains("URL: https://example.com/sub_"));
        assert!(!text.contains("Enter 激活"));
    }

    #[test]
    fn test_render_confirm_popup() {
        let mut subs = Subscriptions::default();
        subs.items.push(item("sub1", "https://a", None, None));
        let mut state = state_with_subs(subs);
        state.ui.confirm_remove = Some("sub1".into());

        let terminal = draw(&state);
        let text = buffer_text(&terminal);
        assert!(text.contains("Remove subscription"));
        assert!(text.contains("Remove 'sub1'?"));
        assert!(text.contains("y confirm · Esc cancel"));
    }

    #[test]
    fn test_centered_rect_values() {
        let area = Rect::new(0, 0, 100, 50);
        let r = centered_rect(46, 18, area);
        assert_eq!(r.width, 46);
        assert_eq!(r.height, 9);
        assert_eq!(r.x, 27);
        assert_eq!(r.y, 20);
    }

    #[test]
    fn test_centered_rect_saturates_small_area() {
        let area = Rect::new(5, 3, 1, 1);
        let r = centered_rect(50, 50, area);
        assert_eq!(r.width, 0);
        assert_eq!(r.height, 0);
        assert_eq!(r.x, 0);
        assert_eq!(r.y, 0);
    }
}
