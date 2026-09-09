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

/// A command carrying the session lease generation (fencing).
fn envelope_fenced(
    command_id: Id,
    command_type: &str,
    payload: Vec<u8>,
    generation: Option<u64>,
) -> CommandEnvelope {
    let mut e = envelope(command_id, command_type, payload);
    e.expected_generation = generation;
    e
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
            command_id.clone(),
            "CreateSession",
            CreateSession { space_id: None }.encode_to_vec(),
        ))
        .await
        .unwrap();
    let r: SessionCreated = Client::result(&ack).unwrap();
    let session = r.session_id.unwrap();
    // Tests act as the single mutation owner: acquire the lease right away.
    acquire_lease(
        c,
        Id {
            value: command_id.value.iter().map(|b| b ^ 0x5A).collect(),
        },
        session.clone(),
        "test",
    )
    .await;
    (session, r.offset)
}

thread_local! {
    static LEASE: std::cell::RefCell<std::collections::HashMap<Vec<u8>, u64>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
}

fn lease_for(session: &Id) -> Option<u64> {
    LEASE.with(|l| l.borrow().get(&session.value).copied())
}

async fn acquire_lease(c: &mut Client, command_id: Id, session: Id, owner: &str) -> u64 {
    use modbit_protocol::v1::{AcquireSessionLease, SessionLeaseAcquired};
    let ack = c
        .command(envelope(
            command_id,
            "AcquireSessionLease",
            AcquireSessionLease {
                session_id: Some(session.clone()),
                owner: owner.into(),
            }
            .encode_to_vec(),
        ))
        .await
        .unwrap();
    let r: SessionLeaseAcquired = Client::result(&ack).unwrap();
    LEASE.with(|l| {
        l.borrow_mut()
            .insert(session.value.clone(), r.lease_generation)
    });
    r.lease_generation
}

async fn create_task(
    c: &mut Client,
    command_id: Id,
    session: Id,
    goal: &str,
) -> Result<(Id, u64, i32), ClientError> {
    let ack = c
        .command(envelope_fenced(
            command_id,
            "CreateTask",
            CreateTask {
                session_id: Some(session.clone()),
                goal_text: goal.into(),
                workspace_id: None,
                execution_profile: String::new(),
                origin: "cli".into(),
                workspace_root: String::new(),
            }
            .encode_to_vec(),
            lease_for(&session),
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
    // A task is three events: TaskCreated, TaskQueued and (M2.5) its CapabilityLeaseGranted.
    assert_eq!((off2, status), (5, CommandStatus::Accepted as i32));

    // A second client (REQ-EV-0192: multiple clients, one session) subscribes from the start.
    let mut b = core.client().await;
    b.subscribe(session.clone(), 0).await.unwrap();
    let mut seen = Vec::new();
    for _ in 0..5 {
        let e = b.next_event().await.unwrap().unwrap();
        seen.push((e.offset, e.event.unwrap().event_type));
    }
    assert_eq!(
        seen,
        vec![
            (1, "SessionCreated".into()),
            (2, "SessionLeaseAcquired".into()),
            (3, "TaskCreated".into()),
            (4, "TaskQueued".into()),
            (5, "CapabilityLeaseGranted".into())
        ]
    );

    // Live delivery: a new task created by client A reaches subscriber B.
    let (task2, off4, _) = create_task(&mut a, id16(0x12), session.clone(), "second")
        .await
        .unwrap();
    assert_eq!(off4, 8);
    let e = tokio::time::timeout(Duration::from_secs(5), b.next_event())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(
        (e.offset, e.event.as_ref().unwrap().event_type.as_str()),
        (6, "TaskCreated")
    );
    let e = b.next_event().await.unwrap().unwrap();
    assert_eq!(e.offset, 7);
    let e = b.next_event().await.unwrap().unwrap();
    assert_eq!(e.offset, 8);

    // Disconnect B, produce more, reconnect from the last offset: exact continuation (REQ-EV-0010).
    drop(b);
    let (_, off6, _) = create_task(&mut a, id16(0x13), session.clone(), "third")
        .await
        .unwrap();
    assert_eq!(off6, 11);
    let mut b2 = core.client().await;
    b2.subscribe(session.clone(), 8).await.unwrap();
    let e = b2.next_event().await.unwrap().unwrap();
    assert_eq!(
        (e.offset, e.event.as_ref().unwrap().event_type.as_str()),
        (9, "TaskCreated")
    );
    let e = b2.next_event().await.unwrap().unwrap();
    assert_eq!(e.offset, 10);
    let e = b2.next_event().await.unwrap().unwrap();
    assert_eq!(e.offset, 11);

    // Idempotent replay: the same command_id and request replays; no new events.
    // The replayed ack reports the command's own last event (TaskQueued, 4);
    // the lease grant that followed it is not part of the command record.
    let (task1_again, off_again, status) =
        create_task(&mut a, id16(0x11), session.clone(), "first")
            .await
            .unwrap();
    assert_eq!(
        (task1_again, off_again, status),
        (task1, 4, CommandStatus::Replayed as i32)
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
    assert_eq!(snap.last_offset, 11);

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

/// M1.5 kill-point suite (docs/54 faults 1 and 2, docs/19 resume): a writer
/// task streams CreateTask commands continuously while the test hard-kills
/// the Core at a random moment, so the kill lands before a commit, between
/// commit and ack, or between commands. After every restart: recovery runs,
/// the report is served, and re-sending every command of the round with its
/// original command_id never duplicates a task (never-committed → Accepted
/// exactly once; committed-but-unacked → Replayed).
#[tokio::test]
async fn kill_points_during_a_command_stream_never_duplicate_or_tear_state() {
    use modbit_protocol::v1::{GetRecoveryReport, RecoveryReport};
    use std::sync::{Arc, Mutex};
    let dir = tempfile::tempdir().unwrap();
    let mut core = CoreProcess::spawn(dir.path());
    let mut c = core.client().await;
    let (session, _) = create_session(&mut c, id16(0x60)).await;
    let mut expected_tasks: u64 = 0;
    let mut next: u16 = 0x0100;
    let mut in_flight_kills = 0;
    for round in 0..5u32 {
        // Writer: fire commands back to back, recording every id sent and every id acked.
        let sent: Arc<Mutex<Vec<(Id, String)>>> = Arc::new(Mutex::new(Vec::new()));
        let acked: Arc<Mutex<Vec<Id>>> = Arc::new(Mutex::new(Vec::new()));
        let writer = {
            let mut wc = core.client().await;
            let (sent, acked, session) = (Arc::clone(&sent), Arc::clone(&acked), session.clone());
            let base = next;
            tokio::spawn(async move {
                for i in 0..200u16 {
                    let n = base + i;
                    let cid = Id {
                        value: vec![
                            (n >> 8) as u8,
                            n as u8,
                            0x77,
                            0x77,
                            0x77,
                            0x77,
                            0x77,
                            0x77,
                            0x77,
                            0x77,
                            0x77,
                            0x77,
                            0x77,
                            0x77,
                            0x77,
                            0x77,
                        ],
                    };
                    let goal = format!("round {round} item {i}");
                    sent.lock().unwrap().push((cid.clone(), goal.clone()));
                    match create_task(&mut wc, cid.clone(), session.clone(), &goal).await {
                        Ok(_) => acked.lock().unwrap().push(cid),
                        Err(_) => break,
                    }
                }
            })
        };
        // Wait until the writer has real progress (platform latency varies:
        // Windows named pipes are slower than Unix sockets), then kill at a
        // round-dependent moment so kills land at different points.
        let progress_target = 3 + (round as usize * 5) % 12;
        let deadline = std::time::Instant::now() + Duration::from_secs(30);
        while acked.lock().unwrap().len() < progress_target && std::time::Instant::now() < deadline
        {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
        tokio::time::sleep(Duration::from_micros(((round as u64 * 7919) % 5000) + 200)).await;
        core.kill();
        let _ = writer.await;
        let sent = sent.lock().unwrap().clone();
        let acked = acked.lock().unwrap().clone();
        next += 200;
        assert!(
            !acked.is_empty(),
            "round {round}: the writer must have made progress before the kill"
        );
        let unacked = sent.len() - acked.len();
        if unacked > 0 {
            in_flight_kills += 1;
        }
        expected_tasks += acked.len() as u64;

        // Restart, recover, and replay the whole round.
        core = CoreProcess::spawn(dir.path());
        c = core.client().await;
        let ack = c
            .command(envelope(
                id16(0xF0 + round as u8),
                "GetRecoveryReport",
                GetRecoveryReport {}.encode_to_vec(),
            ))
            .await
            .unwrap();
        let report: RecoveryReport = Client::result(&ack).unwrap();
        assert_eq!(
            report.boot_generation,
            u64::from(round) + 2,
            "boot generation increments per start"
        );
        assert_eq!(report.sessions, 1);
        assert!(
            report.notes.is_empty(),
            "a hard kill must leave nothing to repair beyond what SQLite guarantees: {:?}",
            report.notes
        );
        assert!(
            report.tasks >= expected_tasks,
            "committed-but-unacked commands may add tasks, acked ones never vanish: {report:?}"
        );
        let committed_unacked = report.tasks - expected_tasks;
        assert!(committed_unacked as usize <= unacked, "{report:?}");
        expected_tasks = report.tasks;
        let mut accepted_on_replay = 0u64;
        for (cid, goal) in &sent {
            let (_, _, status) = create_task(&mut c, cid.clone(), session.clone(), goal)
                .await
                .unwrap();
            if status == CommandStatus::Accepted as i32 {
                accepted_on_replay += 1;
            }
        }
        assert_eq!(
            accepted_on_replay as usize,
            unacked - committed_unacked as usize,
            "only never-committed commands are accepted on replay"
        );
        expected_tasks += accepted_on_replay;
        let ack = c
            .command(envelope(
                id16(0xE0 + round as u8),
                "GetSessionSnapshot",
                GetSessionSnapshot {
                    session_id: Some(session.clone()),
                }
                .encode_to_vec(),
            ))
            .await
            .unwrap();
        let snap: SessionSnapshot = Client::result(&ack).unwrap();
        assert_eq!(
            snap.tasks.len() as u64,
            expected_tasks,
            "round {round}: tasks in the projection must equal accepted commands"
        );
        assert!(snap.tasks.iter().all(|t| t.state == "Queued"));
        let mut goals: Vec<_> = snap.tasks.iter().map(|t| t.goal_text.clone()).collect();
        let n = goals.len();
        goals.sort();
        goals.dedup();
        assert_eq!(goals.len(), n, "no duplicate task for the same command");
    }
    assert!(
        expected_tasks >= 40,
        "the suite must have exercised real work: {expected_tasks}"
    );
    eprintln!(
        "kill-point suite: {expected_tasks} tasks, {in_flight_kills} rounds killed with commands in flight"
    );
}

/// QUAL-EV-0054 / QUAL-EV-0273: dual resume — two clients contend for one
/// session; only the current lease generation can append mutation events, the
/// stale writer is rejected, and fencing survives a Core restart.
#[tokio::test]
async fn qual_ev_0054_0273_session_lease_fences_out_stale_writers_across_restart() {
    let dir = tempfile::tempdir().unwrap();
    let mut core = CoreProcess::spawn(dir.path());
    let mut a = core.client().await;
    let (session, _) = create_session(&mut a, id16(0xA0)).await; // a holds generation 1
    let ga = lease_for(&session).unwrap();
    assert_eq!(ga, 1);
    assert!(
        create_task(&mut a, id16(0xA1), session.clone(), "by a")
            .await
            .is_ok()
    );

    // Client B resumes the same session and takes the lease: generation 2.
    let mut b = core.client().await;
    let gb = acquire_lease(&mut b, id16(0xA2), session.clone(), "b").await;
    assert_eq!(gb, 2);
    assert!(
        create_task(&mut b, id16(0xA3), session.clone(), "by b")
            .await
            .is_ok()
    );
    // A is now stale: its generation 1 is rejected, nothing appended.
    let stale = a
        .command(envelope_fenced(
            id16(0xA4),
            "CreateTask",
            CreateTask {
                session_id: Some(session.clone()),
                goal_text: "stale".into(),
                workspace_id: None,
                execution_profile: String::new(),
                origin: "cli".into(),
                workspace_root: String::new(),
            }
            .encode_to_vec(),
            Some(1),
        ))
        .await
        .unwrap_err();
    assert!(
        matches!(stale, ClientError::Rejected { ref code, .. } if code == "STALE_LEASE"),
        "{stale}"
    );
    // No generation at all is also refused.
    let none = a
        .command(envelope(
            id16(0xA5),
            "CreateTask",
            CreateTask {
                session_id: Some(session.clone()),
                goal_text: "no lease".into(),
                workspace_id: None,
                execution_profile: String::new(),
                origin: "cli".into(),
                workspace_root: String::new(),
            }
            .encode_to_vec(),
        ))
        .await
        .unwrap_err();
    assert!(
        matches!(none, ClientError::Rejected { ref code, .. } if code == "LEASE_REQUIRED"),
        "{none}"
    );

    // Restart: the persisted generation still fences A; B must re-acquire (3) to continue.
    core.kill();
    let core2 = CoreProcess::spawn(dir.path());
    let mut a2 = core2.client().await;
    let stale = a2
        .command(envelope_fenced(
            id16(0xA6),
            "CreateTask",
            CreateTask {
                session_id: Some(session.clone()),
                goal_text: "stale after restart".into(),
                workspace_id: None,
                execution_profile: String::new(),
                origin: "cli".into(),
                workspace_root: String::new(),
            }
            .encode_to_vec(),
            Some(1),
        ))
        .await
        .unwrap_err();
    assert!(matches!(stale, ClientError::Rejected { ref code, .. } if code == "STALE_LEASE"));
    let mut b2 = core2.client().await;
    assert_eq!(
        acquire_lease(&mut b2, id16(0xA7), session.clone(), "b").await,
        3
    );
    assert!(
        create_task(&mut b2, id16(0xA8), session.clone(), "by b after restart")
            .await
            .is_ok()
    );
    let ack = b2
        .command(envelope(
            id16(0xA9),
            "GetSessionSnapshot",
            GetSessionSnapshot {
                session_id: Some(session.clone()),
            }
            .encode_to_vec(),
        ))
        .await
        .unwrap();
    let snap: SessionSnapshot = Client::result(&ack).unwrap();
    assert_eq!(snap.tasks.len(), 3, "exactly the leased writes landed");
}

/// QUAL-EV-0262: queued inputs are durable, typed, ordered events; ordering is
/// preserved across reconnect and Core restart; retries replay.
#[tokio::test]
async fn qual_ev_0262_queued_inputs_keep_order_across_reconnect_and_restart() {
    use modbit_protocol::v1::{InputQueued, QueueInput};
    let dir = tempfile::tempdir().unwrap();
    let mut core = CoreProcess::spawn(dir.path());
    let mut c = core.client().await;
    let (session, _) = create_session(&mut c, id16(0xB0)).await;
    let (task, _, _) = create_task(&mut c, id16(0xB1), session.clone(), "queue target")
        .await
        .unwrap();
    let q = |i: u8, mode: &str, text: &str| {
        QueueInput {
            task_id: Some(task.clone()),
            input_id: format!("in-{i}"),
            mode: mode.into(),
            text: text.into(),
        }
        .encode_to_vec()
    };
    let g = lease_for(&session);
    let mut seqs = Vec::new();
    for (i, (mode, text)) in [
        ("FOLLOW_UP", "first"),
        ("COLLECT", "second"),
        ("STEER", "third"),
    ]
    .iter()
    .enumerate()
    {
        let ack = c
            .command(envelope_fenced(
                id16(0xB2 + i as u8),
                "QueueInput",
                q(i as u8, mode, text),
                g,
            ))
            .await
            .unwrap();
        let r: InputQueued = Client::result(&ack).unwrap();
        seqs.push(r.sequence);
    }
    assert_eq!(
        seqs,
        vec![3, 4, 5],
        "inputs follow the task's aggregate sequence"
    );
    // Retry of the second input replays, appending nothing.
    let ack = c
        .command(envelope_fenced(
            id16(0xB3),
            "QueueInput",
            q(1, "COLLECT", "second"),
            g,
        ))
        .await
        .unwrap();
    assert_eq!(ack.status, CommandStatus::Replayed as i32);
    let bad = c
        .command(envelope_fenced(
            id16(0xB9),
            "QueueInput",
            q(9, "SHOUT", "x"),
            g,
        ))
        .await
        .unwrap_err();
    assert!(matches!(bad, ClientError::Rejected { ref code, .. } if code == "BAD_PAYLOAD"));

    // Reconnect and replay from the start: the input events arrive in order with their modes.
    drop(c);
    let mut r = core.client().await;
    r.subscribe(session.clone(), 0).await.unwrap();
    let mut inputs = Vec::new();
    while inputs.len() < 3 {
        let e = r.next_event().await.unwrap().unwrap();
        let ev = e.event.unwrap();
        if ev.event_type == "TaskInputQueued" {
            let p: serde_json::Value = serde_json::from_slice(&ev.payload).unwrap();
            inputs.push((
                ev.sequence,
                p["payload"]["mode"].as_str().unwrap().to_owned(),
                p["payload"]["text"].as_str().unwrap().to_owned(),
            ));
        }
    }
    assert_eq!(
        inputs,
        vec![
            (3, "FOLLOW_UP".into(), "first".into()),
            (4, "COLLECT".into(), "second".into()),
            (5, "STEER".into(), "third".into())
        ]
    );
    // And after a Core restart the same order is served.
    core.kill();
    let core2 = CoreProcess::spawn(dir.path());
    let mut r2 = core2.client().await;
    r2.subscribe(session.clone(), 5).await.unwrap();
    let mut seen = Vec::new();
    for _ in 0..3 {
        let e = r2.next_event().await.unwrap().unwrap();
        seen.push(e.event.unwrap().sequence);
    }
    assert_eq!(seen, vec![3, 4, 5]);
}

/// QUAL-EV-0108: a multi-megabyte object is served only as bounded ranges;
/// an over-large range is refused; the stream stays responsive for others.
#[tokio::test]
async fn qual_ev_0108_multi_mb_object_is_read_in_bounded_ranges() {
    use modbit_protocol::v1::{ObjectRangeChunk, ReadObjectRange};
    let dir = tempfile::tempdir().unwrap();
    // Write a 10 MiB object straight into the store's object directory (as a
    // terminal/browser result would be spilled by ref), then read it over the socket.
    let objects =
        modbit_event_store::ObjectStore::open(dir.path().join("core").join("objects")).unwrap();
    let big: Vec<u8> = (0..10 * 1024 * 1024u32).map(|i| (i % 251) as u8).collect();
    let hash = objects.put(&big).unwrap();
    let core = CoreProcess::spawn(dir.path());
    let mut c = core.client().await;
    let too_big = c
        .command(envelope(
            id16(0xC0),
            "ReadObjectRange",
            ReadObjectRange {
                object_hash: hash.clone(),
                offset: 0,
                length: 2 * 1024 * 1024,
            }
            .encode_to_vec(),
        ))
        .await
        .unwrap_err();
    assert!(
        matches!(too_big, ClientError::Rejected { ref code, .. } if code == "RANGE_TOO_LARGE"),
        "{too_big}"
    );
    let mut got = Vec::with_capacity(big.len());
    let mut offset = 0u64;
    let mut chunks = 0;
    let started = std::time::Instant::now();
    loop {
        let ack = c
            .command(envelope(
                id16(0xC1),
                "ReadObjectRange",
                ReadObjectRange {
                    object_hash: hash.clone(),
                    offset,
                    length: 1024 * 1024,
                }
                .encode_to_vec(),
            ))
            .await
            .unwrap();
        let chunk: ObjectRangeChunk = Client::result(&ack).unwrap();
        assert_eq!(chunk.total_bytes as usize, big.len());
        assert!(chunk.data.len() <= 1024 * 1024);
        if chunk.data.is_empty() {
            break;
        }
        got.extend_from_slice(&chunk.data);
        offset += chunk.data.len() as u64;
        chunks += 1;
    }
    assert_eq!(chunks, 10);
    assert_eq!(got, big, "ranges reassemble to the exact object");
    assert!(started.elapsed() < Duration::from_secs(20));
    let unknown = c
        .command(envelope(
            id16(0xC2),
            "ReadObjectRange",
            ReadObjectRange {
                object_hash: "0".repeat(64),
                offset: 0,
                length: 16,
            }
            .encode_to_vec(),
        ))
        .await
        .unwrap_err();
    assert!(matches!(unknown, ClientError::Rejected { ref code, .. } if code == "UNKNOWN_OBJECT"));
}

/// QUAL-EV-0010: reconnecting from the last offset yields the exact stream;
/// a cursor beyond the log is refused so the client rehydrates from a snapshot
/// instead of silently skipping events.
#[tokio::test]
async fn qual_ev_0010_offset_resume_is_exact_and_invalid_cursors_force_rehydrate() {
    let dir = tempfile::tempdir().unwrap();
    let core = CoreProcess::spawn(dir.path());
    let mut a = core.client().await;
    let (session, _) = create_session(&mut a, id16(0xD0)).await;
    for i in 0..5u8 {
        create_task(&mut a, id16(0xD1 + i), session.clone(), &format!("t{i}"))
            .await
            .unwrap();
    }
    // Full stream once, then resume from the middle: identical suffix.
    let mut s1 = core.client().await;
    s1.subscribe(session.clone(), 0).await.unwrap();
    let mut all = Vec::new();
    for _ in 0..17 {
        let e = s1.next_event().await.unwrap().unwrap();
        all.push((e.offset, e.event.unwrap().event_type));
    }
    drop(s1);
    let mut s2 = core.client().await;
    s2.subscribe(session.clone(), all[6].0).await.unwrap();
    let mut tail = Vec::new();
    for _ in 0..10 {
        let e = s2.next_event().await.unwrap().unwrap();
        tail.push((e.offset, e.event.unwrap().event_type));
    }
    assert_eq!(tail, all[7..17].to_vec());
    // Beyond the log: INVALID_CURSOR, connection closed; snapshot gives the true last offset.
    let mut s3 = core.client().await;
    s3.subscribe(session.clone(), 10_000).await.unwrap();
    let err = s3.next_event().await.unwrap_err();
    assert!(
        matches!(err, ClientError::Protocol { ref code, .. } if code == "INVALID_CURSOR"),
        "{err}"
    );
    let ack = a
        .command(envelope(
            id16(0xDF),
            "GetSessionSnapshot",
            GetSessionSnapshot {
                session_id: Some(session.clone()),
            }
            .encode_to_vec(),
        ))
        .await
        .unwrap();
    let snap: SessionSnapshot = Client::result(&ack).unwrap();
    assert_eq!(snap.last_offset, all[16].0);
}

/// M2.4: tools are reachable only through the registry, the kernel port and
/// the event loop (docs/16 "Tool completion proof"): every call lands as
/// ToolCall events on the canonical log; real fs, git and broker effectors.
#[tokio::test]
async fn m2_4_invoke_tool_runs_direct_tools_through_registry_policy_and_event_log() {
    use modbit_protocol::v1::{InvokeTool, ListTools, ToolInvoked, ToolList};
    let dir = tempfile::tempdir().unwrap();
    // A real repository as the task's approved workspace.
    let repo = tempfile::tempdir().unwrap();
    std::fs::write(repo.path().join("README.md"), "# demo\n").unwrap();
    std::fs::write(repo.path().join(".env"), "SECRET=1\n").unwrap();
    for args in [
        vec!["init", "-q", "-b", "main"],
        vec!["add", "-A"],
        vec![
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@e",
            "commit",
            "-q",
            "-m",
            "base",
        ],
    ] {
        assert!(
            Command::new("git")
                .arg("-C")
                .arg(repo.path())
                .args(&args)
                .status()
                .unwrap()
                .success()
        );
    }
    let root = repo
        .path()
        .canonicalize()
        .unwrap()
        .to_string_lossy()
        .trim_start_matches(r"\\?\")
        .to_owned();
    let core = CoreProcess::spawn(dir.path());
    let mut c = core.client().await;
    let (session, _) = create_session(&mut c, id16(0xE0)).await;
    let ack = c
        .command(envelope_fenced(
            id16(0xE1),
            "CreateTask",
            CreateTask {
                session_id: Some(session.clone()),
                goal_text: "use tools".into(),
                workspace_id: None,
                execution_profile: String::new(),
                origin: "cli".into(),
                workspace_root: root.clone(),
            }
            .encode_to_vec(),
            lease_for(&session),
        ))
        .await
        .unwrap();
    let task = Client::result::<TaskCreated>(&ack)
        .unwrap()
        .task_id
        .unwrap();
    let g = lease_for(&session);
    let ack = c
        .command(envelope(
            id16(0xE2),
            "ListTools",
            ListTools {}.encode_to_vec(),
        ))
        .await
        .unwrap();
    let list: ToolList = Client::result(&ack).unwrap();
    let names: Vec<_> = list.tools.iter().map(|t| t.name.as_str()).collect();
    for n in [
        "fs.read",
        "change.apply",
        "git.status",
        "shell.exec",
        "test.run",
    ] {
        assert!(names.contains(&n), "{names:?}");
    }
    async fn call(
        c: &mut Client,
        id: u8,
        task: &Id,
        g: Option<u64>,
        tool: &str,
        args: &str,
    ) -> Result<ToolInvoked, ClientError> {
        let ack = c
            .command(envelope_fenced(
                id16(id),
                "InvokeTool",
                InvokeTool {
                    task_id: Some(task.clone()),
                    tool_name: tool.into(),
                    arguments_json: args.into(),
                    tool_call_id: Some(id16(0xC0 ^ id)),
                    output_budget_bytes: 4096,
                }
                .encode_to_vec(),
                g,
            ))
            .await?;
        Ok(Client::result(&ack).unwrap())
    }
    // fs.read succeeds with the revision-bound content hash.
    let r = call(&mut c, 0x10, &task, g, "fs.read", r#"{"path":"README.md"}"#)
        .await
        .unwrap();
    assert_eq!(r.status, "SUCCESS", "{r:?}");
    let out: serde_json::Value = serde_json::from_str(&r.structured_output_json).unwrap();
    assert_eq!(out["content"], "# demo\n");
    let hash = out["content_hash"].as_str().unwrap().to_owned();
    // change.apply with the precondition, then git.status sees the change.
    let r = call(&mut c, 0x11, &task, g, "change.apply", &format!(r##"{{"path":"README.md","op":"replace","content":"# changed\n","expected_content_hash":"{hash}"}}"##)).await.unwrap();
    assert_eq!(r.status, "SUCCESS", "{r:?}");
    assert!(r.workspace_revision_after >= 2);
    assert_eq!(
        std::fs::read_to_string(repo.path().join("README.md")).unwrap(),
        "# changed\n"
    );
    let r = call(&mut c, 0x12, &task, g, "git.status", "{}")
        .await
        .unwrap();
    assert_eq!(r.status, "SUCCESS");
    assert!(r.structured_output_json.contains("README.md"));
    // Protected path: application failure, file untouched.
    let r = call(
        &mut c,
        0x13,
        &task,
        g,
        "change.apply",
        r#"{"path":".env","op":"replace","content":"pwned"}"#,
    )
    .await
    .unwrap();
    assert_eq!(
        (r.status.as_str(), r.error_code.as_str()),
        ("APPLICATION_FAILURE", "PATH_PROTECTED")
    );
    assert_eq!(
        std::fs::read_to_string(repo.path().join(".env")).unwrap(),
        "SECRET=1\n"
    );
    // Schema violation never reaches an effector; unknown tool is typed.
    let r = call(&mut c, 0x14, &task, g, "fs.read", r#"{"nope":1}"#)
        .await
        .unwrap();
    assert_eq!(r.status, "INVALID_ARGUMENTS");
    let r = call(&mut c, 0x15, &task, g, "no.such", "{}").await.unwrap();
    assert_eq!(r.status, "UNKNOWN_TOOL");
    // shell.exec and test.run through the Core-supervised broker.
    let r = call(
        &mut c,
        0x16,
        &task,
        g,
        "shell.exec",
        r#"{"argv":["git","rev-parse","--verify","HEAD"],"inherit_env":true}"#,
    )
    .await
    .unwrap();
    assert_eq!(r.status, "SUCCESS", "{r:?}");
    assert_eq!(r.stdout_ref.len(), 64);
    let r = call(
        &mut c,
        0x17,
        &task,
        g,
        "test.run",
        r#"{"argv":["git","rev-parse","--verify","nope"],"inherit_env":true}"#,
    )
    .await
    .unwrap();
    assert_eq!(r.status, "SUCCESS");
    let rep: serde_json::Value = serde_json::from_str(&r.structured_output_json).unwrap();
    assert_eq!(rep["status"], "FAILED");
    assert_eq!(rep["parser"]["confidence"], "HEURISTIC");
    // Lease is required for tool invocation; replay of a tool_call_id returns the recorded result.
    let err = c
        .command(envelope(
            id16(0xE3),
            "InvokeTool",
            InvokeTool {
                task_id: Some(task.clone()),
                tool_name: "fs.read".into(),
                arguments_json: "{}".into(),
                tool_call_id: Some(id16(0x99)),
                output_budget_bytes: 0,
            }
            .encode_to_vec(),
        ))
        .await
        .unwrap_err();
    assert!(matches!(err, ClientError::Rejected { ref code, .. } if code == "LEASE_REQUIRED"));
    let again = c
        .command(envelope_fenced(
            id16(0xE4),
            "InvokeTool",
            InvokeTool {
                task_id: Some(task.clone()),
                tool_name: "fs.read".into(),
                arguments_json: r#"{"path":"README.md"}"#.into(),
                tool_call_id: Some(id16(0xC0 ^ 0x10)),
                output_budget_bytes: 4096,
            }
            .encode_to_vec(),
            g,
        ))
        .await
        .unwrap();
    assert_eq!(again.status, CommandStatus::Replayed as i32);
    // Every call is on the log as ToolCall events with the pipeline trail.
    let mut s = core.client().await;
    s.subscribe(session.clone(), 0).await.unwrap();
    let mut types: Vec<(String, String)> = Vec::new();
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while std::time::Instant::now() < deadline {
        match tokio::time::timeout(Duration::from_millis(500), s.next_event()).await {
            Ok(Ok(Some(e))) => {
                let ev = e.event.unwrap();
                if ev.aggregate_type == "tool_call" {
                    types.push((hex_id(&ev.aggregate_id.unwrap()), ev.event_type));
                }
            }
            _ => break,
        }
    }
    let for_call = |id: u8| {
        types
            .iter()
            .filter(|(a, _)| *a == hex_id(&id16(0xC0 ^ id)))
            .map(|(_, t)| t.as_str())
            .collect::<Vec<_>>()
    };
    assert_eq!(
        for_call(0x10),
        [
            "ToolCallProposed",
            "ToolCallValidated",
            "ToolCallPolicyDecision",
            "ToolCallDispatched",
            "ToolCallSucceeded"
        ]
    );
    assert_eq!(
        for_call(0x13),
        [
            "ToolCallProposed",
            "ToolCallValidated",
            "ToolCallPolicyDecision",
            "ToolCallDispatched",
            "ToolCallFailed"
        ]
    );
    assert_eq!(for_call(0x14), ["ToolCallProposed", "ToolCallFailed"]);
    assert_eq!(for_call(0x15), ["ToolCallProposed", "ToolCallFailed"]);
}

fn hex_id(id: &Id) -> String {
    id.value.iter().map(|b| format!("{b:02x}")).collect()
}

/// M2.5: Capability Kernel + basic approval flow (docs/23). The lease granted
/// at task creation is the authority; destructive effects wait for an approval
/// bound to the exact intent; receipts chain; emergency stop revokes leases.
#[tokio::test]
async fn m2_5_capability_kernel_gates_destructive_tools_behind_intent_bound_approvals() {
    use modbit_protocol::v1::{
        ApprovalList, ApprovalResolvedAck, CapabilityLeaseList, EffectReceiptList, EmergencyStop,
        EmergencyStopped, GetCapabilityLeases, GetEffectReceipts, InvokeTool, ListApprovals,
        ResolveApproval, ToolInvoked,
    };
    let dir = tempfile::tempdir().unwrap();
    let repo = tempfile::tempdir().unwrap();
    std::fs::write(repo.path().join("README.md"), "# demo\n").unwrap();
    for args in [
        vec!["init", "-q", "-b", "main"],
        vec!["add", "-A"],
        vec![
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@e",
            "commit",
            "-q",
            "-m",
            "base",
        ],
    ] {
        assert!(
            Command::new("git")
                .arg("-C")
                .arg(repo.path())
                .args(&args)
                .status()
                .unwrap()
                .success()
        );
    }
    let root = repo
        .path()
        .canonicalize()
        .unwrap()
        .to_string_lossy()
        .trim_start_matches(r"\\?\")
        .to_owned();
    let root_json = root.replace('\\', "/");
    let core = CoreProcess::spawn(dir.path());
    let mut c = core.client().await;
    let (session, _) = create_session(&mut c, id16(0xF0)).await;
    let g = lease_for(&session);
    async fn new_task(
        c: &mut Client,
        id: u8,
        session: &Id,
        root: &str,
        profile: &str,
        g: Option<u64>,
    ) -> Id {
        let ack = c
            .command(envelope_fenced(
                id16(id),
                "CreateTask",
                CreateTask {
                    session_id: Some(session.clone()),
                    goal_text: "kernel".into(),
                    workspace_id: None,
                    execution_profile: profile.into(),
                    origin: "cli".into(),
                    workspace_root: root.into(),
                }
                .encode_to_vec(),
                g,
            ))
            .await
            .unwrap();
        Client::result::<TaskCreated>(&ack)
            .unwrap()
            .task_id
            .unwrap()
    }
    let task = new_task(&mut c, 0xF1, &session, &root, "", g).await;
    // The task's default lease exists, is ACTIVE and names its operations and ceiling.
    let ack = c
        .command(envelope(
            id16(0xF2),
            "GetCapabilityLeases",
            GetCapabilityLeases {
                task_id: Some(task.clone()),
            }
            .encode_to_vec(),
        ))
        .await
        .unwrap();
    let leases: CapabilityLeaseList = Client::result(&ack).unwrap();
    assert_eq!(leases.leases.len(), 1, "{leases:?}");
    let lease = &leases.leases[0];
    assert_eq!(
        (
            lease.status.as_str(),
            lease.execution_profile.as_str(),
            lease.effect_ceiling.as_str()
        ),
        ("ACTIVE", "local_trusted", "Destructive")
    );
    assert!(
        lease.operations.iter().any(|o| o == "git.worktree"),
        "{lease:?}"
    );
    assert!(
        lease.resources.iter().any(|r| r.starts_with("fs.write:")),
        "{lease:?}"
    );

    async fn call(
        c: &mut Client,
        cmd: u8,
        call: u8,
        task: &Id,
        g: Option<u64>,
        tool: &str,
        args: &str,
    ) -> Result<ToolInvoked, ClientError> {
        let ack = c
            .command(envelope_fenced(
                id16(cmd),
                "InvokeTool",
                InvokeTool {
                    task_id: Some(task.clone()),
                    tool_name: tool.into(),
                    arguments_json: args.into(),
                    tool_call_id: Some(id16(call)),
                    output_budget_bytes: 4096,
                }
                .encode_to_vec(),
                g,
            ))
            .await?;
        Ok(Client::result(&ack).unwrap())
    }
    // Reversible write under the lease: allowed.
    let wt = format!("{root_json}-wt");
    let r = call(
        &mut c,
        0x20,
        0xA0,
        &task,
        g,
        "git.worktree.create",
        &format!(r#"{{"branch":"task/k","path":"{wt}"}}"#),
    )
    .await
    .unwrap();
    assert_eq!(r.status, "SUCCESS", "{r:?}");
    assert!(std::path::Path::new(&wt).exists());
    // Destructive: no effect; an approval bound to this intent is opened.
    let close_args = format!(r#"{{"path":"{wt}"}}"#);
    let r = call(
        &mut c,
        0x21,
        0xA1,
        &task,
        g,
        "git.worktree.close",
        &close_args,
    )
    .await
    .unwrap();
    assert_eq!(
        (r.status.as_str(), r.error_code.as_str()),
        ("APPROVAL_PENDING", "APPROVAL_REQUIRED"),
        "{r:?}"
    );
    assert_eq!(r.approval_id.len(), 32, "{r:?}");
    let approval_hex = r.approval_id.clone();
    assert!(
        std::path::Path::new(&wt).exists(),
        "nothing happened before approval"
    );
    // Re-asking with the same intent replays the open approval; a different
    // intent under the same id is refused; the approval is listed as REQUESTED.
    let again = call(
        &mut c,
        0x22,
        0xA1,
        &task,
        g,
        "git.worktree.close",
        &close_args,
    )
    .await
    .unwrap();
    assert_eq!(
        (again.status.as_str(), again.approval_id.as_str()),
        ("APPROVAL_PENDING", approval_hex.as_str())
    );
    let err = call(
        &mut c,
        0x23,
        0xA1,
        &task,
        g,
        "git.worktree.close",
        r#"{"path":"/somewhere/else"}"#,
    )
    .await
    .unwrap_err();
    assert!(
        matches!(err, ClientError::Rejected { ref code, .. } if code == "TOOL_CALL_ID_REUSED"),
        "{err:?}"
    );
    let ack = c
        .command(envelope(
            id16(0x24),
            "ListApprovals",
            ListApprovals {
                session_id: Some(session.clone()),
            }
            .encode_to_vec(),
        ))
        .await
        .unwrap();
    let list: ApprovalList = Client::result(&ack).unwrap();
    assert_eq!(list.approvals.len(), 1, "{list:?}");
    let a = &list.approvals[0];
    assert_eq!(
        (
            a.status.as_str(),
            a.tool_name.as_str(),
            a.effect_class.as_str()
        ),
        ("REQUESTED", "git.worktree.close", "Destructive")
    );
    assert_eq!(hex_id(a.approval_id.as_ref().unwrap()), approval_hex);
    assert!(a.expires_at_ms > a.requested_at_ms);
    let approval_id = a.approval_id.clone().unwrap();
    // Resolving needs the session lease.
    let err = c
        .command(envelope(
            id16(0x25),
            "ResolveApproval",
            ResolveApproval {
                approval_id: Some(approval_id.clone()),
                approve: true,
                reason: "ok".into(),
            }
            .encode_to_vec(),
        ))
        .await
        .unwrap_err();
    assert!(matches!(err, ClientError::Rejected { ref code, .. } if code == "LEASE_REQUIRED"));
    let ack = c
        .command(envelope_fenced(
            id16(0x26),
            "ResolveApproval",
            ResolveApproval {
                approval_id: Some(approval_id.clone()),
                approve: true,
                reason: "ok".into(),
            }
            .encode_to_vec(),
            g,
        ))
        .await
        .unwrap();
    let res: ApprovalResolvedAck = Client::result(&ack).unwrap();
    assert_eq!(res.status, "APPROVED");
    // The same call re-enters the pipeline, executes, and gets a receipt.
    let r = call(
        &mut c,
        0x27,
        0xA1,
        &task,
        g,
        "git.worktree.close",
        &close_args,
    )
    .await
    .unwrap();
    assert_eq!(r.status, "SUCCESS", "{r:?}");
    assert_eq!(r.approval_id, approval_hex);
    assert_eq!(r.effect_receipt_ids.len(), 1, "{r:?}");
    assert!(!std::path::Path::new(&wt).exists());
    // Idempotent afterwards.
    let r = call(
        &mut c,
        0x28,
        0xA1,
        &task,
        g,
        "git.worktree.close",
        &close_args,
    )
    .await
    .unwrap();
    assert_eq!(r.status, "SUCCESS");
    let ack = c
        .command(envelope(
            id16(0x29),
            "GetEffectReceipts",
            GetEffectReceipts {
                task_id: Some(task.clone()),
            }
            .encode_to_vec(),
        ))
        .await
        .unwrap();
    let receipts: EffectReceiptList = Client::result(&ack).unwrap();
    assert!(receipts.chain_valid, "{}", receipts.detail);
    assert_eq!(receipts.receipts.len(), 1);
    let rc = &receipts.receipts[0];
    assert_eq!(hex_id(rc.approval_id.as_ref().unwrap()), approval_hex);
    assert_eq!(
        hex_id(rc.capability_lease_id.as_ref().unwrap()),
        hex_id(lease.lease_id.as_ref().unwrap())
    );
    assert_eq!(
        (rc.status.as_str(), rc.previous_receipt_hash.as_str()),
        ("SUCCESS", "")
    );
    assert_eq!(rc.receipt_hash.len(), 64);
    assert!(rc.policy_decision.starts_with("approval:"));
    // Denied approval: the call fails and stays failed.
    let wt2 = format!("{root_json}-wt2");
    let r = call(
        &mut c,
        0x2A,
        0xA2,
        &task,
        g,
        "git.worktree.create",
        &format!(r#"{{"branch":"task/k2","path":"{wt2}"}}"#),
    )
    .await
    .unwrap();
    assert_eq!(r.status, "SUCCESS", "{r:?}");
    let close2 = format!(r#"{{"path":"{wt2}"}}"#);
    let r = call(&mut c, 0x2B, 0xA3, &task, g, "git.worktree.close", &close2)
        .await
        .unwrap();
    assert_eq!(r.status, "APPROVAL_PENDING");
    let ack = c
        .command(envelope_fenced(
            id16(0x2C),
            "ResolveApproval",
            ResolveApproval {
                approval_id: Some(Id {
                    value: decode_hex(&r.approval_id).unwrap(),
                }),
                approve: false,
                reason: "keep it".into(),
            }
            .encode_to_vec(),
            g,
        ))
        .await
        .unwrap();
    assert_eq!(
        Client::result::<ApprovalResolvedAck>(&ack).unwrap().status,
        "DENIED"
    );
    let r = call(&mut c, 0x2D, 0xA3, &task, g, "git.worktree.close", &close2)
        .await
        .unwrap();
    assert_eq!(
        (r.status.as_str(), r.error_code.as_str()),
        ("POLICY_DENIED", "APPROVAL_DENIED"),
        "{r:?}"
    );
    assert!(std::path::Path::new(&wt2).exists());
    // QUAL-EV-0045 at the Core: an autonomous task cannot even ask.
    let auto = new_task(&mut c, 0xF3, &session, &root, "local_autonomous", g).await;
    let r = call(&mut c, 0x2E, 0xA4, &auto, g, "git.worktree.close", &close2)
        .await
        .unwrap();
    assert_eq!(
        (r.status.as_str(), r.error_code.as_str()),
        ("POLICY_DENIED", "PROFILE_CEILING"),
        "{r:?}"
    );
    let r = call(
        &mut c,
        0x2F,
        0xA5,
        &auto,
        g,
        "fs.read",
        r#"{"path":"README.md"}"#,
    )
    .await
    .unwrap();
    assert_eq!(r.status, "SUCCESS", "{r:?}");
    // Emergency stop revokes every lease in the session; new effects (and
    // lease-bearing reads) are refused; the log records it.
    let ack = c
        .command(envelope_fenced(
            id16(0x30),
            "EmergencyStop",
            EmergencyStop {
                session_id: Some(session.clone()),
                reason: "operator".into(),
            }
            .encode_to_vec(),
            g,
        ))
        .await
        .unwrap();
    let stopped: EmergencyStopped = Client::result(&ack).unwrap();
    assert_eq!(stopped.leases_revoked, 2);
    let r = call(
        &mut c,
        0x31,
        0xA6,
        &task,
        g,
        "change.apply",
        r#"{"path":"README.md","op":"replace","content":"x"}"#,
    )
    .await
    .unwrap();
    assert_eq!(
        (r.status.as_str(), r.error_code.as_str()),
        ("POLICY_DENIED", "EMERGENCY_STOP"),
        "{r:?}"
    );
    assert_eq!(
        std::fs::read_to_string(repo.path().join("README.md")).unwrap(),
        "# demo\n"
    );
    let ack = c
        .command(envelope(
            id16(0x32),
            "GetCapabilityLeases",
            GetCapabilityLeases {
                task_id: Some(task.clone()),
            }
            .encode_to_vec(),
        ))
        .await
        .unwrap();
    let leases: CapabilityLeaseList = Client::result(&ack).unwrap();
    assert_eq!(leases.leases[0].status, "REVOKED");
    assert!(leases.leases[0].revoke_reason.starts_with("EMERGENCY_STOP"));
    // Event trail on the canonical log.
    let mut s = core.client().await;
    s.subscribe(session.clone(), 0).await.unwrap();
    let mut seen: Vec<(String, String)> = Vec::new();
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while std::time::Instant::now() < deadline {
        match tokio::time::timeout(Duration::from_millis(500), s.next_event()).await {
            Ok(Ok(Some(e))) => {
                let ev = e.event.unwrap();
                seen.push((ev.aggregate_type, ev.event_type));
            }
            _ => break,
        }
    }
    let types = |agg: &str| {
        seen.iter()
            .filter(|(a, _)| a == agg)
            .map(|(_, t)| t.as_str())
            .collect::<Vec<_>>()
    };
    assert_eq!(
        types("approval"),
        [
            "ApprovalRequested",
            "ApprovalResolved",
            "ApprovalRequested",
            "ApprovalResolved"
        ]
    );
    let leases_trail = types("capability_lease");
    assert_eq!(
        leases_trail
            .iter()
            .filter(|t| **t == "CapabilityLeaseGranted")
            .count(),
        2
    );
    assert_eq!(
        leases_trail
            .iter()
            .filter(|t| **t == "CapabilityLeaseRevoked")
            .count(),
        2
    );
    assert!(types("session").contains(&"EmergencyStopActivated"));
    let calls = types("tool_call");
    assert!(
        calls.contains(&"ToolCallApprovalRequested") && calls.contains(&"EffectReceiptAppended"),
        "{calls:?}"
    );
}
