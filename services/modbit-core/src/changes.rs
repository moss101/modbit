//! The Changes app's actions (REQ-PX-048; docs/62 PX-048, docs/65 AFW-G05 and
//! AFW-G06): the state the app shows beside the review bundle, per-file and
//! per-hunk revert, stage and unstage, and Commit and push.
//!
//! * **Revert** is the canonical ChangeTransaction on the Workspace File
//!   Service, like the inline patch of `user_patch.rs`: the file revision the
//!   client saw is a precondition (`STALE_REVISION`), the path is resolved
//!   through symlinks and checked against the protected patterns
//!   (`PROTECTED_PATH`), a path with staged changes or an unmerged path is a
//!   `DIRTY_CONFLICT`, and a checkpoint of the task is taken first so the
//!   revert is undone like any other restore. The result lands as
//!   `FileChanged` events (provenance `user_revert`) and one `ChangesReverted`
//!   record in one transaction under the command record.
//! * **Stage** moves the git index only; the working tree is untouched.
//! * **Commit and push** is ONE protected effect: the governed tool
//!   `git.changes.commit_push` through the tool pipeline, bound to the exact
//!   intent (message, files, branch, remote, the HEAD it commits onto, the
//!   staged digest). Nothing has run until the person's approval stands; the
//!   authorization receipt is written before the effect, the result receipt
//!   after it, and a Core that stopped between them leaves the effect
//!   `AUTHORIZED_IN_DOUBT`, never assumed done. The commit and the push go
//!   through the hardened Git runner (`modbit_git`), and a token for an https
//!   forge remote comes from the credential broker into the push process's
//!   environment only.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use modbit_domain::event::{Actor, AggregateType};
use modbit_domain::task::Task;
use modbit_domain::toolcall::ToolCallState;
use modbit_domain::{TaskId, ToolCallId};
use modbit_event_store::{AppendRequest, CommandOutcome, CommandRecord, NewEvent};
use modbit_git::{Repo, apply_selected};
use modbit_protocol::v1 as wire;
use modbit_workspace::{ChangeOp, ChangeOpKind, WritePrecondition};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::apply_back::{Called, call_tool, digest_of};
use crate::git_state::CoreGitState;
use crate::runtime::{Lineage, typed};
use crate::server::{Core, id16};
use crate::tools::{file_changed_events, workspace_aggregate_id};

type Refusal = (String, String);

/// The governed tool a Commit and push runs as.
pub(crate) const TOOL: &str = "git.changes.commit_push";

const MAX_MESSAGE: usize = 4000;

fn refuse(code: &str, msg: impl Into<String>) -> Refusal {
    (code.to_owned(), msg.into())
}

/// The aggregate this app's records live on: one per task, beside (never on)
/// the workspace's own `FileChanged` log.
pub(crate) fn records_aggregate(task: TaskId) -> [u8; 16] {
    let h = Sha256::digest(format!("changes-app:{task}").as_bytes());
    let mut b = [0u8; 16];
    b.copy_from_slice(&h[..16]);
    b
}

pub(crate) fn record(event_type: &str, payload: Value, actor: &Actor) -> NewEvent {
    typed(event_type, &payload, actor.clone())
}

pub(crate) fn append_request(core: &Core, task: &Task, events: Vec<NewEvent>) -> AppendRequest {
    AppendRequest {
        tenant_id: core.tenant_id,
        session_id: task.session_id,
        task_id: Some(task.task_id),
        run_id: None,
        turn_id: None,
        step_id: None,
        aggregate_type: AggregateType::Workspace,
        aggregate_id: records_aggregate(task.task_id),
        expected_sequence: None,
        events,
    }
}

pub(crate) async fn load_task_pub(core: &Core, id: Option<&wire::Id>) -> Result<Task, Refusal> {
    load_task(core, id).await
}

async fn load_task(core: &Core, id: Option<&wire::Id>) -> Result<Task, Refusal> {
    let Some(bytes) = id.and_then(id16) else {
        return Err(refuse("BAD_PAYLOAD", "task_id required"));
    };
    let task_id = TaskId::from_bytes(bytes);
    core.store
        .lock()
        .await
        .task(&task_id)
        .map_err(|e| refuse("STORE", e.to_string()))?
        .ok_or_else(|| refuse("UNKNOWN_TASK", task_id.to_string()))
}

fn code_of(e: &modbit_workspace::Error) -> &'static str {
    use modbit_workspace::Error as E;
    match e {
        E::StepFailed { cause, .. } => code_of(cause),
        E::Protected { .. } => "PROTECTED_PATH",
        E::OutsideRoot { .. } => "PATH_OUTSIDE_ROOT",
        E::Precondition { .. } => "STALE_REVISION",
        E::AmbiguousTarget { .. } | E::NoMatch { .. } => "NO_UNIQUE_MATCH",
        E::Invalid { .. } | E::EditOutOfBounds { .. } => "BAD_PAYLOAD",
        E::Io { source, .. } if source.kind() == std::io::ErrorKind::NotFound => "NO_SUCH_FILE",
        E::Io { .. } => "WORKSPACE",
    }
}

fn git_err(e: impl std::fmt::Display) -> Refusal {
    refuse("GIT", e.to_string())
}

/// The loop owns the tree while it runs; a person acts between runs.
async fn require_stopped(core: &Core, task: &Task, what: &str) -> Result<(), Refusal> {
    if core.runtime.is_running(&task.task_id).await {
        return Err(refuse(
            "TASK_RUNNING",
            format!("the task's loop is running; {what} waits for it to stop"),
        ));
    }
    Ok(())
}

fn root_of(task: &Task) -> Result<String, Refusal> {
    task.workspace_root
        .clone()
        .ok_or_else(|| refuse("NO_WORKSPACE", "task has no workspace root"))
}

/// A status code column as the wire spells it: `A`, `M`, `D` or nothing.
fn column(c: char) -> &'static str {
    match c {
        'A' => "A",
        'M' | 'T' => "M",
        'D' => "D",
        _ => "",
    }
}

fn is_unmerged(code: &str) -> bool {
    matches!(code, "UU" | "AA" | "DD" | "AU" | "UA" | "DU" | "UD")
}

/// The remote a push goes to: the one the person named, else `origin`, else
/// the only remote there is.
fn pick_remote(repo: &Repo, requested: &str) -> Result<String, Refusal> {
    let names = repo.remote_names().map_err(git_err)?;
    if names.is_empty() {
        return Err(refuse(
            "NO_REMOTE",
            "the repository has no remote; add one before pushing",
        ));
    }
    if !requested.is_empty() {
        return if names.iter().any(|n| n == requested) {
            Ok(requested.to_owned())
        } else {
            Err(refuse(
                "NO_REMOTE",
                format!(
                    "`{}` is not a remote of this repository",
                    modbit_git::redact_url_credentials(requested)
                ),
            ))
        };
    }
    if names.iter().any(|n| n == "origin") {
        return Ok("origin".to_owned());
    }
    if names.len() == 1 {
        return Ok(names[0].clone());
    }
    Err(refuse(
        "NO_REMOTE",
        "the repository has several remotes and none is named origin; name one",
    ))
}

// -------------------------------------------------------------- the state

/// `GetChangesState`.
pub(crate) async fn state(
    core: &Core,
    p: &wire::GetChangesState,
) -> Result<wire::ChangesState, Refusal> {
    let task = load_task(core, p.task_id.as_ref()).await?;
    let root = root_of(&task)?;
    let residue = crate::review::residue_of(core, &task).await;
    let task_running = core.runtime.is_running(&task.task_id).await;
    let (ws, _) = core
        .tools
        .workspace(&root)
        .await
        .map_err(|e| refuse("WORKSPACE", e.to_string()))?;
    let task2 = task.clone();
    let root2 = root.clone();
    let (cand, status, staged, branch, head, remote, remote_url, unpushed, merging, identity) =
        tokio::task::spawn_blocking(move || {
            let cand = crate::review::candidate(&task2, &residue)?;
            let repo = Repo::open(Path::new(&root2)).map_err(git_err)?;
            let status = repo.status().map_err(git_err)?;
            let staged = repo.staged().map_err(git_err)?;
            let branch = repo.current_branch().ok().flatten().unwrap_or_default();
            let head = repo.head().unwrap_or_default();
            let remote = pick_remote(&repo, "").unwrap_or_default();
            let remote_url = if remote.is_empty() {
                String::new()
            } else {
                modbit_git::redact_url_credentials(&repo.remote_url(&remote).unwrap_or_default())
            };
            let unpushed = if remote.is_empty() {
                0
            } else {
                repo.unpushed_count(&remote).unwrap_or(0)
            };
            let identity = {
                let n = modbit_git::output(Path::new(&root2), &["config", "--get", "user.name"]);
                let e = modbit_git::output(Path::new(&root2), &["config", "--get", "user.email"]);
                match (n, e) {
                    (Ok(n), Ok(e)) if n.status.success() && e.status.success() => format!(
                        "{} <{}>",
                        String::from_utf8_lossy(&n.stdout).trim(),
                        String::from_utf8_lossy(&e.stdout).trim()
                    ),
                    _ => String::new(),
                }
            };
            Ok::<_, Refusal>((
                cand,
                status,
                staged,
                branch,
                head,
                remote,
                remote_url,
                unpushed,
                repo.merge_in_progress(),
                identity,
            ))
        })
        .await
        .map_err(|e| refuse("STATE_FAILED", e.to_string()))??;
    let codes: HashMap<&str, &str> = status
        .iter()
        .map(|e| (e.path.as_str(), e.code.as_str()))
        .collect();
    let mut paths = Vec::new();
    {
        let svc = ws.lock().await;
        for f in &cand.files {
            let code = codes.get(f.path.as_str()).copied().unwrap_or("..");
            let mut chars = code.chars();
            let (x, y) = (chars.next().unwrap_or('.'), chars.next().unwrap_or('.'));
            let protected_path = matches!(
                svc.resolve(&f.path),
                Err(modbit_workspace::Error::Protected { .. })
            );
            let file_revision = svc
                .read(&f.path)
                .map(|r| r.content_hash)
                .unwrap_or_default();
            paths.push(wire::ChangedPathState {
                path: f.path.clone(),
                status: f.status.to_string(),
                staged: column(x).to_owned(),
                unstaged: column(y).to_owned(),
                file_revision,
                unmerged: is_unmerged(code),
                protected_path,
            });
        }
    }
    let workspace_revision = ws.lock().await.revision().number;
    let effects = effects_of(core, &task).await;
    Ok(wire::ChangesState {
        workspace_revision,
        branch,
        head,
        remote,
        remote_url,
        paths,
        unpushed,
        task_running,
        merge_in_progress: merging,
        staged_digest: staged.digest,
        effects,
        identity,
    })
}

/// The task's Commit and push attempts, joined with their approvals and
/// receipts.
async fn effects_of(core: &Core, task: &Task) -> Vec<wire::ChangeEffectView> {
    let store = core.store.lock().await;
    let calls: Vec<_> = store
        .tool_calls_for_task(&task.task_id)
        .unwrap_or_default()
        .into_iter()
        .filter(|c| c.tool_name == TOOL)
        .collect();
    if calls.is_empty() {
        return Vec::new();
    }
    let receipts = store.receipts(Some(&task.task_id)).unwrap_or_default();
    let events = store
        .read_aggregate(&records_aggregate(task.task_id), 0, usize::MAX)
        .unwrap_or_default();
    let mut intents: HashMap<String, Value> = HashMap::new();
    let mut ended: HashMap<String, Value> = HashMap::new();
    for e in &events {
        let Ok(p) = store.payload(&e.envelope) else {
            continue;
        };
        let key = p["intent_hash"].as_str().unwrap_or_default().to_owned();
        match e.envelope.event_type.as_str() {
            "ChangesCommitPushIntent" => {
                intents.insert(key, p);
            }
            "ChangesCommitPushEnded" => {
                ended.insert(
                    format!("{key}:{}", p["tool_call_id"].as_str().unwrap_or_default()),
                    p,
                );
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    for c in calls {
        let intent = intents
            .get(&c.arguments_hash)
            .cloned()
            .unwrap_or(Value::Null);
        let auth = modbit_policy::ledger::authorization_of(&receipts, &c.tool_call_id);
        let result = modbit_policy::ledger::result_of(&receipts, &c.tool_call_id);
        let end = ended.get(&format!(
            "{}:{}",
            c.arguments_hash,
            hex::encode(c.tool_call_id.as_bytes())
        ));
        let approval = store.approval_for_call(&c.tool_call_id).ok().flatten();
        let denied = approval
            .as_ref()
            .is_some_and(|a| a.state == modbit_domain::approval::ApprovalState::Denied);
        let state = match c.state {
            ToolCallState::Succeeded => "COMPLETED",
            ToolCallState::Failed | ToolCallState::Cancelled if denied => "DENIED",
            ToolCallState::Failed => "FAILED",
            ToolCallState::Cancelled => "DENIED",
            ToolCallState::ApprovalPending => "APPROVAL_PENDING",
            ToolCallState::Dispatched
            | ToolCallState::Streaming
            | ToolCallState::UnknownOutcome => {
                if result.is_some() {
                    "FAILED"
                } else {
                    "AUTHORIZED_IN_DOUBT"
                }
            }
            _ => "APPROVAL_PENDING",
        };
        out.push(wire::ChangeEffectView {
            tool_call_id: hex::encode(c.tool_call_id.as_bytes()),
            state: state.to_owned(),
            approval_id: approval
                .map(|a| a.approval_id.to_string())
                .or_else(|| c.approval_id.map(|a| a.to_string()))
                .unwrap_or_default(),
            intent_hash: c.arguments_hash.clone(),
            message: intent["message"].as_str().unwrap_or_default().to_owned(),
            paths: strs(&intent["paths"]),
            branch: intent["branch"].as_str().unwrap_or_default().to_owned(),
            remote: intent["remote"].as_str().unwrap_or_default().to_owned(),
            push_only: intent["push_only"].as_bool().unwrap_or(false),
            head_before: intent["head_before"]
                .as_str()
                .unwrap_or_default()
                .to_owned(),
            commit: end
                .and_then(|e| e["commit"].as_str())
                .unwrap_or_default()
                .to_owned(),
            authorized_receipt: auth.map(|r| r.receipt_hash.clone()).unwrap_or_default(),
            result_receipt: result.map(|r| r.receipt_hash.clone()).unwrap_or_default(),
            result_status: result.map(|r| r.status.clone()).unwrap_or_default(),
            code: end
                .and_then(|e| e["code"].as_str())
                .unwrap_or_default()
                .to_owned(),
            detail: end
                .and_then(|e| e["detail"].as_str())
                .unwrap_or_default()
                .to_owned(),
        });
    }
    out
}

fn strs(v: &Value) -> Vec<String> {
    v.as_array()
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

// ----------------------------------------------------------------- revert

fn revert_ack_of(
    events: &[modbit_event_store::StoredEvent],
    store: &modbit_event_store::EventStore,
) -> Option<wire::RevertChangesAck> {
    let e = events
        .iter()
        .find(|e| e.envelope.event_type == "ChangesReverted")?;
    let p = store.payload(&e.envelope).ok()?;
    Some(wire::RevertChangesAck {
        path: p["path"].as_str().unwrap_or_default().to_owned(),
        action: p["action"].as_str().unwrap_or_default().to_owned(),
        hunk_index: p["hunk_index"].as_u64().unwrap_or(0) as u32,
        workspace_revision: p["workspace_revision"].as_u64().unwrap_or(0),
        previous_revision: p["previous_revision"].as_u64().unwrap_or(0),
        file_revision: p["file_revision"].as_str().unwrap_or_default().to_owned(),
        before_hash: p["before_hash"].as_str().unwrap_or_default().to_owned(),
        checkpoint_id: p["checkpoint_id"].as_str().unwrap_or_default().to_owned(),
        offset: e.offset,
        replayed: true,
    })
}

/// `RevertChanges` (the caller has checked the session lease).
pub(crate) async fn revert(
    core: &Core,
    p: &wire::RevertChanges,
    command_id: [u8; 16],
    record_: CommandRecord,
    actor: Actor,
) -> Result<wire::RevertChangesAck, Refusal> {
    revert_impl(core, p, command_id, record_, actor, None)
        .await
        .map(|(a, _)| a)
}

/// The revert, and with `decide` (PX-103) the per-hunk rejection that is
/// allowed while the task's loop runs: the hunk is attributed and checked
/// against the digest the person saw, and the `HunkDecided` record lands in
/// the same transaction as the revert. A replay returns the recorded decision.
pub(crate) async fn revert_impl(
    core: &Core,
    p: &wire::RevertChanges,
    command_id: [u8; 16],
    record_: CommandRecord,
    actor: Actor,
    decide: Option<&crate::hunks::DecideCtx>,
) -> Result<(wire::RevertChangesAck, Option<(Value, u64)>), Refusal> {
    if p.path.trim().is_empty() {
        return Err(refuse("BAD_PAYLOAD", "path required"));
    }
    // A retry replays the record; nothing is written twice.
    {
        let store = core.store.lock().await;
        match store.prior_command(&record_) {
            Ok(Some(CommandOutcome::Replayed(events))) => {
                let decided = events
                    .iter()
                    .find(|e| e.envelope.event_type == "HunkDecided")
                    .and_then(|e| store.payload(&e.envelope).ok().map(|p| (p, e.offset)));
                return revert_ack_of(&events, &store)
                    .map(|a| (a, decided))
                    .ok_or_else(|| refuse("REPLAY", "the recorded command carries no revert"));
            }
            Ok(_) => {}
            Err(e) => return Err(refuse(crate::server::error_code(&e), e.to_string())),
        }
    }
    let task = load_task(core, p.task_id.as_ref()).await?;
    let root = root_of(&task)?;
    // The loop owns the tree while it runs; a per-hunk decision is the one
    // revert that does not wait for it: the file service's content-hash
    // precondition refuses it if the loop wrote the file in the meantime.
    if decide.is_none() {
        require_stopped(core, &task, "a revert").await?;
    }
    let run_live = core.runtime.is_running(&task.task_id).await;
    let (ws, _) = core
        .tools
        .workspace(&root)
        .await
        .map_err(|e| refuse("WORKSPACE", e.to_string()))?;
    // Path policy after symlink resolution, before anything is read for the
    // revert (docs/23): the resolved target decides, not the name.
    let relative = {
        let svc = ws.lock().await;
        svc.resolve(&p.path)
            .map_err(|e| refuse(code_of(&e), e.to_string()))?
            .relative
    };
    // The change set as it is now, and what git says about the path.
    let residue = crate::review::residue_of(core, &task).await;
    let task2 = task.clone();
    let root2 = root.clone();
    let rel2 = relative.clone();
    let (cand, code, merging) = tokio::task::spawn_blocking(move || {
        let cand = crate::review::candidate(&task2, &residue)?;
        let repo = Repo::open(Path::new(&root2)).map_err(git_err)?;
        let code = repo
            .status()
            .map_err(git_err)?
            .into_iter()
            .find(|e| e.path == rel2)
            .map(|e| e.code)
            .unwrap_or_else(|| "..".to_owned());
        Ok::<_, Refusal>((cand, code, repo.merge_in_progress()))
    })
    .await
    .map_err(|e| refuse("REVERT_FAILED", e.to_string()))??;
    let Some(file) = cand.files.iter().find(|f| f.path == relative) else {
        return Err(refuse(
            "NO_SUCH_CHANGE",
            format!("`{relative}` has no change against HEAD to revert"),
        ));
    };
    let current = ws.lock().await.read(&relative).ok();
    let current_revision_of_file = current
        .as_ref()
        .map(|r| r.content_hash.clone())
        .unwrap_or_default();
    if p.expected_file_revision != current_revision_of_file {
        return Err(refuse(
            "STALE_REVISION",
            format!(
                "`{relative}` is at revision {}, the revert was made against {}; reload the changes",
                if current_revision_of_file.is_empty() {
                    "(deleted)"
                } else {
                    &current_revision_of_file
                },
                if p.expected_file_revision.is_empty() {
                    "(deleted)"
                } else {
                    &p.expected_file_revision
                },
            ),
        ));
    }
    if merging || is_unmerged(&code) {
        return Err(refuse(
            "DIRTY_CONFLICT",
            format!("`{relative}` is part of an unfinished merge; finish or abort it first"),
        ));
    }
    if !code.starts_with(['.', '?']) {
        return Err(refuse(
            "DIRTY_CONFLICT",
            format!("`{relative}` has staged changes; unstage it before reverting it"),
        ));
    }
    // What the revert writes.
    let base_path = file.old_path.clone().unwrap_or_else(|| file.path.clone());
    let base_bytes = if file.status == 'A' {
        None
    } else {
        Some(
            cand.repo
                .show(&cand.base_commit, &base_path)
                .map_err(git_err)?,
        )
    };
    let mut decided_info: Option<crate::hunks::HunkInfo> = None;
    let (kind, action) = if p.whole_file {
        match (file.status, &base_bytes) {
            ('A', _) => (ChangeOpKind::Delete, "DELETED_FILE"),
            ('D', Some(b)) => (ChangeOpKind::Create(b.clone()), "RESTORED_FILE"),
            (_, Some(b)) => (ChangeOpKind::ReplaceExact(b.clone()), "REVERTED_FILE"),
            (_, None) => return Err(refuse("NO_SUCH_CHANGE", "the file has no base to restore")),
        }
    } else {
        if file.binary {
            return Err(refuse(
                "BAD_PAYLOAD",
                "a binary file reverts as a whole file, not by hunk",
            ));
        }
        if p.hunk_index as usize >= file.hunks.len() {
            return Err(refuse(
                "UNKNOWN_HUNK",
                format!("{relative}#{} does not exist", p.hunk_index),
            ));
        }
        let base = base_bytes.clone().unwrap_or_default();
        let base = String::from_utf8(base).map_err(|_| {
            refuse(
                "BAD_PAYLOAD",
                "a hunk of a non-UTF-8 file cannot be reverted",
            )
        })?;
        let now = String::from_utf8(
            current
                .as_ref()
                .map(|r| r.bytes.clone())
                .ok_or_else(|| refuse("NO_SUCH_FILE", relative.clone()))?,
        )
        .map_err(|_| {
            refuse(
                "BAD_PAYLOAD",
                "a hunk of a non-UTF-8 file cannot be reverted",
            )
        })?;
        let all: Vec<u32> = file.hunks.iter().map(|h| h.index).collect();
        // The hunks must rebuild the file exactly, or the reverse would not be
        // the change that is shown.
        if apply_selected(&base, &file.hunks, &all) != now {
            return Err(refuse(
                "UNSUPPORTED_HUNK",
                format!(
                    "`{relative}`'s hunks do not rebuild the file exactly; revert the whole file"
                ),
            ));
        }
        let keep: Vec<u32> = all.iter().copied().filter(|i| *i != p.hunk_index).collect();
        let rebuilt = apply_selected(&base, &file.hunks, &keep);
        if let Some(d) = decide {
            let shown = &file.hunks[p.hunk_index as usize];
            if !d.expected_digest.is_empty()
                && d.expected_digest != crate::hunks::hunk_digest(shown)
            {
                return Err(refuse(
                    "STALE_HUNK",
                    format!(
                        "{relative}#{} is no longer the hunk that was shown; reload the changes",
                        p.hunk_index
                    ),
                ));
            }
            let inputs = [crate::hunks::FileInput {
                path: &relative,
                base: Some(base.clone()),
                current: Some(now.clone()),
                hunks: &file.hunks,
            }];
            decided_info = crate::hunks::attribute(core, &root, &inputs)
                .await
                .remove(&relative)
                .and_then(|v| v.into_iter().nth(p.hunk_index as usize));
        }
        (
            ChangeOpKind::ReplaceExact(rebuilt.into_bytes()),
            "REVERTED_HUNK",
        )
    };
    // A checkpoint before the destructive write, so the revert is undone like
    // any other restore (docs/14 §8).
    let lt = Lineage::task(core.tenant_id, task.session_id, task.task_id);
    let captured = crate::checkpoint::capture(core, &task, lt, &actor, None, "before_revert")
        .await
        .map_err(|e| {
            refuse(
                "CHECKPOINT_FAILED",
                format!("no checkpoint, so no revert: {e}"),
            )
        })?;
    if captured.committed.is_err() {
        return Err(refuse(
            "CHECKPOINT_FAILED",
            "a newer checkpoint epoch superseded this one; nothing was reverted",
        ));
    }
    let checkpoint_id = captured.manifest.checkpoint_id.to_string();
    // The write: both preconditions are checked by the file service itself.
    let mut svc = ws.lock().await;
    let revision_before = svc.revision().number;
    let mut pre_bytes: HashMap<String, Option<Vec<u8>>> = HashMap::new();
    pre_bytes.insert(relative.clone(), current.as_ref().map(|r| r.bytes.clone()));
    let before_hash = current_revision_of_file.clone();
    let pre = if matches!(kind, ChangeOpKind::Create(_)) {
        WritePrecondition {
            expected_content_hash: None,
            expected_workspace_revision: None,
            expect_absent: true,
        }
    } else {
        WritePrecondition {
            expected_content_hash: Some(before_hash.clone()),
            expected_workspace_revision: None,
            expect_absent: false,
        }
    };
    let changes = svc
        .apply_transaction(&[ChangeOp {
            path: relative.clone(),
            kind,
            pre,
        }])
        .map_err(|e| refuse(code_of(&e), e.to_string()))?;
    let Some(change) = changes.first() else {
        return Err(refuse("WORKSPACE", "no change recorded"));
    };
    let after_hash = change.after_hash.clone().unwrap_or_default();
    let workspace_revision = change.workspace_revision.number;
    let mut store = core.store.lock().await;
    let objects = store.objects().clone();
    let mut file_events = file_changed_events(
        &objects,
        &svc,
        task.task_id,
        ToolCallId::from_bytes(command_id),
        &changes,
        &pre_bytes,
        revision_before,
        "user_revert:",
        "user_revert",
    );
    for e in &mut file_events {
        e.actor = actor.clone();
    }
    drop(svc);
    let note = record(
        "ChangesReverted",
        json!({
            "path": relative,
            "action": action,
            "hunk_index": p.hunk_index,
            "whole_file": p.whole_file,
            "before_hash": before_hash,
            "file_revision": after_hash,
            "workspace_revision": workspace_revision,
            "previous_revision": revision_before,
            "checkpoint_id": checkpoint_id,
            "provenance": "user_revert",
            "command_id": hex::encode(command_id),
        }),
        &actor,
    );
    let mut ws_request = append_request(core, &task, file_events);
    ws_request.aggregate_id = workspace_aggregate_id(&root);
    let mut records = vec![note];
    if let (Some(d), Some(info)) = (decide, &decided_info) {
        let shown = cand
            .files
            .iter()
            .find(|f| f.path == relative)
            .and_then(|f| f.hunks.get(p.hunk_index as usize))
            .cloned();
        if let Some(h) = shown {
            records.push(crate::hunks::rejected_record(
                d,
                &relative,
                &h,
                info,
                command_id,
                run_live,
                &before_hash,
                &after_hash,
                workspace_revision,
                &checkpoint_id,
                &actor,
            ));
        }
    }
    let outcome = store
        .execute_command_all(
            record_,
            vec![ws_request, append_request(core, &task, records)],
        )
        .map_err(|e| refuse(crate::server::error_code(&e), e.to_string()))?;
    let events = match outcome {
        CommandOutcome::Applied(e) | CommandOutcome::Replayed(e) => e,
    };
    let offset = events.last().map_or(0, |e| e.offset);
    core.last_offset.send_replace(offset);
    Ok((
        wire::RevertChangesAck {
            path: relative,
            action: action.to_owned(),
            hunk_index: p.hunk_index,
            workspace_revision,
            previous_revision: revision_before,
            file_revision: after_hash,
            before_hash,
            checkpoint_id,
            offset,
            replayed: false,
        },
        None,
    ))
}

// ------------------------------------------------------------------- stage

/// `StageChanges` (the caller has checked the session lease).
pub(crate) async fn stage(
    core: &Core,
    p: &wire::StageChanges,
    actor: Actor,
) -> Result<wire::StageChangesAck, Refusal> {
    if p.paths.is_empty() || p.paths.len() > 2000 {
        return Err(refuse("BAD_PAYLOAD", "name between 1 and 2000 paths"));
    }
    let task = load_task(core, p.task_id.as_ref()).await?;
    let root = root_of(&task)?;
    require_stopped(core, &task, "staging").await?;
    let (ws, _) = core
        .tools
        .workspace(&root)
        .await
        .map_err(|e| refuse("WORKSPACE", e.to_string()))?;
    let mut paths: Vec<(String, String)> = Vec::new();
    {
        let svc = ws.lock().await;
        for sp in &p.paths {
            let resolved = svc
                .resolve(&sp.path)
                .map_err(|e| refuse(code_of(&e), e.to_string()))?;
            let revision = svc
                .read(&resolved.relative)
                .map(|r| r.content_hash)
                .unwrap_or_default();
            if p.stage && sp.expected_file_revision != revision {
                return Err(refuse(
                    "STALE_REVISION",
                    format!(
                        "`{}` changed since the client read it; reload the changes",
                        resolved.relative
                    ),
                ));
            }
            paths.push((resolved.relative, revision));
        }
    }
    let root2 = root.clone();
    let stage = p.stage;
    let names: Vec<String> = paths.iter().map(|(p, _)| p.clone()).collect();
    let names2 = names.clone();
    let digest = tokio::task::spawn_blocking(move || {
        let repo = Repo::open(Path::new(&root2)).map_err(git_err)?;
        if repo.merge_in_progress() {
            return Err(refuse(
                "DIRTY_CONFLICT",
                "a merge is in progress; finish or abort it first",
            ));
        }
        let status = repo.status().map_err(git_err)?;
        for n in &names2 {
            let Some(e) = status.iter().find(|e| &e.path == n) else {
                return Err(refuse(
                    "NO_SUCH_CHANGE",
                    format!(
                        "`{n}` has no change to {}",
                        if stage { "stage" } else { "unstage" }
                    ),
                ));
            };
            if is_unmerged(&e.code) {
                return Err(refuse(
                    "DIRTY_CONFLICT",
                    format!("`{n}` is unmerged; resolve it first"),
                ));
            }
        }
        let refs: Vec<&str> = names2.iter().map(String::as_str).collect();
        if stage {
            repo.stage_paths(&refs).map_err(git_err)?;
        } else {
            repo.unstage_paths(&refs).map_err(git_err)?;
        }
        Ok::<_, Refusal>(repo.staged().map_err(git_err)?.digest)
    })
    .await
    .map_err(|e| refuse("STAGE_FAILED", e.to_string()))??;
    let offset = {
        let mut store = core.store.lock().await;
        let stored = store
            .append(append_request(
                core,
                &task,
                vec![record(
                    "ChangesStaged",
                    json!({"paths": names, "staged": p.stage, "staged_digest": digest}),
                    &actor,
                )],
            ))
            .map_err(|e| refuse("STORE", e.to_string()))?;
        stored.last().map_or(0, |e| e.offset)
    };
    core.last_offset.send_replace(offset);
    Ok(wire::StageChangesAck {
        paths: names,
        staged: p.stage,
        staged_digest: digest,
        offset,
    })
}

// ---------------------------------------------------------- commit and push

/// What a Commit and push is about to be, read from the repository now.
struct Plan {
    message: String,
    paths: Vec<String>,
    branch: String,
    remote: String,
    remote_url: String,
    head: String,
    staged_digest: String,
    push_only: bool,
}

impl Plan {
    fn args(&self) -> Value {
        json!({
            "message": self.message,
            "paths": self.paths,
            "branch": self.branch,
            "remote": self.remote,
            "head_before": self.head,
            "staged_digest": self.staged_digest,
            "push_only": self.push_only,
        })
    }
}

/// Read the repository and decide the intent, or refuse with the typed
/// reason before anything is asked of the person.
fn plan_of(task_root: &Path, p: &wire::CommitAndPush) -> Result<Plan, Refusal> {
    let repo = Repo::open(task_root).map_err(git_err)?;
    let remote = pick_remote(&repo, &p.remote)?;
    let Some(branch) = repo.current_branch().map_err(git_err)? else {
        return Err(refuse(
            "NO_BRANCH",
            "HEAD is detached; switch to a branch before pushing",
        ));
    };
    if repo.merge_in_progress() {
        return Err(refuse(
            "DIRTY_CONFLICT",
            "a merge is in progress; finish or abort it first",
        ));
    }
    if repo
        .status()
        .map_err(git_err)?
        .iter()
        .any(|e| is_unmerged(&e.code))
    {
        return Err(refuse(
            "DIRTY_CONFLICT",
            "a path is unmerged; resolve it before committing",
        ));
    }
    if repo.is_unborn() {
        return Err(refuse(
            "NO_BRANCH",
            "the repository has no commit yet; make a first commit outside the app",
        ));
    }
    let head = repo.head().map_err(git_err)?;
    let staged = repo.staged().map_err(git_err)?;
    if !p.expected_head.is_empty() && p.expected_head != head {
        return Err(refuse(
            "STALE_STAGE",
            format!(
                "HEAD is {head}, the commit was prepared against {}; reload",
                p.expected_head
            ),
        ));
    }
    if !p.expected_staged_digest.is_empty() && p.expected_staged_digest != staged.digest {
        return Err(refuse(
            "STALE_STAGE",
            "the staged files are not the ones the commit was prepared for; reload",
        ));
    }
    let remote_url =
        modbit_git::redact_url_credentials(&repo.remote_url(&remote).map_err(git_err)?);
    if staged.entries.is_empty() {
        let ahead = repo.unpushed_count(&remote).map_err(git_err)?;
        if ahead == 0 {
            return Err(refuse(
                "NOTHING_STAGED",
                "nothing is staged and the remote lacks no commit",
            ));
        }
        return Ok(Plan {
            message: repo.subject_of("HEAD").map_err(git_err)?,
            paths: repo.unpushed_paths(&remote).map_err(git_err)?,
            branch,
            remote,
            remote_url,
            head,
            staged_digest: staged.digest,
            push_only: true,
        });
    }
    let message = p.message.trim_end().to_owned();
    if message.trim().is_empty() {
        return Err(refuse("BAD_PAYLOAD", "a commit message is required"));
    }
    if message.len() > MAX_MESSAGE || message.contains('\0') {
        return Err(refuse(
            "BAD_PAYLOAD",
            format!("the commit message is at most {MAX_MESSAGE} bytes of text"),
        ));
    }
    let policy = crate::apply_back::protected_fn(task_root)?;
    for e in &staged.entries {
        if policy.check(&e.path).is_err() {
            return Err(refuse(
                "PROTECTED_PATH",
                format!(
                    "`{}` is a protected path and is not committed from here",
                    e.path
                ),
            ));
        }
    }
    Ok(Plan {
        message,
        paths: staged.entries.iter().map(|e| e.path.clone()).collect(),
        branch,
        remote,
        remote_url,
        head,
        staged_digest: staged.digest,
        push_only: false,
    })
}

fn ack_base(plan: &Plan) -> wire::CommitAndPushAck {
    wire::CommitAndPushAck {
        message: plan.message.clone(),
        paths: plan.paths.clone(),
        branch: plan.branch.clone(),
        remote: plan.remote.clone(),
        remote_url: plan.remote_url.clone(),
        head_before: plan.head.clone(),
        push_only: plan.push_only,
        staged_digest: plan.staged_digest.clone(),
        ..Default::default()
    }
}

/// `CommitAndPush` (the caller has checked the session lease).
pub(crate) async fn commit_push_command(
    core: &Arc<Core>,
    p: &wire::CommitAndPush,
    actor: &Actor,
    lease_generation: Option<u64>,
) -> Result<wire::CommitAndPushAck, Refusal> {
    let task = load_task(core, p.task_id.as_ref()).await?;
    let root = root_of(&task)?;
    require_stopped(core, &task, "a commit").await?;
    let plan = {
        let root = root.clone();
        let p = p.clone();
        tokio::task::spawn_blocking(move || plan_of(Path::new(&root), &p))
            .await
            .map_err(|e| refuse("PLAN_FAILED", e.to_string()))??
    };
    let args = plan.args();
    let arguments_json = args.to_string();
    let intent_hash = modbit_tools::arguments_hash(&arguments_json).unwrap_or_default();
    // The record of what this intent is, once, so a restart can still say
    // what an in-doubt effect was about.
    let failures = {
        let mut store = core.store.lock().await;
        let events = store
            .read_aggregate(&records_aggregate(task.task_id), 0, usize::MAX)
            .unwrap_or_default();
        let mut known = false;
        let mut failed = 0u32;
        for e in &events {
            let Ok(pl) = store.payload(&e.envelope) else {
                continue;
            };
            if pl["intent_hash"].as_str() != Some(intent_hash.as_str()) {
                continue;
            }
            match e.envelope.event_type.as_str() {
                "ChangesCommitPushIntent" => known = true,
                "ChangesCommitPushEnded" if pl["status"] == "FAILED" => failed += 1,
                _ => {}
            }
        }
        if !known {
            let mut body = args.clone();
            body["intent_hash"] = json!(intent_hash);
            store
                .append(append_request(
                    core,
                    &task,
                    vec![record("ChangesCommitPushIntent", body, actor)],
                ))
                .map_err(|e| refuse("STORE", e.to_string()))?;
        }
        failed
    };
    // One call per (intent, how often this intent has failed): a retry after a
    // failure is a new call with its own approval; the retry after an approval
    // is the same call.
    let seed = format!(
        "{}:{failures}",
        &digest_of(&[
            &plan.message,
            &plan.paths.join("\n"),
            &plan.branch,
            &plan.remote,
            &plan.head,
            &plan.staged_digest,
            if plan.push_only { "push" } else { "commit" },
        ])[..16]
    );
    let called = call_tool(core, &task, TOOL, &args, &seed, actor, lease_generation).await?;
    let tool_call_id = {
        let store = core.store.lock().await;
        store
            .tool_calls_for_task(&task.task_id)
            .unwrap_or_default()
            .into_iter()
            .rfind(|c| c.tool_name == TOOL && c.arguments_hash == intent_hash)
            .map(|c| hex::encode(c.tool_call_id.as_bytes()))
            .unwrap_or_default()
    };
    let mut ack = ack_base(&plan);
    ack.intent_hash = intent_hash.clone();
    ack.tool_call_id = tool_call_id.clone();
    match called {
        Called::Pending {
            approval_id,
            intent_hash,
        } => {
            ack.status = "APPROVAL_PENDING".into();
            ack.approval_id = approval_id;
            ack.intent_hash = intent_hash;
            ack.detail =
                "decide the approval, then call CommitAndPush again with the same arguments".into();
        }
        Called::Denied(why) => {
            ack.status = "DENIED".into();
            ack.detail = why;
        }
        Called::Done(r) => {
            ack.effect_receipt_ids = r.effect_receipt_ids.clone();
            let ok = r.status == modbit_tools::ToolStatus::Success;
            if ok {
                let o = &r.structured_output;
                ack.status = "COMPLETED".into();
                ack.commit = o["commit"].as_str().unwrap_or_default().to_owned();
                ack.pushed = o["pushed"].as_bool().unwrap_or(false);
                ack.replayed = o["replayed"].as_bool().unwrap_or(false);
            } else {
                ack.status = "FAILED".into();
                ack.code = r.error_code.clone().unwrap_or_default();
                ack.detail = r.error_message.clone().unwrap_or_default();
            }
            // The outcome beside the intent, once per call.
            let mut store = core.store.lock().await;
            let events = store
                .read_aggregate(&records_aggregate(task.task_id), 0, usize::MAX)
                .unwrap_or_default();
            let recorded = events.iter().any(|e| {
                e.envelope.event_type == "ChangesCommitPushEnded"
                    && store
                        .payload(&e.envelope)
                        .is_ok_and(|pl| pl["tool_call_id"].as_str() == Some(tool_call_id.as_str()))
            });
            if !recorded {
                let _ = store.append(append_request(
                    core,
                    &task,
                    vec![record(
                        "ChangesCommitPushEnded",
                        json!({
                            "intent_hash": intent_hash,
                            "tool_call_id": tool_call_id,
                            "status": if ok { "COMPLETED" } else { "FAILED" },
                            "code": ack.code,
                            "detail": ack.detail,
                            "commit": ack.commit,
                        }),
                        actor,
                    )],
                ));
            }
        }
    }
    let offset = core.store.lock().await.last_offset().unwrap_or(0);
    core.last_offset.send_replace(offset);
    Ok(ack)
}

/// The `Authorization` header git's HTTP transport sends for a token (GitHub
/// takes the token as the basic-auth password).
fn basic_header(token: &str) -> String {
    use base64::Engine as _;
    let raw = format!("x-access-token:{token}");
    format!(
        "Authorization: Basic {}",
        base64::engine::general_purpose::STANDARD.encode(raw)
    )
}

/// The environment (never an argument) that hands the push process a forge
/// token for an https remote on the forge's own host. `None` for any other
/// remote: a local path, ssh or a host the forge does not own gets no token.
pub(crate) fn push_env(
    forge: Option<&modbit_tools::forge::ForgeConfig>,
    task: TaskId,
    remote_url: &str,
) -> Result<Vec<(String, String)>, Refusal> {
    let Some(cfg) = forge else {
        return Ok(vec![]);
    };
    let Some(token) = &cfg.token else {
        return Ok(vec![]);
    };
    let Some(rest) = remote_url.strip_prefix("https://") else {
        return Ok(vec![]);
    };
    let host = rest
        .split('/')
        .next()
        .unwrap_or_default()
        .split('@')
        .next_back()
        .unwrap_or_default();
    if host != cfg.web_host {
        return Ok(vec![]);
    }
    let secret = token
        .for_principal(&format!("task:{task}"))
        .acquire_for(&cfg.kind, "git.push")
        .map_err(|r| {
            refuse(
                "CREDENTIAL_REFUSED",
                format!("the credential broker refused the push token: {}", r.code()),
            )
        })?;
    Ok(vec![
        ("GIT_CONFIG_COUNT".to_owned(), "1".to_owned()),
        ("GIT_CONFIG_KEY_0".to_owned(), "http.extraHeader".to_owned()),
        (
            "GIT_CONFIG_VALUE_0".to_owned(),
            basic_header(secret.expose()),
        ),
    ])
}

/// What the governed tool `git.changes.commit_push` does (the port op
/// `changes.commit_push`): the approved intent, re-checked against the
/// repository now, then the commit and the push.
pub(crate) async fn exec_commit_push(port: &CoreGitState, args: &Value) -> Result<Value, Refusal> {
    let root = port
        .root
        .clone()
        .ok_or_else(|| refuse("NO_WORKSPACE", "the task has no workspace root"))?;
    let message = args["message"].as_str().unwrap_or_default().to_owned();
    let branch = args["branch"].as_str().unwrap_or_default().to_owned();
    let remote = args["remote"].as_str().unwrap_or_default().to_owned();
    let head_before = args["head_before"].as_str().unwrap_or_default().to_owned();
    let digest = args["staged_digest"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    let push_only = args["push_only"].as_bool().unwrap_or(false);
    let paths = strs(&args["paths"]);
    let forge = port.forge.clone();
    let task = port.task;
    let redactor = forge
        .as_ref()
        .and_then(|f| f.token.as_ref())
        .map(modbit_tools::forge::ForgeToken::redactor);
    let run = move || -> Result<Value, Refusal> {
        let repo = Repo::open(&root).map_err(git_err)?;
        if repo.current_branch().map_err(git_err)?.as_deref() != Some(branch.as_str()) {
            return Err(refuse(
                "STALE_INTENT",
                format!("the checkout is no longer on `{branch}`; nothing was committed"),
            ));
        }
        let mut commit = repo.head().map_err(git_err)?;
        if !push_only {
            // The intent is the HEAD it commits onto and the staged content.
            if commit != head_before {
                return Err(refuse(
                    "STALE_INTENT",
                    "HEAD moved since the approval; nothing was committed",
                ));
            }
            let staged = repo.staged().map_err(git_err)?;
            if staged.digest != digest {
                return Err(refuse(
                    "STALE_INTENT",
                    "the staged files changed since the approval; nothing was committed",
                ));
            }
            let policy = crate::apply_back::protected_fn(&root)?;
            if let Some(e) = staged
                .entries
                .iter()
                .find(|e| policy.check(&e.path).is_err())
            {
                return Err(refuse(
                    "PROTECTED_PATH",
                    format!("`{}` is a protected path", e.path),
                ));
            }
            commit = repo.commit_staged(&message).map_err(git_err)?;
        } else if repo.unpushed_count(&remote).map_err(git_err)? == 0 {
            // Everything the remote lacked is there: the push has nothing to send.
            return Ok(json!({
                "commit": commit, "branch": branch, "remote": remote,
                "pushed": true, "push_only": true, "paths": paths, "replayed": true,
            }));
        }
        let url = repo.remote_url(&remote).map_err(git_err)?;
        let env = push_env(forge.as_deref(), task, &url)?;
        let envs: Vec<(&str, &str)> = env.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
        match repo.push_with_env(&remote, &branch, &envs) {
            Ok(_) => Ok(json!({
                "commit": commit, "branch": branch, "remote": remote,
                "pushed": true, "push_only": push_only, "paths": paths,
            })),
            Err(e) => {
                let text = e.to_string();
                let text = redactor
                    .as_ref()
                    .map_or(text.clone(), |r| r.error_text(&text));
                let code = if text.contains("rejected") || text.contains("non-fast-forward") {
                    "PUSH_REJECTED"
                } else {
                    "GIT_PUSH"
                };
                let local = if push_only {
                    String::new()
                } else {
                    format!("the commit {commit} was made locally; ")
                };
                Err(refuse(
                    code,
                    format!("{local}pushing `{branch}` to `{remote}` failed: {text}"),
                ))
            }
        }
    };
    tokio::task::spawn_blocking(run)
        .await
        .map_err(|e| refuse("COMMIT_PUSH_FAILED", e.to_string()))?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_basic_header_carries_the_token_only_as_the_password() {
        let h = basic_header("tok123");
        assert!(h.starts_with("Authorization: Basic "));
        assert!(!h.contains("tok123"));
    }

    #[test]
    fn a_remote_the_forge_does_not_own_gets_no_token() {
        let cfg = modbit_tools::forge::ForgeConfig {
            kind: "github".into(),
            api_base: "https://api.github.com".into(),
            web_host: "github.com".into(),
            token: None,
        };
        let t = TaskId::new();
        for url in [
            "/tmp/remote.git",
            "git@github.com:o/r.git",
            "https://example.com/o/r.git",
        ] {
            assert!(push_env(Some(&cfg), t, url).unwrap().is_empty(), "{url}");
        }
        assert!(
            push_env(None, t, "https://github.com/o/r.git")
                .unwrap()
                .is_empty()
        );
    }
}
