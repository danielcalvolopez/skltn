use std::io::{self, Write};
use std::path::PathBuf;
use std::process;

use clap::Parser;
use ignore::WalkBuilder;
use is_terminal::IsTerminal;

use skltn_core::backend::{backend_for_extension, backend_for_lang};
use skltn_core::engine::SkeletonEngine;
use skltn_core::options::SkeletonOptions;

#[derive(Parser)]
#[command(name = "skltn", version, about = "Skeletonize source code for AI context compression")]
struct Cli {
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
}

fn main() {
    let cli = Cli::parse();
    let options = SkeletonOptions {
        max_depth: cli.max_depth,
    };

    let is_tty = io::stdout().is_terminal();
    let use_markdown = !cli.raw && is_tty;

    if cli.path.is_file() {
        process_file(&cli.path, &options, cli.lang.as_deref(), use_markdown);
    } else if cli.path.is_dir() {
        process_directory(&cli.path, &options, use_markdown);
    } else {
        eprintln!("Error: '{}' is not a valid file or directory", cli.path.display());
        process::exit(1);
    }
}

fn process_file(
    path: &PathBuf,
    options: &SkeletonOptions,
    lang_override: Option<&str>,
    use_markdown: bool,
) {
    let backend = if let Some(lang) = lang_override {
        match backend_for_lang(lang) {
            Some(b) => b,
            None => {
                eprintln!("Error: unsupported language '{}'", lang);
                process::exit(1);
            }
        }
    } else {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");
        match backend_for_extension(ext) {
            Some(b) => b,
            None => {
                eprintln!("Error: unsupported file extension '.{}'", ext);
                process::exit(1);
            }
        }
    };

    let source = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error reading '{}': {}", path.display(), e);
            process::exit(1);
        }
    };

    let skeleton = match SkeletonEngine::skeletonize(&source, backend.as_ref(), options) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error skeletonizing '{}': {}", path.display(), e);
            process::exit(1);
        }
    };

    let stdout = io::stdout();
    let mut out = stdout.lock();

    if use_markdown {
        let lang_tag = path
            .extension()
            .and_then(|e| e.to_str())
            .map(ext_to_lang_tag)
            .unwrap_or("");
        writeln!(out, "```{}", lang_tag).unwrap();
        write!(out, "{}", skeleton).unwrap();
        writeln!(out, "```").unwrap();
    } else {
        write!(out, "{}", skeleton).unwrap();
    }
}

fn process_directory(dir: &PathBuf, options: &SkeletonOptions, use_markdown: bool) {
    let mut found_any = false;

    let walker = WalkBuilder::new(dir)
        .standard_filters(true) // respects .gitignore
        .build();

    for entry in walker.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        let ext = match path.extension().and_then(|e| e.to_str()) {
            Some(e) => e,
            None => continue,
        };

        let backend = match backend_for_extension(ext) {
            Some(b) => b,
            None => continue,
        };

        let source = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Warning: could not read '{}': {}", path.display(), e);
                continue;
            }
        };

        let skeleton = match SkeletonEngine::skeletonize(&source, backend.as_ref(), options) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Warning: skeletonize failed for '{}': {}", path.display(), e);
                continue;
            }
        };

        found_any = true;
        let relative = path.strip_prefix(dir).unwrap_or(path);
        let stdout = io::stdout();
        let mut out = stdout.lock();

        if use_markdown {
            let lang_tag = ext_to_lang_tag(ext);
            writeln!(out, "## File: {}", relative.display()).unwrap();
            writeln!(out, "```{}", lang_tag).unwrap();
            write!(out, "{}", skeleton).unwrap();
            writeln!(out, "```").unwrap();
            writeln!(out).unwrap();
        } else {
            writeln!(out, "// === {} ===", relative.display()).unwrap();
            write!(out, "{}", skeleton).unwrap();
            writeln!(out).unwrap();
        }
    }

    if !found_any {
        eprintln!(
            "Warning: no supported files found in '{}'",
            dir.display()
        );
    }
}

fn ext_to_lang_tag(ext: &str) -> &str {
    match ext {
        "rs" => "rust",
        "py" => "python",
        "ts" => "typescript",
        "js" => "javascript",
        _ => ext,
    }
}
