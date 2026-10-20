//! Projects over the WorkGraph (PX-063; docs/65 AFW-J01, AFW-J03, AFW-J04).
//!
//! A project is an event-sourced record (`modbit_domain::project`): a name, a
//! palette colour and an icon, the workspace it is bound to, an archived flag
//! and a membership map of top-level tasks. It is a grouping and nothing
//! more: no coordinator, no scheduler, no task owned. The commands here
//! validate against the log, then append one event on the project's own
//! aggregate under the command record, so a retry replays and a Core killed
//! at any point leaves the old or the new membership, never a mixture (one
//! transaction carries the guard-checked event and its projection).
//!
//! The reads answer from the projection tables and the agent headers the list
//! already uses: counts by status class, the attention rollup, and the pull
//! request and CI facts the log holds. Nothing is estimated and no client
//! total is trusted.

use std::collections::HashMap;

use modbit_domain::event::{Actor, AggregateType};
use modbit_domain::project::{self, ProjectEvent};
use modbit_domain::task::Task;
use modbit_domain::{ProjectId, SessionId, TaskId};
use modbit_event_store::{
    AppendRequest, CommandOutcome, CommandRecord, EventStore, ProjectMemberRow, ProjectRow,
};
use modbit_protocol::v1 as wire;
use prost::Message;

use crate::server::{Core, accept, error_code, id16, reject, require_lease, typed, wire_id};

type Failure = (String, String);

fn fail(code: &str, detail: impl Into<String>) -> Failure {
    (code.to_owned(), detail.into())
}

fn from_domain((code, detail): project::Refusal) -> Failure {
    (code.to_owned(), detail)
}

fn store_err(e: &modbit_event_store::Error) -> Failure {
    (error_code(e).to_owned(), e.to_string())
}

/// A workspace root as projects compare it: canonical when it exists, with no
/// Windows verbatim prefix.
fn canonical_root(root: &str) -> String {
    let root = root.trim();
    std::path::Path::new(root)
        .canonicalize()
        .map(|p| crate::worktrees::plain_path(&p))
        .unwrap_or_else(|_| root.trim_start_matches(r"\\?\").to_owned())
}

fn project_id_of(id: Option<&wire::Id>) -> Result<ProjectId, Failure> {
    id.and_then(id16)
        .map(ProjectId::from_bytes)
        .ok_or_else(|| fail("BAD_PAYLOAD", "project_id required"))
}

fn task_id_of(id: Option<&wire::Id>) -> Result<TaskId, Failure> {
    id.and_then(id16)
        .map(TaskId::from_bytes)
        .ok_or_else(|| fail("BAD_PAYLOAD", "task_id required"))
}

fn session_of(id: Option<&wire::Id>) -> Result<SessionId, Failure> {
    id.and_then(id16)
        .map(SessionId::from_bytes)
        .ok_or_else(|| fail("BAD_PAYLOAD", "session_id required"))
}

// ---------------------------------------------------------------- the reads

/// The project and task facts a view is made from, loaded once per call.
struct Reader<'a> {
    core: &'a Core,
    store: &'a EventStore,
    /// Headers by session, loaded on first use (archived included).
    headers: HashMap<SessionId, HashMap<TaskId, wire::AgentHeader>>,
}

impl<'a> Reader<'a> {
    fn new(core: &'a Core, store: &'a EventStore) -> Self {
        Self {
            core,
            store,
            headers: HashMap::new(),
        }
    }

    fn header(&mut self, session: SessionId, task: TaskId) -> Option<wire::AgentHeader> {
        if !self.headers.contains_key(&session) {
            let map = crate::transcript::headers(self.core, self.store, session, true)
                .map(|h| {
                    h.headers
                        .into_iter()
                        .filter_map(|h| {
                            let id = h.task_id.as_ref().and_then(id16).map(TaskId::from_bytes)?;
                            Some((id, h))
                        })
                        .collect()
                })
                .unwrap_or_default();
            self.headers.insert(session, map);
        }
        self.headers.get(&session)?.get(&task).cloned()
    }

    fn view(&mut self, row: &ProjectRow) -> Result<wire::ProjectView, Failure> {
        let members = self
            .store
            .project_members(&row.project_id)
            .map_err(|e| store_err(&e))?;
        let mut views = Vec::with_capacity(members.len());
        for m in &members {
            if let Some(v) = self.member_view(m) {
                views.push(v);
            }
        }
        Ok(wire::ProjectView {
            project_id: Some(wire_id(row.project_id.as_bytes())),
            name: row.name.clone(),
            color: row.color.clone(),
            icon: row.icon.clone(),
            workspace_root: row.workspace_root.clone(),
            archived: row.archived,
            created_at_ms: row.created_at_ms,
            updated_at_ms: row.updated_at_ms,
            created_offset: row.created_offset,
            last_offset: row.last_offset,
            rollup: Some(rollup_of(&views)),
            members: views,
        })
    }

    fn member_view(&mut self, m: &ProjectMemberRow) -> Option<wire::ProjectMemberView> {
        let task = self.store.task(&m.task_id).ok().flatten()?;
        let h = self.header(task.session_id, m.task_id)?;
        let ts = h
            .updated_at
            .as_ref()
            .map_or(0, |t| t.seconds * 1000 + i64::from(t.nanos) / 1_000_000);
        Some(wire::ProjectMemberView {
            task_id: h.task_id.clone(),
            session_id: h.session_id.clone(),
            title: h.title.clone(),
            status_class: h.status_class,
            status_label: h.status_label.clone(),
            unread: h.unread,
            pending_approval: h.pending_approval,
            attention_items: h.attention_items,
            task_state: h.task_state.clone(),
            archived: h.archived,
            added_at_ms: m.added_at_ms,
            updated_at_ms: ts,
            pull_request: pull_request_of(self.store, m.task_id),
        })
    }
}

/// The pull request the log holds for a task, with the CI the log recorded.
fn pull_request_of(store: &EventStore, task: TaskId) -> Option<wire::ProjectPullRequestView> {
    let events = store
        .read_aggregate_of_types(
            task.as_bytes(),
            &[
                "ForgePullRequestOpened",
                "ForgePullRequestUpdated",
                "CiEvidenceRecorded",
            ],
            0,
        )
        .unwrap_or_default();
    let mut pr: Option<wire::ProjectPullRequestView> = None;
    let mut ci: Option<(u32, u32, u32)> = None;
    for e in &events {
        let Ok(p) = store.payload(&e.envelope) else {
            continue;
        };
        match e.envelope.event_type.as_str() {
            "ForgePullRequestOpened" => {
                pr = Some(wire::ProjectPullRequestView {
                    number: p["number"].as_u64().unwrap_or(0),
                    url: p["url"].as_str().unwrap_or_default().to_owned(),
                    head: p["head"].as_str().unwrap_or_default().to_owned(),
                    ..Default::default()
                });
                ci = None;
            }
            "ForgePullRequestUpdated" => {
                if let Some(v) = pr.as_mut() {
                    v.state = p["state"].as_str().unwrap_or_default().to_owned();
                }
            }
            "CiEvidenceRecorded" => {
                let (mut ok, mut bad, mut wait) = (0u32, 0u32, 0u32);
                for c in p["checks"].as_array().into_iter().flatten() {
                    let status = c["status"].as_str().unwrap_or_default();
                    let conclusion = c["conclusion"].as_str().unwrap_or_default();
                    if status != "completed" {
                        wait += 1;
                    } else if matches!(conclusion, "success" | "neutral" | "skipped") {
                        ok += 1;
                    } else {
                        bad += 1;
                    }
                }
                ci = Some((ok, bad, wait));
            }
            _ => {}
        }
    }
    let mut pr = pr?;
    if let Some((ok, bad, wait)) = ci {
        pr.has_ci = true;
        pr.checks_passed = ok;
        pr.checks_failed = bad;
        pr.checks_pending = wait;
    }
    Some(pr)
}

fn rollup_of(members: &[wire::ProjectMemberView]) -> wire::ProjectRollup {
    let mut by_class: std::collections::BTreeMap<i32, u32> = std::collections::BTreeMap::new();
    let mut r = wire::ProjectRollup {
        members: u32::try_from(members.len()).unwrap_or(u32::MAX),
        ..Default::default()
    };
    for m in members {
        *by_class.entry(m.status_class).or_default() += 1;
        if m.status_class == wire::AgentStatusClass::NeedsAttention as i32 {
            r.attention_tasks += 1;
        }
        r.attention_items += m.attention_items;
        r.pending_approvals += u32::from(m.pending_approval);
        r.unread += u32::from(m.unread);
        if let Some(pr) = &m.pull_request {
            r.pull_requests += 1;
            if pr.has_ci {
                if pr.checks_failed > 0 {
                    r.ci_failing += 1;
                } else if pr.checks_pending > 0 {
                    r.ci_pending += 1;
                } else if pr.checks_passed > 0 {
                    r.ci_passing += 1;
                }
            }
        }
    }
    r.by_status = by_class
        .into_iter()
        .map(|(class, count)| wire::ProjectStatusCount {
            status_class: class,
            label: wire::AgentStatusClass::try_from(class)
                .map(|c| status_words(c).to_owned())
                .unwrap_or_default(),
            count,
        })
        .collect();
    r
}

fn status_words(c: wire::AgentStatusClass) -> &'static str {
    use wire::AgentStatusClass as C;
    match c {
        C::NeedsAttention => "Needs attention",
        C::Failed => "Failed",
        C::ReadyForReviewUnseen => "Ready for review",
        C::ReadyForReviewSeen => "Ready for review (seen)",
        C::Running => "Running",
        C::Waiting => "Waiting",
        C::Completed => "Completed",
        C::Draft => "Draft",
        C::Archived => "Archived",
        C::Unspecified => "",
    }
}

fn changed(
    core: &Core,
    store: &EventStore,
    id: ProjectId,
    offset: u64,
) -> Result<Vec<u8>, Failure> {
    let row = store
        .project(&id)
        .map_err(|e| store_err(&e))?
        .ok_or_else(|| fail("UNKNOWN_PROJECT", id.to_string()))?;
    let view = Reader::new(core, store).view(&row)?;
    Ok(wire::ProjectChanged {
        project: Some(view),
        offset,
    }
    .encode_to_vec())
}

// -------------------------------------------------------------- the commands

/// Dispatch one project command or read. The caller has decoded nothing yet.
pub(crate) async fn handle(
    core: &Core,
    env: &wire::CommandEnvelope,
    record: CommandRecord,
    actor: Actor,
) -> wire::CommandAck {
    let cid = env.command_id.clone();
    match run(core, env, record, actor).await {
        Ok((replayed, result)) => accept(cid, replayed, result),
        Err((code, detail)) => reject(cid, &code, detail),
    }
}

async fn run(
    core: &Core,
    env: &wire::CommandEnvelope,
    record: CommandRecord,
    actor: Actor,
) -> Result<(bool, Vec<u8>), Failure> {
    let bad = |what: &str| fail("BAD_PAYLOAD", what.to_owned());
    match env.command_type.as_str() {
        "ListProjects" => {
            let p = wire::ListProjects::decode(env.payload.as_slice())
                .map_err(|_| bad("ListProjects"))?;
            let store = core.store.lock().await;
            let want =
                (!p.workspace_root.trim().is_empty()).then(|| canonical_root(&p.workspace_root));
            let mut reader = Reader::new(core, &store);
            let mut out = Vec::new();
            for row in store.projects().map_err(|e| store_err(&e))? {
                if row.archived && !p.include_archived {
                    continue;
                }
                if want
                    .as_deref()
                    .is_some_and(|w| !crate::worktrees::same_path(w, &row.workspace_root))
                {
                    continue;
                }
                out.push(reader.view(&row)?);
            }
            Ok((
                false,
                wire::ProjectList {
                    projects: out,
                    last_offset: store.last_offset().unwrap_or(0),
                }
                .encode_to_vec(),
            ))
        }
        "GetProject" => {
            let p =
                wire::GetProject::decode(env.payload.as_slice()).map_err(|_| bad("GetProject"))?;
            let id = project_id_of(p.project_id.as_ref())?;
            let store = core.store.lock().await;
            let offset = store
                .project(&id)
                .ok()
                .flatten()
                .map_or(0, |r| r.last_offset);
            Ok((false, changed(core, &store, id, offset)?))
        }
        "CreateProject" => {
            let p = wire::CreateProject::decode(env.payload.as_slice())
                .map_err(|_| bad("CreateProject"))?;
            let session = session_of(p.session_id.as_ref())?;
            fence(core, env, &session).await?;
            create(core, env, record, actor, session, &p).await
        }
        "RenameProject" => {
            let p = wire::RenameProject::decode(env.payload.as_slice())
                .map_err(|_| bad("RenameProject"))?;
            let session = session_of(p.session_id.as_ref())?;
            let id = project_id_of(p.project_id.as_ref())?;
            fence(core, env, &session).await?;
            rename(core, record, actor, session, id, &p).await
        }
        "ArchiveProject" => {
            let p = wire::ArchiveProject::decode(env.payload.as_slice())
                .map_err(|_| bad("ArchiveProject"))?;
            let session = session_of(p.session_id.as_ref())?;
            let id = project_id_of(p.project_id.as_ref())?;
            fence(core, env, &session).await?;
            archive(core, record, actor, session, id, p.archived).await
        }
        "AddProjectMember" => {
            let p = wire::AddProjectMember::decode(env.payload.as_slice())
                .map_err(|_| bad("AddProjectMember"))?;
            let session = session_of(p.session_id.as_ref())?;
            let id = project_id_of(p.project_id.as_ref())?;
            let task = task_id_of(p.task_id.as_ref())?;
            fence(core, env, &session).await?;
            membership(core, record, actor, session, id, task, true).await
        }
        "RemoveProjectMember" => {
            let p = wire::RemoveProjectMember::decode(env.payload.as_slice())
                .map_err(|_| bad("RemoveProjectMember"))?;
            let session = session_of(p.session_id.as_ref())?;
            let id = project_id_of(p.project_id.as_ref())?;
            let task = task_id_of(p.task_id.as_ref())?;
            fence(core, env, &session).await?;
            membership(core, record, actor, session, id, task, false).await
        }
        other => Err(fail("UNKNOWN_COMMAND", other.to_owned())),
    }
}

/// The acting session's lease fences every project command.
async fn fence(
    core: &Core,
    env: &wire::CommandEnvelope,
    session: &SessionId,
) -> Result<(), Failure> {
    require_lease(core, &env.command_id, env, session)
        .await
        .map_err(|ack| (ack.error_code, ack.error_message))
}

/// What an earlier execution of this command recorded, answered as it was.
fn replay(
    core: &Core,
    store: &EventStore,
    record: &CommandRecord,
    id: ProjectId,
) -> Result<Option<Vec<u8>>, Failure> {
    match store.prior_command(record).map_err(|e| store_err(&e))? {
        Some(CommandOutcome::Replayed(events) | CommandOutcome::Applied(events)) => {
            let offset = events.last().map_or(0, |e| e.offset);
            Ok(Some(changed(core, store, id, offset)?))
        }
        None => Ok(None),
    }
}

/// Kill the Core at a named point, as a `SIGKILL` there would
/// (`MODBIT_FAULT_PROJECT=before-append|after-append`; unset in production).
/// A project command is one transaction, so a kill leaves the old state
/// (`before-append`) or the new one with the command recorded
/// (`after-append`); a retry of the same command id then replays.
fn fault_point(point: &str) {
    if std::env::var("MODBIT_FAULT_PROJECT").is_ok_and(|v| v == point) {
        eprintln!("modbit-core: fault injection: aborting at {point}");
        std::process::abort();
    }
}

fn append(
    core: &Core,
    store: &mut EventStore,
    record: CommandRecord,
    actor: Actor,
    session: SessionId,
    id: ProjectId,
    event: &ProjectEvent,
) -> Result<(bool, Vec<u8>), Failure> {
    let req = AppendRequest {
        tenant_id: core.tenant_id,
        session_id: session,
        task_id: None,
        run_id: None,
        turn_id: None,
        step_id: None,
        aggregate_type: AggregateType::Project,
        aggregate_id: *id.as_bytes(),
        expected_sequence: None,
        events: vec![typed(event.event_type(), event, actor)],
    };
    fault_point("before-append");
    let outcome = store
        .execute_command(record, req)
        .map_err(|e| store_err(&e))?;
    fault_point("after-append");
    let (events, replayed) = crate::server::split(outcome);
    let offset = events.last().map_or(0, |e| e.offset);
    if !replayed {
        core.last_offset.send_replace(offset);
    }
    Ok((replayed, changed(core, store, id, offset)?))
}

/// Live (unarchived) projects of a workspace, optionally without one.
fn live_in(
    store: &EventStore,
    root: &str,
    except: Option<ProjectId>,
) -> Result<Vec<ProjectRow>, Failure> {
    Ok(store
        .projects()
        .map_err(|e| store_err(&e))?
        .into_iter()
        .filter(|r| !r.archived && Some(r.project_id) != except)
        .filter(|r| crate::worktrees::same_path(&r.workspace_root, root))
        .collect())
}

fn check_unique(
    store: &EventStore,
    root: &str,
    name: &str,
    except: Option<ProjectId>,
) -> Result<(), Failure> {
    let key = project::name_key(name);
    if live_in(store, root, except)?
        .iter()
        .any(|r| project::name_key(&r.name) == key)
    {
        return Err(fail(
            "PROJECT_NAME_TAKEN",
            format!("this workspace already has a project named `{name}`"),
        ));
    }
    Ok(())
}

async fn create(
    core: &Core,
    env: &wire::CommandEnvelope,
    record: CommandRecord,
    actor: Actor,
    session: SessionId,
    p: &wire::CreateProject,
) -> Result<(bool, Vec<u8>), Failure> {
    // The project id derives from the command id, so a replay names the same project.
    let id = env
        .command_id
        .as_ref()
        .and_then(id16)
        .map(ProjectId::from_bytes)
        .ok_or_else(|| fail("BAD_COMMAND_ID", "command_id must be 16 bytes"))?;
    let mut store = core.store.lock().await;
    if let Some(r) = replay(core, &store, &record, id)? {
        return Ok((true, r));
    }
    let name = project::check_name(&p.name).map_err(from_domain)?;
    let color = project::check_color(&p.color, None).map_err(from_domain)?;
    let icon = project::check_icon(&p.icon, None).map_err(from_domain)?;
    if p.workspace_root.trim().is_empty() {
        return Err(fail("BAD_PAYLOAD", "workspace_root required"));
    }
    let root = canonical_root(&p.workspace_root);
    check_unique(&store, &root, &name, None)?;
    if live_in(&store, &root, None)?.len() >= project::MAX_LIVE_PROJECTS_PER_WORKSPACE {
        return Err(fail(
            "PROJECT_LIMIT",
            format!(
                "a workspace holds at most {} live projects; archive one first",
                project::MAX_LIVE_PROJECTS_PER_WORKSPACE
            ),
        ));
    }
    let event = ProjectEvent::ProjectCreated {
        project_id: id,
        name,
        color,
        icon,
        workspace_root: root,
    };
    append(core, &mut store, record, actor, session, id, &event)
}

fn existing(store: &EventStore, id: ProjectId) -> Result<ProjectRow, Failure> {
    store
        .project(&id)
        .map_err(|e| store_err(&e))?
        .ok_or_else(|| fail("UNKNOWN_PROJECT", id.to_string()))
}

async fn rename(
    core: &Core,
    record: CommandRecord,
    actor: Actor,
    session: SessionId,
    id: ProjectId,
    p: &wire::RenameProject,
) -> Result<(bool, Vec<u8>), Failure> {
    let mut store = core.store.lock().await;
    if let Some(r) = replay(core, &store, &record, id)? {
        return Ok((true, r));
    }
    let row = existing(&store, id)?;
    if row.archived {
        return Err(from_domain((
            "PROJECT_ARCHIVED",
            format!("`{}` is archived; restore it before changing it", row.name),
        )));
    }
    let name = if p.name.trim().is_empty() {
        row.name.clone()
    } else {
        project::check_name(&p.name).map_err(from_domain)?
    };
    let color = project::check_color(&p.color, Some(&row.color)).map_err(from_domain)?;
    let icon = project::check_icon(&p.icon, Some(&row.icon)).map_err(from_domain)?;
    if name == row.name && color == row.color && icon == row.icon {
        return Ok((false, changed(core, &store, id, 0)?));
    }
    check_unique(&store, &row.workspace_root, &name, Some(id))?;
    let event = ProjectEvent::ProjectRenamed {
        project_id: id,
        name,
        color,
        icon,
    };
    append(core, &mut store, record, actor, session, id, &event)
}

async fn archive(
    core: &Core,
    record: CommandRecord,
    actor: Actor,
    session: SessionId,
    id: ProjectId,
    want: bool,
) -> Result<(bool, Vec<u8>), Failure> {
    let mut store = core.store.lock().await;
    if let Some(r) = replay(core, &store, &record, id)? {
        return Ok((true, r));
    }
    let row = existing(&store, id)?;
    if row.archived == want {
        return Ok((false, changed(core, &store, id, 0)?));
    }
    let event = if want {
        ProjectEvent::ProjectArchived { project_id: id }
    } else {
        // Coming back must not collide with a project made in its absence.
        check_unique(&store, &row.workspace_root, &row.name, Some(id))?;
        ProjectEvent::ProjectUnarchived { project_id: id }
    };
    append(core, &mut store, record, actor, session, id, &event)
}

/// The workspace a task belongs to as a project sees it: the checkout a
/// worktree task was made from, else the root it works in.
fn workspace_of(store: &EventStore, task: &Task) -> Option<String> {
    let root = task.workspace_root.as_deref()?;
    Some(
        crate::worktrees::origin_of(store, task.session_id, root)
            .unwrap_or_else(|| root.to_owned()),
    )
}

async fn membership(
    core: &Core,
    record: CommandRecord,
    actor: Actor,
    session: SessionId,
    id: ProjectId,
    task_id: TaskId,
    add: bool,
) -> Result<(bool, Vec<u8>), Failure> {
    let mut store = core.store.lock().await;
    if let Some(r) = replay(core, &store, &record, id)? {
        return Ok((true, r));
    }
    let row = existing(&store, id)?;
    let held = store.project_of_task(&task_id).map_err(|e| store_err(&e))?;
    if !add {
        if row.archived {
            return Err(from_domain((
                "PROJECT_ARCHIVED",
                format!("`{}` is archived; restore it before changing it", row.name),
            )));
        }
        match held {
            None => return Ok((false, changed(core, &store, id, 0)?)),
            Some(h) if h.project_id != id => {
                let other = store
                    .project(&h.project_id)
                    .ok()
                    .flatten()
                    .map_or_else(|| h.project_id.to_string(), |r| r.name);
                return Err(fail(
                    "NOT_A_MEMBER",
                    format!("the task is in `{other}`, not in `{}`", row.name),
                ));
            }
            Some(_) => {}
        }
        let event = ProjectEvent::ProjectMemberRemoved {
            project_id: id,
            task_id,
        };
        return append(core, &mut store, record, actor, session, id, &event);
    }
    let task = store
        .task(&task_id)
        .map_err(|e| store_err(&e))?
        .ok_or_else(|| fail("UNKNOWN_TASK", task_id.to_string()))?;
    let archived = store
        .task_digests(&task.session_id)
        .map_err(|e| store_err(&e))?
        .into_iter()
        .find(|d| d.task_id == task_id)
        .is_some_and(|d| d.archived);
    let root = workspace_of(&store, &task);
    // The workspace guard compares the way the rest of the Core does.
    let same = root
        .as_deref()
        .is_some_and(|r| crate::worktrees::same_path(r, &row.workspace_root));
    let candidate = project::Candidate {
        task_id,
        origin: task.origin,
        state: task.state,
        archived,
        workspace_root: if same {
            Some(row.workspace_root.as_str())
        } else {
            root.as_deref()
        },
    };
    let members = store.project_members(&id).map_err(|e| store_err(&e))?.len();
    let held_name = held.as_ref().map(|h| {
        let name = store
            .project(&h.project_id)
            .ok()
            .flatten()
            .map_or_else(String::new, |r| r.name);
        (h.project_id, name)
    });
    let target = project::Target {
        project_id: id,
        name: &row.name,
        workspace_root: &row.workspace_root,
        archived: row.archived,
        members,
    };
    project::check_add(
        &target,
        &candidate,
        held_name.as_ref().map(|(p, n)| (*p, n.as_str())),
    )
    .map_err(from_domain)?;
    if held.is_some_and(|h| h.project_id == id) {
        // Already a member: the answer is the project as it is.
        return Ok((false, changed(core, &store, id, 0)?));
    }
    let event = ProjectEvent::ProjectMemberAdded {
        project_id: id,
        task_id,
    };
    append(core, &mut store, record, actor, session, id, &event)
}

/// The project holding each task, for the agent headers.
pub(crate) fn memberships(store: &EventStore) -> HashMap<TaskId, ProjectId> {
    store
        .project_memberships()
        .unwrap_or_default()
        .into_iter()
        .map(|m| (m.task_id, m.project_id))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use modbit_domain::task::{CiCheckRecord, TaskEvent, TaskOrigin};
    use modbit_domain::{TenantId, WorkspaceId};

    fn view(
        class: wire::AgentStatusClass,
        attention: u32,
        pr: Option<wire::ProjectPullRequestView>,
    ) -> wire::ProjectMemberView {
        wire::ProjectMemberView {
            status_class: class as i32,
            attention_items: attention,
            pull_request: pr,
            ..Default::default()
        }
    }

    fn pr(has_ci: bool, ok: u32, bad: u32, wait: u32) -> wire::ProjectPullRequestView {
        wire::ProjectPullRequestView {
            number: 1,
            has_ci,
            checks_passed: ok,
            checks_failed: bad,
            checks_pending: wait,
            ..Default::default()
        }
    }

    #[test]
    fn the_rollup_counts_only_what_the_members_say() {
        use wire::AgentStatusClass as C;
        let members = vec![
            view(C::NeedsAttention, 2, Some(pr(true, 3, 1, 0))),
            view(C::Running, 0, Some(pr(true, 2, 0, 1))),
            view(C::Completed, 0, Some(pr(true, 4, 0, 0))),
            view(C::Completed, 0, Some(pr(false, 0, 0, 0))),
            view(C::Draft, 0, None),
        ];
        let r = rollup_of(&members);
        assert_eq!((r.members, r.attention_tasks, r.attention_items), (5, 1, 2));
        // A pull request with no CI evidence is counted as a pull request and as nothing about CI.
        assert_eq!(
            (r.pull_requests, r.ci_passing, r.ci_failing, r.ci_pending),
            (4, 1, 1, 1)
        );
        let by: Vec<(i32, u32)> = r
            .by_status
            .iter()
            .map(|s| (s.status_class, s.count))
            .collect();
        assert_eq!(
            by,
            vec![
                (C::NeedsAttention as i32, 1),
                (C::Running as i32, 1),
                (C::Completed as i32, 2),
                (C::Draft as i32, 1)
            ],
            "classes in the list's order, only those that exist"
        );
        assert_eq!(rollup_of(&[]).members, 0);
    }

    #[test]
    fn a_pull_request_and_its_ci_come_from_the_events_the_log_holds() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = EventStore::open(dir.path()).unwrap();
        let (tenant, session, task) = (TenantId::new(), SessionId::new(), TaskId::new());
        let put = |store: &mut EventStore, e: TaskEvent, seq: Option<u64>| {
            store
                .append(AppendRequest {
                    tenant_id: tenant,
                    session_id: session,
                    task_id: Some(task),
                    run_id: None,
                    turn_id: None,
                    step_id: None,
                    aggregate_type: AggregateType::Task,
                    aggregate_id: *task.as_bytes(),
                    expected_sequence: seq,
                    events: vec![typed(e.event_type(), &e, Actor::Core("t".into()))],
                })
                .unwrap();
        };
        store
            .append(AppendRequest {
                tenant_id: tenant,
                session_id: session,
                task_id: None,
                run_id: None,
                turn_id: None,
                step_id: None,
                aggregate_type: AggregateType::Session,
                aggregate_id: *session.as_bytes(),
                expected_sequence: Some(0),
                events: vec![typed(
                    "SessionCreated",
                    &modbit_domain::session::SessionEvent::SessionCreated {
                        tenant_id: tenant,
                        user_id: modbit_domain::UserId::new(),
                        space_id: modbit_domain::SpaceId::new(),
                    },
                    Actor::Core("t".into()),
                )],
            })
            .unwrap();
        assert!(
            pull_request_of(&store, task).is_none(),
            "no task, no pull request"
        );
        put(
            &mut store,
            TaskEvent::TaskCreated {
                session_id: session,
                goal_text: "g".into(),
                workspace_id: WorkspaceId::new(),
                workspace_root: Some("/w".into()),
                base_revision: None,
                execution_profile: "local_trusted".into(),
                policy_profile_id: None,
                origin: TaskOrigin::Cli,
            },
            Some(0),
        );
        assert!(
            pull_request_of(&store, task).is_none(),
            "a task with no pull request has none"
        );
        put(
            &mut store,
            TaskEvent::ForgePullRequestOpened {
                idempotency_key: "k".into(),
                owner: "o".into(),
                repo: "r".into(),
                number: 42,
                url: "https://example.test/o/r/pull/42".into(),
                head: "task/x".into(),
                head_sha: "abc".into(),
                base: "main".into(),
                result: serde_json::json!({}),
            },
            None,
        );
        let v = pull_request_of(&store, task).unwrap();
        assert_eq!(
            (v.number, v.has_ci),
            (42, false),
            "no CI evidence recorded, none claimed"
        );
        let check = |name: &str, status: &str, conclusion: &str| CiCheckRecord {
            name: name.into(),
            run_id: 1,
            status: status.into(),
            conclusion: conclusion.into(),
            url: String::new(),
            completed_at: String::new(),
            log_ref: String::new(),
            log_truncated: false,
        };
        put(
            &mut store,
            TaskEvent::CiEvidenceRecorded {
                provider: "github".into(),
                owner: "o".into(),
                repo: "r".into(),
                pull_number: 42,
                commit: "abc".into(),
                checks: vec![
                    check("a", "completed", "success"),
                    check("b", "completed", "failure"),
                    check("c", "in_progress", ""),
                ],
                rejected: vec![],
                tool_call_id: "t".into(),
                provenance: "ci".into(),
            },
            None,
        );
        let v = pull_request_of(&store, task).unwrap();
        assert!(v.has_ci);
        assert_eq!(
            (v.checks_passed, v.checks_failed, v.checks_pending),
            (1, 1, 1)
        );
    }
}
