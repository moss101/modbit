//! The backend contract suite (QUAL-EV-0291: the same suite runs against the
//! reference backend and the MicroVM substrate; QUAL-EV-0285, QUAL-EV-0287,
//! QUAL-EV-0289, QUAL-EV-0290 draw on its steps). Every step is a real call
//! through a real guest; a backend passes when every step holds. Network
//! isolation is asserted only of a backend that claims to isolate.

#[cfg(feature = "client")]
use std::sync::Arc;
use std::time::Duration;

use modbit_protocol::v1::{self as wire, guest_call, guest_reply};

use crate::backend::{Channel, SandboxBackend};
use crate::link::GuestLink;
use crate::policy::{CompiledPolicy, SandboxSpec, compile};
use crate::{Result, SandboxError};

/// What the suite needs from its caller.
pub struct Fixture {
    /// The spec to provision (its workspace source holds `NOTES.md`).
    pub spec: SandboxSpec,
    /// A shell command line that prints `hello` and exits 3
    /// (`/bin/sh -c 'echo hello; exit 3'` on a POSIX guest).
    pub exec_probe: Vec<String>,
    /// A command line that copies its stdin to its stdout (`cat`).
    pub cat_probe: Vec<String>,
    /// A shell command line that sleeps for longer than a second.
    pub sleep_probe: Vec<String>,
    /// A shell command line that prints more than the output bound.
    pub flood_probe: Vec<String>,
    /// The environment every exec probe runs with (the guest inherits
    /// nothing; a Windows host's `cmd` needs `SystemRoot`).
    pub exec_env: Vec<String>,
    /// A host and port on the control plane the guest must not reach.
    pub control_endpoint: (String, u16),
    /// A shell command line that prints `line 1` … `line 5`, one every
    /// ~300 ms, then exits 0 (M8.5: followed with replay across links).
    pub lines_probe: Vec<String>,
    /// A shell command line that reads one line from stdin and prints it
    /// prefixed with `got:` (M8.5: stdin to a followed process).
    pub echo_stdin_probe: Vec<String>,
    /// Whether the guest can open a PTY (`false` on a Windows host).
    pub pty: bool,
    /// A shell command line that tries to write `.git/hooks/pre-commit`
    /// under the workspace and exits non-zero when it cannot.
    pub write_hook_probe: Vec<String>,
    /// A command line printing the process environment (`env`).
    pub env_probe: Vec<String>,
    /// Egress (M8.6): the destinations the suite fetches through the
    /// guest's proxy and the secrets the broker holds; `None` skips the
    /// egress steps (the spec grants nothing).
    pub egress: Option<EgressFixture>,
}

/// What the egress steps need (M8.6).
pub struct EgressFixture {
    /// A URL on a host the spec admits (`http://127.0.0.1:<port>/hello`),
    /// answering `hello from allowed`.
    pub allowed_url: String,
    /// A URL on a host the spec does not admit.
    pub denied_url: String,
    /// An `https://` URL the spec does not admit (a CONNECT tunnel).
    pub denied_tunnel_url: String,
    /// A URL on the credentialed virtual host (`http://forge.modbit.internal/user`),
    /// whose target answers `"authorized":true` only with the real secret.
    pub credentialed_url: String,
    /// The secret the broker holds under the grant's handle.
    pub secret: String,
    /// The secrets by handle.
    pub secrets: std::collections::HashMap<String, String>,
    /// A command line fetching a URL's body to stdout (`wget -qO- <url>` on
    /// BusyBox, `curl -s <url>` elsewhere); the URL is appended.
    pub fetch: Vec<String>,
    /// A command line asking the sandbox's proxy for a CONNECT tunnel to a
    /// `host:port` (appended) and printing the proxy's answer — for a
    /// userland whose fetcher does not tunnel `https://` itself (BusyBox
    /// wget sends it as a plain proxied GET). `None`: `fetch` of
    /// `denied_tunnel_url` tunnels (curl does).
    pub tunnel: Option<Vec<String>>,
}

/// One step's outcome.
#[derive(Clone, Debug, serde::Serialize)]
pub struct Step {
    /// Name.
    pub name: &'static str,
    /// Held.
    pub ok: bool,
    /// What was observed.
    pub detail: String,
}

/// The suite's report.
#[derive(Clone, Debug, serde::Serialize)]
pub struct Report {
    /// Backend kind.
    pub backend: &'static str,
    /// Whether it claims isolation.
    pub isolated: bool,
    /// The guest's hello.
    pub guest_version: String,
    /// Steps.
    pub steps: Vec<Step>,
}

impl Report {
    /// Every step held.
    #[must_use]
    pub fn passed(&self) -> bool {
        self.steps.iter().all(|s| s.ok)
    }

    /// The failed steps.
    #[must_use]
    pub fn failures(&self) -> Vec<&Step> {
        self.steps.iter().filter(|s| !s.ok).collect()
    }
}

fn refusal_code(r: &Result<impl std::fmt::Debug>) -> String {
    match r {
        Err(SandboxError::Refused { code, .. }) => code.clone(),
        Ok(v) => format!("ok: {v:?}"),
        Err(e) => format!("error: {e}"),
    }
}

/// Run the suite on `backend`.
pub async fn run(backend: &dyn SandboxBackend, fx: &Fixture) -> Result<Report> {
    let policy: CompiledPolicy = compile(&fx.spec)?;
    let sandbox_id = format!("conf-{}", uuid::Uuid::now_v7().simple());
    let task = fx.spec.task_id.to_string();
    let mut provisioned = backend.provision(&sandbox_id, &policy).await?;
    #[allow(unused_mut, unused_variables)]
    let mut egress_rx = provisioned.egress.take();
    let mut steps = Vec::new();
    let mut push =
        |name: &'static str, ok: bool, detail: String| steps.push(Step { name, ok, detail });
    push(
        "provisioned",
        provisioned.sandbox_id == sandbox_id,
        provisioned.detail.clone(),
    );
    let mut link: GuestLink<Channel> = GuestLink::admit_image(
        provisioned.channel,
        &sandbox_id,
        &policy,
        Duration::from_secs(30),
        backend.image(),
    )
    .await?;
    link.set_call_timeout(Duration::from_secs(30));
    let guest_version = link.hello.guest_version.clone();
    let boot_id = link.hello.boot_id.clone();
    push(
        "negotiated",
        link.hello.protocol_major == crate::GUEST_PROTOCOL_MAJOR,
        format!(
            "guest {} protocol {}.{} methods {:?}",
            guest_version, link.hello.protocol_major, link.hello.protocol_minor, link.hello.methods
        ),
    );
    // health
    let h = link.health(&task).await;
    push(
        "health",
        h.as_ref().is_ok_and(|h| h.boot_id == link.hello.boot_id),
        format!("{h:?}"),
    );
    // exec
    let r = link
        .exec(
            &task,
            "eff-1",
            wire::GuestExec {
                argv: fx.exec_probe.clone(),
                cwd: policy.workspace_root.clone(),
                env: fx.exec_env.clone(),
                timeout_ms: 20_000,
                stdin: vec![],
            },
        )
        .await;
    push(
        "exec",
        r.as_ref().is_ok_and(|r| {
            r.exit_code == 3 && String::from_utf8_lossy(&r.stdout).trim() == "hello" && !r.timed_out
        }),
        format!("{r:?}"),
    );
    // exec with stdin: what the call feeds is what the process reads
    let r = link
        .exec(
            &task,
            "eff-2",
            wire::GuestExec {
                argv: fx.cat_probe.clone(),
                cwd: policy.workspace_root.clone(),
                env: fx.exec_env.clone(),
                timeout_ms: 20_000,
                stdin: b"fed by the suite".to_vec(),
            },
        )
        .await;
    push(
        "exec_stdin",
        r.as_ref()
            .is_ok_and(|r| String::from_utf8_lossy(&r.stdout).contains("fed by the suite")),
        format!("{r:?}"),
    );
    // exec timeout
    let r = link
        .exec(
            &task,
            "eff-3",
            wire::GuestExec {
                argv: fx.sleep_probe.clone(),
                cwd: policy.workspace_root.clone(),
                env: fx.exec_env.clone(),
                timeout_ms: 700,
                stdin: vec![],
            },
        )
        .await;
    push(
        "exec_timeout",
        r.as_ref().is_ok_and(|r| r.timed_out),
        format!("{r:?}"),
    );
    // exec timeout above the ceiling is refused
    let r = link
        .exec(
            &task,
            "eff-4",
            wire::GuestExec {
                argv: fx.exec_probe.clone(),
                cwd: policy.workspace_root.clone(),
                env: fx.exec_env.clone(),
                timeout_ms: policy.spec.resources.exec_timeout_ms + 1,
                stdin: vec![],
            },
        )
        .await;
    push(
        "exec_timeout_ceiling",
        refusal_code(&r) == "LIMIT_EXCEEDED",
        refusal_code(&r),
    );
    // output bound
    let r = link
        .exec(
            &task,
            "eff-5",
            wire::GuestExec {
                argv: fx.flood_probe.clone(),
                cwd: policy.workspace_root.clone(),
                env: fx.exec_env.clone(),
                timeout_ms: 25_000,
                stdin: vec![],
            },
        )
        .await;
    push(
        "exec_output_bounded",
        r.as_ref().is_ok_and(|r| {
            r.stdout_truncated && (r.stdout.len() as u64) <= policy.spec.resources.max_output_bytes
        }),
        r.as_ref()
            .map(|r| format!("{} bytes, truncated {}", r.stdout.len(), r.stdout_truncated))
            .unwrap_or_else(|e| e.to_string()),
    );
    // fs: the workspace arrived, a write lands, a read reads it back
    let r = link
        .read_file(&task, &format!("{}/NOTES.md", policy.workspace_root), 1024)
        .await;
    push(
        "fs_workspace_present",
        r.as_ref().is_ok_and(|f| f.content.starts_with(b"# notes")),
        format!("{r:?}"),
    );
    let path = format!("{}/out/result.txt", policy.workspace_root);
    let w = link
        .write_file(&task, "eff-6", &path, b"written by the suite\n".to_vec())
        .await;
    let r = link.read_file(&task, &path, 1024).await;
    push(
        "fs_write_read",
        w.is_ok()
            && r.as_ref()
                .is_ok_and(|f| f.content == b"written by the suite\n"),
        format!("{w:?} {r:?}"),
    );
    // fs: protected path, outside the workspace, a control path
    let r = link
        .write_file(
            &task,
            "eff-7",
            &format!("{}/.git/hooks/pre-commit", policy.workspace_root),
            b"x".to_vec(),
        )
        .await;
    push(
        "fs_protected_path_refused",
        refusal_code(&r) == "PROTECTED_PATH",
        refusal_code(&r),
    );
    let r = link
        .write_file(&task, "eff-8", "/etc/hostname", b"x".to_vec())
        .await;
    push(
        "fs_outside_workspace_refused",
        refusal_code(&r) == "OUTSIDE_WORKSPACE",
        refusal_code(&r),
    );
    let r = link
        .write_file(
            &task,
            "eff-9",
            &format!("{}/../init", policy.workspace_root),
            b"x".to_vec(),
        )
        .await;
    push(
        "fs_traversal_refused",
        refusal_code(&r) == "OUTSIDE_WORKSPACE",
        refusal_code(&r),
    );
    let r = link.read_file(&task, "/init", 16).await;
    push(
        "fs_control_path_unreadable",
        refusal_code(&r) == "OUTSIDE_WORKSPACE",
        refusal_code(&r),
    );
    // A process cannot write a protected path either, where the backend
    // isolates (a MicroVM pins them read-only at the mount level); the
    // reference backend enforces protected paths in its own calls only.
    let r = link
        .exec(
            &task,
            "eff-9b",
            wire::GuestExec {
                argv: fx.write_hook_probe.clone(),
                cwd: policy.workspace_root.clone(),
                env: fx.exec_env.clone(),
                timeout_ms: 20_000,
                stdin: vec![],
            },
        )
        .await;
    let refused = r.as_ref().is_ok_and(|r| r.exit_code != 0);
    push(
        "proc_protected_path_enforced",
        if backend.isolates() {
            refused
        } else {
            r.is_ok()
        },
        format!("{r:?} (asserted: {})", backend.isolates()),
    );
    // network: the control plane is unreachable from an isolated guest
    let p = link
        .net_probe(&task, &fx.control_endpoint.0, fx.control_endpoint.1, 3_000)
        .await;
    let unreachable = p.as_ref().is_ok_and(|p| !p.reachable);
    push(
        "net_control_plane_unreachable",
        if backend.isolates() {
            unreachable
        } else {
            p.is_ok()
        },
        format!("{p:?} (asserted: {})", backend.isolates()),
    );
    // M8.5 — followed processes: output after a cursor, the exit, and a
    // replay across a lost link: the link is dropped mid-run, a new channel
    // is admitted (a new credential), and the follow resumes from the
    // cursor with nothing re-run and nothing lost.
    let started = link
        .proc_start(
            &task,
            "eff-10",
            wire::GuestProcStart {
                argv: fx.lines_probe.clone(),
                cwd: policy.workspace_root.clone(),
                env: fx.exec_env.clone(),
                timeout_ms: 20_000,
                pty: false,
                cols: 0,
                rows: 0,
                stdin_open: false,
            },
        )
        .await;
    let proc_id = started
        .as_ref()
        .map(|p| p.proc_id.clone())
        .unwrap_or_default();
    push(
        "proc_start",
        started.as_ref().is_ok_and(|p| !p.proc_id.is_empty()),
        format!("{started:?}"),
    );
    let first = link.proc_follow(&task, &proc_id, 0, 0, 5_000).await;
    let cursor = first.as_ref().map(|o| o.cursor).unwrap_or(0);
    push(
        "proc_follow_first",
        first
            .as_ref()
            .is_ok_and(|o| String::from_utf8_lossy(&o.data).contains("line 1") && o.running),
        format!("{first:?}"),
    );
    // Drop the link mid-run; the guest keeps the process and its output.
    let old_channel = link.into_inner();
    drop(old_channel);
    let fresh = backend.reconnect(&sandbox_id).await?;
    let mut link: GuestLink<Channel> = GuestLink::admit_image(
        fresh,
        &sandbox_id,
        &policy,
        Duration::from_secs(30),
        backend.image(),
    )
    .await?;
    link.set_call_timeout(Duration::from_secs(30));
    push(
        "relinked",
        link.hello.boot_id == boot_id,
        format!("same boot {}", link.hello.boot_id),
    );
    let mut all = first.as_ref().map(|o| o.data.clone()).unwrap_or_default();
    let mut cur = cursor;
    let mut exit = None;
    let mut truncated = false;
    for _ in 0..40 {
        let Ok(o) = link.proc_follow(&task, &proc_id, cur, 0, 3_000).await else {
            break;
        };
        all.extend_from_slice(&o.data);
        cur = o.cursor;
        truncated |= o.truncated;
        if !o.running {
            exit = o.exit_code;
            break;
        }
    }
    let text = String::from_utf8_lossy(&all).into_owned();
    push(
        "proc_replay_across_links",
        (1..=5).all(|i| text.contains(&format!("line {i}")))
            && text.matches("line 3").count() == 1
            && exit == Some(0)
            && !truncated,
        format!("exit {exit:?} truncated {truncated} text {text:?}"),
    );
    // A replay from cursor 0 of an exited process returns the whole output again.
    let again = link.proc_follow(&task, &proc_id, 0, 0, 100).await;
    push(
        "proc_replay_from_start",
        again
            .as_ref()
            .is_ok_and(|o| String::from_utf8_lossy(&o.data).contains("line 5") && !o.running),
        format!("{again:?}"),
    );
    // stdin to a followed process, then cancel of a long one.
    let started = link
        .proc_start(
            &task,
            "eff-11",
            wire::GuestProcStart {
                argv: fx.echo_stdin_probe.clone(),
                cwd: policy.workspace_root.clone(),
                env: fx.exec_env.clone(),
                timeout_ms: 20_000,
                pty: false,
                cols: 0,
                rows: 0,
                stdin_open: true,
            },
        )
        .await;
    let pid2 = started
        .as_ref()
        .map(|p| p.proc_id.clone())
        .unwrap_or_default();
    let w = link
        .proc_write(&task, &pid2, b"hello stdin\n".to_vec(), true)
        .await;
    let mut got = Vec::new();
    let mut cur = 0;
    let mut running = true;
    for _ in 0..20 {
        let Ok(o) = link.proc_follow(&task, &pid2, cur, 0, 3_000).await else {
            break;
        };
        got.extend_from_slice(&o.data);
        cur = o.cursor;
        running = o.running;
        if !running {
            break;
        }
    }
    push(
        "proc_stdin",
        w.is_ok() && !running && String::from_utf8_lossy(&got).contains("got:hello stdin"),
        format!("{w:?} {:?}", String::from_utf8_lossy(&got)),
    );
    let started = link
        .proc_start(
            &task,
            "eff-12",
            wire::GuestProcStart {
                argv: fx.sleep_probe.clone(),
                cwd: policy.workspace_root.clone(),
                env: fx.exec_env.clone(),
                timeout_ms: 20_000,
                pty: false,
                cols: 0,
                rows: 0,
                stdin_open: false,
            },
        )
        .await;
    let pid3 = started
        .as_ref()
        .map(|p| p.proc_id.clone())
        .unwrap_or_default();
    let c = link.proc_cancel(&task, &pid3).await;
    let after = link.proc_follow(&task, &pid3, 0, 0, 2_000).await;
    push(
        "proc_cancel",
        c.is_ok() && after.as_ref().is_ok_and(|o| !o.running && o.cancelled),
        format!("{c:?} {after:?}"),
    );
    // PTY: the same call with a window; output arrives as one stream; resize is accepted.
    if fx.pty {
        let started = link
            .proc_start(
                &task,
                "eff-13",
                wire::GuestProcStart {
                    argv: fx.exec_probe.clone(),
                    cwd: policy.workspace_root.clone(),
                    env: fx.exec_env.clone(),
                    timeout_ms: 20_000,
                    pty: true,
                    cols: 80,
                    rows: 24,
                    stdin_open: false,
                },
            )
            .await;
        let pid4 = started
            .as_ref()
            .map(|p| p.proc_id.clone())
            .unwrap_or_default();
        let rs = link.pty_resize(&task, &pid4, 100, 30).await;
        let mut out = Vec::new();
        let mut cur = 0;
        let mut code = None;
        for _ in 0..20 {
            let Ok(o) = link.proc_follow(&task, &pid4, cur, 0, 3_000).await else {
                break;
            };
            out.extend_from_slice(&o.data);
            cur = o.cursor;
            if !o.running {
                code = o.exit_code;
                break;
            }
        }
        push(
            "pty",
            started.is_ok() && String::from_utf8_lossy(&out).contains("hello") && code == Some(3),
            format!(
                "{started:?} resize {rs:?} exit {code:?} {:?}",
                String::from_utf8_lossy(&out)
            ),
        );
    } else {
        push("pty", true, "not applicable on this host".into());
    }
    // Directory operations under the policy.
    let m = link
        .mkdir(
            &task,
            "eff-14",
            &format!("{}/dir/sub", policy.workspace_root),
        )
        .await;
    let w = link
        .write_file(
            &task,
            "eff-15",
            &format!("{}/dir/sub/a.txt", policy.workspace_root),
            b"a".to_vec(),
        )
        .await;
    let l = link
        .list_dir(&task, &format!("{}/dir", policy.workspace_root), 0)
        .await;
    let st = link
        .stat(&task, &format!("{}/dir/sub/a.txt", policy.workspace_root))
        .await;
    let rn = link
        .rename(
            &task,
            "eff-16",
            &format!("{}/dir/sub/a.txt", policy.workspace_root),
            &format!("{}/dir/b.txt", policy.workspace_root),
        )
        .await;
    let st2 = link
        .stat(&task, &format!("{}/dir/sub/a.txt", policy.workspace_root))
        .await;
    let rm = link
        .remove(
            &task,
            "eff-17",
            &format!("{}/dir", policy.workspace_root),
            true,
        )
        .await;
    let st3 = link
        .stat(&task, &format!("{}/dir", policy.workspace_root))
        .await;
    push(
        "fs_dir_ops",
        m.is_ok()
            && w.is_ok()
            && l.as_ref()
                .is_ok_and(|l| l.entries.iter().any(|e| e.name == "sub" && e.kind == "dir"))
            && st
                .as_ref()
                .is_ok_and(|s| s.exists && s.kind == "file" && s.size == 1)
            && rn.is_ok()
            && st2.as_ref().is_ok_and(|s| !s.exists)
            && rm.is_ok()
            && st3.as_ref().is_ok_and(|s| !s.exists),
        format!("{m:?} {w:?} {l:?} {st:?} {rn:?} {st2:?} {rm:?} {st3:?}"),
    );
    // The workspace root lists workspace content on every backend: the
    // block device's own `lost+found` on a MicroVM is not an entry.
    let r = link.list_dir(&task, &policy.workspace_root, 0).await;
    push(
        "fs_root_lists_workspace_content",
        r.as_ref().is_ok_and(|l| {
            !l.entries.is_empty() && l.entries.iter().all(|e| e.name != "lost+found")
        }),
        format!("{r:?}"),
    );
    let r = link
        .remove(
            &task,
            "eff-18",
            &format!("{}/.git/hooks", policy.workspace_root),
            true,
        )
        .await;
    push(
        "fs_dir_protected_refused",
        refusal_code(&r) == "PROTECTED_PATH",
        refusal_code(&r),
    );
    let r = link
        .remove(&task, "eff-19", &policy.workspace_root, true)
        .await;
    push(
        "fs_workspace_root_not_removable",
        refusal_code(&r) == "PROTECTED_PATH",
        refusal_code(&r),
    );
    let r = link.list_dir(&task, "/etc", 0).await;
    push(
        "fs_dir_outside_refused",
        refusal_code(&r) == "OUTSIDE_WORKSPACE",
        refusal_code(&r),
    );
    // M8.6 — egress through the guest's proxy and the host's broker: an
    // admitted plain-HTTP destination is reached, a destination outside
    // the allow-list is refused (HTTP and CONNECT alike), a credentialed
    // virtual host is reached with the secret injected by the broker while
    // the guest never held it; every decision is on the audit.
    #[cfg(feature = "client")]
    if let Some(eg) = &fx.egress {
        let audit = Arc::new(crate::egress::MemoryAudit::default());
        let broker = crate::egress::EgressBroker::new(
            &sandbox_id,
            fx.spec.network.clone(),
            eg.secrets.clone(),
            Arc::clone(&audit) as Arc<dyn crate::egress::EgressAudit>,
        );
        let broker_task = egress_rx
            .take()
            .map(|rx| tokio::spawn(Arc::clone(&broker).serve(rx)));
        push(
            "egress_broker_channel",
            broker_task.is_some(),
            "the backend handed the broker the guest's egress channels".into(),
        );
        let fetch = |url: &str| -> Vec<String> {
            let mut v = eg.fetch.clone();
            v.push(url.to_owned());
            v
        };
        let r = link
            .exec(
                &task,
                "eff-20",
                wire::GuestExec {
                    argv: fetch(&eg.allowed_url),
                    cwd: policy.workspace_root.clone(),
                    env: fx.exec_env.clone(),
                    timeout_ms: 20_000,
                    stdin: vec![],
                },
            )
            .await;
        push(
            "egress_allowed_http",
            r.as_ref()
                .is_ok_and(|r| String::from_utf8_lossy(&r.stdout).contains("hello from allowed")),
            format!("{r:?}"),
        );
        let r = link
            .exec(
                &task,
                "eff-21",
                wire::GuestExec {
                    argv: fetch(&eg.denied_url),
                    cwd: policy.workspace_root.clone(),
                    env: fx.exec_env.clone(),
                    timeout_ms: 20_000,
                    stdin: vec![],
                },
            )
            .await;
        push(
            "egress_denied_http",
            r.as_ref()
                .is_ok_and(|r| !String::from_utf8_lossy(&r.stdout).contains("hello")),
            format!("{r:?}"),
        );
        // With a tunnel probe the proxy's refusal is in the answer; a
        // tunnelling fetcher fails instead.
        let (tunnel_argv, direct_probe) = match &eg.tunnel {
            Some(argv) => {
                let mut v = argv.clone();
                let host = eg
                    .denied_tunnel_url
                    .trim_start_matches("https://")
                    .split('/')
                    .next()
                    .unwrap_or_default()
                    .to_owned();
                v.push(host);
                (v, true)
            }
            None => (fetch(&eg.denied_tunnel_url), false),
        };
        let tunnel_ok = |r: &wire::GuestExecResult| {
            if direct_probe {
                String::from_utf8_lossy(&r.stdout).contains("403")
            } else {
                r.exit_code != 0
            }
        };
        let r = link
            .exec(
                &task,
                "eff-22",
                wire::GuestExec {
                    argv: tunnel_argv,
                    cwd: policy.workspace_root.clone(),
                    env: fx.exec_env.clone(),
                    timeout_ms: 20_000,
                    stdin: vec![],
                },
            )
            .await;
        push(
            "egress_denied_tunnel",
            r.as_ref().is_ok_and(tunnel_ok),
            format!("{r:?}"),
        );
        let r = link
            .exec(
                &task,
                "eff-23",
                wire::GuestExec {
                    argv: fetch(&eg.credentialed_url),
                    cwd: policy.workspace_root.clone(),
                    env: fx.exec_env.clone(),
                    timeout_ms: 20_000,
                    stdin: vec![],
                },
            )
            .await;
        let body = r
            .as_ref()
            .map(|r| String::from_utf8_lossy(&r.stdout).into_owned())
            .unwrap_or_default();
        push(
            "egress_credentialed",
            body.contains("\"authorized\":true") && !body.contains(&eg.secret),
            format!("{r:?}"),
        );
        // The guest never held the secret: its environment and its output
        // never carried it (the process saw the proxy, nothing more).
        let r = link
            .exec(
                &task,
                "eff-24",
                wire::GuestExec {
                    argv: fx.env_probe.clone(),
                    cwd: policy.workspace_root.clone(),
                    env: fx.exec_env.clone(),
                    timeout_ms: 20_000,
                    stdin: vec![],
                },
            )
            .await;
        let env_text = r
            .as_ref()
            .map(|r| String::from_utf8_lossy(&r.stdout).into_owned())
            .unwrap_or_default();
        push(
            "egress_secret_never_in_guest",
            !env_text.contains(&eg.secret) && env_text.contains("http_proxy=http://127.0.0.1:"),
            format!("{env_text:?}"),
        );
        let records = audit.records(&sandbox_id);
        let allowed_http = records.iter().any(|x| x.kind == "http" && x.allowed);
        let denied_http = records.iter().any(|x| x.kind == "http" && !x.allowed);
        let denied_tunnel = records.iter().any(|x| x.kind == "tunnel" && !x.allowed);
        let credentialed = records
            .iter()
            .any(|x| x.kind == "credentialed" && x.allowed && !x.capability.is_empty());
        push(
            "egress_audited",
            allowed_http && denied_http && denied_tunnel && credentialed,
            format!("{records:?}"),
        );
        // REQ-EV-0288: nothing the substrate provisions from carries the
        // secret. The compiled policy — the artifact the backend builds the
        // guest out of — names the virtual host and the handle and holds no
        // value, and every grant in it is short-lived: an expiry that is
        // set and still ahead. A guest image built from this cannot contain
        // a standing provider secret because the policy it is built from
        // never had one.
        let policy_text = serde_json::to_string(&policy).unwrap_or_default();
        let grants = &policy.spec.network.credentials;
        let short_lived = !grants.is_empty()
            && grants.iter().all(|g| {
                g.expires_at_ms > 0
                    && g.remaining_ms(
                        std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
                            .unwrap_or_default(),
                    ) > 0
            });
        push(
            "credential_handle_is_short_lived_and_absent_from_the_policy",
            short_lived
                && !policy_text.contains(&eg.secret)
                && grants.iter().all(|g| policy_text.contains(&g.virtual_host)),
            format!(
                "grants: {:?}",
                grants
                    .iter()
                    .map(|g| (g.handle.as_str(), g.expires_at_ms))
                    .collect::<Vec<_>>()
            ),
        );
        // REQ-EV-0288: the handle the guest reaches its credential through
        // is short-lived. Expire it and the very same request stops being
        // injected — the secret is dropped from the broker's memory, the
        // refusal names the expiry, and nothing of the secret is in what the
        // guest gets back. Renewing the handle makes the same request work
        // again: a task that outlives one lifetime is renewed rather than
        // given a standing secret.
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
            .unwrap_or_default();
        for (handle, secret) in &eg.secrets {
            broker.renew(handle, secret.clone(), now - 1);
        }
        let r = link
            .exec(
                &task,
                "eff-25",
                wire::GuestExec {
                    argv: fetch(&eg.credentialed_url),
                    cwd: policy.workspace_root.clone(),
                    env: fx.exec_env.clone(),
                    timeout_ms: 20_000,
                    stdin: vec![],
                },
            )
            .await;
        let expired_body = r
            .as_ref()
            .map(|r| String::from_utf8_lossy(&r.stdout).into_owned())
            .unwrap_or_default();
        let expired_record = audit
            .records(&sandbox_id)
            .into_iter()
            .any(|x| x.kind == "credentialed" && !x.allowed && x.detail.contains("expired"));
        push(
            "credential_handle_expires",
            !expired_body.contains("\"authorized\":true")
                && !expired_body.contains(&eg.secret)
                && expired_record,
            format!("{expired_body:?}"),
        );
        // And once expired it is really gone: renewing with a fresh
        // lifetime is what brings it back, not waiting.
        for (handle, secret) in &eg.secrets {
            broker.renew(handle, secret.clone(), now + 60_000);
        }
        let r = link
            .exec(
                &task,
                "eff-26",
                wire::GuestExec {
                    argv: fetch(&eg.credentialed_url),
                    cwd: policy.workspace_root.clone(),
                    env: fx.exec_env.clone(),
                    timeout_ms: 20_000,
                    stdin: vec![],
                },
            )
            .await;
        let renewed_body = r
            .as_ref()
            .map(|r| String::from_utf8_lossy(&r.stdout).into_owned())
            .unwrap_or_default();
        push(
            "credential_handle_renews",
            renewed_body.contains("\"authorized\":true") && !renewed_body.contains(&eg.secret),
            format!("{renewed_body:?}"),
        );
        if let Some(t) = broker_task {
            t.abort();
        }
    }
    // authentication: an unsigned call and a replayed call are refused
    let unsigned = wire::GuestCall {
        call_id: uuid::Uuid::now_v7().to_string(),
        task_id: task.clone(),
        effect_id: String::new(),
        capability: "health".into(),
        auth: vec![0; 32],
        body: Some(guest_call::Body::Health(wire::GuestHealth {})),
    };
    let r = link.send_raw(unsigned).await;
    let code = r.as_ref().ok().and_then(|r| match &r.body {
        Some(guest_reply::Body::Refusal(x)) => Some(x.code.clone()),
        _ => None,
    });
    push(
        "unauthenticated_call_refused",
        code.as_deref() == Some("UNAUTHENTICATED"),
        format!("{r:?}"),
    );
    let id = uuid::Uuid::now_v7().to_string();
    let signed = link.signed(
        &id,
        &task,
        "health",
        guest_call::Body::Health(wire::GuestHealth {}),
    );
    let first = link.send_raw(signed.clone()).await;
    let second = link.send_raw(signed).await;
    let first_ok = first
        .as_ref()
        .is_ok_and(|r| matches!(r.body, Some(guest_reply::Body::Health(_))));
    let second_code = second.as_ref().ok().and_then(|r| match &r.body {
        Some(guest_reply::Body::Refusal(x)) => Some(x.code.clone()),
        _ => None,
    });
    push(
        "replayed_call_refused",
        first_ok && second_code.as_deref() == Some("REPLAYED"),
        format!("{first:?} then {second:?}"),
    );
    // still alive after the refusals
    let h = link.health(&task).await;
    push("alive_after_refusals", h.is_ok(), format!("{h:?}"));
    // M8.8: the guest's browser, when the spec grants one — started on
    // the guest's loopback, its DevTools reached through a forwarded link
    // and nothing else, answering CDP; stopped with the sandbox.
    if fx.spec.browser {
        let started = link.browser_start(&task, 800, 600).await;
        push(
            "browser_started",
            started
                .as_ref()
                .is_ok_and(|b| b.port > 0 && b.ws_path.starts_with("/devtools/browser/")),
            format!("{started:?}"),
        );
        #[cfg(feature = "client")]
        if let Ok(b) = &started {
            let fresh = backend.reconnect(&sandbox_id).await?;
            let l2: GuestLink<Channel> = GuestLink::admit_image(
                fresh,
                &sandbox_id,
                &policy,
                Duration::from_secs(30),
                backend.image(),
            )
            .await?;
            let version = async {
                let (raw, port) = l2.browser_forward(&task).await?;
                let url = format!("ws://127.0.0.1:{port}{}", b.ws_path);
                let (mut ws, _) = tokio_tungstenite::client_async(url.as_str(), raw)
                    .await
                    .map_err(|e| crate::SandboxError::Guest(format!("DevTools handshake: {e}")))?;
                use futures_util::{SinkExt, StreamExt};
                ws.send(tokio_tungstenite::tungstenite::Message::Text(
                    r#"{"id":1,"method":"Browser.getVersion","params":{}}"#.into(),
                ))
                .await
                .map_err(|e| crate::SandboxError::Guest(e.to_string()))?;
                let reply = tokio::time::timeout(Duration::from_secs(20), ws.next())
                    .await
                    .map_err(|_| crate::SandboxError::Guest("no CDP answer".into()))?
                    .ok_or_else(|| crate::SandboxError::Guest("CDP closed".into()))?
                    .map_err(|e| crate::SandboxError::Guest(e.to_string()))?;
                Ok::<String, crate::SandboxError>(reply.to_text().unwrap_or_default().to_owned())
            }
            .await;
            push(
                "browser_answers_cdp",
                version
                    .as_ref()
                    .is_ok_and(|v| v.contains("\"product\"") && v.contains("Chrome")),
                format!("{version:?}"),
            );
        }
        let stopped = link.browser_stop(&task).await;
        push("browser_stopped", stopped.is_ok(), format!("{stopped:?}"));
    }
    // destroy, twice
    let d1 = backend.destroy(&sandbox_id).await;
    let d2 = backend.destroy(&sandbox_id).await;
    push(
        "destroyed",
        d1.is_ok() && d2.is_ok(),
        format!("{d1:?} {d2:?}"),
    );
    Ok(Report {
        backend: backend.kind(),
        isolated: backend.isolates(),
        guest_version,
        steps,
    })
}
