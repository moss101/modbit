//! REQ-PX-052 (QUAL-PX-052) on the real Core over its real socket: the slash
//! menu's inventory is one typed union of the skills, the extension commands
//! and the subagent profiles the Core really holds — ordered built-ins first,
//! then a divider, then the rest alphabetically — metadata only, scoped to the
//! caller's session, rebuilt from disk on every read, with the state of an
//! entry that cannot be used (an untrusted skill, a quarantined extension's
//! command, an unreadable profile) shown instead of hidden.
//!
//! Real: the `modbit-core` binary, the skill registry and trust files on
//! disk, signed and unsigned extensions loaded through `LoadExtension`, the
//! profile directories. No model is involved.
#![cfg(unix)]

mod px_common;

use modbit_protocol::client::{Client, ClientError};
use modbit_protocol::v1::{
    Id, InspectExtension, ListSkills, LoadExtension, SkillList, SlashEntry, TrustSkill,
};
use prost::Message;
use px_common::*;
use serde_json::json;

fn skill_pkg(root: &std::path::Path, name: &str, front: &str, body: &str) -> String {
    let d = root.join(name);
    std::fs::create_dir_all(&d).unwrap();
    std::fs::write(
        d.join("SKILL.md"),
        format!("---\nname: {name}\nversion: 1.0.0\n{front}\n---\n{body}\n"),
    )
    .unwrap();
    modbit_skills::load_package(&d).unwrap().content_hash
}

fn profile(root: &std::path::Path, name: &str, description: &str, body: &str) {
    std::fs::create_dir_all(root).unwrap();
    std::fs::write(
        root.join(format!("{name}.md")),
        format!("---\nname: {name}\ndescription: {description}\n---\n{body}\n"),
    )
    .unwrap();
}

fn extension(
    root: &std::path::Path,
    dir: &str,
    manifest: &serde_json::Value,
    publisher: Option<(&str, &ed25519_dalek::SigningKey)>,
) -> String {
    use ed25519_dalek::Signer;
    let d = root.join(dir);
    std::fs::create_dir_all(&d).unwrap();
    let text = manifest.to_string();
    std::fs::write(d.join("modbit-extension.json"), &text).unwrap();
    if let Some((id, key)) = publisher {
        std::fs::write(
            d.join("modbit-extension.sig"),
            json!({"key_id": id, "signature_hex": hex::encode(key.sign(text.as_bytes()).to_bytes())})
                .to_string(),
        )
        .unwrap();
    }
    d.to_string_lossy().into_owned()
}

async fn list(
    c: &mut Client,
    task: Option<&Id>,
    envelope_session: Option<&Id>,
) -> Result<SkillList, ClientError> {
    let mut e = envelope(
        rand_id(),
        "ListSkills",
        ListSkills {
            task_id: task.cloned(),
        }
        .encode_to_vec(),
    );
    e.session_id = envelope_session.cloned();
    let ack = c.command(e).await?;
    Ok(Client::result(&ack).unwrap())
}

fn entry<'a>(l: &'a SkillList, kind: &str, id: &str) -> &'a SlashEntry {
    l.slash
        .iter()
        .find(|e| e.kind == kind && e.id == id)
        .unwrap_or_else(|| panic!("no {kind} {id} in {:#?}", l.slash))
}

fn order(l: &SkillList) -> Vec<String> {
    l.slash.iter().map(|e| e.display_name.clone()).collect()
}

/// The union, its order and divider, its states, its scoping, and that it is
/// rebuilt from what is on disk now.
#[tokio::test(flavor = "multi_thread")]
#[allow(clippy::too_many_lines)]
async fn qual_px_052_the_slash_inventory_is_one_ordered_union_of_what_the_core_holds() {
    let (repo, root) = plain_repo(&[("NOTES.md", "# notes\n")]);
    let dir = tempfile::tempdir().unwrap();
    let system = tempfile::tempdir().unwrap();
    let exts = tempfile::tempdir().unwrap();
    const BODY: &str = "SKILL-BODY-MARKER-0001";
    const TEMPLATE: &str = "COMMAND-TEMPLATE-MARKER-0002 {{arguments}}";
    const CONTEXT: &str = "PROFILE-CONTEXT-MARKER-0003";

    // Skills: a System one (built in), a trusted and an untrusted user skill, a project one.
    skill_pkg(
        system.path(),
        "core-guide",
        "description: how the product itself works",
        BODY,
    );
    let h_alpha = skill_pkg(
        &dir.path().join("skills"),
        "alpha-user",
        "description: a user skill the owner trusts",
        BODY,
    );
    skill_pkg(
        &dir.path().join("skills"),
        "zeta-user",
        "description: a user skill nobody vouches for\nmodel_invocable: false",
        BODY,
    );
    skill_pkg(
        &repo.path().join(".modbit").join("skills"),
        "repo-skill",
        "description: a skill the repository ships",
        BODY,
    );
    // Profiles: the operator's, the project's, an active extension's, and a broken one.
    profile(
        &dir.path().join("agents"),
        "reviewer",
        "reviews a diff",
        CONTEXT,
    );
    profile(
        &repo.path().join(".modbit").join("agents"),
        "auditor",
        "audits dependencies",
        CONTEXT,
    );
    std::fs::write(
        dir.path().join("agents").join("broken.md"),
        "no front matter at all\n",
    )
    .unwrap();
    // Extensions: a signed one (active) with a command and a profile, an unsigned one (quarantined).
    let publisher = ed25519_dalek::SigningKey::from_bytes(&[52u8; 32]);
    let keys = format!("acme:{}", hex::encode(publisher.verifying_key().to_bytes()));
    profile(
        &exts.path().join("kit").join("agents"),
        "linter",
        "lints a file",
        CONTEXT,
    );
    let linter_sha = {
        use sha2::Digest;
        hex::encode(sha2::Sha256::digest(
            std::fs::read(exts.path().join("kit").join("agents").join("linter.md")).unwrap(),
        ))
    };
    let kit = extension(
        exts.path(),
        "kit",
        &json!({
            "name": "kit", "version": "1.2.0", "publisher": "Acme", "source": "https://example.test/kit",
            "commands": [{"name": "tidy", "description": "tidy up a file", "template": TEMPLATE}],
            "files": {"agents/linter.md": linter_sha},
        }),
        Some(("acme", &publisher)),
    );
    let wild = extension(
        exts.path(),
        "wild",
        &json!({
            "name": "wild", "version": "0.1.0", "publisher": "Nobody", "source": "https://example.test/wild",
            "commands": [{"name": "wipe", "description": "wipe everything", "template": TEMPLATE}],
        }),
        None,
    );

    let core = CoreProcess::spawn_with_env(
        dir.path(),
        &[
            ("MODBIT_SYSTEM_SKILLS", system.path().to_str().unwrap()),
            ("MODBIT_EXTENSION_KEYS", keys.as_str()),
        ],
    );
    let mut c = core.client().await;
    let (session, g) = session_with_lease(&mut c, 0x30).await;
    let task = create_task(&mut c, &session, g, &root, 0x41, "local_trusted", "tidy").await;
    for (path, id) in [(&kit, 0x51u8), (&wild, 0x52)] {
        let ack = c
            .command(envelope_fenced(
                id16(id),
                "InspectExtension",
                InspectExtension { path: path.clone() }.encode_to_vec(),
                g,
            ))
            .await
            .unwrap();
        let seen: modbit_protocol::v1::ExtensionInspectionView = Client::result(&ack).unwrap();
        c.command(envelope_fenced(
            id16(id + 0x10),
            "LoadExtension",
            LoadExtension {
                session_id: Some(session.clone()),
                path: path.clone(),
                expected_digest: seen.manifest_digest,
            }
            .encode_to_vec(),
            g,
        ))
        .await
        .unwrap();
    }
    // The owner trusts one user skill by its hash.
    c.command(envelope(
        rand_id(),
        "TrustSkill",
        TrustSkill {
            name: "alpha-user".into(),
            content_hash: h_alpha,
            task_id: Some(task.clone()),
        }
        .encode_to_vec(),
    ))
    .await
    .unwrap();

    // ---- without a task: the profile's and the System scope's, no commands.
    let bare = list(&mut c, None, None).await.unwrap();
    assert_eq!(order(&bare)[0], "core-guide", "{:#?}", order(&bare));
    assert!(entry(&bare, "SKILL", "core-guide").built_in);
    assert_eq!(bare.slash_divider_at, 1);
    assert!(bare.slash.iter().all(|e| e.kind != "COMMAND"));
    assert!(
        bare.slash
            .iter()
            .all(|e| e.id != "repo-skill" && e.id != "auditor"),
        "a task's project scope is not in a taskless inventory"
    );
    assert_eq!(entry(&bare, "SUBAGENT", "reviewer").scope, "USER");

    // ---- with a task: the whole union.
    let l = list(&mut c, Some(&task), None).await.unwrap();
    assert_eq!(
        order(&l),
        [
            "core-guide", // built in, before the divider
            "alpha-user",
            "auditor",
            "kit/tidy",
            "linter",
            "repo-skill",
            "reviewer",
            "wild/wipe",
            "zeta-user",
        ],
        "built-ins, a divider, then the rest alphabetically"
    );
    assert_eq!(l.slash_divider_at, 1);
    // Skills carry what ListSkills says of them.
    let alpha = entry(&l, "SKILL", "alpha-user");
    assert_eq!(
        (alpha.scope.as_str(), alpha.enabled),
        ("USER", true),
        "{alpha:?}"
    );
    assert_eq!(alpha.invocation, "BOTH");
    let zeta = entry(&l, "SKILL", "zeta-user");
    assert_eq!(
        (zeta.trust.as_str(), zeta.enabled, zeta.invocation.as_str()),
        ("UNTRUSTED", false, "USER_ONLY")
    );
    assert!(
        zeta.trust_detail.contains("skill trust zeta-user@"),
        "{zeta:?}"
    );
    assert_eq!(entry(&l, "SKILL", "repo-skill").scope, "PROJECT");
    for s in &l.skills {
        let e = entry(&l, "SKILL", &s.name);
        assert_eq!(
            (
                &e.scope,
                &e.trust,
                e.enabled,
                &e.invocation,
                &e.content_hash
            ),
            (
                &s.scope,
                &s.trust,
                s.enabled,
                &s.invocation,
                &s.content_hash
            ),
            "the union is a view of the skill registry, not a second one"
        );
    }
    // Commands: active, and quarantined (listed, not usable).
    let tidy = entry(&l, "COMMAND", "kit/tidy");
    assert_eq!(
        (tidy.scope.as_str(), tidy.enabled, tidy.invocation.as_str()),
        ("EXTENSION", true, "USER_ONLY")
    );
    assert_eq!(tidy.description, "tidy up a file");
    assert!(tidy.trust.contains("VERIFIED"), "{tidy:?}");
    assert_eq!(tidy.provenance_source, "extension:kit@1.2.0");
    let wipe = entry(&l, "COMMAND", "wild/wipe");
    assert_eq!(
        (wipe.enabled, wipe.trust.as_str()),
        (false, "QUARANTINED"),
        "{wipe:?}"
    );
    assert!(wipe.trust_detail.contains("unsigned"), "{wipe:?}");
    // Profiles: scope by where they are read from; the model's, not the person's.
    assert_eq!(entry(&l, "SUBAGENT", "auditor").scope, "PROJECT");
    assert_eq!(entry(&l, "SUBAGENT", "linter").scope, "EXTENSION");
    let reviewer = entry(&l, "SUBAGENT", "reviewer");
    assert_eq!(
        (
            reviewer.scope.as_str(),
            reviewer.invocation.as_str(),
            reviewer.enabled
        ),
        ("USER", "MODEL_ONLY", true)
    );
    assert_eq!(reviewer.content_hash.len(), 64);
    // The unreadable profile is told about, not listed.
    assert!(
        l.rejected
            .iter()
            .any(|r| r.code == "INVALID_PROFILE" && r.source.ends_with("broken.md")),
        "{:#?}",
        l.rejected
    );
    assert!(l.slash.iter().all(|e| e.id != "broken"));
    // Metadata only: no body, template or context crosses the wire.
    let wire = format!("{l:?}");
    for marker in [BODY, "COMMAND-TEMPLATE-MARKER", CONTEXT] {
        assert!(!wire.contains(marker), "the inventory leaked {marker}");
    }

    // ---- scoped to the caller's session
    let (other, _) = session_with_lease(&mut c, 0x60).await;
    let refused = list(&mut c, Some(&task), Some(&other)).await;
    match refused {
        Err(ClientError::Rejected { code, .. }) => assert_eq!(code, "WRONG_SESSION"),
        other => panic!("expected WRONG_SESSION, got {other:?}"),
    }
    assert!(list(&mut c, Some(&task), Some(&session)).await.is_ok());

    // ---- rebuilt from disk on every read
    skill_pkg(
        &dir.path().join("skills"),
        "midway-user",
        "description: installed after the last read",
        BODY,
    );
    profile(
        &dir.path().join("agents"),
        "archivist",
        "keeps the notes",
        CONTEXT,
    );
    std::fs::remove_dir_all(dir.path().join("skills").join("zeta-user")).unwrap();
    std::fs::remove_file(dir.path().join("agents").join("reviewer.md")).unwrap();
    let changed = list(&mut c, Some(&task), None).await.unwrap();
    let names = order(&changed);
    assert!(names.contains(&"midway-user".to_owned()), "{names:?}");
    assert!(names.contains(&"archivist".to_owned()), "{names:?}");
    assert!(!names.contains(&"zeta-user".to_owned()), "{names:?}");
    assert!(!names.contains(&"reviewer".to_owned()), "{names:?}");
    let mut rest: Vec<String> = names[1..].iter().map(|n| n.to_lowercase()).collect();
    let sorted = {
        let mut s = rest.clone();
        s.sort();
        s
    };
    assert_eq!(rest, sorted, "still alphabetical after the change");
    rest.clear();
    // One edited byte of a trusted skill makes it untrusted again.
    std::fs::write(
        dir.path().join("skills").join("alpha-user").join("SKILL.md"),
        "---\nname: alpha-user\nversion: 1.0.0\ndescription: a user skill the owner trusts\n---\nEDITED\n",
    )
    .unwrap();
    let edited = list(&mut c, Some(&task), None).await.unwrap();
    let alpha = entry(&edited, "SKILL", "alpha-user");
    assert!(!alpha.enabled, "{alpha:?}");
    assert_ne!(alpha.trust, "TRUSTED_BY_OWNER");
}
