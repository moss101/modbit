//! The seam between the portable actuator (handshake, lease, Stop, human
//! input, masking, canvas) and the operating system. The macOS backend
//! (`mac/`) is the production implementation; other platforms answer every
//! call `UNSUPPORTED_REQUEST` (docs/66 CUC-A03: macOS first, nothing else is
//! claimed until it passes its own packaged E2E).

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use modbit_protocol::v1 as wire;

pub use wire::Delivery;

/// A typed refusal: the Core's taxonomy code, a message for the person and
/// how far an input got (CUC-D01, CUC-D02).
#[derive(Clone, Debug, PartialEq)]
pub struct Fail {
    /// One of the codes of `ActuatorError`.
    pub code: &'static str,
    /// Human-readable cause; never contains typed text or a field value.
    pub message: String,
    /// Whether an input was injected.
    pub delivery: Delivery,
}

impl Fail {
    /// Nothing was injected.
    pub fn nd(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            delivery: Delivery::NotDelivered,
        }
    }

    /// Injection began and the outcome cannot be told.
    pub fn unknown(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            delivery: Delivery::Unknown,
        }
    }
}

impl From<Fail> for wire::ActuatorError {
    fn from(f: Fail) -> Self {
        Self {
            code: f.code.into(),
            message: f.message,
            delivery: f.delivery as i32,
        }
    }
}

/// A backend result.
pub type Res<T> = Result<T, Fail>;

/// Which operating-system permissions the helper holds right now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Permissions {
    /// Accessibility (AXIsProcessTrusted on macOS).
    pub accessibility: bool,
    /// Screen Recording (CGPreflightScreenCaptureAccess on macOS).
    pub screen_recording: bool,
}

/// Cancellation and deadline for one blocking backend call. A backend checks
/// [`Ctl::check`] between every unit of work (an element, a key event), so a
/// Stop or an expired deadline ends the call at the next unit - the 500 ms
/// bound of CUC-D05 is a bound on one unit, not on the whole input.
#[derive(Clone)]
pub struct Ctl {
    epoch: Arc<AtomicU64>,
    at_start: u64,
    deadline: Instant,
    own_hid: Arc<AtomicU64>,
    origin: Instant,
}

/// Why a call must end now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Halt {
    /// Stop, a lease release, a parked controller or a permission loss.
    Stopped,
    /// The request's deadline passed.
    Deadline,
}

impl Ctl {
    /// A control block for a call that starts now.
    #[must_use]
    pub fn new(
        epoch: Arc<AtomicU64>,
        own_hid: Arc<AtomicU64>,
        origin: Instant,
        deadline: Duration,
    ) -> Self {
        let at_start = epoch.load(Ordering::SeqCst);
        Self {
            epoch,
            at_start,
            deadline: Instant::now() + deadline,
            own_hid,
            origin,
        }
    }

    /// `Err` when the call must end now.
    ///
    /// # Errors
    /// [`Halt`] says why.
    pub fn check(&self) -> Result<(), Halt> {
        if self.epoch.load(Ordering::SeqCst) != self.at_start {
            return Err(Halt::Stopped);
        }
        if Instant::now() >= self.deadline {
            return Err(Halt::Deadline);
        }
        Ok(())
    }

    /// Time left before the deadline.
    #[must_use]
    pub fn remaining(&self) -> Duration {
        self.deadline.saturating_duration_since(Instant::now())
    }

    /// The backend is about to inject at the hardware-event level (SCREEN
    /// scope): events in the next `span` are the actuator's own, not a
    /// person's (CUC-D04: "excludes its own synthetic events").
    pub fn own_hid_for(&self, span: Duration) {
        let until = self.origin.elapsed() + span;
        self.own_hid.fetch_max(
            u64::try_from(until.as_millis()).unwrap_or(u64::MAX),
            Ordering::SeqCst,
        );
    }

    /// The typed refusal for a halt.
    #[must_use]
    pub fn halt_fail(h: Halt, delivered_any: bool) -> Fail {
        let (code, message) = match h {
            Halt::Stopped => ("USER_ABORTED", "stopped while the call was in flight"),
            Halt::Deadline => ("TIMEOUT", "the request's deadline passed"),
        };
        if delivered_any {
            Fail::unknown(code, message)
        } else {
            Fail::nd(code, message)
        }
    }
}

/// What a read of the tree asks.
pub struct ReadArgs<'a> {
    /// The identity the person approved.
    pub app: &'a wire::ApplicationIdentity,
    /// "" = the main window.
    pub window_id: &'a str,
    /// Minted by the Core.
    pub snapshot_id: &'a str,
    /// Node cap (already defaulted and bounded).
    pub max_nodes: usize,
    /// Do not replace the window's current snapshot.
    pub probe: bool,
}

/// What a capture asks.
pub struct CaptureArgs<'a> {
    /// The identity the person approved.
    pub app: &'a wire::ApplicationIdentity,
    /// "" = the main window.
    pub window_id: &'a str,
    /// APP = the window only; SCREEN = the display.
    pub scope: wire::Scope,
}

/// A raw frame before it is fitted to the canvas.
pub struct RawFrame {
    /// Straight RGBA, row-major.
    pub rgba: Vec<u8>,
    /// Width in pixels.
    pub px_w: u32,
    /// Height in pixels.
    pub px_h: u32,
    /// Width in display points (what the Core's letterbox is in).
    pub pt_w: u32,
    /// Height in display points.
    pub pt_h: u32,
    /// Every secure field in the frame, in display points relative to the
    /// frame's top-left. A frame whose secure fields could not all be found
    /// is never returned: the backend fails the capture instead.
    pub secure: Vec<crate::canvas::Rect>,
    /// The window as captured.
    pub window: wire::WindowInfo,
}

/// What a perform asks.
pub struct PerformArgs<'a> {
    /// The request.
    pub req: &'a wire::PerformRequest,
    /// The scope the lease was taken in.
    pub scope: wire::Scope,
}

/// The operating-system side of the actuator.
pub trait Backend: Send + Sync + 'static {
    /// Actuator name for the welcome.
    fn name(&self) -> &'static str;
    /// Platform for the welcome.
    fn platform(&self) -> &'static str;
    /// Capabilities for the welcome (only what is true).
    fn capabilities(&self) -> Vec<String>;
    /// The permissions held now.
    fn permissions(&self) -> Permissions;
    /// Whether the screen is locked or a secure prompt owns input.
    fn secure_desktop(&self) -> bool;
    /// Running applications that own a window.
    ///
    /// # Errors
    /// A typed refusal.
    fn list_apps(&self) -> Res<Vec<wire::ApplicationInfo>>;
    /// One application, without activating it.
    ///
    /// # Errors
    /// `WINDOW_UNVERIFIABLE` when there is no such application.
    fn resolve(&self, bundle_id: &str, name: &str, pid: u32) -> Res<wire::ApplicationInfo>;
    /// The application as it is NOW, checked against the identity the person
    /// approved (bundle id, executable path, signing identity, pid).
    ///
    /// # Errors
    /// `WINDOW_UNVERIFIABLE` when it is not the same application.
    fn verify(&self, id: &wire::ApplicationIdentity) -> Res<wire::ApplicationInfo>;
    /// Read the accessibility tree of a window.
    ///
    /// # Errors
    /// A typed refusal.
    fn read_tree(&self, a: &ReadArgs<'_>, ctl: &Ctl) -> Res<wire::ReadStateResponse>;
    /// The tree digest of a window, for `Wait`.
    ///
    /// # Errors
    /// A typed refusal.
    fn tree_digest(
        &self,
        app: &wire::ApplicationIdentity,
        window_id: &str,
        ctl: &Ctl,
    ) -> Res<(String, wire::WindowInfo)>;
    /// Grab a frame.
    ///
    /// # Errors
    /// A typed refusal.
    fn capture(&self, a: &CaptureArgs<'_>, ctl: &Ctl) -> Res<RawFrame>;
    /// Inject one input.
    ///
    /// # Errors
    /// A typed refusal that says how far the input got.
    fn perform(&self, a: &PerformArgs<'_>, ctl: &Ctl) -> Res<wire::PerformResponse>;
    /// Time since the last hardware keyboard or pointer event, if known.
    fn human_idle(&self) -> Option<Duration>;
    /// Whether a window still exists.
    fn window_exists(&self, pid: u32, window_id: &str) -> bool;
}
