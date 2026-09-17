//! The backend contract suite (QUAL-EV-0291: the same suite runs against the
//! reference backend and the MicroVM substrate; QUAL-EV-0285, QUAL-EV-0287,
//! QUAL-EV-0289, QUAL-EV-0290 draw on its steps). Every step is a real call
//! through a real guest; a backend passes when every step holds. Network
//! isolation is asserted only of a backend that claims to isolate.

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
    let provisioned = backend.provision(&sandbox_id, &policy).await?;
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
