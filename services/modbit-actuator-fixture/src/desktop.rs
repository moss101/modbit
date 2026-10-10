//! The fixture's in-memory desktop: applications, windows and elements, and
//! the few behaviours an element can have. Nothing here touches a display or
//! an operating-system permission.

use std::collections::HashMap;

use serde::Deserialize;
use sha2::{Digest, Sha256};

/// A rectangle in display points.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl Rect {
    pub fn contains(&self, x: f64, y: f64) -> bool {
        x >= f64::from(self.x)
            && y >= f64::from(self.y)
            && x < f64::from(self.x + self.width)
            && y < f64::from(self.y + self.height)
    }
}

/// What pressing, typing into or keying an element does, beyond its own state.
#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "op")]
pub enum Op {
    /// Set another element's value (`{n}` in `value` is the running count of this behaviour).
    SetValue { target: String, value: String },
    /// Append to another element's value.
    Append { target: String, text: String },
    /// Move the keyboard focus.
    Focus { target: String },
    /// Open a hidden window of the application (a dialog or sheet).
    OpenWindow { window: String },
    /// Close a window.
    CloseWindow { window: String },
    /// Replace the application's identity (a look-alike takes its place).
    SwapIdentity {
        bundle_id: String,
        signing_identity: String,
    },
    /// Retitle the window.
    RenameWindow { title: String },
    /// Move the window.
    MoveWindow { x: i32, y: i32 },
    /// Remove an element.
    Remove { target: String },
    /// A physical key was pressed (the person is using the machine).
    EmitHumanInput,
    /// Die after the input was applied and before any answer: the real
    /// "delivered, outcome unknown" crash.
    AbortAfterDeliver,
    /// Apply the input, then stall the answer.
    HangMs { ms: u64 },
    /// Show or hide the secure desktop.
    SecureDesktop { on: bool },
    /// Refuse the input before anything is injected, with this code.
    FailInput { code: String },
}

/// One element.
#[derive(Clone, Debug, Deserialize)]
pub struct El {
    pub id: String,
    #[serde(default)]
    pub parent: Option<String>,
    pub role: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub value: String,
    #[serde(default)]
    pub settable: bool,
    #[serde(default)]
    pub secure: bool,
    #[serde(default = "yes")]
    pub enabled: bool,
    #[serde(default)]
    pub focused: bool,
    #[serde(default)]
    pub actions: Vec<String>,
    #[serde(default)]
    pub bounds: Rect,
    /// Behaviours by action name (`press`, `enter`, `key:cmd+a`, ...).
    #[serde(default)]
    pub behaviors: HashMap<String, Vec<Op>>,
    /// Render as photographic noise.
    #[serde(default)]
    pub photo: bool,
}

fn yes() -> bool {
    true
}

/// A window.
#[derive(Clone, Debug, Deserialize)]
pub struct Win {
    pub id: String,
    pub title: String,
    pub bounds: Rect,
    #[serde(default)]
    pub modal: bool,
    #[serde(default)]
    pub minimized: bool,
    #[serde(default = "yes")]
    pub focused: bool,
    /// Not on screen until a behaviour opens it.
    #[serde(default)]
    pub hidden: bool,
    #[serde(default)]
    pub elements: Vec<El>,
    #[serde(skip)]
    pub version: u64,
    /// The snapshot id of the last non-probe tree read.
    #[serde(skip)]
    pub snapshot: String,
}

/// An application.
#[derive(Clone, Debug, Deserialize)]
pub struct App {
    pub bundle_id: String,
    pub name: String,
    #[serde(default)]
    pub executable_path: String,
    #[serde(default)]
    pub signing_identity: String,
    pub pid: u32,
    #[serde(default)]
    pub elevated: bool,
    /// The application exposes no accessibility tree.
    #[serde(default)]
    pub no_accessibility: bool,
    pub windows: Vec<Win>,
}

/// Faults the whole desktop can have.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct Faults {
    /// Do not mask secure fields in frames (the Core must).
    #[serde(default)]
    pub skip_mask: bool,
    /// Do not report secure regions at all.
    #[serde(default)]
    pub hide_secure_regions: bool,
    /// Report a letterbox that is not the canvas scaler's.
    #[serde(default)]
    pub bad_letterbox: bool,
    /// Return frames that are not 1280 x 800.
    #[serde(default)]
    pub wrong_size: bool,
    /// Read state slowly.
    #[serde(default)]
    pub slow_read_ms: u64,
    /// The secure desktop is up from the start.
    #[serde(default)]
    pub secure_desktop: bool,
    /// The operating system has not granted the actuator its permission.
    #[serde(default)]
    pub permission_missing: bool,
}

/// The whole desktop.
#[derive(Clone, Debug, Deserialize)]
pub struct Desktop {
    #[serde(default = "default_display")]
    pub display: Rect,
    pub apps: Vec<App>,
    #[serde(default)]
    pub faults: Faults,
}

fn default_display() -> Rect {
    Rect {
        x: 0,
        y: 0,
        width: 1920,
        height: 1200,
    }
}

impl Desktop {
    pub fn app_mut_by_pid(&mut self, pid: u32) -> Option<&mut App> {
        self.apps.iter_mut().find(|a| a.pid == pid)
    }

    pub fn app_by_pid(&self, pid: u32) -> Option<&App> {
        self.apps.iter().find(|a| a.pid == pid)
    }
}

impl Win {
    /// The nodes of the tree in order: the window, then its elements. An
    /// element's id within a read is `n<index>`.
    pub fn nodes(&self) -> Vec<(String, &El)> {
        self.elements
            .iter()
            .enumerate()
            .map(|(i, e)| (format!("n{}", i + 1), e))
            .collect()
    }

    pub fn node_id_of(&self, logical: &str) -> Option<String> {
        self.nodes()
            .into_iter()
            .find(|(_, e)| e.id == logical)
            .map(|(id, _)| id)
    }

    pub fn index_of_node(&self, node_id: &str) -> Option<usize> {
        let n: usize = node_id.strip_prefix('n')?.parse().ok()?;
        (n >= 1 && n <= self.elements.len()).then(|| n - 1)
    }

    /// A digest of the structure and the non-secret content, for change detection.
    pub fn digest(&self) -> String {
        let mut h = Sha256::new();
        h.update(self.title.as_bytes());
        h.update(self.bounds.x.to_be_bytes());
        h.update(self.bounds.y.to_be_bytes());
        for e in &self.elements {
            h.update(e.id.as_bytes());
            h.update(e.role.as_bytes());
            h.update(e.name.as_bytes());
            if !e.secure {
                h.update(e.value.as_bytes());
            }
            h.update([u8::from(e.enabled), u8::from(e.focused)]);
        }
        hex::encode(h.finalize())
    }
}

/// The default desktop the fixture runs with when none is given: a form
/// application, a document application with a photographic pane, a login
/// application with a secure field, a credential store (never drivable) and
/// a look-alike.
pub fn default_desktop() -> Desktop {
    serde_json::from_str(DEFAULT_DESKTOP).expect("the built-in desktop parses")
}

pub const DEFAULT_DESKTOP: &str = include_str!("default_desktop.json");
