//! PX-066 (QUAL-PX-066 "the same from the CLI"): `worktree apply` / `worktree
//! undo` of the real `modbit-cli` against the real `modbit-core` binary and
//! real Git. The CLI holds no apply logic; every assertion below is on file
//! hashes of the real checkout (the user's working tree), compared after the
//! apply, after each conflict option and after each Undo.
//!
//! SIGKILL-mid-apply recovery is a Core property and is proven by the Core's
//! own test (`services/modbit-core/tests/worktrees_px.rs`,
//! `px_066_a_core_killed_*`); it is not re-proven here.

use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

const TEN: &str = "one\ntwo\nthree\nfour\nfive\nsix\nseven\neight\nnine\nten\n";

fn core_bin() -> PathBuf {
    let cli = PathBuf::from(env!("CARGO_BIN_EXE_modbit-cli"));
    let core = cli.parent().unwrap().join(if cfg!(windows) {
        "modbit-core.exe"
    } else {
        "modbit-core"
    });
    assert!(
        core.exists(),
        "{} (build modbit-core first)",
        core.display()
    );
    core
}

struct Out {
    code: i32,
    stdout: String,
    all: String,
}

struct Fx {
    core: PathBuf,
    data_dir: PathBuf,
    root: PathBuf,
    sid: String,
    _tmp: tempfile::TempDir,
}

fn git(dir: &Path, args: &[&str]) {
    assert!(
        Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .status()
            .unwrap()
            .success()
    );
}

fn tree_hashes(root: &Path) -> BTreeMap<String, String> {
    fn walk(root: &Path, dir: &Path, out: &mut BTreeMap<String, String>) {
        let Ok(rd) = std::fs::read_dir(dir) else {
            return;
        };
        for e in rd.flatten() {
            let p = e.path();
            if e.file_name() == ".git" {
                continue;
            }
            let Ok(m) = std::fs::symlink_metadata(&p) else {
                continue;
            };
            if m.is_dir() {
                walk(root, &p, out);
            } else if m.is_file() {
                let rel = p
                    .strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/");
                out.insert(rel, hex::encode(Sha256::digest(std::fs::read(&p).unwrap())));
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(root, root, &mut out);
    out
}

fn write(p: &Path, s: &str) {
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, s).unwrap();
}

fn text(p: &Path) -> String {
    std::fs::read_to_string(p).unwrap()
}

impl Fx {
    fn new() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let data_dir = tmp.path().join("profile");
        std::fs::create_dir_all(&data_dir).unwrap();
        let repo = tmp.path().join("repo");
        for (p, c) in [
            ("a.txt", TEN),
            ("c.txt", "to be deleted\n"),
            ("keep.txt", "untouched\n"),
            (
                ".modbit/verification.json",
                r#"{"commands": [{"id": "fixture-noop", "argv": ["git", "--version"]}]}"#,
            ),
        ] {
            write(&repo.join(p), c);
        }
        git(&repo, &["init", "-q", "-b", "main"]);
        git(&repo, &["config", "core.autocrlf", "false"]);
        git(&repo, &["add", "-A"]);
        git(
            &repo,
            &[
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@e",
                "commit",
                "-q",
                "-m",
                "base",
            ],
        );
        let root = repo.canonicalize().unwrap();
        let mut fx = Fx {
            core: core_bin(),
            data_dir,
            root,
            sid: String::new(),
            _tmp: tmp,
        };
        let o = fx.cli(&["session", "create"]);
        assert_eq!(o.code, 0, "{}", o.all);
        fx.sid = o.stdout.trim().strip_prefix("session ").unwrap().to_owned();
        fx
    }

    fn cli(&self, args: &[&str]) -> Out {
        let out = Command::new(env!("CARGO_BIN_EXE_modbit-cli"))
            .env("MODBIT_CORE_BIN", &self.core)
            .arg("--data-dir")
            .arg(&self.data_dir)
            .args(args)
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&out.stdout).to_string();
        let all = format!("{stdout}{}", String::from_utf8_lossy(&out.stderr));
        Out {
            code: out.status.code().unwrap_or(-1),
            stdout,
            all,
        }
    }

    fn ws(&self) -> String {
        self.root
            .to_string_lossy()
            .trim_start_matches(r"\\?\")
            .to_owned()
    }

    /// An isolated task whose worktree holds: a.txt edited on its first
    /// line, new/dir/b.txt added, c.txt deleted. Returns the task id and the
    /// worktree path.
    fn task_with_changes(&self) -> (String, PathBuf) {
        let ws = self.ws();
        let o = self.cli(&[
            "task",
            "create",
            "--session",
            &self.sid,
            "--workspace",
            &ws,
            "--profile",
            "local_trusted",
            "--isolation",
            "worktree",
            "a goal",
        ]);
        assert_eq!(o.code, 0, "{}", o.all);
        let task = o.stdout.trim().strip_prefix("task ").unwrap().to_owned();
        for (call, args) in [
            (
                "a.txt",
                serde_json::json!({"path": "a.txt", "op": "replace",
                    "content": "ONE\ntwo\nthree\nfour\nfive\nsix\nseven\neight\nnine\nten\n"}),
            ),
            (
                "new/dir/b.txt",
                serde_json::json!({"path": "new/dir/b.txt", "op": "replace",
                    "content": "added by the task\n"}),
            ),
            (
                "c.txt",
                serde_json::json!({"path": "c.txt", "op": "delete"}),
            ),
        ] {
            let o = self.cli(&[
                "tool",
                "invoke",
                "--session",
                &self.sid,
                "--task",
                &task,
                "change.apply",
                &args.to_string(),
            ]);
            assert!(
                o.code == 0 && o.stdout.contains("status=SUCCESS"),
                "{call}: {}",
                o.all
            );
        }
        let o = self.cli(&["worktree", "list", "--task", &task, "--json"]);
        assert_eq!(o.code, 0, "{}", o.all);
        let l = json(&o.stdout);
        let w = &l["worktrees"][0];
        assert_eq!(w["task_id"], task.as_str());
        assert_eq!(w["unapplied"], true);
        (task, PathBuf::from(w["path"].as_str().unwrap()))
    }

    fn apply(&self, task: &str, extra: &[&str]) -> (i32, serde_json::Value, String) {
        let mut a = vec![
            "worktree",
            "apply",
            "--session",
            &self.sid,
            "--task",
            task,
            "--json",
        ];
        a.extend_from_slice(extra);
        let o = self.cli(&a);
        (o.code, json(&o.stdout), o.all)
    }

    fn undo(&self, task: &str) -> (i32, serde_json::Value, String) {
        let o = self.cli(&[
            "worktree",
            "undo",
            "--session",
            &self.sid,
            "--task",
            task,
            "--json",
        ]);
        (o.code, json(&o.stdout), o.all)
    }

    fn approve(&self, pending: &serde_json::Value) {
        let id = pending["approval_id"].as_str().unwrap();
        let intent = pending["intent_hash"].as_str().unwrap();
        let o = self.cli(&[
            "approval",
            "resolve",
            "--session",
            &self.sid,
            "--approval",
            id,
            "--intent",
            intent,
            "approve",
        ]);
        assert_eq!(o.code, 0, "{}", o.all);
    }

    /// Apply to completion through the CLI: classify, approve exactly that
    /// intent through `approval resolve`, apply again.
    fn apply_approved(&self, task: &str, extra: &[&str]) -> serde_json::Value {
        let (code, first, all) = self.apply(task, extra);
        assert_eq!(
            (code, first["status"].as_str()),
            (2, Some("APPROVAL_PENDING")),
            "{all}"
        );
        self.approve(&first);
        let (code, done, all) = self.apply(task, extra);
        assert_eq!(
            (code, done["status"].as_str()),
            (0, Some("APPLIED")),
            "{all}"
        );
        // The APPLIED ack does not repeat the intent hash (the receipts and
        // the log name it; see the receipt assertions in the clean-apply
        // test); the plan the approval bound is the plan applied.
        assert_eq!(done["plan_digest"], first["plan_digest"]);
        done
    }

    fn undo_to(&self, task: &str, expected: &BTreeMap<String, String>) {
        let (code, u, all) = self.undo(task);
        assert_eq!(
            (code, u["status"].as_str()),
            (2, Some("APPROVAL_PENDING")),
            "{all}"
        );
        self.approve(&u);
        let (code, u, all) = self.undo(task);
        assert_eq!((code, u["status"].as_str()), (0, Some("UNDONE")), "{all}");
        assert_eq!(
            &tree_hashes(&self.root),
            expected,
            "undo restored the user's bytes exactly"
        );
    }
}

fn json(out: &str) -> serde_json::Value {
    let line = out.lines().find(|l| l.starts_with('{')).unwrap_or("null");
    serde_json::from_str(line).unwrap_or_else(|e| panic!("{e}: {out}"))
}

fn head(root: &Path) -> String {
    let o = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["rev-parse", "HEAD"])
        .output()
        .unwrap();
    String::from_utf8_lossy(&o.stdout).trim().to_owned()
}

#[test]
fn qual_px_066_cli_clean_apply_is_hash_exact_receipted_and_undo_restores_the_users_bytes() {
    let fx = Fx::new();
    write(&fx.root.join("keep.txt"), "user edit in keep\n");
    let (task, wt) = fx.task_with_changes();
    let before = tree_hashes(&fx.root);
    let cand = tree_hashes(&wt);

    // No approval, no apply: held on an approval naming the intent; exit 2;
    // the checkout does not change.
    let (code, pending, all) = fx.apply(&task, &[]);
    assert_eq!(code, 2, "{all}");
    assert_eq!(pending["status"], "APPROVAL_PENDING");
    assert!(!pending["intent_hash"].as_str().unwrap().is_empty());
    assert_eq!(tree_hashes(&fx.root), before);
    let listed = fx.cli(&["approval", "list", "--session", &fx.sid]);
    assert!(
        listed
            .stdout
            .contains(pending["approval_id"].as_str().unwrap())
            && listed
                .stdout
                .contains(pending["intent_hash"].as_str().unwrap()),
        "{}",
        listed.all
    );

    // A stale review revision is refused, and nothing changes.
    let rev = pending["candidate_revision"].as_u64().unwrap();
    let (code, stale, all) = fx.apply(&task, &["--revision", &(rev + 7).to_string()]);
    assert_eq!(code, 1, "{all}");
    assert_eq!(stale["status"], "STALE");
    assert_eq!(stale["code"], "STALE_REVISION");
    assert_eq!(tree_hashes(&fx.root), before);

    // Approve exactly that intent; apply.
    fx.approve(&pending);
    let (code, done, all) = fx.apply(&task, &[]);
    assert_eq!(
        (code, done["status"].as_str()),
        (0, Some("APPLIED")),
        "{all}"
    );
    let after = tree_hashes(&fx.root);
    assert_eq!(after["a.txt"], cand["a.txt"]);
    assert_eq!(after["new/dir/b.txt"], cand["new/dir/b.txt"]);
    assert!(!after.contains_key("c.txt"));
    assert_eq!(
        after["keep.txt"], before["keep.txt"],
        "the user's edit is untouched"
    );

    // The receipt names the intent hash and both revisions (the candidate's
    // and the checkout's head before the apply).
    assert_eq!(done["plan_digest"], pending["plan_digest"]);
    assert_eq!(done["candidate_revision"], pending["candidate_revision"]);
    assert!(!done["candidate_tree"].as_str().unwrap().is_empty());
    assert_eq!(done["checkout_head_before"], head(&fx.root).as_str());
    assert!(!done["effect_receipt_ids"].as_array().unwrap().is_empty());
    let ev = fx.cli(&["events", "tail", "--session", &fx.sid, "--json"]);
    let intent = pending["intent_hash"].as_str().unwrap();
    assert!(
        ev.stdout.contains("EffectReceiptAppended") && ev.stdout.contains(intent),
        "the effect receipt names the intent hash:\n{}",
        ev.all
    );
    assert!(
        ev.stdout.contains("WorktreeApplied")
            && ev.stdout.contains(done["candidate_tree"].as_str().unwrap())
            && ev
                .stdout
                .contains(done["checkout_head_before"].as_str().unwrap()),
        "{}",
        ev.all
    );

    // Applying again changes nothing.
    let (code, again, all) = fx.apply(&task, &[]);
    assert_eq!(
        (code, again["status"].as_str()),
        (0, Some("NOTHING_TO_APPLY")),
        "{all}"
    );
    assert_eq!(tree_hashes(&fx.root), after);

    // Undo needs its own approval, then restores every byte.
    fx.undo_to(&task, &before);
    assert!(!fx.root.join("new").exists());
}

#[test]
fn qual_px_066_cli_every_conflict_option_is_exact_and_undo_after_a_second_edit_destroys_nothing() {
    let fx = Fx::new();
    let (task, wt) = fx.task_with_changes();
    // The user edits the same line in the checkout after the task started.
    write(
        &fx.root.join("a.txt"),
        "uno\ntwo\nthree\nfour\nfive\nsix\nseven\neight\nnine\nten\n",
    );
    write(&fx.root.join("keep.txt"), "user edit in keep\n");
    write(&fx.root.join("scratch.txt"), "untracked\n");
    let before = tree_hashes(&fx.root);

    // The first call is the typed conflict result (exit 3), changes nothing.
    let (code, c, all) = fx.apply(&task, &[]);
    assert_eq!((code, c["status"].as_str()), (3, Some("CONFLICT")), "{all}");
    let view = &c["conflict"];
    assert_eq!(view["conflicting_paths"], serde_json::json!(["a.txt"]));
    assert_eq!(view["default_option"], "CANCEL");
    let opt = |name: &str| {
        view["options"]
            .as_array()
            .unwrap()
            .iter()
            .find(|o| o["option"] == name)
            .unwrap()
            .clone()
    };
    assert_eq!(opt("OVERWRITE")["destructive"], true);
    assert_eq!(
        opt("OVERWRITE")["confirm_paths"],
        serde_json::json!(["a.txt"])
    );
    assert_eq!(
        opt("FULL_OVERWRITE")["confirm_paths"],
        serde_json::json!(["a.txt", "keep.txt", "scratch.txt"])
    );
    assert_eq!(opt("UNDO_AND_APPLY")["available"], false);
    assert_eq!(tree_hashes(&fx.root), before);
    // The human output names the options, marks the default and the
    // destructive ones.
    let o = fx.cli(&["worktree", "apply", "--session", &fx.sid, "--task", &task]);
    assert_eq!(o.code, 3, "{}", o.all);
    assert!(
        o.stdout.contains("option CANCEL [default]")
            && o.stdout.contains("option OVERWRITE [destructive]"),
        "{}",
        o.all
    );
    let o = fx.cli(&["approval", "list", "--session", &fx.sid]);
    assert!(
        !o.stdout.contains("status=REQUESTED"),
        "no approval was asked: {}",
        o.all
    );

    // Cancel is a no-op.
    let (code, r, all) = fx.apply(&task, &["--option", "cancel"]);
    assert_eq!(
        (code, r["status"].as_str()),
        (0, Some("CANCELLED")),
        "{all}"
    );
    assert_eq!(tree_hashes(&fx.root), before);
    // An overwrite without the files typed is refused by the Core before any
    // approval is asked.
    for (option, confirm) in [
        ("OVERWRITE", vec![]),
        ("OVERWRITE", vec!["--confirm", "keep.txt"]),
        ("FULL_OVERWRITE", vec!["--confirm", "a.txt"]),
    ] {
        let mut extra = vec!["--option", option];
        extra.extend(confirm.iter().copied());
        let (code, r, all) = fx.apply(&task, &extra);
        assert_eq!(code, 1, "{option} {confirm:?}: {all}");
        assert_eq!(r["status"], "REFUSED", "{all}");
        assert_eq!(r["code"], "OVERWRITE_NOT_CONFIRMED", "{all}");
    }
    assert_eq!(tree_hashes(&fx.root), before);
    let o = fx.cli(&["approval", "list", "--session", &fx.sid]);
    assert!(
        !o.stdout.contains("status=REQUESTED"),
        "no approval was asked: {}",
        o.all
    );

    // MERGE_MANUALLY: markers in the conflicting file; the rest applied.
    let done = fx.apply_approved(&task, &["--option", "MERGE_MANUALLY"]);
    assert_eq!(done["unresolved_paths"], serde_json::json!(["a.txt"]));
    let merged = text(&fx.root.join("a.txt"));
    assert!(
        merged.contains("<<<<<<< checkout") && merged.contains("uno") && merged.contains("ONE"),
        "{merged}"
    );
    assert_eq!(text(&fx.root.join("new/dir/b.txt")), "added by the task\n");
    fx.undo_to(&task, &before);

    // OVERWRITE with the file typed: the conflicting file only.
    fx.apply_approved(&task, &["--option", "OVERWRITE", "--confirm", "a.txt"]);
    let after = tree_hashes(&fx.root);
    assert_eq!(after["a.txt"], tree_hashes(&wt)["a.txt"]);
    assert_eq!(after["keep.txt"], before["keep.txt"]);
    assert_eq!(after["scratch.txt"], before["scratch.txt"]);
    fx.undo_to(&task, &before);

    // STASH: the user's dirty state is kept in a snapshot; the change
    // applies on the committed checkout.
    let done = fx.apply_approved(&task, &["--option", "STASH"]);
    let stash = done["stash_ref"].as_str().unwrap();
    assert!(!stash.is_empty(), "{done}");
    let shown = Command::new("git")
        .arg("-C")
        .arg(&fx.root)
        .args(["show", &format!("{stash}:keep.txt")])
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8_lossy(&shown.stdout).trim(),
        "user edit in keep"
    );
    assert_eq!(text(&fx.root.join("keep.txt")), "untouched\n");
    assert!(!fx.root.join("scratch.txt").exists());
    fx.undo_to(&task, &before);

    // FULL_OVERWRITE with every file typed.
    fx.apply_approved(
        &task,
        &[
            "--option",
            "FULL_OVERWRITE",
            "--confirm",
            "a.txt",
            "--confirm",
            "keep.txt",
            "--confirm",
            "scratch.txt",
        ],
    );
    assert!(!fx.root.join("scratch.txt").exists());
    fx.undo_to(&task, &before);

    // Undo after a second user edit reports the conflict and destroys nothing.
    fx.apply_approved(&task, &["--option", "OVERWRITE", "--confirm", "a.txt"]);
    write(&fx.root.join("a.txt"), "ONE and then my own second edit\n");
    let held = tree_hashes(&fx.root);
    let (_, u, all) = fx.undo(&task);
    assert_eq!(u["status"], "APPROVAL_PENDING", "{all}");
    fx.approve(&u);
    let (code, u, all) = fx.undo(&task);
    assert_eq!(
        (code, u["status"].as_str()),
        (3, Some("UNDO_CONFLICT")),
        "{all}"
    );
    assert_eq!(u["divergent_paths"], serde_json::json!(["a.txt"]));
    assert_eq!(tree_hashes(&fx.root), held, "nothing was changed");
}
