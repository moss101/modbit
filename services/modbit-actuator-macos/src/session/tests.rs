//! The portable actuator behaviour against a scripted backend: lease, Stop,
//! human parking, permission loss, the masking contract, idempotency,
//! deadlines. (The real macOS backend is proven in `tests/`.)

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use modbit_protocol::v1 as wire;
use tokio::sync::mpsc::{Receiver, channel};

use super::*;
use crate::backend::{CaptureArgs, RawFrame};

type PerformFn = dyn Fn(&wire::PerformRequest, &Ctl) -> Res<wire::PerformResponse> + Send + Sync;

struct Script {
    perms: Mutex<Permissions>,
    secure: AtomicBool,
    idle: Mutex<Option<Duration>>,
    perform: Mutex<Arc<PerformFn>>,
    window_alive: AtomicBool,
    performs: AtomicUsize,
    last_touch: Mutex<Option<Instant>>,
    raw: Mutex<Option<(u32, u32, Vec<crate::canvas::Rect>)>>,
}

impl Script {
    /// The simulated hardware idle counter moves now.
    fn touch(&self) {
        *self.last_touch.lock().unwrap() = Some(Instant::now());
    }

    fn new() -> Arc<Self> {
        Arc::new(Self {
            perms: Mutex::new(Permissions {
                accessibility: true,
                screen_recording: true,
            }),
            secure: AtomicBool::new(false),
            idle: Mutex::new(None),
            perform: Mutex::new(Arc::new(|_, _| Ok(wire::PerformResponse::default()))),
            window_alive: AtomicBool::new(true),
            performs: AtomicUsize::new(0),
            last_touch: Mutex::new(None),
            raw: Mutex::new(None),
        })
    }
}

fn ident() -> wire::ApplicationIdentity {
    wire::ApplicationIdentity {
        bundle_id: "org.example.app".into(),
        executable_path: "/Applications/App.app/Contents/MacOS/App".into(),
        signing_identity: "TEAM:org.example.app".into(),
        pid: 4242,
        name: "App".into(),
    }
}

fn info() -> wire::ApplicationInfo {
    wire::ApplicationInfo {
        identity: Some(ident()),
        windows: vec![wire::WindowInfo {
            window_id: "7".into(),
            ..Default::default()
        }],
        elevated: false,
    }
}

impl Backend for Script {
    fn name(&self) -> &'static str {
        "script"
    }
    fn platform(&self) -> &'static str {
        "script"
    }
    fn capabilities(&self) -> Vec<String> {
        vec![]
    }
    fn permissions(&self) -> Permissions {
        *self.perms.lock().unwrap()
    }
    fn secure_desktop(&self) -> bool {
        self.secure.load(Ordering::SeqCst)
    }
    fn list_apps(&self) -> Res<Vec<wire::ApplicationInfo>> {
        Ok(vec![info()])
    }
    fn resolve(&self, _: &str, _: &str, _: u32) -> Res<wire::ApplicationInfo> {
        Ok(info())
    }
    fn verify(&self, id: &wire::ApplicationIdentity) -> Res<wire::ApplicationInfo> {
        if *id == ident() {
            Ok(info())
        } else {
            Err(Fail::nd(
                "WINDOW_UNVERIFIABLE",
                "not the approved application",
            ))
        }
    }
    fn read_tree(&self, a: &ReadArgs<'_>, _: &Ctl) -> Res<wire::ReadStateResponse> {
        Ok(wire::ReadStateResponse {
            snapshot_id: a.snapshot_id.into(),
            ..Default::default()
        })
    }
    fn tree_digest(
        &self,
        _: &wire::ApplicationIdentity,
        _: &str,
        _: &Ctl,
    ) -> Res<(String, wire::WindowInfo)> {
        Ok(("d".into(), wire::WindowInfo::default()))
    }
    fn capture(&self, _: &CaptureArgs<'_>, _: &Ctl) -> Res<RawFrame> {
        let (w, h, secure) = self.raw.lock().unwrap().clone().expect("a raw frame");
        // A frame whose every pixel is the same loud colour: a secret.
        let rgba = [12u8, 200, 34, 255].repeat((w * h) as usize);
        Ok(RawFrame {
            rgba,
            px_w: w,
            px_h: h,
            pt_w: w,
            pt_h: h,
            secure,
            window: wire::WindowInfo::default(),
        })
    }
    fn perform(&self, a: &PerformArgs<'_>, ctl: &Ctl) -> Res<wire::PerformResponse> {
        self.performs.fetch_add(1, Ordering::SeqCst);
        let f = Arc::clone(&*self.perform.lock().unwrap());
        f(a.req, ctl)
    }
    fn human_idle(&self) -> Option<Duration> {
        if let Some(d) = *self.idle.lock().unwrap() {
            return Some(d);
        }
        self.last_touch.lock().unwrap().map(|t| t.elapsed())
    }
    fn window_exists(&self, _: u32, _: &str) -> bool {
        self.window_alive.load(Ordering::SeqCst)
    }
}

fn hub_over(b: &Arc<Script>, denied: Denied) -> (Arc<Hub>, Receiver<wire::ActuatorFrame>) {
    let (tx, rx) = channel(64);
    let be: Arc<dyn Backend> = Arc::clone(b) as Arc<dyn Backend>;
    (Hub::new(be, tx, denied), rx)
}

fn req(id: u64, call: wire::actuator_request::Call) -> wire::ActuatorRequest {
    wire::ActuatorRequest {
        request_id: id,
        deadline_ms: 0,
        call: Some(call),
    }
}

fn acquire(control: &str) -> wire::actuator_request::Call {
    wire::actuator_request::Call::Acquire(wire::AcquireRequest {
        control_id: control.into(),
        scope: wire::Scope::App as i32,
        application: Some(ident()),
        window_id: String::new(),
        lease_ttl_ms: 30_000,
    })
}

fn perform_call(control: &str, key: &str) -> wire::actuator_request::Call {
    wire::actuator_request::Call::Perform(wire::PerformRequest {
        control_id: control.into(),
        application: Some(ident()),
        idempotency_key: key.into(),
        action: Some(wire::perform_request::Action::Key(wire::KeyAction {
            keys: vec!["tab".into()],
        })),
        ..Default::default()
    })
}

fn code_of(r: &wire::ActuatorResponse) -> String {
    match &r.result {
        Some(wire::actuator_response::Result::Error(e)) => e.code.clone(),
        Some(_) => "OK".into(),
        None => "EMPTY".into(),
    }
}

fn delivery_of(r: &wire::ActuatorResponse) -> i32 {
    match &r.result {
        Some(wire::actuator_response::Result::Error(e)) => e.delivery,
        _ => -1,
    }
}

async fn next_event(
    rx: &mut Receiver<wire::ActuatorFrame>,
    within: Duration,
) -> Option<wire::actuator_event::Event> {
    let deadline = Instant::now() + within;
    while Instant::now() < deadline {
        if let Ok(Some(f)) = tokio::time::timeout(Duration::from_millis(50), rx.recv()).await
            && let Some(wire::actuator_frame::Body::Event(e)) = f.body
        {
            return e.event;
        }
    }
    None
}

#[tokio::test]
async fn one_controller_holds_the_machine_until_it_lets_go() {
    let b = Script::new();
    let (hub, _rx) = hub_over(&b, Denied::default());
    assert_eq!(code_of(&hub.handle(req(1, acquire("A"))).await), "OK");
    assert_eq!(
        code_of(&hub.handle(req(2, acquire("B"))).await),
        "SESSION_BUSY"
    );
    // The same controller may re-acquire (a lease renewal).
    assert_eq!(code_of(&hub.handle(req(3, acquire("A"))).await), "OK");
    let rel = hub
        .handle(req(
            4,
            wire::actuator_request::Call::Release(wire::ReleaseRequest {
                control_id: "A".into(),
            }),
        ))
        .await;
    assert!(
        matches!(rel.result, Some(wire::actuator_response::Result::Release(ref r)) if r.was_held)
    );
    assert_eq!(code_of(&hub.handle(req(5, acquire("B"))).await), "OK");
}

#[tokio::test]
async fn an_input_without_the_lease_or_for_another_application_is_refused_and_nothing_runs() {
    let b = Script::new();
    let (hub, _rx) = hub_over(&b, Denied::default());
    assert_eq!(
        code_of(&hub.handle(req(1, perform_call("A", ""))).await),
        "WINDOW_UNVERIFIABLE"
    );
    let mut other = ident();
    other.signing_identity = "EVIL:org.example.app".into();
    let r = hub
        .handle(req(
            2,
            wire::actuator_request::Call::Acquire(wire::AcquireRequest {
                control_id: "A".into(),
                scope: wire::Scope::App as i32,
                application: Some(other),
                ..Default::default()
            }),
        ))
        .await;
    assert_eq!(
        code_of(&r),
        "WINDOW_UNVERIFIABLE",
        "a swapped signing identity is not the approved application"
    );
    assert_eq!(b.performs.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn stop_ends_an_input_in_flight_within_the_bound_and_nothing_is_injected_after_it() {
    let b = Script::new();
    let injected = Arc::new(AtomicU32::new(0));
    let inj = Arc::clone(&injected);
    *b.perform.lock().unwrap() = Arc::new(move |_, ctl| {
        // A long typing: one unit every 5 ms, checking the Ctl before each.
        for _ in 0..2000 {
            if let Err(h) = ctl.check() {
                return Err(Ctl::halt_fail(h, inj.load(Ordering::SeqCst) > 0));
            }
            inj.fetch_add(1, Ordering::SeqCst);
            std::thread::sleep(Duration::from_millis(5));
        }
        Ok(wire::PerformResponse::default())
    });
    let (hub, _rx) = hub_over(&b, Denied::default());
    assert_eq!(code_of(&hub.handle(req(1, acquire("A"))).await), "OK");
    let h2 = Arc::clone(&hub);
    let flight = tokio::spawn(async move { h2.handle(req(2, perform_call("A", ""))).await });
    // Wait for the input to be under way (a loaded runner is slow to start it).
    let t_wait = Instant::now();
    while injected.load(Ordering::SeqCst) <= 5 {
        assert!(
            t_wait.elapsed() < Duration::from_secs(10),
            "the input is under way"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let t0 = Instant::now();
    let stop = hub.stop().await;
    let took = t0.elapsed();
    let at_stop = injected.load(Ordering::SeqCst);
    assert!(took < Duration::from_millis(500), "stop took {took:?}");
    assert_eq!(stop.halted_leases, 1);
    assert!(stop.stopped_within_ms < 500, "{stop:?}");
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(
        injected.load(Ordering::SeqCst),
        at_stop,
        "no injection after the stop answered"
    );
    let r = flight.await.unwrap();
    assert_eq!(code_of(&r), "USER_ABORTED");
    assert_eq!(
        delivery_of(&r),
        wire::Delivery::Unknown as i32,
        "part of it was injected: the outcome is unknown"
    );
    // The lease went with it.
    assert_eq!(
        code_of(&hub.handle(req(3, perform_call("A", ""))).await),
        "WINDOW_UNVERIFIABLE"
    );
}

#[tokio::test]
async fn an_input_queued_behind_a_running_one_does_not_run_after_stop() {
    let b = Script::new();
    *b.perform.lock().unwrap() = Arc::new(|_, ctl| {
        for _ in 0..400 {
            if let Err(h) = ctl.check() {
                return Err(Ctl::halt_fail(h, true));
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        Ok(wire::PerformResponse::default())
    });
    let (hub, _rx) = hub_over(&b, Denied::default());
    hub.handle(req(1, acquire("A"))).await;
    let (h1, h2) = (Arc::clone(&hub), Arc::clone(&hub));
    let first = tokio::spawn(async move { h1.handle(req(2, perform_call("A", ""))).await });
    tokio::time::sleep(Duration::from_millis(60)).await;
    let second = tokio::spawn(async move { h2.handle(req(3, perform_call("A", ""))).await });
    tokio::time::sleep(Duration::from_millis(60)).await;
    hub.stop().await;
    let (a, c) = (first.await.unwrap(), second.await.unwrap());
    assert_eq!(code_of(&a), "USER_ABORTED");
    assert_eq!(
        code_of(&c),
        "WINDOW_UNVERIFIABLE",
        "the queued input found no lease"
    );
    assert_eq!(
        b.performs.load(Ordering::SeqCst),
        1,
        "the second input never reached the backend"
    );
}

#[tokio::test]
async fn a_physical_event_parks_the_controller_ends_the_input_in_flight_and_is_reported() {
    let b = Script::new();
    *b.perform.lock().unwrap() = Arc::new(|_, ctl| {
        for _ in 0..600 {
            if let Err(h) = ctl.check() {
                return Err(Ctl::halt_fail(h, true));
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        Ok(wire::PerformResponse::default())
    });
    let (hub, mut rx) = hub_over(&b, Denied::default());
    hub.spawn_monitors();
    hub.handle(req(1, acquire("A"))).await;
    let h2 = Arc::clone(&hub);
    let flight = tokio::spawn(async move { h2.handle(req(2, perform_call("A", ""))).await });
    tokio::time::sleep(Duration::from_millis(100)).await;
    // A person touches the keyboard.
    *b.idle.lock().unwrap() = Some(Duration::from_millis(1));
    let ev = next_event(&mut rx, Duration::from_secs(2)).await;
    assert!(
        matches!(ev, Some(wire::actuator_event::Event::HumanInput(_))),
        "{ev:?}"
    );
    *b.idle.lock().unwrap() = Some(Duration::from_secs(30));
    let r = flight.await.unwrap();
    assert_eq!(
        code_of(&r),
        "USER_ABORTED",
        "the input in flight ended at its next event"
    );
    // Re-acquire: the next agent input is HUMAN_ACTIVE until the cooldown ends.
    hub.handle(req(3, acquire("A"))).await;
    assert_eq!(
        code_of(&hub.handle(req(4, perform_call("A", ""))).await),
        "HUMAN_ACTIVE"
    );
    let ping = hub
        .handle(req(
            5,
            wire::actuator_request::Call::Ping(wire::PingRequest {
                control_id: "A".into(),
            }),
        ))
        .await;
    assert!(
        matches!(ping.result, Some(wire::actuator_response::Result::Ping(ref p)) if p.human_active)
    );
    tokio::time::sleep(HUMAN_COOLDOWN + Duration::from_millis(200)).await;
    *b.perform.lock().unwrap() = Arc::new(|_, _| Ok(wire::PerformResponse::default()));
    assert_eq!(
        code_of(&hub.handle(req(6, perform_call("A", ""))).await),
        "OK"
    );
}

#[tokio::test]
async fn the_actuators_own_events_are_not_a_person_but_a_person_during_them_is() {
    let b = Script::new();
    let b2 = Arc::clone(&b);
    *b.perform.lock().unwrap() = Arc::new(move |_, ctl| {
        // Posts a synthetic event every 10 ms for 300 ms; each one resets the
        // (simulated) hardware idle counter, as it does on some macOS versions.
        for _ in 0..30 {
            ctl.own_event();
            b2.touch();
            std::thread::sleep(Duration::from_millis(10));
        }
        Ok(wire::PerformResponse::default())
    });
    let (hub, mut rx) = hub_over(&b, Denied::default());
    hub.spawn_monitors();
    hub.handle(req(1, acquire("A"))).await;
    let resp = hub.handle(req(2, perform_call("A", ""))).await;
    assert_eq!(
        code_of(&resp),
        "OK",
        "the injection was not halted by its own events"
    );
    assert!(
        next_event(&mut rx, Duration::from_millis(200))
            .await
            .is_none(),
        "no HumanInput for the actuator's own events"
    );
    // A person touches the machine at a moment the actuator did not post.
    tokio::time::sleep(Duration::from_millis(100)).await;
    b.touch();
    let ev = next_event(&mut rx, Duration::from_secs(2)).await;
    assert!(
        matches!(ev, Some(wire::actuator_event::Event::HumanInput(_))),
        "{ev:?}"
    );
}

#[tokio::test]
async fn a_missing_permission_refuses_every_operation_that_needs_it_and_crashes_nothing() {
    let b = Script::new();
    let (hub, _rx) = hub_over(
        &b,
        Denied {
            accessibility: true,
            screen_recording: false,
        },
    );
    assert_eq!(
        code_of(&hub.handle(req(1, acquire("A"))).await),
        "PERMISSION_REQUIRED"
    );
    for (i, call) in [
        wire::actuator_request::Call::ReadState(wire::ReadStateRequest {
            control_id: "A".into(),
            application: Some(ident()),
            ..Default::default()
        }),
        wire::actuator_request::Call::Capture(wire::CaptureRequest {
            control_id: "A".into(),
            application: Some(ident()),
            ..Default::default()
        }),
        wire::actuator_request::Call::Wait(wire::WaitRequest {
            control_id: "A".into(),
            application: Some(ident()),
            ..Default::default()
        }),
        perform_call("A", ""),
    ]
    .into_iter()
    .enumerate()
    {
        let r = hub.handle(req(10 + i as u64, call)).await;
        let e = match &r.result {
            Some(wire::actuator_response::Result::Error(e)) => e.clone(),
            other => panic!("expected an error, got {other:?}"),
        };
        assert_eq!(e.code, "PERMISSION_REQUIRED", "call {i}");
        assert_eq!(e.delivery, wire::Delivery::NotDelivered as i32, "call {i}");
        assert!(e.message.contains("Accessibility"), "{}", e.message);
    }
    assert_eq!(b.performs.load(Ordering::SeqCst), 0);
    // Reads that need no permission still work; Stop and Release never fail.
    assert_eq!(
        code_of(
            &hub.handle(req(
                20,
                wire::actuator_request::Call::ListApplications(wire::ListApplicationsRequest {})
            ))
            .await
        ),
        "OK"
    );
    assert_eq!(
        code_of(
            &hub.handle(req(
                21,
                wire::actuator_request::Call::Stop(wire::StopRequest::default())
            ))
            .await
        ),
        "OK"
    );
}

#[tokio::test]
async fn screen_recording_alone_gates_capture_and_a_revocation_mid_lease_ends_control() {
    let b = Script::new();
    *b.raw.lock().unwrap() = Some((64, 40, vec![]));
    let (hub, mut rx) = hub_over(&b, Denied::default());
    hub.spawn_monitors();
    hub.handle(req(1, acquire("A"))).await;
    // Revoke Screen Recording while the lease is held.
    tokio::time::sleep(Duration::from_millis(600)).await; // the monitor has seen "granted"
    b.perms.lock().unwrap().screen_recording = false;
    let ev = next_event(&mut rx, Duration::from_secs(3)).await;
    assert!(
        matches!(&ev, Some(wire::actuator_event::Event::PermissionChanged(p)) if p.permission == "screen_recording" && !p.granted),
        "{ev:?}"
    );
    assert_eq!(
        code_of(&hub.handle(req(2, perform_call("A", ""))).await),
        "WINDOW_UNVERIFIABLE",
        "the lease ended with the permission"
    );
    hub.handle(req(3, acquire("A"))).await;
    let cap = hub
        .handle(req(
            4,
            wire::actuator_request::Call::Capture(wire::CaptureRequest {
                control_id: "A".into(),
                application: Some(ident()),
                scope: wire::Scope::App as i32,
                ..Default::default()
            }),
        ))
        .await;
    assert_eq!(code_of(&cap), "PERMISSION_REQUIRED");
}

#[tokio::test]
async fn a_repeated_idempotency_key_is_answered_never_injected_again() {
    let b = Script::new();
    let (hub, _rx) = hub_over(&b, Denied::default());
    hub.handle(req(1, acquire("A"))).await;
    assert_eq!(
        code_of(&hub.handle(req(2, perform_call("A", "call-9"))).await),
        "OK"
    );
    assert_eq!(
        code_of(&hub.handle(req(3, perform_call("A", "call-9"))).await),
        "OK"
    );
    assert_eq!(b.performs.load(Ordering::SeqCst), 1);
    assert_eq!(
        code_of(&hub.handle(req(4, perform_call("A", "call-10"))).await),
        "OK"
    );
    assert_eq!(b.performs.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn a_frame_leaves_at_the_canvas_size_with_its_secure_region_black_and_declared() {
    let b = Script::new();
    // A 640x400 window with a "password field" at (100,50) 300x30 points.
    *b.raw.lock().unwrap() = Some((
        640,
        400,
        vec![crate::canvas::Rect {
            x: 100.0,
            y: 50.0,
            w: 300.0,
            h: 30.0,
        }],
    ));
    let (hub, _rx) = hub_over(&b, Denied::default());
    hub.handle(req(1, acquire("A"))).await;
    let r = hub
        .handle(req(
            2,
            wire::actuator_request::Call::Capture(wire::CaptureRequest {
                control_id: "A".into(),
                application: Some(ident()),
                scope: wire::Scope::App as i32,
                canvas_width: 1280,
                canvas_height: 800,
                ..Default::default()
            }),
        ))
        .await;
    let Some(wire::actuator_response::Result::Capture(c)) = r.result else {
        panic!("{r:?}")
    };
    assert_eq!((c.width, c.height), (1280, 800));
    assert_eq!(c.masked_regions, 1);
    assert_eq!(c.secure_regions.len(), 1);
    let lb = c.letterbox.unwrap();
    assert!(
        (lb.scale - 2.0).abs() < 1e-9,
        "640x400 fits the canvas at 2x"
    );
    // Decode and look at the pixels.
    let dec = png::Decoder::new(std::io::Cursor::new(&c.png));
    let mut reader = dec.read_info().unwrap();
    let mut buf = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut buf).unwrap();
    assert_eq!((info.width, info.height), (1280, 800));
    let px = |x: usize, y: usize| {
        let o = (y * 1280 + x) * 4;
        [buf[o], buf[o + 1], buf[o + 2], buf[o + 3]]
    };
    let s = &c.secure_regions[0];
    for y in s.y..s.y + s.height {
        for x in s.x..s.x + s.width {
            assert_eq!(
                px(x as usize, y as usize),
                [0, 0, 0, 255],
                "({x},{y}) is masked"
            );
        }
    }
    assert!(s.x <= 200 && s.y <= 100 && s.x + s.width >= 800 && s.y + s.height >= 160);
    assert_eq!(
        px(10, 10),
        [12, 200, 34, 255],
        "outside the secure region the frame is untouched"
    );
    // The "secret" colour appears nowhere inside the region.
    let secret_in_region = (s.y..s.y + s.height)
        .flat_map(|y| (s.x..s.x + s.width).map(move |x| (x, y)))
        .any(|(x, y)| px(x as usize, y as usize) == [12, 200, 34, 255]);
    assert!(!secret_in_region);
}

#[tokio::test]
async fn an_input_that_outlives_its_deadline_times_out_with_an_unknown_outcome() {
    let b = Script::new();
    *b.perform.lock().unwrap() = Arc::new(|_, ctl| {
        let mut sent = 0;
        loop {
            if let Err(h) = ctl.check() {
                return Err(Ctl::halt_fail(h, sent > 0));
            }
            sent += 1;
            std::thread::sleep(Duration::from_millis(5));
        }
    });
    let (hub, _rx) = hub_over(&b, Denied::default());
    hub.handle(req(1, acquire("A"))).await;
    let mut r = req(2, perform_call("A", ""));
    r.deadline_ms = 120;
    let t0 = Instant::now();
    let resp = hub.handle(r).await;
    assert_eq!(code_of(&resp), "TIMEOUT");
    assert_eq!(delivery_of(&resp), wire::Delivery::Unknown as i32);
    assert!(t0.elapsed() < Duration::from_secs(2));
}

#[tokio::test]
async fn a_lease_the_core_stops_renewing_is_released_and_a_lost_window_is_reported() {
    let b = Script::new();
    let (hub, mut rx) = hub_over(&b, Denied::default());
    hub.spawn_monitors();
    let mut a = acquire("A");
    if let wire::actuator_request::Call::Acquire(ref mut x) = a {
        x.lease_ttl_ms = 500;
    }
    hub.handle(req(1, a)).await;
    b.window_alive.store(false, Ordering::SeqCst);
    let ev = next_event(&mut rx, Duration::from_secs(2)).await;
    assert!(
        matches!(&ev, Some(wire::actuator_event::Event::WindowGone(w)) if w.window_id == "7" && w.control_id == "A"),
        "{ev:?}"
    );
    tokio::time::sleep(Duration::from_millis(900)).await;
    let ping = hub
        .handle(req(
            2,
            wire::actuator_request::Call::Ping(wire::PingRequest {
                control_id: "A".into(),
            }),
        ))
        .await;
    assert!(
        matches!(ping.result, Some(wire::actuator_response::Result::Ping(ref p)) if !p.lease_held),
        "{ping:?}"
    );
}

#[test]
fn the_deny_list_parses_and_can_only_remove_capability() {
    assert_eq!(
        Denied::parse("accessibility"),
        Denied {
            accessibility: true,
            screen_recording: false
        }
    );
    assert_eq!(
        Denied::parse(" accessibility , screen_recording,bogus"),
        Denied {
            accessibility: true,
            screen_recording: true
        }
    );
    assert_eq!(Denied::parse(""), Denied::default());
}
