//! What the page did, as the agent may read it (PX-121): the console
//! messages and network requests the host recorded in bounded ring buffers,
//! and the redaction every line passes before it leaves the Core. The page's
//! console text is the page's: untrusted content, scanned for injection like
//! any other page string, and never an instruction. A secret — a URL
//! credential, a token in a query string, a bearer header copied into a log
//! line, a value the credential broker holds — is replaced before the model
//! sees it, on the host that recorded it and again here.

use serde::{Deserialize, Serialize};

/// Longest console line the Core returns.
pub const MAX_CONSOLE_TEXT: usize = 500;
/// Longest URL the Core returns.
pub const MAX_URL: usize = 300;
/// Entries one read returns at most.
pub const MAX_ENTRIES: usize = 100;

/// The replacement for a redacted span.
pub const REDACTED: &str = "[REDACTED]";

/// One console message (or uncaught exception, or browser log entry).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConsoleEntry {
    /// Position in the session's buffer; reads resume after one.
    pub seq: u64,
    /// `log` | `info` | `warning` | `error` | `debug`.
    pub level: String,
    /// `console` | `exception` | `log`.
    #[serde(default)]
    pub source: String,
    /// The message (untrusted page text).
    pub text: String,
    /// Where the page says it came from (a URL; redacted).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub url: String,
    /// Line in that URL, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    /// When the host recorded it (ms since the epoch).
    #[serde(default)]
    pub at_ms: u64,
}

/// One network request the page made.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetworkEntry {
    /// Position in the session's buffer.
    pub seq: u64,
    /// `GET`, `POST`, …
    pub method: String,
    /// The URL, credentials and secret-shaped query values redacted.
    pub url: String,
    /// HTTP status, 0 while pending or when none arrived.
    #[serde(default)]
    pub status: u32,
    /// `Document`, `Script`, `XHR`, `Fetch`, `Image`, …
    #[serde(default)]
    pub resource_type: String,
    /// Why it failed (`net::ERR_CONNECTION_REFUSED`, `blocked: TARGET_NOT_ALLOWED`), when it did.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failed: Option<String>,
    /// Milliseconds from request to finish, when finished.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    /// When the host recorded it.
    #[serde(default)]
    pub at_ms: u64,
}

/// Query-parameter names whose value is a secret.
const SECRET_PARAMS: &[&str] = &[
    "token",
    "access_token",
    "id_token",
    "refresh_token",
    "api_key",
    "apikey",
    "key",
    "secret",
    "client_secret",
    "password",
    "passwd",
    "pwd",
    "auth",
    "authorization",
    "session",
    "sessionid",
    "sid",
    "code",
    "sig",
    "signature",
    "x-amz-signature",
    "x-amz-credential",
    "jwt",
    "bearer",
    "otp",
    "csrf",
    "xsrf",
];

fn is_secret_param(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    SECRET_PARAMS.contains(&n.as_str())
        || n.ends_with("_token")
        || n.ends_with("-token")
        || n.ends_with("_secret")
        || n.ends_with("_key")
        || n.ends_with("password")
}

/// A URL for the model: the userinfo, the fragment's secret-shaped values
/// and every secret-shaped query value replaced, the length bounded.
#[must_use]
pub fn redact_url(url: &str) -> String {
    let mut s = url.trim().to_owned();
    // scheme://user:pass@host → scheme://[REDACTED]@host
    if let Some((scheme, rest)) = s.split_once("://") {
        let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
        let (authority, tail) = rest.split_at(end);
        if let Some((_, host)) = authority.rsplit_once('@') {
            s = format!("{scheme}://{REDACTED}@{host}{tail}");
        }
    }
    let redact_pairs = |part: &str| -> String {
        part.split('&')
            .map(|pair| match pair.split_once('=') {
                Some((k, v)) if is_secret_param(k) && !v.is_empty() => {
                    format!("{k}={REDACTED}")
                }
                _ => pair.to_owned(),
            })
            .collect::<Vec<_>>()
            .join("&")
    };
    let (before_fragment, fragment) = match s.split_once('#') {
        Some((a, b)) => (a.to_owned(), Some(b.to_owned())),
        None => (s.clone(), None),
    };
    let rebuilt = match before_fragment.split_once('?') {
        Some((path, query)) => format!("{path}?{}", redact_pairs(query)),
        None => before_fragment,
    };
    let rebuilt = match fragment {
        Some(f) if f.contains('=') => format!("{rebuilt}#{}", redact_pairs(&f)),
        Some(f) => format!("{rebuilt}#{f}"),
        None => rebuilt,
    };
    truncate(&rebuilt, MAX_URL)
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_owned();
    }
    let mut out: String = s.chars().take(max).collect();
    out.push('…');
    out
}

/// Whether a token has the shape of a credential.
fn looks_secret(token: &str) -> bool {
    let t = token.trim_matches(|c: char| !(c.is_ascii_alphanumeric() || "-_.+/=".contains(c)));
    if t.len() < 12 {
        return false;
    }
    let lower = t.to_ascii_lowercase();
    // Provider-shaped prefixes.
    const PREFIXES: &[&str] = &[
        "sk-",
        "sk_live",
        "sk_test",
        "pk_live",
        "ghp_",
        "gho_",
        "ghu_",
        "ghs_",
        "ghr_",
        "github_pat_",
        "xoxb-",
        "xoxp-",
        "xoxa-",
        "xoxs-",
        "glpat-",
        "ya29.",
    ];
    if PREFIXES.iter().any(|p| lower.starts_with(p)) {
        return true;
    }
    // An AWS access key id (20 upper-case alphanumerics) and a Google API key.
    if (t.starts_with("AKIA") || t.starts_with("ASIA"))
        && t.len() == 20
        && t.chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
    {
        return true;
    }
    if t.starts_with("AIza") && t.len() == 39 {
        return true;
    }
    // A JWT: three dot-separated base64url parts, the first starting `eyJ`.
    if t.starts_with("eyJ") && t.matches('.').count() == 2 {
        return true;
    }
    // A long opaque run mixing letters and digits (a session id, an API key).
    if t.len() >= 32 {
        let digits = t.chars().filter(char::is_ascii_digit).count();
        let letters = t.chars().filter(char::is_ascii_alphabetic).count();
        let spaced = t.contains(' ');
        if !spaced && digits >= 4 && letters >= 8 && !t.contains('/') && !t.contains('.') {
            return true;
        }
        // A long hex digest.
        if t.len() >= 40 && t.chars().all(|c| c.is_ascii_hexdigit()) {
            return true;
        }
    }
    false
}

/// A console line for the model: bounded; every `Bearer`/`Basic` credential,
/// `key=value` pair with a secret name, secret-shaped token and URL is
/// redacted; each of `secrets` (values the broker holds) is removed.
#[must_use]
pub fn redact_text(text: &str, secrets: &[String]) -> String {
    let mut out = text.to_owned();
    for s in secrets.iter().filter(|s| s.len() >= 4) {
        out = out.replace(s.as_str(), REDACTED);
    }
    let mut words: Vec<String> = Vec::new();
    let mut redact_next = false;
    for raw in out.split(' ') {
        let w = raw.to_owned();
        if redact_next && !w.is_empty() {
            redact_next = false;
            words.push(REDACTED.to_owned());
            continue;
        }
        let lower = w.to_ascii_lowercase();
        if lower == "bearer" || lower == "basic" || lower == "token:" || lower == "authorization:" {
            redact_next = true;
            words.push(w);
            continue;
        }
        if w.contains("://") {
            // A URL inside a line: userinfo and query secrets.
            let (lead, url) = match w.find("http") {
                Some(i) => (w[..i].to_owned(), w[i..].to_owned()),
                None => (String::new(), w.clone()),
            };
            words.push(format!("{lead}{}", redact_url(&url)));
            continue;
        }
        if let Some((k, v)) = w.split_once('=')
            && is_secret_param(
                k.trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '_' && c != '-'),
            )
            && !v.is_empty()
        {
            words.push(format!("{k}={REDACTED}"));
            continue;
        }
        if let Some((k, v)) = w.split_once(':')
            && is_secret_param(
                k.trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '_' && c != '-'),
            )
            && !v.is_empty()
            && !v.starts_with("//")
        {
            words.push(format!("{k}:{REDACTED}"));
            continue;
        }
        if looks_secret(&w) {
            words.push(REDACTED.to_owned());
            continue;
        }
        words.push(w);
    }
    truncate(&words.join(" "), MAX_CONSOLE_TEXT)
}

impl ConsoleEntry {
    /// The entry as the Core returns it: text and URL redacted and bounded.
    #[must_use]
    pub fn redacted(mut self, secrets: &[String]) -> Self {
        self.text = redact_text(&self.text, secrets);
        self.url = redact_url(&self.url);
        self
    }
}

impl NetworkEntry {
    /// The entry as the Core returns it.
    #[must_use]
    pub fn redacted(mut self, secrets: &[String]) -> Self {
        self.url = redact_text(&redact_url(&self.url), secrets);
        if let Some(f) = self.failed.take() {
            self.failed = Some(redact_text(&f, secrets));
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls_lose_credentials_and_secret_query_values_but_keep_their_shape() {
        assert_eq!(
            redact_url(
                "https://ada:hunter2@api.test/v1/items?id=7&token=abc123&page=2#access_token=zzz&x=1"
            ),
            "https://[REDACTED]@api.test/v1/items?id=7&token=[REDACTED]&page=2#access_token=[REDACTED]&x=1"
        );
        assert_eq!(
            redact_url("http://127.0.0.1:3000/a?q=hello"),
            "http://127.0.0.1:3000/a?q=hello"
        );
        assert!(
            redact_url(&format!("http://x.test/{}", "a".repeat(900)))
                .chars()
                .count()
                <= MAX_URL + 1
        );
    }

    #[test]
    fn console_lines_lose_bearer_tokens_secret_shapes_and_custody_values() {
        let line = "fetch failed Authorization: Bearer eyJhbGciOi.payload.sig for https://u:p@x.test/a?api_key=K1 key=sk-live-1234567890abcdef and BROKER-VALUE-9 too";
        let r = redact_text(line, &["BROKER-VALUE-9".to_owned()]);
        assert!(!r.contains("eyJhbGciOi"), "{r}");
        assert!(!r.contains("sk-live"), "{r}");
        assert!(!r.contains("BROKER-VALUE-9"), "{r}");
        assert!(!r.contains("u:p@"), "{r}");
        assert!(!r.contains("api_key=K1"), "{r}");
        assert!(r.contains("fetch failed"), "{r}");
        // Plain prose is untouched.
        assert_eq!(
            redact_text("Uncaught TypeError: x is not a function", &[]),
            "Uncaught TypeError: x is not a function"
        );
        assert!(redact_text(&"y ".repeat(600), &[]).chars().count() <= MAX_CONSOLE_TEXT + 1);
    }

    #[test]
    fn an_entry_redacts_every_field_that_can_carry_a_secret() {
        let e = ConsoleEntry {
            seq: 1,
            level: "error".into(),
            source: "console".into(),
            text: "token=abcdefabcdef".into(),
            url: "https://a:b@x.test/?sid=1".into(),
            ..Default::default()
        }
        .redacted(&[]);
        assert_eq!(e.text, "token=[REDACTED]");
        assert!(e.url.contains("[REDACTED]@x.test") && e.url.contains("sid=[REDACTED]"));
        let n = NetworkEntry {
            url: "https://x.test/cb?code=42&ok=1".into(),
            failed: Some("blocked secret=zz9".into()),
            ..Default::default()
        }
        .redacted(&[]);
        assert_eq!(n.url, "https://x.test/cb?code=[REDACTED]&ok=1");
        assert_eq!(n.failed.as_deref(), Some("blocked secret=[REDACTED]"));
    }
}
