use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::thread::{self, JoinHandle};
use crossbeam_channel::{Receiver, Sender, bounded};
use chrono::Local;
use crate::app::{LogLine, Direction};

pub struct LogWriter {
    pub tx: Sender<LogLine>,
    handle: Option<JoinHandle<()>>,
}

impl LogWriter {
    pub fn spawn(dir: Option<PathBuf>) -> Self {
        let (tx, rx): (Sender<LogLine>, Receiver<LogLine>) = bounded(1024);
        let handle = thread::spawn(move || run(dir, rx));
        Self { tx, handle: Some(handle) }
    }
    pub fn shutdown(mut self) {
        drop(self.tx);
        if let Some(h) = self.handle.take() { let _ = h.join(); }
    }
}

fn open_file(dir: &PathBuf) -> std::io::Result<File> {
    std::fs::create_dir_all(dir)?;
    let name = format!("uart-mon-{}.log", Local::now().format("%Y%m%d-%H%M%S"));
    let path = dir.join(name);
    OpenOptions::new().create(true).append(true).open(&path)
}

fn run(dir: Option<PathBuf>, rx: Receiver<LogLine>) {
    let mut file: Option<File> = None;
    let mut disabled = false;
    if let Some(d) = dir.as_ref() {
        match open_file(d) {
            Ok(f) => file = Some(f),
            Err(_) => disabled = true,
        }
    } else {
        disabled = true;
    }
    while let Ok(line) = rx.recv() {
        if disabled || file.is_none() { continue; }
        let dir_tag = match line.direction {
            Direction::Rx => "RX",
            Direction::Tx => "TX",
            Direction::System => "SYS",
        };
        let text = String::from_utf8_lossy(&line.bytes);
        let ts = line.ts.format("%Y-%m-%d %H:%M:%S%.3f");
        if let Some(f) = file.as_mut() {
            if writeln!(f, "[{}] {} {}", ts, dir_tag, text).is_err() {
                disabled = true;
            }
        }
    }
}

impl Clone for LogWriter {
    fn clone(&self) -> Self {
        Self { tx: self.tx.clone(), handle: None }
    }
}
