//! The runtime's own types for what an actuator reports, and their
//! conversions from and to the wire messages of `computer.proto`. The wire
//! types stay at the edge: everything inside the runtime, the journal and the
//! tools works on these.

use modbit_protocol::v1 as wire;
use serde::{Deserialize, Serialize};

/// The identity of an application as the operating system vouches for it
/// (CUC-C01). Never the window title.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AppIdentity {
    /// Bundle identifier (`com.apple.TextEdit`).
    pub bundle_id: String,
    /// Executable path.
    pub executable_path: String,
    /// Code-signing identity; empty = unsigned.
    pub signing_identity: String,
    /// Process id.
    pub pid: u32,
    /// Display name (an untrusted label).
    pub name: String,
}

impl AppIdentity {
    /// Whether `other` is the same application: bundle, executable, signer
    /// and process, all four. A look-alike with the same name or title is not.
    #[must_use]
    pub fn same_as(&self, other: &AppIdentity) -> bool {
        self.bundle_id == other.bundle_id
            && self.executable_path == other.executable_path
            && self.signing_identity == other.signing_identity
            && self.pid == other.pid
    }

    /// The identity facts an approval binds (display name excluded).
    #[must_use]
    pub fn binding(&self) -> serde_json::Value {
        serde_json::json!({
            "bundle_id": self.bundle_id,
            "executable_path": self.executable_path,
            "signing_identity": self.signing_identity,
            "pid": self.pid,
        })
    }
}

/// A rectangle in display points (or canvas pixels, where said).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Rect {
    /// Left.
    pub x: i32,
    /// Top.
    pub y: i32,
    /// Width.
    pub width: i32,
    /// Height.
    pub height: i32,
}

impl Rect {
    /// Whether the rectangle has area.
    #[must_use]
    pub const fn has_area(&self) -> bool {
        self.width > 0 && self.height > 0
    }
}

/// A window of an application.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Window {
    /// Opaque id, stable for the window's life.
    pub id: String,
    /// Title (an untrusted label).
    pub title: String,
    /// Bounds in display points.
    pub bounds: Rect,
    /// Focused.
    pub focused: bool,
    /// A modal sheet or dialog is in front of it.
    pub modal: bool,
    /// Minimized.
    pub minimized: bool,
    /// Changes whenever the window moves, resizes or is replaced.
    pub version: u64,
}

/// A running application and its windows.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppInfo {
    /// Identity.
    pub identity: AppIdentity,
    /// Windows.
    pub windows: Vec<Window>,
    /// Runs with more privilege than the actuator.
    pub elevated: bool,
}

/// One element of an accessibility tree.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Node {
    /// Element id (valid only with the snapshot of its tree).
    pub id: String,
    /// Parent element; empty = a root.
    pub parent: String,
    /// Role.
    pub role: String,
    /// Name (an untrusted label).
    pub name: String,
    /// Value (untrusted; empty for a secure field).
    pub value: String,
    /// Takes a value.
    pub settable: bool,
    /// A secure or password input.
    pub secure: bool,
    /// Enabled.
    pub enabled: bool,
    /// Holds the keyboard focus.
    pub focused: bool,
    /// Element actions.
    pub actions: Vec<String>,
    /// Bounds in display points; zero = unknown.
    pub bounds: Rect,
}

/// The scope of a control session (CUC-A02).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Scope {
    /// One application, in the background, through its accessibility tree.
    App,
    /// The display, the real cursor, a visible Stop control.
    Screen,
}

impl Scope {
    /// The stable name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Scope::App => "APP",
            Scope::Screen => "SCREEN",
        }
    }

    pub(crate) const fn to_wire(self) -> wire::Scope {
        match self {
            Scope::App => wire::Scope::App,
            Scope::Screen => wire::Scope::Screen,
        }
    }
}

pub(crate) fn identity_from_wire(i: &wire::ApplicationIdentity) -> AppIdentity {
    AppIdentity {
        bundle_id: i.bundle_id.clone(),
        executable_path: i.executable_path.clone(),
        signing_identity: i.signing_identity.clone(),
        pid: i.pid,
        name: i.name.clone(),
    }
}

pub(crate) fn identity_to_wire(i: &AppIdentity) -> wire::ApplicationIdentity {
    wire::ApplicationIdentity {
        bundle_id: i.bundle_id.clone(),
        executable_path: i.executable_path.clone(),
        signing_identity: i.signing_identity.clone(),
        pid: i.pid,
        name: i.name.clone(),
    }
}

pub(crate) fn rect_from_wire(r: Option<&wire::Rect>) -> Rect {
    r.map_or_else(Rect::default, |r| Rect {
        x: r.x,
        y: r.y,
        width: r.width,
        height: r.height,
    })
}

pub(crate) fn window_from_wire(w: &wire::WindowInfo) -> Window {
    Window {
        id: w.window_id.clone(),
        title: w.title.clone(),
        bounds: rect_from_wire(w.bounds.as_ref()),
        focused: w.focused,
        modal: w.modal,
        minimized: w.minimized,
        version: w.version,
    }
}

pub(crate) fn app_from_wire(a: &wire::ApplicationInfo) -> AppInfo {
    AppInfo {
        identity: a
            .identity
            .as_ref()
            .map(identity_from_wire)
            .unwrap_or_default(),
        windows: a.windows.iter().map(window_from_wire).collect(),
        elevated: a.elevated,
    }
}

pub(crate) fn node_from_wire(n: &wire::AxNode) -> Node {
    Node {
        id: n.element_id.clone(),
        parent: n.parent_id.clone(),
        role: n.role.clone(),
        name: n.name.clone(),
        value: n.value.clone(),
        settable: n.settable,
        secure: n.secure,
        enabled: n.enabled,
        focused: n.focused,
        actions: n.actions.clone(),
        bounds: rect_from_wire(n.bounds.as_ref()),
    }
}
