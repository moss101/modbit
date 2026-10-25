//! QUAL-PX-106 (the Core's half of the Customize view) against the real Core
//! process over its real socket, a real SQLite event store and real hook
//! processes (`sh` scripts): a hook switched off with `SetHookEnabled` fires on
//! no later event, the switch survives a SIGKILL, a layer above the person
//! cannot be switched off, the hook list carries the journal's last result,
//! and the session's extensions are structured rows (quarantined and inert
//! until trusted by the exact manifest digest, with their tool servers).
//! No model is involved: tools are invoked through the Core's own pipeline.
#![cfg(unix)]

mod px_common;

use modbit_protocol::client::{Client, ClientError};
use modbit_protocol::v1::{
    AcquireSessionLease, ExtensionLoadedView, HookEnabledSet, HookListView, Id, ListHooks,
    LoadExtension, SessionLeaseAcquired, SetHookEnabled, TrustExtension, UnloadExtension,
};
use prost::Message;
use px_common::mcp_support::invoke_tool;
use px_common::*;
use serde_json::{Value, json};

type Reject = (String, String);

fn script(dir: &std::path::Path, name: &str, body: &str) -> Vec<String> {
    let path = dir.join(name);
    std::fs::write(&path, body).unwrap();
    vec!["sh".to_owned(), path.to_string_lossy().replace('\\', "/")]
}

/// A handler that appends one line to `log` per invocation and answers nothing.
fn recorder(dir: &std::path::Path, name: &str, log: &std::path::Path) -> Vec<String> {
    let log = log.to_string_lossy().replace('\\', "/");
    script(
        dir,
        name,
        &format!("cat > /dev/null\necho fired >> '{log}'\n"),
    )
}

fn fired(log: &std::path::Path) -> usize {
    std::fs::read_to_string(log)
        .unwrap_or_default()
        .lines()
        .count()
}

fn hook(name: &str, command: Vec<String>) -> Value {
    json!({"name": name, "point": "before_tool", "mode": "observe", "command": command, "tools": ["fs.read"]})
}

struct Fx {
    core: CoreProcess,
    c: Client,
    session: Id,
    g: Option<u64>,
    root: String,
    dir: tempfile::TempDir,
    n: u8,
    _repo: tempfile::TempDir,
}

impl Fx {
    fn id(&mut self) -> u8 {
        self.n += 1;
        self.n
    }

    async fn task(&mut self) -> Id {
        let id = self.id();
        create_task(
            &mut self.c,
            &self.session,
            self.g,
            &self.root,
            id,
            "local_trusted",
            "customize",
        )
        .await
    }

    async fn read(&mut self, task: &Id) {
        let (a, b) = (self.id(), self.id());
        let r = invoke_tool(
            &mut self.c,
            task,
            self.g,
            a,
            b,
            "fs.read",
            r#"{"path":"notes.md"}"#,
        )
        .await;
        assert_eq!(r.status, "SUCCESS", "{r:?}");
    }

    async fn list(&mut self, task: &Id) -> HookListView {
        let id = self.id();
        let ack = self
            .c
            .command(envelope(
                id16(id),
                "ListHooks",
                ListHooks {
                    task_id: Some(task.clone()),
                    ..Default::default()
                }
                .encode_to_vec(),
            ))
            .await
            .unwrap();
        Client::result(&ack).unwrap()
    }

    async fn list_session(&mut self) -> HookListView {
        let id = self.id();
        let ack = self
            .c
            .command(envelope(
                id16(id),
                "ListHooks",
                ListHooks {
                    session_id: Some(self.session.clone()),
                    ..Default::default()
                }
                .encode_to_vec(),
            ))
            .await
            .unwrap();
        Client::result(&ack).unwrap()
    }

    async fn set(
        &mut self,
        task: &Id,
        hook: &str,
        enabled: bool,
        fenced: bool,
    ) -> Result<HookEnabledSet, Reject> {
        let id = self.id();
        let g = if fenced { self.g } else { None };
        match self
            .c
            .command(envelope_fenced(
                id16(id),
                "SetHookEnabled",
                SetHookEnabled {
                    session_id: Some(self.session.clone()),
                    task_id: Some(task.clone()),
                    hook_id: hook.into(),
                    enabled,
                }
                .encode_to_vec(),
                g,
            ))
            .await
        {
            Ok(ack) => Ok(Client::result(&ack).unwrap()),
            Err(ClientError::Rejected { code, message }) => Err((code, message)),
            Err(e) => panic!("SetHookEnabled: {e}"),
        }
    }

    async fn ext<M: Message>(&mut self, kind: &str, m: M) -> Result<ExtensionLoadedView, Reject> {
        let id = self.id();
        let g = self.g;
        match self
            .c
            .command(envelope_fenced(id16(id), kind, m.encode_to_vec(), g))
            .await
        {
            Ok(ack) => Ok(Client::result(&ack).unwrap()),
            Err(ClientError::Rejected { code, message }) => Err((code, message)),
            Err(e) => panic!("{kind}: {e}"),
        }
    }

    async fn restart(&mut self) {
        self.core.kill();
        self.core = CoreProcess::spawn_with_env(self.dir.path(), &[]);
        self.c = self.core.client().await;
        let id = self.id();
        let ack = self
            .c
            .command(envelope(
                id16(id),
                "AcquireSessionLease",
                AcquireSessionLease {
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
}

async fn fixture(user: &[Value], admin: &[Value]) -> Fx {
    let (repo, root) = plain_repo(&[("notes.md", "notes\n")]);
    let dir = tempfile::tempdir().unwrap();
    let layer = |hooks: &[Value]| {
        json!({"hooks": hooks.iter().map(Value::to_string).collect::<Vec<_>>()}).to_string()
    };
    std::fs::write(dir.path().join("config.json"), layer(user)).unwrap();
    std::fs::write(dir.path().join("admin-config.json"), layer(admin)).unwrap();
    let core = CoreProcess::spawn_with_env(dir.path(), &[]);
    let mut c = core.client().await;
    let (session, g) = session_with_lease(&mut c, 0x30).await;
    Fx {
        core,
        c,
        session,
        g,
        root,
        dir,
        n: 0x40,
        _repo: repo,
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn qual_px_106_a_hook_switched_off_fires_on_no_later_event_and_the_switch_survives_a_sigkill()
{
    let scripts = tempfile::tempdir().unwrap();
    let (log_user, log_admin) = (
        scripts.path().join("user.log"),
        scripts.path().join("admin.log"),
    );
    let user = hook("user-rec", recorder(scripts.path(), "u.sh", &log_user));
    let admin = hook("admin-rec", recorder(scripts.path(), "a.sh", &log_admin));
    let mut fx = fixture(&[user], &[admin]).await;
    let t1 = fx.task().await;

    // Both layers fire on a tool event.
    fx.read(&t1).await;
    assert_eq!((fired(&log_user), fired(&log_admin)), (1, 1));
    // The list: scope, the person's right to switch it off, and the journal's last result.
    let l = fx.list(&t1).await;
    let u = l
        .hooks
        .iter()
        .find(|h| h.id.ends_with("/user-rec"))
        .expect("user hook listed");
    assert_eq!(
        (u.scope.as_str(), u.enabled, u.can_disable),
        ("USER", true, true),
        "{u:?}"
    );
    assert_eq!((u.invocations, u.last_outcome.as_str()), (1, "OK"), "{u:?}");
    let a = l
        .hooks
        .iter()
        .find(|h| h.id.ends_with("/admin-rec"))
        .unwrap();
    assert_eq!((a.scope.as_str(), a.can_disable), ("ADMIN", false), "{a:?}");
    let (uid, aid) = (u.id.clone(), a.id.clone());

    // A layer above the person cannot be switched off; an unknown id and a call
    // without the session lease are refused; nothing was recorded by any of them.
    assert_eq!(
        fx.set(&t1, &aid, false, true).await.unwrap_err().0,
        "HOOK_SCOPE_FORBIDS"
    );
    assert_eq!(
        fx.set(&t1, "config:user/nope", false, true)
            .await
            .unwrap_err()
            .0,
        "HOOK_UNKNOWN"
    );
    assert!(
        fx.set(&t1, &uid, false, false).await.is_err(),
        "an unfenced switch is refused"
    );
    fx.read(&t1).await;
    assert_eq!(
        (fired(&log_user), fired(&log_admin)),
        (2, 2),
        "nothing was switched off yet"
    );

    // Switch the person's hook off: it fires on no later event, the admin's still does.
    let ack = fx.set(&t1, &uid, false, true).await.unwrap();
    assert_eq!((ack.enabled, ack.scope.as_str()), (false, "USER"));
    fx.read(&t1).await;
    assert_eq!(
        (fired(&log_user), fired(&log_admin)),
        (2, 3),
        "the disabled hook did not fire"
    );
    let l = fx.list(&t1).await;
    let u = l
        .hooks
        .iter()
        .find(|h| h.id == uid)
        .expect("a disabled hook is still listed");
    assert!(!u.enabled && u.can_disable && u.invocations == 2, "{u:?}");
    // It is for the session: a task made later is covered too.
    let t2 = fx.task().await;
    fx.read(&t2).await;
    assert_eq!((fired(&log_user), fired(&log_admin)), (2, 4));

    // A hard kill: the switch is on the log, so the restarted Core agrees.
    fx.restart().await;
    let t3 = fx.task().await;
    fx.read(&t3).await;
    assert_eq!(
        (fired(&log_user), fired(&log_admin)),
        (2, 5),
        "still off after SIGKILL"
    );
    assert!(
        !fx.list(&t3)
            .await
            .hooks
            .iter()
            .find(|h| h.id == uid)
            .unwrap()
            .enabled
    );

    // And back on: it fires again.
    fx.set(&t3, &uid, true, true).await.unwrap();
    fx.read(&t3).await;
    assert_eq!((fired(&log_user), fired(&log_admin)), (3, 6));
    assert!(
        fx.list(&t3)
            .await
            .hooks
            .iter()
            .find(|h| h.id == uid)
            .unwrap()
            .enabled
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn qual_px_106_an_extension_is_a_structured_row_inert_until_its_exact_manifest_is_trusted() {
    let scripts = tempfile::tempdir().unwrap();
    let log = scripts.path().join("ext.log");
    let mut fx = fixture(&[], &[]).await;
    let t1 = fx.task().await;
    // An unsigned extension with a hook, a tool server (named credential, never a value) and a command.
    let ext_dir = scripts.path().join("kit");
    std::fs::create_dir_all(&ext_dir).unwrap();
    let handler = recorder(&ext_dir, "h.sh", &log);
    let file_sha = {
        use sha2::Digest;
        hex::encode(sha2::Sha256::digest(
            std::fs::read(ext_dir.join("h.sh")).unwrap(),
        ))
    };
    let manifest = json!({
        "name": "kit", "version": "1.0.0", "publisher": "Acme", "source": "https://example.test/kit",
        "hooks": [{"name": "kit-rec", "point": "before_tool", "mode": "observe", "command": handler, "tools": ["fs.read"]}],
        "tools": [{"name": "docs", "transport": {"kind": "stdio", "command": "true", "args": []}, "credential": "docs-token", "scopes": ["docs:read"]}],
        "commands": [{"name": "tidy", "description": "tidy up", "template": "tidy {{args}}"}],
        "files": {"h.sh": file_sha},
    });
    std::fs::write(ext_dir.join("modbit-extension.json"), manifest.to_string()).unwrap();
    let path = ext_dir.to_string_lossy().into_owned();

    let loaded = fx
        .ext(
            "LoadExtension",
            LoadExtension {
                session_id: Some(fx.session.clone()),
                path: path.clone(),
                expected_digest: String::new(),
            },
        )
        .await
        .unwrap();
    assert!(
        !loaded.quarantined.is_empty(),
        "unsigned: quarantined {loaded:?}"
    );

    // The session's rows need no task; the extension is inert and says so.
    let rows = fx.list_session().await.extension_rows;
    assert_eq!(rows.len(), 1, "{rows:?}");
    let r = &rows[0];
    assert_eq!(
        (r.name.as_str(), r.state.as_str(), r.scope.as_str()),
        ("kit", "QUARANTINED", "SESSION"),
        "{r:?}"
    );
    assert_eq!(r.manifest_digest, loaded.manifest_digest);
    assert_eq!(r.signature, "UNSIGNED");
    assert!(
        r.can_trust && !r.quarantine_reason.is_empty() && r.path.ends_with("kit"),
        "{r:?}"
    );
    assert_eq!(r.hooks, ["kit-rec@before_tool"]);
    assert_eq!(r.commands, ["tidy"]);
    assert_eq!(r.servers.len(), 1);
    let s = &r.servers[0];
    assert_eq!(
        (
            s.name.as_str(),
            s.state.as_str(),
            s.credential_required,
            s.credential_held
        ),
        ("docs", "INERT", true, false),
        "{s:?}"
    );
    assert_eq!(s.scopes, ["docs:read"]);
    fx.read(&t1).await;
    assert_eq!(fired(&log), 0, "a quarantined extension's hook never runs");
    assert!(fx.list(&t1).await.hooks.is_empty());

    // Trust is bound to the exact manifest.
    let wrong = fx
        .ext(
            "TrustExtension",
            TrustExtension {
                session_id: Some(fx.session.clone()),
                extension_id: r.extension_id.clone(),
                manifest_digest: "0".repeat(64),
            },
        )
        .await
        .unwrap_err();
    assert_eq!(wrong.0, "DIGEST_MISMATCH");
    fx.ext(
        "TrustExtension",
        TrustExtension {
            session_id: Some(fx.session.clone()),
            extension_id: r.extension_id.clone(),
            manifest_digest: r.manifest_digest.clone(),
        },
    )
    .await
    .unwrap();
    let r = fx.list_session().await.extension_rows.remove(0);
    assert_eq!(
        (r.state.as_str(), r.servers[0].state.as_str()),
        ("ACTIVE", "CONFIGURED"),
        "{r:?}"
    );
    assert!(!r.can_trust);
    fx.read(&t1).await;
    assert_eq!(fired(&log), 1, "an active extension's hook runs");
    let l = fx.list(&t1).await;
    let h = l
        .hooks
        .iter()
        .find(|h| h.scope == "EXTENSION")
        .expect("the extension's hook is listed");
    assert!(
        h.enabled && h.can_disable && h.invocations == 1 && h.last_outcome == "OK",
        "{h:?}"
    );

    // Disabled at its scope: the extension stays loaded, its hook does not fire.
    let hid = h.id.clone();
    fx.set(&t1, &hid, false, true).await.unwrap();
    fx.read(&t1).await;
    assert_eq!(fired(&log), 1);
    assert_eq!(fx.list_session().await.extension_rows[0].state, "ACTIVE");

    // Unloaded, the row is gone.
    fx.ext(
        "UnloadExtension",
        UnloadExtension {
            session_id: Some(fx.session.clone()),
            extension_id: r.extension_id.clone(),
        },
    )
    .await
    .unwrap();
    assert!(fx.list_session().await.extension_rows.is_empty());
}
