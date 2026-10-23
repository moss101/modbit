//! Accessibility trees as text (docs/66 CUC-B01) and the secure-field rules
//! that keep a secret out of what the model, the log and a frame carry
//! (CUC-B06, CUC-F01).
//!
//! A tree is rendered one element per line: its id (qualified by the
//! snapshot it came from), role, name, value, whether it takes a value and
//! the actions it offers. A secure or password-like element never shows a
//! value, whatever the actuator sent; text that looks like a credential is
//! redacted by the caller's redactor before it is rendered. Everything an
//! application says about itself is untrusted data, never instruction.

use std::sync::LazyLock;

use regex::Regex;

use crate::model::Node;

/// What every application-derived string is labelled.
pub const PROVENANCE: &str = "UNTRUSTED_APPLICATION_CONTENT";

/// The most characters of one name or value a line carries.
const FIELD_MAX: usize = 200;

static PASSWORD_LIKE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)\b(pass(word|code|phrase)?|pwd|pin|cvv|cvc|otp|secret|api[ _-]?key|token|ssn|social security)\b",
    )
    .expect("static pattern")
});

const INPUT_ROLES: &[&str] = &[
    "textfield",
    "securetextfield",
    "textarea",
    "text field",
    "text",
    "edit",
    "input",
    "combobox",
    "searchfield",
    "passwordfield",
    "password",
];

/// Whether an element is secure: flagged by the actuator, a secure role, or
/// an input whose own label says it takes a password-like value.
#[must_use]
pub fn is_secure(n: &Node) -> bool {
    if n.secure {
        return true;
    }
    let role = n.role.to_ascii_lowercase();
    if role.contains("secure") || role.contains("password") {
        return true;
    }
    INPUT_ROLES.contains(&role.as_str()) && PASSWORD_LIKE.is_match(&n.name)
}

fn clip(s: &str) -> String {
    let one_line: String = s
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    if one_line.chars().count() > FIELD_MAX {
        let mut t: String = one_line.chars().take(FIELD_MAX).collect();
        t.push('…');
        t
    } else {
        one_line
    }
}

fn quoted(s: &str) -> String {
    format!("\"{}\"", clip(s).replace('"', "'"))
}

static LOGIN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(sign[ -]?in|log[ -]?in|authenticate|sign[ -]?on)\b")
        .expect("static pattern")
});
static PASSKEY: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(passkey|security key|touch id|face id|hardware key|two[- ]factor|2fa|verification code)\b")
        .expect("static pattern")
});
static CAPTCHA: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(captcha|not a robot|verify (that )?you are (a )?human|prove you are human)")
        .expect("static pattern")
});
static PERMISSION: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(would like to (access|use|send)|wants to (access|use)|allow .{0,40}(access|permission)|grant (access|permission)|permission request)")
        .expect("static pattern")
});
static DESTRUCTIVE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(are you sure|cannot be undone|can't be undone|delete permanently|erase|remove permanently|discard (all )?changes|this will delete)")
        .expect("static pattern")
});

/// The things an agent reports and does not improvise around (docs/66
/// CUC-D06): a login, a passkey or second-factor prompt, a captcha, a
/// permission prompt and a destructive confirmation. Detected from the
/// application's own labels - a heuristic that adds a warning, never one that
/// takes an action - and returned as `{kind, evidence}`.
#[must_use]
pub fn detect_blockers(nodes: &[Node], modal: bool) -> Vec<serde_json::Value> {
    let mut out: Vec<serde_json::Value> = Vec::new();
    let mut add = |kind: &str, evidence: &str| {
        if !out.iter().any(|o| o["kind"] == kind) {
            out.push(serde_json::json!({"kind": kind, "evidence": clip(evidence)}));
        }
    };
    let has_secure = nodes.iter().find(|n| is_secure(n));
    for n in nodes {
        let text = format!(
            "{} {}",
            n.name,
            if is_secure(n) { "" } else { n.value.as_str() }
        );
        if PASSKEY.is_match(&text) {
            add("passkey", &n.name);
        }
        if CAPTCHA.is_match(&text) {
            add("captcha", &n.name);
        }
        if PERMISSION.is_match(&text) {
            add("permission_prompt", &n.name);
        }
        if (modal || n.role.eq_ignore_ascii_case("alert") || n.role.eq_ignore_ascii_case("dialog"))
            && DESTRUCTIVE.is_match(&text)
        {
            add("destructive_confirmation", &n.name);
        }
        if has_secure.is_some() && LOGIN.is_match(&n.name) {
            add("login", &n.name);
        }
    }
    if let Some(p) = has_secure {
        add("login", &format!("a secure field `{}`", p.name));
    }
    out
}

/// A rendered tree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rendered {
    /// One element per line.
    pub text: String,
    /// Elements rendered.
    pub count: usize,
    /// Ids of the secure elements (their values were withheld).
    pub secure: Vec<String>,
    /// Values the redactor changed.
    pub redacted: usize,
}

/// Render `nodes` read in `snapshot`. `redact` removes credential shapes from
/// a label or value.
#[must_use]
pub fn render(nodes: &[Node], snapshot: &str, redact: &dyn Fn(&str) -> String) -> Rendered {
    let mut lines = Vec::with_capacity(nodes.len());
    let mut secure = Vec::new();
    let mut redacted = 0usize;
    let mut depth: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for n in nodes {
        let d = if n.parent.is_empty() {
            0
        } else {
            depth.get(n.parent.as_str()).copied().unwrap_or(0) + 1
        };
        depth.insert(n.id.as_str(), d);
        let is_sec = is_secure(n);
        if is_sec {
            secure.push(n.id.clone());
        }
        let name = redact(&n.name);
        if name != n.name {
            redacted += 1;
        }
        let value = if is_sec {
            String::new()
        } else {
            let v = redact(&n.value);
            if v != n.value {
                redacted += 1;
            }
            v
        };
        let mut flags: Vec<&str> = Vec::new();
        if n.settable && !is_sec {
            flags.push("settable");
        }
        if is_sec {
            flags.push("secure");
        }
        if !n.enabled {
            flags.push("disabled");
        }
        if n.focused {
            flags.push("focused");
        }
        let mut line = format!(
            "{}{snapshot}/{} | {} | {} | value={}",
            "  ".repeat(d.min(8)),
            n.id,
            clip(&n.role),
            quoted(&name),
            if is_sec {
                "(withheld)".to_owned()
            } else {
                quoted(&value)
            }
        );
        if !flags.is_empty() {
            line.push_str(" | ");
            line.push_str(&flags.join(","));
        }
        if !n.actions.is_empty() {
            line.push_str(" | actions=");
            line.push_str(
                &n.actions
                    .iter()
                    .map(|a| clip(a))
                    .collect::<Vec<_>>()
                    .join(","),
            );
        }
        lines.push(line);
    }
    Rendered {
        text: lines.join("\n"),
        count: nodes.len(),
        secure,
        redacted,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Rect;

    fn node(id: &str, parent: &str, role: &str, name: &str, value: &str) -> Node {
        Node {
            id: id.into(),
            parent: parent.into(),
            role: role.into(),
            name: name.into(),
            value: value.into(),
            enabled: true,
            bounds: Rect::default(),
            ..Node::default()
        }
    }

    #[test]
    fn secure_and_password_like_inputs_never_show_a_value() {
        let mut flagged = node("e2", "e1", "textfield", "Account", "hunter2");
        flagged.secure = true;
        let by_role = node("e3", "e1", "SecureTextField", "x", "hunter2");
        let by_label = node("e4", "e1", "textfield", "Password", "hunter2");
        let plain = node("e5", "e1", "textfield", "Name", "alice");
        let button = node("e6", "e1", "button", "Reset password", "");
        let nodes = vec![
            node("e1", "", "window", "Login", ""),
            flagged,
            by_role,
            by_label,
            plain,
            button,
        ];
        let r = render(&nodes, "s1", &|s| s.to_owned());
        assert!(!r.text.contains("hunter2"), "{}", r.text);
        assert!(r.text.contains("value=\"alice\""));
        assert_eq!(r.secure, vec!["e2", "e3", "e4"]);
        // A button whose label mentions a password is not an input.
        assert!(!is_secure(&nodes[5]));
        assert!(
            r.text.lines().nth(1).unwrap().starts_with("  s1/e2"),
            "children indent"
        );
    }

    #[test]
    fn blockers_are_found_in_the_applications_own_labels() {
        let mut pw = node("e3", "e1", "SecureTextField", "Password", "");
        pw.secure = true;
        let login = vec![
            node("e1", "", "window", "Sign in", ""),
            node("e2", "e1", "textfield", "Username", "alice"),
            pw,
            node("e4", "e1", "button", "Sign in", ""),
        ];
        let kinds = |v: Vec<serde_json::Value>| -> Vec<String> {
            v.iter()
                .map(|b| b["kind"].as_str().unwrap().to_owned())
                .collect()
        };
        assert_eq!(kinds(detect_blockers(&login, false)), vec!["login"]);
        let other = vec![
            node(
                "a",
                "",
                "label",
                "Enter the verification code from your passkey",
                "",
            ),
            node("b", "", "label", "Please complete the CAPTCHA", ""),
            node(
                "c",
                "",
                "label",
                "\"App\" would like to access your Documents folder",
                "",
            ),
            node("d", "", "label", "Are you sure? This cannot be undone.", ""),
        ];
        let got = kinds(detect_blockers(&other, true));
        for k in [
            "passkey",
            "captcha",
            "permission_prompt",
            "destructive_confirmation",
        ] {
            assert!(got.contains(&k.to_owned()), "{k} in {got:?}");
        }
        assert!(detect_blockers(&[node("n", "", "button", "Submit", "")], false).is_empty());
    }

    #[test]
    fn labels_and_values_pass_the_redactor_and_control_characters_do_not_break_lines() {
        let nodes = vec![node(
            "e1",
            "",
            "label",
            "key sk-live-ABCDEF\nnext line",
            "tok sk-live-ABCDEF",
        )];
        let r = render(&nodes, "s9", &|s| s.replace("sk-live-ABCDEF", "[REDACTED]"));
        assert!(!r.text.contains("sk-live"));
        assert_eq!(r.text.lines().count(), 1, "{}", r.text);
        assert_eq!(r.redacted, 2);
    }
}
