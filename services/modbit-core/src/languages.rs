//! Language support labels (PX-026, docs/76 "Current classification"): what
//! the product claims per language today, proven on the real fixtures, and
//! what it explicitly does not claim yet. Every client shows these labels.

use modbit_protocol::v1 as wire;

const ALPHA_PROVEN: &[&str] = &[
    "exact_and_bm25_retrieval",
    "revision_bound_edits_with_preserved_encoding_and_line_endings",
    "compile_and_test_evidence_attributed_to_tasks",
];
const ALPHA_PROVISIONAL: &[&str] = &[
    "tree_sitter_symbols (M3.3)",
    "headless_language_service_diagnostics_symbols_references (M3.4)",
    "dependency_graph_and_L2_L3_retrieval (M3.6, M3.7)",
];
const ALPHA_NOT_CLAIMED: &[&str] = &[
    "tier_a_full_engineering_intelligence (PX-028 conformance suite)",
    "language_specific_change_strategies_and_skills",
];

fn alpha(language: &str, fixture: &str, evidence: &[&str]) -> wire::LanguageSupportView {
    wire::LanguageSupportView {
        language: language.into(),
        tier: "ALPHA_BASELINE".into(),
        label: "Tier A candidate at the Alpha baseline: Tier C plus compile and test evidence"
            .into(),
        fixture: fixture.into(),
        proven: ALPHA_PROVEN.iter().map(|s| (*s).to_owned()).collect(),
        provisional: ALPHA_PROVISIONAL.iter().map(|s| (*s).to_owned()).collect(),
        not_claimed: ALPHA_NOT_CLAIMED.iter().map(|s| (*s).to_owned()).collect(),
        evidence_tests: evidence.iter().map(|s| (*s).to_owned()).collect(),
        note:
            "Structural intelligence is not claimed until the Tier A suite passes (docs/76 PX-028)."
                .into(),
    }
}

/// The catalog every client shows.
pub(crate) fn catalog() -> Vec<wire::LanguageSupportView> {
    vec![
        alpha(
            "typescript",
            "tests/fixtures/repos/ts-webapp",
            &[
                "vitest_fixture_parses_structured_reports_when_vitest_is_installed",
                "m3_1_exact_regex_path_index_serves_bounded_revision_bound_hits_and_refreshes_on_writes",
                "edits_preserve_crlf_and_refuse_non_utf8",
            ],
        ),
        alpha(
            "javascript",
            "tests/fixtures/repos/ts-webapp",
            &["vitest_fixture_parses_structured_reports_when_vitest_is_installed"],
        ),
        alpha(
            "python",
            "tests/fixtures/repos/python-service",
            &[
                "pytest_fixture_produces_structured_reports_with_stable_failure_signatures_when_pytest_is_installed",
                "edits_preserve_crlf_and_refuse_non_utf8",
            ],
        ),
        alpha(
            "rust",
            "tests/fixtures/repos/rust-cli",
            &[
                "m2_8_verification_engine_gates_completion_on_real_cargo_fixture",
                "edits_preserve_crlf_and_refuse_non_utf8",
            ],
        ),
        wire::LanguageSupportView {
            language: "*".into(),
            tier: "UNSUPPORTED".into(),
            label: "Unsupported: read-only analysis; edits need explicit per-task opt-in; no verification claim beyond configured commands".into(),
            fixture: String::new(),
            proven: vec!["exact_and_bm25_retrieval".into()],
            provisional: vec![],
            not_claimed: vec!["any structural, semantic or language-service claim".into()],
            evidence_tests: vec![],
            note: "Classified only after its conformance suite passes (docs/76).".into(),
        },
    ]
}
