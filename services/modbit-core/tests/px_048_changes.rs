//! PX-048 (the desktop Changes app's actions) against the real Core process
//! over its real socket, a real SQLite event store and real Git repositories
//! with a real bare remote: per-file and per-hunk revert through the Core's
//! ChangeTransaction with file-revision guards and a checkpoint before the
//! write, stage and unstage, and Commit and push as ONE protected effect bound
//! to its exact intent. No model is involved: what is under test is the Core's
//! behaviour, driven through its own commands.

mod px_common;

use std::io::Read;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use modbit_protocol::client::{Client, ClientError};
use modbit_protocol::v1::{
    ApprovalResolvedAck, ChangesState, CheckpointRestoreResult, CommitAndPush, CommitAndPushAck,
    CreateTask, GetChangesState, Id, InvokeTool, ResolveApproval, RestoreCheckpoint, RevertChanges,
    RevertChangesAck, StageChanges, StageChangesAck, StagePath, TaskCreated, TaskIsolation,
    ToolInvoked,
};
use prost::Message;
use px_common::{CoreProcess, envelope_fenced, id16, plain_repo, session_with_lease};
use sha2::{Digest, Sha256};

type Reject = (String, String);

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@e")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@e")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

fn write(p: &Path, s: &str) {
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, s).unwrap();
}

fn text(p: &Path) -> String {
    std::fs::read_to_string(p).unwrap().replace("\r\n", "\n")
}

fn sha(s: &str) -> String {
    hex::encode(Sha256::digest(s.as_bytes()))
}

/// Twelve numbered lines: edits at the top and the bottom are separate hunks.
fn notes() -> String {
    (1..=24).map(|i| format!("line {i}\n")).collect()
}

struct Fx {
    core: CoreProcess,
    c: Client,
    session: Id,
    g: Option<u64>,
    root: String,
    task: Id,
    /// Where the task's files are: the checkout, or its worktree.
    work: PathBuf,
    dir: tempfile::TempDir,
    _repo: tempfile::TempDir,
    next: u8,
}

impl Fx {
    async fn new(files: &[(&str, &str)]) -> Self {
        Self::with_isolation(files, false).await
    }

    /// `isolated`: the task works in its own git worktree, which is where the actions land.
    async fn with_isolation(files: &[(&str, &str)], isolated: bool) -> Self {
        let (repo, root) = plain_repo(files);
        let dir = tempfile::tempdir().unwrap();
        let core = CoreProcess::spawn_with_env(dir.path(), &[]);
        let mut c = core.client().await;
        let (session, g) = session_with_lease(&mut c, 0x11).await;
        let mut fx = Self {
            core,
            c,
            session,
            g,
            task: id16(0),
            work: PathBuf::from(&root),
            root,
            dir,
            _repo: repo,
            next: 0x40,
        };
        let id = fx.n();
        let root = fx.root.clone();
        let ack =
            fx.c.command(envelope_fenced(
                id16(id),
                "CreateTask",
                CreateTask {
                    session_id: Some(fx.session.clone()),
                    goal_text: "edit things".into(),
                    execution_profile: "local_trusted".into(),
                    origin: "cli".into(),
                    workspace_root: root,
                    isolation: if isolated {
                        TaskIsolation::Worktree as i32
                    } else {
                        TaskIsolation::Unspecified as i32
                    },
                    ..Default::default()
                }
                .encode_to_vec(),
                fx.g,
            ))
            .await
            .unwrap();
        fx.task = Client::result::<TaskCreated>(&ack)
            .unwrap()
            .task_id
            .unwrap();
        if isolated {
            let mut b = [0u8; 16];
            b.copy_from_slice(&fx.task.value);
            let h = hex::encode(b);
            let uuid = format!(
                "{}-{}-{}-{}-{}",
                &h[0..8],
                &h[8..12],
                &h[12..16],
                &h[16..20],
                &h[20..32]
            );
            fx.work = fx.dir.path().join("worktrees").join(uuid);
            assert!(
                fx.work.join(".git").exists(),
                "the task has its own worktree"
            );
        }
        fx
    }

    fn n(&mut self) -> u8 {
        self.next = self.next.wrapping_add(1);
        self.next
    }

    fn path(&self, rel: &str) -> PathBuf {
        assert!(
            self.work.is_absolute(),
            "the fixture knows where the task's files are"
        );
        self.work.join(rel)
    }

    /// Kill the Core hard and start another on the same data directory.
    async fn restart(&mut self) {
        self.core.kill();
        self.core = CoreProcess::spawn_with_env(self.dir.path(), &[]);
        self.c = self.core.client().await;
        let n = self.n();
        let ack = self
            .c
            .command(px_common::envelope(
                id16(n.wrapping_add(0x70)),
                "AcquireSessionLease",
                modbit_protocol::v1::AcquireSessionLease {
                    session_id: Some(self.session.clone()),
                    owner: "test2".into(),
                }
                .encode_to_vec(),
            ))
            .await
            .unwrap();
        let l: modbit_protocol::v1::SessionLeaseAcquired = Client::result(&ack).unwrap();
        self.g = Some(l.lease_generation);
    }

    async fn cmd_with<M: Message, R: Message + Default>(
        &mut self,
        id: Id,
        kind: &str,
        msg: M,
        fenced: bool,
    ) -> Result<R, Reject> {
        let g = if fenced { self.g } else { None };
        match self
            .c
            .command(envelope_fenced(id, kind, msg.encode_to_vec(), g))
            .await
        {
            Ok(ack) => Ok(Client::result(&ack).unwrap()),
            Err(ClientError::Rejected { code, message }) => Err((code, message)),
            Err(e) => panic!("{kind}: {e}"),
        }
    }

    async fn cmd<M: Message, R: Message + Default>(
        &mut self,
        kind: &str,
        msg: M,
        fenced: bool,
    ) -> Result<R, Reject> {
        let id = self.n();
        self.cmd_with(id16(id), kind, msg, fenced).await
    }

    async fn state(&mut self) -> ChangesState {
        let task = self.task.clone();
        self.cmd(
            "GetChangesState",
            GetChangesState {
                task_id: Some(task),
            },
            false,
        )
        .await
        .unwrap()
    }

    fn revision_of(state: &ChangesState, path: &str) -> String {
        state
            .paths
            .iter()
            .find(|p| p.path == path)
            .unwrap_or_else(|| panic!("{path} is not in the change set: {:?}", state.paths))
            .file_revision
            .clone()
    }

    async fn revert(
        &mut self,
        path: &str,
        whole: bool,
        hunk: u32,
        revision: &str,
    ) -> Result<RevertChangesAck, Reject> {
        let task = self.task.clone();
        self.cmd(
            "RevertChanges",
            RevertChanges {
                task_id: Some(task),
                path: path.into(),
                whole_file: whole,
                hunk_index: hunk,
                expected_file_revision: revision.into(),
            },
            true,
        )
        .await
    }

    async fn stage(
        &mut self,
        paths: &[(&str, &str)],
        stage: bool,
    ) -> Result<StageChangesAck, Reject> {
        let task = self.task.clone();
        self.cmd(
            "StageChanges",
            StageChanges {
                task_id: Some(task),
                paths: paths
                    .iter()
                    .map(|(p, r)| StagePath {
                        path: (*p).into(),
                        expected_file_revision: (*r).into(),
                    })
                    .collect(),
                stage,
            },
            true,
        )
        .await
    }

    async fn commit_push(&mut self, message: &str) -> Result<CommitAndPushAck, Reject> {
        let task = self.task.clone();
        self.cmd(
            "CommitAndPush",
            CommitAndPush {
                task_id: Some(task),
                message: message.into(),
                ..Default::default()
            },
            true,
        )
        .await
    }

    async fn resolve(
        &mut self,
        approval_id: &str,
        approve: bool,
        intent: &str,
    ) -> Result<(), Reject> {
        let r: Result<ApprovalResolvedAck, Reject> = self
            .cmd(
                "ResolveApproval",
                ResolveApproval {
                    approval_id: Some(Id {
                        value: hex::decode(approval_id.replace('-', "")).unwrap(),
                    }),
                    approve,
                    reason: "test".into(),
                    intent_hash: intent.into(),
                },
                true,
            )
            .await;
        r.map(|_| ())
    }

    /// Commit and push with the approval granted for exactly what was asked.
    async fn commit_push_approved(&mut self, message: &str) -> CommitAndPushAck {
        let first = self.commit_push(message).await.unwrap();
        assert_eq!(first.status, "APPROVAL_PENDING", "{first:?}");
        self.resolve(&first.approval_id, true, &first.intent_hash)
            .await
            .unwrap();
        self.commit_push(message).await.unwrap()
    }

    async fn events(&self) -> Vec<serde_json::Value> {
        px_common::replay(&self.core, &self.session).await
    }
}

/// A bare repository to push to, added as `origin`.
fn add_bare_remote(fx: &Fx) -> tempfile::TempDir {
    let remote = tempfile::tempdir().unwrap();
    git(remote.path(), &["init", "-q", "--bare", "-b", "main"]);
    git(
        Path::new(&fx.root),
        &["remote", "add", "origin", &remote.path().to_string_lossy()],
    );
    remote
}

fn payload_of(e: &serde_json::Value) -> serde_json::Value {
    let p = &e["payload"];
    if p["kind"] == "inline" {
        p["payload"].clone()
    } else {
        p.clone()
    }
}

fn receipts_of(evs: &[serde_json::Value], intent: &str) -> Vec<String> {
    evs.iter()
        .filter(|e| e["event_type"] == "EffectReceiptAppended")
        .map(payload_of)
        .filter(|p| p["receipt"]["intent_hash"] == intent)
        .map(|p| p["receipt"]["status"].as_str().unwrap().to_owned())
        .collect()
}

// ===========================================================================
// Revert
// ===========================================================================

#[tokio::test]
async fn px_048_revert_by_hunk_and_file_goes_through_the_change_transaction_with_a_checkpoint() {
    let base = notes();
    let mut fx = Fx::new(&[
        ("notes.txt", &base),
        ("gone.txt", "to be deleted\n"),
        (".env", "SECRET=1\n"),
    ])
    .await;
    // Two edits far apart (two hunks), a new file and a deleted one.
    let edited = base
        .replace("line 2\n", "line 2 changed\n")
        .replace("line 22\n", "line 22 changed\n");
    write(&fx.path("notes.txt"), &edited);
    write(&fx.path("fresh.txt"), "brand new\n");
    std::fs::remove_file(fx.path("gone.txt")).unwrap();
    write(&fx.path(".env"), "SECRET=2\n");

    let st = fx.state().await;
    let notes_state = st.paths.iter().find(|p| p.path == "notes.txt").unwrap();
    assert_eq!(notes_state.status, "M");
    assert_eq!(notes_state.file_revision, sha(&edited));
    assert!(
        st.paths
            .iter()
            .any(|p| p.path == "fresh.txt" && p.status == "A")
    );
    assert!(
        st.paths
            .iter()
            .any(|p| p.path == "gone.txt" && p.status == "D" && p.file_revision.is_empty())
    );
    assert!(
        st.paths
            .iter()
            .any(|p| p.path == ".env" && p.protected_path)
    );
    assert!(!st.task_running);
    let revision = st.workspace_revision;

    // One hunk of two: the first edit goes back, the second stays; the file
    // moves one workspace revision and a checkpoint was taken before it.
    let ack = fx
        .revert("notes.txt", false, 0, &sha(&edited))
        .await
        .unwrap();
    assert_eq!(ack.action, "REVERTED_HUNK");
    assert_eq!(
        text(&fx.path("notes.txt")),
        base.replace("line 22\n", "line 22 changed\n")
    );
    assert_eq!(ack.previous_revision, revision);
    assert_eq!(ack.workspace_revision, revision + 1);
    assert_eq!(ack.file_revision, sha(&text(&fx.path("notes.txt"))));
    assert!(!ack.checkpoint_id.is_empty());

    // The revert lands as a FileChanged with its own provenance and one
    // ChangesReverted record on the log.
    let evs = fx.events().await;
    assert!(evs.iter().any(|e| e["event_type"] == "FileChanged" && {
        let p = payload_of(e);
        p["provenance"] == "user_revert" && p["path"] == "notes.txt"
    }));
    assert!(evs.iter().any(|e| e["event_type"] == "ChangesReverted"));

    // A stale revert (the file revision the client held before the first
    // revert) is refused and writes nothing.
    let before = text(&fx.path("notes.txt"));
    let stale = fx.revert("notes.txt", false, 0, &sha(&edited)).await;
    assert_eq!(stale.unwrap_err().0, "STALE_REVISION");
    assert_eq!(text(&fx.path("notes.txt")), before);

    // A file edited by anyone since the client read it is refused the same way.
    let fresh_revision = sha(&text(&fx.path("notes.txt")));
    write(
        &fx.path("notes.txt"),
        &format!("{before}tail added by hand\n"),
    );
    let stale = fx.revert("notes.txt", true, 0, &fresh_revision).await;
    assert_eq!(stale.unwrap_err().0, "STALE_REVISION");
    assert!(text(&fx.path("notes.txt")).ends_with("tail added by hand\n"));

    // Whole files: the new file goes, the deleted one comes back, the edited
    // one returns to HEAD.
    let st = fx.state().await;
    let ack = fx
        .revert("fresh.txt", true, 0, &Fx::revision_of(&st, "fresh.txt"))
        .await
        .unwrap();
    assert_eq!(ack.action, "DELETED_FILE");
    assert!(!fx.path("fresh.txt").exists());
    let ack = fx.revert("gone.txt", true, 0, "").await.unwrap();
    assert_eq!(ack.action, "RESTORED_FILE");
    assert_eq!(text(&fx.path("gone.txt")), "to be deleted\n");
    let ack = fx
        .revert("notes.txt", true, 0, &Fx::revision_of(&st, "notes.txt"))
        .await
        .unwrap();
    assert_eq!(ack.action, "REVERTED_FILE");
    assert_eq!(text(&fx.path("notes.txt")), base);

    // A protected path is refused by the Core's path policy.
    let denied = fx
        .revert(".env", true, 0, &sha("SECRET=2\n"))
        .await
        .unwrap_err();
    assert_eq!(denied.0, "PROTECTED_PATH");
    assert_eq!(text(&fx.path(".env")), "SECRET=2\n");

    // A path with staged changes is a dirty conflict, not a silent fight with
    // the index.
    write(&fx.path("notes.txt"), &edited);
    let st = fx.state().await;
    let rev = Fx::revision_of(&st, "notes.txt");
    fx.stage(&[("notes.txt", &rev)], true).await.unwrap();
    let conflict = fx.revert("notes.txt", true, 0, &rev).await.unwrap_err();
    assert_eq!(conflict.0, "DIRTY_CONFLICT");
    assert_eq!(text(&fx.path("notes.txt")), edited);

    // The checkpoint taken before the first revert puts the tree back as it
    // was then: restoring it is the undo.
    let first_checkpoint = ack_checkpoint(&mut fx).await;
    let (restored, _): (CheckpointRestoreResult, bool) = {
        let task = fx.task.clone();
        let r: CheckpointRestoreResult = fx
            .cmd(
                "RestoreCheckpoint",
                RestoreCheckpoint {
                    task_id: Some(task),
                    checkpoint_id: first_checkpoint,
                    ..Default::default()
                },
                true,
            )
            .await
            .unwrap();
        (r, false)
    };
    assert!(restored.restored, "{restored:?}");
    assert_eq!(text(&fx.path("notes.txt")), edited, "both edits are back");
}

/// The checkpoint the first revert took: the oldest `before_revert` one.
async fn ack_checkpoint(fx: &mut Fx) -> String {
    let task = fx.task.clone();
    let list: modbit_protocol::v1::CheckpointList = fx
        .cmd(
            "ListCheckpoints",
            modbit_protocol::v1::ListCheckpoints {
                task_id: Some(task),
            },
            false,
        )
        .await
        .unwrap();
    let mut mine: Vec<_> = list
        .checkpoints
        .iter()
        .filter(|c| c.reason == "before_revert")
        .collect();
    mine.sort_by_key(|c| c.epoch);
    mine.first()
        .expect("a before_revert checkpoint")
        .checkpoint_id
        .clone()
}

#[tokio::test]
async fn px_048_a_revert_command_id_replays_without_writing_twice() {
    let base = notes();
    let mut fx = Fx::new(&[("notes.txt", &base)]).await;
    let edited = base.replace("line 2\n", "line 2 changed\n");
    write(&fx.path("notes.txt"), &edited);
    let task = fx.task.clone();
    let msg = RevertChanges {
        task_id: Some(task),
        path: "notes.txt".into(),
        whole_file: true,
        hunk_index: 0,
        expected_file_revision: sha(&edited),
    };
    let id = id16(0xEE);
    let first: RevertChangesAck = fx
        .cmd_with(id.clone(), "RevertChanges", msg.clone(), true)
        .await
        .unwrap();
    assert!(!first.replayed);
    // Someone edits again; the replayed command must not touch it.
    write(&fx.path("notes.txt"), "someone else wrote this\n");
    let again: RevertChangesAck = fx.cmd_with(id, "RevertChanges", msg, true).await.unwrap();
    assert!(again.replayed);
    assert_eq!(again.workspace_revision, first.workspace_revision);
    assert_eq!(text(&fx.path("notes.txt")), "someone else wrote this\n");
}

// ===========================================================================
// Stage
// ===========================================================================

#[tokio::test]
async fn px_048_stage_and_unstage_act_on_the_real_index() {
    let base = notes();
    let mut fx = Fx::new(&[
        ("notes.txt", &base),
        ("other.txt", "other\n"),
        (".env", "S=1\n"),
    ])
    .await;
    write(
        &fx.path("notes.txt"),
        &base.replace("line 2\n", "line 2 changed\n"),
    );
    write(&fx.path("fresh.txt"), "brand new\n");
    write(&fx.path(".env"), "S=2\n");
    let st = fx.state().await;
    assert!(st.paths.iter().all(|p| p.staged.is_empty()));
    let digest_before = st.staged_digest.clone();

    // A stale file revision is refused and the index is untouched.
    let stale = fx.stage(&[("notes.txt", &sha("not the file"))], true).await;
    assert_eq!(stale.unwrap_err().0, "STALE_REVISION");
    assert_eq!(
        git(Path::new(&fx.root), &["diff", "--cached", "--name-only"]),
        ""
    );

    // A protected path is refused.
    let protected = fx.stage(&[(".env", &sha("S=2\n"))], true).await;
    assert_eq!(protected.unwrap_err().0, "PROTECTED_PATH");

    // Staging a modified and an added file puts exactly those in the index.
    let n_rev = Fx::revision_of(&st, "notes.txt");
    let f_rev = Fx::revision_of(&st, "fresh.txt");
    let ack = fx
        .stage(&[("notes.txt", &n_rev), ("fresh.txt", &f_rev)], true)
        .await
        .unwrap();
    assert_ne!(ack.staged_digest, digest_before);
    let mut cached: Vec<String> = git(Path::new(&fx.root), &["diff", "--cached", "--name-only"])
        .lines()
        .map(str::to_owned)
        .collect();
    cached.sort();
    assert_eq!(cached, ["fresh.txt", "notes.txt"]);
    let st = fx.state().await;
    let notes_state = st.paths.iter().find(|p| p.path == "notes.txt").unwrap();
    assert_eq!(
        (notes_state.staged.as_str(), notes_state.unstaged.as_str()),
        ("M", "")
    );
    assert_eq!(st.staged_digest, ack.staged_digest);

    // Unstaging takes one back out and leaves the file alone.
    let ack = fx.stage(&[("fresh.txt", "")], false).await.unwrap();
    assert!(!ack.staged);
    assert_eq!(
        git(Path::new(&fx.root), &["diff", "--cached", "--name-only"]),
        "notes.txt"
    );
    assert_eq!(text(&fx.path("fresh.txt")), "brand new\n");

    // A path with no change is refused rather than ignored.
    let none = fx.stage(&[("other.txt", &sha("other\n"))], true).await;
    assert_eq!(none.unwrap_err().0, "NO_SUCH_CHANGE");
}

// ===========================================================================
// Commit and push
// ===========================================================================

#[tokio::test]
async fn px_048_commit_and_push_is_one_protected_effect_bound_to_its_exact_intent() {
    let base = notes();
    let mut fx = Fx::new(&[("notes.txt", &base), ("other.txt", "other\n")]).await;
    let remote = add_bare_remote(&fx);
    let root = PathBuf::from(&fx.root);
    let head0 = git(&root, &["rev-parse", "HEAD"]);
    git(&root, &["push", "-q", "origin", "main"]);

    // Typed refusals come before anything is asked: nothing staged.
    assert_eq!(fx.commit_push("m").await.unwrap_err().0, "NOTHING_STAGED");

    write(
        &fx.path("notes.txt"),
        &base.replace("line 2\n", "line 2 changed\n"),
    );
    write(&fx.path("fresh.txt"), "brand new\n");
    let st = fx.state().await;
    fx.stage(
        &[
            ("notes.txt", &Fx::revision_of(&st, "notes.txt")),
            ("fresh.txt", &Fx::revision_of(&st, "fresh.txt")),
        ],
        true,
    )
    .await
    .unwrap();
    // A message is required.
    assert_eq!(fx.commit_push("   ").await.unwrap_err().0, "BAD_PAYLOAD");

    // The first call asks: the exact intent comes back, nothing has run.
    let pending = fx.commit_push("Change the notes").await.unwrap();
    assert_eq!(pending.status, "APPROVAL_PENDING", "{pending:?}");
    assert_eq!(pending.message, "Change the notes");
    assert_eq!(pending.paths, ["fresh.txt", "notes.txt"]);
    assert_eq!(pending.branch, "main");
    assert_eq!(pending.remote, "origin");
    assert_eq!(pending.head_before, head0);
    assert!(!pending.push_only && !pending.approval_id.is_empty());
    assert_eq!(
        git(&root, &["rev-parse", "HEAD"]),
        head0,
        "no commit before the approval"
    );
    assert_eq!(
        git(remote.path(), &["rev-parse", "main"]),
        head0,
        "no push before the approval"
    );
    assert!(
        fx.state()
            .await
            .effects
            .iter()
            .any(|e| e.state == "APPROVAL_PENDING" && e.message == "Change the notes")
    );

    // Asking again without deciding runs nothing either.
    let again = fx.commit_push("Change the notes").await.unwrap();
    assert_eq!(again.status, "APPROVAL_PENDING");
    assert_eq!(git(&root, &["rev-parse", "HEAD"]), head0);

    // An approval cannot be decided for another intent than the one shown.
    let wrong = fx
        .resolve(&pending.approval_id, true, &"0".repeat(64))
        .await
        .unwrap_err();
    assert_eq!(wrong.0, "INTENT_MISMATCH");

    // The tool itself, invoked as the person without that approval, asks too:
    // there is no way around the approval.
    let id = fx.n();
    let direct = {
        let task = fx.task.clone();
        fx.cmd_with::<_, ToolInvoked>(
            id16(id),
            "InvokeTool",
            InvokeTool {
                task_id: Some(task),
                tool_name: "git.changes.commit_push".into(),
                arguments_json: serde_json::json!({
                    "message": "sneaky", "paths": ["notes.txt"], "branch": "main", "remote": "origin",
                    "head_before": head0, "staged_digest": "x", "push_only": false
                })
                .to_string(),
                tool_call_id: Some(id16(0xC1)),
                output_budget_bytes: 1 << 20,
            },
            true,
        )
        .await
        .unwrap()
    };
    assert_eq!(direct.status, "APPROVAL_PENDING", "{direct:?}");
    assert_eq!(git(&root, &["rev-parse", "HEAD"]), head0);

    // Approved for exactly what was shown, the same call runs it once.
    fx.resolve(&pending.approval_id, true, &pending.intent_hash)
        .await
        .unwrap();
    let done = fx.commit_push("Change the notes").await.unwrap();
    assert_eq!(done.status, "COMPLETED", "{done:?}");
    assert!(done.pushed && !done.commit.is_empty());
    assert_eq!(git(&root, &["rev-parse", "HEAD"]), done.commit);
    assert_eq!(
        git(remote.path(), &["rev-parse", "main"]),
        done.commit,
        "the bare remote holds the commit"
    );
    assert_eq!(
        git(&root, &["log", "-1", "--format=%s"]),
        "Change the notes"
    );
    assert_eq!(
        git(&root, &["show", "--name-only", "--format=", "HEAD"])
            .lines()
            .count(),
        2
    );
    // One protected effect, one AUTHORIZED receipt and one result receipt.
    let receipts = receipts_of(&fx.events().await, &pending.intent_hash);
    assert_eq!(
        receipts.iter().filter(|s| *s == "AUTHORIZED").count(),
        1,
        "{receipts:?}"
    );
    assert_eq!(receipts.len(), 2, "{receipts:?}");
    assert!(!done.effect_receipt_ids.is_empty());
    let st = fx.state().await;
    let eff = st
        .effects
        .iter()
        .find(|e| e.state == "COMPLETED")
        .expect("completed effect");
    assert!(!eff.authorized_receipt.is_empty() && !eff.result_receipt.is_empty());
    assert_eq!(eff.commit, done.commit);
    assert_eq!(st.unpushed, 0);

    // Nothing is left to commit: the same call now refuses instead of
    // committing a second time.
    assert_eq!(
        fx.commit_push("Change the notes").await.unwrap_err().0,
        "NOTHING_STAGED"
    );
    assert_eq!(git(&root, &["rev-parse", "HEAD"]), done.commit);

    // A denied approval leaves the repository and the remote as they were.
    write(&fx.path("other.txt"), "other, changed\n");
    let rev = sha("other, changed\n");
    fx.stage(&[("other.txt", &rev)], true).await.unwrap();
    let pending = fx.commit_push("Second change").await.unwrap();
    assert_eq!(pending.status, "APPROVAL_PENDING");
    fx.resolve(&pending.approval_id, false, &pending.intent_hash)
        .await
        .unwrap();
    let denied = fx.commit_push("Second change").await.unwrap();
    assert_ne!(denied.status, "COMPLETED", "{denied:?}");
    assert!(
        fx.state().await.effects.iter().any(|e| e.state == "DENIED"),
        "the denied attempt is listed as denied"
    );
    assert_eq!(git(&root, &["rev-parse", "HEAD"]), done.commit);
    assert_eq!(git(remote.path(), &["rev-parse", "main"]), done.commit);
}

#[tokio::test]
async fn px_048_a_changed_stage_after_the_approval_runs_nothing_and_a_rejected_push_keeps_the_commit()
 {
    let base = notes();
    let mut fx = Fx::new(&[("notes.txt", &base)]).await;
    let remote = add_bare_remote(&fx);
    let root = PathBuf::from(&fx.root);
    git(&root, &["push", "-q", "origin", "main"]);
    let head0 = git(&root, &["rev-parse", "HEAD"]);

    // The remote moves on by another clone: our push will not be a fast-forward.
    let other = tempfile::tempdir().unwrap();
    git(
        other.path(),
        &["clone", "-q", &remote.path().to_string_lossy(), "."],
    );
    write(&other.path().join("theirs.txt"), "theirs\n");
    git(other.path(), &["add", "-A"]);
    git(
        other.path(),
        &[
            "-c",
            "user.name=o",
            "-c",
            "user.email=o@e",
            "commit",
            "-q",
            "-m",
            "theirs",
        ],
    );
    git(other.path(), &["push", "-q", "origin", "main"]);
    let theirs = git(remote.path(), &["rev-parse", "main"]);

    write(
        &fx.path("notes.txt"),
        &base.replace("line 2\n", "line 2 changed\n"),
    );
    let st = fx.state().await;
    fx.stage(&[("notes.txt", &Fx::revision_of(&st, "notes.txt"))], true)
        .await
        .unwrap();

    // The approval was given for this staged content; the content then
    // changes: the call the approval covers refuses and commits nothing.
    let pending = fx.commit_push("Refused").await.unwrap();
    fx.resolve(&pending.approval_id, true, &pending.intent_hash)
        .await
        .unwrap();
    let args = serde_json::json!({
        "message": pending.message, "paths": pending.paths, "branch": pending.branch,
        "remote": pending.remote, "head_before": pending.head_before,
        "staged_digest": pending.staged_digest, "push_only": false
    });
    write(
        &fx.path("notes.txt"),
        &base.replace("line 2\n", "line 2 CHANGED AGAIN\n"),
    );
    let st = fx.state().await;
    fx.stage(&[("notes.txt", &Fx::revision_of(&st, "notes.txt"))], true)
        .await
        .unwrap();
    let id = fx.n();
    let task = fx.task.clone();
    let tool_call_id = {
        let st = fx.state().await;
        hex::decode(
            &st.effects
                .iter()
                .find(|e| e.intent_hash == pending.intent_hash)
                .unwrap()
                .tool_call_id,
        )
        .unwrap()
    };
    let r: ToolInvoked = fx
        .cmd_with(
            id16(id),
            "InvokeTool",
            InvokeTool {
                task_id: Some(task),
                tool_name: "git.changes.commit_push".into(),
                arguments_json: args.to_string(),
                tool_call_id: Some(Id {
                    value: tool_call_id,
                }),
                output_budget_bytes: 1 << 20,
            },
            true,
        )
        .await
        .unwrap();
    assert_ne!(r.status, "SUCCESS", "{r:?}");
    assert!(r.error_code.contains("STALE_INTENT"), "{r:?}");
    assert_eq!(
        git(&root, &["rev-parse", "HEAD"]),
        head0,
        "nothing was committed"
    );

    // The new intent (the staged content as it is now) is approved and pushed
    // into a diverged remote: the push is rejected, the commit stays local and
    // the state says one commit is unpushed.
    let done = fx.commit_push_approved("Rejected push").await;
    assert_eq!(done.status, "FAILED", "{done:?}");
    assert_eq!(done.code, "PUSH_REJECTED", "{done:?}");
    assert!(done.detail.contains("was made locally"), "{done:?}");
    assert_ne!(git(&root, &["rev-parse", "HEAD"]), head0);
    assert_eq!(
        git(remote.path(), &["rev-parse", "main"]),
        theirs,
        "the remote is untouched"
    );
    let st = fx.state().await;
    assert_eq!(st.unpushed, 1);
    assert!(
        st.effects
            .iter()
            .any(|e| e.state == "FAILED" && e.code == "PUSH_REJECTED")
    );
}

#[tokio::test]
async fn px_048_no_remote_and_a_detached_head_are_refused_before_anything_is_asked() {
    let base = notes();
    let mut fx = Fx::new(&[("notes.txt", &base)]).await;
    write(
        &fx.path("notes.txt"),
        &base.replace("line 2\n", "line 2 changed\n"),
    );
    let st = fx.state().await;
    fx.stage(&[("notes.txt", &Fx::revision_of(&st, "notes.txt"))], true)
        .await
        .unwrap();
    // No remote at all.
    assert_eq!(fx.commit_push("m").await.unwrap_err().0, "NO_REMOTE");
    let _remote = add_bare_remote(&fx);
    // A remote that is not configured.
    let task = fx.task.clone();
    let unknown: Result<CommitAndPushAck, Reject> = fx
        .cmd(
            "CommitAndPush",
            CommitAndPush {
                task_id: Some(task),
                message: "m".into(),
                remote: "nowhere".into(),
                ..Default::default()
            },
            true,
        )
        .await;
    assert_eq!(unknown.unwrap_err().0, "NO_REMOTE");
    // A detached HEAD has no branch to push.
    git(Path::new(&fx.root), &["checkout", "-q", "--detach"]);
    assert_eq!(fx.commit_push("m").await.unwrap_err().0, "NO_BRANCH");
    // A mutating command without the session lease is fenced.
    git(Path::new(&fx.root), &["checkout", "-q", "main"]);
    let task = fx.task.clone();
    let unfenced: Result<CommitAndPushAck, Reject> = fx
        .cmd(
            "CommitAndPush",
            CommitAndPush {
                task_id: Some(task),
                message: "m".into(),
                ..Default::default()
            },
            false,
        )
        .await;
    assert!(unfenced.is_err(), "an unfenced Commit and push is refused");
}

// ===========================================================================
// A Core killed mid-push
// ===========================================================================

#[tokio::test]
async fn px_048_a_core_killed_mid_push_leaves_the_effect_in_doubt_and_a_retry_converges() {
    let base = notes();
    let mut fx = Fx::new(&[("notes.txt", &base)]).await;
    let remote = add_bare_remote(&fx);
    let root = PathBuf::from(&fx.root);
    git(&root, &["push", "-q", "origin", "main"]);
    let pushed_head = git(&root, &["rev-parse", "HEAD"]);

    // The remote is a server that takes the connection and never answers, so
    // the push is in flight until the test says otherwise.
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let (hit_tx, hit_rx) = std::sync::mpsc::channel::<()>();
    let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut buf = [0u8; 256];
        let _ = stream.read(&mut buf);
        hit_tx.send(()).unwrap();
        let _ = release_rx.recv();
        drop(stream);
    });
    git(
        &root,
        &[
            "remote",
            "set-url",
            "origin",
            &format!("http://127.0.0.1:{port}/repo.git"),
        ],
    );

    write(
        &fx.path("notes.txt"),
        &base.replace("line 2\n", "line 2 changed\n"),
    );
    let st = fx.state().await;
    fx.stage(&[("notes.txt", &Fx::revision_of(&st, "notes.txt"))], true)
        .await
        .unwrap();
    let pending = fx.commit_push("Pushed into the void").await.unwrap();
    assert_eq!(pending.status, "APPROVAL_PENDING");
    fx.resolve(&pending.approval_id, true, &pending.intent_hash)
        .await
        .unwrap();

    // The approved call runs on its own connection; the test waits for the
    // server to see the push (a real signal), then kills the Core.
    let mut second = fx.core.client().await;
    let g = fx.g;
    let task = fx.task.clone();
    let runner = tokio::spawn(async move {
        let _ = second
            .command(envelope_fenced(
                id16(0xD1),
                "CommitAndPush",
                CommitAndPush {
                    task_id: Some(task),
                    message: "Pushed into the void".into(),
                    ..Default::default()
                }
                .encode_to_vec(),
                g,
            ))
            .await;
    });
    tokio::task::spawn_blocking(move || hit_rx.recv_timeout(Duration::from_secs(60)))
        .await
        .unwrap()
        .expect("the push reached the server");
    fx.restart().await;
    runner.abort();
    let _ = release_tx.send(());
    server.join().unwrap();

    // After the restart the effect is in doubt: authorized, no result. The
    // commit exists locally, and the remote never got it.
    let st = fx.state().await;
    let eff = st
        .effects
        .iter()
        .find(|e| e.intent_hash == pending.intent_hash)
        .expect("the effect survives the restart");
    assert_eq!(eff.state, "AUTHORIZED_IN_DOUBT", "{eff:?}");
    assert!(
        !eff.authorized_receipt.is_empty() && eff.result_receipt.is_empty(),
        "{eff:?}"
    );
    assert_eq!(eff.message, "Pushed into the void");
    assert_ne!(
        git(&root, &["rev-parse", "HEAD"]),
        pushed_head,
        "the commit was made before the push"
    );
    assert_eq!(git(remote.path(), &["rev-parse", "main"]), pushed_head);
    assert_eq!(st.unpushed, 1);
    let receipts = receipts_of(&fx.events().await, &pending.intent_hash);
    assert_eq!(receipts, ["AUTHORIZED"], "an authorization and no result");

    // The retry is a new call with its own approval; with nothing staged and a
    // commit the remote lacks, it is a push alone — and it converges.
    git(
        &root,
        &[
            "remote",
            "set-url",
            "origin",
            &remote.path().to_string_lossy(),
        ],
    );
    let retry = fx.commit_push("").await.unwrap();
    assert_eq!(retry.status, "APPROVAL_PENDING", "{retry:?}");
    assert!(retry.push_only);
    assert_eq!(retry.paths, ["notes.txt"]);
    assert_ne!(
        retry.approval_id, pending.approval_id,
        "a new call needs its own approval"
    );
    fx.resolve(&retry.approval_id, true, &retry.intent_hash)
        .await
        .unwrap();
    let done = fx.commit_push("").await.unwrap();
    assert_eq!(done.status, "COMPLETED", "{done:?}");
    assert!(done.push_only && done.pushed);
    assert_eq!(
        git(remote.path(), &["rev-parse", "main"]),
        git(&root, &["rev-parse", "HEAD"]),
        "the retry delivered the one commit, no second one was made"
    );
    assert_eq!(fx.state().await.unpushed, 0);
}

// ===========================================================================
// In a task's own worktree
// ===========================================================================

#[tokio::test]
async fn px_048_the_actions_work_in_a_task_worktree_and_the_checkout_never_changes() {
    let base = notes();
    let mut fx = Fx::with_isolation(&[("notes.txt", &base)], true).await;
    let remote = add_bare_remote(&fx);
    let checkout = PathBuf::from(&fx.root);
    let checkout_head = git(&checkout, &["rev-parse", "HEAD"]);
    let work = fx.work.clone();
    assert_ne!(work, checkout);

    let edited = base
        .replace("line 2\n", "line 2 changed\n")
        .replace("line 22\n", "line 22 changed\n");
    write(&fx.path("notes.txt"), &edited);
    let st = fx.state().await;
    assert_eq!(st.paths.len(), 1);
    assert!(
        st.branch.starts_with("modbit/") || !st.branch.is_empty(),
        "{}",
        st.branch
    );
    let branch = st.branch.clone();

    // A hunk revert lands in the worktree; the person's checkout is untouched.
    let ack = fx
        .revert("notes.txt", false, 1, &sha(&edited))
        .await
        .unwrap();
    assert_eq!(ack.action, "REVERTED_HUNK");
    assert_eq!(
        text(&fx.path("notes.txt")),
        base.replace("line 2\n", "line 2 changed\n")
    );
    assert_eq!(text(&checkout.join("notes.txt")), base);

    // Stage, then Commit and push from the worktree's own branch.
    let st = fx.state().await;
    fx.stage(&[("notes.txt", &Fx::revision_of(&st, "notes.txt"))], true)
        .await
        .unwrap();
    let done = fx.commit_push_approved("Worktree commit").await;
    assert_eq!(done.status, "COMPLETED", "{done:?}");
    assert_eq!(done.branch, branch);
    assert_eq!(git(&work, &["rev-parse", "HEAD"]), done.commit);
    assert_eq!(
        git(
            remote.path(),
            &["rev-parse", &format!("refs/heads/{branch}")]
        ),
        done.commit,
        "the worktree's branch is on the remote"
    );
    assert_eq!(git(&checkout, &["rev-parse", "HEAD"]), checkout_head);
    assert_eq!(
        text(&checkout.join("notes.txt")),
        base,
        "the checkout never changed"
    );
}
