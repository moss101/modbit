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
use modbit_sandbox::policy::{
    CredentialGrant, EgressRule, NetworkPolicy, Resources, SandboxSpec, compile,
};
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
        .route("/v1/sandboxes/{sandbox_id}/relink", post(relink))
        .route(
            "/v1/sandboxes/{sandbox_id}/credentials",
            post(renew_credential),
        )
        .route("/v1/sandboxes/{sandbox_id}/egress", get(egress))
        .route("/v1/sandboxes/{sandbox_id}/browser/cdp", get(browser_cdp))
        .with_state(state)
}

async fn health(State(st): State<Arc<AppState>>) -> Json<Value> {
    Json(json!({
        "ok": true,
        "backend": st.backend.kind(),
        "isolated": st.backend.isolates(),
        "guest_protocol": format!("{}.{}", modbit_sandbox::GUEST_PROTOCOL_MAJOR, modbit_sandbox::GUEST_PROTOCOL_MINOR),
        // IMP-EV-0072: what guests here can do — a worker announces it.
        "features": st.backend.features(),
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

/// How long a credential handle lives when the provisioner does not say
/// (REQ-EV-0288: short-lived by construction). `MODBIT_SANDBOX_CREDENTIAL_TTL_MS`
/// overrides it; a provisioner may always ask for less.
fn default_credential_ttl_ms() -> i64 {
    std::env::var("MODBIT_SANDBOX_CREDENTIAL_TTL_MS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(15 * 60 * 1000)
}

/// The credential grants of a provision body, with the secrets they carry
/// (memory only from here on).
fn credentials_of(
    v: &Value,
) -> (
    Vec<CredentialGrant>,
    std::collections::HashMap<String, String>,
) {
    let mut grants = Vec::new();
    let mut secrets = std::collections::HashMap::new();
    if let Some(a) = v["spec"]["credentials"].as_array() {
        for c in a {
            let (Some(handle), Some(virtual_host), Some(target_url)) = (
                c["handle"].as_str(),
                c["virtual_host"].as_str(),
                c["target_url"].as_str(),
            ) else {
                continue;
            };
            // REQ-EV-0288: a handle that carries a secret is short-lived
            // by construction. A provisioner may ask for less than the
            // host's default; it cannot ask for a standing secret.
            let asked = c["expires_at_ms"].as_i64().unwrap_or(0);
            let ceiling = now_ms() + default_credential_ttl_ms();
            let expires_at_ms = if asked > 0 {
                asked.min(ceiling)
            } else {
                ceiling
            };
            grants.push(CredentialGrant {
                handle: handle.to_owned(),
                virtual_host: virtual_host.to_owned(),
                target_url: target_url.to_owned(),
                header: c["header"].as_str().unwrap_or("Authorization").to_owned(),
                value_prefix: c["value_prefix"].as_str().unwrap_or("Bearer ").to_owned(),
                capability: c["capability"].as_str().unwrap_or("secret.use").to_owned(),
                expires_at_ms,
            });
            if let Some(secret) = c["secret"].as_str() {
                secrets.insert(handle.to_owned(), secret.to_owned());
            }
        }
    }
    (grants, secrets)
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
        network: NetworkPolicy {
            egress,
            credentials: credentials_of(v).0,
        },
        resources,
        browser: spec["browser"].as_bool().unwrap_or(false),
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
    let mut provisioned = st
        .backend
        .provision(&sandbox_id.to_string(), &policy)
        .await?;
    let backend = provisioned.backend;
    let isolated = provisioned.isolated;
    let detail = provisioned.detail.clone();
    // M8.6: the egress broker for this sandbox — the policy's grants and
    // the secrets the provision carried, in memory only.
    st.tenants
        .lock()
        .await
        .insert(sandbox_id.to_string(), tenant);
    let (egress, broker) = match provisioned.egress.take() {
        Some(rx) => {
            let (_, secrets) = credentials_of(&body);
            let broker = modbit_sandbox::egress::EgressBroker::new(
                &sandbox_id.to_string(),
                spec.network.clone(),
                secrets,
                Arc::clone(&st.audit) as Arc<dyn modbit_sandbox::egress::EgressAudit>,
            );
            let task = tokio::spawn(Arc::clone(&broker).serve(rx));
            (Some(broker), Some(task))
        }
        None => (None, None),
    };
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
            policy: policy.clone(),
            broker,
            egress,
        }),
    );
    Ok((
        StatusCode::CREATED,
        Json(json!({
            "sandbox_id": sandbox_id.to_string(),
            "backend": backend,
            "isolated": isolated,
            "policy": {"workspace_root": policy.workspace_root, "protected_paths": policy.protected_paths, "network_interface": policy.network_interface, "egress_proxy": policy.egress_proxy, "egress": policy.egress.iter().map(|r| format!("{}:{}", r.host, r.port)).collect::<Vec<_>>(), "credentials": policy.spec.network.credentials.iter().map(|c| c.virtual_host.clone()).collect::<Vec<_>>(), "browser": policy.browser},
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
            // Text as `content`; bytes that are not UTF-8 as `content_base64`.
            let content = match c["content_base64"].as_str() {
                Some(b) => base64::Engine::decode(&base64::engine::general_purpose::STANDARD, b)
                    .map_err(|e| ApiError::bad(format!("content_base64: {e}")))?,
                None => c["content"]
                    .as_str()
                    .unwrap_or_default()
                    .as_bytes()
                    .to_vec(),
            };
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
        // M8.5: followed processes, PTYs and directory operations.
        "proc.start" => {
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
            let p = link
                .proc_start(
                    &task_id,
                    &effect_id,
                    wire::GuestProcStart {
                        argv,
                        cwd: c["cwd"].as_str().unwrap_or_default().to_owned(),
                        env,
                        timeout_ms: c["timeout_ms"].as_u64().unwrap_or(0),
                        pty: c["pty"].as_bool().unwrap_or(false),
                        cols: c["cols"].as_u64().unwrap_or(120) as u32,
                        rows: c["rows"].as_u64().unwrap_or(40) as u32,
                        stdin_open: c["stdin_open"].as_bool().unwrap_or(false),
                    },
                )
                .await?;
            json!({"kind": "proc.start", "proc_id": p.proc_id, "pid": p.pid})
        }
        "proc.follow" => {
            let proc_id = c["proc_id"]
                .as_str()
                .ok_or_else(|| ApiError::bad("call.proc_id is required"))?;
            let o = link
                .proc_follow(
                    &task_id,
                    proc_id,
                    c["after_cursor"].as_u64().unwrap_or(0),
                    c["max_bytes"].as_u64().unwrap_or(0),
                    c["wait_ms"].as_u64().unwrap_or(1_000),
                )
                .await?;
            json!({
                "kind": "proc.follow", "proc_id": o.proc_id, "data_base64": base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &o.data),
                "cursor": o.cursor, "truncated": o.truncated, "running": o.running, "exit_code": o.exit_code, "timed_out": o.timed_out, "cancelled": o.cancelled, "total_bytes": o.total_bytes,
            })
        }
        "proc.write" => {
            let proc_id = c["proc_id"]
                .as_str()
                .ok_or_else(|| ApiError::bad("call.proc_id is required"))?;
            let data = match c["data_base64"].as_str() {
                Some(b) => base64::Engine::decode(&base64::engine::general_purpose::STANDARD, b)
                    .map_err(|_| ApiError::bad("call.data_base64"))?,
                None => c["data"].as_str().unwrap_or_default().as_bytes().to_vec(),
            };
            link.proc_write(
                &task_id,
                proc_id,
                data,
                c["close_stdin"].as_bool().unwrap_or(false),
            )
            .await?;
            json!({"kind": "proc.write", "proc_id": proc_id})
        }
        "proc.cancel" => {
            let proc_id = c["proc_id"]
                .as_str()
                .ok_or_else(|| ApiError::bad("call.proc_id is required"))?;
            link.proc_cancel(&task_id, proc_id).await?;
            json!({"kind": "proc.cancel", "proc_id": proc_id})
        }
        "pty.resize" => {
            let proc_id = c["proc_id"]
                .as_str()
                .ok_or_else(|| ApiError::bad("call.proc_id is required"))?;
            link.pty_resize(
                &task_id,
                proc_id,
                c["cols"].as_u64().unwrap_or(120) as u32,
                c["rows"].as_u64().unwrap_or(40) as u32,
            )
            .await?;
            json!({"kind": "pty.resize", "proc_id": proc_id})
        }
        "fs.list" => {
            let path = c["path"]
                .as_str()
                .ok_or_else(|| ApiError::bad("call.path is required"))?;
            let l = link
                .list_dir(
                    &task_id,
                    path,
                    c["max_entries"].as_u64().unwrap_or(0) as u32,
                )
                .await?;
            json!({"kind": "fs.list", "entries": l.entries.iter().map(|e| json!({"name": e.name, "kind": e.kind, "size": e.size, "modified_ms": e.modified_ms})).collect::<Vec<_>>(), "truncated": l.truncated})
        }
        "fs.stat" => {
            let path = c["path"]
                .as_str()
                .ok_or_else(|| ApiError::bad("call.path is required"))?;
            let st_ = link.stat(&task_id, path).await?;
            json!({"kind": "fs.stat", "exists": st_.exists, "file_kind": st_.kind, "size": st_.size, "modified_ms": st_.modified_ms, "mode": st_.mode})
        }
        "fs.mkdir" => {
            let path = c["path"]
                .as_str()
                .ok_or_else(|| ApiError::bad("call.path is required"))?;
            link.mkdir(&task_id, &effect_id, path).await?;
            json!({"kind": "fs.mkdir", "path": path})
        }
        "fs.remove" => {
            let path = c["path"]
                .as_str()
                .ok_or_else(|| ApiError::bad("call.path is required"))?;
            link.remove(
                &task_id,
                &effect_id,
                path,
                c["recursive"].as_bool().unwrap_or(false),
            )
            .await?;
            json!({"kind": "fs.remove", "path": path})
        }
        "fs.rename" => {
            let from = c["from"]
                .as_str()
                .ok_or_else(|| ApiError::bad("call.from is required"))?;
            let to = c["to"]
                .as_str()
                .ok_or_else(|| ApiError::bad("call.to is required"))?;
            link.rename(&task_id, &effect_id, from, to).await?;
            json!({"kind": "fs.rename", "from": from, "to": to})
        }
        // M8.9: the guest's worktree, hashed.
        "fs.snapshot" => {
            let root = c["root"].as_str().unwrap_or_default();
            let r = link
                .fs_snapshot(
                    &task_id,
                    root,
                    c["max_entries"].as_u64().unwrap_or(0) as u32,
                )
                .await?;
            json!({"kind": "fs.snapshot", "entries": r.entries.iter().map(|e| json!({"path": e.path, "size": e.size, "sha256": e.sha256})).collect::<Vec<_>>(), "truncated": r.truncated})
        }
        // M8.8: the guest's browser — started on the policy's grant, its
        // DevTools reached only through `/browser/cdp` below.
        "browser.start" => {
            if !live.policy.browser {
                return Err(ApiError::new(
                    StatusCode::FORBIDDEN,
                    "POLICY_DENIED",
                    "the sandbox's policy grants no browser",
                ));
            }
            let width = c["width"].as_u64().unwrap_or(0) as u32;
            let height = c["height"].as_u64().unwrap_or(0) as u32;
            let b = link.browser_start(&task_id, width, height).await?;
            st.store
                .set_sandbox_state(
                    tenant,
                    sandbox,
                    "READY",
                    &format!("{}; browser pid {}", rec.detail, b.pid),
                )
                .await?;
            json!({"kind": "browser.start", "port": b.port, "ws_path": b.ws_path, "pid": b.pid, "already_running": b.already_running, "cdp_path": format!("/v1/sandboxes/{sandbox}/browser/cdp")})
        }
        "browser.stop" => {
            link.browser_stop(&task_id).await?;
            json!({"kind": "browser.stop"})
        }
        other => {
            return Err(ApiError::bad(format!(
                "call.kind `{other}` is not a guest method"
            )));
        }
    };
    Ok(Json(out))
}

/// `GET /v1/sandboxes/{id}/browser/cdp?tenant_id=` (WebSocket; M8.8,
/// docs/22 "Cloud browser"): the browser's DevTools, relayed. The worker's
/// bearer token authenticates the upgrade; the gateway opens a fresh
/// admitted link to the guest, asks it to forward, performs the DevTools
/// WebSocket handshake over the relay and passes messages both ways. The
/// guest's DevTools port is reachable by no other route.
async fn browser_cdp(
    State(st): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Query(q): Query<std::collections::HashMap<String, String>>,
    ws: axum::extract::ws::WebSocketUpgrade,
) -> ApiResult<Response> {
    let w = worker(&st, &headers)?;
    let tenant = q
        .get("tenant_id")
        .and_then(|s| TenantId::parse(s).ok())
        .ok_or_else(|| ApiError::bad("tenant_id is required"))?;
    let sandbox = sandbox_id_of(&id)?;
    let (rec, live) = owned(&st, tenant, &w.worker_id, sandbox).await?;
    if !live.policy.browser {
        return Err(ApiError::new(
            StatusCode::FORBIDDEN,
            "POLICY_DENIED",
            "the sandbox's policy grants no browser",
        ));
    }
    let policy = live.policy.clone();
    let fresh = st.backend.reconnect(&sandbox.to_string()).await?;
    let link: GuestLink<Channel> = GuestLink::admit_image(
        fresh,
        &sandbox.to_string(),
        &policy,
        Duration::from_secs(30),
        st.backend.image(),
    )
    .await?;
    let task_id = rec.task_id.to_string();
    // The browser target's path, from the guest (a start is idempotent:
    // the browser running is reported, one not yet running is started).
    let ws_path = {
        let mut l = live.link.lock().await;
        l.browser_start(&task_id, 0, 0).await?.ws_path
    };
    let (raw, port) = link.browser_forward(&task_id).await?;
    let url = format!("ws://127.0.0.1:{port}{ws_path}");
    let (upstream, _) = tokio_tungstenite::client_async(url.as_str(), raw)
        .await
        .map_err(|e| {
            ApiError::new(
                StatusCode::BAD_GATEWAY,
                "GUEST",
                format!("DevTools handshake: {e}"),
            )
        })?;
    Ok(ws.on_upgrade(move |socket| relay_cdp(socket, upstream)))
}

async fn relay_cdp<S>(
    client: axum::extract::ws::WebSocket,
    upstream: tokio_tungstenite::WebSocketStream<S>,
) where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    use axum::extract::ws::Message as A;
    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::tungstenite::Message as T;
    let (mut c_tx, mut c_rx) = client.split();
    let (mut u_tx, mut u_rx) = upstream.split();
    let down = async {
        while let Some(Ok(m)) = u_rx.next().await {
            let out = match m {
                T::Text(t) => A::Text(t.as_str().into()),
                T::Binary(b) => A::Binary(b),
                T::Close(_) => break,
                _ => continue,
            };
            if c_tx.send(out).await.is_err() {
                break;
            }
        }
    };
    let up = async {
        while let Some(Ok(m)) = c_rx.next().await {
            let out = match m {
                A::Text(t) => T::Text(t.as_str().into()),
                A::Binary(b) => T::Binary(b),
                A::Close(_) => break,
                _ => continue,
            };
            if u_tx.send(out).await.is_err() {
                break;
            }
        }
    };
    tokio::select! {
        _ = down => {}
        _ = up => {}
    }
}

/// `GET /v1/sandboxes/{id}/egress?tenant_id=`: the broker's audit for a
/// sandbox — every admission and refusal (M8.6).
async fn egress(
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
    let Some(_rec) = st.store.sandbox(tenant, sandbox).await? else {
        st.store
            .record_denial(
                Some(tenant),
                None,
                &format!("sandbox:{sandbox}"),
                &format!(
                    "worker {} of tenant {tenant} read the egress audit of a sandbox that is not the tenant's",
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
    let live = st.audit.records(&sandbox.to_string());
    let stored = st.store.egress_audit(tenant, sandbox).await?;
    Ok(Json(json!({
        "sandbox_id": sandbox.to_string(),
        "records": live.iter().map(|r| json!({"kind": r.kind, "destination": r.destination, "allowed": r.allowed, "capability": r.capability, "detail": r.detail, "at_ms": r.at_ms})).collect::<Vec<_>>(),
        "stored": stored.len(),
    })))
}

/// `POST /v1/sandboxes/{id}/credentials {tenant_id, handle, secret, expires_at_ms}`
/// (M9.3, REQ-EV-0288): hand a live sandbox a fresh secret and lifetime for
/// a handle it already grants. Not journaled and never echoed: the value
/// crosses once, is held in the broker's memory and reaches no guest. A
/// handle the sandbox does not grant is refused — this renews, it never
/// widens what the sandbox may reach.
async fn renew_credential(
    State(st): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(body): Json<Value>,
) -> ApiResult<Json<Value>> {
    let w = worker(&st, &headers)?;
    let tenant = tenant_of(&body)?;
    let sandbox = sandbox_id_of(&id)?;
    let (_, live) = owned(&st, tenant, &w.worker_id, sandbox).await?;
    let handle = body["handle"]
        .as_str()
        .unwrap_or_default()
        .trim()
        .to_owned();
    let secret = body["secret"].as_str().unwrap_or_default().to_owned();
    if handle.is_empty() || secret.is_empty() {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "BAD_REQUEST",
            "handle and secret are required",
        ));
    }
    let asked = body["expires_at_ms"].as_i64().unwrap_or(0);
    let ceiling = now_ms() + default_credential_ttl_ms();
    let expires_at_ms = if asked > 0 {
        asked.min(ceiling)
    } else {
        ceiling
    };
    let Some(broker) = &live.egress else {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            "NO_EGRESS_BROKER",
            "this sandbox has no egress broker",
        ));
    };
    if !broker.renew(&handle, secret, expires_at_ms) {
        return Err(ApiError::new(
            StatusCode::NOT_FOUND,
            "UNKNOWN_CREDENTIAL_HANDLE",
            format!("sandbox {sandbox} grants no credential under `{handle}`"),
        ));
    }
    Ok(Json(
        json!({"sandbox_id": sandbox.to_string(), "handle": handle, "expires_at_ms": expires_at_ms}),
    ))
}

/// `POST /v1/sandboxes/{id}/relink {tenant_id}`: replace a lost link to a
/// live guest — a fresh channel from the backend, admitted anew (a new
/// credential); the guest's processes and their output are untouched.
async fn relink(
    State(st): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(body): Json<Value>,
) -> ApiResult<Json<Value>> {
    let w = worker(&st, &headers)?;
    let tenant = tenant_of(&body)?;
    let sandbox = sandbox_id_of(&id)?;
    let (rec, live) = owned(&st, tenant, &w.worker_id, sandbox).await?;
    let policy = live.policy.clone();
    // M8.9 (docs/21 "Sandbox recovery"): a guest that cannot be reached
    // again is lost — recorded so, its remains torn down, `410
    // SANDBOX_GONE` from here on; the worker's Core replaces it.
    let admitted = async {
        let fresh = tokio::time::timeout(
            Duration::from_secs(20),
            st.backend.reconnect(&sandbox.to_string()),
        )
        .await
        .map_err(|_| SandboxError::Guest("the guest did not accept a link within 20 s".into()))??;
        GuestLink::admit_image(
            fresh,
            &sandbox.to_string(),
            &policy,
            Duration::from_secs(30),
            st.backend.image(),
        )
        .await
    }
    .await;
    let new_link: GuestLink<Channel> = match admitted {
        Ok(l) => l,
        Err(e) => {
            st.live.lock().await.remove(&sandbox);
            if let Some(b) = &live.broker {
                b.abort();
            }
            let _ = st.backend.destroy(&sandbox.to_string()).await;
            st.store
                .set_sandbox_state(
                    tenant,
                    sandbox,
                    "LOST",
                    &format!("{}; relink failed: {e}", rec.detail),
                )
                .await?;
            return Err(ApiError::new(
                StatusCode::GONE,
                "SANDBOX_GONE",
                format!("sandbox {sandbox} is lost: {e}"),
            ));
        }
    };
    let boot_id = new_link.hello.boot_id.clone();
    *live.link.lock().await = new_link;
    Ok(Json(
        json!({"sandbox_id": sandbox.to_string(), "state": rec.state, "boot_id": boot_id, "same_boot": boot_id == live.hello.boot_id}),
    ))
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
    let (rec, live) = owned(&st, tenant, &w.worker_id, sandbox).await?;
    st.live.lock().await.remove(&sandbox);
    if let Some(b) = &live.broker {
        b.abort();
    }
    // M8.8: the guest's browser goes with the guest — asked to stop first,
    // so a reference guest leaves no orphan Chromium behind (a MicroVM's
    // dies with the VM regardless).
    if live.policy.browser
        && let Ok(mut link) = tokio::time::timeout(Duration::from_secs(2), live.link.lock()).await
    {
        let _ = tokio::time::timeout(
            Duration::from_secs(3),
            link.browser_stop(&rec.task_id.to_string()),
        )
        .await;
    }
    st.backend.destroy(&sandbox.to_string()).await?;
    st.store
        .set_sandbox_state(tenant, sandbox, "DESTROYED", &rec.detail)
        .await?;
    Ok(Json(
        json!({"sandbox_id": sandbox.to_string(), "state": "DESTROYED"}),
    ))
}
