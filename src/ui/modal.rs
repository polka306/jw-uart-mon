use ratatui::{
    layout::Rect,
    widgets::{Block, Borders, Paragraph, Clear, List, ListItem},
    Frame,
};
use crate::app::{AppState, Modal};

pub fn render(f: &mut Frame, app: &AppState, area: Rect) {
    if app.modal == Modal::None { return; }
    let r = centered(area, 60, 40);
    f.render_widget(Clear, r);
    match app.modal {
        Modal::PortPicker => {
            let items: Vec<ListItem> = crate::serial::list_ports()
                .into_iter()
                .map(ListItem::new)
                .collect();
            let list = List::new(items).block(
                Block::default().borders(Borders::ALL).title("Ports (Esc to close)"),
            );
            f.render_widget(list, r);
        }
        Modal::Settings => {
            let c = &app.config.serial;
            let text = format!(
                "baud: {}\ndata: {}\nparity: {:?}\nstop: {}\nflow: {:?}\nline ending: {:?}\n\n(read-only in MVP; edit config.toml)",
                c.baud, c.data_bits, c.parity, c.stop_bits, c.flow, c.line_ending
            );
            let p = Paragraph::new(text)
                .block(Block::default().borders(Borders::ALL).title("Settings"));
            f.render_widget(p, r);
        }
        Modal::MacroEditor => {
            let items: Vec<ListItem> = app
                .config
                .macros
                .iter()
                .map(|m| ListItem::new(format!("F{} {} : {}", m.slot, m.name, m.payload)))
                .collect();
            let list = List::new(items).block(
                Block::default().borders(Borders::ALL).title("Macros (edit config.toml)"),
            );
            f.render_widget(list, r);
        }
        Modal::Search => {
            let text = app.search.clone().unwrap_or_default();
            let p = Paragraph::new(text)
                .block(Block::default().borders(Borders::ALL).title("Search"));
            f.render_widget(p, r);
        }
        Modal::None => {}
    }
}

fn centered(area: Rect, w: u16, h: u16) -> Rect {
    let w = area.width.saturating_mul(w) / 100;
    let h = area.height.saturating_mul(h) / 100;
    let x = area.x + (area.width.saturating_sub(w)) / 2;
    let y = area.y + (area.height.saturating_sub(h)) / 2;
    Rect { x, y, width: w, height: h }
}
