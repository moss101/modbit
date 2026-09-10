//! Workspace change events (REQ-EV-0106): every write lands a typed,
//! revision-bound `FileChanged` event carrying content and diff references
//! (never bytes), and the typed inverse plan for undo (REQ-EV-0064/0065).

use serde::{Deserialize, Serialize};

use crate::ids::{TaskId, ToolCallId};

/// Events on the `Workspace` aggregate (id: the worktree id digest).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum WorkspaceEvent {
    /// One file changed by a tool call.
    FileChanged {
        /// Task.
        task_id: TaskId,
        /// Tool call that produced the change.
        tool_call_id: ToolCallId,
        /// Root-relative path.
        path: String,
        /// Operation (`create`, `atomic_replace`, `apply_patch`, `edit`, `delete`, `undo:*`).
        op: String,
        /// Content hash before (`None` = created).
        before_hash: Option<String>,
        /// Content hash after (`None` = deleted).
        after_hash: Option<String>,
        /// Object ref of the content before (`None` = created or binary omitted).
        before_ref: Option<String>,
        /// Object ref of the content after (`None` = deleted).
        after_ref: Option<String>,
        /// Object ref of the unified diff (before → after), text files only.
        diff_ref: Option<String>,
        /// Workspace revision after the change.
        workspace_revision: u64,
        /// Workspace revision before the change.
        previous_revision: u64,
        /// The language state of the file at the time of the change
        /// (PX-029): the language label, and `unsupported_language` when the
        /// product claims no tier for it. Empty on events written before the
        /// language state existed.
        #[serde(default)]
        language: String,
        /// Whether this edit was made under an explicit per-task opt-in
        /// because the product claims nothing about the language (docs/76).
        #[serde(default)]
        unsupported_language: bool,
    },
}

/// One typed inverse action of an undo plan.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct UndoStep {
    /// Root-relative path.
    pub path: String,
    /// Inverse action: `delete` (undo create), `restore` (undo delete), `replace` (undo modify).
    pub action: String,
    /// Content hash the path must still have for the inverse to apply (the post-edit hash).
    pub expected_content_hash: Option<String>,
    /// Whether the path must be absent (undoing a delete).
    pub expect_absent: bool,
    /// Object ref of the content to restore (`None` for `delete`).
    pub restore_ref: Option<String>,
    /// Content hash the path will have after the inverse (`None` = absent).
    pub resulting_hash: Option<String>,
}

/// Typed undo plan for one tool call (REQ-EV-0064): inverse actions, never a blind checkout.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct UndoPlan {
    /// Tool call being undone.
    pub tool_call_id: ToolCallId,
    /// Steps in application order (latest change first).
    pub steps: Vec<UndoStep>,
}
