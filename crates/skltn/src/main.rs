use std::path::PathBuf;
use std::process;

use clap::{Parser, Subcommand};

mod pid;
mod skeletonize;
mod start;
mod status;
mod stop;

#[derive(Parser)]
#[command(
    name = "skltn",
    version,
    about = "AI context window optimization via AST-based skeletonization"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start skltn proxy, register MCP server, and launch Claude Code
    Start {
        /// Port for the observability proxy
        #[arg(long, default_value = "8080")]
        port: u16,

        /// Skip the observability proxy (MCP-only mode)
        #[arg(long)]
        no_obs: bool,
    },

    /// Stop the running skltn-obs proxy
    Stop,

    /// Show proxy status and registered projects
    Status,

    /// Skeletonize source files for AI context compression
    Skeletonize {
        /// File or directory to skeletonize
        path: PathBuf,

        /// Maximum nesting depth (default: unlimited)
        #[arg(long)]
        max_depth: Option<usize>,

        /// Force language detection (rust, python, typescript, javascript)
        #[arg(long)]
        lang: Option<String>,

        /// Output without markdown fencing
        #[arg(long)]
        raw: bool,
    },
}

fn main() {
    let cli = Cli::parse();

    let result = match cli.command {
        Commands::Start { port, no_obs } => start::run(port, no_obs),
        Commands::Stop => stop::run(),
        Commands::Status => status::run(),
        Commands::Skeletonize {
            path,
            max_depth,
            lang,
            raw,
        } => skeletonize::run(path, max_depth, lang, raw),
    };

    if let Err(e) = result {
        eprintln!("Error: {e}");
        process::exit(1);
    }
}
