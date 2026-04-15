# uart-mon

Cross-platform TUI UART monitor written in Rust.

See `docs/superpowers/specs/` for design, `docs/superpowers/plans/` for the implementation plan.

## Build

```
cargo build --release
```

## Run

```
uart-mon --port /dev/ttyUSB0 --baud 115200
uart-mon --port COM3 --baud 9600
```

### CLI flags
- `--port <name>` — serial port (e.g. `/dev/ttyUSB0`, `COM3`)
- `--baud <n>` — baudrate
- `--log-dir <path>` — override log directory
- `--config <path>` — override config file
- `--no-log` — disable file logging

## Keybindings

| Key | Action |
|---|---|
| `Ctrl+Q` | Quit |
| `Ctrl+P` | Port picker |
| `Ctrl+R` | Reconnect |
| `Ctrl+L` | Clear log |
| `Ctrl+H` | Toggle HEX/ASCII |
| `Ctrl+T` | Toggle timestamps |
| `Ctrl+S` | Settings |
| `Ctrl+M` | Macro editor |
| `Ctrl+X` | Toggle HEX TX input |
| `Ctrl+F` | Search |
| `F1`–`F12` | Send macro slot 1–12 |
| `Esc` | Close modal |
| `PageUp` / `PageDown` / `End` | Scroll RX |

### Inside Settings (Ctrl+S)
- `↑` / `↓` — select field
- `←` / `→` — cycle value
- `Enter` — apply to running worker + save to config file
- `Esc` — cancel

### Inside Macro editor (Ctrl+M)
- `↑` / `↓` — select macro slot
- `n` — edit name
- `p` — edit payload
- `h` — toggle HEX mode for that macro
- `s` — save to config file
- `Esc` — close

### Inside Search (Ctrl+F)
- type — live-filter RX lines
- `Enter` — keep filter
- `Esc` — cancel & clear

## Config & log paths

- Linux: config `~/.config/uart-mon/config.toml`, logs `~/.local/share/uart-mon/logs/`
- Windows: config `%APPDATA%\uart-mon\config.toml`, logs `%LOCALAPPDATA%\uart-mon\logs\`

## Linux permissions

If you get permission denied on `/dev/ttyUSB*`, add your user to the `dialout` group (Debian/Ubuntu) or `uucp` (Arch):

```
sudo usermod -aG dialout $USER
```

Then log out and back in.

Example udev rule (`/etc/udev/rules.d/99-uart.rules`):

```
SUBSYSTEM=="tty", ATTRS{idVendor}=="10c4", MODE="0660", GROUP="dialout"
```
