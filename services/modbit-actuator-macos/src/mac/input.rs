//! Synthesised input - the LAST rung of the ladder (CUC-A04). Two routes:
//!
//! * [`Route::Pid`] (APP scope): events go straight to one process with a
//!   private event source. They do not move the real cursor, do not enter the
//!   hardware event stream, and so never count as human activity.
//! * [`Route::Hid`] (SCREEN scope): events enter at the HID tap like a real
//!   device, the real cursor moves, and the actuator marks its own injection
//!   window so the human-input monitor does not mistake it for a person.
//!
//! Every event is a unit of work: a Stop or a deadline (the `Ctl`) ends the
//! sequence at the next event, and the caller is told how many were sent.
#![allow(unsafe_code)]
// PX-168 lint policy: this file baselined expect/unwrap sites; new ones need review.
#![allow(clippy::expect_used)]
use std::time::Duration;

use super::cf::Cf;
use super::ffi::{
    CGEventCreateKeyboardEvent, CGEventCreateMouseEvent, CGEventCreateScrollWheelEvent2,
    CGEventKeyboardSetUnicodeString, CGEventPost, CGEventPostToPid, CGEventRef, CGEventSetFlags,
    CGEventSetIntegerValueField, CGEventSetLocation, CGEventSourceCreate, CGEventSourceRef,
    CGPoint,
};
use crate::backend::{Ctl, Halt};

/// Where an event is delivered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Route {
    /// To one process, no cursor, no hardware stream.
    Pid(u32),
    /// Into the hardware event stream (SCREEN scope).
    Hid,
}

/// An event source for one route.
pub struct Poster {
    source: Cf,
    route: Route,
}

const SPACING: Duration = Duration::from_millis(4);

impl Poster {
    /// A poster for `route`.
    ///
    /// # Panics
    /// If the system cannot create an event source (no window-server session).
    #[must_use]
    pub fn new(route: Route) -> Self {
        // kCGEventSourceStatePrivate = -1, kCGEventSourceStateHIDSystemState = 1.
        let state = if route == Route::Hid { 1 } else { -1 };
        // SAFETY: "create" rule.
        let s: CGEventSourceRef = unsafe { CGEventSourceCreate(state) };
        Self {
            source: Cf::own(s.cast_const()).expect("a window-server session"),
            route,
        }
    }

    fn src(&self) -> CGEventSourceRef {
        self.source.raw().cast_mut()
    }

    fn post(&self, ev: CGEventRef, ctl: &Ctl) {
        if ev.is_null() {
            return;
        }
        ctl.own_event();
        match self.route {
            Route::Pid(pid) => {
                // SAFETY: `ev` is live; the pid fits i32 (checked at route creation).
                unsafe { CGEventPostToPid(i32::try_from(pid).unwrap_or(0), ev) };
            }
            Route::Hid => {
                // SAFETY: `ev` is live; tap 0 = kCGHIDEventTap.
                unsafe { CGEventPost(0, ev) };
            }
        }
        // SAFETY: "create" rule; the event is ours to release.
        drop(Cf::own(ev.cast_const()));
    }

    fn mouse(&self, ty: u32, at: CGPoint, button: u32, count: i64, ctl: &Ctl) {
        // SAFETY: live source; "create" rule on the result.
        let ev = unsafe { CGEventCreateMouseEvent(self.src(), ty, at, button) };
        if !ev.is_null() {
            // SAFETY: `ev` live. Field 1 = kCGMouseEventClickState.
            unsafe { CGEventSetIntegerValueField(ev, 1, count) };
        }
        self.post(ev, ctl);
    }

    /// Click `count` times with `button` ("left" | "right" | "middle").
    ///
    /// # Errors
    /// [`Halt`] with the number of events already sent.
    pub fn click(
        &self,
        at: CGPoint,
        button: &str,
        count: u32,
        ctl: &Ctl,
    ) -> Result<usize, (Halt, usize)> {
        let (down, up, b) = match button {
            "right" => (3, 4, 1),
            "middle" => (25, 26, 2),
            _ => (1, 2, 0),
        };
        let mut sent = 0;
        // Move the pointer there first for the hardware route.
        if self.route == Route::Hid {
            self.mouse(5, at, b, 0, ctl);
            sent += 1;
        }
        for i in 1..=count.max(1) {
            ctl.check().map_err(|h| (h, sent))?;
            self.mouse(down, at, b, i64::from(i), ctl);
            sent += 1;
            std::thread::sleep(Duration::from_millis(12));
            self.mouse(up, at, b, i64::from(i), ctl);
            sent += 1;
            std::thread::sleep(Duration::from_millis(12));
        }
        Ok(sent)
    }

    /// Move the real pointer (hardware route only is meaningful).
    pub fn move_to(&self, at: CGPoint, ctl: &Ctl) {
        self.mouse(5, at, 0, 0, ctl);
    }

    /// Press at `from`, drag to `to` in steps, release.
    ///
    /// # Errors
    /// [`Halt`] with the number of events already sent.
    pub fn drag(&self, from: CGPoint, to: CGPoint, ctl: &Ctl) -> Result<usize, (Halt, usize)> {
        let mut sent = 0;
        self.mouse(1, from, 0, 1, ctl);
        sent += 1;
        for i in 1..=12 {
            ctl.check().map_err(|h| (h, sent))?;
            let t = f64::from(i) / 12.0;
            let p = CGPoint {
                x: from.x + (to.x - from.x) * t,
                y: from.y + (to.y - from.y) * t,
            };
            self.mouse(6, p, 0, 1, ctl);
            sent += 1;
            std::thread::sleep(Duration::from_millis(8));
        }
        self.mouse(2, to, 0, 1, ctl);
        Ok(sent + 1)
    }

    /// Scroll by `dx`, `dy` pixels at `at`.
    pub fn scroll(&self, at: CGPoint, dx: f64, dy: f64, ctl: &Ctl) {
        // SAFETY: live source; units 0 = pixels.
        #[allow(clippy::cast_possible_truncation)]
        let ev = unsafe {
            CGEventCreateScrollWheelEvent2(
                self.src(),
                0,
                2,
                dy.round() as i32,
                dx.round() as i32,
                0,
            )
        };
        if !ev.is_null() {
            // SAFETY: `ev` live.
            unsafe { CGEventSetLocation(ev, at) };
        }
        self.post(ev, ctl);
    }

    fn key(&self, vkey: u16, down: bool, flags: u64, unicode: Option<&[u16]>, ctl: &Ctl) {
        // SAFETY: live source; "create" rule.
        let ev = unsafe { CGEventCreateKeyboardEvent(self.src(), vkey, u8::from(down)) };
        if ev.is_null() {
            return;
        }
        // SAFETY: `ev` live; the buffer outlives the call (the API copies it).
        unsafe {
            CGEventSetFlags(ev, flags);
            if let Some(u) = unicode {
                CGEventKeyboardSetUnicodeString(ev, u.len(), u.as_ptr());
            }
        }
        self.post(ev, ctl);
    }

    /// Type `text` one character at a time as unicode key events (layout
    /// independent). Checks the `Ctl` before every character.
    ///
    /// # Errors
    /// [`Halt`] with the number of characters already typed.
    pub fn type_text(&self, text: &str, ctl: &Ctl) -> Result<usize, (Halt, usize)> {
        let mut typed = 0;
        for ch in text.chars() {
            ctl.check().map_err(|h| (h, typed))?;
            let mut buf = [0u16; 2];
            let u = ch.encode_utf16(&mut buf);
            self.key(0, true, 0, Some(u), ctl);
            self.key(0, false, 0, Some(u), ctl);
            typed += 1;
            std::thread::sleep(SPACING);
        }
        Ok(typed)
    }

    /// Press a chord such as `cmd+a`, `shift+tab`, `escape`.
    ///
    /// # Errors
    /// A message for an unknown key name; [`Halt`] is folded into it by the caller.
    pub fn chord(&self, chord: &str, ctl: &Ctl) -> Result<(), String> {
        let (flags, vkey) = parse_chord(chord)?;
        self.key(vkey, true, flags, None, ctl);
        std::thread::sleep(SPACING);
        self.key(vkey, false, flags, None, ctl);
        Ok(())
    }
}

const CMD: u64 = 0x10_0000;
const SHIFT: u64 = 0x2_0000;
const ALT: u64 = 0x8_0000;
const CTRL: u64 = 0x4_0000;

/// `("cmd+shift", vkey)` for `cmd+shift+z`. ANSI virtual key codes.
///
/// # Errors
/// Names the unknown part.
pub fn parse_chord(chord: &str) -> Result<(u64, u16), String> {
    let parts: Vec<String> = chord
        .split('+')
        .map(|p| p.trim().to_ascii_lowercase())
        .collect();
    let Some((key, mods)) = parts.split_last() else {
        return Err("an empty key".into());
    };
    let mut flags = 0;
    for m in mods {
        flags |= match m.as_str() {
            "cmd" | "command" | "meta" | "super" => CMD,
            "shift" => SHIFT,
            "alt" | "option" | "opt" => ALT,
            "ctrl" | "control" => CTRL,
            other => return Err(format!("unknown modifier {other:?}")),
        };
    }
    let v: u16 = match key.as_str() {
        "a" => 0,
        "s" => 1,
        "d" => 2,
        "f" => 3,
        "h" => 4,
        "g" => 5,
        "z" => 6,
        "x" => 7,
        "c" => 8,
        "v" => 9,
        "b" => 11,
        "q" => 12,
        "w" => 13,
        "e" => 14,
        "r" => 15,
        "y" => 16,
        "t" => 17,
        "1" => 18,
        "2" => 19,
        "3" => 20,
        "4" => 21,
        "6" => 22,
        "5" => 23,
        "=" => 24,
        "9" => 25,
        "7" => 26,
        "-" => 27,
        "8" => 28,
        "0" => 29,
        "]" => 30,
        "o" => 31,
        "u" => 32,
        "[" => 33,
        "i" => 34,
        "p" => 35,
        "enter" | "return" => 36,
        "l" => 37,
        "j" => 38,
        "'" => 39,
        "k" => 40,
        ";" => 41,
        "\\" => 42,
        "," => 43,
        "/" => 44,
        "n" => 45,
        "m" => 46,
        "." => 47,
        "tab" => 48,
        "space" => 49,
        "`" => 50,
        "delete" | "backspace" => 51,
        "escape" | "esc" => 53,
        "forwarddelete" => 117,
        "home" => 115,
        "end" => 119,
        "pageup" => 116,
        "pagedown" => 121,
        "left" => 123,
        "right" => 124,
        "down" => 125,
        "up" => 126,
        "f1" => 122,
        "f2" => 120,
        "f3" => 99,
        "f4" => 118,
        "f5" => 96,
        "f6" => 97,
        "f7" => 98,
        "f8" => 100,
        "f9" => 101,
        "f10" => 109,
        "f11" => 103,
        "f12" => 111,
        other => return Err(format!("unknown key {other:?}")),
    };
    Ok((flags, v))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chords_parse_to_flags_and_virtual_keys() {
        assert_eq!(parse_chord("cmd+a"), Ok((CMD, 0)));
        assert_eq!(parse_chord("Shift+Tab"), Ok((SHIFT, 48)));
        assert_eq!(parse_chord("escape"), Ok((0, 53)));
        assert_eq!(parse_chord("ctrl+alt+delete"), Ok((CTRL | ALT, 51)));
        assert!(parse_chord("hyper+a").unwrap_err().contains("modifier"));
        assert!(parse_chord("cmd+blah").unwrap_err().contains("key"));
    }
}
