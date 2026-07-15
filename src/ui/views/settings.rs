use crate::app::state::AppState;
use crate::ui::theme::CATPPUCCIN_MOCHA as T;
use ratatui::{
    layout::Rect,
    style::Style,
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame,
};

pub fn render(f: &mut Frame, state: &AppState) {
    let area = centered_rect(50, 60, f.area());
    let block = Block::default()
        .title(" Settings ")
        .borders(Borders::ALL)
        .style(Style::default().bg(T.surface));
    let inner = block.inner(area);

    f.render_widget(Clear, area);
    f.render_widget(block, area);

    let cfg = &state.config;
    let connected = if state.connected { "yes" } else { "no" };

    let text = format!(
        " Mihomo Connection\n\
         ─────────────────\n\
         API:  {}\n\
         Auth: {}\n\
         Connected: {}\n\
         Version: {}\n\n\
         Subscriptions\n\
         ─────────────\n\
         Count: {}\n\
         Update interval: {} min\n\n\
         Preferences\n\
         ───────────\n\
         Delay test URL: {}\n\
         Delay timeout: {} ms\n\n\
         Config file\n\
         ───────────\n\
         {}\n\n\
         Press Esc or click outside to close",
        cfg.mihomo.external_controller,
        if cfg.mihomo.secret.is_empty() {
            "none"
        } else {
            "***"
        },
        connected,
        state.version,
        state.config.subscriptions.items.len(),
        state.config.subscriptions.update_interval_minutes,
        state.config.preferences.delay_test_url,
        state.config.preferences.delay_test_timeout_ms,
        crate::config::mioctl_config::MioctlConfig::config_path().display(),
    );

    let p = Paragraph::new(text)
        .style(Style::default().fg(T.text))
        .wrap(Wrap { trim: true });
    f.render_widget(p, inner);
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
