//! Helpers for the real-MCP-server tests (PX-115).
#![allow(dead_code)]

#[cfg(unix)]
use std::process::{Command, Stdio};
use std::time::Duration;

use modbit_protocol::client::Client;
use modbit_protocol::v1::Id;
use prost::Message;

use super::{envelope_fenced, id16};

/// The MCP test server binary, next to the test executables.
pub fn mcp_testserver_bin() -> std::path::PathBuf {
    let exe = std::env::current_exe().expect("test exe");
    let dir = exe.parent().and_then(|p| p.parent()).expect("target/debug");
    dir.join(if cfg!(windows) {
        "modbit-mcp-testserver.exe"
    } else {
        "modbit-mcp-testserver"
    })
}

/// One `InvokeTool` through the real Core.
pub async fn invoke_tool(
    c: &mut Client,
    task: &Id,
    g: Option<u64>,
    cmd: u8,
    call: u8,
    tool: &str,
    args: &str,
) -> modbit_protocol::v1::ToolInvoked {
    let ack = c
        .command(envelope_fenced(
            id16(cmd),
            "InvokeTool",
            modbit_protocol::v1::InvokeTool {
                task_id: Some(task.clone()),
                tool_name: tool.into(),
                arguments_json: args.into(),
                tool_call_id: Some(id16(call)),
                output_budget_bytes: 8 * 1024 * 1024,
            }
            .encode_to_vec(),
            g,
        ))
        .await
        .unwrap();
    Client::result(&ack).unwrap()
}

/// Whether a process still exists (a zombie counts as existing: it has not
/// been reaped). Unix only.
#[cfg(unix)]
pub fn process_exists(pid: u32) -> bool {
    Command::new("kill")
        .args(["-0", &pid.to_string()])
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// Kill a process outright. Unix only.
#[cfg(unix)]
pub fn kill_9(pid: u32) {
    let _ = Command::new("kill").args(["-9", &pid.to_string()]).status();
}

/// Poll until `done`, up to `secs`.
pub async fn eventually(secs: u64, mut done: impl FnMut() -> bool) -> bool {
    let deadline = std::time::Instant::now() + Duration::from_secs(secs);
    while std::time::Instant::now() < deadline {
        if done() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    done()
}

/// The pid a test server wrote to its pidfile.
pub fn pid_of(path: &std::path::Path) -> Option<u32> {
    std::fs::read_to_string(path).ok()?.trim().parse().ok()
}

/// A server entry for `MODBIT_MCP_SERVERS`.
pub fn server_entry(
    name: &str,
    env: serde_json::Value,
    reads: &[&str],
    trust: &str,
) -> serde_json::Value {
    let mut env = env;
    env["MODBIT_MCP_TESTSRV_NAME"] = serde_json::json!(name);
    serde_json::json!({
        "name": name,
        "transport": { "kind": "stdio", "command": mcp_testserver_bin().to_string_lossy(), "args": [] },
        "env": env,
        "read_only_tools": reads,
        "scopes": ["docs:read"],
        "trust": trust,
        "layer": "project",
    })
}

/// The requests a test server logged, as JSON.
pub fn server_log(path: &std::path::Path) -> Vec<serde_json::Value> {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect()
}
