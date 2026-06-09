use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::Span,
    widgets::{Block, Borders, Cell, Paragraph, Row, Table, TableState},
    Frame,
};
use crate::app::state::{AppState, LoadingKind};
use crate::ui::theme::CATPPUCCIN_MOCHA as T;
use crate::ui::util::readable_name;
use crate::ui::widgets::sparkline::TrafficSpark;

const SPINNER: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

/// Returns the card value to display, showing spinner when a relevant
/// loading operation is active.
fn card_value(normal: &str, loading_kind: LoadingKind, state: &AppState) -> String {
    if state.ui.loading.as_ref() == Some(&loading_kind) {
        let frame = SPINNER[state.ui.spinner_frame as usize % SPINNER.len()];
        format!("{} ...", frame)
    } else {
        normal.to_string()
    }
}

pub fn render(f: &mut Frame, area: Rect, state: &AppState, spark: &TrafficSpark) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5),  // row 1: Mode/Upload/Download/Conns
            Constraint::Length(5),  // row 2: TUN/SysProxy/MixPort/AllowLAN
            Constraint::Length(3),  // traffic sparkline
            Constraint::Min(1),     // groups table + info line
        ])
        .split(area);

    // Row 1: traffic stats
    let cards1 = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Ratio(1, 4),
            Constraint::Ratio(1, 4),
            Constraint::Ratio(1, 4),
            Constraint::Ratio(1, 4),
        ])
        .split(chunks[0]);

    let mode_val = card_value(&format!("{:?}", state.proxy_mode), LoadingKind::SwitchMode, state);
    card(f, cards1[0], "Mode", &mode_val, T.primary);
    card(f, cards1[1], "Upload", &format!("{:.1} KB/s", state.traffic.up as f64 / 1024.0), T.green);
    card(f, cards1[2], "Download", &format!("{:.1} KB/s", state.traffic.down as f64 / 1024.0), T.red);
    card(f, cards1[3], "Conns", &state.connections.len().to_string(), T.yellow);

    // Row 2: proxy status
    let cards2 = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Ratio(1, 4),
            Constraint::Ratio(1, 4),
            Constraint::Ratio(1, 4),
            Constraint::Ratio(1, 4),
        ])
        .split(chunks[1]);

    let tun_enabled = state.tun.as_ref().map(|t| t.enable).unwrap_or(false);
    let tun_color = if tun_enabled { T.green } else { T.surface };
    let tun_val = card_value(if tun_enabled { "ON" } else { "OFF" }, LoadingKind::ToggleProxy, state);
    card(f, cards2[0], "TUN", &tun_val, tun_color);

    let sp_color = if state.system_proxy_enabled { T.green } else { T.surface };
    let sp_val = card_value(if state.system_proxy_enabled { "ON" } else { "OFF" }, LoadingKind::ToggleProxy, state);
    card(f, cards2[1], "SysProxy", &sp_val, sp_color);

    let port_str = state.mixed_port
        .map(|p| format!(":{}", p))
        .unwrap_or_else(|| "—".into());
    card(f, cards2[2], "MixPort", &port_str, T.text);

    let lan_str = state.allow_lan
        .map(|b| if b { "Yes" } else { "No" })
        .unwrap_or("—");
    card(f, cards2[3], "AllowLAN", lan_str, T.text_secondary);

    // Traffic sparkline
    let spark_block = Block::default().title("Traffic").style(Style::default().fg(T.text_secondary));
    let inner = spark_block.inner(chunks[2]);
    f.render_widget(spark_block, chunks[2]);
    crate::ui::widgets::sparkline::render(f, inner, (T.green, T.red), spark);

    // Bottom: groups table + info line
    let bottom_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(chunks[3]);

    render_groups_table(f, bottom_chunks[0], state);

    let info_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Ratio(1, 2), Constraint::Ratio(1, 2)])
        .split(bottom_chunks[1]);
    f.render_widget(
        Paragraph::new(format!("Memory: {:.1} MB", state.memory.inuse as f64 / 1024.0))
            .style(Style::default().fg(T.text)),
        info_chunks[0],
    );
    f.render_widget(
        Paragraph::new(format!("Version: mihomo {}", state.version))
            .style(Style::default().fg(T.text)),
        info_chunks[1],
    );
}

fn render_groups_table(f: &mut Frame, area: Rect, state: &AppState) {
    let block = Block::default()
        .title(" Proxy Groups ")
        .borders(Borders::ALL)
        .style(Style::default().fg(T.text_secondary));
    let inner = block.inner(area);
    f.render_widget(block, area);

    if state.groups.is_empty() {
        f.render_widget(
            Paragraph::new("No proxy groups loaded. Press r to refresh.")
                .style(Style::default().fg(T.text_secondary)),
            inner,
        );
        return;
    }

    let header = Row::new(vec![
        Cell::from(Span::styled("Group", Style::default().fg(T.primary))),
        Cell::from(Span::styled("Type", Style::default().fg(T.primary))),
        Cell::from(Span::styled("Current Node", Style::default().fg(T.primary))),
    ])
    .style(Style::default().add_modifier(ratatui::style::Modifier::BOLD));

    let rows: Vec<Row> = state.groups.iter().map(|g| {
        let now = g.now.as_deref().unwrap_or("—");
        let now_display = readable_name(now);
        let node_style = if now == "DIRECT" {
            Style::default().fg(T.green)
        } else if now == "REJECT" {
            Style::default().fg(T.red)
        } else {
            Style::default().fg(T.text)
        };
        Row::new(vec![
            Cell::from(Span::styled(readable_name(&g.name), Style::default().fg(T.text))),
            Cell::from(Span::styled(&g.group_type, Style::default().fg(T.text_secondary))),
            Cell::from(Span::styled(now_display, node_style)),
        ])
    }).collect();

    let widths = [
        Constraint::Percentage(30),
        Constraint::Percentage(15),
        Constraint::Percentage(55),
    ];

    let table = Table::new(rows, widths)
        .header(header)
        .style(Style::default().fg(T.text))
        .row_highlight_style(Style::default().add_modifier(ratatui::style::Modifier::REVERSED));

    let mut table_state = TableState::default();
    f.render_stateful_widget(table, inner, &mut table_state);
}

fn card(f: &mut Frame, area: Rect, label: &str, value: &str, accent: Color) {
    let block = Block::default().borders(Borders::ALL).style(Style::default().fg(T.text_secondary));
    let inner = block.inner(area);
    f.render_widget(block, area);
    let chunks = Layout::default().direction(Direction::Vertical).constraints([Constraint::Length(1), Constraint::Length(1)]).split(inner);
    f.render_widget(Paragraph::new(label).style(Style::default().fg(T.text_secondary)), chunks[0]);
    f.render_widget(Paragraph::new(value).style(Style::default().fg(accent)), chunks[1]);
}
