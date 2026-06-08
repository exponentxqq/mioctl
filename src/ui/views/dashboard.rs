use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    widgets::{Block, Borders, Paragraph},
    Frame,
};
use crate::app::state::AppState;
use crate::ui::theme::CATPPUCCIN_MOCHA as T;
use crate::ui::widgets::sparkline::TrafficSpark;

pub fn render(f: &mut Frame, area: Rect, state: &AppState, spark: &TrafficSpark) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(5), Constraint::Length(3), Constraint::Length(3)])
        .split(area);

    let cards = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Ratio(1,4), Constraint::Ratio(1,4), Constraint::Ratio(1,4), Constraint::Ratio(1,4)])
        .split(chunks[0]);

    card(f, cards[0], "Mode", &format!("{:?}", state.proxy_mode), T.primary);
    card(f, cards[1], "Upload", &format!("{:.1} KB/s", state.traffic.up as f64 / 1024.0), T.green);
    card(f, cards[2], "Download", &format!("{:.1} KB/s", state.traffic.down as f64 / 1024.0), T.red);
    card(f, cards[3], "Conns", &state.connections.len().to_string(), T.yellow);

    let spark_block = Block::default().title("Traffic").style(Style::default().fg(T.text_secondary));
    let inner = spark_block.inner(chunks[1]);
    f.render_widget(spark_block, chunks[1]);
    crate::ui::widgets::sparkline::render(f, inner, (T.green, T.red), spark);

    let info_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Ratio(1,2), Constraint::Ratio(1,2)])
        .split(chunks[2]);
    f.render_widget(Paragraph::new(format!("Memory: {:.1} MB", state.memory.inuse as f64 / 1024.0)).style(Style::default().fg(T.text)), info_chunks[0]);
    f.render_widget(Paragraph::new(format!("Version: mihomo {}", state.version)).style(Style::default().fg(T.text)), info_chunks[1]);
}

fn card(f: &mut Frame, area: Rect, label: &str, value: &str, accent: Color) {
    let block = Block::default().borders(Borders::ALL).style(Style::default().fg(T.text_secondary));
    let inner = block.inner(area);
    f.render_widget(block, area);
    let chunks = Layout::default().direction(Direction::Vertical).constraints([Constraint::Length(1), Constraint::Length(1)]).split(inner);
    f.render_widget(Paragraph::new(label).style(Style::default().fg(T.text_secondary)), chunks[0]);
    f.render_widget(Paragraph::new(value).style(Style::default().fg(accent)), chunks[1]);
}
