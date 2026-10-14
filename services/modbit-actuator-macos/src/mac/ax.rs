//! The Accessibility API: windows, bounded tree walks, element handles.
//!
//! Every walk is bounded in depth, node count and time, and says so when it
//! was cut (`truncated`): a partial tree is never presented as the whole one
//! (V02). A secure text field's VALUE is never requested - not read, not
//! hashed, not logged - so it cannot reach the model, the Core or a frame
//! (CUC-B06, CUC-F01).
#![allow(unsafe_code)]

use std::collections::VecDeque;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use modbit_protocol::v1 as wire;
use sha2::{Digest, Sha256};

use super::cf::{Cf, CfStr, out};
use super::ffi::{
    AXError, AXUIElementCopyActionNames, AXUIElementCopyAttributeValue,
    AXUIElementCopyElementAtPosition, AXUIElementCreateApplication, AXUIElementIsAttributeSettable,
    AXUIElementPerformAction, AXUIElementSetAttributeValue, AXUIElementSetMessagingTimeout,
    AXValueGetValue, AxGetWindowFn, CGPoint, CGSize, RTLD_DEFAULT, dlsym, kAXErrorAPIDisabled,
    kAXErrorActionUnsupported, kAXErrorAttributeUnsupported, kAXErrorCannotComplete,
    kAXErrorFailure, kAXErrorIllegalArgument, kAXErrorInvalidUIElement, kAXErrorNoValue,
    kAXErrorNotImplemented, kAXValueCGPointType, kAXValueCGSizeType, kCFBooleanFalse,
    kCFBooleanTrue,
};
use crate::backend::{Ctl, Fail, Res};

/// Deepest nesting a walk follows.
pub const MAX_DEPTH: usize = 40;
/// How long one walk may run.
pub const WALK_BUDGET: Duration = Duration::from_secs(3);
/// How long one AX call to an unresponsive application may block.
const MESSAGING_TIMEOUT: f32 = 1.5;
const MAX_NAME: usize = 200;
const MAX_VALUE: usize = 2000;

/// The attribute names the helper uses, built once.
pub struct Names {
    pub role: CfStr,
    pub subrole: CfStr,
    pub title: CfStr,
    pub desc: CfStr,
    pub value: CfStr,
    pub children: CfStr,
    pub position: CfStr,
    pub size: CfStr,
    pub enabled: CfStr,
    pub focused: CfStr,
    pub windows: CfStr,
    pub main: CfStr,
    pub minimized: CfStr,
    pub modal: CfStr,
    pub focused_element: CfStr,
    pub selected_text: CfStr,
    pub manual_ax: CfStr,
    pub window: CfStr,
}

/// The shared names.
pub fn names() -> &'static Names {
    static N: OnceLock<Names> = OnceLock::new();
    N.get_or_init(|| Names {
        role: CfStr::new("AXRole"),
        subrole: CfStr::new("AXSubrole"),
        title: CfStr::new("AXTitle"),
        desc: CfStr::new("AXDescription"),
        value: CfStr::new("AXValue"),
        children: CfStr::new("AXChildren"),
        position: CfStr::new("AXPosition"),
        size: CfStr::new("AXSize"),
        enabled: CfStr::new("AXEnabled"),
        focused: CfStr::new("AXFocused"),
        windows: CfStr::new("AXWindows"),
        main: CfStr::new("AXMain"),
        minimized: CfStr::new("AXMinimized"),
        modal: CfStr::new("AXModal"),
        focused_element: CfStr::new("AXFocusedUIElement"),
        selected_text: CfStr::new("AXSelectedText"),
        manual_ax: CfStr::new("AXManualAccessibility"),
        window: CfStr::new("AXWindow"),
    })
}

/// An accessibility element, retained.
pub struct Ax(pub Cf);

impl Clone for Ax {
    fn clone(&self) -> Self {
        Self(Cf::retain(self.0.raw()).expect("a live element"))
    }
}

impl Ax {
    /// The application element of `pid`, with a bounded messaging timeout.
    #[must_use]
    pub fn app(pid: u32) -> Option<Self> {
        let pid = i32::try_from(pid).ok()?;
        // SAFETY: "create" rule.
        let e = Cf::own(unsafe { AXUIElementCreateApplication(pid) })?;
        // SAFETY: `e` is live.
        unsafe { AXUIElementSetMessagingTimeout(e.raw(), MESSAGING_TIMEOUT) };
        Some(Self(e))
    }

    /// Ask a Chromium/Electron application to build its accessibility tree.
    pub fn enable_manual_accessibility(&self) {
        // SAFETY: both are live; the result is ignored (most apps do not have it).
        unsafe {
            AXUIElementSetAttributeValue(self.0.raw(), names().manual_ax.raw(), kCFBooleanTrue);
        }
    }

    /// The attribute, or the AX error.
    ///
    /// # Errors
    /// The raw `AXError`.
    pub fn attr(&self, name: &CfStr) -> Result<Cf, AXError> {
        let mut v = std::ptr::null();
        // SAFETY: `v` is a local out value; "copy" rule.
        let e = unsafe { AXUIElementCopyAttributeValue(self.0.raw(), name.raw(), &raw mut v) };
        if e != 0 {
            return Err(e);
        }
        Cf::own(v).ok_or(kAXErrorNoValue)
    }

    /// A string attribute.
    #[must_use]
    pub fn string(&self, name: &CfStr) -> Option<String> {
        self.attr(name).ok().and_then(|v| v.as_string())
    }

    /// A boolean attribute.
    #[must_use]
    pub fn boolean(&self, name: &CfStr) -> Option<bool> {
        self.attr(name).ok().and_then(|v| v.as_bool())
    }

    /// Elements in an array attribute.
    #[must_use]
    pub fn elements(&self, name: &CfStr) -> Option<Vec<Ax>> {
        let a = self.attr(name).ok()?;
        Some(a.as_array()?.into_iter().map(Ax).collect())
    }

    /// An element-valued attribute.
    #[must_use]
    pub fn element(&self, name: &CfStr) -> Option<Ax> {
        self.attr(name).ok().map(Ax)
    }

    /// Position in display points.
    #[must_use]
    pub fn point(&self) -> Option<CGPoint> {
        let v = self.attr(&names().position).ok()?;
        let mut p = CGPoint::default();
        // SAFETY: `v` is an AXValue of point type if the call succeeds.
        (unsafe { AXValueGetValue(v.raw(), kAXValueCGPointType, out(&mut p)) } != 0).then_some(p)
    }

    /// Size in display points.
    #[must_use]
    pub fn size(&self) -> Option<CGSize> {
        let v = self.attr(&names().size).ok()?;
        let mut s = CGSize::default();
        // SAFETY: as above.
        (unsafe { AXValueGetValue(v.raw(), kAXValueCGSizeType, out(&mut s)) } != 0).then_some(s)
    }

    /// Whether `name` can be set on this element.
    #[must_use]
    pub fn settable(&self, name: &CfStr) -> bool {
        let mut b = 0u8;
        // SAFETY: `b` is a local out value.
        let e = unsafe { AXUIElementIsAttributeSettable(self.0.raw(), name.raw(), &raw mut b) };
        e == 0 && b != 0
    }

    /// Set a string attribute.
    ///
    /// # Errors
    /// The raw `AXError`.
    pub fn set_string(&self, name: &CfStr, v: &str) -> Result<(), AXError> {
        let s = CfStr::new(v);
        // SAFETY: all live.
        let e = unsafe { AXUIElementSetAttributeValue(self.0.raw(), name.raw(), s.raw()) };
        if e == 0 { Ok(()) } else { Err(e) }
    }

    /// Set a boolean attribute.
    ///
    /// # Errors
    /// The raw `AXError`.
    pub fn set_bool(&self, name: &CfStr, v: bool) -> Result<(), AXError> {
        // SAFETY: the CFBoolean constants are immortal.
        let e = unsafe {
            AXUIElementSetAttributeValue(
                self.0.raw(),
                name.raw(),
                if v { kCFBooleanTrue } else { kCFBooleanFalse },
            )
        };
        if e == 0 { Ok(()) } else { Err(e) }
    }

    /// The AX action names this element offers.
    #[must_use]
    pub fn actions(&self) -> Vec<String> {
        let mut a = std::ptr::null();
        // SAFETY: local out value; "copy" rule.
        let e = unsafe { AXUIElementCopyActionNames(self.0.raw(), &raw mut a) };
        if e != 0 {
            return vec![];
        }
        Cf::own(a)
            .and_then(|a| a.as_array())
            .map(|v| v.iter().filter_map(Cf::as_string).collect())
            .unwrap_or_default()
    }

    /// Perform an AX action.
    ///
    /// # Errors
    /// The raw `AXError`.
    pub fn perform(&self, action: &str) -> Result<(), AXError> {
        let a = CfStr::new(action);
        // SAFETY: all live.
        let e = unsafe { AXUIElementPerformAction(self.0.raw(), a.raw()) };
        if e == 0 { Ok(()) } else { Err(e) }
    }

    /// The topmost element of this application at a global point.
    #[must_use]
    pub fn element_at(&self, x: f64, y: f64) -> Option<Ax> {
        let mut e = std::ptr::null();
        // SAFETY: local out value; "copy" rule. Narrowing to f32 is the API's.
        #[allow(clippy::cast_possible_truncation)]
        let r = unsafe {
            AXUIElementCopyElementAtPosition(self.0.raw(), x as f32, y as f32, &raw mut e)
        };
        if r != 0 { None } else { Cf::own(e).map(Ax) }
    }

    /// The `CGWindowID` of a window element, if the (private, stable) symbol exists.
    #[must_use]
    pub fn window_number(&self) -> Option<u32> {
        static F: OnceLock<Option<AxGetWindowFn>> = OnceLock::new();
        let f = (*F.get_or_init(|| {
            let name = c"_AXUIElementGetWindow";
            // SAFETY: dlsym of a C symbol name; the transmute target is its documented shape.
            let p = unsafe { dlsym(RTLD_DEFAULT, name.as_ptr()) };
            (!p.is_null()).then(|| unsafe { std::mem::transmute::<*mut _, AxGetWindowFn>(p) })
        }))?;
        let mut n = 0u32;
        // SAFETY: `n` is a local out value.
        let e = unsafe { f(self.0.raw(), &raw mut n) };
        (e == 0 && n != 0).then_some(n)
    }
}

#[allow(non_upper_case_globals)]
/// The typed refusal for an AX error. `after_start`: an input call was made
/// (so a failure that is not provably harmless leaves the outcome unknown).
#[must_use]
pub fn ax_fail(e: AXError, what: &str, after_start: bool) -> Fail {
    match e {
        kAXErrorAPIDisabled => Fail::nd(
            "PERMISSION_REQUIRED",
            "Accessibility is not granted to the actuator",
        ),
        kAXErrorInvalidUIElement => Fail::nd("TARGET_STALE", format!("{what}: the element is gone")),
        kAXErrorActionUnsupported | kAXErrorAttributeUnsupported | kAXErrorNotImplemented => {
            Fail::nd(
                "UNSUPPORTED_REQUEST",
                format!("{what}: the element does not support it"),
            )
        }
        kAXErrorIllegalArgument => Fail::nd("UNSUPPORTED_REQUEST", format!("{what}: refused the argument")),
        kAXErrorCannotComplete | kAXErrorFailure | kAXErrorNoValue => {
            if after_start {
                Fail::unknown(
                    "INPUT_FAILED",
                    format!("{what}: the application did not answer (AX error {e}); the effect is unknown"),
                )
            } else {
                Fail::nd(
                    "ACCESSIBILITY_UNAVAILABLE",
                    format!("{what}: the application did not answer (AX error {e})"),
                )
            }
        }
        _ => {
            if after_start {
                Fail::unknown("INPUT_FAILED", format!("{what}: AX error {e}"))
            } else {
                Fail::nd("ACCESSIBILITY_UNAVAILABLE", format!("{what}: AX error {e}"))
            }
        }
    }
}

fn fnv(parts: &[&[u8]]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for p in parts {
        for b in *p {
            h ^= u64::from(*b);
            h = h.wrapping_mul(0x100_0000_01b3);
        }
        h ^= 0xff;
        h = h.wrapping_mul(0x100_0000_01b3);
    }
    h
}

/// The version of a window: changes when it moves, resizes or is replaced.
#[must_use]
pub fn window_version(id: &str, r: &wire::Rect) -> u64 {
    fnv(&[
        id.as_bytes(),
        &r.x.to_le_bytes(),
        &r.y.to_le_bytes(),
        &r.width.to_le_bytes(),
        &r.height.to_le_bytes(),
    ])
}

#[allow(clippy::cast_possible_truncation)]
fn rect_of(p: Option<CGPoint>, s: Option<CGSize>) -> wire::Rect {
    match (p, s) {
        (Some(p), Some(s)) => wire::Rect {
            x: p.x.round() as i32,
            y: p.y.round() as i32,
            width: s.width.round() as i32,
            height: s.height.round() as i32,
        },
        _ => wire::Rect::default(),
    }
}

/// One window of an application.
pub struct Window {
    /// The AX element.
    pub el: Ax,
    /// The wire view.
    pub info: wire::WindowInfo,
}

/// The windows of an application, with `modal` computed (another window is
/// modal, or this one carries a sheet).
#[must_use]
pub fn windows(app: &Ax) -> Vec<Window> {
    let n = names();
    let Some(els) = app.elements(&n.windows) else {
        return vec![];
    };
    let mut raw: Vec<(Ax, wire::WindowInfo, bool, bool)> = Vec::new();
    for (i, el) in els.into_iter().enumerate() {
        // Only real window elements. When the session denies window access
        // (a locked screen) the API answers with the application element for
        // every "window"; walking that is walking a loop, not a window.
        if !matches!(
            el.string(&n.role).as_deref(),
            Some("AXWindow" | "AXSheet" | "AXDialog" | "AXFloatingWindow" | "AXSystemDialog")
        ) {
            continue;
        }
        let bounds = rect_of(el.point(), el.size());
        let id = el
            .window_number()
            .map_or_else(|| format!("ax{i}"), |n| n.to_string());
        let sheet = el
            .elements(&n.children)
            .is_some_and(|c| c.iter().any(|c| c.string(&n.role).as_deref() == Some("AXSheet")));
        let modal = el.boolean(&n.modal).unwrap_or(false);
        let info = wire::WindowInfo {
            version: window_version(&id, &bounds),
            window_id: id,
            title: el.string(&n.title).unwrap_or_default().chars().take(MAX_NAME).collect(),
            bounds: Some(bounds),
            focused: el.boolean(&n.main).unwrap_or(false) || el.boolean(&n.focused).unwrap_or(false),
            modal: false,
            minimized: el.boolean(&n.minimized).unwrap_or(false),
        };
        raw.push((el, info, modal, sheet));
    }
    let modal_ids: Vec<String> = raw
        .iter()
        .filter(|(_, _, m, _)| *m)
        .map(|(_, i, _, _)| i.window_id.clone())
        .collect();
    raw.into_iter()
        .map(|(el, mut info, _, sheet)| {
            info.modal = sheet || modal_ids.iter().any(|m| *m != info.window_id);
            Window { el, info }
        })
        .collect()
}

/// The window `id` ("" = the main, else the first not minimised).
///
/// # Errors
/// `WINDOW_UNVERIFIABLE` when there is none; `ACCESSIBILITY_UNAVAILABLE` when
/// the application exposes no windows at all.
pub fn find_window(app: &Ax, id: &str) -> Res<Window> {
    app.enable_manual_accessibility();
    let all = windows(app);
    if all.is_empty() {
        return Err(Fail::nd(
            "ACCESSIBILITY_UNAVAILABLE",
            "the application exposes no accessibility windows",
        ));
    }
    let pick = if id.is_empty() {
        let main = all.iter().position(|w| w.info.focused && !w.info.minimized);
        main.or_else(|| all.iter().position(|w| !w.info.minimized))
    } else {
        all.iter().position(|w| w.info.window_id == id)
    };
    match pick {
        Some(i) => Ok(all.into_iter().nth(i).expect("index in range")),
        None => Err(Fail::nd("WINDOW_UNVERIFIABLE", "no such window")),
    }
}

/// The wire role of an AX role/subrole, and whether it is a secure field.
#[must_use]
pub fn role_of(role: &str, subrole: &str) -> (String, bool) {
    if subrole == "AXSecureTextField" || role == "AXSecureTextField" {
        return ("securetextfield".into(), true);
    }
    let r = match role {
        "AXTextField" | "AXComboBox" | "AXSearchField" => "textfield",
        "AXTextArea" => "textarea",
        "AXStaticText" => "label",
        "AXPopUpButton" => "popup",
        "AXList" | "AXTable" | "AXOutline" => "list",
        other => {
            let s = other.strip_prefix("AX").unwrap_or(other);
            return (s.to_ascii_lowercase(), false);
        }
    };
    (r.into(), false)
}

fn action_name(ax: &str) -> Option<&'static str> {
    Some(match ax {
        "AXPress" => "press",
        "AXIncrement" => "increment",
        "AXDecrement" => "decrement",
        "AXShowMenu" => "show_menu",
        "AXConfirm" => "confirm",
        "AXCancel" => "cancel",
        "AXPick" => "pick",
        "AXRaise" => "raise",
        _ => return None,
    })
}

/// The AX action behind a wire action name.
#[must_use]
pub fn ax_action(name: &str) -> Option<&'static str> {
    Some(match name {
        "press" => "AXPress",
        "increment" => "AXIncrement",
        "decrement" => "AXDecrement",
        "show_menu" => "AXShowMenu",
        "confirm" => "AXConfirm",
        "cancel" => "AXCancel",
        "pick" => "AXPick",
        "raise" => "AXRaise",
        _ => return None,
    })
}

/// A walked tree: wire nodes, the handles behind them, and whether it was cut.
pub struct Walked {
    /// `n0` is the window.
    pub nodes: Vec<wire::AxNode>,
    /// `handles[i]` is the element of `nodes[i]`.
    pub handles: Vec<Ax>,
    /// A limit (nodes, depth, time) cut the walk.
    pub truncated: bool,
}

fn clip(s: String, max: usize) -> String {
    if s.chars().count() <= max {
        s
    } else {
        s.chars().take(max).collect()
    }
}

fn value_text(v: &Cf) -> String {
    if let Some(s) = v.as_string() {
        return s;
    }
    if let Some(b) = v.as_bool() {
        return if b { "true".into() } else { "false".into() };
    }
    if let Some(i) = v.as_i64() {
        return i.to_string();
    }
    if let Some(f) = v.as_f64() {
        return f.to_string();
    }
    String::new()
}

fn node_of(el: &Ax, index: usize, parent: Option<usize>) -> wire::AxNode {
    let n = names();
    let role_raw = el.string(&n.role).unwrap_or_default();
    let sub = el.string(&n.subrole).unwrap_or_default();
    let (role, secure) = role_of(&role_raw, &sub);
    let mut name = el
        .string(&n.title)
        .filter(|s| !s.is_empty())
        .or_else(|| el.string(&n.desc).filter(|s| !s.is_empty()))
        .unwrap_or_default();
    let mut value = String::new();
    // A secure field's value is never requested.
    if !secure && let Ok(v) = el.attr(&n.value) {
        value = value_text(&v);
    }
    if name.is_empty() && role == "label" {
        name = std::mem::take(&mut value);
    }
    let settable = el.settable(&n.value);
    let mut actions: Vec<String> = el
        .actions()
        .iter()
        .filter_map(|a| action_name(a).map(str::to_owned))
        .collect();
    if matches!(role.as_str(), "textfield" | "textarea" | "securetextfield") {
        actions.push("focus".into());
    }
    wire::AxNode {
        element_id: format!("n{index}"),
        parent_id: parent.map_or_else(String::new, |p| format!("n{p}")),
        role,
        name: clip(name, MAX_NAME),
        value: clip(value, MAX_VALUE),
        settable,
        secure,
        enabled: el.boolean(&n.enabled).unwrap_or(true),
        focused: el.boolean(&n.focused).unwrap_or(false),
        actions,
        bounds: Some(rect_of(el.point(), el.size())),
    }
}

/// Walk `window` breadth-first, bounded by `max` nodes, [`MAX_DEPTH`] and
/// `budget`.
///
/// # Errors
/// A Stop or a deadline from `ctl`.
pub fn walk(window: &Ax, max: usize, budget: Duration, ctl: &Ctl) -> Res<Walked> {
    let n = names();
    let started = Instant::now();
    let mut nodes = vec![node_of(window, 0, None)];
    let mut handles = vec![Ax(Cf::retain(window.0.raw()).expect("live window"))];
    let mut queue: VecDeque<(usize, usize)> = VecDeque::from([(0usize, 0usize)]);
    let mut truncated = false;
    // Elements already in the tree, so a loop in a misbehaving application's
    // tree (an element that lists an ancestor as a child) cannot run the walk
    // to its caps and cannot present one element twice.
    let mut seen: std::collections::HashMap<usize, Vec<usize>> = std::collections::HashMap::new();
    seen.entry(window.0.hash_code()).or_default().push(0);
    'outer: while let Some((idx, depth)) = queue.pop_front() {
        if let Err(h) = ctl.check() {
            return Err(Ctl::halt_fail(h, false));
        }
        if depth >= MAX_DEPTH {
            truncated = true;
            continue;
        }
        let Some(kids) = handles[idx].elements(&n.children) else {
            continue;
        };
        for k in kids {
            if nodes.len() >= max || started.elapsed() > budget {
                truncated = true;
                break 'outer;
            }
            let bucket = seen.entry(k.0.hash_code()).or_default();
            if bucket.iter().any(|&j| handles[j].0.same_as(&k.0)) {
                continue;
            }
            let i = nodes.len();
            bucket.push(i);
            nodes.push(node_of(&k, i, Some(idx)));
            handles.push(k);
            queue.push_back((i, depth + 1));
        }
    }
    Ok(Walked {
        nodes,
        handles,
        truncated,
    })
}

/// sha256 over the content of a tree: roles, names, values, flags and bounds.
/// A secure field contributes no value (it has none to give).
#[must_use]
pub fn tree_digest(nodes: &[wire::AxNode]) -> String {
    let mut h = Sha256::new();
    for n in nodes {
        let b = n.bounds.clone().unwrap_or_default();
        h.update(
            format!(
                "{}|{}|{}|{}|{}|{}|{}|{}|{},{},{},{}\n",
                n.element_id,
                n.parent_id,
                n.role,
                n.name,
                n.value,
                n.enabled,
                n.focused,
                n.secure,
                b.x,
                b.y,
                b.width,
                b.height
            )
            .as_bytes(),
        );
    }
    hex::encode(h.finalize())
}

/// sha256 over the shape of a tree (parents and roles only): a sheet opening
/// or a tree being replaced changes it, a typed character does not.
#[must_use]
pub fn structure_digest(nodes: &[wire::AxNode]) -> String {
    let mut h = Sha256::new();
    for n in nodes {
        h.update(format!("{}|{}|{}\n", n.element_id, n.parent_id, n.role).as_bytes());
    }
    hex::encode(h.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roles_map_and_a_secure_field_is_recognised_by_subrole_or_role() {
        assert_eq!(role_of("AXButton", ""), ("button".into(), false));
        assert_eq!(role_of("AXTextField", ""), ("textfield".into(), false));
        assert_eq!(role_of("AXTextField", "AXSecureTextField"), ("securetextfield".into(), true));
        assert_eq!(role_of("AXSecureTextField", ""), ("securetextfield".into(), true));
        assert_eq!(role_of("AXStaticText", ""), ("label".into(), false));
        assert_eq!(role_of("AXCheckBox", ""), ("checkbox".into(), false));
    }

    #[test]
    fn a_window_version_changes_with_geometry_and_identity_only() {
        let r = wire::Rect { x: 1, y: 2, width: 300, height: 200 };
        let v = window_version("7", &r);
        assert_eq!(v, window_version("7", &r));
        assert_ne!(v, window_version("8", &r));
        assert_ne!(v, window_version("7", &wire::Rect { x: 2, ..r.clone() }));
        assert_ne!(v, window_version("7", &wire::Rect { width: 301, ..r }));
    }

    #[test]
    fn digests_ignore_nothing_but_the_secure_value_is_never_present() {
        let mut a = wire::AxNode { element_id: "n1".into(), role: "textfield".into(), value: "x".into(), ..Default::default() };
        let d1 = tree_digest(std::slice::from_ref(&a));
        a.value = "y".into();
        assert_ne!(d1, tree_digest(std::slice::from_ref(&a)));
        assert_eq!(structure_digest(std::slice::from_ref(&a)), structure_digest(&[wire::AxNode { element_id: "n1".into(), role: "textfield".into(), ..Default::default() }]));
    }

    #[test]
    fn ax_errors_become_typed_refusals_with_honest_delivery() {
        assert_eq!(ax_fail(kAXErrorAPIDisabled, "x", true).code, "PERMISSION_REQUIRED");
        assert_eq!(ax_fail(kAXErrorInvalidUIElement, "x", true).code, "TARGET_STALE");
        let f = ax_fail(kAXErrorCannotComplete, "press", true);
        assert_eq!((f.code, f.delivery), ("INPUT_FAILED", wire::Delivery::Unknown));
        let f = ax_fail(kAXErrorCannotComplete, "read", false);
        assert_eq!((f.code, f.delivery), ("ACCESSIBILITY_UNAVAILABLE", wire::Delivery::NotDelivered));
    }
}
