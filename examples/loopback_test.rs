// Loopback test: requires TX and RX pins shorted on the adapter.
// Opens the given port via SerialWorker, sends a payload, verifies it echoes back.

use std::time::{Duration, Instant};
use crossbeam_channel::unbounded;
use uart_mon::config::SerialConfig;
use uart_mon::serial::{SerialEvent, SerialWorker, TxCommand};

fn main() {
    let port = std::env::args().nth(1).unwrap_or_else(|| "/dev/ttyUSB0".into());
    let baud: u32 = std::env::args()
        .nth(2)
        .and_then(|s| s.parse().ok())
        .unwrap_or(115200);
    println!("testing port={} baud={}", port, baud);

    let (evt_tx, evt_rx) = unbounded::<SerialEvent>();
    let mut cfg = SerialConfig::default();
    cfg.port = Some(port.clone());
    cfg.baud = baud;
    let worker = SerialWorker::spawn(cfg, evt_tx);

    // Wait for Connected
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut connected = false;
    while Instant::now() < deadline {
        match evt_rx.recv_timeout(Duration::from_millis(200)) {
            Ok(SerialEvent::Connected) => { connected = true; break; }
            Ok(SerialEvent::Disconnected(e)) => {
                eprintln!("disconnected: {}", e);
            }
            _ => {}
        }
    }
    if !connected {
        eprintln!("FAIL: could not connect to {}", port);
        worker.shutdown();
        std::process::exit(2);
    }
    println!("connected");

    let payloads: Vec<&[u8]> = vec![
        b"hello loopback\n",
        b"second line\r\n",
        &[0xDE, 0xAD, 0xBE, 0xEF],
    ];

    for (i, p) in payloads.iter().enumerate() {
        worker.tx_cmd.send(TxCommand::Send(p.to_vec())).unwrap();
        let mut got = Vec::new();
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline && got.len() < p.len() {
            match evt_rx.recv_timeout(Duration::from_millis(200)) {
                Ok(SerialEvent::RxBytes(b)) => got.extend(b),
                Ok(SerialEvent::Disconnected(e)) => {
                    eprintln!("disconnected mid-test: {}", e);
                    break;
                }
                _ => {}
            }
        }
        if got.starts_with(p) {
            println!("  [{}] OK  sent {} bytes, echoed matches", i, p.len());
        } else {
            println!(
                "  [{}] FAIL  sent={:02X?} got={:02X?}",
                i, p, got
            );
            worker.shutdown();
            std::process::exit(1);
        }
    }

    println!("ALL PASS");
    worker.shutdown();
}
