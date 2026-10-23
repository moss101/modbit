//! Read-only file browsing of a task's workspace (REQ-PX-048, docs/65
//! AFW-I01): `ListWorkspaceDir` and `ReadWorkspaceFile`. Both go through the
//! Workspace File Service's path policy, which normalizes the path, resolves
//! symlinks and refuses anything that leaves the root or matches a protected
//! pattern; a protected child is listed by name and never opened. Nothing here
//! writes, and a file that is binary or larger than the inline bound is
//! refused with a typed status instead of being sent.

use modbit_domain::TaskId;
use modbit_protocol::v1 as wire;
use modbit_workspace::service::{EntryKind, WorkspaceService, content_hash};
use prost::Message;

use crate::server::{Core, accept, id16, reject};

/// The largest file shown inline (the same bound as the review's code view).
const INLINE_LIMIT: u64 = 256 * 1024;
/// The most entries one listing carries.
const LIST_LIMIT: usize = 2000;

fn code_of(e: &modbit_workspace::Error) -> &'static str {
    match e {
        modbit_workspace::Error::OutsideRoot { .. } => "OUTSIDE_ROOT",
        modbit_workspace::Error::Protected { .. } => "PROTECTED",
        modbit_workspace::Error::Io { source, .. }
            if source.kind() == std::io::ErrorKind::NotFound =>
        {
            "NOT_FOUND"
        }
        _ => "PATH",
    }
}

fn io_code(e: &std::io::Error) -> &'static str {
    if e.kind() == std::io::ErrorKind::NotFound {
        "NOT_FOUND"
    } else {
        "PATH"
    }
}

async fn workspace_of(
    core: &Core,
    task_id: Option<&wire::Id>,
) -> Result<std::sync::Arc<tokio::sync::Mutex<WorkspaceService>>, (String, String)> {
    let Some(id) = task_id.and_then(id16) else {
        return Err(("BAD_PAYLOAD".into(), "task_id must be 16 bytes".into()));
    };
    let task_id = TaskId::from_bytes(id);
    let task = core
        .store
        .lock()
        .await
        .task(&task_id)
        .map_err(|e| ("STORE".to_owned(), e.to_string()))?
        .ok_or_else(|| ("UNKNOWN_TASK".to_owned(), task_id.to_string()))?;
    let root = task.workspace_root.ok_or_else(|| {
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
    Ok(ws)
}

/// `ListWorkspaceDir`.
pub(crate) async fn list(core: &Core, env: wire::CommandEnvelope) -> wire::CommandAck {
    let cid = env.command_id.clone();
    let Ok(p) = wire::ListWorkspaceDir::decode(env.payload.as_slice()) else {
        return reject(cid, "BAD_PAYLOAD", "ListWorkspaceDir");
    };
    let ws = match workspace_of(core, p.task_id.as_ref()).await {
        Ok(w) => w,
        Err((code, msg)) => return reject(cid, &code, msg),
    };
    let ws = ws.lock().await;
    let resolved = match ws.resolve(&p.path) {
        Ok(r) => r,
        Err(e) => return reject(cid, code_of(&e), e.to_string()),
    };
    let rd = match std::fs::read_dir(&resolved.absolute) {
        Ok(rd) => rd,
        Err(e) => return reject(cid, io_code(&e), e.to_string()),
    };
    let mut entries = Vec::new();
    let mut truncated = false;
    for entry in rd.flatten() {
        if entries.len() >= LIST_LIMIT {
            truncated = true;
            break;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        let rel = if resolved.relative.is_empty() {
            name.clone()
        } else {
            format!("{}/{name}", resolved.relative)
        };
        let Ok(meta) = std::fs::symlink_metadata(entry.path()) else {
            continue;
        };
        let ft = meta.file_type();
        let kind = if ft.is_dir() {
            EntryKind::Dir
        } else if ft.is_file() {
            EntryKind::File
        } else if ft.is_symlink() {
            EntryKind::Symlink
        } else {
            EntryKind::Other
        };
        // A path the policy refuses (a protected pattern, a link out of the root) is named, never opened.
        let protected = ws.resolve(&rel).is_err();
        entries.push(wire::WorkspaceEntryView {
            path: rel,
            name,
            kind: match kind {
                EntryKind::File => "file",
                EntryKind::Dir => "dir",
                EntryKind::Symlink => "symlink",
                EntryKind::Other => "other",
            }
            .into(),
            size: if kind == EntryKind::File {
                meta.len()
            } else {
                0
            },
            protected,
        });
    }
    // Directories first, then by name: the order a file tree shows.
    entries.sort_by(|a, b| {
        (a.kind != "dir")
            .cmp(&(b.kind != "dir"))
            .then_with(|| a.name.cmp(&b.name))
    });
    accept(
        cid,
        false,
        wire::WorkspaceDirListing {
            path: resolved.relative,
            entries,
            workspace_revision: ws.revision().number,
            truncated,
        }
        .encode_to_vec(),
    )
}

/// `ReadWorkspaceFile`.
pub(crate) async fn read(core: &Core, env: wire::CommandEnvelope) -> wire::CommandAck {
    let cid = env.command_id.clone();
    let Ok(p) = wire::ReadWorkspaceFile::decode(env.payload.as_slice()) else {
        return reject(cid, "BAD_PAYLOAD", "ReadWorkspaceFile");
    };
    let ws = match workspace_of(core, p.task_id.as_ref()).await {
        Ok(w) => w,
        Err((code, msg)) => return reject(cid, &code, msg),
    };
    let ws = ws.lock().await;
    let resolved = match ws.resolve(&p.path) {
        Ok(r) => r,
        Err(e) => return reject(cid, code_of(&e), e.to_string()),
    };
    let meta = match std::fs::metadata(&resolved.absolute) {
        Ok(m) => m,
        Err(e) => return reject(cid, io_code(&e), e.to_string()),
    };
    if !meta.is_file() {
        return reject(
            cid,
            "NOT_A_FILE",
            format!("`{}` is not a regular file", p.path),
        );
    }
    let mut view = wire::WorkspaceFileView {
        path: resolved.relative.clone(),
        status: "TOO_LARGE".into(),
        size: meta.len(),
        workspace_revision: ws.revision().number,
        language: crate::review::language_for(&resolved.relative).into(),
        ..Default::default()
    };
    if meta.len() > INLINE_LIMIT {
        return accept(cid, false, view.encode_to_vec());
    }
    let bytes = match std::fs::read(&resolved.absolute) {
        Ok(b) => b,
        Err(e) => return reject(cid, "PATH", e.to_string()),
    };
    view.file_revision = content_hash(&bytes);
    view.size = bytes.len() as u64;
    match String::from_utf8(bytes) {
        Ok(text) if !text.contains('\0') => {
            view.status = "TEXT".into();
            view.text = text;
        }
        _ => view.status = "BINARY".into(),
    }
    accept(cid, false, view.encode_to_vec())
}
