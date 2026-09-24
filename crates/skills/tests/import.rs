//! REQ-EV-0137 / REQ-EV-0183: the importer on the committed compatibility
//! fixture (`tests/fixtures/agent-configs/mixed`) — every item labelled, the
//! extension it writes exactly what the report says.

use std::path::{Path, PathBuf};

use modbit_skills::import::{Existing, ItemKind, ItemStatus, import};

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/agent-configs/mixed")
        .canonicalize()
        .expect("fixture")
}

fn status_of(
    r: &modbit_skills::import::ImportReport,
    source: &str,
) -> (ItemStatus, String, String) {
    let i = r
        .items
        .iter()
        .find(|i| i.source == source)
        .unwrap_or_else(|| panic!("{source} not in the report: {:#?}", r.items));
    (i.status, i.target.clone(), i.reason.clone())
}

#[test]
fn the_compatibility_fixture_imports_and_every_item_is_labelled() {
    let out_root = tempfile::tempdir().unwrap();
    let out = out_root.path().join("mixed");
    let r = import(&fixture(), "mixed", &out, &Existing::default(), false).expect("imported");
    for f in [
        "agents-md",
        "claude",
        "cursor",
        "copilot",
        "gemini",
        "vscode",
    ] {
        assert!(r.formats.iter().any(|x| x == f), "{f}: {:?}", r.formats);
    }
    use ItemStatus::{Conflict, Mapped, Skipped};
    let expect: &[(&str, ItemStatus, &str)] = &[
        ("AGENTS.md", Mapped, "rules/imported-agents.md"),
        ("CLAUDE.md", Mapped, "rules/imported-claude.md"),
        (".cursorrules", Mapped, "rules/imported-cursorrules.md"),
        (
            ".github/copilot-instructions.md",
            Mapped,
            "rules/imported-copilot-instructions.md",
        ),
        ("pkg/AGENTS.md", Mapped, "rules/imported-pkg-agents.md"),
        (
            ".cursor/rules/typescript.mdc",
            Mapped,
            "rules/imported-cursor-typescript.md",
        ),
        (".claude/commands/review.md", Mapped, "command:review"),
        (
            ".claude/commands/ops/status.md",
            Mapped,
            "command:ops-status",
        ),
        (".claude/commands/deploy.md", Skipped, ""),
        (".gemini/commands/explain.toml", Skipped, ""),
        (".claude/agents/reviewer.md", Mapped, "agents/reviewer.md"),
        (".claude/agents/rooted.md", Skipped, ""),
        (".claude/skills/pdf", Mapped, "skills/pdf-extract"),
        (".cursor/mcp.json#docs", Mapped, "tool:docs"),
        (".cursor/mcp.json#remote", Skipped, ""),
        (".vscode/mcp.json#docs", Conflict, ""),
        (".claude/settings.json#hooks.PreToolUse", Skipped, ""),
        (".claude/settings.json#permissions", Skipped, ""),
        (".claude/settings.json#model", Skipped, ""),
    ];
    for (source, status, target) in expect {
        let (s, t, reason) = status_of(&r, source);
        assert_eq!((s, t.as_str()), (*status, *target), "{source}: {reason}");
        assert!(
            !reason.is_empty() || s == Mapped,
            "{source}: a skip or conflict says why"
        );
    }
    assert_eq!(r.items.len(), expect.len(), "{:#?}", r.items);
    // The reasons say what a reader needs to know.
    assert!(
        status_of(&r, "CLAUDE.md")
            .2
            .contains("`@file` imports are not followed")
    );
    assert!(
        status_of(&r, ".claude/commands/deploy.md")
            .2
            .contains("shell")
    );
    assert!(
        status_of(&r, ".claude/commands/review.md")
            .2
            .contains("grants nothing")
    );
    assert!(
        status_of(&r, ".claude/agents/rooted.md")
            .2
            .contains("permissions")
    );
    assert!(
        status_of(&r, ".claude/skills/pdf")
            .2
            .contains("scripts/extract.py")
    );
    assert!(
        status_of(&r, ".vscode/mcp.json#docs")
            .2
            .contains("differently")
    );

    // What was written is what the report says, and the manifest lists every
    // file by digest.
    let manifest: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(out.join("modbit-extension.json")).unwrap())
            .unwrap();
    assert_eq!(manifest["name"], "mixed");
    assert_eq!(manifest["publisher"], "");
    let files = manifest["files"].as_object().unwrap();
    for (path, digest) in files {
        let bytes = std::fs::read(out.join(path)).unwrap();
        assert_eq!(
            digest.as_str().unwrap(),
            hex::encode(<sha2::Sha256 as sha2::Digest>::digest(&bytes)),
            "{path}"
        );
    }
    assert!(files.contains_key("IMPORT_REPORT.json"));
    assert!(files.contains_key("skills/pdf-extract/resources/reference.md"));
    assert!(
        !files.keys().any(|p| p.contains("extract.py")),
        "no script imported"
    );
    let review = manifest["commands"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "review")
        .unwrap();
    assert_eq!(
        review["template"].as_str().unwrap().trim(),
        "Review {{arguments}} for defects and missing tests. Report findings only."
    );
    let rule = std::fs::read_to_string(out.join("rules/imported-cursor-typescript.md")).unwrap();
    assert!(
        rule.starts_with(
            "---\nid: imported-cursor-typescript\npaths: [src/**/*.ts, test/**/*.ts]\n---\n"
        ),
        "{rule}"
    );
    let skill = std::fs::read_to_string(out.join("skills/pdf-extract/SKILL.md")).unwrap();
    let (m, _) = modbit_skills::parse_skill_md(&skill).expect("a valid Modbit skill");
    assert_eq!(
        (m.name.as_str(), m.version.as_str()),
        ("pdf-extract", "imported")
    );
    let agent = std::fs::read_to_string(out.join("agents/reviewer.md")).unwrap();
    let p = modbit_domain::agent_profile::parse_profile(&agent).expect("a valid profile");
    assert_eq!(p.tools, vec!["fs.read", "search.regex", "search.paths"]);
    assert_eq!(p.source, "claude");

    // What already exists wins: the same import against a destination that
    // has a `reviewer` profile and a `docs` server labels both CONFLICT.
    let existing = Existing {
        agents: ["reviewer".to_owned()].into(),
        servers: ["docs".to_owned()].into(),
        ..Existing::default()
    };
    let again = import(&fixture(), "mixed", &out, &existing, true).expect("replaced");
    assert_eq!(
        status_of(&again, ".claude/agents/reviewer.md").0,
        ItemStatus::Conflict
    );
    assert_eq!(
        status_of(&again, ".cursor/mcp.json#docs").0,
        ItemStatus::Conflict
    );
    assert!(!out.join("agents/reviewer.md").exists());
    // And an existing import is not overwritten unless asked.
    assert!(import(&fixture(), "mixed", &out, &Existing::default(), false).is_err());
    assert!(r.items.iter().all(|i| i.kind != ItemKind::Other));
}
