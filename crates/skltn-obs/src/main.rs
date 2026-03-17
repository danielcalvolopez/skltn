use clap::Parser;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

use axum::routing::get;
use axum::Router;
use tokio::net::TcpListener;
use tokio::signal;

use skltn_obs::proxy::AppState;
use skltn_obs::tracker::CostTracker;
use skltn_obs::{proxy, ws};

#[derive(Parser)]
#[command(name = "skltn-obs", about = "Anthropic API observability proxy")]
struct Cli {
    /// Local port to listen on
    #[arg(long, default_value = "8080")]
    port: u16,

    /// Bind address (default: 127.0.0.1 — localhost only for security)
    #[arg(long, default_value = "127.0.0.1")]
    bind: String,

    /// Required when --bind is non-loopback (safety gate for API key exposure)
    #[arg(long, default_value_t = false)]
    allow_external: bool,

    /// Anthropic API base URL
    #[arg(long, default_value = "https://api.anthropic.com")]
    upstream: String,

    /// Data directory for JSONL persistence
    #[arg(long, default_value_os_t = default_data_dir())]
    data_dir: PathBuf,
}

fn default_data_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".skltn")
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();

    // Validate --upstream: must be HTTPS unless loopback
    if let Ok(url) = url::Url::parse(&cli.upstream) {
        let is_loopback_host = matches!(
            url.host_str(),
            Some("127.0.0.1") | Some("localhost") | Some("::1") | Some("[::1]")
        );
        if url.scheme() != "https" && !is_loopback_host {
            eprintln!(
                "Error: --upstream must use HTTPS for non-loopback hosts. Got: {}",
                cli.upstream
            );
            std::process::exit(1);
        }
        if let Some(host) = url.host_str() {
            if !host.contains("anthropic.com") && !is_loopback_host {
                tracing::warn!(
                    "Upstream '{}' is not an Anthropic endpoint — API key will be sent to this server.",
                    cli.upstream
                );
            }
        }
    }

    tracing::info!(
        port = cli.port,
        upstream = %cli.upstream,
        data_dir = %cli.data_dir.display(),
        "Starting skltn-obs proxy"
    );

    let tracker = CostTracker::new(cli.data_dir).await;

    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .tcp_nodelay(true)
        .timeout(Duration::from_secs(300))
        .build()
        .expect("failed to build HTTP client");

    let state = AppState {
        client,
        upstream: cli.upstream,
        tracker: tracker.clone(),
    };

    let app = Router::new()
        .route("/ws", get(ws::ws_handler))
        .fallback(proxy::proxy_handler)
        .with_state(state)
        .layer(axum::extract::DefaultBodyLimit::max(200 * 1024 * 1024));

    let addr: SocketAddr = format!("{}:{}", cli.bind, cli.port)
        .parse()
        .expect("invalid bind address");

    // Refuse non-loopback bind unless --allow-external is passed
    if !addr.ip().is_loopback() {
        if !cli.allow_external {
            eprintln!(
                "Error: Binding to non-loopback address {} requires --allow-external flag.\n\
                 API keys will be transmitted in cleartext HTTP over the network.",
                addr.ip()
            );
            std::process::exit(1);
        }
        eprintln!(
            "\u{26a0} WARNING: Binding to {} — API keys will be transmitted in cleartext HTTP \
             over the network. Only use this on trusted networks.",
            addr
        );
    }

    let listener = TcpListener::bind(addr)
        .await
        .expect("failed to bind address");
    tracing::info!(%addr, "Proxy listening");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .expect("server error");

    tracing::info!("Proxy shut down, draining JSONL writer...");
    tracker.shutdown().await;
    tracing::info!("Shutdown complete");
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => { tracing::info!("Received SIGINT"); }
        _ = terminate => { tracing::info!("Received SIGTERM"); }
    }
}
