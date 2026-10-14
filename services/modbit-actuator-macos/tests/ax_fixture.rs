//! QUAL-PX-071 on a real Mac with real permissions: the real helper drives a
//! real AppKit application (`fixtures/axfixture`) through the real
//! Accessibility API, the real window server and real synthesised events.
//!
//! These tests need what a CI runner does not have: Accessibility and Screen
//! Recording granted to the process that runs them, and an unlocked screen.
//! They are `#[ignore]`d so a run without those does NOT pass them silently:
//! libtest lists them as ignored with the reason. Run them with
//! `scripts/actuator-macos-ax-tests.sh` (it checks the permissions first and
//! prints SKIPPED, loudly, when they are absent), or by hand:
//!
//!   cargo test -p modbit-actuator-macos --test ax_fixture -- --ignored --test-threads=1
//!
//! One-time owner steps: System Settings > Privacy & Security > Accessibility
//! AND Screen & System Audio Recording > add the terminal (or Electron) app
//! that runs `cargo`; unlock the screen.
#![cfg(target_os = "macos")]
#![allow(unsafe_code)]

mod common;

use std::ffi::c_void;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use common::*;
use modbit_computer::actuator::{Actuator, Call, CallError, ProcessActuator, Reply};
use modbit_protocol::v1 as wire;

const NEEDS: &str = "needs Accessibility + Screen Recording for this process and an unlocked screen: scripts/actuator-macos-ax-tests.sh";

// ---------------------------------------------------------------- the fixture app

fn fixture_app() -> &'static PathBuf {
    static APP: OnceLock<PathBuf> = OnceLock::new();
    APP.get_or_init(|| {
        let dir = std::env::temp_dir().join(format!("modbit-axfixture-{}", std::process::id()));
        let exe_dir = dir.join("AxFixture.app/Contents/MacOS");
        std::fs::create_dir_all(&exe_dir).unwrap();
        let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/axfixture/main.swift");
        let st = Command::new("swiftc")
            .args(["-O", "-o"])
            .arg(exe_dir.join("AxFixture"))
            .arg(&src)
            .status()
            .expect("swiftc is part of the macOS developer tools");
        assert!(st.success(), "building the fixture application");
        std::fs::write(
            dir.join("AxFixture.app/Contents/Info.plist"),
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleIdentifier</key><string>org.modbit.axfixture</string>
<key>CFBundleName</key><string>AxFixture</string>
<key>CFBundleExecutable</key><string>AxFixture</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>LSUIElement</key><true/>
</dict></plist>"#,
        )
        .unwrap();
        let st = Command::new("codesign")
            .args(["--force", "--sign", "-"])
            .arg(dir.join("AxFixture.app"))
            .status()
            .expect("codesign");
        assert!(st.success());
        dir.join("AxFixture.app")
    })
}

struct Fixture {
    child: Child,
    state: PathBuf,
    _dir: tempfile::TempDir,
}

impl Fixture {
    fn launch() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let state = dir.path().join("state");
        let child = Command::new(fixture_app().join("Contents/MacOS/AxFixture"))
            .arg(&state)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("launch the fixture application");
        let f = Self { child, state, _dir: dir };
        let t0 = Instant::now();
        while f.get("ready").as_deref() != Some("1") {
            assert!(t0.elapsed() < Duration::from_secs(20), "the fixture application did not start");
            std::thread::sleep(Duration::from_millis(100));
        }
        f
    }

    fn pid(&self) -> u32 {
        self.child.id()
    }

    fn get(&self, key: &str) -> Option<String> {
        let t = std::fs::read_to_string(&self.state).ok()?;
        t.lines().find_map(|l| l.strip_prefix(&format!("{key}=")).map(str::to_owned))
    }

    /// Epoch-ms timestamps of every keyDown the application received.
    fn keys(&self) -> Vec<f64> {
        let p = format!("{}.keys", self.state.display());
        std::fs::read_to_string(p)
            .unwrap_or_default()
            .lines()
            .filter_map(|l| l.parse().ok())
            .collect()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn now_ms() -> f64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs_f64() * 1000.0
}

// ---------------------------------------------------------------- CoreGraphics, for the test only

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
struct Pt {
    x: f64,
    y: f64,
}

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGEventCreate(source: *const c_void) -> *mut c_void;
    fn CGEventGetLocation(e: *const c_void) -> Pt;
    fn CGEventSourceCreate(state: i32) -> *mut c_void;
    fn CGEventCreateMouseEvent(source: *const c_void, ty: u32, p: Pt, button: u32) -> *mut c_void;
    fn CGEventPost(tap: u32, e: *mut c_void);
}
#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFRelease(p: *const c_void);
}

fn cursor() -> Pt {
    // SAFETY: create/get/release on a live event.
    unsafe {
        let e = CGEventCreate(std::ptr::null());
        let p = CGEventGetLocation(e);
        CFRelease(e);
        p
    }
}

/// A "physical" pointer event: a zero-distance move entering at the HID tap
/// through the HID system state, indistinguishable from a device to the
/// event counters the helper watches.
fn physical_nudge() {
    let at = cursor();
    // SAFETY: create/post/release; state 1 = HID system state, tap 0 = HID.
    unsafe {
        let src = CGEventSourceCreate(1);
        let e = CGEventCreateMouseEvent(src, 5, at, 0);
        CGEventPost(0, e);
        CFRelease(e);
        CFRelease(src);
    }
}

// ---------------------------------------------------------------- a control session

struct Sess {
    a: ProcessActuator,
    ident: wire::ApplicationIdentity,
    info: wire::ApplicationInfo,
    control: String,
    scope: wire::Scope,
    n: u32,
}

impl Sess {
    async fn start(fx: &Fixture, scope: wire::Scope) -> Self {
        Self::start_with(fx, scope, &[]).await
    }

    async fn start_with(fx: &Fixture, scope: wire::Scope, env: &[(&str, &str)]) -> Self {
        let a = launch(env).await.expect("the helper attaches");
        let r = call(&a, Call::ResolveApplication(wire::ResolveApplicationRequest { pid: fx.pid(), ..Default::default() })).await;
        let Ok(Reply::ResolveApplication(r)) = r else { panic!("resolve: {r:?}") };
        let info = r.application.unwrap();
        let ident = ident_of(&info);
        assert_eq!(ident.bundle_id, "org.modbit.axfixture", "the bundle identifier comes from the bundle, not the title");
        let control = "ctl-1".to_owned();
        let acq = call(
            &a,
            Call::Acquire(wire::AcquireRequest {
                control_id: control.clone(),
                scope: scope as i32,
                application: Some(ident.clone()),
                window_id: String::new(),
                lease_ttl_ms: 60_000,
            }),
        )
        .await;
        assert!(matches!(acq, Ok(Reply::Acquire(_))), "acquire: {acq:?} ({NEEDS})");
        Self { a, ident, info, control, scope, n: 0 }
    }

    async fn read(&mut self, max_nodes: u32) -> wire::ReadStateResponse {
        self.n += 1;
        let r = call(
            &self.a,
            Call::ReadState(wire::ReadStateRequest {
                control_id: self.control.clone(),
                application: Some(self.ident.clone()),
                window_id: String::new(),
                snapshot_id: format!("snap-{}", self.n),
                max_nodes,
                probe: false,
            }),
        )
        .await;
        let Ok(Reply::ReadState(r)) = r else { panic!("read: {r:?}") };
        r
    }


    async fn perform(&self, snapshot: &str, window_version: u64, action: wire::perform_request::Action) -> Result<Reply, CallError> {
        static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        call(
            &self.a,
            Call::Perform(wire::PerformRequest {
                control_id: self.control.clone(),
                application: Some(self.ident.clone()),
                window_id: String::new(),
                snapshot_id: snapshot.to_owned(),
                frame_token: String::new(),
                window_version,
                scope: self.scope as i32,
                idempotency_key: format!("k{}", SEQ.fetch_add(1, std::sync::atomic::Ordering::SeqCst)),
                action: Some(action),
            }),
        )
        .await
    }

    async fn press(&self, tree: &wire::ReadStateResponse, role: &str, name: &str, action: &str) -> Result<Reply, CallError> {
        let id = find(tree, role, name).element_id.clone();
        self.perform(&tree.snapshot_id, tree.window.as_ref().unwrap().version, wire::perform_request::Action::Press(wire::PressAction { element_id: id, action: action.into() })).await
    }
}

fn find<'a>(t: &'a wire::ReadStateResponse, role: &str, name: &str) -> &'a wire::AxNode {
    t.nodes
        .iter()
        .find(|n| n.role == role && n.name.contains(name))
        .unwrap_or_else(|| panic!("no {role} {name:?} in {:#?}", t.nodes.iter().map(|n| (&n.element_id, &n.role, &n.name)).collect::<Vec<_>>()))
}

fn performed(r: &Result<Reply, CallError>) -> &wire::PerformResponse {
    match r {
        Ok(Reply::Perform(p)) => p,
        other => panic!("expected a performed input, got {other:?}"),
    }
}

// ---------------------------------------------------------------- tests

#[tokio::test]
#[ignore = "needs Accessibility + Screen Recording and an unlocked screen: scripts/actuator-macos-ax-tests.sh"]
async fn the_tree_lists_the_known_elements_is_bounded_and_stable_and_never_carries_a_secret() {
    let fx = Fixture::launch();
    let mut s = Sess::start(&fx, wire::Scope::App).await;
    let t = s.read(0).await;
    assert!(
        !t.truncated,
        "{} nodes, truncated: {:#?}",
        t.nodes.len(),
        t.nodes.iter().take(40).map(|n| (&n.element_id, &n.parent_id, &n.role, &n.name)).collect::<Vec<_>>()
    );
    let w = t.window.as_ref().unwrap();
    assert_eq!(w.title, "Modbit AX Fixture");
    assert_eq!(find(&t, "button", "Increment").actions.first().map(String::as_str), Some("press"));
    assert!(find(&t, "textfield", "Name").settable);
    assert!(find(&t, "checkbox", "Agree").enabled);
    assert!(find(&t, "label", "count=0").value.is_empty());
    let pw = find(&t, "securetextfield", "Password");
    assert!(pw.secure);
    assert_eq!(pw.value, "", "a secure field's value is never read");
    assert!(!format!("{t:?}").contains("hunter2"), "the secret is nowhere in the tree");
    // Bounded: a small cap truncates and says so.
    let small = s.read(3).await;
    assert_eq!(small.nodes.len(), 3);
    assert!(small.truncated);
    // V02: repeated reads of an idle window agree.
    let digests: Vec<String> = {
        let mut v = vec![];
        for _ in 0..5 {
            let r = s.read(0).await;
            v.push(format!("{}:{}", r.nodes.len(), r.tree_digest));
        }
        v
    };
    assert!(digests.windows(2).all(|p| p[0] == p[1]), "unstable tree: {digests:?}");
    eprintln!("V02: {} nodes, digest stable across 5 reads", t.nodes.len());
}

#[tokio::test]
#[ignore = "needs Accessibility + Screen Recording and an unlocked screen: scripts/actuator-macos-ax-tests.sh"]
async fn a_press_runs_in_the_background_and_the_real_cursor_does_not_move() {
    let fx = Fixture::launch();
    let mut s = Sess::start(&fx, wire::Scope::App).await;
    let before = cursor();
    let t = s.read(0).await;
    let r = s.press(&t, "button", "Increment", "press").await;
    let p = performed(&r);
    assert_eq!(p.modality, "ax");
    assert_eq!(p.delivery, wire::Delivery::Delivered as i32);
    // The application itself says it happened.
    let t0 = Instant::now();
    while fx.get("pressed").as_deref() != Some("1") {
        assert!(t0.elapsed() < Duration::from_secs(3), "the button was not pressed: {:?}", fx.get("pressed"));
        std::thread::sleep(Duration::from_millis(50));
    }
    assert_eq!(cursor(), before, "the person's cursor stayed where it was");
    // The next read shows the new label; the press did not change the structure.
    assert!(!p.structure_changed);
    let t2 = s.read(0).await;
    find(&t2, "label", "count=1");
    assert!(p.changes.iter().any(|c| c.kind == "value"), "{:?}", p.changes);
}

#[tokio::test]
#[ignore = "needs Accessibility + Screen Recording and an unlocked screen: scripts/actuator-macos-ax-tests.sh"]
async fn a_value_is_set_through_the_tree_and_a_secure_field_is_refused_every_way() {
    let fx = Fixture::launch();
    let mut s = Sess::start(&fx, wire::Scope::App).await;
    let t = s.read(0).await;
    let ver = t.window.as_ref().unwrap().version;
    let name = find(&t, "textfield", "Name").element_id.clone();
    let r = s.perform(&t.snapshot_id, ver, wire::perform_request::Action::SetValue(wire::SetValueAction { element_id: name, value: "Ada Lovelace".into() })).await;
    assert_eq!(performed(&r).modality, "ax");
    let t0 = Instant::now();
    // (A programmatic set does not fire the delegate; the tree is the witness.)
    loop {
        let t2 = s.read(0).await;
        if find(&t2, "textfield", "Name").value == "Ada Lovelace" {
            break;
        }
        assert!(t0.elapsed() < Duration::from_secs(3), "value not set");
    }
    // The secure field: never set, never typed into.
    let t = s.read(0).await;
    let pw = find(&t, "securetextfield", "Password").element_id.clone();
    let r = s.perform(&t.snapshot_id, ver, wire::perform_request::Action::SetValue(wire::SetValueAction { element_id: pw.clone(), value: "x".into() })).await;
    assert_eq!(code(&r), "ACTION_UNSAFE");
    assert_eq!(delivery(&r), wire::Delivery::NotDelivered as i32);
    let r = s.perform(&t.snapshot_id, ver, wire::perform_request::Action::Press(wire::PressAction { element_id: pw, action: "focus".into() })).await;
    assert_eq!(code(&r), "OK", "focusing a secure field is allowed");
    let r = s.perform("", ver, wire::perform_request::Action::Type(wire::TypeAction { text: "leak".into(), then: String::new() })).await;
    assert_eq!(code(&r), "ACTION_UNSAFE", "typing into the focused secure field is refused");
    assert_eq!(fx.get("secure_len").as_deref(), Some("14"), "the password field is untouched");
    assert!(fx.keys().is_empty(), "not one key reached the application");
}

#[tokio::test]
#[ignore = "needs Accessibility + Screen Recording and an unlocked screen: scripts/actuator-macos-ax-tests.sh"]
async fn text_is_typed_into_the_focused_field_without_taking_the_keyboard() {
    let fx = Fixture::launch();
    let mut s = Sess::start(&fx, wire::Scope::App).await;
    let t = s.read(0).await;
    let ver = t.window.as_ref().unwrap().version;
    assert_eq!(code(&s.press(&t, "textfield", "Name", "focus").await), "OK");
    let r = s.perform("", ver, wire::perform_request::Action::Type(wire::TypeAction { text: "typed".into(), then: String::new() })).await;
    let p = performed(&r);
    eprintln!("V01: typing used the {:?} rung: {}", p.modality, p.modality_reason);
    let t0 = Instant::now();
    loop {
        let t2 = s.read(0).await;
        if find(&t2, "textfield", "Name").value.contains("typed") {
            break;
        }
        assert!(t0.elapsed() < Duration::from_secs(3), "text did not arrive via {} ({})", p.modality, p.modality_reason);
    }
}

#[tokio::test]
#[ignore = "needs Accessibility + Screen Recording and an unlocked screen: scripts/actuator-macos-ax-tests.sh"]
async fn a_stale_snapshot_is_refused_and_nothing_is_actuated() {
    let fx = Fixture::launch();
    let mut s = Sess::start(&fx, wire::Scope::App).await;
    let old = s.read(0).await;
    let _newer = s.read(0).await;
    let r = s.press(&old, "button", "Increment", "press").await;
    assert_eq!(code(&r), "TARGET_STALE");
    assert_eq!(delivery(&r), wire::Delivery::NotDelivered as i32);
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(fx.get("pressed").as_deref(), Some("0"));
    // A coordinate action against a moved window is stale too.
    let r = s
        .perform("", 12345, wire::perform_request::Action::Click(wire::ClickAction { at: Some(wire::Point { x: 640.0, y: 400.0 }), button: "left".into(), count: 1 }))
        .await;
    assert_eq!(code(&r), "TARGET_STALE");
}

#[tokio::test]
#[ignore = "needs Accessibility + Screen Recording and an unlocked screen: scripts/actuator-macos-ax-tests.sh"]
async fn a_swapped_identity_is_not_the_approved_application() {
    let fx = Fixture::launch();
    let s = Sess::start(&fx, wire::Scope::App).await;
    let mut other = s.ident.clone();
    other.signing_identity = "EVILTEAM:org.modbit.axfixture".into();
    let r = call(
        &s.a,
        Call::ReadState(wire::ReadStateRequest { control_id: s.control.clone(), application: Some(other), snapshot_id: "x".into(), ..Default::default() }),
    )
    .await;
    assert_eq!(code(&r), "WINDOW_UNVERIFIABLE");
    let mut other = s.ident.clone();
    other.executable_path = "/Applications/Other.app/Contents/MacOS/Other".into();
    let r = call(&s.a, Call::ReadState(wire::ReadStateRequest { control_id: s.control.clone(), application: Some(other), snapshot_id: "x".into(), ..Default::default() })).await;
    assert_eq!(code(&r), "WINDOW_UNVERIFIABLE");
}

#[tokio::test]
#[ignore = "needs Accessibility + Screen Recording and an unlocked screen: scripts/actuator-macos-ax-tests.sh"]
async fn a_frame_is_the_canvas_the_secure_field_is_black_and_a_coordinate_click_lands_where_it_was_aimed() {
    let fx = Fixture::launch();
    let mut s = Sess::start(&fx, wire::Scope::App).await;
    let t = s.read(0).await;
    let cap = call(
        &s.a,
        Call::Capture(wire::CaptureRequest {
            control_id: s.control.clone(),
            application: Some(s.ident.clone()),
            window_id: String::new(),
            scope: wire::Scope::App as i32,
            canvas_width: 1280,
            canvas_height: 800,
        }),
    )
    .await;
    let Ok(Reply::Capture(c)) = cap else { panic!("capture: {cap:?}") };
    assert_eq!((c.width, c.height), (1280, 800));
    assert_eq!(c.masked_regions, 1);
    let lb = c.letterbox.clone().unwrap();
    let dec = png::Decoder::new(std::io::Cursor::new(&c.png));
    let mut rd = dec.read_info().unwrap();
    let mut buf = vec![0; rd.output_buffer_size().unwrap()];
    let info = rd.next_frame(&mut buf).unwrap();
    assert_eq!((info.width, info.height), (1280, 800));
    let px = |x: i32, y: i32| {
        let o = ((y as usize) * 1280 + x as usize) * 4;
        [buf[o], buf[o + 1], buf[o + 2], buf[o + 3]]
    };
    let sr = &c.secure_regions[0];
    for y in sr.y..sr.y + sr.height {
        for x in sr.x..sr.x + sr.width {
            assert_eq!(px(x, y), [0, 0, 0, 255], "secure pixel ({x},{y})");
        }
    }
    // The mask sits on the password field's bounds.
    let pw = find(&t, "securetextfield", "Password").bounds.clone().unwrap();
    let w = c.window.as_ref().unwrap().bounds.clone().unwrap();
    let cx = f64::from(pw.x - w.x) * lb.scale + lb.offset_x + f64::from(pw.width) * lb.scale / 2.0;
    let cy = f64::from(pw.y - w.y) * lb.scale + lb.offset_y + f64::from(pw.height) * lb.scale / 2.0;
    assert!(cx >= f64::from(sr.x) && cx <= f64::from(sr.x + sr.width) && cy >= f64::from(sr.y) && cy <= f64::from(sr.y + sr.height), "mask {sr:?} vs field centre ({cx},{cy})");
    let non_black = (0..800).step_by(7).flat_map(|y| (0..1280).step_by(7).map(move |x| (x, y))).filter(|&(x, y)| px(x, y)[..3] != [0, 0, 0]).count();
    assert!(non_black > 500, "the window is visible outside the mask");
    // Click the Increment button by coordinates from this screenshot.
    let btn = find(&t, "button", "Increment").bounds.clone().unwrap();
    let bx = f64::from(btn.x - w.x + btn.width / 2) * lb.scale + lb.offset_x;
    let by = f64::from(btn.y - w.y + btn.height / 2) * lb.scale + lb.offset_y;
    let before = cursor();
    let r = s
        .perform("", c.window.as_ref().unwrap().version, wire::perform_request::Action::Click(wire::ClickAction { at: Some(wire::Point { x: bx, y: by }), button: "left".into(), count: 1 }))
        .await;
    let p = performed(&r);
    assert_eq!(p.modality, "coordinate");
    let t0 = Instant::now();
    while fx.get("pressed").as_deref() != Some("1") {
        assert!(t0.elapsed() < Duration::from_secs(3), "click did not land ({})", p.modality_reason);
        std::thread::sleep(Duration::from_millis(50));
    }
    assert_eq!(cursor(), before, "an APP-scope click does not move the cursor");
    eprintln!("click rung: {}", p.modality_reason);
}

#[tokio::test]
#[ignore = "needs Accessibility + Screen Recording and an unlocked screen: scripts/actuator-macos-ax-tests.sh"]
async fn stop_halts_an_input_in_flight_within_500_ms_and_the_application_sees_no_more_keys() {
    let fx = Fixture::launch();
    let mut s = Sess::start(&fx, wire::Scope::App).await;
    let t = s.read(0).await;
    let ver = t.window.as_ref().unwrap().version;
    assert_eq!(code(&s.press(&t, "textfield", "Name", "focus").await), "OK");
    let a2 = s.a.clone();
    let (ident, control) = (s.ident.clone(), s.control.clone());
    let flight = tokio::spawn(async move {
        a2.call(
            Call::Perform(wire::PerformRequest {
                control_id: control,
                application: Some(ident),
                window_version: ver,
                scope: wire::Scope::App as i32,
                idempotency_key: "long".into(),
                action: Some(wire::perform_request::Action::Key(wire::KeyAction { keys: vec!["a".into(); 600] })),
                ..Default::default()
            }),
            Duration::from_secs(30),
        )
        .await
    });
    // Wait until keys are arriving.
    let t0 = Instant::now();
    while fx.keys().len() < 5 {
        assert!(t0.elapsed() < Duration::from_secs(8), "no key reached the application: the raw route does not deliver to a background app");
        std::thread::sleep(Duration::from_millis(20));
    }
    let t_stop = now_ms();
    let stop = call(&s.a, Call::Stop(wire::StopRequest { reason: "test".into(), source: "test".into() })).await;
    let answered = now_ms();
    let Ok(Reply::Stop(st)) = stop else { panic!("{stop:?}") };
    assert_eq!(st.halted_leases, 1);
    std::thread::sleep(Duration::from_millis(400));
    let keys = fx.keys();
    let last = keys.iter().copied().fold(0.0_f64, f64::max);
    let n = keys.len();
    std::thread::sleep(Duration::from_millis(400));
    assert_eq!(fx.keys().len(), n, "no key after the stop settled");
    let latency = last - t_stop;
    eprintln!("V01/QUAL-PX-071: Stop->last injected event = {latency:.1} ms (stop answered after {:.1} ms; helper reports {} ms); {n} of 600 keys delivered", answered - t_stop, st.stopped_within_ms);
    assert!(latency < 500.0, "an event arrived {latency} ms after the Stop");
    assert!(n < 600);
    let r = flight.await.unwrap();
    assert_eq!(code(&r), "USER_ABORTED");
    assert_eq!(delivery(&r), wire::Delivery::Unknown as i32);
}

#[tokio::test]
#[ignore = "needs Accessibility + Screen Recording and an unlocked screen: scripts/actuator-macos-ax-tests.sh"]
async fn physical_input_parks_the_controller_but_the_helpers_own_events_do_not() {
    let fx = Fixture::launch();
    let mut s = Sess::start(&fx, wire::Scope::App).await;
    let t = s.read(0).await;
    let ver = t.window.as_ref().unwrap().version;
    let mut events = s.a.events();
    // The helper's own synthesised keys are not a person.
    let r = s.perform("", ver, wire::perform_request::Action::Key(wire::KeyAction { keys: vec!["a".into(); 5] })).await;
    assert_eq!(code(&r), "OK");
    tokio::time::sleep(Duration::from_millis(300)).await;
    let ping = call(&s.a, Call::Ping(wire::PingRequest { control_id: s.control.clone() })).await;
    assert!(matches!(ping, Ok(Reply::Ping(ref p)) if !p.human_active), "own events counted as human: {ping:?}");
    assert!(events.try_recv().is_err(), "a HumanInput event for the helper's own keys");
    // A device-level event is.
    let t_nudge = Instant::now();
    physical_nudge();
    let ev = tokio::time::timeout(Duration::from_secs(2), events.recv()).await.expect("an event").unwrap();
    assert!(matches!(ev.event, Some(wire::actuator_event::Event::HumanInput(_))), "{ev:?}");
    eprintln!("V01: physical event -> HumanInput event in {:?}", t_nudge.elapsed());
    let r = s.perform("", ver, wire::perform_request::Action::Key(wire::KeyAction { keys: vec!["a".into()] })).await;
    assert_eq!(code(&r), "HUMAN_ACTIVE");
    assert_eq!(delivery(&r), wire::Delivery::NotDelivered as i32);
    tokio::time::sleep(Duration::from_millis(1800)).await;
    let r = s.perform("", ver, wire::perform_request::Action::Key(wire::KeyAction { keys: vec!["a".into()] })).await;
    assert_eq!(code(&r), "OK", "after the cooldown");
}

#[tokio::test]
#[ignore = "needs Accessibility + Screen Recording and an unlocked screen: scripts/actuator-macos-ax-tests.sh"]
async fn the_core_going_away_stops_the_helper_mid_input_and_killing_the_helper_mid_input_is_noticed() {
    let fx = Fixture::launch();
    // (a) The Core's connection ends while an input is in flight.
    let mut s = Sess::start(&fx, wire::Scope::App).await;
    let t = s.read(0).await;
    let ver = t.window.as_ref().unwrap().version;
    assert_eq!(code(&s.press(&t, "textfield", "Name", "focus").await), "OK");
    let (a2, ident, control) = (s.a.clone(), s.ident.clone(), s.control.clone());
    let helper_pid = s.a.pid();
    let _flight = tokio::spawn(async move {
        a2.call(
            Call::Perform(wire::PerformRequest {
                control_id: control,
                application: Some(ident),
                window_version: ver,
                scope: wire::Scope::App as i32,
                idempotency_key: "long2".into(),
                action: Some(wire::perform_request::Action::Key(wire::KeyAction { keys: vec!["a".into(); 600] })),
                ..Default::default()
            }),
            Duration::from_secs(30),
        )
        .await
    });
    let t0 = Instant::now();
    while fx.keys().len() < 5 {
        assert!(t0.elapsed() < Duration::from_secs(8), "no key arrived");
        std::thread::sleep(Duration::from_millis(20));
    }
    // SIGKILL the helper: the strongest "it died" there is.
    let t_kill = now_ms();
    s.a.kill();
    tokio::time::timeout(Duration::from_secs(3), s.a.closed()).await.expect("the loss is noticed");
    std::thread::sleep(Duration::from_millis(400));
    let keys = fx.keys();
    let last = keys.iter().copied().fold(0.0_f64, f64::max);
    assert!(last - t_kill < 500.0, "keys kept arriving {} ms after the helper died", last - t_kill);
    let n = keys.len();
    std::thread::sleep(Duration::from_millis(400));
    assert_eq!(fx.keys().len(), n);
    assert!(!s.a.is_alive());
    let _ = helper_pid;
    // (b) A restarted helper serves again, and the application is still the same one.
    let s2 = Sess::start(&fx, wire::Scope::App).await;
    assert_ne!(s2.a.pid(), helper_pid);
    assert_eq!(s2.info.identity.as_ref().unwrap().pid, fx.pid());
}

#[tokio::test]
#[ignore = "needs Accessibility + Screen Recording and an unlocked screen: scripts/actuator-macos-ax-tests.sh"]
async fn closing_the_connection_stops_injection_within_500_ms() {
    let fx = Fixture::launch();
    let mut s = Sess::start(&fx, wire::Scope::App).await;
    let t = s.read(0).await;
    let ver = t.window.as_ref().unwrap().version;
    assert_eq!(code(&s.press(&t, "textfield", "Name", "focus").await), "OK");
    // Drive the helper by hand so the "Core" can disconnect cleanly: the
    // ProcessActuator owns its child, so use a raw child here.
    drop(s);
    use modbit_protocol::framing::{read_message, write_message};
    let mut c = tokio::process::Command::new(helper_bin());
    c.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).kill_on_drop(true);
    let mut child = c.spawn().unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = child.stdout.take().unwrap();
    let frame = |b: wire::actuator_frame::Body| wire::ActuatorFrame { body: Some(b) };
    write_message(&mut stdin, &frame(wire::actuator_frame::Body::Hello(wire::ActuatorHello { protocol_major: 1, protocol_minor: 0, token: vec![9; 32], core_pid: std::process::id(), core_version: "t".into() }))).await.unwrap();
    let _w: wire::ActuatorFrame = read_message(&mut stdout).await.unwrap().unwrap();
    let ident = {
        let r = write_req(&mut stdin, 1, Call::ResolveApplication(wire::ResolveApplicationRequest { pid: fx.pid(), ..Default::default() })).await;
        let _ = r;
        let resp = read_resp(&mut stdout).await;
        let Some(wire::actuator_response::Result::ResolveApplication(r)) = resp.result else { panic!("{resp:?}") };
        r.application.unwrap().identity.unwrap()
    };
    write_req(&mut stdin, 2, Call::Acquire(wire::AcquireRequest { control_id: "c".into(), scope: wire::Scope::App as i32, application: Some(ident.clone()), window_id: String::new(), lease_ttl_ms: 60_000 })).await;
    let resp = read_resp(&mut stdout).await;
    assert!(matches!(resp.result, Some(wire::actuator_response::Result::Acquire(_))), "{resp:?}");
    write_req(&mut stdin, 3, Call::Perform(wire::PerformRequest { control_id: "c".into(), application: Some(ident), window_version: ver, scope: wire::Scope::App as i32, action: Some(wire::perform_request::Action::Key(wire::KeyAction { keys: vec!["a".into(); 600] })), ..Default::default() })).await;
    let t0 = Instant::now();
    while fx.keys().len() < 5 {
        assert!(t0.elapsed() < Duration::from_secs(8), "no key arrived");
        std::thread::sleep(Duration::from_millis(20));
    }
    let t_close = now_ms();
    drop(stdin); // the Core disconnects
    let status = tokio::time::timeout(Duration::from_secs(3), child.wait()).await.expect("the helper left").unwrap();
    let left_after = now_ms() - t_close;
    std::thread::sleep(Duration::from_millis(400));
    let last = fx.keys().iter().copied().fold(0.0_f64, f64::max);
    eprintln!("QUAL-PX-071: Core disconnect -> helper gone in {left_after:.0} ms ({status}); last key {:.0} ms after", last - t_close);
    assert!(last - t_close < 500.0);
    assert!(left_after < 500.0);
}

async fn write_req(w: &mut tokio::process::ChildStdin, id: u64, call: Call) {
    modbit_protocol::framing::write_message(
        w,
        &wire::ActuatorFrame {
            body: Some(wire::actuator_frame::Body::Request(wire::ActuatorRequest { request_id: id, deadline_ms: 0, call: Some(call) })),
        },
    )
    .await
    .unwrap();
}

async fn read_resp(r: &mut tokio::process::ChildStdout) -> wire::ActuatorResponse {
    loop {
        let f: wire::ActuatorFrame = modbit_protocol::framing::read_message(r).await.unwrap().unwrap();
        if let Some(wire::actuator_frame::Body::Response(x)) = f.body {
            return x;
        }
    }
}

#[tokio::test]
#[ignore = "needs Accessibility + Screen Recording and an unlocked screen: scripts/actuator-macos-ax-tests.sh"]
async fn an_unlocked_screen_with_the_permissions_reports_them_and_a_denied_one_refuses_every_call() {
    let fx = Fixture::launch();
    // Permission revoked (forced off for this run): every operation is PERMISSION_REQUIRED, nothing is touched.
    let a = launch(&[("MODBIT_ACTUATOR_DENY", "accessibility")]).await.unwrap();
    let r = call(&a, Call::ResolveApplication(wire::ResolveApplicationRequest { pid: fx.pid(), ..Default::default() })).await;
    assert!(matches!(r, Ok(Reply::ResolveApplication(_))), "resolving needs no permission: {r:?}");
    let ident = {
        let Ok(Reply::ResolveApplication(r)) = r else { unreachable!() };
        ident_of(&r.application.unwrap())
    };
    for c in [
        Call::Acquire(wire::AcquireRequest { control_id: "c".into(), scope: wire::Scope::App as i32, application: Some(ident.clone()), ..Default::default() }),
        Call::ReadState(wire::ReadStateRequest { control_id: "c".into(), application: Some(ident.clone()), ..Default::default() }),
        Call::Capture(wire::CaptureRequest { control_id: "c".into(), application: Some(ident.clone()), ..Default::default() }),
        Call::Perform(wire::PerformRequest { control_id: "c".into(), application: Some(ident.clone()), action: Some(wire::perform_request::Action::Key(wire::KeyAction { keys: vec!["a".into()] })), ..Default::default() }),
    ] {
        let r = call(&a, c).await;
        assert_eq!(code(&r), "PERMISSION_REQUIRED");
        assert_eq!(delivery(&r), wire::Delivery::NotDelivered as i32);
    }
    assert!(fx.keys().is_empty());
    assert_eq!(fx.get("pressed").as_deref(), Some("0"));
}
