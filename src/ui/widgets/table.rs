use ratatui::widgets::TableState;

pub struct SelectableTable {
    pub state: TableState,
    pub items: Vec<Vec<String>>,
}

impl SelectableTable {
    pub fn new() -> Self {
        Self {
            state: TableState::default().with_selected(0),
            items: Vec::new(),
        }
    }

    pub fn next(&mut self) {
        if !self.items.is_empty() {
            let i = self.state.selected().unwrap_or(0);
            self.state.select(Some((i + 1).min(self.items.len() - 1)));
        }
    }

    pub fn prev(&mut self) {
        if !self.items.is_empty() {
            let i = self.state.selected().unwrap_or(0);
            self.state.select(Some(i.saturating_sub(1)));
        }
    }

    pub fn select_first(&mut self) {
        self.state.select(if self.items.is_empty() { None } else { Some(0) });
    }

    pub fn select_last(&mut self) {
        self.state.select(if self.items.is_empty() {
            None
        } else {
            Some(self.items.len() - 1)
        });
    }
}
