//! PX-063 against the real Core process over its real socket, a real SQLite
//! event store and real Git repositories: project records and membership over
//! the WorkGraph. The model is the repository's scripted OpenAI-compatible
//! stand-in, used only to bring tasks to a state worth grouping; what is under
//! test is the Core's own commands, guards, projection and recovery.

mod px_common;

use std::path::Path;

use modbit_protocol::client::{Client, ClientError};
use modbit_protocol::v1::{
    AddProjectMember, AgentHeaders, AgentStatusClass, ApprovalResolvedAck, ArchiveProject,
    CancelTask, CommandStatus, CreateProject, CreateTask, GetAgentHeaders, GetProject, Id,
    InvokeTool, ListProjects, ListWorktrees, ProjectChanged, ProjectList, ProjectView,
    RemoveProjectMember, RemoveWorktree, RenameProject, ResolveApproval, SessionLeaseAcquired,
    TaskCreated, TaskIsolation, ToolInvoked, WorktreeList, WorktreeRemoval,
};
use prost::Message;
use px_common::{
    CoreProcess, create_task, envelope_fenced, id16, model_env, plain_repo, rand_id,
    scripted_model, session_with_lease, start_task, wait_task,
};
use serde_json::json;

type Reject = (String, String);

fn complete_script() -> Vec<serde_json::Value> {
    vec![
        json!({"calls": [{"name": "task.complete", "args": {"summary": "done", "self_review": {"findings": []}}}]}),
    ]
}

struct Fx {
    core: CoreProcess,
    c: Client,
    session: Id,
    g: Option<u64>,
    dir: tempfile::TempDir,
    env: Vec<(String, String)>,
    repos: Vec<(tempfile::TempDir, String)>,
}

impl Fx {
    /// A Core with the scripted model, a session with its lease and `n` repositories.
    async fn new(repos: usize, extra: &[(&str, &str)]) -> Self {
        let (base, _seen) = scripted_model(complete_script(), vec![]).await;
        let mut env = model_env(&base);
        env.push((
            "MODBIT_WORKTREE_CLEANUP_CATCHUP_MS".into(),
            "3600000".into(),
        ));
        env.push((
            "MODBIT_WORKTREE_CLEANUP_INTERVAL_MS".into(),
            "3600000".into(),
        ));
        for (k, v) in extra {
            env.push(((*k).into(), (*v).into()));
        }
        let dir = tempfile::tempdir().unwrap();
        let refs: Vec<(&str, &str)> = env.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
        let core = CoreProcess::spawn_with_env(dir.path(), &refs);
        let mut c = core.client().await;
        let (session, g) = session_with_lease(&mut c, 0x11).await;
        Self {
            core,
            c,
            session,
            g,
            dir,
            env,
            repos: (0..repos)
                .map(|i| plain_repo(&[("a.txt", &format!("alpha {i}\n"))]))
                .collect(),
        }
    }

    fn root(&self, i: usize) -> String {
        self.repos[i].1.clone()
    }

    /// Stop the Core as a `SIGKILL` would and start it again on the same data
    /// directory with `extra` environment; the session lease is taken again.
    async fn restart(&mut self, extra: &[(&str, &str)]) {
        self.core.kill();
        let mut env = self.env.clone();
        for (k, v) in extra {
            env.push(((*k).into(), (*v).into()));
        }
        let refs: Vec<(&str, &str)> = env.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
        self.core = CoreProcess::spawn_with_env(self.dir.path(), &refs);
        self.c = self.core.client().await;
        let ack = self
            .c
            .command(px_common::envelope(
                rand_id(),
                "AcquireSessionLease",
                modbit_protocol::v1::AcquireSessionLease {
                    session_id: Some(self.session.clone()),
                    owner: "test2".into(),
                }
                .encode_to_vec(),
            ))
            .await
            .unwrap();
        let l: SessionLeaseAcquired = Client::result(&ack).unwrap();
        self.g = Some(l.lease_generation);
    }

    async fn call<R: prost::Message + Default>(
        &mut self,
        id: Id,
        kind: &str,
        payload: Vec<u8>,
        fenced: bool,
    ) -> Result<(R, bool), Reject> {
        let g = if fenced { self.g } else { None };
        match self.c.command(envelope_fenced(id, kind, payload, g)).await {
            Ok(ack) => Ok((
                Client::result(&ack).unwrap(),
                ack.status == CommandStatus::Replayed as i32,
            )),
            Err(ClientError::Rejected { code, message }) => Err((code, message)),
            Err(e) => panic!("{kind}: {e}"),
        }
    }

    async fn create_project(&mut self, name: &str, root: &str) -> Result<ProjectView, Reject> {
        self.create_project_as(rand_id(), name, "", "", root)
            .await
            .map(|(v, _)| v)
    }

    async fn create_project_as(
        &mut self,
        id: Id,
        name: &str,
        color: &str,
        icon: &str,
        root: &str,
    ) -> Result<(ProjectView, bool), Reject> {
        let msg = CreateProject {
            session_id: Some(self.session.clone()),
            name: name.into(),
            color: color.into(),
            icon: icon.into(),
            workspace_root: root.into(),
        };
        let (r, replayed): (ProjectChanged, bool) = self
            .call(id, "CreateProject", msg.encode_to_vec(), true)
            .await?;
        Ok((r.project.unwrap(), replayed))
    }

    async fn rename(
        &mut self,
        p: &ProjectView,
        name: &str,
        color: &str,
        icon: &str,
    ) -> Result<ProjectChanged, Reject> {
        let msg = RenameProject {
            session_id: Some(self.session.clone()),
            project_id: p.project_id.clone(),
            name: name.into(),
            color: color.into(),
            icon: icon.into(),
        };
        self.call(rand_id(), "RenameProject", msg.encode_to_vec(), true)
            .await
            .map(|(r, _)| r)
    }

    async fn archive(&mut self, p: &ProjectView, archived: bool) -> Result<ProjectChanged, Reject> {
        let msg = ArchiveProject {
            session_id: Some(self.session.clone()),
            project_id: p.project_id.clone(),
            archived,
        };
        self.call(rand_id(), "ArchiveProject", msg.encode_to_vec(), true)
            .await
            .map(|(r, _)| r)
    }

    async fn add_as(
        &mut self,
        id: Id,
        p: &ProjectView,
        task: &Id,
    ) -> Result<(ProjectChanged, bool), Reject> {
        let msg = AddProjectMember {
            session_id: Some(self.session.clone()),
            project_id: p.project_id.clone(),
            task_id: Some(task.clone()),
        };
        self.call(id, "AddProjectMember", msg.encode_to_vec(), true)
            .await
    }

    async fn add(&mut self, p: &ProjectView, task: &Id) -> Result<ProjectChanged, Reject> {
        self.add_as(rand_id(), p, task).await.map(|(r, _)| r)
    }

    async fn remove(&mut self, p: &ProjectView, task: &Id) -> Result<ProjectChanged, Reject> {
        let msg = RemoveProjectMember {
            session_id: Some(self.session.clone()),
            project_id: p.project_id.clone(),
            task_id: Some(task.clone()),
        };
        self.call(rand_id(), "RemoveProjectMember", msg.encode_to_vec(), true)
            .await
            .map(|(r, _)| r)
    }

    async fn get(&mut self, p: &ProjectView) -> ProjectView {
        let msg = GetProject {
            project_id: p.project_id.clone(),
        };
        let (r, _): (ProjectChanged, bool) = self
            .call(rand_id(), "GetProject", msg.encode_to_vec(), false)
            .await
            .unwrap();
        r.project.unwrap()
    }

    async fn list(&mut self, include_archived: bool) -> ProjectList {
        let msg = ListProjects {
            workspace_root: String::new(),
            include_archived,
        };
        let (r, _) = self
            .call(rand_id(), "ListProjects", msg.encode_to_vec(), false)
            .await
            .unwrap();
        r
    }

    async fn headers(&mut self) -> AgentHeaders {
        let msg = GetAgentHeaders {
            session_id: Some(self.session.clone()),
            include_archived: true,
        };
        let (r, _) = self
            .call(rand_id(), "GetAgentHeaders", msg.encode_to_vec(), false)
            .await
            .unwrap();
        r
    }

    /// A task of repository `i`, started, run to the end by the scripted model.
    async fn finished_task(&mut self, i: usize, n: u8) -> Id {
        let root = self.root(i);
        let t = create_task(
            &mut self.c,
            &self.session,
            self.g,
            &root,
            n,
            "local_trusted",
            "do it",
        )
        .await;
        start_task(&mut self.c, &t, self.g, n.wrapping_add(1), "gpt-5-mini").await;
        let st = wait_task(&mut self.c, &t, 120).await;
        assert!(!st.loop_alive, "{st:?}");
        t
    }

    /// A task that was created and then cancelled: not a draft, no model needed.
    async fn cancelled_task(&mut self, i: usize, n: u8) -> Id {
        let root = self.root(i);
        let t = create_task(
            &mut self.c,
            &self.session,
            self.g,
            &root,
            n,
            "local_trusted",
            "x",
        )
        .await;
        let _: (modbit_protocol::v1::TaskCancelRequested, bool) = self
            .call(
                rand_id(),
                "CancelTask",
                CancelTask {
                    task_id: Some(t.clone()),
                }
                .encode_to_vec(),
                true,
            )
            .await
            .unwrap();
        t
    }

    async fn draft_task(&mut self, i: usize, n: u8) -> Id {
        let root = self.root(i);
        create_task(
            &mut self.c,
            &self.session,
            self.g,
            &root,
            n,
            "local_trusted",
            "later",
        )
        .await
    }

    /// A pending approval on the task: a destructive tool call that stops at its approval.
    async fn pending_approval(&mut self, task: &Id, i: usize, name: &str) -> String {
        let wt = modbit_tools::direct::worktree_root(Path::new(&self.root(i)))
            .unwrap()
            .join(name)
            .to_string_lossy()
            .replace('\\', "/");
        let r = self
            .invoke(
                task,
                "git.worktree.create",
                format!(r#"{{"branch":"task/{name}","path":"{wt}"}}"#),
            )
            .await;
        assert_eq!(r.status, "SUCCESS", "{r:?}");
        let r = self
            .invoke(task, "git.worktree.close", format!(r#"{{"path":"{wt}"}}"#))
            .await;
        assert_eq!(r.status, "APPROVAL_PENDING", "{r:?}");
        r.approval_id
    }

    async fn invoke(&mut self, task: &Id, tool: &str, args: String) -> ToolInvoked {
        let msg = InvokeTool {
            task_id: Some(task.clone()),
            tool_name: tool.into(),
            arguments_json: args,
            tool_call_id: Some(rand_id()),
            output_budget_bytes: 4096,
        };
        let (r, _) = self
            .call(rand_id(), "InvokeTool", msg.encode_to_vec(), true)
            .await
            .unwrap();
        r
    }

    async fn deny(&mut self, approval_hex: &str) {
        let msg = ResolveApproval {
            approval_id: Some(Id {
                value: hex::decode(approval_hex.replace('-', "")).unwrap(),
            }),
            approve: false,
            reason: "no".into(),
            intent_hash: String::new(),
        };
        let _: (ApprovalResolvedAck, bool) = self
            .call(rand_id(), "ResolveApproval", msg.encode_to_vec(), true)
            .await
            .unwrap();
    }
}

fn ids(p: &ProjectView) -> Vec<Vec<u8>> {
    let mut v: Vec<_> = p
        .members
        .iter()
        .map(|m| m.task_id.as_ref().unwrap().value.clone())
        .collect();
    v.sort();
    v
}

fn sorted(tasks: &[&Id]) -> Vec<Vec<u8>> {
    let mut v: Vec<_> = tasks.iter().map(|t| t.value.clone()).collect();
    v.sort();
    v
}

fn code(r: Result<impl std::fmt::Debug, Reject>) -> String {
    r.expect_err("the Core must refuse").0
}

// ===========================================================================

#[tokio::test]
async fn qual_px_063_records_membership_and_each_guard_with_its_typed_reason() {
    // Two workspaces and ten tasks.
    let mut fx = Fx::new(2, &[]).await;
    let (ra, rb) = (fx.root(0), fx.root(1));
    let mut a = Vec::new();
    for i in 0..4u8 {
        a.push(fx.finished_task(0, 0x20 + i * 4).await);
    }
    let mut b = Vec::new();
    for i in 0..3u8 {
        b.push(fx.cancelled_task(1, 0x50 + i * 2).await);
    }
    let draft = fx.draft_task(0, 0x70).await;
    let iso_root = ra.clone();
    let isolated = {
        let ack =
            fx.c.command(envelope_fenced(
                id16(0x72),
                "CreateTask",
                CreateTask {
                    session_id: Some(fx.session.clone()),
                    goal_text: "isolated".into(),
                    execution_profile: "local_trusted".into(),
                    origin: "cli".into(),
                    workspace_root: iso_root,
                    isolation: TaskIsolation::Worktree as i32,
                    ..Default::default()
                }
                .encode_to_vec(),
                fx.g,
            ))
            .await
            .unwrap();
        let t = Client::result::<TaskCreated>(&ack)
            .unwrap()
            .task_id
            .unwrap();
        let _: (modbit_protocol::v1::TaskCancelRequested, bool) = fx
            .call(
                rand_id(),
                "CancelTask",
                CancelTask {
                    task_id: Some(t.clone()),
                }
                .encode_to_vec(),
                true,
            )
            .await
            .unwrap();
        t
    };
    let draft_b = fx.draft_task(1, 0x74).await;
    assert_eq!(a.len() + b.len() + 3, 10);

    // Create: a name, a palette role and an icon; nothing else.
    let alpha = fx.create_project("Alpha", &ra).await.unwrap();
    assert_eq!(
        (alpha.color.as_str(), alpha.icon.as_str()),
        ("accent", "folder")
    );
    assert_eq!(alpha.workspace_root, ra);
    assert!(!alpha.archived);
    assert_eq!(alpha.rollup.as_ref().unwrap().members, 0);
    // Name, colour, icon and workspace are validated by the Core.
    assert_eq!(
        code(fx.create_project("  ", &ra).await),
        "PROJECT_NAME_INVALID"
    );
    assert_eq!(
        code(fx.create_project("x".repeat(81).as_str(), &ra).await),
        "PROJECT_NAME_INVALID"
    );
    assert_eq!(
        code(
            fx.create_project_as(rand_id(), "Z", "#ff0000", "", &ra)
                .await
        ),
        "PROJECT_COLOR_INVALID"
    );
    assert_eq!(
        code(fx.create_project_as(rand_id(), "Z", "", "skull", &ra).await),
        "PROJECT_ICON_INVALID"
    );
    assert_eq!(
        code(fx.create_project("alpha", &ra).await),
        "PROJECT_NAME_TAKEN"
    );
    // The same name in another workspace is a different project.
    let beta = fx.create_project("Alpha", &rb).await.unwrap();

    // Add: top-level tasks of the project's own workspace.
    fx.add(&alpha, &a[0]).await.unwrap();
    let r = fx.add(&alpha, &a[1]).await.unwrap();
    assert!(r.offset > 0);
    let r = fx.add(&alpha, &isolated).await.unwrap();
    assert!(
        r.offset > 0,
        "a worktree task is grouped by the checkout it came from"
    );
    // Adding a member again is a no-op: nothing recorded.
    let again = fx.add(&alpha, &a[1]).await.unwrap();
    assert_eq!(again.offset, 0);
    assert_eq!(again.project.unwrap().rollup.unwrap().members, 3);

    // The projection, the header and ListProjects agree.
    let got = fx.get(&alpha).await;
    assert_eq!(ids(&got), sorted(&[&a[0], &a[1], &isolated]));
    let listed = fx.list(false).await;
    let listed_alpha = listed
        .projects
        .iter()
        .find(|p| p.project_id == alpha.project_id)
        .unwrap();
    assert_eq!(ids(listed_alpha), ids(&got));
    let headers = fx.headers().await;
    // A task in a worktree of its own names the checkout it came from, and works in a root of its own.
    let iso = headers
        .headers
        .iter()
        .find(|h| h.task_id.as_ref() == Some(&isolated))
        .unwrap();
    assert_eq!(Path::new(&iso.checkout_root), Path::new(&ra));
    assert_ne!(Path::new(&iso.workspace_root), Path::new(&ra));
    let plain = headers
        .headers
        .iter()
        .find(|h| h.task_id.as_ref() == Some(&a[0]))
        .unwrap();
    assert_eq!(
        plain.checkout_root, "",
        "a task in its own root names no other checkout"
    );
    let in_alpha: Vec<Vec<u8>> = {
        let mut v: Vec<_> = headers
            .headers
            .iter()
            .filter(|h| h.project_id == alpha.project_id)
            .map(|h| h.task_id.as_ref().unwrap().value.clone())
            .collect();
        v.sort();
        v
    };
    assert_eq!(
        in_alpha,
        ids(&got),
        "the header's project id is the Core's map"
    );
    assert!(
        headers
            .headers
            .iter()
            .filter(|h| h.project_id.is_none())
            .count()
            >= 6,
        "tasks outside any project carry no project id"
    );

    // Every guard refuses its violation with its own typed reason, and changes nothing.
    let before = ids(&fx.get(&alpha).await);
    assert_eq!(code(fx.add(&alpha, &draft).await), "TASK_IS_DRAFT");
    assert_eq!(
        code(fx.add(&alpha, &b[0]).await),
        "WORKSPACE_MISMATCH",
        "a task of another workspace"
    );
    assert_eq!(
        code(fx.add(&beta, &a[2]).await),
        "WORKSPACE_MISMATCH",
        "a project of another workspace"
    );
    let other = fx.create_project("Other", &ra).await.unwrap();
    assert_eq!(
        code(fx.add(&other, &a[0]).await),
        "IN_ANOTHER_PROJECT",
        "no move without leaving first"
    );
    assert_eq!(
        code(fx.add(&alpha, &Id { value: vec![9; 16] }).await),
        "UNKNOWN_TASK"
    );
    let ghost = ProjectView {
        project_id: Some(Id { value: vec![8; 16] }),
        ..Default::default()
    };
    assert_eq!(code(fx.add(&ghost, &a[0]).await), "UNKNOWN_PROJECT");
    assert_eq!(code(fx.remove(&other, &a[0]).await), "NOT_A_MEMBER");
    assert_eq!(ids(&fx.get(&alpha).await), before);
    assert!(fx.get(&other).await.members.is_empty());
    let _ = draft_b;

    // Leave, then move: the legal way across projects.
    fx.remove(&alpha, &a[0]).await.unwrap();
    fx.add(&other, &a[0]).await.unwrap();
    assert_eq!(ids(&fx.get(&other).await), sorted(&[&a[0]]));
    // Removing a task that is in no project is a no-op, not an error.
    assert_eq!(fx.remove(&alpha, &a[3]).await.unwrap().offset, 0);

    // Rename: the workspace never changes; a taken name is refused.
    let renamed = fx
        .rename(&alpha, "Release 2", "warn", "rocket")
        .await
        .unwrap();
    let p = renamed.project.unwrap();
    assert_eq!(
        (p.name.as_str(), p.color.as_str(), p.icon.as_str()),
        ("Release 2", "warn", "rocket")
    );
    assert_eq!(p.workspace_root, ra);
    assert_eq!(
        code(fx.rename(&other, "release 2", "", "").await),
        "PROJECT_NAME_TAKEN"
    );
    // An empty field keeps what the project has.
    let kept = fx
        .rename(&alpha, "", "", "bug")
        .await
        .unwrap()
        .project
        .unwrap();
    assert_eq!(
        (kept.name.as_str(), kept.color.as_str(), kept.icon.as_str()),
        ("Release 2", "warn", "bug")
    );

    // Archive: the record leaves the list; the tasks are untouched.
    let task_states: Vec<_> = {
        let h = fx.headers().await;
        let mut v: Vec<_> = h
            .headers
            .iter()
            .map(|h| {
                (
                    h.task_id.clone().unwrap().value,
                    h.task_state.clone(),
                    h.archived,
                )
            })
            .collect();
        v.sort();
        v
    };
    let archived = fx.archive(&alpha, true).await.unwrap().project.unwrap();
    assert!(archived.archived);
    assert_eq!(ids(&archived), sorted(&[&a[1], &isolated]), "members stay");
    assert!(
        fx.list(false)
            .await
            .projects
            .iter()
            .all(|p| p.project_id != alpha.project_id)
    );
    assert!(
        fx.list(true)
            .await
            .projects
            .iter()
            .any(|p| p.project_id == alpha.project_id)
    );
    let after: Vec<_> = {
        let h = fx.headers().await;
        let mut v: Vec<_> = h
            .headers
            .iter()
            .map(|h| {
                (
                    h.task_id.clone().unwrap().value,
                    h.task_state.clone(),
                    h.archived,
                )
            })
            .collect();
        v.sort();
        v
    };
    assert_eq!(
        after, task_states,
        "archiving a project archives and stops no task"
    );
    // An archived project accepts no change but its own return.
    assert_eq!(code(fx.add(&archived, &a[3]).await), "PROJECT_ARCHIVED");
    assert_eq!(code(fx.remove(&archived, &a[1]).await), "PROJECT_ARCHIVED");
    assert_eq!(
        code(fx.rename(&archived, "New", "", "").await),
        "PROJECT_ARCHIVED"
    );
    // Archiving twice records nothing; its undo brings everything back.
    assert_eq!(fx.archive(&alpha, true).await.unwrap().offset, 0);
    // A name taken while it was away blocks its return.
    let squatter = fx.create_project("release 2", &ra).await.unwrap();
    assert_eq!(code(fx.archive(&alpha, false).await), "PROJECT_NAME_TAKEN");
    fx.rename(&squatter, "Squatter", "", "").await.unwrap();
    let back = fx.archive(&alpha, false).await.unwrap().project.unwrap();
    assert!(!back.archived);
    assert_eq!(ids(&back), sorted(&[&a[1], &isolated]));

    // A mutation presents the session lease.
    let msg = CreateProject {
        session_id: Some(fx.session.clone()),
        name: "Unfenced".into(),
        workspace_root: ra.clone(),
        ..Default::default()
    };
    let r: Result<(ProjectChanged, bool), Reject> = fx
        .call(rand_id(), "CreateProject", msg.encode_to_vec(), false)
        .await;
    assert_eq!(code(r), "LEASE_REQUIRED");
    fx.core.kill();
}

#[tokio::test]
async fn qual_px_063_a_replayed_command_applies_once_and_a_kill_converges_to_one_state() {
    let mut fx = Fx::new(1, &[]).await;
    let ra = fx.root(0);
    let t1 = fx.finished_task(0, 0x20).await;
    let t2 = fx.finished_task(0, 0x30).await;
    let t3 = fx.finished_task(0, 0x40).await;

    // The same command id twice: one project, one membership.
    let cid = rand_id();
    let (p1, replayed) = fx
        .create_project_as(cid.clone(), "Once", "", "", &ra)
        .await
        .unwrap();
    assert!(!replayed);
    let (p2, replayed) = fx
        .create_project_as(cid, "Once", "", "", &ra)
        .await
        .unwrap();
    assert!(replayed);
    assert_eq!(p1.project_id, p2.project_id);
    assert_eq!(fx.list(true).await.projects.len(), 1);
    let add_id = rand_id();
    let (_, replayed) = fx.add_as(add_id.clone(), &p1, &t1).await.unwrap();
    assert!(!replayed);
    let (r, replayed) = fx.add_as(add_id.clone(), &p1, &t1).await.unwrap();
    assert!(replayed);
    assert_eq!(r.project.unwrap().rollup.unwrap().members, 1);
    // The same id with a different request is a conflict, not a second add.
    let conflict = fx.add_as(add_id, &p1, &t2).await;
    assert!(conflict.is_err(), "{conflict:?}");

    // SIGKILL the Core after the event is committed and before it answers:
    // the client sees a dead connection; after the restart the membership is
    // the new one, once, and the retry replays.
    fx.restart(&[("MODBIT_FAULT_PROJECT", "after-append")])
        .await;
    let retry_id = rand_id();
    let msg = AddProjectMember {
        session_id: Some(fx.session.clone()),
        project_id: p1.project_id.clone(),
        task_id: Some(t2.clone()),
    };
    let died =
        fx.c.command(envelope_fenced(
            retry_id.clone(),
            "AddProjectMember",
            msg.encode_to_vec(),
            fx.g,
        ))
        .await;
    assert!(died.is_err(), "the Core died before it answered: {died:?}");
    fx.restart(&[]).await;
    let now = fx.get(&p1).await;
    assert_eq!(
        ids(&now),
        sorted(&[&t1, &t2]),
        "the committed add survived the kill"
    );
    let (r, replayed) = fx.add_as(retry_id, &p1, &t2).await.unwrap();
    assert!(replayed, "the retry of the same command replays");
    assert_eq!(ids(&r.project.unwrap()), sorted(&[&t1, &t2]));

    // SIGKILL before the event is committed: the old membership, and the
    // retry now applies.
    fx.restart(&[("MODBIT_FAULT_PROJECT", "before-append")])
        .await;
    let retry_id = rand_id();
    let msg = AddProjectMember {
        session_id: Some(fx.session.clone()),
        project_id: p1.project_id.clone(),
        task_id: Some(t3.clone()),
    };
    let died =
        fx.c.command(envelope_fenced(
            retry_id.clone(),
            "AddProjectMember",
            msg.encode_to_vec(),
            fx.g,
        ))
        .await;
    assert!(died.is_err());
    fx.restart(&[]).await;
    assert_eq!(
        ids(&fx.get(&p1).await),
        sorted(&[&t1, &t2]),
        "nothing of the lost command"
    );
    let (r, replayed) = fx.add_as(retry_id, &p1, &t3).await.unwrap();
    assert!(!replayed);
    assert_eq!(ids(&r.project.unwrap()), sorted(&[&t1, &t2, &t3]));

    // A restart reads the same records: the list is what it was.
    let before = fx.list(true).await;
    fx.restart(&[]).await;
    let after = fx.list(true).await;
    assert_eq!(before.projects, after.projects);
    // The map the headers show is the map the Core projected.
    let headers = fx.headers().await;
    assert_eq!(
        headers
            .headers
            .iter()
            .filter(|h| h.project_id == p1.project_id)
            .count(),
        3
    );

    // The tables are projections of the log: empty them and drop how far they
    // were read, and the Core's startup rebuilds exactly the same records and
    // map from the `Project` events alone.
    let want = fx.list(true).await;
    fx.core.kill();
    {
        let conn = rusqlite::Connection::open(fx.dir.path().join("core/core.db")).unwrap();
        conn.execute_batch(
            "DELETE FROM project_members; DELETE FROM projects; UPDATE projection_state SET last_offset = 0;",
        )
        .unwrap();
    }
    fx.restart(&[]).await;
    let rebuilt = fx.list(true).await;
    assert_eq!(rebuilt.projects.len(), 1);
    assert_eq!(
        rebuilt.projects, want.projects,
        "the rebuild from the log is the same projection"
    );
    assert_eq!(ids(&fx.get(&p1).await), sorted(&[&t1, &t2, &t3]));
    fx.core.kill();
}

#[tokio::test]
async fn qual_px_063_the_rollup_counts_by_class_and_surfaces_attention_from_the_log() {
    let mut fx = Fx::new(1, &[]).await;
    let ra = fx.root(0);
    let stopped1 = fx.cancelled_task(0, 0x20).await;
    let stopped2 = fx.cancelled_task(0, 0x30).await;
    let stopped3 = fx.cancelled_task(0, 0x40).await;
    // A task the scripted model ran to its end: the Core leaves it waiting for a person.
    let needy = fx.finished_task(0, 0x50).await;
    let p = fx.create_project("Rollup", &ra).await.unwrap();
    for t in [&stopped1, &stopped2, &stopped3, &needy] {
        fx.add(&p, t).await.unwrap();
    }
    // The rollup is a count over the very headers the list shows.
    let check = |view: &ProjectView, headers: &AgentHeaders| {
        let mine: Vec<_> = headers
            .headers
            .iter()
            .filter(|h| h.project_id == view.project_id)
            .collect();
        let r = view.rollup.clone().unwrap();
        assert_eq!(r.members as usize, mine.len());
        assert_eq!(
            r.attention_items,
            mine.iter().map(|h| h.attention_items).sum::<u32>()
        );
        assert_eq!(
            r.attention_tasks as usize,
            mine.iter()
                .filter(|h| h.status_class == AgentStatusClass::NeedsAttention as i32)
                .count()
        );
        assert_eq!(
            r.pending_approvals as usize,
            mine.iter().filter(|h| h.pending_approval).count()
        );
        assert_eq!(r.by_status.iter().map(|s| s.count).sum::<u32>(), r.members);
        for s in &r.by_status {
            let n = mine
                .iter()
                .filter(|h| h.status_class == s.status_class)
                .count();
            assert_eq!(s.count as usize, n, "{s:?}");
            assert!(!s.label.is_empty());
        }
        r
    };
    let headers = fx.headers().await;
    let view = fx.get(&p).await;
    let base = check(&view, &headers);
    assert_eq!(base.members, 4);
    assert_eq!(base.pending_approvals, 0);
    assert_eq!(
        (
            base.pull_requests,
            base.ci_passing,
            base.ci_failing,
            base.ci_pending
        ),
        (0, 0, 0, 0)
    );
    let classes: Vec<_> = base.by_status.iter().map(|s| s.label.as_str()).collect();
    assert!(
        classes.contains(&"Cancelled") || classes.contains(&"Completed"),
        "{classes:?}"
    );

    // A protected effect waiting for a person: one more item, on exactly that task.
    let approval = fx.pending_approval(&needy, 0, "wt-a").await;
    let headers = fx.headers().await;
    let view = fx.get(&p).await;
    let r = check(&view, &headers);
    assert_eq!(r.pending_approvals, 1, "{r:?}");
    assert!(
        r.attention_items > base.attention_items,
        "{r:?} vs {base:?}"
    );
    let flagged: Vec<_> = view
        .members
        .iter()
        .filter(|m| m.status_class == AgentStatusClass::NeedsAttention as i32)
        .collect();
    assert_eq!(flagged.len(), 1);
    assert_eq!(flagged[0].task_id.as_ref(), Some(&needy));
    assert!(flagged[0].pending_approval);

    // Resolving it clears the item: the rollup follows the log.
    fx.deny(&approval).await;
    let headers = fx.headers().await;
    let view = fx.get(&p).await;
    let r = check(&view, &headers);
    assert_eq!(r.pending_approvals, 0, "{r:?}");
    assert_eq!(r.attention_items, base.attention_items, "{r:?}");
    fx.core.kill();
}

// ===========================================================================
// PX-068: removing one worktree by the rule the cleanup applies
// ===========================================================================

impl Fx {
    async fn isolated_task(&mut self, i: usize, n: u8, cancel: bool) -> Id {
        let root = self.root(i);
        let ack = self
            .c
            .command(envelope_fenced(
                id16(n),
                "CreateTask",
                CreateTask {
                    session_id: Some(self.session.clone()),
                    goal_text: "isolated".into(),
                    execution_profile: "local_trusted".into(),
                    origin: "cli".into(),
                    workspace_root: root,
                    isolation: TaskIsolation::Worktree as i32,
                    ..Default::default()
                }
                .encode_to_vec(),
                self.g,
            ))
            .await
            .unwrap();
        let t = Client::result::<TaskCreated>(&ack)
            .unwrap()
            .task_id
            .unwrap();
        if cancel {
            let _: (modbit_protocol::v1::TaskCancelRequested, bool) = self
                .call(
                    rand_id(),
                    "CancelTask",
                    CancelTask {
                        task_id: Some(t.clone()),
                    }
                    .encode_to_vec(),
                    true,
                )
                .await
                .unwrap();
        }
        t
    }

    async fn worktrees(&mut self) -> WorktreeList {
        let (r, _) = self
            .call(
                rand_id(),
                "ListWorktrees",
                ListWorktrees::default().encode_to_vec(),
                false,
            )
            .await
            .unwrap();
        r
    }

    async fn remove_worktree(
        &mut self,
        id: &str,
        dry_run: bool,
        fenced: bool,
    ) -> Result<WorktreeRemoval, Reject> {
        let msg = RemoveWorktree {
            session_id: Some(self.session.clone()),
            worktree_id: id.into(),
            dry_run,
        };
        self.call(rand_id(), "RemoveWorktree", msg.encode_to_vec(), fenced)
            .await
            .map(|(r, _)| r)
    }
}

#[tokio::test]
async fn qual_px_068_remove_one_worktree_only_when_the_cleanup_rule_allows_it() {
    let mut fx = Fx::new(1, &[("MODBIT_WORKTREE_PROTECT_MS", "1")]).await;
    let clean = fx.isolated_task(0, 0x20, true).await;
    let dirty = fx.isolated_task(0, 0x30, true).await;
    let open = fx.isolated_task(0, 0x40, false).await;
    let l = fx.worktrees().await;
    assert_eq!(l.worktrees.len(), 3);
    let of = |l: &WorktreeList, t: &Id| {
        l.worktrees
            .iter()
            .find(|w| w.task_id.as_ref() == Some(t))
            .unwrap()
            .clone()
    };
    let (w_clean, w_dirty, w_open) = (of(&l, &clean), of(&l, &dirty), of(&l, &open));
    // Edit the dirty one on disk, as the agent would have.
    std::fs::write(Path::new(&w_dirty.path).join("work.txt"), "unfinished\n").unwrap();
    std::thread::sleep(std::time::Duration::from_millis(20)); // past the 1 ms protection window

    // Refused with the typed reason, and nothing touched.
    let r = fx.remove_worktree(&w_open.worktree_id, false, true).await;
    let (code, why) = r.unwrap_err();
    assert_eq!(code, "TASK_RUNNING", "{why}");
    assert!(why.contains("task has not ended"), "{why}");
    assert!(Path::new(&w_open.path).exists());
    let (code, why) = fx
        .remove_worktree(&w_dirty.worktree_id, false, true)
        .await
        .unwrap_err();
    assert_eq!(code, "WORKTREE_NEEDS_DECISION", "{why}");
    assert!(why.contains("dirty"), "{why}");
    assert!(why.contains("apply, merge or discard"), "{why}");
    assert_eq!(
        std::fs::read_to_string(Path::new(&w_dirty.path).join("work.txt")).unwrap(),
        "unfinished\n"
    );
    let (code, _) = fx
        .remove_worktree("no-such-worktree", false, true)
        .await
        .unwrap_err();
    assert_eq!(code, "UNKNOWN_WORKTREE");

    // A dry run says what would be lost and removes nothing, with no lease.
    let p = fx
        .remove_worktree(&w_clean.worktree_id, true, false)
        .await
        .unwrap();
    assert!(p.dry_run && !p.removed);
    assert!(
        p.loses.iter().any(|l| l.contains(&w_clean.path)),
        "{:?}",
        p.loses
    );
    assert!(Path::new(&w_clean.path).exists());
    // The real removal needs the lease ...
    let (code, _) = fx
        .remove_worktree(&w_clean.worktree_id, false, false)
        .await
        .unwrap_err();
    assert_eq!(code, "LEASE_REQUIRED");
    assert!(Path::new(&w_clean.path).exists());
    // ... and then removes exactly that directory and records it.
    let done = fx
        .remove_worktree(&w_clean.worktree_id, false, true)
        .await
        .unwrap();
    assert!(done.removed && done.offset > 0, "{done:?}");
    assert!(!Path::new(&w_clean.path).exists());
    assert!(Path::new(&w_dirty.path).exists() && Path::new(&w_open.path).exists());
    let after = fx.worktrees().await;
    assert!(
        after
            .worktrees
            .iter()
            .all(|w| w.worktree_id != w_clean.worktree_id)
    );
    // Asking again names it unknown: it is gone, not repeated.
    let (code, _) = fx
        .remove_worktree(&w_clean.worktree_id, false, true)
        .await
        .unwrap_err();
    assert_eq!(code, "UNKNOWN_WORKTREE");
    fx.core.kill();
}
