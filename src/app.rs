use std::collections::VecDeque;
use chrono::{DateTime, Local};
use crate::config::{Config, Macro};

#[derive(Debug, Clone)]
pub struct LogLine {
    pub ts: DateTime<Local>,
    pub direction: Direction,
    pub bytes: Vec<u8>,
}
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Direction { Rx, Tx, System }

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NoticeLevel { Info, Warn, Error }

#[derive(Debug, Clone)]
pub struct Notice { pub level: NoticeLevel, pub text: String, pub at: DateTime<Local> }

#[derive(Debug, Clone, PartialEq)]
pub enum Modal { None, PortPicker, Settings, MacroEditor, Search }

#[derive(Debug, Clone, PartialEq)]
pub enum InputMode { Ascii, Hex }

pub struct AppState {
    pub config: Config,
    pub lines: VecDeque<LogLine>,
    pub capacity: usize,
    pub input: String,
    pub input_mode: InputMode,
    pub history: VecDeque<String>,
    pub history_cursor: Option<usize>,
    pub modal: Modal,
    pub notice: Option<Notice>,
    pub connected: bool,
    pub scroll: Option<usize>,
    pub show_hex: bool,
    pub show_ts: bool,
    pub search: Option<String>,
    pub quit: bool,
    pub rx_bytes: u64,
    pub tx_bytes: u64,
}

impl AppState {
    pub fn new(config: Config) -> Self {
        let capacity = config.ui.ring_capacity.max(100);
        let show_hex = config.ui.hex;
        let show_ts = config.ui.timestamps;
        Self {
            config,
            capacity,
            lines: VecDeque::with_capacity(capacity),
            input: String::new(),
            input_mode: InputMode::Ascii,
            history: VecDeque::with_capacity(50),
            history_cursor: None,
            modal: Modal::None,
            notice: None,
            connected: false,
            scroll: None,
            show_hex,
            show_ts,
            search: None,
            quit: false,
            rx_bytes: 0,
            tx_bytes: 0,
        }
    }

    pub fn push_line(&mut self, line: LogLine) {
        if self.lines.len() == self.capacity {
            self.lines.pop_front();
        }
        self.lines.push_back(line);
    }
    pub fn clear_lines(&mut self) {
        self.lines.clear();
        self.scroll = None;
    }
    /// Scroll up (toward older lines) by `n`. Caps at history top.
    pub fn scroll_up(&mut self, n: usize) {
        let cur = self.scroll.unwrap_or(0);
        let max_back = self.lines.len().saturating_sub(1);
        self.scroll = Some((cur + n).min(max_back));
    }
    /// Scroll down (toward newer lines) by `n`. Reaching 0 resumes follow-bottom.
    pub fn scroll_down(&mut self, n: usize) {
        match self.scroll {
            None => {}
            Some(cur) => {
                if cur <= n { self.scroll = None; }
                else { self.scroll = Some(cur - n); }
            }
        }
    }
    pub fn scroll_bottom(&mut self) { self.scroll = None; }
    pub fn macro_by_slot(&self, slot: u8) -> Option<&Macro> {
        self.config.macros.iter().find(|m| m.slot == slot)
    }
    pub fn set_notice(&mut self, level: NoticeLevel, text: impl Into<String>) {
        self.notice = Some(Notice { level, text: text.into(), at: Local::now() });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    fn line(s: &str) -> LogLine {
        LogLine { ts: Local::now(), direction: Direction::Rx, bytes: s.as_bytes().to_vec() }
    }
    #[test]
    fn ring_buffer_drops_oldest() {
        let mut cfg = Config::default();
        cfg.ui.ring_capacity = 3;
        let mut app = AppState::new(cfg);
        // capacity is max(3, 100) = 100 per code; adjust test:
        app.capacity = 3;
        for i in 0..5 { app.push_line(line(&i.to_string())); }
        assert_eq!(app.lines.len(), 3);
        assert_eq!(app.lines.front().unwrap().bytes, b"2");
    }
    #[test]
    fn clear_resets_scroll() {
        let mut app = AppState::new(Config::default());
        app.push_line(line("a"));
        app.scroll = Some(0);
        app.clear_lines();
        assert!(app.lines.is_empty());
        assert_eq!(app.scroll, None);
    }
    #[test]
    fn macro_slot_lookup() {
        let mut cfg = Config::default();
        cfg.macros.push(Macro { slot: 3, name: "x".into(), payload: "p".into(), hex: false });
        let app = AppState::new(cfg);
        assert!(app.macro_by_slot(3).is_some());
        assert!(app.macro_by_slot(1).is_none());
    }
}
