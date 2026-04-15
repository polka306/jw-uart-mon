use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use crate::app::{AppState, Modal};

#[derive(Debug, Clone)]
pub enum Action {
    Quit,
    OpenModal(Modal),
    OpenSearch,
    CloseModal,
    ToggleHex,
    ToggleTs,
    ClearLog,
    Reconnect,
    ToggleHexInput,
    SubmitInput,
    InputChar(char),
    InputBackspace,
    HistoryUp,
    HistoryDown,
    ScrollUp,
    ScrollDown,
    ScrollBottom,
    SendMacro(u8),
    SearchChar(char),
    SearchBackspace,
    SearchCommit,
    SearchCancel,
    SettingsCursorUp,
    SettingsCursorDown,
    SettingsValuePrev,
    SettingsValueNext,
    SettingsApply,
    MacroCursorUp,
    MacroCursorDown,
    MacroToggleHex,
    MacroBeginEditName,
    MacroBeginEditPayload,
    MacroEditChar(char),
    MacroEditBackspace,
    MacroEditCommit,
    MacroEditCancel,
    MacroSave,
    None,
}

pub fn map_key(app: &AppState, key: KeyEvent) -> Action {
    if app.modal == Modal::Settings {
        return match key.code {
            KeyCode::Esc => Action::CloseModal,
            KeyCode::Up => Action::SettingsCursorUp,
            KeyCode::Down => Action::SettingsCursorDown,
            KeyCode::Left => Action::SettingsValuePrev,
            KeyCode::Right => Action::SettingsValueNext,
            KeyCode::Enter => Action::SettingsApply,
            _ => Action::None,
        };
    }
    if app.modal == Modal::MacroEditor {
        if app.macro_edit_field.is_some() {
            return match key.code {
                KeyCode::Esc => Action::MacroEditCancel,
                KeyCode::Enter => Action::MacroEditCommit,
                KeyCode::Backspace => Action::MacroEditBackspace,
                KeyCode::Char(c) => Action::MacroEditChar(c),
                _ => Action::None,
            };
        }
        return match key.code {
            KeyCode::Esc => Action::CloseModal,
            KeyCode::Up => Action::MacroCursorUp,
            KeyCode::Down => Action::MacroCursorDown,
            KeyCode::Char('n') => Action::MacroBeginEditName,
            KeyCode::Char('p') => Action::MacroBeginEditPayload,
            KeyCode::Char('h') => Action::MacroToggleHex,
            KeyCode::Char('s') => Action::MacroSave,
            _ => Action::None,
        };
    }
    if app.modal == Modal::Search {
        return match key.code {
            KeyCode::Esc => Action::SearchCancel,
            KeyCode::Enter => Action::SearchCommit,
            KeyCode::Backspace => Action::SearchBackspace,
            KeyCode::Char(c) => Action::SearchChar(c),
            _ => Action::None,
        };
    }
    if app.modal != Modal::None {
        return match key.code {
            KeyCode::Esc => Action::CloseModal,
            _ => Action::None,
        };
    }
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    match (key.code, ctrl) {
        (KeyCode::Char('q'), true) => Action::Quit,
        (KeyCode::Char('p'), true) => Action::OpenModal(Modal::PortPicker),
        (KeyCode::Char('s'), true) => Action::OpenModal(Modal::Settings),
        (KeyCode::Char('m'), true) => Action::OpenModal(Modal::MacroEditor),
        (KeyCode::Char('f'), true) => Action::OpenSearch,
        (KeyCode::Char('h'), true) => Action::ToggleHex,
        (KeyCode::Char('t'), true) => Action::ToggleTs,
        (KeyCode::Char('l'), true) => Action::ClearLog,
        (KeyCode::Char('r'), true) => Action::Reconnect,
        (KeyCode::Char('x'), true) => Action::ToggleHexInput,
        (KeyCode::F(n), _) if (1..=12).contains(&n) => Action::SendMacro(n),
        (KeyCode::Enter, _) => Action::SubmitInput,
        (KeyCode::Backspace, _) => Action::InputBackspace,
        (KeyCode::Up, _) => Action::HistoryUp,
        (KeyCode::Down, _) => Action::HistoryDown,
        (KeyCode::PageUp, _) => Action::ScrollUp,
        (KeyCode::PageDown, _) => Action::ScrollDown,
        (KeyCode::End, _) => Action::ScrollBottom,
        (KeyCode::Char(c), false) => Action::InputChar(c),
        _ => Action::None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    fn k(code: KeyCode, mods: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, mods)
    }
    #[test]
    fn ctrl_q_quits() {
        let app = AppState::new(Config::default());
        assert!(matches!(map_key(&app, k(KeyCode::Char('q'), KeyModifiers::CONTROL)), Action::Quit));
    }
    #[test]
    fn f1_sends_macro_1() {
        let app = AppState::new(Config::default());
        assert!(matches!(map_key(&app, k(KeyCode::F(1), KeyModifiers::NONE)), Action::SendMacro(1)));
    }
    #[test]
    fn modal_only_esc() {
        let mut app = AppState::new(Config::default());
        app.modal = Modal::Settings;
        assert!(matches!(map_key(&app, k(KeyCode::Char('q'), KeyModifiers::CONTROL)), Action::None));
        assert!(matches!(map_key(&app, k(KeyCode::Esc, KeyModifiers::NONE)), Action::CloseModal));
    }
}
