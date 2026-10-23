//! The one redactor (REQ-EV-0017; docs/23 "Secrets": known secret
//! fingerprints are detected before persistence or display).
//!
//! Two rules, one owner:
//!
//! * a value the Core holds (a provider key, the forge token, an external
//!   server's credential) is replaced wherever it appears — in error text and
//!   in anything a tool or server returned. A credential is handed out to be
//!   used, never to come back into a log, a person's view or a model's
//!   context;
//! * error text — a message, a reason, a diagnostic, a rejection — also has
//!   every credential-*shaped* token replaced, whoever it belongs to. Data a
//!   tool read (a file, a page) is not shape-redacted: a fixture's example key
//!   is the work, not a leak.
//!
//! Held values shorter than [`MIN_HELD_LEN`] are never matched: a short value
//! would match ordinary text.

use std::sync::OnceLock;

use regex::Regex;
use serde_json::Value;

/// What replaces a secret.
pub const REDACTED: &str = "[redacted]";

/// Held values shorter than this are never matched.
pub const MIN_HELD_LEN: usize = 8;

/// The shapes credentials take: `(pattern, what it is)`.
fn shapes() -> &'static [(Regex, &'static str)] {
    static SHAPES: OnceLock<Vec<(Regex, &'static str)>> = OnceLock::new();
    SHAPES.get_or_init(|| {
        [
            (r"\bsk-[A-Za-z0-9_-]{16,}", "a provider API key"),
            (r"\b(ghp|gho|ghu|ghs|ghr)_[A-Za-z0-9]{20,}", "a forge token"),
            (r"\bgithub_pat_[A-Za-z0-9_]{20,}", "a forge token"),
            (r"\bmbw_[0-9a-f]{8,}\.[0-9a-f]{16,}", "a worker token"),
            (r"\bBearer\s+[A-Za-z0-9._~+/=-]{20,}", "a bearer credential"),
            (r"\bAKIA[0-9A-Z]{16}\b", "an AWS access key id"),
            (r"\bAIza[0-9A-Za-z_-]{35}", "a Google API key"),
            (r"\bxox[abprs]-[A-Za-z0-9-]{10,}", "a Slack token"),
            (
                r"(?i)\b(api[_-]?key|access[_-]?token|auth[_-]?token)=[A-Za-z0-9._~+/-]{16,}",
                "a credential parameter",
            ),
            (r"-----BEGIN [A-Z ]*PRIVATE KEY-----", "a private key"),
        ]
        .into_iter()
        .map(|(re, what)| (Regex::new(re).expect("a valid shape"), what))
        .collect()
    })
}

/// What kind of credential `text` carries by shape, if any.
#[must_use]
pub fn shape_of(text: &str) -> Option<&'static str> {
    shapes()
        .iter()
        .find(|(re, _)| re.is_match(text))
        .map(|(_, what)| *what)
}

/// How far back from the end of a stream of text a credential still being
/// received is looked for.
pub const TAIL_WINDOW: usize = 256;

/// The literal starts of credential shapes: text that is a proper prefix of
/// one of these may be the start of one.
const GROWTH_PREFIXES: &[&str] = &[
    "sk-",
    "ghp_",
    "gho_",
    "ghu_",
    "ghs_",
    "ghr_",
    "github_pat_",
    "mbw_",
    "Bearer",
    "AKIA",
    "AIza",
    "xoxa-",
    "xoxb-",
    "xoxp-",
    "xoxr-",
    "xoxs-",
    "-----BEGIN",
];

/// Names a credential parameter starts with (matched case-insensitively).
const PARAMETER_NAMES: &[&str] = &[
    "api_key",
    "api-key",
    "apikey",
    "access_token",
    "access-token",
    "accesstoken",
    "auth_token",
    "auth-token",
    "authtoken",
];

/// Credential shapes that are anchored at both ends: a tail that matches one
/// is a credential that may still be growing.
fn growing() -> &'static [Regex] {
    static GROWING: OnceLock<Vec<Regex>> = OnceLock::new();
    GROWING.get_or_init(|| {
        [
            r"^sk-[A-Za-z0-9_-]*$",
            r"^(?:ghp|gho|ghu|ghs|ghr)_[A-Za-z0-9]*$",
            r"^github_pat_[A-Za-z0-9_]*$",
            r"^mbw_[0-9a-f]*(?:\.[0-9a-f]*)?$",
            r"^Bearer\s+[A-Za-z0-9._~+/=-]*$",
            r"^AKIA[0-9A-Z]*$",
            r"^AIza[0-9A-Za-z_-]*$",
            r"^xox[abprs]-[A-Za-z0-9-]*$",
            r"(?i)^(?:api[_-]?key|access[_-]?token|auth[_-]?token)=?[A-Za-z0-9._~+/-]*$",
            r"^-----BEGIN[A-Z -]*$",
        ]
        .into_iter()
        .map(|re| Regex::new(re).expect("a valid growth shape"))
        .collect()
    })
}

/// Redacted text and what was replaced.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Redacted {
    /// The text with every secret replaced by [`REDACTED`].
    pub text: String,
    /// Occurrences of held values replaced.
    pub held: usize,
    /// Credential-shaped tokens replaced.
    pub shaped: usize,
}

impl Redacted {
    /// Whether anything was replaced.
    #[must_use]
    pub fn changed(&self) -> bool {
        self.held + self.shaped > 0
    }
}

/// Keys whose values are error text in an event payload.
const ERROR_KEYS: &[&str] = &[
    "reason",
    "detail",
    "message",
    "error",
    "error_message",
    "fallback_reason",
    "diagnostic",
];

/// Events sealed by a hash over their own fields (an effect receipt): never
/// rewritten after sealing, so never here.
const SEALED: &[&str] = &["EffectReceiptAppended"];

/// The values a Core holds, longest first so a value containing another is
/// replaced whole.
#[derive(Clone, Debug, Default)]
pub struct Redactor {
    held: Vec<String>,
}

impl Redactor {
    /// A redactor for these held values (shorter than [`MIN_HELD_LEN`] ignored).
    pub fn new<I, S>(held: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let mut held: Vec<String> = held
            .into_iter()
            .map(Into::into)
            .filter(|s| s.len() >= MIN_HELD_LEN)
            .collect();
        held.sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| a.cmp(b)));
        held.dedup();
        Self { held }
    }

    /// Whether `text` contains a held value.
    #[must_use]
    pub fn holds_in(&self, text: &str) -> bool {
        self.held.iter().any(|s| text.contains(s.as_str()))
    }

    /// Data a tool or server returned: held values only.
    #[must_use]
    pub fn data(&self, text: &str) -> Redacted {
        let mut out = text.to_owned();
        let mut held = 0;
        for s in &self.held {
            if out.contains(s.as_str()) {
                held += out.matches(s.as_str()).count();
                out = out.replace(s.as_str(), REDACTED);
            }
        }
        Redacted {
            text: out,
            held,
            shaped: 0,
        }
    }

    /// Error text (a message, a reason, a diagnostic, a rejection): held
    /// values and every credential-shaped token.
    #[must_use]
    pub fn error(&self, text: &str) -> Redacted {
        let mut r = self.data(text);
        for (re, _) in shapes() {
            let n = re.find_iter(&r.text).count();
            if n > 0 {
                r.shaped += n;
                r.text = re.replace_all(&r.text, REDACTED).into_owned();
            }
        }
        r
    }

    /// [`Self::error`], the text only.
    #[must_use]
    pub fn error_text(&self, text: &str) -> String {
        self.error(text).text
    }

    /// How many bytes at the end of `raw` could still turn into a secret
    /// once more text arrives: a partial held value, or a credential-shaped
    /// token that is still growing. A stream of text redacts what it has
    /// before it publishes and keeps this tail back, so a secret split
    /// across two chunks is replaced whole rather than leaking in halves
    /// (PX-041). Only the last [`TAIL_WINDOW`] bytes are considered; a
    /// credential longer than that is outside what streaming can hold back.
    #[must_use]
    pub fn pending_tail(&self, raw: &str) -> usize {
        let from = raw.len().saturating_sub(TAIL_WINDOW);
        for (i, _) in raw.char_indices().filter(|(i, _)| *i >= from) {
            if self.could_grow(&raw[i..]) {
                return raw.len() - i;
            }
        }
        0
    }

    fn could_grow(&self, tail: &str) -> bool {
        if self
            .held
            .iter()
            .any(|h| h.len() > tail.len() && h.starts_with(tail))
        {
            return true;
        }
        if GROWTH_PREFIXES
            .iter()
            .any(|l| l.len() > tail.len() && l.starts_with(tail))
        {
            return true;
        }
        let lower = tail.to_ascii_lowercase();
        if PARAMETER_NAMES
            .iter()
            .any(|n| n.starts_with(lower.as_str()) && n.len() > lower.len())
        {
            return true;
        }
        growing().iter().any(|re| re.is_match(tail))
    }

    /// Every string in `v` redacted as data; returns the replacements.
    pub fn data_json(&self, v: &mut Value) -> usize {
        self.walk(v, false)
    }

    /// Every string in `v` redacted as error text; returns the replacements.
    pub fn error_json(&self, v: &mut Value) -> usize {
        self.walk(v, true)
    }

    /// An event payload before it is persisted: every held value anywhere,
    /// and every credential shape under an error-text key (`reason`,
    /// `detail`, `message`, `error`, a `diagnostic`). Returns the
    /// replacements.
    pub fn event_payload(&self, event_type: &str, payload: &mut Value) -> usize {
        if SEALED.contains(&event_type) {
            return 0;
        }
        self.walk_keyed(payload, false)
    }

    fn walk_keyed(&self, v: &mut Value, error: bool) -> usize {
        match v {
            Value::Object(o) => o
                .iter_mut()
                .map(|(k, x)| self.walk_keyed(x, error || ERROR_KEYS.contains(&k.as_str())))
                .sum(),
            Value::Array(a) => a.iter_mut().map(|x| self.walk_keyed(x, error)).sum(),
            Value::String(_) => self.walk(v, error),
            _ => 0,
        }
    }

    fn walk(&self, v: &mut Value, error: bool) -> usize {
        match v {
            Value::String(s) => {
                let r = if error { self.error(s) } else { self.data(s) };
                let n = r.held + r.shaped;
                if n > 0 {
                    *s = r.text;
                }
                n
            }
            Value::Array(a) => a.iter_mut().map(|x| self.walk(x, error)).sum(),
            Value::Object(o) => o.values_mut().map(|x| self.walk(x, error)).sum(),
            _ => 0,
        }
    }
}

/// Error text with credential-shaped tokens replaced, for code that holds no
/// values of its own (a classifier, a transport error).
#[must_use]
pub fn error_text(text: &str) -> String {
    Redactor::default().error_text(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &str = "sk-live-0123456789abcdefghij";

    #[test]
    fn a_held_value_is_replaced_wherever_it_appears_and_counted() {
        let r = Redactor::new([KEY, "short"]);
        let out = r.data(&format!("key {KEY}, again \"{KEY}\"; short stays"));
        assert_eq!(
            out.text, "key [redacted], again \"[redacted]\"; short stays",
            "a quoted value is replaced too"
        );
        assert_eq!((out.held, out.shaped), (2, 0));
        assert!(r.holds_in(KEY) && !r.holds_in("nothing"));
    }

    #[test]
    fn a_longer_held_value_is_replaced_whole() {
        let r = Redactor::new(["abcdefgh", "abcdefgh-ijklmnop"]);
        assert_eq!(r.data("x abcdefgh-ijklmnop y").text, "x [redacted] y");
    }

    #[test]
    fn error_text_also_loses_every_credential_shape_but_data_keeps_them() {
        let r = Redactor::default();
        for (text, what) in [
            (
                format!("Incorrect API key provided: \"{KEY}\""),
                "a provider API key",
            ),
            (
                "auth: Bearer abcdefghijklmnopqrstuvwxyz0123".to_owned(),
                "a bearer credential",
            ),
            (
                "GET /v1?api_key=abcdefghijklmnop1234 failed".to_owned(),
                "a credential parameter",
            ),
            (
                "token ghp_abcdefghijklmnopqrstuvwxyz0123 refused".to_owned(),
                "a forge token",
            ),
        ] {
            assert_eq!(shape_of(&text), Some(what), "{text}");
            let e = r.error(&text);
            assert!(e.shaped >= 1 && e.text.contains(REDACTED), "{e:?}");
            assert_eq!(shape_of(&e.text), None, "{e:?}");
            assert_eq!(r.data(&text).text, text, "data is not shape-redacted");
        }
        assert_eq!(shape_of("an ordinary sentence about keys"), None);
    }

    #[test]
    fn an_event_payload_loses_held_values_everywhere_and_shapes_only_in_error_text() {
        let r = Redactor::new([KEY]);
        let shaped = "ghp_abcdefghijklmnopqrstuvwxyz0123";
        let mut p = serde_json::json!({
            "reason": format!("provider failure AUTH_REJECTED: HTTP 401: {shaped}"),
            "diagnostic": {"code": "AUTH_REJECTED", "detail": format!("key {KEY}")},
            "summary": format!("a note that quotes {shaped} and {KEY}"),
        });
        assert_eq!(r.event_payload("TaskNeedsAttention", &mut p), 3);
        let text = p.to_string();
        assert!(!text.contains(KEY), "{text}");
        assert!(!p["reason"].as_str().unwrap().contains(shaped), "{p}");
        assert!(
            p["summary"].as_str().unwrap().contains(shaped),
            "a shape outside error text is data: {p}"
        );
        let mut sealed = serde_json::json!({"receipt": {"detail": KEY}});
        assert_eq!(r.event_payload("EffectReceiptAppended", &mut sealed), 0);
    }

    #[test]
    fn a_tail_that_may_still_become_a_secret_is_held_back() {
        let r = Redactor::new([KEY]);
        // a partial held value, a growing token, a lone prefix, prose
        assert_eq!(
            r.pending_tail("the key is sk-live-0123"),
            "sk-live-0123".len()
        );
        assert_eq!(
            r.pending_tail("use sk-abcdefghijklmnopqrstu"),
            "sk-abcdefghijklmnopqrstu".len()
        );
        assert_eq!(
            r.pending_tail("then Bearer abcdefghijklmnopqrstuvwxyz"),
            "Bearer abcdefghijklmnopqrstuvwxyz".len()
        );
        assert_eq!(r.pending_tail("and Bear"), 4);
        assert_eq!(r.pending_tail("set API_KEY=abcdef"), "API_KEY=abcdef".len());
        // prose holds at most the one letter that could start a prefix
        assert_eq!(r.pending_tail("an ordinary sentence about keys"), 1);
        assert_eq!(r.pending_tail("an ordinary sentence about key"), 0);
        assert_eq!(r.pending_tail("ends with a word "), 0);
        // split anywhere, the redacted halves never carry the secret
        let text = format!("before {KEY} after");
        for cut in 0..=text.len() {
            if !text.is_char_boundary(cut) {
                continue;
            }
            let mut pending = text[..cut].to_owned();
            let held = r.pending_tail(&pending);
            let first = r.error(&pending[..pending.len() - held]).text;
            pending = pending[pending.len() - held..].to_owned();
            pending.push_str(&text[cut..]);
            let second = r.error(&pending).text;
            assert!(
                !first.contains("live") && !second.contains("live"),
                "{cut}: {first:?} {second:?}"
            );
            assert_eq!(
                format!("{first}{second}"),
                "before [redacted] after",
                "{cut}"
            );
        }
    }

    #[test]
    fn json_is_redacted_string_by_string() {
        let r = Redactor::new([KEY]);
        let mut v = serde_json::json!({"error": {"message": format!("bad key {KEY}"), "n": 3},
                                        "list": [KEY, "fine"]});
        assert_eq!(r.data_json(&mut v), 2);
        assert!(!v.to_string().contains(KEY));
        assert_eq!(v["list"][1], "fine");
        assert_eq!(v["error"]["n"], 3);
    }
}
