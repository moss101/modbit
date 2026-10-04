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
    /// M8.8: a host the Core runs itself (`cloud-cdp`) takes view control
    /// — watchers of the page's screencast and the person's input; a
    /// desktop host has none (its view is on the desktop already).
    pub view: Option<mpsc::Sender<crate::browser_cloud::ViewControl>>,
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

    /// The view control of the session's host, when the host streams (M8.8).
    pub(crate) async fn view_control(
        &self,
        id: BrowserSessionId,
    ) -> Option<(String, mpsc::Sender<crate::browser_cloud::ViewControl>)> {
        let s = self.sessions.lock().await;
        let rec = s.get(&id)?;
        let h = rec.host.as_ref()?;
        h.view.clone().map(|v| (h.kind.clone(), v))
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

impl BrowserSessions {
    /// One request to the session's host, under the control lease. An agent
    /// input (`Navigate`, `Act`) is stamped with the lease generation its
    /// caller decided it under (`observed`; the current one when the caller
    /// did not say) and is refused `STALE_GENERATION` when the session has
    /// been handed over since — checked against the generation the session
    /// holds when the request is read and again as it is sent, not against
    /// itself. The frame carries the stamp, so the host fences it too.
    async fn send(
        &self,
        session: BrowserSessionId,
        request: HostRequest,
        observed: Option<u64>,
    ) -> Result<HostResponse, PortError> {
        {
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
            // it reaches the host (docs/22: takeover blocks input at once);
            // one decided under an earlier generation is refused as stale.
            let agent_input = matches!(
                request,
                HostRequest::Navigate { .. } | HostRequest::Act { .. }
            );
            let stamp = observed.unwrap_or(lease.generation);
            if agent_input {
                if lease.controller != modbit_browser::Controller::Agent {
                    return Err(PortError::Refused {
                        code: "HUMAN_ACTIVE".into(),
                        message: format!(
                            "the person holds control of the session (lease generation {})",
                            lease.generation
                        ),
                    });
                }
                if !lease.admits_agent_input(stamp) {
                    return Err(stale_generation(stamp, lease.generation));
                }
            }
            let request_id = BrowserSessionId::new().to_string();
            let (tx, rx) = oneshot::channel();
            self.pending
                .lock()
                .await
                .insert(request_id.clone(), (session, tx));
            // The awaits above are where a hand-over can land: the lease
            // read again, now, is what the stamp is held against.
            if agent_input {
                let now = self.sessions.lock().await.get(&session).map(|r| r.lease);
                match now {
                    Some(l) if l.admits_agent_input(stamp) => {}
                    Some(l) if l.controller != modbit_browser::Controller::Agent => {
                        self.pending.lock().await.remove(&request_id);
                        return Err(PortError::Refused {
                            code: "HUMAN_ACTIVE".into(),
                            message: format!(
                                "the person took control of the session (lease generation {})",
                                l.generation
                            ),
                        });
                    }
                    other => {
                        self.pending.lock().await.remove(&request_id);
                        return Err(stale_generation(stamp, other.map_or(0, |l| l.generation)));
                    }
                }
            }
            let frame = wire::BrowserHostRequest {
                request_id: request_id.clone(),
                browser_session_id: Some(wire::Id {
                    value: session.as_bytes().to_vec(),
                }),
                // An agent input carries the generation it was decided under;
                // an observation is not fenced and carries the current one.
                lease_generation: if agent_input { stamp } else { lease.generation },
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
        }
    }
}

/// The refusal for an agent input decided under a lease generation the
/// session has since moved past (docs/22: an input from before a hand-over
/// never lands after it).
fn stale_generation(stamped: u64, current: u64) -> PortError {
    PortError::Refused {
        code: "STALE_GENERATION".into(),
        message: format!(
            "the input was decided under lease generation {stamped}; the session is at {current}"
        ),
    }
}

impl BrowserPort for BrowserSessions {
    fn request<'a>(
        &'a self,
        session: BrowserSessionId,
        request: HostRequest,
    ) -> BoxFuture<'a, Result<HostResponse, PortError>> {
        Box::pin(self.send(session, request, None))
    }

    fn request_stamped<'a>(
        &'a self,
        session: BrowserSessionId,
        request: HostRequest,
        observed_generation: Option<u64>,
    ) -> BoxFuture<'a, Result<HostResponse, PortError>> {
        Box::pin(self.send(session, request, observed_generation))
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

#[cfg(test)]
mod tests {
    //! FIX-19: the Core's control-lease fence compares the generation an
    //! agent input was decided under with the generation the session holds
    //! now — a real fence, no longer the lease's generation against itself.
    use super::*;
    use modbit_browser::Controller;

    /// A session with a host that answers every request with the page state
    /// and records the lease generation each frame carried.
    async fn session_with_host() -> (
        Arc<BrowserSessions>,
        BrowserSessionId,
        Arc<std::sync::Mutex<Vec<u64>>>,
    ) {
        let sessions = Arc::new(BrowserSessions::default());
        let id = BrowserSessionId::new();
        sessions.open(id, TaskId::new(), SessionId::new()).await;
        let (tx, mut rx) = mpsc::channel::<wire::BrowserHostRequest>(8);
        sessions
            .attach(
                id,
                HostLink {
                    kind: "test".into(),
                    tx,
                    connection: 1,
                    view: None,
                },
            )
            .await
            .unwrap();
        let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
        let (s2, seen2) = (Arc::clone(&sessions), Arc::clone(&seen));
        tokio::spawn(async move {
            while let Some(frame) = rx.recv().await {
                seen2.lock().unwrap().push(frame.lease_generation);
                s2.deliver(
                    &frame.request_id,
                    HostResponse::State {
                        state: PageState::default(),
                    },
                )
                .await;
            }
        });
        (sessions, id, seen)
    }

    fn act() -> HostRequest {
        HostRequest::Act {
            backend_dom_node_id: 1,
            action: "click".into(),
            value: String::new(),
            key: String::new(),
            at: None,
            credential_handle: None,
        }
    }

    fn refused_code(r: Result<HostResponse, PortError>) -> String {
        match r {
            Err(PortError::Refused { code, .. }) => code,
            other => panic!("expected a refusal, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn an_input_decided_before_a_hand_over_is_stale_after_it_even_when_the_agent_holds_control_again()
     {
        let (sessions, id, seen) = session_with_host().await;
        // Decided under generation 1, sent under generation 1: lands, stamped 1.
        sessions
            .request_stamped(id, act(), Some(1))
            .await
            .expect("current generation lands");
        assert_eq!(*seen.lock().unwrap(), vec![1]);
        // The person takes control (2) and returns it (3): the agent holds
        // the session again, but an input decided under 1 is not applied.
        sessions.hand_control(id, Controller::User).await.unwrap();
        sessions.hand_control(id, Controller::Agent).await.unwrap();
        let code = refused_code(sessions.request_stamped(id, act(), Some(1)).await);
        assert_eq!(code, "STALE_GENERATION");
        let code = refused_code(
            sessions
                .request_stamped(
                    id,
                    HostRequest::Navigate {
                        url: "https://a.test/".into(),
                    },
                    Some(2),
                )
                .await,
        );
        assert_eq!(code, "STALE_GENERATION", "generation 2 was the person's");
        assert_eq!(
            *seen.lock().unwrap(),
            vec![1],
            "nothing stale reached the host"
        );
        // Decided under the current generation: lands, and the host is told
        // the generation it was decided under.
        sessions
            .request_stamped(id, act(), Some(3))
            .await
            .expect("current generation lands");
        // A caller that does not say (`request`) is stamped with the current one.
        sessions.request(id, act()).await.expect("unstamped lands");
        assert_eq!(*seen.lock().unwrap(), vec![1, 3, 3]);
    }

    #[tokio::test]
    async fn the_person_holding_control_is_human_active_and_observation_is_never_fenced() {
        let (sessions, id, seen) = session_with_host().await;
        sessions.hand_control(id, Controller::User).await.unwrap();
        // Whatever the stamp, while the person holds control the input is
        // HUMAN_ACTIVE (the model is told to wait, not to re-read).
        for stamp in [None, Some(1), Some(2)] {
            let code = refused_code(sessions.request_stamped(id, act(), stamp).await);
            assert_eq!(code, "HUMAN_ACTIVE", "{stamp:?}");
        }
        // Observation is not an agent input: allowed under any stamp, any holder.
        for stamp in [None, Some(1), Some(99)] {
            sessions
                .request_stamped(id, HostRequest::State, stamp)
                .await
                .expect("observation is never fenced");
        }
        assert_eq!(
            seen.lock().unwrap().len(),
            3,
            "only the observations reached the host"
        );
    }
}
