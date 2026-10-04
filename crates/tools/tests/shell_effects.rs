//! VER-02 / FIX-02 / FIX-06 / FIX-10 (audit remediation): what a `shell.exec`
//! or `test.run` call may do without the Capability Kernel hearing about it.
//! Everything here runs real processes through the real `modbit-execd` broker
//! against a real git repository, a real bare remote and the real file
//! system. No effector is mocked; the only stand-ins are the kernel adapters
//! that model the Core's "approval granted" / "approval denied" answers.

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex as StdMutex};

use modbit_domain::{TaskId, ToolCallId};
use modbit_protocol::local::{ReadyLine, decode_hex};
use modbit_tools::pipeline::ExecTarget;
use modbit_tools::{
    CapabilityPort, EffectClass, InvokeContext, ObjectSink, PipelineOutcome, PolicyDecision,
    PolicyRequest, ProfilePolicy, ToolRegistry, ToolRuntime, ToolStatus,
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

fn git_out(dir: &Path, args: &[&str]) -> String {
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

fn execd_bin() -> PathBuf {
    let deps = std::env::current_exe().unwrap();
    let debug = deps.parent().unwrap().parent().unwrap();
    let p = debug.join(if cfg!(windows) {
        "modbit-execd.exe"
    } else {
        "modbit-execd"
    });
    if !p.exists() {
        let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
        assert!(
            Command::new(cargo)
                .args(["build", "-p", "modbit-execd", "--locked"])
                .status()
                .unwrap()
                .success()
        );
    }
    p
}

struct Execd(Child, ReadyLine);
impl Drop for Execd {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn spawn_execd(dir: &Path) -> Execd {
    let mut child = Command::new(execd_bin())
        .arg("--data-dir")
        .arg(dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap();
    let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
    let ready = loop {
        let l = lines.next().expect("ready").unwrap();
        if let Some(r) = ReadyLine::parse(&l) {
            break r;
        }
    };
    std::thread::spawn(move || for _ in lines {});
    Execd(child, ready)
}

struct World {
    _data: tempfile::TempDir,
    _execd: Execd,
    _state: tempfile::TempDir,
    _root: tempfile::TempDir,
    root: PathBuf,
    ctx: InvokeContext,
    runtime: ToolRuntime,
    sink: Arc<MemSink>,
}

/// A real repository with committed files, a broker and a `local_trusted`
/// task context (lease-less default policy).
fn world(profile: &str) -> World {
    let data = tempfile::tempdir().unwrap();
    let execd = spawn_execd(data.path());
    let target = ExecTarget {
        endpoint: execd.1.endpoint.clone(),
        boot_secret: decode_hex(&execd.1.boot_secret_hex).unwrap(),
        replay_generation: 0,
    };
    let root = tempfile::tempdir().unwrap();
    let state = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(root.path().join("src")).unwrap();
    std::fs::write(root.path().join("src/main.rs"), "fn main() {}\n").unwrap();
    std::fs::write(root.path().join("README.md"), "# demo\n").unwrap();
    std::fs::write(root.path().join(".env"), "SECRET=1\n").unwrap();
    git_out(root.path(), &["init", "-q", "-b", "main"]);
    git_out(root.path(), &["add", "-A"]);
    git_out(root.path(), &["commit", "-q", "-m", "base"]);
    let canonical = root.path().canonicalize().unwrap();
    let ws = WorkspaceService::open(&canonical, state.path(), &[]).unwrap();
    let sink = Arc::new(MemSink(StdMutex::new(vec![])));
    let ctx = InvokeContext {
        task_id: TaskId::new(),
        execution_profile: profile.into(),
        capability_lease_id: None,
        workspace: Some(Arc::new(Mutex::new(ws))),
        workspace_root: Some(canonical.clone()),
        exec: Some(target),
        sink: sink.clone(),
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
    World {
        _data: data,
        _execd: execd,
        _state: state,
        _root: root,
        root: canonical,
        ctx,
        runtime,
        sink,
    }
}

/// The kernel as the Core adapts it once a user answered an approval: the
/// classes that need one are allowed (`approved`) or denied.
struct AnsweredApproval {
    approved: bool,
    seen: StdMutex<Vec<(String, EffectClass)>>,
}
impl CapabilityPort for AnsweredApproval {
    fn decide(&self, req: &PolicyRequest) -> PolicyDecision {
        self.seen
            .lock()
            .unwrap()
            .push((req.tool_name.clone(), req.effect_class));
        match ProfilePolicy.decide(req) {
            PolicyDecision::ApprovalRequired { .. } if self.approved => PolicyDecision::Allow {
                rule: "approval:granted".into(),
                approval_id: Some("approval-1".into()),
            },
            PolicyDecision::ApprovalRequired { reason, .. } => PolicyDecision::Deny {
                code: "APPROVAL_DENIED".into(),
                reason,
                approval_required: true,
            },
            other => other,
        }
    }
}

async fn shell(w: &World, tool: &str, args: Value) -> PipelineOutcome {
    w.runtime
        .invoke(&w.ctx, ToolCallId::new(), tool, &args.to_string())
        .await
}

fn argv_args(argv: &[&str]) -> Value {
    json!({"argv": argv, "inherit_env": true})
}

/// A bare remote the workspace pushes `main` to, then a rewritten local
/// history that only a force-push can publish.
fn remote_with_diverged_history(w: &World) -> (tempfile::TempDir, String, String) {
    let remote = tempfile::tempdir().unwrap();
    git_out(remote.path(), &["init", "-q", "--bare", "-b", "main"]);
    git_out(
        &w.root,
        &["remote", "add", "origin", remote.path().to_str().unwrap()],
    );
    git_out(&w.root, &["push", "-q", "origin", "main"]);
    let published = git_out(remote.path(), &["rev-parse", "main"]);
    // Rewrite the published commit: a plain push is now rejected, a forced one wins.
    git_out(
        &w.root,
        &["commit", "-q", "--amend", "-m", "rewritten history"],
    );
    let rewritten = git_out(&w.root, &["rev-parse", "HEAD"]);
    assert_ne!(published, rewritten);
    (remote, published, rewritten)
}

/// VER-02 (audit G, `direct.rs:1894-1913`): under `local_trusted` a model
/// could force-push over a published branch without any approval.
#[tokio::test]
async fn ver_02_git_push_force_stops_at_an_approval_and_a_denial_leaves_the_remote_intact() {
    let w = world("local_trusted");
    let (remote, published, rewritten) = remote_with_diverged_history(&w);
    let remote_head = || git_out(remote.path(), &["rev-parse", "main"]);

    let o = shell(
        &w,
        "shell.exec",
        argv_args(&["git", "push", "--force", "origin", "main"]),
    )
    .await;
    assert_eq!(
        o.result.status,
        ToolStatus::ApprovalPending,
        "a force-push must stop at an approval under local_trusted (class {:?}): {:?}",
        o.effect_class,
        o.result
    );
    assert_eq!(o.effect_class, Some(EffectClass::Destructive));
    assert_eq!(remote_head(), published, "nothing may have been pushed");

    // The kernel's answer is a denial: still nothing pushed.
    let mut denied = w.ctx.clone();
    let port = Arc::new(AnsweredApproval {
        approved: false,
        seen: StdMutex::new(vec![]),
    });
    denied.kernel = Some(port.clone());
    let o = w
        .runtime
        .invoke(
            &denied,
            ToolCallId::new(),
            "shell.exec",
            &argv_args(&["git", "push", "--force", "origin", "main"]).to_string(),
        )
        .await;
    assert_eq!(o.result.status, ToolStatus::PolicyDenied, "{:?}", o.result);
    assert_eq!(
        remote_head(),
        published,
        "a denial leaves the remote ref intact"
    );
    assert_eq!(
        port.seen.lock().unwrap().as_slice(),
        [("shell.exec".to_owned(), EffectClass::Destructive)],
        "the kernel is asked about the class of what the argv does"
    );

    // Once the approval is granted the very same call runs and the real effect lands.
    let mut approved = w.ctx.clone();
    approved.kernel = Some(Arc::new(AnsweredApproval {
        approved: true,
        seen: StdMutex::new(vec![]),
    }));
    let o = w
        .runtime
        .invoke(
            &approved,
            ToolCallId::new(),
            "shell.exec",
            &argv_args(&["git", "push", "--force", "origin", "main"]).to_string(),
        )
        .await;
    assert_eq!(o.result.status, ToolStatus::Success, "{:?}", o.result);
    assert_eq!(
        remote_head(),
        rewritten,
        "the approved force-push published"
    );
}

/// A plain push and a network fetch are external side effects: approval first.
#[tokio::test]
async fn ver_02_git_push_and_network_clients_are_external_effects() {
    let w = world("local_trusted");
    let (remote, published, _rewritten) = remote_with_diverged_history(&w);
    // A fast-forwardable commit on top of the remote's head.
    git_out(&w.root, &["reset", "-q", "--hard", &published]);
    std::fs::write(w.root.join("more.txt"), "x\n").unwrap();
    git_out(&w.root, &["add", "-A"]);
    git_out(&w.root, &["commit", "-q", "-m", "more"]);
    let o = shell(
        &w,
        "shell.exec",
        argv_args(&["git", "push", "origin", "main"]),
    )
    .await;
    assert_eq!(
        o.result.status,
        ToolStatus::ApprovalPending,
        "{:?}",
        o.result
    );
    assert_eq!(o.effect_class, Some(EffectClass::ExternalSideEffect));
    assert_eq!(git_out(remote.path(), &["rev-parse", "main"]), published);
    // The same push wrapped in `sh -c`, `env` and `sudo`-less wrappers is still judged.
    for argv in [
        vec!["sh", "-c", "git push origin main"],
        vec!["bash", "-lc", "cd . && git push origin main"],
        vec!["env", "FOO=1", "git", "push", "origin", "main"],
        vec!["curl", "-X", "POST", "http://127.0.0.1:9/never"],
        vec!["sh", "-c", "echo hi | xargs curl http://127.0.0.1:9/never"],
    ] {
        let o = shell(&w, "shell.exec", argv_args(&argv)).await;
        assert_eq!(
            o.result.status,
            ToolStatus::ApprovalPending,
            "{argv:?}: {:?}",
            o.result
        );
        assert_eq!(
            o.effect_class,
            Some(EffectClass::ExternalSideEffect),
            "{argv:?}"
        );
    }
    assert_eq!(git_out(remote.path(), &["rev-parse", "main"]), published);
}

/// VER-02: `rm -rf <dir>` under `local_trusted` ran with no approval.
#[tokio::test]
async fn ver_02_rm_rf_stops_at_an_approval_and_a_denial_leaves_the_directory_intact() {
    let w = world("local_trusted");
    let victim = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(victim.path().join("deep/er")).unwrap();
    std::fs::write(
        victim.path().join("deep/er/precious.txt"),
        "irreplaceable\n",
    )
    .unwrap();
    let victim_path = victim.path().to_str().unwrap().to_owned();

    let o = shell(&w, "shell.exec", argv_args(&["rm", "-rf", &victim_path])).await;
    assert_eq!(
        o.result.status,
        ToolStatus::ApprovalPending,
        "rm -rf must stop at an approval under local_trusted (class {:?}): {:?}",
        o.effect_class,
        o.result
    );
    assert_eq!(o.effect_class, Some(EffectClass::Destructive));
    assert!(victim.path().join("deep/er/precious.txt").exists());

    // Hidden behind a shell string, a wrapper or find, it is the same effect.
    for argv in [
        vec![
            "sh".to_owned(),
            "-c".into(),
            format!("rm -rf {victim_path}"),
        ],
        vec![
            "bash".into(),
            "-c".into(),
            format!("cd / && rm -fr '{victim_path}'"),
        ],
        vec!["env".into(), "rm".into(), "-r".into(), victim_path.clone()],
        vec!["find".into(), victim_path.clone(), "-delete".into()],
        vec![
            "find".into(),
            victim_path.clone(),
            "-exec".into(),
            "rm".into(),
            "-rf".into(),
            "{}".into(),
            ";".into(),
        ],
    ] {
        let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
        let o = shell(&w, "shell.exec", argv_args(&refs)).await;
        assert_eq!(
            o.result.status,
            ToolStatus::ApprovalPending,
            "{argv:?}: {:?}",
            o.result
        );
        assert_eq!(o.effect_class, Some(EffectClass::Destructive), "{argv:?}");
        assert!(
            victim.path().join("deep/er/precious.txt").exists(),
            "{argv:?}"
        );
    }

    let mut denied = w.ctx.clone();
    denied.kernel = Some(Arc::new(AnsweredApproval {
        approved: false,
        seen: StdMutex::new(vec![]),
    }));
    let o = w
        .runtime
        .invoke(
            &denied,
            ToolCallId::new(),
            "shell.exec",
            &argv_args(&["rm", "-rf", &victim_path]).to_string(),
        )
        .await;
    assert_eq!(o.result.status, ToolStatus::PolicyDenied, "{:?}", o.result);
    assert!(
        victim.path().join("deep/er/precious.txt").exists(),
        "a denial leaves the directory intact"
    );

    // Approved, it really deletes.
    let mut approved = w.ctx.clone();
    approved.kernel = Some(Arc::new(AnsweredApproval {
        approved: true,
        seen: StdMutex::new(vec![]),
    }));
    let o = w
        .runtime
        .invoke(
            &approved,
            ToolCallId::new(),
            "shell.exec",
            &argv_args(&["rm", "-rf", &victim_path]).to_string(),
        )
        .await;
    assert_eq!(o.result.status, ToolStatus::Success, "{:?}", o.result);
    assert!(!victim.path().exists(), "the approved rm -rf ran");
    // tempdir drop of a removed dir is fine.
    std::mem::forget(victim);
}

/// The classifier must not break ordinary work: reads, workspace writes,
/// local git and project scripts keep running without an approval.
#[tokio::test]
async fn ordinary_commands_still_run_without_an_approval_under_local_trusted() {
    let w = world("local_trusted");
    std::fs::write(w.root.join("check.sh"), "echo checked > checked.txt\n").unwrap();
    for argv in [
        vec!["git", "status", "--porcelain"],
        vec!["git", "log", "--oneline", "-n", "1"],
        vec!["ls", "-la"],
        vec!["cat", "README.md"],
        vec!["grep", "-rn", "main", "src"],
        vec![
            "sh",
            "-c",
            "mkdir -p out && echo hi > out/a.txt && cat out/a.txt",
        ],
        vec!["sh", "check.sh"],
        vec!["touch", "new-file.txt"],
        vec!["rm", "new-file.txt"],
        vec![
            "sh",
            "-c",
            "i=0; while [ $i -lt 3 ]; do echo line $i; i=$((i+1)); done",
        ],
    ] {
        let o = shell(&w, "shell.exec", argv_args(&argv)).await;
        assert!(
            matches!(
                o.result.status,
                ToolStatus::Success | ToolStatus::ApplicationFailure
            ),
            "{argv:?} must run without an approval: {:?} ({:?})",
            o.result,
            o.stages
        );
        assert_ne!(o.result.status, ToolStatus::ApprovalPending, "{argv:?}");
    }
    assert_eq!(
        std::fs::read_to_string(w.root.join("out/a.txt")).unwrap(),
        "hi\n"
    );
    // An unknown program is not a project script: it needs an approval under local_trusted ...
    let o = shell(
        &w,
        "shell.exec",
        argv_args(&["/opt/unknown-tool/bin/frobnicate"]),
    )
    .await;
    assert_eq!(
        o.result.status,
        ToolStatus::ApprovalPending,
        "{:?}",
        o.result
    );
    // ... but not under local_autonomous, where nothing can wait for an approval.
    let mut auto = w.ctx.clone();
    auto.execution_profile = "local_autonomous".into();
    let o = w
        .runtime
        .invoke(
            &auto,
            ToolCallId::new(),
            "shell.exec",
            &argv_args(&["/opt/unknown-tool/bin/frobnicate"]).to_string(),
        )
        .await;
    assert_ne!(
        o.result.status,
        ToolStatus::ApprovalPending,
        "{:?}",
        o.result
    );
    // And under local_autonomous a force-push is above the profile ceiling: denied, not run.
    let o = w
        .runtime
        .invoke(
            &auto,
            ToolCallId::new(),
            "shell.exec",
            &argv_args(&["git", "push", "--force", "origin", "main"]).to_string(),
        )
        .await;
    assert_ne!(o.result.status, ToolStatus::Success, "{:?}", o.result);
    let _ = &w.sink;
}
