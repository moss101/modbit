//! Per-task digests of a session's log (PX-042, docs/65 AFW-B10): what an
//! agent list needs about each task — when it last moved, whether anything a
//! person would want to see landed after they last looked, how much it has
//! changed, how full its context is — computed by the database in grouped
//! passes over the session's events. No event is loaded into memory and no
//! transcript is read; the answer is derived from the canonical log every
//! time and stored nowhere.

use modbit_domain::{SessionId, TaskId, Timestamp};
use rusqlite::{Connection, params};

use crate::Result;

/// Event types after which a person has something new to look at: a finished
/// or aborted message, a state that needs them, a question or an approval.
const VISIBLE: &str = "'AssistantMessageCompleted','AssistantMessageAborted','TaskReadyForReview','TaskNeedsAttention','TaskFailed','TaskCompleted','TaskCancelled','UserQuestionAsked','ApprovalRequested'";

/// Event types that can raise an attention item (`services/modbit-core`
/// `attention`): the only events whose absence proves a task needs no one.
const ATTENTION: &str = "'TaskNeedsAttention','UserQuestionAsked','CapacityDenied','SubagentAdmissionRefused','SubagentProtectedEffect'";

/// What the log says about one task, in numbers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TaskDigest {
    /// The task.
    pub task_id: TaskId,
    /// Offset of the task's latest event.
    pub last_offset: u64,
    /// Time of the task's latest event.
    pub last_at: Timestamp,
    /// Offset of the latest event a person would want to see (0 = none).
    pub visible_offset: u64,
    /// Distinct files the task's tool calls changed.
    pub files_changed: u32,
    /// Lines added across those edits (cumulative).
    pub lines_added: u32,
    /// Lines removed across those edits (cumulative).
    pub lines_removed: u32,
    /// Time of the latest committed checkpoint.
    pub last_checkpoint_at: Option<Timestamp>,
    /// Highest log offset a person marked as read (0 = never).
    pub read_offset: u64,
    /// Whether the conversation is archived (the latest of archive and
    /// unarchive wins).
    pub archived: bool,
    /// Input tokens of the latest model invocation.
    pub context_tokens: Option<u64>,
    /// Endpoint and model of the latest model invocation.
    pub model: Option<(String, String)>,
    /// Offset of the first event of the task's own aggregate (its creation).
    pub created_offset: u64,
    /// Events that can raise an attention item (a needs-attention record, a
    /// question, a refused capacity ticket, a child's refusal): a task with
    /// none cannot be in need of anyone, so nothing about it is read to find
    /// out.
    pub attention_events: u32,
    /// Plan versions recorded on the task (`PlanRecorded` and `PlanRevised`).
    pub plan_versions: u32,
    /// The mode the task was last set to (`TaskModeSet`), as the log spells
    /// it (`AGENT`, `PLAN`, ...); `None` = never set, which is the default
    /// mode. A task in `PLAN` mode with a plan recorded is waiting for the
    /// person to accept the plan (leaving PLAN is the acceptance).
    pub mode: Option<String>,
}

/// Digest every task of `session` that has at least one event.
pub(crate) fn task_digests(conn: &Connection, session: &SessionId) -> Result<Vec<TaskDigest>> {
    let sid = session.as_bytes().as_slice();
    let mut stmt = conn.prepare_cached(&format!(
        "SELECT task_id,
                MAX(offset),
                MAX(occurred_at),
                COALESCE(MAX(CASE WHEN event_type IN ({VISIBLE}) THEN offset END), 0),
                COUNT(DISTINCT CASE WHEN event_type = 'FileChanged' THEN json_extract(payload_inline, '$.path') END),
                COALESCE(SUM(CASE WHEN event_type = 'FileChanged' THEN COALESCE(json_extract(payload_inline, '$.lines_added'), 0) ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN event_type = 'FileChanged' THEN COALESCE(json_extract(payload_inline, '$.lines_removed'), 0) ELSE 0 END), 0),
                MAX(CASE WHEN event_type = 'CheckpointCommitted' THEN occurred_at END),
                COALESCE(MAX(CASE WHEN event_type = 'ConversationRead' THEN json_extract(payload_inline, '$.up_to_offset') END), 0),
                COALESCE(MAX(CASE WHEN event_type = 'ConversationArchived' THEN offset END), 0),
                COALESCE(MAX(CASE WHEN event_type = 'ConversationUnarchived' THEN offset END), 0),
                COALESCE(MIN(CASE WHEN event_type = 'TaskCreated' THEN offset END), 0),
                COALESCE(SUM(CASE WHEN event_type IN ({ATTENTION}) THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN event_type IN ('PlanRecorded','PlanRevised') THEN 1 ELSE 0 END), 0)
         FROM events
         WHERE session_id = ?1 AND task_id IS NOT NULL
         GROUP BY task_id
         ORDER BY MIN(offset)"
    ))?;
    let mut out: Vec<TaskDigest> = stmt
        .query_map(params![sid], |r| {
            let archived_at: i64 = r.get(9)?;
            let unarchived_at: i64 = r.get(10)?;
            Ok(TaskDigest {
                task_id: TaskId::from_bytes(
                    r.get::<_, Vec<u8>>(0)?
                        .try_into()
                        .map_err(|_| rusqlite::Error::InvalidQuery)?,
                ),
                last_offset: r.get::<_, i64>(1)? as u64,
                last_at: Timestamp(r.get(2)?),
                visible_offset: r.get::<_, i64>(3)? as u64,
                files_changed: u32::try_from(r.get::<_, i64>(4)?).unwrap_or(u32::MAX),
                lines_added: u32::try_from(r.get::<_, i64>(5)?).unwrap_or(u32::MAX),
                lines_removed: u32::try_from(r.get::<_, i64>(6)?).unwrap_or(u32::MAX),
                last_checkpoint_at: r.get::<_, Option<i64>>(7)?.map(Timestamp),
                read_offset: r.get::<_, i64>(8)? as u64,
                archived: archived_at > unarchived_at,
                context_tokens: None,
                model: None,
                created_offset: r.get::<_, i64>(11)? as u64,
                attention_events: u32::try_from(r.get::<_, i64>(12)?).unwrap_or(u32::MAX),
                plan_versions: u32::try_from(r.get::<_, i64>(13)?).unwrap_or(u32::MAX),
                mode: None,
            })
        })?
        .collect::<std::result::Result<_, _>>()?;

    // The latest invocation of each task: its input tokens and its route.
    let index: std::collections::HashMap<[u8; 16], usize> = out
        .iter()
        .enumerate()
        .map(|(i, d)| (*d.task_id.as_bytes(), i))
        .collect();
    for (task, tokens, _, _) in latest(conn, sid, "ModelUsageRecorded")? {
        if let Some(&i) = index.get(&task) {
            out[i].context_tokens = tokens.map(|t| t.max(0) as u64);
        }
    }
    for (task, _, endpoint, model) in latest(conn, sid, "ModelInvocationStarted")? {
        if let Some(&i) = index.get(&task) {
            out[i].model = endpoint.zip(model);
        }
    }
    // The mode the person last set, from the latest `TaskModeSet` of each task.
    for (task, mode) in latest_modes(conn, sid)? {
        if let Some(&i) = index.get(&task) {
            out[i].mode = Some(mode);
        }
    }
    Ok(out)
}

/// Each task's latest `TaskModeSet` and the mode it set.
fn latest_modes(conn: &Connection, session: &[u8]) -> Result<Vec<([u8; 16], String)>> {
    let mut stmt = conn.prepare_cached(
        "SELECT task_id, json_extract(payload_inline, '$.mode')
         FROM (SELECT task_id, payload_inline,
                      ROW_NUMBER() OVER (PARTITION BY task_id ORDER BY offset DESC) AS rn
               FROM events
               WHERE session_id = ?1 AND task_id IS NOT NULL AND event_type = 'TaskModeSet')
         WHERE rn = 1",
    )?;
    let rows = stmt.query_map(params![session], |r| {
        Ok((
            r.get::<_, Vec<u8>>(0)?
                .try_into()
                .map_err(|_| rusqlite::Error::InvalidQuery)?,
            r.get::<_, Option<String>>(1)?,
        ))
    })?;
    Ok(rows
        .collect::<std::result::Result<Vec<_>, _>>()?
        .into_iter()
        .filter_map(|(t, m)| m.map(|m| (t, m)))
        .collect())
}

/// One task's latest event of a type: input tokens, endpoint and model, as
/// far as that payload carries them.
type Latest = ([u8; 16], Option<i64>, Option<String>, Option<String>);

fn latest(conn: &Connection, session: &[u8], event_type: &str) -> Result<Vec<Latest>> {
    let mut stmt = conn.prepare_cached(
        "SELECT task_id, json_extract(payload_inline, '$.input_tokens'),
                json_extract(payload_inline, '$.model_route.endpoint'),
                json_extract(payload_inline, '$.model_route.model')
         FROM (SELECT task_id, payload_inline,
                      ROW_NUMBER() OVER (PARTITION BY task_id ORDER BY offset DESC) AS rn
               FROM events
               WHERE session_id = ?1 AND task_id IS NOT NULL AND event_type = ?2)
         WHERE rn = 1",
    )?;
    let rows = stmt.query_map(params![session, event_type], |r| {
        Ok((
            r.get::<_, Vec<u8>>(0)?
                .try_into()
                .map_err(|_| rusqlite::Error::InvalidQuery)?,
            r.get::<_, Option<i64>>(1)?,
            r.get::<_, Option<String>>(2)?,
            r.get::<_, Option<String>>(3)?,
        ))
    })?;
    Ok(rows.collect::<std::result::Result<_, _>>()?)
}
