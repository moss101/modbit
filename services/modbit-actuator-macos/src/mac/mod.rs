//! The macOS backend: Accessibility first, window-server capture, synthesised
//! input last. Everything unsafe is in this directory, each call site small
//! and commented; the portable logic (lease, Stop, masking contract) is not.
#![allow(unsafe_code)]

mod ax;
mod capture;
mod cf;
mod cgwin;
mod ffi;
mod input;
mod perform;
mod procinfo;

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

use modbit_protocol::v1 as wire;

use crate::backend::{
    Backend, CaptureArgs, Ctl, Fail, PerformArgs, Permissions, RawFrame, ReadArgs, Res,
};

/// Ask the OS for the permission prompts, from this process (never another).
pub fn request_permissions() {
    // SAFETY: the dictionary and its two CF objects are created, used for the
    // call, and released; the callbacks are the system's.
    unsafe {
        let key = cf::CfStr::new("AXTrustedCheckOptionPrompt");
        let keys = [key.raw()];
        let vals = [ffi::kCFBooleanTrue];
        let d = ffi::CFDictionaryCreate(
            ffi::kCFAllocatorDefault,
            keys.as_ptr(),
            vals.as_ptr(),
            1,
            (&raw const ffi::kCFTypeDictionaryKeyCallBacks).cast(),
            (&raw const ffi::kCFTypeDictionaryValueCallBacks).cast(),
        );
        if let Some(d) = cf::Cf::own(d) {
            ffi::AXIsProcessTrustedWithOptions(d.raw());
        }
        ffi::CGRequestScreenCaptureAccess();
    }
}

/// What a tree read left behind, so a later element id can be resolved.
pub(crate) struct Snapshot {
    pub id: String,
    pub handles: Vec<ax::Ax>,
    pub nodes: Vec<wire::AxNode>,
    pub structure: String,
}

/// The production backend.
pub struct Mac {
    pub(crate) snapshots: Mutex<HashMap<(u32, String), Snapshot>>,
    /// The element the actuator last gave the keyboard focus in a window: a
    /// background application may report no focused element until it is
    /// active, and typing still has to know where it is going.
    /// Applications whose accessibility tree has been walked once. The first
    /// walk wakes the application's accessibility support up and AppKit adds
    /// elements it had not built (measured: 11 elements, then 12 on every
    /// later read), so the first walk is made, discarded, and made again.
    warmed: Mutex<std::collections::HashSet<u32>>,
    pub(crate) focus_hint: Mutex<HashMap<(u32, String), ax::Ax>>,
    own_pid: u32,
}

impl Mac {
    /// A backend for this process.
    #[must_use]
    pub fn new() -> Self {
        Self {
            snapshots: Mutex::default(),
            warmed: Mutex::default(),
            focus_hint: Mutex::default(),
            own_pid: std::process::id(),
        }
    }

    pub(crate) fn trusted() -> bool {
        // SAFETY: no arguments.
        unsafe { ffi::AXIsProcessTrusted() != 0 }
    }

    fn cg_info(&self, w: &cgwin::CgWin) -> wire::WindowInfo {
        let r = wire::Rect {
            x: round(w.bounds.origin.x),
            y: round(w.bounds.origin.y),
            width: round(w.bounds.size.width),
            height: round(w.bounds.size.height),
        };
        let id = w.number.to_string();
        wire::WindowInfo {
            version: ax::window_version(&id, &r),
            window_id: id,
            title: w.title.chars().take(200).collect(),
            bounds: Some(r),
            focused: false,
            modal: false,
            minimized: !w.on_screen,
        }
    }

    /// The application with windows from the Accessibility API when it is
    /// granted (modal and focus state are only known there), else from the
    /// window server.
    fn app_info(&self, ident: wire::ApplicationIdentity) -> wire::ApplicationInfo {
        let pid = ident.pid;
        let windows = if Self::trusted()
            && let Some(app) = ax::Ax::app(pid)
        {
            let w: Vec<_> = ax::windows(&app).into_iter().map(|w| w.info).collect();
            if w.is_empty() {
                self.cg_windows(pid)
            } else {
                w
            }
        } else {
            self.cg_windows(pid)
        };
        wire::ApplicationInfo {
            elevated: procinfo::is_elevated(pid),
            identity: Some(ident),
            windows,
        }
    }

    fn cg_windows(&self, pid: u32) -> Vec<wire::WindowInfo> {
        cgwin::list(false)
            .iter()
            .filter(|w| w.pid == pid && w.layer == 0 && w.bounds.size.width > 1.0)
            .take(20)
            .map(|w| self.cg_info(w))
            .collect()
    }

    pub(crate) fn verified(
        &self,
        id: &wire::ApplicationIdentity,
    ) -> Res<wire::ApplicationIdentity> {
        let Some(live) = procinfo::identity(id.pid) else {
            return Err(Fail::nd("WINDOW_UNVERIFIABLE", "no such process"));
        };
        if !procinfo::same(&live, id) {
            return Err(Fail::nd(
                "WINDOW_UNVERIFIABLE",
                "the application running as that process is not the one that was approved",
            ));
        }
        if procinfo::is_elevated(id.pid) {
            return Err(Fail::nd("TARGET_ELEVATED", "the application runs elevated"));
        }
        Ok(live)
    }
}

impl Default for Mac {
    fn default() -> Self {
        Self::new()
    }
}

#[allow(clippy::cast_possible_truncation)]
pub(crate) fn round(v: f64) -> i32 {
    v.round() as i32
}

impl Backend for Mac {
    fn name(&self) -> &'static str {
        "modbit-actuator-macos"
    }

    fn platform(&self) -> &'static str {
        "macos"
    }

    fn capabilities(&self) -> Vec<String> {
        // Only what is true: no on-screen Stop overlay is drawn by this
        // helper (the desktop's Stop control sends `Stop` over the Core).
        [
            "app_scope",
            "screen_scope",
            "screenshot",
            "secure_field_masking",
            "human_input_detection",
        ]
        .map(String::from)
        .to_vec()
    }

    fn permissions(&self) -> Permissions {
        Permissions {
            accessibility: Self::trusted(),
            // SAFETY: no arguments.
            screen_recording: unsafe { ffi::CGPreflightScreenCaptureAccess() != 0 },
        }
    }

    fn secure_desktop(&self) -> bool {
        cgwin::secure_desktop()
    }

    fn list_apps(&self) -> Res<Vec<wire::ApplicationInfo>> {
        let mut pids: Vec<u32> = Vec::new();
        for w in cgwin::list(true) {
            if w.layer == 0 && w.pid != self.own_pid && !pids.contains(&w.pid) {
                pids.push(w.pid);
            }
        }
        let cg = cgwin::list(false);
        Ok(pids
            .into_iter()
            .filter_map(|pid| {
                let ident = procinfo::identity(pid)?;
                let windows = cg
                    .iter()
                    .filter(|w| w.pid == pid && w.layer == 0 && w.bounds.size.width > 1.0)
                    .take(20)
                    .map(|w| self.cg_info(w))
                    .collect();
                Some(wire::ApplicationInfo {
                    elevated: procinfo::is_elevated(pid),
                    identity: Some(ident),
                    windows,
                })
            })
            .collect())
    }

    fn resolve(&self, bundle_id: &str, name: &str, pid: u32) -> Res<wire::ApplicationInfo> {
        let none = || Fail::nd("WINDOW_UNVERIFIABLE", "no such application");
        if pid != 0 {
            return procinfo::identity(pid)
                .map(|i| self.app_info(i))
                .ok_or_else(none);
        }
        let matches = |i: &wire::ApplicationIdentity| {
            if !bundle_id.is_empty() {
                i.bundle_id == bundle_id
            } else {
                !name.is_empty() && i.name.eq_ignore_ascii_case(name)
            }
        };
        if bundle_id.is_empty() && name.is_empty() {
            return Err(none());
        }
        // Applications with a window first, then every main executable of an
        // application bundle (menu-bar agents have no window).
        let mut seen = Vec::new();
        for w in cgwin::list(false) {
            if w.layer == 0 && w.pid != self.own_pid && !seen.contains(&w.pid) {
                seen.push(w.pid);
                if let Some(i) = procinfo::identity(w.pid)
                    && matches(&i)
                {
                    return Ok(self.app_info(i));
                }
            }
        }
        for p in procinfo::all_pids() {
            if seen.contains(&p) || p == self.own_pid {
                continue;
            }
            let Some(exe) = procinfo::exe_path(p) else {
                continue;
            };
            if !exe.to_string_lossy().contains(".app/Contents/MacOS/") {
                continue;
            }
            if let Some(i) = procinfo::identity(p)
                && matches(&i)
            {
                return Ok(self.app_info(i));
            }
        }
        Err(none())
    }

    fn verify(&self, id: &wire::ApplicationIdentity) -> Res<wire::ApplicationInfo> {
        let live = procinfo::identity(id.pid)
            .ok_or_else(|| Fail::nd("WINDOW_UNVERIFIABLE", "no such process"))?;
        if !procinfo::same(&live, id) {
            return Err(Fail::nd(
                "WINDOW_UNVERIFIABLE",
                "the application is not the one named",
            ));
        }
        Ok(self.app_info(live))
    }

    fn read_tree(&self, a: &ReadArgs<'_>, ctl: &Ctl) -> Res<wire::ReadStateResponse> {
        self.verified(a.app)?;
        let app = ax::Ax::app(a.app.pid)
            .ok_or_else(|| Fail::nd("WINDOW_UNVERIFIABLE", "no such process"))?;
        let mut win = ax::find_window(&app, a.window_id)?;
        let first_touch = self
            .warmed
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(a.app.pid);
        if first_touch {
            let _ = ax::walk(&win.el, a.max_nodes, ax::WALK_BUDGET, ctl)?;
            std::thread::sleep(Duration::from_millis(200));
            win = ax::find_window(&app, a.window_id)?;
        }
        let walked = ax::walk(&win.el, a.max_nodes, ax::WALK_BUDGET, ctl)?;
        let digest = ax::tree_digest(&walked.nodes);
        if !a.probe {
            let snap = Snapshot {
                id: a.snapshot_id.to_owned(),
                handles: walked.handles,
                nodes: walked.nodes.clone(),
                structure: ax::structure_digest(&walked.nodes),
            };
            self.snapshots
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .insert((a.app.pid, win.info.window_id.clone()), snap);
        }
        Ok(wire::ReadStateResponse {
            snapshot_id: a.snapshot_id.to_owned(),
            window: Some(win.info),
            nodes: walked.nodes,
            truncated: walked.truncated,
            tree_digest: digest,
        })
    }

    fn tree_digest(
        &self,
        app: &wire::ApplicationIdentity,
        window_id: &str,
        ctl: &Ctl,
    ) -> Res<(String, wire::WindowInfo)> {
        self.verified(app)?;
        let ax_app = ax::Ax::app(app.pid)
            .ok_or_else(|| Fail::nd("WINDOW_UNVERIFIABLE", "no such process"))?;
        let win = ax::find_window(&ax_app, window_id)?;
        let w = ax::walk(&win.el, 400, Duration::from_secs(2), ctl)?;
        Ok((ax::tree_digest(&w.nodes), win.info))
    }

    fn capture(&self, a: &CaptureArgs<'_>, ctl: &Ctl) -> Res<RawFrame> {
        capture::capture(self, a, ctl)
    }

    fn perform(&self, a: &PerformArgs<'_>, ctl: &Ctl) -> Res<wire::PerformResponse> {
        perform::perform(self, a, ctl)
    }

    fn human_idle(&self) -> Option<Duration> {
        // SAFETY: state 1 = HID system state; type ~0 = any input event.
        let s = unsafe { ffi::CGEventSourceSecondsSinceLastEventType(1, u32::MAX) };
        (s.is_finite() && s >= 0.0).then(|| Duration::from_secs_f64(s))
    }

    fn window_exists(&self, _pid: u32, window_id: &str) -> bool {
        match window_id.parse::<u32>() {
            Ok(n) => cgwin::list(false).iter().any(|w| w.number == n),
            Err(_) => true,
        }
    }
}
