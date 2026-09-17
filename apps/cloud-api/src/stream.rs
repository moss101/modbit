//! WSS `/v1/stream` (docs/30): an authenticated subscription with a resume
//! cursor — the events after the cursor are replayed first, then every
//! committed event as it lands (Postgres notifies on append). A client that
//! loses the socket reconnects with the last cursor it saw; task execution
//! never depends on the socket (docs/24 "Sync model").

use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Query, State};
use axum::response::Response;
use modbit_domain::SessionId;
use serde_json::json;

use crate::AppState;
use crate::routes::Caller;

#[derive(serde::Deserialize)]
pub(crate) struct StreamQuery {
    session_id: String,
    #[serde(default)]
    after: u64,
}

pub(crate) async fn stream(
    State(state): State<Arc<AppState>>,
    ext: axum::Extension<Caller>,
    Query(q): Query<StreamQuery>,
    ws: WebSocketUpgrade,
) -> Response {
    let principal = ext.0.0.clone();
    ws.on_upgrade(move |socket| run(socket, state, principal, q))
}

async fn run(
    mut socket: WebSocket,
    state: Arc<AppState>,
    principal: modbit_event_store::cloud::Principal,
    q: StreamQuery,
) {
    let Ok(sid) = SessionId::parse(&q.session_id) else {
        let _ = socket
            .send(Message::Text(
                json!({"error": {"code": "BAD_PAYLOAD", "message": "session_id must be a uuid"}})
                    .to_string()
                    .into(),
            ))
            .await;
        return;
    };
    match state.store.session(principal.tenant_id, sid).await {
        Ok(Some(_)) => {}
        _ => {
            let _ = state
                .store
                .record_denial(
                    Some(principal.tenant_id),
                    Some(principal.principal_id),
                    &format!("session:{sid}"),
                    "not in tenant",
                )
                .await;
            let _ = socket
                .send(Message::Text(
                    json!({"error": {"code": "NOT_FOUND", "message": format!("session {sid}")}})
                        .to_string()
                        .into(),
                ))
                .await;
            return;
        }
    }
    let mut cursor = q.after;
    let mut notes = state.notify.subscribe();
    // Replay, then live.
    loop {
        match state
            .store
            .events_after(principal.tenant_id, sid, cursor, 500)
            .await
        {
            Ok(batch) => {
                if batch.is_empty() {
                    break;
                }
                for e in batch {
                    cursor = e.session_offset;
                    if socket
                        .send(Message::Text(json!({"event": e}).to_string().into()))
                        .await
                        .is_err()
                    {
                        return;
                    }
                }
            }
            Err(e) => {
                let _ = socket
                    .send(Message::Text(
                        json!({"error": {"code": "STORE", "message": e.to_string()}})
                            .to_string()
                            .into(),
                    ))
                    .await;
                return;
            }
        }
    }
    if socket
        .send(Message::Text(
            json!({"caught_up": cursor}).to_string().into(),
        ))
        .await
        .is_err()
    {
        return;
    }
    loop {
        tokio::select! {
            note = notes.recv() => {
                match note {
                    Ok((s, _)) if s == sid => {}
                    Ok(_) => continue,
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                    Err(_) => return,
                }
                loop {
                    let batch = match state.store.events_after(principal.tenant_id, sid, cursor, 500).await {
                        Ok(b) => b,
                        Err(_) => return,
                    };
                    if batch.is_empty() {
                        break;
                    }
                    for e in batch {
                        cursor = e.session_offset;
                        if socket.send(Message::Text(json!({"event": e}).to_string().into())).await.is_err() {
                            return;
                        }
                    }
                }
            }
            incoming = socket.recv() => {
                match incoming {
                    Some(Ok(Message::Close(_))) | None | Some(Err(_)) => return,
                    Some(Ok(Message::Ping(p))) => {
                        if socket.send(Message::Pong(p)).await.is_err() { return; }
                    }
                    Some(Ok(_)) => {}
                }
            }
            () = tokio::time::sleep(std::time::Duration::from_secs(20)) => {
                if socket.send(Message::Ping(Vec::new().into())).await.is_err() { return; }
            }
        }
    }
}
