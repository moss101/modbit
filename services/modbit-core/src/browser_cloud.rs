//! The cloud browser host (M8.8, docs/22 "Cloud browser"): for a task
//! under `cloud_isolated`, the Core is the browser host itself — the
//! Browser Bridge speaks CDP to the Chromium inside the task's sandbox
//! over the gateway's authenticated DevTools relay, answering the same
//! [`HostRequest`]s the desktop's Electron host answers for a local
//! session (navigate, state, snapshot, capture, act, isolation, close).
//! The person's view is the page's screencast, relayed to every surface
//! connection watching the session (`WatchBrowserView`), and the person's
//! input goes in under the control lease (`BrowserViewInput`, only while
//! they hold it) — structural and visual control share the one session.
//!
//! Nothing here reaches the sandbox by any path but the gateway: the guest's
//! DevTools port is on its own loopback, the relay is one admitted link the
//! gateway turned into a byte pipe.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use base64::Engine;
use modbit_browser::compiler::RawAxNode;
use modbit_browser::{BrowserSessionId, Clip, HostRequest, HostResponse, PageState};
use modbit_domain::event::{Actor, AggregateType};
use modbit_domain::task::{Task, TaskEvent};
use modbit_protocol::v1 as wire;
use modbit_sandbox::client::SandboxHandle;
use modbit_sandbox::port::{CdpTransport, SandboxPort};
use serde_json::{Value, json};
use tokio::sync::{Mutex, broadcast, mpsc, oneshot};

use crate::server::Core;

/// The host kind on the log.
pub const HOST_KIND: &str = "cloud-cdp";
/// The synthetic connection a cloud host attaches on (no surface socket).
pub const CLOUD_CONNECTION: u64 = u64::MAX;

// ---- the CDP client ----

/// A CDP connection: commands by id, events by broadcast.
pub struct Cdp {
    out: mpsc::Sender<String>,
    pending: Mutex<HashMap<u64, oneshot::Sender<Value>>>,
    next: AtomicU64,
    events: broadcast::Sender<Value>,
}

impl Cdp {
    /// Drive `transport`: one task writes what is sent and reads what
    /// arrives, answering pending calls and fanning events out.
    pub fn connect(mut transport: Box<dyn CdpTransport>) -> Arc<Self> {
        let (out, mut out_rx) = mpsc::channel::<String>(256);
        let (events, _) = broadcast::channel(1024);
        let cdp = Arc::new(Self {
            out,
            pending: Mutex::new(HashMap::new()),
            next: AtomicU64::new(1),
            events,
        });
        let me = Arc::clone(&cdp);
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    m = out_rx.recv() => match m {
                        Some(text) => {
                            if transport.send(text).await.is_err() {
                                break;
                            }
                        }
                        None => break,
                    },
                    r = transport.recv() => match r {
                        Some(Ok(text)) => me.dispatch(&text).await,
                        _ => break,
                    },
                }
            }
            // The relay ended: every waiter hears it.
            let mut p = me.pending.lock().await;
            for (_, tx) in p.drain() {
                let _ = tx.send(json!({"error": {"message": "the DevTools relay closed"}}));
            }
        });
        cdp
    }

    async fn dispatch(&self, text: &str) {
        let Ok(v) = serde_json::from_str::<Value>(text) else {
            return;
        };
        if let Some(id) = v["id"].as_u64() {
            if let Some(tx) = self.pending.lock().await.remove(&id) {
                let _ = tx.send(v);
            }
        } else if v["method"].is_string() {
            let _ = self.events.send(v);
        }
    }

    /// One command; `session`: the flattened target session it is for.
    pub async fn call(
        &self,
        session: Option<&str>,
        method: &str,
        params: Value,
    ) -> Result<Value, String> {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        let mut msg = json!({"id": id, "method": method, "params": params});
        if let Some(s) = session {
            msg["sessionId"] = json!(s);
        }
        let (tx, rx) = oneshot::channel();
        self.pending.lock().await.insert(id, tx);
        if self
            .out
            .send(serde_json::to_string(&msg).unwrap_or_default())
            .await
            .is_err()
        {
            self.pending.lock().await.remove(&id);
            return Err("the DevTools relay is gone".into());
        }
        let reply = match tokio::time::timeout(Duration::from_secs(30), rx).await {
            Ok(Ok(v)) => v,
            Ok(Err(_)) => return Err("the DevTools relay closed".into()),
            Err(_) => {
                self.pending.lock().await.remove(&id);
                return Err(format!("{method}: no answer within 30 s"));
            }
        };
        if let Some(e) = reply.get("error") {
            return Err(format!(
                "{method}: {}",
                e["message"].as_str().unwrap_or("CDP error")
            ));
        }
        Ok(reply["result"].clone())
    }

    /// Events as they arrive.
    pub fn events(&self) -> broadcast::Receiver<Value> {
        self.events.subscribe()
    }
}

// ---- the view ----

/// What a watcher of the view receives.
#[derive(Clone)]
pub struct ViewFrame {
    pub jpeg: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub page_width: u32,
    pub page_height: u32,
    pub seq: u64,
    pub url: String,
    pub title: String,
}

/// What the person does in the view.
#[derive(Clone, Debug)]
pub struct ViewInput {
    pub kind: String,
    pub x: f64,
    pub y: f64,
    pub button: String,
    pub text: String,
    pub key: String,
    pub delta_x: i32,
    pub delta_y: i32,
    pub modifiers: u32,
}

/// Control of the view: watchers come and go, input arrives.
pub enum ViewControl {
    Watch {
        id: u64,
        tx: mpsc::Sender<ViewFrame>,
        max_width: u32,
        max_height: u32,
        quality: u32,
    },
    Unwatch(u64),
    Input(ViewInput, oneshot::Sender<Result<String, (String, String)>>),
}

/// The page's live state, kept by the event task.
struct Page {
    session: String,
    state_version: u64,
    url: String,
    title: String,
    ready: bool,
    dialog: Option<(String, String)>,
    /// A navigation the page started since the last mark (for settle).
    nav_started: u64,
    nav_done: u64,
    watchers: HashMap<u64, mpsc::Sender<ViewFrame>>,
    screencast: bool,
    seq: u64,
}

/// The cloud host of one browser session.
pub struct CloudHost {
    core: Arc<Core>,
    bsid: BrowserSessionId,
    task_id: modbit_domain::TaskId,
    sandbox: Arc<SandboxHandle>,
    cdp: Mutex<Option<Arc<Cdp>>>,
    page: Arc<Mutex<Option<Page>>>,
}

type Fail = (String, String);

fn cdp_err(e: String) -> Fail {
    ("CDP".to_owned(), e)
}

impl CloudHost {
    /// Attach a cloud host to the task's browser session, opening the
    /// session when the task has none: `BrowserSessionOpened` (when opened)
    /// and `BrowserHostAttached {host_kind: cloud-cdp}` on the log. The
    /// browser itself starts on the first request (nothing runs for a task
    /// that never browses).
    pub async fn attach(
        core: &Arc<Core>,
        task: &Task,
        sandbox: Arc<SandboxHandle>,
        actor: &Actor,
    ) -> Result<BrowserSessionId, String> {
        let existing = core.browser.session_for_task(task.task_id).await;
        let mut events = Vec::new();
        let bsid = match existing {
            Some(b) => b,
            None => {
                let b = BrowserSessionId::new();
                let rec = core.browser.open(b, task.task_id, task.session_id).await;
                events.push(crate::runtime::typed(
                    "BrowserSessionOpened",
                    &TaskEvent::BrowserSessionOpened {
                        browser_session_id: b.to_string(),
                        partition: rec.partition,
                    },
                    actor.clone(),
                ));
                b
            }
        };
        let (tx, rx) = mpsc::channel::<wire::BrowserHostRequest>(32);
        let (ctl_tx, ctl_rx) = mpsc::channel::<ViewControl>(64);
        let partition = format!("sandbox:{}", sandbox.identity().sandbox_id);
        core.browser
            .attach(
                bsid,
                crate::browser::HostLink {
                    kind: HOST_KIND.into(),
                    tx,
                    connection: CLOUD_CONNECTION,
                    view: Some(ctl_tx),
                },
            )
            .await
            .map_err(|e| format!("{e:?}"))?;
        let isolated = sandbox.identity().isolated;
        events.push(crate::runtime::typed(
            "BrowserHostAttached",
            &TaskEvent::BrowserHostAttached {
                browser_session_id: bsid.to_string(),
                host_kind: HOST_KIND.into(),
                partition,
                sandboxed: isolated,
                context_isolated: true,
                node_integration: false,
            },
            actor.clone(),
        ));
        crate::runtime::append_batch(
            &mut *core.store.lock().await,
            core,
            crate::runtime::Lineage::task(core.tenant_id, task.session_id, task.task_id),
            vec![(AggregateType::Task, *task.task_id.as_bytes(), events)],
        )
        .map_err(|e| e.to_string())?;
        let host = Arc::new(Self {
            core: Arc::clone(core),
            bsid,
            task_id: task.task_id,
            sandbox,
            cdp: Mutex::new(None),
            page: Arc::new(Mutex::new(None)),
        });
        tokio::spawn(host.serve(rx, ctl_rx));
        Ok(bsid)
    }

    async fn serve(
        self: Arc<Self>,
        mut rx: mpsc::Receiver<wire::BrowserHostRequest>,
        mut ctl: mpsc::Receiver<ViewControl>,
    ) {
        loop {
            tokio::select! {
                r = rx.recv() => {
                    let Some(r) = r else { break };
                    let req: HostRequest = match serde_json::from_str(&r.request_json) {
                        Ok(q) => q,
                        Err(e) => {
                            self.core.browser.deliver(&r.request_id, HostResponse::Error { code: "MALFORMED".into(), message: e.to_string() }).await;
                            continue;
                        }
                    };
                    let closing = matches!(req, HostRequest::Close);
                    let response = match self.answer(req).await {
                        Ok(r) => r,
                        Err((code, message)) => HostResponse::Error { code, message },
                    };
                    self.core.browser.deliver(&r.request_id, response).await;
                    if closing {
                        break;
                    }
                }
                c = ctl.recv() => {
                    let Some(c) = c else { break };
                    self.control(c).await;
                }
            }
        }
        eprintln!(
            "modbit-core: cloud browser host of session {} for task {} ended",
            self.bsid, self.task_id
        );
    }

    /// The browser, started and attached on first use.
    async fn ensure(&self) -> Result<(Arc<Cdp>, String), Fail> {
        if let (Some(c), Some(p)) = (&*self.cdp.lock().await, &*self.page.lock().await) {
            return Ok((Arc::clone(c), p.session.clone()));
        }
        let task = self.task_id.to_string();
        self.sandbox
            .browser_start(&task, 1024, 768)
            .await
            .map_err(|e| ("BROWSER_UNAVAILABLE".to_owned(), e.to_string()))?;
        let transport = self
            .sandbox
            .browser_connect()
            .await
            .map_err(|e| ("BROWSER_UNAVAILABLE".to_owned(), e.to_string()))?;
        let cdp = Cdp::connect(transport);
        // The session's page lives in a browser context of its own —
        // ephemeral, in-memory cookies and storage, gone with the browser
        // — never the browser's default profile.
        let context = cdp
            .call(
                None,
                "Target.createBrowserContext",
                json!({"disposeOnDetach": true}),
            )
            .await
            .map_err(cdp_err)?["browserContextId"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        let target_id = cdp
            .call(
                None,
                "Target.createTarget",
                json!({"url": "about:blank", "browserContextId": context, "width": 1024, "height": 768}),
            )
            .await
            .map_err(cdp_err)?["targetId"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        let attached = cdp
            .call(
                None,
                "Target.attachToTarget",
                json!({"targetId": target_id, "flatten": true}),
            )
            .await
            .map_err(cdp_err)?;
        let session = attached["sessionId"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        for m in [
            "Page.enable",
            "Runtime.enable",
            "DOM.enable",
            "Accessibility.enable",
        ] {
            cdp.call(Some(&session), m, json!({}))
                .await
                .map_err(cdp_err)?;
        }
        *self.page.lock().await = Some(Page {
            session: session.clone(),
            state_version: 0,
            url: "about:blank".into(),
            title: String::new(),
            ready: true,
            dialog: None,
            nav_started: 0,
            nav_done: 0,
            watchers: HashMap::new(),
            screencast: false,
            seq: 0,
        });
        // The page's own events: navigations and loads bump the state
        // version (what a reference is scoped to); dialogs are recorded;
        // screencast frames go to the watchers and are acknowledged.
        let mut ev = cdp.events();
        let page = Arc::clone(&self.page);
        let sess = session.clone();
        let cdp_for_ack = Arc::clone(&cdp);
        tokio::spawn(async move {
            loop {
                let e = match ev.recv().await {
                    Ok(e) => e,
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => break,
                };
                if e["sessionId"].as_str() != Some(sess.as_str()) {
                    continue;
                }
                let method = e["method"].as_str().unwrap_or_default();
                let mut p = page.lock().await;
                let Some(pg) = p.as_mut() else { continue };
                match method {
                    "Page.frameStartedNavigating" | "Page.frameRequestedNavigation"
                        if e["params"]["frameId"].is_string() =>
                    {
                        pg.nav_started += 1;
                        pg.ready = false;
                    }
                    "Page.frameNavigated" if e["params"]["frame"]["parentId"].is_null() => {
                        pg.state_version += 1;
                        if let Some(u) = e["params"]["frame"]["url"].as_str() {
                            pg.url = u.to_owned();
                        }
                    }
                    "Page.navigatedWithinDocument" => {
                        pg.state_version += 1;
                        if let Some(u) = e["params"]["url"].as_str() {
                            pg.url = u.to_owned();
                        }
                    }
                    "Page.loadEventFired" => {
                        pg.state_version += 1;
                        pg.nav_done += 1;
                        pg.ready = true;
                    }
                    "Page.javascriptDialogOpening" => {
                        pg.dialog = Some((
                            e["params"]["type"].as_str().unwrap_or("dialog").to_owned(),
                            e["params"]["message"]
                                .as_str()
                                .unwrap_or_default()
                                .chars()
                                .take(200)
                                .collect(),
                        ));
                    }
                    "Page.javascriptDialogClosed" => pg.dialog = None,
                    "Page.screencastFrame" => {
                        let sid = e["params"]["sessionId"].as_i64().unwrap_or(0);
                        let data = e["params"]["data"].as_str().unwrap_or_default();
                        let meta = &e["params"]["metadata"];
                        pg.seq += 1;
                        let jpeg = base64::engine::general_purpose::STANDARD
                            .decode(data)
                            .unwrap_or_default();
                        let frame = ViewFrame {
                            jpeg,
                            width: 0,
                            height: 0,
                            page_width: meta["deviceWidth"].as_f64().unwrap_or(0.0) as u32,
                            page_height: meta["deviceHeight"].as_f64().unwrap_or(0.0) as u32,
                            seq: pg.seq,
                            url: pg.url.clone(),
                            title: pg.title.clone(),
                        };
                        pg.watchers
                            .retain(|_, tx| tx.try_send(frame.clone()).is_ok() || !tx.is_closed());
                        let cdp = Arc::clone(&cdp_for_ack);
                        let s2 = sess.clone();
                        tokio::spawn(async move {
                            let _ = cdp
                                .call(
                                    Some(&s2),
                                    "Page.screencastFrameAck",
                                    json!({"sessionId": sid}),
                                )
                                .await;
                        });
                    }
                    _ => {}
                }
            }
        });
        *self.cdp.lock().await = Some(Arc::clone(&cdp));
        Ok((cdp, session))
    }

    async fn state(&self, cdp: &Cdp, session: &str) -> Result<PageState, Fail> {
        let r = cdp
            .call(
                Some(session),
                "Runtime.evaluate",
                json!({"expression": "JSON.stringify({url: location.href, title: document.title, ready: document.readyState === 'complete'})", "returnByValue": true}),
            )
            .await
            .map_err(cdp_err)?;
        let v: Value = r["result"]["value"]
            .as_str()
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or(Value::Null);
        let mut p = self.page.lock().await;
        let pg = p
            .as_mut()
            .ok_or_else(|| ("NO_SUCH_SESSION".to_owned(), "no page".to_owned()))?;
        if let Some(u) = v["url"].as_str() {
            pg.url = u.to_owned();
        }
        if let Some(t) = v["title"].as_str() {
            pg.title = t.chars().take(300).collect();
        }
        pg.ready = v["ready"].as_bool().unwrap_or(pg.ready);
        Ok(PageState {
            url: pg.url.clone(),
            title: pg.title.clone(),
            ready: pg.ready,
            state_version: pg.state_version,
        })
    }

    async fn version(&self) -> u64 {
        self.page
            .lock()
            .await
            .as_ref()
            .map(|p| p.state_version)
            .unwrap_or(0)
    }

    async fn dialog(&self) -> Option<(String, String)> {
        self.page
            .lock()
            .await
            .as_ref()
            .and_then(|p| p.dialog.clone())
    }

    async fn nav_marks(&self) -> (u64, u64) {
        self.page
            .lock()
            .await
            .as_ref()
            .map(|p| (p.nav_started, p.nav_done))
            .unwrap_or((0, 0))
    }

    /// Answer one request the Core's tools made.
    async fn answer(&self, req: HostRequest) -> Result<HostResponse, Fail> {
        let (cdp, session) = self.ensure().await?;
        let session = session.as_str();
        match req {
            HostRequest::Navigate { url } => {
                if !(url.starts_with("http://") || url.starts_with("https://"))
                    || url.contains(char::is_whitespace)
                {
                    return Err((
                        "NAVIGATION_BLOCKED".into(),
                        format!("{url} is not an http(s) URL"),
                    ));
                }
                if self.dialog().await.is_some() {
                    return Err((
                        "MODAL_BLOCKING".into(),
                        "the page has a dialog open; it is the person's to answer".into(),
                    ));
                }
                let (_, done_before) = self.nav_marks().await;
                let r = cdp
                    .call(Some(session), "Page.navigate", json!({"url": url}))
                    .await
                    .map_err(cdp_err)?;
                if let Some(err) = r["errorText"].as_str() {
                    return Err(("NAVIGATION_FAILED".into(), err.to_owned()));
                }
                // The load, bounded well under the Core's deadline.
                let t0 = Instant::now();
                while self.nav_marks().await.1 == done_before
                    && t0.elapsed() < Duration::from_secs(15)
                {
                    tokio::time::sleep(Duration::from_millis(30)).await;
                }
                Ok(HostResponse::State {
                    state: self.state(&cdp, session).await?,
                })
            }
            HostRequest::State => Ok(HostResponse::State {
                state: self.state(&cdp, session).await?,
            }),
            HostRequest::Snapshot { max_nodes } => {
                if self.dialog().await.is_some() {
                    return Err((
                        "MODAL_BLOCKING".into(),
                        "the page has a dialog open; it is the person's to answer".into(),
                    ));
                }
                let (nodes, truncated) = self.snapshot(&cdp, session, max_nodes).await?;
                Ok(HostResponse::Snapshot {
                    state: self.state(&cdp, session).await?,
                    nodes,
                    truncated,
                })
            }
            HostRequest::Capture { clip } => {
                let mut params = json!({"format": "png", "fromSurface": true});
                if let Some(c) = clip {
                    params["clip"] = json!({"x": c.x, "y": c.y, "width": c.width.max(1), "height": c.height.max(1), "scale": 1});
                }
                let r = cdp
                    .call(Some(session), "Page.captureScreenshot", params)
                    .await
                    .map_err(|e| ("CAPTURE_FAILED".to_owned(), e))?;
                let png = r["data"].as_str().unwrap_or_default().to_owned();
                if png.is_empty() {
                    return Err(("CAPTURE_EMPTY".into(), "the page produced no frame".into()));
                }
                Ok(HostResponse::Capture {
                    state: self.state(&cdp, session).await?,
                    png_base64: png,
                    clip,
                })
            }
            HostRequest::Act {
                backend_dom_node_id,
                action,
                value,
                key,
                at,
                credential_handle,
            } => {
                if self.dialog().await.is_some() {
                    return Err((
                        "MODAL_BLOCKING".into(),
                        "the page has a dialog open (alert, confirm or prompt); it is the person's to answer".into(),
                    ));
                }
                self.act(
                    &cdp,
                    session,
                    backend_dom_node_id,
                    &action,
                    &value,
                    &key,
                    at,
                    credential_handle.as_deref(),
                )
                .await
            }
            HostRequest::Isolation => {
                let r = cdp
                    .call(
                        Some(session),
                        "Runtime.evaluate",
                        json!({"expression": "typeof process !== 'undefined' || typeof require !== 'undefined' || typeof window.electron !== 'undefined' || typeof window.modbit !== 'undefined'", "returnByValue": true}),
                    )
                    .await
                    .map_err(cdp_err)?;
                Ok(HostResponse::Isolation {
                    node_reachable: r["result"]["value"].as_bool().unwrap_or(false),
                    partition: format!("sandbox:{}", self.sandbox.identity().sandbox_id),
                    sandboxed: self.sandbox.identity().isolated,
                    context_isolated: true,
                })
            }
            HostRequest::Close => {
                let state = self.state(&cdp, session).await.unwrap_or_default();
                let _ = self.sandbox.browser_stop(&self.task_id.to_string()).await;
                Ok(HostResponse::State { state })
            }
        }
    }

    async fn snapshot(
        &self,
        cdp: &Cdp,
        session: &str,
        max_nodes: u32,
    ) -> Result<(Vec<RawAxNode>, bool), Fail> {
        let tree = cdp
            .call(Some(session), "Accessibility.getFullAXTree", json!({}))
            .await
            .map_err(cdp_err)?;
        let all: Vec<&Value> = tree["nodes"]
            .as_array()
            .map(|a| a.iter().collect())
            .unwrap_or_default();
        let parent_of: HashMap<&str, &str> = all
            .iter()
            .filter_map(|n| Some((n["nodeId"].as_str()?, n["parentId"].as_str()?)))
            .collect();
        let depth_of = |id: &str| -> u32 {
            let mut d = 0;
            let mut cur = id;
            while let Some(p) = parent_of.get(cur) {
                d += 1;
                cur = p;
                if d > 64 {
                    break;
                }
            }
            d
        };
        let limit = max_nodes.clamp(1, 400) as usize;
        let live: Vec<&Value> = all
            .iter()
            .copied()
            .filter(|n| !n["ignored"].as_bool().unwrap_or(false))
            .collect();
        let truncated = live.len() > limit;
        let mut nodes: Vec<RawAxNode> = live
            .iter()
            .take(limit)
            .map(|n| {
                let id = n["nodeId"].as_str().unwrap_or_default().to_owned();
                let props = n["properties"].as_array();
                let disabled = props.is_some_and(|ps| {
                    ps.iter().any(|p| {
                        matches!(p["name"].as_str(), Some("disabled") | Some("readonly"))
                            && p["value"]["value"].as_bool() == Some(true)
                    })
                });
                RawAxNode {
                    depth: depth_of(&id),
                    id,
                    parent: n["parentId"].as_str().map(str::to_owned),
                    role: n["role"]["value"].as_str().unwrap_or_default().to_owned(),
                    name: n["name"]["value"]
                        .as_str()
                        .unwrap_or_default()
                        .chars()
                        .take(200)
                        .collect(),
                    value: n["value"]["value"]
                        .as_str()
                        .map(|s| s.chars().take(200).collect())
                        .unwrap_or_else(|| {
                            n["value"]["value"]
                                .as_f64()
                                .map(|f| f.to_string())
                                .unwrap_or_default()
                        }),
                    ignored: false,
                    backend_dom_node_id: n["backendDOMNodeId"].as_i64(),
                    bounds: None,
                    disabled,
                }
            })
            .collect();
        // The layout box of what can be acted on (bounded: the first 80
        // actionable nodes), in CSS pixels.
        const ACTIONABLE: &[&str] = &[
            "button",
            "link",
            "textbox",
            "searchbox",
            "combobox",
            "checkbox",
            "radio",
            "slider",
            "spinbutton",
            "listbox",
            "menuitem",
            "tab",
            "option",
            "switch",
            "canvas",
            "image",
            "img",
            "figure",
            "graphics-document",
        ];
        let mut boxes = 0;
        for n in &mut nodes {
            if boxes >= 80 {
                break;
            }
            let Some(bid) = n.backend_dom_node_id else {
                continue;
            };
            if !ACTIONABLE.contains(&n.role.to_ascii_lowercase().as_str()) {
                continue;
            }
            boxes += 1;
            if let Ok(bm) = cdp
                .call(
                    Some(session),
                    "DOM.getBoxModel",
                    json!({"backendNodeId": bid}),
                )
                .await
                && let Some(q) = bm["model"]["border"].as_array()
                && q.len() >= 8
            {
                let f = |i: usize| q[i].as_f64().unwrap_or(0.0);
                let xs = [f(0), f(2), f(4), f(6)];
                let ys = [f(1), f(3), f(5), f(7)];
                let x = xs.iter().cloned().fold(f64::INFINITY, f64::min);
                let y = ys.iter().cloned().fold(f64::INFINITY, f64::min);
                let xm = xs.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
                let ym = ys.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
                n.bounds = Some(Clip {
                    x: x.max(0.0).round() as u32,
                    y: y.max(0.0).round() as u32,
                    width: (xm - x).max(0.0).round() as u32,
                    height: (ym - y).max(0.0).round() as u32,
                });
            }
        }
        Ok((nodes, truncated))
    }

    #[allow(clippy::too_many_arguments)]
    async fn act(
        &self,
        cdp: &Cdp,
        session: &str,
        backend_node_id: i64,
        action: &str,
        value: &str,
        key: &str,
        at: Option<(u32, u32)>,
        credential_handle: Option<&str>,
    ) -> Result<HostResponse, Fail> {
        let object_id = cdp
            .call(
                Some(session),
                "DOM.resolveNode",
                json!({"backendNodeId": backend_node_id}),
            )
            .await
            .map_err(|e| {
                (
                    "TARGET_GONE".to_owned(),
                    format!("the element is no longer in the document ({e})"),
                )
            })?["object"]["objectId"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        let call = |fn_decl: &str, args: Vec<Value>| {
            let object_id = object_id.clone();
            let fn_decl = fn_decl.to_owned();
            async move {
                cdp.call(
                    Some(session),
                    "Runtime.callFunctionOn",
                    json!({"objectId": object_id, "functionDeclaration": fn_decl, "arguments": args.into_iter().map(|a| json!({"value": a})).collect::<Vec<_>>(), "returnByValue": true}),
                )
                .await
                .map(|r| r["result"]["value"].clone())
                .map_err(cdp_err)
            }
        };
        let version_before = self.version().await;
        let (started_before, _) = self.nav_marks().await;
        let editable_check = "function() { const t = this.tagName; const notText = ['button','submit','checkbox','radio','file','image','reset','range','color']; const ok = (t === 'INPUT' && !notText.includes((this.type || 'text').toLowerCase())) || t === 'TEXTAREA' || this.isContentEditable === true; if (!ok) return 'NOT_A_TEXT_FIELD:' + t + (this.type ? '/' + this.type : ''); if (this.disabled) return 'DISABLED'; if (this.readOnly) return 'READ_ONLY'; return 'ok'; }";
        let detail = match action {
            "click" | "check" | "uncheck" => {
                if action != "click" {
                    let checked =
                        call("function() { return this.checked === true; }", vec![]).await?;
                    if (checked == json!(true)) == (action == "check") {
                        return Ok(HostResponse::Acted {
                            state: self.state(cdp, session).await?,
                            navigated: false,
                            detail: format!("already {action}ed"),
                        });
                    }
                }
                let _ = cdp
                    .call(
                        Some(session),
                        "DOM.scrollIntoViewIfNeeded",
                        json!({"backendNodeId": backend_node_id}),
                    )
                    .await;
                let b = call("function() { const r = this.getBoundingClientRect(); return { left: r.left, top: r.top, right: r.right, bottom: r.bottom }; }", vec![]).await?;
                let (l, t, r, btm) = (
                    b["left"].as_f64().unwrap_or(0.0),
                    b["top"].as_f64().unwrap_or(0.0),
                    b["right"].as_f64().unwrap_or(0.0),
                    b["bottom"].as_f64().unwrap_or(0.0),
                );
                if r - l <= 0.0 || btm - t <= 0.0 {
                    return Err((
                        "TARGET_OCCLUDED".into(),
                        "the element has no visible box (hidden or collapsed)".into(),
                    ));
                }
                let x = match at {
                    Some((ax, _)) => (l + f64::from(ax)).clamp(l, r - 1.0),
                    None => (l + r) / 2.0,
                };
                let y = match at {
                    Some((_, ay)) => (t + f64::from(ay)).clamp(t, btm - 1.0),
                    None => (t + btm) / 2.0,
                };
                let hit = call("function(x, y) { const el = document.elementFromPoint(x, y); if (!el) return 'NONE'; return (el === this || this.contains(el) || el.contains(this)) ? 'ok' : (el.tagName + (el.id ? '#' + el.id : '')); }", vec![json!(x), json!(y)]).await?;
                if hit != json!("ok") {
                    let what = if hit == json!("NONE") {
                        "nothing".to_owned()
                    } else {
                        hit.as_str().unwrap_or_default().to_owned()
                    };
                    return Err((
                        "TARGET_OCCLUDED".into(),
                        format!("{what} is at the click point, over the element"),
                    ));
                }
                let bounded = |m: &'static str, p: Value| async move {
                    let _ = tokio::time::timeout(
                        Duration::from_millis(1500),
                        cdp.call(Some(session), m, p),
                    )
                    .await;
                };
                bounded(
                    "Input.dispatchMouseEvent",
                    json!({"type": "mouseMoved", "x": x, "y": y}),
                )
                .await;
                bounded("Input.dispatchMouseEvent", json!({"type": "mousePressed", "x": x, "y": y, "button": "left", "clickCount": 1})).await;
                bounded("Input.dispatchMouseEvent", json!({"type": "mouseReleased", "x": x, "y": y, "button": "left", "clickCount": 1})).await;
                format!("{action} at {},{}", x.round(), y.round())
            }
            "fill" => {
                let ok = call(editable_check, vec![]).await?;
                if ok != json!("ok") {
                    return Err((
                        "TARGET_NOT_EDITABLE".into(),
                        format!(
                            "the element does not take text ({})",
                            ok.as_str().unwrap_or_default()
                        ),
                    ));
                }
                cdp.call(
                    Some(session),
                    "DOM.focus",
                    json!({"backendNodeId": backend_node_id}),
                )
                .await
                .map_err(cdp_err)?;
                call("function() { if ('value' in this) { this.value = ''; this.dispatchEvent(new Event('input', {bubbles:true})); } else if (this.isContentEditable) { this.textContent = ''; } }", vec![]).await?;
                if !value.is_empty() {
                    cdp.call(Some(session), "Input.insertText", json!({"text": value}))
                        .await
                        .map_err(cdp_err)?;
                }
                call(
                    "function() { this.dispatchEvent(new Event('change', {bubbles:true})); }",
                    vec![],
                )
                .await?;
                format!("inserted {} chars", value.chars().count())
            }
            "fill_credential" => {
                // The cloud host holds no credential custody: a page
                // credential is the desktop broker's (M7.8), not the
                // sandbox's. Reported, never guessed.
                return Err((
                    "CREDENTIAL_UNAVAILABLE".into(),
                    format!(
                        "no credential {} in the cloud: page credentials are filled by the desktop's broker",
                        credential_handle.unwrap_or("")
                    ),
                ));
            }
            "select" => {
                let r = call("function(v) { if (this.tagName !== 'SELECT') return 'NOT_SELECT'; const o = [...this.options].find(o => o.value === v || o.textContent.trim() === v); if (!o) return 'NO_OPTION'; this.value = o.value; this.dispatchEvent(new Event('input', {bubbles:true})); this.dispatchEvent(new Event('change', {bubbles:true})); return 'ok'; }", vec![json!(value)]).await?;
                if r != json!("ok") {
                    return Err((
                        r.as_str().unwrap_or("SELECT_FAILED").to_owned(),
                        format!("select {value:?}"),
                    ));
                }
                format!("selected {value:?}")
            }
            "press" => {
                let _ = cdp
                    .call(
                        Some(session),
                        "DOM.focus",
                        json!({"backendNodeId": backend_node_id}),
                    )
                    .await;
                let code = match key {
                    "Enter" => 13,
                    "Tab" => 9,
                    "Escape" => 27,
                    "Backspace" => 8,
                    "ArrowDown" => 40,
                    "ArrowUp" => 38,
                    "ArrowLeft" => 37,
                    "ArrowRight" => 39,
                    "Space" => 32,
                    _ => return Err(("UNSUPPORTED_KEY".into(), key.to_owned())),
                };
                let text = match key {
                    "Enter" => Some("\r"),
                    "Space" => Some(" "),
                    _ => None,
                };
                let mut down = json!({"type": if text.is_some() { "keyDown" } else { "rawKeyDown" }, "key": key, "code": key, "windowsVirtualKeyCode": code, "nativeVirtualKeyCode": code});
                if let Some(t) = text {
                    down["text"] = json!(t);
                }
                cdp.call(Some(session), "Input.dispatchKeyEvent", down)
                    .await
                    .map_err(cdp_err)?;
                cdp.call(Some(session), "Input.dispatchKeyEvent", json!({"type": "keyUp", "key": key, "code": key, "windowsVirtualKeyCode": code, "nativeVirtualKeyCode": code})).await.map_err(cdp_err)?;
                format!("pressed {key}")
            }
            other => return Err(("UNSUPPORTED_ACTION".into(), other.to_owned())),
        };
        // Settle: a navigation the action started settles at its load
        // (bounded); otherwise the page gets a short quiet period.
        let t0 = Instant::now();
        while self.nav_marks().await.0 == started_before
            && t0.elapsed() < Duration::from_millis(400)
        {
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        let (started_now, done_at_start) = self.nav_marks().await;
        let nav_started = started_now != started_before;
        if nav_started {
            let t1 = Instant::now();
            while self.nav_marks().await.1 == done_at_start && t1.elapsed() < Duration::from_secs(8)
            {
                tokio::time::sleep(Duration::from_millis(30)).await;
            }
        } else {
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
        if let Some((kind, message)) = self.dialog().await {
            return Err((
                "MODAL_BLOCKING".into(),
                format!(
                    "the {action} opened a {kind} dialog ({message:?}); it is the person's to answer and is still open"
                ),
            ));
        }
        let navigated = nav_started || self.version().await != version_before;
        Ok(HostResponse::Acted {
            state: self.state(cdp, session).await?,
            navigated,
            detail,
        })
    }

    /// The view: watchers and the person's input.
    async fn control(&self, c: ViewControl) {
        match c {
            ViewControl::Watch {
                id,
                tx,
                max_width,
                max_height,
                quality,
            } => {
                let Ok((cdp, session)) = self.ensure().await else {
                    return;
                };
                let start = {
                    let mut p = self.page.lock().await;
                    let Some(pg) = p.as_mut() else { return };
                    pg.watchers.insert(id, tx);
                    let start = !pg.screencast;
                    pg.screencast = true;
                    start
                };
                if start {
                    let _ = cdp
                        .call(
                            Some(&session),
                            "Page.startScreencast",
                            json!({"format": "jpeg", "quality": if quality == 0 { 60 } else { quality.min(100) }, "maxWidth": if max_width == 0 { 1024 } else { max_width.min(4096) }, "maxHeight": if max_height == 0 { 768 } else { max_height.min(4096) }, "everyNthFrame": 1}),
                        )
                        .await;
                }
            }
            ViewControl::Unwatch(id) => {
                let stop = {
                    let mut p = self.page.lock().await;
                    let Some(pg) = p.as_mut() else { return };
                    pg.watchers.remove(&id);
                    let stop = pg.watchers.is_empty() && pg.screencast;
                    if stop {
                        pg.screencast = false;
                    }
                    stop
                };
                if stop && let Some(cdp) = self.cdp.lock().await.clone() {
                    let session = self.page.lock().await.as_ref().map(|p| p.session.clone());
                    if let Some(s) = session {
                        let _ = cdp.call(Some(&s), "Page.stopScreencast", json!({})).await;
                    }
                }
            }
            ViewControl::Input(input, reply) => {
                let r = self.input(input).await;
                let _ = reply.send(r);
            }
        }
    }

    async fn input(&self, i: ViewInput) -> Result<String, Fail> {
        let (cdp, session) = self.ensure().await?;
        let session = session.as_str();
        let button = if i.button.is_empty() {
            "left"
        } else {
            i.button.as_str()
        };
        match i.kind.as_str() {
            "mouse_move" => {
                cdp.call(
                    Some(session),
                    "Input.dispatchMouseEvent",
                    json!({"type": "mouseMoved", "x": i.x, "y": i.y, "modifiers": i.modifiers}),
                )
                .await
                .map_err(cdp_err)?;
                Ok(format!("moved to {},{}", i.x.round(), i.y.round()))
            }
            "mouse_down" | "mouse_up" | "click" => {
                if i.kind != "mouse_up" {
                    cdp.call(Some(session), "Input.dispatchMouseEvent", json!({"type": "mousePressed", "x": i.x, "y": i.y, "button": button, "clickCount": 1, "modifiers": i.modifiers})).await.map_err(cdp_err)?;
                }
                if i.kind != "mouse_down" {
                    cdp.call(Some(session), "Input.dispatchMouseEvent", json!({"type": "mouseReleased", "x": i.x, "y": i.y, "button": button, "clickCount": 1, "modifiers": i.modifiers})).await.map_err(cdp_err)?;
                }
                Ok(format!("{} at {},{}", i.kind, i.x.round(), i.y.round()))
            }
            "wheel" => {
                cdp.call(Some(session), "Input.dispatchMouseEvent", json!({"type": "mouseWheel", "x": i.x, "y": i.y, "deltaX": i.delta_x, "deltaY": i.delta_y, "modifiers": i.modifiers})).await.map_err(cdp_err)?;
                Ok("wheel".into())
            }
            "key_down" | "key_up" => {
                let key = i.key.as_str();
                let code = match key {
                    "Enter" => 13,
                    "Tab" => 9,
                    "Escape" => 27,
                    "Backspace" => 8,
                    "Delete" => 46,
                    "ArrowDown" => 40,
                    "ArrowUp" => 38,
                    "ArrowLeft" => 37,
                    "ArrowRight" => 39,
                    "Space" | " " => 32,
                    "Home" => 36,
                    "End" => 35,
                    "PageUp" => 33,
                    "PageDown" => 34,
                    k if k.chars().count() == 1 => k
                        .to_ascii_uppercase()
                        .chars()
                        .next()
                        .map(|c| c as i32)
                        .unwrap_or(0),
                    _ => 0,
                };
                let text = if i.kind == "key_down" {
                    match key {
                        "Enter" => Some("\r".to_owned()),
                        k if k.chars().count() == 1 => Some(k.to_owned()),
                        _ => None,
                    }
                } else {
                    None
                };
                let mut p = json!({"type": if i.kind == "key_down" { if text.is_some() { "keyDown" } else { "rawKeyDown" } } else { "keyUp" }, "key": key, "code": key, "windowsVirtualKeyCode": code, "nativeVirtualKeyCode": code, "modifiers": i.modifiers});
                if let Some(t) = text {
                    p["text"] = json!(t);
                }
                cdp.call(Some(session), "Input.dispatchKeyEvent", p)
                    .await
                    .map_err(cdp_err)?;
                Ok(format!("{} {key}", i.kind))
            }
            "insert_text" => {
                cdp.call(Some(session), "Input.insertText", json!({"text": i.text}))
                    .await
                    .map_err(cdp_err)?;
                Ok(format!("inserted {} chars", i.text.chars().count()))
            }
            other => Err(("UNSUPPORTED_INPUT".into(), other.to_owned())),
        }
    }
}
