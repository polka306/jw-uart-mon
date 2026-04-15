# uart-mon — Design Spec

- Date: 2026-04-15
- Status: Approved (brainstorm)
- Owner: jwkang

## 1. Purpose

A cross-platform (Windows + Linux primary; macOS best-effort) terminal-UI UART monitoring tool written in Rust. Single binary, focused on day-to-day embedded debugging: connect to a serial port, watch RX, send TX (with macros), and persist logs.

## 2. Scope

### In scope (this spec)
- Port enumeration and selection
- Configurable serial parameters: baudrate, data bits, parity, stop bits, flow control
- RX display with ASCII / HEX toggle, optional timestamps
- TX line input with line-ending selection (None / CR / LF / CRLF)
- HEX TX input mode (e.g. `DE AD BE EF`)
- Macro slots (F1–F12), editable in-app, persisted
- Log file output with timestamps
- Auto-reconnect on disconnect (1s backoff)
- Search / filter inside RX buffer
- Config persistence in OS-standard location; log directory overridable via CLI
- Cross-platform: Windows + Linux first-class; macOS expected to build/run but not gated in CI

### Non-goals (deferred to follow-up specs)
- Multi-port simultaneous monitoring (tabs)
- File send (text/binary)
- Throughput / error statistics dashboard
- Named connection profiles beyond a single default config
- Hardware mocks beyond socat-based virtual ports

## 3. Target platforms

- Primary: Linux (x86_64), Windows 10/11 (x86_64)
- Best-effort: macOS (built locally, not in CI matrix)
- Permission notes (Linux): document `dialout` / `tty` group requirement and a sample udev rule in README.

## 4. Architecture

### 4.1 Concurrency model

Synchronous threads with `crossbeam-channel` message passing. No async runtime.

- **Main thread** — ratatui render loop, owns `AppState`
- **Input thread** — `crossterm::event::poll`, forwards `KeyEvent` into the event bus
- **SerialWorker** — internally spawns:
  - **RX thread** — blocking reads on the serial port, emits `SerialEvent::RxBytes`, `Disconnect`, `Connected`
  - **TX thread** — receives `TxCommand`, writes to the serial port
- **LogWriter thread** — receives `LogLine`, appends to the active log file

### 4.2 Module layout

```
uart-mon/
├── Cargo.toml
└── src/
    ├── main.rs          # CLI parsing (clap), runtime bootstrap
    ├── config.rs        # TOML load/save, macros, paths via `directories`
    ├── serial.rs        # SerialWorker, channel definitions, line splitter
    ├── app.rs           # AppState: ring buffer, input, modal state, macros
    ├── ui/
    │   ├── mod.rs       # Main layout (status bar / RX / input)
    │   ├── modal.rs     # Port picker, macro editor, settings
    │   └── theme.rs
    ├── input.rs         # crossterm key events → AppEvent mapping
    ├── log_writer.rs    # File append thread
    └── error.rs         # thiserror domain errors
```

Module boundaries:
- `serial` knows nothing about ratatui/crossterm; communicates only via channels.
- `app` is pure state; UI render functions read it, mutations go through `AppEvent`.
- `ui` renders from `&AppState`; never mutates.

### 4.3 Dependencies

`ratatui`, `crossterm`, `serialport`, `crossbeam-channel`, `clap` (derive),
`serde` + `toml`, `directories`, `chrono`, `thiserror`, `anyhow` (in `main` only).

## 5. Data flow

```
KeyEvent ─┐
          ├─► EventBus (crossbeam) ─► Main loop ─► AppState.apply()
SerialEvent ┘                                    │
                                                 ├─► render(AppState)
                                                 └─► log_tx.send(LogLine)
TxCommand  ◄── Main loop ◄── AppState (Enter / macro key)
   │
   ▼
SerialWorker (RX + TX threads) ◄── /dev/ttyUSBx | COMx
```

Channels:
- `tx_cmd: Sender<TxCommand>` — variants: `Send(Vec<u8>)`, `Reconnect`, `ChangeConfig(SerialConfig)`, `Disconnect`
- `evt: Sender<AppEvent>` — variants: `Serial(SerialEvent)`, `Key(KeyEvent)`, `Tick`, `Notice(Level, String)`
- `log_tx: Sender<LogLine>`

Main loop runs at ~60fps via `evt.recv_timeout(16ms)`.

### 5.1 Auto-reconnect

On RX read error of kind `Io(BrokenPipe | NotFound | ...)`, the RX thread emits `Disconnect`, sleeps 1s, and retries `open()`. On success it emits `Connected` and resumes. `TimedOut` is treated as normal idle.

### 5.2 RX line handling

RX bytes are split on `\n`. Incomplete tail bytes are buffered and prepended to the next chunk. Each completed line is timestamped and pushed to a ring buffer (default 10000 lines, configurable).

## 6. UX

### 6.1 Layout

```
┌ Status bar (1 line) ───────────────────────────────────────────────┐
│ [●Connected] /dev/ttyUSB0 115200-8N1 None | RX:1.2KB/s TX:0.1KB/s   │
│ HEX:off TS:on LE:LF                                                 │
├ RX log (fills remaining height) ────────────────────────────────────┤
│ [12:34:56.789] hello world                                          │
│ ...                                                                 │
├ TX input (1 line) ──────────────────────────────────────────────────┤
│ > _                                                                 │
└─────────────────────────────────────────────────────────────────────┘
```

Modals (centered overlay): port picker, settings, macro editor.

### 6.2 Key bindings

| Key | Action |
|---|---|
| `Ctrl+Q` | Quit |
| `Ctrl+P` | Port picker modal |
| `Ctrl+R` | Manual reconnect |
| `Ctrl+L` | Clear RX log |
| `Ctrl+H` | Toggle HEX/ASCII display |
| `Ctrl+T` | Toggle timestamps |
| `Ctrl+S` | Settings modal (baud, data, parity, stop, flow, line ending) |
| `Ctrl+M` | Macro editor modal |
| `Ctrl+X` | Toggle HEX TX input mode |
| `Ctrl+F` | Search/filter mode |
| `F1`–`F12` | Send macro slot 1–12 |
| `Esc` | Close modal / leave mode |
| `PageUp` / `PageDown` | Scroll RX |
| `End` | Jump to bottom (resume autoscroll) |
| `↑` / `↓` (in input) | TX history (50 entries, in-memory) |

### 6.3 HEX TX input

Input like `DE AD BE EF` or `deadbeef` is parsed into raw bytes and sent without a line ending. Invalid input shows an inline error and is not sent.

## 7. Configuration & files

- Resolved via `directories` crate.
- Config file: `~/.config/uart-mon/config.toml` (Linux) / `%APPDATA%\uart-mon\config.toml` (Windows).
- Default log directory: `~/.local/share/uart-mon/logs/` / `%LOCALAPPDATA%\uart-mon\logs\`.
- CLI: `uart-mon [--port <name>] [--baud <n>] [--log-dir <path>] [--config <path>] [--no-log]`.

`config.toml` shape (illustrative):
```toml
[serial]
port = "/dev/ttyUSB0"
baud = 115200
data_bits = 8
parity = "none"
stop_bits = 1
flow = "none"
line_ending = "lf"

[ui]
hex = false
timestamps = true
ring_capacity = 10000

[[macros]]
slot = 1
name = "version"
payload = "AT+VER\r\n"
hex = false
```

## 8. Error handling

- `error.rs` defines `SerialError`, `ConfigError`, `LogError` via `thiserror`.
- `main` returns `anyhow::Result<()>`; only startup failures terminate.
- Runtime errors never panic. They are converted to `AppEvent::Notice(Level, String)` and shown briefly on the status bar (and written to the log file).
- Config load failure → fall back to defaults + `Notice(Warn, ...)`.
- Log write failure → emit `Notice(Error, ...)` once, then disable logging for the session (no spam).
- Serial read error classification:
  - `TimedOut` → ignored
  - `BrokenPipe | NotFound | NoDevice` → `Disconnect` + backoff reconnect
  - Other → `Notice(Error, ...)` + reconnect attempt

## 9. Testing strategy

### 9.1 Unit tests (`#[cfg(test)]`)
- `app`: ring buffer append/clear/scroll, HEX render, macro slot management, autoscroll behavior
- `config`: TOML round-trip, defaults for missing fields
- `serial::line_splitter`: chunk splitting/merging across CR / LF / CRLF
- HEX TX parser: spaced, unspaced, invalid input

### 9.2 Integration tests (`tests/`)
- Linux only: `socat -d -d pty,raw,echo=0 pty,raw,echo=0` creates a virtual serial pair. Tests open one end with SerialWorker and inject bytes from the other end, verifying channel delivery, line splitting, and reconnect behavior (kill + restart socat).
- Windows: documented manual test checklist using a USB-serial adapter (no `com0com` dependency).

### 9.3 TUI snapshot tests
- Use ratatui `TestBackend` to render key screens (connected, disconnected, HEX mode, each modal) and compare buffer snapshots.

### 9.4 CI
- GitHub Actions matrix: `ubuntu-latest`, `windows-latest`.
- Both: `cargo build`, `cargo test` (unit + snapshot).
- Ubuntu only: socat-based integration tests.

## 10. Open questions

None at spec time. All decisions captured above.
