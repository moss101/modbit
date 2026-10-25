//! Page-declared tools (PX-124, docs/22 "Action hierarchy" rung 1, doc 79
//! ADC-E05): the WebMCP rung. A page that declares tools through the
//! browser's model-context interface (`navigator.modelContext`) is making a
//! **proposal**: the host discovers what each frame declared, binds every
//! tool to the frame and origin the *browser* says declared it (never a
//! claim in the page's own text) and to the page load that declared it, and
//! offers the Core a catalog. The Core labels it `UNTRUSTED_PAGE_PROPOSAL`,
//! lets the model describe a tool on demand, and judges a call like any
//! external tool: a tool the page annotates read-only is a read, an
//! unannotated one is a write and asks. A declaration cannot name a host
//! tool, cannot change another origin's tools and cannot outlive its page.
//!
//! This module is the pure part both sides share: the wire records, the
//! identity of a catalog entry, the reserved-name rule and the annotation
//! reading. The host's discovery is `apps/desktop/src/main/browser.ts`; the
//! tools are `crates/tools/src/browser_page_tools.rs`.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

/// The label every catalog entry carries: what a page says about its own
/// tools is a proposal, never a grant (docs/22; REQ-EV-0224).
pub const TRUST_LABEL: &str = "UNTRUSTED_PAGE_PROPOSAL";

/// The rung of the action hierarchy this module serves.
pub const RUNG: &str = "page_declared_tool";

/// The hierarchy, best first (docs/22 "Action hierarchy"): the record of a
/// call names the rung it took and, when it fell, the next one and why.
pub const HIERARCHY: &[&str] = &[
    "page_declared_tool",
    "derived_action",
    "accessibility_action",
    "raw_input",
];

/// Tools one page may offer at most (the rest are dropped and said so).
pub const MAX_TOOLS: usize = 64;
/// The longest tool name.
pub const MAX_NAME: usize = 64;
/// The longest description kept in a listing.
pub const LIST_DESCRIPTION: usize = 240;
/// The longest description kept at all.
pub const MAX_DESCRIPTION: usize = 2_000;
/// The largest input schema kept (serialised bytes).
pub const MAX_SCHEMA_BYTES: usize = 8 * 1024;
/// The largest result the host returns (serialised bytes).
pub const MAX_RESULT_BYTES: usize = 64 * 1024;

/// A tool as one frame declared it, with the identity the browser (not the
/// page) vouches for.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PageToolDecl {
    /// The frame the tool is registered in (a DevTools frame id).
    pub frame: String,
    /// That frame's origin, from the browser.
    pub origin: String,
    /// That frame's URL, from the browser.
    pub url: String,
    /// Whether the frame is the page's top frame.
    pub top: bool,
    /// The page load (document loader) that declared it: another load, another catalog.
    pub load: String,
    /// The name the page gave it.
    pub name: String,
    /// What the page says it does (untrusted).
    #[serde(default)]
    pub description: String,
    /// The page's input schema for it (untrusted; bounded by the host).
    #[serde(default)]
    pub input_schema: Value,
    /// The page's annotations (`readOnlyHint`, …) as it declared them.
    #[serde(default)]
    pub annotations: Value,
}

/// A declaration the host refused to list, and why.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PageToolRefusal {
    /// The frame that declared it.
    #[serde(default)]
    pub frame: String,
    /// Its origin.
    #[serde(default)]
    pub origin: String,
    /// The name as declared (bounded).
    pub name: String,
    /// `NAME_INVALID`, `NAME_RESERVED`, `TOO_MANY_TOOLS`, `SCHEMA_TOO_LARGE`, …
    pub code: String,
    /// What is wrong, for the person.
    #[serde(default)]
    pub reason: String,
}

/// `pt_` + 12 hex of sha256 over the session, the frame, the page load and
/// the name: a handle cannot be mistaken for another origin's tool of the
/// same name, or for a tool of an earlier page.
#[must_use]
pub fn tool_id(session: &str, frame: &str, load: &str, name: &str) -> String {
    let mut h = Sha256::new();
    for part in [session, frame, load, name] {
        h.update((part.len() as u64).to_be_bytes());
        h.update(part.as_bytes());
    }
    format!("pt_{}", &hex::encode(h.finalize())[..12])
}

/// Whether `id` has the shape of a catalog handle.
#[must_use]
pub fn is_tool_id(id: &str) -> bool {
    id.len() == 15 && id.starts_with("pt_") && id[3..].bytes().all(|b| b.is_ascii_hexdigit())
}

/// Whether a page's annotations call the tool read-only: `readOnlyHint` is
/// `true` and `destructiveHint` is not. Anything else — no annotation, an
/// annotation of another type, a destructive hint — is not a read: an
/// unannotated tool is treated as a write (doc 79 ADC-E05). The annotation
/// is the page's claim about itself (see docs/22 for the residual risk).
#[must_use]
pub fn declared_read_only(annotations: &Value) -> bool {
    annotations["readOnlyHint"] == Value::Bool(true)
        && annotations["destructiveHint"] != Value::Bool(true)
}

/// The host-tool namespaces no ordinary page vocabulary uses: a page tool
/// whose name reads as one of these (`browser_act`, `fs-write`) is trying to
/// be taken for a host tool. Words a shop or a mail client plausibly uses
/// (`search`, `test`, `task`, `review`, `plan`…) are not here: `search_orders`
/// is a fine page tool; the handle, not the name, is what a call uses.
const HOST_NAMESPACES: &[&str] = &[
    "browser", "fs", "git", "shell", "proc", "forge", "external", "mcp", "computer", "lsp",
];

/// Why a declared name may not be listed, or `None`. Host tools are named
/// `family.verb`; a page name is a plain identifier, so nothing a page
/// declares can be a host tool's name — and a name that merely *reads like*
/// one (`browser_act`, `fs-write`) is refused as well, so the model is not
/// offered a look-alike.
#[must_use]
pub fn refused_name(name: &str) -> Option<(&'static str, String)> {
    let valid = !name.is_empty()
        && name.len() <= MAX_NAME
        && name.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    if !valid {
        return Some((
            "NAME_INVALID",
            format!(
                "a page tool's name is a plain identifier (letters, digits, `_` and `-`, at most {MAX_NAME}); `{}` is not",
                name.chars().take(80).collect::<String>()
            ),
        ));
    }
    let lower = name.to_ascii_lowercase();
    let head = lower.split(['_', '-']).next().unwrap_or_default();
    if lower.contains(['_', '-']) && HOST_NAMESPACES.contains(&head) {
        return Some((
            "NAME_RESERVED",
            format!(
                "`{name}` reads like the host tool namespace `{head}.*`; a page cannot declare a tool named like a host tool"
            ),
        ));
    }
    if HOST_NAMESPACES.contains(&lower.as_str()) {
        return Some((
            "NAME_RESERVED",
            format!("`{name}` is a host tool namespace"),
        ));
    }
    None
}

/// Cut `s` to `max` characters (on a character boundary).
#[must_use]
pub fn clip(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_owned()
    } else {
        let mut out: String = s.chars().take(max).collect();
        out.push('…');
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_tool_is_a_read_only_when_the_page_says_so_and_nothing_contradicts_it() {
        assert!(declared_read_only(&json!({"readOnlyHint": true})));
        assert!(!declared_read_only(&json!({})), "unannotated is a write");
        assert!(!declared_read_only(&Value::Null));
        assert!(!declared_read_only(&json!({"readOnlyHint": "true"})));
        assert!(!declared_read_only(&json!({"readOnlyHint": 1})));
        assert!(!declared_read_only(
            &json!({"readOnlyHint": true, "destructiveHint": true})
        ));
        assert!(!declared_read_only(&json!({"readOnlyHint": false})));
    }

    #[test]
    fn a_handle_binds_session_frame_load_and_name() {
        let a = tool_id("s1", "F1", "L1", "search");
        assert!(is_tool_id(&a), "{a}");
        assert_ne!(a, tool_id("s2", "F1", "L1", "search"));
        assert_ne!(a, tool_id("s1", "F2", "L1", "search"), "another frame");
        assert_ne!(a, tool_id("s1", "F1", "L2", "search"), "another page load");
        assert_ne!(a, tool_id("s1", "F1", "L1", "search2"));
        assert_ne!(tool_id("s", "ab", "c", "d"), tool_id("s", "a", "bc", "d"));
        assert!(!is_tool_id("pt_zz"));
        assert!(!is_tool_id("ref_0123456789"));
    }

    #[test]
    fn a_name_that_is_or_reads_like_a_host_tool_is_refused() {
        for ok in [
            "search_products",
            "get_cart",
            "addToCart",
            "lookup-order",
            "x",
        ] {
            assert!(refused_name(ok).is_none(), "{ok}");
        }
        for (bad, code) in [
            ("browser.act", "NAME_INVALID"),
            ("fs.write", "NAME_INVALID"),
            ("shell.exec", "NAME_INVALID"),
            ("", "NAME_INVALID"),
            ("1abc", "NAME_INVALID"),
            ("has space", "NAME_INVALID"),
            ("browser_act", "NAME_RESERVED"),
            ("Browser-Act", "NAME_RESERVED"),
            ("fs_write", "NAME_RESERVED"),
            ("git-push", "NAME_RESERVED"),
            ("external_call", "NAME_RESERVED"),
            ("browser", "NAME_RESERVED"),
        ] {
            assert_eq!(refused_name(bad).map(|r| r.0), Some(code), "{bad}");
        }
        assert_eq!(
            refused_name(&"a".repeat(65)).map(|r| r.0),
            Some("NAME_INVALID")
        );
    }

    #[test]
    fn clip_never_splits_a_character() {
        assert_eq!(clip("héllo", 2), "hé…");
        assert_eq!(clip("abc", 3), "abc");
    }
}
