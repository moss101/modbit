//! The runtime's logic against an in-process actuator that answers from a
//! script: serialisation, the latch (an input is never retried after it may
//! have been delivered), the single controller, the sticky stop. The same
//! paths run over a real process boundary in `modbit-core`'s `px_computer`
//! tests; these pin the rules where a clock or a race would make a process
//! test slow or flaky.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use modbit_domain::{SessionId, TaskId};
use modbit_protocol::v1 as wire;
use serde_json::json;
use tokio::sync::broadcast;

use super::*;
use crate::actuator::{Actuator, ActuatorInfo, BoxFuture, Call, CallError, Reply};
use crate::ops::{Act, ElementRef, Op, parse};
use crate::taxonomy::Code;

type Handler = Box<dyn Fn(&Call) -> Result<Reply, CallError> + Send + Sync>;

struct Fake {
    info: ActuatorInfo,
    handler: Handler,
    delay: Duration,
    log: Arc<Mutex<Vec<String>>>,
    events: broadcast::Sender<wire::ActuatorEvent>,
    alive: AtomicBool,
}

fn name_of(c: &Call) -> &'static str {
    match c {
        Call::ListApplications(_) => "list",
        Call::ResolveApplication(_) => "resolve",
        Call::Acquire(_) => "acquire",
        Call::Release(_) => "release",
        Call::ReadState(_) => "read_state",
        Call::Capture(_) => "capture",
        Call::Wait(_) => "wait",
        Call::Perform(_) => "perform",
        Call::Stop(_) => "stop",
        Call::Ping(_) => "ping",
    }
}

impl Actuator for Fake {
    fn info(&self) -> &ActuatorInfo {
        &self.info
    }

    fn call<'a>(
        &'a self,
        call: Call,
        _deadline: Duration,
    ) -> BoxFuture<'a, Result<Reply, CallError>> {
        Box::pin(async move {
            let n = name_of(&call);
            self.log.lock().unwrap().push(format!("{n}:start"));
            if matches!(call, Call::Perform(_)) {
                tokio::time::sleep(self.delay).await;
            }
            let r = (self.handler)(&call);
            self.log.lock().unwrap().push(format!("{n}:end"));
            r
        })
    }

    fn events(&self) -> broadcast::Receiver<wire::ActuatorEvent> {
        self.events.subscribe()
    }

    fn is_alive(&self) -> bool {
        self.alive.load(Ordering::SeqCst)
    }

    fn closed<'a>(&'a self) -> BoxFuture<'a, ()> {
        Box::pin(std::future::pending())
    }

    fn kill(&self) {
        self.alive.store(false, Ordering::SeqCst);
    }
}

fn ident(bundle: &str, pid: u32) -> wire::ApplicationIdentity {
    wire::ApplicationIdentity {
        bundle_id: bundle.into(),
        executable_path: format!("/apps/{bundle}"),
        signing_identity: "TEAM".into(),
        pid,
        name: bundle.into(),
    }
}

fn app(bundle: &str, pid: u32) -> wire::ApplicationInfo {
    wire::ApplicationInfo {
        identity: Some(ident(bundle, pid)),
        windows: vec![wire::WindowInfo {
            window_id: "w1".into(),
            title: "Main".into(),
            bounds: Some(wire::Rect {
                x: 0,
                y: 0,
                width: 800,
                height: 600,
            }),
            focused: true,
            ..Default::default()
        }],
        elevated: false,
    }
}

/// An actuator whose `Perform` answers `perform`; everything else answers sensibly.
fn fake(
    delay_ms: u64,
    perform: impl Fn() -> Result<Reply, CallError> + Send + Sync + 'static,
) -> (Arc<Fake>, Arc<Mutex<Vec<String>>>) {
    let log: Arc<Mutex<Vec<String>>> = Arc::default();
    let handler: Handler = Box::new(move |c| match c {
        Call::ListApplications(_) => Ok(Reply::ListApplications(wire::ListApplicationsResponse {
            applications: vec![app("org.example.a", 11), app("org.example.b", 12)],
        })),
        Call::ResolveApplication(r) => Ok(Reply::ResolveApplication(
            wire::ResolveApplicationResponse {
                application: Some(app(
                    if r.pid == 12 {
                        "org.example.b"
                    } else {
                        "org.example.a"
                    },
                    r.pid,
                )),
            },
        )),
        Call::Acquire(a) => Ok(Reply::Acquire(wire::AcquireResponse {
            control_id: a.control_id.clone(),
            lease_generation: 1,
            application: Some(app(
                &a.application.as_ref().unwrap().bundle_id,
                a.application.as_ref().unwrap().pid,
            )),
        })),
        Call::Release(_) => Ok(Reply::Release(wire::ReleaseResponse { was_held: true })),
        Call::ReadState(r) => Ok(Reply::ReadState(wire::ReadStateResponse {
            snapshot_id: r.snapshot_id.clone(),
            window: Some(app("x", 1).windows[0].clone()),
            nodes: vec![
                wire::AxNode {
                    element_id: "n0".into(),
                    role: "window".into(),
                    name: "Main".into(),
                    enabled: true,
                    ..Default::default()
                },
                wire::AxNode {
                    element_id: "n1".into(),
                    parent_id: "n0".into(),
                    role: "button".into(),
                    name: "Go".into(),
                    enabled: true,
                    actions: vec!["press".into()],
                    ..Default::default()
                },
            ],
            truncated: false,
            tree_digest: "d".into(),
        })),
        Call::Perform(_) => perform(),
        Call::Stop(_) => Ok(Reply::Stop(wire::StopResponse::default())),
        Call::Ping(_) => Ok(Reply::Ping(wire::PingResponse::default())),
        _ => Err(CallError::Protocol("unscripted".into())),
    });
    let (events, _) = broadcast::channel(8);
    (
        Arc::new(Fake {
            info: ActuatorInfo {
                name: "fake".into(),
                version: "0".into(),
                pid: 1,
                platform: "fake".into(),
                capabilities: vec![],
            },
            handler,
            delay: Duration::from_millis(delay_ms),
            log: Arc::clone(&log),
            events,
            alive: AtomicBool::new(true),
        }),
        log,
    )
}

fn performed() -> Result<Reply, CallError> {
    Ok(Reply::Perform(wire::PerformResponse {
        delivery: wire::Delivery::Delivered as i32,
        modality: "ax".into(),
        ..Default::default()
    }))
}

fn ctx(task: TaskId) -> CallCtx {
    CallCtx::bare(task, SessionId::new(), "run-1")
}

async fn session_with_snapshot(rt: &ComputerRuntime, c: &CallCtx, bundle: &str) -> String {
    let op = parse("computer.start", &json!({"application": bundle})).unwrap();
    rt.invoke(c, "computer.start", op).await.unwrap();
    let a = rt
        .invoke(
            c,
            "computer.state",
            Op::State {
                window: None,
                max_nodes: None,
            },
        )
        .await
        .unwrap();
    a.output["snapshot"].as_str().unwrap().to_owned()
}

fn press(snapshot: &str) -> Op {
    Op::Act(Act::Press {
        element: ElementRef {
            snapshot: snapshot.to_owned(),
            id: "n1".into(),
        },
        action: "press".into(),
    })
}

fn runtime() -> ComputerRuntime {
    ComputerRuntime::new(RuntimeConfig {
        idle_ttl: Duration::from_secs(60),
        ..RuntimeConfig::default()
    })
}

#[tokio::test]
async fn two_inputs_to_one_session_run_one_at_a_time() {
    let (a, log) = fake(60, performed);
    let rt = runtime();
    rt.install(a);
    let c = ctx(TaskId::new());
    let snap = session_with_snapshot(&rt, &c, "org.example.a").await;
    let (r1, r2) = tokio::join!(
        rt.invoke(&c, "computer.press", press(&snap)),
        rt.invoke(&c, "computer.press", press(&snap)),
    );
    assert!(r1.is_ok() && r2.is_ok(), "{r1:?} {r2:?}");
    let l = log.lock().unwrap().clone();
    let performs: Vec<&str> = l
        .iter()
        .map(String::as_str)
        .filter(|x| x.starts_with("perform"))
        .collect();
    assert_eq!(
        performs,
        [
            "perform:start",
            "perform:end",
            "perform:start",
            "perform:end"
        ],
        "{l:?}"
    );
}

#[tokio::test]
async fn an_input_is_never_retried_after_it_may_have_been_delivered_and_the_task_latches() {
    for lost in [CallError::Lost, CallError::TimedOut] {
        let l = lost.clone();
        let (a, log) = fake(0, move || Err(l.clone()));
        let rt = runtime();
        rt.install(a);
        let c = ctx(TaskId::new());
        let snap = session_with_snapshot(&rt, &c, "org.example.a").await;
        let first = rt.invoke(&c, "computer.press", press(&snap)).await;
        assert!(
            matches!(first, Err(Failure::Unknown { .. })),
            "{lost:?}: {first:?}"
        );
        // The next input is refused without being sent.
        let again = rt.invoke(&c, "computer.press", press(&snap)).await;
        match again {
            Err(Failure::Refused(r)) => {
                assert_eq!(r.code, Code::OutcomeUnknown);
                assert_eq!(r.facts["input_sent"], false);
            }
            other => panic!("{other:?}"),
        }
        let performs = log
            .lock()
            .unwrap()
            .iter()
            .filter(|x| *x == "perform:start")
            .count();
        assert_eq!(
            performs,
            1,
            "no retry, no second send: {:?}",
            log.lock().unwrap()
        );
        // Its session closed with the outcome; an unknown outcome is on the audit.
        let events = rt.drain_events();
        assert!(
            events
                .iter()
                .any(|e| matches!(e.event, TaskEvent::ComputerLatched { .. }))
        );
        assert!(
            events.iter().any(|e| matches!(&e.event, TaskEvent::ComputerSessionClosed { outcome, .. } if outcome == "LATCHED"))
        );
    }
}

#[tokio::test]
async fn a_failure_before_any_delivery_refuses_without_a_latch() {
    #[allow(clippy::type_complexity)]
    let cases: Vec<(
        &str,
        Box<dyn Fn() -> Result<Reply, CallError> + Send + Sync>,
    )> = vec![
        (
            "not delivered",
            Box::new(|| Err(CallError::NotDelivered("write failed".into()))),
        ),
        (
            "stale",
            Box::new(|| {
                Err(CallError::Remote(wire::ActuatorError {
                    code: "TARGET_STALE".into(),
                    message: "moved".into(),
                    delivery: wire::Delivery::NotDelivered as i32,
                }))
            }),
        ),
    ];
    for (label, f) in cases {
        let (a, _) = fake(0, f);
        let rt = runtime();
        rt.install(a);
        let c = ctx(TaskId::new());
        let snap = session_with_snapshot(&rt, &c, "org.example.a").await;
        let r = rt.invoke(&c, "computer.press", press(&snap)).await;
        assert!(matches!(r, Err(Failure::Refused(_))), "{label}: {r:?}");
        assert!(
            rt.lock().latches.is_empty(),
            "{label}: a refusal before delivery does not latch"
        );
    }
    // An actuator that says UNKNOWN after injecting began: the opposite.
    let (a, _) = fake(0, || {
        Err(CallError::Remote(wire::ActuatorError {
            code: "INPUT_FAILED".into(),
            message: "half".into(),
            delivery: wire::Delivery::Unknown as i32,
        }))
    });
    let rt = runtime();
    rt.install(a);
    let c = ctx(TaskId::new());
    let snap = session_with_snapshot(&rt, &c, "org.example.a").await;
    assert!(matches!(
        rt.invoke(&c, "computer.press", press(&snap)).await,
        Err(Failure::Unknown { .. })
    ));
    // An answer that does not say DELIVERED is not a success.
    let (a, _) = fake(0, || Ok(Reply::Perform(wire::PerformResponse::default())));
    let rt = runtime();
    rt.install(a);
    let c = ctx(TaskId::new());
    let snap = session_with_snapshot(&rt, &c, "org.example.a").await;
    assert!(matches!(
        rt.invoke(&c, "computer.press", press(&snap)).await,
        Err(Failure::Unknown { .. })
    ));
}

#[tokio::test]
async fn only_one_controller_holds_the_machine_and_releasing_frees_it() {
    let (a, _) = fake(0, performed);
    let rt = runtime();
    rt.install(a);
    let (c1, c2) = (ctx(TaskId::new()), ctx(TaskId::new()));
    let start_a = || parse("computer.start", &json!({"application": "org.example.a"})).unwrap();
    rt.invoke(&c1, "computer.start", start_a()).await.unwrap();
    match rt.invoke(&c2, "computer.start", start_a()).await {
        Err(Failure::Refused(r)) => assert_eq!(r.code, Code::SessionBusy),
        other => panic!("{other:?}"),
    }
    // One session per task, too.
    match rt.invoke(&c1, "computer.start", start_a()).await {
        Err(Failure::Refused(r)) => assert_eq!(r.code, Code::SessionBusy),
        other => panic!("{other:?}"),
    }
    rt.invoke(&c1, "computer.release", Op::Release)
        .await
        .unwrap();
    rt.invoke(&c2, "computer.start", start_a()).await.unwrap();
}

#[tokio::test]
async fn a_stop_is_final_for_its_run_and_not_for_the_next() {
    let (a, _) = fake(0, performed);
    let rt = runtime();
    rt.install(a);
    let task = TaskId::new();
    let c = ctx(task);
    session_with_snapshot(&rt, &c, "org.example.a").await;
    let (closed, _) = rt
        .stop_task(task, c.session_id, "run-1", "on-screen Stop")
        .await;
    assert_eq!(closed, 1);
    let start = || parse("computer.start", &json!({"application": "org.example.a"})).unwrap();
    for (tool, op) in [
        ("computer.start", start()),
        ("computer.release", Op::Release),
        ("computer.press", press("s1")),
    ] {
        match rt.invoke(&c, tool, op).await {
            Err(Failure::Refused(r)) => assert_eq!(r.code, Code::UserAborted, "{tool}"),
            other => panic!("{tool}: {other:?}"),
        }
    }
    // A new run is a new turn: the stop does not follow it.
    let mut next = c.clone();
    next.run_id = "run-2".into();
    assert!(rt.invoke(&next, "computer.start", start()).await.is_ok());
}

#[tokio::test]
async fn a_restored_latch_refuses_every_input_until_resolved() {
    let (a, log) = fake(0, performed);
    let rt = runtime();
    rt.install(a);
    let task = TaskId::new();
    let c = ctx(task);
    let snap = session_with_snapshot(&rt, &c, "org.example.a").await;
    rt.hydrate(
        task,
        Some(Latch {
            control_id: "c0".into(),
            tool_call_id: "t0".into(),
            tool: "computer.press".into(),
            action: "press".into(),
            reason: "CORE_RESTART".into(),
            at_ms: 1,
        }),
        None,
    );
    match rt.invoke(&c, "computer.press", press(&snap)).await {
        Err(Failure::Refused(r)) => assert_eq!(r.code, Code::OutcomeUnknown),
        other => panic!("{other:?}"),
    }
    assert!(!log.lock().unwrap().iter().any(|x| x == "perform:start"));
    assert!(rt.resolve_latch(task, c.session_id));
    assert!(rt.invoke(&c, "computer.press", press(&snap)).await.is_ok());
}

#[tokio::test]
async fn an_identity_the_policy_forbids_or_the_list_never_allows_is_refused_before_any_actuator_call()
 {
    let (a, log) = fake(0, performed);
    let rt = runtime();
    rt.install(a);
    let mut c = ctx(TaskId::new());
    c.policy.apps_allow = Some(["org.example.b".to_owned()].into_iter().collect());
    let start = |b: &str| parse("computer.start", &json!({"application": b})).unwrap();
    match rt
        .invoke(&c, "computer.start", start("org.example.a"))
        .await
    {
        Err(Failure::Refused(r)) => assert_eq!(r.code, Code::WindowUnverifiable),
        other => panic!("{other:?}"),
    }
    assert!(
        !log.lock().unwrap().iter().any(|x| x == "acquire:start"),
        "nothing was acquired"
    );
    assert!(
        rt.invoke(&c, "computer.start", start("org.example.b"))
            .await
            .is_ok()
    );
    // The binding names the application by identity: a different process is a different intent.
    let c2 = ctx(TaskId::new());
    let b1 = rt
        .prepare(&c, "computer.start", &start("org.example.b"))
        .await;
    assert!(
        matches!(b1, Err(ref r) if r.code == Code::SessionBusy),
        "{b1:?}"
    );
    let _ = c2;
}
