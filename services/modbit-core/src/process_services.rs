//! Process intelligence (REQ-PX-132, docs/21 "Durable modbit-execd"): the
//! Core knows which ports the processes of a task's terminals listen on,
//! whether a dev server is ready and whether it stays healthy, without asking
//! the model to rediscover it.
//!
//! The broker (`modbit-execd`) started the processes, so it alone answers
//! which listening sockets belong to a session's process tree
//! (`ListServices`). This module polls it for the running sessions of tasks,
//! classifies what it finds, probes it on loopback — a TCP connect, then one
//! bounded `GET` — and records every change of state as a
//! `ProcessServiceObserved` event on the owning task: `STARTING`, `READY`,
//! `UNHEALTHY`, `GONE`. The read model (`ListProcessServices`) and the lines
//! the agent's next tool result carries are folded from the same records.
//!
//! What this is not: authority. A detected server is a fact. It widens no
//! policy, grants no capability and is not a navigation target for the
//! browser; naming one remains the person's act. Probes never leave
//! loopback: a listener bound to another interface is reported as listening
//! and not probed.
//!
//! A restarted Core reattaches to the durable broker, finds the sessions
//! still running, reads each task's last recorded state per service from the
//! log, and records only what changed since — including `GONE` for a service
//! that stopped while the Core was down.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};

use modbit_domain::event::AggregateType;
use modbit_domain::task::TaskEvent;
use modbit_domain::{SessionId, TaskId};
use modbit_event_store::AppendRequest;
use modbit_protocol::v1 as wire;
use modbit_terminal::{Event, ExecClient};
use prost::Message;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::server::{Core, accept, id16, reject, typed, wire_id};

/// How often the broker is asked while a task has a running terminal.
const POLL_MS: u64 = 750;
/// How often it is asked while none has.
const IDLE_POLL_MS: u64 = 2_000;
/// The most services probed in one pass.
const MAX_PER_PASS: usize = 32;
/// The most tasks whose recorded state is read back in one pass.
const MAX_SEEDS_PER_PASS: usize = 8;
const TCP_TIMEOUT: Duration = Duration::from_millis(400);
const HTTP_TIMEOUT: Duration = Duration::from_millis(1_500);
/// A probe that fails this many times in a row turns a ready service
/// unhealthy: one dropped probe is not an outage.
const FAILURES_TO_UNHEALTHY: u32 = 2;
/// Notices kept for the agent per task; older ones are dropped.
const MAX_NOTICES: usize = 8;

/// The probe path (`MODBIT_SERVICE_PROBE_PATH`, default `/`): a `GET` to it
/// on the loopback address, after the TCP connect.
fn probe_path() -> String {
    std::env::var("MODBIT_SERVICE_PROBE_PATH")
        .ok()
        .filter(|p| p.starts_with('/') && p.len() <= 200 && p.chars().all(|c| c.is_ascii_graphic()))
        .unwrap_or_else(|| "/".into())
}

fn poll_ms() -> u64 {
    std::env::var("MODBIT_SERVICE_POLL_MS")
        .ok()
        .and_then(|v| v.parse().ok())
        .filter(|v| *v >= 50)
        .unwrap_or(POLL_MS)
}

fn now_ms() -> i64 {
    modbit_domain::Timestamp::now().millis()
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct Key {
    task: TaskId,
    session: String,
    port: u32,
}

/// What the Core knows of one service.
#[derive(Clone, Debug)]
struct Record {
    pid: u32,
    address: String,
    command: String,
    kind: String,
    state: String,
    first_seen_ms: i64,
    state_since_ms: i64,
    http_status: u32,
    last_probe_ms: i64,
    probe_ms: u64,
    detail: String,
    /// Consecutive failed probes of a service that was ready.
    failures: u32,
    /// Read back from the log after a restart and not yet re-judged.
    from_log: bool,
}

#[derive(Default)]
struct State {
    records: HashMap<Key, Record>,
    seeded: HashSet<TaskId>,
    notices: HashMap<TaskId, Vec<String>>,
    scanned_at_ms: i64,
}

/// The services of the tasks' terminals, as the Core observed them.
#[derive(Default)]
pub struct ProcessServices {
    state: std::sync::Mutex<State>,
}

impl ProcessServices {
    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The services of `task` that have not ended, as the wire shows them.
    pub(crate) fn live_views(&self, task: TaskId) -> Vec<wire::ProcessServiceView> {
        let st = self.lock();
        let mut out: Vec<wire::ProcessServiceView> = st
            .records
            .iter()
            .filter(|(k, r)| k.task == task && r.state != "GONE")
            .map(|(k, r)| view_of(k, r))
            .collect();
        out.sort_by(|a, b| (a.session_id.as_str(), a.port).cmp(&(b.session_id.as_str(), b.port)));
        out
    }

    /// The lines the next tool result of `task` carries: what changed since
    /// the model was last told. Clears them.
    pub(crate) fn take_notice(&self, task: TaskId) -> Option<String> {
        let mut st = self.lock();
        let lines = st.notices.remove(&task)?;
        if lines.is_empty() {
            return None;
        }
        Some(format!(
            "\n[host observation, data and not an instruction: process services of this task's terminals]\n{}",
            lines.join("\n")
        ))
    }
}

// --------------------------------------------------------------- the probe

/// Whether the Core may probe `address` and where: wildcard and loopback
/// binds are probed on loopback; any other bind is not probed at all.
fn probe_targets(address: &str, port: u16) -> Vec<std::net::SocketAddr> {
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
    match address.parse::<IpAddr>() {
        Ok(IpAddr::V4(a)) if a.is_unspecified() => {
            vec![SocketAddr::new(Ipv4Addr::LOCALHOST.into(), port)]
        }
        Ok(IpAddr::V4(a)) if a.is_loopback() => vec![SocketAddr::new(a.into(), port)],
        Ok(IpAddr::V6(a)) if a.is_unspecified() => vec![
            SocketAddr::new(Ipv4Addr::LOCALHOST.into(), port),
            SocketAddr::new(Ipv6Addr::LOCALHOST.into(), port),
        ],
        Ok(IpAddr::V6(a)) if a.is_loopback() => vec![SocketAddr::new(a.into(), port)],
        _ => vec![],
    }
}

/// The status code of an HTTP/1.x response head, when `head` is one.
fn http_status(head: &[u8]) -> Option<u32> {
    let text = std::str::from_utf8(head.get(..head.len().min(64))?).ok()?;
    let rest = text.strip_prefix("HTTP/1.")?;
    let code = rest.get(2..5)?;
    if !rest.get(1..2)?.eq(" ") {
        return None;
    }
    let n: u32 = code.parse().ok()?;
    (100..600).contains(&n).then_some(n)
}

/// What one probe found.
#[derive(Debug, PartialEq, Eq)]
enum Probe {
    /// Not probed: the listener is on an interface the Core does not probe.
    NotProbed,
    /// Nothing accepted the connection in time.
    Refused(String),
    /// The connection was accepted; `status` is the HTTP status when the
    /// service answered HTTP and `None` when it did not speak it (a plain
    /// TCP service).
    Answered { status: Option<u32>, ms: u64 },
}

async fn probe(address: &str, port: u16) -> Probe {
    let targets = probe_targets(address, port);
    if targets.is_empty() {
        return Probe::NotProbed;
    }
    let started = Instant::now();
    let mut last = String::from("no address to probe");
    for target in targets {
        let mut stream =
            match tokio::time::timeout(TCP_TIMEOUT, tokio::net::TcpStream::connect(target)).await {
                Ok(Ok(s)) => s,
                Ok(Err(e)) => {
                    last = format!("connect {target}: {e}");
                    continue;
                }
                Err(_) => {
                    last = format!("connect {target}: timed out");
                    continue;
                }
            };
        let request = format!(
            "GET {} HTTP/1.1\r\nHost: localhost:{port}\r\nUser-Agent: modbit-probe\r\nAccept: */*\r\nConnection: close\r\n\r\n",
            probe_path()
        );
        let status = async {
            stream.write_all(request.as_bytes()).await.ok()?;
            let mut buf = [0u8; 64];
            let mut got = 0;
            while got < buf.len() {
                let n = stream.read(&mut buf[got..]).await.ok()?;
                if n == 0 {
                    break;
                }
                got += n;
                if got >= 12 {
                    break;
                }
            }
            http_status(&buf[..got])
        };
        let status = tokio::time::timeout(HTTP_TIMEOUT, status)
            .await
            .ok()
            .flatten();
        return Probe::Answered {
            status,
            ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
        };
    }
    Probe::Refused(last)
}

// ---------------------------------------------------------- classification

/// Whether a command line is one a dev server runs as.
fn looks_like_dev_server(command: &str) -> bool {
    let lower = command.to_lowercase();
    let words: Vec<&str> = lower
        .split(|c: char| c.is_whitespace() || c == '/' || c == '\\')
        .collect();
    let has = |w: &str| words.contains(&w);
    const RUNTIMES: &[&str] = &[
        "node",
        "nodejs",
        "deno",
        "bun",
        "npm",
        "npx",
        "pnpm",
        "yarn",
        "vite",
        "next",
        "nuxt",
        "webpack",
        "webpack-dev-server",
        "parcel",
        "astro",
        "remix",
        "svelte-kit",
        "ng",
        "uvicorn",
        "gunicorn",
        "flask",
        "rails",
        "puma",
        "hugo",
        "jekyll",
        "trunk",
        "nodemon",
        "http-server",
        "live-server",
        "serve",
    ];
    RUNTIMES.iter().any(|r| has(r))
        || (words.iter().any(|w| w.starts_with("python")) && lower.contains("http.server"))
        || (words.iter().any(|w| w.starts_with("python"))
            && (lower.contains("runserver")
                || lower.contains("flask")
                || lower.contains("uvicorn")))
        || (has("cargo") && (has("run") || has("watch")))
        || (has("go") && has("run"))
        || (has("php") && lower.contains(" -s "))
        || (has("dotnet") && (has("watch") || has("run")))
}

/// The state a probe result puts a service in, given where it was.
fn judged(previous: Option<&Record>, probe: &Probe) -> (String, u32, u64, String, u32) {
    // (state, http_status, probe_ms, detail, consecutive failures)
    match probe {
        Probe::NotProbed => (
            "READY".into(),
            0,
            0,
            "listening on an interface other than loopback; not probed (probes never leave loopback)"
                .into(),
            0,
        ),
        Probe::Answered { status: Some(s), ms } if *s >= 500 => (
            "UNHEALTHY".into(),
            *s,
            *ms,
            format!("the service answered HTTP {s}"),
            previous.map_or(0, |p| p.failures) + 1,
        ),
        Probe::Answered { status, ms } => (
            "READY".into(),
            status.unwrap_or(0),
            *ms,
            match status {
                Some(s) => format!("the service answered HTTP {s}"),
                None => "the port accepts connections".into(),
            },
            0,
        ),
        Probe::Refused(why) => {
            let failures = previous.map_or(0, |p| p.failures) + 1;
            match previous.map(|p| p.state.as_str()) {
                // Bound but not accepting yet.
                None | Some("STARTING") => ("STARTING".into(), 0, 0, why.clone(), failures),
                Some(_) if failures >= FAILURES_TO_UNHEALTHY => (
                    "UNHEALTHY".into(),
                    0,
                    0,
                    format!("{failures} probes in a row failed: {why}"),
                    failures,
                ),
                Some(state) => (state.to_owned(), 0, 0, why.clone(), failures),
            }
        }
    }
}

// ------------------------------------------------------------ the poll loop

/// Start the observer: it lives as long as the Core.
pub(crate) fn spawn(core: Arc<Core>) {
    tokio::spawn(async move {
        let mut client: Option<ExecClient> = None;
        loop {
            let delay = match pass(&core, &mut client).await {
                Ok(true) => poll_ms(),
                Ok(false) => IDLE_POLL_MS,
                Err(e) => {
                    // The broker is away or answered badly: reconnect next time.
                    client = None;
                    eprintln!("modbit-core: process services: {e}");
                    IDLE_POLL_MS
                }
            };
            tokio::time::sleep(Duration::from_millis(delay)).await;
        }
    });
}

async fn next_event(c: &mut ExecClient) -> Result<Event, String> {
    match tokio::time::timeout(Duration::from_secs(10), c.next()).await {
        Ok(Ok(Some(e))) => Ok(e),
        Ok(Ok(None)) => Err("the broker closed".into()),
        Ok(Err(e)) => Err(e.to_string()),
        Err(_) => Err("the broker did not answer in 10s".into()),
    }
}

fn owner_task(owner: &str) -> Option<TaskId> {
    owner
        .strip_prefix("task:")
        .and_then(|t| TaskId::parse(t).ok())
}

/// One observation pass. `Ok(true)` when a task has a running terminal (poll
/// again soon), `Ok(false)` when the Core is idle.
async fn pass(core: &Arc<Core>, client: &mut Option<ExecClient>) -> Result<bool, String> {
    let Some(execd) = &core.tools.execd else {
        return Ok(false);
    };
    if client.is_none() {
        *client = Some(
            ExecClient::connect(&execd.target.endpoint, &execd.target.boot_secret)
                .await
                .map_err(|e| e.to_string())?,
        );
    }
    let c = client.as_mut().expect("connected above");
    c.list().await.map_err(|e| e.to_string())?;
    let sessions = loop {
        if let Event::Sessions(l) = next_event(c).await? {
            break l;
        }
    };
    // Running sessions of tasks, and every task the broker names a session of
    // (a service recorded for a session that has since ended is `GONE`).
    let mut running: HashSet<(TaskId, String)> = HashSet::new();
    let mut tasks: Vec<TaskId> = Vec::new();
    for s in &sessions {
        let Some(task) = owner_task(&s.owner) else {
            continue;
        };
        if !tasks.contains(&task) {
            tasks.push(task);
        }
        if s.running {
            running.insert((task, s.session_id.clone()));
        }
    }
    seed(core, &tasks).await;
    let listeners = if running.is_empty() {
        None
    } else {
        c.list_services().await.map_err(|e| e.to_string())?;
        loop {
            if let Event::Listeners(l) = next_event(c).await? {
                break Some(l);
            }
        }
    };
    if let Some(l) = &listeners
        && !l.scan_error.is_empty()
    {
        // A failed scan says nothing about the services: judge none.
        return Ok(true);
    }
    let scanned: Vec<wire::SessionListener> = listeners
        .map(|l| l.listeners)
        .unwrap_or_default()
        .into_iter()
        .filter(|l| {
            sessions
                .iter()
                .find(|s| s.session_id == l.session_id)
                .and_then(|s| owner_task(&s.owner))
                .is_some_and(|t| running.contains(&(t, l.session_id.clone())))
        })
        .collect();
    let session_task: HashMap<&str, TaskId> = sessions
        .iter()
        .filter_map(|s| Some((s.session_id.as_str(), owner_task(&s.owner)?)))
        .collect();

    // ---- judge what is listening
    let mut seen: HashSet<Key> = HashSet::new();
    let mut changes: Vec<(TaskId, Key, Record, Option<String>, bool)> = Vec::new();
    for l in scanned.iter().take(MAX_PER_PASS) {
        let Some(task) = session_task.get(l.session_id.as_str()).copied() else {
            continue;
        };
        let key = Key {
            task,
            session: l.session_id.clone(),
            port: l.port,
        };
        seen.insert(key.clone());
        let Ok(port) = u16::try_from(l.port) else {
            continue;
        };
        let before = core
            .tools
            .process_services
            .lock()
            .records
            .get(&key)
            .cloned();
        let result = probe(&l.address, port).await;
        let (state, status, ms, detail, failures) = judged(before.as_ref(), &result);
        let now = now_ms();
        let command = core.tools.redactor().error_text(&l.command);
        let kind = if looks_like_dev_server(&l.command) || status > 0 {
            "dev_server"
        } else {
            "service"
        };
        let record = Record {
            pid: l.pid,
            address: l.address.clone(),
            command,
            kind: kind.into(),
            state: state.clone(),
            first_seen_ms: before
                .as_ref()
                .filter(|b| b.state != "GONE")
                .map_or(now, |b| b.first_seen_ms),
            state_since_ms: match &before {
                Some(b) if b.state == state => b.state_since_ms,
                _ => now,
            },
            http_status: status,
            last_probe_ms: now,
            probe_ms: ms,
            detail,
            failures,
            from_log: false,
        };
        let moved = before.as_ref().is_none_or(|b| b.state != state);
        let previous = before.as_ref().map(|b| b.state.clone());
        let reobserved = before.as_ref().is_some_and(|b| b.from_log);
        if moved {
            changes.push((task, key, record, previous, reobserved));
        } else {
            core.tools
                .process_services
                .lock()
                .records
                .insert(key, record);
        }
    }
    // ---- judge what is no longer listening
    let gone: Vec<(Key, Record)> = {
        let st = core.tools.process_services.lock();
        st.records
            .iter()
            .filter(|(k, r)| r.state != "GONE" && !seen.contains(*k))
            // A task whose broker session we have no listing for (the broker
            // is the only authority) is not judged.
            .filter(|(k, _)| tasks.contains(&k.task))
            .map(|(k, r)| (k.clone(), r.clone()))
            .collect()
    };
    for (key, mut r) in gone {
        let running_still = running.contains(&(key.task, key.session.clone()));
        let reobserved = r.from_log;
        let previous = Some(r.state.clone());
        r.state = "GONE".into();
        r.state_since_ms = now_ms();
        r.detail = if running_still {
            "the listening socket was closed".into()
        } else {
            "the terminal that owned the process ended".into()
        };
        r.failures = 0;
        r.from_log = false;
        changes.push((key.task, key, r, previous, reobserved));
    }
    for (task, key, record, previous, reobserved) in changes {
        publish(core, task, key, record, previous, reobserved).await;
    }
    core.tools.process_services.lock().scanned_at_ms = now_ms();
    Ok(!running.is_empty())
}

/// Read back, once per task, the last recorded state of each of its services.
async fn seed(core: &Arc<Core>, tasks: &[TaskId]) {
    let todo: Vec<TaskId> = {
        let st = core.tools.process_services.lock();
        tasks
            .iter()
            .filter(|t| !st.seeded.contains(t))
            .take(MAX_SEEDS_PER_PASS)
            .copied()
            .collect()
    };
    for task in todo {
        let found = seed_task(core, task).await;
        let mut st = core.tools.process_services.lock();
        st.seeded.insert(task);
        for (k, r) in found {
            st.records.entry(k).or_insert(r);
        }
    }
}

async fn seed_task(core: &Core, task: TaskId) -> Vec<(Key, Record)> {
    let store = core.store.lock().await;
    let Ok(events) = store.read_aggregate_of_types(task.as_bytes(), &["ProcessServiceObserved"], 0)
    else {
        return vec![];
    };
    fold_events(&store, task, events)
}

/// Record a change of state on the task's log and tell the agent.
async fn publish(
    core: &Arc<Core>,
    task: TaskId,
    key: Key,
    record: Record,
    previous: Option<String>,
    reobserved: bool,
) {
    let previous = previous.unwrap_or_default();
    let event = TaskEvent::ProcessServiceObserved {
        handle_id: key.session.clone(),
        port: key.port,
        address: record.address.clone(),
        pid: record.pid,
        command: record.command.clone(),
        kind: record.kind.clone(),
        state: record.state.clone(),
        previous: previous.clone(),
        http_status: record.http_status,
        probe_ms: record.probe_ms,
        detail: record.detail.clone(),
        reobserved,
    };
    let session: Option<SessionId> = {
        let store = core.store.lock().await;
        store.task(&task).ok().flatten().map(|t| t.session_id)
    };
    let Some(session_id) = session else { return };
    let appended = {
        let mut store = core.store.lock().await;
        store.append(AppendRequest {
            tenant_id: core.tenant_id,
            session_id,
            task_id: Some(task),
            run_id: None,
            turn_id: None,
            step_id: None,
            aggregate_type: AggregateType::Task,
            aggregate_id: *task.as_bytes(),
            expected_sequence: None,
            events: vec![typed(
                "ProcessServiceObserved",
                &event,
                modbit_domain::event::Actor::Core("process-services".into()),
            )],
        })
    };
    match appended {
        Ok(stored) => {
            if let Some(last) = stored.last() {
                core.last_offset.send_replace(last.offset);
            }
        }
        Err(e) => {
            eprintln!("modbit-core: recording a service of task {task}: {e}");
            return;
        }
    }
    let line = format!(
        "- {state} {kind} {address}:{port} (terminal {handle}) `{command}`{detail}",
        state = record.state,
        kind = record.kind,
        address = record.address,
        port = key.port,
        handle = key.session,
        command = record.command.chars().take(120).collect::<String>(),
        detail = if record.state == "UNHEALTHY" || record.state == "GONE" {
            format!(": {}", record.detail)
        } else {
            String::new()
        },
    );
    let mut st = core.tools.process_services.lock();
    let mut record = record;
    record.from_log = false;
    st.records.insert(key, record);
    let lines = st.notices.entry(task).or_default();
    lines.push(core.tools.redactor().error_text(&line));
    if lines.len() > MAX_NOTICES {
        let drop = lines.len() - MAX_NOTICES;
        lines.drain(..drop);
    }
}

// ----------------------------------------------------------- the read model

fn view_of(key: &Key, r: &Record) -> wire::ProcessServiceView {
    wire::ProcessServiceView {
        task_id: Some(wire_id(key.task.as_bytes())),
        session_id: key.session.clone(),
        port: key.port,
        address: r.address.clone(),
        pid: r.pid,
        command: r.command.clone(),
        kind: r.kind.clone(),
        state: r.state.clone(),
        http_status: i32::try_from(r.http_status).unwrap_or(0),
        first_seen_ms: r.first_seen_ms,
        state_since_ms: r.state_since_ms,
        last_probe_ms: r.last_probe_ms,
        probe_latency_ms: r.probe_ms,
        detail: r.detail.clone(),
    }
}

/// `ListProcessServices`: the services of one task (or every task).
pub(crate) async fn list(core: &Core, env: wire::CommandEnvelope) -> wire::CommandAck {
    let cid = env.command_id.clone();
    let Ok(p) = wire::ListProcessServices::decode(env.payload.as_slice()) else {
        return reject(cid, "BAD_PAYLOAD", "ListProcessServices");
    };
    let filter = match p.task_id.as_ref() {
        None => None,
        Some(id) => match id16(id) {
            Some(b) => Some(TaskId::from_bytes(b)),
            None => return reject(cid, "BAD_PAYLOAD", "task_id must be 16 bytes"),
        },
    };
    // A client may ask before the observer has read a restarted task's log.
    if let Some(t) = filter {
        let known = core.tools.process_services.lock().seeded.contains(&t);
        if !known {
            let found = seed_task(core, t).await;
            let mut st = core.tools.process_services.lock();
            st.seeded.insert(t);
            for (k, r) in found {
                st.records.entry(k).or_insert(r);
            }
        }
    }
    let st = core.tools.process_services.lock();
    let mut services: Vec<wire::ProcessServiceView> = st
        .records
        .iter()
        .filter(|(k, r)| {
            filter.is_none_or(|t| k.task == t) && (p.include_gone || r.state != "GONE")
        })
        .map(|(k, r)| view_of(k, r))
        .collect();
    services.sort_by(|a, b| {
        (
            a.task_id.as_ref().map(|i| i.value.clone()),
            a.session_id.as_str(),
            a.port,
        )
            .cmp(&(
                b.task_id.as_ref().map(|i| i.value.clone()),
                b.session_id.as_str(),
                b.port,
            ))
    });
    accept(
        cid,
        false,
        wire::ProcessServiceList {
            services,
            scanned_at_ms: st.scanned_at_ms,
        }
        .encode_to_vec(),
    )
}

fn fold_events(
    store: &modbit_event_store::EventStore,
    task: TaskId,
    events: Vec<modbit_event_store::StoredEvent>,
) -> Vec<(Key, Record)> {
    let mut last: HashMap<Key, Record> = HashMap::new();
    for e in events {
        let Ok(p) = store.payload(&e.envelope) else {
            continue;
        };
        let Ok(TaskEvent::ProcessServiceObserved {
            handle_id,
            port,
            address,
            pid,
            command,
            kind,
            state,
            http_status,
            probe_ms,
            detail,
            ..
        }) = serde_json::from_value::<TaskEvent>(p)
        else {
            continue;
        };
        let at = e.envelope.occurred_at.millis();
        let key = Key {
            task,
            session: handle_id,
            port,
        };
        let first = last.get(&key).map_or(at, |r| r.first_seen_ms);
        last.insert(
            key,
            Record {
                pid,
                address,
                command,
                kind,
                state,
                first_seen_ms: first,
                state_since_ms: at,
                http_status,
                last_probe_ms: at,
                probe_ms,
                detail,
                failures: 0,
                from_log: true,
            },
        );
    }
    last.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_wildcard_and_loopback_binds_are_probed_and_only_on_loopback() {
        use std::net::{IpAddr, Ipv4Addr};
        let loop4 = IpAddr::V4(Ipv4Addr::LOCALHOST);
        for bind in ["0.0.0.0", "127.0.0.1", "127.1.2.3"] {
            let t = probe_targets(bind, 80);
            assert_eq!(t.len(), 1, "{bind}");
            assert!(t[0].ip().is_loopback());
        }
        assert_eq!(
            probe_targets("0.0.0.0", 80)[0].ip(),
            loop4,
            "a wildcard bind is reached on 127.0.0.1"
        );
        let v6 = probe_targets("::", 80);
        assert!(v6.iter().all(|t| t.ip().is_loopback()) && v6.len() == 2);
        assert_eq!(probe_targets("::1", 80).len(), 1);
        for bind in [
            "192.168.1.5",
            "10.0.0.2",
            "8.8.8.8",
            "fe80::1",
            "example.com",
            "",
        ] {
            assert!(probe_targets(bind, 80).is_empty(), "{bind} is never probed");
        }
    }

    #[test]
    fn an_http_status_line_is_read_and_anything_else_is_not_http() {
        assert_eq!(http_status(b"HTTP/1.1 200 OK\r\n"), Some(200));
        assert_eq!(http_status(b"HTTP/1.0 503 Service Unavailable"), Some(503));
        assert_eq!(http_status(b"SSH-2.0-OpenSSH_9"), None);
        assert_eq!(http_status(b"HTTP/1.1 9999 x"), None);
        assert_eq!(http_status(b""), None);
        assert_eq!(http_status(b"\x16\x03\x01\x02\x00"), None);
    }

    #[test]
    fn dev_servers_are_recognised_by_their_command_and_other_processes_are_not() {
        for c in [
            "node server.js",
            "/usr/local/bin/node /app/server.js --port 3000",
            "python3 -m http.server 8000",
            "python manage.py runserver",
            "npm run dev",
            "pnpm exec vite --host",
            "cargo run --bin web",
            "go run ./cmd/api",
            "uvicorn app:app --reload",
        ] {
            assert!(looks_like_dev_server(c), "{c}");
        }
        for c in [
            "postgres -D /data",
            "/usr/sbin/sshd",
            "redis-server *:6379",
            "",
        ] {
            assert!(!looks_like_dev_server(c), "{c}");
        }
    }

    fn rec(state: &str, failures: u32) -> Record {
        Record {
            pid: 1,
            address: "127.0.0.1".into(),
            command: String::new(),
            kind: "dev_server".into(),
            state: state.into(),
            first_seen_ms: 0,
            state_since_ms: 0,
            http_status: 0,
            last_probe_ms: 0,
            probe_ms: 0,
            detail: String::new(),
            failures,
            from_log: false,
        }
    }

    #[test]
    fn the_state_machine_needs_two_failed_probes_to_turn_a_ready_service_unhealthy() {
        let refused = Probe::Refused("connect: refused".into());
        assert_eq!(judged(None, &refused).0, "STARTING");
        assert_eq!(judged(Some(&rec("STARTING", 3)), &refused).0, "STARTING");
        let one = judged(Some(&rec("READY", 0)), &refused);
        assert_eq!((one.0.as_str(), one.4), ("READY", 1));
        assert_eq!(judged(Some(&rec("READY", 1)), &refused).0, "UNHEALTHY");
        let ok = Probe::Answered {
            status: Some(200),
            ms: 3,
        };
        assert_eq!(judged(Some(&rec("UNHEALTHY", 5)), &ok).0, "READY");
        assert_eq!(judged(None, &ok).0, "READY");
        let bad = Probe::Answered {
            status: Some(500),
            ms: 3,
        };
        assert_eq!(judged(None, &bad).0, "UNHEALTHY", "listening but erroring");
        let tcp = Probe::Answered {
            status: None,
            ms: 1,
        };
        assert_eq!(judged(None, &tcp).0, "READY", "a plain TCP service");
        assert_eq!(judged(None, &Probe::NotProbed).0, "READY");
    }
}
