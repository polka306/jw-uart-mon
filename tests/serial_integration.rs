#![cfg(target_os = "linux")]
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};
use crossbeam_channel::unbounded;
use uart_mon::config::SerialConfig;
use uart_mon::serial::{SerialEvent, SerialWorker};

struct Socat {
    child: Child,
    a: String,
    b: String,
}
impl Drop for Socat {
    fn drop(&mut self) {
        let _ = self.child.kill();
    }
}

fn have_socat() -> bool {
    Command::new("sh")
        .arg("-c")
        .arg("command -v socat")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn start_socat() -> Option<Socat> {
    if !have_socat() {
        return None;
    }
    let mut child = Command::new("socat")
        .args(["-d", "-d", "pty,raw,echo=0", "pty,raw,echo=0"])
        .stderr(Stdio::piped())
        .spawn()
        .ok()?;
    let stderr = child.stderr.take()?;
    let mut reader = BufReader::new(stderr);
    let mut a = String::new();
    let mut b = String::new();
    for _ in 0..6 {
        let mut line = String::new();
        if reader.read_line(&mut line).ok()? == 0 {
            break;
        }
        if let Some(idx) = line.find("/dev/pts/") {
            let path = line[idx..].trim().to_string();
            if a.is_empty() {
                a = path;
            } else if b.is_empty() {
                b = path;
                break;
            }
        }
    }
    thread::spawn(move || {
        for _ in reader.lines() {}
    });
    if a.is_empty() || b.is_empty() {
        return None;
    }
    thread::sleep(Duration::from_millis(200));
    Some(Socat { child, a, b })
}

#[test]
fn rx_receives_bytes_from_peer() {
    let Some(sc) = start_socat() else {
        eprintln!("socat unavailable, skipping");
        return;
    };
    let (evt_tx, evt_rx) = unbounded::<SerialEvent>();
    let mut cfg = SerialConfig::default();
    cfg.port = Some(sc.a.clone());
    let worker = SerialWorker::spawn(cfg, evt_tx);

    let deadline = Instant::now() + Duration::from_secs(5);
    let mut connected = false;
    while Instant::now() < deadline {
        if let Ok(SerialEvent::Connected) =
            evt_rx.recv_timeout(Duration::from_millis(200))
        {
            connected = true;
            break;
        }
    }
    assert!(connected, "did not connect to {}", sc.a);

    let mut peer = std::fs::OpenOptions::new()
        .write(true)
        .open(&sc.b)
        .expect("open peer pts");
    peer.write_all(b"hello\n").unwrap();
    peer.flush().unwrap();

    let mut got = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline && !got.windows(6).any(|w| w == b"hello\n") {
        if let Ok(SerialEvent::RxBytes(b)) =
            evt_rx.recv_timeout(Duration::from_millis(200))
        {
            got.extend(b);
        }
    }
    assert!(
        got.windows(6).any(|w| w == b"hello\n"),
        "got {:?}",
        got
    );

    worker.shutdown();
}
