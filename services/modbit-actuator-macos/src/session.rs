//! The actuator's request handling: the single-controller lease, the
//! host-owned Stop, human-input parking, permission loss, the secure-field
//! masking contract, idempotent inputs. Portable: it drives a [`Backend`]
//! and knows nothing about macOS (docs/66 CUC-D04, CUC-D05, CUC-F01).

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use modbit_protocol::v1 as wire;
use tokio::sync::mpsc::Sender;

use crate::backend::{Backend, CaptureArgs, Ctl, Fail, PerformArgs, Permissions, ReadArgs, Res};
use crate::canvas;

/// How long after a physical event the controller stays parked (CUC-D04's
/// cooldown; the Core re-acquires after it).
pub const HUMAN_COOLDOWN: Duration = Duration::from_millis(1500);
/// The poll interval of the human-input monitor: one event is seen within it.
const HUMAN_POLL: Duration = Duration::from_millis(15);
/// Default deadline of a request that names none.
const DEFAULT_DEADLINE: Duration = Duration::from_secs(30);
/// How long `Stop` waits for in-flight work to wind down before it answers.
const STOP_GRACE: Duration = Duration::from_millis(400);
const DEFAULT_NODES: usize = 400;
/// Hard ceiling on a tree read, whatever the Core asks.
const MAX_NODES: usize = 5000;
const IDEMPOTENT_KEEP: usize = 256;

type Tx = Sender<wire::ActuatorFrame>;

struct Lease {
    control_id: String,
    scope: wire::Scope,
    generation: u64,
    last_seen: Instant,
    ttl: Duration,
    pid: u32,
    window_id: String,
    gone_reported: bool,
}

#[derive(Default)]
struct St {
    lease: Option<Lease>,
    generation: u64,
    human_until: Option<Instant>,
    idem: HashMap<String, wire::PerformResponse>,
    idem_order: VecDeque<String>,
    last_perms: Option<Permissions>,
}

/// Permissions an operator or a test has forced OFF (`MODBIT_ACTUATOR_DENY`:
/// a comma list of `accessibility` and `screen_recording`). It can only
/// remove capability, never grant one.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Denied {
    /// Pretend Accessibility is not granted.
    pub accessibility: bool,
    /// Pretend Screen Recording is not granted.
    pub screen_recording: bool,
}

impl Denied {
    /// Parse the comma list; unknown words are ignored.
    #[must_use]
    pub fn parse(list: &str) -> Self {
        let mut d = Self::default();
        for w in list.split(',').map(str::trim) {
            match w {
                "accessibility" => d.accessibility = true,
                "screen_recording" => d.screen_recording = true,
                _ => {}
            }
        }
        d
    }
}

/// The shared state of one actuator connection.
pub struct Hub {
    backend: Arc<dyn Backend>,
    st: Mutex<St>,
    epoch: Arc<AtomicU64>,
    own_hid: Arc<AtomicU64>,
    inflight: AtomicUsize,
    work: tokio::sync::Mutex<()>,
    tx: Tx,
    denied: Denied,
    origin: Instant,
}

fn fail_resp(id: u64, f: Fail) -> wire::ActuatorResponse {
    wire::ActuatorResponse {
        request_id: id,
        result: Some(wire::actuator_response::Result::Error(f.into())),
    }
}

impl Hub {
    /// A hub over `backend` that pushes events on `tx`.
    #[must_use]
    pub fn new(backend: Arc<dyn Backend>, tx: Tx, denied: Denied) -> Arc<Self> {
        Arc::new(Self {
            backend,
            st: Mutex::new(St::default()),
            epoch: Arc::default(),
            own_hid: Arc::default(),
            inflight: AtomicUsize::new(0),
            work: tokio::sync::Mutex::new(()),
            tx,
            denied,
            origin: Instant::now(),
        })
    }

    fn lock(&self) -> MutexGuard<'_, St> {
        self.st.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The permissions in force: the operating system's, minus any forced off.
    #[must_use]
    pub fn permissions(&self) -> Permissions {
        let p = self.backend.permissions();
        Permissions {
            accessibility: p.accessibility && !self.denied.accessibility,
            screen_recording: p.screen_recording && !self.denied.screen_recording,
        }
    }

    fn ctl(&self, deadline_ms: u64) -> Ctl {
        let d = if deadline_ms == 0 {
            DEFAULT_DEADLINE
        } else {
            Duration::from_millis(deadline_ms)
        };
        Ctl::new(
            Arc::clone(&self.epoch),
            Arc::clone(&self.own_hid),
            self.origin,
            d,
        )
    }

    /// Halt everything in flight and drop the lease. Returns whether a lease was held.
    fn halt(&self) -> bool {
        self.epoch.fetch_add(1, Ordering::SeqCst);
        self.lock().lease.take().is_some()
    }

    async fn event(&self, e: wire::actuator_event::Event) {
        let _ = self
            .tx
            .send(wire::ActuatorFrame {
                body: Some(wire::actuator_frame::Body::Event(wire::ActuatorEvent {
                    event: Some(e),
                })),
            })
            .await;
    }

    /// Run a blocking backend call under a [`Ctl`], bounded by its deadline.
    async fn blocking<T: Send + 'static>(
        &self,
        deadline_ms: u64,
        f: impl FnOnce(&Ctl) -> Res<T> + Send + 'static,
    ) -> Res<T> {
        let ctl = self.ctl(deadline_ms);
        let limit = ctl.remaining() + Duration::from_secs(1);
        self.inflight.fetch_add(1, Ordering::SeqCst);
        let h = tokio::task::spawn_blocking(move || f(&ctl));
        let out = match tokio::time::timeout(limit, h).await {
            Ok(Ok(r)) => r,
            Ok(Err(e)) => Err(Fail::unknown(
                "INPUT_FAILED",
                format!("the backend call panicked: {e}"),
            )),
            Err(_) => Err(Fail::unknown(
                "TIMEOUT",
                "the backend did not finish within the deadline",
            )),
        };
        self.inflight.fetch_sub(1, Ordering::SeqCst);
        out
    }

    fn need_ax(&self) -> Res<()> {
        // A missing permission is reported first: the first-run window must be
        // able to say it even when the screen happens to be locked.
        if !self.permissions().accessibility {
            return Err(Fail::nd(
                "PERMISSION_REQUIRED",
                "Accessibility has not been granted to the actuator (System Settings > Privacy & Security > Accessibility)",
            ));
        }
        if self.backend.secure_desktop() {
            return Err(Fail::nd(
                "SECURE_DESKTOP",
                "the screen is locked or a secure prompt is up; nothing is captured and no input is sent",
            ));
        }
        Ok(())
    }

    fn need_screen(&self) -> Res<()> {
        if !self.permissions().screen_recording {
            return Err(Fail::nd(
                "PERMISSION_REQUIRED",
                "Screen Recording has not been granted to the actuator (System Settings > Privacy & Security > Screen & System Audio Recording)",
            ));
        }
        self.need_ax()
    }

    fn need_lease(&self, control_id: &str) -> Res<(wire::Scope, u64)> {
        let mut st = self.lock();
        match &mut st.lease {
            Some(l) if l.control_id == control_id => {
                l.last_seen = Instant::now();
                Ok((l.scope, l.generation))
            }
            _ => Err(Fail::nd(
                "WINDOW_UNVERIFIABLE",
                "no control lease is held under that id",
            )),
        }
    }

    /// Serve one request.
    pub async fn handle(self: &Arc<Self>, req: wire::ActuatorRequest) -> wire::ActuatorResponse {
        use wire::actuator_request::Call;
        use wire::actuator_response::Result as R;
        let id = req.request_id;
        let Some(call) = req.call else {
            return fail_resp(id, Fail::nd("UNSUPPORTED_REQUEST", "an empty request"));
        };
        let dl = req.deadline_ms;
        let out: Res<R> = match call {
            Call::ListApplications(_) => {
                let b = Arc::clone(&self.backend);
                self.blocking(dl, move |_| b.list_apps())
                    .await
                    .map(|applications| {
                        R::ListApplications(wire::ListApplicationsResponse { applications })
                    })
            }
            Call::ResolveApplication(r) => {
                let b = Arc::clone(&self.backend);
                self.blocking(dl, move |_| b.resolve(&r.bundle_id, &r.name, r.pid))
                    .await
                    .map(|a| {
                        R::ResolveApplication(wire::ResolveApplicationResponse {
                            application: Some(a),
                        })
                    })
            }
            Call::Acquire(a) => self.acquire(a, dl).await.map(R::Acquire),
            Call::Release(r) => {
                let held = {
                    let st = self.lock();
                    st.lease
                        .as_ref()
                        .is_some_and(|l| l.control_id == r.control_id)
                };
                if held {
                    self.halt();
                }
                Ok(R::Release(wire::ReleaseResponse { was_held: held }))
            }
            Call::ReadState(r) => self.read_state(r, dl).await.map(R::ReadState),
            Call::Capture(c) => self.capture(c, dl).await.map(R::Capture),
            Call::Wait(w) => self.wait(w).await.map(R::Wait),
            Call::Perform(p) => self.perform(p, dl).await.map(R::Perform),
            Call::Stop(_) => Ok(R::Stop(self.stop().await)),
            Call::Ping(p) => Ok(R::Ping(self.ping(&p.control_id))),
        };
        match out {
            Ok(r) => wire::ActuatorResponse {
                request_id: id,
                result: Some(r),
            },
            Err(f) => fail_resp(id, f),
        }
    }

    /// Stop injecting NOW: every in-flight call sees the new epoch at its next
    /// unit of work, the lease is dropped, and the answer waits (briefly) for
    /// the in-flight work to wind down so `stopped_within_ms` is measured.
    pub async fn stop(&self) -> wire::StopResponse {
        let t0 = Instant::now();
        let n = u32::from(self.halt());
        while self.inflight.load(Ordering::SeqCst) > 0 && t0.elapsed() < STOP_GRACE {
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
        wire::StopResponse {
            halted_leases: n,
            stopped_within_ms: u64::try_from(t0.elapsed().as_millis()).unwrap_or(u64::MAX),
        }
    }

    fn ping(&self, control_id: &str) -> wire::PingResponse {
        let mut st = self.lock();
        let human = st.human_until.is_some_and(|t| t > Instant::now());
        let held = match &mut st.lease {
            Some(l) if l.control_id == control_id => {
                l.last_seen = Instant::now();
                Some(l.generation)
            }
            _ => None,
        };
        wire::PingResponse {
            lease_held: held.is_some(),
            lease_generation: held.unwrap_or(0),
            human_active: human,
        }
    }

    async fn acquire(&self, a: wire::AcquireRequest, dl: u64) -> Res<wire::AcquireResponse> {
        {
            let st = self.lock();
            if st
                .lease
                .as_ref()
                .is_some_and(|l| l.control_id != a.control_id)
            {
                return Err(Fail::nd(
                    "SESSION_BUSY",
                    "another controller holds the machine",
                ));
            }
        }
        let screen = a.scope == wire::Scope::Screen as i32;
        if screen {
            self.need_screen()?;
        } else {
            self.need_ax()?;
        }
        let Some(id) = a.application.clone() else {
            return Err(Fail::nd("WINDOW_UNVERIFIABLE", "no application identity"));
        };
        let b = Arc::clone(&self.backend);
        let window_id = a.window_id.clone();
        let info = self
            .blocking(dl, move |_| {
                let info = b.verify(&id)?;
                if info.elevated {
                    return Err(Fail::nd("TARGET_ELEVATED", "the application runs elevated"));
                }
                if !window_id.is_empty() && !info.windows.iter().any(|w| w.window_id == window_id) {
                    return Err(Fail::nd("WINDOW_UNVERIFIABLE", "no such window"));
                }
                Ok(info)
            })
            .await?;
        let mut st = self.lock();
        // Re-check under the lock: two acquires can race.
        if st
            .lease
            .as_ref()
            .is_some_and(|l| l.control_id != a.control_id)
        {
            return Err(Fail::nd(
                "SESSION_BUSY",
                "another controller holds the machine",
            ));
        }
        st.generation += 1;
        let generation = st.generation;
        let scope = if screen {
            wire::Scope::Screen
        } else {
            wire::Scope::App
        };
        let window_id = if a.window_id.is_empty() {
            info.windows
                .first()
                .map(|w| w.window_id.clone())
                .unwrap_or_default()
        } else {
            a.window_id.clone()
        };
        st.lease = Some(Lease {
            control_id: a.control_id.clone(),
            scope,
            generation,
            last_seen: Instant::now(),
            ttl: Duration::from_millis(a.lease_ttl_ms.max(500)),
            pid: info.identity.as_ref().map_or(0, |i| i.pid),
            window_id,
            gone_reported: false,
        });
        Ok(wire::AcquireResponse {
            control_id: a.control_id,
            lease_generation: generation,
            application: Some(info),
        })
    }

    async fn read_state(&self, r: wire::ReadStateRequest, dl: u64) -> Res<wire::ReadStateResponse> {
        self.need_ax()?;
        self.need_lease(&r.control_id)?;
        let Some(app) = r.application.clone() else {
            return Err(Fail::nd("WINDOW_UNVERIFIABLE", "no application identity"));
        };
        let _w = self.work.lock().await;
        let b = Arc::clone(&self.backend);
        let max = if r.max_nodes == 0 {
            DEFAULT_NODES
        } else {
            (r.max_nodes as usize).min(MAX_NODES)
        };
        self.blocking(dl, move |ctl| {
            b.read_tree(
                &ReadArgs {
                    app: &app,
                    window_id: &r.window_id,
                    snapshot_id: &r.snapshot_id,
                    max_nodes: max,
                    probe: r.probe,
                },
                ctl,
            )
        })
        .await
    }

    async fn capture(&self, c: wire::CaptureRequest, dl: u64) -> Res<wire::CaptureResponse> {
        self.need_screen()?;
        let (scope, _) = self.need_lease(&c.control_id)?;
        let Some(app) = c.application.clone() else {
            return Err(Fail::nd("WINDOW_UNVERIFIABLE", "no application identity"));
        };
        // The scope asked must be the scope of the lease: a lease taken in APP
        // scope never captures the display.
        let asked = if c.scope == wire::Scope::Screen as i32 {
            wire::Scope::Screen
        } else {
            wire::Scope::App
        };
        if asked == wire::Scope::Screen && scope != wire::Scope::Screen {
            return Err(Fail::nd(
                "UNSUPPORTED_REQUEST",
                "the display is captured only under a SCREEN-scope lease",
            ));
        }
        let _w = self.work.lock().await;
        let b = Arc::clone(&self.backend);
        self.blocking(dl, move |ctl| {
            let raw = b.capture(
                &CaptureArgs {
                    app: &app,
                    window_id: &c.window_id,
                    scope: asked,
                },
                ctl,
            )?;
            let lb = canvas::fit(raw.pt_w, raw.pt_h);
            let mut px = canvas::compose(&raw.rgba, raw.px_w, raw.px_h, &lb);
            let rects: Vec<(u32, u32, u32, u32)> =
                raw.secure.iter().filter_map(|r| lb.to_canvas(*r)).collect();
            canvas::mask(&mut px, &rects);
            let secure_regions = rects
                .iter()
                .map(|&(x0, y0, x1, y1)| wire::Rect {
                    x: i32::try_from(x0).unwrap_or(0),
                    y: i32::try_from(y0).unwrap_or(0),
                    width: i32::try_from(x1 - x0).unwrap_or(0),
                    height: i32::try_from(y1 - y0).unwrap_or(0),
                })
                .collect();
            Ok(wire::CaptureResponse {
                png: canvas::encode_png(&px),
                width: canvas::W,
                height: canvas::H,
                letterbox: Some(wire::Letterbox {
                    source_width: lb.source_w,
                    source_height: lb.source_h,
                    scale: lb.scale,
                    offset_x: lb.offset_x,
                    offset_y: lb.offset_y,
                }),
                window: Some(raw.window),
                masked_regions: u32::try_from(rects.len()).unwrap_or(u32::MAX),
                secure_regions,
                frame_digest: canvas::digest(&px),
            })
        })
        .await
    }

    async fn wait(&self, w: wire::WaitRequest) -> Res<wire::WaitResponse> {
        self.need_ax()?;
        self.need_lease(&w.control_id)?;
        let Some(app) = w.application.clone() else {
            return Err(Fail::nd("WINDOW_UNVERIFIABLE", "no application identity"));
        };
        let started = Instant::now();
        let timeout = Duration::from_millis(w.timeout_ms.min(10_000));
        let epoch0 = self.epoch.load(Ordering::SeqCst);
        let since = (!w.since_digest.is_empty()).then(|| w.since_digest.clone());
        let mut last = String::new();
        let mut changed = false;
        loop {
            if self.epoch.load(Ordering::SeqCst) != epoch0 {
                break;
            }
            self.need_lease(&w.control_id)?;
            if since.is_some() || last.is_empty() {
                let _g = self.work.lock().await;
                let b = Arc::clone(&self.backend);
                let (app2, wid) = (app.clone(), w.window_id.clone());
                let (d, _) = self
                    .blocking(2000, move |ctl| b.tree_digest(&app2, &wid, ctl))
                    .await?;
                last = d;
                if since.as_ref().is_some_and(|s| *s != last) {
                    changed = true;
                    break;
                }
            }
            if started.elapsed() >= timeout {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50).min(timeout)).await;
        }
        Ok(wire::WaitResponse {
            changed,
            tree_digest: last,
            waited_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
        })
    }

    async fn perform(&self, p: wire::PerformRequest, dl: u64) -> Res<wire::PerformResponse> {
        if !p.idempotency_key.is_empty()
            && let Some(prior) = self.lock().idem.get(&p.idempotency_key)
        {
            return Ok(prior.clone());
        }
        self.need_ax()?;
        if self.lock().human_until.is_some_and(|t| t > Instant::now()) {
            return Err(Fail::nd(
                "HUMAN_ACTIVE",
                "a physical key or pointer event was seen",
            ));
        }
        let (scope, _) = self.need_lease(&p.control_id)?;
        let _w = self.work.lock().await;
        // A Stop, a release or a revoked permission may have come while this
        // waited for the previous call: the lease is the authority, again.
        self.need_lease(&p.control_id)?;
        self.need_ax()?;
        // The human may have acted while this waited for the previous call.
        if self.lock().human_until.is_some_and(|t| t > Instant::now()) {
            return Err(Fail::nd(
                "HUMAN_ACTIVE",
                "a physical key or pointer event was seen",
            ));
        }
        let b = Arc::clone(&self.backend);
        let key = p.idempotency_key.clone();
        let resp = self
            .blocking(dl, move |ctl| {
                b.perform(&PerformArgs { req: &p, scope }, ctl)
            })
            .await?;
        if !key.is_empty() {
            let mut st = self.lock();
            if st.idem.insert(key.clone(), resp.clone()).is_none() {
                st.idem_order.push_back(key);
            }
            while st.idem_order.len() > IDEMPOTENT_KEEP {
                if let Some(old) = st.idem_order.pop_front() {
                    st.idem.remove(&old);
                }
            }
        }
        Ok(resp)
    }

    /// The monitors: lease watchdog, human input, permission changes, window
    /// loss. Spawned once per connection; they end with the process.
    pub fn spawn_monitors(self: &Arc<Self>) {
        let me = Arc::clone(self);
        tokio::spawn(async move { me.watch_lease().await });
        let me = Arc::clone(self);
        tokio::spawn(async move { me.watch_human().await });
        let me = Arc::clone(self);
        tokio::spawn(async move { me.watch_permissions().await });
    }

    async fn watch_lease(self: Arc<Self>) {
        loop {
            tokio::time::sleep(Duration::from_millis(200)).await;
            let gone = {
                let mut st = self.lock();
                if st
                    .lease
                    .as_ref()
                    .is_some_and(|l| l.last_seen.elapsed() > l.ttl)
                {
                    st.lease = None;
                    drop(st);
                    self.epoch.fetch_add(1, Ordering::SeqCst);
                    None
                } else {
                    st.lease
                        .as_ref()
                        .filter(|l| !l.gone_reported && !l.window_id.is_empty())
                        .map(|l| (l.pid, l.window_id.clone(), l.control_id.clone()))
                }
            };
            if let Some((pid, wid, cid)) = gone {
                let b = Arc::clone(&self.backend);
                let w2 = wid.clone();
                let exists = tokio::task::spawn_blocking(move || b.window_exists(pid, &w2))
                    .await
                    .unwrap_or(true);
                if !exists {
                    if let Some(l) = self.lock().lease.as_mut() {
                        l.gone_reported = true;
                    }
                    self.event(wire::actuator_event::Event::WindowGone(wire::WindowGone {
                        control_id: cid,
                        window_id: wid,
                    }))
                    .await;
                }
            }
        }
    }

    /// A physical key or pointer event parks the controller within one poll:
    /// the epoch moves (an input in flight ends at its next event), the next
    /// input is `HUMAN_ACTIVE` until the cooldown passes (CUC-D04).
    async fn watch_human(self: Arc<Self>) {
        let mut last_event = Instant::now()
            .checked_sub(Duration::from_secs(60))
            .unwrap_or_else(Instant::now);
        loop {
            tokio::time::sleep(HUMAN_POLL).await;
            let b = Arc::clone(&self.backend);
            let Ok(Some(idle)) = tokio::task::spawn_blocking(move || b.human_idle()).await else {
                continue;
            };
            // An event since the previous poll (with slack for scheduling).
            if idle > HUMAN_POLL * 3 {
                continue;
            }
            let now_ms = u64::try_from(self.origin.elapsed().as_millis()).unwrap_or(u64::MAX);
            if now_ms <= self.own_hid.load(Ordering::SeqCst) {
                continue; // the actuator's own SCREEN-scope injection
            }
            let fresh = {
                let mut st = self.lock();
                let was = st.human_until.is_some_and(|t| t > Instant::now());
                st.human_until = Some(Instant::now() + HUMAN_COOLDOWN);
                !was || last_event.elapsed() > Duration::from_millis(250)
            };
            self.epoch.fetch_add(1, Ordering::SeqCst);
            if fresh {
                last_event = Instant::now();
                self.event(wire::actuator_event::Event::HumanInput(wire::HumanInput {
                    at_ms: now_ms,
                    kind: "hid".into(),
                }))
                .await;
            }
        }
    }

    async fn watch_permissions(self: Arc<Self>) {
        loop {
            tokio::time::sleep(Duration::from_millis(500)).await;
            let now = self.permissions();
            let prev = {
                let mut st = self.lock();
                st.last_perms.replace(now)
            };
            let Some(prev) = prev else { continue };
            if prev == now {
                continue;
            }
            if prev.accessibility != now.accessibility {
                self.event(wire::actuator_event::Event::PermissionChanged(
                    wire::PermissionChanged {
                        permission: "accessibility".into(),
                        granted: now.accessibility,
                    },
                ))
                .await;
            }
            if prev.screen_recording != now.screen_recording {
                self.event(wire::actuator_event::Event::PermissionChanged(
                    wire::PermissionChanged {
                        permission: "screen_recording".into(),
                        granted: now.screen_recording,
                    },
                ))
                .await;
            }
            // A revoked permission ends control at once.
            if (prev.accessibility && !now.accessibility)
                || (prev.screen_recording && !now.screen_recording)
            {
                self.halt();
            }
        }
    }
}

#[cfg(test)]
mod tests;
