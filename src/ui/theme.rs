use ratatui::style::{Color, Style, Modifier};

pub fn status_connected() -> Style {
    Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)
}
pub fn status_disconnected() -> Style {
    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
}
pub fn rx_line() -> Style { Style::default().fg(Color::White) }
pub fn ts() -> Style { Style::default().fg(Color::DarkGray) }
pub fn notice(level: crate::app::NoticeLevel) -> Style {
    use crate::app::NoticeLevel::*;
    match level {
        Info => Style::default().fg(Color::Cyan),
        Warn => Style::default().fg(Color::Yellow),
        Error => Style::default().fg(Color::Red),
    }
}
