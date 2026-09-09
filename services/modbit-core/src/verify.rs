//! Verification Engine binding (M2.8, docs/64): commands run through the
//! process broker, raw output and reports land in the object store, and
//! every stage is recorded on the agent Run as events.

use std::path::Path;

use modbit_domain::event::Actor;
use modbit_domain::run::{CheckSummary, RunEvent};
use modbit_event_store::{EventStore, ObjectStore};
use modbit_protocol::v1::ExecRequest;
use modbit_terminal::{Event, ExecClient};
use modbit_tools::pipeline::ExecTarget;
use modbit_verification::engine::BoxFuture;
use modbit_verification::{
    ArtifactSink, ChangedFile, CommandRunner, Quarantine, RawRun, Stage, VerificationRun,
};

/// Runs verification commands through `modbit-execd`.
pub struct BrokerRunner {
    /// Broker target (`None` = no broker: every command is UNKNOWN).
    pub target: Option<ExecTarget>,
    /// Execution profile label carried on requests.
    pub execution_profile: String,
}

impl CommandRunner for BrokerRunner {
    fn run<'a>(
        &'a self,
        argv: &'a [String],
        cwd: &'a Path,
        env: &'a [(String, String)],
        timeout_ms: u64,
    ) -> BoxFuture<'a, RawRun> {
        Box::pin(async move {
            let Some(target) = &self.target else {
                return RawRun {
                    exit_code: None,
                    stderr: "no terminal broker attached".into(),
                    ..Default::default()
                };
            };
            let req = ExecRequest {
                request_id: format!("verify-{}", modbit_domain::EventId::new()),
                argv: argv.to_vec(),
                cwd: cwd.to_string_lossy().into_owned(),
                env: env.iter().cloned().collect(),
                inherit_env: true,
                timeout_ms,
                pty: false,
                stdin_mode: "closed".into(),
                output_budget_bytes: 1 << 20,
                execution_profile: self.execution_profile.clone(),
                capability_lease_id: None,
                terminal_session_id: None,
            };
            let mut client = match ExecClient::connect(&target.endpoint, &target.boot_secret).await
            {
                Ok(c) => c,
                Err(e) => {
                    return RawRun {
                        exit_code: None,
                        stderr: format!("broker unavailable: {e}"),
                        ..Default::default()
                    };
                }
            };
            if let Err(e) = client.exec(req).await {
                return RawRun {
                    exit_code: None,
                    stderr: format!("broker send: {e}"),
                    ..Default::default()
                };
            }
            let (mut out, mut err) = (Vec::new(), Vec::new());
            loop {
                match client.next().await {
                    Ok(Some(Event::Output(o))) => {
                        if o.stream == "stderr" {
                            err.extend(o.data);
                        } else {
                            out.extend(o.data);
                        }
                    }
                    Ok(Some(Event::Exited(x))) => {
                        return RawRun {
                            exit_code: x.exit_code,
                            stdout: String::from_utf8_lossy(&out).into_owned(),
                            stderr: String::from_utf8_lossy(&err).into_owned(),
                            reporter_file: None,
                            timed_out: x.timed_out,
                            cancelled: x.cancelled,
                        };
                    }
                    Ok(Some(_)) => {}
                    Ok(None) | Err(_) => {
                        return RawRun {
                            exit_code: None,
                            stdout: String::from_utf8_lossy(&out).into_owned(),
                            stderr: format!(
                                "{}\nbroker connection ended before exit",
                                String::from_utf8_lossy(&err)
                            ),
                            ..Default::default()
                        };
                    }
                }
            }
        })
    }
}

/// Object store as the artifact sink.
pub struct ObjectSinkAdapter(pub ObjectStore);

impl ArtifactSink for ObjectSinkAdapter {
    fn put(&self, bytes: &[u8]) -> String {
        self.0.put(bytes).unwrap_or_default()
    }
}

/// Check summaries for the log.
#[must_use]
pub fn summaries(run: &VerificationRun) -> Vec<CheckSummary> {
    run.checks()
        .into_iter()
        .map(|c| CheckSummary {
            check_id: c.check_id.clone(),
            kind: format!("{:?}", c.kind).to_lowercase(),
            status: format!("{:?}", c.status).to_uppercase(),
            duration_ms: c.duration_ms,
            error_class: c.error_class.clone(),
            message_fingerprint: c.message_fingerprint.clone(),
            path: c.location.path.clone(),
        })
        .collect()
}

/// Events recording a stage (and its quarantines) on the agent Run.
#[must_use]
pub fn stage_events(
    run: &VerificationRun,
    quarantines: &[Quarantine],
    actor: &Actor,
) -> Vec<modbit_event_store::NewEvent> {
    let typed = |t: &str, e: &RunEvent| {
        let mut ev = modbit_event_store::NewEvent::new(
            t,
            serde_json::to_value(e).expect("serializable"),
            actor.clone(),
        );
        ev.occurred_at = Some(modbit_domain::Timestamp::now());
        ev
    };
    let mut out = Vec::new();
    let status = format!("{:?}", run.status).to_uppercase();
    match run.stage {
        Stage::Baseline => out.push(typed(
            "VerificationBaselineRecorded",
            &RunEvent::VerificationBaselineRecorded {
                verification_run_id: run.verification_run_id.clone(),
                plan_ref: run.plan_ref.clone(),
                candidate_revision: run.candidate_revision.clone(),
                environment_digest: run.environment_digest.clone(),
                status,
                report_refs: run.report_refs.clone(),
                checks: summaries(run),
            },
        )),
        stage => out.push(typed(
            "VerificationRunRecorded",
            &RunEvent::VerificationRunRecorded {
                verification_run_id: run.verification_run_id.clone(),
                stage: format!("{stage:?}").to_uppercase(),
                plan_ref: run.plan_ref.clone(),
                candidate_revision: run.candidate_revision.clone(),
                environment_digest: run.environment_digest.clone(),
                status,
                report_refs: run.report_refs.clone(),
                checks: summaries(run),
            },
        )),
    }
    for q in quarantines {
        out.push(typed(
            "FlakyCheckQuarantined",
            &RunEvent::FlakyCheckQuarantined {
                check_id: q.check_id.clone(),
                first_run_id: q.first_run_id.clone(),
                rerun_id: q.rerun_id.clone(),
                candidate_revision: q.candidate_revision.clone(),
            },
        ));
    }
    out
}

/// The working tree's changes against HEAD as `ChangedFile`s (whole-diff invariants).
#[must_use]
pub fn changed_files(root: &Path) -> Vec<ChangedFile> {
    let Ok(repo) = modbit_git::Repo::open(root) else {
        return vec![];
    };
    let Ok(entries) = repo.status() else {
        return vec![];
    };
    entries
        .into_iter()
        .filter(|e| !e.path.starts_with(".modbit"))
        .map(|e| {
            let old = repo
                .show("HEAD", &e.path)
                .ok()
                .map(|b| String::from_utf8_lossy(&b).into_owned());
            let new = std::fs::read(root.join(&e.path))
                .ok()
                .map(|b| String::from_utf8_lossy(&b).into_owned());
            ChangedFile {
                path: e.path,
                old,
                new,
            }
        })
        .collect()
}

/// The model's bounded view of a run: failing checks first, then references (REQ-EV-0107).
#[must_use]
pub fn observation(run: &VerificationRun, store: &EventStore) -> String {
    let _ = store;
    let mut text = format!(
        "verification_run: {}\nstage: {:?}\nstatus: {:?}\ncandidate_revision: {}\n",
        run.verification_run_id, run.stage, run.status, run.candidate_revision
    );
    let failing = run
        .checks()
        .into_iter()
        .filter(|c| {
            matches!(
                c.status,
                modbit_verification::CheckStatus::Fail
                    | modbit_verification::CheckStatus::Error
                    | modbit_verification::CheckStatus::Timeout
                    | modbit_verification::CheckStatus::Unknown
            )
        })
        .collect::<Vec<_>>();
    text.push_str(&format!(
        "checks: {} failing / {} total\n",
        failing.len(),
        run.checks().len()
    ));
    for c in &failing {
        text.push_str(&format!(
            "- {} [{:?}] {}{}{}\n",
            c.check_id,
            c.status,
            c.error_class.as_deref().unwrap_or("-"),
            c.location
                .path
                .as_ref()
                .map(|p| format!(
                    " at {p}{}",
                    c.location.line.map(|l| format!(":{l}")).unwrap_or_default()
                ))
                .unwrap_or_default(),
            c.message_excerpt
                .as_ref()
                .map(|m| format!(
                    "\n    {}",
                    m.lines().take(6).collect::<Vec<_>>().join("\n    ")
                ))
                .unwrap_or_default()
        ));
    }
    let flaky: Vec<&str> = run
        .checks()
        .into_iter()
        .filter(|c| c.status == modbit_verification::CheckStatus::Flaky)
        .map(|c| c.check_id.as_str())
        .collect();
    if !flaky.is_empty() {
        text.push_str(&format!(
            "quarantined (FLAKY, excluded from failure signatures): {}\n",
            flaky.join(", ")
        ));
    }
    for r in &run.reports {
        text.push_str(&format!(
            "report {:?} {} parser={} confidence={:?} raw_output_ref={}\n",
            r.stage, r.report_id, r.parser.adapter, r.parser.confidence, r.raw_output_ref
        ));
    }
    text.push_str("(page any raw log with artifact.range on its raw_output_ref)\n");
    text
}
