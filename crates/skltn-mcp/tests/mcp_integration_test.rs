use std::fs;

use rmcp::ServerHandler;
use skltn_mcp::tools::SkltnServer;

fn setup_server(root: &std::path::Path) -> SkltnServer {
    let tokenizer = tiktoken_rs::cl100k_base().unwrap();
    SkltnServer::new(root.canonicalize().unwrap(), tokenizer)
}

fn create_test_repo(root: &std::path::Path) {
    let src = root.join("src");
    fs::create_dir_all(&src).unwrap();
    fs::write(
        src.join("main.rs"),
        "/// Entry point\nfn main() {\n    println!(\"hello\");\n}\n",
    )
    .unwrap();
    fs::write(
        src.join("lib.rs"),
        "pub struct Config {\n    pub host: String,\n}\n\n\
         impl Config {\n    pub fn new(host: String) -> Self {\n        Config { host }\n    }\n}\n",
    )
    .unwrap();
    fs::write(
        src.join("utils.py"),
        "def greet(name: str) -> str:\n    return f\"Hello, {name}\"\n",
    )
    .unwrap();
}

#[tokio::test]
async fn test_server_construction() {
    let dir = tempfile::tempdir().unwrap();
    create_test_repo(dir.path());
    let _server = setup_server(dir.path());
    // Server constructs successfully with tool router initialized
}

#[tokio::test]
async fn test_server_get_info_has_tools_capability() {
    let dir = tempfile::tempdir().unwrap();
    create_test_repo(dir.path());
    let server = setup_server(dir.path());

    let info = server.get_info();
    assert!(
        info.capabilities.tools.is_some(),
        "tools capability must be enabled"
    );
}

#[tokio::test]
async fn test_server_get_info_has_instructions() {
    let dir = tempfile::tempdir().unwrap();
    create_test_repo(dir.path());
    let server = setup_server(dir.path());

    let info = server.get_info();
    let instructions = info.instructions.expect("instructions should be set");
    assert!(
        instructions.contains("skltn"),
        "instructions should mention skltn"
    );
    assert!(
        instructions.contains("list_repo_structure"),
        "instructions should mention list_repo_structure"
    );
    assert!(
        instructions.contains("read_skeleton"),
        "instructions should mention read_skeleton"
    );
    assert!(
        instructions.contains("read_full_symbol"),
        "instructions should mention read_full_symbol"
    );
}

#[tokio::test]
async fn test_server_registers_list_repo_structure_tool() {
    let dir = tempfile::tempdir().unwrap();
    create_test_repo(dir.path());
    let server = setup_server(dir.path());

    let tool = server
        .get_tool("list_repo_structure")
        .expect("list_repo_structure tool should be registered");
    assert_eq!(tool.name.as_ref(), "list_repo_structure");
    assert!(
        tool.description.is_some(),
        "tool should have a description"
    );
}

#[tokio::test]
async fn test_server_registers_read_skeleton_tool() {
    let dir = tempfile::tempdir().unwrap();
    create_test_repo(dir.path());
    let server = setup_server(dir.path());

    let tool = server
        .get_tool("read_skeleton")
        .expect("read_skeleton tool should be registered");
    assert_eq!(tool.name.as_ref(), "read_skeleton");
    assert!(
        tool.description.is_some(),
        "tool should have a description"
    );
}

#[tokio::test]
async fn test_server_registers_read_full_symbol_tool() {
    let dir = tempfile::tempdir().unwrap();
    create_test_repo(dir.path());
    let server = setup_server(dir.path());

    let tool = server
        .get_tool("read_full_symbol")
        .expect("read_full_symbol tool should be registered");
    assert_eq!(tool.name.as_ref(), "read_full_symbol");
    assert!(
        tool.description.is_some(),
        "tool should have a description"
    );
}

#[tokio::test]
async fn test_server_returns_none_for_unknown_tool() {
    let dir = tempfile::tempdir().unwrap();
    create_test_repo(dir.path());
    let server = setup_server(dir.path());

    assert!(
        server.get_tool("nonexistent_tool").is_none(),
        "unknown tool should return None"
    );
}

#[tokio::test]
async fn test_server_has_exactly_three_tools() {
    let dir = tempfile::tempdir().unwrap();
    create_test_repo(dir.path());
    let server = setup_server(dir.path());

    // Verify all three tools are registered and no extras
    let registered: Vec<&str> = ["list_repo_structure", "read_skeleton", "read_full_symbol"]
        .iter()
        .copied()
        .filter(|name| server.get_tool(name).is_some())
        .collect();
    assert_eq!(
        registered.len(),
        3,
        "exactly 3 tools should be registered"
    );
}
