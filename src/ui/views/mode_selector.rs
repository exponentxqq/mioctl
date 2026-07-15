use crate::app::state::AppState;
use crate::ui::theme::CATPPUCCIN_MOCHA as T;
use ratatui::{
    layout::Rect,
    style::{Style, Stylize},
    text::Line,
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame,
};

const MODES: &[(&str, &str)] = &[
    ("Rule", "按规则路由，匹配分流策略"),
    ("Global", "全部流量经代理服务器转发"),
    ("Direct", "全部流量直连，不经代理"),
];

pub fn render(f: &mut Frame, state: &AppState) {
    let area = centered_rect(50, 55, f.area());
    let block = Block::default()
        .title(" Proxy Mode  ↑↓/jk 选 ")
        .borders(Borders::ALL)
        .style(Style::default().bg(T.surface));
    let inner = block.inner(area);

    f.render_widget(Clear, area);
    f.render_widget(block, area);

    let mut lines: Vec<Line> = Vec::new();
    let current_mode = format!("{:?}", state.proxy_mode);

    for (i, (name, desc)) in MODES.iter().enumerate() {
        let is_current = *name == current_mode;
        let is_highlighted = i == state.ui.mode_selector_idx;

        let prefix = if is_current { "✓ " } else { "  " };
        let label = if is_highlighted {
            format!("{}{}", prefix, name).fg(T.primary).bold()
        } else {
            format!("{}{}", prefix, name).fg(T.text)
        };
        lines.push(Line::from(label));

        let detail = if is_highlighted {
            format!("   {}", desc).fg(T.text)
        } else {
            format!("   {}", desc).fg(T.text_secondary)
        };
        lines.push(Line::from(detail));
        lines.push(Line::from(""));
    }

    lines.push(Line::from("Enter 确认 · Esc 取消".fg(T.text_secondary)));

    let text = Paragraph::new(lines).wrap(Wrap { trim: true });
    f.render_widget(text, inner);
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
