use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Row, Table, TableState},
    Frame,
};
use crate::app::state::AppState;
use crate::ui::theme::CATPPUCCIN_MOCHA as T;
use crate::ui::util::readable_name;

pub fn render(f: &mut Frame, area: Rect, state: &AppState, table_state: &mut TableState) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Ratio(1, 3), Constraint::Ratio(2, 3)])
        .split(area);

    if state.groups.is_empty() {
        let msg = if state.connected {
            "No proxy groups found.\n\nYour mihomo config may not have any\nproxy-groups configured, or the\nproxies API returned empty data."
        } else {
            "Waiting for connection...\n\nMake sure mihomo is running with:\n  external-controller: 127.0.0.1:9090\n\nEdit config in:\n  ~/.config/mioctl/config.toml"
        };
        f.render_widget(
            Paragraph::new(msg).style(Style::default().fg(T.text_secondary)),
            area,
        );
        return;
    }

    let group_items: Vec<ListItem> = state.groups.iter().enumerate().map(|(i, g)| {
        let style = if i == state.ui.selected_group_idx {
            Style::default().fg(T.primary).bg(T.surface)
        } else {
            Style::default().fg(T.text)
        };
        ListItem::new(Line::from(Span::styled(format!("{} ({})", readable_name(&g.name), g.all.len()), style)))
    }).collect();
    let groups = List::new(group_items).block(Block::default().title("Groups").borders(Borders::RIGHT));
    f.render_widget(groups, chunks[0]);

    let group = match state.groups.get(state.ui.selected_group_idx) {
        Some(g) => g,
        None => {
            f.render_widget(Paragraph::new("Select a group").style(Style::default().fg(T.text)), chunks[1]);
            return;
        }
    };

    // Filter nodes by search query
    let visible_nodes: Vec<(usize, &String)> = if state.ui.search_query.is_empty() {
        group.all.iter().enumerate().collect()
    } else {
        let query = state.ui.search_query.to_lowercase();
        group.all.iter().enumerate()
            .filter(|(_, name)| name.to_lowercase().contains(&query))
            .collect()
    };

    let header = Row::new(vec!["Name", "Type", "Latency"]).style(Style::default().fg(T.text_secondary));
    let selected_name = group.now.as_deref().unwrap_or("");

    let search_query = state.ui.search_query.to_lowercase();
    let rows: Vec<Row> = visible_nodes.iter().map(|(orig_idx, name)| {
        let proxy = state.proxies.proxies.get(*name);
        let ptype = proxy.map(|p| p.proxy_type.as_str()).unwrap_or("?");
        let delay = proxy.and_then(|p| p.history.last().map(|h| format!("{}ms", h.delay))).unwrap_or_else(|| "-".into());
        let prefix = if *name == selected_name { "* " } else { "  " };
        let is_match = !search_query.is_empty() && name.to_lowercase().contains(&search_query);
        let style = if *orig_idx == state.ui.selected_node_idx {
            Style::default().fg(T.primary).bg(T.surface)
        } else if **name == selected_name {
            Style::default().fg(T.green)
        } else if is_match {
            Style::default().fg(T.yellow)
        } else {
            Style::default().fg(T.text)
        };
        Row::new(vec![format!("{}{}", prefix, readable_name(name)), ptype.into(), delay]).style(style)
    }).collect();

    let title = if state.ui.search_mode {
        format!("Nodes - {} [search: /{}]", group.name, state.ui.search_query)
    } else if !state.ui.search_query.is_empty() {
        format!("Nodes - {} [filter: {}]", group.name, state.ui.search_query)
    } else {
        format!("Nodes - {}", group.name)
    };

    let table = Table::new(rows, [Constraint::Ratio(2,5), Constraint::Ratio(1,5), Constraint::Ratio(2,5)])
        .header(header)
        .block(Block::default().title(title));

    // Sync TableState selection with visible_nodes index
    let visible_idx = visible_nodes.iter().position(|(orig_idx, _)| *orig_idx == state.ui.selected_node_idx);
    table_state.select(visible_idx);

    f.render_stateful_widget(table, chunks[1], table_state);
}
