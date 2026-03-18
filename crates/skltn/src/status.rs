use crate::pid;

pub fn run() -> Result<(), String> {
    match pid::read() {
        Some(info) => {
            if pid::is_process_alive(info.pid) {
                println!("skltn-obs proxy");
                println!("  Status:  running");
                println!("  PID:     {}", info.pid);
                println!("  Port:    {}", info.port);
                println!("  Dashboard: http://localhost:{}/dashboard", info.port);
            } else {
                pid::remove();
                println!("skltn-obs proxy");
                println!("  Status:  not running (cleaned up stale PID file)");
            }
        }
        None => {
            println!("skltn-obs proxy");
            println!("  Status:  not running");
        }
    }

    Ok(())
}
