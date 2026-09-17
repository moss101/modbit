//! The worker-side client of the Sandbox Gateway (M8.5): provisions a
//! sandbox for a task the worker holds the session lease for, and speaks
//! every guest operation through the gateway's HTTP surface. A
//! [`SandboxHandle`] implements [`SandboxPort`], so a Core's tools use it
//! without knowing what is behind the gateway.

use std::sync::Arc;

use modbit_protocol::v1 as wire;
use serde_json::{Value, json};

use crate::port::{PortFuture, SandboxIdentity, SandboxPort};
use crate::{Result, SandboxError};

/// How to reach the gateway.
#[derive(Clone)]
pub struct GatewayClient {
    base_url: String,
    token: String,
    http: reqwest::Client,
}

impl std::fmt::Debug for GatewayClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GatewayClient")
            .field("base_url", &self.base_url)
            .finish_non_exhaustive()
    }
}

/// What a provision names.
#[derive(Clone, Debug)]
pub struct ProvisionRequest {
    /// Tenant.
    pub tenant_id: String,
    /// Session.
    pub session_id: String,
    /// Task.
    pub task_id: String,
    /// The session lease generation the worker holds.
    pub lease_generation: u64,
    /// The workspace source on the gateway's host.
    pub workspace_source: String,
    /// Protected paths under the workspace.
    pub protected_paths: Vec<String>,
    /// Resource bounds (JSON object; the gateway's defaults for what is absent).
    pub resources: Value,
}

impl GatewayClient {
    /// A client with the worker's bearer token (memory only).
    #[must_use]
    pub fn new(base_url: &str, token: &str) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_owned(),
            token: token.to_owned(),
            http: reqwest::Client::new(),
        }
    }

    async fn post(&self, path: &str, body: Value) -> Result<(u16, Value)> {
        let r = self
            .http
            .post(format!("{}{path}", self.base_url))
            .bearer_auth(&self.token)
            .json(&body)
            .send()
            .await
            .map_err(|e| SandboxError::Guest(format!("gateway: {e}")))?;
        let status = r.status().as_u16();
        let v = r.json::<Value>().await.unwrap_or(Value::Null);
        Ok((status, v))
    }

    async fn delete(&self, path: &str) -> Result<(u16, Value)> {
        let r = self
            .http
            .delete(format!("{}{path}", self.base_url))
            .bearer_auth(&self.token)
            .send()
            .await
            .map_err(|e| SandboxError::Guest(format!("gateway: {e}")))?;
        let status = r.status().as_u16();
        let v = r.json::<Value>().await.unwrap_or(Value::Null);
        Ok((status, v))
    }

    /// Provision a sandbox.
    pub async fn provision(&self, req: &ProvisionRequest) -> Result<Arc<SandboxHandle>> {
        let (status, v) = self
            .post(
                "/v1/sandboxes",
                json!({
                    "tenant_id": req.tenant_id, "session_id": req.session_id, "task_id": req.task_id, "lease_generation": req.lease_generation,
                    "spec": {"workspace_source": req.workspace_source, "protected_paths": req.protected_paths, "resources": req.resources},
                }),
            )
            .await?;
        if status != 201 {
            return Err(gateway_error(status, &v));
        }
        let identity = SandboxIdentity {
            sandbox_id: v["sandbox_id"].as_str().unwrap_or_default().to_owned(),
            backend: v["backend"].as_str().unwrap_or_default().to_owned(),
            isolated: v["isolated"].as_bool().unwrap_or(false),
            image_version: v["image"]["guest_version"].as_str().map(str::to_owned),
            boot_id: v["guest"]["boot_id"]
                .as_str()
                .unwrap_or_default()
                .to_owned(),
            workspace_root: v["policy"]["workspace_root"]
                .as_str()
                .unwrap_or("/workspace")
                .to_owned(),
        };
        Ok(Arc::new(SandboxHandle {
            client: self.clone(),
            tenant_id: req.tenant_id.clone(),
            identity,
        }))
    }
}

fn gateway_error(status: u16, v: &Value) -> SandboxError {
    let code = v["code"].as_str().unwrap_or("GATEWAY").to_owned();
    let message = v["message"].as_str().unwrap_or_default().to_owned();
    match code.as_str() {
        "GUEST_REFUSED" => {
            let (c, m) = message.split_once(": ").unwrap_or((&message, ""));
            SandboxError::Refused {
                code: c.to_owned(),
                message: m.to_owned(),
            }
        }
        "POLICY_UNSUPPORTED" => SandboxError::Unsupported(message),
        "SUBSTRATE" => SandboxError::Substrate(message),
        _ => SandboxError::Guest(format!("gateway {status} {code}: {message}")),
    }
}

/// A provisioned sandbox, spoken to through the gateway.
pub struct SandboxHandle {
    client: GatewayClient,
    tenant_id: String,
    identity: SandboxIdentity,
}

impl SandboxHandle {
    async fn call(&self, task_id: &str, effect_id: &str, call: Value) -> Result<Value> {
        let (status, v) = self
            .client
            .post(
                &format!("/v1/sandboxes/{}/calls", self.identity.sandbox_id),
                json!({"tenant_id": self.tenant_id, "task_id": task_id, "effect_id": effect_id, "call": call}),
            )
            .await?;
        if status != 200 {
            return Err(gateway_error(status, &v));
        }
        Ok(v)
    }

    /// Destroy the sandbox.
    pub async fn destroy(&self) -> Result<()> {
        let (status, v) = self
            .client
            .delete(&format!(
                "/v1/sandboxes/{}?tenant_id={}",
                self.identity.sandbox_id, self.tenant_id
            ))
            .await?;
        if status != 200 && status != 410 {
            return Err(gateway_error(status, &v));
        }
        Ok(())
    }

    /// Replace a lost link to the guest.
    pub async fn relink(&self) -> Result<bool> {
        let (status, v) = self
            .client
            .post(
                &format!("/v1/sandboxes/{}/relink", self.identity.sandbox_id),
                json!({"tenant_id": self.tenant_id}),
            )
            .await?;
        if status != 200 {
            return Err(gateway_error(status, &v));
        }
        Ok(v["same_boot"].as_bool().unwrap_or(false))
    }
}

fn bytes_of(v: &Value) -> Vec<u8> {
    v.as_str()
        .map(|s| s.as_bytes().to_vec())
        .unwrap_or_default()
}

impl SandboxPort for SandboxHandle {
    fn identity(&self) -> &SandboxIdentity {
        &self.identity
    }

    fn exec<'a>(
        &'a self,
        task_id: &'a str,
        effect_id: &'a str,
        exec: wire::GuestExec,
    ) -> PortFuture<'a, Result<wire::GuestExecResult>> {
        Box::pin(async move {
            let v = self
                .call(
                    task_id,
                    effect_id,
                    json!({"kind": "exec", "argv": exec.argv, "cwd": exec.cwd, "env": exec.env, "timeout_ms": exec.timeout_ms, "stdin": String::from_utf8_lossy(&exec.stdin)}),
                )
                .await?;
            Ok(wire::GuestExecResult {
                exit_code: v["exit_code"].as_i64().unwrap_or(-1) as i32,
                stdout: bytes_of(&v["stdout"]),
                stderr: bytes_of(&v["stderr"]),
                stdout_truncated: v["stdout_truncated"].as_bool().unwrap_or(false),
                stderr_truncated: v["stderr_truncated"].as_bool().unwrap_or(false),
                timed_out: v["timed_out"].as_bool().unwrap_or(false),
                duration_ms: v["duration_ms"].as_u64().unwrap_or(0),
            })
        })
    }

    fn proc_start<'a>(
        &'a self,
        task_id: &'a str,
        effect_id: &'a str,
        start: wire::GuestProcStart,
    ) -> PortFuture<'a, Result<wire::GuestProcStarted>> {
        Box::pin(async move {
            let v = self
                .call(
                    task_id,
                    effect_id,
                    json!({"kind": "proc.start", "argv": start.argv, "cwd": start.cwd, "env": start.env, "timeout_ms": start.timeout_ms, "pty": start.pty, "cols": start.cols, "rows": start.rows, "stdin_open": start.stdin_open}),
                )
                .await?;
            Ok(wire::GuestProcStarted {
                proc_id: v["proc_id"].as_str().unwrap_or_default().to_owned(),
                pid: v["pid"].as_u64().unwrap_or(0) as u32,
            })
        })
    }

    fn proc_follow<'a>(
        &'a self,
        task_id: &'a str,
        proc_id: &'a str,
        after_cursor: u64,
        max_bytes: u64,
        wait_ms: u64,
    ) -> PortFuture<'a, Result<wire::GuestProcOutput>> {
        Box::pin(async move {
            let v = self
                .call(task_id, "", json!({"kind": "proc.follow", "proc_id": proc_id, "after_cursor": after_cursor, "max_bytes": max_bytes, "wait_ms": wait_ms}))
                .await?;
            let data = v["data_base64"]
                .as_str()
                .and_then(|b| {
                    base64::Engine::decode(&base64::engine::general_purpose::STANDARD, b).ok()
                })
                .unwrap_or_default();
            Ok(wire::GuestProcOutput {
                proc_id: proc_id.to_owned(),
                data,
                cursor: v["cursor"].as_u64().unwrap_or(after_cursor),
                truncated: v["truncated"].as_bool().unwrap_or(false),
                running: v["running"].as_bool().unwrap_or(false),
                exit_code: v["exit_code"].as_i64().map(|c| c as i32),
                timed_out: v["timed_out"].as_bool().unwrap_or(false),
                cancelled: v["cancelled"].as_bool().unwrap_or(false),
                total_bytes: v["total_bytes"].as_u64().unwrap_or(0),
            })
        })
    }

    fn proc_write<'a>(
        &'a self,
        task_id: &'a str,
        proc_id: &'a str,
        data: Vec<u8>,
        close_stdin: bool,
    ) -> PortFuture<'a, Result<()>> {
        Box::pin(async move {
            let b = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &data);
            self.call(task_id, "", json!({"kind": "proc.write", "proc_id": proc_id, "data_base64": b, "close_stdin": close_stdin})).await?;
            Ok(())
        })
    }

    fn proc_cancel<'a>(&'a self, task_id: &'a str, proc_id: &'a str) -> PortFuture<'a, Result<()>> {
        Box::pin(async move {
            self.call(
                task_id,
                "",
                json!({"kind": "proc.cancel", "proc_id": proc_id}),
            )
            .await?;
            Ok(())
        })
    }

    fn read_file<'a>(
        &'a self,
        task_id: &'a str,
        path: &'a str,
        max_bytes: u64,
    ) -> PortFuture<'a, Result<wire::GuestFileContent>> {
        Box::pin(async move {
            let v = self
                .call(
                    task_id,
                    "",
                    json!({"kind": "fs.read", "path": path, "max_bytes": max_bytes}),
                )
                .await?;
            Ok(wire::GuestFileContent {
                content: bytes_of(&v["content"]),
                truncated: v["truncated"].as_bool().unwrap_or(false),
            })
        })
    }

    fn write_file<'a>(
        &'a self,
        task_id: &'a str,
        effect_id: &'a str,
        path: &'a str,
        content: Vec<u8>,
    ) -> PortFuture<'a, Result<wire::GuestFileWritten>> {
        Box::pin(async move {
            let v = self.call(task_id, effect_id, json!({"kind": "fs.write", "path": path, "content": String::from_utf8_lossy(&content)})).await?;
            Ok(wire::GuestFileWritten {
                bytes: v["bytes"].as_u64().unwrap_or(0),
            })
        })
    }

    fn list_dir<'a>(
        &'a self,
        task_id: &'a str,
        path: &'a str,
        max_entries: u32,
    ) -> PortFuture<'a, Result<wire::GuestDirListing>> {
        Box::pin(async move {
            let v = self
                .call(
                    task_id,
                    "",
                    json!({"kind": "fs.list", "path": path, "max_entries": max_entries}),
                )
                .await?;
            let entries = v["entries"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .map(|e| wire::GuestDirEntry {
                            name: e["name"].as_str().unwrap_or_default().to_owned(),
                            kind: e["kind"].as_str().unwrap_or_default().to_owned(),
                            size: e["size"].as_u64().unwrap_or(0),
                            modified_ms: e["modified_ms"].as_i64().unwrap_or(0),
                        })
                        .collect()
                })
                .unwrap_or_default();
            Ok(wire::GuestDirListing {
                entries,
                truncated: v["truncated"].as_bool().unwrap_or(false),
            })
        })
    }

    fn stat<'a>(
        &'a self,
        task_id: &'a str,
        path: &'a str,
    ) -> PortFuture<'a, Result<wire::GuestStatResult>> {
        Box::pin(async move {
            let v = self
                .call(task_id, "", json!({"kind": "fs.stat", "path": path}))
                .await?;
            Ok(wire::GuestStatResult {
                exists: v["exists"].as_bool().unwrap_or(false),
                kind: v["file_kind"].as_str().unwrap_or_default().to_owned(),
                size: v["size"].as_u64().unwrap_or(0),
                modified_ms: v["modified_ms"].as_i64().unwrap_or(0),
                mode: v["mode"].as_u64().unwrap_or(0) as u32,
            })
        })
    }
}
