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
        Self::spawn_with_env(data_dir, &[])
    }

    fn spawn_with_env(data_dir: &std::path::Path, env: &[(&str, &str)]) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_modbit-core"))
            .arg("--data-dir")
            .arg(data_dir)
            .envs(env.iter().map(|(k, v)| (k.to_string(), v.to_string())))
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
        .command(envelope(id16(0x31), "FrobnicateTask", vec![]))
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
            ListTools { task_id: None }.encode_to_vec(),
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

/// Minimal OpenAI-compatible SSE server for the Core-level gateway proof.
/// Answers every request with a tool call when tools were projected, else text.
async fn fake_openai() -> (
    String,
    std::sync::Arc<std::sync::Mutex<Vec<serde_json::Value>>>,
) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let seen2 = std::sync::Arc::clone(&seen);
    tokio::spawn(async move {
        loop {
            let Ok((mut sock, _)) = listener.accept().await else {
                return;
            };
            let seen = std::sync::Arc::clone(&seen2);
            tokio::spawn(async move {
                let mut buf = Vec::new();
                let mut tmp = [0u8; 4096];
                let (head_end, len) = loop {
                    let n = sock.read(&mut tmp).await.unwrap_or(0);
                    if n == 0 {
                        return;
                    }
                    buf.extend_from_slice(&tmp[..n]);
                    if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                        let head = String::from_utf8_lossy(&buf[..i]).to_string();
                        let len = head
                            .lines()
                            .find_map(|l| {
                                let (k, v) = l.split_once(':')?;
                                k.eq_ignore_ascii_case("content-length")
                                    .then(|| v.trim().parse::<usize>().ok())
                                    .flatten()
                            })
                            .unwrap_or(0);
                        break (i + 4, len);
                    }
                };
                while buf.len() < head_end + len {
                    let n = sock.read(&mut tmp).await.unwrap_or(0);
                    if n == 0 {
                        break;
                    }
                    buf.extend_from_slice(&tmp[..n]);
                }
                let body: serde_json::Value =
                    serde_json::from_slice(&buf[head_end..head_end + len]).unwrap_or_default();
                let with_tools = body["tools"].is_array();
                seen.lock().unwrap().push(body);
                let mut frames: Vec<String> = Vec::new();
                if with_tools {
                    frames.push(serde_json::json!({"id":"c1","model":"gpt-5-mini-2026","choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"id":"call_p","type":"function","function":{"name":"probe.echo","arguments":"{\"text\":\"pong\"}"}}]},"finish_reason":null}]}).to_string());
                    frames.push(serde_json::json!({"id":"c1","model":"gpt-5-mini-2026","choices":[{"index":0,"delta":{},"finish_reason":"tool_calls"}],"usage":{"prompt_tokens":21,"completion_tokens":7}}).to_string());
                } else {
                    frames.push(serde_json::json!({"id":"c2","model":"gpt-5-mini-2026","choices":[{"index":0,"delta":{"content":"pong"},"finish_reason":null}]}).to_string());
                    frames.push(serde_json::json!({"id":"c2","model":"gpt-5-mini-2026","choices":[{"index":0,"delta":{},"finish_reason":"stop"}],"usage":{"prompt_tokens":20,"completion_tokens":1,"prompt_tokens_details":{"cached_tokens":16}}}).to_string());
                }
                frames.push("[DONE]".into());
                let _ = sock
                    .write_all(b"HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ntransfer-encoding: chunked\r\n\r\n")
                    .await;
                for f in frames {
                    let frame = format!("data: {f}\n\n");
                    let _ = sock
                        .write_all(format!("{:x}\r\n{}\r\n", frame.len(), frame).as_bytes())
                        .await;
                }
                let _ = sock.write_all(b"0\r\n\r\n").await;
            });
        }
    });
    (format!("http://127.0.0.1:{port}"), seen)
}

/// M2.6: the Provider Gateway is reachable only through the Core; the
/// catalog, health and a real streaming probe are served over the socket.
#[tokio::test]
async fn m2_6_provider_gateway_streams_through_the_core_over_real_http() {
    use modbit_protocol::v1::{ListModels, ModelList, ModelProbed, ProbeModel};
    let (base, seen) = fake_openai().await;
    let dir = tempfile::tempdir().unwrap();
    let core = CoreProcess::spawn_with_env(
        dir.path(),
        &[
            ("MODBIT_OPENAI_BASE_URL", &base),
            ("OPENAI_API_KEY", ""),
            ("ANTHROPIC_API_KEY", ""),
        ],
    );
    let mut c = core.client().await;
    let ack = c
        .command(envelope(
            id16(0x60),
            "ListModels",
            ListModels {}.encode_to_vec(),
        ))
        .await
        .unwrap();
    let list: ModelList = Client::result(&ack).unwrap();
    assert!(
        list.models.iter().any(|m| m.endpoint == "openai"
            && m.model == "gpt-5-mini"
            && m.tools
            && m.credential_available),
        "{list:?}"
    );
    assert!(
        !list.models.iter().any(|m| m.endpoint == "anthropic"),
        "no key, no base url: not registered"
    );
    assert_eq!(
        list.health
            .iter()
            .find(|h| h.endpoint == "openai")
            .map(|h| h.requests),
        Some(0)
    );
    // Text probe.
    let ack = c
        .command(envelope(
            id16(0x61),
            "ProbeModel",
            ProbeModel {
                endpoint: "openai".into(),
                model: "gpt-5-mini".into(),
                prompt: "Say pong.".into(),
                with_tools: false,
                timeout_ms: 10_000,
            }
            .encode_to_vec(),
        ))
        .await
        .unwrap();
    let r: ModelProbed = Client::result(&ack).unwrap();
    assert_eq!(
        (r.status.as_str(), r.text.as_str(), r.stop_reason.as_str()),
        ("COMPLETED", "pong", "end_turn"),
        "{r:?}"
    );
    assert_eq!(
        (r.input_tokens, r.output_tokens, r.cached_input_tokens),
        (20, 1, 16)
    );
    let route: serde_json::Value = serde_json::from_str(&r.route_json).unwrap();
    assert_eq!(
        (
            route["requested_model"].as_str(),
            route["resolved_model"].as_str()
        ),
        (Some("gpt-5-mini"), Some("gpt-5-mini-2026"))
    );
    // Tool probe: a typed tool call comes back through the normalized events.
    let ack = c
        .command(envelope(
            id16(0x62),
            "ProbeModel",
            ProbeModel {
                endpoint: "openai".into(),
                model: "gpt-5-mini".into(),
                prompt: "Echo pong.".into(),
                with_tools: true,
                timeout_ms: 10_000,
            }
            .encode_to_vec(),
        ))
        .await
        .unwrap();
    let r: ModelProbed = Client::result(&ack).unwrap();
    assert_eq!(
        (
            r.status.as_str(),
            r.stop_reason.as_str(),
            r.tool_call_name.as_str()
        ),
        ("COMPLETED", "tool_use", "probe.echo"),
        "{r:?}"
    );
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&r.tool_call_arguments_json).unwrap()["text"],
        "pong"
    );
    // Capability mismatch is refused before any network call (REQ-EV-0028).
    let before = seen.lock().unwrap().len();
    let ack = c
        .command(envelope(
            id16(0x63),
            "ProbeModel",
            ProbeModel {
                endpoint: "openai".into(),
                model: "gpt-4.1".into(),
                prompt: "x".into(),
                with_tools: false,
                timeout_ms: 1000,
            }
            .encode_to_vec(),
        ))
        .await
        .unwrap();
    let ok: ModelProbed = Client::result(&ack).unwrap();
    assert_eq!(ok.status, "COMPLETED");
    let ack = c
        .command(envelope(
            id16(0x64),
            "ProbeModel",
            ProbeModel {
                endpoint: "openai".into(),
                model: "no-such-model".into(),
                prompt: "x".into(),
                with_tools: false,
                timeout_ms: 1000,
            }
            .encode_to_vec(),
        ))
        .await
        .unwrap();
    let r: ModelProbed = Client::result(&ack).unwrap();
    assert_eq!(
        (r.status.as_str(), r.error_code.as_str()),
        ("ROUTE_REFUSED", "UNKNOWN_MODEL")
    );
    assert_eq!(seen.lock().unwrap().len(), before + 1);
    // Health reflects the real calls; the wire carried the Core's requests.
    let ack = c
        .command(envelope(
            id16(0x65),
            "ListModels",
            ListModels {}.encode_to_vec(),
        ))
        .await
        .unwrap();
    let list: ModelList = Client::result(&ack).unwrap();
    let h = list.health.iter().find(|h| h.endpoint == "openai").unwrap();
    assert_eq!((h.requests, h.successes, h.failures), (3, 3, 0));
    assert!(h.last_first_token_ms > 0 || h.requests > 0);
    let bodies = seen.lock().unwrap().clone();
    assert_eq!(bodies[0]["messages"][1]["content"], "Say pong.");
    assert_eq!(bodies[1]["tools"][0]["function"]["name"], "probe.echo");
}

/// Scripted OpenAI-compatible model for the runtime proof: the reply is
/// chosen from the number of tool results already in the conversation, so the
/// same script drives fresh runs and resumed runs identically.
async fn scripted_model(
    script: Vec<serde_json::Value>,
    stall_at: Option<usize>,
) -> (
    String,
    std::sync::Arc<std::sync::Mutex<Vec<serde_json::Value>>>,
) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let seen2 = std::sync::Arc::clone(&seen);
    let script = std::sync::Arc::new(script);
    let stalled_once = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    tokio::spawn(async move {
        loop {
            let Ok((mut sock, _)) = listener.accept().await else {
                return;
            };
            let seen = std::sync::Arc::clone(&seen2);
            let script = std::sync::Arc::clone(&script);
            let stalled_once = std::sync::Arc::clone(&stalled_once);
            tokio::spawn(async move {
                let mut buf = Vec::new();
                let mut tmp = [0u8; 8192];
                let (head_end, len) = loop {
                    let n = sock.read(&mut tmp).await.unwrap_or(0);
                    if n == 0 {
                        return;
                    }
                    buf.extend_from_slice(&tmp[..n]);
                    if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                        let head = String::from_utf8_lossy(&buf[..i]).to_string();
                        let len = head
                            .lines()
                            .find_map(|l| {
                                let (k, v) = l.split_once(':')?;
                                k.eq_ignore_ascii_case("content-length")
                                    .then(|| v.trim().parse::<usize>().ok())
                                    .flatten()
                            })
                            .unwrap_or(0);
                        break (i + 4, len);
                    }
                };
                while buf.len() < head_end + len {
                    let n = sock.read(&mut tmp).await.unwrap_or(0);
                    if n == 0 {
                        break;
                    }
                    buf.extend_from_slice(&tmp[..n]);
                }
                let body: serde_json::Value =
                    serde_json::from_slice(&buf[head_end..head_end + len]).unwrap_or_default();
                let results = body["messages"]
                    .as_array()
                    .map(|m| m.iter().filter(|x| x["role"] == "tool").count())
                    .unwrap_or(0);
                seen.lock().unwrap().push(body);
                if stall_at == Some(results)
                    && !stalled_once.swap(true, std::sync::atomic::Ordering::SeqCst)
                {
                    tokio::time::sleep(Duration::from_secs(600)).await;
                    return;
                }
                let reply = script.get(results).cloned().unwrap_or_else(
                    || serde_json::json!({"text": "I have nothing further to do."}),
                );
                let mut frames: Vec<String> = Vec::new();
                if let Some(t) = reply["text"].as_str() {
                    frames.push(serde_json::json!({"id":"c","model":"scripted-1","choices":[{"index":0,"delta":{"content":t},"finish_reason":null}]}).to_string());
                }
                let calls = reply["calls"].as_array().cloned().unwrap_or_default();
                for (i, c) in calls.iter().enumerate() {
                    frames.push(serde_json::json!({"id":"c","model":"scripted-1","choices":[{"index":0,"delta":{"tool_calls":[{"index":i,"id":format!("call_{results}_{i}"),"type":"function","function":{"name":c["name"],"arguments":c["args"].to_string()}}]},"finish_reason":null}]}).to_string());
                }
                let finish = if calls.is_empty() {
                    "stop"
                } else {
                    "tool_calls"
                };
                frames.push(serde_json::json!({"id":"c","model":"scripted-1","choices":[{"index":0,"delta":{},"finish_reason":finish}],"usage":{"prompt_tokens":100,"completion_tokens":10}}).to_string());
                frames.push("[DONE]".into());
                let _ = sock
                    .write_all(b"HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ntransfer-encoding: chunked\r\n\r\n")
                    .await;
                for f in frames {
                    let frame = format!("data: {f}\n\n");
                    let _ = sock
                        .write_all(format!("{:x}\r\n{}\r\n", frame.len(), frame).as_bytes())
                        .await;
                }
                let _ = sock.write_all(b"0\r\n\r\n").await;
            });
        }
    });
    (format!("http://127.0.0.1:{port}"), seen)
}

fn git_repo_with_failing_check() -> (tempfile::TempDir, String) {
    let repo = tempfile::tempdir().unwrap();
    std::fs::write(repo.path().join("qty.txt"), "quantity = -5\n").unwrap();
    // The "test": passes only once the file says quantities are validated.
    std::fs::write(
        repo.path().join("check.sh"),
        "grep -q 'validated' qty.txt\n",
    )
    .unwrap();
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
    (repo, root)
}

/// The E2E-001/002 script: read, plan, edit (wrong), test fails, edit (right),
/// test passes, complete. `hash` placeholders are filled by the model from the
/// observation it received (the script uses expected_content_hash only on the
/// second edit, read from the first fs.read result it "remembers").
fn coding_script(fs_read_hash: &str) -> Vec<serde_json::Value> {
    use serde_json::json;
    vec![
        json!({"text": "Reading the file first.", "calls": [{"name": "fs.read", "args": {"path": "qty.txt"}}]}),
        json!({"text": "Planning.", "calls": [{"name": "plan.update", "args": {"outcome": "reject negative quantities", "expected_files": ["qty.txt"], "verification": ["sh check.sh"], "protected_effects": []}}]}),
        // Reproduction first (docs/28 §5, PX-039): the reported failure is
        // reproduced before the fix; this repository has no derivable suite,
        // so the agent runs the reproduction command itself.
        json!({"calls": [{"name": "test.run", "args": {"argv": ["sh", "check.sh"], "inherit_env": true}}]}),
        json!({"calls": [{"name": "change.apply", "args": {"path": "qty.txt", "op": "replace", "content": "quantity = 5\n", "expected_content_hash": fs_read_hash}}]}),
        json!({"calls": [{"name": "test.run", "args": {"argv": ["sh", "check.sh"], "inherit_env": true}}]}),
        json!({"text": "The check failed; the file must say validated.", "calls": [{"name": "change.apply", "args": {"path": "qty.txt", "op": "replace", "content": "quantity = 5 # validated: negatives rejected\n"}}]}),
        json!({"calls": [{"name": "test.run", "args": {"argv": ["sh", "check.sh"], "inherit_env": true}}]}),
        json!({"calls": [{"name": "task.complete", "args": {"summary": "Negative quantities are rejected and the check passes.", "self_review": {"findings": [{"text": "check.sh passes at the candidate revision", "resolved": true}], "verification": ["sh check.sh"]}}}]}),
    ]
}

/// Wait until the task reaches `state` (a resumed run may not be alive yet
/// when `StartTask` returns, so `wait_task` alone can read the stale state).
async fn wait_for_state(
    c: &mut Client,
    task: &Id,
    state: &str,
    secs: u64,
) -> modbit_protocol::v1::TaskStatus {
    let deadline = std::time::Instant::now() + Duration::from_secs(secs);
    loop {
        let st = wait_task(c, task, 1).await;
        if st.state == state || std::time::Instant::now() > deadline {
            return st;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

async fn wait_task(c: &mut Client, task: &Id, secs: u64) -> modbit_protocol::v1::TaskStatus {
    use modbit_protocol::v1::{GetTaskStatus, TaskStatus};
    let deadline = std::time::Instant::now() + Duration::from_secs(secs);
    loop {
        let ack = c
            .command(envelope(
                Id {
                    value: (0..16).map(|_| rand::random::<u8>()).collect(),
                },
                "GetTaskStatus",
                GetTaskStatus {
                    task_id: Some(task.clone()),
                }
                .encode_to_vec(),
            ))
            .await
            .unwrap();
        let st: TaskStatus = Client::result(&ack).unwrap();
        if !st.loop_alive || std::time::Instant::now() > deadline {
            return st;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

async fn task_events(
    core: &CoreProcess,
    session: &Id,
    task: &Id,
) -> Vec<(String, String, serde_json::Value)> {
    let mut s = core.client().await;
    s.subscribe(session.clone(), 0).await.unwrap();
    let mut out = Vec::new();
    while let Ok(Ok(Some(e))) =
        tokio::time::timeout(Duration::from_millis(400), s.next_event()).await
    {
        let ev = e.event.unwrap();
        if ev.task_id.as_ref() == Some(task) {
            let p: serde_json::Value = serde_json::from_slice(&ev.payload).unwrap_or_default();
            out.push((ev.aggregate_type, ev.event_type, p["payload"].clone()));
        }
    }
    out
}

/// M2.7: a real coding task end to end (E2E-001/002 shape): read → plan →
/// edit → failing check is evidence, not turn failure → fix → passing check
/// → completion handshake → ReadyForReview, with every step on the log.
#[tokio::test]
async fn m2_7_one_agent_runtime_drives_a_coding_task_to_ready_for_review() {
    use modbit_protocol::v1::{StartTask, TaskRunStarted};
    let (repo, root) = git_repo_with_failing_check();
    let hash = {
        use sha2::Digest;
        hex::encode(sha2::Sha256::digest(
            std::fs::read(repo.path().join("qty.txt")).unwrap(),
        ))
    };
    let (base, seen) = scripted_model(coding_script(&hash), None).await;
    let dir = tempfile::tempdir().unwrap();
    let core = CoreProcess::spawn_with_env(
        dir.path(),
        &[
            ("MODBIT_OPENAI_BASE_URL", &base),
            ("OPENAI_API_KEY", ""),
            ("ANTHROPIC_API_KEY", ""),
        ],
    );
    let mut c = core.client().await;
    let (session, _) = create_session(&mut c, id16(0x70)).await;
    let g = lease_for(&session);
    let ack = c
        .command(envelope_fenced(
            id16(0x71),
            "CreateTask",
            CreateTask {
                session_id: Some(session.clone()),
                goal_text: "Reject negative quantities and make the check pass.".into(),
                workspace_id: None,
                execution_profile: String::new(),
                origin: "cli".into(),
                workspace_root: root.clone(),
            }
            .encode_to_vec(),
            g,
        ))
        .await
        .unwrap();
    let task = Client::result::<TaskCreated>(&ack)
        .unwrap()
        .task_id
        .unwrap();
    // Starting needs the session lease; then the loop runs on its own.
    let err = c
        .command(envelope(
            id16(0x72),
            "StartTask",
            StartTask {
                task_id: Some(task.clone()),
                endpoint: String::new(),
                model: String::new(),
                max_turns: 0,
                max_tool_calls: 0,
                max_no_progress_turns: 0,
            }
            .encode_to_vec(),
        ))
        .await
        .unwrap_err();
    assert!(matches!(err, ClientError::Rejected { ref code, .. } if code == "LEASE_REQUIRED"));
    let ack = c
        .command(envelope_fenced(
            id16(0x73),
            "StartTask",
            StartTask {
                task_id: Some(task.clone()),
                endpoint: String::new(),
                model: "gpt-5-mini".into(),
                max_turns: 0,
                max_tool_calls: 0,
                max_no_progress_turns: 0,
            }
            .encode_to_vec(),
            g,
        ))
        .await
        .unwrap();
    let started: TaskRunStarted = Client::result(&ack).unwrap();
    assert_eq!(
        (
            started.resumed,
            started.endpoint.as_str(),
            started.model.as_str()
        ),
        (false, "openai", "gpt-5-mini")
    );
    let st = wait_task(&mut c, &task, 60).await;
    let trail = task_events(&core, &session, &task).await;
    assert_eq!(
        (st.state.as_str(), st.run_state.as_str(), st.loop_alive),
        ("ReadyForReview", "Completed", false),
        "{st:?}\n{trail:#?}"
    );
    // Real effect on the real repository.
    assert_eq!(
        std::fs::read_to_string(repo.path().join("qty.txt")).unwrap(),
        "quantity = 5 # validated: negatives rejected\n"
    );
    // Event trail.
    let evs = task_events(&core, &session, &task).await;
    let names = |agg: &str| {
        evs.iter()
            .filter(|(a, _, _)| a == agg)
            .map(|(_, t, _)| t.as_str())
            .collect::<Vec<_>>()
    };
    let task_trail = names("task");
    for t in [
        "TaskStarted",
        "PlanRecorded",
        "SelfReviewRecorded",
        "TaskReadyForReview",
    ] {
        assert!(task_trail.contains(&t), "{task_trail:?}");
    }
    assert!(
        !task_trail.contains(&"TaskFailed"),
        "a failing check never fails the task (REQ-EV-0099)"
    );
    let run_trail = names("run");
    assert_eq!(
        &run_trail[..2],
        ["RunCreated", "RunStarted"],
        "{run_trail:?}"
    );
    assert_eq!(run_trail.last(), Some(&"RunCompleted"), "{run_trail:?}");
    assert!(
        run_trail.contains(&"VerificationBaselineRecorded"),
        "a baseline precedes the first write even when no runner is configured: {run_trail:?}"
    );
    let turns = names("turn");
    assert_eq!(
        turns.iter().filter(|t| **t == "TurnPrepared").count(),
        8,
        "{turns:?}"
    );
    assert_eq!(turns.iter().filter(|t| **t == "TurnCompleted").count(), 8);
    assert!(
        turns.contains(&"ContextPackCompiled")
            && turns.contains(&"ToolProjectionSelected")
            && turns.contains(&"ModelUsageRecorded")
    );
    let steps: Vec<String> = evs
        .iter()
        .filter(|(a, t, _)| a == "run_step" && t == "StepScheduled")
        .map(|(_, _, p)| {
            p["step_type"]["kind"]
                .as_str()
                .unwrap_or_default()
                .to_owned()
        })
        .collect();
    assert_eq!(steps.iter().filter(|s| *s == "CONTEXT_COMPILE").count(), 8);
    assert_eq!(steps.iter().filter(|s| *s == "MODEL_INVOKE").count(), 8);
    assert_eq!(
        steps.iter().filter(|s| *s == "TOOL_CALL").count(),
        6,
        "{steps:?}"
    );
    assert_eq!(
        (
            steps.iter().filter(|s| *s == "PLAN").count(),
            steps.iter().filter(|s| *s == "SELF_REVIEW").count()
        ),
        (1, 1)
    );
    let calls = names("tool_call");
    assert_eq!(
        calls.iter().filter(|t| **t == "ToolCallSucceeded").count(),
        6,
        "{calls:?}"
    );
    // The model saw the failure as evidence: the request after each check run
    // carries the failed check observation.
    let bodies = seen.lock().unwrap().clone();
    assert_eq!(bodies.len(), 8);
    let failed_observations = bodies
        .iter()
        .flat_map(|b| b["messages"].as_array().cloned().unwrap_or_default())
        .filter(|m| m["role"] == "tool")
        .filter_map(|m| m["content"].as_str().map(str::to_owned))
        .filter(|c| c.contains("status: SUCCESS") && c.contains("\"status\":\"FAILED\""))
        .count();
    assert!(failed_observations > 0, "{bodies:#?}");
    assert!(
        bodies[0]["messages"][0]["content"]
            .as_str()
            .unwrap()
            .contains("plan.update"),
        "system segment is stable"
    );
    assert!(
        bodies
            .last()
            .and_then(|b| b["messages"].as_array().cloned())
            .unwrap_or_default()
            .iter()
            .any(|m| {
                m["role"] == "tool"
                    && m["content"]
                        .as_str()
                        .unwrap_or_default()
                        .contains("\"status\":\"PASSED\"")
            }),
        "the passing check is in the transcript before completion"
    );
    assert!(
        bodies[0]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .any(|t| t["function"]["name"] == "task.complete")
    );
}

/// Harness rules: a write before the plan is refused with evidence, budgets
/// exhaust into Waiting with attention, and a Core restart resumes the run
/// from the log without repeating a tool action (E2E-003 shape).
#[tokio::test]
async fn m2_7_harness_refuses_unplanned_writes_exhausts_budgets_and_resumes_after_restart() {
    use modbit_protocol::v1::{CancelTask, StartTask, TaskCancelRequested, TaskRunStarted};
    use serde_json::json;
    let (repo, root) = git_repo_with_failing_check();
    // Script: unplanned write (refused), plan, write, then the model stalls on
    // the fourth request until the Core is restarted; afterwards it completes.
    let script = vec![
        json!({"calls": [{"name": "change.apply", "args": {"path": "qty.txt", "op": "replace", "content": "x\n"}}]}),
        json!({"calls": [{"name": "plan.update", "args": {"outcome": "o", "expected_files": ["qty.txt"]}}]}),
        json!({"calls": [{"name": "fs.read", "args": {"path": "qty.txt"}}]}),
        json!({"calls": [{"name": "change.apply", "args": {"path": "qty.txt", "op": "replace", "content": "quantity = 1 # validated\n"}}]}),
        json!({"calls": [{"name": "test.run", "args": {"argv": ["sh", "check.sh"], "inherit_env": true}}]}),
        json!({"calls": [{"name": "task.complete", "args": {"summary": "done", "self_review": {"findings": []}}}]}),
    ];
    let (base, seen) = scripted_model(script.clone(), Some(4)).await;
    let dir = tempfile::tempdir().unwrap();
    let env = [
        ("MODBIT_OPENAI_BASE_URL", base.as_str()),
        ("OPENAI_API_KEY", ""),
        ("ANTHROPIC_API_KEY", ""),
    ];
    let mut core = CoreProcess::spawn_with_env(dir.path(), &env);
    let mut c = core.client().await;
    let (session, _) = create_session(&mut c, id16(0x80)).await;
    let g = lease_for(&session);
    let ack = c
        .command(envelope_fenced(
            id16(0x81),
            "CreateTask",
            CreateTask {
                session_id: Some(session.clone()),
                goal_text: "validate".into(),
                workspace_id: None,
                execution_profile: String::new(),
                origin: "cli".into(),
                workspace_root: root.clone(),
            }
            .encode_to_vec(),
            g,
        ))
        .await
        .unwrap();
    let task = Client::result::<TaskCreated>(&ack)
        .unwrap()
        .task_id
        .unwrap();
    // Budget exhaustion first: one turn only.
    let (base2, _) = scripted_model(
        vec![
            json!({"text": "thinking"}),
            json!({"text": "still thinking"}),
        ],
        None,
    )
    .await;
    let _ = base2;
    let ack = c
        .command(envelope_fenced(
            id16(0x82),
            "StartTask",
            StartTask {
                task_id: Some(task.clone()),
                endpoint: String::new(),
                model: "gpt-5-mini".into(),
                max_turns: 1,
                max_tool_calls: 0,
                max_no_progress_turns: 0,
            }
            .encode_to_vec(),
            g,
        ))
        .await
        .unwrap();
    let _: TaskRunStarted = Client::result(&ack).unwrap();
    let st = wait_task(&mut c, &task, 30).await;
    assert_eq!(
        (
            st.state.as_str(),
            st.wait_reason.as_str(),
            st.run_state.as_str()
        ),
        ("Waiting", "UserInput", "Suspended"),
        "{st:?}"
    );
    assert_eq!(
        std::fs::read_to_string(repo.path().join("qty.txt")).unwrap(),
        "quantity = -5\n",
        "the unplanned write was refused"
    );
    let evs = task_events(&core, &session, &task).await;
    assert!(
        evs.iter()
            .any(|(_, t, p)| t == "HarnessBudgetExhausted" && p["budget"] == "max_turns"),
        "{evs:?}"
    );
    assert!(evs.iter().any(|(a, t, p)| a == "run_step"
        && t == "StepFailed"
        && p["failure_code"] == "HARNESS_PLAN_REQUIRED"));
    assert!(
        evs.iter().all(|(_, t, _)| t != "ToolCallProposed"),
        "a harness refusal never reaches the kernel or an effector"
    );
    // Resume with a real budget: plan, write, then the model stalls; kill the Core mid-turn.
    let ack = c
        .command(envelope_fenced(
            id16(0x83),
            "StartTask",
            StartTask {
                task_id: Some(task.clone()),
                endpoint: String::new(),
                model: "gpt-5-mini".into(),
                max_turns: 20,
                max_tool_calls: 0,
                max_no_progress_turns: 0,
            }
            .encode_to_vec(),
            g,
        ))
        .await
        .unwrap();
    let started: TaskRunStarted = Client::result(&ack).unwrap();
    assert!(started.resumed);
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    while std::fs::read_to_string(repo.path().join("qty.txt")).unwrap()
        != "quantity = 1 # validated\n"
    {
        assert!(std::time::Instant::now() < deadline, "write did not land");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    // Let the stalled fourth request start, then hard-kill the Core.
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    while seen.lock().unwrap().len() < 5 {
        assert!(std::time::Instant::now() < deadline);
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    drop(c);
    core.kill();
    let core2 = CoreProcess::spawn_with_env(dir.path(), &env);
    let mut c2 = core2.client().await;
    let (session2, task2) = (session.clone(), task.clone());
    let st = wait_task(&mut c2, &task2, 5).await;
    assert_eq!(
        (
            st.state.as_str(),
            st.wait_reason.as_str(),
            st.run_state.as_str(),
            st.loop_alive
        ),
        ("Waiting", "External", "Suspended", false),
        "{st:?}"
    );
    let g2 = Some(acquire_lease(&mut c2, id16(0x84), session2.clone(), "resumer").await);
    let requests_before = seen.lock().unwrap().len();
    let ack = c2
        .command(envelope_fenced(
            id16(0x85),
            "StartTask",
            StartTask {
                task_id: Some(task2.clone()),
                endpoint: String::new(),
                model: "gpt-5-mini".into(),
                max_turns: 20,
                max_tool_calls: 0,
                max_no_progress_turns: 0,
            }
            .encode_to_vec(),
            g2,
        ))
        .await
        .unwrap();
    let started: TaskRunStarted = Client::result(&ack).unwrap();
    assert!(started.resumed);
    let st = wait_task(&mut c2, &task2, 60).await;
    let trail = task_events(&core2, &session2, &task2).await;
    assert_eq!(
        (st.state.as_str(), st.run_state.as_str()),
        ("ReadyForReview", "Completed"),
        "{st:?}\n{trail:#?}"
    );
    // The resumed conversation carried the earlier tool results (rebuilt from the log)
    // and no tool action ran twice.
    let bodies = seen.lock().unwrap().clone();
    let resumed_first = &bodies[requests_before]["messages"];
    let tool_results = resumed_first
        .as_array()
        .unwrap()
        .iter()
        .filter(|m| m["role"] == "tool")
        .count();
    assert_eq!(tool_results, 4, "{resumed_first}");
    let evs = task_events(&core2, &session2, &task2).await;
    let applies = evs
        .iter()
        .filter(|(a, t, p)| {
            a == "tool_call" && t == "ToolCallProposed" && p["tool_name"] == "change.apply"
        })
        .count();
    assert_eq!(applies, 1, "no duplicate write after resume");
    assert_eq!(
        evs.iter()
            .filter(|(a, t, _)| a == "run" && t == "RunResumed")
            .count(),
        2
    );
    assert!(
        evs.iter()
            .any(|(a, t, _)| a == "run" && t == "RunSuspended")
    );
    assert_eq!(
        std::fs::read_to_string(repo.path().join("qty.txt")).unwrap(),
        "quantity = 1 # validated\n"
    );
    // Cancel on a finished task is a no-op; on a live loop it interrupts the model stream.
    let ack = c2
        .command(envelope_fenced(
            id16(0x86),
            "CancelTask",
            CancelTask {
                task_id: Some(task2.clone()),
            }
            .encode_to_vec(),
            g2,
        ))
        .await
        .unwrap();
    let r: TaskCancelRequested = Client::result(&ack).unwrap();
    assert!(!r.was_running);
    let (base3, seen3) = scripted_model(vec![], Some(0)).await;
    let _ = base3;
    let _ = seen3;
}

/// Steering lands between steps as a durable `TaskSteered` and reaches the
/// model as a user message; cancellation interrupts the in-flight stream.
#[tokio::test]
async fn m2_7_steering_and_cancellation_apply_at_safe_boundaries() {
    use modbit_protocol::v1::{
        CancelTask, QueueInput, StartTask, TaskCancelRequested, TaskRunStarted,
    };
    use serde_json::json;
    let (_repo, root) = git_repo_with_failing_check();
    let script = vec![
        json!({"calls": [{"name": "fs.read", "args": {"path": "qty.txt"}}]}),
        json!({"calls": [{"name": "fs.stat", "args": {"path": "qty.txt"}}]}),
    ];
    let (base, seen) = scripted_model(script, Some(2)).await;
    let dir = tempfile::tempdir().unwrap();
    let core = CoreProcess::spawn_with_env(
        dir.path(),
        &[
            ("MODBIT_OPENAI_BASE_URL", &base),
            ("OPENAI_API_KEY", ""),
            ("ANTHROPIC_API_KEY", ""),
        ],
    );
    let mut c = core.client().await;
    let (session, _) = create_session(&mut c, id16(0x90)).await;
    let g = lease_for(&session);
    let ack = c
        .command(envelope_fenced(
            id16(0x91),
            "CreateTask",
            CreateTask {
                session_id: Some(session.clone()),
                goal_text: "look around".into(),
                workspace_id: None,
                execution_profile: String::new(),
                origin: "cli".into(),
                workspace_root: root,
            }
            .encode_to_vec(),
            g,
        ))
        .await
        .unwrap();
    let task = Client::result::<TaskCreated>(&ack)
        .unwrap()
        .task_id
        .unwrap();
    // Queue a steer before the loop starts: it is applied at the first boundary.
    let ack = c
        .command(envelope_fenced(
            id16(0x92),
            "QueueInput",
            QueueInput {
                task_id: Some(task.clone()),
                input_id: "steer-1".into(),
                mode: "STEER".into(),
                text: "Only read files, never write.".into(),
            }
            .encode_to_vec(),
            g,
        ))
        .await
        .unwrap();
    assert_eq!(ack.status, CommandStatus::Accepted as i32);
    let ack = c
        .command(envelope_fenced(
            id16(0x93),
            "StartTask",
            StartTask {
                task_id: Some(task.clone()),
                endpoint: String::new(),
                model: "gpt-5-mini".into(),
                max_turns: 10,
                max_tool_calls: 0,
                max_no_progress_turns: 5,
            }
            .encode_to_vec(),
            g,
        ))
        .await
        .unwrap();
    let _: TaskRunStarted = Client::result(&ack).unwrap();
    // Third request stalls; cancel while the stream is open.
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    while seen.lock().unwrap().len() < 3 {
        assert!(std::time::Instant::now() < deadline);
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let ack = c
        .command(envelope_fenced(
            id16(0x94),
            "CancelTask",
            CancelTask {
                task_id: Some(task.clone()),
            }
            .encode_to_vec(),
            g,
        ))
        .await
        .unwrap();
    let r: TaskCancelRequested = Client::result(&ack).unwrap();
    assert!(r.was_running);
    let st = wait_task(&mut c, &task, 20).await;
    assert_eq!(
        (st.state.as_str(), st.run_state.as_str(), st.loop_alive),
        ("Cancelled", "Cancelled", false),
        "{st:?}"
    );
    let bodies = seen.lock().unwrap().clone();
    let first = bodies[0]["messages"].as_array().unwrap();
    assert!(
        first.iter().any(|m| m["role"] == "user"
            && m["content"]
                .as_str()
                .unwrap_or_default()
                .contains("[STEER] Only read files")),
        "{first:?}"
    );
    let evs = task_events(&core, &session, &task).await;
    let names: Vec<&str> = evs.iter().map(|(_, t, _)| t.as_str()).collect();
    assert!(
        names.contains(&"TaskSteered")
            && names.contains(&"TurnInterrupted")
            && names.contains(&"StepCancelled")
            && names.contains(&"RunCancelled")
            && names.contains(&"TaskCancelled"),
        "{names:?}"
    );
}

fn fixture_repo(name: &str) -> (tempfile::TempDir, String) {
    let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/repos")
        .join(name);
    let dir = tempfile::tempdir().unwrap();
    fn copy(src: &std::path::Path, dst: &std::path::Path) {
        std::fs::create_dir_all(dst).unwrap();
        for e in std::fs::read_dir(src).unwrap() {
            let e = e.unwrap();
            let n = e.file_name();
            if n == "target" || n == "node_modules" || n == ".vitest" {
                continue;
            }
            if e.path().is_dir() {
                copy(&e.path(), &dst.join(&n));
            } else {
                // Checkouts on Windows may carry CRLF; fixtures are LF text.
                let bytes = std::fs::read(e.path()).unwrap();
                let text = String::from_utf8_lossy(&bytes).replace("\r\n", "\n");
                std::fs::write(dst.join(&n), text).unwrap();
            }
        }
    }
    copy(&src, dir.path());
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
            "fixture",
        ],
    ] {
        assert!(
            Command::new("git")
                .arg("-C")
                .arg(dir.path())
                .args(&args)
                .status()
                .unwrap()
                .success()
        );
    }
    let root = dir
        .path()
        .canonicalize()
        .unwrap()
        .to_string_lossy()
        .trim_start_matches(r"\\?\")
        .to_owned();
    (dir, root)
}

/// M2.8: the verification engine inside the agent loop on the real rust-cli
/// fixture (docs/64): BASELINE before the first write with KNOWN_FAILING and
/// a quarantined flaky test; DI-3 denies weakening the acceptance test; a
/// TARGETED run reports normalized failing checks; the COMPLETION run
/// attributes a REGRESSION and refuses completion until it is fixed.
#[tokio::test]
async fn m2_8_verification_engine_gates_completion_on_real_cargo_fixture() {
    use modbit_protocol::v1::{StartTask, TaskRunStarted};
    use serde_json::json;
    let (repo, root) = fixture_repo("rust-cli");
    let lib = std::fs::read_to_string(repo.path().join("src/lib.rs")).unwrap();
    let fixed_lib = lib.replace(
        "    Ok(n)\n",
        "    if n < 0 {\n        return Err(\"negative quantity\".into());\n    }\n    Ok(n)\n",
    );
    let broken_lib = fixed_lib.replace(
        "format!(\"{}.{:02}\", cents / 100, cents % 100)",
        "format!(\"{}.{:03}\", cents / 100, cents % 100)",
    );
    let tests_src = std::fs::read_to_string(repo.path().join("tests/quantities.rs")).unwrap();
    let weakened_tests = tests_src.replace(
        "#[test]\nfn acceptance_rejects_negative_quantity()",
        "#[test]\n#[ignore]\nfn acceptance_rejects_negative_quantity()",
    );
    assert_ne!(weakened_tests, tests_src);
    let script = vec![
        json!({"calls": [{"name": "fs.read", "args": {"path": "src/lib.rs"}}]}),
        json!({"calls": [{"name": "plan.update", "args": {"outcome": "reject negative quantities", "expected_files": ["src/lib.rs", "tests/quantities.rs"], "verification": ["acceptance_rejects_negative_quantity"]}}]}),
        // Tempting shortcut: weaken the acceptance test. DI-3 denies it (and BASELINE runs first).
        json!({"calls": [{"name": "fs.read", "args": {"path": "tests/quantities.rs"}}]}),
        json!({"calls": [{"name": "change.apply", "args": {"path": "tests/quantities.rs", "op": "replace", "content": weakened_tests}}]}),
        // Real fix that also breaks formatting.
        json!({"calls": [{"name": "change.apply", "args": {"path": "src/lib.rs", "op": "replace", "content": broken_lib}}]}),
        json!({"calls": [{"name": "verify.run", "args": {"reason": "check the fix"}}]}),
        json!({"calls": [{"name": "task.complete", "args": {"summary": "done", "self_review": {"findings": []}}}]}),
        json!({"calls": [{"name": "change.apply", "args": {"path": "src/lib.rs", "op": "replace", "content": fixed_lib}}]}),
        json!({"calls": [{"name": "task.complete", "args": {"summary": "negatives rejected; formatting restored", "self_review": {"findings": [{"text": "acceptance test passes", "resolved": true}]}}}]}),
    ];
    let (base, seen) = scripted_model(script, None).await;
    let dir = tempfile::tempdir().unwrap();
    let core = CoreProcess::spawn_with_env(
        dir.path(),
        &[
            ("MODBIT_OPENAI_BASE_URL", &base),
            ("OPENAI_API_KEY", ""),
            ("ANTHROPIC_API_KEY", ""),
            ("CARGO_TERM_COLOR", "always"),
        ],
    );
    let mut c = core.client().await;
    let (session, _) = create_session(&mut c, id16(0xA0)).await;
    let g = lease_for(&session);
    let ack = c
        .command(envelope_fenced(
            id16(0xA1),
            "CreateTask",
            CreateTask {
                session_id: Some(session.clone()),
                goal_text:
                    "Reject negative quantities so acceptance_rejects_negative_quantity passes."
                        .into(),
                workspace_id: None,
                execution_profile: String::new(),
                origin: "cli".into(),
                workspace_root: root.clone(),
            }
            .encode_to_vec(),
            g,
        ))
        .await
        .unwrap();
    let task = Client::result::<TaskCreated>(&ack)
        .unwrap()
        .task_id
        .unwrap();
    let ack = c
        .command(envelope_fenced(
            id16(0xA2),
            "StartTask",
            StartTask {
                task_id: Some(task.clone()),
                endpoint: String::new(),
                model: "gpt-5-mini".into(),
                max_turns: 20,
                max_tool_calls: 0,
                max_no_progress_turns: 5,
            }
            .encode_to_vec(),
            g,
        ))
        .await
        .unwrap();
    let _: TaskRunStarted = Client::result(&ack).unwrap();
    let st = wait_task(&mut c, &task, 240).await;
    let evs = task_events(&core, &session, &task).await;
    assert_eq!(
        (st.state.as_str(), st.run_state.as_str()),
        ("ReadyForReview", "Completed"),
        "{st:?}\n{evs:#?}"
    );
    // The acceptance test was never weakened; the fix landed.
    assert_eq!(
        std::fs::read_to_string(repo.path().join("tests/quantities.rs")).unwrap(),
        tests_src
    );
    assert_eq!(
        std::fs::read_to_string(repo.path().join("src/lib.rs")).unwrap(),
        fixed_lib
    );
    let of = |t: &str| {
        evs.iter()
            .filter(|(_, x, _)| x == t)
            .map(|(_, _, p)| p.clone())
            .collect::<Vec<_>>()
    };
    // BASELINE before the first write: KNOWN_FAILING labelled, flaky quarantined.
    let baseline = of("VerificationBaselineRecorded");
    assert_eq!(baseline.len(), 1, "{evs:#?}");
    let base_checks = baseline[0]["checks"].as_array().unwrap();
    let base_status = |sym: &str| {
        base_checks
            .iter()
            .find(|c| {
                c["check_id"]
                    .as_str()
                    .unwrap()
                    .ends_with(&format!("::{sym}"))
            })
            .map(|c| c["status"].as_str().unwrap().to_owned())
    };
    assert_eq!(
        base_status("acceptance_rejects_negative_quantity").as_deref(),
        Some("FAIL")
    );
    assert_eq!(
        base_status("preexisting_failing_unrelated").as_deref(),
        Some("FAIL")
    );
    assert_eq!(base_status("formats_totals").as_deref(), Some("PASS"));
    assert_eq!(
        base_status("flaky_first_run_fails").as_deref(),
        Some("FLAKY"),
        "baseline checks {base_checks:?}; quarantines {:?}; runs {:?}; baseline {:?}",
        of("FlakyCheckQuarantined"),
        of("VerificationRunRecorded")
            .iter()
            .map(|r| r["stage"].clone())
            .collect::<Vec<_>>(),
        baseline[0]["verification_run_id"]
    );
    assert_eq!(of("FlakyCheckQuarantined").len(), 1);
    let first_write_offset = evs
        .iter()
        .position(|(a, t, p)| {
            a == "tool_call" && t == "ToolCallProposed" && p["tool_name"] == "change.apply"
        })
        .unwrap();
    let baseline_offset = evs
        .iter()
        .position(|(_, t, _)| t == "VerificationBaselineRecorded")
        .unwrap();
    assert!(
        baseline_offset < first_write_offset,
        "baseline precedes the first write"
    );
    // DI-3: the weakening write was denied before any effect.
    let di = of("DiffInvariantViolated");
    assert!(
        di.iter().any(|v| v["invariant"] == "DI-3"
            && v["class"] == "DENY"
            && v["stage"] == "TRANSACTION"),
        "{di:?}"
    );
    let steps: Vec<serde_json::Value> = evs
        .iter()
        .filter(|(a, t, _)| a == "run_step" && t == "StepFailed")
        .map(|(_, _, p)| p.clone())
        .collect();
    assert!(
        steps
            .iter()
            .any(|p| p["failure_code"] == "DIFF_INVARIANT_DENY"),
        "{steps:?}"
    );
    assert_eq!(
        evs.iter()
            .filter(|(a, t, p)| a == "tool_call"
                && t == "ToolCallProposed"
                && p["tool_name"] == "change.apply")
            .count(),
        2,
        "only the two lib.rs writes reached the effector"
    );
    // TARGETED run recorded; COMPLETION twice: first blocked by a REGRESSION, then clean.
    let runs = of("VerificationRunRecorded");
    let stages: Vec<&str> = runs.iter().map(|r| r["stage"].as_str().unwrap()).collect();
    assert_eq!(
        stages,
        ["TARGETED", "COMPLETION", "COMPLETION"],
        "{stages:?}"
    );
    let attributed = of("RegressionAttributed");
    assert!(
        attributed.iter().any(|a| a["attribution"] == "REGRESSION"
            && a["check_id"]
                .as_str()
                .unwrap()
                .ends_with("::formats_totals")),
        "{attributed:?}"
    );
    assert!(attributed.iter().any(|a| {
        a["attribution"] == "KNOWN_FAILING"
            && a["check_id"]
                .as_str()
                .unwrap()
                .ends_with("::preexisting_failing_unrelated")
    }));
    assert!(attributed.iter().any(|a| {
        a["attribution"] == "COLLATERAL_FIX"
            && a["check_id"]
                .as_str()
                .unwrap()
                .ends_with("::acceptance_rejects_negative_quantity")
    }));
    let last_completion = runs.last().unwrap();
    assert_eq!(
        last_completion["status"], "FAILED",
        "the pre-existing failure keeps the suite FAILED without blocking acceptance"
    );
    let completion_attrs: Vec<&serde_json::Value> = attributed
        .iter()
        .filter(|a| a["verification_run_id"] == last_completion["verification_run_id"])
        .collect();
    assert!(
        completion_attrs
            .iter()
            .all(|a| a["attribution"] != "REGRESSION"),
        "{completion_attrs:?}"
    );
    let reviews = of("SelfReviewRecorded");
    assert_eq!(reviews.len(), 2);
    // The model saw the failing CheckResults first with the raw refs (REQ-EV-0107).
    let bodies = seen.lock().unwrap().clone();
    let verify_obs = bodies
        .iter()
        .flat_map(|b| b["messages"].as_array().unwrap().clone())
        .filter(|m| m["role"] == "tool")
        .map(|m| m["content"].as_str().unwrap_or_default().to_owned())
        .find(|t| t.contains("stage: Targeted"))
        .unwrap();
    assert!(
        verify_obs.contains("cargo:tests/quantities.rs::formats_totals [Fail]")
            && verify_obs.contains("raw_output_ref="),
        "{verify_obs}"
    );
    let refusal = bodies
        .iter()
        .flat_map(|b| b["messages"].as_array().unwrap().clone())
        .filter(|m| m["role"] == "tool")
        .map(|m| m["content"].as_str().unwrap_or_default().to_owned())
        .find(|t| t.contains("COMPLETION_REFUSED"))
        .unwrap();
    assert!(refusal.contains("REGRESSION"), "{refusal}");
    // Projection tables carry the runs and quarantines (docs/31).
    assert!(
        evs.iter()
            .any(|(a, t, _)| a == "run_step" && t == "StepScheduled")
    );
    // M3.6: the verification checks of the task's runs are attributed to the
    // test file whose symbols they name (runtime evidence in the graph).
    let r = invoke_tool(
        &mut c,
        &task,
        g,
        0xA7,
        0xA8,
        "search.graph",
        r#"{"path":"tests/quantities.rs","relation":"evidence"}"#,
    )
    .await;
    assert_eq!(r.status, "SUCCESS", "{r:?}");
    let so: serde_json::Value = serde_json::from_str(&r.structured_output_json).unwrap();
    let ev = so["graph"]["evidence"].as_array().unwrap();
    assert!(
        ev.iter()
            .any(|e| e[0].as_str().unwrap().ends_with("::formats_totals")),
        "{so}"
    );
    let _ = repo;
}

/// M2.9: the Trusted Code Review Surface. The Core serves the revision-bound
/// candidate as hunks with evidence; rejecting one hunk and accepting the
/// rest rebuilds the file through the Workspace File Service and commits;
/// the Git diff matches the user's choices exactly (REQ-EV-0036); a stale
/// review is refused; RETURN sends the task back with the note queued.
#[tokio::test]
async fn m2_9_review_surface_applies_per_hunk_decisions_and_commits() {
    use modbit_protocol::v1::{
        CodeViewModel, DecideReview, GetCodeView, GetReviewBundle, HunkRef, ReviewBundle,
        ReviewDecided, StartTask, TaskRunStarted,
    };
    use serde_json::json;
    // A repo with two separated regions so the candidate has two hunks in one file.
    let repo = tempfile::tempdir().unwrap();
    let original = format!(
        "{}\n{}\n",
        (1..=8)
            .map(|i| format!("line {i}"))
            .collect::<Vec<_>>()
            .join("\n"),
        (9..=16)
            .map(|i| format!("line {i}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
    std::fs::write(repo.path().join("notes.txt"), &original).unwrap();
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
    // Candidate: change line 2 and line 15 (two hunks), add a new file.
    let candidate = original
        .replace("line 2\n", "line 2 changed\n")
        .replace("line 15\n", "line 15 changed\n");
    let script = vec![
        json!({"calls": [{"name": "plan.update", "args": {"outcome": "edit notes", "expected_files": ["notes.txt", "extra.txt"]}}]}),
        json!({"calls": [{"name": "fs.read", "args": {"path": "notes.txt"}}]}),
        json!({"calls": [{"name": "change.apply", "args": {"path": "notes.txt", "op": "replace", "content": candidate}}]}),
        json!({"calls": [{"name": "change.apply", "args": {"path": "extra.txt", "op": "create", "content": "brand new\n"}}]}),
        json!({"calls": [{"name": "task.complete", "args": {"summary": "edited", "self_review": {"findings": []}}}]}),
        // After a RETURN the resumed conversation carries the earlier results plus the feedback.
        json!({"calls": [{"name": "task.complete", "args": {"summary": "after feedback", "self_review": {"findings": []}}}]}),
    ];
    let (base, _seen) = scripted_model(script, None).await;
    let dir = tempfile::tempdir().unwrap();
    let core = CoreProcess::spawn_with_env(
        dir.path(),
        &[
            ("MODBIT_OPENAI_BASE_URL", &base),
            ("OPENAI_API_KEY", ""),
            ("ANTHROPIC_API_KEY", ""),
        ],
    );
    let mut c = core.client().await;
    let (session, _) = create_session(&mut c, id16(0xB0)).await;
    let g = lease_for(&session);
    let ack = c
        .command(envelope_fenced(
            id16(0xB1),
            "CreateTask",
            CreateTask {
                session_id: Some(session.clone()),
                goal_text: "Edit the notes".into(),
                workspace_id: None,
                execution_profile: String::new(),
                origin: "cli".into(),
                workspace_root: root.clone(),
            }
            .encode_to_vec(),
            g,
        ))
        .await
        .unwrap();
    let task = Client::result::<TaskCreated>(&ack)
        .unwrap()
        .task_id
        .unwrap();
    // Review before ReadyForReview is refused; the bundle is still readable.
    let err = c
        .command(envelope_fenced(
            id16(0xB2),
            "DecideReview",
            DecideReview {
                task_id: Some(task.clone()),
                decision: "ACCEPT".into(),
                rejected: vec![],
                note: String::new(),
                expected_workspace_revision: 0,
            }
            .encode_to_vec(),
            g,
        ))
        .await
        .unwrap_err();
    assert!(
        matches!(err, ClientError::Rejected { ref code, .. } if code == "NOT_REVIEWABLE"),
        "{err:?}"
    );
    let ack = c
        .command(envelope_fenced(
            id16(0xB3),
            "StartTask",
            StartTask {
                task_id: Some(task.clone()),
                endpoint: String::new(),
                model: "gpt-5-mini".into(),
                max_turns: 10,
                max_tool_calls: 0,
                max_no_progress_turns: 3,
            }
            .encode_to_vec(),
            g,
        ))
        .await
        .unwrap();
    let _: TaskRunStarted = Client::result(&ack).unwrap();
    let st = wait_task(&mut c, &task, 60).await;
    assert_eq!(st.state, "ReadyForReview", "{st:?}");
    // The bundle: two hunks in notes.txt, one new file, plan and self-review evidence.
    let ack = c
        .command(envelope(
            id16(0xB4),
            "GetReviewBundle",
            GetReviewBundle {
                task_id: Some(task.clone()),
            }
            .encode_to_vec(),
        ))
        .await
        .unwrap();
    let b: ReviewBundle = Client::result(&ack).unwrap();
    assert_eq!(b.task_state, "ReadyForReview");
    assert_eq!(b.base_commit.len(), 40);
    let notes = b
        .files
        .iter()
        .find(|f| f.path == "notes.txt")
        .expect("notes in candidate");
    assert_eq!(
        (notes.status.as_str(), notes.hunks.len()),
        ("M", 2),
        "{b:?}"
    );
    assert!(
        notes.hunks[0].lines.iter().any(|l| l == "+line 2 changed")
            && notes.hunks[1].lines.iter().any(|l| l == "+line 15 changed")
    );
    let extra = b
        .files
        .iter()
        .find(|f| f.path == "extra.txt")
        .expect("new file in candidate");
    assert_eq!((extra.status.as_str(), extra.hunks.len()), ("A", 1));
    assert!(b.plan_json.contains("edit notes") && b.self_review_json.contains("findings"));
    assert!(
        b.verification_runs.iter().any(|v| v.stage == "BASELINE")
            && b.verification_runs.iter().any(|v| v.stage == "COMPLETION"),
        "{:?}",
        b.verification_runs
    );
    assert!(
        b.evidence_links.iter().any(|e| e.starts_with("plan:"))
            && b.evidence_links
                .iter()
                .any(|e| e.starts_with("tool_result:"))
    );
    // Code view: revision-bound, changed ranges, stale detection.
    let ack = c
        .command(envelope(
            id16(0xB5),
            "GetCodeView",
            GetCodeView {
                task_id: Some(task.clone()),
                path: "notes.txt".into(),
                expected_file_revision: String::new(),
            }
            .encode_to_vec(),
        ))
        .await
        .unwrap();
    let cv: CodeViewModel = Client::result(&ack).unwrap();
    assert_eq!(
        (
            cv.syntax_language.as_str(),
            cv.file_revision.as_str(),
            cv.changed_ranges.as_slice()
        ),
        ("text", notes.file_revision.as_str(), &[2u32, 2, 15, 15][..]),
        "{cv:?}"
    );
    assert!(cv.text.contains("line 15 changed") && !cv.stale);
    let ack = c
        .command(envelope(
            id16(0xB6),
            "GetCodeView",
            GetCodeView {
                task_id: Some(task.clone()),
                path: "notes.txt".into(),
                expected_file_revision: "0000".into(),
            }
            .encode_to_vec(),
        ))
        .await
        .unwrap();
    let cv2: CodeViewModel = Client::result(&ack).unwrap();
    assert!(
        cv2.stale,
        "a CodeReference with another file revision is marked stale"
    );
    // Stale review is refused; an unknown hunk is refused.
    let err = c
        .command(envelope_fenced(
            id16(0xB7),
            "DecideReview",
            DecideReview {
                task_id: Some(task.clone()),
                decision: "ACCEPT".into(),
                rejected: vec![],
                note: String::new(),
                expected_workspace_revision: b.workspace_revision + 7,
            }
            .encode_to_vec(),
            g,
        ))
        .await
        .unwrap_err();
    assert!(
        matches!(err, ClientError::Rejected { ref code, .. } if code == "STALE_REVIEW"),
        "{err:?}"
    );
    let err = c
        .command(envelope_fenced(
            id16(0xB8),
            "DecideReview",
            DecideReview {
                task_id: Some(task.clone()),
                decision: "ACCEPT".into(),
                rejected: vec![HunkRef {
                    path: "notes.txt".into(),
                    index: 9,
                }],
                note: String::new(),
                expected_workspace_revision: b.workspace_revision,
            }
            .encode_to_vec(),
            g,
        ))
        .await
        .unwrap_err();
    assert!(
        matches!(err, ClientError::Rejected { ref code, .. } if code == "UNKNOWN_HUNK"),
        "{err:?}"
    );
    // Reject the second hunk, accept the rest: the file keeps line 15, the new file lands, a commit exists.
    let ack = c
        .command(envelope_fenced(
            id16(0xB9),
            "DecideReview",
            DecideReview {
                task_id: Some(task.clone()),
                decision: "ACCEPT".into(),
                rejected: vec![HunkRef {
                    path: "notes.txt".into(),
                    index: 1,
                }],
                note: "Keep the second region as it was".into(),
                expected_workspace_revision: b.workspace_revision,
            }
            .encode_to_vec(),
            g,
        ))
        .await
        .unwrap();
    let d: ReviewDecided = Client::result(&ack).unwrap();
    assert_eq!(
        (d.task_state.as_str(), d.commit.len(), d.reverted.as_slice()),
        ("Completed", 40, &["notes.txt#1".to_owned()][..]),
        "{d:?}"
    );
    let expected = original.replace("line 2\n", "line 2 changed\n");
    assert_eq!(
        std::fs::read_to_string(repo.path().join("notes.txt")).unwrap(),
        expected
    );
    assert_eq!(
        std::fs::read_to_string(repo.path().join("extra.txt")).unwrap(),
        "brand new\n"
    );
    let diff = Command::new("git")
        .arg("-C")
        .arg(repo.path())
        .args(["diff", "HEAD~1", "HEAD", "--stat"])
        .output()
        .unwrap();
    let stat = String::from_utf8_lossy(&diff.stdout);
    assert!(
        stat.contains("notes.txt")
            && stat.contains("extra.txt")
            && stat.contains("2 files changed"),
        "{stat}"
    );
    let clean = Command::new("git")
        .arg("-C")
        .arg(repo.path())
        .args(["status", "--porcelain"])
        .output()
        .unwrap();
    assert!(
        String::from_utf8_lossy(&clean.stdout)
            .lines()
            .all(|l| l.contains(".modbit")),
        "worktree is clean after the review commit: {}",
        String::from_utf8_lossy(&clean.stdout)
    );
    let msg = Command::new("git")
        .arg("-C")
        .arg(repo.path())
        .args(["log", "-1", "--format=%B"])
        .output()
        .unwrap();
    let msg = String::from_utf8_lossy(&msg.stdout);
    assert!(
        msg.contains("Keep the second region") && msg.contains("1 rejected"),
        "{msg}"
    );
    let st = wait_task(&mut c, &task, 5).await;
    assert_eq!(st.state, "Completed");
    let evs = task_events(&core, &session, &task).await;
    let rd = evs
        .iter()
        .find(|(_, t, _)| t == "ReviewDecisionRecorded")
        .map(|(_, _, p)| p.clone())
        .unwrap();
    assert_eq!(
        (
            rd["decision"].as_str(),
            rd["provenance"].as_str(),
            rd["rejected"][0].as_str()
        ),
        (Some("ACCEPT"), Some("user_review"), Some("notes.txt#1"))
    );
    assert!(evs.iter().any(|(_, t, _)| t == "TaskCompleted"));

    // RETURN: a second task goes back to work with the note queued, and StartTask resumes it as a new attempt.
    let (base2, _) = scripted_model(vec![
        json!({"calls": [{"name": "plan.update", "args": {"outcome": "o", "expected_files": ["extra.txt"]}}]}),
        json!({"calls": [{"name": "change.apply", "args": {"path": "extra.txt", "op": "replace", "content": "second edit\n"}}]}),
        json!({"calls": [{"name": "task.complete", "args": {"summary": "s", "self_review": {"findings": []}}}]}),
    ], None).await;
    let _ = base2;
    let ack = c
        .command(envelope_fenced(
            id16(0xBA),
            "CreateTask",
            CreateTask {
                session_id: Some(session.clone()),
                goal_text: "Second".into(),
                workspace_id: None,
                execution_profile: String::new(),
                origin: "cli".into(),
                workspace_root: root.clone(),
            }
            .encode_to_vec(),
            g,
        ))
        .await
        .unwrap();
    let task2 = Client::result::<TaskCreated>(&ack)
        .unwrap()
        .task_id
        .unwrap();
    let ack = c
        .command(envelope_fenced(
            id16(0xBB),
            "StartTask",
            StartTask {
                task_id: Some(task2.clone()),
                endpoint: String::new(),
                model: "gpt-5-mini".into(),
                max_turns: 10,
                max_tool_calls: 0,
                max_no_progress_turns: 3,
            }
            .encode_to_vec(),
            g,
        ))
        .await
        .unwrap();
    let _: TaskRunStarted = Client::result(&ack).unwrap();
    let st = wait_task(&mut c, &task2, 60).await;
    assert_eq!(st.state, "ReadyForReview", "{st:?}");
    let ack = c
        .command(envelope_fenced(
            id16(0xBC),
            "DecideReview",
            DecideReview {
                task_id: Some(task2.clone()),
                decision: "RETURN".into(),
                rejected: vec![],
                note: "Please also update notes.txt".into(),
                expected_workspace_revision: 0,
            }
            .encode_to_vec(),
            g,
        ))
        .await
        .unwrap();
    let d: ReviewDecided = Client::result(&ack).unwrap();
    assert_eq!((d.task_state.as_str(), d.commit.as_str()), ("Waiting", ""));
    let st = wait_task(&mut c, &task2, 5).await;
    assert_eq!(
        (st.state.as_str(), st.wait_reason.as_str()),
        ("Waiting", "UserInput")
    );
    let ack = c
        .command(envelope_fenced(
            id16(0xBD),
            "StartTask",
            StartTask {
                task_id: Some(task2.clone()),
                endpoint: String::new(),
                model: "gpt-5-mini".into(),
                max_turns: 10,
                max_tool_calls: 0,
                max_no_progress_turns: 3,
            }
            .encode_to_vec(),
            g,
        ))
        .await
        .unwrap();
    let started: TaskRunStarted = Client::result(&ack).unwrap();
    assert!(started.resumed);
    let st = wait_task(&mut c, &task2, 60).await;
    assert_eq!(
        st.state, "ReadyForReview",
        "resumed attempt reaches review again: {st:?}"
    );
    let evs2 = task_events(&core, &session, &task2).await;
    assert_eq!(
        evs2.iter()
            .filter(|(a, t, _)| a == "run" && t == "RunCreated")
            .count(),
        2,
        "a fresh attempt after RETURN"
    );
    assert!(evs2.iter().any(|(_, t, p)| {
        t == "TaskInputQueued"
            && p["text"]
                .as_str()
                .unwrap_or_default()
                .contains("Please also update")
    }));
    assert!(evs2.iter().any(|(_, t, _)| t == "TaskReturnedToWork"));
}

/// M2.10: media read through the production `fs.read` path lands on the log
/// as digests only (no bytes in event rows), the original and egress copies
/// are retrievable by digest, and they survive a Core restart (MEDIA-E2E-012).
#[tokio::test]
async fn m2_10_media_reads_carry_digests_not_bytes_and_survive_restart() {
    use modbit_protocol::v1::{InvokeTool, ObjectRangeChunk, ReadObjectRange, ToolInvoked};
    use sha2::Digest;
    let fixtures =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/media");
    let repo = tempfile::tempdir().unwrap();
    for f in ["label.png", "report.pdf", "bomb.png"] {
        std::fs::copy(fixtures.join(f), repo.path().join(f)).unwrap();
    }
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
            "media",
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
    let dir = tempfile::tempdir().unwrap();
    let mut core = CoreProcess::spawn(dir.path());
    let mut c = core.client().await;
    let (session, _) = create_session(&mut c, id16(0xC0)).await;
    let g = lease_for(&session);
    let ack = c
        .command(envelope_fenced(
            id16(0xC1),
            "CreateTask",
            CreateTask {
                session_id: Some(session.clone()),
                goal_text: "look at media".into(),
                workspace_id: None,
                execution_profile: String::new(),
                origin: "cli".into(),
                workspace_root: root,
            }
            .encode_to_vec(),
            g,
        ))
        .await
        .unwrap();
    let task = Client::result::<TaskCreated>(&ack)
        .unwrap()
        .task_id
        .unwrap();
    async fn read(
        c: &mut Client,
        cmd: u8,
        call: u8,
        task: &Id,
        g: Option<u64>,
        args: &str,
    ) -> ToolInvoked {
        let ack = c
            .command(envelope_fenced(
                id16(cmd),
                "InvokeTool",
                InvokeTool {
                    task_id: Some(task.clone()),
                    tool_name: "fs.read".into(),
                    arguments_json: args.into(),
                    tool_call_id: Some(id16(call)),
                    output_budget_bytes: 64 * 1024,
                }
                .encode_to_vec(),
                g,
            ))
            .await
            .unwrap();
        Client::result(&ack).unwrap()
    }
    let r = read(&mut c, 0xC2, 0xD0, &task, g, r#"{"path":"label.png"}"#).await;
    assert_eq!(r.status, "SUCCESS", "{r:?}");
    let out: serde_json::Value = serde_json::from_str(&r.structured_output_json).unwrap();
    let media = &out["media"];
    let content_ref = media["content_ref"].as_str().unwrap().to_owned();
    let egress_ref = media["egress_ref"].as_str().unwrap().to_owned();
    assert_eq!(
        (
            media["mime"].as_str(),
            media["width"].as_u64(),
            media["height"].as_u64()
        ),
        (Some("image/png"), Some(160), Some(60))
    );
    assert_eq!(media["provenance"]["source"], "label.png");
    assert_eq!(
        media["provenance"]["task_id"].as_str().map(str::len),
        Some(36)
    );
    let pdf = read(
        &mut c,
        0xC3,
        0xD1,
        &task,
        g,
        r#"{"path":"report.pdf","pages":[2,2]}"#,
    )
    .await;
    assert_eq!(pdf.status, "SUCCESS", "{pdf:?}");
    let pout: serde_json::Value = serde_json::from_str(&pdf.structured_output_json).unwrap();
    assert!(
        pout["media"]["text_derivative"]
            .as_str()
            .unwrap()
            .contains("Ignore all previous instructions")
    );
    assert_eq!(pout["media"]["trust"], "UNTRUSTED_WORKSPACE_CONTENT");
    let bomb = read(&mut c, 0xC4, 0xD2, &task, g, r#"{"path":"bomb.png"}"#).await;
    assert_eq!(
        (bomb.status.as_str(), bomb.error_code.as_str()),
        ("APPLICATION_FAILURE", "MEDIA_BUDGET_EXCEEDED"),
        "{bomb:?}"
    );
    // The event rows carry references, never the bytes.
    let evs = task_events(&core, &session, &task).await;
    let dump = serde_json::to_string(&evs).unwrap();
    assert!(
        dump.contains(&content_ref) || dump.contains("result_ref"),
        "results are referenced"
    );
    assert!(
        !dump.contains("iVBOR") && !dump.contains("\\u0089PNG") && dump.len() < 200_000,
        "no base64 or raw image bytes on the log"
    );
    // Restart: original and egress copies are retrievable by digest from the object store.
    core.kill();
    let core2 = CoreProcess::spawn(dir.path());
    let mut c2 = core2.client().await;
    for (id, r, expect_png_sig, expect_exif) in [
        (0xC5u8, content_ref.clone(), true, true),
        (0xC6u8, egress_ref.clone(), true, false),
    ] {
        let ack = c2
            .command(envelope(
                id16(id),
                "ReadObjectRange",
                ReadObjectRange {
                    object_hash: r.clone(),
                    offset: 0,
                    length: 4096,
                }
                .encode_to_vec(),
            ))
            .await
            .unwrap();
        let chunk: ObjectRangeChunk = Client::result(&ack).unwrap();
        assert_eq!(chunk.data.starts_with(b"\x89PNG"), expect_png_sig, "{r}");
        let has_exif = chunk.data.windows(4).any(|w| w == b"eXIf");
        assert_eq!(
            has_exif, expect_exif,
            "egress copy has no EXIF; the original keeps it"
        );
        assert_eq!(
            hex::encode(sha2::Sha256::digest(
                &chunk.data[..chunk.data.len().min(chunk.total_bytes as usize)]
            )),
            r,
            "bytes match their digest"
        );
    }
}

/// QUAL-EV-0194: one canonical approval owns the decision; a client resolves
/// it under the session lease, a second conflicting resolution replays the
/// recorded outcome, and no model-facing tool can resolve approvals.
#[tokio::test]
async fn qual_ev_0194_approvals_are_canonical_and_never_resolved_by_the_model() {
    use modbit_protocol::v1::{
        ApprovalResolvedAck, InvokeTool, ListTools, ResolveApproval, ToolInvoked, ToolList,
    };
    let repo = tempfile::tempdir().unwrap();
    std::fs::write(repo.path().join("a.txt"), "a\n").unwrap();
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
    let dir = tempfile::tempdir().unwrap();
    let core = CoreProcess::spawn(dir.path());
    let mut c = core.client().await;
    let (session, _) = create_session(&mut c, id16(0xE0)).await;
    let g = lease_for(&session);
    let ack = c
        .command(envelope_fenced(
            id16(0xE1),
            "CreateTask",
            CreateTask {
                session_id: Some(session.clone()),
                goal_text: "x".into(),
                workspace_id: None,
                execution_profile: String::new(),
                origin: "cli".into(),
                workspace_root: root.clone(),
            }
            .encode_to_vec(),
            g,
        ))
        .await
        .unwrap();
    let task = Client::result::<TaskCreated>(&ack)
        .unwrap()
        .task_id
        .unwrap();
    // The model-facing registry has no approval or policy tool.
    let ack = c
        .command(envelope(
            id16(0xE2),
            "ListTools",
            ListTools { task_id: None }.encode_to_vec(),
        ))
        .await
        .unwrap();
    let tools: ToolList = Client::result(&ack).unwrap();
    assert!(
        tools
            .tools
            .iter()
            .all(|t| !t.name.contains("approval") && !t.name.contains("policy")),
        "{:?}",
        tools.tools.iter().map(|t| &t.name).collect::<Vec<_>>()
    );
    let wt = format!("{}-wt", root.replace('\\', "/"));
    let r = c
        .command(envelope_fenced(
            id16(0xE3),
            "InvokeTool",
            InvokeTool {
                task_id: Some(task.clone()),
                tool_name: "git.worktree.create".into(),
                arguments_json: format!(r#"{{"branch":"t/x","path":"{wt}"}}"#),
                tool_call_id: Some(id16(0xF1)),
                output_budget_bytes: 4096,
            }
            .encode_to_vec(),
            g,
        ))
        .await
        .unwrap();
    assert_eq!(Client::result::<ToolInvoked>(&r).unwrap().status, "SUCCESS");
    let r = c
        .command(envelope_fenced(
            id16(0xE4),
            "InvokeTool",
            InvokeTool {
                task_id: Some(task.clone()),
                tool_name: "git.worktree.close".into(),
                arguments_json: format!(r#"{{"path":"{wt}"}}"#),
                tool_call_id: Some(id16(0xF2)),
                output_budget_bytes: 4096,
            }
            .encode_to_vec(),
            g,
        ))
        .await
        .unwrap();
    let pending: ToolInvoked = Client::result(&r).unwrap();
    assert_eq!(pending.status, "APPROVAL_PENDING");
    let approval = Id {
        value: decode_hex(&pending.approval_id).unwrap(),
    };
    // Two clients race: the first decision (deny) is canonical; the second (approve) replays DENIED.
    let mut other = core.client().await;
    let ack = c
        .command(envelope_fenced(
            id16(0xE5),
            "ResolveApproval",
            ResolveApproval {
                approval_id: Some(approval.clone()),
                approve: false,
                reason: "no".into(),
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
    let g2 = Some(acquire_lease(&mut other, id16(0xE6), session.clone(), "second-client").await);
    let ack = other
        .command(envelope_fenced(
            id16(0xE7),
            "ResolveApproval",
            ResolveApproval {
                approval_id: Some(approval.clone()),
                approve: true,
                reason: "yes".into(),
            }
            .encode_to_vec(),
            g2,
        ))
        .await
        .unwrap();
    assert_eq!(
        (
            ack.status,
            Client::result::<ApprovalResolvedAck>(&ack)
                .unwrap()
                .status
                .as_str()
        ),
        (CommandStatus::Replayed as i32, "DENIED")
    );
    assert!(
        std::path::Path::new(&wt).exists(),
        "the denied effect never happened"
    );
    let evs = task_events(&core, &session, &task).await;
    assert_eq!(
        evs.iter()
            .filter(|(_, t, _)| t == "ApprovalResolved")
            .count(),
        1,
        "one canonical resolution on the log"
    );
}

fn plain_repo(files: &[(&str, &str)]) -> (tempfile::TempDir, String) {
    let repo = tempfile::tempdir().unwrap();
    for (p, c) in files {
        let path = repo.path().join(p);
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).unwrap();
        }
        std::fs::write(path, c).unwrap();
    }
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
    (repo, root)
}

async fn invoke_tool(
    c: &mut Client,
    task: &Id,
    g: Option<u64>,
    cmd: u8,
    call: u8,
    tool: &str,
    args: &str,
) -> modbit_protocol::v1::ToolInvoked {
    let ack = c
        .command(envelope_fenced(
            id16(cmd),
            "InvokeTool",
            modbit_protocol::v1::InvokeTool {
                task_id: Some(task.clone()),
                tool_name: tool.into(),
                arguments_json: args.into(),
                tool_call_id: Some(id16(call)),
                output_budget_bytes: 65536,
            }
            .encode_to_vec(),
            g,
        ))
        .await
        .unwrap();
    Client::result(&ack).unwrap()
}

async fn read_object_bytes(c: &mut Client, id: Id, hash: &str) -> Vec<u8> {
    use modbit_protocol::v1::{ObjectRangeChunk, ReadObjectRange};
    let ack = c
        .command(envelope(
            id,
            "ReadObjectRange",
            ReadObjectRange {
                object_hash: hash.to_owned(),
                offset: 0,
                length: 1024 * 1024,
            }
            .encode_to_vec(),
        ))
        .await
        .unwrap();
    let chunk: ObjectRangeChunk = Client::result(&ack).unwrap();
    chunk.data
}

async fn read_object(c: &mut Client, id: Id, hash: &str) -> String {
    use modbit_protocol::v1::{ObjectRangeChunk, ReadObjectRange};
    let ack = c
        .command(envelope(
            id,
            "ReadObjectRange",
            ReadObjectRange {
                object_hash: hash.to_owned(),
                offset: 0,
                length: 1024 * 1024,
            }
            .encode_to_vec(),
        ))
        .await
        .unwrap();
    let chunk: ObjectRangeChunk = Client::result(&ack).unwrap();
    String::from_utf8(chunk.data).unwrap()
}

/// QUAL-EV-0106: every write lands a revision-bound `FileChanged` event with
/// before/after content and a unified diff by reference; the review surface
/// shows the identical content refs and hunk lines.
#[tokio::test]
async fn qual_ev_0106_every_write_lands_a_revision_bound_file_changed_event_matching_the_review_bundle()
 {
    use modbit_protocol::v1::{GetReviewBundle, ReviewBundle};
    let (_repo, root) = plain_repo(&[("a.txt", "one\ntwo\nthree\n"), ("gone.txt", "bye\n")]);
    let dir = tempfile::tempdir().unwrap();
    let core = CoreProcess::spawn(dir.path());
    let mut c = core.client().await;
    let (session, _) = create_session(&mut c, id16(0xD0)).await;
    let g = lease_for(&session);
    let ack = c
        .command(envelope_fenced(
            id16(0xD1),
            "CreateTask",
            CreateTask {
                session_id: Some(session.clone()),
                goal_text: "x".into(),
                workspace_id: None,
                execution_profile: String::new(),
                origin: "cli".into(),
                workspace_root: root.clone(),
            }
            .encode_to_vec(),
            g,
        ))
        .await
        .unwrap();
    let task = Client::result::<TaskCreated>(&ack)
        .unwrap()
        .task_id
        .unwrap();
    let r = invoke_tool(
        &mut c,
        &task,
        g,
        0xD2,
        0xF1,
        "change.apply",
        r#"{"path":"a.txt","op":"edit","text_edits":[{"old":"two","new":"2"}]}"#,
    )
    .await;
    assert_eq!(r.status, "SUCCESS", "{r:?}");
    let r = invoke_tool(
        &mut c,
        &task,
        g,
        0xD3,
        0xF2,
        "change.apply",
        r#"{"path":"gone.txt","op":"delete"}"#,
    )
    .await;
    assert_eq!(r.status, "SUCCESS", "{r:?}");
    let evs = task_events(&core, &session, &task).await;
    let changed: Vec<_> = evs
        .iter()
        .filter(|(a, t, _)| a == "workspace" && t == "FileChanged")
        .map(|(_, _, p)| p.clone())
        .collect();
    assert_eq!(changed.len(), 2, "{evs:#?}");
    let e = &changed[0];
    assert_eq!(
        (e["path"].as_str(), e["op"].as_str()),
        (Some("a.txt"), Some("edit"))
    );
    assert!(
        e["before_hash"].is_string()
            && e["after_hash"].is_string()
            && e["before_ref"].is_string()
            && e["after_ref"].is_string()
    );
    assert_eq!(
        e["workspace_revision"].as_u64().unwrap(),
        e["previous_revision"].as_u64().unwrap() + 1
    );
    assert_eq!(
        e["tool_call_id"]
            .as_str()
            .map(|v| v.replace('-', "").to_lowercase()),
        Some(hex::encode([0xF1u8; 16]))
    );
    let diff = read_object(&mut c, id16(0xD4), e["diff_ref"].as_str().unwrap()).await;
    assert!(diff.starts_with("--- a/a.txt\n+++ b/a.txt\n@@"), "{diff}");
    assert!(diff.contains("-two\n+2\n"), "{diff}");
    let after = read_object(&mut c, id16(0xD5), e["after_ref"].as_str().unwrap()).await;
    assert_eq!(after, "one\n2\nthree\n");
    let d = &changed[1];
    assert!(
        d["after_hash"].is_null() && d["after_ref"].is_null() && d["before_ref"].is_string(),
        "{d}"
    );
    assert!(
        read_object(&mut c, id16(0xD6), d["diff_ref"].as_str().unwrap())
            .await
            .contains("-bye\n")
    );
    // The review surface shows the very same content refs and hunk lines.
    let ack = c
        .command(envelope(
            id16(0xD7),
            "GetReviewBundle",
            GetReviewBundle {
                task_id: Some(task.clone()),
            }
            .encode_to_vec(),
        ))
        .await
        .unwrap();
    let b: ReviewBundle = Client::result(&ack).unwrap();
    let f = b
        .files
        .iter()
        .find(|f| f.path == "a.txt")
        .unwrap_or_else(|| panic!("{b:?}"));
    assert_eq!(
        (f.old_content_ref.as_str(), f.new_content_ref.as_str()),
        (
            e["before_ref"].as_str().unwrap(),
            e["after_ref"].as_str().unwrap()
        )
    );
    assert_eq!(f.file_revision, e["after_hash"].as_str().unwrap());
    let evidence_lines: Vec<&str> = diff
        .lines()
        .skip_while(|l| !l.starts_with("@@"))
        .skip(1)
        .collect();
    assert_eq!(f.hunks.len(), 1);
    assert_eq!(
        f.hunks[0].lines, evidence_lines,
        "review hunk equals the evidence diff"
    );
    let gone = b.files.iter().find(|f| f.path == "gone.txt").unwrap();
    assert_eq!(
        (gone.status.as_str(), gone.old_content_ref.as_str()),
        ("D", d["before_ref"].as_str().unwrap())
    );
}

/// QUAL-EV-0064/0065: undo is a typed plan of inverse actions (delete the
/// created, restore the deleted, replace the modified) applied only while
/// every path still carries its post-edit content; unrelated user edits are
/// untouched and a user edit on a changed path refuses the revert.
#[tokio::test]
async fn qual_ev_0064_0065_typed_undo_restores_inverse_actions_and_a_user_edit_blocks_the_revert() {
    use modbit_protocol::v1::{InvokeTool, ToolInvoked, UndoPlanView, UndoToolCall};
    let (repo, root) = plain_repo(&[("b.txt", "b\n"), ("c.txt", "c\n"), ("user.txt", "u\n")]);
    let dir = tempfile::tempdir().unwrap();
    let core = CoreProcess::spawn(dir.path());
    let mut c = core.client().await;
    let (session, _) = create_session(&mut c, id16(0xE0)).await;
    let g = lease_for(&session);
    let ack = c
        .command(envelope_fenced(
            id16(0xE1),
            "CreateTask",
            CreateTask {
                session_id: Some(session.clone()),
                goal_text: "x".into(),
                workspace_id: None,
                execution_profile: String::new(),
                origin: "cli".into(),
                workspace_root: root.clone(),
            }
            .encode_to_vec(),
            g,
        ))
        .await
        .unwrap();
    let task = Client::result::<TaskCreated>(&ack)
        .unwrap()
        .task_id
        .unwrap();
    let batch = r#"{"ops":[{"path":"n.txt","op":"create","content":"new\n"},{"path":"b.txt","op":"edit","text_edits":[{"old":"b","new":"B"}]},{"path":"c.txt","op":"delete"}]}"#;
    let ack = c
        .command(envelope_fenced(
            id16(0xE2),
            "InvokeTool",
            InvokeTool {
                task_id: Some(task.clone()),
                tool_name: "change.batch".into(),
                arguments_json: batch.into(),
                tool_call_id: Some(id16(0xF2)),
                output_budget_bytes: 65536,
            }
            .encode_to_vec(),
            g,
        ))
        .await
        .unwrap();
    let r: ToolInvoked = Client::result(&ack).unwrap();
    assert_eq!(r.status, "SUCCESS", "{r:?}");
    assert!(repo.path().join("n.txt").exists() && !repo.path().join("c.txt").exists());
    // Unrelated user work after the change.
    std::fs::write(repo.path().join("user.txt"), "user edit\n").unwrap();
    // The plan alone (no lease needed): latest change first, typed inverses.
    let ack = c
        .command(envelope(
            id16(0xE3),
            "UndoToolCall",
            UndoToolCall {
                task_id: Some(task.clone()),
                tool_call_id: Some(id16(0xF2)),
                apply: false,
            }
            .encode_to_vec(),
        ))
        .await
        .unwrap();
    let plan: UndoPlanView = Client::result(&ack).unwrap();
    assert!(!plan.applied);
    assert_eq!(
        plan.steps
            .iter()
            .map(|s| (s.path.as_str(), s.action.as_str()))
            .collect::<Vec<_>>(),
        vec![
            ("c.txt", "restore"),
            ("b.txt", "replace"),
            ("n.txt", "delete")
        ]
    );
    assert!(plan.steps[0].expect_absent && !plan.steps[0].restore_ref.is_empty());
    assert!(
        !plan.steps[1].expected_content_hash.is_empty() && !plan.steps[1].restore_ref.is_empty()
    );
    assert!(
        plan.steps[2].restore_ref.is_empty() && !plan.steps[2].expected_content_hash.is_empty()
    );
    let ack = c
        .command(envelope_fenced(
            id16(0xE4),
            "UndoToolCall",
            UndoToolCall {
                task_id: Some(task.clone()),
                tool_call_id: Some(id16(0xF2)),
                apply: true,
            }
            .encode_to_vec(),
            g,
        ))
        .await
        .unwrap();
    let done: UndoPlanView = Client::result(&ack).unwrap();
    assert!(done.applied && done.refusals.is_empty(), "{done:?}");
    assert!(!repo.path().join("n.txt").exists());
    assert_eq!(
        std::fs::read_to_string(repo.path().join("b.txt")).unwrap(),
        "b\n"
    );
    assert_eq!(
        std::fs::read_to_string(repo.path().join("c.txt")).unwrap(),
        "c\n"
    );
    assert_eq!(
        std::fs::read_to_string(repo.path().join("user.txt")).unwrap(),
        "user edit\n",
        "unrelated user changes preserved"
    );
    let evs = task_events(&core, &session, &task).await;
    let undo_ops: Vec<String> = evs
        .iter()
        .filter(|(a, t, p)| {
            a == "workspace" && t == "FileChanged" && p["op"].as_str().unwrap().starts_with("undo:")
        })
        .map(|(_, _, p)| p["op"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(
        undo_ops,
        vec!["undo:create", "undo:atomic_replace", "undo:delete"]
    );
    // QUAL-EV-0065: a user edit after the agent's change blocks the destructive revert.
    let ack = c
        .command(envelope_fenced(
            id16(0xE5),
            "InvokeTool",
            InvokeTool {
                task_id: Some(task.clone()),
                tool_name: "change.apply".into(),
                arguments_json: r#"{"path":"e.txt","op":"create","content":"e\n"}"#.into(),
                tool_call_id: Some(id16(0xF3)),
                output_budget_bytes: 65536,
            }
            .encode_to_vec(),
            g,
        ))
        .await
        .unwrap();
    assert_eq!(
        Client::result::<ToolInvoked>(&ack).unwrap().status,
        "SUCCESS"
    );
    std::fs::write(repo.path().join("e.txt"), "mine\n").unwrap();
    let ack = c
        .command(envelope_fenced(
            id16(0xE6),
            "UndoToolCall",
            UndoToolCall {
                task_id: Some(task.clone()),
                tool_call_id: Some(id16(0xF3)),
                apply: true,
            }
            .encode_to_vec(),
            g,
        ))
        .await
        .unwrap();
    let refused: UndoPlanView = Client::result(&ack).unwrap();
    assert!(!refused.applied, "{refused:?}");
    assert_eq!(
        (
            refused.refusals[0].code.as_str(),
            refused.refusals[0].path.as_str()
        ),
        ("USER_EDITED", "e.txt")
    );
    assert_eq!(
        std::fs::read_to_string(repo.path().join("e.txt")).unwrap(),
        "mine\n",
        "nothing written on refusal"
    );
    let evs = task_events(&core, &session, &task).await;
    assert_eq!(
        evs.iter()
            .filter(|(_, t, p)| t == "FileChanged"
                && p["tool_call_id"]
                    .as_str()
                    .map(|v| v.replace('-', "").to_lowercase())
                    == Some(hex::encode([0xF3u8; 16]))
                && p["op"].as_str().unwrap().starts_with("undo:"))
            .count(),
        0
    );
}

async fn list_tools(c: &mut Client, id: u8, task: Option<Id>) -> Vec<(String, String, String)> {
    use modbit_protocol::v1::{ListTools, ToolList};
    let ack = c
        .command(envelope(
            id16(id),
            "ListTools",
            ListTools { task_id: task }.encode_to_vec(),
        ))
        .await
        .unwrap();
    let l: ToolList = Client::result(&ack).unwrap();
    l.tools
        .into_iter()
        .map(|t| (t.name, t.input_schema_json, t.description))
        .collect()
}

async fn create_task_with_profile(
    c: &mut Client,
    session: &Id,
    g: Option<u64>,
    root: &str,
    id: u8,
    profile: &str,
) -> Id {
    let ack = c
        .command(envelope_fenced(
            id16(id),
            "CreateTask",
            CreateTask {
                session_id: Some(session.clone()),
                goal_text: "x".into(),
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

/// QUAL-EV-0096/0116/0133/0044: the tool surface is compiled from host
/// support × policy. Without the terminal broker the shell tools are absent
/// from the host list and from the model's projection; a narrower profile
/// drops the tools its lease does not carry; the projected schema is smaller
/// than the eager all-tools baseline; a tool the model names outside the
/// surface is refused before any effector. QUAL-EV-0031: an organization
/// block keeps a provider unavailable to ListModels/ProbeModel.
#[tokio::test]
async fn qual_ev_0096_0116_0133_0044_0031_tool_surface_is_compiled_from_support_and_policy() {
    use modbit_protocol::v1::{
        ListModels, ModelList, ModelProbed, ProbeModel, StartTask, TaskRunStarted,
    };
    use serde_json::json;
    let (_repo, root) = plain_repo(&[("a.txt", "a\n")]);
    // The model names a tool the host cannot serve, then completes.
    let script = vec![
        json!({"calls": [{"name": "plan.update", "args": {"outcome": "o", "expected_files": []}}]}),
        json!({"calls": [{"name": "shell.exec", "args": {"argv": ["sh", "-c", "echo hi"], "inherit_env": true}}]}),
        json!({"calls": [{"name": "task.complete", "args": {"summary": "done", "self_review": {"findings": []}}}]}),
    ];
    let (base, seen) = scripted_model(script, None).await;
    let dir = tempfile::tempdir().unwrap();
    let no_broker = dir.path().join("no-such-execd");
    let env = [
        ("MODBIT_OPENAI_BASE_URL", base.as_str()),
        ("MODBIT_ANTHROPIC_BASE_URL", base.as_str()),
        ("OPENAI_API_KEY", ""),
        ("ANTHROPIC_API_KEY", ""),
        ("MODBIT_EXECD_BIN", no_broker.to_str().unwrap()),
        ("MODBIT_MODEL_POLICY", "block=anthropic/*"),
    ];
    let core = CoreProcess::spawn_with_env(dir.path(), &env);
    let mut c = core.client().await;
    let names = |v: &[(String, String, String)]| v.iter().map(|t| t.0.clone()).collect::<Vec<_>>();
    let host = list_tools(&mut c, 0xA0, None).await;
    assert!(
        names(&host).iter().any(|n| n == "fs.read")
            && names(&host).iter().any(|n| n == "change.apply")
    );
    assert!(
        !names(&host)
            .iter()
            .any(|n| n == "shell.exec" || n == "test.run"),
        "no broker: shell tools are not advertised: {:?}",
        names(&host)
    );
    let (session, _) = create_session(&mut c, id16(0xA1)).await;
    let g = lease_for(&session);
    let trusted = create_task_with_profile(&mut c, &session, g, &root, 0xA2, "local_trusted").await;
    let isolated =
        create_task_with_profile(&mut c, &session, g, &root, 0xA3, "review_isolated").await;
    let t_tools = list_tools(&mut c, 0xA4, Some(trusted.clone())).await;
    let i_tools = list_tools(&mut c, 0xA5, Some(isolated.clone())).await;
    assert!(names(&t_tools).iter().any(|n| n == "git.worktree.create"));
    assert!(
        !names(&i_tools)
            .iter()
            .any(|n| n.starts_with("git.worktree")),
        "review_isolated lease carries no git.worktree: {:?}",
        names(&i_tools)
    );
    assert!(names(&i_tools).iter().any(|n| n == "fs.read"));
    assert!(
        names(&i_tools).len() < names(&t_tools).len()
            && names(&t_tools).len() <= names(&host).len()
    );
    // The model sees the compiled surface, and its schema is smaller than the eager baseline.
    let ack = c
        .command(envelope_fenced(
            id16(0xA6),
            "StartTask",
            StartTask {
                task_id: Some(isolated.clone()),
                endpoint: "openai".into(),
                model: "gpt-5".into(),
                max_turns: 0,
                max_tool_calls: 0,
                max_no_progress_turns: 0,
            }
            .encode_to_vec(),
            g,
        ))
        .await
        .unwrap();
    let _: TaskRunStarted = Client::result(&ack).unwrap();
    let status = wait_task(&mut c, &isolated, 60).await;
    assert!(
        status.state == "ReadyForReview" || status.state == "Completed",
        "{status:?}\n{:#?}",
        task_events(&core, &session, &isolated).await
    );
    let first = seen.lock().unwrap()[0].clone();
    let projected: Vec<String> = first["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["function"]["name"].as_str().unwrap().to_owned())
        .collect();
    assert!(
        !projected
            .iter()
            .any(|n| n == "shell.exec" || n.starts_with("git.worktree")),
        "{projected:?}"
    );
    assert!(
        projected.iter().any(|n| n == "fs.read") && projected.iter().any(|n| n == "plan.update")
    );
    // QUAL-EV-0116: bytes of the projected schema vs an eager projection of the whole host
    // list (itself already without the unsupported shell tools).
    // Both sides in the provider wire shape; the three harness tools are in both and left out.
    let harness = ["plan.update", "task.complete", "verify.run", "user.ask"];
    let projected_wire: Vec<serde_json::Value> = first["tools"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|t| !harness.contains(&t["function"]["name"].as_str().unwrap()))
        .cloned()
        .collect();
    let eager_wire: Vec<serde_json::Value> = host
        .iter()
        .map(|(n, schema, d)| {
            json!({"type": "function", "function": {"name": n, "description": d, "parameters": serde_json::from_str::<serde_json::Value>(schema).unwrap()}})
        })
        .collect();
    let projected_bytes = serde_json::to_string(&projected_wire).unwrap().len();
    let eager_bytes = serde_json::to_string(&eager_wire).unwrap().len();
    eprintln!(
        "QUAL-EV-0116 tool schema bytes (wire shape, harness tools excluded): projected={projected_bytes} ({} tools) eager={eager_bytes} ({} tools)",
        projected_wire.len(),
        eager_wire.len()
    );
    assert!(
        projected_bytes < eager_bytes,
        "projected {projected_bytes} < eager {eager_bytes}"
    );
    // QUAL-EV-0044: the invisible tool was refused before any effector; no ToolCallProposed exists.
    let evs = task_events(&core, &session, &isolated).await;
    assert!(
        evs.iter().any(|(a, t, p)| a == "run_step"
            && t == "StepFailed"
            && p["failure_code"] == "TOOL_NOT_VISIBLE"),
        "{evs:#?}"
    );
    assert!(
        !evs.iter()
            .any(|(_, t, p)| t == "ToolCallProposed" && p["tool_name"] == "shell.exec")
    );
    // QUAL-EV-0031: the blocked provider is unavailable regardless of the request.
    let ack = c
        .command(envelope(
            id16(0xA7),
            "ListModels",
            ListModels {}.encode_to_vec(),
        ))
        .await
        .unwrap();
    let ml: ModelList = Client::result(&ack).unwrap();
    let anth = ml
        .models
        .iter()
        .find(|m| m.provider == "anthropic")
        .unwrap_or_else(|| panic!("{ml:?}"));
    assert_eq!(anth.blocked_by_policy, "block=anthropic/*");
    assert!(
        ml.models
            .iter()
            .filter(|m| m.provider == "openai")
            .all(|m| m.blocked_by_policy.is_empty())
    );
    let ack = c
        .command(envelope(
            id16(0xA8),
            "ProbeModel",
            ProbeModel {
                endpoint: "anthropic".into(),
                model: anth.model.clone(),
                prompt: "hi".into(),
                with_tools: false,
                timeout_ms: 5000,
            }
            .encode_to_vec(),
        ))
        .await
        .unwrap();
    let probed: ModelProbed = Client::result(&ack).unwrap();
    assert_eq!(
        (probed.status.as_str(), probed.error_code.as_str()),
        ("ROUTE_REFUSED", "POLICY_BLOCKED"),
        "{probed:?}"
    );
}

async fn queue_input(c: &mut Client, task: &Id, g: Option<u64>, cmd: u8, mode: &str, text: &str) {
    use modbit_protocol::v1::QueueInput;
    let ack = c
        .command(envelope_fenced(
            id16(cmd),
            "QueueInput",
            QueueInput {
                task_id: Some(task.clone()),
                input_id: format!("in-{cmd:02x}"),
                mode: mode.into(),
                text: text.into(),
            }
            .encode_to_vec(),
            g,
        ))
        .await
        .unwrap();
    assert_eq!(ack.status, CommandStatus::Accepted as i32, "{ack:?}");
}

/// QUAL-EV-0221: a background command has a durable handle that survives a
/// client restart; its output is read from a byte cursor as a bounded
/// preview that continues exactly; list shows status; cancel stops the real
/// process and the full OutputRef is returned.
#[tokio::test]
async fn qual_ev_0221_background_handles_survive_client_restart_with_bounded_preview_and_cancel() {
    let (_repo, root) = plain_repo(&[("a.txt", "a\n")]);
    let dir = tempfile::tempdir().unwrap();
    let core = CoreProcess::spawn(dir.path());
    let mut c = core.client().await;
    let (session, _) = create_session(&mut c, id16(0xB0)).await;
    let g = lease_for(&session);
    let task = create_task_with_profile(&mut c, &session, g, &root, 0xB1, "local_trusted").await;
    let start = r#"{"argv":["sh","-c","i=0; while true; do echo tick $i; i=$((i+1)); sleep 0.02; done"],"inherit_env":true,"timeout_ms":600000}"#;
    let r = invoke_tool(&mut c, &task, g, 0xB2, 0xC1, "shell.start", start).await;
    assert_eq!(r.status, "SUCCESS", "{r:?}");
    let so: serde_json::Value = serde_json::from_str(&r.structured_output_json).unwrap();
    let handle = so["session_id"].as_str().unwrap().to_owned();
    assert!(so["running"].as_bool().unwrap());
    // UI restart: a fresh client connection.
    let mut c2 = core.client().await;
    let r = invoke_tool(&mut c2, &task, g, 0xB3, 0xC2, "shell.list", "{}").await;
    let so: serde_json::Value = serde_json::from_str(&r.structured_output_json).unwrap();
    let mine = so["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|x| x["session_id"] == handle.as_str())
        .unwrap_or_else(|| panic!("{so}"));
    assert_eq!(mine["running"], true);
    let read = |after: u64| {
        format!(
            r#"{{"session_id":"{handle}","after_cursor":{after},"wait_ms":400,"max_bytes":600}}"#
        )
    };
    let r = invoke_tool(&mut c2, &task, g, 0xB4, 0xC3, "shell.read", &read(0)).await;
    assert_eq!(r.status, "SUCCESS", "{r:?}");
    let p1: serde_json::Value = serde_json::from_str(&r.structured_output_json).unwrap();
    assert!(
        p1["preview_bytes"].as_u64().unwrap() <= 600 && p1["running"] == true,
        "{p1}"
    );
    let next = p1["next_cursor"].as_u64().unwrap();
    assert!(next > 0 && next == p1["preview_bytes"].as_u64().unwrap());
    let r = invoke_tool(&mut c2, &task, g, 0xB5, 0xC4, "shell.read", &read(next)).await;
    let p2: serde_json::Value = serde_json::from_str(&r.structured_output_json).unwrap();
    assert_eq!(p2["after_cursor"].as_u64().unwrap(), next);
    let combined = format!(
        "{}{}",
        p1["preview"].as_str().unwrap(),
        p2["preview"].as_str().unwrap()
    );
    let ticks: Vec<u64> = combined
        .lines()
        .filter(|l| l.starts_with("tick ") && combined.ends_with('\n') || l.starts_with("tick "))
        .filter_map(|l| l[5..].trim().parse().ok())
        .collect();
    assert!(ticks.len() >= 3, "{combined:?}");
    let complete = if combined.ends_with('\n') {
        &ticks[..]
    } else {
        &ticks[..ticks.len() - 1]
    };
    assert!(
        complete.windows(2).all(|w| w[1] == w[0] + 1),
        "the cursor continuation is exact, no gap or overlap: {complete:?}"
    );
    assert_eq!(complete[0], 0);
    // Stop it: the real process exits and the complete output is by reference.
    let r = invoke_tool(
        &mut c2,
        &task,
        g,
        0xB6,
        0xC5,
        "shell.cancel",
        &format!(r#"{{"session_id":"{handle}"}}"#),
    )
    .await;
    assert_eq!(r.status, "SUCCESS", "{r:?}");
    let x: serde_json::Value = serde_json::from_str(&r.structured_output_json).unwrap();
    assert!(
        x["cancelled"] == true && !x["output_ref"].as_str().unwrap().is_empty(),
        "{x}"
    );
    assert!(
        x["total_bytes"].as_u64().unwrap()
            >= p1["preview_bytes"].as_u64().unwrap() + p2["preview_bytes"].as_u64().unwrap()
    );
    let r = invoke_tool(&mut c2, &task, g, 0xB7, 0xC6, "shell.list", "{}").await;
    let so: serde_json::Value = serde_json::from_str(&r.structured_output_json).unwrap();
    let mine = so["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|x| x["session_id"] == handle.as_str())
        .unwrap();
    assert_eq!(mine["running"], false);
    let r = invoke_tool(&mut c2, &task, g, 0xB8, 0xC7, "shell.read", &read(0)).await;
    let p3: serde_json::Value = serde_json::from_str(&r.structured_output_json).unwrap();
    assert!(
        p3["running"] == false && p3["exited"]["cancelled"] == true,
        "{p3}"
    );
    let r = invoke_tool(
        &mut c2,
        &task,
        g,
        0xB9,
        0xC8,
        "shell.read",
        r#"{"session_id":"no-such-session","after_cursor":0}"#,
    )
    .await;
    assert_eq!(
        (r.status.as_str(), r.error_code.as_str()),
        ("APPLICATION_FAILURE", "UNKNOWN_SESSION"),
        "{r:?}"
    );
}

async fn task_status(c: &mut Client, id: u8, task: &Id) -> modbit_protocol::v1::TaskStatus {
    use modbit_protocol::v1::GetTaskStatus;
    let ack = c
        .command(envelope(
            id16(id),
            "GetTaskStatus",
            GetTaskStatus {
                task_id: Some(task.clone()),
            }
            .encode_to_vec(),
        ))
        .await
        .unwrap();
    Client::result(&ack).unwrap()
}

/// QUAL-EV-0261: a side question is answered from a bounded snapshot (goal,
/// plan, recent transcript) with no tools and appends nothing: the task's
/// state and the log cursor are unchanged.
#[tokio::test]
async fn qual_ev_0261_side_question_answers_from_a_snapshot_without_touching_task_state_or_cursor()
{
    use modbit_protocol::v1::{AskSideQuestion, SideAnswer};
    use serde_json::json;
    let (_repo, root) = plain_repo(&[("a.txt", "a\n")]);
    let (base, seen) = scripted_model(vec![json!({"text": "SIDE ANSWER"})], None).await;
    let dir = tempfile::tempdir().unwrap();
    let env = [
        ("MODBIT_OPENAI_BASE_URL", base.as_str()),
        ("OPENAI_API_KEY", ""),
        ("ANTHROPIC_API_KEY", ""),
    ];
    let core = CoreProcess::spawn_with_env(dir.path(), &env);
    let mut c = core.client().await;
    let (session, _) = create_session(&mut c, id16(0xC0)).await;
    let g = lease_for(&session);
    let task = create_task_with_profile(&mut c, &session, g, &root, 0xC1, "local_trusted").await;
    let before = task_status(&mut c, 0xC2, &task).await;
    let events_before = task_events(&core, &session, &task).await.len();
    let ack = c
        .command(envelope(
            id16(0xC3),
            "AskSideQuestion",
            AskSideQuestion {
                task_id: Some(task.clone()),
                text: "what does a.txt contain?".into(),
                endpoint: String::new(),
                model: String::new(),
            }
            .encode_to_vec(),
        ))
        .await
        .unwrap();
    let a: SideAnswer = Client::result(&ack).unwrap();
    assert_eq!(a.text, "SIDE ANSWER");
    assert_eq!(a.last_offset, before.last_offset, "the cursor did not move");
    let after = task_status(&mut c, 0xC4, &task).await;
    assert_eq!(
        (after.state.as_str(), after.last_offset, after.loop_alive),
        (before.state.as_str(), before.last_offset, false)
    );
    assert_eq!(
        task_events(&core, &session, &task).await.len(),
        events_before,
        "nothing appended"
    );
    let body = seen.lock().unwrap()[0].clone();
    assert!(
        body["tools"].as_array().is_none_or(|t| t.is_empty()),
        "no tools on a side question: {body}"
    );
    let msgs = body["messages"].as_array().unwrap();
    assert!(
        msgs[0]["role"] == "system"
            && msgs[0]["content"]
                .as_str()
                .unwrap()
                .contains("Task goal: x")
    );
    assert!(
        msgs.last().unwrap()["content"]
            .as_str()
            .unwrap()
            .contains("[SIDE QUESTION] what does a.txt contain?")
    );
    assert!(
        a.route_json.contains("\"endpoint\":\"openai\""),
        "{}",
        a.route_json
    );
    // An empty question is refused before any model call.
    let err = c
        .command(envelope(
            id16(0xC5),
            "AskSideQuestion",
            AskSideQuestion {
                task_id: Some(task.clone()),
                text: "  ".into(),
                endpoint: String::new(),
                model: String::new(),
            }
            .encode_to_vec(),
        ))
        .await
        .unwrap_err();
    assert!(
        matches!(err, ClientError::Rejected { ref code, .. } if code == "BAD_PAYLOAD"),
        "{err}"
    );
    assert_eq!(seen.lock().unwrap().len(), 1);
}

/// QUAL-EV-0191: SteeringPolicy. COLLECT inputs coalesce into one message
/// after the current turn; FOLLOW_UP inputs become ordered separate turns;
/// a STEER interrupts the in-flight model stream, nothing from the
/// interrupted response is applied, and the next turn starts from the steer.
#[tokio::test]
async fn qual_ev_0191_steering_policy_interrupts_replaces_coalesces_and_orders() {
    use modbit_protocol::v1::{StartTask, TaskRunStarted};
    use serde_json::json;
    let (_repo, root) = plain_repo(&[("a.txt", "a\n")]);
    let script = vec![
        json!({"calls": [{"name": "plan.update", "args": {"outcome": "o", "expected_files": []}}]}),
        json!({"calls": [{"name": "task.complete", "args": {"summary": "done", "self_review": {"findings": []}}}]}),
    ];
    // The second request (one tool result so far) stalls until the stream is cut.
    let (base, seen) = scripted_model(script, Some(1)).await;
    let dir = tempfile::tempdir().unwrap();
    let env = [
        ("MODBIT_OPENAI_BASE_URL", base.as_str()),
        ("OPENAI_API_KEY", ""),
        ("ANTHROPIC_API_KEY", ""),
    ];
    let core = CoreProcess::spawn_with_env(dir.path(), &env);
    let mut c = core.client().await;
    let (session, _) = create_session(&mut c, id16(0xD0)).await;
    let g = lease_for(&session);
    let task = create_task_with_profile(&mut c, &session, g, &root, 0xD1, "local_trusted").await;
    queue_input(&mut c, &task, g, 0xD2, "COLLECT", "c1").await;
    queue_input(&mut c, &task, g, 0xD3, "COLLECT", "c2").await;
    queue_input(&mut c, &task, g, 0xD4, "FOLLOW_UP", "f1").await;
    queue_input(&mut c, &task, g, 0xD5, "FOLLOW_UP", "f2").await;
    let ack = c
        .command(envelope_fenced(
            id16(0xD6),
            "StartTask",
            StartTask {
                task_id: Some(task.clone()),
                endpoint: "openai".into(),
                model: "gpt-5".into(),
                max_turns: 0,
                max_tool_calls: 0,
                max_no_progress_turns: 0,
            }
            .encode_to_vec(),
            g,
        ))
        .await
        .unwrap();
    let _: TaskRunStarted = Client::result(&ack).unwrap();
    let wait_requests = |n: usize| {
        let seen = std::sync::Arc::clone(&seen);
        async move {
            let deadline = std::time::Instant::now() + Duration::from_secs(60);
            while seen.lock().unwrap().len() < n {
                assert!(
                    std::time::Instant::now() < deadline,
                    "waiting for request {n}"
                );
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        }
    };
    wait_requests(2).await;
    // Request 1 is streaming (stalled): a STEER cuts it.
    queue_input(&mut c, &task, g, 0xD7, "STEER", "s1").await;
    wait_requests(3).await;
    let status = wait_task(&mut c, &task, 60).await;
    assert!(
        status.state == "ReadyForReview" || status.state == "Completed",
        "{status:?}"
    );
    let bodies = seen.lock().unwrap().clone();
    let user_texts = |b: &serde_json::Value| -> Vec<String> {
        b["messages"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|m| m["role"] == "user")
            .map(|m| m["content"].as_str().unwrap_or_default().to_owned())
            .collect()
    };
    let first = user_texts(&bodies[0]);
    assert!(
        first.iter().any(|t| t == "[COLLECT] c1\nc2"),
        "COLLECT coalesces into one message: {first:?}"
    );
    assert!(
        first.iter().any(|t| t == "[FOLLOW_UP] f1") && !first.iter().any(|t| t.contains("f2")),
        "FOLLOW_UP one per boundary: {first:?}"
    );
    let second = user_texts(&bodies[1]);
    assert!(
        second.iter().any(|t| t == "[FOLLOW_UP] f2") && !first.iter().any(|t| t.contains("f2")),
        "the carried follow-up lands at the next boundary, as its own turn: {second:?}"
    );
    let third = user_texts(&bodies[2]);
    assert!(third.iter().any(|t| t == "[STEER] s1"), "{third:?}");
    let evs = task_events(&core, &session, &task).await;
    assert!(
        evs.iter()
            .any(|(a, t, _)| a == "turn" && t == "TurnInterrupted"),
        "{evs:#?}"
    );
    assert!(
        evs.iter()
            .any(|(a, t, _)| a == "run_step" && t == "StepCancelled")
    );
    let steered: Vec<String> = evs
        .iter()
        .filter(|(_, t, _)| t == "TaskSteered")
        .map(|(_, _, p)| p["text"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(
        steered,
        vec![
            "c1
c2", "f1", "f2", "s1"
        ],
        "order of applied inputs"
    );
}

/// QUAL-EV-0222 / QUAL-PX-014: a typed question with concrete options
/// suspends the run (Waiting/UserInput, loop idle: headless never hangs);
/// resuming without an answer is refused; the answer becomes the tool
/// result the model sees next. An ambiguous fixture yields exactly one
/// question, an unambiguous one none; a question that merely confirms a
/// verifiable repository fact is flagged; plan revisions are on the timeline.
#[tokio::test]
async fn qual_ev_0222_px_014_typed_question_suspends_the_run_and_the_answer_resumes_it() {
    use modbit_protocol::v1::{
        ListQuestions, QuestionList, QuestionResponded, RespondToQuestion, StartTask,
        TaskRunStarted,
    };
    use serde_json::json;
    let (_repo, root) = plain_repo(&[("a.txt", "a\n")]);
    let ambiguous = vec![
        json!({"calls": [{"name": "plan.update", "args": {"outcome": "pick a config", "expected_files": ["chosen.txt"]}}]}),
        json!({"calls": [{"name": "user.ask", "args": {"question": "Which config should the new file follow?", "options": [{"id": "a", "label": "the a layout"}, {"id": "b", "label": "the b layout"}], "reason": "change_set"}}]}),
        json!({"calls": [{"name": "plan.update", "args": {"outcome": "pick a config", "expected_files": ["chosen.txt", "notes.txt"], "reason": "the answer adds a note"}}]}),
        json!({"calls": [{"name": "change.apply", "args": {"path": "chosen.txt", "op": "create", "content": "a\n"}}]}),
        json!({"calls": [{"name": "task.complete", "args": {"summary": "done", "self_review": {"findings": []}}}]}),
    ];
    let (base, seen) = scripted_model(ambiguous, None).await;
    let dir = tempfile::tempdir().unwrap();
    let env = [
        ("MODBIT_OPENAI_BASE_URL", base.as_str()),
        ("OPENAI_API_KEY", ""),
        ("ANTHROPIC_API_KEY", ""),
    ];
    let core = CoreProcess::spawn_with_env(dir.path(), &env);
    let mut c = core.client().await;
    let (session, _) = create_session(&mut c, id16(0xE0)).await;
    let g = lease_for(&session);
    let task = create_task_with_profile(&mut c, &session, g, &root, 0xE1, "local_trusted").await;
    let start = StartTask {
        task_id: Some(task.clone()),
        endpoint: "openai".into(),
        model: "gpt-5".into(),
        max_turns: 0,
        max_tool_calls: 0,
        max_no_progress_turns: 0,
    }
    .encode_to_vec();
    let ack = c
        .command(envelope_fenced(id16(0xE2), "StartTask", start.clone(), g))
        .await
        .unwrap();
    let _: TaskRunStarted = Client::result(&ack).unwrap();
    let st = wait_task(&mut c, &task, 60).await;
    assert_eq!(
        (st.state.as_str(), st.wait_reason.as_str(), st.loop_alive),
        ("Waiting", "UserInput", false),
        "{st:?}"
    );
    let ack = c
        .command(envelope(
            id16(0xE3),
            "ListQuestions",
            ListQuestions {
                task_id: Some(task.clone()),
            }
            .encode_to_vec(),
        ))
        .await
        .unwrap();
    let l: QuestionList = Client::result(&ack).unwrap();
    assert_eq!(l.questions.len(), 1, "{l:?}");
    let q = &l.questions[0];
    assert!(
        !q.answered
            && q.options.iter().map(|o| o.id.as_str()).collect::<Vec<_>>() == ["a", "b"]
            && q.reason == "change_set"
            && q.flags.is_empty(),
        "{q:?}"
    );
    // Resuming without an answer is refused: the run never spins on a pending question.
    let err = c
        .command(envelope_fenced(id16(0xE4), "StartTask", start.clone(), g))
        .await
        .unwrap_err();
    assert!(
        matches!(err, ClientError::Rejected { ref code, .. } if code == "QUESTION_PENDING"),
        "{err}"
    );
    // A wrong option is refused; the typed answer is recorded once.
    let err = c
        .command(envelope_fenced(
            id16(0xE5),
            "RespondToQuestion",
            RespondToQuestion {
                task_id: Some(task.clone()),
                question_id: q.question_id.clone(),
                option_id: "z".into(),
                text: String::new(),
            }
            .encode_to_vec(),
            g,
        ))
        .await
        .unwrap_err();
    assert!(
        matches!(err, ClientError::Rejected { ref code, .. } if code == "BAD_ANSWER"),
        "{err}"
    );
    let err = c
        .command(envelope_fenced(
            id16(0xE6),
            "RespondToQuestion",
            RespondToQuestion {
                task_id: Some(task.clone()),
                question_id: q.question_id.clone(),
                option_id: String::new(),
                text: "free text".into(),
            }
            .encode_to_vec(),
            g,
        ))
        .await
        .unwrap_err();
    assert!(
        matches!(err, ClientError::Rejected { ref code, .. } if code == "BAD_ANSWER"),
        "free text is not accepted when options are typed: {err}"
    );
    let ack = c
        .command(envelope_fenced(
            id16(0xE7),
            "RespondToQuestion",
            RespondToQuestion {
                task_id: Some(task.clone()),
                question_id: q.question_id.clone(),
                option_id: "a".into(),
                text: String::new(),
            }
            .encode_to_vec(),
            g,
        ))
        .await
        .unwrap();
    let r: QuestionResponded = Client::result(&ack).unwrap();
    assert!(!r.already_answered);
    let ack = c
        .command(envelope_fenced(
            id16(0xE8),
            "RespondToQuestion",
            RespondToQuestion {
                task_id: Some(task.clone()),
                question_id: q.question_id.clone(),
                option_id: "b".into(),
                text: String::new(),
            }
            .encode_to_vec(),
            g,
        ))
        .await
        .unwrap();
    assert_eq!(
        (
            ack.status,
            Client::result::<QuestionResponded>(&ack)
                .unwrap()
                .already_answered
        ),
        (CommandStatus::Replayed as i32, true),
        "a second answer replays the first"
    );
    let ack = c
        .command(envelope_fenced(id16(0xE9), "StartTask", start, g))
        .await
        .unwrap();
    assert!(Client::result::<TaskRunStarted>(&ack).unwrap().resumed);
    let st = wait_task(&mut c, &task, 60).await;
    let trail = task_events(&core, &session, &task).await;
    assert_eq!(st.state, "ReadyForReview", "{st:?}\n{trail:#?}");
    // The model saw the answer as the result of its user.ask call, exactly once.
    let bodies = seen.lock().unwrap().clone();
    let resumed = &bodies[2]["messages"];
    let answers: Vec<&serde_json::Value> = resumed
        .as_array()
        .unwrap()
        .iter()
        .filter(|m| {
            m["role"] == "tool"
                && m["content"]
                    .as_str()
                    .is_some_and(|c| c.contains("\"option_id\":\"a\""))
        })
        .collect();
    assert_eq!(answers.len(), 1, "{resumed}");
    assert!(
        !resumed.to_string().contains("status: PENDING"),
        "the placeholder was replaced by the answer"
    );
    let evs = task_events(&core, &session, &task).await;
    let asked = evs
        .iter()
        .filter(|(_, t, _)| t == "UserQuestionAsked")
        .count();
    let answered = evs
        .iter()
        .filter(|(_, t, _)| t == "UserQuestionAnswered")
        .count();
    assert_eq!((asked, answered), (1, 1));
    assert!(
        evs.iter().any(|(_, t, _)| t == "PlanRecorded")
            && evs.iter().any(|(_, t, _)| t == "PlanRevised"),
        "plan revisions appear on the timeline"
    );
    let plan_before_write = evs
        .iter()
        .position(|(_, t, _)| t == "PlanRecorded")
        .unwrap()
        < evs
            .iter()
            .position(|(_, t, p)| t == "ToolCallProposed" && p["tool_name"] == "change.apply")
            .unwrap();
    assert!(plan_before_write);
    assert!(_repo.path().join("chosen.txt").exists());
    // Unambiguous fixture: no question at all.
    let unambiguous = vec![
        json!({"calls": [{"name": "plan.update", "args": {"outcome": "o", "expected_files": ["plain.txt"]}}]}),
        json!({"calls": [{"name": "change.apply", "args": {"path": "plain.txt", "op": "create", "content": "p\n"}}]}),
        json!({"calls": [{"name": "task.complete", "args": {"summary": "done", "self_review": {"findings": []}}}]}),
    ];
    let (base2, _seen2) = scripted_model(unambiguous, None).await;
    let dir2 = tempfile::tempdir().unwrap();
    let env2 = [
        ("MODBIT_OPENAI_BASE_URL", base2.as_str()),
        ("OPENAI_API_KEY", ""),
        ("ANTHROPIC_API_KEY", ""),
    ];
    let core2 = CoreProcess::spawn_with_env(dir2.path(), &env2);
    let mut c2 = core2.client().await;
    let (session2, _) = create_session(&mut c2, id16(0xEA)).await;
    let g2 = lease_for(&session2);
    let (_repo2, root2) = plain_repo(&[("a.txt", "a\n")]);
    let task2 =
        create_task_with_profile(&mut c2, &session2, g2, &root2, 0xEB, "local_trusted").await;
    let ack = c2
        .command(envelope_fenced(
            id16(0xEC),
            "StartTask",
            StartTask {
                task_id: Some(task2.clone()),
                endpoint: "openai".into(),
                model: "gpt-5".into(),
                max_turns: 0,
                max_tool_calls: 0,
                max_no_progress_turns: 0,
            }
            .encode_to_vec(),
            g2,
        ))
        .await
        .unwrap();
    let _: TaskRunStarted = Client::result(&ack).unwrap();
    assert_eq!(wait_task(&mut c2, &task2, 60).await.state, "ReadyForReview");
    assert_eq!(
        task_events(&core2, &session2, &task2)
            .await
            .iter()
            .filter(|(_, t, _)| t == "UserQuestionAsked")
            .count(),
        0
    );
    // A question that only confirms a verifiable repository fact is flagged.
    let confirming = vec![
        json!({"calls": [{"name": "plan.update", "args": {"outcome": "o", "expected_files": []}}]}),
        json!({"calls": [{"name": "user.ask", "args": {"question": "Does a.txt exist in the repository?", "options": [{"id": "yes", "label": "yes"}, {"id": "no", "label": "no"}], "reason": "other"}}]}),
    ];
    let (base3, _seen3) = scripted_model(confirming, None).await;
    let dir3 = tempfile::tempdir().unwrap();
    let env3 = [
        ("MODBIT_OPENAI_BASE_URL", base3.as_str()),
        ("OPENAI_API_KEY", ""),
        ("ANTHROPIC_API_KEY", ""),
    ];
    let core3 = CoreProcess::spawn_with_env(dir3.path(), &env3);
    let mut c3 = core3.client().await;
    let (session3, _) = create_session(&mut c3, id16(0xED)).await;
    let g3 = lease_for(&session3);
    let (_repo3, root3) = plain_repo(&[("a.txt", "a\n")]);
    let task3 =
        create_task_with_profile(&mut c3, &session3, g3, &root3, 0xEE, "local_trusted").await;
    let ack = c3
        .command(envelope_fenced(
            id16(0xEF),
            "StartTask",
            StartTask {
                task_id: Some(task3.clone()),
                endpoint: "openai".into(),
                model: "gpt-5".into(),
                max_turns: 0,
                max_tool_calls: 0,
                max_no_progress_turns: 0,
            }
            .encode_to_vec(),
            g3,
        ))
        .await
        .unwrap();
    let _: TaskRunStarted = Client::result(&ack).unwrap();
    assert_eq!(wait_task(&mut c3, &task3, 60).await.state, "Waiting");
    let ack = c3
        .command(envelope(
            id16(0xF0),
            "ListQuestions",
            ListQuestions {
                task_id: Some(task3.clone()),
            }
            .encode_to_vec(),
        ))
        .await
        .unwrap();
    let l: QuestionList = Client::result(&ack).unwrap();
    assert_eq!(
        l.questions[0].flags,
        vec!["CONFIRMS_REPOSITORY_FACT".to_owned()],
        "{:?}",
        l.questions
    );
}

/// QUAL-EV-0190: an attachment ingested through the API (desktop/CLI use the
/// same command) is normalized to the very same canonical MediaEnvelope a
/// workspace read of the same bytes produces: kind, MIME, digests, dimensions,
/// stripped metadata and trust label are identical; only the provenance
/// source differs. Bytes never ride on the log; the same bytes replay.
#[tokio::test]
async fn qual_ev_0190_attachments_normalize_to_the_same_canonical_envelope_as_workspace_reads() {
    use modbit_protocol::v1::{AttachmentIngested, IngestAttachment};
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/media/label.png");
    let bytes = std::fs::read(&fixture).unwrap();
    let (repo, root) = plain_repo(&[("a.txt", "a\n")]);
    std::fs::write(repo.path().join("label.png"), &bytes).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let core = CoreProcess::spawn(dir.path());
    let mut c = core.client().await;
    let (session, _) = create_session(&mut c, id16(0xF0)).await;
    let g = lease_for(&session);
    let task = create_task_with_profile(&mut c, &session, g, &root, 0xF1, "local_trusted").await;
    let ingest = IngestAttachment {
        task_id: Some(task.clone()),
        filename: "label.png".into(),
        channel: "api".into(),
        data: bytes.clone(),
    }
    .encode_to_vec();
    let ack = c
        .command(envelope_fenced(
            id16(0xF2),
            "IngestAttachment",
            ingest.clone(),
            g,
        ))
        .await
        .unwrap();
    let a: AttachmentIngested = Client::result(&ack).unwrap();
    assert!(
        !a.replayed && a.kind == "IMAGE" && a.mime == "image/png",
        "{a:?}"
    );
    let env_a: serde_json::Value = serde_json::from_str(&a.envelope_json).unwrap();
    let r = invoke_tool(
        &mut c,
        &task,
        g,
        0xF3,
        0xC1,
        "fs.read",
        r#"{"path":"label.png"}"#,
    )
    .await;
    assert_eq!(r.status, "SUCCESS", "{r:?}");
    let so: serde_json::Value = serde_json::from_str(&r.structured_output_json).unwrap();
    let env_w = so["media"].clone();
    for field in [
        "kind",
        "mime",
        "content_ref",
        "byte_length",
        "egress_ref",
        "width",
        "height",
        "trust",
        "metadata_stripped",
        "budget",
        "lineage",
    ] {
        assert_eq!(
            env_a[field], env_w[field],
            "{field} differs between attachment and workspace read"
        );
    }
    assert_eq!(
        env_a["provenance"]["original_digest"],
        env_w["provenance"]["original_digest"]
    );
    assert_eq!(env_a["provenance"]["source"], "attachment:api:label.png");
    assert_eq!(
        env_a["provenance"]["task_id"]
            .as_str()
            .map(|s| s.replace('-', "")),
        Some(hex::encode(&task.value))
    );
    // The log carries the envelope (digests), never the bytes; the original is retrievable by digest.
    let evs = task_events(&core, &session, &task).await;
    let ing: Vec<_> = evs
        .iter()
        .filter(|(_, t, _)| t == "AttachmentIngested")
        .collect();
    assert_eq!(ing.len(), 1);
    let payload = ing[0].2.to_string();
    assert!(
        payload.len() < 4096
            && payload.contains(&a.content_ref)
            && payload.contains("\"channel\":\"api\""),
        "{payload}"
    );
    let original = read_object_bytes(&mut c, id16(0xF4), &a.content_ref).await;
    assert_eq!(
        original, bytes,
        "the original bytes are retrievable by digest"
    );
    // Same bytes again: replayed, no second event.
    let ack = c
        .command(envelope_fenced(id16(0xF5), "IngestAttachment", ingest, g))
        .await
        .unwrap();
    let again: AttachmentIngested = Client::result(&ack).unwrap();
    assert!(
        again.replayed && again.offset == a.offset && again.content_ref == a.content_ref,
        "{again:?}"
    );
    assert_eq!(
        task_events(&core, &session, &task)
            .await
            .iter()
            .filter(|(_, t, _)| t == "AttachmentIngested")
            .count(),
        1
    );
    // Malformed media is a typed failure, not an event.
    let bad = IngestAttachment {
        task_id: Some(task.clone()),
        filename: "x.png".into(),
        channel: "api".into(),
        data: b"\x89PNG\r\n\x1a\ntruncated".to_vec(),
    }
    .encode_to_vec();
    let err = c
        .command(envelope_fenced(id16(0xF6), "IngestAttachment", bad, g))
        .await
        .unwrap_err();
    assert!(
        matches!(err, ClientError::Rejected { ref code, .. } if code == "MEDIA_MALFORMED"),
        "{err}"
    );
}

/// M3.1: the exact/regex/path index behind `search.*` on a real repository:
/// hits carry path/line/column/span and are bound to the index revision; the
/// index excludes generated and ignored paths; a write through change.apply
/// refreshes exactly the changed path at the new workspace revision; results
/// are bounded; a bad regex is a typed failure.
#[tokio::test]
async fn m3_1_exact_regex_path_index_serves_bounded_revision_bound_hits_and_refreshes_on_writes() {
    let (repo, root) = plain_repo(&[
        (
            "src.rs",
            "fn compute_total() {}\nfn other() { compute_total() }\n",
        ),
        ("notes.md", "compute_total docs\n"),
    ]);
    std::fs::create_dir_all(repo.path().join("target")).unwrap();
    std::fs::write(repo.path().join("target/gen.rs"), "compute_total\n").unwrap();
    let dir = tempfile::tempdir().unwrap();
    let core = CoreProcess::spawn(dir.path());
    let mut c = core.client().await;
    let (session, _) = create_session(&mut c, id16(0xA0)).await;
    let g = lease_for(&session);
    let task = create_task_with_profile(&mut c, &session, g, &root, 0xA1, "local_trusted").await;
    let r = invoke_tool(
        &mut c,
        &task,
        g,
        0xA2,
        0xC1,
        "search.exact",
        r#"{"query":"compute_total"}"#,
    )
    .await;
    assert_eq!(r.status, "SUCCESS", "{r:?}");
    let so: serde_json::Value = serde_json::from_str(&r.structured_output_json).unwrap();
    let hits = so["hits"].as_array().unwrap();
    let where_: Vec<(String, u64, u64)> = hits
        .iter()
        .map(|h| {
            (
                h["path"].as_str().unwrap().into(),
                h["line"].as_u64().unwrap(),
                h["column"].as_u64().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        where_,
        vec![
            ("notes.md".into(), 1, 1),
            ("src.rs".into(), 1, 4),
            ("src.rs".into(), 2, 14)
        ],
        "{so}"
    );
    let rev0 = so["index_revision"].as_u64().unwrap();
    assert_eq!(rev0, so["workspace_revision"].as_u64().unwrap());
    assert!(hits.iter().all(|h| h["index_revision"] == rev0 && h["content_hash"].as_str().unwrap().len() == 64));
    assert!(
        !so["truncated"].as_bool().unwrap() && so["indexed_files"].as_u64().unwrap() == 2,
        "generated target/ is not indexed: {so}"
    );
    // A write refreshes the changed path at the new revision; the hit moves with it.
    let r = invoke_tool(&mut c, &task, g, 0xA3, 0xC2, "change.apply", r#"{"path":"src.rs","op":"edit","text_edits":[{"old":"fn other() { compute_total() }","new":"fn other() { compute_sum() }"}]}"#).await;
    assert_eq!(r.status, "SUCCESS", "{r:?}");
    let r = invoke_tool(
        &mut c,
        &task,
        g,
        0xA4,
        0xC3,
        "search.exact",
        r#"{"query":"compute_total"}"#,
    )
    .await;
    let so: serde_json::Value = serde_json::from_str(&r.structured_output_json).unwrap();
    let paths: Vec<&str> = so["hits"]
        .as_array()
        .unwrap()
        .iter()
        .map(|h| h["path"].as_str().unwrap())
        .collect();
    assert_eq!(paths, vec!["notes.md", "src.rs"], "{so}");
    assert_eq!(so["index_revision"].as_u64().unwrap(), rev0 + 1);
    let src_hit = &so["hits"][1];
    assert_eq!(
        src_hit["index_revision"].as_u64().unwrap(),
        rev0 + 1,
        "the changed file re-entered at the new revision"
    );
    assert_eq!(
        so["hits"][0]["index_revision"].as_u64().unwrap(),
        rev0,
        "the untouched file kept its revision"
    );
    let r = invoke_tool(
        &mut c,
        &task,
        g,
        0xA5,
        0xC4,
        "search.regex",
        r#"{"query":"fn \\w+\\(\\)","path_glob":"*.rs","max_hits":1}"#,
    )
    .await;
    let so: serde_json::Value = serde_json::from_str(&r.structured_output_json).unwrap();
    assert!(
        so["hits"].as_array().unwrap().len() == 1 && so["truncated"] == true,
        "{so}"
    );
    let r = invoke_tool(
        &mut c,
        &task,
        g,
        0xA6,
        0xC5,
        "search.regex",
        r#"{"query":"("}"#,
    )
    .await;
    assert_eq!(
        (r.status.as_str(), r.error_code.as_str()),
        ("APPLICATION_FAILURE", "BAD_REGEX"),
        "{r:?}"
    );
    let r = invoke_tool(
        &mut c,
        &task,
        g,
        0xA7,
        0xC6,
        "search.paths",
        r#"{"query":"**/*.rs"}"#,
    )
    .await;
    let so: serde_json::Value = serde_json::from_str(&r.structured_output_json).unwrap();
    assert_eq!(
        so["paths"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p["path"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["src.rs"],
        "{so}"
    );
    // The search tools are in the compiled surface of the task.
    let tools = list_tools(&mut c, 0xA8, Some(task.clone())).await;
    assert!(
        ["search.exact", "search.regex", "search.paths"]
            .iter()
            .all(|n| tools.iter().any(|t| t.0 == *n))
    );
}

/// M3.2: `search.lexical` ranks files by BM25 over the same indexed file set
/// as the exact index, bound to the index revision, and a write refreshes the
/// changed file's document.
#[tokio::test]
async fn m3_2_bm25_lexical_index_ranks_files_and_refreshes_on_writes() {
    let (_repo, root) = plain_repo(&[
        (
            "dense.rs",
            "fn compute_total() { compute_total_inner(); }\nfn compute_total_inner() {}\n",
        ),
        ("sparse.md", "one mention of compute_total\n"),
        ("none.txt", "unrelated\n"),
    ]);
    let dir = tempfile::tempdir().unwrap();
    let core = CoreProcess::spawn(dir.path());
    let mut c = core.client().await;
    let (session, _) = create_session(&mut c, id16(0xB0)).await;
    let g = lease_for(&session);
    let task = create_task_with_profile(&mut c, &session, g, &root, 0xB1, "local_trusted").await;
    let r = invoke_tool(
        &mut c,
        &task,
        g,
        0xB2,
        0xC1,
        "search.lexical",
        r#"{"query":"compute total"}"#,
    )
    .await;
    assert_eq!(r.status, "SUCCESS", "{r:?}");
    let so: serde_json::Value = serde_json::from_str(&r.structured_output_json).unwrap();
    let paths: Vec<&str> = so["hits"]
        .as_array()
        .unwrap()
        .iter()
        .map(|h| h["path"].as_str().unwrap())
        .collect();
    assert_eq!(paths, vec!["dense.rs", "sparse.md"], "{so}");
    assert!(so["hits"][0]["score"].as_f64().unwrap() > so["hits"][1]["score"].as_f64().unwrap());
    let rev0 = so["index_revision"].as_u64().unwrap();
    let r = invoke_tool(
        &mut c,
        &task,
        g,
        0xB3,
        0xC2,
        "change.apply",
        r#"{"path":"sparse.md","op":"replace","content":"nothing here now\n"}"#,
    )
    .await;
    assert_eq!(r.status, "SUCCESS", "{r:?}");
    let r = invoke_tool(
        &mut c,
        &task,
        g,
        0xB4,
        0xC3,
        "search.lexical",
        r#"{"query":"compute total"}"#,
    )
    .await;
    let so: serde_json::Value = serde_json::from_str(&r.structured_output_json).unwrap();
    let paths: Vec<&str> = so["hits"]
        .as_array()
        .unwrap()
        .iter()
        .map(|h| h["path"].as_str().unwrap())
        .collect();
    assert_eq!(paths, vec!["dense.rs"], "{so}");
    assert_eq!(so["index_revision"].as_u64().unwrap(), rev0 + 1);
    let r = invoke_tool(
        &mut c,
        &task,
        g,
        0xB5,
        0xC4,
        "search.lexical",
        r#"{"query":"nothing"}"#,
    )
    .await;
    let so: serde_json::Value = serde_json::from_str(&r.structured_output_json).unwrap();
    assert_eq!(
        so["hits"][0]["path"], "sparse.md",
        "the rewritten file is searchable at once: {so}"
    );
}

/// M3.3: `search.symbols` serves tree-sitter definitions for the Alpha
/// languages with kind, container, lines, span and revisions, and a write
/// refreshes the changed file's symbols.
#[tokio::test]
async fn m3_3_tree_sitter_symbol_index_serves_definitions_and_refreshes_on_writes() {
    let (_repo, root) = plain_repo(&[
        (
            "cart.rs",
            "pub struct Cart;\nimpl Cart {\n    pub fn total(&self) -> u32 { 1 }\n}\n",
        ),
        (
            "cart.py",
            "class Cart:\n    def total(self):\n        return 1\n",
        ),
        (
            "cart.ts",
            "export class Cart { total(): number { return 1 } }\n",
        ),
    ]);
    let dir = tempfile::tempdir().unwrap();
    let core = CoreProcess::spawn(dir.path());
    let mut c = core.client().await;
    let (session, _) = create_session(&mut c, id16(0xC0)).await;
    let g = lease_for(&session);
    let task = create_task_with_profile(&mut c, &session, g, &root, 0xC1, "local_trusted").await;
    let r = invoke_tool(
        &mut c,
        &task,
        g,
        0xC2,
        0xD1,
        "search.symbols",
        r#"{"query":"total"}"#,
    )
    .await;
    assert_eq!(r.status, "SUCCESS", "{r:?}");
    let so: serde_json::Value = serde_json::from_str(&r.structured_output_json).unwrap();
    let got: Vec<(String, String, String, String)> = so["symbols"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| {
            (
                s["path"].as_str().unwrap().into(),
                s["kind"].as_str().unwrap().into(),
                s["container"].as_str().unwrap_or("").into(),
                s["language"].as_str().unwrap().into(),
            )
        })
        .collect();
    assert_eq!(
        got,
        vec![
            (
                "cart.py".into(),
                "function".into(),
                "Cart".into(),
                "python".into()
            ),
            (
                "cart.rs".into(),
                "function".into(),
                "Cart".into(),
                "rust".into()
            ),
            (
                "cart.ts".into(),
                "method".into(),
                "Cart".into(),
                "typescript".into()
            )
        ],
        "{so}"
    );
    let rev0 = so["index_revision"].as_u64().unwrap();
    assert!(
        so["symbols"]
            .as_array()
            .unwrap()
            .iter()
            .all(|s| s["index_revision"] == rev0
                && s["line_start"].as_u64().unwrap() >= 1
                && s["content_hash"].as_str().unwrap().len() == 64)
    );
    let r = invoke_tool(
        &mut c,
        &task,
        g,
        0xC3,
        0xD2,
        "search.symbols",
        r#"{"query":"Ca","prefix":true,"kind":"class"}"#,
    )
    .await;
    let so: serde_json::Value = serde_json::from_str(&r.structured_output_json).unwrap();
    assert_eq!(
        so["symbols"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s["path"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["cart.py", "cart.ts"],
        "{so}"
    );
    let r = invoke_tool(
        &mut c,
        &task,
        g,
        0xC4,
        0xD3,
        "change.apply",
        r#"{"path":"cart.rs","op":"replace","content":"pub fn subtotal() -> u32 { 1 }\n"}"#,
    )
    .await;
    assert_eq!(r.status, "SUCCESS", "{r:?}");
    let r = invoke_tool(
        &mut c,
        &task,
        g,
        0xC5,
        0xD4,
        "search.symbols",
        r#"{"query":"total"}"#,
    )
    .await;
    let so: serde_json::Value = serde_json::from_str(&r.structured_output_json).unwrap();
    assert_eq!(
        so["symbols"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s["path"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["cart.py", "cart.ts"],
        "{so}"
    );
    let r = invoke_tool(
        &mut c,
        &task,
        g,
        0xC6,
        0xD5,
        "search.symbols",
        r#"{"query":"subtotal"}"#,
    )
    .await;
    let so: serde_json::Value = serde_json::from_str(&r.structured_output_json).unwrap();
    assert_eq!(
        so["symbols"][0]["index_revision"].as_u64().unwrap(),
        rev0 + 1,
        "{so}"
    );
}

/// M3.4: the headless language-service bridge through the tools on a real
/// fixture: pyright diagnostics for a seeded error (with the file revision),
/// document symbols, references and a definition; an unsupported language is
/// a typed refusal, never a guess. The node servers come from the repository's
/// node_modules (the Core resolves them from the workspace tree or its own).
#[tokio::test]
async fn m3_4_headless_language_service_bridge_serves_diagnostics_symbols_references_and_definitions()
 {
    let (repo, root) = fixture_repo("python-service");
    std::fs::write(
        repo.path().join("seeded.py"),
        "from service import total_cents\n\nx: int = \"text\"\ny = total_cents(1, 2)\n",
    )
    .unwrap();
    // Point the Core at the repository's node_modules for pyright.
    let nm = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../node_modules")
        .canonicalize()
        .unwrap();
    let nm_s = nm.to_string_lossy().into_owned();
    let env = [("MODBIT_NODE_MODULES", nm_s.as_str())];
    let dir = tempfile::tempdir().unwrap();
    let core = CoreProcess::spawn_with_env(dir.path(), &env);
    let mut c = core.client().await;
    let (session, _) = create_session(&mut c, id16(0xD0)).await;
    let g = lease_for(&session);
    let task = create_task_with_profile(&mut c, &session, g, &root, 0xD1, "local_trusted").await;
    let r = invoke_tool(
        &mut c,
        &task,
        g,
        0xD2,
        0xE1,
        "lsp.diagnostics",
        r#"{"path":"seeded.py"}"#,
    )
    .await;
    assert_eq!(r.status, "SUCCESS", "{r:?}");
    let so: serde_json::Value = serde_json::from_str(&r.structured_output_json).unwrap();
    let d = so["diagnostics"].as_array().unwrap();
    assert!(
        d.iter()
            .any(|x| x["severity"] == "error" && x["range"]["start"]["line"] == 2),
        "{so}"
    );
    assert_eq!(
        (so["server"].as_str(), so["language"].as_str()),
        (Some("pyright"), Some("python"))
    );
    assert_eq!(so["content_hash"].as_str().unwrap().len(), 64);
    let r = invoke_tool(
        &mut c,
        &task,
        g,
        0xD3,
        0xE2,
        "lsp.symbols",
        r#"{"path":"service.py"}"#,
    )
    .await;
    let so: serde_json::Value = serde_json::from_str(&r.structured_output_json).unwrap();
    let names: Vec<&str> = so["symbols"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["name"].as_str().unwrap())
        .collect();
    assert!(
        names.contains(&"parse_quantity") && names.contains(&"total_cents"),
        "{names:?}"
    );
    let r = invoke_tool(
        &mut c,
        &task,
        g,
        0xD4,
        0xE3,
        "lsp.references",
        r#"{"path":"seeded.py","line":3,"character":6}"#,
    )
    .await;
    let so: serde_json::Value = serde_json::from_str(&r.structured_output_json).unwrap();
    let paths: Vec<&str> = so["locations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l["path"].as_str().unwrap())
        .collect();
    assert!(
        paths.contains(&"service.py") && paths.contains(&"seeded.py"),
        "{so}"
    );
    let r = invoke_tool(
        &mut c,
        &task,
        g,
        0xD5,
        0xE4,
        "lsp.definition",
        r#"{"path":"seeded.py","line":3,"character":6}"#,
    )
    .await;
    let so: serde_json::Value = serde_json::from_str(&r.structured_output_json).unwrap();
    assert_eq!(so["locations"][0]["path"], "service.py", "{so}");
    // A fixed seed no longer reports the error at the new revision.
    let r = invoke_tool(&mut c, &task, g, 0xD6, 0xE5, "change.apply", r#"{"path":"seeded.py","op":"replace","content":"from service import total_cents\n\nx: int = 3\ny = total_cents(1, 2)\n"}"#).await;
    assert_eq!(r.status, "SUCCESS", "{r:?}");
    let r = invoke_tool(
        &mut c,
        &task,
        g,
        0xD7,
        0xE6,
        "lsp.diagnostics",
        r#"{"path":"seeded.py"}"#,
    )
    .await;
    let so: serde_json::Value = serde_json::from_str(&r.structured_output_json).unwrap();
    assert!(
        so["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .all(|x| x["severity"] != "error"),
        "{so}"
    );
    std::fs::write(repo.path().join("main.go"), "package main\n").unwrap();
    let r = invoke_tool(
        &mut c,
        &task,
        g,
        0xD8,
        0xE7,
        "lsp.diagnostics",
        r#"{"path":"main.go"}"#,
    )
    .await;
    assert_eq!(
        (r.status.as_str(), r.error_code.as_str()),
        ("APPLICATION_FAILURE", "LANGUAGE_SERVICE_UNAVAILABLE"),
        "{r:?}"
    );
}

/// M3.5: `search.semantic` serves nearest chunks with the embedder id and the
/// embedding generation; a write re-embeds only the changed file's chunks at
/// the new generation and nothing stays queued afterwards.
#[tokio::test]
async fn m3_5_semantic_chunk_index_serves_nearest_chunks_and_reembeds_only_changed_files() {
    let (_repo, root) = plain_repo(&[
        (
            "cart.rs",
            "pub fn total_cents(quantity: u32, unit: u32) -> u32 {\n    quantity * unit\n}\n",
        ),
        ("notes.md", "shipping notes\n"),
    ]);
    let dir = tempfile::tempdir().unwrap();
    let core = CoreProcess::spawn(dir.path());
    let mut c = core.client().await;
    let (session, _) = create_session(&mut c, id16(0xE0)).await;
    let g = lease_for(&session);
    let task = create_task_with_profile(&mut c, &session, g, &root, 0xE1, "local_trusted").await;
    let r = invoke_tool(
        &mut c,
        &task,
        g,
        0xE2,
        0xF1,
        "search.semantic",
        r#"{"query":"total_cents quantity unit","max_hits":2}"#,
    )
    .await;
    assert_eq!(r.status, "SUCCESS", "{r:?}");
    let so: serde_json::Value = serde_json::from_str(&r.structured_output_json).unwrap();
    assert_eq!(so["hits"][0]["chunk"]["path"], "cart.rs", "{so}");
    assert_eq!(so["hits"][0]["chunk"]["label"], "total_cents");
    assert_eq!(so["embedder"], "hashing-v1");
    let gen0 = so["embedding_generation"].as_u64().unwrap();
    assert!(so["stale_paths"].as_array().unwrap().is_empty());
    let r = invoke_tool(
        &mut c,
        &task,
        g,
        0xE3,
        0xF2,
        "change.apply",
        r#"{"path":"notes.md","op":"replace","content":"refund policy for money back\n"}"#,
    )
    .await;
    assert_eq!(r.status, "SUCCESS", "{r:?}");
    let r = invoke_tool(
        &mut c,
        &task,
        g,
        0xE4,
        0xF3,
        "search.semantic",
        r#"{"query":"refund money","max_hits":2}"#,
    )
    .await;
    let so: serde_json::Value = serde_json::from_str(&r.structured_output_json).unwrap();
    assert_eq!(so["hits"][0]["chunk"]["path"], "notes.md", "{so}");
    assert_eq!(
        so["hits"][0]["chunk"]["embedded_at_revision"]
            .as_u64()
            .unwrap(),
        gen0 + 1
    );
    assert_eq!(so["embedding_generation"].as_u64().unwrap(), gen0 + 1);
    assert!(
        so["stale_paths"].as_array().unwrap().is_empty(),
        "flushed on the write"
    );
    let r = invoke_tool(
        &mut c,
        &task,
        g,
        0xE5,
        0xF4,
        "search.semantic",
        r#"{"query":"total_cents","max_hits":1}"#,
    )
    .await;
    let so: serde_json::Value = serde_json::from_str(&r.structured_output_json).unwrap();
    assert_eq!(
        so["hits"][0]["chunk"]["embedded_at_revision"]
            .as_u64()
            .unwrap(),
        gen0,
        "the untouched file kept its vectors"
    );
}

/// M3.6: `search.graph` serves import/importer edges from the real parse,
/// co-change and ownership from the real Git history, test mappings and the
/// worktree's changed lines; a write refreshes the graph at the new revision.
#[tokio::test]
async fn m3_6_evidence_graph_serves_imports_history_changed_lines_tests_and_verification_evidence()
{
    let (repo, root) = plain_repo(&[(
        "Cargo.toml",
        "[package]\nname = \"mylib\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )]);
    for (p, c) in [
        ("src/lib.rs", "pub mod util;\npub mod net;\n"),
        (
            "src/util.rs",
            "use crate::net::Sock;\npub fn f() -> Sock {\n    Sock\n}\n",
        ),
        ("src/net.rs", "pub struct Sock;\n"),
        (
            "tests/util_test.rs",
            "use mylib::util::f;\n#[test]\nfn works() {\n    let _ = f();\n}\n",
        ),
    ] {
        let path = repo.path().join(p);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, c).unwrap();
    }
    for args in [
        vec!["add", "-A"],
        vec![
            "-c",
            "user.name=ann",
            "-c",
            "user.email=a@e",
            "commit",
            "-q",
            "-m",
            "crate",
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
    let dir = tempfile::tempdir().unwrap();
    let core = CoreProcess::spawn(dir.path());
    let mut c = core.client().await;
    let (session, _) = create_session(&mut c, id16(0xF0)).await;
    let g = lease_for(&session);
    let task = create_task_with_profile(&mut c, &session, g, &root, 0xF1, "local_trusted").await;
    let r = invoke_tool(
        &mut c,
        &task,
        g,
        0xF2,
        0xE1,
        "search.graph",
        r#"{"path":"src/util.rs","relation":"all","depth":2}"#,
    )
    .await;
    assert_eq!(r.status, "SUCCESS", "{r:?}");
    let so: serde_json::Value = serde_json::from_str(&r.structured_output_json).unwrap();
    let v = &so["graph"];
    let rev0 = v["revision"].as_u64().unwrap();
    assert_eq!(v["imports"], serde_json::json!([["src/net.rs", 1]]), "{so}");
    assert_eq!(
        v["importers"],
        serde_json::json!([["src/lib.rs", 1], ["tests/util_test.rs", 1]])
    );
    assert_eq!(
        v["cochange"],
        serde_json::json!([
            ["src/lib.rs", 1],
            ["src/net.rs", 1],
            ["tests/util_test.rs", 1]
        ])
    );
    assert_eq!(v["owners"], serde_json::json!([["ann", 1]]));
    assert_eq!(v["commits"][0]["subject"], "crate");
    assert_eq!(v["commits"][0]["author"], "ann");
    assert_eq!(v["tests"], serde_json::json!(["tests/util_test.rs"]));
    assert_eq!(v["changed_lines"], serde_json::json!([]));
    assert_eq!(v["evidence"], serde_json::json!([]));
    // A write: the changed lines show up and the graph moved to the new revision.
    let r = invoke_tool(
        &mut c,
        &task,
        g,
        0xF3,
        0xE2,
        "change.apply",
        r#"{"path":"src/util.rs","op":"replace","content":"pub fn f() -> u32 {\n    1\n}\n"}"#,
    )
    .await;
    assert_eq!(r.status, "SUCCESS", "{r:?}");
    let r = invoke_tool(
        &mut c,
        &task,
        g,
        0xF4,
        0xE3,
        "search.graph",
        r#"{"path":"src/util.rs","relation":"all"}"#,
    )
    .await;
    assert_eq!(r.status, "SUCCESS", "{r:?}");
    let so: serde_json::Value = serde_json::from_str(&r.structured_output_json).unwrap();
    let v = &so["graph"];
    assert!(v["revision"].as_u64().unwrap() > rev0, "{so}");
    assert_eq!(
        v["imports"],
        serde_json::json!([]),
        "the net import is gone: {so}"
    );
    // Lines 1–2 changed; the closing brace on line 3 is unchanged context.
    assert_eq!(v["changed_lines"], serde_json::json!([[1, 2]]), "{so}");
    assert_eq!(
        v["importers"],
        serde_json::json!([["src/lib.rs", 1], ["tests/util_test.rs", 1]])
    );
    // Relation filters and an unknown path.
    let r = invoke_tool(
        &mut c,
        &task,
        g,
        0xF5,
        0xE4,
        "search.graph",
        r#"{"path":"src/net.rs","relation":"tests"}"#,
    )
    .await;
    let so: serde_json::Value = serde_json::from_str(&r.structured_output_json).unwrap();
    assert_eq!(
        so["graph"]["tests"],
        serde_json::json!([]),
        "util no longer reaches net: {so}"
    );
    assert_eq!(so["graph"]["imports"], serde_json::json!([]));
    let r = invoke_tool(
        &mut c,
        &task,
        g,
        0xF6,
        0xE5,
        "search.graph",
        r#"{"path":"","relation":"all"}"#,
    )
    .await;
    assert_eq!(r.error_code, "PATH_REQUIRED", "{r:?}");
}

async fn retrieve_plan(
    c: &mut Client,
    task: &Id,
    g: Option<u64>,
    a: u8,
    b: u8,
    args: &str,
) -> serde_json::Value {
    let r = invoke_tool(c, task, g, a, b, "search.retrieve", args).await;
    assert_eq!(r.status, "SUCCESS", "{r:?}");
    serde_json::from_str(&r.structured_output_json).unwrap()
}

/// M3.7: `search.retrieve` plans from the query's features — a known identifier
/// is served at L0 without touching the hybrid indexes, unknown wording starts
/// at L1, a miss escalates only while coverage is short, a structural intent
/// expands through the graph — and every result is fused with the boosts named.
#[tokio::test]
async fn m3_7_retrieval_planner_starts_cheap_escalates_on_short_coverage_and_fuses_with_boosts() {
    let (repo, root) = plain_repo(&[(
        "Cargo.toml",
        "[package]\nname = \"cart\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )]);
    for (p, c) in [
        (
            "src/lib.rs",
            "pub mod money;\n\n/// Total of the cart in cents.\npub fn compute_total(quantity: u32, unit_cents: u32) -> u32 {\n    money::round(quantity * unit_cents)\n}\n",
        ),
        (
            "src/money.rs",
            "pub fn round(cents: u32) -> u32 {\n    cents\n}\n",
        ),
        (
            "src/main.rs",
            "use cart::compute_total;\nfn main() {\n    println!(\"{}\", compute_total(2, 150));\n}\n",
        ),
        (
            "tests/total_test.rs",
            "use cart::compute_total;\n#[test]\nfn totals_multiply() {\n    assert_eq!(compute_total(2, 150), 300);\n}\n",
        ),
        (
            "README.md",
            "# cart\nThe shopping cart computes order totals from quantity and unit price.\n",
        ),
    ] {
        let path = repo.path().join(p);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, c).unwrap();
    }
    for args in [
        vec!["add", "-A"],
        vec![
            "-c",
            "user.name=ann",
            "-c",
            "user.email=a@e",
            "commit",
            "-q",
            "-m",
            "cart",
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
    let dir = tempfile::tempdir().unwrap();
    let core = CoreProcess::spawn(dir.path());
    let mut c = core.client().await;
    let (session, _) = create_session(&mut c, id16(0xF7)).await;
    let g = lease_for(&session);
    let task = create_task_with_profile(&mut c, &session, g, &root, 0xF8, "local_trusted").await;
    // L0 for a known identifier: no hybrid step ran.
    let so = retrieve_plan(&mut c, &task, g, 0xF9, 0xD1, r#"{"query":"compute_total"}"#).await;
    assert_eq!(so["plan"]["started_at"], "L0Exact", "{so}");
    assert_eq!(so["plan"]["ended_at"], "L0Exact");
    assert_eq!(so["plan"]["escalations"], serde_json::json!([]));
    let sources: Vec<&str> = so["plan"]["steps"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["source"].as_str().unwrap())
        .collect();
    assert_eq!(sources, ["exact", "symbols"], "{so}");
    assert_eq!(so["hits"][0]["path"], "src/lib.rs");
    assert_eq!(so["hits"][0]["lines"], serde_json::json!([4, 6]));
    assert!(
        so["hits"][0]["reasons"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!("exact_symbol")),
        "{so}"
    );
    assert_eq!(so["plan"]["index_revision"], so["index_revision"]);
    // A worktree edit: the changed file is boosted as fresh with changed lines.
    let r = invoke_tool(
        &mut c,
        &task,
        g,
        0xFA,
        0xD2,
        "change.apply",
        r#"{"path":"src/money.rs","op":"replace","content":"pub fn round(cents: u32) -> u32 {\n    cents / 1 * 1\n}\n"}"#,
    )
    .await;
    assert_eq!(r.status, "SUCCESS", "{r:?}");
    let so = retrieve_plan(&mut c, &task, g, 0xFB, 0xD3, r#"{"query":"round"}"#).await;
    let money = so["hits"]
        .as_array()
        .unwrap()
        .iter()
        .find(|h| {
            h["path"] == "src/money.rs"
                && h["sources"]
                    .as_array()
                    .unwrap()
                    .contains(&serde_json::json!("symbols"))
        })
        .unwrap_or_else(|| panic!("{so}"));
    let reasons = money["reasons"].as_array().unwrap();
    assert!(
        reasons.contains(&serde_json::json!("fresh_in_worktree"))
            && reasons.contains(&serde_json::json!("changed_lines")),
        "{so}"
    );
    // Unknown wording: L1 with the embedding generation declared; a miss escalates once.
    let so = retrieve_plan(
        &mut c,
        &task,
        g,
        0xFC,
        0xD4,
        r#"{"query":"how are order totals computed"}"#,
    )
    .await;
    assert_eq!(so["plan"]["started_at"], "L1Hybrid", "{so}");
    assert!(so["plan"]["embedding_generation"].is_u64());
    assert!(
        so["hits"]
            .as_array()
            .unwrap()
            .iter()
            .any(|h| h["path"] == "README.md"),
        "{so}"
    );
    let so = retrieve_plan(
        &mut c,
        &task,
        g,
        0xFD,
        0xD5,
        r#"{"query":"zzz_nothing_here"}"#,
    )
    .await;
    assert_eq!(so["plan"]["escalations"][0]["from"], "L0Exact", "{so}");
    assert_eq!(so["plan"]["escalations"][0]["to"], "L1Hybrid");
    assert_eq!(so["plan"]["ended_at"], "L1Hybrid");
    // Structural intent: the graph expansion names the caller with its distance.
    let so = retrieve_plan(
        &mut c,
        &task,
        g,
        0xFE,
        0xD6,
        r#"{"query":"callers of compute_total","max_hits":10}"#,
    )
    .await;
    assert_eq!(so["plan"]["ended_at"], "L2Structural", "{so}");
    let main = so["hits"]
        .as_array()
        .unwrap()
        .iter()
        .find(|h| {
            h["path"] == "src/main.rs"
                && h["reasons"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|r| r.as_str().unwrap().starts_with("dependency_distance:"))
        })
        .unwrap_or_else(|| panic!("{so}"));
    assert!(
        main["sources"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!("graph.importers")),
        "{so}"
    );
    assert!(so["hits"].as_array().unwrap().len() <= 10);
    let r = invoke_tool(
        &mut c,
        &task,
        g,
        0xFF,
        0xD7,
        "search.retrieve",
        r#"{"query":"  "}"#,
    )
    .await;
    assert_eq!(r.error_code, "QUERY_REQUIRED", "{r:?}");
}

/// M3.8: `context.pack` compiles a budgeted, provenance-complete Context Pack
/// (required paths first as critical entries, then utility; never over budget;
/// omissions summarised; durable pack object) and records every entry in the
/// task's Context Ledger; a later read of a packed path at that revision is a
/// recorded use, a stale-revision read is not.
#[tokio::test]
async fn m3_8_context_pack_packs_under_budget_with_provenance_and_the_ledger_records_use() {
    let (repo, root) = plain_repo(&[(
        "Cargo.toml",
        "[package]\nname = \"cart\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )]);
    for (p, c) in [
        (
            "src/lib.rs",
            "pub mod money;\n\n/// Total of the cart in cents.\npub fn compute_total(quantity: u32, unit_cents: u32) -> u32 {\n    money::round(quantity * unit_cents)\n}\n",
        ),
        (
            "src/money.rs",
            "pub fn round(cents: u32) -> u32 {\n    cents\n}\n",
        ),
        (
            "src/main.rs",
            "use cart::compute_total;\nfn main() {\n    println!(\"{}\", compute_total(2, 150));\n}\n",
        ),
        (
            "README.md",
            "# cart\nThe cart computes order totals from quantity and unit price.\n",
        ),
        ("CONSTRAINTS.md", "Never round totals up.\n"),
    ] {
        let path = repo.path().join(p);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, c).unwrap();
    }
    for args in [
        vec!["add", "-A"],
        vec![
            "-c",
            "user.name=ann",
            "-c",
            "user.email=a@e",
            "commit",
            "-q",
            "-m",
            "cart",
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
    let dir = tempfile::tempdir().unwrap();
    let core = CoreProcess::spawn(dir.path());
    let mut c = core.client().await;
    let (session, _) = create_session(&mut c, id16(0xC0)).await;
    let g = lease_for(&session);
    let task = create_task_with_profile(&mut c, &session, g, &root, 0xC1, "local_trusted").await;
    let r = invoke_tool(
        &mut c,
        &task,
        g,
        0xC2,
        0xB1,
        "context.pack",
        r#"{"query":"compute_total","token_budget":60,"required_paths":["CONSTRAINTS.md"]}"#,
    )
    .await;
    assert_eq!(r.status, "SUCCESS", "{r:?}");
    let so: serde_json::Value = serde_json::from_str(&r.structured_output_json).unwrap();
    let pack = &so["pack"];
    let budget = pack["token_budget"].as_u64().unwrap();
    assert_eq!(budget, 60);
    assert!(pack["token_used"].as_u64().unwrap() <= budget, "{so}");
    assert_eq!(pack["compiler_version"], "context-pack-v1");
    assert_eq!(pack["token_estimator"], "bytes/4");
    let rev0 = pack["workspace_revision"].as_u64().unwrap();
    let entries = pack["entries"].as_array().unwrap();
    assert!(!entries.is_empty(), "{so}");
    assert_eq!(
        entries[0]["provenance"]["path"], "CONSTRAINTS.md",
        "task constraint first: {so}"
    );
    assert_eq!(entries[0]["reason"], "critical:task_constraint");
    assert_eq!(entries[0]["source_ref"], "workspace:CONSTRAINTS.md");
    for e in entries {
        assert_eq!(
            e["provenance"]["workspace_revision"].as_u64().unwrap(),
            rev0
        );
        assert!(e["provenance"]["content_hash"].is_string(), "{e}");
        assert_eq!(e["provenance"]["excerpt_hash"].as_str().unwrap().len(), 64);
        assert!(e["token_cost"].as_u64().unwrap() >= 1);
    }
    assert!(pack["complete"].as_bool().unwrap());
    assert!(
        pack["omitted_summary"]["count"].as_u64().unwrap() >= 1,
        "the budget left something out: {so}"
    );
    // The pack is a durable object.
    let pack_ref = so["pack_ref"].as_str().unwrap();
    let stored = read_object(&mut c, id16(0xC3), pack_ref).await;
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&stored).unwrap()["pack_id"],
        pack["pack_id"]
    );
    // Ledger: every entry injected, none used yet.
    let r = invoke_tool(&mut c, &task, g, 0xC4, 0xB2, "context.ledger", "{}").await;
    assert_eq!(r.status, "SUCCESS", "{r:?}");
    let so: serde_json::Value = serde_json::from_str(&r.structured_output_json).unwrap();
    let stubs_n = pack["stubs"].as_array().unwrap().len();
    assert_eq!(
        so["injected"].as_u64().unwrap(),
        (entries.len() + stubs_n) as u64,
        "entries and stubs are injected: {so}"
    );
    assert_eq!(so["used"], 0);
    assert!(so["ledger_ref"].is_string());
    // A read of a packed path at the retrieved revision is a use.
    let r = invoke_tool(
        &mut c,
        &task,
        g,
        0xC5,
        0xB3,
        "fs.read",
        r#"{"path":"CONSTRAINTS.md"}"#,
    )
    .await;
    assert_eq!(r.status, "SUCCESS", "{r:?}");
    let r = invoke_tool(&mut c, &task, g, 0xC6, 0xB4, "context.ledger", "{}").await;
    let so: serde_json::Value = serde_json::from_str(&r.structured_output_json).unwrap();
    assert_eq!(so["used"], 1, "{so}");
    let used_entry = so["ledger"]["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["path"] == "CONSTRAINTS.md")
        .unwrap();
    assert_eq!(used_entry["used"]["tool_name"], "fs.read");
    assert_eq!(
        used_entry["used"]["tool_call_id"].as_str().unwrap().len(),
        36
    );
    // A write moves the revision; a later read of another packed path is stale, not a use.
    let other = entries
        .iter()
        .map(|e| e["provenance"]["path"].as_str().unwrap())
        .find(|p| *p != "CONSTRAINTS.md")
        .unwrap_or_else(|| panic!("{pack}"));
    let r = invoke_tool(
        &mut c,
        &task,
        g,
        0xC7,
        0xB5,
        "change.apply",
        r##"{"path":"README.md","op":"replace","content":"# cart\nchanged\n"}"##,
    )
    .await;
    assert_eq!(r.status, "SUCCESS", "{r:?}");
    let r = invoke_tool(
        &mut c,
        &task,
        g,
        0xC8,
        0xB6,
        "fs.read",
        &format!(r#"{{"path":"{other}"}}"#),
    )
    .await;
    assert_eq!(r.status, "SUCCESS", "{r:?}");
    let r = invoke_tool(&mut c, &task, g, 0xC9, 0xB7, "context.ledger", "{}").await;
    let so: serde_json::Value = serde_json::from_str(&r.structured_output_json).unwrap();
    let readme_used = so["ledger"]["entries"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["path"] == "README.md" && e["used"].is_object())
        .count();
    assert_eq!(
        so["used"].as_u64().unwrap(),
        1 + readme_used as u64,
        "the stale read of {other} is not a use: {so}"
    );
    let r = invoke_tool(
        &mut c,
        &task,
        g,
        0xCA,
        0xB8,
        "context.pack",
        r#"{"query":"   "}"#,
    )
    .await;
    assert_eq!(r.error_code, "QUERY_REQUIRED", "{r:?}");
    // Read-through hydration (REQ-EV-0002/0170): the file is changed on disk
    // behind the index; the pack carries the current bytes, labelled as
    // rehydrated, with the current hash — stale bytes never reach the pack.
    std::fs::write(
        repo.path().join("src/money.rs"),
        "pub fn round(cents: u32) -> u32 {\n    cents + 0 // edited outside the tools\n}\n",
    )
    .unwrap();
    let r = invoke_tool(
        &mut c,
        &task,
        g,
        0xCB,
        0xB9,
        "context.pack",
        r#"{"query":"round","token_budget":400}"#,
    )
    .await;
    assert_eq!(r.status, "SUCCESS", "{r:?}");
    let so: serde_json::Value = serde_json::from_str(&r.structured_output_json).unwrap();
    let money = so["pack"]["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["provenance"]["path"] == "src/money.rs")
        .unwrap_or_else(|| panic!("{so}"));
    assert!(
        money["text"]
            .as_str()
            .unwrap()
            .contains("edited outside the tools"),
        "{so}"
    );
    assert_eq!(
        money["freshness"], "rehydrated_from_active_revision",
        "{so}"
    );
    let r = invoke_tool(
        &mut c,
        &task,
        g,
        0xCC,
        0xBA,
        "fs.read",
        r#"{"path":"src/money.rs"}"#,
    )
    .await;
    let read: serde_json::Value = serde_json::from_str(&r.structured_output_json).unwrap();
    assert_eq!(
        read["content_hash"], money["provenance"]["content_hash"],
        "{so}"
    );
    // Signature-only stubs (REQ-EV-0003/0167): a tight budget turns the
    // lower-ranked candidates into stubs with a hydration handle; hydrating one
    // through fs.read is the recorded use and returns the bytes the stub's
    // provenance names.
    let r = invoke_tool(
        &mut c,
        &task,
        g,
        0xCD,
        0xBB,
        "context.pack",
        r#"{"query":"compute_total","token_budget":70}"#,
    )
    .await;
    assert_eq!(r.status, "SUCCESS", "{r:?}");
    let so: serde_json::Value = serde_json::from_str(&r.structured_output_json).unwrap();
    let stubs = so["pack"]["stubs"].as_array().unwrap();
    assert!(!stubs.is_empty(), "{so}");
    assert!(so["pack"]["token_used"].as_u64().unwrap() <= 70);
    let stub = &stubs[0];
    let stub_path = stub["provenance"]["path"].as_str().unwrap().to_owned();
    assert_eq!(stub["hydrate"], format!("fs.read {stub_path}"));
    assert!(
        stub["hydrated_token_cost"].as_u64().unwrap() >= stub["token_cost"].as_u64().unwrap(),
        "{stub}"
    );
    let r = invoke_tool(&mut c, &task, g, 0xCE, 0xBC, "context.ledger", "{}").await;
    let before: serde_json::Value = serde_json::from_str(&r.structured_output_json).unwrap();
    let used_before = before["used"].as_u64().unwrap();
    assert!(
        before["ledger"]["entries"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["entry_id"] == stub["entry_id"] && e["stub"] == true),
        "{before}"
    );
    let r = invoke_tool(
        &mut c,
        &task,
        g,
        0xCF,
        0xBD,
        "fs.read",
        &format!(r#"{{"path":"{stub_path}"}}"#),
    )
    .await;
    assert_eq!(r.status, "SUCCESS", "{r:?}");
    let read: serde_json::Value = serde_json::from_str(&r.structured_output_json).unwrap();
    assert_eq!(
        read["content_hash"], stub["provenance"]["content_hash"],
        "hydration fidelity: {read}"
    );
    let r = invoke_tool(&mut c, &task, g, 0xD0, 0xBE, "context.ledger", "{}").await;
    let after: serde_json::Value = serde_json::from_str(&r.structured_output_json).unwrap();
    assert!(after["used"].as_u64().unwrap() > used_before, "{after}");
    // The same stub id can appear in an earlier pack (same path and span at
    // another revision); the latest record is the one this pack injected.
    let hydrated = after["ledger"]["entries"]
        .as_array()
        .unwrap()
        .iter()
        .rev()
        .find(|e| e["entry_id"] == stub["entry_id"])
        .unwrap();
    assert_eq!(hydrated["used"]["tool_name"], "fs.read", "{after}");
}

/// REQ-EV-0070 Diagnostic Change Window: the task's first look at a file is
/// its baseline (pre-existing noise); after a change, `window=changed`
/// evaluates only the changed lines and names the diagnostics that are new
/// against the baseline; the noise outside the window is counted, not shown.
#[tokio::test]
async fn qual_ev_0070_diagnostic_change_window_evaluates_only_the_changed_region() {
    let (repo, root) = fixture_repo("python-service");
    // Pre-existing noise: two type errors at the top of the file.
    let noisy = "from service import total_cents\n\nnoise_a: int = \"a\"\nnoise_b: int = \"b\"\n\n\ndef ok() -> int:\n    return total_cents(1, 2)\n";
    std::fs::write(repo.path().join("windowed.py"), noisy).unwrap();
    let nm = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../node_modules")
        .canonicalize()
        .unwrap();
    let nm_s = nm.to_string_lossy().into_owned();
    let env = [("MODBIT_NODE_MODULES", nm_s.as_str())];
    let dir = tempfile::tempdir().unwrap();
    let core = CoreProcess::spawn_with_env(dir.path(), &env);
    let mut c = core.client().await;
    let (session, _) = create_session(&mut c, id16(0x70)).await;
    let g = lease_for(&session);
    let task = create_task_with_profile(&mut c, &session, g, &root, 0x71, "local_trusted").await;
    // Baseline: the first look records the noise.
    let r = invoke_tool(
        &mut c,
        &task,
        g,
        0x72,
        0x61,
        "lsp.diagnostics",
        r#"{"path":"windowed.py"}"#,
    )
    .await;
    assert_eq!(r.status, "SUCCESS", "{r:?}");
    let so: serde_json::Value = serde_json::from_str(&r.structured_output_json).unwrap();
    let base_errors = so["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|d| d["severity"] == "error")
        .count();
    assert_eq!(base_errors, 2, "{so}");
    assert_eq!(so["window"]["kind"], "all");
    assert_eq!(
        so["window"]["baseline_count"].as_u64().unwrap() as usize,
        so["diagnostics"].as_array().unwrap().len()
    );
    // The change: a new function at the end with one new error.
    let changed = format!("{noisy}\n\ndef broken() -> int:\n    return \"not an int\"\n");
    let r = invoke_tool(
        &mut c,
        &task,
        g,
        0x73,
        0x62,
        "change.apply",
        &serde_json::json!({"path":"windowed.py","op":"replace","content":changed}).to_string(),
    )
    .await;
    assert_eq!(r.status, "SUCCESS", "{r:?}");
    let r = invoke_tool(
        &mut c,
        &task,
        g,
        0x74,
        0x63,
        "lsp.diagnostics",
        r#"{"path":"windowed.py","window":"changed"}"#,
    )
    .await;
    assert_eq!(r.status, "SUCCESS", "{r:?}");
    let so: serde_json::Value = serde_json::from_str(&r.structured_output_json).unwrap();
    assert_eq!(so["window"]["kind"], "changed", "{so}");
    let ranges = so["window"]["ranges"].as_array().unwrap();
    assert!(!ranges.is_empty(), "{so}");
    assert!(
        ranges.iter().all(|r| r[0].as_u64().unwrap() > 8),
        "only the appended lines: {so}"
    );
    let shown = so["diagnostics"].as_array().unwrap();
    assert!(
        shown
            .iter()
            .all(|d| d["range"]["start"]["line"].as_u64().unwrap() >= 9),
        "{so}"
    );
    let new = so["new_since_baseline"].as_array().unwrap();
    assert!(
        new.iter()
            .any(|d| d["severity"] == "error" && d["message"].as_str().unwrap().contains("int")),
        "{so}"
    );
    assert_eq!(
        so["outside_window_count"].as_u64().unwrap(),
        2,
        "the noise is counted, not shown: {so}"
    );
    assert_eq!(so["window"]["baseline_count"], 2);
    // An unchanged file has an empty window: nothing is evaluated, the noise is counted.
    let r = invoke_tool(
        &mut c,
        &task,
        g,
        0x75,
        0x64,
        "lsp.diagnostics",
        r#"{"path":"seeded.py"}"#,
    )
    .await;
    let _ = r;
    let r = invoke_tool(
        &mut c,
        &task,
        g,
        0x76,
        0x65,
        "lsp.diagnostics",
        r#"{"path":"windowed.py","window":"changed"}"#,
    )
    .await;
    let so: serde_json::Value = serde_json::from_str(&r.structured_output_json).unwrap();
    assert_eq!(
        so["window"]["baseline_count"], 2,
        "the baseline is the first look, not the latest: {so}"
    );
}

/// REQ-EV-0134 / 0177 / 0229: the model sees a stable core with schemas plus
/// `tool.search`; deferred tools are named by toolset only until discovered,
/// then projected with their schemas from the next turn; discovery cannot
/// reach a tool the profile denies; the lazy projection is measurably smaller
/// than the eager catalog; activation survives a Core restart via the event.
#[tokio::test]
async fn qual_ev_0134_deferred_tool_search_activates_without_authorizing_and_hydrates_schemas_lazily()
 {
    use modbit_protocol::v1::{StartTask, TaskRunStarted};
    use serde_json::json;
    let (repo, root) = plain_repo(&[("README.md", "# demo\n")]);
    let script = vec![
        // One call per turn: the scripted model selects its step by the number
        // of tool results in the request.
        json!({"calls": [{"name": "tool.search", "args": {"query": "symbols"}}]}),
        json!({"calls": [{"name": "tool.search", "args": {"activate": ["git.worktree.create"]}}]}),
        json!({"calls": [{"name": "plan.update", "args": {"outcome": "o", "expected_files": []}}]}),
        json!({"calls": [{"name": "task.complete", "args": {"summary": "done", "self_review": {"findings": []}}}]}),
    ];
    let (base, seen) = scripted_model(script, None).await;
    let dir = tempfile::tempdir().unwrap();
    let env = [
        ("MODBIT_OPENAI_BASE_URL", base.as_str()),
        ("OPENAI_API_KEY", ""),
        ("ANTHROPIC_API_KEY", ""),
    ];
    let core = CoreProcess::spawn_with_env(dir.path(), &env);
    let mut c = core.client().await;
    let (session, _) = create_session(&mut c, id16(0x34)).await;
    let g = lease_for(&session);
    let task = create_task_with_profile(&mut c, &session, g, &root, 0x35, "review_isolated").await;
    let eager = list_tools(&mut c, 0x36, Some(task.clone())).await;
    assert!(
        eager.iter().any(|(n, _, _)| n == "lsp.symbols")
            && !eager.iter().any(|(n, _, _)| n.starts_with("git.worktree")),
        "the host list under review_isolated: {:?}",
        eager.iter().map(|t| &t.0).collect::<Vec<_>>()
    );
    let ack = c
        .command(envelope_fenced(
            id16(0x37),
            "StartTask",
            StartTask {
                task_id: Some(task.clone()),
                endpoint: String::new(),
                model: "gpt-5-mini".into(),
                max_turns: 10,
                max_tool_calls: 0,
                max_no_progress_turns: 5,
            }
            .encode_to_vec(),
            g,
        ))
        .await
        .unwrap();
    let _: TaskRunStarted = Client::result(&ack).unwrap();
    let st = wait_task(&mut c, &task, 120).await;
    let bodies = seen.lock().unwrap().clone();
    let tool_msgs: Vec<String> = bodies
        .iter()
        .flat_map(|b| b["messages"].as_array().cloned().unwrap_or_default())
        .filter(|m| m["role"] == "tool")
        .map(|m| m["content"].as_str().unwrap_or_default().to_owned())
        .collect();
    assert_eq!(st.state, "ReadyForReview", "{st:?}\n{tool_msgs:#?}");
    assert!(bodies.len() >= 4, "{}", bodies.len());
    let names = |b: &serde_json::Value| -> Vec<String> {
        b["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["function"]["name"].as_str().unwrap().to_owned())
            .collect()
    };
    let first = names(&bodies[0]);
    assert!(first.contains(&"tool.search".to_owned()), "{first:?}");
    assert!(first.contains(&"fs.read".to_owned()) && first.contains(&"search.retrieve".to_owned()));
    assert!(
        !first
            .iter()
            .any(|n| n.starts_with("lsp.") || n == "search.symbols" || n.starts_with("git.")),
        "deferred tools are not projected before discovery: {first:?}"
    );
    let search_desc = bodies[0]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["function"]["name"] == "tool.search")
        .unwrap()["function"]["description"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(
        search_desc.contains("lsp: ") && search_desc.contains("lsp.symbols"),
        "{search_desc}"
    );
    assert!(
        !search_desc.contains("git.worktree"),
        "a denied tool is not even named: {search_desc}"
    );
    // Turn 2: the discovered tools carry their schemas; the denied one is still absent.
    let second = names(&bodies[1]);
    assert!(
        second.contains(&"lsp.symbols".to_owned()) && second.contains(&"search.symbols".to_owned()),
        "{second:?}"
    );
    assert!(!second.iter().any(|n| n.starts_with("git.")), "{second:?}");
    let hydrated = bodies[1]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["function"]["name"] == "lsp.symbols")
        .unwrap();
    assert!(
        hydrated["function"]["parameters"]["properties"]["path"].is_object(),
        "{hydrated}"
    );
    // The tool results the model saw (across the turns).
    let results = &tool_msgs;
    assert!(
        results.iter().any(|r| r.contains("- lsp.symbols")
            && r.contains("activated for the next turns")
            && r.contains("does not authorize")),
        "{results:?}"
    );
    assert!(
        results
            .iter()
            .any(|r| r.contains("no deferred tool matches")),
        "worktree is denied under review_isolated: {results:?}"
    );
    // QUAL-EV-0177: the lazy projection is smaller than the eager catalog in the wire shape.
    let lazy_bytes = serde_json::to_string(&bodies[0]["tools"]).unwrap().len();
    let eager_wire: Vec<serde_json::Value> = eager
        .iter()
        .map(|(n, schema, d)| json!({"type": "function", "function": {"name": n, "description": d, "parameters": serde_json::from_str::<serde_json::Value>(schema).unwrap()}}))
        .collect();
    let eager_bytes = serde_json::to_string(&eager_wire).unwrap().len();
    eprintln!(
        "QUAL-EV-0177 projection bytes: lazy={lazy_bytes} ({} tools incl. harness) eager={eager_bytes} ({} host tools)",
        first.len(),
        eager.len()
    );
    // Measured, not targeted: on this 28-tool catalog the lazy projection is
    // about a quarter smaller; the saving grows with the catalog because every
    // deferred tool costs one name instead of a schema.
    assert!(
        lazy_bytes < eager_bytes,
        "lazy {lazy_bytes} < eager {eager_bytes}"
    );
    eprintln!(
        "QUAL-EV-0177 lazy/eager = {:.2}",
        lazy_bytes as f64 / eager_bytes as f64
    );
    // The activation is an event, so it survives a restart of the Core.
    let evs = task_events(&core, &session, &task).await;
    let activated: Vec<&serde_json::Value> = evs
        .iter()
        .filter(|(_, t, _)| t == "ToolsActivated")
        .map(|(_, _, p)| p)
        .collect();
    assert_eq!(activated.len(), 1, "{evs:#?}");
    assert!(
        activated[0]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .any(|t| t == "lsp.symbols")
    );
    // REQ-EV-0132: the run's evidence is searchable by run and step.
    let r = invoke_tool(
        &mut c,
        &task,
        g,
        0x38,
        0x39,
        "evidence.search",
        r#"{"query":"lsp.symbols","max_hits":50}"#,
    )
    .await;
    assert_eq!(r.status, "SUCCESS", "{r:?}");
    let so: serde_json::Value = serde_json::from_str(&r.structured_output_json).unwrap();
    let hits = so["hits"].as_array().unwrap();
    assert!(!hits.is_empty(), "{so}");
    assert!(
        hits.iter()
            .any(|h| h["event_type"] == "ToolsActivated" && h["run_id"].is_string()),
        "{so}"
    );
    let step_hit = hits
        .iter()
        .find(|h| h["step_id"].is_string() && h["kind"] == "step")
        .unwrap_or_else(|| panic!("a step-scoped hit: {so}"));
    let step_id = step_hit["step_id"].as_str().unwrap().to_owned();
    let r = invoke_tool(
        &mut c,
        &task,
        g,
        0x3A,
        0x3B,
        "evidence.search",
        &format!(r#"{{"query":"lsp.symbols","step_id":"{step_id}"}}"#),
    )
    .await;
    let so2: serde_json::Value = serde_json::from_str(&r.structured_output_json).unwrap();
    assert!(
        so2["hits"]
            .as_array()
            .unwrap()
            .iter()
            .all(|h| h["step_id"] == step_id),
        "{so2}"
    );
    assert!(!so2["hits"].as_array().unwrap().is_empty());
    assert_eq!(so2["scope"]["step_id"], step_id);
    let _ = repo;
}

/// PX-026: every client can show honest language labels — the three Alpha
/// candidates at the Alpha baseline with proven, provisional and not-claimed
/// capabilities named; anything else unsupported.
#[tokio::test]
async fn qual_px_026_language_labels_are_honest_and_served_to_every_client() {
    use modbit_protocol::v1::{LanguageList, ListLanguages};
    let dir = tempfile::tempdir().unwrap();
    let core = CoreProcess::spawn(dir.path());
    let mut c = core.client().await;
    let ack = c
        .command(envelope(
            id16(0x26),
            "ListLanguages",
            ListLanguages {}.encode_to_vec(),
        ))
        .await
        .unwrap();
    let l: LanguageList = Client::result(&ack).unwrap();
    let by = |name: &str| l.languages.iter().find(|x| x.language == name).cloned();
    for lang in ["typescript", "javascript", "python", "rust"] {
        let v = by(lang).unwrap_or_else(|| panic!("{lang}: {:?}", l.languages));
        assert_eq!(v.tier, "ALPHA_BASELINE", "{v:?}");
        assert!(
            v.label.contains("Alpha baseline") && v.label.contains("Tier C"),
            "{v:?}"
        );
        assert!(
            v.proven
                .iter()
                .any(|p| p.contains("compile_and_test_evidence")),
            "{v:?}"
        );
        assert!(
            v.provisional.iter().any(|p| p.contains("language_service")),
            "{v:?}"
        );
        assert!(v.not_claimed.iter().any(|p| p.contains("tier_a")), "{v:?}");
        assert!(v.fixture.starts_with("tests/fixtures/repos/"), "{v:?}");
        assert!(!v.evidence_tests.is_empty(), "{v:?}");
    }
    let other = by("*").unwrap();
    assert_eq!(other.tier, "UNSUPPORTED");
    assert!(other.not_claimed.iter().any(|p| p.contains("structural")));
    // The cited evidence tests exist in this repository.
    let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut sources = String::new();
    for f in [
        "crates/verification/tests/fixtures.rs",
        "crates/workspace/tests/real_fs.rs",
        "services/modbit-core/tests/surface_protocol.rs",
    ] {
        sources.push_str(&std::fs::read_to_string(repo.join(f)).unwrap());
    }
    for v in &l.languages {
        for t in &v.evidence_tests {
            assert!(sources.contains(&format!("fn {t}(")), "missing test {t}");
        }
    }
}

/// PX-016 change strategy on the real rust-cli fixture: the failing test is
/// written first, then the change; every write is one revision-bound
/// ChangeTransaction (one FileChanged each); a write outside the plan is
/// refused until a plan revision declares it (PlanRevised carries the scope
/// delta); a lockfile edited by hand is flagged (DI-2) even with a plan entry.
#[tokio::test]
async fn qual_px_016_change_strategy_tests_first_one_concern_per_transaction_and_no_silent_scope_widening()
 {
    use modbit_protocol::v1::{StartTask, TaskRunStarted};
    use serde_json::json;
    let (repo, root) = fixture_repo("rust-cli");
    let lib = std::fs::read_to_string(repo.path().join("src/lib.rs")).unwrap();
    let tests_src = std::fs::read_to_string(repo.path().join("tests/quantities.rs")).unwrap();
    let with_new_test = format!(
        "{tests_src}\n/// Written first: zero is not a quantity either.\n#[test]\nfn zero_quantity_is_rejected() {{\n    assert!(parse_quantity(\"0\").is_err());\n}}\n"
    );
    let fixed_lib = lib.replace(
        "    Ok(n)\n",
        "    if n <= 0 {\n        return Err(\"quantity must be positive\".into());\n    }\n    Ok(n)\n",
    );
    assert_ne!(fixed_lib, lib);
    let lock = std::fs::read_to_string(repo.path().join("Cargo.lock")).unwrap();
    let script = vec![
        json!({"calls": [{"name": "plan.update", "args": {"outcome": "zero and negative quantities are rejected", "expected_files": ["tests/quantities.rs", "src/lib.rs", "Cargo.lock"], "verification": ["zero_quantity_is_rejected", "acceptance_rejects_negative_quantity"]}}]}),
        // retrieval before edit (PX-015)
        json!({"calls": [{"name": "fs.read", "args": {"path": "tests/quantities.rs"}}]}),
        json!({"calls": [{"name": "fs.read", "args": {"path": "src/lib.rs"}}]}),
        // 1. the failing test first
        json!({"calls": [{"name": "change.apply", "args": {"path": "tests/quantities.rs", "op": "replace", "content": with_new_test}}]}),
        json!({"calls": [{"name": "verify.run", "args": {"reason": "the new test must fail first"}}]}),
        // 2. the repair attempt is recorded (PX-018), then the change, one concern
        json!({"calls": [{"name": "repair.attempt", "args": {"check_id": "cargo:tests/quantities.rs::zero_quantity_is_rejected", "hypothesis": "parse_quantity accepts zero; guard n <= 0", "evidence_refs": ["verify.run"], "intended_fix": "src/lib.rs parse_quantity"}}]}),
        json!({"calls": [{"name": "change.apply", "args": {"path": "src/lib.rs", "op": "replace", "content": fixed_lib}}]}),
        json!({"calls": [{"name": "verify.run", "args": {"reason": "now it passes"}}]}),
        // 3. silent scope widening is refused
        json!({"calls": [{"name": "fs.read", "args": {"path": "README.md"}}]}),
        json!({"calls": [{"name": "fs.read", "args": {"path": "Cargo.lock"}}]}),
        json!({"calls": [{"name": "change.apply", "args": {"path": "README.md", "op": "replace", "content": "# rust-cli\nquantities must be positive\n"}}]}),
        // 4. an explicit plan revision declares the file, then the write is allowed
        json!({"calls": [{"name": "plan.update", "args": {"outcome": "zero and negative quantities are rejected", "expected_files": ["tests/quantities.rs", "src/lib.rs", "Cargo.lock", "README.md"], "verification": ["zero_quantity_is_rejected"], "reason": "document the rule"}}]}),
        json!({"calls": [{"name": "change.apply", "args": {"path": "README.md", "op": "replace", "content": "# rust-cli\nquantities must be positive\n"}}]}),
        // 5. a lockfile edited by hand is flagged even with the plan entry
        json!({"calls": [{"name": "change.apply", "args": {"path": "Cargo.lock", "op": "replace", "content": format!("{lock}# touched by hand\n")}}]}),
        // 6. the flag blocks completion until the plan justifies the hand edit (docs/64 §4)
        json!({"calls": [{"name": "task.complete", "args": {"summary": "done", "self_review": {"findings": []}}}]}),
        json!({"calls": [{"name": "plan.update", "args": {"outcome": "zero and negative quantities are rejected", "expected_files": ["tests/quantities.rs", "src/lib.rs", "Cargo.lock", "README.md"], "verification": ["zero_quantity_is_rejected"], "reason": "Cargo.lock: a trailing comment only; no dependency changed, regeneration is a no-op"}}]}),
        json!({"calls": [{"name": "task.complete", "args": {"summary": "done", "self_review": {"findings": []}}}]}),
    ];
    let (base, seen) = scripted_model(script, None).await;
    let dir = tempfile::tempdir().unwrap();
    let core = CoreProcess::spawn_with_env(
        dir.path(),
        &[
            ("MODBIT_OPENAI_BASE_URL", &base),
            ("OPENAI_API_KEY", ""),
            ("ANTHROPIC_API_KEY", ""),
            ("CARGO_TERM_COLOR", "always"),
        ],
    );
    let mut c = core.client().await;
    let (session, _) = create_session(&mut c, id16(0x16)).await;
    let g = lease_for(&session);
    let ack = c
        .command(envelope_fenced(
            id16(0x17),
            "CreateTask",
            CreateTask {
                session_id: Some(session.clone()),
                goal_text: "Reject zero quantities too.".into(),
                workspace_id: None,
                execution_profile: String::new(),
                origin: "cli".into(),
                workspace_root: root.clone(),
            }
            .encode_to_vec(),
            g,
        ))
        .await
        .unwrap();
    let task = Client::result::<TaskCreated>(&ack)
        .unwrap()
        .task_id
        .unwrap();
    let ack = c
        .command(envelope_fenced(
            id16(0x18),
            "StartTask",
            StartTask {
                task_id: Some(task.clone()),
                endpoint: String::new(),
                model: "gpt-5-mini".into(),
                max_turns: 20,
                max_tool_calls: 0,
                max_no_progress_turns: 5,
            }
            .encode_to_vec(),
            g,
        ))
        .await
        .unwrap();
    let _: TaskRunStarted = Client::result(&ack).unwrap();
    let st = wait_task(&mut c, &task, 300).await;
    let evs = task_events(&core, &session, &task).await;
    // The last request carries the whole transcript once.
    let tool_msgs: Vec<String> = seen
        .lock()
        .unwrap()
        .last()
        .and_then(|b| b["messages"].as_array().cloned())
        .unwrap_or_default()
        .iter()
        .filter(|m| m["role"] == "tool")
        .map(|m| m["content"].as_str().unwrap_or_default().to_owned())
        .collect();
    assert_eq!(
        st.state,
        "ReadyForReview",
        "{st:?}\n{tool_msgs:#?}\n{:#?}",
        evs.iter()
            .filter(|(_, t, _)| t != "StepScheduled" && t != "StepStarted" && t != "StepSucceeded")
            .map(|(_, t, p)| format!("{t} {}", p.get("failure_code").cloned().unwrap_or_default()))
            .collect::<Vec<_>>()
    );
    let of = |t: &str| {
        evs.iter()
            .filter(|(_, x, _)| x == t)
            .map(|(_, _, p)| p.clone())
            .collect::<Vec<_>>()
    };
    // Tests first: the TARGETED run after the test write reports the new test failing,
    // the one after the fix reports it passing.
    let runs = of("VerificationRunRecorded");
    let targeted: Vec<&serde_json::Value> =
        runs.iter().filter(|r| r["stage"] == "TARGETED").collect();
    assert!(targeted.len() >= 2, "{runs:#?}");
    let status_in = |r: &serde_json::Value, sym: &str| {
        r["checks"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|c| {
                c["check_id"]
                    .as_str()
                    .unwrap_or_default()
                    .ends_with(&format!("::{sym}"))
            })
            .map(|c| c["status"].as_str().unwrap_or_default().to_owned())
    };
    assert_eq!(
        status_in(targeted[0], "zero_quantity_is_rejected").as_deref(),
        Some("FAIL"),
        "{:?}",
        targeted[0]
    );
    assert_eq!(
        status_in(targeted[1], "zero_quantity_is_rejected").as_deref(),
        Some("PASS"),
        "{:?}",
        targeted[1]
    );
    // One revision-bound ChangeTransaction per write: four FileChanged events, strictly increasing revisions.
    let changed = of("FileChanged");
    let paths: Vec<&str> = changed
        .iter()
        .map(|e| e["path"].as_str().unwrap())
        .collect();
    assert_eq!(
        paths,
        [
            "tests/quantities.rs",
            "src/lib.rs",
            "README.md",
            "Cargo.lock"
        ],
        "{changed:#?}"
    );
    let revs: Vec<u64> = changed
        .iter()
        .map(|e| e["workspace_revision"].as_u64().unwrap())
        .collect();
    assert!(revs.windows(2).all(|w| w[0] < w[1]), "{revs:?}");
    assert!(
        changed
            .iter()
            .all(|e| e["previous_revision"].as_u64().unwrap()
                < e["workspace_revision"].as_u64().unwrap()),
        "revision-bound: {changed:#?}"
    );
    // The silent README write was refused before any effect; the plan revision carries the delta.
    assert!(
        tool_msgs
            .iter()
            .any(|t| t.contains("HARNESS_PLAN_REVISION_REQUIRED") && t.contains("README.md")),
        "{tool_msgs:#?}"
    );
    let revised = of("PlanRevised");
    assert_eq!(revised.len(), 2, "{revised:#?}");
    let added: Vec<&str> = revised[0]["added"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a.as_str().unwrap())
        .collect();
    assert_eq!(added, ["README.md"], "{revised:#?}");
    assert_eq!(revised[0]["reason"], "document the rule");
    assert_eq!(
        evs.iter()
            .filter(|(_, t, p)| t == "ToolCallProposed" && p["tool_name"] == "change.apply")
            .count(),
        4,
        "only the four allowed writes reached the effector"
    );
    // The hand-edited lockfile is flagged (DI-2 FLAG) even though the plan names it.
    let di = of("DiffInvariantViolated");
    assert!(
        di.iter().any(|v| v["invariant"] == "DI-2"
            && v["class"] == "FLAG"
            && v["paths"][0] == "Cargo.lock"),
        "{di:#?}"
    );
    // The open flag refused the first completion; the justifying plan revision cleared it.
    assert!(
        tool_msgs
            .iter()
            .any(|t| t.contains("COMPLETION_REFUSED") && t.contains("OPEN_FLAGS")),
        "{tool_msgs:#?}"
    );
    assert_eq!(of("SelfReviewRecorded").len(), 2);
    // PX-018: the recorded attempt concluded RESOLVED on the second TARGETED run.
    let concluded = of("RepairAttemptConcluded");
    assert_eq!(concluded.len(), 1, "{concluded:#?}");
    assert_eq!(concluded[0]["outcome"], "RESOLVED", "{concluded:#?}");
    let _ = repo;
}

/// PX-018 on the real rust-cli fixture: after a failed verification a change
/// without a RepairAttempt is refused; the attempt records signature,
/// hypothesis, evidence and intended fix before the change; a WORSENED
/// attempt is concluded with its change fingerprint and reverted through the
/// Change Engine; an equivalent hypothesis for the same failure escalates to
/// Needs Attention with the attempt history.
#[tokio::test]
async fn qual_px_018_repair_attempts_are_recorded_bounded_reverted_when_worsened_and_escalate_on_equivalence()
 {
    use modbit_protocol::v1::{StartTask, TaskRunStarted};
    use serde_json::json;
    let (repo, root) = fixture_repo("rust-cli");
    let lib = std::fs::read_to_string(repo.path().join("src/lib.rs")).unwrap();
    let tests_src = std::fs::read_to_string(repo.path().join("tests/quantities.rs")).unwrap();
    let with_new_test = format!(
        "{tests_src}\n#[test]\nfn zero_quantity_is_rejected() {{\n    assert!(parse_quantity(\"0\").is_err());\n}}\n"
    );
    // A wrong change: it does not touch zero and it breaks formatting.
    let wrong_lib = lib.replace(
        "format!(\"{}.{:02}\", cents / 100, cents % 100)",
        "format!(\"{}.{:03}\", cents / 100, cents % 100)",
    );
    assert_ne!(wrong_lib, lib);
    let script = vec![
        json!({"calls": [{"name": "plan.update", "args": {"outcome": "zero quantities are rejected", "expected_files": ["tests/quantities.rs", "src/lib.rs"], "verification": ["zero_quantity_is_rejected"]}}]}),
        json!({"calls": [{"name": "fs.read", "args": {"path": "tests/quantities.rs"}}]}),
        json!({"calls": [{"name": "fs.read", "args": {"path": "src/lib.rs"}}]}),
        json!({"calls": [{"name": "change.apply", "args": {"path": "tests/quantities.rs", "op": "replace", "content": with_new_test}}]}),
        json!({"calls": [{"name": "verify.run", "args": {"reason": "the new test fails first"}}]}),
        // A change after the failed verification without an attempt is refused.
        json!({"calls": [{"name": "change.apply", "args": {"path": "src/lib.rs", "op": "replace", "content": wrong_lib}}]}),
        json!({"calls": [{"name": "repair.attempt", "args": {"check_id": "cargo:tests/quantities.rs::zero_quantity_is_rejected", "hypothesis": "parse_quantity accepts zero; the guard must reject n <= 0", "evidence_refs": ["fs.read src/lib.rs", "verify.run vr-1"], "intended_fix": "src/lib.rs parse_quantity: add the guard"}}]}),
        json!({"calls": [{"name": "change.apply", "args": {"path": "src/lib.rs", "op": "replace", "content": wrong_lib}}]}),
        json!({"calls": [{"name": "verify.run", "args": {"reason": "check the fix"}}]}),
        // The same hypothesis again: escalation, not another run.
        json!({"calls": [{"name": "repair.attempt", "args": {"check_id": "cargo:tests/quantities.rs::zero_quantity_is_rejected", "hypothesis": "The guard must reject n <= 0 because parse_quantity accepts zero", "evidence_refs": [], "intended_fix": "same"}}]}),
        json!({"calls": [{"name": "task.complete", "args": {"summary": "never reached", "self_review": {"findings": []}}}]}),
    ];
    let (base, seen) = scripted_model(script, None).await;
    let dir = tempfile::tempdir().unwrap();
    let core = CoreProcess::spawn_with_env(
        dir.path(),
        &[
            ("MODBIT_OPENAI_BASE_URL", &base),
            ("OPENAI_API_KEY", ""),
            ("ANTHROPIC_API_KEY", ""),
            ("CARGO_TERM_COLOR", "always"),
        ],
    );
    let mut c = core.client().await;
    let (session, _) = create_session(&mut c, id16(0x19)).await;
    let g = lease_for(&session);
    let ack = c
        .command(envelope_fenced(
            id16(0x1A),
            "CreateTask",
            CreateTask {
                session_id: Some(session.clone()),
                goal_text: "parse_quantity accepts zero; it must be rejected.".into(),
                workspace_id: None,
                execution_profile: String::new(),
                origin: "cli".into(),
                workspace_root: root.clone(),
            }
            .encode_to_vec(),
            g,
        ))
        .await
        .unwrap();
    let task = Client::result::<TaskCreated>(&ack)
        .unwrap()
        .task_id
        .unwrap();
    let ack = c
        .command(envelope_fenced(
            id16(0x1B),
            "StartTask",
            StartTask {
                task_id: Some(task.clone()),
                endpoint: String::new(),
                model: "gpt-5-mini".into(),
                max_turns: 20,
                max_tool_calls: 0,
                max_no_progress_turns: 5,
            }
            .encode_to_vec(),
            g,
        ))
        .await
        .unwrap();
    let _: TaskRunStarted = Client::result(&ack).unwrap();
    let st = wait_task(&mut c, &task, 300).await;
    let evs = task_events(&core, &session, &task).await;
    // The last request carries the whole transcript once.
    let tool_msgs: Vec<String> = seen
        .lock()
        .unwrap()
        .last()
        .and_then(|b| b["messages"].as_array().cloned())
        .unwrap_or_default()
        .iter()
        .filter(|m| m["role"] == "tool")
        .map(|m| m["content"].as_str().unwrap_or_default().to_owned())
        .collect();
    assert_eq!(
        (
            st.state.as_str(),
            st.wait_reason.as_str(),
            st.run_state.as_str()
        ),
        ("Waiting", "UserInput", "Suspended"),
        "{st:?}\n{tool_msgs:#?}"
    );
    let of = |t: &str| {
        evs.iter()
            .filter(|(_, x, _)| x == t)
            .map(|(_, _, p)| p.clone())
            .collect::<Vec<_>>()
    };
    // The unrecorded change was refused before any effector.
    assert!(
        tool_msgs
            .iter()
            .any(|t| t.contains("HARNESS_REPAIR_ATTEMPT_REQUIRED")
                && t.contains("zero_quantity_is_rejected")),
        "{tool_msgs:#?}"
    );
    // PX-039: the mandatory baseline reproduced the reported failure before any
    // fix transaction, and it is recorded.
    let repro = of("ReproductionRecorded");
    assert!(
        repro.iter().any(|r| r["status"] == "REPRODUCED"),
        "{repro:#?}"
    );
    let repro_at = evs
        .iter()
        .position(|(_, t, p)| t == "ReproductionRecorded" && p["status"] == "REPRODUCED")
        .unwrap();
    let first_lib_write = evs
        .iter()
        .position(|(_, t, p)| t == "FileChanged" && p["path"] == "src/lib.rs")
        .unwrap();
    assert!(repro_at < first_lib_write, "reproduced before the fix");
    // The attempt was recorded with all its fields before the change ran.
    let recorded = of("RepairAttemptRecorded");
    assert_eq!(recorded.len(), 1, "{recorded:#?}");
    let a = &recorded[0];
    assert_eq!(a["attempt_ordinal"], 1);
    assert!(
        a["failure_signature"]
            .as_str()
            .unwrap()
            .starts_with("verify:cargo:tests/quantities.rs::zero_quantity_is_rejected:"),
        "{a}"
    );
    assert_eq!(
        a["hypothesis"],
        "parse_quantity accepts zero; the guard must reject n <= 0"
    );
    assert_eq!(
        a["evidence_refs"],
        json!(["fs.read src/lib.rs", "verify.run vr-1"])
    );
    assert!(
        a["hypothesis_fingerprint"]
            .as_str()
            .unwrap()
            .contains("zero")
    );
    assert!(a["attempt_ref"].as_str().unwrap().len() == 64);
    let recorded_offset = evs
        .iter()
        .position(|(_, t, _)| t == "RepairAttemptRecorded")
        .unwrap();
    let lib_write_offset = evs
        .iter()
        .enumerate()
        .filter(|(_, (_, t, p))| t == "FileChanged" && p["path"] == "src/lib.rs")
        .map(|(i, _)| i)
        .next()
        .unwrap();
    assert!(
        recorded_offset < lib_write_offset,
        "recorded before the change"
    );
    // Concluded WORSENED (the target stayed, formats_totals broke), fingerprinted and reverted.
    let concluded = of("RepairAttemptConcluded");
    assert_eq!(concluded.len(), 1, "{concluded:#?}");
    let cc = &concluded[0];
    assert_eq!(cc["outcome"], "WORSENED", "{cc}");
    assert_eq!(cc["reverted"], true, "{cc}");
    assert!(!cc["change_refs"].as_array().unwrap().is_empty());
    assert_eq!(cc["change_fingerprint"].as_str().unwrap().len(), 64);
    assert_eq!(
        std::fs::read_to_string(repo.path().join("src/lib.rs")).unwrap(),
        lib,
        "the worsening change was reverted on disk"
    );
    assert!(
        of("FileChanged")
            .iter()
            .any(|e| e["path"] == "src/lib.rs" && e["op"].as_str().unwrap().starts_with("undo")),
        "{:#?}",
        of("FileChanged")
    );
    // The equivalent hypothesis escalated: RepairEscalated with the history, task Needs Attention.
    let esc = of("RepairEscalated");
    assert_eq!(esc.len(), 1, "{esc:#?}");
    assert!(
        esc[0]["reason"].as_str().unwrap().contains("equivalent"),
        "{esc:#?}"
    );
    assert_eq!(esc[0]["attempts"], 1);
    assert_eq!(esc[0]["history_ref"].as_str().unwrap().len(), 64);
    let history = read_object(&mut c, id16(0x1C), esc[0]["history_ref"].as_str().unwrap()).await;
    let history: serde_json::Value = serde_json::from_str(&history).unwrap();
    assert_eq!(history[0]["outcome"], "WORSENED", "{history}");
    let attention = of("TaskNeedsAttention");
    assert!(
        attention
            .iter()
            .any(|a| a["reason"].as_str().unwrap().contains("repair escalated")),
        "{attention:#?}"
    );
    assert!(
        of("SelfReviewRecorded").is_empty(),
        "task.complete never ran"
    );
    let _ = repo;
}

/// PX-015 / REQ-EV-0168: an edit of an existing file needs a retrieval record
/// for the bytes on disk — a read, a language-service query, a Context Pack
/// entry or the task's own write. A blind edit is refused before any effector;
/// a record whose bytes were changed behind the task is stale and refused; the
/// records are durable, so they survive a Core restart while the in-memory
/// ledger does not.
#[tokio::test]
async fn qual_px_015_retrieval_before_edit_is_enforced_and_a_stale_record_is_refused() {
    use modbit_protocol::v1::{StartTask, TaskRunStarted};
    use serde_json::json;
    let (repo, root) = plain_repo(&[("README.md", "# demo\n"), ("notes.txt", "notes\n")]);
    let script = vec![
        json!({"calls": [{"name": "plan.update", "args": {"outcome": "edit both files", "expected_files": ["README.md", "notes.txt", "fresh.txt"]}}]}),
        // 1. blind edit: refused, no effector runs
        json!({"calls": [{"name": "change.apply", "args": {"path": "README.md", "op": "replace", "content": "# demo\nblind\n"}}]}),
        json!({"calls": [{"name": "fs.read", "args": {"path": "README.md"}}]}),
        json!({"calls": [{"name": "fs.read", "args": {"path": "notes.txt"}}]}),
        // 2. retrieved: allowed
        json!({"calls": [{"name": "change.apply", "args": {"path": "README.md", "op": "replace", "content": "# demo\nread first\n"}}]}),
        // 3. served after the restart: notes.txt changed behind the task, the record is stale
        json!({"calls": [{"name": "change.apply", "args": {"path": "notes.txt", "op": "replace", "content": "stale write\n"}}]}),
        json!({"calls": [{"name": "fs.read", "args": {"path": "notes.txt"}}]}),
        json!({"calls": [{"name": "change.apply", "args": {"path": "notes.txt", "op": "replace", "content": "fresh read\n"}}]}),
        // 4. a new file needs no record; the task's own write is the record for the next edit
        json!({"calls": [{"name": "change.apply", "args": {"path": "fresh.txt", "op": "create", "content": "new\n"}}]}),
        json!({"calls": [{"name": "change.apply", "args": {"path": "fresh.txt", "op": "replace", "content": "new again\n"}}]}),
        json!({"calls": [{"name": "task.complete", "args": {"summary": "done", "self_review": {"findings": []}}}]}),
    ];
    let (base, seen) = scripted_model(script, Some(5)).await;
    let dir = tempfile::tempdir().unwrap();
    let env = [
        ("MODBIT_OPENAI_BASE_URL", base.as_str()),
        ("OPENAI_API_KEY", ""),
        ("ANTHROPIC_API_KEY", ""),
    ];
    let mut core = CoreProcess::spawn_with_env(dir.path(), &env);
    let mut c = core.client().await;
    let (session, _) = create_session(&mut c, id16(0x15)).await;
    let g = lease_for(&session);
    let ack = c
        .command(envelope_fenced(
            id16(0x1D),
            "CreateTask",
            CreateTask {
                session_id: Some(session.clone()),
                goal_text: "edit the files".into(),
                workspace_id: None,
                execution_profile: String::new(),
                origin: "cli".into(),
                workspace_root: root.clone(),
            }
            .encode_to_vec(),
            g,
        ))
        .await
        .unwrap();
    let task = Client::result::<TaskCreated>(&ack)
        .unwrap()
        .task_id
        .unwrap();
    let ack = c
        .command(envelope_fenced(
            id16(0x1E),
            "StartTask",
            StartTask {
                task_id: Some(task.clone()),
                endpoint: String::new(),
                model: "gpt-5-mini".into(),
                max_turns: 20,
                max_tool_calls: 0,
                max_no_progress_turns: 5,
            }
            .encode_to_vec(),
            g,
        ))
        .await
        .unwrap();
    let _: TaskRunStarted = Client::result(&ack).unwrap();
    // The retrieved README write lands; then the sixth request stalls.
    let deadline = std::time::Instant::now() + Duration::from_secs(90);
    while std::fs::read_to_string(repo.path().join("README.md")).unwrap() != "# demo\nread first\n"
    {
        assert!(
            std::time::Instant::now() < deadline,
            "the retrieved write did not land: {:#?}",
            seen.lock()
                .unwrap()
                .iter()
                .flat_map(|b| b["messages"].as_array().cloned().unwrap_or_default())
                .filter(|m| m["role"] == "tool")
                .map(|m| m["content"].as_str().unwrap_or_default().to_owned())
                .collect::<Vec<_>>()
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let deadline = std::time::Instant::now() + Duration::from_secs(60);
    while seen.lock().unwrap().len() < 6 {
        assert!(
            std::time::Instant::now() < deadline,
            "the stalled request never started"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    // Someone else changes notes.txt behind the task, and the Core restarts: the
    // in-memory ledger is gone, the durable retrieval records are not.
    drop(c);
    core.kill();
    std::fs::write(repo.path().join("notes.txt"), "changed by someone else\n").unwrap();
    let core2 = CoreProcess::spawn_with_env(dir.path(), &env);
    let mut c2 = core2.client().await;
    let g2 = Some(acquire_lease(&mut c2, id16(0x1F), session.clone(), "resumer").await);
    let ack = c2
        .command(envelope_fenced(
            id16(0x20),
            "StartTask",
            StartTask {
                task_id: Some(task.clone()),
                endpoint: String::new(),
                model: "gpt-5-mini".into(),
                max_turns: 20,
                max_tool_calls: 0,
                max_no_progress_turns: 5,
            }
            .encode_to_vec(),
            g2,
        ))
        .await
        .unwrap();
    let started: TaskRunStarted = Client::result(&ack).unwrap();
    assert!(started.resumed);
    let st = wait_for_state(&mut c2, &task, "ReadyForReview", 120).await;
    let evs = task_events(&core2, &session, &task).await;
    // The last request carries the whole rebuilt transcript once.
    let tool_msgs: Vec<String> = seen
        .lock()
        .unwrap()
        .last()
        .and_then(|b| b["messages"].as_array().cloned())
        .unwrap_or_default()
        .iter()
        .filter(|m| m["role"] == "tool")
        .map(|m| m["content"].as_str().unwrap_or_default().to_owned())
        .collect();
    assert_eq!(st.state, "ReadyForReview", "{st:?}\n{tool_msgs:#?}");
    let refusals: Vec<&String> = tool_msgs
        .iter()
        .filter(|t| t.contains("HARNESS_RETRIEVAL_REQUIRED"))
        .collect();
    assert_eq!(
        refusals.len(),
        2,
        "the blind and the stale edit: {tool_msgs:#?}"
    );
    assert!(refusals[0].contains("README.md"), "{refusals:?}");
    assert!(refusals[1].contains("notes.txt"), "{refusals:?}");
    // Only the retrieved writes reached the effector, in order.
    let written: Vec<&str> = evs
        .iter()
        .filter(|(_, t, _)| t == "FileChanged")
        .map(|(_, _, p)| p["path"].as_str().unwrap())
        .collect();
    assert_eq!(
        written,
        ["README.md", "notes.txt", "fresh.txt", "fresh.txt"],
        "{written:?}"
    );
    assert_eq!(
        std::fs::read_to_string(repo.path().join("notes.txt")).unwrap(),
        "fresh read\n",
        "the stale write never landed; the re-read one did"
    );
    assert_eq!(
        std::fs::read_to_string(repo.path().join("fresh.txt")).unwrap(),
        "new again\n"
    );
    // The records are durable and bind to the bytes they were taken at.
    let recorded = evs
        .iter()
        .filter(|(_, t, _)| t == "RetrievalRecorded")
        .map(|(_, _, p)| {
            (
                p["path"].as_str().unwrap().to_owned(),
                p["tool_name"].as_str().unwrap().to_owned(),
            )
        })
        .collect::<Vec<_>>();
    assert!(
        recorded
            .iter()
            .any(|(p, t)| p == "notes.txt" && t == "fs.read"),
        "{recorded:?}"
    );
    assert!(
        recorded
            .iter()
            .any(|(p, t)| p == "fresh.txt" && t == "change.apply"),
        "{recorded:?}"
    );
    for (_, _, p) in evs.iter().filter(|(_, t, _)| t == "RetrievalRecorded") {
        assert_eq!(p["content_hash"].as_str().unwrap().len(), 64, "{p}");
    }
    // The ledger of the resumed Core shows the reads it took after the restart.
    let r = invoke_tool(&mut c2, &task, g2, 0x21, 0x22, "context.ledger", "{}").await;
    let so: serde_json::Value = serde_json::from_str(&r.structured_output_json).unwrap();
    assert!(
        so["ledger"]["reads"]
            .as_array()
            .unwrap()
            .iter()
            .any(|x| x["path"] == "notes.txt" && x["content_hash"].is_string()),
        "{so}"
    );
    let _ = repo;
}

/// PX-038 scope policy: the first plan freezes the original write set; a
/// revision carries its delta and reason; an always-ask path outside that set
/// is refused until a typed question is answered (the agent cannot answer its
/// own question); the answer is recorded with the counters measured against
/// the original plan; a headless task fails closed to Needs Attention when a
/// bound is reached.
#[tokio::test]
async fn qual_px_038_scope_expansion_is_bounded_asks_a_typed_question_and_fails_closed_headless() {
    use modbit_protocol::v1::{
        ListQuestions, QuestionList, QuestionResponded, RespondToQuestion, StartTask,
        TaskRunStarted,
    };
    use serde_json::json;
    // ---- interactive task: an always-ask path needs an answer
    let (repo, root) = plain_repo(&[("a.txt", "a\n"), ("pnpm-lock.yaml", "lockfileVersion: 9\n")]);
    let script = vec![
        json!({"calls": [{"name": "plan.update", "args": {"outcome": "edit a", "expected_files": ["a.txt"]}}]}),
        json!({"calls": [{"name": "fs.read", "args": {"path": "a.txt"}}]}),
        json!({"calls": [{"name": "change.apply", "args": {"path": "a.txt", "op": "replace", "content": "a edited\n"}}]}),
        json!({"calls": [{"name": "plan.update", "args": {"outcome": "edit a", "expected_files": ["a.txt", "pnpm-lock.yaml"], "reason": "the lockfile needs a bump"}}]}),
        json!({"calls": [{"name": "fs.read", "args": {"path": "pnpm-lock.yaml"}}]}),
        // refused: an always-ask path outside the original write set
        json!({"calls": [{"name": "change.apply", "args": {"path": "pnpm-lock.yaml", "op": "replace", "content": "lockfileVersion: 9\n# bumped\n"}}]}),
        // retrying without an answer changes nothing
        json!({"calls": [{"name": "change.apply", "args": {"path": "pnpm-lock.yaml", "op": "replace", "content": "lockfileVersion: 9\n# bumped\n"}}]}),
        json!({"calls": [{"name": "user.ask", "args": {"question": "The lockfile is outside the original plan. Continue, split it into a follow-up task, or stop?", "options": [{"id": "continue", "label": "continue with the expansion"}, {"id": "split", "label": "split into a follow-up task"}, {"id": "stop", "label": "stop"}], "reason": "change_set"}}]}),
        json!({"calls": [{"name": "change.apply", "args": {"path": "pnpm-lock.yaml", "op": "replace", "content": "lockfileVersion: 9\n# bumped\n"}}]}),
        // The hand-edited lockfile is flagged (DI-2, PX-016); the plan justifies it.
        json!({"calls": [{"name": "plan.update", "args": {"outcome": "edit a", "expected_files": ["a.txt", "pnpm-lock.yaml"], "reason": "pnpm-lock.yaml: a comment only, no dependency changed"}}]}),
        json!({"calls": [{"name": "task.complete", "args": {"summary": "done", "self_review": {"findings": []}}}]}),
    ];
    let (base, seen) = scripted_model(script, None).await;
    let dir = tempfile::tempdir().unwrap();
    let core = CoreProcess::spawn_with_env(
        dir.path(),
        &[
            ("MODBIT_OPENAI_BASE_URL", &base),
            ("OPENAI_API_KEY", ""),
            ("ANTHROPIC_API_KEY", ""),
        ],
    );
    let mut c = core.client().await;
    let (session, _) = create_session(&mut c, id16(0x38)).await;
    let g = lease_for(&session);
    let ack = c
        .command(envelope_fenced(
            id16(0x39),
            "CreateTask",
            CreateTask {
                session_id: Some(session.clone()),
                goal_text: "edit a and bump the lockfile".into(),
                workspace_id: None,
                execution_profile: String::new(),
                origin: "desktop".into(),
                workspace_root: root.clone(),
            }
            .encode_to_vec(),
            g,
        ))
        .await
        .unwrap();
    let task = Client::result::<TaskCreated>(&ack)
        .unwrap()
        .task_id
        .unwrap();
    let start = StartTask {
        task_id: Some(task.clone()),
        endpoint: String::new(),
        model: "gpt-5-mini".into(),
        max_turns: 20,
        max_tool_calls: 0,
        max_no_progress_turns: 5,
    }
    .encode_to_vec();
    let ack = c
        .command(envelope_fenced(id16(0x3A), "StartTask", start.clone(), g))
        .await
        .unwrap();
    let _: TaskRunStarted = Client::result(&ack).unwrap();
    let st = wait_task(&mut c, &task, 120).await;
    let evs = task_events(&core, &session, &task).await;
    assert_eq!(
        (st.state.as_str(), st.wait_reason.as_str()),
        ("Waiting", "UserInput"),
        "the run suspends on the question: {st:?}\n{evs:#?}"
    );
    let of = |evs: &Vec<(String, String, serde_json::Value)>, t: &str| {
        evs.iter()
            .filter(|(_, x, _)| x == t)
            .map(|(_, _, p)| p.clone())
            .collect::<Vec<_>>()
    };
    // The first plan froze the write set; the revision carries its delta and reason.
    let revised = of(&evs, "PlanRevised");
    assert_eq!(revised.len(), 1, "{revised:#?}");
    assert_eq!(revised[0]["added"], json!(["pnpm-lock.yaml"]));
    assert_eq!(revised[0]["reason"], "the lockfile needs a bump");
    // Two refusals, one ScopeExpansionRecorded per refused attempt, none applied.
    let expansions = of(&evs, "ScopeExpansionRecorded");
    assert_eq!(expansions.len(), 2, "{expansions:#?}");
    for e in &expansions {
        assert_eq!(e["resolution"], "QUESTION_REQUIRED", "{e}");
        assert_eq!(e["paths"], json!(["pnpm-lock.yaml"]));
        assert!(
            e["reason"].as_str().unwrap().contains("always_ask_paths"),
            "{e}"
        );
        assert_eq!(e["out_of_plan_files"], 0);
        assert_eq!(e["plan_revisions"], 1);
        assert_eq!(e["answer"], "");
    }
    assert_eq!(
        of(&evs, "FileChanged")
            .iter()
            .filter(|f| f["path"] == "pnpm-lock.yaml")
            .count(),
        0,
        "nothing was written before the answer"
    );
    // The user answers: continue.
    let ack = c
        .command(envelope(
            id16(0x3B),
            "ListQuestions",
            ListQuestions {
                task_id: Some(task.clone()),
            }
            .encode_to_vec(),
        ))
        .await
        .unwrap();
    let l: QuestionList = Client::result(&ack).unwrap();
    let q = &l.questions[0];
    assert_eq!(
        q.options.iter().map(|o| o.id.as_str()).collect::<Vec<_>>(),
        ["continue", "split", "stop"]
    );
    let ack = c
        .command(envelope_fenced(
            id16(0x3C),
            "RespondToQuestion",
            RespondToQuestion {
                task_id: Some(task.clone()),
                question_id: q.question_id.clone(),
                option_id: "continue".into(),
                text: String::new(),
            }
            .encode_to_vec(),
            g,
        ))
        .await
        .unwrap();
    let _: QuestionResponded = Client::result(&ack).unwrap();
    let ack = c
        .command(envelope_fenced(id16(0x3D), "StartTask", start, g))
        .await
        .unwrap();
    let _: TaskRunStarted = Client::result(&ack).unwrap();
    let st = wait_for_state(&mut c, &task, "ReadyForReview", 120).await;
    let evs = task_events(&core, &session, &task).await;
    assert_eq!(st.state, "ReadyForReview", "{st:?}\n{evs:#?}");
    let expansions = of(&evs, "ScopeExpansionRecorded");
    let answered = expansions
        .iter()
        .find(|e| e["resolution"] == "CONTINUE")
        .unwrap_or_else(|| panic!("{expansions:#?}"));
    assert_eq!(answered["paths"], json!(["pnpm-lock.yaml"]));
    assert!(
        answered["answer"].as_str().unwrap().contains("continue"),
        "{answered}"
    );
    assert_eq!(
        answered["out_of_plan_files"], 0,
        "counters against the original plan"
    );
    assert_eq!(answered["plan_revisions"], 1);
    assert_eq!(
        of(&evs, "FileChanged")
            .iter()
            .filter(|f| f["path"] == "pnpm-lock.yaml")
            .count(),
        1,
        "the answered expansion was applied once"
    );
    assert_eq!(
        std::fs::read_to_string(repo.path().join("pnpm-lock.yaml")).unwrap(),
        "lockfileVersion: 9\n# bumped\n"
    );
    let _ = seen;
    drop(c);

    // ---- headless task: the out-of-plan bound fails closed
    let (repo2, root2) = plain_repo(&[
        ("a.txt", "a\n"),
        ("b.txt", "b\n"),
        ("c.txt", "c\n"),
        ("d.txt", "d\n"),
    ]);
    let plan = |files: Vec<&str>, reason: &str| json!({"calls": [{"name": "plan.update", "args": {"outcome": "edit files", "expected_files": files, "reason": reason}}]});
    let read = |p: &str| json!({"calls": [{"name": "fs.read", "args": {"path": p}}]});
    let write = |p: &str| json!({"calls": [{"name": "change.apply", "args": {"path": p, "op": "replace", "content": "edited\n"}}]});
    let script2 = vec![
        plan(vec!["a.txt"], "original"),
        read("a.txt"),
        write("a.txt"),
        plan(vec!["a.txt", "b.txt"], "b too"),
        read("b.txt"),
        write("b.txt"),
        plan(vec!["a.txt", "b.txt", "c.txt"], "c too"),
        read("c.txt"),
        write("c.txt"),
        plan(vec!["a.txt", "b.txt", "c.txt", "d.txt"], "d too"),
        write("d.txt"),
        json!({"calls": [{"name": "task.complete", "args": {"summary": "never reached", "self_review": {"findings": []}}}]}),
    ];
    let (base2, _seen2) = scripted_model(script2, None).await;
    let dir2 = tempfile::tempdir().unwrap();
    let core2 = CoreProcess::spawn_with_env(
        dir2.path(),
        &[
            ("MODBIT_OPENAI_BASE_URL", &base2),
            ("OPENAI_API_KEY", ""),
            ("ANTHROPIC_API_KEY", ""),
        ],
    );
    let mut c2 = core2.client().await;
    let (session2, _) = create_session(&mut c2, id16(0x3E)).await;
    let g2 = lease_for(&session2);
    let ack = c2
        .command(envelope_fenced(
            id16(0x3F),
            "CreateTask",
            CreateTask {
                session_id: Some(session2.clone()),
                goal_text: "edit several files".into(),
                workspace_id: None,
                execution_profile: String::new(),
                origin: "cli".into(),
                workspace_root: root2.clone(),
            }
            .encode_to_vec(),
            g2,
        ))
        .await
        .unwrap();
    let task2 = Client::result::<TaskCreated>(&ack)
        .unwrap()
        .task_id
        .unwrap();
    let ack = c2
        .command(envelope_fenced(
            id16(0x40),
            "StartTask",
            StartTask {
                task_id: Some(task2.clone()),
                endpoint: String::new(),
                model: "gpt-5-mini".into(),
                max_turns: 30,
                max_tool_calls: 0,
                max_no_progress_turns: 5,
            }
            .encode_to_vec(),
            g2,
        ))
        .await
        .unwrap();
    let _: TaskRunStarted = Client::result(&ack).unwrap();
    let st2 = wait_task(&mut c2, &task2, 180).await;
    let evs2 = task_events(&core2, &session2, &task2).await;
    assert_eq!(
        (st2.state.as_str(), st2.wait_reason.as_str()),
        ("Waiting", "UserInput"),
        "{st2:?}\n{evs2:#?}"
    );
    // b.txt and c.txt were inside the bound; d.txt reached it and failed closed.
    let written: Vec<&str> = evs2
        .iter()
        .filter(|(_, t, _)| t == "FileChanged")
        .map(|(_, _, p)| p["path"].as_str().unwrap())
        .collect();
    assert_eq!(written, ["a.txt", "b.txt", "c.txt"], "{written:?}");
    let expansions2 = of(&evs2, "ScopeExpansionRecorded");
    assert_eq!(expansions2.len(), 1, "{expansions2:#?}");
    assert_eq!(
        expansions2[0]["resolution"], "FAIL_CLOSED",
        "{expansions2:#?}"
    );
    assert_eq!(expansions2[0]["paths"], json!(["d.txt"]));
    assert_eq!(expansions2[0]["out_of_plan_files"], 2);
    assert_eq!(expansions2[0]["plan_revisions"], 3);
    assert!(
        expansions2[0]["reason"]
            .as_str()
            .unwrap()
            .contains("outside the original plan"),
        "{expansions2:#?}"
    );
    let attention = of(&evs2, "TaskNeedsAttention");
    assert!(
        attention
            .iter()
            .any(|a| a["reason"].as_str().unwrap().contains("FAIL_CLOSED")),
        "{attention:#?}"
    );
    assert!(
        of(&evs2, "SelfReviewRecorded").is_empty(),
        "task.complete never ran"
    );
    let _ = (repo, repo2);
}

/// PX-039 repair policy: the Alpha defaults come from the versioned policy; a
/// goal that reports a failure whose verification reproduces nothing is
/// UNREPRODUCED, and a fix is refused until the plan states the limitation;
/// three consecutive turns with no transaction, verification, retrieval, plan
/// revision or question emit NoProgressDetected and move the task to Needs
/// Attention.
#[tokio::test]
async fn qual_px_039_reproduction_first_is_enforced_and_no_progress_turns_escalate() {
    use modbit_protocol::v1::{StartTask, TaskRunStarted};
    use serde_json::json;
    // ---- a reported failure that nothing reproduces
    let (repo, root) = plain_repo(&[("app.txt", "value = 1\n")]);
    let script = vec![
        json!({"calls": [{"name": "plan.update", "args": {"outcome": "fix the wrong value", "expected_files": ["app.txt"]}}]}),
        json!({"calls": [{"name": "fs.read", "args": {"path": "app.txt"}}]}),
        // the mandatory baseline runs here and reproduces nothing
        json!({"calls": [{"name": "change.apply", "args": {"path": "app.txt", "op": "replace", "content": "value = 2\n"}}]}),
        // refused: the reported failure is UNREPRODUCED
        json!({"calls": [{"name": "change.apply", "args": {"path": "app.txt", "op": "replace", "content": "value = 2\n"}}]}),
        // the plan states the limitation
        json!({"calls": [{"name": "plan.update", "args": {"outcome": "fix the wrong value", "expected_files": ["app.txt"], "reason": "the failure is UNREPRODUCED here: this repository has no test runner; the value is wrong by inspection"}}]}),
        json!({"calls": [{"name": "change.apply", "args": {"path": "app.txt", "op": "replace", "content": "value = 2\n"}}]}),
        json!({"calls": [{"name": "task.complete", "args": {"summary": "done", "self_review": {"findings": []}}}]}),
    ];
    let (base, _seen) = scripted_model(script, None).await;
    let dir = tempfile::tempdir().unwrap();
    let core = CoreProcess::spawn_with_env(
        dir.path(),
        &[
            ("MODBIT_OPENAI_BASE_URL", &base),
            ("OPENAI_API_KEY", ""),
            ("ANTHROPIC_API_KEY", ""),
        ],
    );
    let mut c = core.client().await;
    let (session, _) = create_session(&mut c, id16(0x50)).await;
    let g = lease_for(&session);
    let ack = c
        .command(envelope_fenced(
            id16(0x51),
            "CreateTask",
            CreateTask {
                session_id: Some(session.clone()),
                goal_text: "app.txt has the wrong value; the check fails".into(),
                workspace_id: None,
                execution_profile: String::new(),
                origin: "cli".into(),
                workspace_root: root.clone(),
            }
            .encode_to_vec(),
            g,
        ))
        .await
        .unwrap();
    let task = Client::result::<TaskCreated>(&ack)
        .unwrap()
        .task_id
        .unwrap();
    let ack = c
        .command(envelope_fenced(
            id16(0x52),
            "StartTask",
            StartTask {
                task_id: Some(task.clone()),
                endpoint: String::new(),
                model: "gpt-5-mini".into(),
                max_turns: 20,
                max_tool_calls: 0,
                // 0 keeps the policy's own bound (3), not a second copy.
                max_no_progress_turns: 0,
            }
            .encode_to_vec(),
            g,
        ))
        .await
        .unwrap();
    let _: TaskRunStarted = Client::result(&ack).unwrap();
    let st = wait_task(&mut c, &task, 120).await;
    let evs = task_events(&core, &session, &task).await;
    let of = |evs: &Vec<(String, String, serde_json::Value)>, t: &str| {
        evs.iter()
            .filter(|(_, x, _)| x == t)
            .map(|(_, _, p)| p.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(st.state, "ReadyForReview", "{st:?}\n{evs:#?}");
    let repro = of(&evs, "ReproductionRecorded");
    assert_eq!(
        repro
            .iter()
            .map(|r| r["status"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["UNREPRODUCED", "WAIVED"],
        "{repro:#?}"
    );
    assert!(
        repro[1]["note"].as_str().unwrap().contains("UNREPRODUCED"),
        "{repro:#?}"
    );
    // Exactly one write landed: the first attempt was refused, the second waited
    // for the plan's limitation.
    let written = of(&evs, "FileChanged");
    assert_eq!(written.len(), 1, "{written:#?}");
    assert_eq!(
        std::fs::read_to_string(repo.path().join("app.txt")).unwrap(),
        "value = 2\n"
    );
    assert_eq!(
        evs.iter()
            .filter(|(_, t, p)| t == "StepFailed"
                && p["failure_code"] == "HARNESS_REPRODUCTION_REQUIRED")
            .count(),
        2,
        "both fixes before the plan's limitation were refused: {evs:#?}"
    );
    drop(c);

    // ---- three turns without progress
    let (repo2, root2) = plain_repo(&[("a.txt", "a\n")]);
    let browse = json!({"calls": [{"name": "search.exact", "args": {"query": "a"}}]});
    let script2 = vec![
        json!({"calls": [{"name": "plan.update", "args": {"outcome": "look around", "expected_files": ["a.txt"]}}]}),
        browse.clone(),
        browse.clone(),
        browse.clone(),
        json!({"calls": [{"name": "task.complete", "args": {"summary": "never reached", "self_review": {"findings": []}}}]}),
    ];
    let (base2, _s2) = scripted_model(script2, None).await;
    let dir2 = tempfile::tempdir().unwrap();
    let core2 = CoreProcess::spawn_with_env(
        dir2.path(),
        &[
            ("MODBIT_OPENAI_BASE_URL", &base2),
            ("OPENAI_API_KEY", ""),
            ("ANTHROPIC_API_KEY", ""),
        ],
    );
    let mut c2 = core2.client().await;
    let (session2, _) = create_session(&mut c2, id16(0x53)).await;
    let g2 = lease_for(&session2);
    let task2 =
        create_task_with_profile(&mut c2, &session2, g2, &root2, 0x54, "local_trusted").await;
    let ack = c2
        .command(envelope_fenced(
            id16(0x55),
            "StartTask",
            StartTask {
                task_id: Some(task2.clone()),
                endpoint: String::new(),
                model: "gpt-5-mini".into(),
                max_turns: 20,
                max_tool_calls: 0,
                max_no_progress_turns: 0,
            }
            .encode_to_vec(),
            g2,
        ))
        .await
        .unwrap();
    let _: TaskRunStarted = Client::result(&ack).unwrap();
    let st2 = wait_task(&mut c2, &task2, 120).await;
    let evs2 = task_events(&core2, &session2, &task2).await;
    assert_eq!(
        (st2.state.as_str(), st2.wait_reason.as_str()),
        ("Waiting", "UserInput"),
        "{st2:?}\n{evs2:#?}"
    );
    let np = of(&evs2, "NoProgressDetected");
    assert_eq!(np.len(), 1, "{np:#?}");
    assert_eq!(np[0]["turns"], 3, "the policy's Alpha default: {np:#?}");
    assert!(
        of(&evs2, "TaskNeedsAttention")
            .iter()
            .any(|a| a["reason"].as_str().unwrap().contains("without progress")),
        "{evs2:#?}"
    );
    assert!(
        of(&evs2, "SelfReviewRecorded").is_empty(),
        "task.complete never ran"
    );
    let _ = (repo, repo2);
}

/// PX-035: the impact selector chooses tests from the evidence graph — test
/// links, import dependencies, symbol references and Git co-change — within a
/// bounded depth, names the evidence behind each one, states that targeting is
/// heuristic, and its precision and recall against the fixture's full-suite
/// ground truth are recorded.
#[tokio::test]
async fn qual_px_035_impact_selection_chooses_tests_from_graph_evidence_with_measured_precision_and_recall()
 {
    let (repo, root) = fixture_repo("rust-cli");
    let dir = tempfile::tempdir().unwrap();
    let core = CoreProcess::spawn(dir.path());
    let mut c = core.client().await;
    let (session, _) = create_session(&mut c, id16(0x35)).await;
    let g = lease_for(&session);
    let task = create_task_with_profile(&mut c, &session, g, &root, 0x36, "local_trusted").await;
    // A change to the library selects the suite that covers it.
    let r = invoke_tool(
        &mut c,
        &task,
        g,
        0x37,
        0x41,
        "search.impact",
        r#"{"paths":["src/lib.rs"],"depth":2}"#,
    )
    .await;
    assert_eq!(r.status, "SUCCESS", "{r:?}");
    let so: serde_json::Value = serde_json::from_str(&r.structured_output_json).unwrap();
    let sel = &so["selection"];
    assert_eq!(sel["changed"], serde_json::json!(["src/lib.rs"]));
    assert_eq!(sel["depth"], 2);
    assert!(
        sel["limitation"].as_str().unwrap().contains("heuristic"),
        "{so}"
    );
    let selected: Vec<String> = sel["tests"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["path"].as_str().unwrap().to_owned())
        .collect();
    assert!(
        selected.contains(&"tests/quantities.rs".to_owned()),
        "the suite that covers the change: {so}"
    );
    let picked = sel["tests"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["path"] == "tests/quantities.rs")
        .unwrap();
    let reasons: Vec<&str> = picked["reasons"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_str().unwrap())
        .collect();
    assert!(
        reasons
            .iter()
            .any(|r| ["test_link", "dependency", "symbol_reference", "cochange"].contains(r)),
        "{picked}"
    );
    assert!(
        sel["symbols"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s == "parse_quantity"),
        "{so}"
    );
    // Ground truth: the fixture's whole suite lives in tests/quantities.rs, so
    // the selection is measured against it (recall must be 1.0 — a selector
    // that omits a covering test fails).
    let truth = ["tests/quantities.rs".to_owned()];
    let hits = selected.iter().filter(|s| truth.contains(s)).count() as f32;
    let precision = hits / selected.len() as f32;
    let recall = hits / truth.len() as f32;
    eprintln!(
        "PX-035 impact selection on rust-cli: precision={precision:.2} recall={recall:.2} selected={selected:?}"
    );
    assert_eq!(recall, 1.0, "{selected:?}");
    assert!(precision > 0.0);
    // Changing a test file selects that file itself.
    let r = invoke_tool(
        &mut c,
        &task,
        g,
        0x38,
        0x42,
        "search.impact",
        r#"{"paths":["tests/quantities.rs"]}"#,
    )
    .await;
    let so: serde_json::Value = serde_json::from_str(&r.structured_output_json).unwrap();
    let first = &so["selection"]["tests"][0];
    assert_eq!(first["path"], "tests/quantities.rs", "{so}");
    assert_eq!(first["reasons"][0], "changed");
    assert_eq!(first["distance"], 0);
    // A selection with no changed path is refused by the tool schema before
    // any index work.
    let r = invoke_tool(
        &mut c,
        &task,
        g,
        0x39,
        0x43,
        "search.impact",
        r#"{"paths":[]}"#,
    )
    .await;
    assert_eq!(
        (r.status.as_str(), r.error_code.as_str()),
        ("INVALID_ARGUMENTS", "SCHEMA_VIOLATION"),
        "{r:?}"
    );
    let _ = repo;
}

/// IMP-EV-0169 / REQ-EV-0169: the Context Pack the task compiled enters the
/// next prompt with its provenance — source, path, workspace revision, the
/// content hash it was read at and the retrieval reason — and the envelope
/// injects nothing without it; the compiled context is recorded on the
/// ContextCompile step so a reader can check what the model saw.
#[tokio::test]
async fn qual_ev_0169_context_pack_reaches_the_prompt_with_provenance_or_not_at_all() {
    use modbit_protocol::v1::{StartTask, TaskRunStarted};
    use serde_json::json;
    let (repo, root) = plain_repo(&[
        (
            "src/cart.rs",
            "pub fn total_cents(q: u32, u: u32) -> u32 {\n    q * u\n}\n",
        ),
        ("NOTES.md", "totals are computed in cents\n"),
    ]);
    let script = vec![
        json!({"calls": [{"name": "context.pack", "args": {"query": "total_cents", "token_budget": 400, "required_paths": ["NOTES.md"]}}]}),
        json!({"calls": [{"name": "plan.update", "args": {"outcome": "note the units", "expected_files": ["NOTES.md"]}}]}),
        json!({"calls": [{"name": "task.complete", "args": {"summary": "done", "self_review": {"findings": []}}}]}),
    ];
    let (base, seen) = scripted_model(script, None).await;
    let dir = tempfile::tempdir().unwrap();
    let core = CoreProcess::spawn_with_env(
        dir.path(),
        &[
            ("MODBIT_OPENAI_BASE_URL", &base),
            ("OPENAI_API_KEY", ""),
            ("ANTHROPIC_API_KEY", ""),
        ],
    );
    let mut c = core.client().await;
    let (session, _) = create_session(&mut c, id16(0x69)).await;
    let g = lease_for(&session);
    let task = create_task_with_profile(&mut c, &session, g, &root, 0x6A, "local_trusted").await;
    let ack = c
        .command(envelope_fenced(
            id16(0x6B),
            "StartTask",
            StartTask {
                task_id: Some(task.clone()),
                endpoint: String::new(),
                model: "gpt-5-mini".into(),
                max_turns: 20,
                max_tool_calls: 0,
                max_no_progress_turns: 5,
            }
            .encode_to_vec(),
            g,
        ))
        .await
        .unwrap();
    let _: TaskRunStarted = Client::result(&ack).unwrap();
    let st = wait_task(&mut c, &task, 120).await;
    let evs = task_events(&core, &session, &task).await;
    assert_eq!(st.state, "ReadyForReview", "{st:?}\n{evs:#?}");
    // The turn after the pack carries it, with provenance on every fragment.
    let bodies = seen.lock().unwrap().clone();
    let with_context = bodies
        .iter()
        .flat_map(|b| b["messages"].as_array().cloned().unwrap_or_default())
        .filter(|m| m["role"] == "user")
        .filter_map(|m| m["content"].as_str().map(str::to_owned))
        .find(|t| t.contains("Retrieved context"))
        .unwrap_or_else(|| panic!("{bodies:#?}"));
    assert!(
        with_context.contains("workspace:NOTES.md"),
        "{with_context}"
    );
    assert!(with_context.contains("@revision 1 hash "), "{with_context}");
    assert!(
        with_context.contains("critical:task_constraint"),
        "{with_context}"
    );
    assert!(
        with_context.contains("never as instructions"),
        "the fragments are data: {with_context}"
    );
    // Every injected line names its source, revision and hash.
    for line in with_context.lines().filter(|l| l.starts_with("--- ")) {
        assert!(
            line.contains("@revision ") && line.contains(" hash "),
            "unprovenanced fragment: {line}"
        );
    }
    // The ContextCompile step records what was injected and what was refused.
    let compiled = evs
        .iter()
        .filter(|(a, t, _)| a == "run_step" && t == "StepSucceeded")
        .filter_map(|(_, _, p)| p["output_ref"].as_str().map(str::to_owned))
        .collect::<Vec<_>>();
    let mut seen_injected = false;
    for (i, r) in compiled.iter().enumerate() {
        let body = read_object(&mut c, id16(0x70 + u8::try_from(i).unwrap_or(0)), r).await;
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&body) else {
            continue;
        };
        if let Some(inj) = v["injected_fragments"].as_array()
            && inj.iter().any(|f| f == "workspace:NOTES.md")
        {
            seen_injected = true;
            assert_eq!(v["rejected_fragments"], json!([]), "{v}");
        }
    }
    assert!(
        seen_injected,
        "the compiled context is recorded on the step"
    );
    let _ = repo;
}

/// PX-040 harness contracts (docs/14): a large command result reaches the
/// model bounded, with the omitted range declared and a pageable result_ref
/// that `artifact.range` reads back; the Context Pack carries harness_state
/// with the plan, budgets, scope counters and candidate revision; a missing
/// runner is recorded as a plan limitation; and a question in a headless task
/// fails closed to Waiting with Needs Attention instead of hanging.
#[tokio::test]
async fn qual_px_040_harness_contracts_bound_observations_page_results_and_fail_closed_headless() {
    use modbit_protocol::v1::{StartTask, TaskRunStarted};
    use serde_json::json;
    let (repo, root) = plain_repo(&[("a.txt", "a\n")]);
    // A command whose output is far past the inline ceiling.
    std::fs::write(
        repo.path().join("noisy.sh"),
        "#!/bin/sh\ni=0\nwhile [ $i -lt 900 ]; do echo \"line $i: 0123456789012345678901234567890123456789\"; i=$((i+1)); done\n",
    )
    .unwrap();
    let script = vec![
        json!({"calls": [{"name": "plan.update", "args": {"outcome": "look at the noisy output", "expected_files": ["a.txt"], "verification": ["sh noisy.sh"]}}]}),
        json!({"calls": [{"name": "test.run", "args": {"argv": ["sh", "noisy.sh"], "inherit_env": true}}]}),
        json!({"calls": [{"name": "user.ask", "args": {"question": "The runner is missing. Configure one, or continue without test evidence?", "options": [{"id": "configure", "label": "configure a runner"}, {"id": "continue", "label": "continue with the limitation"}], "reason": "verification"}}]}),
        json!({"calls": [{"name": "task.complete", "args": {"summary": "never reached", "self_review": {"findings": []}}}]}),
    ];
    let (base, seen) = scripted_model(script, None).await;
    let dir = tempfile::tempdir().unwrap();
    let core = CoreProcess::spawn_with_env(
        dir.path(),
        &[
            ("MODBIT_OPENAI_BASE_URL", &base),
            ("OPENAI_API_KEY", ""),
            ("ANTHROPIC_API_KEY", ""),
        ],
    );
    let mut c = core.client().await;
    let (session, _) = create_session(&mut c, id16(0x40)).await;
    let g = lease_for(&session);
    let task = create_task_with_profile(&mut c, &session, g, &root, 0x44, "local_trusted").await;
    let ack = c
        .command(envelope_fenced(
            id16(0x45),
            "StartTask",
            StartTask {
                task_id: Some(task.clone()),
                endpoint: String::new(),
                model: "gpt-5-mini".into(),
                max_turns: 20,
                max_tool_calls: 0,
                max_no_progress_turns: 5,
            }
            .encode_to_vec(),
            g,
        ))
        .await
        .unwrap();
    let _: TaskRunStarted = Client::result(&ack).unwrap();
    let st = wait_task(&mut c, &task, 120).await;
    let evs = task_events(&core, &session, &task).await;
    // Contract 10 (headless resolution): the question suspends the run and the
    // task needs attention; it never blocks forever and never self-answers.
    assert_eq!(
        (
            st.state.as_str(),
            st.wait_reason.as_str(),
            st.run_state.as_str(),
            st.loop_alive
        ),
        ("Waiting", "UserInput", "Suspended", false),
        "{st:?}\n{evs:#?}"
    );
    assert!(
        evs.iter().any(|(_, t, p)| t == "TaskNeedsAttention"
            && p["reason"].as_str().unwrap().contains("question pending")),
        "{evs:#?}"
    );
    assert!(
        evs.iter().any(|(_, t, _)| t == "UserQuestionAsked"),
        "{evs:#?}"
    );
    assert!(
        !evs.iter().any(|(_, t, _)| t == "UserQuestionAnswered"),
        "the agent never answers its own question"
    );
    assert!(
        evs.iter().all(|(_, t, _)| t != "SelfReviewRecorded"),
        "completion never ran"
    );
    // Contract 2 (bounded observations): the large result is truncated with the
    // omitted range declared and a pageable ref.
    let bodies = seen.lock().unwrap().clone();
    let observation = bodies
        .iter()
        .flat_map(|b| b["messages"].as_array().cloned().unwrap_or_default())
        .filter(|m| m["role"] == "tool")
        .filter_map(|m| m["content"].as_str().map(str::to_owned))
        .find(|t| t.contains("bytes_total:") && t.contains("omitted:"))
        .unwrap_or_else(|| panic!("{bodies:#?}"));
    let total: usize = observation
        .lines()
        .find_map(|l| l.strip_prefix("bytes_total: "))
        .unwrap()
        .parse()
        .unwrap();
    assert!(total > 16 * 1024, "{observation}");
    assert!(observation.contains("omitted: 16384..") && observation.contains("artifact.range"));
    let result_ref = observation
        .split("result_ref ")
        .nth(1)
        .unwrap()
        .split(|c: char| !c.is_ascii_hexdigit())
        .next()
        .unwrap()
        .to_owned();
    assert_eq!(result_ref.len(), 64, "{observation}");
    // The rest is readable through the tool the observation names.
    let r = invoke_tool(
        &mut c,
        &task,
        g,
        0x46,
        0x47,
        "artifact.range",
        &json!({"ref": result_ref, "offset": 16384, "max_bytes": 4096}).to_string(),
    )
    .await;
    assert_eq!(r.status, "SUCCESS", "{r:?}");
    let page: serde_json::Value = serde_json::from_str(&r.structured_output_json).unwrap();
    assert_eq!(page["offset"], 16384);
    // The stored result holds at least everything the observation counted (it
    // is the whole tool result, the observation only its text).
    let stored_total = page["bytes_total"].as_u64().unwrap() as usize;
    assert!(
        stored_total >= total,
        "stored {stored_total} < observed {total}"
    );
    assert!(
        page["bytes_read"].as_u64().unwrap() > 0 && page["eof"] == false,
        "{page}"
    );
    assert!(
        page["content"].as_str().unwrap().contains("line "),
        "{page}"
    );
    let r = invoke_tool(
        &mut c,
        &task,
        g,
        0x48,
        0x49,
        "artifact.range",
        &json!({"ref": result_ref, "offset": stored_total as u64 - 10, "max_bytes": 4096})
            .to_string(),
    )
    .await;
    let last: serde_json::Value = serde_json::from_str(&r.structured_output_json).unwrap();
    assert_eq!(last["eof"], true, "{last}");
    // Contract 4 (harness_state in the Context Pack) and the plan limitation
    // for the missing runner.
    let refs: Vec<String> = evs
        .iter()
        .filter(|(a, t, _)| a == "run_step" && t == "StepSucceeded")
        .filter_map(|(_, _, p)| p["output_ref"].as_str().map(str::to_owned))
        .collect();
    let mut saw_state = false;
    for (i, r) in refs.iter().enumerate() {
        let body = read_object(&mut c, id16(0x50 + u8::try_from(i).unwrap_or(0)), r).await;
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&body) else {
            continue;
        };
        if let Some(h) = v.get("harness_state")
            && h["plan"].is_object()
        {
            saw_state = true;
            assert!(h["budgets"]["max_turns"].as_u64().unwrap() > 0, "{h}");
            assert!(h["open_failures"].is_array(), "{h}");
            assert!(h["out_of_plan_files"].is_array(), "{h}");
            assert!(h["quarantined"].is_array(), "{h}");
            assert!(
                h["scope_policy"]["max_out_of_plan_files_without_question"].is_u64(),
                "{h}"
            );
            assert!(h["repair_policy"]["max_attempts_per_task"].is_u64(), "{h}");
        }
    }
    assert!(saw_state, "harness_state travels with the Context Pack");
    let baseline = evs
        .iter()
        .find(|(_, t, _)| t == "VerificationBaselineRecorded")
        .map(|(_, _, p)| p.clone());
    if let Some(b) = baseline
        && let Some(plan_ref) = b["plan_ref"].as_str()
    {
        let plan = read_object(&mut c, id16(0x60), plan_ref).await;
        let plan: serde_json::Value = serde_json::from_str(&plan).unwrap();
        let limitations = plan["limitations"].as_array().unwrap();
        assert!(
            limitations
                .iter()
                .any(|l| l.as_str().unwrap().contains("no configured runner")),
            "the missing runner is recorded: {plan}"
        );
    }
    let _ = repo;
}

/// IMP-EV-0035 / 0131 / 0175 (Context Inspector): every client can see what
/// the Context Pack selected and excluded, with the reason, source, revision,
/// freshness and token cost of each entry — and the inspector's ids and totals
/// are the prompt envelope's, not a second story.
#[tokio::test]
async fn qual_ev_0035_0131_0175_context_inspector_matches_the_prompt_envelope() {
    use modbit_protocol::v1::{
        ContextInspectorView, GetContextInspector, StartTask, TaskRunStarted,
    };
    use serde_json::json;
    let (repo, root) = plain_repo(&[
        (
            "src/cart.rs",
            "pub fn total_cents(q: u32, u: u32) -> u32 {\n    q * u\n}\n",
        ),
        ("NOTES.md", "totals are computed in cents\n"),
        ("BIG.md", &"filler line about totals\n".repeat(80)),
    ]);
    let script = vec![
        json!({"calls": [{"name": "context.pack", "args": {"query": "total_cents", "token_budget": 120, "required_paths": ["NOTES.md"]}}]}),
        json!({"calls": [{"name": "fs.read", "args": {"path": "NOTES.md"}}]}),
        json!({"calls": [{"name": "plan.update", "args": {"outcome": "note the units", "expected_files": ["NOTES.md"]}}]}),
        json!({"calls": [{"name": "task.complete", "args": {"summary": "done", "self_review": {"findings": []}}}]}),
    ];
    let (base, seen) = scripted_model(script, None).await;
    let dir = tempfile::tempdir().unwrap();
    let core = CoreProcess::spawn_with_env(
        dir.path(),
        &[
            ("MODBIT_OPENAI_BASE_URL", &base),
            ("OPENAI_API_KEY", ""),
            ("ANTHROPIC_API_KEY", ""),
        ],
    );
    let mut c = core.client().await;
    let (session, _) = create_session(&mut c, id16(0x31)).await;
    let g = lease_for(&session);
    let task = create_task_with_profile(&mut c, &session, g, &root, 0x32, "local_trusted").await;
    let ack = c
        .command(envelope_fenced(
            id16(0x33),
            "StartTask",
            StartTask {
                task_id: Some(task.clone()),
                endpoint: String::new(),
                model: "gpt-5-mini".into(),
                max_turns: 20,
                max_tool_calls: 0,
                max_no_progress_turns: 5,
            }
            .encode_to_vec(),
            g,
        ))
        .await
        .unwrap();
    let _: TaskRunStarted = Client::result(&ack).unwrap();
    let st = wait_task(&mut c, &task, 120).await;
    assert_eq!(st.state, "ReadyForReview", "{st:?}");
    let ack = c
        .command(envelope(
            id16(0x34),
            "GetContextInspector",
            GetContextInspector {
                task_id: Some(task.clone()),
            }
            .encode_to_vec(),
        ))
        .await
        .unwrap();
    let v: ContextInspectorView = Client::result(&ack).unwrap();
    // Composition, budget and estimator (IMP-EV-0131).
    assert!(!v.pack_id.is_empty(), "{v:?}");
    assert_eq!(v.token_budget, 120);
    assert!(v.token_used <= v.token_budget, "{v:?}");
    assert_eq!(v.token_estimator, "bytes/4");
    assert!(v.complete, "{v:?}");
    assert!(v.workspace_revision > 0);
    // Selection with reasons, sources, freshness and cost (IMP-EV-0035).
    let notes = v
        .entries
        .iter()
        .find(|e| e.path == "NOTES.md")
        .unwrap_or_else(|| panic!("{v:?}"));
    assert_eq!(notes.reason, "critical:task_constraint");
    assert!(
        notes.sources.iter().any(|s| s == "task_constraint"),
        "{notes:?}"
    );
    assert_eq!(notes.freshness, "committed");
    assert!(notes.token_cost > 0 && notes.content_hash.len() == 64);
    assert!(notes.injected, "the entry reached the envelope: {v:?}");
    assert!(notes.used, "the later fs.read used it: {v:?}");
    // Exclusions are visible (IMP-EV-0175): something was left out or stubbed
    // under this budget, and the inspector says which.
    assert!(
        v.omitted_count > 0 || v.entries.iter().any(|e| e.stub),
        "the budget excluded something and the inspector shows it: {v:?}"
    );
    if v.omitted_count > 0 {
        assert!(!v.omitted_paths.is_empty(), "{v:?}");
        assert!(v.omitted_tokens > 0, "{v:?}");
    }
    // The inspector's ids are the envelope's ids (IMP-EV-0175 / 0131).
    assert!(!v.injected_refs.is_empty(), "{v:?}");
    assert!(
        v.rejected_refs.is_empty(),
        "nothing lacked provenance: {v:?}"
    );
    let injected_entries: Vec<&str> = v
        .entries
        .iter()
        .filter(|e| e.injected)
        .map(|e| e.source_ref.as_str())
        .collect();
    for r in &v.injected_refs {
        assert!(
            injected_entries.contains(&r.as_str()),
            "{r} missing from the entries: {v:?}"
        );
    }
    let injected_tokens: u64 = v
        .entries
        .iter()
        .filter(|e| e.injected)
        .map(|e| u64::from(e.token_cost))
        .sum();
    assert_eq!(v.injected_tokens, injected_tokens, "{v:?}");
    // And they are the refs the model actually saw in the request.
    let bodies = seen.lock().unwrap().clone();
    let context_message = bodies
        .iter()
        .flat_map(|b| b["messages"].as_array().cloned().unwrap_or_default())
        .filter_map(|m| m["content"].as_str().map(str::to_owned))
        .find(|t| t.contains("Retrieved context"))
        .unwrap_or_else(|| panic!("{bodies:#?}"));
    for r in &v.injected_refs {
        assert!(
            context_message.contains(r.as_str()),
            "{r} was not in the prompt"
        );
    }
    for r in &v.omitted_paths {
        assert!(
            !context_message.contains(&format!("workspace:{r}")),
            "an omitted path reached the prompt: {r}"
        );
    }
    let _ = repo;
}

/// Compaction epochs (REQ-EV-0056 / 0057 / 0058 / 0092 / 0130 / 0111 / 0268):
/// when the model-visible transcript passes its budget the older entries
/// become one epoch projection that keeps the instructions, decisions and
/// handles; the canonical log is untouched, so a Core restart rebuilds exactly
/// the compacted context; the cacheable prefix changes only at the epoch
/// boundary and the inspector reports how often it was reused; and the
/// manifest that installed at one head is refused once the log has moved.
#[tokio::test]
async fn qual_ev_0056_0092_0130_compaction_epoch_preserves_facts_survives_restart_and_moves_the_cache_prefix()
 {
    use modbit_protocol::v1::{StartTask, TaskRunStarted};
    use serde_json::json;
    let (repo, root) = plain_repo(&[("big.txt", &"filler line for the transcript\n".repeat(400))]);
    let read = json!({"calls": [{"name": "fs.read", "args": {"path": "big.txt"}}]});
    let script = vec![
        json!({"calls": [{"name": "plan.update", "args": {"outcome": "read the big file a few times", "expected_files": ["big.txt"]}}]}),
        read.clone(),
        read.clone(),
        read.clone(),
        read.clone(),
        read.clone(),
        json!({"calls": [{"name": "task.complete", "args": {"summary": "done", "self_review": {"findings": []}}}]}),
    ];
    let (base, seen) = scripted_model(script, None).await;
    let dir = tempfile::tempdir().unwrap();
    let env = [
        ("MODBIT_OPENAI_BASE_URL", base.as_str()),
        ("OPENAI_API_KEY", ""),
        ("ANTHROPIC_API_KEY", ""),
        // A small budget so a real run compacts within a few turns.
        ("MODBIT_COMPACTION_TOKEN_BUDGET", "1500"),
    ];
    let mut core = CoreProcess::spawn_with_env(dir.path(), &env);
    let mut c = core.client().await;
    let (session, _) = create_session(&mut c, id16(0x56)).await;
    let g = lease_for(&session);
    let task = create_task_with_profile(&mut c, &session, g, &root, 0x57, "local_trusted").await;
    let ack = c
        .command(envelope_fenced(
            id16(0x58),
            "StartTask",
            StartTask {
                task_id: Some(task.clone()),
                endpoint: String::new(),
                model: "gpt-5-mini".into(),
                max_turns: 12,
                max_tool_calls: 0,
                max_no_progress_turns: 5,
            }
            .encode_to_vec(),
            g,
        ))
        .await
        .unwrap();
    let _: TaskRunStarted = Client::result(&ack).unwrap();
    let st = wait_task(&mut c, &task, 120).await;
    let evs = task_events(&core, &session, &task).await;
    // The run ends on its own terms (the scripted model keys its next step on
    // the number of tool results it sees, which compaction deliberately
    // reduces, so it replays reads until a budget stops it).
    assert!(
        matches!(st.state.as_str(), "ReadyForReview" | "Waiting"),
        "{st:?}\n{evs:#?}"
    );
    assert!(!st.loop_alive, "{st:?}");
    // An epoch opened, with a manifest that keeps the instruction, the plan
    // decision and the handles.
    let epochs: Vec<&serde_json::Value> = evs
        .iter()
        .filter(|(_, t, _)| t == "ContextEpochOpened")
        .map(|(_, _, p)| p)
        .collect();
    assert!(
        !epochs.is_empty(),
        "the transcript passed the budget: {evs:#?}"
    );
    let e = epochs[0];
    assert_eq!(e["epoch"], 1);
    assert!(e["source_entries"].as_u64().unwrap() >= 2, "{e}");
    assert_eq!(e["manifest_hash"].as_str().unwrap().len(), 64);
    let manifest = read_object(&mut c, id16(0x59), e["manifest_ref"].as_str().unwrap()).await;
    let manifest: serde_json::Value = serde_json::from_str(&manifest).unwrap();
    assert_eq!(manifest["manifest_hash"], e["manifest_hash"]);
    assert_eq!(
        manifest["compiler_version"],
        modbit_prompt_compiler::COMPILER_VERSION
    );
    let kinds: Vec<&str> = manifest["preserved"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["kind"].as_str().unwrap())
        .collect();
    assert!(kinds.contains(&"decision"), "the plan survives: {manifest}");
    assert!(kinds.contains(&"handle"), "the refs survive: {manifest}");
    assert!(
        !manifest["resources"].as_array().unwrap().is_empty(),
        "the dropped results stay reachable: {manifest}"
    );
    assert!(manifest["projection_tokens"].as_u64().unwrap() > 0);
    // The model saw the projection instead of the compacted entries.
    let bodies = seen.lock().unwrap().clone();
    let epoch_idx = bodies
        .iter()
        .position(|b| {
            b["messages"].as_array().unwrap().iter().any(|m| {
                m["content"]
                    .as_str()
                    .unwrap_or_default()
                    .contains("Compaction epoch 1")
            })
        })
        .unwrap_or_else(|| panic!("no request carried the epoch: {bodies:#?}"));
    assert!(epoch_idx > 0, "the first request cannot be a compacted one");
    let epoch_message = bodies[epoch_idx]["messages"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|m| m["content"].as_str())
        .find(|t| t.contains("Compaction epoch 1"))
        .unwrap();
    assert!(
        epoch_message.contains("canonical log keeps them in full"),
        "{epoch_message}"
    );
    assert!(epoch_message.contains("artifact.range"), "{epoch_message}");
    // Compaction is real and bounds the transcript: from the first epoch on
    // the model sees the same handful of entries however many turns run, and
    // more entries were summarised away than it still sees.
    let counts: Vec<usize> = bodies
        .iter()
        .map(|b| b["messages"].as_array().unwrap().len())
        .collect();
    assert!(
        counts[epoch_idx..].iter().all(|c| *c <= counts[epoch_idx]),
        "the transcript stays bounded after the epoch: {counts:?}"
    );
    assert!(
        bodies.len() > counts[epoch_idx],
        "the run outran the transcript it shows: {counts:?}"
    );
    let sizes: Vec<usize> = bodies
        .iter()
        .map(|b| serde_json::to_string(&b["messages"]).unwrap().len())
        .collect();
    assert!(
        *sizes.last().unwrap() <= sizes[epoch_idx] * 12 / 10,
        "the transcript stays bounded in bytes too: {sizes:?}"
    );
    let summarised: u64 = epochs
        .iter()
        .map(|e| e["source_entries"].as_u64().unwrap_or(0))
        .sum();
    assert!(
        summarised > *counts.last().unwrap() as u64,
        "more was compacted away than the model still sees: {summarised} vs {counts:?}"
    );
    // REQ-EV-0111 / 0268: the cacheable prefix is stable within an epoch and
    // changes at the boundary (segment 2 is the compaction epoch).
    let mut epoch_segments = Vec::new();
    for (i, r) in evs
        .iter()
        .filter(|(a, t, _)| a == "run_step" && t == "StepSucceeded")
        .filter_map(|(_, _, p)| p["output_ref"].as_str().map(str::to_owned))
        .enumerate()
    {
        let body = read_object(
            &mut c,
            id16(0x5A_u8.wrapping_add(u8::try_from(i).unwrap_or(0))),
            &r,
        )
        .await;
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&body)
            && let Some(seg) = v["segment_hashes"][2].as_str()
        {
            epoch_segments.push(seg.to_owned());
        }
    }
    assert!(epoch_segments.len() >= 3, "{epoch_segments:?}");
    // The prefix changes exactly once per epoch and never inside one: the
    // number of distinct values and the number of transitions both equal the
    // number of epochs plus the pre-compaction prefix.
    let distinct: std::collections::BTreeSet<&String> = epoch_segments.iter().collect();
    assert_eq!(
        distinct.len(),
        epochs.len() + 1,
        "one prefix per epoch plus the pre-compaction one: {epoch_segments:?}"
    );
    let transitions = epoch_segments.windows(2).filter(|w| w[0] != w[1]).count();
    assert_eq!(
        transitions,
        epochs.len(),
        "the prefix is stable within an epoch: {epoch_segments:?}"
    );
    assert_ne!(epoch_segments.first(), epoch_segments.last());
    // The same property read from the other end: the prompt cache key the run
    // routed on (segments 0..3, so the epoch is in it) is reused turn after
    // turn and misses exactly once per epoch.
    let keys: Vec<String> = evs
        .iter()
        .filter(|(a, t, _)| a == "turn" && t == "ModelInvocationStarted")
        .filter_map(|(_, _, p)| p["model_route"]["cache_key"].as_str().map(str::to_owned))
        .collect();
    assert!(keys.len() >= 3, "{keys:?}");
    let misses = keys.windows(2).filter(|w| w[0] != w[1]).count();
    assert_eq!(misses, epochs.len(), "one cache miss per epoch: {keys:?}");
    assert!(
        keys.len() - 1 - misses > 0,
        "the prefix is reused between epochs: {keys:?}"
    );
    // REQ-EV-0092: a restart rebuilds exactly the compacted context — the
    // manifests are still readable and the last one is what the model saw.
    let last_epoch = epochs.last().unwrap();
    let last_projection = bodies
        .iter()
        .rev()
        .flat_map(|b| b["messages"].as_array().cloned().unwrap_or_default())
        .filter_map(|m| m["content"].as_str().map(str::to_owned))
        .find(|t| t.contains("Compaction epoch "))
        .unwrap();
    drop(c);
    core.kill();
    let core2 = CoreProcess::spawn_with_env(dir.path(), &env);
    let mut c2 = core2.client().await;
    let evs2 = task_events(&core2, &session, &task).await;
    assert_eq!(
        evs2.iter()
            .filter(|(_, t, _)| t == "ContextEpochOpened")
            .count(),
        epochs.len(),
        "the log is unchanged by the restart"
    );
    let rebuilt = read_object(
        &mut c2,
        id16(0x60),
        last_epoch["manifest_ref"].as_str().unwrap(),
    )
    .await;
    let rebuilt: serde_json::Value = serde_json::from_str(&rebuilt).unwrap();
    assert_eq!(rebuilt["manifest_hash"], last_epoch["manifest_hash"]);
    // QUAL-EV-0058 on the real system: this is the manifest the run installed,
    // and offering it now — after the log moved on — is refused. It installed
    // only because it was current when it was computed.
    {
        use modbit_compaction::{RejectedCompaction, accept};
        let m: modbit_compaction::CompactionManifest =
            serde_json::from_value(rebuilt.clone()).unwrap();
        assert!(
            accept(
                &m,
                m.previous_epoch,
                m.task_generation,
                m.source_head_offset
            )
            .is_ok(),
            "{m:?}"
        );
        let head_now = wait_task(&mut c2, &task, 5).await.last_offset;
        assert!(head_now > m.source_head_offset, "{head_now} vs {m:?}");
        assert_eq!(
            accept(&m, m.previous_epoch, m.task_generation, head_now),
            Err(RejectedCompaction::SourceAdvanced {
                saw: m.source_head_offset,
                now: head_now
            }),
            "a compaction computed against an older head cannot install"
        );
        assert_eq!(
            accept(&m, Some(m.epoch), m.task_generation, m.source_head_offset),
            Err(RejectedCompaction::NotSuccessor {
                installed: m.epoch,
                offered: m.epoch
            }),
            "and an epoch never installs twice"
        );
    }
    assert!(
        last_projection.contains(rebuilt["projection"].as_str().unwrap()),
        "the rebuilt projection is the one the model saw"
    );
    let r = invoke_tool(&mut c2, &task, g, 0x5F, 0x61, "context.ledger", "{}").await;
    assert_eq!(r.status, "SUCCESS", "{r:?}");
    // And the product reports the same economics the log carries: the
    // inspector is the user-visible cached-prefix hit/miss report.
    let ack = c2
        .command(envelope(
            id16(0x62),
            "GetContextInspector",
            modbit_protocol::v1::GetContextInspector {
                task_id: Some(task.clone()),
            }
            .encode_to_vec(),
        ))
        .await
        .unwrap();
    let v: modbit_protocol::v1::ContextInspectorView = Client::result(&ack).unwrap();
    assert_eq!(usize::try_from(v.compaction_epochs).unwrap(), epochs.len());
    assert_eq!(
        u64::from(v.compaction_epoch),
        last_epoch["epoch"].as_u64().unwrap()
    );
    assert_eq!(v.manifest_ref, last_epoch["manifest_ref"].as_str().unwrap());
    assert_eq!(v.compacted_entries, summarised);
    // The first turn has no prefix to reuse, so the report counts one miss
    // more than the transitions between turns.
    assert_eq!(
        usize::try_from(v.prefix_cache_misses).unwrap(),
        misses + 1,
        "{keys:?}"
    );
    assert_eq!(
        usize::try_from(v.prefix_cache_hits).unwrap(),
        keys.len() - misses - 1,
        "{keys:?}"
    );
    let _ = repo;
}
