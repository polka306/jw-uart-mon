use std::io::Write;
use std::process::{Command, Stdio};

/// Read text from the system clipboard by shelling out to platform tools.
/// Linux: tries wl-paste, then xclip, then xsel.
/// Windows: powershell Get-Clipboard.
/// macOS: pbpaste.
pub fn paste() -> Result<String, String> {
    #[cfg(target_os = "linux")]
    {
        if let Ok(s) = run("wl-paste", &["--no-newline"], None) { return Ok(s); }
        if let Ok(s) = run("xclip", &["-selection", "clipboard", "-o"], None) { return Ok(s); }
        if let Ok(s) = run("xsel", &["--clipboard", "--output"], None) { return Ok(s); }
        Err("no clipboard tool found (install wl-clipboard, xclip, or xsel)".into())
    }
    #[cfg(target_os = "windows")]
    {
        run("powershell", &["-NoProfile", "-Command", "Get-Clipboard"], None)
            .map_err(|e| format!("powershell: {}", e))
    }
    #[cfg(target_os = "macos")]
    {
        run("pbpaste", &[], None).map_err(|e| format!("pbpaste: {}", e))
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
    {
        Err("unsupported platform".into())
    }
}

fn run(prog: &str, args: &[&str], stdin_bytes: Option<&[u8]>) -> Result<String, String> {
    let mut cmd = Command::new(prog);
    cmd.args(args).stdout(Stdio::piped()).stderr(Stdio::null());
    if stdin_bytes.is_some() { cmd.stdin(Stdio::piped()); } else { cmd.stdin(Stdio::null()); }
    let mut child = cmd.spawn().map_err(|e| format!("spawn {}: {}", prog, e))?;
    if let (Some(bytes), Some(mut stdin)) = (stdin_bytes, child.stdin.take()) {
        let _ = stdin.write_all(bytes);
    }
    let out = child.wait_with_output().map_err(|e| format!("wait {}: {}", prog, e))?;
    if !out.status.success() {
        return Err(format!("{} exited {}", prog, out.status));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}
