use std::io::{self, Write};
use std::path::PathBuf;

use ignore::WalkBuilder;
use is_terminal::IsTerminal;

use skltn_core::backend::{backend_for_extension, backend_for_lang};
use skltn_core::engine::SkeletonEngine;
use skltn_core::options::SkeletonOptions;

pub fn run(
    path: PathBuf,
    max_depth: Option<usize>,
    lang: Option<String>,
    raw: bool,
) -> Result<(), String> {
    let options = SkeletonOptions { max_depth };
    let is_tty = io::stdout().is_terminal();
    let use_markdown = !raw && is_tty;

    if path.is_file() {
        process_file(&path, &options, lang.as_deref(), use_markdown)
    } else if path.is_dir() {
        process_directory(&path, &options, use_markdown)
    } else {
        Err(format!(
            "'{}' is not a valid file or directory",
            path.display()
        ))
    }
}

fn process_file(
    path: &PathBuf,
    options: &SkeletonOptions,
    lang_override: Option<&str>,
    use_markdown: bool,
) -> Result<(), String> {
    let backend = if let Some(lang) = lang_override {
        backend_for_lang(lang).ok_or_else(|| format!("Unsupported language '{lang}'"))?
    } else {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");
        backend_for_extension(ext)
            .ok_or_else(|| format!("Unsupported file extension '.{ext}'"))?
    };

    let source = std::fs::read_to_string(path)
        .map_err(|e| format!("Error reading '{}': {e}", path.display()))?;

    let skeleton = SkeletonEngine::skeletonize(&source, backend.as_ref(), options)
        .map_err(|e| format!("Error skeletonizing '{}': {e}", path.display()))?;

    let stdout = io::stdout();
    let mut out = stdout.lock();

    if use_markdown {
        let lang_tag = path
            .extension()
            .and_then(|e| e.to_str())
            .map(ext_to_lang_tag)
            .unwrap_or("");
        writeln!(out, "```{lang_tag}").unwrap();
        write!(out, "{skeleton}").unwrap();
        writeln!(out, "```").unwrap();
    } else {
        write!(out, "{skeleton}").unwrap();
    }

    Ok(())
}

fn process_directory(
    dir: &PathBuf,
    options: &SkeletonOptions,
    use_markdown: bool,
) -> Result<(), String> {
    let mut found_any = false;

    let walker = WalkBuilder::new(dir)
        .standard_filters(true)
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
                eprintln!("Warning: could not read '{}': {e}", path.display());
                continue;
            }
        };

        let skeleton = match SkeletonEngine::skeletonize(&source, backend.as_ref(), options) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Warning: skeletonize failed for '{}': {e}", path.display());
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
            writeln!(out, "```{lang_tag}").unwrap();
            write!(out, "{skeleton}").unwrap();
            writeln!(out, "```").unwrap();
            writeln!(out).unwrap();
        } else {
            writeln!(out, "// === {} ===", relative.display()).unwrap();
            write!(out, "{skeleton}").unwrap();
            writeln!(out).unwrap();
        }
    }

    if !found_any {
        eprintln!(
            "Warning: no supported files found in '{}'",
            dir.display()
        );
    }

    Ok(())
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
