pub mod list_repo_structure;
pub mod read_full_symbol;
pub mod read_skeleton;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::*;
use rmcp::{tool, tool_handler, tool_router, ErrorData, ServerHandler};
use serde::Deserialize;
use skltn_core::backend::LanguageBackend;
use tiktoken_rs::CoreBPE;

use crate::cache::SkeletonCache;
use crate::savings::{DrilldownWriter, SavingsWriter};
use crate::session::SessionTracker;

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

pub fn backend_for_extension(ext: &str) -> Option<Box<dyn LanguageBackend>> {
    skltn_core::backend::backend_for_extension(ext)
}

pub fn language_name(ext: &str) -> &'static str {
    match ext {
        "rs" => "rust",
        "py" => "python",
        "ts" => "typescript",
        "tsx" => "tsx",
        "js" => "javascript",
        "jsx" => "jsx",
        _ => "unknown",
    }
}

pub fn has_parse_errors(source: &str, backend: &dyn LanguageBackend) -> bool {
    let mut parser = tree_sitter::Parser::new();
    if parser.set_language(&backend.language()).is_err() {
        return true;
    }
    match parser.parse(source, None) {
        Some(tree) => has_error_nodes(tree.root_node()),
        None => true,
    }
}

fn has_error_nodes(node: tree_sitter::Node) -> bool {
    if node.is_error() || node.is_missing() {
        return true;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if has_error_nodes(child) {
            return true;
        }
    }
    false
}

// ---------------------------------------------------------------------------
// Parameter structs
// ---------------------------------------------------------------------------

fn default_path() -> String {
    ".".to_string()
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ListRepoStructureParams {
    /// Subdirectory to list, relative to repo root. Defaults to "." (repo root).
    #[serde(default = "default_path")]
    pub path: String,
    /// Maximum directory depth to traverse. Omit for unlimited.
    #[serde(default)]
    pub max_depth: Option<usize>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ReadSkeletonParams {
    /// File path relative to repo root.
    pub file: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ReadFullSymbolParams {
    /// File path relative to repo root.
    pub file: String,
    /// Symbol name to find. Exact, case-sensitive match.
    pub symbol: String,
    /// Line number hint for disambiguation (1-indexed).
    #[serde(default)]
    pub start_line: Option<usize>,
}

// ---------------------------------------------------------------------------
// SkltnServer
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct SkltnServer {
    root: PathBuf,
    tokenizer: Arc<CoreBPE>,
    session_tracker: Arc<Mutex<SessionTracker>>,
    savings_writer: Arc<Option<SavingsWriter>>,
    drilldown_writer: Arc<Option<DrilldownWriter>>,
    skeleton_cache: Arc<Option<SkeletonCache>>,
    tool_router: ToolRouter<Self>,
}

impl SkltnServer {
    pub fn new(root: PathBuf, tokenizer: CoreBPE) -> Self {
        let tool_router = Self::tool_router();
        let savings_writer = SavingsWriter::new();
        if savings_writer.is_none() {
            tracing::warn!("Savings writer unavailable — skeletonization savings will not be recorded");
        }
        let drilldown_writer = DrilldownWriter::new();
        if drilldown_writer.is_none() {
            tracing::warn!("Drilldown writer unavailable — drilldown records will not be recorded");
        }
        let skeleton_cache = SkeletonCache::new(&root);
        if skeleton_cache.is_some() {
            tracing::info!("Skeleton cache initialized for project");
        } else {
            tracing::warn!("Skeleton cache unavailable — skeletons will not be cached across sessions");
        }
        Self {
            root,
            tokenizer: Arc::new(tokenizer),
            session_tracker: Arc::new(Mutex::new(SessionTracker::new())),
            savings_writer: Arc::new(savings_writer),
            drilldown_writer: Arc::new(drilldown_writer),
            skeleton_cache: Arc::new(skeleton_cache),
            tool_router,
        }
    }
}

#[tool_router]
impl SkltnServer {
    /// List the repository file structure as a tree. Shows supported source
    /// files (.rs, .py, .ts, .tsx, .js, .jsx) with byte sizes and detected languages.
    #[tool(
        name = "list_repo_structure",
        description = "List the repository file structure as a tree. Shows supported source files (.rs, .py, .ts, .tsx, .js, .jsx) with byte sizes and detected languages."
    )]
    async fn list_repo_structure(
        &self,
        Parameters(params): Parameters<ListRepoStructureParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let root = self.root.clone();
        let path = params.path;
        let max_depth = params.max_depth;

        let output = tokio::task::spawn_blocking(move || {
            match crate::resolve::resolve_safe_path(&root, &path) {
                Ok(resolved) => {
                    if resolved.is_file() {
                        return crate::error::McpError::PathIsFile(path).to_string();
                    }
                    if !resolved.is_dir() {
                        return crate::error::McpError::DirectoryNotFound(path).to_string();
                    }
                    let tree = list_repo_structure::build_tree(&root, &path, max_depth);
                    if tree.trim().is_empty() {
                        crate::error::McpError::NoSupportedFiles(path).to_string()
                    } else {
                        tree
                    }
                }
                Err(crate::error::McpError::PathOutsideRoot) => {
                    crate::error::McpError::PathOutsideRoot.to_string()
                }
                Err(_) => crate::error::McpError::DirectoryNotFound(path).to_string(),
            }
        })
        .await
        .map_err(|e| ErrorData::internal_error(format!("Internal error: {e}"), None))?;

        Ok(CallToolResult::success(vec![Content::text(output)]))
    }

    /// Read a source file, returning either the full file or a skeletonized
    /// version depending on the token budget (threshold: 2000 tokens).
    #[tool(
        name = "read_skeleton",
        description = "Read a source file, returning either the full file (<=2000 tokens) or a skeletonized version (>2000 tokens)."
    )]
    async fn read_skeleton(
        &self,
        Parameters(params): Parameters<ReadSkeletonParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let root = self.root.clone();
        let file = params.file;
        let tokenizer = Arc::clone(&self.tokenizer);
        let tracker = Arc::clone(&self.session_tracker);
        let savings_writer = Arc::clone(&self.savings_writer);
        let skeleton_cache = Arc::clone(&self.skeleton_cache);

        let output = tokio::task::spawn_blocking(move || {
            read_skeleton::read_skeleton_or_full(&root, &file, &tokenizer, &tracker, &savings_writer, skeleton_cache.as_ref().as_ref(), true)
        })
        .await
        .map_err(|e| ErrorData::internal_error(format!("Internal error: {e}"), None))?;

        Ok(CallToolResult::success(vec![Content::text(output)]))
    }

    /// Read the full source code of a specific symbol. If multiple matches
    /// exist, returns a disambiguation list.
    #[tool(
        name = "read_full_symbol",
        description = "Read the full source code of a specific symbol. If multiple matches, returns disambiguation list."
    )]
    async fn read_full_symbol(
        &self,
        Parameters(params): Parameters<ReadFullSymbolParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let root = self.root.clone();
        let file = params.file;
        let symbol = params.symbol;
        let start_line = params.start_line;
        let tokenizer = Arc::clone(&self.tokenizer);
        let drilldown_writer = Arc::clone(&self.drilldown_writer);

        let output = tokio::task::spawn_blocking(move || {
            read_full_symbol::read_full_symbol(&root, &file, &symbol, start_line, &tokenizer, &drilldown_writer)
        })
        .await
        .map_err(|e| ErrorData::internal_error(format!("Internal error: {e}"), None))?;

        Ok(CallToolResult::success(vec![Content::text(output)]))
    }
}

#[tool_handler]
impl ServerHandler for SkltnServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_instructions(
                "Skeleton (skltn) MCP server. Navigate codebases efficiently: \
                 list_repo_structure -> read_skeleton -> read_full_symbol."
                    .to_string(),
            )
    }
}
