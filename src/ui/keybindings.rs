use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

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
    CycleMode,
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
    Refresh,
}

pub fn parse_key(event: KeyEvent) -> Option<Action> {
    match event {
        KeyEvent { code: KeyCode::Char('q'), modifiers: KeyModifiers::NONE, .. } => Some(Action::Quit),
        KeyEvent { code: KeyCode::Char('1'), .. } => Some(Action::SwitchView(0)),
        KeyEvent { code: KeyCode::Char('2'), .. } => Some(Action::SwitchView(1)),
        KeyEvent { code: KeyCode::Char('3'), .. } => Some(Action::SwitchView(2)),
        KeyEvent { code: KeyCode::Char('4'), .. } => Some(Action::SwitchView(3)),
        KeyEvent { code: KeyCode::Char('5'), .. } => Some(Action::SwitchView(4)),
        KeyEvent { code: KeyCode::Char('j'), .. } => Some(Action::MoveDown),
        KeyEvent { code: KeyCode::Char('k'), .. } => Some(Action::MoveUp),
        KeyEvent { code: KeyCode::Down, .. } => Some(Action::MoveDown),
        KeyEvent { code: KeyCode::Up, .. } => Some(Action::MoveUp),
        KeyEvent { code: KeyCode::Char('g'), modifiers: KeyModifiers::NONE, .. } => Some(Action::JumpTop),
        KeyEvent { code: KeyCode::Char('G'), modifiers: KeyModifiers::SHIFT, .. } => Some(Action::JumpBottom),
        KeyEvent { code: KeyCode::Char('/'), .. } => Some(Action::Search),
        KeyEvent { code: KeyCode::Char('n'), .. } => Some(Action::SearchNext),
        KeyEvent { code: KeyCode::Char('N'), modifiers: KeyModifiers::SHIFT, .. } => Some(Action::SearchPrev),
        KeyEvent { code: KeyCode::Char(':'), .. } => Some(Action::CommandMode),
        KeyEvent { code: KeyCode::Char('m'), .. } => Some(Action::CycleMode),
        KeyEvent { code: KeyCode::Enter, .. } => Some(Action::SwitchNode),
        KeyEvent { code: KeyCode::Char('t'), .. } => Some(Action::TestNodeDelay),
        KeyEvent { code: KeyCode::Char('T'), .. } => Some(Action::TestGroupDelay),
        KeyEvent { code: KeyCode::Char('h'), .. } => Some(Action::PrevGroup),
        KeyEvent { code: KeyCode::Left, .. } => Some(Action::PrevGroup),
        KeyEvent { code: KeyCode::Char('l'), .. } => Some(Action::NextGroup),
        KeyEvent { code: KeyCode::Right, .. } => Some(Action::NextGroup),
        KeyEvent { code: KeyCode::Esc, .. } => Some(Action::Back),
        KeyEvent { code: KeyCode::Char('d'), .. } => Some(Action::CloseConnection),
        KeyEvent { code: KeyCode::Char('D'), modifiers: KeyModifiers::SHIFT, .. } => Some(Action::CloseAllConnections),
        KeyEvent { code: KeyCode::Char(' '), .. } => Some(Action::TogglePause),
        KeyEvent { code: KeyCode::Char('s'), .. } => Some(Action::CycleLogLevel),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn k(c: char) -> KeyEvent { KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE) }
    fn ks(c: char) -> KeyEvent { KeyEvent::new(KeyCode::Char(c), KeyModifiers::SHIFT) }

    #[test] fn test_quit() { assert_eq!(parse_key(k('q')), Some(Action::Quit)); }
    #[test] fn test_views() { assert_eq!(parse_key(k('1')), Some(Action::SwitchView(0))); assert_eq!(parse_key(k('5')), Some(Action::SwitchView(4))); }
    #[test] fn test_nav() { assert_eq!(parse_key(k('j')), Some(Action::MoveDown)); assert_eq!(parse_key(k('k')), Some(Action::MoveUp)); }
    #[test] fn test_jump() { assert_eq!(parse_key(k('g')), Some(Action::JumpTop)); assert_eq!(parse_key(ks('G')), Some(Action::JumpBottom)); }
    #[test] fn test_search() { assert_eq!(parse_key(k('/')), Some(Action::Search)); assert_eq!(parse_key(ks('N')), Some(Action::SearchPrev)); }
    #[test] fn test_dashboard() { assert_eq!(parse_key(k('m')), Some(Action::CycleMode)); }
    #[test] fn test_proxies() { assert_eq!(parse_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)), Some(Action::SwitchNode)); assert_eq!(parse_key(k('t')), Some(Action::TestNodeDelay)); }
    #[test] fn test_connections() { assert_eq!(parse_key(k('d')), Some(Action::CloseConnection)); assert_eq!(parse_key(ks('D')), Some(Action::CloseAllConnections)); }
    #[test] fn test_logs() { assert_eq!(parse_key(k(' ')), Some(Action::TogglePause)); }
    #[test] fn test_unknown() { assert_eq!(parse_key(k('z')), None); }
}
