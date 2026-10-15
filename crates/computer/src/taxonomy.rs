//! The typed refusal taxonomy of native control (docs/66 CUC-D01, REQ-EV-0089).
//!
//! Every refusal is a [`Refusal`]: a stable [`Code`], a message, the
//! recovery guidance the model reads and one of four machine-readable
//! escalations (the shared [`Escalation`] type of the browser runtime:
//! `retry` = nothing was actuated and the same call is safe, `ask_user` =
//! only the person at the machine can clear it, `re_observe` = use a
//! different tool and observe first, `abandon` = stop: no further input or
//! control this turn). The table below is the one source: the tool
//! descriptions, the refusals and the tests all read it, so what the model is
//! told and what the code does cannot drift apart.

use modbit_browser::refusal::Escalation;
use serde_json::{Value, json};

/// A refusal or failure code of the Computer Runtime.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Code {
    /// An element id or coordinate token no longer names what it did.
    TargetStale,
    /// Something covers the target.
    TargetOccluded,
    /// The application or window could not be verified, or is not drivable.
    WindowUnverifiable,
    /// The application exposes no accessibility tree.
    AccessibilityUnavailable,
    /// The person is using the machine.
    HumanActive,
    /// A modal dialog blocks the window.
    ModalBlocking,
    /// The element does not take the input.
    TargetNotEditable,
    /// The action is protected or forbidden.
    ActionUnsafe,
    /// The operating system has not granted the actuator a permission.
    PermissionRequired,
    /// The secure desktop or a locked screen: nothing is captured.
    SecureDesktop,
    /// The target runs with more privilege than the actuator.
    TargetElevated,
    /// The capture failed.
    CaptureFailed,
    /// A coordinate action without a fresh screenshot.
    ScreenshotRequired,
    /// An argument is missing, misspelt or out of range.
    InvalidArguments,
    /// No such tool.
    UnknownTool,
    /// The input failed provably before it was delivered.
    InputFailed,
    /// A read did not finish in time.
    Timeout,
    /// The actuator cannot do what was asked.
    UnsupportedRequest,
    /// No actuator is attached.
    ActuatorUnavailable,
    /// An earlier input may have happened and nothing says whether it did.
    OutcomeUnknown,
    /// No control session is open.
    SessionRequired,
    /// Another control session holds the machine.
    SessionBusy,
    /// The person pressed Stop.
    UserAborted,
    /// The person declined the approval.
    ApprovalDenied,
    /// The observation grant ended.
    GrantExpired,
}

impl Code {
    /// Every code, in the order doc 66 lists them.
    pub const ALL: [Code; 25] = [
        Code::TargetStale,
        Code::TargetOccluded,
        Code::WindowUnverifiable,
        Code::AccessibilityUnavailable,
        Code::HumanActive,
        Code::ModalBlocking,
        Code::TargetNotEditable,
        Code::ActionUnsafe,
        Code::PermissionRequired,
        Code::SecureDesktop,
        Code::TargetElevated,
        Code::CaptureFailed,
        Code::ScreenshotRequired,
        Code::InvalidArguments,
        Code::UnknownTool,
        Code::InputFailed,
        Code::Timeout,
        Code::UnsupportedRequest,
        Code::ActuatorUnavailable,
        Code::OutcomeUnknown,
        Code::SessionRequired,
        Code::SessionBusy,
        Code::UserAborted,
        Code::ApprovalDenied,
        Code::GrantExpired,
    ];

    /// The stable wire name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Code::TargetStale => "TARGET_STALE",
            Code::TargetOccluded => "TARGET_OCCLUDED",
            Code::WindowUnverifiable => "WINDOW_UNVERIFIABLE",
            Code::AccessibilityUnavailable => "ACCESSIBILITY_UNAVAILABLE",
            Code::HumanActive => "HUMAN_ACTIVE",
            Code::ModalBlocking => "MODAL_BLOCKING",
            Code::TargetNotEditable => "TARGET_NOT_EDITABLE",
            Code::ActionUnsafe => "ACTION_UNSAFE",
            Code::PermissionRequired => "PERMISSION_REQUIRED",
            Code::SecureDesktop => "SECURE_DESKTOP",
            Code::TargetElevated => "TARGET_ELEVATED",
            Code::CaptureFailed => "CAPTURE_FAILED",
            Code::ScreenshotRequired => "SCREENSHOT_REQUIRED",
            Code::InvalidArguments => "INVALID_ARGUMENTS",
            Code::UnknownTool => "UNKNOWN_TOOL",
            Code::InputFailed => "INPUT_FAILED",
            Code::Timeout => "TIMEOUT",
            Code::UnsupportedRequest => "UNSUPPORTED_REQUEST",
            Code::ActuatorUnavailable => "ACTUATOR_UNAVAILABLE",
            Code::OutcomeUnknown => "OUTCOME_UNKNOWN",
            Code::SessionRequired => "SESSION_REQUIRED",
            Code::SessionBusy => "SESSION_BUSY",
            Code::UserAborted => "USER_ABORTED",
            Code::ApprovalDenied => "APPROVAL_DENIED",
            Code::GrantExpired => "GRANT_EXPIRED",
        }
    }

    /// The code a wire name denotes.
    #[must_use]
    pub fn parse(name: &str) -> Option<Code> {
        Code::ALL.into_iter().find(|c| c.as_str() == name)
    }

    /// What the caller does next (CUC-D01).
    #[must_use]
    pub const fn escalation(self) -> Escalation {
        match self {
            // Nothing was actuated and the same call is safe after a wait.
            Code::HumanActive | Code::CaptureFailed | Code::Timeout | Code::SessionBusy => {
                Escalation::Retry
            }
            // Only the person at the machine can clear it.
            Code::WindowUnverifiable
            | Code::ModalBlocking
            | Code::PermissionRequired
            | Code::SecureDesktop
            | Code::TargetElevated
            | Code::ActuatorUnavailable
            | Code::OutcomeUnknown
            | Code::GrantExpired => Escalation::AskUser,
            // Observe first, or use a different tool.
            Code::TargetStale
            | Code::TargetOccluded
            | Code::AccessibilityUnavailable
            | Code::TargetNotEditable
            | Code::ScreenshotRequired
            | Code::InvalidArguments
            | Code::UnknownTool
            | Code::InputFailed
            | Code::SessionRequired => Escalation::ReObserve,
            // No further input or control this turn.
            Code::ActionUnsafe
            | Code::UnsupportedRequest
            | Code::UserAborted
            | Code::ApprovalDenied => Escalation::Abandon,
        }
    }

    /// What the model should do about it, in words.
    #[must_use]
    pub const fn recovery(self) -> &'static str {
        match self {
            Code::TargetStale => {
                "the element id or coordinate token is from an earlier look: read the state again (computer.state) or take a new screenshot, and act on what it names now; nothing was actuated"
            }
            Code::TargetOccluded => {
                "something covers the target: take a screenshot, dismiss what is over it, then act again; nothing was actuated"
            }
            Code::WindowUnverifiable => {
                "the application or its window could not be verified as the one approved, or it is one no approval can lift (a credential store, system settings, a terminal, an authentication dialog, Modbit itself, or an application the policy does not allow): do not look for another way to drive it; tell the person"
            }
            Code::AccessibilityUnavailable => {
                "the application exposes no accessibility tree: take a screenshot (computer.screenshot) and use coordinates against it"
            }
            Code::HumanActive => {
                "the person is using the keyboard or pointer: wait, then observe again (computer.state) to take control back; nothing was actuated"
            }
            Code::ModalBlocking => {
                "a modal dialog blocks the window and is the person's to answer: observe and wait, do not act around it"
            }
            Code::TargetNotEditable => {
                "the element does not take that input (it is not settable, is disabled, holds a secure value, or nothing verified is focused): read the state and pick the field the application names for this value"
            }
            Code::ActionUnsafe => {
                "the action is refused as unsafe or forbidden (a destructive key the call did not name, a secure field, an application that is never driven, a blocker only the person can deal with, or policy): do not act around it; report what you need"
            }
            Code::PermissionRequired => {
                "the operating system has not granted the actuator the permission this needs (accessibility or screen recording): the person grants it in System Settings; the task cannot do this until they do"
            }
            Code::SecureDesktop => {
                "the screen is locked or showing a secure prompt: nothing is captured and nothing can be driven; ask the person"
            }
            Code::TargetElevated => {
                "the application runs with more privilege than the actuator and cannot be driven: tell the person"
            }
            Code::CaptureFailed => {
                "the capture failed; read the state (computer.state) and try the screenshot once more"
            }
            Code::ScreenshotRequired => {
                "a coordinate action needs a screenshot taken after the last change: call computer.screenshot, then use its frame token with coordinates from that image"
            }
            Code::InvalidArguments => {
                "an argument is missing, misspelt or out of range; the answer lists the advertised arguments: repeat the call with exactly those; nothing ran and no default action was substituted"
            }
            Code::UnknownTool => "no such computer tool; the answer lists the tools",
            Code::InputFailed => {
                "the input failed before it was delivered; nothing happened. Read the state and decide again"
            }
            Code::Timeout => {
                "the read did not finish in time; wait and call it again, or read a smaller window"
            }
            Code::UnsupportedRequest => {
                "this actuator cannot do that; use another rung (an element action, then coordinates, then keys) or tell the person"
            }
            Code::ActuatorUnavailable => {
                "no actuator is attached to this Core, or it is not running: the person must start or repair it; native control is unavailable until then"
            }
            Code::OutcomeUnknown => {
                "an earlier input may have happened and nothing says whether it did: no further input runs. Start a new control session (computer.start, which the person approves) and observe (computer.state) to see the real state; never repeat the earlier input blind"
            }
            Code::SessionRequired => {
                "no control session is open for this task: call computer.start for the application first (the person approves it)"
            }
            Code::SessionBusy => {
                "another control session holds the machine (one controller at a time): wait, or release yours (computer.release) first"
            }
            Code::UserAborted => {
                "the person pressed Stop: no input, start or release is accepted for the rest of this turn; stop and report"
            }
            Code::ApprovalDenied => {
                "the person declined this action: do not retry it; report and ask what to do"
            }
            Code::GrantExpired => {
                "the observation grant ended (its time, its call cap, the turn or the mode changed): call computer.start again and the person is asked again"
            }
        }
    }
}

impl std::fmt::Display for Code {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The escalation of a code name; a name outside the table asks the person.
#[must_use]
pub fn escalation_for(code: &str) -> Escalation {
    Code::parse(code).map_or(Escalation::AskUser, Code::escalation)
}

/// A typed refusal: what happened, what to do, and (when the caller can use
/// it) the facts that explain it.
#[derive(Clone, Debug, PartialEq)]
pub struct Refusal {
    /// The code.
    pub code: Code,
    /// What happened, in words (may quote untrusted application text; the
    /// caller labels it).
    pub message: String,
    /// Structured facts (the advertised arguments, the changed handle, the
    /// latch) merged into the answer.
    pub facts: Value,
}

impl Refusal {
    /// A refusal with no extra facts.
    #[must_use]
    pub fn new(code: Code, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            facts: Value::Null,
        }
    }

    /// Add structured facts.
    #[must_use]
    pub fn with_facts(mut self, facts: Value) -> Self {
        self.facts = facts;
        self
    }

    /// The structured answer the model and the programs read.
    #[must_use]
    pub fn to_json(&self) -> Value {
        let mut v = json!({
            "code": self.code.as_str(),
            "escalation": self.code.escalation().label(),
            "message": self.message,
            "recovery": self.code.recovery(),
            "actuated": false,
        });
        if let (Some(o), Some(f)) = (v.as_object_mut(), self.facts.as_object()) {
            for (k, val) in f {
                o.insert(k.clone(), val.clone());
            }
        }
        v
    }

    /// The message with its recovery guidance, as the tool failure text.
    #[must_use]
    pub fn text(&self) -> String {
        format!("{}; {}", self.message, self.code.recovery())
    }
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for Refusal {}

/// The contract explained to the model from the same table the code uses.
#[must_use]
pub fn contract_text() -> String {
    let mut out = String::from(
        "Refusals are typed: {code, escalation, message, recovery}. Escalation `retry` = nothing was actuated and the same call is safe after a wait; `ask_user` = only the person can clear it; `re_observe` = observe first or use a different tool; `abandon` = stop. Codes: ",
    );
    let names: Vec<String> = Code::ALL
        .iter()
        .map(|c| format!("{} ({})", c.as_str(), c.escalation().label()))
        .collect();
    out.push_str(&names.join(", "));
    out.push('.');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_code_round_trips_and_has_an_escalation_and_recovery() {
        let mut seen = std::collections::HashSet::new();
        for c in Code::ALL {
            assert!(seen.insert(c.as_str()), "{c} listed twice");
            assert_eq!(Code::parse(c.as_str()), Some(c));
            assert!(c.recovery().len() > 20, "{c} has no recovery guidance");
            assert_eq!(escalation_for(c.as_str()), c.escalation());
        }
        assert_eq!(Code::ALL.len(), 25);
        assert_eq!(Code::parse("NOT_A_CODE"), None);
        // A name outside the table asks the person rather than inviting a retry.
        assert_eq!(escalation_for("NOT_A_CODE"), Escalation::AskUser);
    }

    #[test]
    fn the_four_escalations_are_all_used_and_the_latch_asks_the_person() {
        use std::collections::HashSet;
        let kinds: HashSet<&str> = Code::ALL.iter().map(|c| c.escalation().label()).collect();
        assert_eq!(kinds.len(), 4);
        assert_eq!(Code::OutcomeUnknown.escalation(), Escalation::AskUser);
        assert_eq!(Code::HumanActive.escalation(), Escalation::Retry);
        assert_eq!(Code::UserAborted.escalation(), Escalation::Abandon);
        assert_eq!(Code::TargetStale.escalation(), Escalation::ReObserve);
    }

    #[test]
    fn a_refusal_serialises_its_code_escalation_and_facts_and_says_nothing_was_actuated() {
        let r = Refusal::new(Code::InvalidArguments, "unknown argument `elemnt`")
            .with_facts(json!({"advertised": ["element", "action"]}));
        let v = r.to_json();
        assert_eq!(v["code"], "INVALID_ARGUMENTS");
        assert_eq!(v["escalation"], "re_observe");
        assert_eq!(v["actuated"], false);
        assert_eq!(v["advertised"], json!(["element", "action"]));
        assert!(r.text().contains("repeat the call with exactly those"));
        assert!(contract_text().contains("OUTCOME_UNKNOWN (ask_user)"));
    }
}
