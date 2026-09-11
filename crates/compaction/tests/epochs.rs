//! Compaction epochs (REQ-EV-0056 / 0057 / 0058 / 0130): what survives, what
//! stays recoverable, and which results may install.
use modbit_compaction::{
    CompactionManifest, CompactionRequest, FactKind, RejectedCompaction, SourceEntry, accept,
    accept_async, compact, estimate_tokens, fidelity, handles_in, source_digest,
};

fn request<'a>(
    entries: &'a [SourceEntry],
    previous: Option<&'a CompactionManifest>,
    target_tokens: u32,
) -> CompactionRequest<'a> {
    CompactionRequest {
        entries,
        previous,
        task_generation: 3,
        source_head_offset: 120,
        branch_generation: 7,
        compiler_version: "prompt-compiler-v1",
        target_tokens,
    }
}

fn entry(role: &str, name: &str, text: &str) -> SourceEntry {
    SourceEntry {
        role: role.into(),
        name: name.into(),
        text: text.into(),
        failure_signature: None,
    }
}

fn corpus() -> Vec<SourceEntry> {
    let mut v = vec![
        entry(
            "user",
            "",
            "Reject negative quantities, and never touch the lockfile.",
        ),
        entry("assistant", "", "Reading the file."),
        entry(
            "tool",
            "fs.read",
            &format!(
                "status: SUCCESS\\nbytes_total: 9000\\noutput:\\n{}\\nresult_ref {}",
                "x".repeat(4000),
                "a".repeat(64)
            ),
        ),
        entry(
            "tool",
            "plan.update",
            "status: SUCCESS\\nplan version 1 recorded (ref abc)",
        ),
        entry("assistant", "", "Applying the guard."),
        entry(
            "tool",
            "change.apply",
            &format!(
                "status: SUCCESS\\nwrote src/lib.rs\\nresult_ref {}",
                "b".repeat(64)
            ),
        ),
        entry(
            "tool",
            "approval",
            "status: SUCCESS\\nthe user approved the protected effect",
        ),
    ];
    let mut failing = entry("tool", "test.run", "status: SUCCESS\\nthe suite failed");
    failing.failure_signature = Some("verify:cargo:tests/q.rs::acceptance:abcd".into());
    v.push(failing);
    v
}

#[test]
fn compaction_preserves_labelled_facts_and_keeps_the_rest_recoverable() {
    let entries = corpus();
    let m = compact(&request(&entries, None, 400));
    assert_eq!((m.epoch, m.previous_epoch, m.task_generation), (1, None, 3));
    assert_eq!(
        (m.source_head_offset, m.source_entries),
        (120, entries.len())
    );
    assert_eq!(m.manifest_hash.len(), 64);
    let kinds: Vec<FactKind> = m.preserved.iter().map(|f| f.kind).collect();
    for want in [
        FactKind::Instruction,
        FactKind::Decision,
        FactKind::Approval,
        FactKind::OpenFailure,
        FactKind::Handle,
    ] {
        assert!(kinds.contains(&want), "{want:?} did not survive: {m:?}");
    }
    // The instruction survives verbatim.
    assert!(
        m.preserved.iter().any(|f| f.kind == FactKind::Instruction
            && f.text == "Reject negative quantities, and never touch the lockfile."),
        "{m:?}"
    );
    // The failure signature and both object refs are carried forward.
    assert!(
        m.preserved
            .iter()
            .any(|f| f.kind == FactKind::OpenFailure && f.text.contains("acceptance")),
        "{m:?}"
    );
    assert_eq!(m.resources.len(), 2, "{:?}", m.resources);
    assert!(m.resources.iter().all(|r| r.len() == 64));
    // The projection is far smaller than the source and says so.
    let source_tokens: u32 = entries.iter().map(|e| estimate_tokens(&e.text)).sum();
    assert!(
        m.projection_tokens < source_tokens / 2,
        "{} vs {source_tokens}",
        m.projection_tokens
    );
    assert!(m.projection.contains("canonical log keeps them in full"));
    assert!(m.projection_tokens <= m.target_tokens + 40, "{m:?}");
    // A tight budget truncates the projection and says that too.
    let tight = compact(&request(&entries, None, 20));
    assert!(
        tight.projection.contains("omitted for the epoch budget"),
        "{tight:?}"
    );
    assert!(
        tight.preserved.len() == m.preserved.len(),
        "the manifest still lists every fact"
    );
    assert_eq!(handles_in("nothing here"), Vec::<String>::new());
}

#[test]
fn a_stale_or_out_of_order_compaction_result_is_refused() {
    let corpus = corpus();
    let m: CompactionManifest = compact(&request(&corpus, None, 400));
    assert!(accept(&m, None, 3, 120).is_ok());
    assert_eq!(
        accept(&m, None, 3, 121),
        Err(RejectedCompaction::SourceAdvanced { saw: 120, now: 121 }),
        "the log advanced while the compaction was computed"
    );
    assert_eq!(
        accept(&m, None, 4, 120),
        Err(RejectedCompaction::GenerationChanged { saw: 3, now: 4 }),
        "a fork or revert invalidates it"
    );
    assert_eq!(
        accept(&m, Some(1), 3, 120),
        Err(RejectedCompaction::NotSuccessor {
            installed: 1,
            offered: 1
        }),
        "an epoch never installs twice"
    );
    let second = compact(&request(&corpus, Some(&m), 400));
    assert!(accept(&second, Some(1), 3, 120).is_ok());
    assert_ne!(second.manifest_hash, m.manifest_hash);
}

/// docs/19 "Compaction epochs" / docs/54 fault 10 (M4.2): an asynchronous
/// result is judged by the branch generation it captured, the exact prefix it
/// summarised and the epoch order — never by the log head, which has moved
/// on by the time a worker returns.
#[test]
fn an_asynchronous_result_installs_only_onto_the_prefix_and_branch_it_saw() {
    let corpus = corpus();
    let m = compact(&request(&corpus, None, 400));
    assert_eq!(m.branch_generation, 7);
    assert_eq!(m.source_digest, source_digest(&corpus));
    assert_eq!(m.source_digest.len(), 64);
    // The log advanced while the worker ran: still fine.
    assert!(accept_async(&m, None, 7, &source_digest(&corpus)).is_ok());
    // A fork or revert moved the branch generation: refused.
    assert_eq!(
        accept_async(&m, None, 8, &source_digest(&corpus)),
        Err(RejectedCompaction::BranchChanged { saw: 7, now: 8 })
    );
    // Another epoch drained the prefix first: refused as out of order.
    assert_eq!(
        accept_async(&m, Some(1), 7, &source_digest(&corpus[3..])),
        Err(RejectedCompaction::NotSuccessor {
            installed: 1,
            offered: 1
        })
    );
    // The prefix itself was rewritten (history rebuilt): refused.
    let mut rewritten = corpus.clone();
    rewritten[0].text.push_str(" (edited)");
    let now = source_digest(&rewritten);
    assert_eq!(
        accept_async(&m, None, 7, &now),
        Err(RejectedCompaction::SourceRewritten {
            saw: m.source_digest.clone(),
            now
        })
    );
    assert_eq!(
        RejectedCompaction::BranchChanged { saw: 1, now: 2 }.code(),
        "BRANCH_CHANGED"
    );
    // The digest is order- and content-sensitive, and the manifest hash
    // binds it and the branch generation.
    let mut swapped = corpus.clone();
    swapped.swap(0, 1);
    assert_ne!(source_digest(&swapped), source_digest(&corpus));
    let other_branch = compact(&CompactionRequest {
        branch_generation: 8,
        ..request(&corpus, None, 400)
    });
    assert_ne!(other_branch.manifest_hash, m.manifest_hash);
}

/// REQ-EV-0056: a second epoch cannot drop what the first one preserved.
#[test]
fn a_later_epoch_carries_the_facts_and_handles_of_the_one_before_it() {
    let first = compact(&request(&corpus(), None, 400));
    // The second round sees only fresh material: no instruction, no plan, no
    // approval, no failure and a different object ref.
    let later = vec![
        entry("assistant", "", "Continuing."),
        entry(
            "tool",
            "fs.read",
            &format!("status: SUCCESS\nresult_ref {}", "c".repeat(64)),
        ),
    ];
    let second = compact(&request(&later, Some(&first), 400));
    assert_eq!((second.epoch, second.previous_epoch), (2, Some(1)));
    assert_eq!(second.source_entries, later.len());
    for want in [
        FactKind::Instruction,
        FactKind::Decision,
        FactKind::Approval,
        FactKind::OpenFailure,
    ] {
        assert!(
            second.preserved.iter().any(|f| f.kind == want),
            "{want:?} was dropped by the second epoch: {second:?}"
        );
    }
    assert!(
        second
            .preserved
            .iter()
            .any(|f| f.kind == FactKind::Instruction
                && f.text == "Reject negative quantities, and never touch the lockfile."),
        "{second:?}"
    );
    // Every earlier handle is still reachable, plus the new one.
    for r in &first.resources {
        assert!(second.resources.contains(r), "{r} was dropped: {second:?}");
    }
    assert_eq!(second.resources.len(), first.resources.len() + 1);
    assert!(second.projection.contains("carried below"), "{second:?}");
    // Carrying does not duplicate: a third epoch over the same material keeps
    // one copy of each fact.
    let third = compact(&request(&later, Some(&second), 400));
    let mut texts: Vec<String> = third
        .preserved
        .iter()
        .map(|f| format!("{:?}|{}", f.kind, f.text))
        .collect();
    let before = texts.len();
    texts.sort_unstable();
    texts.dedup();
    assert_eq!(texts.len(), before, "{third:?}");
}

/// REQ-EV-0130: a critical-fact corpus meets a fidelity threshold — every
/// labelled fact survives into the manifest, and the projection shows them all
/// when the budget allows it.
#[test]
fn a_critical_fact_corpus_meets_the_fidelity_threshold() {
    // Twelve turns of noise around eight critical facts and four handles.
    let mut entries = vec![
        entry("user", "", "Never touch the lockfile."),
        entry("user", "", "Reject negative quantities."),
    ];
    for i in 0..8 {
        entries.push(entry("assistant", "", &format!("Thinking about step {i}.")));
        entries.push(entry(
            "tool",
            "fs.read",
            &format!(
                "status: SUCCESS\noutput:\n{}\nresult_ref {}",
                "noise ".repeat(300),
                format!("{i}").repeat(64)
            ),
        ));
    }
    entries.push(entry(
        "tool",
        "plan.update",
        "status: SUCCESS\nplan version 2 recorded",
    ));
    entries.push(entry(
        "tool",
        "effect.dispatch",
        "status: SUCCESS\nthe user approved the network effect",
    ));
    let mut failing = entry("tool", "check.run", "status: SUCCESS\nthe suite failed");
    failing.failure_signature = Some("verify:cargo:tests/a.rs::acceptance:9f9f".into());
    entries.push(failing);
    let m = compact(&request(&entries, None, 2_000));
    let f = fidelity(&entries, &m);
    assert!(f.critical >= 12, "{f:?}");
    assert!(
        (f.preserved_ratio() - 1.0).abs() < f32::EPSILON,
        "every labelled fact must survive: {f:?}"
    );
    assert!(
        f.projected_ratio() >= 0.9,
        "the model must see them: {f:?} in {}",
        m.projection
    );
    // And the projection is a fraction of the source it replaced.
    let source_tokens: u32 = entries.iter().map(|e| estimate_tokens(&e.text)).sum();
    assert!(m.projection_tokens * 4 < source_tokens, "{m:?}");
    // Under a budget too small to show them, the manifest still keeps them:
    // fidelity distinguishes what was preserved from what was projected.
    let tight = compact(&request(&entries, None, 60));
    let tf = fidelity(&entries, &tight);
    assert!((tf.preserved_ratio() - 1.0).abs() < f32::EPSILON, "{tf:?}");
    assert!(tf.projected_ratio() < 1.0, "{tf:?}");
}
