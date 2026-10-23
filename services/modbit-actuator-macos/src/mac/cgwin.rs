//! The window server's view: which windows exist, where, in what order, and
//! whether the screen is locked. Needs no permission for bounds, owner and
//! order (titles of other processes' windows need Screen Recording).
#![allow(unsafe_code)]

use super::cf::{Cf, CfStr};
use super::ffi::{
    CGDisplayBounds, CGMainDisplayID, CGRect, CGRectMakeWithDictionaryRepresentation,
    CGSessionCopyCurrentDictionary, CGWindowListCopyWindowInfo,
};

/// One window of the window server.
#[derive(Clone, Debug)]
pub struct CgWin {
    /// `CGWindowID`.
    pub number: u32,
    /// Owner process.
    pub pid: u32,
    /// Window layer (0 = ordinary windows).
    pub layer: i32,
    /// Global bounds in display points, top-left origin.
    pub bounds: CGRect,
    /// Title when the server will say it.
    pub title: String,
    /// Currently on screen.
    pub on_screen: bool,
}

/// kCGWindowListOptionAll.
const ALL: u32 = 0;
/// kCGWindowListOptionOnScreenOnly | kCGWindowListExcludeDesktopElements.
const ON_SCREEN: u32 = 1 | 16;

/// Every window (or the on-screen ones), front to back.
#[must_use]
pub fn list(on_screen_only: bool) -> Vec<CgWin> {
    let opt = if on_screen_only { ON_SCREEN } else { ALL };
    // SAFETY: "copy" rule: owned by the returned Cf; keys are live CFStrings.
    let Some(arr) = Cf::own(unsafe { CGWindowListCopyWindowInfo(opt, 0) }) else {
        return vec![];
    };
    let (k_num, k_pid, k_layer, k_bounds, k_name, k_on) = (
        CfStr::new("kCGWindowNumber"),
        CfStr::new("kCGWindowOwnerPID"),
        CfStr::new("kCGWindowLayer"),
        CfStr::new("kCGWindowBounds"),
        CfStr::new("kCGWindowName"),
        CfStr::new("kCGWindowIsOnscreen"),
    );
    let mut out = Vec::new();
    for d in arr.as_array().unwrap_or_default() {
        let (Some(num), Some(pid), Some(b)) = (
            d.dict_get(&k_num).and_then(|v| v.as_i64()),
            d.dict_get(&k_pid).and_then(|v| v.as_i64()),
            d.dict_get(&k_bounds),
        ) else {
            continue;
        };
        let mut rect = CGRect::default();
        // SAFETY: `b` is a live CFDictionary; `rect` is a local out value.
        if unsafe { CGRectMakeWithDictionaryRepresentation(b.raw(), &raw mut rect) } == 0 {
            continue;
        }
        out.push(CgWin {
            number: u32::try_from(num).unwrap_or(0),
            pid: u32::try_from(pid).unwrap_or(0),
            layer: d
                .dict_get(&k_layer)
                .and_then(|v| v.as_i64())
                .and_then(|v| i32::try_from(v).ok())
                .unwrap_or(0),
            bounds: rect,
            title: d
                .dict_get(&k_name)
                .and_then(|v| v.as_string())
                .unwrap_or_default(),
            on_screen: d.dict_get(&k_on).and_then(|v| v.as_bool()).unwrap_or(false),
        });
    }
    out
}

/// The main display's bounds in points.
#[must_use]
pub fn main_display() -> CGRect {
    // SAFETY: no pointers.
    unsafe { CGDisplayBounds(CGMainDisplayID()) }
}

/// The screen is locked, or this login session is not the one on the console
/// (fast user switching): input and capture belong to someone else.
#[must_use]
pub fn secure_desktop() -> bool {
    // SAFETY: "copy" rule.
    let Some(d) = Cf::own(unsafe { CGSessionCopyCurrentDictionary() }) else {
        // No session dictionary at all: not a console session we can act in.
        return true;
    };
    let locked = d
        .dict_get(&CfStr::new("CGSSessionScreenIsLocked"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let on_console = d
        .dict_get(&CfStr::new("kCGSSessionOnConsoleKey"))
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    locked || !on_console
}
