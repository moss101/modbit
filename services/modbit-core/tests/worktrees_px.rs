//! PX-118, PX-065, PX-066, PX-119 (and the PX-067 remainder at the Core
//! boundary): worktree isolation, lifecycle, apply-back and agent merge
//! integration against the real Core process over its real socket, a real
//! SQLite event store, real Git repositories and the real process broker.
//!
//! The model is a scripted OpenAI-compatible server where a test needs a run
//! at all; most of the claims are made by driving the Core's own commands and
//! governed tools directly (`InvokeTool` as the person), because what is
//! under test is the Core's behaviour, not a model's.

mod px_common;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use modbit_protocol::client::{Client, ClientError};
use modbit_protocol::v1::{
    ApplyWorktree, ApprovalResolvedAck, CreateTask, DiscardWorktree, Id, InvokeTool, ListWorktrees,
    ResolveApproval, RunWorktreeCleanup, SessionSnapshot, TaskCreated, TaskIsolation, ToolInvoked,
    UndoApply, WorktreeApplyAck, WorktreeCleanupReport, WorktreeDiscarded, WorktreeList,
    WorktreePolicy,
};
use prost::Message;
use px_common::{CoreProcess, envelope_fenced, id16, plain_repo, session_with_lease};
use sha2::{Digest, Sha256};

type Reject = (String, String);

fn uuid_text(b: [u8; 16]) -> String {
    let h = hex::encode(b);
    format!(
        "{}-{}-{}-{}-{}",
        &h[0..8],
        &h[8..12],
        &h[12..16],
        &h[16..20],
        &h[20..32]
    )
}

/// Every file under `root` except `.git`, by path, with the sha256 of its bytes.
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

/// The scheduled cleanup stays out of a test's way unless the test sets it.
fn with_defaults<'a>(env: &[(&'a str, &'a str)]) -> Vec<(&'a str, &'a str)> {
    let mut v = env.to_vec();
    for (k, d) in [
        ("MODBIT_WORKTREE_CLEANUP_CATCHUP_MS", "3600000"),
        ("MODBIT_WORKTREE_CLEANUP_INTERVAL_MS", "3600000"),
    ] {
        if !v.iter().any(|(ek, _)| *ek == k) {
            v.push((k, d));
        }
    }
    v
}

/// A Core, a connection with a session lease, and a repository.
struct Fx {
    core: CoreProcess,
    c: Client,
    session: Id,
    g: Option<u64>,
    root: String,
    dir: tempfile::TempDir,
    _repo: tempfile::TempDir,
    next: u8,
}

impl Fx {
    async fn new(files: &[(&str, &str)], env: &[(&str, &str)]) -> Self {
        let (repo, root) = plain_repo(files);
        Self::with_repo(repo, root, env, tempfile::tempdir().unwrap()).await
    }

    async fn with_repo(
        repo: tempfile::TempDir,
        root: String,
        env: &[(&str, &str)],
        dir: tempfile::TempDir,
    ) -> Self {
        let core = CoreProcess::spawn_with_env(dir.path(), &with_defaults(env));
        let mut c = core.client().await;
        let (session, g) = session_with_lease(&mut c, 0x11).await;
        Self {
            core,
            c,
            session,
            g,
            root,
            dir,
            _repo: repo,
            next: 0x40,
        }
    }

    fn n(&mut self) -> u8 {
        self.next = self.next.wrapping_add(1);
        self.next
    }

    /// Restart the Core on the same data directory (a clean stop).
    async fn restart(&mut self, env: &[(&str, &str)]) {
        self.core.kill();
        self.core = CoreProcess::spawn_with_env(self.dir.path(), &with_defaults(env));
        self.c = self.core.client().await;
        let (session, g) = self.reacquire().await;
        self.session = session;
        self.g = g;
    }

    async fn reacquire(&mut self) -> (Id, Option<u64>) {
        let ack = self
            .c
            .command(px_common::envelope(
                id16(self.next.wrapping_add(0x70)),
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
        (self.session.clone(), Some(l.lease_generation))
    }

    async fn cmd<M: Message, R: Message + Default>(
        &mut self,
        kind: &str,
        msg: M,
        fenced: bool,
    ) -> Result<R, Reject> {
        let id = self.n();
        let g = if fenced { self.g } else { None };
        match self
            .c
            .command(envelope_fenced(id16(id), kind, msg.encode_to_vec(), g))
            .await
        {
            Ok(ack) => Ok(Client::result(&ack).unwrap()),
            Err(ClientError::Rejected { code, message }) => Err((code, message)),
            Err(e) => panic!("{kind}: {e}"),
        }
    }

    /// `CreateTask` with the command id `id`, in `root`, with an isolation.
    async fn create(
        &mut self,
        id: u8,
        root: &str,
        isolation: TaskIsolation,
        profile: &str,
    ) -> Result<Id, Reject> {
        let req = CreateTask {
            session_id: Some(self.session.clone()),
            goal_text: "a goal".into(),
            execution_profile: profile.into(),
            origin: "cli".into(),
            workspace_root: root.into(),
            isolation: isolation as i32,
            ..Default::default()
        };
        match self
            .c
            .command(envelope_fenced(
                id16(id),
                "CreateTask",
                req.encode_to_vec(),
                self.g,
            ))
            .await
        {
            Ok(ack) => Ok(Client::result::<TaskCreated>(&ack)
                .unwrap()
                .task_id
                .unwrap()),
            Err(ClientError::Rejected { code, message }) => Err((code, message)),
            Err(e) => panic!("CreateTask: {e}"),
        }
    }

    async fn isolated(&mut self, id: u8) -> Id {
        let root = self.root.clone();
        self.create(id, &root, TaskIsolation::Worktree, "local_trusted")
            .await
            .unwrap()
    }

    async fn invoke(
        &mut self,
        task: &Id,
        call: u8,
        tool: &str,
        args: serde_json::Value,
    ) -> ToolInvoked {
        let id = self.n();
        let ack = self
            .c
            .command(envelope_fenced(
                id16(id),
                "InvokeTool",
                InvokeTool {
                    task_id: Some(task.clone()),
                    tool_name: tool.into(),
                    arguments_json: args.to_string(),
                    tool_call_id: Some(id16(call)),
                    output_budget_bytes: 1 << 20,
                }
                .encode_to_vec(),
                self.g,
            ))
            .await
            .unwrap_or_else(|e| panic!("InvokeTool {tool}: {e}"));
        Client::result(&ack).unwrap()
    }

    /// Invoke a protected tool: ask, approve exactly what was asked, ask again.
    async fn invoke_approved(
        &mut self,
        task: &Id,
        call: u8,
        tool: &str,
        args: serde_json::Value,
    ) -> ToolInvoked {
        let first = self.invoke(task, call, tool, args.clone()).await;
        assert_eq!(first.status, "APPROVAL_PENDING", "{first:?}");
        let _: ApprovalResolvedAck = self
            .cmd(
                "ResolveApproval",
                ResolveApproval {
                    approval_id: Some(Id {
                        value: hex::decode(&first.approval_id).unwrap(),
                    }),
                    approve: true,
                    reason: "ok".into(),
                    intent_hash: String::new(),
                },
                true,
            )
            .await
            .unwrap();
        self.invoke(task, call, tool, args).await
    }

    async fn resolve(&mut self, approval_id: &str, approve: bool) {
        let _: ApprovalResolvedAck = self
            .cmd(
                "ResolveApproval",
                ResolveApproval {
                    approval_id: Some(Id {
                        value: hex::decode(approval_id.replace('-', "")).unwrap(),
                    }),
                    approve,
                    reason: "test".into(),
                    intent_hash: String::new(),
                },
                true,
            )
            .await
            .unwrap();
    }

    async fn list(&mut self) -> WorktreeList {
        self.cmd("ListWorktrees", ListWorktrees::default(), false)
            .await
            .unwrap()
    }

    async fn cleanup(
        &mut self,
        policy: Option<WorktreePolicy>,
        dry_run: bool,
    ) -> Result<WorktreeCleanupReport, Reject> {
        self.cmd(
            "RunWorktreeCleanup",
            RunWorktreeCleanup {
                reason: "test".into(),
                dry_run,
                policy,
            },
            false,
        )
        .await
    }

    async fn apply(
        &mut self,
        task: &Id,
        option: &str,
        confirm: &[&str],
        remember: bool,
    ) -> Result<WorktreeApplyAck, Reject> {
        self.cmd(
            "ApplyWorktree",
            ApplyWorktree {
                task_id: Some(task.clone()),
                option: option.into(),
                confirm_paths: confirm.iter().map(|s| (*s).to_owned()).collect(),
                remember,
                ..Default::default()
            },
            true,
        )
        .await
    }

    async fn undo(&mut self, task: &Id) -> Result<WorktreeApplyAck, Reject> {
        self.cmd(
            "UndoApply",
            UndoApply {
                task_id: Some(task.clone()),
                apply_id: String::new(),
            },
            true,
        )
        .await
    }

    /// Apply to completion: classify, approve the exact intent, apply.
    async fn apply_approved(
        &mut self,
        task: &Id,
        option: &str,
        confirm: &[&str],
    ) -> WorktreeApplyAck {
        let first = self.apply(task, option, confirm, false).await.unwrap();
        assert_eq!(first.status, "APPROVAL_PENDING", "{first:?}");
        self.resolve(&first.approval_id, true).await;
        let done = self.apply(task, option, confirm, false).await.unwrap();
        assert_eq!(done.status, "APPLIED", "{done:?}");
        done
    }

    /// The events of the session as JSON.
    async fn events(&self) -> Vec<serde_json::Value> {
        px_common::replay(&self.core, &self.session).await
    }

    async fn snapshot(&mut self) -> SessionSnapshot {
        self.cmd(
            "GetSessionSnapshot",
            modbit_protocol::v1::GetSessionSnapshot {
                session_id: Some(self.session.clone()),
            },
            false,
        )
        .await
        .unwrap()
    }

    fn data(&self) -> PathBuf {
        self.dir.path().to_path_buf()
    }

    fn worktree_dir(&self, task: &Id) -> PathBuf {
        let mut b = [0u8; 16];
        b.copy_from_slice(&task.value);
        self.data().join("worktrees").join(uuid_text(b))
    }
}

/// The events of one type, each reduced to its payload.
fn events_of(evs: &[serde_json::Value], ty: &str) -> Vec<serde_json::Value> {
    evs.iter()
        .filter(|e| e["event_type"] == ty)
        .map(|e| {
            let p = &e["payload"];
            if p["kind"] == "inline" {
                p["payload"].clone()
            } else {
                p.clone()
            }
        })
        .collect()
}

fn args_write(path: &str, content: &str) -> serde_json::Value {
    serde_json::json!({"path": path, "op": "replace", "content": content})
}

// ===========================================================================
// PX-118: a task in its own worktree
// ===========================================================================

#[tokio::test]
async fn px_118_an_isolated_task_works_in_its_worktree_and_the_checkout_never_changes() {
    let mut fx = Fx::new(
        &[("src/lib.rs", "pub fn a() {}\n"), ("README.md", "# demo\n")],
        &[],
    )
    .await;
    // The user's own work in the checkout: a tracked edit and an untracked file.
    write(
        &Path::new(&fx.root).join("README.md"),
        "# demo, edited by the user\n",
    );
    write(&Path::new(&fx.root).join("notes.txt"), "user notes\n");
    let before = tree_hashes(Path::new(&fx.root));
    let head_before = git(Path::new(&fx.root), &["rev-parse", "HEAD"]);

    let task = fx.isolated(0x21).await;
    let wt = fx.worktree_dir(&task);
    assert!(wt.join(".git").exists(), "the worktree exists at {wt:?}");
    // The task is created in it: its root is the worktree, not the checkout.
    let snap = fx.snapshot().await;
    let view = snap
        .tasks
        .iter()
        .find(|t| t.task_id.as_ref() == Some(&task))
        .unwrap();
    assert_eq!(
        Path::new(&view.workspace_root).canonicalize().unwrap(),
        wt.canonicalize().unwrap()
    );
    assert_eq!(view.isolation, TaskIsolation::Worktree as i32);
    assert_eq!(
        Path::new(&view.origin_root).canonicalize().unwrap(),
        Path::new(&fx.root).canonicalize().unwrap()
    );
    // The user's uncommitted and untracked files were carried by copy.
    assert_eq!(text(&wt.join("README.md")), "# demo, edited by the user\n");
    assert_eq!(text(&wt.join("notes.txt")), "user notes\n");

    // Tools: a file write, a shell command that creates a path, and one that
    // reads. All of it lands in the worktree.
    let r = fx
        .invoke(
            &task,
            0x31,
            "change.apply",
            args_write("src/lib.rs", "pub fn changed() {}\n"),
        )
        .await;
    assert_eq!(r.status, "SUCCESS", "{r:?}");
    let r = fx
        .invoke(
            &task,
            0x32,
            "shell.exec",
            serde_json::json!({"argv": ["git", "init", "-q", "made-by-shell"]}),
        )
        .await;
    assert_eq!(r.status, "SUCCESS", "{r:?}");
    assert!(wt.join("made-by-shell").is_dir());
    assert_eq!(text(&wt.join("src/lib.rs")), "pub fn changed() {}\n");

    // The checkout is exactly as the user left it.
    assert_eq!(tree_hashes(Path::new(&fx.root)), before);
    assert_eq!(
        git(Path::new(&fx.root), &["rev-parse", "HEAD"]),
        head_before
    );
    assert!(!Path::new(&fx.root).join("made-by-shell").exists());

    // A path escaping the worktree is refused: through `..` ...
    let r = fx
        .invoke(
            &task,
            0x33,
            "change.apply",
            args_write("../escape.txt", "x"),
        )
        .await;
    assert_eq!(r.status, "APPLICATION_FAILURE", "{r:?}");
    assert_eq!(r.error_code, "PATH_OUTSIDE_ROOT");
    // ... and through an absolute path into the checkout.
    let target = Path::new(&fx.root).join("src/lib.rs");
    let r = fx
        .invoke(
            &task,
            0x34,
            "change.apply",
            args_write(&target.to_string_lossy(), "pub fn pwned() {}\n"),
        )
        .await;
    assert_eq!(r.status, "APPLICATION_FAILURE", "{r:?}");
    assert_eq!(
        tree_hashes(Path::new(&fx.root)),
        before,
        "nothing reached the checkout"
    );
    // ... and through a symlink in the worktree that points at the checkout.
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&fx.root, wt.join("sneaky")).unwrap();
        let r = fx
            .invoke(
                &task,
                0x35,
                "change.apply",
                args_write("sneaky/src/lib.rs", "pub fn pwned() {}\n"),
            )
            .await;
        assert_eq!(r.status, "APPLICATION_FAILURE", "{r:?}");
        assert_eq!(tree_hashes(Path::new(&fx.root)), before);
    }

    // The worktree is registered on the log, with where it came from.
    let evs = fx.events().await;
    let prov = events_of(&evs, "WorktreeProvisioned");
    assert_eq!(prov.len(), 1, "{prov:#?}");
    assert_eq!(prov[0]["kind"], "TASK");
    assert_eq!(
        prov[0]["branch"],
        format!(
            "modbit/task-{}",
            uuid_text({
                let mut b = [0u8; 16];
                b.copy_from_slice(&task.value);
                b
            })
        )
    );
    let carried: Vec<String> = prov[0]["carried"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_owned())
        .collect();
    assert!(
        carried.contains(&"README.md".to_owned()) && carried.contains(&"notes.txt".to_owned()),
        "{carried:?}"
    );
}

#[tokio::test]
async fn px_118_an_unwritable_root_refuses_the_creation_typed_and_writes_nothing() {
    let mut fx = Fx::new(&[("a.txt", "a\n")], &[]).await;
    let before = tree_hashes(Path::new(&fx.root));
    // The worktree root is a file: nothing can be made under it.
    std::fs::write(fx.data().join("worktrees"), b"in the way").unwrap();
    let root = fx.root.clone();
    let err = fx
        .create(0x22, &root, TaskIsolation::Worktree, "local_trusted")
        .await
        .unwrap_err();
    assert_eq!(err.0, "ISOLATION_ROOT_UNWRITABLE", "{err:?}");
    assert!(
        err.1.contains("nothing was written to the checkout"),
        "{err:?}"
    );
    assert_eq!(tree_hashes(Path::new(&fx.root)), before);
    // No task exists, and no silent fallback made one in the checkout.
    assert!(fx.snapshot().await.tasks.is_empty());
    // A directory that is not a repository is typed too.
    std::fs::remove_file(fx.data().join("worktrees")).unwrap();
    let plain = tempfile::tempdir().unwrap();
    let err = fx
        .create(
            0x23,
            &plain.path().to_string_lossy(),
            TaskIsolation::Worktree,
            "local_trusted",
        )
        .await
        .unwrap_err();
    assert_eq!(err.0, "ISOLATION_NOT_A_REPOSITORY", "{err:?}");
    let err = fx
        .create(0x24, "", TaskIsolation::Worktree, "local_trusted")
        .await
        .unwrap_err();
    assert_eq!(err.0, "ISOLATION_NO_WORKSPACE", "{err:?}");
    // The same call without isolation still works where the repository does.
    let root = fx.root.clone();
    fx.create(0x25, &root, TaskIsolation::None, "local_trusted")
        .await
        .unwrap();
}

#[tokio::test]
async fn px_065_a_branch_checked_out_elsewhere_is_a_typed_refusal() {
    let mut fx = Fx::new(&[("a.txt", "a\n")], &[]).await;
    // Someone holds `modbit/task-<this task's id>` in another worktree.
    let id = uuid_text([0x26; 16]);
    let other = fx.data().join("someone-elses");
    git(
        Path::new(&fx.root),
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            &format!("modbit/task-{id}"),
            &other.to_string_lossy(),
        ],
    );
    let root = fx.root.clone();
    let err = fx
        .create(0x26, &root, TaskIsolation::Worktree, "local_trusted")
        .await
        .unwrap_err();
    assert_eq!(err.0, "ISOLATION_BRANCH_IN_USE", "{err:?}");
    assert!(err.1.contains("already checked out"), "{err:?}");
    assert!(fx.snapshot().await.tasks.is_empty());
    assert!(
        !fx.data().join("worktrees").join(&id).exists(),
        "nothing left behind"
    );
}

#[tokio::test]
async fn px_065_an_empty_repository_gets_an_orphan_worktree() {
    let repo = tempfile::tempdir().unwrap();
    git(repo.path(), &["init", "-q", "-b", "main"]);
    let root = repo
        .path()
        .canonicalize()
        .unwrap()
        .to_string_lossy()
        .trim_start_matches(r"\\?\")
        .to_owned();
    let mut fx = Fx::with_repo(repo, root.clone(), &[], tempfile::tempdir().unwrap()).await;
    write(&Path::new(&root).join("draft.txt"), "a first draft\n");
    let task = fx
        .create(0x27, &root, TaskIsolation::Worktree, "local_trusted")
        .await
        .unwrap();
    let wt = fx.worktree_dir(&task);
    assert_eq!(text(&wt.join("draft.txt")), "a first draft\n");
    let evs = fx.events().await;
    let prov = events_of(&evs, "WorktreeProvisioned");
    assert_eq!(prov[0]["base_revision"], "");
    assert_eq!(prov[0]["branch_policy"], "ORPHAN");
}

#[tokio::test]
async fn px_118_a_replayed_creation_makes_one_worktree() {
    let mut fx = Fx::new(&[("a.txt", "a\n")], &[]).await;
    let root = fx.root.clone();
    let t1 = fx
        .create(0x28, &root, TaskIsolation::Worktree, "local_trusted")
        .await
        .unwrap();
    let t2 = fx
        .create(0x28, &root, TaskIsolation::Worktree, "local_trusted")
        .await
        .unwrap();
    assert_eq!(t1, t2);
    let dirs: Vec<_> = std::fs::read_dir(fx.data().join("worktrees"))
        .unwrap()
        .flatten()
        .collect();
    assert_eq!(dirs.len(), 1);
    let evs = fx.events().await;
    assert_eq!(events_of(&evs, "WorktreeProvisioned").len(), 1);
}

#[tokio::test]
async fn px_118_killing_the_core_and_restarting_resumes_in_the_same_worktree() {
    let mut fx = Fx::new(&[("a.txt", "a\n")], &[]).await;
    let task = fx.isolated(0x29).await;
    let wt = fx.worktree_dir(&task);
    let r = fx
        .invoke(
            &task,
            0x36,
            "change.apply",
            args_write("made.txt", "before the kill\n"),
        )
        .await;
    assert_eq!(r.status, "SUCCESS");
    fx.restart(&[]).await;
    let snap = fx.snapshot().await;
    let view = snap
        .tasks
        .iter()
        .find(|t| t.task_id.as_ref() == Some(&task))
        .unwrap();
    assert_eq!(
        Path::new(&view.workspace_root).canonicalize().unwrap(),
        wt.canonicalize().unwrap()
    );
    assert_eq!(view.isolation, TaskIsolation::Worktree as i32);
    assert_eq!(text(&wt.join("made.txt")), "before the kill\n");
    let r = fx
        .invoke(
            &task,
            0x37,
            "change.apply",
            args_write("after.txt", "after\n"),
        )
        .await;
    assert_eq!(r.status, "SUCCESS", "{r:?}");
    assert!(wt.join("after.txt").exists());
    assert!(!Path::new(&fx.root).join("after.txt").exists());
}

#[tokio::test]
async fn px_065_the_listing_matches_the_filesystem() {
    let mut fx = Fx::new(&[("a.txt", "a\n")], &[]).await;
    let task = fx.isolated(0x2a).await;
    let l = fx.list().await;
    assert_eq!(l.count, 1);
    let w = &l.worktrees[0];
    assert_eq!(w.kind, "TASK");
    assert_eq!(w.state, "ACTIVE");
    assert_eq!(w.task_id.as_ref(), Some(&task));
    assert!(
        w.protected,
        "a new worktree is inside the protection window: {w:?}"
    );
    assert!(!w.removable);
    assert!(Path::new(&w.path).join(".git").exists());
    assert_eq!(l.policy.as_ref().unwrap().max_worktrees, 25);
    assert_eq!(l.policy.as_ref().unwrap().protect_ms, 10 * 60 * 1000);
}

// ===========================================================================
// PX-066: apply-back
// ===========================================================================

const TEN: &str = "one\ntwo\nthree\nfour\nfive\nsix\nseven\neight\nnine\nten\n";

/// An isolated task whose worktree holds: a.txt edited on its first line,
/// new/dir/b.txt added, c.txt deleted.
async fn task_with_changes(fx: &mut Fx, id: u8) -> Id {
    let task = fx.isolated(id).await;
    let r = fx
        .invoke(
            &task,
            id.wrapping_add(0x80),
            "change.apply",
            args_write(
                "a.txt",
                "ONE\ntwo\nthree\nfour\nfive\nsix\nseven\neight\nnine\nten\n",
            ),
        )
        .await;
    assert_eq!(r.status, "SUCCESS", "{r:?}");
    let r = fx
        .invoke(
            &task,
            id.wrapping_add(0x81),
            "change.apply",
            args_write("new/dir/b.txt", "added by the task\n"),
        )
        .await;
    assert_eq!(r.status, "SUCCESS", "{r:?}");
    let r = fx
        .invoke(
            &task,
            id.wrapping_add(0x82),
            "change.apply",
            serde_json::json!({"path": "c.txt", "op": "delete"}),
        )
        .await;
    assert_eq!(r.status, "SUCCESS", "{r:?}");
    task
}

fn apply_files() -> Vec<(&'static str, &'static str)> {
    vec![
        ("a.txt", TEN),
        ("c.txt", "to be deleted\n"),
        ("keep.txt", "untouched\n"),
    ]
}

async fn receipts_for(fx: &Fx, intent: &str) -> Vec<String> {
    events_of(&fx.events().await, "EffectReceiptAppended")
        .into_iter()
        .filter(|e| e["receipt"]["intent_hash"] == intent)
        .map(|e| e["receipt"]["status"].as_str().unwrap().to_owned())
        .collect()
}

async fn approvals(fx: &mut Fx) -> modbit_protocol::v1::ApprovalList {
    let session = fx.session.clone();
    fx.cmd(
        "ListApprovals",
        modbit_protocol::v1::ListApprovals {
            session_id: Some(session),
        },
        false,
    )
    .await
    .unwrap()
}

#[tokio::test]
async fn px_066_apply_is_a_protected_effect_bound_to_its_intent_and_undo_is_exact() {
    let mut fx = Fx::new(&apply_files(), &[]).await;
    write(&Path::new(&fx.root).join("keep.txt"), "user edit in keep\n");
    let task = task_with_changes(&mut fx, 0x51).await;
    let wt = fx.worktree_dir(&task);
    let before = tree_hashes(Path::new(&fx.root));

    // No approval, no apply: the call is held on an approval for exactly this
    // intent, and the checkout does not change.
    let pending = fx.apply(&task, "", &[], false).await.unwrap();
    assert_eq!(pending.status, "APPROVAL_PENDING", "{pending:?}");
    assert!(!pending.intent_hash.is_empty() && !pending.plan_digest.is_empty());
    assert_eq!(tree_hashes(Path::new(&fx.root)), before);
    let listed = approvals(&mut fx).await;
    let mine = listed
        .approvals
        .iter()
        .find(|a| {
            hex::encode(&a.approval_id.as_ref().unwrap().value)
                == pending.approval_id.replace('-', "")
        })
        .expect("the approval is listed");
    assert_eq!(mine.tool_name, "git.apply.worktree");
    assert_eq!(mine.intent_hash, pending.intent_hash);
    assert_eq!(mine.status, "REQUESTED");

    // Denied: nothing changes, and the denial is final for that approval; a
    // new request is a new question to the person.
    fx.resolve(&pending.approval_id, false).await;
    assert_eq!(tree_hashes(Path::new(&fx.root)), before);
    let again = fx.apply(&task, "", &[], false).await.unwrap();
    assert_eq!(again.status, "APPROVAL_PENDING", "{again:?}");
    assert_ne!(again.approval_id, pending.approval_id, "a new approval");
    assert_eq!(again.intent_hash, pending.intent_hash, "the same intent");
    fx.resolve(&again.approval_id, true).await;
    let done = fx.apply(&task, "", &[], false).await.unwrap();
    assert_eq!(done.status, "APPLIED", "{done:?}");
    assert_eq!(done.candidate_tree, pending.candidate_tree);
    assert_eq!(done.plan_digest, pending.plan_digest);
    assert!(!done.effect_receipt_ids.is_empty());

    // The checkout holds the candidate on every path the task changed; the
    // user's own edit is exactly as it was.
    let after = tree_hashes(Path::new(&fx.root));
    let cand = tree_hashes(&wt);
    assert_eq!(after["a.txt"], cand["a.txt"]);
    assert_eq!(after["new/dir/b.txt"], cand["new/dir/b.txt"]);
    assert!(!after.contains_key("c.txt"));
    assert_eq!(after["keep.txt"], before["keep.txt"]);
    // One authorization receipt and its result, both naming the intent.
    assert_eq!(
        receipts_for(&fx, &pending.intent_hash).await,
        vec!["AUTHORIZED".to_owned(), "SUCCESS".to_owned()]
    );
    let evs = fx.events().await;
    let applied = events_of(&evs, "WorktreeApplied");
    assert_eq!(applied.len(), 1);
    assert_eq!(applied[0]["candidate_tree"], done.candidate_tree);
    assert_eq!(
        applied[0]["checkout_head_before"],
        done.checkout_head_before
    );

    // Applying again changes nothing: the checkout already holds the result.
    let replay = fx.apply(&task, "", &[], false).await.unwrap();
    assert_eq!(replay.status, "NOTHING_TO_APPLY", "{replay:?}");
    assert_eq!(tree_hashes(Path::new(&fx.root)), after);
    assert_eq!(events_of(&fx.events().await, "WorktreeApplied").len(), 1);

    // Undo: approval, then every byte back, the created directory gone.
    let u = fx.undo(&task).await.unwrap();
    assert_eq!(u.status, "APPROVAL_PENDING", "{u:?}");
    assert_eq!(
        tree_hashes(Path::new(&fx.root)),
        after,
        "an undo without approval changes nothing"
    );
    fx.resolve(&u.approval_id, true).await;
    let u = fx.undo(&task).await.unwrap();
    assert_eq!(u.status, "UNDONE", "{u:?}");
    assert_eq!(tree_hashes(Path::new(&fx.root)), before);
    assert!(!Path::new(&fx.root).join("new").exists());
    // After an undo the result is unapplied again, so it is not removable.
    let l = fx.list().await;
    let w = l
        .worktrees
        .iter()
        .find(|w| w.task_id.as_ref() == Some(&task))
        .unwrap();
    assert_eq!(w.disposition, "");
    assert!(w.unapplied, "{w:?}");
}

#[tokio::test]
async fn px_066_a_stale_review_or_plan_is_refused_and_a_forged_plan_cannot_ride_an_approval() {
    let mut fx = Fx::new(&apply_files(), &[]).await;
    let task = task_with_changes(&mut fx, 0x52).await;
    let before = tree_hashes(Path::new(&fx.root));
    let first = fx.apply(&task, "", &[], false).await.unwrap();
    assert_eq!(first.status, "APPROVAL_PENDING");
    // The review was of an older candidate revision.
    let stale: WorktreeApplyAck = fx
        .cmd(
            "ApplyWorktree",
            ApplyWorktree {
                task_id: Some(task.clone()),
                expected_candidate_revision: first.candidate_revision + 7,
                ..Default::default()
            },
            true,
        )
        .await
        .unwrap();
    assert_eq!(stale.status, "STALE");
    assert_eq!(stale.code, "STALE_REVISION");
    // The task changes more after the person looked: the plan digest differs.
    let r = fx
        .invoke(
            &task,
            0x90,
            "change.apply",
            args_write("more.txt", "more\n"),
        )
        .await;
    assert_eq!(r.status, "SUCCESS");
    let stale: WorktreeApplyAck = fx
        .cmd(
            "ApplyWorktree",
            ApplyWorktree {
                task_id: Some(task.clone()),
                expected_plan_digest: first.plan_digest.clone(),
                ..Default::default()
            },
            true,
        )
        .await
        .unwrap();
    assert_eq!(stale.status, "STALE");
    assert_eq!(stale.code, "STALE_PLAN");
    assert_ne!(stale.plan_digest, first.plan_digest);
    // The governed tool itself refuses a plan other than the live one, even
    // under a real approval: an approved intent cannot be reused for a plan
    // the person did not see.
    let l = fx.list().await;
    let wid = l.worktrees[0].worktree_id.clone();
    let forged = serde_json::json!({
        "worktree_id": wid,
        "plan_digest": "00".repeat(32),
        "apply_id": "ap-forged-0001",
    });
    let r = fx
        .invoke_approved(&task, 0x91, "git.apply.worktree", forged)
        .await;
    assert_eq!(r.status, "APPLICATION_FAILURE", "{r:?}");
    assert_eq!(r.error_code, "STALE_PLAN");
    assert_eq!(
        tree_hashes(Path::new(&fx.root)),
        before,
        "nothing was written"
    );
}

#[tokio::test]
async fn px_066_conflicts_are_typed_never_overwritten_and_each_option_is_exact() {
    let mut fx = Fx::new(&apply_files(), &[]).await;
    let task = task_with_changes(&mut fx, 0x53).await;
    let wt = fx.worktree_dir(&task);
    // The user edits the same line in the checkout after the task started.
    let user_a = "uno\ntwo\nthree\nfour\nfive\nsix\nseven\neight\nnine\nten\n";
    write(&Path::new(&fx.root).join("a.txt"), user_a);
    write(&Path::new(&fx.root).join("keep.txt"), "user edit in keep\n");
    write(&Path::new(&fx.root).join("scratch.txt"), "untracked\n");
    let before = tree_hashes(Path::new(&fx.root));

    // The first call yields the typed conflict result and asks for nothing.
    let c = fx.apply(&task, "", &[], false).await.unwrap();
    assert_eq!(c.status, "CONFLICT", "{c:?}");
    let view = c.conflict.clone().unwrap();
    assert_eq!(view.conflicting_paths, vec!["a.txt".to_owned()]);
    assert_eq!(view.default_option, "CANCEL");
    let opt = |name: &str| {
        view.options
            .iter()
            .find(|o| o.option == name)
            .unwrap()
            .clone()
    };
    assert!(!opt("CANCEL").destructive && opt("CANCEL").available);
    assert!(opt("OVERWRITE").destructive && opt("OVERWRITE").needs_confirmation);
    assert_eq!(opt("OVERWRITE").confirm_paths, vec!["a.txt".to_owned()]);
    assert_eq!(
        opt("FULL_OVERWRITE").confirm_paths,
        vec![
            "a.txt".to_owned(),
            "keep.txt".to_owned(),
            "scratch.txt".to_owned()
        ]
    );
    assert!(opt("MERGE_MANUALLY").available && opt("STASH").available);
    assert!(!opt("UNDO_AND_APPLY").available);
    assert_eq!(
        tree_hashes(Path::new(&fx.root)),
        before,
        "classifying changed nothing"
    );
    // And no approval was asked of anyone.
    assert!(approvals(&mut fx).await.approvals.is_empty());

    // Cancel is a no-op.
    let r = fx.apply(&task, "CANCEL", &[], false).await.unwrap();
    assert_eq!(r.status, "CANCELLED");
    // An overwrite without the files typed is refused by the Core before any
    // approval is asked, whatever the client sent.
    for (opt_name, confirm) in [
        ("OVERWRITE", vec![]),
        ("OVERWRITE", vec!["keep.txt"]),
        ("FULL_OVERWRITE", vec!["a.txt"]),
    ] {
        let r = fx.apply(&task, opt_name, &confirm, false).await.unwrap();
        assert_eq!(r.status, "REFUSED", "{opt_name} {confirm:?}: {r:?}");
        assert_eq!(r.code, "OVERWRITE_NOT_CONFIRMED");
    }
    assert_eq!(tree_hashes(Path::new(&fx.root)), before);
    assert!(approvals(&mut fx).await.approvals.is_empty());

    // MERGE_MANUALLY: markers in the conflicting file; the rest applied.
    let done = fx.apply_approved(&task, "MERGE_MANUALLY", &[]).await;
    assert_eq!(done.unresolved_paths, vec!["a.txt".to_owned()]);
    let merged = text(&Path::new(&fx.root).join("a.txt"));
    assert!(
        merged.contains("<<<<<<< checkout") && merged.contains("uno") && merged.contains("ONE"),
        "{merged}"
    );
    assert_eq!(
        text(&Path::new(&fx.root).join("new/dir/b.txt")),
        "added by the task\n"
    );
    undo_to(&mut fx, &task, &before).await;

    // OVERWRITE with the file typed: the conflicting file only.
    let done = fx.apply_approved(&task, "OVERWRITE", &["a.txt"]).await;
    assert!(done.unresolved_paths.is_empty());
    let after = tree_hashes(Path::new(&fx.root));
    assert_eq!(after["a.txt"], tree_hashes(&wt)["a.txt"]);
    assert_eq!(after["keep.txt"], before["keep.txt"]);
    assert_eq!(after["scratch.txt"], before["scratch.txt"]);
    // A destructive choice rides an approval of the destructive class.
    let listed = approvals(&mut fx).await;
    assert!(
        listed
            .approvals
            .iter()
            .any(|a| a.tool_name == "git.apply.worktree" && a.effect_class == "Destructive"),
        "{listed:?}"
    );
    undo_to(&mut fx, &task, &before).await;

    // STASH: the user's dirty state is kept in a snapshot, the task's change
    // applies on the committed checkout.
    let done = fx.apply_approved(&task, "STASH", &[]).await;
    assert!(!done.stash_ref.is_empty(), "{done:?}");
    assert_eq!(
        git(
            Path::new(&fx.root),
            &["show", &format!("{}:keep.txt", done.stash_ref)]
        ),
        "user edit in keep"
    );
    assert_eq!(text(&Path::new(&fx.root).join("keep.txt")), "untouched\n");
    assert!(!Path::new(&fx.root).join("scratch.txt").exists());
    undo_to(&mut fx, &task, &before).await;

    // FULL_OVERWRITE with every file typed.
    let done = fx
        .apply_approved(
            &task,
            "FULL_OVERWRITE",
            &["a.txt", "keep.txt", "scratch.txt"],
        )
        .await;
    assert!(done.unresolved_paths.is_empty());
    assert!(!Path::new(&fx.root).join("scratch.txt").exists());
    undo_to(&mut fx, &task, &before).await;
}

/// Undo the apply in force (approval and all) and assert the checkout is
/// exactly `expected`.
async fn undo_to(fx: &mut Fx, task: &Id, expected: &BTreeMap<String, String>) {
    let u = fx.undo(task).await.unwrap();
    assert_eq!(u.status, "APPROVAL_PENDING", "{u:?}");
    fx.resolve(&u.approval_id, true).await;
    let u = fx.undo(task).await.unwrap();
    assert_eq!(u.status, "UNDONE", "{u:?}");
    assert_eq!(
        &tree_hashes(Path::new(&fx.root)),
        expected,
        "undo restored the user's bytes exactly"
    );
}

#[tokio::test]
async fn px_066_undo_after_a_second_user_edit_reports_the_conflict_and_destroys_nothing() {
    let mut fx = Fx::new(&apply_files(), &[]).await;
    let task = task_with_changes(&mut fx, 0x54).await;
    fx.apply_approved(&task, "", &[]).await;
    // The user keeps working on a file the apply wrote.
    write(
        &Path::new(&fx.root).join("a.txt"),
        "ONE and then my own second edit\n",
    );
    let held = tree_hashes(Path::new(&fx.root));
    let u = fx.undo(&task).await.unwrap();
    assert_eq!(u.status, "APPROVAL_PENDING");
    fx.resolve(&u.approval_id, true).await;
    let u = fx.undo(&task).await.unwrap();
    assert_eq!(u.status, "UNDO_CONFLICT", "{u:?}");
    assert_eq!(u.divergent_paths, vec!["a.txt".to_owned()]);
    assert_eq!(
        tree_hashes(Path::new(&fx.root)),
        held,
        "nothing was changed"
    );
}

#[tokio::test]
async fn px_066_a_remembered_choice_is_applied_only_when_it_is_non_destructive() {
    let mut fx = Fx::new(&apply_files(), &[]).await;
    let task = task_with_changes(&mut fx, 0x55).await;
    write(
        &Path::new(&fx.root).join("a.txt"),
        "uno\ntwo\nthree\nfour\nfive\nsix\nseven\neight\nnine\nten\n",
    );
    // A destructive choice asked to be remembered is not.
    let r = fx
        .apply(&task, "OVERWRITE", &["a.txt"], true)
        .await
        .unwrap();
    assert_eq!(r.status, "APPROVAL_PENDING");
    assert!(r.detail.contains("never remembered"), "{r:?}");
    fx.resolve(&r.approval_id, false).await;
    let c = fx.apply(&task, "", &[], false).await.unwrap();
    assert_eq!(
        c.status, "CONFLICT",
        "a destructive choice is not honoured later: {c:?}"
    );
    assert_eq!(c.conflict.unwrap().remembered_option, "");
    // A non-destructive one is.
    let r = fx.apply(&task, "MERGE_MANUALLY", &[], true).await.unwrap();
    assert_eq!(r.status, "APPROVAL_PENDING");
    fx.resolve(&r.approval_id, true).await;
    let r = fx.apply(&task, "MERGE_MANUALLY", &[], true).await.unwrap();
    assert_eq!(r.status, "APPLIED", "{r:?}");
    let u = fx.undo(&task).await.unwrap();
    fx.resolve(&u.approval_id, true).await;
    assert_eq!(fx.undo(&task).await.unwrap().status, "UNDONE");
    // The next apply uses it without being told.
    let r = fx.apply(&task, "", &[], false).await.unwrap();
    assert_eq!(
        r.status, "APPROVAL_PENDING",
        "the remembered choice carried it through: {r:?}"
    );
    assert_eq!(r.option, "MERGE_MANUALLY");
}

#[tokio::test]
async fn px_066_protected_paths_need_their_names_typed() {
    let mut fx = Fx::new(&apply_files(), &[]).await;
    let task = fx.isolated(0x56).await;
    // A workflow file in the worktree (a protected pattern), written directly.
    let wt = fx.worktree_dir(&task);
    write(&wt.join(".github/workflows/ci.yml"), "name: ci\n");
    let c = fx.apply(&task, "", &[], false).await.unwrap();
    assert_eq!(c.status, "CONFLICT", "{c:?}");
    assert_eq!(c.code, "PROTECTED_PATHS");
    assert_eq!(
        c.conflict.unwrap().protected_paths,
        vec![".github/workflows/ci.yml".to_owned()]
    );
    let r = fx.apply(&task, "MERGE_MANUALLY", &[], false).await.unwrap();
    assert_eq!(r.status, "REFUSED");
    assert_eq!(r.code, "PROTECTED_PATHS");
    assert!(!Path::new(&fx.root).join(".github").exists());
    let done = fx
        .apply_approved(&task, "MERGE_MANUALLY", &[".github/workflows/ci.yml"])
        .await;
    assert_eq!(
        done.applied_paths,
        vec![".github/workflows/ci.yml".to_owned()]
    );
    assert!(
        Path::new(&fx.root)
            .join(".github/workflows/ci.yml")
            .exists()
    );
}

#[tokio::test]
async fn px_066_discard_is_a_typed_decision_that_touches_no_checkout() {
    let mut fx = Fx::new(&apply_files(), &[]).await;
    let task = task_with_changes(&mut fx, 0x57).await;
    let before = tree_hashes(Path::new(&fx.root));
    // Needs the branch name typed.
    let err = fx
        .cmd::<_, WorktreeDiscarded>(
            "DiscardWorktree",
            DiscardWorktree {
                task_id: Some(task.clone()),
                reason: "no".into(),
                confirm: "nope".into(),
            },
            true,
        )
        .await
        .unwrap_err();
    assert_eq!(err.0, "DISCARD_NOT_CONFIRMED");
    let l = fx.list().await;
    let branch = l.worktrees[0].branch.clone();
    let d: WorktreeDiscarded = fx
        .cmd(
            "DiscardWorktree",
            DiscardWorktree {
                task_id: Some(task.clone()),
                reason: "not wanted".into(),
                confirm: branch,
            },
            true,
        )
        .await
        .unwrap();
    assert_eq!(d.disposition, "DISCARDED");
    assert_eq!(tree_hashes(Path::new(&fx.root)), before);
    let l = fx.list().await;
    assert_eq!(l.worktrees[0].disposition, "DISCARDED");
}

// ===========================================================================
// PX-066: a Core killed mid-apply
// ===========================================================================

/// Run an apply whose Core is killed at `point`; restart the Core; the
/// checkout is the exact pre-apply state, the log says the apply was rolled
/// back, and the same apply then completes.
async fn kill_mid_apply(point: &str, expect_partial: bool) {
    let mut fx = Fx::new(&apply_files(), &[("MODBIT_FAULT_WORKTREE", point)]).await;
    write(&Path::new(&fx.root).join("keep.txt"), "user edit in keep\n");
    let task = task_with_changes(&mut fx, 0x58).await;
    let before = tree_hashes(Path::new(&fx.root));
    let pending = fx.apply(&task, "", &[], false).await.unwrap();
    assert_eq!(pending.status, "APPROVAL_PENDING");
    fx.resolve(&pending.approval_id, true).await;
    // The Core dies inside this call.
    let id = fx.n();
    let died =
        fx.c.command(envelope_fenced(
            id16(id),
            "ApplyWorktree",
            ApplyWorktree {
                task_id: Some(task.clone()),
                ..Default::default()
            }
            .encode_to_vec(),
            fx.g,
        ))
        .await;
    assert!(died.is_err(), "the Core was killed at {point}: {died:?}");
    let mid = tree_hashes(Path::new(&fx.root));
    if expect_partial {
        assert_ne!(
            mid, before,
            "at {point} one file had been written: a half-applied tree"
        );
        assert_ne!(mid.get("a.txt"), before.get("a.txt"));
    } else if point == "WORKTREE_APPLY_AFTER_STARTED" {
        assert_eq!(mid, before, "nothing had been written yet");
    }
    // Restart without the fault: recovery puts the checkpoint back.
    fx.restart(&[]).await;
    assert_eq!(
        tree_hashes(Path::new(&fx.root)),
        before,
        "after recovery the checkout is exactly the pre-apply state ({point})"
    );
    let evs = fx.events().await;
    assert_eq!(events_of(&evs, "WorktreeApplyStarted").len(), 1);
    assert_eq!(events_of(&evs, "WorktreeApplyRolledBack").len(), 1);
    assert!(events_of(&evs, "WorktreeApplied").is_empty());
    // No half-written temp file is left beside any path.
    let strays: Vec<String> = tree_hashes(Path::new(&fx.root))
        .into_keys()
        .filter(|p| p.contains(".modbit-apply-"))
        .collect();
    assert!(strays.is_empty(), "{strays:?}");
    // And the apply can be done again, whole.
    let first = fx.apply(&task, "", &[], false).await.unwrap();
    assert_eq!(first.status, "APPROVAL_PENDING", "{first:?}");
    fx.resolve(&first.approval_id, true).await;
    let done = fx.apply(&task, "", &[], false).await.unwrap();
    assert_eq!(done.status, "APPLIED", "{done:?}");
    let after = tree_hashes(Path::new(&fx.root));
    assert_eq!(
        after["a.txt"],
        tree_hashes(&fx.worktree_dir(&task))["a.txt"]
    );
    assert!(!after.contains_key("c.txt"));
}

#[tokio::test]
async fn px_066_a_core_killed_before_the_first_write_recovers_whole() {
    kill_mid_apply("WORKTREE_APPLY_AFTER_STARTED", false).await;
}

#[tokio::test]
async fn px_066_a_core_killed_in_the_middle_of_the_writes_recovers_whole() {
    kill_mid_apply("WORKTREE_APPLY_MID", true).await;
}

#[tokio::test]
async fn px_066_a_core_killed_after_the_writes_before_the_record_recovers_whole() {
    kill_mid_apply("WORKTREE_APPLY_AFTER_WRITES", true).await;
}

// ===========================================================================
// PX-065: setup commands
// ===========================================================================

use px_common::{model_env, start_task, wait_task};

fn setup_repo(commands: serde_json::Value) -> (tempfile::TempDir, String) {
    plain_repo(&[
        ("README.md", "# setup\n"),
        (
            ".modbit/worktree-setup.json",
            &serde_json::json!({"commands": commands}).to_string(),
        ),
    ])
}

async fn trust(fx: &mut Fx) {
    let root = fx.root.clone();
    let session = fx.session.clone();
    let _: modbit_protocol::v1::RepositoryTrusted = fx
        .cmd(
            "TrustRepository",
            modbit_protocol::v1::TrustRepository {
                session_id: Some(session),
                workspace_root: root,
                scope: String::new(),
            },
            true,
        )
        .await
        .unwrap();
}

async fn run_to_end(fx: &mut Fx, task: &Id, id: u8) {
    start_task(&mut fx.c, task, fx.g, id, "gpt-5-mini").await;
    let st = wait_task(&mut fx.c, task, 90).await;
    assert!(!st.loop_alive, "{st:?}");
}

#[tokio::test]
async fn px_065_setup_commands_run_only_in_a_trusted_repository_as_typed_processes() {
    let (model, seen) = px_common::scripted_model(vec![], vec![]).await;
    let envs = model_env(&model);
    let env: Vec<(&str, &str)> = envs.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    let (repo, root) = setup_repo(serde_json::json!([
        {"id": "make-marker", "argv": ["git", "init", "-q", "setup-ran"]},
        {"id": "second", "argv": ["git", "--version"]},
    ]));
    let mut fx = Fx::with_repo(repo, root, &env, tempfile::tempdir().unwrap()).await;

    // Untrusted: nothing runs, and the card says why.
    let t1 = fx.isolated(0x61).await;
    run_to_end(&mut fx, &t1, 0x62).await;
    assert!(!fx.worktree_dir(&t1).join("setup-ran").exists());
    let l = fx.list().await;
    let w = l
        .worktrees
        .iter()
        .find(|w| w.task_id.as_ref() == Some(&t1))
        .unwrap();
    assert_eq!(w.setup_status, "SKIPPED_UNTRUSTED", "{w:?}");
    assert!(w.setup_detail.contains("not trusted"), "{w:?}");

    // Trusted: the list runs as processes, output kept as an OutputRef.
    trust(&mut fx).await;
    let t2 = fx.isolated(0x63).await;
    run_to_end(&mut fx, &t2, 0x64).await;
    assert!(
        fx.worktree_dir(&t2).join("setup-ran").is_dir(),
        "the setup command ran in the worktree"
    );
    assert!(
        !Path::new(&fx.root).join("setup-ran").exists(),
        "and never in the checkout"
    );
    let l = fx.list().await;
    let w = l
        .worktrees
        .iter()
        .find(|w| w.task_id.as_ref() == Some(&t2))
        .unwrap();
    assert_eq!(w.setup_status, "PASSED", "{w:?}");
    let evs = fx.events().await;
    let rec = events_of(&evs, "WorktreeSetupRecorded");
    let last = rec.last().unwrap();
    assert_eq!(last["status"], "PASSED");
    assert_eq!(last["commands"].as_array().unwrap().len(), 2);
    assert!(
        last["commands"][0]["output_ref"]
            .as_str()
            .is_some_and(|r| r.len() == 64),
        "{last}"
    );
    // The model saw requests: the tasks went on after setup.
    assert!(!seen.lock().unwrap().is_empty());
}

#[tokio::test]
async fn px_065_a_failing_setup_is_visible_and_the_task_goes_on() {
    let (model, seen) = px_common::scripted_model(vec![], vec![]).await;
    let envs = model_env(&model);
    let env: Vec<(&str, &str)> = envs.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    let (repo, root) = setup_repo(serde_json::json!([
        {"id": "boom", "argv": ["git", "no-such-subcommand-xyz"]},
        {"id": "never", "argv": ["git", "init", "-q", "must-not-exist"]},
    ]));
    let mut fx = Fx::with_repo(repo, root, &env, tempfile::tempdir().unwrap()).await;
    trust(&mut fx).await;
    let t = fx.isolated(0x65).await;
    run_to_end(&mut fx, &t, 0x66).await;
    let l = fx.list().await;
    let w = l
        .worktrees
        .iter()
        .find(|w| w.task_id.as_ref() == Some(&t))
        .unwrap();
    assert_eq!(w.setup_status, "FAILED", "{w:?}");
    assert!(w.setup_detail.contains("boom"), "{w:?}");
    assert!(
        !fx.worktree_dir(&t).join("must-not-exist").exists(),
        "stopped at the first failure"
    );
    assert!(!seen.lock().unwrap().is_empty(), "the task ran on");
    let evs = fx.events().await;
    let last = events_of(&evs, "WorktreeSetupRecorded").pop().unwrap();
    assert_eq!(last["commands"][1]["skipped"], true);
}

#[cfg(unix)]
#[tokio::test]
async fn px_065_a_setup_command_past_its_timeout_is_stopped_and_reported() {
    let (model, _seen) = px_common::scripted_model(vec![], vec![]).await;
    let mut envs: Vec<(String, String)> = model_env(&model);
    envs.push(("MODBIT_WORKTREE_SETUP_TIMEOUT_MS".into(), "800".into()));
    let env: Vec<(&str, &str)> = envs.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    let (repo, root) = setup_repo(serde_json::json!([
        {"id": "sleeper", "argv": ["sleep", "60"]},
    ]));
    let mut fx = Fx::with_repo(repo, root, &env, tempfile::tempdir().unwrap()).await;
    trust(&mut fx).await;
    let t = fx.isolated(0x67).await;
    let started = std::time::Instant::now();
    run_to_end(&mut fx, &t, 0x68).await;
    assert!(
        started.elapsed() < Duration::from_secs(40),
        "{:?}",
        started.elapsed()
    );
    let l = fx.list().await;
    let w = l
        .worktrees
        .iter()
        .find(|w| w.task_id.as_ref() == Some(&t))
        .unwrap();
    assert_eq!(w.setup_status, "TIMED_OUT", "{w:?}");
}

// ===========================================================================
// PX-065: cleanup, lease, schedule, caps
// ===========================================================================

impl Fx {
    async fn cancel(&mut self, task: &Id) {
        let _: modbit_protocol::v1::TaskCancelRequested = self
            .cmd(
                "CancelTask",
                modbit_protocol::v1::CancelTask {
                    task_id: Some(task.clone()),
                },
                true,
            )
            .await
            .unwrap();
        // The cancellation of a task with no loop is immediate; wait for it
        // to be terminal all the same.
        let deadline = std::time::Instant::now() + Duration::from_secs(20);
        loop {
            let snap = self.snapshot().await;
            let t = snap
                .tasks
                .iter()
                .find(|t| t.task_id.as_ref() == Some(task))
                .unwrap();
            if t.state == "Cancelled" {
                return;
            }
            assert!(std::time::Instant::now() < deadline, "{t:?}");
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    async fn dirty(&mut self, task: &Id, call: u8) {
        let r = self
            .invoke(
                task,
                call,
                "change.apply",
                args_write("dirty.txt", "work in progress\n"),
            )
            .await;
        assert_eq!(r.status, "SUCCESS", "{r:?}");
    }
}

/// QUAL-PX-065: thirty worktrees (five dirty, three with unapplied commits,
/// two with a running task, one inside the protection window) and two
/// orphans, under a cap of 25: the cleanup removes only what is eligible,
/// orphans first and then least recently used, down to 80 percent of the
/// cap; the report matches the filesystem.
#[tokio::test]
async fn px_065_cleanup_removes_only_what_is_eligible_in_orphan_then_lru_order() {
    // The model for the two running tasks answers after ten minutes.
    let (model, _seen) = px_common::scripted_model_fn(std::sync::Arc::new(
        |_, _| serde_json::json!({"delay_ms": 600_000, "text": "late"}),
    ))
    .await;
    let mut envs: Vec<(String, String)> = model_env(&model);
    envs.push(("MODBIT_WORKTREE_PROTECT_MS".into(), "2500".into()));
    envs.push(("MODBIT_CAPACITY".into(), "model=4,provider=8".into()));
    let env: Vec<(&str, &str)> = envs.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    let mut fx = Fx::new(&[("README.md", "# cleanup\n")], &env).await;

    // 19 clean tasks (eligible), 5 dirty, 3 with commits only, 2 running.
    let mut clean: Vec<Id> = Vec::new();
    let mut dirty: Vec<Id> = Vec::new();
    let mut committed: Vec<Id> = Vec::new();
    let mut running: Vec<Id> = Vec::new();
    for i in 0..19u8 {
        let t = fx.isolated(0x80 + i).await;
        fx.cancel(&t).await;
        clean.push(t);
    }
    for i in 0..5u8 {
        let t = fx.isolated(0xa0 + i).await;
        fx.dirty(&t, 0xc0 + i).await;
        fx.cancel(&t).await;
        dirty.push(t);
    }
    for i in 0..3u8 {
        let t = fx.isolated(0xb0 + i).await;
        let wt = fx.worktree_dir(&t);
        write(&wt.join("committed.txt"), "a commit of the task\n");
        git(&wt, &["add", "-A"]);
        git(
            &wt,
            &[
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@e",
                "commit",
                "-q",
                "-m",
                "work",
            ],
        );
        fx.cancel(&t).await;
        committed.push(t);
    }
    for i in 0..2u8 {
        let t = fx.isolated(0xd0 + i).await;
        start_task(&mut fx.c, &t, fx.g, 0xe0 + i, "gpt-5-mini").await;
        running.push(t);
    }
    // Two orphans: directories under the managed root that no task owns.
    let orphans: Vec<PathBuf> = (0..2)
        .map(|i| {
            let p = fx.data().join("worktrees").join(format!("orphan-{i}"));
            git(
                Path::new(&fx.root),
                &[
                    "worktree",
                    "add",
                    "-q",
                    "-b",
                    &format!("orphan-branch-{i}"),
                    &p.to_string_lossy(),
                ],
            );
            p
        })
        .collect();
    // Everything so far outlives the protection window; one more does not.
    tokio::time::sleep(Duration::from_millis(3000)).await;
    let young = fx.isolated(0xf0).await;
    fx.cancel(&young).await;

    let all_before = fx.list().await;
    assert_eq!(all_before.count, 19 + 5 + 3 + 2 + 1 + 2);
    let by_task = |l: &WorktreeList, t: &Id| {
        l.worktrees
            .iter()
            .find(|w| w.task_id.as_ref() == Some(t))
            .unwrap()
            .clone()
    };
    for t in &dirty {
        let w = by_task(&all_before, t);
        assert!(w.dirty && w.unapplied && !w.removable, "{w:?}");
    }
    for t in &committed {
        let w = by_task(&all_before, t);
        assert!(!w.dirty && w.unapplied && !w.removable, "{w:?}");
    }
    for t in &running {
        let w = by_task(&all_before, t);
        assert!(w.task_running && w.protected && !w.removable, "{w:?}");
    }
    assert!(by_task(&all_before, &young).protected);
    // A dry run decides and removes nothing.
    let dry = fx.cleanup(None, true).await.unwrap();
    assert_eq!(dry.status, "DRY_RUN");
    assert_eq!(fx.list().await.count, all_before.count);

    // The real run, under the default caps (25 worktrees): 32 present, so it
    // removes eligible ones down to 80 percent of 25 = 20.
    let before_dirs: Vec<PathBuf> = all_before
        .worktrees
        .iter()
        .map(|w| PathBuf::from(&w.path))
        .collect();
    let report = fx.cleanup(None, false).await.unwrap();
    assert_eq!(report.status, "COMPLETED", "{report:?}");
    assert_eq!(report.scanned, 32);
    assert_eq!(report.removed, 12, "{report:?}");
    assert_eq!(report.remaining, 20, "{report:?}");
    // Orphans go first.
    for o in &orphans {
        assert!(!o.exists(), "{o:?}");
        assert!(
            report.removed_ids.iter().any(|id| id.contains("orphan-")),
            "{report:?}"
        );
    }
    // Then the least recently used of the clean tasks: the first ten made.
    let uuid_of = |t: &Id| {
        let mut b = [0u8; 16];
        b.copy_from_slice(&t.value);
        uuid_text(b)
    };
    let removed_tasks: Vec<String> = report
        .removed_ids
        .iter()
        .filter(|id| !id.contains("orphan-"))
        .cloned()
        .collect();
    assert_eq!(removed_tasks.len(), 10);
    for t in &clean[..10] {
        assert!(
            removed_tasks.contains(&uuid_of(t)),
            "oldest first: {removed_tasks:?}"
        );
        assert!(!fx.worktree_dir(t).exists());
    }
    for t in &clean[10..] {
        assert!(
            fx.worktree_dir(t).join(".git").exists(),
            "the caps were met without it"
        );
    }
    // Never a dirty, unapplied, running or young one.
    for t in dirty
        .iter()
        .chain(&committed)
        .chain(&running)
        .chain([&young])
    {
        assert!(
            fx.worktree_dir(t).join(".git").exists(),
            "{t:?} must survive"
        );
    }
    // The report matches the filesystem.
    let survivors = before_dirs
        .iter()
        .filter(|p| p.join(".git").exists())
        .count();
    assert_eq!(survivors as u32, report.remaining);
    let l = fx.list().await;
    assert_eq!(l.count, report.remaining);
    assert_eq!(
        l.last_cleanup.as_ref().unwrap().removed,
        report.removed,
        "the last run is served from the persisted state"
    );
    assert!(l.last_completed_at_ms > 0);
    // The dirty and unapplied ones raised an attention item, once, on their
    // own logs.
    assert!(
        report.attention.len() >= 8,
        "dirty and unapplied worktrees need a decision: {:?}",
        report.attention
    );
    let evs = fx.events().await;
    assert_eq!(events_of(&evs, "WorktreeNeedsDecision").len(), 8);
    assert_eq!(events_of(&evs, "WorktreeRemoved").len(), 10);
    // A second run under the caps removes nothing.
    let again = fx.cleanup(None, false).await.unwrap();
    assert_eq!(again.removed, 0, "{again:?}");
}

#[tokio::test]
async fn px_065_the_size_cap_removes_by_bytes_and_frees_what_the_report_says() {
    let mut fx = Fx::new(
        &[("README.md", "# bytes\n")],
        &[("MODBIT_WORKTREE_PROTECT_MS", "1")],
    )
    .await;
    let mut tasks = Vec::new();
    for i in 0..3u8 {
        let t = fx.isolated(0x70 + i).await;
        fx.cancel(&t).await;
        tasks.push(t);
    }
    let keeper = fx.isolated(0x74).await;
    fx.dirty(&keeper, 0x75).await;
    fx.cancel(&keeper).await;
    tokio::time::sleep(Duration::from_millis(50)).await;
    let before = fx.list().await;
    let sizes: u64 = before
        .worktrees
        .iter()
        .filter(|w| w.task_id.as_ref() != Some(&keeper))
        .map(|w| w.bytes)
        .sum();
    assert!(sizes > 0);
    let report = fx
        .cleanup(
            Some(WorktreePolicy {
                max_bytes: 1,
                ..Default::default()
            }),
            false,
        )
        .await
        .unwrap();
    assert_eq!(report.removed, 3, "{report:?}");
    assert_eq!(report.bytes_freed, sizes);
    assert!(
        fx.worktree_dir(&keeper).join(".git").exists(),
        "the dirty one is never removed"
    );
    for t in &tasks {
        assert!(!fx.worktree_dir(t).exists());
        // Its branch went with it.
    }
    let branches = git(Path::new(&fx.root), &["branch", "--list", "modbit/*"]);
    assert_eq!(branches.lines().count(), 1, "{branches}");
}

#[tokio::test]
async fn px_065_a_second_cleanup_while_the_lease_is_held_is_refused_naming_the_holder() {
    let mut fx = Fx::new(
        &[("README.md", "# lease\n")],
        &[("MODBIT_FAULT_WORKTREE_CLEANUP_HOLD_MS", "2500")],
    )
    .await;
    let mut second = fx.core.client().await;
    let first = tokio::spawn({
        let g = fx.g;
        let mut c = fx.core.client().await;
        async move {
            c.command(envelope_fenced(
                id16(0x90),
                "RunWorktreeCleanup",
                RunWorktreeCleanup {
                    reason: "first".into(),
                    ..Default::default()
                }
                .encode_to_vec(),
                g,
            ))
            .await
        }
    });
    tokio::time::sleep(Duration::from_millis(600)).await;
    let err = second
        .command(envelope_fenced(
            id16(0x91),
            "RunWorktreeCleanup",
            RunWorktreeCleanup {
                reason: "second".into(),
                ..Default::default()
            }
            .encode_to_vec(),
            fx.g,
        ))
        .await
        .unwrap_err();
    match err {
        ClientError::Rejected { code, message } => {
            assert_eq!(code, "CLEANUP_LEASE_HELD");
            assert!(
                message.contains("first"),
                "names the holder's reason: {message}"
            );
        }
        other => panic!("{other:?}"),
    }
    let done = first.await.unwrap().unwrap();
    let report: WorktreeCleanupReport = Client::result(&done).unwrap();
    assert_eq!(report.status, "COMPLETED");
    assert!(report.owner.contains("core-"), "{report:?}");
    // The lease is free again.
    let third = fx.cleanup(None, true).await;
    assert!(third.is_ok(), "{third:?}");
}

#[tokio::test]
async fn px_065_the_schedule_catches_up_after_a_restart_and_persists_the_last_run() {
    let dir = tempfile::tempdir().unwrap();
    // A last completed run, long overdue, from an earlier Core.
    std::fs::write(
        dir.path().join("worktree-cleanup.json"),
        r#"{"last_completed_at_ms": 1000, "last_run": null}"#,
    )
    .unwrap();
    let (repo, root) = plain_repo(&[("README.md", "# schedule\n")]);
    let mut fx = Fx::with_repo(
        repo,
        root,
        &[
            ("MODBIT_WORKTREE_CLEANUP_CATCHUP_MS", "700"),
            ("MODBIT_WORKTREE_CLEANUP_INTERVAL_MS", "3600000"),
        ],
        dir,
    )
    .await;
    let l = fx.list().await;
    assert_eq!(l.last_completed_at_ms, 1000, "nothing has run yet");
    assert!(l.next_cleanup_at_ms > 0, "the catch-up is scheduled");
    // It runs, as a catch-up, on its own.
    let mut seen = None;
    for _ in 0..60 {
        let l = fx.list().await;
        if let Some(r) = &l.last_cleanup
            && r.status == "COMPLETED"
        {
            seen = Some((r.clone(), l.last_completed_at_ms));
            break;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    let (report, completed) = seen.expect("the catch-up ran");
    assert_eq!(report.reason, "catch-up");
    assert!(report.owner.contains("scheduler"));
    assert!(
        completed > 1_000_000,
        "the time of the last completed run moved"
    );
    // Persisted: a restarted Core knows it ran, and does not run again.
    fx.restart(&[
        ("MODBIT_WORKTREE_CLEANUP_CATCHUP_MS", "300"),
        ("MODBIT_WORKTREE_CLEANUP_INTERVAL_MS", "3600000"),
    ])
    .await;
    tokio::time::sleep(Duration::from_millis(1200)).await;
    let l = fx.list().await;
    assert_eq!(l.last_completed_at_ms, completed);
    assert_eq!(
        l.last_cleanup.unwrap().run_id,
        report.run_id,
        "no second run"
    );
}

#[tokio::test]
async fn px_065_a_scheduled_run_that_finds_the_lease_held_is_skipped_not_completed() {
    let mut fx = Fx::new(
        &[("README.md", "# skipped\n")],
        &[
            ("MODBIT_FAULT_WORKTREE_CLEANUP_HOLD_MS", "2500"),
            ("MODBIT_WORKTREE_CLEANUP_CATCHUP_MS", "800"),
            ("MODBIT_WORKTREE_CLEANUP_INTERVAL_MS", "3600000"),
        ],
    )
    .await;
    // A manual run takes the lease first and holds it past the schedule.
    let mut holder = fx.core.client().await;
    let g = fx.g;
    let manual = tokio::spawn(async move {
        holder
            .command(envelope_fenced(
                id16(0x92),
                "RunWorktreeCleanup",
                RunWorktreeCleanup {
                    reason: "manual".into(),
                    ..Default::default()
                }
                .encode_to_vec(),
                g,
            ))
            .await
    });
    let mut skipped = None;
    for _ in 0..30 {
        let l = fx.list().await;
        if let Some(r) = &l.last_cleanup
            && r.status == "SKIPPED"
        {
            skipped = Some((r.clone(), l.last_completed_at_ms));
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let (report, completed) = skipped.expect("the scheduled attempt was recorded as skipped");
    assert!(
        report.skipped_reason.contains("CLEANUP_LEASE_HELD"),
        "{report:?}"
    );
    assert!(report.holder_reason.contains("manual"), "{report:?}");
    assert_eq!(report.removed, 0);
    assert_eq!(completed, 0, "a skipped run is never a completed one");
    manual.await.unwrap().unwrap();
}

/// Removal is confined to the worktree root after symlinks are resolved: a
/// recorded worktree whose directory was swapped for a link to somewhere else
/// is never followed, and what is behind the link is untouched.
#[cfg(unix)]
#[tokio::test]
async fn px_065_removal_never_follows_a_link_out_of_the_worktree_root() {
    let mut fx = Fx::new(
        &[("README.md", "# links\n")],
        &[("MODBIT_WORKTREE_PROTECT_MS", "1")],
    )
    .await;
    let t = fx.isolated(0x71).await;
    fx.cancel(&t).await;
    let outside = tempfile::tempdir().unwrap();
    write(&outside.path().join("precious.txt"), "do not delete\n");
    let wt = fx.worktree_dir(&t);
    let parked = fx.data().join("parked");
    std::fs::rename(&wt, &parked).unwrap();
    std::os::unix::fs::symlink(outside.path(), &wt).unwrap();
    // And an unrecorded link in the managed root.
    std::os::unix::fs::symlink(outside.path(), fx.data().join("worktrees").join("sneaky")).unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;
    let report = fx
        .cleanup(
            Some(WorktreePolicy {
                max_worktrees: 1,
                ..Default::default()
            }),
            false,
        )
        .await
        .unwrap();
    assert_eq!(report.removed, 0, "{report:?}");
    assert_eq!(
        text(&outside.path().join("precious.txt")),
        "do not delete\n"
    );
    assert!(parked.join(".git").exists());
}

// ===========================================================================
// PX-119: git.merge.* over the merge transaction
// ===========================================================================

/// A repository whose mandatory check passes while `src/shared.txt` holds
/// either side of the merge and fails when it holds neither, with two branches
/// that conflict on that file.
fn merge_repo() -> (tempfile::TempDir, String) {
    let (repo, root) = plain_repo(&[
        ("README.md", "# merge\n"),
        ("src/shared.txt", "base\n"),
        (
            ".modbit/verification.json",
            r#"{"commands": [{"id": "has-a-side", "argv": ["git", "grep", "-q", "-F", "-e", "alpha", "-e", "beta", "--", "src/shared.txt"]}]}"#,
        ),
    ]);
    let r = Path::new(&root);
    for (branch, content) in [("feat-a", "alpha\n"), ("feat-b", "beta\n")] {
        git(r, &["checkout", "-q", "-b", branch]);
        write(&r.join("src/shared.txt"), content);
        git(r, &["add", "-A"]);
        git(
            r,
            &[
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@e",
                "commit",
                "-q",
                "-m",
                branch,
            ],
        );
        git(r, &["checkout", "-q", "main"]);
    }
    (repo, root)
}

impl Fx {
    async fn try_invoke(
        &mut self,
        task: &Id,
        call: u8,
        tool: &str,
        args: serde_json::Value,
    ) -> Result<ToolInvoked, ClientError> {
        let id = self.n();
        let ack = self
            .c
            .command(envelope_fenced(
                id16(id),
                "InvokeTool",
                InvokeTool {
                    task_id: Some(task.clone()),
                    tool_name: tool.into(),
                    arguments_json: args.to_string(),
                    tool_call_id: Some(id16(call)),
                    output_budget_bytes: 1 << 20,
                }
                .encode_to_vec(),
                self.g,
            ))
            .await?;
        Ok(Client::result(&ack).unwrap())
    }
}

fn out(r: &ToolInvoked) -> serde_json::Value {
    serde_json::from_str(&r.structured_output_json).unwrap_or_default()
}

fn parent_head(fx: &Fx, task: &Id) -> String {
    git(&fx.worktree_dir(task), &["rev-parse", "HEAD"])
}

#[tokio::test]
async fn px_119_two_branches_conflict_the_resolution_is_verified_and_every_merge_has_a_receipt() {
    let (repo, root) = merge_repo();
    let mut fx = Fx::with_repo(repo, root, &[], tempfile::tempdir().unwrap()).await;
    let parent = fx.isolated(0x41).await;
    let wt = fx.worktree_dir(&parent);

    // The first branch merges cleanly: prepare (approval), commit (approval,
    // then the mandatory check runs on the merged tree).
    let p = fx
        .invoke_approved(
            &parent,
            0xb1,
            "git.merge.prepare",
            serde_json::json!({"source": "feat-a", "id": "tx-a"}),
        )
        .await;
    assert_eq!(p.status, "SUCCESS", "{p:?}");
    assert_eq!(out(&p)["status"], "STAGED");
    let c = fx
        .invoke_approved(
            &parent,
            0xb2,
            "git.merge.commit",
            serde_json::json!({"id": "tx-a"}),
        )
        .await;
    assert_eq!(c.status, "SUCCESS", "{c:?}");
    let o = out(&c);
    assert_eq!(o["status"], "COMMITTED");
    assert_eq!(o["verdict"], "PASS");
    assert_eq!(text(&wt.join("src/shared.txt")), "alpha\n");
    assert_eq!(
        parent_head(&fx, &parent),
        o["result_commit"].as_str().unwrap()
    );
    let after_a = tree_hashes(&wt);
    let head_a = parent_head(&fx, &parent);

    // The second conflicts: per-file evidence, nothing committed.
    let p = fx
        .invoke_approved(
            &parent,
            0xb3,
            "git.merge.prepare",
            serde_json::json!({"source": "feat-b", "id": "tx-b"}),
        )
        .await;
    assert_eq!(p.status, "SUCCESS", "{p:?}");
    let o = out(&p);
    assert_eq!(o["status"], "CONFLICTED");
    assert_eq!(o["target_before"], head_a.as_str());
    let conflicts = o["conflicts"].as_array().unwrap();
    assert_eq!(conflicts.len(), 1);
    let ev = &conflicts[0];
    assert_eq!(ev["path"], "src/shared.txt");
    assert_eq!(ev["base"].as_str().unwrap().trim(), "base");
    assert_eq!(ev["ours"].as_str().unwrap().trim(), "alpha");
    assert_eq!(ev["theirs"].as_str().unwrap().trim(), "beta");
    assert_eq!(ev["has_markers"], true);
    assert!(text(&wt.join("src/shared.txt")).contains("<<<<<<<"));
    // Committing without resolving is refused, typed.
    let c = fx
        .invoke_approved(
            &parent,
            0xb4,
            "git.merge.commit",
            serde_json::json!({"id": "tx-b"}),
        )
        .await;
    assert_eq!(c.status, "APPLICATION_FAILURE", "{c:?}");
    assert_eq!(c.error_code, "UNRESOLVED_MARKERS");
    assert_eq!(parent_head(&fx, &parent), head_a);

    // A resolution that fails the mandatory check does not count.
    let r = fx
        .invoke(
            &parent,
            0xb5,
            "change.apply",
            args_write("src/shared.txt", "neither side\n"),
        )
        .await;
    assert_eq!(r.status, "SUCCESS", "{r:?}");
    let c = fx
        .invoke_approved(
            &parent,
            0xb6,
            "git.merge.commit",
            serde_json::json!({"id": "tx-b"}),
        )
        .await;
    assert_eq!(c.status, "APPLICATION_FAILURE", "{c:?}");
    assert_eq!(c.error_code, "POST_MERGE_VERIFICATION_FAILED");
    assert!(
        c.error_message.contains("git grep") && c.error_message.contains("FAIL"),
        "{c:?}"
    );
    assert_eq!(
        parent_head(&fx, &parent),
        head_a,
        "the merge did not commit"
    );

    // Abort: the parent is byte-identical to before the merge began.
    let a = fx
        .invoke(
            &parent,
            0xb7,
            "git.merge.abort",
            serde_json::json!({"id": "tx-b"}),
        )
        .await;
    assert_eq!(a.status, "SUCCESS", "abort needs no approval: {a:?}");
    assert_eq!(out(&a)["status"], "ABORTED");
    assert_eq!(tree_hashes(&wt), after_a);
    assert_eq!(parent_head(&fx, &parent), head_a);
    assert_eq!(git(&wt, &["status", "--porcelain"]), "");

    // Again, resolved properly, verified by the real check, committed.
    let p = fx
        .invoke_approved(
            &parent,
            0xb8,
            "git.merge.prepare",
            serde_json::json!({"source": "feat-b", "id": "tx-b2"}),
        )
        .await;
    assert_eq!(out(&p)["status"], "CONFLICTED", "{p:?}");
    let r = fx
        .invoke(
            &parent,
            0xb9,
            "change.apply",
            args_write("src/shared.txt", "alpha and beta\n"),
        )
        .await;
    assert_eq!(r.status, "SUCCESS");
    let c = fx
        .invoke_approved(
            &parent,
            0xba,
            "git.merge.commit",
            serde_json::json!({"id": "tx-b2", "message": "integrate beta"}),
        )
        .await;
    assert_eq!(c.status, "SUCCESS", "{c:?}");
    let o = out(&c);
    assert_eq!(o["status"], "COMMITTED");
    assert_eq!(o["verdict"], "PASS");
    assert_eq!(o["target_before"], head_a.as_str());
    assert_eq!(
        o["source"],
        git(Path::new(&fx.root), &["rev-parse", "feat-b"]).as_str()
    );
    assert_eq!(o["resolutions"][0]["path"], "src/shared.txt");
    assert!(
        o["resolutions"][0]["author"]
            .as_str()
            .unwrap()
            .contains("User"),
        "{o}"
    );
    assert_eq!(text(&wt.join("src/shared.txt")), "alpha and beta\n");
    assert_eq!(
        git(&wt, &["log", "--merges", "--format=%s"])
            .lines()
            .count(),
        2
    );
    assert_eq!(git(&wt, &["log", "-1", "--format=%s"]), "integrate beta");
    // The user's checkout was never touched.
    assert_eq!(text(&Path::new(&fx.root).join("src/shared.txt")), "base\n");

    // Every commit that counts has a receipt: an authorization and a result,
    // both naming the intent, the result's evidence holding both revisions.
    let evs = fx.events().await;
    let proposals: Vec<(String, String)> = events_of(&evs, "ToolCallProposed")
        .into_iter()
        .filter(|p| p["tool_name"] == "git.merge.commit")
        .map(|p| {
            (
                p["arguments_hash"].as_str().unwrap().to_owned(),
                p["tool_name"].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    assert_eq!(proposals.len(), 4, "{proposals:?}");
    let receipts = events_of(&evs, "EffectReceiptAppended");
    let results = receipts
        .iter()
        .filter(|r| r["receipt"]["status"] == "SUCCESS")
        .filter(|r| {
            proposals
                .iter()
                .any(|(h, _)| r["receipt"]["intent_hash"] == h.as_str())
        })
        .count();
    assert_eq!(
        results, 2,
        "one result receipt per merge commit that happened"
    );
    let authorized = receipts
        .iter()
        .filter(|r| r["receipt"]["status"] == "AUTHORIZED")
        .filter(|r| {
            proposals
                .iter()
                .any(|(h, _)| r["receipt"]["intent_hash"] == h.as_str())
        })
        .count();
    assert_eq!(
        authorized,
        proposals.len(),
        "every commit attempt was authorized in the chain before it ran"
    );
    assert_eq!(c.effect_receipt_ids.len(), 1);

    // The merge state is on the log: replayable.
    let txs = events_of(&evs, "MergeTransactionRecorded");
    let states = |id: &str| -> Vec<String> {
        txs.iter()
            .filter(|t| t["tx_id"] == id)
            .map(|t| t["state"].as_str().unwrap().to_owned())
            .collect()
    };
    assert_eq!(states("tx-a"), ["STARTED", "STAGED", "STAGED", "COMMITTED"]);
    assert_eq!(
        states("tx-b"),
        ["STARTED", "CONFLICTED", "VERIFY_FAILED", "ABORTED"]
    );
    assert_eq!(
        states("tx-b2"),
        ["STARTED", "CONFLICTED", "STAGED", "COMMITTED"]
    );
    let verified = events_of(&evs, "MergeVerificationRecorded");
    assert_eq!(verified.iter().filter(|v| v["pass"] == true).count(), 2);
    assert_eq!(verified.iter().filter(|v| v["pass"] == false).count(), 1);
}

#[tokio::test]
async fn px_119_a_merge_needs_a_clean_target_and_an_approval() {
    let (repo, root) = merge_repo();
    let mut fx = Fx::with_repo(repo, root, &[], tempfile::tempdir().unwrap()).await;
    let parent = fx.isolated(0x42).await;
    let wt = fx.worktree_dir(&parent);
    // Without an approval nothing happens.
    let first = fx
        .invoke(
            &parent,
            0xc1,
            "git.merge.prepare",
            serde_json::json!({"source": "feat-a", "id": "tx-1"}),
        )
        .await;
    assert_eq!(first.status, "APPROVAL_PENDING", "{first:?}");
    assert_eq!(text(&wt.join("src/shared.txt")), "base\n");
    assert!(events_of(&fx.events().await, "MergeTransactionRecorded").is_empty());
    // A dirty target is refused: aborting must be able to restore it exactly.
    write(&wt.join("scratch.txt"), "work in progress\n");
    let r = fx
        .invoke_approved(
            &parent,
            0xc2,
            "git.merge.prepare",
            serde_json::json!({"source": "feat-a", "id": "tx-2"}),
        )
        .await;
    assert_eq!(r.status, "APPLICATION_FAILURE", "{r:?}");
    assert_eq!(r.error_code, "MERGE_TARGET_DIRTY");
    // An option-shaped source is an invalid ref, not an argument.
    std::fs::remove_file(wt.join("scratch.txt")).unwrap();
    let r = fx
        .invoke_approved(
            &parent,
            0xc3,
            "git.merge.prepare",
            serde_json::json!({"source": "--abort", "id": "tx-3"}),
        )
        .await;
    assert_eq!(r.status, "APPLICATION_FAILURE", "{r:?}");
    assert_eq!(r.error_code, "INVALID_REF");
    // The tools refuse a task whose lease does not grant them: a subagent's.
    let r = fx
        .invoke(
            &parent,
            0xc4,
            "git.merge.commit",
            serde_json::json!({"id": "no-such"}),
        )
        .await;
    assert!(
        r.status == "APPROVAL_PENDING",
        "the call is judged before the transaction is looked up: {r:?}"
    );
}

/// `SIGKILL` at three points of a merge; after the restart the tree is whole.
async fn kill_mid_merge(point: &str) -> (Fx, Id, String) {
    let (repo, root) = merge_repo();
    let mut fx = Fx::with_repo(
        repo,
        root,
        &[("MODBIT_FAULT_WORKTREE", point)],
        tempfile::tempdir().unwrap(),
    )
    .await;
    let parent = fx.isolated(0x43).await;
    let wt = fx.worktree_dir(&parent);
    let head0 = parent_head(&fx, &parent);
    let before = tree_hashes(&wt);
    let args = serde_json::json!({"source": "feat-a", "id": "tx-k"});
    if point == "MERGE_AFTER_COMMIT" {
        // The fault is on the commit path: prepare first (no fault there).
        // (Prepare passes MERGE_AFTER_STARTED/BEGIN only for those points.)
    }
    let first = fx
        .invoke(&parent, 0xd1, "git.merge.prepare", args.clone())
        .await;
    assert_eq!(first.status, "APPROVAL_PENDING");
    fx.resolve(&first.approval_id, true).await;
    let died = fx
        .try_invoke(&parent, 0xd1, "git.merge.prepare", args)
        .await;
    if point == "MERGE_AFTER_COMMIT" {
        // Prepare survived; the commit is where the Core dies.
        assert!(died.is_ok(), "{died:?}");
        let c = fx
            .invoke(
                &parent,
                0xd2,
                "git.merge.commit",
                serde_json::json!({"id": "tx-k"}),
            )
            .await;
        assert_eq!(c.status, "APPROVAL_PENDING");
        fx.resolve(&c.approval_id, true).await;
        let died = fx
            .try_invoke(
                &parent,
                0xd2,
                "git.merge.commit",
                serde_json::json!({"id": "tx-k"}),
            )
            .await;
        assert!(died.is_err(), "the Core was killed at {point}");
    } else {
        assert!(died.is_err(), "the Core was killed at {point}: {died:?}");
    }
    fx.restart(&[]).await;
    let _ = before;
    (fx, parent, head0)
}

#[tokio::test]
async fn px_119_a_core_killed_before_git_ran_aborts_the_merge_to_a_whole_tree() {
    let (mut fx, parent, head0) = kill_mid_merge("MERGE_AFTER_STARTED").await;
    let wt = fx.worktree_dir(&parent);
    assert_eq!(parent_head(&fx, &parent), head0);
    assert_eq!(git(&wt, &["status", "--porcelain"]), "");
    assert_eq!(text(&wt.join("src/shared.txt")), "base\n");
    let evs = fx.events().await;
    let states: Vec<String> = events_of(&evs, "MergeTransactionRecorded")
        .iter()
        .map(|t| t["state"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(
        states,
        ["STARTED", "ABORTED"],
        "recovery recorded the abort"
    );
    // The same merge can be done again from a whole tree.
    let p = fx
        .invoke_approved(
            &parent,
            0xd3,
            "git.merge.prepare",
            serde_json::json!({"source": "feat-a", "id": "tx-k2"}),
        )
        .await;
    assert_eq!(out(&p)["status"], "STAGED", "{p:?}");
}

#[tokio::test]
async fn px_119_a_core_killed_with_the_merge_staged_resumes_it_or_aborts_it_whole() {
    let (mut fx, parent, head0) = kill_mid_merge("MERGE_AFTER_BEGIN").await;
    let wt = fx.worktree_dir(&parent);
    // Git still holds the merge the Core began; the log only had the intent.
    // Recovery reconciles the two: the transaction is open and resumable.
    let evs = fx.events().await;
    let txs = events_of(&evs, "MergeTransactionRecorded");
    let last = txs.last().unwrap();
    assert_eq!(last["state"], "STAGED", "{last}");
    assert!(last["note"].as_str().unwrap().contains("resumed"), "{last}");
    assert_eq!(parent_head(&fx, &parent), head0);
    // Resume: verify and commit.
    let c = fx
        .invoke_approved(
            &parent,
            0xd4,
            "git.merge.commit",
            serde_json::json!({"id": "tx-k"}),
        )
        .await;
    assert_eq!(c.status, "SUCCESS", "{c:?}");
    assert_eq!(text(&wt.join("src/shared.txt")), "alpha\n");
    assert_eq!(
        git(&wt, &["log", "--merges", "--format=%s"])
            .lines()
            .count(),
        1
    );
}

#[tokio::test]
async fn px_119_a_core_killed_after_git_committed_records_the_commit() {
    let (mut fx, parent, head0) = kill_mid_merge("MERGE_AFTER_COMMIT").await;
    let wt = fx.worktree_dir(&parent);
    let head = parent_head(&fx, &parent);
    assert_ne!(head, head0, "git had committed");
    let evs = fx.events().await;
    let txs = events_of(&evs, "MergeTransactionRecorded");
    let last = txs.last().unwrap();
    assert_eq!(last["state"], "COMMITTED", "{last}");
    assert_eq!(last["result"], head.as_str());
    assert_eq!(git(&wt, &["status", "--porcelain"]), "");
    let _ = &mut fx;
}

// ===========================================================================
// PX-119: children, the merge tools through a real run, and the completion gate
// ===========================================================================

use serde_json::json;

fn child_script(file_content: &str) -> Vec<serde_json::Value> {
    vec![
        json!({"calls": [{"name": "plan.update", "args": {"outcome": "the file says it", "expected_files": ["src/shared.txt"]}}]}),
        json!({"calls": [{"name": "change.apply", "args": {"path": "src/shared.txt", "op": "replace", "content": file_content}}]}),
        json!({"calls": [{"name": "task.complete", "args": {"summary": "done", "self_review": {"findings": []}}}]}),
    ]
}

fn spawn_call(key: &str, word: &str) -> serde_json::Value {
    json!({"calls": [{"name": "agent.spawn", "args": {
        "idempotency_key": key,
        "objective": format!("write {word} into src/shared.txt"),
        "write_scope": ["src/shared.txt"],
        "max_turns": 8,
        "verification": "the file says it",
    }}]})
}

fn wait_call(key: &str) -> serde_json::Value {
    json!({"calls": [{"name": "agent.wait", "args": {"idempotency_key": key, "timeout_ms": 90000}}]})
}

/// A scripted model: the parent follows `parent`; the children are told apart
/// by the goal the Core puts in their prompt.
async fn family(parent: Vec<serde_json::Value>) -> (String, px_common::Seen) {
    let (a, b) = (child_script("alpha\n"), child_script("beta\n"));
    px_common::scripted_model_fn(std::sync::Arc::new(move |body, results| {
        let t = body.to_string();
        let pick = if t.contains("Task goal: write alpha") {
            &a
        } else if t.contains("Task goal: write beta") {
            &b
        } else {
            &parent
        };
        pick.get(results)
            .cloned()
            .unwrap_or_else(|| json!({"text": "I have nothing further to do."}))
    }))
    .await
}

/// The tool results the parent was shown, in order, from its longest request.
fn parent_tool_texts(seen: &px_common::Seen) -> Vec<String> {
    let seen = seen.lock().unwrap();
    let mut best: Vec<String> = Vec::new();
    for body in seen.iter() {
        if body.to_string().contains("Task goal: write") {
            continue;
        }
        let texts: Vec<String> = body["messages"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|m| m["role"] == "tool")
            .map(|m| m["content"].as_str().unwrap_or_default().to_owned())
            .collect();
        if texts.len() > best.len() {
            best = texts;
        }
    }
    best
}

/// Run a task to its end while approving every approval it asks for (the
/// person at the keyboard); returns the final status.
async fn drive(fx: &mut Fx, task: &Id, secs: u64) -> modbit_protocol::v1::TaskStatus {
    let deadline = std::time::Instant::now() + Duration::from_secs(secs);
    let mut approved: std::collections::HashSet<String> = Default::default();
    loop {
        for a in approvals(fx).await.approvals {
            if a.status == "REQUESTED" {
                let id = hex::encode(&a.approval_id.as_ref().unwrap().value);
                if approved.insert(id.clone()) {
                    fx.resolve(&id, true).await;
                }
            }
        }
        let st: modbit_protocol::v1::TaskStatus = fx
            .cmd(
                "GetTaskStatus",
                modbit_protocol::v1::GetTaskStatus {
                    task_id: Some(task.clone()),
                },
                false,
            )
            .await
            .unwrap();
        if !st.loop_alive {
            return st;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the run never ended: {st:?}"
        );
        tokio::time::sleep(Duration::from_millis(150)).await;
    }
}

/// `StartTask` with room for a parent that spawns, waits and merges.
async fn start_long(fx: &mut Fx, task: &Id, id: u8) {
    let _: modbit_protocol::v1::TaskRunStarted = fx
        .cmd(
            "StartTask",
            modbit_protocol::v1::StartTask {
                task_id: Some(task.clone()),
                model: "gpt-5-mini".into(),
                max_turns: 60,
                max_no_progress_turns: 12,
                ..Default::default()
            },
            true,
        )
        .await
        .unwrap();
    let _ = id;
}

async fn family_fx(parent: Vec<serde_json::Value>) -> (Fx, Id, px_common::Seen) {
    let (model, seen) = family(parent).await;
    let mut envs: Vec<(String, String)> = model_env(&model);
    envs.push(("MODBIT_CAPACITY".into(), "model=4,provider=8".into()));
    let env: Vec<(&str, &str)> = envs.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    let (repo, root) = merge_repo();
    let mut fx = Fx::with_repo(repo, root, &env, tempfile::tempdir().unwrap()).await;
    let parent = fx.isolated(0x44).await;
    (fx, parent, seen)
}

#[tokio::test]
async fn px_119_a_parent_merges_two_conflicting_children_through_a_real_run() {
    let parent_script = vec![
        json!({"calls": [{"name": "plan.update", "args": {"outcome": "integrate two children", "expected_files": ["src/shared.txt"], "protected_effects": ["git.merge"]}}]}),
        spawn_call("k-a", "alpha"),
        wait_call("k-a"),
        spawn_call("k-b", "beta"),
        wait_call("k-b"),
        json!({"calls": [{"name": "git.merge.prepare", "args": {"child": "k-a", "id": "tx-a"}}]}),
        json!({"calls": [{"name": "git.merge.commit", "args": {"id": "tx-a"}}]}),
        json!({"calls": [{"name": "git.merge.prepare", "args": {"child": "k-b", "id": "tx-b"}}]}),
        json!({"calls": [{"name": "fs.read", "args": {"path": "src/shared.txt"}}]}),
        json!({"calls": [{"name": "change.apply", "args": {"path": "src/shared.txt", "op": "replace", "content": "alpha and beta\n"}}]}),
        json!({"calls": [{"name": "git.merge.commit", "args": {"id": "tx-b"}}]}),
        json!({"calls": [{"name": "task.complete", "args": {"summary": "both children integrated", "self_review": {"findings": []}}}]}),
    ];
    let (mut fx, parent, seen) = family_fx(parent_script).await;
    let wt = fx.worktree_dir(&parent);
    start_long(&mut fx, &parent, 0x45).await;
    let st = drive(&mut fx, &parent, 240).await;
    assert_eq!(
        st.state,
        "ReadyForReview",
        "{}\n{:#?}",
        st.attention_reason,
        parent_tool_texts(&seen)
            .iter()
            .map(|t| t.chars().take(420).collect::<String>())
            .collect::<Vec<_>>()
    );

    // What the parent was shown: the conflict with its per-file evidence, and
    // the commit naming both revisions.
    let texts = parent_tool_texts(&seen);
    assert!(
        texts[5].contains("STAGED") && texts[5].contains("tx-a"),
        "{}",
        texts[5]
    );
    assert!(
        texts[6].contains("COMMITTED") && texts[6].contains("result_commit"),
        "{}",
        texts[6]
    );
    assert!(
        texts[7].contains("CONFLICTED")
            && texts[7].contains("src/shared.txt")
            && texts[7].contains("ours")
            && texts[7].contains("theirs"),
        "{}",
        texts[7]
    );
    assert!(
        texts[10].contains("COMMITTED") && texts[10].contains("target_before"),
        "{}",
        texts[10]
    );

    // The parent's worktree holds the verified, resolved merge of both.
    assert_eq!(text(&wt.join("src/shared.txt")), "alpha and beta\n");
    assert_eq!(
        git(&wt, &["log", "--merges", "--format=%s"])
            .lines()
            .count(),
        2
    );
    assert_eq!(git(&wt, &["status", "--porcelain"]), "");
    assert_eq!(
        text(&Path::new(&fx.root).join("src/shared.txt")),
        "base\n",
        "the user's checkout is untouched"
    );

    // The log: both children merged, both merges receipted, the resolution
    // authored by the agent.
    let evs = fx.events().await;
    let disposed = events_of(&evs, "WorktreeDisposed");
    assert_eq!(
        disposed
            .iter()
            .filter(|d| d["disposition"] == "MERGED")
            .count(),
        2,
        "{disposed:#?}"
    );
    let txs = events_of(&evs, "MergeTransactionRecorded");
    let b = txs.iter().rfind(|t| t["tx_id"] == "tx-b").unwrap();
    assert_eq!(b["state"], "COMMITTED");
    assert_eq!(b["resolutions"][0]["path"], "src/shared.txt");
    assert!(
        b["resolutions"][0]["author"]
            .as_str()
            .unwrap()
            .contains("Agent"),
        "{b}"
    );
    assert!(b["child_task_id"].is_string());
    let commits = events_of(&evs, "ToolCallProposed")
        .into_iter()
        .filter(|p| p["tool_name"] == "git.merge.commit")
        .count();
    let results = events_of(&evs, "EffectReceiptAppended")
        .into_iter()
        .filter(|r| {
            r["receipt"]["status"] == "SUCCESS" && r["receipt"]["execution_target"].is_string()
        })
        .count();
    assert!(
        commits == 2 && results >= 2,
        "{commits} commits, {results} result receipts"
    );
}

#[tokio::test]
async fn px_119_a_parent_cannot_complete_beside_an_unintegrated_child_until_it_is_merged_or_discarded()
 {
    let parent_script = vec![
        json!({"calls": [{"name": "plan.update", "args": {"outcome": "integrate", "expected_files": ["src/shared.txt"], "protected_effects": ["git.merge"]}}]}),
        spawn_call("k-a", "alpha"),
        wait_call("k-a"),
        spawn_call("k-b", "beta"),
        wait_call("k-b"),
        // Both children are over and both changed a file: completion refused.
        json!({"calls": [{"name": "task.complete", "args": {"summary": "done", "self_review": {"findings": []}}}]}),
        json!({"calls": [{"name": "git.merge.prepare", "args": {"child": "k-a", "id": "tx-a"}}]}),
        json!({"calls": [{"name": "git.merge.commit", "args": {"id": "tx-a"}}]}),
        // Still one unintegrated.
        json!({"calls": [{"name": "task.complete", "args": {"summary": "done", "self_review": {"findings": []}}}]}),
        // A recorded decision settles it.
        json!({"calls": [{"name": "git.merge.abort", "args": {"child": "k-b", "discard": true, "reason": "beta is not wanted"}}]}),
        json!({"calls": [{"name": "task.complete", "args": {"summary": "alpha integrated, beta discarded", "self_review": {"findings": []}}}]}),
    ];
    let (mut fx, parent, seen) = family_fx(parent_script).await;
    let wt = fx.worktree_dir(&parent);
    start_long(&mut fx, &parent, 0x46).await;
    let st = drive(&mut fx, &parent, 240).await;
    assert_eq!(
        st.state,
        "ReadyForReview",
        "{}\n{:#?}",
        st.attention_reason,
        parent_tool_texts(&seen)
            .iter()
            .map(|t| t.chars().take(420).collect::<String>())
            .collect::<Vec<_>>()
    );
    let texts = parent_tool_texts(&seen);
    // The refusal is typed and names each child and what to do.
    assert!(
        texts[5].contains("COMPLETION_REFUSED")
            && texts[5].contains("CHILDREN_NOT_INTEGRATED")
            && texts[5].contains("k-a")
            && texts[5].contains("k-b")
            && texts[5].contains("git.merge.prepare"),
        "{}",
        texts[5]
    );
    assert!(
        texts[8].contains("CHILDREN_NOT_INTEGRATED")
            && texts[8].contains("k-b")
            && !texts[8].contains("`k-a`"),
        "{}",
        texts[8]
    );
    assert!(
        texts[9].contains("ABORTED") || texts[9].contains("discarded_child"),
        "{}",
        texts[9]
    );
    assert_eq!(text(&wt.join("src/shared.txt")), "alpha\n");
    let evs = fx.events().await;
    let disposed = events_of(&evs, "WorktreeDisposed");
    assert_eq!(
        disposed
            .iter()
            .filter(|d| d["disposition"] == "MERGED")
            .count(),
        1
    );
    let discarded: Vec<_> = disposed
        .iter()
        .filter(|d| d["disposition"] == "DISCARDED")
        .collect();
    assert_eq!(discarded.len(), 1);
    assert_eq!(discarded[0]["reason"], "beta is not wanted");
}

// ===========================================================================
// PX-067 at the Core boundary: a hostile repository
// ===========================================================================

#[cfg(unix)]
fn find_bytes(root: &Path, needle: &[u8]) -> Vec<PathBuf> {
    let mut hits = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else {
            continue;
        };
        for e in rd.flatten() {
            let p = e.path();
            let Ok(m) = std::fs::symlink_metadata(&p) else {
                continue;
            };
            if m.is_dir() {
                stack.push(p);
            } else if m.is_file()
                && std::fs::read(&p).is_ok_and(|b| b.windows(needle.len()).any(|w| w == needle))
            {
                hits.push(p);
            }
        }
    }
    hits
}

/// A repository that ships a hook of every kind, an fsmonitor command, a clean
/// filter, an ssh command and a remote carrying a token: every Core operation
/// runs none of it, writes no token anywhere, and says on the task's log what
/// it refused.
#[cfg(unix)]
#[tokio::test]
async fn px_067_a_hostile_repository_runs_nothing_and_leaks_no_token_through_core_operations() {
    use std::os::unix::fs::PermissionsExt;
    let (repo, root) = merge_repo();
    let r = Path::new(&root);
    // The repository's own mandatory check is a command the *user* configured
    // and runs as itself; it is told to leave the fsmonitor out so that what
    // the assertion below sees is the Core's own Git, not the check's.
    write(
        &r.join(".modbit/verification.json"),
        r#"{"commands": [{"id": "has-a-side", "argv": ["git", "-c", "core.fsmonitor=false", "-c", "core.hooksPath=/dev/null", "grep", "-q", "-F", "-e", "alpha", "-e", "beta", "--", "src/shared.txt"]}]}"#,
    );
    git(r, &["add", "-A"]);
    git(
        r,
        &[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@e",
            "commit",
            "-q",
            "-m",
            "check",
        ],
    );
    let trap = tempfile::tempdir().unwrap();
    let t = trap.path().display().to_string();
    let script = |p: &Path, body: &str| {
        std::fs::write(p, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o755)).unwrap();
    };
    for h in [
        "pre-commit",
        "post-commit",
        "post-checkout",
        "post-merge",
        "pre-merge-commit",
        "reference-transaction",
    ] {
        script(
            &r.join(".git/hooks").join(h),
            &format!("touch '{t}/RAN_hook_{h}'"),
        );
    }
    let fsmon = trap.path().join("fsmon.sh");
    script(&fsmon, &format!("touch '{t}/RAN_fsmonitor'\nprintf '\\0'"));
    let ssh = trap.path().join("ssh.sh");
    script(&ssh, &format!("touch '{t}/RAN_ssh'\nexit 1"));
    let cfg = |k: &str, v: &str| {
        git(r, &["config", k, v]);
    };
    cfg("core.fsmonitor", &fsmon.display().to_string());
    cfg("core.sshCommand", &ssh.display().to_string());
    cfg("filter.evil.clean", &format!("touch '{t}/RAN_filter'; cat"));
    cfg(
        "filter.evil.smudge",
        &format!("touch '{t}/RAN_smudge'; cat"),
    );
    write(&r.join(".git/info/attributes"), "*.evil filter=evil\n");
    let token = "ghp_TOPSECRET_TOKEN_0123456789";
    cfg(
        "remote.origin.url",
        &format!("https://bob:{token}@127.0.0.1:1/o/r.git"),
    );
    write(&r.join("dirty.evil"), "user work\n");

    let mut fx = Fx::with_repo(repo, root.clone(), &[], tempfile::tempdir().unwrap()).await;
    // worktree add (post-checkout, smudge), dirty-state copy, status and diff.
    let parent = fx.isolated(0x47).await;
    let s = fx
        .invoke(&parent, 0xe1, "git.status", serde_json::json!({}))
        .await;
    assert_eq!(s.status, "SUCCESS", "{s:?}");
    let d = fx
        .invoke(&parent, 0xe2, "git.diff", serde_json::json!({}))
        .await;
    assert_eq!(d.status, "SUCCESS", "{d:?}");
    let w = fx
        .invoke(
            &parent,
            0xe3,
            "change.apply",
            args_write("touched.evil", "agent work\n"),
        )
        .await;
    assert_eq!(w.status, "SUCCESS", "{w:?}");
    // merge (pre-merge-commit, post-merge, commit hooks) and apply-back, from
    // a clean tree.
    let wt = fx.worktree_dir(&parent);
    for f in ["dirty.evil", "touched.evil"] {
        std::fs::remove_file(wt.join(f)).unwrap();
    }
    let p = fx
        .invoke_approved(
            &parent,
            0xe4,
            "git.merge.prepare",
            serde_json::json!({"source": "feat-a", "id": "tx-h"}),
        )
        .await;
    assert_eq!(p.status, "SUCCESS", "{p:?}");
    let c = fx
        .invoke_approved(
            &parent,
            0xe5,
            "git.merge.commit",
            serde_json::json!({"id": "tx-h"}),
        )
        .await;
    assert!(
        c.status == "SUCCESS" || c.status == "APPLICATION_FAILURE",
        "{c:?}"
    );
    let a = fx.apply(&parent, "", &[], false).await.unwrap();
    assert!(
        a.status == "APPROVAL_PENDING" || a.status == "CONFLICT" || a.status == "NOTHING_TO_APPLY",
        "{a:?}"
    );
    if a.status == "APPROVAL_PENDING" {
        fx.resolve(&a.approval_id, true).await;
        let done = fx.apply(&parent, "", &[], false).await.unwrap();
        assert_eq!(done.status, "APPLIED", "{done:?}");
    }
    // Remote operations are protected effects; the runner's own entry points
    // (the ones a pull request uses) must not run the ssh command either.
    let ran: Vec<String> = std::fs::read_dir(trap.path())
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with("RAN_"))
        .collect();
    assert!(ran.is_empty(), "the repository's programs ran: {ran:?}");

    // No token in any retained artifact: the event log, the object store, the
    // profile's files.
    let hits = find_bytes(&fx.data(), token.as_bytes());
    assert!(hits.is_empty(), "the token reached {hits:?}");
    // What the runner refused is on the task's log, once.
    let evs = fx.events().await;
    let found = events_of(&evs, "GitProgramsNeutralized");
    assert!(!found.is_empty(), "no security event was recorded");
    let kinds: std::collections::BTreeSet<String> = found
        .iter()
        .flat_map(|f| f["items"].as_array().cloned().unwrap_or_default())
        .map(|i| i["kind"].as_str().unwrap_or_default().to_owned())
        .collect();
    for k in ["HOOK", "FSMONITOR", "FILTER", "SSH_COMMAND"] {
        assert!(kinds.contains(k), "{k} not reported: {kinds:?}");
    }
    let distinct: std::collections::BTreeSet<(&str, &str)> = found
        .iter()
        .map(|f| (f["digest"].as_str().unwrap(), f["root"].as_str().unwrap()))
        .collect();
    assert_eq!(
        distinct.len(),
        found.len(),
        "recorded once per distinct finding and root"
    );
}

#[tokio::test]
async fn px_065_a_scheduled_run_fires_on_its_interval() {
    let dir = tempfile::tempdir().unwrap();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis();
    std::fs::write(
        dir.path().join("worktree-cleanup.json"),
        format!(r#"{{"last_completed_at_ms": {now}, "last_run": null}}"#),
    )
    .unwrap();
    let (repo, root) = plain_repo(&[("README.md", "# interval\n")]);
    let mut fx = Fx::with_repo(
        repo,
        root,
        &[
            ("MODBIT_WORKTREE_CLEANUP_INTERVAL_MS", "1500"),
            ("MODBIT_WORKTREE_CLEANUP_CATCHUP_MS", "100"),
        ],
        dir,
    )
    .await;
    // Not overdue: no catch-up. The schedule fires an interval after the last run.
    let first = fx.list().await;
    assert!(first.next_cleanup_at_ms as u128 >= now, "{first:?}");
    let mut seen = None;
    for _ in 0..40 {
        let l = fx.list().await;
        if let Some(r) = &l.last_cleanup
            && r.status == "COMPLETED"
        {
            seen = Some(r.clone());
            break;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    let r = seen.expect("the scheduled run fired");
    assert_eq!(r.reason, "scheduled");
}

// ---- a live worktree is never recorded as removed ----

/// The `WorktreeRemoved` events of a session, as payloads.
async fn removals(fx: &Fx) -> Vec<serde_json::Value> {
    events_of(&fx.events().await, "WorktreeRemoved")
}

/// A running task whose worktree directory cannot be found is not recorded as
/// removed: the cleanup leaves it for a later scan and says why. Once the task
/// has ended, the same scan records it, once.
#[tokio::test]
async fn px_065_a_missing_directory_under_a_running_task_is_not_recorded_as_removed() {
    let (model, _seen) = px_common::scripted_model_fn(std::sync::Arc::new(
        |_, _| serde_json::json!({"delay_ms": 600_000, "text": "late"}),
    ))
    .await;
    let mut envs: Vec<(String, String)> = model_env(&model);
    envs.push(("MODBIT_WORKTREE_PROTECT_MS".into(), "1".into()));
    let env: Vec<(&str, &str)> = envs.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    let mut fx = Fx::new(&[("README.md", "# live\n")], &env).await;
    let t = fx.isolated(0x61).await;
    start_task(&mut fx.c, &t, fx.g, 0x62, "gpt-5-mini").await;
    let mut running = false;
    for _ in 0..100 {
        let l = fx.list().await;
        if l.worktrees
            .iter()
            .any(|w| w.task_id.as_ref() == Some(&t) && w.task_running)
        {
            running = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(running, "the task is running");

    // A scan while the worktree is present and the task is running: nothing.
    let wt = fx.worktree_dir(&t);
    assert!(wt.exists());
    let r = fx.cleanup(None, false).await.unwrap();
    assert_eq!(r.removed, 0, "{r:?}");
    assert!(wt.exists(), "a live worktree is still there");
    assert!(removals(&fx).await.is_empty());

    // The directory cannot be found while the task runs: the scan must not
    // write it off.
    let parked = fx.data().join("parked-wt");
    std::fs::rename(&wt, &parked).unwrap();
    let r = fx.cleanup(None, false).await.unwrap();
    assert!(
        r.kept.iter().any(|k| k.why.contains("task has not ended")),
        "{r:?}"
    );
    assert!(
        removals(&fx).await.is_empty(),
        "no removal is recorded for a running task: {:?}",
        removals(&fx).await
    );

    // The task ends; the next scan records the removal exactly once.
    fx.cancel(&t).await;
    fx.cleanup(None, false).await.unwrap();
    fx.cleanup(None, false).await.unwrap();
    let rem = removals(&fx).await;
    assert_eq!(rem.len(), 1, "{rem:?}");
    assert_eq!(rem[0]["reason"], "its directory was gone");
}

/// A worktree the model made and closed with the governed Git tools is
/// recorded as removed by the close itself, whichever way its path is spelled
/// (forward slashes, a verbatim prefix, a short name), so a later scan has
/// nothing left to write off.
#[tokio::test]
async fn px_065_a_tool_closed_worktree_is_recorded_by_the_close_not_by_a_later_scan() {
    let mut fx = Fx::new(&[("README.md", "# tool\n")], &[]).await;
    let root = fx.root.clone();
    let t = fx
        .create(0x63, &root, TaskIsolation::None, "local_trusted")
        .await
        .unwrap();
    let wt = modbit_tools::direct::worktree_root(Path::new(&root))
        .unwrap()
        .join("scratch");
    let given = wt.to_string_lossy().replace('\\', "/");
    let made = fx
        .invoke(
            &t,
            0x64,
            "git.worktree.create",
            serde_json::json!({"branch": "t/scratch", "path": given}),
        )
        .await;
    assert_eq!(made.status, "SUCCESS", "{made:?}");
    assert!(wt.exists());
    // A scan while it is open removes nothing and records nothing.
    let r = fx.cleanup(None, false).await.unwrap();
    assert_eq!(r.removed, 0, "{r:?}");
    assert!(wt.exists());
    assert!(removals(&fx).await.is_empty());

    let closed = fx
        .invoke_approved(
            &t,
            0x65,
            "git.worktree.close",
            serde_json::json!({"path": given}),
        )
        .await;
    assert_eq!(closed.status, "SUCCESS", "{closed:?}");
    assert!(!wt.exists());
    let rem = removals(&fx).await;
    assert_eq!(rem.len(), 1, "the close recorded the removal: {rem:?}");
    assert_eq!(rem[0]["reason"], "closed with git.worktree.close");
    // A scan afterwards adds nothing.
    fx.cleanup(None, false).await.unwrap();
    assert_eq!(removals(&fx).await.len(), 1);
}
