use std::path::PathBuf;
use std::time::Duration;
use anyhow::Result;
use clap::Parser;
use crossbeam_channel::{select, unbounded};
use crossterm::{
    event::{self, Event, KeyEvent},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use uart_mon::{
    app::{AppState, Direction as Dir, InputMode, LogLine, MacroField, Modal, NoticeLevel},
    config::{default_config_path, default_log_dir, Config, FlowControl, LineEnding, Macro, Parity},
    input::{map_key, Action},
    log_writer::LogWriter,
    serial::{parse_hex_tx, LineSplitter, SerialEvent, SerialWorker, TxCommand},
    ui,
};

const BAUD_PRESETS: &[u32] = &[
    1200, 2400, 4800, 9600, 19200, 38400, 57600, 115200, 230400, 460800, 921600,
];
const SETTINGS_FIELDS: usize = 6;

#[derive(Parser)]
#[command(name = "uart-mon", version)]
struct Cli {
    #[arg(long)]
    port: Option<String>,
    #[arg(long)]
    baud: Option<u32>,
    #[arg(long = "log-dir")]
    log_dir: Option<PathBuf>,
    #[arg(long)]
    config: Option<PathBuf>,
    #[arg(long = "no-log")]
    no_log: bool,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let cfg_path = cli.config.clone().or_else(default_config_path);
    let mut config = match cfg_path.as_deref() {
        Some(p) => Config::load(p).unwrap_or_default(),
        None => Config::default(),
    };
    if let Some(p) = cli.port {
        config.serial.port = Some(p);
    }
    if let Some(b) = cli.baud {
        config.serial.baud = b;
    }

    let log_dir = if cli.no_log { None } else { cli.log_dir.or_else(default_log_dir) };
    let log_writer = LogWriter::spawn(log_dir);

    let mut app = AppState::new(config.clone());

    let (evt_tx, evt_rx) = unbounded::<SerialEvent>();
    let worker = SerialWorker::spawn(config.serial.clone(), evt_tx);

    let (key_tx, key_rx) = unbounded::<KeyEvent>();
    std::thread::spawn(move || loop {
        if event::poll(Duration::from_millis(100)).unwrap_or(false) {
            if let Ok(Event::Key(k)) = event::read() {
                if key_tx.send(k).is_err() {
                    break;
                }
            }
        }
    });

    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout))?;

    let mut splitter = LineSplitter::new();
    let tick = crossbeam_channel::tick(Duration::from_millis(50));

    let result = (|| -> Result<()> {
        loop {
            terminal.draw(|f| ui::render(f, &app))?;
            if app.quit {
                break;
            }
            select! {
                recv(evt_rx) -> msg => {
                    if let Ok(ev) = msg {
                        handle_serial(&mut app, ev, &mut splitter, &log_writer);
                    }
                }
                recv(key_rx) -> msg => {
                    if let Ok(k) = msg {
                        handle_key(&mut app, k, &worker, &log_writer, cfg_path.as_deref());
                    }
                }
                recv(tick) -> _ => { app.tick_rates(); }
            }
        }
        Ok(())
    })();

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    worker.shutdown();
    log_writer.shutdown();
    result
}

fn handle_serial(
    app: &mut AppState,
    ev: SerialEvent,
    splitter: &mut LineSplitter,
    lw: &LogWriter,
) {
    match ev {
        SerialEvent::Connected => {
            app.connected = true;
            app.set_notice(NoticeLevel::Info, "connected");
        }
        SerialEvent::Disconnected(e) => {
            app.connected = false;
            app.set_notice(NoticeLevel::Warn, format!("disconnected: {}", e));
        }
        SerialEvent::RxBytes(b) => {
            app.rx_bytes = app.rx_bytes.saturating_add(b.len() as u64);
            for line in splitter.feed(&b) {
                let l = LogLine {
                    ts: chrono::Local::now(),
                    direction: Dir::Rx,
                    bytes: line,
                };
                let _ = lw.tx.send(l.clone());
                app.push_line(l);
            }
        }
    }
}

fn handle_key(
    app: &mut AppState,
    k: KeyEvent,
    worker: &SerialWorker,
    lw: &LogWriter,
    cfg_path: Option<&std::path::Path>,
) {
    let action = map_key(app, k);
    match action {
        Action::Quit => app.quit = true,
        Action::OpenModal(m) => app.modal = m,
        Action::OpenPortPicker => {
            app.port_list = uart_mon::serial::list_ports();
            if let Some(cur) = app.config.serial.port.as_deref() {
                app.port_cursor = app.port_list.iter().position(|p| p == cur).unwrap_or(0);
            } else {
                app.port_cursor = 0;
            }
            app.modal = Modal::PortPicker;
        }
        Action::PortCursorUp => {
            let n = app.port_list.len().max(1);
            if app.port_cursor == 0 { app.port_cursor = n - 1; }
            else { app.port_cursor -= 1; }
        }
        Action::PortCursorDown => {
            let n = app.port_list.len().max(1);
            app.port_cursor = (app.port_cursor + 1) % n;
        }
        Action::PortRefresh => {
            app.port_list = uart_mon::serial::list_ports();
            if app.port_cursor >= app.port_list.len() { app.port_cursor = 0; }
        }
        Action::PortApply => {
            if let Some(p) = app.port_list.get(app.port_cursor).cloned() {
                app.config.serial.port = Some(p);
                let _ = worker.tx_cmd.send(TxCommand::ChangeConfig(app.config.serial.clone()));
                if let Some(path) = cfg_path {
                    let _ = app.config.save(path);
                }
                app.set_notice(NoticeLevel::Info, format!("port -> {}", app.config.serial.port.as_deref().unwrap_or("")));
                app.modal = Modal::None;
            } else {
                app.set_notice(NoticeLevel::Warn, "no ports available");
            }
        }
        Action::OpenSearch => {
            app.modal = Modal::Search;
            if app.search.is_none() { app.search = Some(String::new()); }
        }
        Action::CloseModal => app.modal = Modal::None,
        Action::SearchChar(c) => {
            if let Some(s) = app.search.as_mut() { s.push(c); }
        }
        Action::SearchBackspace => {
            if let Some(s) = app.search.as_mut() { s.pop(); }
        }
        Action::SearchCommit => { app.modal = Modal::None; }
        Action::SearchCancel => { app.modal = Modal::None; app.search = None; }
        Action::ToggleHex => app.show_hex = !app.show_hex,
        Action::ToggleTs => app.show_ts = !app.show_ts,
        Action::ClearLog => app.clear_lines(),
        Action::Reconnect => {
            let _ = worker.tx_cmd.send(TxCommand::Reconnect);
        }
        Action::ToggleHexInput => {
            app.input_mode = match app.input_mode {
                InputMode::Ascii => InputMode::Hex,
                InputMode::Hex => InputMode::Ascii,
            };
            app.input.clear();
        }
        Action::InputChar(c) => app.input.push(c),
        Action::InputBackspace => {
            app.input.pop();
        }
        Action::SubmitInput => submit(app, worker, lw),
        Action::SendMacro(slot) => send_macro(app, worker, lw, slot),
        Action::HistoryUp => {
            if !app.history.is_empty() {
                let idx = match app.history_cursor {
                    Some(i) if i > 0 => i - 1,
                    Some(i) => i,
                    None => app.history.len() - 1,
                };
                app.history_cursor = Some(idx);
                app.input = app.history[idx].clone();
            }
        }
        Action::HistoryDown => {
            if let Some(i) = app.history_cursor {
                if i + 1 < app.history.len() {
                    app.history_cursor = Some(i + 1);
                    app.input = app.history[i + 1].clone();
                } else {
                    app.history_cursor = None;
                    app.input.clear();
                }
            }
        }
        Action::ScrollUp => app.scroll_up(10),
        Action::ScrollDown => app.scroll_down(10),
        Action::ScrollBottom => app.scroll_bottom(),
        Action::SettingsCursorUp => {
            if app.settings_cursor == 0 { app.settings_cursor = SETTINGS_FIELDS - 1; }
            else { app.settings_cursor -= 1; }
        }
        Action::SettingsCursorDown => {
            app.settings_cursor = (app.settings_cursor + 1) % SETTINGS_FIELDS;
        }
        Action::SettingsValuePrev => cycle_setting(app, -1),
        Action::SettingsValueNext => cycle_setting(app, 1),
        Action::SettingsApply => {
            let _ = worker.tx_cmd.send(TxCommand::ChangeConfig(app.config.serial.clone()));
            if let Some(p) = cfg_path {
                if let Err(e) = app.config.save(p) {
                    app.set_notice(NoticeLevel::Error, format!("save: {}", e));
                } else {
                    app.set_notice(NoticeLevel::Info, "settings applied & saved");
                }
            } else {
                app.set_notice(NoticeLevel::Info, "settings applied (not saved)");
            }
            app.modal = Modal::None;
        }
        Action::MacroCursorUp => {
            let n = app.config.macros.len().max(1);
            if app.macro_cursor == 0 { app.macro_cursor = n - 1; }
            else { app.macro_cursor -= 1; }
        }
        Action::MacroCursorDown => {
            let n = app.config.macros.len().max(1);
            app.macro_cursor = (app.macro_cursor + 1) % n;
        }
        Action::MacroToggleHex => {
            if let Some(m) = app.config.macros.get_mut(app.macro_cursor) {
                m.hex = !m.hex;
            }
        }
        Action::MacroBeginEditName => {
            ensure_macro_slot(app);
            app.macro_edit_field = Some(MacroField::Name);
            app.macro_edit_buf = app.config.macros[app.macro_cursor].name.clone();
        }
        Action::MacroBeginEditPayload => {
            ensure_macro_slot(app);
            app.macro_edit_field = Some(MacroField::Payload);
            app.macro_edit_buf = app.config.macros[app.macro_cursor].payload.clone();
        }
        Action::MacroEditChar(c) => app.macro_edit_buf.push(c),
        Action::MacroEditBackspace => { app.macro_edit_buf.pop(); }
        Action::MacroEditCommit => {
            if let (Some(field), Some(m)) = (app.macro_edit_field, app.config.macros.get_mut(app.macro_cursor)) {
                let buf = std::mem::take(&mut app.macro_edit_buf);
                match field {
                    MacroField::Name => m.name = buf,
                    MacroField::Payload => m.payload = buf,
                    MacroField::HexToggle => {}
                }
            }
            app.macro_edit_field = None;
        }
        Action::MacroEditCancel => {
            app.macro_edit_field = None;
            app.macro_edit_buf.clear();
        }
        Action::MacroSave => {
            if let Some(p) = cfg_path {
                if let Err(e) = app.config.save(p) {
                    app.set_notice(NoticeLevel::Error, format!("save: {}", e));
                } else {
                    app.set_notice(NoticeLevel::Info, "macros saved");
                }
            } else {
                app.set_notice(NoticeLevel::Warn, "no config path; macros kept in memory only");
            }
        }
        Action::None => {}
    }
}

fn cycle_setting(app: &mut AppState, dir: i32) {
    let c = &mut app.config.serial;
    match app.settings_cursor {
        0 => {
            // baud
            let idx = BAUD_PRESETS.iter().position(|&b| b == c.baud).unwrap_or(0);
            let new = wrap_cycle(idx, BAUD_PRESETS.len(), dir);
            c.baud = BAUD_PRESETS[new];
        }
        1 => {
            // data bits
            let bits = [5u8, 6, 7, 8];
            let idx = bits.iter().position(|&b| b == c.data_bits).unwrap_or(3);
            c.data_bits = bits[wrap_cycle(idx, bits.len(), dir)];
        }
        2 => {
            // parity
            let vals = [Parity::None, Parity::Odd, Parity::Even];
            let idx = vals.iter().position(|v| v == &c.parity).unwrap_or(0);
            c.parity = vals[wrap_cycle(idx, vals.len(), dir)].clone();
        }
        3 => {
            // stop bits
            let vals = [1u8, 2];
            let idx = vals.iter().position(|&v| v == c.stop_bits).unwrap_or(0);
            c.stop_bits = vals[wrap_cycle(idx, vals.len(), dir)];
        }
        4 => {
            // flow
            let vals = [FlowControl::None, FlowControl::Software, FlowControl::Hardware];
            let idx = vals.iter().position(|v| v == &c.flow).unwrap_or(0);
            c.flow = vals[wrap_cycle(idx, vals.len(), dir)].clone();
        }
        5 => {
            // line ending
            let vals = [LineEnding::None, LineEnding::Cr, LineEnding::Lf, LineEnding::Crlf];
            let idx = vals.iter().position(|v| v == &c.line_ending).unwrap_or(2);
            c.line_ending = vals[wrap_cycle(idx, vals.len(), dir)].clone();
        }
        _ => {}
    }
}

fn wrap_cycle(idx: usize, len: usize, dir: i32) -> usize {
    if len == 0 { return 0; }
    if dir >= 0 { (idx + 1) % len } else { (idx + len - 1) % len }
}

fn ensure_macro_slot(app: &mut AppState) {
    if app.config.macros.is_empty() {
        app.config.macros.push(Macro { slot: 1, name: String::new(), payload: String::new(), hex: false });
    }
    if app.macro_cursor >= app.config.macros.len() {
        app.macro_cursor = app.config.macros.len() - 1;
    }
}

fn submit(app: &mut AppState, worker: &SerialWorker, lw: &LogWriter) {
    let text = std::mem::take(&mut app.input);
    if text.is_empty() {
        return;
    }
    let bytes = match app.input_mode {
        InputMode::Ascii => {
            let mut v = text.clone().into_bytes();
            v.extend_from_slice(app.config.serial.line_ending.bytes());
            v
        }
        InputMode::Hex => match parse_hex_tx(&text) {
            Ok(v) => v,
            Err(e) => {
                app.set_notice(NoticeLevel::Error, format!("hex: {}", e));
                app.input = text;
                return;
            }
        },
    };
    app.tx_bytes = app.tx_bytes.saturating_add(bytes.len() as u64);
    let line = LogLine {
        ts: chrono::Local::now(),
        direction: Dir::Tx,
        bytes: bytes.clone(),
    };
    let _ = lw.tx.send(line.clone());
    app.push_line(line);
    app.history.push_back(text);
    if app.history.len() > 50 {
        app.history.pop_front();
    }
    app.history_cursor = None;
    let _ = worker.tx_cmd.send(TxCommand::Send(bytes));
}

fn send_macro(app: &mut AppState, worker: &SerialWorker, lw: &LogWriter, slot: u8) {
    let Some(m) = app.macro_by_slot(slot).cloned() else {
        app.set_notice(NoticeLevel::Warn, format!("F{}: empty", slot));
        return;
    };
    let bytes = if m.hex {
        match parse_hex_tx(&m.payload) {
            Ok(v) => v,
            Err(e) => {
                app.set_notice(NoticeLevel::Error, format!("macro hex: {}", e));
                return;
            }
        }
    } else {
        let mut v = m.payload.clone().into_bytes();
        v.extend_from_slice(app.config.serial.line_ending.bytes());
        v
    };
    app.tx_bytes = app.tx_bytes.saturating_add(bytes.len() as u64);
    let line = LogLine {
        ts: chrono::Local::now(),
        direction: Dir::Tx,
        bytes: bytes.clone(),
    };
    let _ = lw.tx.send(line.clone());
    app.push_line(line);
    let _ = worker.tx_cmd.send(TxCommand::Send(bytes));
}
