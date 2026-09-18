//! The person's view of a cloud browser (M8.8, docs/22 "Cloud browser",
//! docs/24): a worker keeps one outbound link to the API (`GET
//! /v1/workers/link`, a WebSocket it opens with its worker token — the
//! worker needs no inbound port), over which the API asks it to watch a
//! browser session's view, to apply the person's input, or to hand
//! control, and over which the worker sends the view's frames. A person
//! watches through `GET /v1/browser-sessions/{id}/stream?session_id=`
//! (WSS): the API finds the worker holding the session's lease, asks it to
//! watch, and relays every frame; `POST /v1/browser-sessions/{id}:input`
//! and `:control` go the same way and answer with the worker's outcome.
//! Frames are JPEG bytes of the page as it is (untrusted content); nothing
//! is stored.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use axum::Json;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use modbit_domain::SessionId;
use serde_json::{Value, json};
use tokio::sync::{Mutex, broadcast, mpsc, oneshot};

use crate::AppState;
use crate::routes::{ApiError, ApiResult, Caller};

/// One worker's link.
pub struct WorkerLink {
    /// The worker.
    pub worker_id: String,
    /// API → worker messages.
    tx: mpsc::Sender<Value>,
    /// Answers awaited by request id.
    pending: Mutex<HashMap<String, oneshot::Sender<Value>>>,
}

/// The links, by worker id, and the frames they send.
pub struct WorkerLinks {
    links: Mutex<HashMap<String, Arc<WorkerLink>>>,
    /// `(browser_session_id, frame)`.
    frames: broadcast::Sender<(String, Arc<Value>)>,
}

impl Default for WorkerLinks {
    fn default() -> Self {
        let (frames, _) = broadcast::channel(256);
        Self {
            links: Mutex::new(HashMap::new()),
            frames,
        }
    }
}

impl WorkerLinks {
    /// The link of `worker_id`, if it is connected.
    pub async fn get(&self, worker_id: &str) -> Option<Arc<WorkerLink>> {
        self.links.lock().await.get(worker_id).cloned()
    }

    /// Whether `worker_id` is linked.
    pub async fn linked(&self, worker_id: &str) -> bool {
        self.links.lock().await.contains_key(worker_id)
    }

    /// Drop `worker_id`'s link (an operator's revocation of the live link;
    /// the worker may link again with a token that still verifies).
    pub async fn disconnect(&self, worker_id: &str) -> bool {
        self.links.lock().await.remove(worker_id).is_some()
    }
}

impl WorkerLink {
    /// Ask the worker and wait for its answer (bounded).
    pub async fn ask(&self, mut msg: Value, wait: Duration) -> Result<Value, ApiError> {
        let request_id = uuid::Uuid::now_v7().to_string();
        msg["request_id"] = json!(request_id);
        let (tx, rx) = oneshot::channel();
        self.pending.lock().await.insert(request_id.clone(), tx);
        if self.tx.send(msg).await.is_err() {
            self.pending.lock().await.remove(&request_id);
            return Err(ApiError::new(
                StatusCode::SERVICE_UNAVAILABLE,
                "WORKER_UNLINKED",
                format!("worker {} dropped its link", self.worker_id),
            ));
        }
        match tokio::time::timeout(wait, rx).await {
            Ok(Ok(v)) => Ok(v),
            Ok(Err(_)) => Err(ApiError::new(
                StatusCode::SERVICE_UNAVAILABLE,
                "WORKER_UNLINKED",
                format!("worker {} dropped its link", self.worker_id),
            )),
            Err(_) => {
                self.pending.lock().await.remove(&request_id);
                Err(ApiError::new(
                    StatusCode::GATEWAY_TIMEOUT,
                    "WORKER_TIMEOUT",
                    format!("worker {} did not answer in time", self.worker_id),
                ))
            }
        }
    }

    /// Tell the worker (no answer awaited).
    pub async fn tell(&self, msg: Value) {
        let _ = self.tx.send(msg).await;
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// `GET /v1/workers/link` (WebSocket; worker bearer token): the worker's
/// outbound link. One per worker id; a new one replaces the old.
pub(crate) async fn worker_link(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> ApiResult<Response> {
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
    let claims = state.worker_key.verify(token, now_ms()).ok_or_else(|| {
        ApiError::new(
            StatusCode::UNAUTHORIZED,
            "UNAUTHENTICATED",
            "the worker token does not verify or has expired",
        )
    })?;
    let worker_id = claims.worker_id;
    Ok(ws.on_upgrade(move |socket| run_worker_link(socket, state, worker_id)))
}

async fn run_worker_link(mut socket: WebSocket, state: Arc<AppState>, worker_id: String) {
    let (tx, mut rx) = mpsc::channel::<Value>(64);
    let link = Arc::new(WorkerLink {
        worker_id: worker_id.clone(),
        tx,
        pending: Mutex::new(HashMap::new()),
    });
    state
        .workers
        .links
        .lock()
        .await
        .insert(worker_id.clone(), Arc::clone(&link));
    eprintln!("modbit-cloud-api: worker {worker_id} linked");
    // The link lives while the map holds it: a disconnect (or a newer
    // link of the same worker) ends this one.
    let mut alive = tokio::time::interval(Duration::from_millis(500));
    loop {
        tokio::select! {
            _ = alive.tick() => {
                let held = state
                    .workers
                    .links
                    .lock()
                    .await
                    .get(&worker_id)
                    .is_some_and(|l| Arc::ptr_eq(l, &link));
                if !held {
                    break;
                }
            }
            out = rx.recv() => {
                let Some(m) = out else { break };
                if socket.send(Message::Text(m.to_string().into())).await.is_err() {
                    break;
                }
            }
            inc = socket.recv() => {
                let Some(Ok(m)) = inc else { break };
                let text = match m {
                    Message::Text(t) => t.to_string(),
                    Message::Close(_) => break,
                    _ => continue,
                };
                let Ok(v) = serde_json::from_str::<Value>(&text) else { continue };
                match v["kind"].as_str() {
                    Some("ack") => {
                        if let Some(id) = v["request_id"].as_str()
                            && let Some(w) = link.pending.lock().await.remove(id)
                        {
                            let _ = w.send(v.clone());
                        }
                    }
                    Some("frame") => {
                        if let Some(b) = v["browser_session_id"].as_str() {
                            let _ = state.workers.frames.send((b.to_owned(), Arc::new(v.clone())));
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    let mut links = state.workers.links.lock().await;
    if links.get(&worker_id).is_some_and(|l| Arc::ptr_eq(l, &link)) {
        links.remove(&worker_id);
    }
    eprintln!("modbit-cloud-api: worker {worker_id} unlinked");
}

/// The worker holding `session`'s lease, linked.
async fn holder_link(
    state: &AppState,
    p: &modbit_event_store::cloud::Principal,
    session: SessionId,
) -> ApiResult<Arc<WorkerLink>> {
    match state.store.session(p.tenant_id, session).await? {
        Some(_) => {}
        None => {
            let _ = state
                .store
                .record_denial(
                    Some(p.tenant_id),
                    Some(p.principal_id),
                    &format!("session:{session}"),
                    "not in tenant",
                )
                .await;
            return Err(ApiError::not_found(format!("session {session}")));
        }
    }
    let Some((worker, _generation)) = state.store.held_by(p.tenant_id, session).await? else {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            "NOT_HOSTED",
            format!("no worker holds session {session}; nothing runs its browser"),
        ));
    };
    state.workers.get(&worker).await.ok_or_else(|| {
        ApiError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "WORKER_UNLINKED",
            format!("worker {worker} holds the session but has no link to this API"),
        )
    })
}

#[derive(serde::Deserialize)]
pub(crate) struct ViewQuery {
    session_id: String,
    #[serde(default)]
    max_width: u32,
    #[serde(default)]
    max_height: u32,
    #[serde(default)]
    quality: u32,
}

/// `GET /v1/browser-sessions/{id}/stream?session_id=` (WSS): the view's
/// frames as `{"frame": {...}}` text messages (JPEG base64 inside), after
/// a `{"watching": {...}}` opener; an error closes the socket.
pub(crate) async fn browser_stream(
    State(state): State<Arc<AppState>>,
    ext: axum::Extension<Caller>,
    Path(bsid): Path<String>,
    Query(q): Query<ViewQuery>,
    ws: WebSocketUpgrade,
) -> Response {
    let principal = ext.0.0.clone();
    ws.on_upgrade(move |socket| run_stream(socket, state, principal, bsid, q))
}

async fn run_stream(
    mut socket: WebSocket,
    state: Arc<AppState>,
    principal: modbit_event_store::cloud::Principal,
    bsid: String,
    q: ViewQuery,
) {
    async fn send_err(socket: &mut WebSocket, e: &ApiError) {
        let m = json!({"error": {"code": e.code, "message": e.message}}).to_string();
        let _ = socket.send(Message::Text(m.into())).await;
    }
    let Ok(sid) = SessionId::parse(&q.session_id) else {
        send_err(&mut socket, &ApiError::bad("session_id must be a uuid")).await;
        return;
    };
    let link = match holder_link(&state, &principal, sid).await {
        Ok(l) => l,
        Err(e) => {
            send_err(&mut socket, &e).await;
            return;
        }
    };
    let mut frames = state.workers.frames.subscribe();
    let watched = link
        .ask(
            json!({"kind": "watch", "tenant_id": principal.tenant_id.to_string(), "session_id": sid.to_string(), "browser_session_id": bsid, "max_width": q.max_width, "max_height": q.max_height, "quality": q.quality}),
            Duration::from_secs(20),
        )
        .await;
    let watched = match watched {
        Ok(v) if v["status"] == "ACCEPTED" => v,
        Ok(v) => {
            let e = ApiError::new(
                StatusCode::CONFLICT,
                "WATCH_REFUSED",
                format!(
                    "{}: {}",
                    v["code"].as_str().unwrap_or("REFUSED"),
                    v["message"].as_str().unwrap_or_default()
                ),
            );
            send_err(&mut socket, &e).await;
            return;
        }
        Err(e) => {
            send_err(&mut socket, &e).await;
            return;
        }
    };
    if socket
        .send(Message::Text(
            json!({"watching": watched["result"]}).to_string().into(),
        ))
        .await
        .is_err()
    {
        link.tell(
            json!({"kind": "unwatch", "session_id": sid.to_string(), "browser_session_id": bsid}),
        )
        .await;
        return;
    }
    loop {
        tokio::select! {
            f = frames.recv() => {
                match f {
                    Ok((b, frame)) if b == bsid => {
                        if socket.send(Message::Text(json!({"frame": &*frame}).to_string().into())).await.is_err() {
                            break;
                        }
                    }
                    Ok(_) => continue,
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(_) => break,
                }
            }
            inc = socket.recv() => {
                match inc {
                    Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break,
                    _ => continue,
                }
            }
        }
    }
    link.tell(
        json!({"kind": "unwatch", "session_id": sid.to_string(), "browser_session_id": bsid}),
    )
    .await;
}

fn outcome(v: Value) -> ApiResult<(StatusCode, Json<Value>)> {
    if v["status"] == "ACCEPTED" {
        Ok((StatusCode::OK, Json(v["result"].clone())))
    } else {
        Ok((
            StatusCode::CONFLICT,
            Json(json!({"code": v["code"], "message": v["message"], "status": 409})),
        ))
    }
}

/// `POST /v1/browser-sessions/{id}:input {session_id, kind, x, y, button,
/// text, key, delta_x, delta_y, modifiers}`: the person's input into the
/// view, applied by the worker's Core under the control lease — refused
/// `AGENT_ACTIVE` while the agent holds it.
pub(crate) async fn browser_input(
    State(state): State<Arc<AppState>>,
    ext: axum::Extension<Caller>,
    Path(bsid): Path<String>,
    Json(body): Json<Value>,
) -> ApiResult<(StatusCode, Json<Value>)> {
    let p = ext.0.0.clone();
    let sid = body["session_id"]
        .as_str()
        .and_then(|s| SessionId::parse(s).ok())
        .ok_or_else(|| ApiError::bad("session_id is required"))?;
    let link = holder_link(&state, &p, sid).await?;
    let v = link
        .ask(
            json!({"kind": "input", "tenant_id": p.tenant_id.to_string(), "session_id": sid.to_string(), "browser_session_id": bsid, "input": body}),
            Duration::from_secs(15),
        )
        .await?;
    outcome(v)
}

/// `POST /v1/browser-sessions/{id}:control {session_id, controller}`: hand
/// control of the session to the person (`USER`) or the agent (`AGENT`).
pub(crate) async fn browser_control(
    State(state): State<Arc<AppState>>,
    ext: axum::Extension<Caller>,
    Path(bsid): Path<String>,
    Json(body): Json<Value>,
) -> ApiResult<(StatusCode, Json<Value>)> {
    let p = ext.0.0.clone();
    let sid = body["session_id"]
        .as_str()
        .and_then(|s| SessionId::parse(s).ok())
        .ok_or_else(|| ApiError::bad("session_id is required"))?;
    let controller = body["controller"].as_str().unwrap_or_default();
    if controller != "USER" && controller != "AGENT" {
        return Err(ApiError::bad("controller must be USER or AGENT"));
    }
    let link = holder_link(&state, &p, sid).await?;
    let v = link
        .ask(
            json!({"kind": "control", "tenant_id": p.tenant_id.to_string(), "session_id": sid.to_string(), "browser_session_id": bsid, "controller": controller}),
            Duration::from_secs(15),
        )
        .await?;
    outcome(v)
}
