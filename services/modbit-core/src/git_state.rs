//! The Core's side of the governed git tools (PX-119, PX-066): the port
//! `git.merge.*` and `git.worktree.apply` / `undo` run on. Built per call
//! from what the Core already decided for it — the task, its lease and
//! profile, the mode, the pinned configuration — so the merge's
//! post-merge verification runs under exactly the authority the task has.

use std::path::PathBuf;
use std::sync::Arc;

use modbit_domain::event::Actor;
use modbit_domain::lease::CapabilityLease;
use modbit_domain::mode::TaskMode;
use modbit_domain::{SessionId, TaskId, TenantId, ToolCallId};
use modbit_event_store::EventStore;
use modbit_tools::GitStatePort;
use modbit_tools::pipeline::ExecTarget;
use modbit_tools::registry::BoxFuture;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tokio::sync::Mutex;

use crate::{apply_back, merge_tx, worktrees};

type Refusal = (String, String);

/// The port for one call.
pub(crate) struct CoreGitState {
    pub store: Arc<Mutex<EventStore>>,
    pub tenant: TenantId,
    pub session: SessionId,
    pub task: TaskId,
    pub actor: Actor,
    /// The task's workspace root, canonical.
    pub root: Option<PathBuf>,
    pub exec: Option<ExecTarget>,
    pub profile: String,
    pub lease: Option<CapabilityLease>,
    pub mode: TaskMode,
    pub emergency: bool,
    pub config: Arc<modbit_policy::config::ResolvedConfig>,
    /// `.modbit/verification.json` as the task pinned it at its start.
    pub verification_json: Option<Option<String>>,
    pub cancel: Option<tokio_util::sync::CancellationToken>,
}

impl CoreGitState {
    /// A worktree the model made with `git.worktree.create`: the registry
    /// knows it, so the cleanup can see it.
    async fn created(&self, args: &Value) -> Result<Value, Refusal> {
        let given = args["path"].as_str().unwrap_or_default();
        // The record carries the platform's one canonical form, so a later
        // existence check (and the close that matches it) agrees on Windows.
        let resolved = worktrees::plain_path(&worktrees::resolve_path(std::path::Path::new(given)));
        let path = resolved.as_str();
        let id = format!(
            "tool-{}",
            &hex::encode(Sha256::digest(path.as_bytes()))[..16]
        );
        let origin = self
            .root
            .as_ref()
            .map(|r| worktrees::plain_path(r))
            .unwrap_or_default();
        let event = worktrees::provisioned_event(
            "TOOL",
            &id,
            self.task,
            path,
            &origin,
            args["branch"].as_str().unwrap_or_default(),
            args["head"].as_str().unwrap_or_default(),
            &self.actor,
        );
        let mut st = self.store.lock().await;
        worktrees::append_events(
            &mut st,
            self.tenant,
            self.session,
            self.task,
            worktrees::aggregate_id(&id),
            vec![event],
        )
        .map_err(|e| ("STORE".to_owned(), e))?;
        Ok(json!({"worktree_id": id}))
    }

    /// A worktree the model closed: the registry says it is gone.
    async fn closed(&self, args: &Value) -> Result<Value, Refusal> {
        let path = args["path"].as_str().unwrap_or_default().to_owned();
        let mut st = self.store.lock().await;
        let found = worktrees::registry_of_session(&st, &self.session)
            .into_iter()
            .find(|r| !r.removed && r.kind == "TOOL" && worktrees::same_path(&r.path, &path));
        let Some(rec) = found else {
            return Ok(json!({"recorded": false}));
        };
        worktrees::append_events(
            &mut st,
            self.tenant,
            self.session,
            self.task,
            worktrees::aggregate_id(&rec.worktree_id),
            vec![worktrees::removed_event(
                &rec.worktree_id,
                "closed with git.worktree.close",
                0,
                "task",
                &self.actor,
            )],
        )
        .map_err(|e| ("STORE".to_owned(), e))?;
        Ok(json!({"recorded": true}))
    }
}

impl GitStatePort for CoreGitState {
    fn call<'a>(
        &'a self,
        op: &'a str,
        _tool_call_id: Option<ToolCallId>,
        args: &'a Value,
    ) -> BoxFuture<'a, Result<Value, Refusal>> {
        Box::pin(async move {
            match op {
                "merge.prepare" => merge_tx::prepare(self, args).await,
                "merge.commit" => merge_tx::commit(self, args).await,
                "merge.abort" => merge_tx::abort(self, args).await,
                "worktree.apply" => {
                    apply_back::exec_apply(
                        &self.store,
                        self.tenant,
                        self.session,
                        self.task,
                        &self.actor,
                        args,
                    )
                    .await
                }
                "worktree.undo" => {
                    apply_back::exec_undo(
                        &self.store,
                        self.tenant,
                        self.session,
                        self.task,
                        &self.actor,
                        args,
                    )
                    .await
                }
                "worktree.created" => self.created(args).await,
                "worktree.closed" => self.closed(args).await,
                other => Err((
                    "UNKNOWN_OP".to_owned(),
                    format!("the git state port has no operation `{other}`"),
                )),
            }
        })
    }
}
