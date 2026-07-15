use crate::app::state::AppState;
use crate::ui::theme::CATPPUCCIN_MOCHA as T;
use ratatui::{
    layout::{Constraint, Rect},
    style::Style,
    widgets::{Block, Row, Table},
    Frame,
};

pub fn render(f: &mut Frame, area: Rect, state: &AppState) {
    let header =
        Row::new(vec!["Type", "Payload", "Proxy"]).style(Style::default().fg(T.text_secondary));
    let rows: Vec<Row> = state
        .rules
        .rules
        .iter()
        .map(|r| {
            Row::new(vec![
                r.rule_type.clone(),
                r.payload.clone(),
                r.proxy.clone(),
            ])
            .style(Style::default().fg(T.text))
        })
        .collect();
    let table = Table::new(
        rows,
        [
            Constraint::Ratio(1, 4),
            Constraint::Ratio(2, 4),
            Constraint::Ratio(1, 4),
        ],
    )
    .header(header)
    .block(Block::default().title("Rules"));
    f.render_widget(table, area);
}
