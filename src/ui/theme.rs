use ratatui::style::Color;

pub struct Theme {
    pub bg: Color,
    pub surface: Color,
    pub primary: Color,
    pub green: Color,
    pub red: Color,
    pub yellow: Color,
    pub text: Color,
    pub text_secondary: Color,
}

pub const CATPPUCCIN_MOCHA: Theme = Theme {
    bg: Color::Rgb(30, 30, 46),
    surface: Color::Rgb(49, 50, 68),
    primary: Color::Rgb(203, 166, 247),
    green: Color::Rgb(166, 227, 161),
    red: Color::Rgb(243, 139, 168),
    yellow: Color::Rgb(249, 226, 175),
    text: Color::Rgb(205, 214, 244),
    text_secondary: Color::Rgb(166, 173, 200),
};
