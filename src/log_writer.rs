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

const ROTATE_BYTES: u64 = 10 * 1024 * 1024;

fn open_file(dir: &PathBuf) -> std::io::Result<File> {
    std::fs::create_dir_all(dir)?;
    let name = format!("uart-mon-{}.log", Local::now().format("%Y%m%d-%H%M%S%.3f"));
    let path = dir.join(name);
    OpenOptions::new().create(true).append(true).open(&path)
}

fn run(dir: Option<PathBuf>, rx: Receiver<LogLine>) {
    let Some(d) = dir else { while rx.recv().is_ok() {} return; };
    let mut file: Option<File> = match open_file(&d) {
        Ok(f) => Some(f),
        Err(_) => None,
    };
    let mut written: u64 = 0;
    let mut disabled = file.is_none();

    while let Ok(line) = rx.recv() {
        if disabled || file.is_none() { continue; }
        let dir_tag = match line.direction {
            Direction::Rx => "RX",
            Direction::Tx => "TX",
            Direction::System => "SYS",
        };
        let text = String::from_utf8_lossy(&line.bytes);
        let ts = line.ts.format("%Y-%m-%d %H:%M:%S%.3f");
        let formatted = format!("[{}] {} {}\n", ts, dir_tag, text);
        let bytes = formatted.as_bytes();

        if let Some(f) = file.as_mut() {
            if f.write_all(bytes).is_err() {
                disabled = true;
                continue;
            }
            written += bytes.len() as u64;
        }

        if written >= ROTATE_BYTES {
            if let Some(mut f) = file.take() {
                let _ = f.flush();
            }
            match open_file(&d) {
                Ok(f) => { file = Some(f); written = 0; }
                Err(_) => { disabled = true; }
            }
        }
    }
}

impl Clone for LogWriter {
    fn clone(&self) -> Self {
        Self { tx: self.tx.clone(), handle: None }
    }
}
