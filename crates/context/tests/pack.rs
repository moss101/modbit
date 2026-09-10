//! M3.8: budget-aware packing with provenance and the Context Ledger.
use modbit_context::{Candidate, ContextLedger, estimate_tokens, pack};

fn cand(path: &str, lines: Option<(u32, u32)>, score: f32, text: &str) -> Candidate {
    Candidate {
        path: path.into(),
        lines,
        span: lines.map(|(a, b)| (u64::from(a) * 10, u64::from(b) * 10)),
        score,
        sources: vec!["exact".into()],
        reasons: vec!["exact_symbol".into()],
        content_hash: Some("abc".into()),
        text: text.into(),
        critical: false,
        critical_reason: None,
        fresh_in_worktree: false,
    }
}

#[test]
fn budget_is_never_exceeded_critical_entries_go_first_and_omissions_are_summarised() {
    let big = "x".repeat(400); // 100 tokens
    let mut diag = cand("src/a.rs", Some((10, 12)), 0.01, &"d".repeat(80)); // 20 tokens
    diag.critical = true;
    diag.critical_reason = Some("diagnostic".into());
    diag.fresh_in_worktree = true;
    let cands = vec![
        cand("src/b.rs", Some((1, 3)), 0.9, &big),
        cand("src/c.rs", Some((1, 2)), 0.5, &"c".repeat(40)), // 10 tokens, utility 0.05
        diag,
        cand("src/d.rs", None, 0.3, &"e".repeat(120)), // 30 tokens, utility 0.01
    ];
    let p = pack(&cands, 60, 7, "q1");
    assert!(p.token_used <= 60, "{p:?}");
    assert_eq!(p.workspace_revision, 7);
    assert_eq!(p.compiler_version, "context-pack-v1");
    assert_eq!(p.token_estimator, "bytes/4");
    assert_eq!(estimate_tokens(&big), 100);
    let paths: Vec<&str> = p
        .entries
        .iter()
        .map(|e| e.provenance.path.as_str())
        .collect();
    assert_eq!(paths, ["src/a.rs", "src/c.rs", "src/d.rs"], "{p:?}");
    assert_eq!(p.entries[0].reason, "critical:diagnostic");
    assert_eq!(p.entries[0].freshness, "fresh_in_worktree");
    assert_eq!(p.entries[1].reason, "utility");
    assert_eq!(p.entries[0].source_ref, "workspace:src/a.rs");
    assert_eq!(p.entries[0].provenance.content_hash.as_deref(), Some("abc"));
    assert_eq!(p.entries[0].provenance.workspace_revision, 7);
    assert_eq!(p.entries[0].provenance.excerpt_hash.len(), 64);
    assert_eq!(p.token_used, 60);
    assert_eq!(p.omitted_summary.count, 1);
    assert_eq!(p.omitted_summary.token_cost, 100);
    assert_eq!(p.omitted_summary.paths, ["src/b.rs"]);
    assert!(p.complete);
    // A critical entry that cannot fit marks the pack incomplete.
    let mut huge = cand("src/z.rs", Some((1, 1)), 1.0, &"z".repeat(4000));
    huge.critical = true;
    let p2 = pack(&[huge], 50, 7, "q2");
    assert!(!p2.complete && p2.omitted_summary.critical_count == 1 && p2.entries.is_empty());
    // Deterministic ids.
    assert_eq!(pack(&cands, 60, 7, "q1").pack_id, p.pack_id);
    assert_ne!(pack(&cands, 60, 8, "q1").pack_id, p.pack_id);
}

#[test]
fn duplicates_collapse_on_span_identity_or_containment() {
    let cands = vec![
        cand("src/a.rs", Some((1, 20)), 0.9, &"a".repeat(80)),
        cand("src/a.rs", Some((5, 6)), 0.8, &"b".repeat(8)),
        cand("src/a.rs", Some((5, 6)), 0.7, &"b".repeat(8)),
        cand("src/a.rs", Some((30, 31)), 0.6, &"c".repeat(8)),
    ];
    let p = pack(&cands, 1000, 1, "q");
    let lines: Vec<Option<(u32, u32)>> = p.entries.iter().map(|e| e.lines).collect();
    assert!(lines.contains(&Some((30, 31))), "{lines:?}");
    assert!(
        !lines.contains(&Some((5, 6))),
        "contained in 1-20: {lines:?}"
    );
    assert_eq!(p.omitted_summary.duplicates, 2);
}

#[test]
fn ledger_records_injections_and_marks_use_only_at_the_retrieved_revision() {
    let cands = vec![
        cand("src/a.rs", Some((1, 2)), 0.9, "aa"),
        cand("src/b.rs", None, 0.5, "bb"),
    ];
    let p = pack(&cands, 100, 3, "q");
    let mut l = ContextLedger::default();
    l.record(&p);
    assert_eq!(l.usage(), (2, 0));
    assert!(l.has_record("src/a.rs", 3) && !l.has_record("src/a.rs", 4));
    assert_eq!(l.mark_used("src/a.rs", 4, "call-1", "fs.read"), 0, "stale");
    assert_eq!(l.mark_used("src/a.rs", 3, "call-2", "fs.read"), 1);
    assert_eq!(
        l.mark_used("src/a.rs", 3, "call-3", "fs.read"),
        0,
        "first use kept"
    );
    assert_eq!(l.usage(), (2, 1));
    let u = l.entries[0].used.as_ref().unwrap();
    assert_eq!(
        (
            u.tool_call_id.as_str(),
            u.tool_name.as_str(),
            u.workspace_revision
        ),
        ("call-2", "fs.read", 3)
    );
    // A second pack at a later revision adds records with their own ordinal.
    let p2 = pack(&cands, 100, 4, "q");
    l.record(&p2);
    assert_eq!(l.entries.last().unwrap().pack_ordinal, 2);
    assert!(l.has_record("src/a.rs", 4));
}
