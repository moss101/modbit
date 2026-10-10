//! REQ-PX-048 (the apps panel's Files app) on the real Core: `ListWorkspaceDir`
//! and `ReadWorkspaceFile` over the real socket, against a real Git checkout.
//! The path policy is the Workspace File Service's own: a path outside the
//! root, a `..` climb, an absolute path, a protected file and a symlink that
//! leaves the root are refused with typed codes, never read; a binary file and
//! an oversized one come back with a typed status and no content.
#![cfg(unix)]

mod px_common;

use modbit_protocol::client::{Client, ClientError};
use modbit_protocol::v1::{
    ClientKind, ListWorkspaceDir, ReadWorkspaceFile, WorkspaceDirListing, WorkspaceFileView,
};
use prost::Message;
use px_common::*;

async fn list(
    c: &mut Client,
    task: &modbit_protocol::v1::Id,
    path: &str,
) -> Result<WorkspaceDirListing, ClientError> {
    let ack = c
        .command(envelope(
            rand_id(),
            "ListWorkspaceDir",
            ListWorkspaceDir {
                task_id: Some(task.clone()),
                path: path.into(),
            }
            .encode_to_vec(),
        ))
        .await?;
    Client::result::<WorkspaceDirListing>(&ack)
}

async fn read(
    c: &mut Client,
    task: &modbit_protocol::v1::Id,
    path: &str,
) -> Result<WorkspaceFileView, ClientError> {
    let ack = c
        .command(envelope(
            rand_id(),
            "ReadWorkspaceFile",
            ReadWorkspaceFile {
                task_id: Some(task.clone()),
                path: path.into(),
            }
            .encode_to_vec(),
        ))
        .await?;
    Client::result::<WorkspaceFileView>(&ack)
}

fn refused(e: &ClientError, code: &str) -> bool {
    format!("{e:?}").contains(code)
}

#[tokio::test(flavor = "multi_thread")]
async fn the_files_app_reads_the_workspace_through_the_path_policy() {
    let (repo, root) = plain_repo(&[("src/lib.rs", "pub fn f() {}\n"), ("notes.txt", "hello\n")]);
    std::fs::write(repo.path().join("blob.bin"), [0u8, 159, 146, 150, 0, 1]).unwrap();
    std::fs::write(repo.path().join("big.txt"), vec![b'a'; 300 * 1024]).unwrap();
    std::fs::write(repo.path().join(".env"), "SECRET=1\n").unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(outside.path().join("secret.txt"), "outside\n").unwrap();
    std::os::unix::fs::symlink(outside.path(), repo.path().join("escape")).unwrap();

    let dir = tempfile::tempdir().unwrap();
    let core = CoreProcess::spawn_with_env(dir.path(), &[]);
    let mut c = core.client_of_kind(ClientKind::Desktop).await;
    let (session, g) = session_with_lease(&mut c, 0x30).await;
    let task = create_task(
        &mut c,
        &session,
        g,
        &root,
        0x41,
        "local_trusted",
        "browse files",
    )
    .await;

    // The root lists directories first, names a protected file without opening it.
    let root_list = list(&mut c, &task, "").await.unwrap();
    let names: Vec<&str> = root_list.entries.iter().map(|e| e.name.as_str()).collect();
    assert!(
        names.contains(&"src") && names.contains(&"notes.txt") && names.contains(&"blob.bin"),
        "{names:?}"
    );
    let first_file = root_list
        .entries
        .iter()
        .position(|e| e.kind != "dir")
        .unwrap();
    assert!(
        root_list.entries[..first_file]
            .iter()
            .all(|e| e.kind == "dir"),
        "directories lead: {names:?}"
    );
    assert!(
        root_list
            .entries
            .iter()
            .find(|e| e.name == ".env")
            .is_some_and(|e| e.protected)
    );
    assert!(
        root_list
            .entries
            .iter()
            .find(|e| e.name == "escape")
            .is_some_and(|e| e.protected),
        "a link out of the root is named, never followed"
    );
    let src = list(&mut c, &task, "src").await.unwrap();
    assert_eq!(src.entries.len(), 1);
    assert_eq!(src.entries[0].path, "src/lib.rs");

    // A text file comes back whole; binary and oversized files come back typed and empty.
    let text = read(&mut c, &task, "src/lib.rs").await.unwrap();
    assert_eq!(
        (
            text.status.as_str(),
            text.text.as_str(),
            text.language.as_str()
        ),
        ("TEXT", "pub fn f() {}\n", "rust")
    );
    assert!(!text.file_revision.is_empty());
    let bin = read(&mut c, &task, "blob.bin").await.unwrap();
    assert_eq!((bin.status.as_str(), bin.text.as_str()), ("BINARY", ""));
    let big = read(&mut c, &task, "big.txt").await.unwrap();
    assert_eq!(
        (big.status.as_str(), big.text.as_str(), big.size),
        ("TOO_LARGE", "", 300 * 1024)
    );

    // Outside the root, protected, missing: typed refusals.
    for (path, code) in [
        ("../outside.txt", "OUTSIDE_ROOT"),
        ("escape/secret.txt", "OUTSIDE_ROOT"),
        (".env", "PROTECTED"),
        ("nope.txt", "NOT_FOUND"),
    ] {
        let e = read(&mut c, &task, path).await.unwrap_err();
        assert!(refused(&e, code), "{path}: want {code}, got {e:?}");
    }
    let abs = outside.path().join("secret.txt");
    let e = read(&mut c, &task, abs.to_str().unwrap())
        .await
        .unwrap_err();
    assert!(refused(&e, "OUTSIDE_ROOT"), "absolute path: {e:?}");
    let e = list(&mut c, &task, "..").await.unwrap_err();
    assert!(refused(&e, "OUTSIDE_ROOT"), "{e:?}");
    let e = list(&mut c, &task, "escape").await.unwrap_err();
    assert!(refused(&e, "OUTSIDE_ROOT"), "{e:?}");
    let e = read(&mut c, &task, "src").await.unwrap_err();
    assert!(refused(&e, "NOT_A_FILE"), "{e:?}");
}
