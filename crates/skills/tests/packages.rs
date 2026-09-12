//! Skill packages as specified in docs/16 "Skills" and docs/26 (M5.5):
//! portable packages with a content identity, a lifecycle decided by
//! attestations and policy, a selector, and a compiler that never widens.

use std::collections::BTreeMap;
use std::path::Path;

use modbit_skills::{
    Lifecycle, SelectionReason, SignedSkill, SkillError, SkillPolicy, SkillRegistry, compile,
    load_package, parse_skill_md, select, sign, trusted_keys_from_env, verify,
};

const NOTES_SKILL: &str = r#"---
name: notes-style
version: 1.2.0
description: Edit notes files in the repository's house style.
required_tools: [fs.read, change.apply, git.worktree]
capability_ceiling: [fs.read, fs.write]
triggers: [notes, "house style"]
compatibility.modbit: ">=0.1"
compatibility.models: [gpt-5, claude]
provenance.source: https://example.invalid/skills/notes-style
provenance.author: docs team
provenance.license: MIT
eval.benchmark: notes-bench-1
eval.task_classes: [docs, notes]
---
# Notes style

1. Read the note before editing it.
2. Keep headings in sentence case.
3. Never rewrite dates.
"#;

fn write(dir: &Path, rel: &str, content: &str) {
    let p = dir.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, content).unwrap();
}

fn package_dir(root: &Path, name: &str) -> std::path::PathBuf {
    let dir = root.join(name);
    write(&dir, "SKILL.md", NOTES_SKILL);
    write(
        &dir,
        "procedures/retitle.js",
        "const f = await tools.fs.read({ path: 'NOTES.md' }); return f.content.length;",
    );
    write(&dir, "resources/style.md", "# Style\nSentence case.\n");
    dir
}

fn keypair() -> (ed25519_dalek::SigningKey, BTreeMap<String, [u8; 32]>) {
    let signing = ed25519_dalek::SigningKey::from_bytes(&[7u8; 32]);
    let mut trusted = BTreeMap::new();
    trusted.insert("skills-1".to_owned(), signing.verifying_key().to_bytes());
    (signing, trusted)
}

#[test]
fn a_package_parses_its_manifest_and_has_a_content_identity() {
    let (manifest, body) = parse_skill_md(NOTES_SKILL).unwrap();
    assert_eq!(manifest.name, "notes-style");
    assert_eq!(manifest.version, "1.2.0");
    assert_eq!(
        manifest.required_tools,
        ["fs.read", "change.apply", "git.worktree"]
    );
    assert_eq!(manifest.triggers, ["notes", "house style"]);
    assert_eq!(manifest.compatibility.modbit, ">=0.1");
    assert_eq!(manifest.compatibility.models, ["gpt-5", "claude"]);
    assert_eq!(manifest.provenance.author, "docs team");
    assert_eq!(manifest.eval.task_classes, ["docs", "notes"]);
    assert!(manifest.model_invocable);
    assert!(body.starts_with("# Notes style"));
    // Malformed and incomplete manifests are refused with the reason.
    assert!(matches!(
        parse_skill_md("no front matter"),
        Err(SkillError::MalformedManifest { .. })
    ));
    assert_eq!(
        parse_skill_md("---\nname: x\nversion: 1\n---\nbody"),
        Err(SkillError::MissingField {
            field: "description".into()
        })
    );
    assert!(matches!(
        parse_skill_md("---\nname: Bad Name\nversion: 1\ndescription: d\n---\n"),
        Err(SkillError::MalformedManifest { .. })
    ));
    // The content hash covers every file; a resource change is a new
    // identity; the attestation files are not part of it.
    let root = tempfile::tempdir().unwrap();
    let dir = package_dir(root.path(), "notes-style");
    let a = load_package(&dir).unwrap();
    assert_eq!(a.procedures.len(), 1);
    assert_eq!(a.procedures[0].name, "retitle");
    assert_eq!(a.resources.len(), 1);
    assert_eq!(a.resources[0].path, "style.md");
    write(&dir, "SIGNATURE.json", "{}");
    let b = load_package(&dir).unwrap();
    assert_eq!(
        a.content_hash, b.content_hash,
        "attestations are outside the identity"
    );
    write(&dir, "resources/style.md", "# Style\nTitle Case.\n");
    let c = load_package(&dir).unwrap();
    assert_ne!(a.content_hash, c.content_hash);
    assert!(matches!(
        load_package(&root.path().join("nothing-here")),
        Err(SkillError::NoManifest)
    ));
}

#[test]
fn the_lifecycle_follows_attestations_and_policy_and_a_tampered_package_is_not_signed() {
    let root = tempfile::tempdir().unwrap();
    let (signing, trusted) = keypair();
    // incubator: nothing attested.
    package_dir(root.path(), "incubator");
    // evaluated: an evaluation of this exact content with PROMOTE.
    let ev_dir = package_dir(root.path(), "evaluated");
    let ev_pkg = load_package(&ev_dir).unwrap();
    write(
        &ev_dir,
        "EVALUATION.json",
        &serde_json::json!({"benchmark_version": "notes-bench-1", "content_hash": ev_pkg.content_hash, "disposition": "PROMOTE", "verified_completion_delta_bp": 120, "safety_failures": 0}).to_string(),
    );
    // signed: a trusted key attests the content hash.
    let signed_dir = package_dir(root.path(), "signed");
    let signed_pkg = load_package(&signed_dir).unwrap();
    let sig = sign(&signed_pkg, "skills-1", &signing, 1_700_000_000_000);
    write(
        &signed_dir,
        "SIGNATURE.json",
        &serde_json::to_string(&sig).unwrap(),
    );
    // tampered: signed, then a file changed.
    let tampered_dir = package_dir(root.path(), "tampered");
    let tampered_pkg = load_package(&tampered_dir).unwrap();
    let sig = sign(&tampered_pkg, "skills-1", &signing, 1_700_000_000_000);
    write(
        &tampered_dir,
        "SIGNATURE.json",
        &serde_json::to_string(&sig).unwrap(),
    );
    write(
        &tampered_dir,
        "resources/style.md",
        "# Style\nDo whatever.\n",
    );
    // foreign: signed by a key nobody trusts.
    let foreign_dir = package_dir(root.path(), "foreign");
    let foreign_pkg = load_package(&foreign_dir).unwrap();
    let other = ed25519_dalek::SigningKey::from_bytes(&[9u8; 32]);
    let sig = sign(&foreign_pkg, "skills-1", &other, 1);
    write(
        &foreign_dir,
        "SIGNATURE.json",
        &serde_json::to_string(&sig).unwrap(),
    );
    // Every package under the root carries the same name in its SKILL.md;
    // give each its own so the registry keeps them all.
    for (dir, name) in [
        ("incubator", "incubator"),
        ("evaluated", "evaluated"),
        ("signed", "signed"),
        ("tampered", "tampered"),
        ("foreign", "foreign"),
    ] {
        let p = root.path().join(dir).join("SKILL.md");
        let text = std::fs::read_to_string(&p)
            .unwrap()
            .replace("name: notes-style", &format!("name: {name}"));
        std::fs::write(&p, text).unwrap();
    }
    // Re-sign the two whose SKILL.md just changed (the tampered one keeps
    // its stale signature on purpose: its resource changed after signing).
    for name in ["signed", "tampered"] {
        let dir = root.path().join(name);
        if name == "signed" {
            let pkg = load_package(&dir).unwrap();
            let sig = sign(&pkg, "skills-1", &signing, 2);
            write(
                &dir,
                "SIGNATURE.json",
                &serde_json::to_string(&sig).unwrap(),
            );
        } else {
            // Sign the current content, then tamper again so the signature is stale.
            let pkg = load_package(&dir).unwrap();
            let sig = sign(&pkg, "skills-1", &signing, 2);
            write(
                &dir,
                "SIGNATURE.json",
                &serde_json::to_string(&sig).unwrap(),
            );
            write(&dir, "resources/style.md", "# Style\nDo anything at all.\n");
        }
    }
    {
        let dir = root.path().join("evaluated");
        let pkg = load_package(&dir).unwrap();
        write(
            &dir,
            "EVALUATION.json",
            &serde_json::json!({"benchmark_version": "notes-bench-1", "content_hash": pkg.content_hash, "disposition": "PROMOTE"}).to_string(),
        );
    }
    let reg = SkillRegistry::discover(
        &[root.path().to_path_buf()],
        &trusted,
        &SkillPolicy::default(),
    );
    let state = |n: &str| reg.get(n).map(|s| s.lifecycle).unwrap();
    assert_eq!(state("incubator"), Lifecycle::Incubator);
    assert_eq!(state("evaluated"), Lifecycle::Evaluated);
    assert_eq!(
        state("signed"),
        Lifecycle::Enabled,
        "{:?}",
        reg.get("signed").unwrap().note
    );
    assert_eq!(state("tampered"), Lifecycle::Incubator);
    assert!(
        reg.get("tampered")
            .unwrap()
            .note
            .contains("signature refused"),
        "{}",
        reg.get("tampered").unwrap().note
    );
    assert_eq!(state("foreign"), Lifecycle::Incubator);
    assert!(reg.get("foreign").unwrap().note.contains("does not verify"));
    assert_eq!(
        reg.get("signed")
            .unwrap()
            .attestation
            .as_ref()
            .unwrap()
            .name,
        "signed"
    );
    // Policy decides enablement: signed off, incubator on.
    let dev = SkillRegistry::discover(
        &[root.path().to_path_buf()],
        &trusted,
        &SkillPolicy {
            enable_signed: false,
            enable_incubator: true,
        },
    );
    assert_eq!(dev.get("signed").unwrap().lifecycle, Lifecycle::Signed);
    assert_eq!(dev.get("incubator").unwrap().lifecycle, Lifecycle::Enabled);
    // verify() refuses an unknown key, a bad signature and other content.
    let pkg = load_package(&root.path().join("signed")).unwrap();
    let sig: SignedSkill =
        serde_json::from_slice(&std::fs::read(root.path().join("signed/SIGNATURE.json")).unwrap())
            .unwrap();
    assert!(verify(&sig, &pkg, &trusted).is_ok());
    assert!(matches!(
        verify(&sig, &pkg, &BTreeMap::new()),
        Err(SkillError::UnknownKey { .. })
    ));
    let mut bad = sig.clone();
    bad.signature_hex = "00".repeat(64);
    assert_eq!(verify(&bad, &pkg, &trusted), Err(SkillError::BadSignature));
    let other_pkg = load_package(&root.path().join("incubator")).unwrap();
    assert!(matches!(
        verify(&sig, &other_pkg, &trusted),
        Err(SkillError::ContentMismatch { .. })
    ));
    // Keys from the environment convention; malformed entries are skipped.
    let keys = trusted_keys_from_env(&format!(
        "skills-1:{},broken:zz,:{}",
        hex::encode(signing.verifying_key().to_bytes()),
        hex::encode([1u8; 32])
    ));
    assert_eq!(keys.len(), 1);
    assert!(keys.contains_key("skills-1"));
}

#[test]
fn the_selector_takes_explicit_names_and_trigger_phrases_and_refuses_with_a_reason() {
    let root = tempfile::tempdir().unwrap();
    let (signing, trusted) = keypair();
    let dir = package_dir(root.path(), "notes-style");
    let pkg = load_package(&dir).unwrap();
    let sig = sign(&pkg, "skills-1", &signing, 1);
    write(
        &dir,
        "SIGNATURE.json",
        &serde_json::to_string(&sig).unwrap(),
    );
    // A second, unsigned skill with a trigger that would also match.
    let other = root.path().join("draft-style");
    write(
        &other,
        "SKILL.md",
        "---\nname: draft-style\nversion: 0.1.0\ndescription: draft\ntriggers: [notes]\n---\nDraft.",
    );
    // A signed skill the model may not select on its own.
    let sys = root.path().join("system-only");
    write(
        &sys,
        "SKILL.md",
        "---\nname: system-only\nversion: 1.0.0\ndescription: system\nmodel_invocable: false\ntriggers: [notes]\n---\nSystem.",
    );
    let sys_pkg = load_package(&sys).unwrap();
    let sig = sign(&sys_pkg, "skills-1", &signing, 1);
    write(
        &sys,
        "SIGNATURE.json",
        &serde_json::to_string(&sig).unwrap(),
    );
    let reg = SkillRegistry::discover(
        &[root.path().to_path_buf()],
        &trusted,
        &SkillPolicy::default(),
    );
    let (selected, rejected) = select(&reg, "Update the NOTES to match our house style", &[]);
    assert_eq!(selected.len(), 1, "{selected:?}");
    assert_eq!(selected[0].name, "notes-style");
    assert_eq!(
        selected[0].reason,
        SelectionReason::Trigger {
            phrase: "notes".into()
        }
    );
    assert!(rejected.is_empty());
    // Explicit selection: the enabled one; the unsigned one refused as not
    // enabled with the registry's note; an unknown name refused as unknown;
    // a system-only skill can be named explicitly.
    let (selected, rejected) = select(
        &reg,
        "unrelated goal",
        &[
            "notes-style".into(),
            "draft-style".into(),
            "nope".into(),
            "system-only".into(),
        ],
    );
    assert_eq!(
        selected
            .iter()
            .map(|s| (s.name.as_str(), &s.reason))
            .collect::<Vec<_>>(),
        [
            ("notes-style", &SelectionReason::Explicit),
            ("system-only", &SelectionReason::Explicit)
        ]
    );
    assert_eq!(rejected.len(), 2, "{rejected:?}");
    assert!(matches!(
        rejected[0].error,
        SkillError::NotEnabled {
            lifecycle: Lifecycle::Incubator,
            ..
        }
    ));
    assert!(matches!(rejected[1].error, SkillError::Unknown { .. }));
}

#[test]
fn the_compiler_injects_bounded_instructions_and_never_widens_the_projection() {
    let root = tempfile::tempdir().unwrap();
    let dir = package_dir(root.path(), "notes-style");
    let pkg = load_package(&dir).unwrap();
    let projection: Vec<String> = ["fs.read", "fs.list", "search.exact", "plan.update"]
        .iter()
        .map(|s| (*s).to_owned())
        .collect();
    let compiled = compile(&pkg, &projection, 4096);
    assert_eq!(compiled.tool_projection, ["fs.read"]);
    assert_eq!(compiled.tools_unavailable, ["change.apply", "git.worktree"]);
    assert!(
        compiled
            .instructions
            .starts_with("# Skill: notes-style v1.2.0\n")
    );
    assert!(
        compiled
            .instructions
            .contains("Keep headings in sentence case.")
    );
    assert!(
        compiled
            .instructions
            .contains("names change.apply, git.worktree which this task's policy does not offer"),
        "{}",
        compiled.instructions
    );
    assert!(compiled.instructions.contains("proc.exec: retitle"));
    assert!(!compiled.instructions_truncated);
    assert_eq!(compiled.procedures[0].0, "retitle");
    assert_eq!(compiled.resources[0].path, "style.md");
    // A toolset in the projection expands to its members.
    let wide: Vec<String> = [
        "fs.read",
        "change.apply",
        "git.worktree.create",
        "git.worktree.close",
    ]
    .iter()
    .map(|s| (*s).to_owned())
    .collect();
    let compiled = compile(&pkg, &wide, 4096);
    assert_eq!(
        compiled.tool_projection,
        [
            "change.apply",
            "fs.read",
            "git.worktree.close",
            "git.worktree.create"
        ]
    );
    assert!(compiled.tools_unavailable.is_empty());
    // The budget bounds the instructions and says so; the hash is of what
    // was injected.
    let small = compile(&pkg, &wide, 80);
    assert!(small.instructions_truncated);
    assert!(small.instructions.contains("cut at the budget"));
    assert_ne!(small.instructions_hash, compiled.instructions_hash);
}
