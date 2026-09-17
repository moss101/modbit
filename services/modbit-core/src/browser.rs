//! Browser sessions and the host bridge (M7.1, docs/22 "Local browser").
//!
//! The Core owns a browser session's identity, its control lease and its
//! recorded state; a *host* — Electron main, a `browser.host` client on the
//! authenticated surface socket (REQ-EV-0110) — owns the live Chromium and
//! answers the Core's typed requests. A session is opened for a task
//! (`OpenBrowserSession`, journaled on the task), a host attaches to it on
//! its connection (`AttachBrowserHost`, journaled), requests go out as
//! `BrowserHostRequest` frames on that connection and come back as
//! `BrowserHostResponse` commands matched by request id. A host that
//! disconnects fails every request it owed with `HOST_GONE`; the session
//! record survives (a host can attach again to the same partition) and a
//! Core restart rebuilds it from the task's events.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use modbit_browser::{
    BoxFuture, BrowserPort, BrowserSessionId, ControlLease, HostRequest, HostResponse, PageState,
    PortError,
};
use modbit_domain::ids::{SessionId, TaskId};
use modbit_protocol::v1 as wire;
use tokio::sync::{Mutex, mpsc, oneshot};

/// A host's link: the connection's outbound request queue.
#[derive(Clone)]
pub(crate) struct HostLink {
    /// `electron-main`.
    pub kind: String,
    /// Sends frames to the host connection's writer.
    pub tx: mpsc::Sender<wire::BrowserHostRequest>,
    /// The connection the host attached on (a later attach replaces it).
    pub connection: u64,
}

/// One session as the Core records it.
#[derive(Clone)]
pub(crate) struct SessionRecord {
    pub task_id: TaskId,
    pub session_id: SessionId,
    pub partition: String,
    pub lease: ControlLease,
    pub host: Option<HostLink>,
    pub state: PageState,
    pub closed: bool,
    /// Every entity a compiled page of this session has named, by
    /// reference (M7.2; bounded).
    pub known: HashMap<String, modbit_browser::compiler::Entity>,
    /// The page last compiled (M7.3: what the next delta starts from).
    pub last_page: Option<modbit_browser::compiler::PageEntities>,
    /// Transitions observed in this session (IMP-EV-0280; bounded), keyed
    /// by the fingerprint before, the reference and the action.
    pub transitions: Vec<modbit_browser::KnownTransition>,
}

/// Why a host could not attach.
#[derive(Debug)]
pub(crate) enum AttachRefusal {
    NoSuchSession,
    /// Another live host connection holds the session (IMP-EV-0084: one
    /// controller per session; IMP-EV-0110: a second peer cannot take it).
    HostConflict {
        kind: String,
    },
}

/// The registry: sessions and the requests awaiting a host's answer.
#[derive(Default)]
pub struct BrowserSessions {
    sessions: Mutex<HashMap<BrowserSessionId, SessionRecord>>,
    pending: Mutex<HashMap<String, (BrowserSessionId, oneshot::Sender<HostResponse>)>>,
    /// The credential handles a host registered (M7.8, docs/22
    /// "Credentials"): handle, label, origin, account name — never a value.
    /// Memory only; a host registers them again when it reconnects.
    credentials: Mutex<HashMap<String, modbit_browser::CredentialHandle>>,
}

impl BrowserSessions {
    /// Register (or replace) a credential handle (M7.8).
    pub(crate) async fn register_credential(&self, c: modbit_browser::CredentialHandle) {
        self.credentials.lock().await.insert(c.handle.clone(), c);
    }

    /// Forget a credential handle (M7.8).
    pub(crate) async fn forget_credential(&self, handle: &str) -> bool {
        self.credentials.lock().await.remove(handle).is_some()
    }
}

/// How long the Core waits for a host's answer.
fn deadline(req: &HostRequest) -> Duration {
    match req {
        HostRequest::Navigate { .. } => Duration::from_secs(40),
        HostRequest::Capture { .. } => Duration::from_secs(20),
        _ => Duration::from_secs(15),
    }
}

impl BrowserSessions {
    /// The partition a session's view lives in: one per session, persisted
    /// so a host that attaches again finds the same cookies and storage.
    #[must_use]
    pub fn partition_for(id: BrowserSessionId) -> String {
        format!("persist:modbit-browser-{id}")
    }

    pub(crate) async fn open(
        &self,
        id: BrowserSessionId,
        task_id: TaskId,
        session_id: SessionId,
    ) -> SessionRecord {
        let rec = SessionRecord {
            task_id,
            session_id,
            partition: Self::partition_for(id),
            lease: ControlLease::initial(),
            host: None,
            state: PageState::default(),
            closed: false,
            known: HashMap::new(),
            last_page: None,
            transitions: Vec::new(),
        };
        self.sessions.lock().await.insert(id, rec.clone());
        rec
    }

    pub(crate) async fn get(&self, id: BrowserSessionId) -> Option<SessionRecord> {
        self.sessions.lock().await.get(&id).cloned()
    }

    /// Put a session record back (a rebuild from the task's events after a restart).
    pub(crate) async fn restore(&self, id: BrowserSessionId, rec: SessionRecord) {
        self.sessions.lock().await.entry(id).or_insert(rec);
    }

    pub(crate) async fn attach(
        &self,
        id: BrowserSessionId,
        link: HostLink,
    ) -> Result<ControlLease, AttachRefusal> {
        let mut s = self.sessions.lock().await;
        let rec = s.get_mut(&id).ok_or(AttachRefusal::NoSuchSession)?;
        if let Some(h) = &rec.host
            && h.connection != link.connection
            && !h.tx.is_closed()
        {
            return Err(AttachRefusal::HostConflict {
                kind: h.kind.clone(),
            });
        }
        rec.host = Some(link);
        Ok(rec.lease)
    }

    /// Hand control to `to` (M7.6): the lease moves to a new generation
    /// unless `to` already holds it. Returns the lease and whether it moved.
    pub(crate) async fn hand_control(
        &self,
        id: BrowserSessionId,
        to: modbit_browser::Controller,
    ) -> Option<(ControlLease, bool)> {
        let mut s = self.sessions.lock().await;
        let rec = s.get_mut(&id)?;
        let before = rec.lease;
        rec.lease = rec.lease.handed_to(to);
        Some((rec.lease, rec.lease != before))
    }

    pub(crate) async fn record_state(&self, id: BrowserSessionId, state: PageState) {
        if let Some(rec) = self.sessions.lock().await.get_mut(&id) {
            rec.state = state;
        }
    }

    pub(crate) async fn close(&self, id: BrowserSessionId) -> Option<SessionRecord> {
        let mut s = self.sessions.lock().await;
        let rec = s.get_mut(&id)?;
        rec.closed = true;
        rec.host = None;
        Some(rec.clone())
    }

    /// The connection `connection` closed: its hosts are gone and every
    /// request they owed fails now (`HOST_GONE`), not at the deadline.
    pub(crate) async fn connection_closed(&self, connection: u64) {
        let gone: Vec<BrowserSessionId> = {
            let mut s = self.sessions.lock().await;
            let mut gone = Vec::new();
            for (id, rec) in s.iter_mut() {
                if rec
                    .host
                    .as_ref()
                    .is_some_and(|h| h.connection == connection)
                {
                    rec.host = None;
                    gone.push(*id);
                }
            }
            gone
        };
        if gone.is_empty() {
            return;
        }
        let mut p = self.pending.lock().await;
        let ids: Vec<String> = p
            .iter()
            .filter(|(_, (sid, _))| gone.contains(sid))
            .map(|(k, _)| k.clone())
            .collect();
        for k in ids {
            // Dropping the sender wakes the waiter with a closed channel → HOST_GONE.
            p.remove(&k);
        }
    }

    /// A host's answer arrived: hand it to the waiting request.
    pub(crate) async fn deliver(&self, request_id: &str, response: HostResponse) -> bool {
        let Some((_, tx)) = self.pending.lock().await.remove(request_id) else {
            return false;
        };
        tx.send(response).is_ok()
    }

    pub(crate) async fn session_for_task(&self, task: TaskId) -> Option<BrowserSessionId> {
        self.sessions
            .lock()
            .await
            .iter()
            .find(|(_, r)| r.task_id == task && !r.closed)
            .map(|(id, _)| *id)
    }
}

impl BrowserPort for BrowserSessions {
    fn request<'a>(
        &'a self,
        session: BrowserSessionId,
        request: HostRequest,
    ) -> BoxFuture<'a, Result<HostResponse, PortError>> {
        Box::pin(async move {
            let (link, lease) = {
                let s = self.sessions.lock().await;
                let Some(rec) = s.get(&session) else {
                    return Err(PortError::Refused {
                        code: "NO_SUCH_SESSION".into(),
                        message: format!("browser session {session} is not open"),
                    });
                };
                if rec.closed {
                    return Err(PortError::Refused {
                        code: "SESSION_CLOSED".into(),
                        message: format!("browser session {session} is closed"),
                    });
                }
                let Some(link) = rec.host.clone() else {
                    return Err(PortError::NoHost);
                };
                (link, rec.lease)
            };
            // An agent input under the user's control is refused here, before
            // it reaches the host (docs/22: takeover blocks input at once).
            if matches!(
                request,
                HostRequest::Navigate { .. } | HostRequest::Act { .. }
            ) && !lease.admits_agent_input(lease.generation)
            {
                return Err(PortError::Refused {
                    code: "HUMAN_ACTIVE".into(),
                    message: format!(
                        "the person holds control of the session (lease generation {})",
                        lease.generation
                    ),
                });
            }
            let request_id = BrowserSessionId::new().to_string();
            let (tx, rx) = oneshot::channel();
            self.pending
                .lock()
                .await
                .insert(request_id.clone(), (session, tx));
            let frame = wire::BrowserHostRequest {
                request_id: request_id.clone(),
                browser_session_id: Some(wire::Id {
                    value: session.as_bytes().to_vec(),
                }),
                lease_generation: lease.generation,
                request_json: serde_json::to_string(&request).unwrap_or_default(),
            };
            if link.tx.send(frame).await.is_err() {
                self.pending.lock().await.remove(&request_id);
                return Err(PortError::HostGone);
            }
            match tokio::time::timeout(deadline(&request), rx).await {
                Ok(Ok(resp)) => {
                    if let HostResponse::State { state } | HostResponse::Snapshot { state, .. } =
                        &resp
                    {
                        self.record_state(session, state.clone()).await;
                    }
                    Ok(resp)
                }
                Ok(Err(_)) => Err(PortError::HostGone),
                Err(_) => {
                    self.pending.lock().await.remove(&request_id);
                    Err(PortError::Timeout)
                }
            }
        })
    }

    fn lease<'a>(&'a self, session: BrowserSessionId) -> BoxFuture<'a, Option<ControlLease>> {
        Box::pin(async move { self.sessions.lock().await.get(&session).map(|r| r.lease) })
    }

    fn session_for<'a>(&'a self, task: TaskId) -> BoxFuture<'a, Option<BrowserSessionId>> {
        Box::pin(self.session_for_task(task))
    }

    fn remember_page<'a>(
        &'a self,
        session: BrowserSessionId,
        page: modbit_browser::compiler::PageEntities,
    ) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            if let Some(rec) = self.sessions.lock().await.get_mut(&session) {
                if rec.known.len() > 4000 {
                    rec.known.clear();
                }
                for e in &page.entities {
                    rec.known.insert(e.reference.clone(), e.clone());
                }
                rec.last_page = Some(page);
            }
        })
    }

    fn last_page<'a>(
        &'a self,
        session: BrowserSessionId,
    ) -> BoxFuture<'a, Option<modbit_browser::compiler::PageEntities>> {
        Box::pin(async move {
            self.sessions
                .lock()
                .await
                .get(&session)
                .and_then(|r| r.last_page.clone())
        })
    }

    fn remember_transition<'a>(
        &'a self,
        session: BrowserSessionId,
        t: modbit_browser::KnownTransition,
    ) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            if let Some(rec) = self.sessions.lock().await.get_mut(&session) {
                if let Some(k) = rec.transitions.iter_mut().find(|k| {
                    k.from_fingerprint == t.from_fingerprint
                        && k.reference == t.reference
                        && k.action == t.action
                }) {
                    k.times += 1;
                    k.to_fingerprint = t.to_fingerprint;
                    k.to_url = t.to_url;
                    k.verified = t.verified.or(k.verified);
                } else {
                    if rec.transitions.len() >= 2000 {
                        rec.transitions.remove(0);
                    }
                    rec.transitions.push(t);
                }
            }
        })
    }

    fn transitions_from<'a>(
        &'a self,
        session: BrowserSessionId,
        fingerprint: &'a str,
    ) -> BoxFuture<'a, Vec<modbit_browser::KnownTransition>> {
        Box::pin(async move {
            self.sessions
                .lock()
                .await
                .get(&session)
                .map(|r| {
                    r.transitions
                        .iter()
                        .filter(|t| t.from_fingerprint == fingerprint)
                        .cloned()
                        .collect()
                })
                .unwrap_or_default()
        })
    }

    fn credential<'a>(
        &'a self,
        handle: &'a str,
    ) -> BoxFuture<'a, Option<modbit_browser::CredentialHandle>> {
        Box::pin(async move { self.credentials.lock().await.get(handle).cloned() })
    }

    fn credentials_for<'a>(
        &'a self,
        origin: &'a str,
    ) -> BoxFuture<'a, Vec<modbit_browser::CredentialHandle>> {
        Box::pin(async move {
            let mut out: Vec<_> = self
                .credentials
                .lock()
                .await
                .values()
                .filter(|c| c.origin == origin)
                .cloned()
                .collect();
            out.sort_by(|a, b| a.handle.cmp(&b.handle));
            out
        })
    }

    fn known_entity<'a>(
        &'a self,
        session: BrowserSessionId,
        reference: &'a str,
    ) -> BoxFuture<'a, Option<modbit_browser::compiler::Entity>> {
        Box::pin(async move {
            self.sessions
                .lock()
                .await
                .get(&session)
                .and_then(|r| r.known.get(reference).cloned())
        })
    }
}

/// The wire view of a session.
pub(crate) fn view(id: BrowserSessionId, rec: &SessionRecord) -> wire::BrowserSessionView {
    wire::BrowserSessionView {
        browser_session_id: Some(wire::Id {
            value: id.as_bytes().to_vec(),
        }),
        task_id: Some(wire::Id {
            value: rec.task_id.as_bytes().to_vec(),
        }),
        partition: rec.partition.clone(),
        controller: match rec.lease.controller {
            modbit_browser::Controller::Agent => "AGENT".into(),
            modbit_browser::Controller::User => "USER".into(),
        },
        lease_generation: rec.lease.generation,
        host_attached: rec.host.is_some(),
        host_kind: rec
            .host
            .as_ref()
            .map(|h| h.kind.clone())
            .unwrap_or_default(),
        url: rec.state.url.clone(),
        title: rec.state.title.clone(),
        state_version: rec.state.state_version,
        fingerprint: rec.state.fingerprint(),
        closed: rec.closed,
    }
}

/// Rebuild a session record from the task's events (after a Core restart).
pub(crate) fn from_events(
    id: BrowserSessionId,
    task_id: TaskId,
    session_id: SessionId,
    events: &[serde_json::Value],
) -> Option<SessionRecord> {
    let sid = id.to_string();
    let mut rec: Option<SessionRecord> = None;
    for e in events {
        if e["browser_session_id"].as_str() != Some(sid.as_str()) {
            continue;
        }
        match e["__type"].as_str().unwrap_or_default() {
            "BrowserSessionOpened" => {
                rec = Some(SessionRecord {
                    task_id,
                    session_id,
                    partition: e["partition"].as_str().unwrap_or_default().to_owned(),
                    lease: ControlLease::initial(),
                    host: None,
                    state: PageState::default(),
                    closed: false,
                    known: HashMap::new(),
                    last_page: None,
                    transitions: Vec::new(),
                });
            }
            // IMP-EV-0280: transitions the session observed are rebuilt from
            // the log too — evidence survives a restart; a changed page will
            // not match their fingerprint.
            "BrowserActionPerformed" => {
                if let Some(r) = rec.as_mut()
                    && let (Some(from), Some(to)) = (
                        e["fingerprint_before"].as_str(),
                        e["fingerprint_after"].as_str(),
                    )
                    && !from.is_empty()
                    && !to.is_empty()
                {
                    r.transitions.push(modbit_browser::KnownTransition {
                        from_fingerprint: from.to_owned(),
                        reference: e["reference"].as_str().unwrap_or_default().to_owned(),
                        action: e["action"].as_str().unwrap_or_default().to_owned(),
                        to_fingerprint: to.to_owned(),
                        to_url: String::new(),
                        verified: e["postcondition_held"].as_bool(),
                        times: 1,
                    });
                }
            }
            "BrowserNavigated" => {
                if let Some(r) = rec.as_mut() {
                    r.state = PageState {
                        url: e["url"].as_str().unwrap_or_default().to_owned(),
                        title: e["title"].as_str().unwrap_or_default().to_owned(),
                        ready: true,
                        state_version: e["state_version"].as_u64().unwrap_or(0),
                    };
                }
            }
            "BrowserControlChanged" => {
                if let Some(r) = rec.as_mut() {
                    r.lease = ControlLease {
                        controller: if e["controller"].as_str() == Some("USER") {
                            modbit_browser::Controller::User
                        } else {
                            modbit_browser::Controller::Agent
                        },
                        generation: e["lease_generation"].as_u64().unwrap_or(r.lease.generation),
                    };
                }
            }
            "BrowserSessionClosed" => {
                if let Some(r) = rec.as_mut() {
                    r.closed = true;
                }
            }
            _ => {}
        }
    }
    rec
}

/// The task's events with their types, for [`from_events`].
pub(crate) fn task_events(
    store: &modbit_event_store::EventStore,
    task_id: TaskId,
) -> Vec<serde_json::Value> {
    store
        .read_aggregate(task_id.as_bytes(), 0, usize::MAX)
        .unwrap_or_default()
        .iter()
        .filter_map(|e| {
            let mut v = store.payload(&e.envelope).ok()?;
            v["__type"] = serde_json::json!(e.envelope.event_type);
            Some(v)
        })
        .collect()
}

/// The port the tool host hands `browser.*`.
pub(crate) fn port(sessions: &Arc<BrowserSessions>) -> Arc<dyn BrowserPort> {
    Arc::clone(sessions) as Arc<dyn BrowserPort>
}
