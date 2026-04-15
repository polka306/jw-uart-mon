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
    app::{AppState, Direction as Dir, InputMode, LogLine, Modal, NoticeLevel},
    config::{default_config_path, default_log_dir, Config},
    input::{map_key, Action},
    log_writer::LogWriter,
    serial::{parse_hex_tx, LineSplitter, SerialEvent, SerialWorker, TxCommand},
    ui,
};

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
                        handle_key(&mut app, k, &worker, &log_writer);
                    }
                }
                recv(tick) -> _ => {}
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

fn handle_key(app: &mut AppState, k: KeyEvent, worker: &SerialWorker, lw: &LogWriter) {
    let action = map_key(app, k);
    match action {
        Action::Quit => app.quit = true,
        Action::OpenModal(m) => app.modal = m,
        Action::CloseModal => app.modal = Modal::None,
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
        Action::None => {}
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
        m.payload.clone().into_bytes()
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
