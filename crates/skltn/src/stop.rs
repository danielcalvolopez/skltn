use std::process::Command;

use crate::pid;

pub fn run() -> Result<(), String> {
    let info = pid::read().ok_or("No running proxy found (no PID file at ~/.skltn/obs.pid)")?;

    if !pid::is_process_alive(info.pid) {
        pid::remove();
        return Err(format!(
            "Proxy (PID {}) is no longer running. Cleaned up stale PID file.",
            info.pid
        ));
    }

    eprintln!("Stopping skltn-obs proxy (PID {}, port {})...", info.pid, info.port);

    let status = Command::new("kill")
        .arg(info.pid.to_string())
        .status()
        .map_err(|e| format!("Failed to send SIGTERM: {e}"))?;

    if !status.success() {
        return Err(format!("Failed to kill process {}", info.pid));
    }

    pid::remove();
    eprintln!("Proxy stopped.");
    Ok(())
}
