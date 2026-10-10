//! The actuator RPC handlers of the fixture: every method of
//! `computer.proto`, over the in-memory desktop.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use modbit_protocol::v1 as wire;
use tokio::sync::mpsc::Sender;

use crate::desktop::{App, Desktop, Op, Rect, Win};
use crate::render;

type Shared = Arc<Mutex<State>>;
type Tx = Sender<wire::ActuatorFrame>;

struct Lease {
    control_id: String,
    scope: i32,
    generation: u64,
    last_seen: Instant,
    ttl: Duration,
}

pub struct State {
    desktop: Desktop,
    lease: Option<Lease>,
    generation: u64,
    human_until: Option<Instant>,
    idempotent: HashMap<String, wire::PerformResponse>,
    secure_desktop: bool,
    stop_epoch: u64,
    counters: HashMap<String, u32>,
}

impl State {
    pub fn new(desktop: Desktop) -> Self {
        let secure = desktop.faults.secure_desktop;
        Self {
            desktop,
            lease: None,
            generation: 0,
            human_until: None,
            idempotent: HashMap::new(),
            secure_desktop: secure,
            stop_epoch: 0,
            counters: HashMap::new(),
        }
    }
}

fn lock(s: &Shared) -> MutexGuard<'_, State> {
    s.lock().unwrap_or_else(|e| e.into_inner())
}

/// Release a lease that has not heard from the Core for its ttl (the
/// watchdog of the actuator's side).
pub fn spawn_lease_watchdog(s: Shared) {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_millis(200)).await;
            let mut st = lock(&s);
            if st
                .lease
                .as_ref()
                .is_some_and(|l| l.last_seen.elapsed() > l.ttl)
            {
                st.lease = None;
                st.stop_epoch += 1;
            }
        }
    });
}

fn err(code: &str, message: impl Into<String>, delivery: wire::Delivery) -> wire::ActuatorError {
    wire::ActuatorError {
        code: code.into(),
        message: message.into(),
        delivery: delivery as i32,
    }
}

fn win_info(app: &App, w: &Win) -> wire::WindowInfo {
    let blocked = app
        .windows
        .iter()
        .any(|o| o.id != w.id && o.modal && !o.hidden);
    wire::WindowInfo {
        window_id: w.id.clone(),
        title: w.title.clone(),
        bounds: Some(rect(w.bounds)),
        focused: w.focused,
        modal: blocked,
        minimized: w.minimized,
        version: w.version,
    }
}

fn rect(r: Rect) -> wire::Rect {
    wire::Rect {
        x: r.x,
        y: r.y,
        width: r.width,
        height: r.height,
    }
}

fn identity(app: &App) -> wire::ApplicationIdentity {
    wire::ApplicationIdentity {
        bundle_id: app.bundle_id.clone(),
        executable_path: app.executable_path.clone(),
        signing_identity: app.signing_identity.clone(),
        pid: app.pid,
        name: app.name.clone(),
    }
}

fn app_info(app: &App) -> wire::ApplicationInfo {
    wire::ApplicationInfo {
        identity: Some(identity(app)),
        windows: app
            .windows
            .iter()
            .filter(|w| !w.hidden)
            .map(|w| win_info(app, w))
            .collect(),
        elevated: app.elevated,
    }
}

fn same_identity(app: &App, id: &wire::ApplicationIdentity) -> bool {
    app.bundle_id == id.bundle_id
        && app.executable_path == id.executable_path
        && app.signing_identity == id.signing_identity
        && app.pid == id.pid
}

fn tree_node(i: usize, e: &crate::desktop::El, win: &Win) -> wire::AxNode {
    let parent = e
        .parent
        .as_ref()
        .and_then(|p| win.node_id_of(p))
        .unwrap_or_else(|| "n0".to_owned());
    wire::AxNode {
        element_id: format!("n{}", i + 1),
        parent_id: parent,
        role: e.role.clone(),
        name: e.name.clone(),
        // The actuator never reads a secure field's value.
        value: if e.secure {
            String::new()
        } else {
            e.value.clone()
        },
        settable: e.settable,
        secure: e.secure,
        enabled: e.enabled,
        focused: e.focused,
        actions: e.actions.clone(),
        bounds: Some(rect(e.bounds)),
    }
}

/// Everything a request needs to be about a held lease and a live application.
struct Target<'a> {
    app: &'a mut App,
}

fn require_lease(st: &mut State, control_id: &str) -> Result<(), wire::ActuatorError> {
    match &mut st.lease {
        Some(l) if l.control_id == control_id => {
            l.last_seen = Instant::now();
            Ok(())
        }
        _ => Err(err(
            "WINDOW_UNVERIFIABLE",
            "no control lease is held under that id",
            wire::Delivery::NotDelivered,
        )),
    }
}

fn target<'a>(
    st: &'a mut State,
    control_id: &str,
    id: Option<&wire::ApplicationIdentity>,
) -> Result<Target<'a>, wire::ActuatorError> {
    require_lease(st, control_id)?;
    let Some(id) = id else {
        return Err(err(
            "WINDOW_UNVERIFIABLE",
            "no application identity",
            wire::Delivery::NotDelivered,
        ));
    };
    let Some(app) = st.desktop.app_mut_by_pid(id.pid) else {
        return Err(err(
            "WINDOW_UNVERIFIABLE",
            "no such process",
            wire::Delivery::NotDelivered,
        ));
    };
    if !same_identity(app, id) {
        return Err(err(
            "WINDOW_UNVERIFIABLE",
            "the application running as that process is not the one that was approved",
            wire::Delivery::NotDelivered,
        ));
    }
    Ok(Target { app })
}

pub async fn handle(
    shared: &Shared,
    tx: &Tx,
    req: wire::ActuatorRequest,
) -> wire::ActuatorResponse {
    use wire::actuator_request::Call;
    use wire::actuator_response::Result as R;
    let Some(call) = req.call else {
        return wire::ActuatorResponse {
            request_id: req.request_id,
            result: Some(R::Error(err(
                "UNSUPPORTED_REQUEST",
                "an empty request",
                wire::Delivery::NotDelivered,
            ))),
        };
    };
    let slow = lock(shared).desktop.faults.slow_read_ms;
    let out: Result<R, wire::ActuatorError> = match call {
        Call::ListApplications(_) => {
            let st = lock(shared);
            Ok(R::ListApplications(wire::ListApplicationsResponse {
                applications: st.desktop.apps.iter().map(app_info).collect(),
            }))
        }
        Call::ResolveApplication(r) => {
            let st = lock(shared);
            let hit = st.desktop.apps.iter().find(|a| {
                (r.pid != 0 && a.pid == r.pid)
                    || (r.pid == 0 && !r.bundle_id.is_empty() && a.bundle_id == r.bundle_id)
                    || (r.pid == 0
                        && r.bundle_id.is_empty()
                        && a.name.eq_ignore_ascii_case(&r.name))
            });
            match hit {
                Some(a) => Ok(R::ResolveApplication(wire::ResolveApplicationResponse {
                    application: Some(app_info(a)),
                })),
                None => Err(err(
                    "WINDOW_UNVERIFIABLE",
                    "no such application",
                    wire::Delivery::NotDelivered,
                )),
            }
        }
        Call::Acquire(a) => acquire(shared, a),
        Call::Release(r) => {
            let mut st = lock(shared);
            let held = st
                .lease
                .as_ref()
                .is_some_and(|l| l.control_id == r.control_id);
            if held {
                st.lease = None;
            }
            Ok(R::Release(wire::ReleaseResponse { was_held: held }))
        }
        Call::ReadState(r) => {
            if slow > 0 {
                tokio::time::sleep(Duration::from_millis(slow)).await;
            }
            read_state(shared, r)
        }
        Call::Capture(c) => capture(shared, c),
        Call::Wait(w) => wait(shared, w).await,
        Call::Perform(p) => perform(shared, tx, p).await,
        Call::Stop(_) => {
            let mut st = lock(shared);
            let n = u32::from(st.lease.take().is_some());
            st.stop_epoch += 1;
            Ok(R::Stop(wire::StopResponse {
                halted_leases: n,
                stopped_within_ms: 1,
            }))
        }
        Call::Ping(p) => {
            let mut st = lock(shared);
            let human = st.human_until.is_some_and(|t| t > Instant::now());
            let held = match &mut st.lease {
                Some(l) if l.control_id == p.control_id => {
                    l.last_seen = Instant::now();
                    Some(l.generation)
                }
                _ => None,
            };
            Ok(R::Ping(wire::PingResponse {
                lease_held: held.is_some(),
                lease_generation: held.unwrap_or(0),
                human_active: human,
            }))
        }
    };
    wire::ActuatorResponse {
        request_id: req.request_id,
        result: Some(match out {
            Ok(r) => r,
            Err(e) => R::Error(e),
        }),
    }
}

fn acquire(
    shared: &Shared,
    a: wire::AcquireRequest,
) -> Result<wire::actuator_response::Result, wire::ActuatorError> {
    let mut st = lock(shared);
    if st
        .lease
        .as_ref()
        .is_some_and(|l| l.control_id != a.control_id)
    {
        return Err(err(
            "SESSION_BUSY",
            "another controller holds the machine",
            wire::Delivery::NotDelivered,
        ));
    }
    let Some(id) = a.application.as_ref() else {
        return Err(err(
            "WINDOW_UNVERIFIABLE",
            "no application identity",
            wire::Delivery::NotDelivered,
        ));
    };
    let Some(app) = st.desktop.app_by_pid(id.pid) else {
        return Err(err(
            "WINDOW_UNVERIFIABLE",
            "no such process",
            wire::Delivery::NotDelivered,
        ));
    };
    if !same_identity(app, id) {
        return Err(err(
            "WINDOW_UNVERIFIABLE",
            "the application is not the one named",
            wire::Delivery::NotDelivered,
        ));
    }
    if app.elevated {
        return Err(err(
            "TARGET_ELEVATED",
            "the application runs elevated",
            wire::Delivery::NotDelivered,
        ));
    }
    if !a.window_id.is_empty() && !app.windows.iter().any(|w| w.id == a.window_id && !w.hidden) {
        return Err(err(
            "WINDOW_UNVERIFIABLE",
            "no such window",
            wire::Delivery::NotDelivered,
        ));
    }
    let info = app_info(app);
    st.generation += 1;
    let generation = st.generation;
    st.lease = Some(Lease {
        control_id: a.control_id.clone(),
        scope: a.scope,
        generation,
        last_seen: Instant::now(),
        ttl: Duration::from_millis(a.lease_ttl_ms.max(500)),
    });
    Ok(wire::actuator_response::Result::Acquire(
        wire::AcquireResponse {
            control_id: a.control_id,
            lease_generation: generation,
            application: Some(info),
        },
    ))
}

fn read_state(
    shared: &Shared,
    r: wire::ReadStateRequest,
) -> Result<wire::actuator_response::Result, wire::ActuatorError> {
    let mut st = lock(shared);
    if st.secure_desktop {
        return Err(err(
            "SECURE_DESKTOP",
            "the secure desktop is up",
            wire::Delivery::NotDelivered,
        ));
    }
    if st.desktop.faults.permission_missing {
        return Err(err(
            "PERMISSION_REQUIRED",
            "accessibility has not been granted to the actuator",
            wire::Delivery::NotDelivered,
        ));
    }
    let t = target(&mut st, &r.control_id, r.application.as_ref())?;
    if t.app.no_accessibility {
        return Err(err(
            "ACCESSIBILITY_UNAVAILABLE",
            "the application exposes no accessibility tree",
            wire::Delivery::NotDelivered,
        ));
    }
    let app = &mut *t.app;
    let wid = if r.window_id.is_empty() {
        app.windows
            .iter()
            .find(|w| !w.hidden)
            .map(|w| w.id.clone())
            .unwrap_or_default()
    } else {
        r.window_id.clone()
    };
    let Some(wi) = app.windows.iter().position(|w| w.id == wid && !w.hidden) else {
        return Err(err(
            "WINDOW_UNVERIFIABLE",
            "no such window",
            wire::Delivery::NotDelivered,
        ));
    };
    if !r.probe {
        app.windows[wi].snapshot = r.snapshot_id.clone();
    }
    let max = if r.max_nodes == 0 {
        400
    } else {
        r.max_nodes as usize
    };
    let win = &app.windows[wi];
    let mut nodes = vec![wire::AxNode {
        element_id: "n0".into(),
        parent_id: String::new(),
        role: "window".into(),
        name: win.title.clone(),
        value: String::new(),
        settable: false,
        secure: false,
        enabled: true,
        focused: win.focused,
        actions: vec![],
        bounds: Some(rect(win.bounds)),
    }];
    let mut truncated = false;
    for (i, e) in win.elements.iter().enumerate() {
        if nodes.len() >= max {
            truncated = true;
            break;
        }
        nodes.push(tree_node(i, e, win));
    }
    let info = win_info(app, win);
    Ok(wire::actuator_response::Result::ReadState(
        wire::ReadStateResponse {
            snapshot_id: r.snapshot_id,
            window: Some(info),
            nodes,
            truncated,
            tree_digest: win.digest(),
        },
    ))
}

fn capture(
    shared: &Shared,
    c: wire::CaptureRequest,
) -> Result<wire::actuator_response::Result, wire::ActuatorError> {
    let mut st = lock(shared);
    if st.secure_desktop {
        return Err(err(
            "SECURE_DESKTOP",
            "the screen is locked or showing a secure prompt",
            wire::Delivery::NotDelivered,
        ));
    }
    let faults = st.desktop.faults.clone();
    if faults.permission_missing {
        return Err(err(
            "PERMISSION_REQUIRED",
            "screen recording has not been granted to the actuator",
            wire::Delivery::NotDelivered,
        ));
    }
    let display = st.desktop.display;
    let t = target(&mut st, &c.control_id, c.application.as_ref())?;
    let app = &*t.app;
    let wid = if c.window_id.is_empty() {
        app.windows
            .iter()
            .find(|w| !w.hidden)
            .map(|w| w.id.clone())
            .unwrap_or_default()
    } else {
        c.window_id.clone()
    };
    let Some(win) = app.windows.iter().find(|w| w.id == wid && !w.hidden) else {
        return Err(err(
            "WINDOW_UNVERIFIABLE",
            "no such window",
            wire::Delivery::NotDelivered,
        ));
    };
    let info = win_info(app, win);
    let screen = c.scope == wire::Scope::Screen as i32;
    let frame = if screen {
        let apps = st.desktop.apps.clone();
        render::render_display(display, &apps, faults.skip_mask, faults.hide_secure_regions)
    } else {
        render::render_window(win, faults.skip_mask, faults.hide_secure_regions)
    };
    let mut lb = frame.letterbox;
    if faults.bad_letterbox {
        lb.scale *= 1.5;
    }
    let png = if faults.wrong_size {
        render::wrong_size_png()
    } else {
        frame.png
    };
    let (w, h) = if faults.wrong_size {
        (640, 400)
    } else {
        (render::CANVAS_W, render::CANVAS_H)
    };
    Ok(wire::actuator_response::Result::Capture(
        wire::CaptureResponse {
            png,
            width: w,
            height: h,
            letterbox: Some(wire::Letterbox {
                source_width: lb.source_w,
                source_height: lb.source_h,
                scale: lb.scale,
                offset_x: lb.offset_x,
                offset_y: lb.offset_y,
            }),
            window: Some(info),
            masked_regions: frame.masked,
            secure_regions: frame.secure.iter().map(|r| rect(*r)).collect(),
            frame_digest: frame.digest,
        },
    ))
}

async fn wait(
    shared: &Shared,
    w: wire::WaitRequest,
) -> Result<wire::actuator_response::Result, wire::ActuatorError> {
    let started = Instant::now();
    let (epoch0, mut last) = {
        let mut st = lock(shared);
        let epoch = st.stop_epoch;
        let t = target(&mut st, &w.control_id, w.application.as_ref())?;
        let wid = if w.window_id.is_empty() {
            t.app
                .windows
                .iter()
                .find(|x| !x.hidden)
                .map(|x| x.id.clone())
                .unwrap_or_default()
        } else {
            w.window_id.clone()
        };
        let digest = t
            .app
            .windows
            .iter()
            .find(|x| x.id == wid)
            .map(Win::digest)
            .unwrap_or_default();
        (epoch, digest)
    };
    let since = if w.since_digest.is_empty() {
        None
    } else {
        Some(w.since_digest.clone())
    };
    let timeout = Duration::from_millis(w.timeout_ms.min(10_000));
    let mut changed = false;
    while started.elapsed() < timeout {
        tokio::time::sleep(Duration::from_millis(25)).await;
        let mut st = lock(shared);
        if st.stop_epoch != epoch0 {
            break;
        }
        let t = target(&mut st, &w.control_id, w.application.as_ref())?;
        let wid = if w.window_id.is_empty() {
            t.app
                .windows
                .iter()
                .find(|x| !x.hidden)
                .map(|x| x.id.clone())
                .unwrap_or_default()
        } else {
            w.window_id.clone()
        };
        last = t
            .app
            .windows
            .iter()
            .find(|x| x.id == wid)
            .map(Win::digest)
            .unwrap_or_default();
        if let Some(s) = &since
            && &last != s
        {
            changed = true;
            break;
        }
    }
    Ok(wire::actuator_response::Result::Wait(wire::WaitResponse {
        changed,
        tree_digest: last,
        waited_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
    }))
}

/// What an input leaves to do after its effect is applied.
#[derive(Default)]
struct After {
    abort: bool,
    hang_ms: u64,
    human: bool,
}

#[derive(Default)]
struct Effects {
    changes: Vec<wire::ElementChange>,
    structure_changed: bool,
    after: After,
}

fn run_ops(st: &mut State, pid: u32, win_id: &str, key: &str, ops: &[Op], fx: &mut Effects) {
    let n = {
        let c = st.counters.entry(key.to_owned()).or_insert(0);
        *c += 1;
        *c
    };
    for op in ops {
        match op {
            Op::SetValue { target, value } => {
                let value = value.replace("{n}", &n.to_string());
                if let Some(app) = st.desktop.app_mut_by_pid(pid)
                    && let Some(w) = app.windows.iter_mut().find(|w| w.id == win_id)
                    && let Some(i) = w.elements.iter().position(|e| &e.id == target)
                {
                    let before = if w.elements[i].secure {
                        String::new()
                    } else {
                        w.elements[i].value.clone()
                    };
                    w.elements[i].value = value.clone();
                    fx.changes.push(wire::ElementChange {
                        element_id: format!("n{}", i + 1),
                        kind: "value".into(),
                        before,
                        after: if w.elements[i].secure {
                            String::new()
                        } else {
                            value
                        },
                    });
                }
            }
            Op::Append { target, text } => {
                if let Some(app) = st.desktop.app_mut_by_pid(pid)
                    && let Some(w) = app.windows.iter_mut().find(|w| w.id == win_id)
                    && let Some(i) = w.elements.iter().position(|e| &e.id == target)
                {
                    let before = w.elements[i].value.clone();
                    w.elements[i].value.push_str(text);
                    fx.changes.push(wire::ElementChange {
                        element_id: format!("n{}", i + 1),
                        kind: "value".into(),
                        before,
                        after: w.elements[i].value.clone(),
                    });
                }
            }
            Op::Focus { target } => {
                if let Some(app) = st.desktop.app_mut_by_pid(pid)
                    && let Some(w) = app.windows.iter_mut().find(|w| w.id == win_id)
                {
                    for e in &mut w.elements {
                        e.focused = &e.id == target;
                    }
                    fx.changes.push(wire::ElementChange {
                        element_id: target.clone(),
                        kind: "focus".into(),
                        before: String::new(),
                        after: String::new(),
                    });
                }
            }
            Op::OpenWindow { window } | Op::CloseWindow { window } => {
                let open = matches!(op, Op::OpenWindow { .. });
                if let Some(app) = st.desktop.app_mut_by_pid(pid)
                    && let Some(w) = app.windows.iter_mut().find(|w| &w.id == window)
                {
                    w.hidden = !open;
                    w.version += 1;
                    fx.structure_changed = true;
                }
            }
            Op::SwapIdentity {
                bundle_id,
                signing_identity,
            } => {
                if let Some(app) = st.desktop.app_mut_by_pid(pid) {
                    app.bundle_id = bundle_id.clone();
                    app.signing_identity = signing_identity.clone();
                }
            }
            Op::RenameWindow { title } => {
                if let Some(app) = st.desktop.app_mut_by_pid(pid)
                    && let Some(w) = app.windows.iter_mut().find(|w| w.id == win_id)
                {
                    w.title = title.clone();
                }
            }
            Op::MoveWindow { x, y } => {
                if let Some(app) = st.desktop.app_mut_by_pid(pid)
                    && let Some(w) = app.windows.iter_mut().find(|w| w.id == win_id)
                {
                    let (dx, dy) = (x - w.bounds.x, y - w.bounds.y);
                    w.bounds.x = *x;
                    w.bounds.y = *y;
                    for e in &mut w.elements {
                        e.bounds.x += dx;
                        e.bounds.y += dy;
                    }
                    w.version += 1;
                }
            }
            Op::Remove { target } => {
                if let Some(app) = st.desktop.app_mut_by_pid(pid)
                    && let Some(w) = app.windows.iter_mut().find(|w| w.id == win_id)
                {
                    w.elements.retain(|e| &e.id != target);
                    fx.structure_changed = true;
                }
            }
            Op::EmitHumanInput => fx.after.human = true,
            Op::AbortAfterDeliver => fx.after.abort = true,
            Op::HangMs { ms } => fx.after.hang_ms = fx.after.hang_ms.max(*ms),
            Op::SecureDesktop { on } => st.secure_desktop = *on,
            Op::FailInput { .. } => {}
        }
    }
}

async fn perform(
    shared: &Shared,
    tx: &Tx,
    p: wire::PerformRequest,
) -> Result<wire::actuator_response::Result, wire::ActuatorError> {
    use wire::perform_request::Action;
    let nd = wire::Delivery::NotDelivered;
    let (response, after, epoch0) = {
        let mut st = lock(shared);
        if !p.idempotency_key.is_empty()
            && let Some(prior) = st.idempotent.get(&p.idempotency_key)
        {
            return Ok(wire::actuator_response::Result::Perform(prior.clone()));
        }
        if st.secure_desktop {
            return Err(err("SECURE_DESKTOP", "the secure desktop is up", nd));
        }
        if st.desktop.faults.permission_missing {
            return Err(err(
                "PERMISSION_REQUIRED",
                "accessibility has not been granted to the actuator",
                nd,
            ));
        }
        if st.human_until.is_some_and(|t| t > Instant::now()) {
            return Err(err(
                "HUMAN_ACTIVE",
                "a physical key or pointer event was seen",
                nd,
            ));
        }
        let scope = st.lease.as_ref().map(|l| l.scope);
        let epoch = st.stop_epoch;
        let display = st.desktop.display;
        let t = target(&mut st, &p.control_id, p.application.as_ref())?;
        let pid = t.app.pid;
        let wid = if p.window_id.is_empty() {
            t.app
                .windows
                .iter()
                .find(|w| !w.hidden)
                .map(|w| w.id.clone())
                .unwrap_or_default()
        } else {
            p.window_id.clone()
        };
        let Some(wi) = t.app.windows.iter().position(|w| w.id == wid && !w.hidden) else {
            return Err(err("WINDOW_UNVERIFIABLE", "no such window", nd));
        };
        // A modal dialog in front of the window blocks it.
        let blocked = t
            .app
            .windows
            .iter()
            .any(|o| o.id != wid && o.modal && !o.hidden);
        if blocked {
            return Err(err(
                "MODAL_BLOCKING",
                "a modal dialog is in front of the window",
                nd,
            ));
        }
        let win = t.app.windows[wi].clone();
        let mut fx = Effects::default();
        let mut modality = "ax";
        let mut reason = String::new();
        let mut ops: Vec<(String, Vec<Op>)> = Vec::new();
        match p.action.as_ref() {
            None => return Err(err("UNSUPPORTED_REQUEST", "no action", nd)),
            Some(Action::Press(a)) => {
                if win.snapshot != p.snapshot_id || p.snapshot_id.is_empty() {
                    return Err(err(
                        "TARGET_STALE",
                        "the snapshot is not the window's current one",
                        nd,
                    ));
                }
                let Some(i) = win.index_of_node(&a.element_id) else {
                    return Err(err("TARGET_STALE", "no such element in the snapshot", nd));
                };
                let e = &win.elements[i];
                if !e.enabled {
                    return Err(err("TARGET_NOT_EDITABLE", "the element is disabled", nd));
                }
                if !e.actions.iter().any(|x| x == &a.action) {
                    return Err(err(
                        "UNSUPPORTED_REQUEST",
                        "the element does not offer that action",
                        nd,
                    ));
                }
                if let Some(b) = e.behaviors.get(&a.action) {
                    ops.push((format!("{wid}/{}/{}", e.id, a.action), b.clone()));
                }
                if a.action == "focus" {
                    ops.push((
                        format!("{wid}/{}/focus", e.id),
                        vec![Op::Focus {
                            target: e.id.clone(),
                        }],
                    ));
                }
            }
            Some(Action::SetValue(a)) => {
                if win.snapshot != p.snapshot_id || p.snapshot_id.is_empty() {
                    return Err(err(
                        "TARGET_STALE",
                        "the snapshot is not the window's current one",
                        nd,
                    ));
                }
                let Some(i) = win.index_of_node(&a.element_id) else {
                    return Err(err("TARGET_STALE", "no such element in the snapshot", nd));
                };
                let e = &win.elements[i];
                if e.secure {
                    return Err(err("ACTION_UNSAFE", "a secure field is never set", nd));
                }
                if !e.settable || !e.enabled {
                    return Err(err(
                        "TARGET_NOT_EDITABLE",
                        "the element does not take a value",
                        nd,
                    ));
                }
                ops.push((
                    format!("{wid}/{}/set", e.id),
                    vec![Op::SetValue {
                        target: e.id.clone(),
                        value: a.value.clone(),
                    }],
                ));
            }
            Some(Action::Click(a)) => {
                modality = "coordinate";
                let Some(pt) = a.at.as_ref() else {
                    return Err(err("UNSUPPORTED_REQUEST", "no point", nd));
                };
                if p.window_version != win.version {
                    return Err(err(
                        "TARGET_STALE",
                        "the window moved or was replaced since the screenshot",
                        nd,
                    ));
                }
                let (lb, origin) = if scope == Some(wire::Scope::Screen as i32) {
                    (
                        render::fit(display.width as u32, display.height as u32),
                        (display.x, display.y),
                    )
                } else {
                    (
                        render::fit(win.bounds.width as u32, win.bounds.height as u32),
                        (win.bounds.x, win.bounds.y),
                    )
                };
                let Some((sx, sy)) = lb.to_source(pt.x, pt.y) else {
                    return Err(err(
                        "TARGET_OCCLUDED",
                        "the point is outside the window",
                        nd,
                    ));
                };
                let (ax, ay) = (sx + f64::from(origin.0), sy + f64::from(origin.1));
                if let Some(e) = win
                    .elements
                    .iter()
                    .rev()
                    .find(|e| e.bounds.contains(ax, ay))
                {
                    reason = format!("hit {}", e.id);
                    if let Some(b) = e.behaviors.get("press") {
                        ops.push((format!("{wid}/{}/press", e.id), b.clone()));
                    }
                    if e.settable {
                        ops.push((
                            format!("{wid}/{}/focus", e.id),
                            vec![Op::Focus {
                                target: e.id.clone(),
                            }],
                        ));
                    }
                } else {
                    reason = "no element at the point".into();
                }
            }
            Some(Action::Move(_)) => {
                modality = "coordinate";
                if scope != Some(wire::Scope::Screen as i32) {
                    return Err(err(
                        "UNSUPPORTED_REQUEST",
                        "moving the pointer needs SCREEN scope",
                        nd,
                    ));
                }
            }
            Some(Action::Drag(_) | Action::Scroll(_)) => {
                modality = "coordinate";
                if p.window_version != win.version {
                    return Err(err(
                        "TARGET_STALE",
                        "the window moved or was replaced since the screenshot",
                        nd,
                    ));
                }
            }
            Some(Action::Type(a)) => {
                modality = "raw";
                reason = "no element action types text".into();
                let Some(e) = win.elements.iter().find(|e| e.focused) else {
                    return Err(err(
                        "TARGET_NOT_EDITABLE",
                        "nothing has the keyboard focus",
                        nd,
                    ));
                };
                if e.secure {
                    return Err(err("ACTION_UNSAFE", "the focused field is secure", nd));
                }
                if !a.text.is_empty() {
                    ops.push((
                        format!("{wid}/{}/type", e.id),
                        vec![Op::Append {
                            target: e.id.clone(),
                            text: a.text.clone(),
                        }],
                    ));
                }
                if a.then == "enter"
                    && let Some(b) = e.behaviors.get("enter")
                {
                    ops.push((format!("{wid}/{}/enter", e.id), b.clone()));
                }
            }
            Some(Action::Key(a)) => {
                modality = "raw";
                reason = "keys have no element action".into();
                if let Some(e) = win.elements.iter().find(|e| e.focused) {
                    for k in &a.keys {
                        if let Some(b) = e.behaviors.get(&format!("key:{k}")) {
                            ops.push((format!("{wid}/{}/key:{k}", e.id), b.clone()));
                        }
                    }
                }
            }
        }
        // An input the element refuses is refused before anything is injected.
        for (_, o) in &ops {
            for op in o {
                if let Op::FailInput { code } = op {
                    return Err(err(code, "the application refused the input", nd));
                }
            }
        }
        for (key, o) in &ops {
            run_ops(&mut st, pid, &wid, key, o, &mut fx);
        }
        // The window after the effect.
        let (info, digest) = match st.desktop.app_by_pid(pid) {
            Some(app) => match app.windows.iter().find(|w| w.id == wid) {
                Some(w) => (win_info(app, w), w.digest()),
                None => (wire::WindowInfo::default(), String::new()),
            },
            None => (wire::WindowInfo::default(), String::new()),
        };
        let response = wire::PerformResponse {
            delivery: wire::Delivery::Delivered as i32,
            modality: modality.into(),
            modality_reason: reason,
            structure_changed: fx.structure_changed,
            tree_digest: digest,
            changes: fx.changes.into_iter().take(50).collect(),
            window: Some(info),
        };
        if !p.idempotency_key.is_empty() {
            st.idempotent
                .insert(p.idempotency_key.clone(), response.clone());
        }
        (response, fx.after, epoch)
    };
    if after.human {
        {
            let mut st = lock(shared);
            st.human_until = Some(Instant::now() + Duration::from_millis(1500));
        }
        let _ = tx
            .send(wire::ActuatorFrame {
                body: Some(wire::actuator_frame::Body::Event(wire::ActuatorEvent {
                    event: Some(wire::actuator_event::Event::HumanInput(wire::HumanInput {
                        at_ms: 0,
                        kind: "key".into(),
                    })),
                })),
            })
            .await;
    }
    if after.abort {
        // The input has been applied; the actuator dies before it answers.
        std::process::abort();
    }
    if after.hang_ms > 0 {
        let until = Instant::now() + Duration::from_millis(after.hang_ms);
        while Instant::now() < until {
            tokio::time::sleep(Duration::from_millis(25)).await;
            if lock(shared).stop_epoch != epoch0 {
                return Err(err(
                    "USER_ABORTED",
                    "stopped while the input was in flight",
                    wire::Delivery::Unknown,
                ));
            }
        }
    }
    Ok(wire::actuator_response::Result::Perform(response))
}
