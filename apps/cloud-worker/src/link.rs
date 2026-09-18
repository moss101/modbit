//! The worker's outbound link to the Cloud API (M8.8, docs/22 "Cloud
//! browser", docs/24): one WebSocket the worker opens with its worker
//! token (no inbound port on the worker), kept open with reconnects, over
//! which the API asks for a browser session's view (`watch` / `unwatch`),
//! forwards the person's input (`input`) and control hand-overs
//! (`control`), and over which this worker sends the view's frames. Every
//! request names a session; the worker acts only on a session it hosts,
//! through that session's own Core.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use base64::Engine;
use futures_util::{SinkExt, StreamExt};
use modbit_domain::SessionId;
use modbit_protocol::client::Client;
use modbit_protocol::local::{Endpoint, decode_hex};
use modbit_protocol::v1::{self as wire, ClientKind};
use prost::Message;
use serde_json::{Value, json};
use tokio::sync::mpsc;

use crate::Config;
use crate::session::{envelope, id_of};

/// A hosted session's Core, as the link reaches it.
#[derive(Clone)]
pub(crate) struct LiveCore {
    pub endpoint: Endpoint,
    pub boot_secret_hex: String,
    /// The local session lease generation this worker holds.
    pub generation: u64,
}

/// The Cores of the sessions this worker hosts.
pub(crate) type Cores = Arc<std::sync::Mutex<HashMap<SessionId, LiveCore>>>;

async fn client_for(core: &LiveCore) -> anyhow::Result<Client> {
    let secret =
        decode_hex(&core.boot_secret_hex).ok_or_else(|| anyhow::anyhow!("bad ready line"))?;
    Ok(Client::connect(
        &core.endpoint,
        &secret,
        ClientKind::CloudWorker,
        concat!("modbit-cloud-worker ", env!("CARGO_PKG_VERSION")),
    )
    .await?)
}

/// Keep the link up until stopped.
pub(crate) async fn run(
    cfg: Arc<Config>,
    cores: Cores,
    mut stop: tokio::sync::watch::Receiver<bool>,
) {
    let Some(api) = cfg.api.clone() else {
        return;
    };
    let mut backoff = Duration::from_millis(500);
    loop {
        if *stop.borrow() {
            return;
        }
        match connect(&api).await {
            Ok(ws) => {
                backoff = Duration::from_millis(500);
                eprintln!(
                    "modbit-cloud-worker[{}]: linked to the Cloud API at {}",
                    cfg.worker_id, api.base_url
                );
                serve(ws, &cfg, &cores, &mut stop).await;
                eprintln!(
                    "modbit-cloud-worker[{}]: the Cloud API link ended",
                    cfg.worker_id
                );
            }
            Err(e) => {
                eprintln!(
                    "modbit-cloud-worker[{}]: Cloud API link: {e}; retrying in {backoff:?}",
                    cfg.worker_id
                );
            }
        }
        tokio::select! {
            _ = tokio::time::sleep(backoff) => {}
            _ = stop.changed() => { if *stop.borrow() { return; } }
        }
        backoff = (backoff * 2).min(Duration::from_secs(10));
    }
}

type Ws =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

async fn connect(api: &crate::ApiLinkConfig) -> anyhow::Result<Ws> {
    let base = api.base_url.trim_end_matches('/');
    let ws_base = if let Some(rest) = base.strip_prefix("https://") {
        format!("wss://{rest}")
    } else if let Some(rest) = base.strip_prefix("http://") {
        format!("ws://{rest}")
    } else {
        base.to_owned()
    };
    use tokio_tungstenite::tungstenite::client::IntoClientRequest;
    let mut req = format!("{ws_base}/v1/workers/link").into_client_request()?;
    req.headers_mut().insert(
        "authorization",
        format!("Bearer {}", api.worker_token).parse()?,
    );
    let (ws, _) = tokio_tungstenite::connect_async(req).await?;
    Ok(ws)
}

fn ack(request_id: &str, result: Result<Value, (String, String)>) -> Value {
    match result {
        Ok(v) => {
            json!({"kind": "ack", "request_id": request_id, "status": "ACCEPTED", "result": v})
        }
        Err((code, message)) => {
            json!({"kind": "ack", "request_id": request_id, "status": "REJECTED", "code": code, "message": message})
        }
    }
}

fn client_err(e: modbit_protocol::client::ClientError) -> (String, String) {
    match e {
        modbit_protocol::client::ClientError::Rejected { code, message } => (code, message),
        other => ("CORE".into(), other.to_string()),
    }
}

async fn serve(
    ws: Ws,
    cfg: &Arc<Config>,
    cores: &Cores,
    stop: &mut tokio::sync::watch::Receiver<bool>,
) {
    let (mut sink, mut source) = ws.split();
    let (out_tx, mut out_rx) = mpsc::channel::<Value>(64);
    // Watchers by (session, browser session): each holds its own client.
    let mut watchers: HashMap<(SessionId, String), tokio::task::JoinHandle<()>> = HashMap::new();
    loop {
        tokio::select! {
            m = out_rx.recv() => {
                let Some(m) = m else { break };
                if sink.send(tokio_tungstenite::tungstenite::Message::Text(m.to_string().into())).await.is_err() {
                    break;
                }
            }
            inc = source.next() => {
                let text = match inc {
                    Some(Ok(tokio_tungstenite::tungstenite::Message::Text(t))) => t.to_string(),
                    Some(Ok(tokio_tungstenite::tungstenite::Message::Close(_))) | None | Some(Err(_)) => break,
                    Some(Ok(_)) => continue,
                };
                let Ok(v) = serde_json::from_str::<Value>(&text) else { continue };
                let request_id = v["request_id"].as_str().unwrap_or_default().to_owned();
                let Some(sid) = v["session_id"].as_str().and_then(|s| SessionId::parse(s).ok()) else {
                    if !request_id.is_empty() {
                        let _ = out_tx.send(ack(&request_id, Err(("BAD_PAYLOAD".into(), "session_id".into())))).await;
                    }
                    continue;
                };
                let bsid = v["browser_session_id"].as_str().unwrap_or_default().to_owned();
                let core = cores.lock().ok().and_then(|m| m.get(&sid).cloned());
                let Some(core) = core else {
                    if !request_id.is_empty() {
                        let _ = out_tx.send(ack(&request_id, Err(("NOT_HOSTED".into(), format!("session {sid} is not hosted by worker {}", cfg.worker_id))))).await;
                    }
                    continue;
                };
                let Some(bsid_bytes): Option<[u8; 16]> = decode_hex(&bsid).and_then(|b| b.try_into().ok()) else {
                    if !request_id.is_empty() {
                        let _ = out_tx.send(ack(&request_id, Err(("BAD_PAYLOAD".into(), "browser_session_id".into())))).await;
                    }
                    continue;
                };
                match v["kind"].as_str() {
                    Some("watch") => {
                        if let Some(h) = watchers.remove(&(sid, bsid.clone())) {
                            h.abort();
                        }
                        let out = out_tx.clone();
                        let rid = request_id.clone();
                        let bsid2 = bsid.clone();
                        let max_width = v["max_width"].as_u64().unwrap_or(0) as u32;
                        let max_height = v["max_height"].as_u64().unwrap_or(0) as u32;
                        let quality = v["quality"].as_u64().unwrap_or(0) as u32;
                        let h = tokio::spawn(async move {
                            let mut c = match client_for(&core).await {
                                Ok(c) => c,
                                Err(e) => {
                                    let _ = out.send(ack(&rid, Err(("CORE".into(), e.to_string())))).await;
                                    return;
                                }
                            };
                            let watched = c
                                .command(envelope(
                                    "WatchBrowserView",
                                    wire::WatchBrowserView {
                                        browser_session_id: Some(id_of(&bsid_bytes)),
                                        max_width,
                                        max_height,
                                        quality,
                                    }
                                    .encode_to_vec(),
                                    None,
                                ))
                                .await
                                .and_then(|a| Client::result::<wire::BrowserViewWatched>(&a));
                            match watched {
                                Ok(w) => {
                                    let _ = out.send(ack(&rid, Ok(json!({"browser_session_id": bsid2, "host_kind": w.host_kind, "controller": w.controller, "lease_generation": w.lease_generation})))).await;
                                }
                                Err(e) => {
                                    let _ = out.send(ack(&rid, Err(client_err(e)))).await;
                                    return;
                                }
                            }
                            while let Ok(Some(f)) = c.next_browser_frame().await {
                                let frame = json!({
                                    "kind": "frame",
                                    "browser_session_id": bsid2,
                                    "seq": f.seq,
                                    "jpeg_base64": base64::engine::general_purpose::STANDARD.encode(&f.jpeg),
                                    "width": f.width,
                                    "height": f.height,
                                    "page_width": f.page_width,
                                    "page_height": f.page_height,
                                    "url": f.url,
                                    "title": f.title,
                                });
                                // A slow link drops frames rather than queueing them.
                                if out.try_send(frame).is_err() && out.is_closed() {
                                    break;
                                }
                            }
                        });
                        watchers.insert((sid, bsid), h);
                    }
                    Some("unwatch") => {
                        if let Some(h) = watchers.remove(&(sid, bsid)) {
                            h.abort();
                        }
                    }
                    Some("input") => {
                        let i = &v["input"];
                        let out = out_tx.clone();
                        let rid = request_id.clone();
                        let payload = wire::BrowserViewInput {
                            browser_session_id: Some(id_of(&bsid_bytes)),
                            kind: i["kind"].as_str().unwrap_or_default().to_owned(),
                            x: i["x"].as_f64().unwrap_or(0.0),
                            y: i["y"].as_f64().unwrap_or(0.0),
                            button: i["button"].as_str().unwrap_or_default().to_owned(),
                            text: i["text"].as_str().unwrap_or_default().to_owned(),
                            key: i["key"].as_str().unwrap_or_default().to_owned(),
                            delta_x: i["delta_x"].as_i64().unwrap_or(0) as i32,
                            delta_y: i["delta_y"].as_i64().unwrap_or(0) as i32,
                            modifiers: i["modifiers"].as_u64().unwrap_or(0) as u32,
                        };
                        tokio::spawn(async move {
                            let r = async {
                                let mut c = client_for(&core).await.map_err(|e| ("CORE".to_owned(), e.to_string()))?;
                                let a = c
                                    .command(envelope("BrowserViewInput", payload.encode_to_vec(), None))
                                    .await
                                    .and_then(|a| Client::result::<wire::BrowserViewInputDelivered>(&a))
                                    .map_err(client_err)?;
                                Ok(json!({"delivered": a.delivered, "detail": a.detail}))
                            }
                            .await;
                            let _ = out.send(ack(&rid, r)).await;
                        });
                    }
                    Some("control") => {
                        let controller = v["controller"].as_str().unwrap_or_default().to_owned();
                        let out = out_tx.clone();
                        let rid = request_id.clone();
                        tokio::spawn(async move {
                            let r = async {
                                let mut c = client_for(&core).await.map_err(|e| ("CORE".to_owned(), e.to_string()))?;
                                let a = c
                                    .command(envelope(
                                        "SetBrowserControl",
                                        wire::SetBrowserControl {
                                            browser_session_id: Some(id_of(&bsid_bytes)),
                                            controller,
                                            task_id: None,
                                        }
                                        .encode_to_vec(),
                                        Some(core.generation),
                                    ))
                                    .await
                                    .and_then(|a| Client::result::<wire::BrowserControlChanged>(&a))
                                    .map_err(client_err)?;
                                Ok(json!({"controller": a.controller, "lease_generation": a.lease_generation, "changed": a.changed, "offset": a.offset}))
                            }
                            .await;
                            let _ = out.send(ack(&rid, r)).await;
                        });
                    }
                    _ => {
                        if !request_id.is_empty() {
                            let _ = out_tx.send(ack(&request_id, Err(("BAD_PAYLOAD".into(), "kind".into())))).await;
                        }
                    }
                }
            }
            _ = stop.changed() => {
                if *stop.borrow() { break; }
            }
        }
    }
    for (_, h) in watchers.drain() {
        h.abort();
    }
    let _ = sink.close().await;
}
