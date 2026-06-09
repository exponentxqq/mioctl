use ratatui::{
    layout::{Constraint, Rect},
    style::Style,
    widgets::{Block, Row, Table, TableState},
    Frame,
};
use crate::app::state::AppState;
use crate::ui::theme::CATPPUCCIN_MOCHA as T;

pub fn render(f: &mut Frame, area: Rect, state: &AppState, table_state: &mut TableState) {
    let header = Row::new(vec!["Source", "Destination", "Proxy", "Rule", "Traffic"])
        .style(Style::default().fg(T.text_secondary));
    let rows: Vec<Row> = state.connections.iter().enumerate().map(|(i, c)| {
        let source = format!("{}:{}", c.metadata.source_ip, c.metadata.source_port);
        let dest_host = if c.metadata.host.is_empty() { &c.metadata.destination_ip } else { &c.metadata.host };
        let dest = format!("{}:{}", dest_host, c.metadata.destination_port);
        let proxy = c.chains.last().cloned().unwrap_or_default();
        let traffic = format_size(c.download + c.upload);
        let style = if i == state.ui.selected_conn_idx {
            Style::default().fg(T.primary).bg(T.surface)
        } else {
            Style::default().fg(T.text)
        };
        Row::new(vec![source, dest, proxy, c.rule.clone(), traffic]).style(style)
    }).collect();

    let table = Table::new(rows, [Constraint::Ratio(1,5); 5])
        .header(header)
        .block(Block::default().title("Connections"));

    // Sync TableState selection with selected_conn_idx
    table_state.select(Some(state.ui.selected_conn_idx));
    f.render_stateful_widget(table, area, table_state);
}

fn format_size(bytes: u64) -> String {
    if bytes < 1024 { format!("{}B", bytes) }
    else if bytes < 1048576 { format!("{:.0}KB", bytes as f64 / 1024.0) }
    else { format!("{:.1}MB", bytes as f64 / 1048576.0) }
}
