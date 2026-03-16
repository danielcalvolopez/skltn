use std::collections::BTreeMap;
use std::path::Path;

use super::language_name;

fn language_for_extension(ext: &str) -> Option<&'static str> {
    let name = language_name(ext);
    if name == "unknown" {
        None
    } else {
        Some(name)
    }
}

enum TreeNode {
    File { size: u64, language: String },
    Dir { children: BTreeMap<String, TreeNode> },
}

impl TreeNode {
    fn has_supported_files(&self) -> bool {
        match self {
            TreeNode::File { .. } => true,
            TreeNode::Dir { children } => children.values().any(|c| c.has_supported_files()),
        }
    }
}

/// Build an indented tree representation of the supported source files
/// under `root/relative`. Only files with recognized language extensions
/// are included. Empty directories (or directories containing only
/// unsupported files) are pruned from the output.
///
/// `max_depth` limits how many directory levels deep to traverse
/// (e.g., `Some(1)` shows immediate children of `relative` only).
pub fn build_tree(root: &Path, relative: &str, max_depth: Option<usize>) -> String {
    let target = if relative == "." {
        root.to_path_buf()
    } else {
        root.join(relative)
    };

    let mut tree_root: BTreeMap<String, TreeNode> = BTreeMap::new();

    let walker = ignore::WalkBuilder::new(&target)
        .hidden(true)
        .git_ignore(true)
        .build();

    for entry in walker.flatten() {
        let path = entry.path();
        if path == target {
            continue;
        }

        let rel = match path.strip_prefix(&target) {
            Ok(r) => r,
            Err(_) => continue,
        };

        let components: Vec<&str> = rel
            .components()
            .filter_map(|c| c.as_os_str().to_str())
            .collect();

        if components.is_empty() {
            continue;
        }

        if path.is_file() {
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            let lang = match language_for_extension(ext) {
                Some(l) => l,
                None => continue,
            };

            // max_depth applies to directory levels; a file at components.len()
            // path segments is at depth (components.len() - 1) directories deep.
            // With max_depth=1 we want files at depth 0 and 1 (i.e., inside
            // one level of subdirectory), so components.len() <= max_depth + 1.
            if let Some(max) = max_depth {
                if components.len() > max + 1 {
                    continue;
                }
            }

            let size = path.metadata().map(|m| m.len()).unwrap_or(0);

            let mut current = &mut tree_root;
            for (i, component) in components.iter().enumerate() {
                if i == components.len() - 1 {
                    current.insert(
                        component.to_string(),
                        TreeNode::File {
                            size,
                            language: lang.to_string(),
                        },
                    );
                } else {
                    let entry = current
                        .entry(component.to_string())
                        .or_insert_with(|| TreeNode::Dir {
                            children: BTreeMap::new(),
                        });
                    if let TreeNode::Dir { children } = entry {
                        current = children;
                    } else {
                        break;
                    }
                }
            }
        }
    }

    let mut output = String::new();
    render_tree(&tree_root, &mut output, 0);
    output
}

fn render_tree(nodes: &BTreeMap<String, TreeNode>, output: &mut String, depth: usize) {
    let indent = "  ".repeat(depth);
    for (name, node) in nodes {
        match node {
            TreeNode::File { size, language } => {
                let formatted_size = format_bytes(*size);
                output.push_str(&format!("{indent}{name} ({formatted_size}, {language})\n"));
            }
            TreeNode::Dir { children } => {
                if !node.has_supported_files() {
                    continue;
                }
                output.push_str(&format!("{indent}{name}/\n"));
                render_tree(children, output, depth + 1);
            }
        }
    }
}

fn format_bytes(bytes: u64) -> String {
    let s = bytes.to_string();
    let mut result = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    let formatted: String = result.chars().rev().collect();
    format!("{formatted} bytes")
}
