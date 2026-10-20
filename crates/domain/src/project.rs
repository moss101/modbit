//! Projects (PX-063; docs/65 AFW-J01, AFW-J03, AFW-J04): a named, archivable
//! record bound to one workspace, with an external membership map from task
//! to project.
//!
//! A project is a grouping over the existing WorkGraph. It carries no
//! coordinator behaviour, schedules nothing and owns no task: archiving a
//! project changes the record and nothing else. Every change is an event on
//! the project's own aggregate; the `projects` and `project_members` tables
//! are projections of these events and rebuild from them. The membership map
//! is one task to at most one project, so "no move across projects without
//! leaving first" is also a key of the projection, not only a check.
//!
//! The rules live here as pure functions so the Core, the CLI and the tests
//! apply the same ones: a refusal is a typed [`Refusal`] with a stable code.

use serde::{Deserialize, Serialize};

use crate::ids::{ProjectId, TaskId};
use crate::task::{TaskOrigin, TaskState};

/// `ProjectCreated`.
pub const CREATED: &str = "ProjectCreated";
/// `ProjectRenamed`.
pub const RENAMED: &str = "ProjectRenamed";
/// `ProjectArchived`.
pub const ARCHIVED: &str = "ProjectArchived";
/// `ProjectUnarchived`.
pub const UNARCHIVED: &str = "ProjectUnarchived";
/// `ProjectMemberAdded`.
pub const MEMBER_ADDED: &str = "ProjectMemberAdded";
/// `ProjectMemberRemoved`.
pub const MEMBER_REMOVED: &str = "ProjectMemberRemoved";

/// Longest project name, in characters.
pub const MAX_NAME_CHARS: usize = 80;
/// Most tasks one project holds.
pub const MAX_MEMBERS: usize = 200;
/// Most live (unarchived) projects one workspace holds.
pub const MAX_LIVE_PROJECTS_PER_WORKSPACE: usize = 50;

/// The colours a project may carry: roles of the design-token palette
/// (PX-044), never a free colour. The renderer maps a role to the theme's
/// value, so a project looks right in every theme and contrast is the
/// palette's, not the person's.
pub const COLORS: [&str; 8] = [
    "accent",
    "ok",
    "warn",
    "danger",
    "info",
    "modeAsk",
    "modePlan",
    "modeDebug",
];
/// The colour a project has when none is named.
pub const DEFAULT_COLOR: &str = "accent";

/// The icons a project may carry: names of the renderer's small icon set.
pub const ICONS: [&str; 8] = [
    "folder", "star", "flag", "bolt", "book", "bug", "rocket", "leaf",
];
/// The icon a project has when none is named.
pub const DEFAULT_ICON: &str = "folder";

/// A typed refusal: a stable code and a sentence for a person.
pub type Refusal = (&'static str, String);

fn refuse(code: &'static str, detail: impl Into<String>) -> Refusal {
    (code, detail.into())
}

/// Events on a `Project` aggregate.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event_type")]
pub enum ProjectEvent {
    /// The project exists, bound to one workspace for good.
    ProjectCreated {
        /// Identity.
        project_id: ProjectId,
        /// Display name.
        name: String,
        /// A palette role of [`COLORS`].
        color: String,
        /// An icon of [`ICONS`].
        icon: String,
        /// The workspace root it is bound to.
        workspace_root: String,
    },
    /// Name, colour and icon after a rename.
    ProjectRenamed {
        /// Identity.
        project_id: ProjectId,
        /// Display name.
        name: String,
        /// A palette role of [`COLORS`].
        color: String,
        /// An icon of [`ICONS`].
        icon: String,
    },
    /// The project left the active list. Its tasks are untouched.
    ProjectArchived {
        /// Identity.
        project_id: ProjectId,
    },
    /// The project came back.
    ProjectUnarchived {
        /// Identity.
        project_id: ProjectId,
    },
    /// A task joined.
    ProjectMemberAdded {
        /// The project.
        project_id: ProjectId,
        /// The task.
        task_id: TaskId,
    },
    /// A task left. The task itself is untouched.
    ProjectMemberRemoved {
        /// The project.
        project_id: ProjectId,
        /// The task.
        task_id: TaskId,
    },
}

impl ProjectEvent {
    /// Canonical event type name.
    #[must_use]
    pub const fn event_type(&self) -> &'static str {
        match self {
            Self::ProjectCreated { .. } => CREATED,
            Self::ProjectRenamed { .. } => RENAMED,
            Self::ProjectArchived { .. } => ARCHIVED,
            Self::ProjectUnarchived { .. } => UNARCHIVED,
            Self::ProjectMemberAdded { .. } => MEMBER_ADDED,
            Self::ProjectMemberRemoved { .. } => MEMBER_REMOVED,
        }
    }
}

/// The key two names are compared by: trimmed, lower-cased.
#[must_use]
pub fn name_key(name: &str) -> String {
    name.trim().to_lowercase()
}

/// A valid display name, trimmed.
pub fn check_name(name: &str) -> Result<String, Refusal> {
    let n = name.trim();
    if n.is_empty() {
        return Err(refuse("PROJECT_NAME_INVALID", "a project needs a name"));
    }
    if n.chars().count() > MAX_NAME_CHARS {
        return Err(refuse(
            "PROJECT_NAME_INVALID",
            format!("a project name has at most {MAX_NAME_CHARS} characters"),
        ));
    }
    if n.chars().any(char::is_control) {
        return Err(refuse(
            "PROJECT_NAME_INVALID",
            "a project name holds no control characters",
        ));
    }
    Ok(n.to_owned())
}

/// A palette role of [`COLORS`]; empty means "keep" (`current`) or the default.
pub fn check_color(color: &str, current: Option<&str>) -> Result<String, Refusal> {
    if color.is_empty() {
        return Ok(current.unwrap_or(DEFAULT_COLOR).to_owned());
    }
    if COLORS.contains(&color) {
        return Ok(color.to_owned());
    }
    Err(refuse(
        "PROJECT_COLOR_INVALID",
        format!(
            "`{color}` is not a palette role; use one of {}",
            COLORS.join(", ")
        ),
    ))
}

/// An icon of [`ICONS`]; empty means "keep" (`current`) or the default.
pub fn check_icon(icon: &str, current: Option<&str>) -> Result<String, Refusal> {
    if icon.is_empty() {
        return Ok(current.unwrap_or(DEFAULT_ICON).to_owned());
    }
    if ICONS.contains(&icon) {
        return Ok(icon.to_owned());
    }
    Err(refuse(
        "PROJECT_ICON_INVALID",
        format!(
            "`{icon}` is not a project icon; use one of {}",
            ICONS.join(", ")
        ),
    ))
}

/// What the Core knows about a task that decides whether it may join.
#[derive(Clone, Debug)]
pub struct Candidate<'a> {
    /// The task.
    pub task_id: TaskId,
    /// Where it came from.
    pub origin: TaskOrigin,
    /// Its lifecycle state.
    pub state: TaskState,
    /// Whether the person archived its conversation.
    pub archived: bool,
    /// The workspace root it works in.
    pub workspace_root: Option<&'a str>,
}

/// What the Core knows about the project a task would join.
#[derive(Clone, Debug)]
pub struct Target<'a> {
    /// Identity.
    pub project_id: ProjectId,
    /// Display name (for the sentence).
    pub name: &'a str,
    /// The workspace root it is bound to.
    pub workspace_root: &'a str,
    /// Whether it is archived.
    pub archived: bool,
    /// How many tasks it holds now.
    pub members: usize,
}

/// A task that is not a top-level task of the person's own: a subagent's, a
/// disposable review environment's or a counterfactual replay's. They exist
/// inside another task's work and are shown inside it.
#[must_use]
pub const fn is_top_level(origin: TaskOrigin) -> bool {
    !matches!(
        origin,
        TaskOrigin::Subagent | TaskOrigin::Review | TaskOrigin::Replay
    )
}

/// A draft: created or queued and not yet started (the list shows it as a
/// Draft). A draft is not yet work to group.
#[must_use]
pub const fn is_draft(state: TaskState, archived: bool) -> bool {
    matches!(state, TaskState::Created | TaskState::Queued) && !archived
}

/// Whether `task` may join `project`, given the project it is in now (if
/// any). The rules of AFW-J03 in the order a person would meet them.
pub fn check_add(
    target: &Target<'_>,
    task: &Candidate<'_>,
    in_project: Option<(ProjectId, &str)>,
) -> Result<(), Refusal> {
    if target.archived {
        return Err(refuse(
            "PROJECT_ARCHIVED",
            format!(
                "`{}` is archived; restore it before adding to it",
                target.name
            ),
        ));
    }
    if !is_top_level(task.origin) {
        return Err(refuse(
            "TASK_NOT_TOP_LEVEL",
            "only a top-level task can join a project; this task belongs to another task's work",
        ));
    }
    if is_draft(task.state, task.archived) {
        return Err(refuse(
            "TASK_IS_DRAFT",
            "a draft task cannot join a project; start it first",
        ));
    }
    match task.workspace_root {
        Some(root) if root == target.workspace_root => {}
        Some(root) => {
            return Err(refuse(
                "WORKSPACE_MISMATCH",
                format!(
                    "the task works in `{root}` and `{}` belongs to `{}`; a project holds tasks of its own workspace only",
                    target.name, target.workspace_root
                ),
            ));
        }
        None => {
            return Err(refuse(
                "WORKSPACE_MISMATCH",
                "the task has no workspace, so it cannot join a workspace's project",
            ));
        }
    }
    if let Some((other, other_name)) = in_project
        && other != target.project_id
    {
        return Err(refuse(
            "IN_ANOTHER_PROJECT",
            format!(
                "the task is in `{other_name}`; remove it from there first, a task is in one project at a time"
            ),
        ));
    }
    if target.members >= MAX_MEMBERS {
        return Err(refuse(
            "PROJECT_FULL",
            format!("a project holds at most {MAX_MEMBERS} tasks"),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(archived: bool, members: usize) -> Target<'static> {
        Target {
            project_id: ProjectId::from_bytes([1; 16]),
            name: "Alpha",
            workspace_root: "/w/a",
            archived,
            members,
        }
    }

    fn task(
        origin: TaskOrigin,
        state: TaskState,
        root: Option<&'static str>,
    ) -> Candidate<'static> {
        Candidate {
            task_id: TaskId::from_bytes([2; 16]),
            origin,
            state,
            archived: false,
            workspace_root: root,
        }
    }

    #[test]
    fn each_guard_has_its_own_code() {
        let ok = task(TaskOrigin::Desktop, TaskState::Running, Some("/w/a"));
        assert!(check_add(&target(false, 0), &ok, None).is_ok());
        let code = |t: &Target<'_>, c: &Candidate<'_>, p| check_add(t, c, p).unwrap_err().0;
        assert_eq!(code(&target(true, 0), &ok, None), "PROJECT_ARCHIVED");
        let sub = task(TaskOrigin::Subagent, TaskState::Running, Some("/w/a"));
        assert_eq!(code(&target(false, 0), &sub, None), "TASK_NOT_TOP_LEVEL");
        let draft = task(TaskOrigin::Desktop, TaskState::Created, Some("/w/a"));
        assert_eq!(code(&target(false, 0), &draft, None), "TASK_IS_DRAFT");
        let other = task(TaskOrigin::Desktop, TaskState::Running, Some("/w/b"));
        assert_eq!(code(&target(false, 0), &other, None), "WORKSPACE_MISMATCH");
        let elsewhere = Some((ProjectId::from_bytes([9; 16]), "Beta"));
        assert_eq!(
            code(&target(false, 0), &ok, elsewhere),
            "IN_ANOTHER_PROJECT"
        );
        assert_eq!(code(&target(false, MAX_MEMBERS), &ok, None), "PROJECT_FULL");
        // Already a member of this very project is not a refusal.
        let here = Some((ProjectId::from_bytes([1; 16]), "Alpha"));
        assert!(check_add(&target(false, 3), &ok, here).is_ok());
    }

    #[test]
    fn names_colours_and_icons_are_validated() {
        assert_eq!(check_name("  Release 2  ").unwrap(), "Release 2");
        assert!(check_name("   ").is_err());
        assert!(check_name(&"x".repeat(MAX_NAME_CHARS + 1)).is_err());
        assert!(check_name("a\u{7}b").is_err());
        assert_eq!(check_color("", None).unwrap(), "accent");
        assert_eq!(check_color("", Some("warn")).unwrap(), "warn");
        assert!(check_color("#ff0000", None).is_err());
        assert!(check_icon("rocket", None).is_ok());
        assert!(check_icon("skull", None).is_err());
        assert_eq!(name_key(" AbC "), "abc");
    }

    #[test]
    fn events_carry_their_type_name() {
        let e = ProjectEvent::ProjectArchived {
            project_id: ProjectId::from_bytes([1; 16]),
        };
        let v = serde_json::to_value(&e).unwrap();
        assert_eq!(v["event_type"], ARCHIVED);
        assert_eq!(e.event_type(), ARCHIVED);
        let back: ProjectEvent = serde_json::from_value(v).unwrap();
        assert_eq!(back, e);
    }
}
