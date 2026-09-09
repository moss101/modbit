//! Trusted Code Review Surface (M2.9; docs/20 "Trusted Code Surface",
//! "Stale reference handling"; REQ-EV-0036 per-hunk review; docs/83 "Agent
//! coding loop"). The Core serves immutable, revision-bound review bundles and
//! code view-models; the user's per-hunk decision is applied through the
//! Workspace File Service as a revision-bound change with provenance
//! `user_review` and, on accept, committed to the worktree branch.

use std::sync::Arc;

use modbit_domain::event::{Actor, AggregateType};
use modbit_domain::task::{InputMode, Task, TaskEvent, TaskState, WaitReason};
use modbit_domain::{SessionId, TaskId, Timestamp};
use modbit_event_store::{AppendRequest, NewEvent};
use modbit_git::{FileDiff, Repo, apply_selected, parse_unified};
use modbit_protocol::v1 as wire;
use modbit_workspace::WritePrecondition;
use sha2::{Digest, Sha256};

use crate::server::Core;

/// Largest inline code view (docs/30 bounded payloads).
const INLINE_TEXT_LIMIT: usize = 256 * 1024;

fn typed<E: serde::Serialize>(event_type: &str, e: &E, actor: Actor) -> NewEvent {
    let mut ev = NewEvent::new(
        event_type,
        serde_json::to_value(e).expect("serializable"),
        actor,
    );
    ev.occurred_at = Some(Timestamp::now());
    ev
}

fn language_for(path: &str) -> &'static str {
    match path.rsplit('.').next().unwrap_or_default() {
        "rs" => "rust",
        "ts" | "tsx" => "typescript",
        "js" | "mjs" | "cjs" | "jsx" => "javascript",
        "py" => "python",
        "go" => "go",
        "md" => "markdown",
        "json" => "json",
        "toml" => "toml",
        "yml" | "yaml" => "yaml",
        "sh" => "shell",
        _ => "text",
    }
}

fn sha(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

/// Everything the review needs about the candidate diff.
struct Candidate {
    root: String,
    repo: Repo,
    base_commit: String,
    files: Vec<FileDiff>,
}

fn candidate(task: &Task) -> Result<Candidate, (String, String)> {
    let root = task.workspace_root.clone().ok_or_else(|| {
        (
            "NO_WORKSPACE".to_owned(),
            "task has no workspace root".to_owned(),
        )
    })?;
    let repo =
        Repo::open(std::path::Path::new(&root)).map_err(|e| ("GIT".to_owned(), e.to_string()))?;
    let base_commit = repo.head().map_err(|e| ("GIT".to_owned(), e.to_string()))?;
    // Untracked files are part of the candidate: stage them into the index view
    // without committing so `git diff HEAD` shows them (`-N` intent-to-add).
    let status = repo
        .status()
        .map_err(|e| ("GIT".to_owned(), e.to_string()))?;
    let untracked: Vec<&str> = status
        .iter()
        .filter(|e| e.code.starts_with('?') && !e.path.starts_with(".modbit"))
        .map(|e| e.path.as_str())
        .collect();
    if !untracked.is_empty() {
        let mut args = vec!["add", "-N", "--"];
        args.extend(untracked.iter().copied());
        let _ = std::process::Command::new("git")
            .arg("-C")
            .arg(&root)
            .args(&args)
            .output();
    }
    let diff = repo
        .diff_worktree()
        .map_err(|e| ("GIT".to_owned(), e.to_string()))?;
    let files = parse_unified(&diff.unified)
        .into_iter()
        .filter(|f| !f.path.starts_with(".modbit"))
        .collect();
    Ok(Candidate {
        root,
        repo,
        base_commit,
        files,
    })
}

async fn load_task(core: &Core, id: &wire::Id) -> Result<Task, (String, String)> {
    let bytes: [u8; 16] = id.value.as_slice().try_into().map_err(|_| {
        (
            "BAD_PAYLOAD".to_owned(),
            "task_id must be 16 bytes".to_owned(),
        )
    })?;
    let task_id = TaskId::from_bytes(bytes);
    core.store
        .lock()
        .await
        .task(&task_id)
        .map_err(|e| ("STORE".to_owned(), e.to_string()))?
        .ok_or_else(|| ("UNKNOWN_TASK".to_owned(), task_id.to_string()))
}

/// `GetReviewBundle`.
pub async fn bundle(
    core: &Core,
    p: &wire::GetReviewBundle,
) -> Result<wire::ReviewBundle, (String, String)> {
    let Some(id) = &p.task_id else {
        return Err(("BAD_PAYLOAD".into(), "task_id required".into()));
    };
    let task = load_task(core, id).await?;
    let cand = candidate(&task)?;
    let store = core.store.lock().await;
    let mut files = Vec::new();
    for f in &cand.files {
        let old = if f.status == 'A' {
            None
        } else {
            cand.repo
                .show(&cand.base_commit, f.old_path.as_deref().unwrap_or(&f.path))
                .ok()
        };
        let new = if f.status == 'D' {
            None
        } else {
            std::fs::read(std::path::Path::new(&cand.root).join(&f.path)).ok()
        };
        let old_ref = old
            .as_deref()
            .map(|b| store.objects().put(b).unwrap_or_default())
            .unwrap_or_default();
        let new_ref = new
            .as_deref()
            .map(|b| store.objects().put(b).unwrap_or_default())
            .unwrap_or_default();
        files.push(wire::ReviewFile {
            path: f.path.clone(),
            status: f.status.to_string(),
            binary: f.binary,
            old_content_ref: old_ref,
            new_content_ref: new_ref,
            file_revision: new.as_deref().map(sha).unwrap_or_default(),
            hunks: f
                .hunks
                .iter()
                .map(|h| wire::HunkView {
                    index: h.index,
                    header: h.header.clone(),
                    old_start: h.old_start,
                    old_lines: h.old_lines,
                    new_start: h.new_start,
                    new_lines: h.new_lines,
                    lines: h.lines.clone(),
                })
                .collect(),
        });
    }
    // Competence and verification evidence from the log (task-scoped events).
    let events = store
        .read_session(&task.session_id, 0, usize::MAX)
        .unwrap_or_default();
    let mut plan_json = String::new();
    let mut self_review_json = String::new();
    let mut attributions = Vec::new();
    let mut quarantined = Vec::new();
    let mut invariant_findings = Vec::new();
    let mut evidence_links = Vec::new();
    let mut receipts = 0u32;
    let mut run_ids = Vec::new();
    for ev in events
        .iter()
        .filter(|e| e.envelope.task_id == Some(task.task_id))
    {
        let p = store.payload(&ev.envelope).unwrap_or_default();
        match ev.envelope.event_type.as_str() {
            "PlanRecorded" | "PlanRevised" => {
                if let Some(r) = p["plan_ref"].as_str()
                    && let Ok(b) = store.objects().get(r)
                {
                    plan_json = String::from_utf8_lossy(&b).into_owned();
                    evidence_links.push(format!("plan:{r}"));
                }
            }
            "SelfReviewRecorded" => {
                if let Some(r) = p["review_ref"].as_str()
                    && let Ok(b) = store.objects().get(r)
                {
                    self_review_json = String::from_utf8_lossy(&b).into_owned();
                    evidence_links.push(format!("self_review:{r}"));
                }
            }
            "RegressionAttributed" => attributions.push(format!(
                "{}={}",
                p["check_id"].as_str().unwrap_or_default(),
                p["attribution"].as_str().unwrap_or_default()
            )),
            "FlakyCheckQuarantined" => {
                quarantined.push(p["check_id"].as_str().unwrap_or_default().to_owned())
            }
            "DiffInvariantViolated" => invariant_findings.push(format!(
                "{} {} {}: {}",
                p["invariant"].as_str().unwrap_or_default(),
                p["class"].as_str().unwrap_or_default(),
                p["paths"][0].as_str().unwrap_or_default(),
                p["evidence"].as_str().unwrap_or_default()
            )),
            "EffectReceiptAppended" => {
                receipts += 1;
                if let Some(h) = p["receipt"]["receipt_hash"].as_str() {
                    evidence_links.push(format!("receipt:{h}"));
                }
            }
            "RunCreated" => {
                run_ids.push(modbit_domain::RunId::from_bytes(ev.envelope.aggregate_id))
            }
            "ToolCallSucceeded" | "ToolCallFailed" => {
                if let Some(r) = p["result_ref"].as_str() {
                    evidence_links.push(format!("tool_result:{r}"));
                }
            }
            _ => {}
        }
    }
    let mut verification_runs = Vec::new();
    for run_id in &run_ids {
        for v in store.verification_runs(run_id).unwrap_or_default() {
            for r in &v.report_refs {
                evidence_links.push(format!("report:{r}"));
            }
            verification_runs.push(wire::VerificationRunView {
                verification_run_id: v.verification_run_id.clone(),
                stage: v.stage.clone(),
                status: v.status.clone(),
                candidate_revision: v.candidate_revision.clone(),
                report_refs: v.report_refs.clone(),
                check_ids: v.checks.iter().map(|(c, _)| c.clone()).collect(),
                check_statuses: v.checks.iter().map(|(_, s)| s.clone()).collect(),
            });
        }
    }
    drop(store);
    let workspace_revision = match core.tools.workspace(&cand.root).await {
        Ok((ws, _)) => ws.lock().await.revision().number,
        Err(_) => 0,
    };
    Ok(wire::ReviewBundle {
        task_id: Some(id.clone()),
        task_state: format!("{:?}", task.state),
        workspace_revision,
        base_commit: cand.base_commit,
        workspace_root: cand.root,
        files,
        plan_json,
        self_review_json,
        verification_runs,
        attributions,
        quarantined,
        invariant_findings,
        receipts,
        evidence_links,
    })
}

/// `GetCodeView`.
pub async fn code_view(
    core: &Core,
    p: &wire::GetCodeView,
) -> Result<wire::CodeViewModel, (String, String)> {
    let Some(id) = &p.task_id else {
        return Err(("BAD_PAYLOAD".into(), "task_id required".into()));
    };
    let task = load_task(core, id).await?;
    let root = task.workspace_root.clone().ok_or_else(|| {
        (
            "NO_WORKSPACE".to_owned(),
            "task has no workspace root".to_owned(),
        )
    })?;
    let (ws, _) = core
        .tools
        .workspace(&root)
        .await
        .map_err(|e| ("WORKSPACE".to_owned(), e.to_string()))?;
    let read = ws
        .lock()
        .await
        .read(&p.path)
        .map_err(|e| ("PATH".to_owned(), e.to_string()))?;
    let content_ref = core
        .store
        .lock()
        .await
        .objects()
        .put(&read.bytes)
        .map_err(|e| ("STORE".to_owned(), e.to_string()))?;
    let stale =
        !p.expected_file_revision.is_empty() && p.expected_file_revision != read.content_hash;
    let mut changed_ranges = Vec::new();
    if let Ok(cand) = candidate(&task)
        && let Some(f) = cand.files.iter().find(|f| f.path == p.path)
    {
        for h in &f.hunks {
            let mut line = h.new_start;
            let mut start: Option<u32> = None;
            for l in &h.lines {
                match l.as_bytes().first() {
                    Some(b'+') => {
                        start.get_or_insert(line);
                        line += 1;
                    }
                    Some(b' ') => {
                        if let Some(s) = start.take() {
                            changed_ranges.push(s);
                            changed_ranges.push(line - 1);
                        }
                        line += 1;
                    }
                    _ => {}
                }
            }
            if let Some(s) = start.take() {
                changed_ranges.push(s);
                changed_ranges.push(line - 1);
            }
        }
    }
    let text = if read.bytes.len() <= INLINE_TEXT_LIMIT {
        String::from_utf8_lossy(&read.bytes).into_owned()
    } else {
        String::new()
    };
    Ok(wire::CodeViewModel {
        workspace_revision: read.workspace_revision,
        file_revision: read.content_hash,
        path: p.path.clone(),
        content_ref,
        syntax_language: language_for(&p.path).into(),
        changed_ranges,
        evidence_links: vec![format!("workspace_revision:{}", read.workspace_revision)],
        stale,
        text,
    })
}

/// `DecideReview` (the caller has checked the session lease).
pub async fn decide(
    core: &Core,
    p: &wire::DecideReview,
    actor: Actor,
) -> Result<wire::ReviewDecided, (String, String)> {
    let Some(id) = &p.task_id else {
        return Err(("BAD_PAYLOAD".into(), "task_id required".into()));
    };
    let task = load_task(core, id).await?;
    if task.state != TaskState::ReadyForReview {
        return Err((
            "NOT_REVIEWABLE".into(),
            format!(
                "task is {:?}; only ReadyForReview tasks take a review decision",
                task.state
            ),
        ));
    }
    let cand = candidate(&task)?;
    let (ws, _) = core
        .tools
        .workspace(&cand.root)
        .await
        .map_err(|e| ("WORKSPACE".to_owned(), e.to_string()))?;
    let current_revision = ws.lock().await.revision().number;
    if p.expected_workspace_revision != 0 && p.expected_workspace_revision != current_revision {
        return Err((
            "STALE_REVIEW".into(),
            format!(
                "review was built at workspace revision {}, the workspace is at {current_revision}; reload the review",
                p.expected_workspace_revision
            ),
        ));
    }
    let decision = match p.decision.as_str() {
        "ACCEPT" => "ACCEPT",
        "RETURN" => "RETURN",
        other => return Err(("BAD_PAYLOAD".into(), format!("unknown decision `{other}`"))),
    };
    // Validate hunk references against the candidate.
    for r in &p.rejected {
        let Some(f) = cand.files.iter().find(|f| f.path == r.path) else {
            return Err((
                "UNKNOWN_HUNK".into(),
                format!("{} is not in the candidate diff", r.path),
            ));
        };
        if r.index as usize >= f.hunks.len() {
            return Err((
                "UNKNOWN_HUNK".into(),
                format!("{}#{} does not exist", r.path, r.index),
            ));
        }
    }
    let mut accepted = Vec::new();
    let mut rejected = Vec::new();
    let mut reverted = Vec::new();
    let mut touched: Vec<String> = Vec::new();
    let mut commit = None;
    if decision == "ACCEPT" {
        for f in &cand.files {
            let rej: Vec<u32> = p
                .rejected
                .iter()
                .filter(|r| r.path == f.path)
                .map(|r| r.index)
                .collect();
            let acc: Vec<u32> = f
                .hunks
                .iter()
                .map(|h| h.index)
                .filter(|i| !rej.contains(i))
                .collect();
            for i in &acc {
                accepted.push(format!("{}#{i}", f.path));
            }
            for i in &rej {
                rejected.push(format!("{}#{i}", f.path));
            }
            touched.push(f.path.clone());
            if rej.is_empty() || f.binary {
                continue;
            }
            // Rebuild the file from the base text plus the accepted hunks only,
            // through the Workspace File Service (revision-bound, provenance user_review).
            let base = if f.status == 'A' {
                String::new()
            } else {
                String::from_utf8_lossy(
                    &cand
                        .repo
                        .show(&cand.base_commit, f.old_path.as_deref().unwrap_or(&f.path))
                        .unwrap_or_default(),
                )
                .into_owned()
            };
            let rebuilt = apply_selected(&base, &f.hunks, &acc);
            let mut svc = ws.lock().await;
            let current = svc.read(&f.path).ok();
            let pre = WritePrecondition {
                expected_content_hash: current.as_ref().map(|c| c.content_hash.clone()),
                expected_workspace_revision: None,
                expect_absent: false,
            };
            let result = if f.status == 'A' && acc.is_empty() {
                svc.delete(&f.path, pre).map(|_| ())
            } else if current.is_none() {
                svc.create(&f.path, rebuilt.as_bytes(), WritePrecondition::default())
                    .map(|_| ())
            } else {
                svc.atomic_replace(&f.path, rebuilt.as_bytes(), pre)
                    .map(|_| ())
            };
            result.map_err(|e| ("WORKSPACE".to_owned(), format!("{}: {e}", f.path)))?;
            for i in &rej {
                reverted.push(format!("{}#{i}", f.path));
            }
        }
        // Git history reflects the accepted change (docs/51 E2E-001).
        let paths: Vec<&str> = touched.iter().map(String::as_str).collect();
        let message = format!(
            "{}\n\nModbit task {} reviewed: {} hunk(s) accepted, {} rejected\nCandidate workspace revision {}",
            if p.note.trim().is_empty() {
                task.goal_text.clone()
            } else {
                p.note.trim().to_owned()
            },
            task.task_id,
            accepted.len(),
            rejected.len(),
            current_revision
        );
        if !paths.is_empty() {
            let sha = cand
                .repo
                .commit(&message, &paths)
                .map_err(|e| ("GIT".to_owned(), e.to_string()))?;
            commit = Some(sha);
        }
    } else {
        for f in &cand.files {
            for h in &f.hunks {
                accepted.push(format!("{}#{}", f.path, h.index));
            }
        }
    }
    let workspace_revision = ws.lock().await.revision().number;
    let mut events = vec![typed(
        "ReviewDecisionRecorded",
        &TaskEvent::ReviewDecisionRecorded {
            decision: decision.into(),
            candidate_revision: current_revision,
            accepted: accepted.clone(),
            rejected: rejected.clone(),
            commit: commit.clone(),
            note: p.note.clone(),
            provenance: "user_review".into(),
        },
        actor.clone(),
    )];
    if decision == "ACCEPT" {
        events.push(typed(
            "TaskCompleted",
            &TaskEvent::TaskCompleted,
            actor.clone(),
        ));
    } else {
        events.push(typed(
            "TaskReturnedToWork",
            &TaskEvent::TaskReturnedToWork,
            actor.clone(),
        ));
        events.push(typed(
            "TaskWaiting",
            &TaskEvent::TaskWaiting {
                reason: WaitReason::UserInput,
            },
            actor.clone(),
        ));
        if !p.note.trim().is_empty() {
            events.push(typed(
                "TaskInputQueued",
                &TaskEvent::TaskInputQueued {
                    input_id: format!("review-{}", Timestamp::now().0),
                    mode: InputMode::FollowUp,
                    text: format!("Review feedback: {}", p.note.trim()),
                },
                actor.clone(),
            ));
        }
    }
    let offset = {
        let mut store = core.store.lock().await;
        let stored = store
            .append(AppendRequest {
                tenant_id: core.tenant_id,
                session_id: task.session_id,
                task_id: Some(task.task_id),
                run_id: None,
                turn_id: None,
                step_id: None,
                aggregate_type: AggregateType::Task,
                aggregate_id: *task.task_id.as_bytes(),
                expected_sequence: None,
                events,
            })
            .map_err(|e| ("STORE".to_owned(), e.to_string()))?;
        stored.last().map(|e| e.offset).unwrap_or(0)
    };
    core.last_offset.send_replace(offset);
    let task_state = if decision == "ACCEPT" {
        "Completed"
    } else {
        "Waiting"
    };
    let _ = Arc::new(());
    let _: Option<SessionId> = None;
    Ok(wire::ReviewDecided {
        task_state: task_state.into(),
        commit: commit.unwrap_or_default(),
        reverted,
        workspace_revision,
        offset,
    })
}
