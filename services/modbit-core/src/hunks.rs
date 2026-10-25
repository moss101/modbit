//! Hunk attribution and per-hunk decisions that keep the run going
//! (REQ-PX-103; docs/62 PX-103, docs/20 "Trusted Code Surface").
//!
//! * **Attribution** is a replay of the workspace's change log over the file:
//!   the base (HEAD) text is folded forward through every `FileChanged` the
//!   file service recorded for the path (`change.apply`, `change.batch`, a
//!   person's revert or direct edit) and every `BarrierChangeRecorded` the
//!   change barrier recorded for a shell or test process (FIX-06), and each
//!   line remembers the call that wrote it. A hunk of the review projection is
//!   attributed to the call whose lines it holds. Where the file moved
//!   without a recorded call (a process outside any tool call, an edit by
//!   hand, an unreadable record) the lines are *unattributed*, the hunk says
//!   so, and it is never auto-accepted. Nothing here is decided by a client.
//! * **Reject** is the Core's per-hunk revert (`changes::revert_impl`) against
//!   the file revision the client saw, allowed while the task's loop runs. The
//!   revert and the `HunkDecided` record are ONE transaction under the command
//!   record, so a Core that stops at any instant holds both or neither. The
//!   agent is told at its next round boundary ([`deliver_notices`]): the
//!   delivery is recorded (`HunkNoticeDelivered`), so a resumed run does not
//!   hear it twice and a notice that never landed is offered again.
//! * **Accept** records that a person accepted this exact hunk (its digest);
//!   it writes nothing. `AUTO_ACCEPT` is refused for an unattributed hunk.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use modbit_domain::ToolCallId;
use modbit_domain::event::{Actor, AggregateType};
use modbit_domain::task::Task;
use modbit_domain::workspace::WorkspaceEvent;
use modbit_event_store::{CommandOutcome, CommandRecord, NewEvent, ObjectStore};
use modbit_git::{FileDiff, Hunk};
use modbit_protocol::v1 as wire;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::changes::{append_request, load_task_pub, record, records_aggregate};
use crate::runtime::Lineage;
use crate::server::Core;
use crate::tools::workspace_aggregate_id;

type Refusal = (String, String);

/// Largest file a barrier record keeps the content of (docs/30 bounded payloads).
const BARRIER_CONTENT_LIMIT: usize = 2 * 1024 * 1024;

fn sha(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

/// The digest a decision names: the hunk's body, not its position, so a
/// neighbour's revert that renumbers the hunk does not change what it is.
#[must_use]
pub(crate) fn hunk_digest(h: &Hunk) -> String {
    let mut d = Sha256::new();
    for l in &h.lines {
        d.update(l.as_bytes());
        d.update(b"\n");
    }
    hex::encode(d.finalize())
}

// ----------------------------------------------------------------- barrier

/// The records of the paths a shell or test process changed (FIX-06's
/// snapshot diff), attributed to the call. They are not `FileChanged`: nothing
/// that undoes, gates or verifies on `FileChanged` is asked to act on a write
/// the file service did not make. The content after the call is kept by
/// reference; the content before is not (the barrier holds hashes), so the
/// replay resolves it from the line state when the hashes agree.
pub(crate) fn barrier_events(
    objects: &ObjectStore,
    root: &Path,
    task_id: modbit_domain::TaskId,
    call: ToolCallId,
    tool: &str,
    workspace_revision: u64,
    list: &[Value],
) -> Vec<NewEvent> {
    let mut out = Vec::new();
    for e in list {
        let Some(path) = e["path"].as_str() else {
            continue;
        };
        let rel = Path::new(path);
        if rel.is_absolute()
            || rel
                .components()
                .any(|c| !matches!(c, std::path::Component::Normal(_)))
        {
            continue;
        }
        let change = e["change"].as_str().unwrap_or("modified");
        let after_bytes = if change == "deleted" {
            None
        } else {
            std::fs::read(root.join(rel))
                .ok()
                .filter(|b| b.len() <= BARRIER_CONTENT_LIMIT)
        };
        let after_ref = after_bytes.as_deref().and_then(|b| objects.put(b).ok());
        let hash_of = |v: &Value| {
            v.as_str()
                .map(|s| s.trim_end_matches("+x"))
                .filter(|s| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit()))
                .map(str::to_owned)
        };
        let mut ev = NewEvent::new(
            "BarrierChangeRecorded",
            json!({
                "type": "BarrierChangeRecorded",
                "task_id": task_id.to_string(),
                "tool_call_id": call,
                "tool_name": tool,
                "path": path,
                "change": change,
                "before_hash": hash_of(&e["before"]),
                "after_hash": after_bytes.as_deref().map(sha),
                "after_ref": after_ref,
                "workspace_revision": workspace_revision,
            }),
            Actor::Core("tool-host".into()),
        );
        ev.occurred_at = Some(modbit_domain::Timestamp::now());
        out.push(ev);
    }
    out
}

// ------------------------------------------------------------------ replay

#[derive(Clone, PartialEq, Eq, Debug)]
enum Origin {
    Base,
    Unknown,
    Call(usize),
}

#[derive(Clone)]
struct Line {
    text: String,
    origin: Origin,
    /// Who deleted lines immediately before this one.
    dels: Vec<Origin>,
}

#[derive(Default)]
struct State {
    lines: Vec<Line>,
    tail: Vec<Origin>,
}

impl State {
    fn from_text(text: &str, origin: &Origin) -> Self {
        Self {
            lines: text
                .split_inclusive('\n')
                .map(|l| Line {
                    text: l.to_owned(),
                    origin: origin.clone(),
                    dels: Vec::new(),
                })
                .collect(),
            tail: Vec::new(),
        }
    }

    /// Fold a change to `new` into the line state, crediting it to `origin`.
    fn apply(&mut self, new: &str, origin: &Origin) {
        let old: String = self.lines.iter().map(|l| l.text.as_str()).collect();
        let diff = similar::TextDiff::from_lines(old.as_str(), new);
        let old_lines = std::mem::take(&mut self.lines);
        let mut out: Vec<Line> = Vec::new();
        let mut pending: Vec<Origin> = Vec::new();
        let mut oi = 0usize;
        for ch in diff.iter_all_changes() {
            match ch.tag() {
                similar::ChangeTag::Equal => {
                    let mut l = old_lines[oi].clone();
                    oi += 1;
                    if !pending.is_empty() {
                        let mut d = std::mem::take(&mut pending);
                        d.extend(l.dels);
                        l.dels = d;
                    }
                    out.push(l);
                }
                similar::ChangeTag::Delete => {
                    pending.push(origin.clone());
                    pending.extend(old_lines[oi].dels.iter().cloned());
                    oi += 1;
                }
                similar::ChangeTag::Insert => out.push(Line {
                    text: ch.value().to_owned(),
                    origin: origin.clone(),
                    dels: std::mem::take(&mut pending),
                }),
            }
        }
        pending.extend(std::mem::take(&mut self.tail));
        self.tail = pending;
        self.lines = out;
    }

    fn text(&self) -> String {
        self.lines.iter().map(|l| l.text.as_str()).collect()
    }
}

/// One recorded write of a path, oldest first.
struct Step {
    call: ToolCallId,
    provenance: String,
    op: String,
    barrier: bool,
    tool: String,
    before_hash: Option<String>,
    created: bool,
    before: Option<String>,
    after: Option<String>,
}

fn utf8(core_objects: &ObjectStore, r: &Option<String>) -> Option<String> {
    let r = r.as_ref()?;
    String::from_utf8(core_objects.get(r).ok()?).ok()
}

/// The recorded writes of each path of `paths`, from the workspace log.
async fn steps_of(core: &Core, root: &str, paths: &HashSet<String>) -> HashMap<String, Vec<Step>> {
    let store = core.store.lock().await;
    let objects = store.objects().clone();
    let events = store
        .read_aggregate(&workspace_aggregate_id(root), 0, 1_000_000)
        .unwrap_or_default();
    let mut out: HashMap<String, Vec<Step>> = HashMap::new();
    for e in &events {
        let t = e.envelope.event_type.as_str();
        if t != "FileChanged" && t != "BarrierChangeRecorded" {
            continue;
        }
        let Ok(p) = store.payload(&e.envelope) else {
            continue;
        };
        let Some(path) = p["path"].as_str().filter(|x| paths.contains(*x)) else {
            continue;
        };
        if t == "FileChanged" {
            let Ok(WorkspaceEvent::FileChanged {
                tool_call_id,
                path,
                op,
                before_hash,
                after_hash,
                before_ref,
                after_ref,
                provenance,
                ..
            }) = serde_json::from_value::<WorkspaceEvent>(p.clone())
            else {
                continue;
            };
            let before = if before_hash.is_none() {
                Some(String::new())
            } else {
                utf8(&objects, &before_ref)
            };
            let after = if after_hash.is_none() {
                Some(String::new())
            } else {
                utf8(&objects, &after_ref)
            };
            out.entry(path).or_default().push(Step {
                call: tool_call_id,
                provenance,
                op,
                barrier: false,
                tool: String::new(),
                before_hash,
                created: false,
                before,
                after,
            });
        } else {
            let Some(call) = serde_json::from_value::<ToolCallId>(p["tool_call_id"].clone()).ok()
            else {
                continue;
            };
            let after = if p["change"] == "deleted" {
                Some(String::new())
            } else {
                utf8(&objects, &p["after_ref"].as_str().map(str::to_owned))
            };
            out.entry(path.to_owned()).or_default().push(Step {
                call,
                provenance: "barrier".into(),
                op: "barrier".into(),
                barrier: true,
                tool: p["tool_name"].as_str().unwrap_or_default().to_owned(),
                before_hash: p["before_hash"].as_str().map(str::to_owned),
                created: p["change"] == "added",
                before: None,
                after,
            });
        }
    }
    out
}

/// What the Core knows about the call that wrote a line.
struct CallMeta {
    id: ToolCallId,
    tool: String,
    turn: String,
    run: String,
    basis: &'static str,
}

fn call_meta(core: &Core, store: &modbit_event_store::EventStore, s: &Step) -> CallMeta {
    let _ = core;
    let mut tool = s.tool.clone();
    let (mut turn, mut run) = (String::new(), String::new());
    if let Ok(evs) = store.read_aggregate(s.call.as_bytes(), 0, 1)
        && let Some(e) = evs.first()
    {
        if tool.is_empty()
            && let Ok(p) = store.payload(&e.envelope)
        {
            tool = p["tool_name"].as_str().unwrap_or_default().to_owned();
        }
        turn = e
            .envelope
            .turn_id
            .map(|t| t.to_string())
            .unwrap_or_default();
        run = e.envelope.run_id.map(|t| t.to_string()).unwrap_or_default();
    }
    let user = s.provenance.starts_with("user_");
    if tool.is_empty() {
        tool = if user {
            s.provenance.clone()
        } else {
            s.op.clone()
        };
    }
    let basis = if user {
        "USER"
    } else if s.barrier {
        "BARRIER_DIFF"
    } else {
        "CHANGE_TRANSACTION"
    };
    CallMeta {
        id: s.call,
        tool,
        turn,
        run,
        basis,
    }
}

/// The attribution of one hunk.
#[derive(Clone, Debug, Default)]
pub(crate) struct HunkInfo {
    pub(crate) attributed: bool,
    pub(crate) call: String,
    pub(crate) tool: String,
    pub(crate) turn: String,
    pub(crate) run: String,
    pub(crate) also: Vec<String>,
    pub(crate) basis: &'static str,
    pub(crate) digest: String,
}

/// One file to attribute: its base text (`None` for an added file), its text
/// now (`None` when deleted) and the hunks the diff shows.
pub(crate) struct FileInput<'a> {
    pub(crate) path: &'a str,
    pub(crate) base: Option<String>,
    pub(crate) current: Option<String>,
    pub(crate) hunks: &'a [Hunk],
}

/// Attribute every hunk of every file.
pub(crate) async fn attribute(
    core: &Core,
    root: &str,
    files: &[FileInput<'_>],
) -> HashMap<String, Vec<HunkInfo>> {
    let wanted: HashSet<String> = files.iter().map(|f| f.path.to_owned()).collect();
    let steps = steps_of(core, root, &wanted).await;
    let store = core.store.lock().await;
    let mut out = HashMap::new();
    for f in files {
        let none = Vec::new();
        let chain = steps.get(f.path).unwrap_or(&none);
        let base = f.base.clone().unwrap_or_default();
        let mut state = State::from_text(&base, &Origin::Base);
        let mut metas: Vec<CallMeta> = Vec::new();
        for s in chain {
            let cur = state.text();
            let before = if s.created && s.before_hash.is_none() {
                Some(String::new())
            } else if s.before.is_some() {
                s.before.clone()
            } else if s.before_hash.as_deref() == Some(sha(cur.as_bytes()).as_str()) {
                Some(cur.clone())
            } else {
                None
            };
            let Some(before) = before else {
                // The file was somewhere the record cannot name: whatever this
                // call left is not explained.
                if let Some(a) = &s.after {
                    state.apply(a, &Origin::Unknown);
                }
                continue;
            };
            if before != cur {
                state.apply(&before, &Origin::Unknown);
            }
            if let Some(a) = &s.after {
                metas.push(call_meta(core, &store, s));
                state.apply(a, &Origin::Call(metas.len() - 1));
            }
        }
        let now = f.current.clone().unwrap_or_default();
        if state.text() != now {
            state.apply(&now, &Origin::Unknown);
        }
        let mut infos = Vec::new();
        for h in f.hunks {
            // Where the hunk's changed lines sit in the file now.
            let mut n = if h.new_lines == 0 {
                h.new_start as usize
            } else {
                (h.new_start as usize).saturating_sub(1)
            };
            let mut added: Vec<Origin> = Vec::new();
            let mut removed: Vec<Origin> = Vec::new();
            for l in &h.lines {
                match l.as_bytes().first() {
                    Some(b'+') => {
                        added.push(
                            state
                                .lines
                                .get(n)
                                .map_or(Origin::Unknown, |x| x.origin.clone()),
                        );
                        n += 1;
                    }
                    Some(b'-') => {
                        let d = state.lines.get(n).map_or(&state.tail, |x| &x.dels);
                        if d.is_empty() {
                            removed.push(Origin::Unknown);
                        } else {
                            removed.extend(d.iter().cloned());
                        }
                    }
                    _ => n += 1,
                }
            }
            let origins = if added.is_empty() { removed } else { added };
            let mut counts: Vec<(usize, usize)> = Vec::new();
            let mut all_known = !origins.is_empty();
            for o in &origins {
                match o {
                    Origin::Call(i) => match counts.iter_mut().find(|c| c.0 == *i) {
                        Some(c) => c.1 += 1,
                        None => counts.push((*i, 1)),
                    },
                    _ => all_known = false,
                }
            }
            // Most lines wins; the later call on a tie.
            counts.sort_by_key(|c| (c.1, c.0));
            let primary = counts.last().map(|c| c.0);
            let mut info = HunkInfo {
                attributed: all_known && primary.is_some(),
                basis: "UNATTRIBUTED",
                digest: hunk_digest(h),
                ..Default::default()
            };
            if let Some(p) = primary {
                let m = &metas[p];
                info.call = m.id.to_string();
                info.tool = m.tool.clone();
                info.turn = m.turn.clone();
                info.run = m.run.clone();
                if info.attributed {
                    info.basis = m.basis;
                }
                info.also = counts[..counts.len() - 1]
                    .iter()
                    .map(|c| metas[c.0].id.to_string())
                    .collect();
            }
            infos.push(info);
        }
        out.insert(f.path.to_owned(), infos);
    }
    out
}

/// Attribution for the files of a candidate: reads the base from Git and the
/// text now from disk. Binary and non-UTF-8 files are unattributed.
pub(crate) async fn attribute_candidate(
    core: &Core,
    cand: &crate::review::Candidate,
) -> HashMap<String, Vec<HunkInfo>> {
    let mut texts: Vec<(String, Option<String>, Option<String>)> = Vec::new();
    for f in &cand.files {
        if f.binary {
            continue;
        }
        let base = if f.status == 'A' {
            Some(String::new())
        } else {
            cand.repo
                .show(&cand.base_commit, f.old_path.as_deref().unwrap_or(&f.path))
                .ok()
                .and_then(|b| String::from_utf8(b).ok())
        };
        let now = if f.status == 'D' {
            Some(String::new())
        } else {
            std::fs::read(Path::new(&cand.root).join(&f.path))
                .ok()
                .and_then(|b| String::from_utf8(b).ok())
        };
        texts.push((f.path.clone(), base, now));
    }
    let inputs: Vec<FileInput<'_>> = texts
        .iter()
        .filter_map(|(p, b, n)| {
            let f: &FileDiff = cand.files.iter().find(|f| &f.path == p)?;
            Some(FileInput {
                path: p,
                base: b.clone(),
                current: n.clone(),
                hunks: &f.hunks,
            })
        })
        .collect();
    attribute(core, &cand.root, &inputs).await
}

// ------------------------------------------------------------- decisions

/// The (path, digest) of every hunk a person accepted, from the log.
pub(crate) async fn accepted(core: &Core, task: &Task) -> HashSet<(String, String)> {
    let store = core.store.lock().await;
    let events = store
        .read_aggregate_of_types(&records_aggregate(task.task_id), &["HunkDecided"], 0)
        .unwrap_or_default();
    events
        .iter()
        .filter_map(|e| store.payload(&e.envelope).ok())
        .filter(|p| matches!(p["decision"].as_str(), Some("ACCEPT" | "AUTO_ACCEPT")))
        .map(|p| {
            (
                p["path"].as_str().unwrap_or_default().to_owned(),
                p["hunk_digest"].as_str().unwrap_or_default().to_owned(),
            )
        })
        .collect()
}

/// What the Core tells the agent after a person rejected a hunk.
fn notice_text(path: &str, h: &Hunk, info: &HunkInfo, reason: &str, file_revision: &str) -> String {
    let from = h.new_start;
    let to = h.new_start + h.new_lines.saturating_sub(1);
    let by = if info.call.is_empty() {
        "a write the Core could not attribute to a tool call".to_owned()
    } else {
        format!("your tool call {} ({})", info.call, info.tool)
    };
    let why = if reason.trim().is_empty() {
        "no reason was given".to_owned()
    } else {
        format!("the person's reason: {}", reason.trim())
    };
    format!(
        "[Core notice: a person rejected one hunk of your change] The hunk of `{path}` around lines {from}-{to}, written by {by}, was reverted; the rest of `{path}` is unchanged (file revision now {file_revision}). {why}. Read `{path}` again before you edit it, and do not re-apply the rejected change unless the person asks."
    )
}

/// The ack of a decision as the log holds it (a replayed command).
fn ack_of(p: &Value, offset: u64, replayed: bool) -> wire::DecideHunkAck {
    wire::DecideHunkAck {
        result: p["result"].as_str().unwrap_or_default().to_owned(),
        path: p["path"].as_str().unwrap_or_default().to_owned(),
        hunk_index: p["hunk_index"].as_u64().unwrap_or(0) as u32,
        hunk_digest: p["hunk_digest"].as_str().unwrap_or_default().to_owned(),
        decision_id: p["decision_id"].as_str().unwrap_or_default().to_owned(),
        attributed_call_id: p["tool_call_id"].as_str().unwrap_or_default().to_owned(),
        attributed_tool: p["tool_name"].as_str().unwrap_or_default().to_owned(),
        run_live: p["run_live"].as_bool().unwrap_or(false),
        agent_notice: p["notice"].as_str().unwrap_or_default().to_owned(),
        file_revision: p["file_revision"].as_str().unwrap_or_default().to_owned(),
        before_hash: p["before_hash"].as_str().unwrap_or_default().to_owned(),
        checkpoint_id: p["checkpoint_id"].as_str().unwrap_or_default().to_owned(),
        workspace_revision: p["workspace_revision"].as_u64().unwrap_or(0),
        offset,
        replayed,
    }
}

/// The extra work a decision does inside `changes::revert_impl`.
pub(crate) struct DecideCtx {
    pub(crate) reason: String,
    pub(crate) expected_digest: String,
}

/// The `HunkDecided` record of a rejection.
#[allow(clippy::too_many_arguments)]
pub(crate) fn rejected_record(
    ctx: &DecideCtx,
    path: &str,
    h: &Hunk,
    info: &HunkInfo,
    command_id: [u8; 16],
    run_live: bool,
    before_hash: &str,
    file_revision: &str,
    workspace_revision: u64,
    checkpoint_id: &str,
    actor: &Actor,
) -> NewEvent {
    let notice = notice_text(path, h, info, &ctx.reason, file_revision);
    record(
        "HunkDecided",
        json!({
            "result": "REJECTED",
            "decision": "REJECT",
            "decision_id": hex::encode(command_id),
            "path": path,
            "hunk_index": h.index,
            "hunk_digest": info.digest,
            "tool_call_id": info.call,
            "tool_name": info.tool,
            "turn_id": info.turn,
            "run_id": info.run,
            "attributed": info.attributed,
            "reason": ctx.reason,
            "run_live": run_live,
            "notice": notice,
            "before_hash": before_hash,
            "file_revision": file_revision,
            "workspace_revision": workspace_revision,
            "checkpoint_id": checkpoint_id,
        }),
        actor,
    )
}

/// `DecideHunk` (the caller has checked the session lease).
pub(crate) async fn decide(
    core: &Core,
    p: &wire::DecideHunk,
    command_id: [u8; 16],
    record_: CommandRecord,
    actor: Actor,
) -> Result<wire::DecideHunkAck, Refusal> {
    let decision = p.decision.trim().to_ascii_uppercase();
    if !matches!(decision.as_str(), "ACCEPT" | "REJECT" | "AUTO_ACCEPT") {
        return Err((
            "BAD_PAYLOAD".into(),
            "decision must be ACCEPT, REJECT or AUTO_ACCEPT".into(),
        ));
    }
    if p.path.trim().is_empty() {
        return Err(("BAD_PAYLOAD".into(), "path required".into()));
    }
    if p.expected_file_revision.is_empty() {
        return Err((
            "BAD_PAYLOAD".into(),
            "expected_file_revision required: a decision is made on the revision that was shown"
                .into(),
        ));
    }
    if decision == "REJECT" {
        let ctx = DecideCtx {
            reason: p.reason.clone(),
            expected_digest: p.expected_hunk_digest.clone(),
        };
        let revert = wire::RevertChanges {
            task_id: p.task_id.clone(),
            path: p.path.clone(),
            whole_file: false,
            hunk_index: p.hunk_index,
            expected_file_revision: p.expected_file_revision.clone(),
        };
        let (ack, replay) =
            crate::changes::revert_impl(core, &revert, command_id, record_, actor, Some(&ctx))
                .await?;
        if let Some((payload, offset)) = replay {
            return Ok(ack_of(&payload, offset, true));
        }
        // The record the transaction wrote is the ack's source of truth.
        let task = load_task_pub(core, p.task_id.as_ref()).await?;
        let store = core.store.lock().await;
        let evs = store
            .read_aggregate_of_types(&records_aggregate(task.task_id), &["HunkDecided"], 0)
            .unwrap_or_default();
        let hex_id = hex::encode(command_id);
        let found = evs.iter().rev().find_map(|e| {
            let pl = store.payload(&e.envelope).ok()?;
            (pl["decision_id"] == hex_id).then_some((pl, e.offset))
        });
        return found
            .map(|(pl, off)| ack_of(&pl, off, ack.replayed))
            .ok_or_else(|| ("REPLAY".into(), "the decision record is missing".into()));
    }
    accept(core, p, &decision, command_id, record_, actor).await
}

async fn accept(
    core: &Core,
    p: &wire::DecideHunk,
    decision: &str,
    command_id: [u8; 16],
    record_: CommandRecord,
    actor: Actor,
) -> Result<wire::DecideHunkAck, Refusal> {
    let hex_id = hex::encode(command_id);
    {
        let store = core.store.lock().await;
        match store.prior_command(&record_) {
            Ok(Some(CommandOutcome::Replayed(events))) => {
                for e in &events {
                    if e.envelope.event_type == "HunkDecided"
                        && let Ok(pl) = store.payload(&e.envelope)
                    {
                        return Ok(ack_of(&pl, e.offset, true));
                    }
                }
                return Err((
                    "REPLAY".into(),
                    "the recorded command carries no decision".into(),
                ));
            }
            Ok(_) => {}
            Err(e) => return Err((crate::server::error_code(&e).into(), e.to_string())),
        }
    }
    let task = load_task_pub(core, p.task_id.as_ref()).await?;
    let residue = crate::review::residue_of(core, &task).await;
    let task2 = task.clone();
    let cand = tokio::task::spawn_blocking(move || crate::review::candidate(&task2, &residue))
        .await
        .map_err(|e| ("GIT".to_owned(), e.to_string()))??;
    let relative = {
        let (ws, _) = core
            .tools
            .workspace(&cand.root)
            .await
            .map_err(|e| ("WORKSPACE".to_owned(), e.to_string()))?;
        let svc = ws.lock().await;
        svc.resolve(&p.path)
            .map_err(|e| ("PATH_OUTSIDE_ROOT".to_owned(), e.to_string()))?
            .relative
    };
    let Some(file) = cand.files.iter().find(|f| f.path == relative) else {
        return Err((
            "NO_SUCH_CHANGE".into(),
            format!("`{relative}` has no change against HEAD"),
        ));
    };
    let now_bytes = std::fs::read(Path::new(&cand.root).join(&relative)).ok();
    let now_rev = now_bytes.as_deref().map(sha).unwrap_or_default();
    if now_rev != p.expected_file_revision {
        return Err((
            "STALE_REVISION".into(),
            format!(
                "`{relative}` is at revision {}, the decision was made against {}; reload the changes",
                if now_rev.is_empty() {
                    "(deleted)"
                } else {
                    &now_rev
                },
                p.expected_file_revision
            ),
        ));
    }
    let Some(h) = file.hunks.get(p.hunk_index as usize) else {
        return Err((
            "UNKNOWN_HUNK".into(),
            format!("{relative}#{} does not exist", p.hunk_index),
        ));
    };
    let digest = hunk_digest(h);
    if !p.expected_hunk_digest.is_empty() && p.expected_hunk_digest != digest {
        return Err((
            "STALE_HUNK".into(),
            format!(
                "{relative}#{} is no longer the hunk that was shown",
                p.hunk_index
            ),
        ));
    }
    let attrs = attribute_candidate(core, &cand).await;
    let info = attrs
        .get(&relative)
        .and_then(|v| v.get(p.hunk_index as usize))
        .cloned()
        .unwrap_or_default();
    if decision == "AUTO_ACCEPT" && !info.attributed {
        return Err((
            "UNATTRIBUTED_HUNK".into(),
            format!(
                "{relative}#{} cannot be attributed to a tool call; it is never accepted automatically",
                p.hunk_index
            ),
        ));
    }
    let rec = record(
        "HunkDecided",
        json!({
            "result": "ACCEPTED",
            "decision": decision,
            "decision_id": hex_id,
            "path": relative,
            "hunk_index": h.index,
            "hunk_digest": digest,
            "tool_call_id": info.call,
            "tool_name": info.tool,
            "turn_id": info.turn,
            "run_id": info.run,
            "attributed": info.attributed,
            "run_live": core.runtime.is_running(&task.task_id).await,
            "file_revision": now_rev,
        }),
        &actor,
    );
    let mut store = core.store.lock().await;
    let outcome = store
        .execute_command_all(record_, vec![append_request(core, &task, vec![rec])])
        .map_err(|e| (crate::server::error_code(&e).to_owned(), e.to_string()))?;
    let (CommandOutcome::Applied(events) | CommandOutcome::Replayed(events)) = outcome;
    let e = events
        .iter()
        .find(|e| e.envelope.event_type == "HunkDecided")
        .ok_or_else(|| ("REPLAY".to_owned(), "no decision recorded".to_owned()))?;
    let pl = store
        .payload(&e.envelope)
        .map_err(|e| ("STORE".to_owned(), e.to_string()))?;
    core.last_offset.send_replace(e.offset);
    Ok(ack_of(&pl, e.offset, false))
}

// ----------------------------------------------------------- the notice

/// Tell the run, once each, of the hunks a person rejected since it last
/// looked. Delivered means recorded: a notice whose record did not land is not
/// handed to the model (it is offered again at the next boundary).
pub(crate) async fn deliver_notices(
    core: &Core,
    task: &Task,
    lt: Lineage,
    actor: &Actor,
    run_id: Option<modbit_domain::RunId>,
) -> Vec<String> {
    let mut store = core.store.lock().await;
    let events = store
        .read_aggregate_of_types(
            &records_aggregate(task.task_id),
            &["HunkDecided", "HunkNoticeDelivered"],
            0,
        )
        .unwrap_or_default();
    let mut delivered: HashSet<String> = HashSet::new();
    let mut due: Vec<Value> = Vec::new();
    for e in &events {
        let Ok(p) = store.payload(&e.envelope) else {
            continue;
        };
        if e.envelope.event_type == "HunkNoticeDelivered" {
            delivered.insert(p["decision_id"].as_str().unwrap_or_default().to_owned());
        } else if p["decision"] == "REJECT" {
            due.push(p);
        }
    }
    let mut notices = Vec::new();
    let mut records = Vec::new();
    for p in due {
        let id = p["decision_id"].as_str().unwrap_or_default().to_owned();
        if delivered.contains(&id) {
            continue;
        }
        let text = p["notice"].as_str().unwrap_or_default().to_owned();
        records.push(record(
            "HunkNoticeDelivered",
            json!({
                "decision_id": id,
                "path": p["path"],
                "run_id": run_id.map(|r| r.to_string()),
                "notice": text,
            }),
            actor,
        ));
        notices.push(text);
    }
    if records.is_empty() {
        return notices;
    }
    match crate::runtime::append(
        &mut store,
        core,
        lt,
        AggregateType::Workspace,
        records_aggregate(task.task_id),
        records,
    ) {
        Ok(_) => notices,
        Err(_) => Vec::new(),
    }
}
