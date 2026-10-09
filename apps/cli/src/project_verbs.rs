//! `modbit-cli project ...` (PX-063; docs/65 AFW-J01..J03): the headless
//! verbs over the same SurfaceProtocol commands the desktop calls —
//! `ListProjects`, `GetProject`, `CreateProject`, `RenameProject`,
//! `ArchiveProject`, `AddProjectMember`, `RemoveProjectMember`. The CLI holds
//! no project logic: it decodes arguments, sends the command under the session
//! lease and prints what the Core answered, including its typed refusal.
//!
//! ```text
//! project list   [--workspace <dir>] [--archived] [--json]
//! project show   <project-id> [--json]
//! project create --session <id> --workspace <dir> [--color c] [--icon i] <name>
//! project rename --session <id> --project <id> [--color c] [--icon i] [<name>]
//! project archive --session <id> --project <id> [--undo]
//! project add    --session <id> --project <id> --task <id>
//! project remove --session <id> --project <id> --task <id>
//! ```

use modbit_protocol::client::Client;
use modbit_protocol::local::encode_hex;
use modbit_protocol::v1::{
    AddProjectMember, ArchiveProject, CreateProject, GetProject, Id, ListProjects, ProjectChanged,
    ProjectList, ProjectView, RemoveProjectMember, RenameProject,
};
use prost::Message;

use crate::{USAGE, envelope, envelope_fenced, join_lease, parse_id};

#[derive(Default)]
struct Args {
    session: Option<String>,
    project: Option<String>,
    task: Option<String>,
    workspace: Option<String>,
    color: Option<String>,
    icon: Option<String>,
    archived: bool,
    undo: bool,
    json: bool,
    positional: Vec<String>,
}

fn parse(words: &[&str]) -> Result<Args, String> {
    let mut a = Args::default();
    let mut it = words.iter().copied();
    while let Some(w) = it.next() {
        let mut value = |flag: &str| {
            it.next()
                .map(str::to_owned)
                .ok_or_else(|| format!("{flag} takes a value"))
        };
        match w {
            "--session" => a.session = Some(value(w)?),
            "--project" => a.project = Some(value(w)?),
            "--task" => a.task = Some(value(w)?),
            "--workspace" => a.workspace = Some(value(w)?),
            "--color" => a.color = Some(value(w)?),
            "--icon" => a.icon = Some(value(w)?),
            "--archived" => a.archived = true,
            "--undo" => a.undo = true,
            "--json" => a.json = true,
            other if other.starts_with("--") => return Err(format!("unknown flag `{other}`")),
            other => a.positional.push(other.to_owned()),
        }
    }
    Ok(a)
}

fn hex(id: &Option<Id>) -> String {
    id.as_ref()
        .map(|i| encode_hex(&i.value))
        .unwrap_or_default()
}

fn view_json(p: &ProjectView) -> serde_json::Value {
    let r = p.rollup.clone().unwrap_or_default();
    serde_json::json!({
        "project_id": hex(&p.project_id),
        "name": p.name, "color": p.color, "icon": p.icon,
        "workspace_root": p.workspace_root, "archived": p.archived,
        "created_at_ms": p.created_at_ms, "updated_at_ms": p.updated_at_ms,
        "rollup": {
            "members": r.members,
            "by_status": r.by_status.iter().map(|s| serde_json::json!({"label": s.label, "count": s.count})).collect::<Vec<_>>(),
            "attention_tasks": r.attention_tasks, "attention_items": r.attention_items,
            "pending_approvals": r.pending_approvals, "unread": r.unread,
            "pull_requests": r.pull_requests, "ci_passing": r.ci_passing,
            "ci_failing": r.ci_failing, "ci_pending": r.ci_pending,
        },
        "members": p.members.iter().map(|m| serde_json::json!({
            "task_id": hex(&m.task_id), "title": m.title, "status": m.status_label,
            "attention_items": m.attention_items, "archived": m.archived,
            "pull_request": m.pull_request.as_ref().map(|pr| serde_json::json!({
                "number": pr.number, "url": pr.url, "has_ci": pr.has_ci,
                "checks_passed": pr.checks_passed, "checks_failed": pr.checks_failed,
                "checks_pending": pr.checks_pending,
            })),
        })).collect::<Vec<_>>(),
    })
}

fn print_view(p: &ProjectView, full: bool) {
    let r = p.rollup.clone().unwrap_or_default();
    let by: Vec<String> = r
        .by_status
        .iter()
        .map(|s| format!("{} {}", s.count, s.label.to_lowercase()))
        .collect();
    println!(
        "{} {}{} [{} {}] {} task(s){}{}",
        hex(&p.project_id),
        p.name,
        if p.archived { " (archived)" } else { "" },
        p.color,
        p.icon,
        r.members,
        if by.is_empty() {
            String::new()
        } else {
            format!(": {}", by.join(", "))
        },
        if r.attention_items > 0 {
            format!("; {} attention item(s)", r.attention_items)
        } else {
            String::new()
        },
    );
    if full {
        println!("  workspace: {}", p.workspace_root);
        if r.pull_requests > 0 {
            println!(
                "  pull requests: {} (CI passing {}, failing {}, pending {})",
                r.pull_requests, r.ci_passing, r.ci_failing, r.ci_pending
            );
        }
        for m in &p.members {
            println!("  {} {} [{}]", hex(&m.task_id), m.title, m.status_label);
        }
    }
}

pub(crate) async fn run(client: &mut Client, words: &[&str]) -> Result<(), String> {
    let Some((verb, rest)) = words.split_first() else {
        return Err(USAGE.into());
    };
    let a = parse(rest)?;
    let mutating = matches!(*verb, "create" | "rename" | "archive" | "add" | "remove");
    let (session, lease) = if mutating {
        let sid = parse_id(a.session.as_deref().ok_or(USAGE)?)?;
        let g = join_lease(client, &sid).await?;
        (Some(sid), Some(g))
    } else {
        (None, None)
    };
    let project = || -> Result<Id, String> { parse_id(a.project.as_deref().ok_or(USAGE)?) };
    let task = || -> Result<Id, String> { parse_id(a.task.as_deref().ok_or(USAGE)?) };
    let send = |kind: &str, payload: Vec<u8>| envelope_fenced(kind, payload, lease);
    match *verb {
        "list" => {
            let ack = client
                .command(envelope(
                    "ListProjects",
                    ListProjects {
                        workspace_root: a.workspace.clone().unwrap_or_default(),
                        include_archived: a.archived,
                    }
                    .encode_to_vec(),
                ))
                .await
                .map_err(|e| e.to_string())?;
            let l: ProjectList = Client::result(&ack).map_err(|e| e.to_string())?;
            if a.json {
                println!(
                    "{}",
                    serde_json::json!({"projects": l.projects.iter().map(view_json).collect::<Vec<_>>()})
                );
                return Ok(());
            }
            if l.projects.is_empty() {
                println!("no projects");
            }
            for p in &l.projects {
                print_view(p, false);
            }
        }
        "show" => {
            let id = parse_id(a.positional.first().ok_or(USAGE)?)?;
            let ack = client
                .command(envelope(
                    "GetProject",
                    GetProject {
                        project_id: Some(id),
                    }
                    .encode_to_vec(),
                ))
                .await
                .map_err(|e| e.to_string())?;
            let r: ProjectChanged = Client::result(&ack).map_err(|e| e.to_string())?;
            let p = r.project.unwrap_or_default();
            if a.json {
                println!("{}", view_json(&p));
            } else {
                print_view(&p, true);
            }
        }
        _ => {
            let (kind, payload) = match *verb {
                "create" => (
                    "CreateProject",
                    CreateProject {
                        session_id: session,
                        name: a.positional.join(" "),
                        color: a.color.clone().unwrap_or_default(),
                        icon: a.icon.clone().unwrap_or_default(),
                        workspace_root: a.workspace.clone().ok_or(USAGE)?,
                    }
                    .encode_to_vec(),
                ),
                "rename" => (
                    "RenameProject",
                    RenameProject {
                        session_id: session,
                        project_id: Some(project()?),
                        name: a.positional.join(" "),
                        color: a.color.clone().unwrap_or_default(),
                        icon: a.icon.clone().unwrap_or_default(),
                    }
                    .encode_to_vec(),
                ),
                "archive" => (
                    "ArchiveProject",
                    ArchiveProject {
                        session_id: session,
                        project_id: Some(project()?),
                        archived: !a.undo,
                    }
                    .encode_to_vec(),
                ),
                "add" => (
                    "AddProjectMember",
                    AddProjectMember {
                        session_id: session,
                        project_id: Some(project()?),
                        task_id: Some(task()?),
                    }
                    .encode_to_vec(),
                ),
                "remove" => (
                    "RemoveProjectMember",
                    RemoveProjectMember {
                        session_id: session,
                        project_id: Some(project()?),
                        task_id: Some(task()?),
                    }
                    .encode_to_vec(),
                ),
                _ => return Err(USAGE.into()),
            };
            let ack = client
                .command(send(kind, payload))
                .await
                .map_err(|e| e.to_string())?;
            let r: ProjectChanged = Client::result(&ack).map_err(|e| e.to_string())?;
            let p = r.project.unwrap_or_default();
            if a.json {
                println!("{}", view_json(&p));
            } else {
                print_view(&p, false);
            }
        }
    }
    Ok(())
}
