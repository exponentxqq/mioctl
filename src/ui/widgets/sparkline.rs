use ratatui::{
    layout::Rect,
    style::{Color, Style},
    symbols,
    widgets::Sparkline,
    Frame,
};

const HISTORY_LEN: usize = 30;

pub struct TrafficSpark {
    pub up_data: Vec<u64>,
    pub down_data: Vec<u64>,
}

impl Default for TrafficSpark {
    fn default() -> Self {
        Self::new()
    }
}

impl TrafficSpark {
    pub fn new() -> Self {
        Self {
            up_data: Vec::with_capacity(HISTORY_LEN),
            down_data: Vec::with_capacity(HISTORY_LEN),
        }
    }

    #[allow(dead_code)]
    pub fn push(&mut self, up: u64, down: u64) {
        self.up_data.push(up);
        self.down_data.push(down);
        if self.up_data.len() > HISTORY_LEN {
            self.up_data.remove(0);
        }
        if self.down_data.len() > HISTORY_LEN {
            self.down_data.remove(0);
        }
    }
}

pub fn render(f: &mut Frame, area: Rect, colors: (Color, Color), data: &TrafficSpark) {
    let up_max = data.up_data.iter().max().copied().unwrap_or(1);
    let down_max = data.down_data.iter().max().copied().unwrap_or(1);
    let up_widget = Sparkline::default()
        .data(&data.up_data)
        .max(up_max)
        .style(Style::default().fg(colors.0))
        .bar_set(symbols::bar::NINE_LEVELS);
    let down_widget = Sparkline::default()
        .data(&data.down_data)
        .max(down_max)
        .style(Style::default().fg(colors.1))
        .bar_set(symbols::bar::NINE_LEVELS);
    let up_area = Rect::new(area.x, area.y, area.width, area.height / 2);
    let down_area = Rect::new(area.x, area.y + area.height / 2, area.width, area.height / 2);
    f.render_widget(up_widget, up_area);
    f.render_widget(down_widget, down_area);
}
