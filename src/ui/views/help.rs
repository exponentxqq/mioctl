use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame,
};
use crate::ui::theme::CATPPUCCIN_MOCHA as T;

const HELP_TEXT: &str = r#"
 Global                     Proxy View
 ──────                     ──────────
 1-5  Switch view           Enter  Switch node
 j/↓  Move down             t      Test node delay
 k/↑  Move up               T      Test group delay
 g    Jump to top           h/←    Prev group
 G    Jump to bottom        l/→    Next group
 /    Search                Esc    Back to groups
 n/N  Next/prev search
 :    Command mode          Connections
 m    Cycle proxy mode      ───────────
 q    Quit                  d      Close connection
 ?    Toggle help           D      Close all

                            Logs
                            ────
                            Space  Pause/resume
                            s      Cycle log level

 Click sidebar to switch views  ·  Press ? or Esc to close
"#;

pub fn render(f: &mut Frame) {
    let area = centered_rect(60, 70, f.area());
    let block = Block::default()
        .title(" Keybindings (? to toggle) ")
        .borders(Borders::ALL)
        .style(Style::default().bg(T.surface));
    let inner = block.inner(area);

    f.render_widget(Clear, area); // clear behind popup
    f.render_widget(block, area);

    let text = Paragraph::new(HELP_TEXT)
        .style(Style::default().fg(T.text))
        .wrap(Wrap { trim: true });
    f.render_widget(text, inner);
}

/// Return a rectangle centered in `area` with given width/height percentages
fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let w = area.width * percent_x / 100;
    let h = area.height * percent_y / 100;
    let x = (area.width.saturating_sub(w)) / 2;
    let y = (area.height.saturating_sub(h)) / 2;
    Rect { x, y, width: w, height: h }
}
