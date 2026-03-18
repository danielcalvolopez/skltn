use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct PidInfo {
    pub pid: u32,
    pub port: u16,
}

pub fn pid_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".skltn")
        .join("obs.pid")
}

pub fn read() -> Option<PidInfo> {
    let path = pid_path();
    let data = fs::read_to_string(&path).ok()?;
    serde_json::from_str(&data).ok()
}

pub fn write(info: &PidInfo) -> Result<(), String> {
    let path = pid_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Failed to create ~/.skltn: {e}"))?;
    }
    let json = serde_json::to_string(info).map_err(|e| format!("Failed to serialize PID info: {e}"))?;
    fs::write(&path, json).map_err(|e| format!("Failed to write PID file: {e}"))?;
    Ok(())
}

pub fn remove() {
    let _ = fs::remove_file(pid_path());
}

pub fn is_process_alive(pid: u32) -> bool {
    use std::process::Command;
    Command::new("kill")
        .args(["-0", &pid.to_string()])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}
