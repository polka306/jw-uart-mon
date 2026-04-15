pub mod modal;
pub mod theme;

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    widgets::{Block, Borders, Paragraph},
    text::{Line, Span},
};
use crate::app::{AppState, Direction as Dir, InputMode};

pub fn render(f: &mut Frame, app: &AppState) {
    let size = f.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1), Constraint::Length(3)])
        .split(size);

    render_status(f, app, chunks[0]);
    render_rx(f, app, chunks[1]);
    render_input(f, app, chunks[2]);
    modal::render(f, app, size);
}

fn render_status(f: &mut Frame, app: &AppState, area: Rect) {
    let status_txt = if app.connected { "●Connected" } else { "○Disconnected" };
    let style = if app.connected { theme::status_connected() } else { theme::status_disconnected() };
    let port = app.config.serial.port.clone().unwrap_or_else(|| "-".into());
    let cfg = &app.config.serial;
    let parity_ch = match cfg.parity {
        crate::config::Parity::None => 'N',
        crate::config::Parity::Odd => 'O',
        crate::config::Parity::Even => 'E',
    };
    let mut spans = vec![
        Span::styled(format!("[{}] ", status_txt), style),
        Span::raw(format!(
            "{} {}-{}{}{} ",
            port, cfg.baud, cfg.data_bits, parity_ch, cfg.stop_bits
        )),
        Span::raw(format!("| RX:{}B TX:{}B ", app.rx_bytes, app.tx_bytes)),
        Span::raw(format!(
            "| HEX:{} TS:{} LE:{:?}",
            if app.show_hex { "on" } else { "off" },
            if app.show_ts { "on" } else { "off" },
            cfg.line_ending
        )),
    ];
    if let Some(n) = &app.notice {
        spans.push(Span::raw(" | "));
        spans.push(Span::styled(n.text.clone(), theme::notice(n.level)));
    }
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn render_rx(f: &mut Frame, app: &AppState, area: Rect) {
    let height = area.height.saturating_sub(2) as usize;
    let query = app.search.as_deref().filter(|q| !q.is_empty());

    // Collect (filtered) line refs
    let filtered: Vec<&crate::app::LogLine> = match query {
        Some(q) => {
            let q_lower = q.to_lowercase();
            app.lines
                .iter()
                .filter(|l| {
                    String::from_utf8_lossy(&l.bytes)
                        .to_lowercase()
                        .contains(&q_lower)
                })
                .collect()
        }
        None => app.lines.iter().collect(),
    };
    let total = filtered.len();
    let back = app.scroll.unwrap_or(0).min(total.saturating_sub(1));
    let end = total.saturating_sub(back);
    let start = end.saturating_sub(height);
    let mut out = Vec::new();
    for line in filtered.iter().take(end).skip(start).copied() {
        let mut spans: Vec<Span> = Vec::new();
        if app.show_ts {
            spans.push(Span::styled(
                format!("[{}] ", line.ts.format("%H:%M:%S%.3f")),
                theme::ts(),
            ));
        }
        let tag = match line.direction {
            Dir::Rx => "",
            Dir::Tx => "TX> ",
            Dir::System => "** ",
        };
        if !tag.is_empty() {
            spans.push(Span::raw(tag.to_string()));
        }
        if app.show_hex {
            let hex: String = line.bytes.iter().map(|b| format!("{:02X} ", b)).collect();
            spans.push(Span::styled(hex, theme::rx_line()));
        } else {
            spans.push(Span::styled(
                String::from_utf8_lossy(&line.bytes).to_string(),
                theme::rx_line(),
            ));
        }
        out.push(Line::from(spans));
    }
    let title = {
        let mut parts = vec!["RX".to_string()];
        if let Some(q) = query {
            parts.push(format!("filter: \"{}\" ({} matches)", q, total));
        }
        if let Some(n) = app.scroll {
            parts.push(format!("scrolled -{}", n));
        }
        parts.join(" | ")
    };
    let block = Block::default().borders(Borders::ALL).title(title);
    f.render_widget(Paragraph::new(out).block(block), area);
}

fn render_input(f: &mut Frame, app: &AppState, area: Rect) {
    let title = match app.input_mode {
        InputMode::Ascii => "Input (Enter=send, Ctrl+X=HEX)",
        InputMode::Hex => "Input HEX (Enter=send)",
    };
    let p = Paragraph::new(format!("> {}", app.input))
        .block(Block::default().borders(Borders::ALL).title(title));
    f.render_widget(p, area);
}
