//! Real-effect tests for the local SurfaceProtocol: spawn the actual
//! `modbit-core` binary, read its ready line, connect over the real Unix
//! socket / named pipe, authenticate with the boot secret, run commands,
//! subscribe from an offset across a disconnect (REQ-EV-0010, REQ-EV-0192),
//! reject a wrong secret and hostile frames (REQ-EV-0103, REQ-EV-0108), and
//! survive a hard kill of the Core process (durability of accepted commands).

use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use modbit_protocol::client::{Client, ClientError, connect_raw};
use modbit_protocol::framing::{read_frame, write_frame};
use modbit_protocol::local::{ReadyLine, decode_hex};
use modbit_protocol::v1::surface_frame::Body;
use modbit_protocol::v1::{
    ClientKind, CommandEnvelope, CommandStatus, CreateSession, CreateTask, GetSessionSnapshot, Id,
    SessionCreated, SessionSnapshot, SurfaceFrame, TaskCreated,
};
use prost::Message;

struct CoreProcess {
    child: Child,
    ready: ReadyLine,
}

impl CoreProcess {
    fn spawn(data_dir: &std::path::Path) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_modbit-core"))
            .arg("--data-dir")
            .arg(data_dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let stdout = child.stdout.take().unwrap();
        let mut lines = BufReader::new(stdout).lines();
        let ready = loop {
            let line = lines
                .next()
                .expect("core exited before ready line")
                .unwrap();
            if let Some(r) = ReadyLine::parse(&line) {
                break r;
            }
        };
        // Keep draining stdout so the child never blocks on a full pipe.
        std::thread::spawn(move || for _ in lines {});
        CoreProcess { child, ready }
    }

    fn secret(&self) -> Vec<u8> {
        decode_hex(&self.ready.boot_secret_hex).unwrap()
    }

    async fn client(&self) -> Client {
        Client::connect(
            &self.ready.endpoint,
            &self.secret(),
            ClientKind::Cli,
            "test",
        )
        .await
        .unwrap()
    }

    fn kill(&mut self) {
        self.child.kill().unwrap();
        self.child.wait().unwrap();
    }
}

impl Drop for CoreProcess {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn id16(b: u8) -> Id {
    Id { value: vec![b; 16] }
}

fn envelope(command_id: Id, command_type: &str, payload: Vec<u8>) -> CommandEnvelope {
    CommandEnvelope {
        command_id: Some(command_id),
        tenant_id: Some(id16(0xA1)),
        user_id: Some(id16(0xB1)),
        session_id: None,
        aggregate_id: None,
        expected_generation: None,
        command_type: command_type.into(),
        schema_version: 1,
        payload,
        issued_at: None,
    }
}

async fn create_session(c: &mut Client, command_id: Id) -> (Id, u64) {
    let ack = c
        .command(envelope(
            command_id,
            "CreateSession",
            CreateSession { space_id: None }.encode_to_vec(),
        ))
        .await
        .unwrap();
    let r: SessionCreated = Client::result(&ack).unwrap();
    (r.session_id.unwrap(), r.offset)
}

async fn create_task(
    c: &mut Client,
    command_id: Id,
    session: Id,
    goal: &str,
) -> Result<(Id, u64, i32), ClientError> {
    let ack = c
        .command(envelope(
            command_id,
            "CreateTask",
            CreateTask {
                session_id: Some(session),
                goal_text: goal.into(),
                workspace_id: None,
                execution_profile: String::new(),
                origin: "cli".into(),
            }
            .encode_to_vec(),
        ))
        .await?;
    let r: TaskCreated = Client::result(&ack).unwrap();
    Ok((r.task_id.unwrap(), r.offset, ack.status))
}

#[tokio::test]
async fn commands_subscription_resume_and_idempotent_replay_over_the_real_socket() {
    let dir = tempfile::tempdir().unwrap();
    let core = CoreProcess::spawn(dir.path());
    let mut a = core.client().await;
    let (session, off1) = create_session(&mut a, id16(0x10)).await;
    assert_eq!(off1, 1);
    let (task1, off2, status) = create_task(&mut a, id16(0x11), session.clone(), "first")
        .await
        .unwrap();
    assert_eq!((off2, status), (3, CommandStatus::Accepted as i32));

    // A second client (REQ-EV-0192: multiple clients, one session) subscribes from the start.
    let mut b = core.client().await;
    b.subscribe(session.clone(), 0).await.unwrap();
    let mut seen = Vec::new();
    for _ in 0..3 {
        let e = b.next_event().await.unwrap().unwrap();
        seen.push((e.offset, e.event.unwrap().event_type));
    }
    assert_eq!(
        seen,
        vec![
            (1, "SessionCreated".into()),
            (2, "TaskCreated".into()),
            (3, "TaskQueued".into())
        ]
    );

    // Live delivery: a new task created by client A reaches subscriber B.
    let (task2, off4, _) = create_task(&mut a, id16(0x12), session.clone(), "second")
        .await
        .unwrap();
    assert_eq!(off4, 5);
    let e = tokio::time::timeout(Duration::from_secs(5), b.next_event())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(
        (e.offset, e.event.as_ref().unwrap().event_type.as_str()),
        (4, "TaskCreated")
    );
    let e = b.next_event().await.unwrap().unwrap();
    assert_eq!(e.offset, 5);

    // Disconnect B, produce more, reconnect from the last offset: exact continuation (REQ-EV-0010).
    drop(b);
    let (_, off6, _) = create_task(&mut a, id16(0x13), session.clone(), "third")
        .await
        .unwrap();
    assert_eq!(off6, 7);
    let mut b2 = core.client().await;
    b2.subscribe(session.clone(), 5).await.unwrap();
    let e = b2.next_event().await.unwrap().unwrap();
    assert_eq!(
        (e.offset, e.event.as_ref().unwrap().event_type.as_str()),
        (6, "TaskCreated")
    );
    let e = b2.next_event().await.unwrap().unwrap();
    assert_eq!(e.offset, 7);

    // Idempotent replay: the same command_id and request replays; no new events.
    let (task1_again, off_again, status) =
        create_task(&mut a, id16(0x11), session.clone(), "first")
            .await
            .unwrap();
    assert_eq!(
        (task1_again, off_again, status),
        (task1, 3, CommandStatus::Replayed as i32)
    );
    // Same command_id, different request: rejected.
    let err = create_task(&mut a, id16(0x11), session.clone(), "changed")
        .await
        .unwrap_err();
    assert!(
        matches!(err, ClientError::Rejected { ref code, .. } if code == "IDEMPOTENCY_CONFLICT"),
        "{err}"
    );

    // Snapshot lists every task with its projected state.
    let ack = a
        .command(envelope(
            id16(0x20),
            "GetSessionSnapshot",
            GetSessionSnapshot {
                session_id: Some(session.clone()),
            }
            .encode_to_vec(),
        ))
        .await
        .unwrap();
    let snap: SessionSnapshot = Client::result(&ack).unwrap();
    assert_eq!(snap.tasks.len(), 3);
    assert!(snap.tasks.iter().all(|t| t.state == "Queued"));
    assert!(
        snap.tasks
            .iter()
            .any(|t| t.task_id.as_ref() == Some(&task2))
    );
    assert_eq!(snap.last_offset, 7);

    // Unknown session and unsupported command are typed rejections.
    let err = create_task(&mut a, id16(0x30), id16(0x77), "x")
        .await
        .unwrap_err();
    assert!(matches!(err, ClientError::Rejected { ref code, .. } if code == "UNKNOWN_SESSION"));
    let err = a
        .command(envelope(id16(0x31), "CancelTask", vec![]))
        .await
        .unwrap_err();
    assert!(matches!(err, ClientError::Rejected { ref code, .. } if code == "UNSUPPORTED_COMMAND"));
}

#[tokio::test]
async fn wrong_secret_hostile_and_oversized_frames_are_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let core = CoreProcess::spawn(dir.path());

    // Wrong secret: refused, nothing served.
    let mut wrong = core.secret();
    wrong[0] ^= 0xff;
    let err = Client::connect(&core.ready.endpoint, &wrong, ClientKind::Cli, "test")
        .await
        .unwrap_err();
    assert!(
        matches!(err, ClientError::Protocol { ref code, .. } if code == "UNAUTHENTICATED"),
        "{err}"
    );
    let err = Client::connect(&core.ready.endpoint, &[], ClientKind::Cli, "test")
        .await
        .unwrap_err();
    assert!(
        matches!(err, ClientError::Protocol { ref code, .. } if code == "UNAUTHENTICATED"),
        "{err}"
    );

    // A command before the handshake (malicious renderer without credentials, REQ-EV-0103).
    let mut raw = connect_raw(&core.ready.endpoint).await.unwrap();
    write_frame(
        &mut raw,
        &SurfaceFrame {
            body: Some(Body::Command(envelope(id16(1), "CreateSession", vec![]))),
        },
    )
    .await
    .unwrap();
    let f = read_frame(&mut raw).await.unwrap().unwrap();
    assert!(
        matches!(f.body, Some(Body::Error(ref e)) if e.code == "HANDSHAKE_REQUIRED"),
        "{f:?}"
    );
    assert!(
        read_frame(&mut raw).await.unwrap().is_none(),
        "connection closed after the error"
    );

    // Malformed bytes before the handshake.
    let mut raw = connect_raw(&core.ready.endpoint).await.unwrap();
    use tokio::io::AsyncWriteExt;
    raw.write_all(&[0, 0, 0, 3, 0xff, 0xff, 0xff])
        .await
        .unwrap();
    let f = read_frame(&mut raw).await.unwrap().unwrap();
    assert!(
        matches!(f.body, Some(Body::Error(ref e)) if e.code == "MALFORMED_FRAME"),
        "{f:?}"
    );

    // Oversized frame after a valid handshake (REQ-EV-0108): rejected without allocation, connection closed.
    let mut c = core.client().await;
    let huge = ((modbit_protocol::framing::MAX_FRAME_BYTES + 1) as u32).to_be_bytes();
    // Reach the raw stream through a fresh raw connection + manual handshake.
    let mut raw = connect_raw(&core.ready.endpoint).await.unwrap();
    write_frame(
        &mut raw,
        &SurfaceFrame {
            body: Some(Body::ClientHello(modbit_protocol::v1::ClientHello {
                hello: Some(modbit_protocol::v1::Hello {
                    protocol_version: Some(modbit_protocol::PROTOCOL_VERSION),
                    client_kind: ClientKind::Cli as i32,
                    client_build: "t".into(),
                    supported_command_types: vec![],
                }),
                auth: Some(modbit_protocol::v1::Auth {
                    boot_secret: core.secret(),
                }),
            })),
        },
    )
    .await
    .unwrap();
    assert!(matches!(
        read_frame(&mut raw).await.unwrap().unwrap().body,
        Some(Body::HelloAck(_))
    ));
    raw.write_all(&huge).await.unwrap();
    let f = read_frame(&mut raw).await.unwrap().unwrap();
    assert!(
        matches!(f.body, Some(Body::Error(ref e)) if e.code == "FRAME_TOO_LARGE"),
        "{f:?}"
    );
    // The Core is still healthy for the well-behaved client.
    let (_, off, _) = {
        let (s, _) = create_session(&mut c, id16(0x40)).await;
        create_task(&mut c, id16(0x41), s, "still alive")
            .await
            .unwrap()
    };
    assert!(off > 0);

    // Protocol major mismatch: refused with upgrade_required.
    let mut raw = connect_raw(&core.ready.endpoint).await.unwrap();
    write_frame(
        &mut raw,
        &SurfaceFrame {
            body: Some(Body::ClientHello(modbit_protocol::v1::ClientHello {
                hello: Some(modbit_protocol::v1::Hello {
                    protocol_version: Some(modbit_protocol::v1::ProtocolVersion {
                        major: 2,
                        minor: 0,
                    }),
                    client_kind: ClientKind::Cli as i32,
                    client_build: "t".into(),
                    supported_command_types: vec![],
                }),
                auth: Some(modbit_protocol::v1::Auth {
                    boot_secret: core.secret(),
                }),
            })),
        },
    )
    .await
    .unwrap();
    let f = read_frame(&mut raw).await.unwrap().unwrap();
    assert!(
        matches!(f.body, Some(Body::HelloAck(ref a)) if !a.compatible && a.upgrade_required),
        "{f:?}"
    );
}

#[tokio::test]
async fn accepted_commands_survive_a_hard_kill_of_the_core_and_a_second_instance_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let mut core = CoreProcess::spawn(dir.path());
    let mut c = core.client().await;
    let (session, _) = create_session(&mut c, id16(0x50)).await;
    let (task, _, _) = create_task(&mut c, id16(0x51), session.clone(), "durable")
        .await
        .unwrap();

    // A second Core on the same profile is refused (docs/33 singleton lock).
    let out = Command::new(env!("CARGO_BIN_EXE_modbit-core"))
        .arg("--data-dir")
        .arg(dir.path())
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("another modbit-core"),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );

    core.kill();
    // Restart: the new Core has a new secret and endpoint; the old secret is useless.
    let core2 = CoreProcess::spawn(dir.path());
    assert_ne!(core2.ready.boot_secret_hex, core.ready.boot_secret_hex);
    let err = Client::connect(
        &core2.ready.endpoint,
        &core.secret(),
        ClientKind::Cli,
        "test",
    )
    .await
    .unwrap_err();
    assert!(matches!(err, ClientError::Protocol { ref code, .. } if code == "UNAUTHENTICATED"));
    let mut c2 = core2.client().await;
    let ack = c2
        .command(envelope(
            id16(0x52),
            "GetSessionSnapshot",
            GetSessionSnapshot {
                session_id: Some(session.clone()),
            }
            .encode_to_vec(),
        ))
        .await
        .unwrap();
    let snap: SessionSnapshot = Client::result(&ack).unwrap();
    assert_eq!(snap.tasks.len(), 1);
    assert_eq!(snap.tasks[0].task_id.as_ref(), Some(&task));
    assert_eq!(snap.tasks[0].goal_text, "durable");
    // Replay of the pre-kill command is still recognised after restart.
    let (again, _, status) = create_task(&mut c2, id16(0x51), session, "durable")
        .await
        .unwrap();
    assert_eq!((again, status), (task, CommandStatus::Replayed as i32));
}
