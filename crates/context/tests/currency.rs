//! FIX-12 (audit N7): a pack excerpt whose file changed after it was read is
//! dropped from the latest pack, never re-injected under its old revision and
//! hash. The ledger itself keeps the record of what was retrieved.
use modbit_context::{Candidate, ContextLedger, FileState, pack};

fn cand(path: &str, hash: &str, lines: Option<(u32, u32)>, text: &str) -> Candidate {
    Candidate {
        path: path.into(),
        lines,
        span: lines.map(|(a, b)| (u64::from(a) * 10, u64::from(b) * 10)),
        score: 0.5,
        sources: vec!["lexical".into()],
        reasons: vec![],
        content_hash: Some(hash.into()),
        text: text.into(),
        ..Candidate::default()
    }
}

fn ledger_with_pack() -> ContextLedger {
    let mut doc = cand("attached/spec.md", "doc-1", None, "the attached spec");
    doc.source_ref = "attached:spec".into();
    // A budget small enough that the last candidate becomes a stub.
    let cands = vec![
        cand("src/a.rs", "ha", Some((1, 40)), &"a".repeat(200)),
        cand("src/b.rs", "hb", Some((5, 9)), &"b".repeat(200)),
        cand("src/c.rs", "hc", None, &"c".repeat(2000)),
        doc,
    ];
    let p = pack(&cands, 190, 3, "q");
    assert_eq!(p.entries.len(), 3, "{p:?}");
    assert!(
        p.stubs.iter().any(|s| s.provenance.path == "src/c.rs"),
        "{p:?}"
    );
    let mut ledger = ContextLedger::default();
    ledger.record(&p);
    ledger
}

#[test]
fn a_fragment_whose_file_changed_is_dropped_and_the_rest_stay() {
    let mut ledger = ledger_with_pack();
    let dropped = ledger.invalidate_stale(|path| match path {
        "src/a.rs" => Some(FileState::Hash("ha".into())), // unchanged
        "src/b.rs" => Some(FileState::Hash("hb-after-edit".into())),
        "src/c.rs" => Some(FileState::Missing),
        _ => None,
    });
    let mut names: Vec<(String, bool, Option<String>)> = dropped
        .iter()
        .map(|d| (d.path.clone(), d.stub, d.current_hash.clone()))
        .collect();
    names.sort();
    assert_eq!(
        names,
        vec![
            (
                "src/b.rs".to_owned(),
                false,
                Some("hb-after-edit".to_owned())
            ),
            ("src/c.rs".to_owned(), true, None),
        ],
        "{dropped:?}"
    );
    let pack = ledger.last_pack.as_ref().unwrap();
    let kept: Vec<&str> = pack
        .entries
        .iter()
        .map(|e| e.provenance.path.as_str())
        .collect();
    // The unchanged file and the attached document (no file to compare) stay.
    assert_eq!(kept, ["attached/spec.md", "src/a.rs"], "{pack:?}");
    assert!(pack.stubs.is_empty(), "{pack:?}");
    // The record of what was retrieved is not rewritten, and the drop is remembered.
    assert!(ledger.has_record("src/b.rs", 3));
    assert_eq!(ledger.invalidated.len(), 2);
    assert_eq!(dropped[0].read_hash, "hb");
    // A second pass over the same files drops nothing more.
    assert!(
        ledger
            .invalidate_stale(|p| (p == "src/a.rs").then(|| FileState::Hash("ha".into())))
            .is_empty()
    );
}

#[test]
fn an_unchanged_pack_and_an_empty_ledger_are_left_alone() {
    let mut ledger = ledger_with_pack();
    let before = ledger.last_pack.clone();
    let dropped = ledger.invalidate_stale(|p| {
        Some(FileState::Hash(
            match p {
                "src/a.rs" => "ha",
                "src/b.rs" => "hb",
                _ => "hc",
            }
            .into(),
        ))
    });
    assert!(dropped.is_empty());
    assert_eq!(ledger.last_pack, before);
    assert!(
        ContextLedger::default()
            .invalidate_stale(|_| Some(FileState::Missing))
            .is_empty()
    );
}

#[test]
fn a_write_is_recorded_after_it_and_used_what_was_retrieved_before_it() {
    let mut ledger = ledger_with_pack();
    // The write moved the workspace to revision 4; it used the revision-3 excerpt.
    assert_eq!(
        ledger.mark_used_before("src/a.rs", 4, "call-1", "change.apply"),
        1
    );
    let (injected, used) = ledger.usage();
    assert!(injected >= 3 && used == 1, "{injected} {used}");
    // Nothing was retrieved before revision 3, so there is nothing to mark.
    assert_eq!(
        ledger.mark_used_before("src/b.rs", 3, "call-2", "change.apply"),
        0
    );
}

#[test]
fn the_ledger_round_trips_through_json_with_its_last_pack_and_invalidations() {
    let mut ledger = ledger_with_pack();
    ledger.invalidate_stale(|p| (p == "src/b.rs").then_some(FileState::Missing));
    ledger.record_read("src/a.rs", 3, Some("ha"), "call-0", "fs.read");
    let json = serde_json::to_vec(&ledger).unwrap();
    let back: ContextLedger = serde_json::from_slice(&json).unwrap();
    assert_eq!(back, ledger);
    // A ledger snapshot written before this field existed still loads.
    let mut v: serde_json::Value = serde_json::from_slice(&json).unwrap();
    v.as_object_mut().unwrap().remove("invalidated");
    let old: ContextLedger = serde_json::from_value(v).unwrap();
    assert!(old.invalidated.is_empty() && old.last_pack.is_some());
}
