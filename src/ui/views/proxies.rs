use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Row, Table, TableState},
    Frame,
};
use crate::app::state::AppState;
use crate::ui::theme::CATPPUCCIN_MOCHA as T;

pub fn render(f: &mut Frame, area: Rect, state: &AppState, table_state: &mut TableState) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Ratio(1, 3), Constraint::Ratio(2, 3)])
        .split(area);

    let group_items: Vec<ListItem> = state.groups.iter().enumerate().map(|(i, g)| {
        let style = if i == state.ui.selected_group_idx {
            Style::default().fg(T.primary).bg(T.surface)
        } else {
            Style::default().fg(T.text)
        };
        ListItem::new(Line::from(Span::styled(format!("{} ({})", g.name, g.all.len()), style)))
    }).collect();
    let groups = List::new(group_items).block(Block::default().title("Groups").borders(Borders::RIGHT));
    f.render_widget(groups, chunks[0]);

    let group = match state.groups.get(state.ui.selected_group_idx) {
        Some(g) => g,
        None => {
            f.render_widget(Paragraph::new("No groups").style(Style::default().fg(T.text)), chunks[1]);
            return;
        }
    };

    let header = Row::new(vec!["Name", "Type", "Latency"]).style(Style::default().fg(T.text_secondary));
    let selected_name = group.now.as_deref().unwrap_or("");
    let rows: Vec<Row> = group.all.iter().enumerate().map(|(i, name)| {
        let proxy = state.proxies.proxies.get(name);
        let ptype = proxy.map(|p| p.proxy_type.as_str()).unwrap_or("?");
        let delay = proxy.and_then(|p| p.history.last().map(|h| format!("{}ms", h.delay))).unwrap_or_else(|| "-".into());
        let prefix = if name == selected_name { "* " } else { "  " };
        let style = if i == state.ui.selected_node_idx {
            Style::default().fg(T.primary).bg(T.surface)
        } else if name == selected_name {
            Style::default().fg(T.green)
        } else {
            Style::default().fg(T.text)
        };
        Row::new(vec![format!("{}{}", prefix, name), ptype.into(), delay]).style(style)
    }).collect();

    let table = Table::new(rows, [Constraint::Ratio(2,5), Constraint::Ratio(1,5), Constraint::Ratio(2,5)])
        .header(header)
        .block(Block::default().title(format!("Nodes - {}", group.name)));
    f.render_stateful_widget(table, chunks[1], table_state);
}
