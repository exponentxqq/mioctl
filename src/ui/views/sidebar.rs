use crate::app::state::{ActiveView, AppState};
use crate::ui::theme::CATPPUCCIN_MOCHA as T;
use ratatui::{
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, List, ListItem},
    Frame,
};

pub fn render(f: &mut Frame, area: Rect, state: &AppState) {
    let conns_label = format!("Conns ({:<3})", state.connections.len());
    let rules_label = format!("Rules ({:<3})", state.rules.rules.len());

    let items: Vec<ListItem> = vec![
        item("Dashboard  ", ActiveView::Dashboard, &state.ui.active_view),
        item("Proxies    ", ActiveView::Proxies, &state.ui.active_view),
        item(&conns_label, ActiveView::Connections, &state.ui.active_view),
        item(&rules_label, ActiveView::Rules, &state.ui.active_view),
        item("Logs       ", ActiveView::Logs, &state.ui.active_view),
        ListItem::new(""),
        item(
            "Subs       ",
            ActiveView::Subscriptions,
            &state.ui.active_view,
        ),
        item("Settings   ", ActiveView::Dashboard, &state.ui.active_view),
    ];
    let list = List::new(items).block(Block::default().style(Style::default().bg(T.bg)));
    f.render_widget(list, area);
}

fn item<'a>(label: &'a str, view: ActiveView, current: &ActiveView) -> ListItem<'a> {
    let is_active = *current == view;
    let style = if is_active {
        Style::default().fg(T.primary).bg(T.surface)
    } else {
        Style::default().fg(T.text_secondary)
    };
    ListItem::new(Line::from(Span::styled(label.to_string(), style)))
}
