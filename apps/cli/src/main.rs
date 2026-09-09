//! `modbit-cli` — headless thin SurfaceProtocol client (docs/29). It owns no
//! orchestration, context, memory, Git, recovery, policy or tool code and holds
//! no provider credentials; it spawns or attaches to a `modbit-core` and speaks
//! the authenticated local SurfaceProtocol.
//!
//! Usage:
//!   modbit-cli --data-dir <dir> session create
//!   modbit-cli --data-dir <dir> task create --session <hex-id> "<goal>"
//!   modbit-cli --data-dir <dir> session show --session <hex-id>
//!   modbit-cli --data-dir <dir> events tail --session <hex-id> [--after <offset>] [--count <n>]
//!
//! M1.3 mode: each invocation spawns a Core for the data directory, runs one
//! command, and exits; the Core exits with the CLI. Attaching to a long-running
//! Core belongs to PX-000 (M2).

use std::io::{BufRead, BufReader};
use std::process::{Command, ExitCode, Stdio};

use modbit_protocol::client::Client;
use modbit_protocol::local::{ReadyLine, decode_hex, encode_hex};
use modbit_protocol::v1::{
    ClientKind, CommandEnvelope, CreateSession, CreateTask, GetSessionSnapshot, Id, SessionCreated,
    SessionSnapshot, TaskCreated,
};
use prost::Message;

const USAGE: &str = "usage: modbit-cli --data-dir <dir> (session create | session show --session <id> | task create --session <id> <goal> | events tail --session <id> [--after N] [--count N])";

fn parse_id(hex: &str) -> Result<Id, String> {
    let bytes = decode_hex(hex)
        .filter(|b| b.len() == 16)
        .ok_or_else(|| format!("`{hex}` is not a 32-hex-char id"))?;
    Ok(Id { value: bytes })
}

fn fresh_id() -> Id {
    Id {
        value: (0..16).map(|_| rand::random::<u8>()).collect(),
    }
}

fn envelope(command_type: &str, payload: Vec<u8>) -> CommandEnvelope {
    CommandEnvelope {
        command_id: Some(fresh_id()),
        tenant_id: None,
        user_id: None,
        session_id: None,
        aggregate_id: None,
        expected_generation: None,
        command_type: command_type.into(),
        schema_version: 1,
        payload,
        issued_at: None,
    }
}

fn spawn_core(data_dir: &str) -> Result<(std::process::Child, ReadyLine), String> {
    let exe = std::env::var("MODBIT_CORE_BIN").unwrap_or_else(|_| "modbit-core".into());
    let mut child = Command::new(&exe)
        .arg("--data-dir")
        .arg(data_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| format!("spawning `{exe}`: {e} (set MODBIT_CORE_BIN)"))?;
    let stdout = child.stdout.take().expect("piped stdout");
    let mut lines = BufReader::new(stdout).lines();
    let ready = loop {
        match lines.next() {
            Some(Ok(line)) => {
                if let Some(r) = ReadyLine::parse(&line) {
                    break r;
                }
            }
            _ => return Err("modbit-core exited before it was ready".into()),
        }
    };
    std::thread::spawn(move || for _ in lines {});
    Ok((child, ready))
}

async fn run(args: Vec<String>) -> Result<(), String> {
    let mut data_dir = None;
    let mut rest = Vec::new();
    let mut it = args.into_iter();
    while let Some(a) = it.next() {
        if a == "--data-dir" {
            data_dir = it.next();
        } else {
            rest.push(a);
        }
    }
    let data_dir = data_dir.ok_or(USAGE)?;
    let (mut child, ready) = spawn_core(&data_dir)?;
    let result = run_command(&ready, rest).await;
    let _ = child.kill();
    let _ = child.wait();
    result
}

async fn run_command(ready: &ReadyLine, rest: Vec<String>) -> Result<(), String> {
    let secret = decode_hex(&ready.boot_secret_hex).ok_or("bad secret in ready line")?;
    let mut client = Client::connect(
        &ready.endpoint,
        &secret,
        ClientKind::Cli,
        env!("CARGO_PKG_VERSION"),
    )
    .await
    .map_err(|e| e.to_string())?;
    let words: Vec<&str> = rest.iter().map(String::as_str).collect();
    let opt = |flag: &str| {
        words
            .iter()
            .position(|w| *w == flag)
            .and_then(|i| words.get(i + 1).copied())
    };
    match words.as_slice() {
        ["session", "create", ..] => {
            let ack = client
                .command(envelope(
                    "CreateSession",
                    CreateSession { space_id: None }.encode_to_vec(),
                ))
                .await
                .map_err(|e| e.to_string())?;
            let r: SessionCreated = Client::result(&ack).map_err(|e| e.to_string())?;
            println!(
                "session {}",
                encode_hex(&r.session_id.unwrap_or_default().value)
            );
        }
        ["session", "show", ..] => {
            let sid = parse_id(opt("--session").ok_or(USAGE)?)?;
            let ack = client
                .command(envelope(
                    "GetSessionSnapshot",
                    GetSessionSnapshot {
                        session_id: Some(sid),
                    }
                    .encode_to_vec(),
                ))
                .await
                .map_err(|e| e.to_string())?;
            let s: SessionSnapshot = Client::result(&ack).map_err(|e| e.to_string())?;
            println!(
                "session state={} generation={} last_offset={}",
                s.state, s.generation, s.last_offset
            );
            for t in s.tasks {
                println!(
                    "task {} state={} generation={} goal={:?}",
                    encode_hex(&t.task_id.unwrap_or_default().value),
                    t.state,
                    t.generation,
                    t.goal_text
                );
            }
        }
        ["task", "create", ..] => {
            let sid = parse_id(opt("--session").ok_or(USAGE)?)?;
            let goal = words
                .iter()
                .skip(2)
                .filter(|w| !w.starts_with("--"))
                .skip_while(|w| parse_id(w).is_ok())
                .cloned()
                .collect::<Vec<_>>()
                .join(" ");
            if goal.is_empty() {
                return Err(USAGE.into());
            }
            let ack = client
                .command(envelope(
                    "CreateTask",
                    CreateTask {
                        session_id: Some(sid),
                        goal_text: goal,
                        workspace_id: None,
                        execution_profile: String::new(),
                        origin: "cli".into(),
                    }
                    .encode_to_vec(),
                ))
                .await
                .map_err(|e| e.to_string())?;
            let r: TaskCreated = Client::result(&ack).map_err(|e| e.to_string())?;
            println!("task {}", encode_hex(&r.task_id.unwrap_or_default().value));
        }
        ["events", "tail", ..] => {
            let sid = parse_id(opt("--session").ok_or(USAGE)?)?;
            let after: u64 = opt("--after")
                .map(|v| v.parse().map_err(|_| USAGE))
                .transpose()?
                .unwrap_or(0);
            let count: usize = opt("--count")
                .map(|v| v.parse().map_err(|_| USAGE))
                .transpose()?
                .unwrap_or(100);
            client
                .subscribe(sid, after)
                .await
                .map_err(|e| e.to_string())?;
            for _ in 0..count {
                match tokio::time::timeout(
                    std::time::Duration::from_millis(500),
                    client.next_event(),
                )
                .await
                {
                    Ok(Ok(Some(e))) => {
                        let ev = e.event.unwrap_or_default();
                        println!("{} seq={} {}", e.offset, ev.sequence, ev.event_type);
                    }
                    Ok(Ok(None)) => break,
                    Ok(Err(e)) => return Err(e.to_string()),
                    Err(_) => break, // idle: nothing more right now
                }
            }
        }
        _ => return Err(USAGE.into()),
    }
    Ok(())
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "-h" || a == "--help") {
        println!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("runtime");
    match rt.block_on(run(args)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("modbit-cli: {e}");
            ExitCode::from(1)
        }
    }
}
