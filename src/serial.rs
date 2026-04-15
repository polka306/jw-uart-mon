use std::io::{Read, Write};
use std::sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}};
use std::thread::{self, JoinHandle};
use std::time::Duration;
use crossbeam_channel::{Sender, Receiver, unbounded};
use crate::config::{SerialConfig, Parity, FlowControl};

pub struct LineSplitter {
    buf: Vec<u8>,
}
impl LineSplitter {
    pub fn new() -> Self { Self { buf: Vec::new() } }
    pub fn feed(&mut self, chunk: &[u8]) -> Vec<Vec<u8>> {
        self.buf.extend_from_slice(chunk);
        let mut out = Vec::new();
        let mut start = 0;
        let mut i = 0;
        while i < self.buf.len() {
            if self.buf[i] == b'\n' {
                let mut end = i;
                if end > start && self.buf[end - 1] == b'\r' { end -= 1; }
                out.push(self.buf[start..end].to_vec());
                start = i + 1;
            }
            i += 1;
        }
        if start > 0 { self.buf.drain(..start); }
        out
    }
    pub fn pending(&self) -> &[u8] { &self.buf }
}

pub fn parse_hex_tx(input: &str) -> Result<Vec<u8>, String> {
    let cleaned: String = input.chars().filter(|c| !c.is_whitespace()).collect();
    if cleaned.is_empty() { return Ok(Vec::new()); }
    if cleaned.len() % 2 != 0 { return Err("odd number of hex digits".into()); }
    let mut out = Vec::with_capacity(cleaned.len() / 2);
    for i in (0..cleaned.len()).step_by(2) {
        let byte = u8::from_str_radix(&cleaned[i..i + 2], 16)
            .map_err(|_| format!("invalid hex: {}", &cleaned[i..i + 2]))?;
        out.push(byte);
    }
    Ok(out)
}

#[derive(Debug, Clone)]
pub enum SerialEvent {
    Connected,
    Disconnected(String),
    RxBytes(Vec<u8>),
}
#[derive(Debug, Clone)]
pub enum TxCommand {
    Send(Vec<u8>),
    Reconnect,
    ChangeConfig(SerialConfig),
    Shutdown,
}

pub struct SerialWorker {
    pub tx_cmd: Sender<TxCommand>,
    stop: Arc<AtomicBool>,
    handles: Vec<JoinHandle<()>>,
}

fn build_settings(cfg: &SerialConfig) -> serialport::SerialPortBuilder {
    let parity = match cfg.parity {
        Parity::None => serialport::Parity::None,
        Parity::Odd => serialport::Parity::Odd,
        Parity::Even => serialport::Parity::Even,
    };
    let flow = match cfg.flow {
        FlowControl::None => serialport::FlowControl::None,
        FlowControl::Software => serialport::FlowControl::Software,
        FlowControl::Hardware => serialport::FlowControl::Hardware,
    };
    let data = match cfg.data_bits {
        5 => serialport::DataBits::Five,
        6 => serialport::DataBits::Six,
        7 => serialport::DataBits::Seven,
        _ => serialport::DataBits::Eight,
    };
    let stop = match cfg.stop_bits {
        2 => serialport::StopBits::Two,
        _ => serialport::StopBits::One,
    };
    serialport::new(cfg.port.clone().unwrap_or_default(), cfg.baud)
        .data_bits(data)
        .parity(parity)
        .stop_bits(stop)
        .flow_control(flow)
        .timeout(Duration::from_millis(200))
}

impl SerialWorker {
    pub fn spawn(initial: SerialConfig, evt_tx: Sender<SerialEvent>) -> Self {
        let (tx_cmd, cmd_rx) = unbounded::<TxCommand>();
        let stop = Arc::new(AtomicBool::new(false));
        let port: Arc<Mutex<Option<Box<dyn serialport::SerialPort>>>> = Arc::new(Mutex::new(None));
        let cfg: Arc<Mutex<SerialConfig>> = Arc::new(Mutex::new(initial));

        let rx_handle = {
            let stop = stop.clone();
            let evt_tx = evt_tx.clone();
            let port = port.clone();
            let cfg = cfg.clone();
            thread::spawn(move || rx_loop(stop, evt_tx, port, cfg))
        };
        let tx_handle = {
            let stop = stop.clone();
            let port = port.clone();
            let cfg = cfg.clone();
            let evt_tx = evt_tx.clone();
            thread::spawn(move || tx_loop(stop, cmd_rx, port, cfg, evt_tx))
        };

        Self { tx_cmd, stop, handles: vec![rx_handle, tx_handle] }
    }
    pub fn shutdown(mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = self.tx_cmd.send(TxCommand::Shutdown);
        for h in self.handles.drain(..) { let _ = h.join(); }
    }
}

fn try_open(cfg: &SerialConfig) -> Option<Box<dyn serialport::SerialPort>> {
    if cfg.port.as_ref().map_or(true, |s| s.is_empty()) { return None; }
    build_settings(cfg).open().ok()
}

fn rx_loop(
    stop: Arc<AtomicBool>,
    evt_tx: Sender<SerialEvent>,
    port: Arc<Mutex<Option<Box<dyn serialport::SerialPort>>>>,
    cfg: Arc<Mutex<SerialConfig>>,
) {
    let mut buf = [0u8; 4096];
    let mut was_connected = false;
    while !stop.load(Ordering::SeqCst) {
        let need_open = { port.lock().unwrap().is_none() };
        if need_open {
            if was_connected {
                was_connected = false;
                let _ = evt_tx.send(SerialEvent::Disconnected("closed".into()));
            }
            let cur = cfg.lock().unwrap().clone();
            if let Some(p) = try_open(&cur) {
                *port.lock().unwrap() = Some(p);
                was_connected = true;
                let _ = evt_tx.send(SerialEvent::Connected);
            } else {
                thread::sleep(Duration::from_millis(1000));
                continue;
            }
        }
        let read_res = {
            let mut guard = port.lock().unwrap();
            if let Some(p) = guard.as_mut() {
                Some(p.read(&mut buf))
            } else {
                None
            }
        };
        match read_res {
            Some(Ok(0)) => { thread::sleep(Duration::from_millis(10)); }
            Some(Ok(n)) => { let _ = evt_tx.send(SerialEvent::RxBytes(buf[..n].to_vec())); }
            Some(Err(e)) if e.kind() == std::io::ErrorKind::TimedOut => { /* idle */ }
            Some(Err(e)) => {
                *port.lock().unwrap() = None;
                let _ = evt_tx.send(SerialEvent::Disconnected(e.to_string()));
                thread::sleep(Duration::from_millis(1000));
            }
            None => { thread::sleep(Duration::from_millis(50)); }
        }
    }
}

fn tx_loop(
    stop: Arc<AtomicBool>,
    cmd_rx: Receiver<TxCommand>,
    port: Arc<Mutex<Option<Box<dyn serialport::SerialPort>>>>,
    cfg: Arc<Mutex<SerialConfig>>,
    evt_tx: Sender<SerialEvent>,
) {
    while !stop.load(Ordering::SeqCst) {
        match cmd_rx.recv_timeout(Duration::from_millis(200)) {
            Ok(TxCommand::Send(bytes)) => {
                let mut guard = port.lock().unwrap();
                if let Some(p) = guard.as_mut() {
                    if let Err(e) = p.write_all(&bytes) {
                        *guard = None;
                        let _ = evt_tx.send(SerialEvent::Disconnected(e.to_string()));
                    }
                }
            }
            Ok(TxCommand::Reconnect) => { *port.lock().unwrap() = None; }
            Ok(TxCommand::ChangeConfig(c)) => {
                *cfg.lock().unwrap() = c;
                *port.lock().unwrap() = None;
            }
            Ok(TxCommand::Shutdown) => break,
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => {}
            Err(_) => break,
        }
    }
}

pub fn list_ports() -> Vec<String> {
    serialport::available_ports()
        .ok()
        .unwrap_or_default()
        .into_iter()
        .map(|p| p.port_name)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn splits_lf() {
        let mut s = LineSplitter::new();
        let lines = s.feed(b"hello\nworld\n");
        assert_eq!(lines, vec![b"hello".to_vec(), b"world".to_vec()]);
    }
    #[test]
    fn splits_crlf() {
        let mut s = LineSplitter::new();
        let lines = s.feed(b"a\r\nb\r\n");
        assert_eq!(lines, vec![b"a".to_vec(), b"b".to_vec()]);
    }
    #[test]
    fn buffers_partial() {
        let mut s = LineSplitter::new();
        assert!(s.feed(b"abc").is_empty());
        let lines = s.feed(b"def\nghi\n");
        assert_eq!(lines, vec![b"abcdef".to_vec(), b"ghi".to_vec()]);
    }
    #[test]
    fn hex_parse_spaced() {
        assert_eq!(parse_hex_tx("DE AD BE EF").unwrap(), vec![0xDE, 0xAD, 0xBE, 0xEF]);
    }
    #[test]
    fn hex_parse_unspaced() {
        assert_eq!(parse_hex_tx("deadbeef").unwrap(), vec![0xDE, 0xAD, 0xBE, 0xEF]);
    }
    #[test]
    fn hex_parse_errors() {
        assert!(parse_hex_tx("de a").is_err());
        assert!(parse_hex_tx("xy").is_err());
    }
}
