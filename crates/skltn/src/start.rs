use std::env;
use std::fs;
use std::net::TcpStream;
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use crate::pid;

pub fn run(port: u16, no_obs: bool) -> Result<(), String> {
    let cwd = env::current_dir().map_err(|e| format!("Failed to get current directory: {e}"))?;

    if !no_obs {
        start_proxy(port)?;
    }

    register_mcp(&cwd)?;

    // exec claude — replaces this process
    let mut cmd = Command::new("claude");
    if !no_obs {
        cmd.env("ANTHROPIC_BASE_URL", format!("http://localhost:{port}"));
    }

    eprintln!("Launching Claude Code...");
    let err = cmd.exec();
    Err(format!("Failed to launch claude: {err}"))
}

fn start_proxy(port: u16) -> Result<(), String> {
    // Check if proxy already running
    if let Some(info) = pid::read() {
        if pid::is_process_alive(info.pid) {
            eprintln!("Proxy already running on port {} (PID {})", info.port, info.pid);
            return Ok(());
        }
        // Stale PID file
        pid::remove();
    }

    let obs_bin = find_sibling_binary("skltn-obs")?;

    let data_dir = dirs::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join(".skltn");
    fs::create_dir_all(&data_dir)
        .map_err(|e| format!("Failed to create ~/.skltn: {e}"))?;

    let log_path = data_dir.join("obs.log");
    let log_file = fs::File::create(&log_path)
        .map_err(|e| format!("Failed to create log file: {e}"))?;
    let log_stderr = log_file
        .try_clone()
        .map_err(|e| format!("Failed to clone log file handle: {e}"))?;

    eprintln!("Starting skltn-obs proxy on port {port}...");

    let child = Command::new(&obs_bin)
        .args(["--port", &port.to_string()])
        .stdout(Stdio::from(log_file))
        .stderr(Stdio::from(log_stderr))
        .stdin(Stdio::null())
        .spawn()
        .map_err(|e| format!("Failed to start skltn-obs: {e}"))?;

    let pid_info = pid::PidInfo {
        pid: child.id(),
        port,
    };
    pid::write(&pid_info)?;

    // Wait for proxy to accept connections (max 3s)
    let start = Instant::now();
    let addr = format!("127.0.0.1:{port}");
    loop {
        if TcpStream::connect(&addr).is_ok() {
            eprintln!("Proxy ready on port {port} (PID {})", pid_info.pid);
            break;
        }
        if start.elapsed() > Duration::from_secs(3) {
            eprintln!(
                "Warning: proxy may not be ready yet (check ~/.skltn/obs.log)"
            );
            break;
        }
        thread::sleep(Duration::from_millis(100));
    }

    Ok(())
}

fn register_mcp(cwd: &std::path::Path) -> Result<(), String> {
    let mcp_bin = find_sibling_binary("skltn-mcp")?;
    let cwd_str = cwd.to_string_lossy();

    eprintln!("Registering MCP server for {cwd_str}...");

    let output = Command::new("claude")
        .args([
            "mcp",
            "add",
            "skltn-mcp",
            "--",
            mcp_bin.to_string_lossy().as_ref(),
            &cwd_str,
        ])
        .output()
        .map_err(|e| format!("Failed to run `claude mcp add`: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // "already exists" is fine — idempotent
        if !stderr.contains("already exists") {
            return Err(format!("claude mcp add failed: {stderr}"));
        }
    }

    Ok(())
}

fn find_sibling_binary(name: &str) -> Result<std::path::PathBuf, String> {
    // Look next to the current executable first
    if let Ok(exe) = env::current_exe() {
        if let Some(dir) = exe.parent() {
            let sibling = dir.join(name);
            if sibling.exists() {
                return Ok(sibling);
            }
        }
    }

    // Fall back to PATH
    which::which(name).map_err(|_| {
        format!(
            "Could not find `{name}` binary. Ensure it is installed alongside `skltn` or is in your PATH."
        )
    })
}
