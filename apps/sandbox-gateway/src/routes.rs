//! The gateway's HTTP surface (worker-facing; docs/33 "Sandbox Gateway"):
//!
//! - `POST /v1/sandboxes` — provision a sandbox for `{tenant_id, session_id,
//!   task_id, lease_generation, spec}`; the caller must hold the session's
//!   lease at that generation.
//! - `GET /v1/sandboxes/{id}?tenant_id=` — the record.
//! - `POST /v1/sandboxes/{id}/calls` — one typed guest call
//!   (`{tenant_id, task_id, effect_id?, call: {kind, ...}}`).
//! - `DELETE /v1/sandboxes/{id}?tenant_id=` — destroy.
//!
//! Every route needs `Authorization: Bearer <worker token>`. A sandbox of
//! another tenant is `404 NOT_FOUND` and the attempt is audited on the
//! requesting tenant (REQ-EV-0286).

use std::sync::Arc;
use std::time::Duration;

use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use modbit_domain::{SessionId, TaskId, TenantId};
use modbit_protocol::v1 as wire;
use modbit_sandbox::auth::WorkerClaims;
use modbit_sandbox::link::GuestLink;
use modbit_sandbox::policy::{EgressRule, NetworkPolicy, Resources, SandboxSpec, compile};
use modbit_sandbox::{SandboxError, backend::Channel};
use serde_json::{Value, json};

use crate::{AppState, Live};

/// Error envelope.
#[derive(Debug)]
pub struct ApiError {
    /// Status.
    pub status: StatusCode,
    /// Stable code.
    pub code: &'static str,
    /// Message.
    pub message: String,
}

impl ApiError {
    fn new(status: StatusCode, code: &'static str, message: impl Into<String>) -> Self {
        Self {
            status,
            code,
            message: message.into(),
        }
    }
    fn bad(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, "BAD_PAYLOAD", message)
    }
}

impl From<SandboxError> for ApiError {
    fn from(e: SandboxError) -> Self {
        match e {
            SandboxError::Refused { code, message } => Self::new(
                StatusCode::CONFLICT,
                "GUEST_REFUSED",
                format!("{code}: {message}"),
            ),
            SandboxError::Unsupported(m) => {
                Self::new(StatusCode::UNPROCESSABLE_ENTITY, "POLICY_UNSUPPORTED", m)
            }
            SandboxError::Substrate(m) => {
                Self::new(StatusCode::SERVICE_UNAVAILABLE, "SUBSTRATE", m)
            }
            other => Self::new(StatusCode::BAD_GATEWAY, "GUEST", other.to_string()),
        }
    }
}

impl From<modbit_event_store::cloud::CloudError> for ApiError {
    fn from(e: modbit_event_store::cloud::CloudError) -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, "STORE", e.to_string())
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(json!({"code": self.code, "message": self.message})),
        )
            .into_response()
    }
}

type ApiResult<T> = Result<T, ApiError>;

/// The router.
pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/v1/health", get(health))
        .route("/v1/sandboxes", post(provision))
        .route("/v1/sandboxes/{sandbox_id}", get(record).delete(destroy))
        .route("/v1/sandboxes/{sandbox_id}/calls", post(call))
        .with_state(state)
}

async fn health(State(st): State<Arc<AppState>>) -> Json<Value> {
    Json(json!({
        "ok": true,
        "backend": st.backend.kind(),
        "isolated": st.backend.isolates(),
        "guest_protocol": format!("{}.{}", modbit_sandbox::GUEST_PROTOCOL_MAJOR, modbit_sandbox::GUEST_PROTOCOL_MINOR),
        "image": st.backend.image().map(|m| json!({"kind": m.kind, "sha256": m.sha256, "guest_version": m.guest_version, "guest_protocol": m.guest_protocol, "built_from": m.built_from})),
    }))
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn worker(st: &AppState, headers: &HeaderMap) -> ApiResult<WorkerClaims> {
    let auth = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    let token = auth.strip_prefix("Bearer ").ok_or_else(|| {
        ApiError::new(
            StatusCode::UNAUTHORIZED,
            "UNAUTHENTICATED",
            "a worker bearer token is required",
        )
    })?;
    st.worker_key.verify(token, now_ms()).ok_or_else(|| {
        ApiError::new(
            StatusCode::UNAUTHORIZED,
            "UNAUTHENTICATED",
            "the worker token does not verify or has expired",
        )
    })
}

fn tenant_of(v: &Value) -> ApiResult<TenantId> {
    v["tenant_id"]
        .as_str()
        .and_then(|s| TenantId::parse(s).ok())
        .ok_or_else(|| ApiError::bad("tenant_id (uuid) is required"))
}

fn sandbox_id_of(s: &str) -> ApiResult<uuid::Uuid> {
    uuid::Uuid::parse_str(s).map_err(|_| ApiError::bad("sandbox id is not a uuid"))
}

/// The sandbox for this tenant, or `404` with the denial audited.
async fn owned(
    st: &AppState,
    tenant: TenantId,
    worker_id: &str,
    sandbox: uuid::Uuid,
) -> ApiResult<(modbit_event_store::cloud::SandboxRecord, Arc<Live>)> {
    let Some(rec) = st.store.sandbox(tenant, sandbox).await? else {
        st.store.record_denial(Some(tenant), None, &format!("sandbox:{sandbox}"), &format!("worker {worker_id} of tenant {tenant} addressed a sandbox that is not the tenant's")).await?;
        return Err(ApiError::new(
            StatusCode::NOT_FOUND,
            "NOT_FOUND",
            format!("sandbox {sandbox}"),
        ));
    };
    if rec.worker_id != worker_id {
        st.store
            .record_denial(
                Some(tenant),
                None,
                &format!("sandbox:{sandbox}"),
                &format!(
                    "worker {worker_id} addressed a sandbox held by {}",
                    rec.worker_id
                ),
            )
            .await?;
        return Err(ApiError::new(
            StatusCode::FORBIDDEN,
            "NOT_HOLDER",
            format!("sandbox {sandbox} is held by another worker"),
        ));
    }
    let live = st.live.lock().await.get(&sandbox).cloned();
    match live {
        Some(l) if rec.state == "READY" => Ok((rec, l)),
        _ => Err(ApiError::new(
            StatusCode::GONE,
            "SANDBOX_GONE",
            format!("sandbox {sandbox} is {}", rec.state),
        )),
    }
}

fn spec_of(
    v: &Value,
    tenant: TenantId,
    session: SessionId,
    task: TaskId,
) -> ApiResult<SandboxSpec> {
    let spec = &v["spec"];
    let workspace_source = spec["workspace_source"]
        .as_str()
        .ok_or_else(|| ApiError::bad("spec.workspace_source is required"))?;
    let protected_paths = spec["protected_paths"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|p| p.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    let egress = spec["egress"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|e| {
                    Some(EgressRule {
                        host: e["host"].as_str()?.to_owned(),
                        port: u16::try_from(e["port"].as_u64()?).ok()?,
                        capability: e["capability"]
                            .as_str()
                            .unwrap_or("network.egress")
                            .to_owned(),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    let mut resources = Resources::default();
    if let Some(r) = spec["resources"].as_object() {
        if let Some(x) = r.get("vcpus").and_then(Value::as_u64) {
            resources.vcpus = x as u32;
        }
        if let Some(x) = r.get("memory_mib").and_then(Value::as_u64) {
            resources.memory_mib = x as u32;
        }
        if let Some(x) = r.get("workspace_mib").and_then(Value::as_u64) {
            resources.workspace_mib = x as u32;
        }
        if let Some(x) = r.get("max_output_bytes").and_then(Value::as_u64) {
            resources.max_output_bytes = x;
        }
        if let Some(x) = r.get("exec_timeout_ms").and_then(Value::as_u64) {
            resources.exec_timeout_ms = x;
        }
        if let Some(x) = r.get("max_processes").and_then(Value::as_u64) {
            resources.max_processes = x as u32;
        }
    }
    Ok(SandboxSpec {
        tenant_id: tenant,
        session_id: session,
        task_id: task,
        workspace_source: workspace_source.into(),
        protected_paths,
        network: NetworkPolicy { egress },
        resources,
    })
}

async fn provision(
    State(st): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> ApiResult<(StatusCode, Json<Value>)> {
    let w = worker(&st, &headers)?;
    let tenant = tenant_of(&body)?;
    let session = body["session_id"]
        .as_str()
        .and_then(|s| SessionId::parse(s).ok())
        .ok_or_else(|| ApiError::bad("session_id (uuid) is required"))?;
    let task = body["task_id"]
        .as_str()
        .and_then(|s| TaskId::parse(s).ok())
        .ok_or_else(|| ApiError::bad("task_id (uuid) is required"))?;
    let generation = body["lease_generation"]
        .as_u64()
        .ok_or_else(|| ApiError::bad("lease_generation is required"))?;
    // The lease: this worker, this generation, on a session of this tenant.
    match st.store.held_by(tenant, session).await? {
        Some((holder, g)) if holder == w.worker_id && g == generation => {}
        Some((holder, g)) if holder == w.worker_id => {
            return Err(ApiError::new(
                StatusCode::CONFLICT,
                "STALE_LEASE",
                format!("the session is held at generation {g}, not {generation}"),
            ));
        }
        Some((holder, _)) => {
            st.store
                .record_denial(
                    Some(tenant),
                    None,
                    &format!("session:{session}"),
                    &format!(
                        "worker {} asked for a sandbox on a session held by {holder}",
                        w.worker_id
                    ),
                )
                .await?;
            return Err(ApiError::new(
                StatusCode::FORBIDDEN,
                "LEASE_NOT_HELD",
                "the session's lease is held by another worker",
            ));
        }
        None => {
            st.store.record_denial(Some(tenant), None, &format!("session:{session}"), &format!("worker {} asked for a sandbox on a session it does not hold (no live lease for this tenant)", w.worker_id)).await?;
            return Err(ApiError::new(
                StatusCode::FORBIDDEN,
                "LEASE_NOT_HELD",
                "no live lease on this session for this tenant and worker",
            ));
        }
    }
    let spec = spec_of(&body, tenant, session, task)?;
    let policy = compile(&spec)?;
    let sandbox_id = uuid::Uuid::now_v7();
    let provisioned = st
        .backend
        .provision(&sandbox_id.to_string(), &policy)
        .await?;
    let backend = provisioned.backend;
    let isolated = provisioned.isolated;
    let detail = provisioned.detail.clone();
    let link: GuestLink<Channel> = match GuestLink::admit(
        provisioned.channel,
        &sandbox_id.to_string(),
        &policy,
        Duration::from_secs(30),
    )
    .await
    {
        Ok(l) => l,
        Err(e) => {
            let _ = st.backend.destroy(&sandbox_id.to_string()).await;
            return Err(e.into());
        }
    };
    let hello = link.hello.clone();
    st.store
        .record_sandbox(
            sandbox_id,
            tenant,
            session,
            task,
            &w.worker_id,
            generation,
            backend,
            isolated,
            "READY",
            &format!(
                "{detail}; guest {} protocol {}.{}",
                hello.guest_version, hello.protocol_major, hello.protocol_minor
            ),
        )
        .await?;
    st.live.lock().await.insert(
        sandbox_id,
        Arc::new(Live {
            link: tokio::sync::Mutex::new(link),
            tenant_id: tenant,
            worker_id: w.worker_id.clone(),
            hello: hello.clone(),
        }),
    );
    Ok((
        StatusCode::CREATED,
        Json(json!({
            "sandbox_id": sandbox_id.to_string(),
            "backend": backend,
            "isolated": isolated,
            "policy": {"workspace_root": policy.workspace_root, "protected_paths": policy.protected_paths, "network_interface": policy.network_interface},
            "guest": {"version": hello.guest_version, "protocol": format!("{}.{}", hello.protocol_major, hello.protocol_minor), "methods": hello.methods, "boot_id": hello.boot_id},
            "image": st.backend.image().map(|m| json!({"kind": m.kind, "sha256": m.sha256, "guest_version": m.guest_version})),
        })),
    ))
}

async fn record(
    State(st): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Query(q): Query<std::collections::HashMap<String, String>>,
) -> ApiResult<Json<Value>> {
    let w = worker(&st, &headers)?;
    let tenant = q
        .get("tenant_id")
        .and_then(|s| TenantId::parse(s).ok())
        .ok_or_else(|| ApiError::bad("tenant_id is required"))?;
    let sandbox = sandbox_id_of(&id)?;
    let Some(rec) = st.store.sandbox(tenant, sandbox).await? else {
        st.store
            .record_denial(
                Some(tenant),
                None,
                &format!("sandbox:{sandbox}"),
                &format!(
                    "worker {} of tenant {tenant} read a sandbox that is not the tenant's",
                    w.worker_id
                ),
            )
            .await?;
        return Err(ApiError::new(
            StatusCode::NOT_FOUND,
            "NOT_FOUND",
            format!("sandbox {sandbox}"),
        ));
    };
    Ok(Json(json!({
        "sandbox_id": rec.sandbox_id.to_string(), "session_id": rec.session_id.to_string(), "task_id": rec.task_id.to_string(),
        "worker_id": rec.worker_id, "lease_generation": rec.lease_generation, "backend": rec.backend, "isolated": rec.isolated, "state": rec.state, "detail": rec.detail,
    })))
}

async fn call(
    State(st): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(body): Json<Value>,
) -> ApiResult<Json<Value>> {
    let w = worker(&st, &headers)?;
    let tenant = tenant_of(&body)?;
    let sandbox = sandbox_id_of(&id)?;
    let (rec, live) = owned(&st, tenant, &w.worker_id, sandbox).await?;
    let task_id = body["task_id"]
        .as_str()
        .unwrap_or(&rec.task_id.to_string())
        .to_owned();
    let effect_id = body["effect_id"].as_str().unwrap_or_default().to_owned();
    let c = &body["call"];
    let kind = c["kind"]
        .as_str()
        .ok_or_else(|| ApiError::bad("call.kind is required"))?;
    let mut link = live.link.lock().await;
    let out = match kind {
        "health" => {
            let h = link.health(&task_id).await?;
            json!({"kind": "health", "boot_id": h.boot_id, "uptime_ms": h.uptime_ms, "processes": h.processes, "kernel": h.kernel})
        }
        "exec" => {
            let argv: Vec<String> = c["argv"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|x| x.as_str().map(str::to_owned))
                        .collect()
                })
                .unwrap_or_default();
            if argv.is_empty() {
                return Err(ApiError::bad("call.argv is required"));
            }
            let env: Vec<String> = c["env"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|x| x.as_str().map(str::to_owned))
                        .collect()
                })
                .unwrap_or_default();
            let stdin = c["stdin"]
                .as_str()
                .map(|s| s.as_bytes().to_vec())
                .unwrap_or_default();
            let r = link
                .exec(
                    &task_id,
                    &effect_id,
                    wire::GuestExec {
                        argv,
                        cwd: c["cwd"].as_str().unwrap_or_default().to_owned(),
                        env,
                        timeout_ms: c["timeout_ms"].as_u64().unwrap_or(60_000),
                        stdin,
                    },
                )
                .await?;
            json!({"kind": "exec", "exit_code": r.exit_code, "stdout": String::from_utf8_lossy(&r.stdout), "stderr": String::from_utf8_lossy(&r.stderr), "stdout_truncated": r.stdout_truncated, "stderr_truncated": r.stderr_truncated, "timed_out": r.timed_out, "duration_ms": r.duration_ms})
        }
        "fs.read" => {
            let path = c["path"]
                .as_str()
                .ok_or_else(|| ApiError::bad("call.path is required"))?;
            let f = link
                .read_file(&task_id, path, c["max_bytes"].as_u64().unwrap_or(0))
                .await?;
            json!({"kind": "fs.read", "content": String::from_utf8_lossy(&f.content), "truncated": f.truncated})
        }
        "fs.write" => {
            let path = c["path"]
                .as_str()
                .ok_or_else(|| ApiError::bad("call.path is required"))?;
            let content = c["content"]
                .as_str()
                .unwrap_or_default()
                .as_bytes()
                .to_vec();
            let wr = link.write_file(&task_id, &effect_id, path, content).await?;
            json!({"kind": "fs.write", "bytes": wr.bytes})
        }
        "net.probe" => {
            let host = c["host"]
                .as_str()
                .ok_or_else(|| ApiError::bad("call.host is required"))?;
            let port = u16::try_from(c["port"].as_u64().unwrap_or(0))
                .map_err(|_| ApiError::bad("call.port"))?;
            let p = link
                .net_probe(
                    &task_id,
                    host,
                    port,
                    c["timeout_ms"].as_u64().unwrap_or(3_000),
                )
                .await?;
            json!({"kind": "net.probe", "reachable": p.reachable, "error": p.error})
        }
        other => {
            return Err(ApiError::bad(format!(
                "call.kind `{other}` is not a guest method"
            )));
        }
    };
    Ok(Json(out))
}

async fn destroy(
    State(st): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Query(q): Query<std::collections::HashMap<String, String>>,
) -> ApiResult<Json<Value>> {
    let w = worker(&st, &headers)?;
    let tenant = q
        .get("tenant_id")
        .and_then(|s| TenantId::parse(s).ok())
        .ok_or_else(|| ApiError::bad("tenant_id is required"))?;
    let sandbox = sandbox_id_of(&id)?;
    let (rec, _live) = owned(&st, tenant, &w.worker_id, sandbox).await?;
    st.live.lock().await.remove(&sandbox);
    st.backend.destroy(&sandbox.to_string()).await?;
    st.store
        .set_sandbox_state(tenant, sandbox, "DESTROYED", &rec.detail)
        .await?;
    Ok(Json(
        json!({"sandbox_id": sandbox.to_string(), "state": "DESTROYED"}),
    ))
}
