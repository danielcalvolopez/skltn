use rmcp::{ServiceExt, transport::io::stdio};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .init();

    let args: Vec<String> = std::env::args().collect();
    if args.len() != 2 {
        eprintln!("Usage: skltn-mcp <ROOT_PATH>");
        std::process::exit(1);
    }

    let root = std::path::PathBuf::from(&args[1]);
    if !root.is_dir() {
        eprintln!("Error: '{}' is not a valid directory", root.display());
        std::process::exit(1);
    }

    let root = root.canonicalize()?;
    tracing::info!("Starting skltn-mcp server with root: {}", root.display());

    let tokenizer = tiktoken_rs::cl100k_base()
        .map_err(|e| format!("Failed to initialize tokenizer: {e}"))?;

    let server = skltn_mcp::tools::SkltnServer::new(root, tokenizer);
    let service = server
        .serve(stdio())
        .await
        .inspect_err(|e| tracing::error!("serving error: {:?}", e))?;

    service.waiting().await?;
    Ok(())
}
