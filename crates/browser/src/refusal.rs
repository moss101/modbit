//! Typed refusals of the browser runtime (PX-073, PX-120, PX-121): the stable
//! codes a host or the Core emits and the machine-readable escalation each
//! one carries. The escalation is one shared type — what the caller should do
//! next — so a model, a client and a supervisor can act on a refusal without
//! reading prose: the recovery text stays prose for the model, the
//! `escalation` field is for programs. The same type serves the native
//! control runtime when it lands (PX-069); nothing here is browser-specific
//! beyond the code table.

use serde::{Deserialize, Serialize};

/// What the caller does next after a refusal.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Escalation {
    /// The same call may be repeated after a short wait; the condition is transient.
    Retry,
    /// A person decides (a policy, a dialog, a hand-over); nothing the agent does clears it.
    AskUser,
    /// Read the page again before doing anything: the state the call was
    /// decided under is gone, or nothing is known about what happened.
    ReObserve,
    /// This route will not work for this task; stop trying it.
    Abandon,
}

impl Escalation {
    /// The wire name (`retry`, `ask_user`, `re_observe`, `abandon`).
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Retry => "retry",
            Self::AskUser => "ask_user",
            Self::ReObserve => "re_observe",
            Self::Abandon => "abandon",
        }
    }
}

/// The escalation of a refusal code. A code outside the table is the
/// bridge's own failure: reading the page again is the only safe next step.
#[must_use]
pub fn escalation_for(code: &str) -> Escalation {
    match code {
        // The state the call was decided under is gone.
        "TARGET_STALE"
        | "TARGET_OCCLUDED"
        | "TARGET_GONE"
        | "TARGET_NOT_EDITABLE"
        | "TARGET_UNLOCATED"
        | "TARGET_DISABLED"
        | "ACTION_UNSAFE"
        | "STALE_GENERATION"
        | "VIEW_RESET"
        | "VIEW_RESTARTING"
        | "ACCESSIBILITY_UNAVAILABLE"
        | "POSTCONDITION_FAILED"
        | "WAIT_TIMEOUT"
        | "SCROLL_NOT_POSSIBLE"
        | "FORM_FIELD_UNAVAILABLE"
        | "FORM_NOT_FOUND"
        | "FORM_FIELD_UNKNOWN"
        | "INVALID_ACTION"
        | "BROWSER_TIMEOUT"
        | "BROWSER_HOST_GONE"
        | "OUTCOME_UNKNOWN"
        | "UNKNOWN_OUTCOME"
        | "UNKNOWN_OUTCOME_LATCHED" => Escalation::ReObserve,
        // Transient: wait, then the same call.
        "HUMAN_ACTIVE" | "NAVIGATION_FAILED" | "CAPTURE_TIMEOUT" => Escalation::Retry,
        // A person decides.
        "MODAL_BLOCKING"
        | "WINDOW_UNVERIFIABLE"
        | "EMERGENCY_STOPPED"
        | "TARGET_NOT_ALLOWED"
        | "ORIGIN_NOT_ALLOWED"
        | "CERTIFICATE_REJECTED"
        | "CERTIFICATE_PENDING"
        | "NO_BROWSER_HOST"
        | "NO_BROWSER_SESSION" => Escalation::AskUser,
        // This route does not work.
        "PERMISSION_REQUIRED"
        | "NAVIGATION_BLOCKED"
        | "FILE_ORIGIN"
        | "SITE_TOOL_PREFERRED"
        | "FRAME_NOT_ACTIONABLE"
        | "SECRET_FIELD_REQUIRES_CREDENTIAL"
        | "CREDENTIAL_TARGET_NOT_FIELD"
        | "CREDENTIAL_UNKNOWN"
        | "CREDENTIAL_ORIGIN_MISMATCH"
        | "CREDENTIAL_UNAVAILABLE"
        | "VISUAL_ACTION_UNSUPPORTED"
        | "UNSUPPORTED"
        | "UNSUPPORTED_ACTION" => Escalation::Abandon,
        _ => Escalation::ReObserve,
    }
}

/// Raw DevTools-protocol domains no browser tool, host request or future
/// raw-protocol rung may use: the page's storage, the browser process, the
/// system and the target list are the host's, never the model's. `Input` is
/// denied to any caller but the host's own structural dispatch.
pub const CDP_DENIED_DOMAINS: &[&str] = &[
    "Browser",
    "Input",
    "Storage",
    "SystemInfo",
    "Target",
    "Tethering",
];

/// Methods denied outside the denied domains: cookies, the cache, file
/// inputs, navigation and navigation history.
pub const CDP_DENIED_METHODS: &[&str] = &[
    "Network.getCookies",
    "Network.getAllCookies",
    "Network.setCookie",
    "Network.setCookies",
    "Network.deleteCookies",
    "Network.clearBrowserCookies",
    "Network.clearBrowserCache",
    "Network.setCacheDisabled",
    "Network.getResponseBody",
    "Network.loadNetworkResource",
    "DOM.setFileInputFiles",
    "Page.navigate",
    "Page.navigateToHistoryEntry",
    "Page.getNavigationHistory",
    "Page.resetNavigationHistory",
    "Page.reload",
    "Page.setDownloadBehavior",
    "Page.addScriptToEvaluateOnLoad",
    "Fetch.enable",
    "Fetch.fulfillRequest",
    "Fetch.continueRequest",
    "Emulation.setUserAgentOverride",
];

/// Whether a DevTools-protocol method is on the deny list. The check is on
/// the method name alone (the host applies it before any attach and before
/// any command), so a new method in a denied domain is denied without a code
/// change.
#[must_use]
pub fn cdp_denied(method: &str) -> bool {
    let domain = method.split('.').next().unwrap_or_default();
    CDP_DENIED_DOMAINS.contains(&domain) || CDP_DENIED_METHODS.contains(&method)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_refusal_has_an_escalation_and_the_new_codes_ask_the_person() {
        assert_eq!(escalation_for("TARGET_NOT_ALLOWED"), Escalation::AskUser);
        assert_eq!(escalation_for("ORIGIN_NOT_ALLOWED"), Escalation::AskUser);
        assert_eq!(escalation_for("TARGET_STALE"), Escalation::ReObserve);
        assert_eq!(escalation_for("HUMAN_ACTIVE"), Escalation::Retry);
        assert_eq!(escalation_for("PERMISSION_REQUIRED"), Escalation::Abandon);
        assert_eq!(
            escalation_for("UNKNOWN_OUTCOME_LATCHED"),
            Escalation::ReObserve
        );
        assert_eq!(escalation_for("SOMETHING_NEW"), Escalation::ReObserve);
        assert_eq!(Escalation::AskUser.label(), "ask_user");
        assert_eq!(
            serde_json::to_value(Escalation::ReObserve).unwrap(),
            "re_observe"
        );
    }

    #[test]
    fn the_deny_list_covers_domains_and_methods_and_nothing_else_the_host_needs() {
        for m in [
            "Browser.close",
            "Input.dispatchMouseEvent",
            "Storage.clearDataForOrigin",
            "SystemInfo.getInfo",
            "Target.setAutoAttach",
            "Tethering.bind",
            "Network.getAllCookies",
            "Network.setCookie",
            "Network.clearBrowserCache",
            "DOM.setFileInputFiles",
            "Page.navigate",
            "Page.getNavigationHistory",
        ] {
            assert!(cdp_denied(m), "{m} must be denied");
        }
        for m in [
            "Accessibility.getFullAXTree",
            "DOM.describeNode",
            "Runtime.callFunctionOn",
            "Page.getFrameTree",
            "Page.captureScreenshot",
            "Network.enable",
        ] {
            assert!(!cdp_denied(m), "{m} is a read the host needs");
        }
    }
}
