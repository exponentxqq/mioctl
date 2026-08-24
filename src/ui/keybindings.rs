use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    Quit,
    SwitchView(usize),
    MoveDown,
    MoveUp,
    JumpTop,
    JumpBottom,
    Search,
    SearchNext,
    SearchPrev,
    CommandMode,
    OpenModeSelector,
    SwitchNode,
    TestNodeDelay,
    TestGroupDelay,
    PrevGroup,
    NextGroup,
    Back,
    CloseConnection,
    CloseAllConnections,
    TogglePause,
    CycleLogLevel,
    ToggleHelp,
    ShowSettings,
    ToggleProxy,
    Refresh,
    LogVisual,
    LogCopy,
    SubUpdate,
    SubAdd,
}

pub fn parse_key(event: KeyEvent) -> Option<Action> {
    match event {
        KeyEvent {
            code: KeyCode::Char('q'),
            modifiers: KeyModifiers::NONE,
            ..
        } => Some(Action::Quit),
        KeyEvent {
            code: KeyCode::Char('1'),
            ..
        } => Some(Action::SwitchView(0)),
        KeyEvent {
            code: KeyCode::Char('2'),
            ..
        } => Some(Action::SwitchView(1)),
        KeyEvent {
            code: KeyCode::Char('3'),
            ..
        } => Some(Action::SwitchView(2)),
        KeyEvent {
            code: KeyCode::Char('4'),
            ..
        } => Some(Action::SwitchView(3)),
        KeyEvent {
            code: KeyCode::Char('5'),
            ..
        } => Some(Action::SwitchView(4)),
        KeyEvent {
            code: KeyCode::Char('6'),
            ..
        } => Some(Action::SwitchView(5)),
        KeyEvent {
            code: KeyCode::Char('j'),
            ..
        } => Some(Action::MoveDown),
        KeyEvent {
            code: KeyCode::Char('k'),
            ..
        } => Some(Action::MoveUp),
        KeyEvent {
            code: KeyCode::Down,
            ..
        } => Some(Action::MoveDown),
        KeyEvent {
            code: KeyCode::Up, ..
        } => Some(Action::MoveUp),
        KeyEvent {
            code: KeyCode::Char('g'),
            modifiers: KeyModifiers::NONE,
            ..
        } => Some(Action::JumpTop),
        KeyEvent {
            code: KeyCode::Char('G'),
            modifiers: KeyModifiers::SHIFT,
            ..
        } => Some(Action::JumpBottom),
        KeyEvent {
            code: KeyCode::Char('/'),
            ..
        } => Some(Action::Search),
        KeyEvent {
            code: KeyCode::Char('n'),
            ..
        } => Some(Action::SearchNext),
        KeyEvent {
            code: KeyCode::Char('N'),
            modifiers: KeyModifiers::SHIFT,
            ..
        } => Some(Action::SearchPrev),
        KeyEvent {
            code: KeyCode::Char(':'),
            ..
        } => Some(Action::CommandMode),
        KeyEvent {
            code: KeyCode::Char('m'),
            ..
        } => Some(Action::OpenModeSelector),
        KeyEvent {
            code: KeyCode::Enter,
            ..
        } => Some(Action::SwitchNode),
        KeyEvent {
            code: KeyCode::Char('t'),
            ..
        } => Some(Action::TestNodeDelay),
        KeyEvent {
            code: KeyCode::Char('T'),
            ..
        } => Some(Action::TestGroupDelay),
        KeyEvent {
            code: KeyCode::Char('h'),
            ..
        } => Some(Action::PrevGroup),
        KeyEvent {
            code: KeyCode::Left,
            ..
        } => Some(Action::PrevGroup),
        KeyEvent {
            code: KeyCode::Char('l'),
            ..
        } => Some(Action::NextGroup),
        KeyEvent {
            code: KeyCode::Right,
            ..
        } => Some(Action::NextGroup),
        KeyEvent {
            code: KeyCode::Esc, ..
        } => Some(Action::Back),
        KeyEvent {
            code: KeyCode::Char('d'),
            ..
        } => Some(Action::CloseConnection),
        KeyEvent {
            code: KeyCode::Char('D'),
            modifiers: KeyModifiers::SHIFT,
            ..
        } => Some(Action::CloseAllConnections),
        KeyEvent {
            code: KeyCode::Char(' '),
            ..
        } => Some(Action::TogglePause),
        KeyEvent {
            code: KeyCode::Char('s'),
            ..
        } => Some(Action::CycleLogLevel),
        KeyEvent {
            code: KeyCode::Char('p'),
            ..
        } => Some(Action::ToggleProxy),
        KeyEvent {
            code: KeyCode::Char('r'),
            ..
        } => Some(Action::Refresh),
        KeyEvent {
            code: KeyCode::Char('?'),
            ..
        } => Some(Action::ToggleHelp),
        KeyEvent {
            code: KeyCode::Char('v'),
            modifiers: KeyModifiers::NONE,
            ..
        } => Some(Action::LogVisual),
        KeyEvent {
            code: KeyCode::Char('y'),
            modifiers: KeyModifiers::NONE,
            ..
        } => Some(Action::LogCopy),
        KeyEvent {
            code: KeyCode::Char('u'),
            modifiers: KeyModifiers::NONE,
            ..
        } => Some(Action::SubUpdate),
        KeyEvent {
            code: KeyCode::Char('a'),
            modifiers: KeyModifiers::NONE,
            ..
        } => Some(Action::SubAdd),
        _ => None,
    }
}

pub fn parse_mouse(event: MouseEvent) -> Option<Action> {
    match event.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            let x = event.column;
            let row = event.row as usize;
            if x < 16 {
                // Sidebar rows: 0-4 = views, 6 = Subs, 7 = Settings
                match row {
                    0..=4 => {
                        return Some(Action::SwitchView(row));
                    }
                    6 => return Some(Action::SwitchView(5)),
                    7 => return Some(Action::ShowSettings),
                    _ => {}
                }
            }
            None
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn k(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)
    }
    fn ks(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::SHIFT)
    }
    fn sidebar_click(row: u16) -> MouseEvent {
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 0,
            row,
            modifiers: KeyModifiers::NONE,
        }
    }

    #[test]
    fn test_quit() {
        assert_eq!(parse_key(k('q')), Some(Action::Quit));
    }
    #[test]
    fn test_views() {
        assert_eq!(parse_key(k('1')), Some(Action::SwitchView(0)));
        assert_eq!(parse_key(k('5')), Some(Action::SwitchView(4)));
    }
    #[test]
    fn test_nav() {
        assert_eq!(parse_key(k('j')), Some(Action::MoveDown));
        assert_eq!(parse_key(k('k')), Some(Action::MoveUp));
    }
    #[test]
    fn test_jump() {
        assert_eq!(parse_key(k('g')), Some(Action::JumpTop));
        assert_eq!(parse_key(ks('G')), Some(Action::JumpBottom));
    }
    #[test]
    fn test_search() {
        assert_eq!(parse_key(k('/')), Some(Action::Search));
        assert_eq!(parse_key(ks('N')), Some(Action::SearchPrev));
    }
    #[test]
    fn test_dashboard() {
        assert_eq!(parse_key(k('m')), Some(Action::OpenModeSelector));
    }
    #[test]
    fn test_toggle_proxy() {
        assert_eq!(parse_key(k('p')), Some(Action::ToggleProxy));
    }
    #[test]
    fn test_proxies() {
        assert_eq!(
            parse_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            Some(Action::SwitchNode)
        );
        assert_eq!(parse_key(k('t')), Some(Action::TestNodeDelay));
    }
    #[test]
    fn test_connections() {
        assert_eq!(parse_key(k('d')), Some(Action::CloseConnection));
        assert_eq!(parse_key(ks('D')), Some(Action::CloseAllConnections));
    }
    #[test]
    fn test_logs() {
        assert_eq!(parse_key(k(' ')), Some(Action::TogglePause));
    }
    #[test]
    fn test_help() {
        assert_eq!(parse_key(k('?')), Some(Action::ToggleHelp));
    }
    #[test]
    fn test_log_visual() {
        assert_eq!(parse_key(k('v')), Some(Action::LogVisual));
    }
    #[test]
    fn test_log_copy() {
        assert_eq!(parse_key(k('y')), Some(Action::LogCopy));
    }
    #[test]
    fn test_sub_keybindings() {
        assert_eq!(parse_key(k('6')), Some(Action::SwitchView(5)));
        assert_eq!(parse_key(k('u')), Some(Action::SubUpdate));
        assert_eq!(parse_key(k('a')), Some(Action::SubAdd));
    }
    #[test]
    fn test_sub_keybindings_require_no_modifiers() {
        assert_eq!(
            parse_key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL)),
            None
        );
        assert_eq!(
            parse_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::ALT)),
            None
        );
    }
    #[test]
    fn test_unknown() {
        assert_eq!(parse_key(k('z')), None);
    }
    #[test]
    fn test_mouse_sidebar_rows_0_to_4_switch_views() {
        assert_eq!(parse_mouse(sidebar_click(0)), Some(Action::SwitchView(0)));
        assert_eq!(parse_mouse(sidebar_click(4)), Some(Action::SwitchView(4)));
    }
    #[test]
    fn test_mouse_sidebar_row_6_switches_to_subscriptions() {
        assert_eq!(parse_mouse(sidebar_click(6)), Some(Action::SwitchView(5)));
    }
    #[test]
    fn test_mouse_sidebar_row_7_opens_settings() {
        assert_eq!(parse_mouse(sidebar_click(7)), Some(Action::ShowSettings));
    }
    #[test]
    fn test_mouse_sidebar_row_5_and_beyond_ignored() {
        assert_eq!(parse_mouse(sidebar_click(5)), None);
        assert_eq!(parse_mouse(sidebar_click(8)), None);
    }
    #[test]
    fn test_mouse_main_area_ignored() {
        let click = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 20,
            row: 6,
            modifiers: KeyModifiers::NONE,
        };
        assert_eq!(parse_mouse(click), None);
    }
}
