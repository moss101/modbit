//! QUAL-EV-0245 (REQ-EV-0245): the fault corpus produces stable diagnostic
//! features. Every case in `fixtures/diagnostics/corpus.json` is classified
//! and compared, field by field, with the pinned diagnosis in
//! `fixtures/diagnostics/expected.json`; a change to the classifier that
//! moves a class, a code, retryability or a feature tag fails here until the
//! pin is regenerated on purpose (`MODBIT_UPDATE_DIAGNOSTICS_CORPUS=1`).
//! QUAL-EV-0073 (REQ-EV-0073) invariants are checked over the whole corpus:
//! a timeout is never a success and is retryable; corrupt state is never
//! retryable; every diagnosis names a class, a code and a recovery path.

use modbit_core_runtime::{FailureSource, classify};
use modbit_domain::failure::{FailureClass, FailureDiagnostic};
use serde::Deserialize;

#[derive(Deserialize)]
struct Case {
    name: String,
    source: Source,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Source {
    Tool {
        tool: String,
        status: String,
        code: Option<String>,
        message: Option<String>,
        result_ref: String,
        timed_out: bool,
        cancelled: bool,
    },
    Provider {
        code: String,
        message: String,
    },
    Store {
        code: String,
        message: String,
    },
    Budget {
        budget: String,
        limit: u64,
        used: u64,
    },
    Loop {
        code: String,
        message: String,
    },
    Restart {
        boundary: String,
        message: String,
        unknown_calls: Vec<String>,
    },
    LeaseLost {
        held: u64,
        current: u64,
        owner: String,
    },
}

fn run(source: &Source) -> FailureDiagnostic {
    match source {
        Source::Tool {
            tool,
            status,
            code,
            message,
            result_ref,
            timed_out,
            cancelled,
        } => classify(&FailureSource::Tool {
            tool,
            status,
            code: code.as_deref(),
            message: message.as_deref(),
            result_ref,
            timed_out: *timed_out,
            cancelled: *cancelled,
        }),
        Source::Provider { code, message } => classify(&FailureSource::Provider { code, message }),
        Source::Store { code, message } => classify(&FailureSource::Store { code, message }),
        Source::Budget {
            budget,
            limit,
            used,
        } => classify(&FailureSource::Budget {
            budget,
            limit: *limit,
            used: *used,
        }),
        Source::Loop { code, message } => classify(&FailureSource::Loop { code, message }),
        Source::Restart {
            boundary,
            message,
            unknown_calls,
        } => classify(&FailureSource::Restart {
            boundary,
            message,
            unknown_calls,
        }),
        Source::LeaseLost {
            held,
            current,
            owner,
        } => classify(&FailureSource::LeaseLost {
            held: *held,
            current: *current,
            owner,
        }),
    }
}

fn fixture(name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/diagnostics")
        .join(name)
}

#[test]
fn qual_ev_0245_fault_corpus_produces_stable_diagnostic_features() {
    let corpus: Vec<Case> =
        serde_json::from_str(&std::fs::read_to_string(fixture("corpus.json")).unwrap()).unwrap();
    assert!(corpus.len() >= 30, "the corpus covers every source kind");
    let mut names = corpus.iter().map(|c| c.name.as_str()).collect::<Vec<_>>();
    names.sort_unstable();
    names.dedup();
    assert_eq!(names.len(), corpus.len(), "case names are unique");

    let actual: serde_json::Map<String, serde_json::Value> = corpus
        .iter()
        .map(|c| {
            (
                c.name.clone(),
                serde_json::to_value(run(&c.source)).unwrap(),
            )
        })
        .collect();
    // Determinism: classifying twice yields the same bytes.
    let again: serde_json::Map<String, serde_json::Value> = corpus
        .iter()
        .map(|c| {
            (
                c.name.clone(),
                serde_json::to_value(run(&c.source)).unwrap(),
            )
        })
        .collect();
    assert_eq!(actual, again, "classification is deterministic");

    let expected_path = fixture("expected.json");
    if std::env::var_os("MODBIT_UPDATE_DIAGNOSTICS_CORPUS").is_some() {
        std::fs::write(
            &expected_path,
            serde_json::to_string_pretty(&serde_json::Value::Object(actual.clone())).unwrap()
                + "\n",
        )
        .unwrap();
    }
    let expected: serde_json::Map<String, serde_json::Value> =
        serde_json::from_str(&std::fs::read_to_string(&expected_path).unwrap()).unwrap();
    for c in &corpus {
        assert_eq!(
            actual[&c.name], expected[&c.name],
            "diagnosis of `{}` moved; regenerate the pin only on purpose",
            c.name
        );
    }
    assert_eq!(
        actual.len(),
        expected.len(),
        "the pin covers exactly the corpus"
    );

    // Every feature set carries the three stable tags an evaluator keys on.
    for c in &corpus {
        let d = run(&c.source);
        let has = |p: &str| d.features.iter().any(|f| f.starts_with(p));
        assert!(has("class:"), "{}: class tag", c.name);
        assert!(has("code:"), "{}: code tag", c.name);
        assert!(has("source:"), "{}: source tag", c.name);
        assert!(
            d.features.contains(&"retryable".to_owned())
                || d.features.contains(&"not_retryable".to_owned()),
            "{}: retryability tag",
            c.name
        );
        let mut sorted = d.features.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(
            sorted, d.features,
            "{}: features are sorted and unique",
            c.name
        );
    }
}

#[test]
fn qual_ev_0073_every_diagnosis_names_class_retryability_action_evidence_and_recovery() {
    let corpus: Vec<Case> =
        serde_json::from_str(&std::fs::read_to_string(fixture("corpus.json")).unwrap()).unwrap();
    for c in &corpus {
        let d = run(&c.source);
        assert!(!d.code.is_empty(), "{}: code", c.name);
        assert!(!d.recovery_path.is_empty(), "{}: recovery path", c.name);
        let text = d.render();
        assert!(
            text.contains("failure_class: "),
            "{}: rendered class",
            c.name
        );
        assert!(
            text.contains("retryable: "),
            "{}: rendered retryability",
            c.name
        );
        assert!(text.contains("recovery: "), "{}: rendered recovery", c.name);
        assert!(
            !text.contains("SUCCESS"),
            "{}: never a generic success",
            c.name
        );
        match &c.source {
            Source::Tool {
                timed_out: true, ..
            } => {
                assert_eq!(d.class, FailureClass::Timeout, "{}", c.name);
                assert!(d.retryable, "{}: a timeout is retryable", c.name);
                assert!(
                    !d.evidence_refs.is_empty(),
                    "{}: the result is evidence",
                    c.name
                );
            }
            Source::Tool {
                code: Some(code), ..
            }
            | Source::Store { code, .. }
                if code == "OBJECT_MISMATCH" || code == "INTEGRITY" =>
            {
                assert_eq!(d.class, FailureClass::CorruptState, "{}", c.name);
                assert!(!d.retryable, "{}: corrupt state is not retryable", c.name);
                assert!(
                    !d.user_action.is_empty(),
                    "{}: the user is told what to do",
                    c.name
                );
            }
            Source::Tool { status, .. } if status == "UNKNOWN_OUTCOME" => {
                assert_eq!(d.class, FailureClass::UnknownOutcome, "{}", c.name);
                assert!(
                    !d.retryable,
                    "{}: unknown outcome is reconciled, not retried",
                    c.name
                );
            }
            Source::Budget { .. } => {
                assert_eq!(d.class, FailureClass::Budget, "{}", c.name);
                assert!(!d.user_action.is_empty(), "{}", c.name);
            }
            Source::LeaseLost { .. } => {
                assert_eq!(d.class, FailureClass::Lease, "{}", c.name);
                assert!(!d.retryable, "{}", c.name);
            }
            Source::Restart { boundary, .. } if boundary == "RECONCILING" => {
                assert_eq!(d.class, FailureClass::UnknownOutcome, "{}", c.name);
                assert!(
                    !d.evidence_refs.is_empty(),
                    "{}: the unknown calls are the evidence",
                    c.name
                );
            }
            _ => {}
        }
    }
}
