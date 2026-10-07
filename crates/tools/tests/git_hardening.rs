//! VER-01 / FIX-01 (research/audit): the model-callable `git.*` tools against a
//! real repository. Argument injection through a "revision", repository-owned
//! executables (hooks, fsmonitor, filters) and unconfined worktree paths must
//! not reach the filesystem or run a program.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex as StdMutex};

use modbit_domain::{TaskId, ToolCallId};
use modbit_tools::{
    InvokeContext, ObjectSink, ProfilePolicy, ToolRegistry, ToolRuntime, ToolStatus,
};
use modbit_workspace::WorkspaceService;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tokio::sync::Mutex;

struct MemSink(StdMutex<Vec<(String, Vec<u8>)>>);
impl ObjectSink for MemSink {
    fn put(&self, bytes: &[u8]) -> modbit_tools::Result<String> {
        let h = hex::encode(Sha256::digest(bytes));
        self.0.lock().unwrap().push((h.clone(), bytes.to_vec()));
        Ok(h)
    }
}

fn git(dir: &Path, args: &[&str]) {
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
}

struct Fixture {
    /// Holds the workspace repo (`<outer>/repo`) and an `evidence` directory
    /// the attacks would write into.
    outer: tempfile::TempDir,
    _state: tempfile::TempDir,
    ctx: InvokeContext,
    runtime: ToolRuntime,
    root: PathBuf,
}

fn fixture() -> Fixture {
    let outer = tempfile::tempdir().unwrap();
    let root = outer.path().join("repo");
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::create_dir_all(outer.path().join("evidence")).unwrap();
    std::fs::write(root.join("src/main.rs"), "fn main() {}\n").unwrap();
    std::fs::write(root.join("README.md"), "# demo\n").unwrap();
    git(&root, &["init", "-q", "-b", "main"]);
    git(&root, &["add", "-A"]);
    git(&root, &["commit", "-q", "-m", "base"]);
    let state = tempfile::tempdir().unwrap();
    let canonical = root.canonicalize().unwrap();
    let ws = WorkspaceService::open(&canonical, state.path(), &[]).unwrap();
    let ctx = InvokeContext {
        task_id: TaskId::new(),
        execution_profile: "local_trusted".into(),
        capability_lease_id: None,
        workspace: Some(Arc::new(Mutex::new(ws))),
        workspace_root: Some(canonical.clone()),
        exec: None,
        sink: Arc::new(MemSink(StdMutex::new(vec![]))),
        output_budget_bytes: 4096,
        kernel: None,
        search: None,
        language: None,
        artifacts: None,
        tool_call_id: None,
        journal: None,
        forge: None,
        forge_ledger: None,
        browser: None,
        sandbox: None,
        effect_class: None,
        secrets_in_custody: vec![],
        environment: None,
        memory: None,
        external: None,
        cancel: None,
        hooks: None,
    };
    let mut registry = ToolRegistry::new();
    modbit_tools::direct::register_direct(&mut registry).unwrap();
    let runtime = ToolRuntime::new(registry, Arc::new(ProfilePolicy));
    Fixture {
        outer,
        _state: state,
        ctx,
        runtime,
        root: canonical,
    }
}

impl Fixture {
    fn evidence(&self) -> PathBuf {
        self.outer.path().join("evidence")
    }

    async fn call(&self, tool: &str, args: Value) -> modbit_tools::pipeline::PipelineOutcome {
        self.runtime
            .invoke(&self.ctx, ToolCallId::new(), tool, &args.to_string())
            .await
    }
}

fn entries(dir: &Path) -> Vec<String> {
    std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect()
}

/// VER-01: `base` was spliced into `{base}..{target}` with no
/// `--end-of-options`, so `--output=<path>` was an option of `git diff` and
/// the read-only `git.diff` tool wrote a file anywhere the process could.
#[tokio::test]
async fn ver_01_git_diff_output_option_in_a_revision_writes_no_file() {
    let f = fixture();
    let victim = f.evidence().join("pwned");
    let mut statuses = Vec::new();
    for (base, target) in [
        (format!("--output={}", victim.display()), "HEAD".to_owned()),
        (format!("--output={}", victim.display()), String::new()),
        ("HEAD".to_owned(), format!("--output={}", victim.display())),
    ] {
        let o = f
            .call("git.diff", json!({"base": base, "target": target}))
            .await;
        statuses.push((o.result.status, o.result.error_code.clone()));
    }
    assert_eq!(
        entries(&f.evidence()),
        Vec::<String>::new(),
        "git.diff must not write any file named by a model-supplied revision"
    );
    for (status, code) in statuses {
        assert_ne!(
            status,
            ToolStatus::Success,
            "an option-shaped revision must be refused ({code:?})"
        );
    }
}

fn branch_exists(root: &Path, name: &str) -> bool {
    Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["rev-parse", "--verify", "-q", &format!("refs/heads/{name}")])
        .output()
        .unwrap()
        .status
        .success()
}

/// The kernel decision the Core binds after an approval of a destructive call.
struct Approved;
impl modbit_tools::CapabilityPort for Approved {
    fn decide(&self, _: &modbit_tools::PolicyRequest) -> modbit_tools::PolicyDecision {
        modbit_tools::PolicyDecision::Allow {
            rule: "approval:test".into(),
            approval_id: Some("test".into()),
        }
    }
}

/// FIX-01(f): `git.worktree.create` places a checkout only under the
/// workspace's worktree root, and refuses before creating the branch.
#[tokio::test]
async fn worktree_create_is_confined_to_the_worktree_root() {
    let f = fixture();
    let outside = f.evidence().join("escaped");
    let inside_tree = f.root.join("nested-wt");
    let wt_root = modbit_tools::direct::worktree_root(&f.root).unwrap();
    for (i, path) in [
        outside.display().to_string(),
        inside_tree.display().to_string(),
        "../escaped".to_owned(),
        format!("{}/../escaped", wt_root.display()),
        "/".to_owned(),
        "..".to_owned(),
        ".".to_owned(),
    ]
    .iter()
    .enumerate()
    {
        let branch = format!("task/refused-{i}");
        let o = f
            .call(
                "git.worktree.create",
                json!({"branch": branch, "path": path}),
            )
            .await;
        assert_eq!(
            o.result.status,
            ToolStatus::ApplicationFailure,
            "{path}: {:?}",
            o.result
        );
        assert_eq!(
            o.result.error_code.as_deref(),
            Some("PATH_OUTSIDE_ROOT"),
            "{path}"
        );
        assert!(
            !branch_exists(&f.root, &branch),
            "a refused path must not leave a branch behind"
        );
    }
    assert!(!outside.exists() && !inside_tree.exists());
    assert!(!f.outer.path().join("escaped").exists());

    // A symlink inside the worktree root that points out is not a way out.
    #[cfg(unix)]
    {
        std::fs::create_dir_all(&wt_root).unwrap();
        std::os::unix::fs::symlink(f.evidence(), wt_root.join("link")).unwrap();
        let o = f
            .call(
                "git.worktree.create",
                json!({"branch": "task/link", "path": "link/wt"}),
            )
            .await;
        assert_eq!(
            o.result.status,
            ToolStatus::ApplicationFailure,
            "{:?}",
            o.result
        );
        assert!(!f.evidence().join("wt").exists());
    }

    // Inside the worktree root (absolute or relative) it works.
    let o = f
        .call(
            "git.worktree.create",
            json!({"branch": "task/ok", "path": wt_root.join("one").display().to_string()}),
        )
        .await;
    assert_eq!(o.result.status, ToolStatus::Success, "{:?}", o.result);
    assert!(wt_root.join("one/README.md").exists());
    let o = f
        .call(
            "git.worktree.create",
            json!({"branch": "task/ok2", "path": "two"}),
        )
        .await;
    assert_eq!(o.result.status, ToolStatus::Success, "{:?}", o.result);
    assert!(wt_root.join("two/README.md").exists());
}

/// FIX-01(f): `git.worktree.close` removes only a registered worktree under
/// the worktree root; never the user's checkout nor another repository's.
#[tokio::test]
async fn worktree_close_refuses_anything_but_a_registered_worktree_under_the_root() {
    let mut f = fixture();
    f.ctx.kernel = Some(Arc::new(Approved));
    let wt_root = modbit_tools::direct::worktree_root(&f.root).unwrap();

    // A real worktree the model did not create through the tool (e.g. the
    // user's own, next to the repository) must survive a close aimed at it.
    let users_wt = f.outer.path().join("users-own-worktree");
    git(&f.root, &["branch", "users-branch"]);
    git(
        &f.root,
        &[
            "worktree",
            "add",
            "-q",
            &users_wt.display().to_string(),
            "users-branch",
        ],
    );
    std::fs::write(users_wt.join("unsaved.txt"), "irreplaceable\n").unwrap();
    // An unrelated directory with files under the worktree root.
    std::fs::create_dir_all(wt_root.join("not-a-worktree")).unwrap();
    std::fs::write(wt_root.join("not-a-worktree/keep.txt"), "keep\n").unwrap();

    for (path, code) in [
        (f.root.display().to_string(), "PATH_OUTSIDE_ROOT"),
        (users_wt.display().to_string(), "PATH_OUTSIDE_ROOT"),
        (f.evidence().display().to_string(), "PATH_OUTSIDE_ROOT"),
        ("../repo".to_owned(), "PATH_OUTSIDE_ROOT"),
        ("not-a-worktree".to_owned(), "NOT_A_WORKTREE"),
        ("missing".to_owned(), "NOT_A_WORKTREE"),
    ] {
        let o = f.call("git.worktree.close", json!({"path": path})).await;
        assert_eq!(
            o.result.status,
            ToolStatus::ApplicationFailure,
            "{path}: {:?}",
            o.result
        );
        assert_eq!(o.result.error_code.as_deref(), Some(code), "{path}");
    }
    assert!(
        f.root.join("README.md").exists(),
        "the user's checkout is intact"
    );
    assert!(users_wt.join("unsaved.txt").exists());
    assert!(wt_root.join("not-a-worktree/keep.txt").exists());

    // The tool's own worktree closes.
    let o = f
        .call(
            "git.worktree.create",
            json!({"branch": "task/c", "path": "c"}),
        )
        .await;
    assert_eq!(o.result.status, ToolStatus::Success, "{:?}", o.result);
    let o = f.call("git.worktree.close", json!({"path": "c"})).await;
    assert_eq!(o.result.status, ToolStatus::Success, "{:?}", o.result);
    assert!(!wt_root.join("c").exists());
}

/// FIX-01(b,c): the read/write git tools on a repository armed with a
/// `post-checkout` hook and a `core.fsmonitor` command run neither.
#[cfg(unix)]
#[tokio::test]
async fn git_tools_do_not_run_repository_hooks_or_fsmonitor() {
    use std::os::unix::fs::PermissionsExt;
    let f = fixture();
    let trap = f.evidence();
    let hook = f.root.join(".git/hooks/post-checkout");
    std::fs::write(
        &hook,
        format!("#!/bin/sh\ntouch '{}/RAN_post_checkout'\n", trap.display()),
    )
    .unwrap();
    std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
    let fsmon = trap.join("fsmon.sh");
    std::fs::write(
        &fsmon,
        format!(
            "#!/bin/sh\ntouch '{}/RAN_fsmonitor'\nprintf '\\0'\n",
            trap.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&fsmon, std::fs::Permissions::from_mode(0o755)).unwrap();
    git(
        &f.root,
        &["config", "core.fsmonitor", &fsmon.display().to_string()],
    );

    // Control: plain git runs both (so a clean result below means something).
    git(&f.root, &["status", "--porcelain"]);
    git(&f.root, &["branch", "control"]);
    git(
        &f.root,
        &[
            "worktree",
            "add",
            "-q",
            &trap.join("control-wt").display().to_string(),
            "control",
        ],
    );
    let ran = entries(&trap);
    assert!(
        ran.contains(&"RAN_fsmonitor".to_owned()) && ran.contains(&"RAN_post_checkout".to_owned()),
        "control: plain git must run the armed programs; saw {ran:?}"
    );
    for n in ["RAN_fsmonitor", "RAN_post_checkout"] {
        std::fs::remove_file(trap.join(n)).unwrap();
    }

    std::fs::write(f.root.join("README.md"), "# changed\n").unwrap();
    let o = f.call("git.status", json!({})).await;
    assert_eq!(o.result.status, ToolStatus::Success, "{:?}", o.result);
    let o = f.call("git.diff", json!({})).await;
    assert_eq!(o.result.status, ToolStatus::Success, "{:?}", o.result);
    let o = f
        .call("git.diff", json!({"base": "HEAD", "target": "HEAD"}))
        .await;
    assert_eq!(o.result.status, ToolStatus::Success, "{:?}", o.result);
    let o = f
        .call(
            "git.worktree.create",
            json!({"branch": "task/armed", "path": "armed"}),
        )
        .await;
    assert_eq!(o.result.status, ToolStatus::Success, "{:?}", o.result);
    let ran: Vec<String> = entries(&trap)
        .into_iter()
        .filter(|n| n.starts_with("RAN_"))
        .collect();
    assert_eq!(ran, Vec::<String>::new(), "no hook or fsmonitor may run");
}

/// An option-shaped, ambiguous or unresolvable revision is a typed refusal
/// (`INVALID_REF` / `GIT`), never a successful call.
#[tokio::test]
async fn git_diff_refuses_malformed_revisions_with_a_typed_error() {
    let f = fixture();
    for (base, target, code) in [
        ("--output=x", "HEAD", "INVALID_REF"),
        ("-p", "HEAD", "INVALID_REF"),
        ("HEAD", "HEAD..HEAD", "INVALID_REF"),
        ("HEAD", "a b", "INVALID_REF"),
        ("HEAD", "no-such-rev", "GIT"),
    ] {
        let o = f
            .call("git.diff", json!({"base": base, "target": target}))
            .await;
        assert_eq!(
            o.result.status,
            ToolStatus::ApplicationFailure,
            "{base} {target}"
        );
        assert_eq!(
            o.result.error_code.as_deref(),
            Some(code),
            "{base} {target}"
        );
    }
    let o = f
        .call("git.diff", json!({"base": "HEAD", "target": "HEAD"}))
        .await;
    assert_eq!(o.result.status, ToolStatus::Success, "{:?}", o.result);
}
