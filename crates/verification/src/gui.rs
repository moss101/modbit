//! GUI task completion evidence and the failure judge (REQ-PX-178,
//! DR-PX-2026-10-26-018 D1/D2).
//!
//! A GUI task cannot be verified by a test suite: whether the order was
//! submitted is a fact of the application, not of the repository. So the one
//! who sets a GUI task (the parent that spawns a computer-use child) declares
//! the evidence its completion must show, and the Core reads that evidence
//! itself when the task asks to complete:
//!
//! * an **accessibility** predicate: an element of the declared application,
//!   found by role and name in a state read the Core makes, holds a value;
//! * a **file** predicate: a workspace file contains a text;
//! * a **recorded** outcome: an event of the task's own log carries a field
//!   value (for example a `ComputerActionPerformed` that was verified).
//!
//! The predicate is evidence, never authority: a pass lets the completion
//! run go on to its other gates, a fail or missing evidence blocks it, and
//! nothing here can approve an effect or resolve an approval. The model's own
//! words (its summary, its claims) are never read.
//!
//! When the run ends, the judge classifies a GUI task's outcome from the
//! predicate and from what the log shows happened: `VERIFIED` when the
//! evidence holds; otherwise `ENVIRONMENT_FAILURE` when the actuator, the
//! application or the session failed under the run, `POLICY_STOP` when a
//! person or policy refused an effect, and `MODEL_FAILURE` when nothing
//! outside the model stood in its way and the evidence still does not hold.
//! The verdict is recorded with its basis.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Longest text a predicate may name, in characters.
pub const FIELD_MAX: usize = 512;

/// What a GUI task's completion must show, as its setter declared it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum CompletionEvidence {
    /// An element of the application's accessibility tree, read by the Core.
    Accessibility {
        /// The application's bundle identifier; a state read of another
        /// application is no evidence.
        app: String,
        /// The element's role (`label`, `button`, ...), when it narrows.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        role: Option<String>,
        /// The element's accessible name, exactly.
        name: String,
        /// The value it must hold, exactly.
        value: String,
    },
    /// A workspace file and a text it must contain.
    File {
        /// Workspace-relative path.
        path: String,
        /// Text the file must contain.
        contains: String,
    },
    /// A recorded outcome on the task's own log.
    Recorded {
        /// The event type (`ComputerActionPerformed`, ...).
        event_type: String,
        /// A top-level field of its payload.
        field: String,
        /// The value it must hold (compared as text).
        equals: String,
    },
}

impl CompletionEvidence {
    /// Refuse a predicate that cannot be evaluated as declared.
    ///
    /// # Errors
    /// A text naming what is wrong.
    pub fn validate(&self) -> Result<(), String> {
        let check = |what: &str, v: &str| {
            if v.trim().is_empty() {
                Err(format!("{what} must not be empty"))
            } else if v.chars().count() > FIELD_MAX {
                Err(format!("{what} is longer than {FIELD_MAX} characters"))
            } else {
                Ok(())
            }
        };
        match self {
            Self::Accessibility {
                app, role, name, ..
            } => {
                check("app", app)?;
                check("name", name)?;
                if let Some(r) = role {
                    check("role", r)?;
                }
                Ok(())
            }
            Self::File { path, contains } => {
                check("path", path)?;
                check("contains", contains)?;
                let p = std::path::Path::new(path);
                // Rooted on any platform: a leading separator (`/etc`, `\\server`), a Windows
                // prefix or a drive (`C:`) is never workspace-relative, whatever this host's
                // own notion of absolute is.
                let rooted = p.is_absolute()
                    || path.starts_with(['/', '\\'])
                    || path.as_bytes().get(1) == Some(&b':')
                    || p.components().any(|c| {
                        matches!(
                            c,
                            std::path::Component::Prefix(_) | std::path::Component::RootDir
                        )
                    });
                if rooted
                    || path.split(['/', '\\']).any(|seg| seg == "..")
                    || p.components()
                        .any(|c| matches!(c, std::path::Component::ParentDir))
                {
                    return Err("path must be workspace-relative, without `..`".into());
                }
                Ok(())
            }
            Self::Recorded {
                event_type, field, ..
            } => {
                check("event_type", event_type)?;
                check("field", field)
            }
        }
    }

    /// One line saying what the evidence is, for the model and the log.
    #[must_use]
    pub fn describe(&self) -> String {
        match self {
            Self::Accessibility {
                app,
                role,
                name,
                value,
            } => format!(
                "the {}\"{name}\" of {app} reads \"{value}\"",
                role.as_deref().map(|r| format!("{r} ")).unwrap_or_default()
            ),
            Self::File { path, contains } => format!("{path} contains \"{contains}\""),
            Self::Recorded {
                event_type,
                field,
                equals,
            } => format!("a {event_type} on the task's log has {field} = \"{equals}\""),
        }
    }
}

/// How a predicate came out.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PredicateStatus {
    /// The evidence holds.
    Pass,
    /// The evidence was read and does not hold.
    Fail,
    /// The evidence could not be read (no state read of the declared
    /// application, the file is absent, the read was refused).
    Missing,
}

impl PredicateStatus {
    /// The wire word.
    #[must_use]
    pub fn word(self) -> &'static str {
        match self {
            Self::Pass => "PASS",
            Self::Fail => "FAIL",
            Self::Missing => "MISSING",
        }
    }
}

/// A predicate's result: its status, what was found, and what was read.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PredicateResult {
    /// Pass, fail or missing.
    pub status: PredicateStatus,
    /// What was found, in words.
    pub detail: String,
    /// What was read to decide (the application and snapshot, the file, the
    /// events) - and nothing else was.
    pub read: String,
}

impl PredicateResult {
    fn of(status: PredicateStatus, detail: impl Into<String>, read: impl Into<String>) -> Self {
        Self {
            status,
            detail: detail.into(),
            read: read.into(),
        }
    }

    /// Evidence that could not be read.
    #[must_use]
    pub fn missing(detail: impl Into<String>, read: impl Into<String>) -> Self {
        Self::of(PredicateStatus::Missing, detail, read)
    }
}

/// One element of a state read: its role, its name and its value (`None`
/// when the value was withheld as secure).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ElementFact {
    /// Role.
    pub role: String,
    /// Accessible name.
    pub name: String,
    /// Value; `None` when withheld.
    pub value: Option<String>,
}

/// Evaluate an accessibility predicate against a state read the Core made of
/// `bundle_id` (snapshot `snapshot`). A read of another application is
/// missing evidence: the predicate reads only the application it declared.
#[must_use]
pub fn eval_accessibility(
    pred: &CompletionEvidence,
    bundle_id: &str,
    snapshot: &str,
    elements: &[ElementFact],
) -> PredicateResult {
    let CompletionEvidence::Accessibility {
        app,
        role,
        name,
        value,
    } = pred
    else {
        return PredicateResult::missing("not an accessibility predicate", "");
    };
    let read = format!("state read of {bundle_id} (snapshot {snapshot})");
    if bundle_id != app {
        return PredicateResult::missing(
            format!("the open control session shows {bundle_id}, not the declared {app}"),
            format!("nothing: the read of {bundle_id} was not used"),
        );
    }
    let candidates: Vec<&ElementFact> = elements
        .iter()
        .filter(|e| e.name == *name && role.as_ref().is_none_or(|r| e.role == *r))
        .collect();
    match candidates.as_slice() {
        [] => PredicateResult::of(
            PredicateStatus::Fail,
            format!("no element named \"{name}\" in {app}"),
            read,
        ),
        [one] => match &one.value {
            None => PredicateResult::missing(
                format!("\"{name}\" is a secure element; its value is withheld"),
                read,
            ),
            Some(v) if v == value => PredicateResult::of(
                PredicateStatus::Pass,
                format!("\"{name}\" reads \"{v}\""),
                read,
            ),
            Some(v) => PredicateResult::of(
                PredicateStatus::Fail,
                format!("\"{name}\" reads \"{v}\", not \"{value}\""),
                read,
            ),
        },
        many => PredicateResult::missing(
            format!(
                "{} elements are named \"{name}\"; name a role that tells them apart",
                many.len()
            ),
            read,
        ),
    }
}

/// Evaluate a file predicate against the file's text (`None` when the file
/// is absent or unreadable).
#[must_use]
pub fn eval_file(pred: &CompletionEvidence, text: Option<&str>) -> PredicateResult {
    let CompletionEvidence::File { path, contains } = pred else {
        return PredicateResult::missing("not a file predicate", "");
    };
    let read = format!("workspace file {path}");
    match text {
        None => PredicateResult::missing(format!("{path} is absent or unreadable"), read),
        Some(t) if t.contains(contains.as_str()) => PredicateResult::of(
            PredicateStatus::Pass,
            format!("{path} contains \"{contains}\""),
            read,
        ),
        Some(_) => PredicateResult::of(
            PredicateStatus::Fail,
            format!("{path} does not contain \"{contains}\""),
            read,
        ),
    }
}

/// Evaluate a recorded-outcome predicate against the task's own events
/// (payloads carrying their type under `__type`).
#[must_use]
pub fn eval_recorded(pred: &CompletionEvidence, events: &[Value]) -> PredicateResult {
    let CompletionEvidence::Recorded {
        event_type,
        field,
        equals,
    } = pred
    else {
        return PredicateResult::missing("not a recorded-outcome predicate", "");
    };
    let of_type: Vec<&Value> = events
        .iter()
        .filter(|e| e["__type"].as_str() == Some(event_type.as_str()))
        .collect();
    let read = format!("{} {event_type} event(s) of the task", of_type.len());
    let text = |v: &Value| match v {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    };
    if of_type.iter().any(|e| text(&e[field.as_str()]) == *equals) {
        PredicateResult::of(
            PredicateStatus::Pass,
            format!("a {event_type} has {field} = \"{equals}\""),
            read,
        )
    } else if of_type.is_empty() {
        PredicateResult::of(
            PredicateStatus::Fail,
            format!("the task's log holds no {event_type}"),
            read,
        )
    } else {
        PredicateResult::of(
            PredicateStatus::Fail,
            format!("no {event_type} has {field} = \"{equals}\""),
            read,
        )
    }
}

/// The judge's verdict on a GUI task's run.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum GuiVerdict {
    /// The declared evidence holds.
    Verified,
    /// The evidence does not hold and nothing outside the model stood in its
    /// way: the model did the wrong thing, or not enough.
    ModelFailure,
    /// The actuator, the application, the window or the control session
    /// failed under the run.
    EnvironmentFailure,
    /// A person or policy refused an effect the task needed.
    PolicyStop,
}

impl GuiVerdict {
    /// The wire word.
    #[must_use]
    pub fn word(self) -> &'static str {
        match self {
            Self::Verified => "VERIFIED",
            Self::ModelFailure => "MODEL_FAILURE",
            Self::EnvironmentFailure => "ENVIRONMENT_FAILURE",
            Self::PolicyStop => "POLICY_STOP",
        }
    }
}

/// What the log shows happened to a run, as lines naming the event and why
/// it counts. The Core gathers these from the task's events, approvals and
/// tool calls; the judge only weighs them.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunFacts {
    /// The environment failed: a latch, a lost actuator, a gone window, an
    /// unavailable accessibility service, a lost permission.
    pub environment: Vec<String>,
    /// A person or policy refused: a denied or expired approval, a policy
    /// denial, the person's stop, an emergency stop.
    pub policy: Vec<String>,
    /// How the run ended (`READY_FOR_REVIEW`, `NEEDS_ATTENTION:<code>`, ...).
    pub run_end: String,
}

/// Computer refusal codes that mean the environment, not the model, failed.
pub const ENVIRONMENT_CODES: &[&str] = &[
    "ACTUATOR_UNAVAILABLE",
    "OUTCOME_UNKNOWN",
    "ACCESSIBILITY_UNAVAILABLE",
    "PERMISSION_REQUIRED",
    "SECURE_DESKTOP",
    "CAPTURE_FAILED",
    "INPUT_FAILED",
    "WINDOW_UNVERIFIABLE",
    "TIMEOUT",
];

/// Control-session close reasons that mean the environment failed.
pub const ENVIRONMENT_CLOSE_REASONS: &[&str] = &["ACTUATOR_LOST", "WINDOW_GONE", "LATCHED"];

/// Failure codes that mean a person or policy refused.
pub const POLICY_CODES: &[&str] = &[
    "APPROVAL_DENIED",
    "APPROVAL_EXPIRED",
    "POLICY_DENIED",
    "USER_ABORTED",
    "EMERGENCY_STOP",
];

/// The judge's finding: the verdict and the lines it rests on.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Judgement {
    /// The verdict.
    pub verdict: GuiVerdict,
    /// The predicate's status (`None` when it was never evaluated).
    pub predicate: Option<PredicateStatus>,
    /// Why: the predicate's finding first, then the facts that decided.
    pub basis: Vec<String>,
}

/// Classify a GUI task's run from its predicate and the facts of its log.
///
/// The evidence decides success, never the model's claim. A run whose
/// evidence does not hold is an environment failure when the environment
/// failed under it (checked first: the model is not blamed for a crashed
/// actuator), a policy stop when a person or policy refused an effect, and
/// otherwise a model failure.
#[must_use]
pub fn judge(predicate: Option<&PredicateResult>, facts: &RunFacts) -> Judgement {
    let mut basis = Vec::new();
    match predicate {
        Some(p) => basis.push(format!(
            "evidence {}: {} (read: {})",
            p.status.word(),
            p.detail,
            p.read
        )),
        None => basis.push("evidence MISSING: the run ended before it was read".into()),
    }
    if predicate.is_some_and(|p| p.status == PredicateStatus::Pass) {
        return Judgement {
            verdict: GuiVerdict::Verified,
            predicate: predicate.map(|p| p.status),
            basis,
        };
    }
    let verdict = if !facts.environment.is_empty() {
        basis.extend(facts.environment.iter().cloned());
        GuiVerdict::EnvironmentFailure
    } else if !facts.policy.is_empty() {
        basis.extend(facts.policy.iter().cloned());
        GuiVerdict::PolicyStop
    } else {
        basis.push(format!(
            "no environment failure and no refusal on the log; the run ended {}",
            if facts.run_end.is_empty() {
                "without a recorded end"
            } else {
                facts.run_end.as_str()
            }
        ));
        GuiVerdict::ModelFailure
    };
    Judgement {
        verdict,
        predicate: predicate.map(|p| p.status),
        basis,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn status() -> CompletionEvidence {
        CompletionEvidence::Accessibility {
            app: "org.example.fixture.form".into(),
            role: Some("label".into()),
            name: "Status".into(),
            value: "submitted".into(),
        }
    }

    fn el(role: &str, name: &str, value: Option<&str>) -> ElementFact {
        ElementFact {
            role: role.into(),
            name: name.into(),
            value: value.map(str::to_owned),
        }
    }

    #[test]
    fn a_predicate_parses_strictly_and_validates() {
        let p: CompletionEvidence = serde_json::from_value(json!({
            "kind": "accessibility", "app": "org.example.fixture.form", "role": "label", "name": "Status", "value": "submitted"
        }))
        .unwrap();
        assert_eq!(p, status());
        assert!(p.validate().is_ok());
        assert!(
            serde_json::from_value::<CompletionEvidence>(
                json!({"kind": "accessibility", "app": "a", "name": "n", "value": "v", "approve": true})
            )
            .is_err(),
            "an unknown field is refused, so nothing else rides on a predicate"
        );
        let bad = CompletionEvidence::File {
            path: "../outside.txt".into(),
            contains: "x".into(),
        };
        assert!(bad.validate().is_err());
        let abs = CompletionEvidence::File {
            path: "/etc/passwd".into(),
            contains: "x".into(),
        };
        assert!(abs.validate().is_err());
        for rooted in [
            "\\\\server\\share\\f.txt",
            "C:\\Windows\\win.ini",
            "c:/x.txt",
            "out\\..\\..\\x",
        ] {
            let f = CompletionEvidence::File {
                path: rooted.into(),
                contains: "x".into(),
            };
            assert!(
                f.validate().is_err(),
                "{rooted} is refused on every platform"
            );
        }
        let empty = CompletionEvidence::Accessibility {
            app: String::new(),
            role: None,
            name: "n".into(),
            value: "v".into(),
        };
        assert!(empty.validate().is_err());
    }

    #[test]
    fn accessibility_reads_only_the_declared_application() {
        let tree = [
            el("label", "Status", Some("submitted")),
            el("button", "Submit", Some("")),
        ];
        let ok = eval_accessibility(&status(), "org.example.fixture.form", "s1", &tree);
        assert_eq!(ok.status, PredicateStatus::Pass, "{ok:?}");
        // The same element in another application is no evidence.
        let other = eval_accessibility(&status(), "org.example.fixture.forms", "s1", &tree);
        assert_eq!(other.status, PredicateStatus::Missing, "{other:?}");
        assert!(other.read.contains("not used"));
        let idle = [el("label", "Status", Some("idle"))];
        let no = eval_accessibility(&status(), "org.example.fixture.form", "s2", &idle);
        assert_eq!(no.status, PredicateStatus::Fail);
        assert!(no.detail.contains("\"idle\""));
        let secure = [el("label", "Status", None)];
        assert_eq!(
            eval_accessibility(&status(), "org.example.fixture.form", "s3", &secure).status,
            PredicateStatus::Missing
        );
        let twice = [
            el("label", "Status", Some("submitted")),
            el("label", "Status", Some("idle")),
        ];
        assert_eq!(
            eval_accessibility(&status(), "org.example.fixture.form", "s4", &twice).status,
            PredicateStatus::Missing,
            "an ambiguous element is never read as a pass"
        );
        assert_eq!(
            eval_accessibility(&status(), "org.example.fixture.form", "s5", &[]).status,
            PredicateStatus::Fail
        );
    }

    #[test]
    fn file_and_recorded_predicates() {
        let f = CompletionEvidence::File {
            path: "out/report.txt".into(),
            contains: "total: 3".into(),
        };
        assert_eq!(
            eval_file(&f, Some("x\ntotal: 3\n")).status,
            PredicateStatus::Pass
        );
        assert_eq!(
            eval_file(&f, Some("total: 2")).status,
            PredicateStatus::Fail
        );
        assert_eq!(eval_file(&f, None).status, PredicateStatus::Missing);
        let r = CompletionEvidence::Recorded {
            event_type: "ComputerActionPerformed".into(),
            field: "verified".into(),
            equals: "true".into(),
        };
        let evs = [
            json!({"__type": "ComputerActionPerformed", "verified": null}),
            json!({"__type": "ComputerActionPerformed", "verified": true}),
        ];
        assert_eq!(eval_recorded(&r, &evs).status, PredicateStatus::Pass);
        assert_eq!(eval_recorded(&r, &evs[..1]).status, PredicateStatus::Fail);
        assert_eq!(eval_recorded(&r, &[]).status, PredicateStatus::Fail);
    }

    fn pass() -> PredicateResult {
        PredicateResult::of(PredicateStatus::Pass, "ok", "state read")
    }

    fn fail() -> PredicateResult {
        PredicateResult::of(PredicateStatus::Fail, "Status reads idle", "state read")
    }

    #[test]
    fn the_judge_separates_model_environment_and_policy() {
        let none = RunFacts {
            run_end: "READY_FOR_REVIEW".into(),
            ..Default::default()
        };
        assert_eq!(judge(Some(&pass()), &none).verdict, GuiVerdict::Verified);
        let model = judge(Some(&fail()), &none);
        assert_eq!(model.verdict, GuiVerdict::ModelFailure);
        assert!(model.basis[0].starts_with("evidence FAIL"), "{model:?}");
        let env = RunFacts {
            environment: vec!["ComputerLatched ACTUATOR_LOST".into()],
            policy: vec!["approval denied".into()],
            run_end: "NEEDS_ATTENTION".into(),
        };
        let e = judge(Some(&fail()), &env);
        assert_eq!(
            e.verdict,
            GuiVerdict::EnvironmentFailure,
            "a crashed actuator is never the model's failure"
        );
        assert!(e.basis.iter().any(|b| b.contains("ACTUATOR_LOST")));
        let policy = RunFacts {
            policy: vec!["approval of computer.press DENIED by user".into()],
            ..Default::default()
        };
        let p = judge(None, &policy);
        assert_eq!(p.verdict, GuiVerdict::PolicyStop);
        assert_eq!(p.predicate, None);
        // A pass is a pass even after an environment hiccup the run recovered from.
        assert_eq!(judge(Some(&pass()), &env).verdict, GuiVerdict::Verified);
        // Missing evidence with nothing in the way is the model's: it left no state to read.
        let missing = PredicateResult::missing("no open control session", "");
        assert_eq!(
            judge(Some(&missing), &none).verdict,
            GuiVerdict::ModelFailure
        );
    }
}
