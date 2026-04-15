use ratatui::{
    layout::Rect,
    widgets::{Block, Borders, Paragraph, Clear, List, ListItem},
    Frame,
};
use crate::app::{AppState, Modal};

pub fn render(f: &mut Frame, app: &AppState, area: Rect) {
    if app.modal == Modal::None { return; }
    let (w, h) = if app.modal == Modal::Help { (70, 90) } else { (60, 40) };
    let r = centered(area, w, h);
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
            let fields = [
                format!("baud:         {}", c.baud),
                format!("data bits:    {}", c.data_bits),
                format!("parity:       {:?}", c.parity),
                format!("stop bits:    {}", c.stop_bits),
                format!("flow:         {:?}", c.flow),
                format!("line ending:  {:?}", c.line_ending),
            ];
            let mut body = String::new();
            for (i, line) in fields.iter().enumerate() {
                let marker = if i == app.settings_cursor { "> " } else { "  " };
                body.push_str(&format!("{}{}\n", marker, line));
            }
            body.push_str("\n↑/↓ select  ←/→ change  Enter=apply+save  Esc=cancel");
            let p = Paragraph::new(body)
                .block(Block::default().borders(Borders::ALL).title("Settings"));
            f.render_widget(p, r);
        }
        Modal::MacroEditor => {
            let mut body = String::new();
            if app.config.macros.is_empty() {
                body.push_str("(no macros — press n or p to add first slot)\n");
            }
            for (i, m) in app.config.macros.iter().enumerate() {
                let marker = if i == app.macro_cursor { "> " } else { "  " };
                let hex_tag = if m.hex { "[HEX]" } else { "[ASCII]" };
                body.push_str(&format!(
                    "{}F{:<2} {} {} : {}\n",
                    marker, m.slot, hex_tag, m.name, m.payload
                ));
            }
            if let Some(field) = app.macro_edit_field {
                body.push_str(&format!(
                    "\nEditing {:?}: {}\nEnter=commit  Esc=cancel\n",
                    field, app.macro_edit_buf
                ));
            } else {
                body.push_str("\n↑/↓ select  n=name  p=payload  h=toggle HEX  s=save  Esc=close\n");
            }
            let p = Paragraph::new(body)
                .block(Block::default().borders(Borders::ALL).title("Macros"));
            f.render_widget(p, r);
        }
        Modal::Search => {
            let text = app.search.clone().unwrap_or_default();
            let p = Paragraph::new(format!(
                "Query: {}\n\nEnter=keep filter  Esc=cancel & clear",
                text
            ))
            .block(Block::default().borders(Borders::ALL).title("Search (live filter)"));
            f.render_widget(p, r);
        }
        Modal::Help => {
            let body = "\
Global
  Ctrl+Q        quit
  Ctrl+G        this help
  Ctrl+P        port picker
  Ctrl+S        settings (editable)
  Ctrl+K        macro editor
  Ctrl+F        search / filter
  Ctrl+R        reconnect
  Ctrl+L        clear RX log
  Ctrl+H        toggle HEX/ASCII display
  Ctrl+T        toggle timestamps
  Ctrl+X        toggle HEX TX input mode

Input
  Enter         send
  Backspace     delete char
  Up / Down     TX history

RX view
  PageUp/Down   scroll
  End           jump to bottom

Macros
  F1..F12       send macro slot 1..12

Settings modal
  Up/Down       select field
  Left/Right    cycle value
  Enter         apply & save
  Esc           cancel

Macro modal
  Up/Down       select slot
  n / p         edit name / payload
  h             toggle HEX mode
  s             save to file
  Esc           close / cancel edit

Search modal
  type          live filter
  Enter         keep filter
  Esc           cancel & clear

Esc closes any modal.";
            let p = Paragraph::new(body)
                .block(Block::default().borders(Borders::ALL).title("Help (Esc to close)"));
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
