//! PX-049 (QUAL-PX-049) on the real Core with real hook processes: the
//! `session_end` hook runs to completion inside `EndSession` and its answer
//! is journaled on the task's log, a hook that fails or times out is in the
//! answer and journaled, and the person's resume choice is a Core input that
//! survives a real kill of the Core: after the restart the recovery report
//! names the tasks to resume, and a choice is spent by the resume it asked
//! for.
//!
//! Real: the `modbit-core` binary (killed with SIGKILL, restarted on the same
//! data directory), its socket and store, the Hook Bus and every hook process
//! (`sh` scripts, so these tests are Unix-only like the other hook suites).
//! Stand-in: the model, a scripted OpenAI-compatible server whose second
//! answer never arrives, so the run is mid-flight when the Core dies.
#![cfg(unix)]

mod px_common;

use modbit_protocol::client::Client;
use modbit_protocol::v1::{
    EndSession, GetRecoveryReport, GetTaskStatus, Id, RecoveryReport, SessionEndedView, TaskStatus,
};
use prost::Message;
use px_common::*;
use serde_json::{Value, json};
use std::sync::Arc;
use std::time::{Duration, Instant};

fn script(dir: &std::path::Path, name: &str, body: &str) -> Vec<String> {
    let path = dir.join(name);
    std::fs::write(&path, body).unwrap();
    vec!["sh".to_owned(), path.to_string_lossy().replace('\\', "/")]
}

fn session_end_hooks(dir: &std::path::Path, hooks: &[Value]) {
    let layer = json!({"hooks": hooks.iter().map(|h| h.to_string()).collect::<Vec<_>>()});
    std::fs::write(dir.join("admin-config.json"), layer.to_string()).unwrap();
}

fn plan() -> Value {
    json!({"calls": [{"name": "plan.update", "args": {"outcome": "note", "expected_files": ["notes.md"]}}]})
}

/// The run's first answer is a plan; every later one never arrives in time,
/// so the run is in flight until the Core is killed.
fn stalled_model() -> Reply {
    Arc::new(|_body, results| {
        if results == 0 {
            plan()
        } else {
            json!({"text": "still thinking", "delay_ms": 120000})
        }
    })
}

struct Fx {
    core: CoreProcess,
    c: Client,
    session: Id,
    g: Option<u64>,
    root: String,
    dir: tempfile::TempDir,
    scripts: tempfile::TempDir,
    next: u8,
    env: Vec<(String, String)>,
    _repo: tempfile::TempDir,
    _seen: Seen,
}

async fn fixture() -> Fx {
    let (repo, root) = plain_repo(&[("notes.md", "notes\n")]);
    let (base, seen) = scripted_model_fn(stalled_model()).await;
    let dir = tempfile::tempdir().unwrap();
    let env = model_env(&base);
    let refs: Vec<(&str, &str)> = env.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    let core = CoreProcess::spawn_with_env(dir.path(), &refs);
    let mut c = core.client().await;
    let (session, g) = session_with_lease(&mut c, 0x30).await;
    Fx {
        core,
        c,
        session,
        g,
        root,
        dir,
        scripts: tempfile::tempdir().unwrap(),
        next: 0x40,
        env,
        _repo: repo,
        _seen: seen,
    }
}

impl Fx {
    fn id(&mut self) -> u8 {
        self.next += 1;
        self.next
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
            "keep notes",
        )
        .await
    }

    async fn running_task(&mut self) -> Id {
        let task = self.task().await;
        let id = self.id();
        start_task(&mut self.c, &task, self.g, id, "gpt-5-mini").await;
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            let st = self.status(&task).await;
            if st.state == "Running" && st.loop_alive {
                return task;
            }
            assert!(Instant::now() < deadline, "the task never ran: {st:?}");
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    async fn status(&mut self, task: &Id) -> TaskStatus {
        let ack = self
            .c
            .command(envelope(
                rand_id(),
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

    async fn end(&mut self, resume: bool) -> SessionEndedView {
        let id = self.id();
        let ack = self
            .c
            .command(envelope_fenced(
                id16(id),
                "EndSession",
                EndSession {
                    session_id: Some(self.session.clone()),
                    resume_on_reopen: resume,
                }
                .encode_to_vec(),
                self.g,
            ))
            .await
            .unwrap();
        Client::result(&ack).unwrap()
    }

    async fn events(&self) -> Vec<Value> {
        replay(&self.core, &self.session).await
    }

    /// Kill the Core (SIGKILL, no goodbye) and start another on the same data.
    async fn restart(&mut self) {
        self.core.kill();
        let refs: Vec<(&str, &str)> = self
            .env
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        self.core = CoreProcess::spawn_with_env(self.dir.path(), &refs);
        self.c = self.core.client().await;
    }

    async fn recovery(&mut self) -> RecoveryReport {
        let ack = self
            .c
            .command(envelope(
                rand_id(),
                "GetRecoveryReport",
                GetRecoveryReport {}.encode_to_vec(),
            ))
            .await
            .unwrap();
        Client::result(&ack).unwrap()
    }
}

fn hook_entries(events: &[Value], task: &Id) -> Vec<Value> {
    events
        .iter()
        .filter(|e| {
            e["event_type"] == "HookInvoked" && e["task_id"].as_str() == Some(&hex_id(task))
        })
        .map(|e| e["payload"]["payload"].clone())
        .collect()
}

/// A session-end hook that takes two seconds completes before `EndSession`
/// answers, and what it said is journaled on the task's log with the
/// session's own record of the choice beside it.
#[tokio::test(flavor = "multi_thread")]
async fn qual_px_049_a_two_second_session_end_hook_completes_before_the_answer_and_is_journaled() {
    let mut fx = fixture().await;
    let sentinel = fx.scripts.path().join("ran.txt");
    let sentinel_path = sentinel.to_string_lossy().replace('\\', "/");
    let cmd = script(
        fx.scripts.path(),
        "archive.sh",
        &format!(
            "cat > '{sentinel_path}.in'\nsleep 2\necho done > '{sentinel_path}'\nprintf '%s' '{{\"decision\":\"continue\",\"reason\":\"archived 3 files\"}}'\n"
        ),
    );
    session_end_hooks(
        fx.dir.path(),
        &[json!({"name": "archive", "point": "session_end", "command": cmd, "timeout_ms": 10000})],
    );
    let task = fx.task().await;

    let t0 = Instant::now();
    let view = fx.end(false).await;
    let took = t0.elapsed();
    assert!(
        took >= Duration::from_millis(1900),
        "the answer waited for the hook: {took:?}"
    );
    assert!(
        sentinel.exists(),
        "the hook ran to completion before the answer"
    );
    assert_eq!(view.hooks.len(), 1, "{view:?}");
    assert_eq!(view.hooks[0].outcome, "OK");
    assert!(view.hooks[0].duration_ms >= 1900, "{view:?}");
    assert_eq!(view.hooks[0].detail, "archived 3 files");

    // The hook was sent the typed, versioned request for the point.
    let sent: Value =
        serde_json::from_str(&std::fs::read_to_string(format!("{sentinel_path}.in")).unwrap())
            .unwrap();
    assert_eq!(sent["point"], "session_end");
    assert_eq!(sent["payload"]["reason"], "quit");
    assert_eq!(sent["payload"]["resume_on_reopen"], false);

    // Journaled: the HookInvoked entry on the task, and the session's record.
    let events = fx.events().await;
    let entries = hook_entries(&events, &task);
    let entry = entries
        .iter()
        .find(|p| p["point"] == "session_end")
        .unwrap_or_else(|| panic!("no session_end entry: {entries:?}"));
    assert_eq!(entry["outcome"], "OK");
    assert_eq!(entry["detail"], "archived 3 files");
    let ending = events
        .iter()
        .find(|e| e["event_type"] == "SessionEnding")
        .expect("the session records that it is ending");
    assert_eq!(ending["payload"]["payload"]["hooks_run"], 1);
    assert_eq!(ending["payload"]["payload"]["hooks_failed"], 0);
}

/// A hook that fails, one that times out and one that answers nonsense are
/// each in the answer (so the host shows them) and on the task's log; the
/// good hook beside them still ran.
#[tokio::test(flavor = "multi_thread")]
async fn qual_px_049_a_failing_session_end_hook_is_reported_and_journaled_and_the_others_still_run()
{
    let mut fx = fixture().await;
    let ok_marker = fx.scripts.path().join("ok.txt");
    let ok_path = ok_marker.to_string_lossy().replace('\\', "/");
    let ok = script(
        fx.scripts.path(),
        "ok.sh",
        &format!(
            "cat >/dev/null\necho ok > '{ok_path}'\nprintf '%s' '{{\"decision\":\"continue\"}}'\n"
        ),
    );
    let boom = script(
        fx.scripts.path(),
        "boom.sh",
        "cat >/dev/null\necho 'disk full' >&2\nexit 1\n",
    );
    let slow = script(fx.scripts.path(), "slow.sh", "cat >/dev/null\nsleep 30\n");
    let nonsense = script(
        fx.scripts.path(),
        "bad.sh",
        "cat >/dev/null\nprintf 'not json'\n",
    );
    session_end_hooks(
        fx.dir.path(),
        &[
            json!({"name": "boom", "point": "session_end", "command": boom, "fail_policy": "open"}),
            json!({"name": "slow", "point": "session_end", "command": slow, "timeout_ms": 500, "fail_policy": "open"}),
            json!({"name": "nonsense", "point": "session_end", "command": nonsense, "fail_policy": "open"}),
            json!({"name": "ok", "point": "session_end", "command": ok}),
        ],
    );
    let task = fx.task().await;

    let view = fx.end(false).await;
    let outcome = |n: &str| {
        view.hooks
            .iter()
            .find(|h| h.hook.ends_with(n))
            .unwrap_or_else(|| panic!("no run of {n}: {view:?}"))
            .outcome
            .clone()
    };
    assert_eq!(outcome("boom"), "FAILED");
    assert_eq!(outcome("slow"), "TIMEOUT");
    assert_eq!(outcome("nonsense"), "MALFORMED");
    assert_eq!(outcome("ok"), "OK");
    assert!(ok_marker.exists(), "a failed hook does not stop the others");

    let events = fx.events().await;
    let entries = hook_entries(&events, &task);
    for (name, outcome) in [
        ("boom", "FAILED"),
        ("slow", "TIMEOUT"),
        ("nonsense", "MALFORMED"),
    ] {
        assert!(
            entries
                .iter()
                .any(|p| p["hook"].as_str().is_some_and(|h| h.ends_with(name))
                    && p["outcome"] == outcome),
            "{name} is journaled as {outcome}: {entries:?}"
        );
    }
    let ending = events
        .iter()
        .find(|e| e["event_type"] == "SessionEnding")
        .unwrap();
    assert_eq!(ending["payload"]["payload"]["hooks_failed"], 3);
}

/// With no hook declared, ending the session is immediate and still records
/// the person's choice.
#[tokio::test(flavor = "multi_thread")]
async fn qual_px_049_ending_a_session_with_no_hooks_is_immediate() {
    let mut fx = fixture().await;
    let _ = fx.task().await;
    let t0 = Instant::now();
    let view = fx.end(false).await;
    assert!(t0.elapsed() < Duration::from_secs(5));
    assert!(view.hooks.is_empty());
    assert!(view.resume_tasks.is_empty());
}

/// The resume choice is a Core input and survives killing the Core while the
/// run is in flight: the next start's recovery report names the task, a task
/// the person chose not to resume is not named, and a choice is spent by the
/// resume it asked for.
#[tokio::test(flavor = "multi_thread")]
async fn qual_px_049_the_resume_choice_survives_killing_the_core_and_is_spent_by_the_resume() {
    let mut fx = fixture().await;
    let task = fx.running_task().await;

    // The person quits and chooses to resume; the Core is then killed before
    // it can say anything else (a shutdown that did not finish).
    let view = fx.end(true).await;
    assert_eq!(view.resume_tasks, vec![task.clone()]);
    fx.restart().await;

    let report = fx.recovery().await;
    assert_eq!(
        report.resume_requested_tasks,
        vec![task.clone()],
        "{report:?}"
    );
    let st = fx.status(&task).await;
    assert_eq!(
        st.state, "Waiting",
        "recovery suspended it and nothing runs until a client resumes it: {st:?}"
    );
    assert!(!st.loop_alive);

    // The client resumes it with the ordinary StartTask; that spends the choice.
    let id = fx.id();
    start_task(&mut fx.c, &task, fx.g, id, "gpt-5-mini").await;
    let events = fx.events().await;
    assert!(
        events
            .iter()
            .any(|e| e["event_type"] == "TaskResumed" && e["task_id"] == hex_id(&task).as_str()),
        "the run continued from the recovered task"
    );
    fx.restart().await;
    let again = fx.recovery().await;
    assert!(
        again.resume_requested_tasks.is_empty(),
        "an old choice never applies twice: {again:?}"
    );
}

/// Quitting without choosing resume leaves the task recoverable and waiting.
#[tokio::test(flavor = "multi_thread")]
async fn qual_px_049_quitting_without_resume_leaves_the_task_recoverable_and_not_resumed() {
    let mut fx = fixture().await;
    let task = fx.running_task().await;
    let view = fx.end(false).await;
    assert!(view.resume_tasks.is_empty());
    fx.restart().await;
    let report = fx.recovery().await;
    assert!(report.resume_requested_tasks.is_empty(), "{report:?}");
    assert_eq!(report.tasks, 1);
    let st = fx.status(&task).await;
    assert_ne!(st.state, "Running", "{st:?}");
    assert!(!st.loop_alive);
}
