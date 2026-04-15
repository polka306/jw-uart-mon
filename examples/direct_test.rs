// Direct serialport test without SerialWorker.
use std::io::{Read, Write};
use std::time::{Duration, Instant};

fn main() {
    let port = std::env::args().nth(1).unwrap_or_else(|| "/dev/ttyUSB0".into());
    let mut p = serialport::new(&port, 115200)
        .data_bits(serialport::DataBits::Eight)
        .parity(serialport::Parity::None)
        .stop_bits(serialport::StopBits::One)
        .flow_control(serialport::FlowControl::None)
        .timeout(Duration::from_millis(500))
        .open()
        .expect("open");
    println!("opened {}", port);
    std::thread::sleep(Duration::from_millis(200));
    p.clear(serialport::ClearBuffer::All).ok();

    p.write_all(b"PING\n").unwrap();
    p.flush().unwrap();
    println!("wrote PING");

    let deadline = Instant::now() + Duration::from_secs(2);
    let mut buf = [0u8; 256];
    let mut got = Vec::new();
    while Instant::now() < deadline && got.len() < 5 {
        match p.read(&mut buf) {
            Ok(0) => {}
            Ok(n) => got.extend_from_slice(&buf[..n]),
            Err(e) if e.kind() == std::io::ErrorKind::TimedOut => {}
            Err(e) => { println!("read err: {}", e); break; }
        }
    }
    println!("got: {:?}", got);
}
