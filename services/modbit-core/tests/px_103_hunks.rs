//! QUAL-PX-103 against the real Core process over its real socket, a real
//! SQLite event store, a real Git repository and real child processes. The
//! model is the repository's scripted OpenAI-compatible server (the live
//! gateway leg is not run here): what is under test is the Core's attribution
//! of hunks to the tool call that wrote them (`change.apply`, a shell
//! redirect, a test runner that rewrites a file), a person's per-hunk
//! decision while the run is live, the typed notice the agent receives, the
//! stale refusals, an unattributable write, and a SIGKILL after the revert.

mod px_common;

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use modbit_protocol::client::{Client, ClientError};
use modbit_protocol::v1::{
    AcquireSessionLease, DecideHunk, DecideHunkAck, GetReviewBundle, GetTaskStatus, Id,
    ReviewBundle, SessionLeaseAcquired, TaskStatus,
};
use prost::Message;
use px_common::*;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

type Reject = (String, String);

fn rows() -> String {
    (1..=45).map(|i| format!("row-{i:02}\n")).collect()
}

fn sha(s: &str) -> String {
    hex::encode(Sha256::digest(s.as_bytes()))
}

const RUNNER: &str =
    "sed 's/^row-24$/row-24 (runner)/' notes.txt > notes.tmp && mv notes.tmp notes.txt\n";

/// The small model-facing surface (`proc.exec` and a few more); the tools a
/// program calls are the governed tools, each its own call on the log.
const MODELS: &str = "px-model=0.15/0.60;projection=exec_only";
const MODEL: &str = "px-model";

const NOTICE: &str = "Core notice: a person rejected one hunk";

fn program(js: &str) -> Value {
    json!({"calls": [{"name": "proc.exec", "args": {"program": js}}]})
}

/// A shell command run the way the model runs one: through `proc.exec`, whose
/// program calls `shell.exec` (the change barrier of FIX-06 guards it).
fn shell(script: &str) -> Value {
    let program = format!(
        "const sh = await tools.shell.exec({{ argv: [\"sh\", \"-c\", {}], inherit_env: true, timeout_ms: 30000 }}); return {{ exit: sh.exit_code }};",
        serde_json::to_string(script).unwrap()
    );
    json!({"calls": [{"name": "proc.exec", "args": {"program": program}}]})
}

fn call(name: &str, args: Value) -> Value {
    json!({"calls": [{"name": name, "args": args}]})
}

struct Fx {
    core: CoreProcess,
    c: Client,
    session: Id,
    g: Option<u64>,
    task: Id,
    root: String,
    seen: Seen,
    gate: std::path::PathBuf,
    base: String,
    dir: tempfile::TempDir,
    _repo: tempfile::TempDir,
    _gate_dir: tempfile::TempDir,
    n: u8,
}

/// Steps 0..hold run, the request that carries `hold` results is held at the
/// gate, then the agent reads a file twice and, once it has been told of a
/// rejected hunk, completes saying so.
async fn fixture(steps: Vec<Value>) -> Fx {
    let hold = steps.len();
    let (repo, root) = plain_repo(&[
        ("notes.txt", &rows()),
        ("run_tests.sh", RUNNER),
        ("other.txt", "other\n"),
    ]);
    let gate_dir = tempfile::tempdir().unwrap();
    let gate = gate_dir.path().join("go");
    let (base, seen) = {
        let gate = gate.clone();
        let steps = steps.clone();
        scripted_model_fn(Arc::new(move |body, results| {
            let told = body["messages"].to_string().contains(NOTICE);
            if told {
                return call(
                    "task.complete",
                    json!({"summary": "acknowledged: the rejected hunk stays reverted", "self_review": {"findings": []}}),
                );
            }
            if let Some(s) = steps.get(results) {
                return s.clone();
            }
            if results == hold {
                let mut r = call("fs.read", json!({"path": "other.txt"}));
                r["wait_file"] = json!(gate.to_string_lossy());
                r["wait_file_secs"] = json!(170);
                return r;
            }
            call("fs.read", json!({"path": "other.txt"}))
        }))
        .await
    };
    let dir = tempfile::tempdir().unwrap();
    let mut env = model_env(&base);
    env.push(("MODBIT_OPENAI_MODELS".into(), MODELS.into()));
    let refs: Vec<(&str, &str)> = env.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    let core = CoreProcess::spawn_with_env(dir.path(), &refs);
    let mut c = core.client().await;
    let (session, g) = session_with_lease(&mut c, 0x61).await;
    let task = create_task(
        &mut c,
        &session,
        g,
        &root,
        0x62,
        "local_trusted",
        "edit notes",
    )
    .await;
    start_task(&mut c, &task, g, 0x63, MODEL).await;
    Fx {
        core,
        c,
        session,
        g,
        task,
        root,
        seen,
        gate,
        base,
        dir,
        _repo: repo,
        _gate_dir: gate_dir,
        n: 0x70,
    }
}

impl Fx {
    fn id(&mut self) -> Id {
        self.n += 1;
        id16(self.n)
    }

    fn path(&self, p: &str) -> std::path::PathBuf {
        Path::new(&self.root).join(p)
    }

    fn text(&self, p: &str) -> String {
        std::fs::read_to_string(self.path(p)).unwrap()
    }

    /// The model has asked for the held request: every earlier step ran.
    async fn held(&self, requests: usize) {
        let deadline = std::time::Instant::now() + Duration::from_secs(120);
        while self.seen.lock().unwrap().len() < requests {
            if std::time::Instant::now() >= deadline {
                let last = self
                    .seen
                    .lock()
                    .unwrap()
                    .last()
                    .cloned()
                    .unwrap_or_default();
                let tools: Vec<String> = last["messages"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default()
                    .iter()
                    .filter(|m| m["role"] == "tool")
                    .map(|m| m["content"].to_string().chars().take(600).collect())
                    .collect();
                panic!(
                    "the run never reached the held request; {} requests; tool results: {tools:#?}",
                    self.seen.lock().unwrap().len()
                );
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        tokio::time::sleep(Duration::from_millis(400)).await;
    }

    async fn status(&mut self) -> TaskStatus {
        let ack = self
            .c
            .command(envelope(
                rand_id(),
                "GetTaskStatus",
                GetTaskStatus {
                    task_id: Some(self.task.clone()),
                }
                .encode_to_vec(),
            ))
            .await
            .unwrap();
        Client::result(&ack).unwrap()
    }

    async fn bundle(&mut self) -> ReviewBundle {
        let ack = self
            .c
            .command(envelope(
                rand_id(),
                "GetReviewBundle",
                GetReviewBundle {
                    task_id: Some(self.task.clone()),
                }
                .encode_to_vec(),
            ))
            .await
            .unwrap();
        Client::result(&ack).unwrap()
    }

    #[allow(clippy::too_many_arguments)]
    async fn decide_with(
        &mut self,
        id: Id,
        path: &str,
        hunk: u32,
        decision: &str,
        revision: &str,
        digest: &str,
        reason: &str,
    ) -> Result<(DecideHunkAck, bool), Reject> {
        let g = self.g;
        match self
            .c
            .command(envelope_fenced(
                id,
                "DecideHunk",
                DecideHunk {
                    task_id: Some(self.task.clone()),
                    path: path.into(),
                    hunk_index: hunk,
                    decision: decision.into(),
                    expected_file_revision: revision.into(),
                    expected_hunk_digest: digest.into(),
                    reason: reason.into(),
                }
                .encode_to_vec(),
                g,
            ))
            .await
        {
            Ok(ack) => {
                let r: DecideHunkAck = Client::result(&ack).unwrap();
                let replayed = r.replayed;
                Ok((r, replayed))
            }
            Err(ClientError::Rejected { code, message }) => Err((code, message)),
            Err(e) => panic!("DecideHunk: {e}"),
        }
    }

    async fn decide(
        &mut self,
        path: &str,
        hunk: u32,
        decision: &str,
        revision: &str,
        digest: &str,
        reason: &str,
    ) -> Result<DecideHunkAck, Reject> {
        let id = self.id();
        self.decide_with(id, path, hunk, decision, revision, digest, reason)
            .await
            .map(|(a, _)| a)
    }

    async fn events(&self) -> Vec<Value> {
        replay(&self.core, &self.session).await
    }

    fn release(&self) {
        std::fs::write(&self.gate, b"go").unwrap();
    }

    async fn restart(&mut self) {
        self.core.kill();
        let mut env = model_env(&self.base);
        env.push(("MODBIT_OPENAI_MODELS".into(), MODELS.into()));
        let refs: Vec<(&str, &str)> = env.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
        self.core = CoreProcess::spawn_with_env(self.dir.path(), &refs);
        self.c = self.core.client().await;
        let n = self.id();
        let ack = self
            .c
            .command(envelope(
                n,
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

fn normalize(id: &str) -> String {
    id.replace('-', "").to_lowercase()
}

/// The ids of the tool calls the model made, by tool name, in order.
fn calls_of(events: &[Value], tool: &str) -> Vec<String> {
    events
        .iter()
        .filter(|e| {
            e["event_type"] == "ToolCallProposed" && e["payload"]["payload"]["tool_name"] == tool
        })
        .map(|e| e["aggregate_id"].as_str().unwrap().to_owned())
        .collect()
}

fn of_type<'a>(events: &'a [Value], t: &str) -> Vec<&'a Value> {
    events.iter().filter(|e| e["event_type"] == t).collect()
}

fn hunk_with<'a>(
    b: &'a ReviewBundle,
    needle: &str,
) -> (&'a modbit_protocol::v1::HunkView, &'a str, &'a str) {
    for f in &b.files {
        for h in &f.hunks {
            if h.lines.iter().any(|l| l.contains(needle)) {
                return (h, f.path.as_str(), f.file_revision.as_str());
            }
        }
    }
    panic!("no hunk holds {needle}: {b:#?}");
}

fn script_three_writers() -> Vec<Value> {
    vec![
        call(
            "plan.update",
            json!({"outcome": "edit the notes", "expected_files": ["notes.txt"]}),
        ),
        program(
            r#"await tools.fs.read({ path: "notes.txt" });
               await tools.change.apply({ path: "notes.txt", op: "edit", text_edits: [{ old: "row-02\n", new: "row-02 (change.apply)\n" }] });
               return "ok";"#,
        ),
        shell("sed 's/^row-12$/row-12 (shell)/' notes.txt > notes.tmp && mv notes.tmp notes.txt"),
        shell("echo 'row-46 (redirect)' >> notes.txt"),
        shell("sh run_tests.sh"),
    ]
}

/// QUAL-PX-103 main leg.
#[tokio::test]
async fn qual_px_103_hunks_name_their_call_and_a_mid_run_rejection_reverts_exactly_that_hunk() {
    let steps = script_three_writers();
    let hold = steps.len();
    let mut fx = fixture(steps).await;
    fx.held(hold + 1).await;
    assert!(
        fx.status().await.loop_alive,
        "the run is live while the person decides"
    );

    // Each hunk names the call that wrote it: change.apply (the file service's
    // transaction), a shell edit, a shell redirect and a test runner.
    let events = fx.events().await;
    let apply = calls_of(&events, "change.apply");
    let shells = calls_of(&events, "shell.exec");
    assert_eq!((apply.len(), shells.len()), (1, 3), "the calls on the log");
    let b = fx.bundle().await;
    let (h02, path, rev) = hunk_with(&b, "+row-02 (change.apply)");
    assert_eq!(path, "notes.txt");
    assert_eq!(normalize(&h02.tool_call_id), apply[0], "{h02:?}");
    assert_eq!(h02.tool_name, "change.apply");
    assert_eq!(h02.attribution_basis, "CHANGE_TRANSACTION");
    assert!(
        h02.attributed
            && h02.auto_accept_eligible
            && !h02.turn_id.is_empty()
            && !h02.run_id.is_empty()
    );
    let (h12, _, _) = hunk_with(&b, "+row-12 (shell)");
    assert_eq!(normalize(&h12.tool_call_id), shells[0], "{h12:?}");
    assert_eq!(h12.tool_name, "shell.exec");
    assert_eq!(h12.attribution_basis, "BARRIER_DIFF");
    assert!(h12.attributed);
    let (h46, _, _) = hunk_with(&b, "+row-46 (redirect)");
    assert_eq!(normalize(&h46.tool_call_id), shells[1], "{h46:?}");
    assert!(h46.attributed);
    let (h24, _, _) = hunk_with(&b, "+row-24 (runner)");
    assert_eq!(normalize(&h24.tool_call_id), shells[2], "{h24:?}");
    assert!(h24.attributed);
    assert_eq!(b.files[0].hunks.len(), 4);
    let (rev, d12, i12) = (rev.to_owned(), h12.hunk_digest.clone(), h12.index);
    let before_text = fx.text("notes.txt");
    assert_eq!(sha(&before_text), rev);

    // A decision made on a file that moved is refused stale, and a hunk that
    // is no longer the one shown is refused STALE_HUNK; nothing is written.
    let wrong = sha("not the file");
    let r = fx
        .decide("notes.txt", i12, "REJECT", &wrong, &d12, "x")
        .await
        .unwrap_err();
    assert_eq!(r.0, "STALE_REVISION", "{r:?}");
    let r = fx
        .decide("notes.txt", i12, "REJECT", &rev, &sha("other hunk"), "x")
        .await
        .unwrap_err();
    assert_eq!(r.0, "STALE_HUNK", "{r:?}");
    assert_eq!(fx.text("notes.txt"), before_text);

    // The person rejects the shell's hunk while the run is live.
    let reason = "keep row-12 as it was; the shell edit was not asked for";
    let ack = fx
        .decide("notes.txt", i12, "REJECT", &rev, &d12, reason)
        .await
        .unwrap();
    assert_eq!(ack.result, "REJECTED");
    assert!(ack.run_live, "{ack:?}");
    assert_eq!(normalize(&ack.attributed_call_id), shells[0]);
    assert_eq!(ack.attributed_tool, "shell.exec");
    assert!(ack.agent_notice.contains(reason) && ack.agent_notice.contains("notes.txt"));
    let expected: String = before_text.replace("row-12 (shell)\n", "row-12\n");
    assert_eq!(
        fx.text("notes.txt"),
        expected,
        "exactly that hunk is reverted"
    );
    assert_eq!(ack.file_revision, sha(&expected));
    assert_eq!(ack.before_hash, rev);
    assert!(!ack.checkpoint_id.is_empty());
    assert!(fx.status().await.loop_alive, "the run was not restarted");

    // The other three hunks still name their calls; the old revision is stale.
    let b2 = fx.bundle().await;
    assert_eq!(b2.files[0].hunks.len(), 3);
    for (needle, id) in [
        ("+row-02 (change.apply)", &apply[0]),
        ("+row-46 (redirect)", &shells[1]),
        ("+row-24 (runner)", &shells[2]),
    ] {
        let (h, _, _) = hunk_with(&b2, needle);
        assert_eq!(normalize(&h.tool_call_id), *id, "{needle}");
        assert!(h.attributed, "{needle}");
    }
    let stale = fx
        .decide("notes.txt", 0, "REJECT", &rev, "", "again")
        .await
        .unwrap_err();
    assert_eq!(stale.0, "STALE_REVISION", "{stale:?}");

    // Accept records a person's accept of that exact hunk; an attributed hunk
    // can be accepted automatically.
    let (h02, _, rev2) = hunk_with(&b2, "+row-02 (change.apply)");
    let (idx02, d02) = (h02.index, h02.hunk_digest.clone());
    let acc = fx
        .decide("notes.txt", idx02, "ACCEPT", rev2, &d02, "")
        .await
        .unwrap();
    assert_eq!(acc.result, "ACCEPTED");
    let (h24, _, _) = hunk_with(&b2, "+row-24 (runner)");
    let auto = fx
        .decide(
            "notes.txt",
            h24.index,
            "AUTO_ACCEPT",
            rev2,
            &h24.hunk_digest,
            "",
        )
        .await
        .unwrap();
    assert_eq!(auto.result, "ACCEPTED");
    assert_eq!(fx.text("notes.txt"), expected, "accept writes nothing");
    let b3 = fx.bundle().await;
    assert_eq!(
        hunk_with(&b3, "+row-02 (change.apply)").0.decision,
        "ACCEPTED"
    );
    assert_eq!(hunk_with(&b3, "+row-24 (runner)").0.decision, "ACCEPTED");
    assert_eq!(hunk_with(&b3, "+row-46 (redirect)").0.decision, "");

    // The run continues: it is told once, as a typed Core notice carrying the
    // reason, at its next round boundary, and answers it.
    fx.release();
    let end = wait_task(&mut fx.c, &fx.task, 120).await;
    assert!(!end.loop_alive, "{end:?}");
    let bodies = fx.seen.lock().unwrap().clone();
    let told: Vec<usize> = bodies
        .iter()
        .enumerate()
        .filter(|(_, b)| b["messages"].to_string().contains(NOTICE))
        .map(|(i, _)| i)
        .collect();
    assert!(!told.is_empty(), "the agent never received the notice");
    assert!(
        told[0] > hold,
        "told at a boundary after the decision: {told:?}"
    );
    let first = bodies[told[0]]["messages"].to_string();
    assert!(first.contains(reason), "the reason reaches the agent");
    assert_eq!(first.matches(NOTICE).count(), 1, "told once");
    let events = fx.events().await;
    assert_eq!(of_type(&events, "HunkNoticeDelivered").len(), 1);
    let decided = of_type(&events, "HunkDecided");
    assert_eq!(decided.len(), 3, "one reject and two accepts: {decided:?}");
    assert_eq!(of_type(&events, "ChangesReverted").len(), 1);
    assert_eq!(
        end.state, "ReadyForReview",
        "the agent answered the notice and went on: {end:?}"
    );
    assert_eq!(fx.text("notes.txt"), expected);
}

/// A write by a process outside any tool call is shown as unattributed and is
/// never accepted automatically; a person can still accept it explicitly.
#[tokio::test]
async fn qual_px_103_an_unattributable_hunk_is_shown_as_such_and_blocks_auto_accept() {
    let steps = vec![
        call(
            "plan.update",
            json!({"outcome": "edit the notes", "expected_files": ["notes.txt"]}),
        ),
        program(
            r#"await tools.fs.read({ path: "notes.txt" });
               await tools.change.apply({ path: "notes.txt", op: "edit", text_edits: [{ old: "row-02\n", new: "row-02 (change.apply)\n" }] });
               return "ok";"#,
        ),
    ];
    let hold = steps.len();
    let mut fx = fixture(steps).await;
    fx.held(hold + 1).await;
    // Something that is not a tool call of the task writes the file.
    let now = fx.text("notes.txt");
    std::fs::write(
        fx.path("notes.txt"),
        now.replace("row-30\n", "row-30 (stranger)\n"),
    )
    .unwrap();
    let b = fx.bundle().await;
    let (h, _, rev) = hunk_with(&b, "+row-30 (stranger)");
    assert!(!h.attributed && h.tool_call_id.is_empty(), "{h:?}");
    assert_eq!(h.attribution_basis, "UNATTRIBUTED");
    assert!(!h.auto_accept_eligible);
    let (idx, digest, rev) = (h.index, h.hunk_digest.clone(), rev.to_owned());
    let r = fx
        .decide("notes.txt", idx, "AUTO_ACCEPT", &rev, &digest, "")
        .await
        .unwrap_err();
    assert_eq!(r.0, "UNATTRIBUTED_HUNK", "{r:?}");
    // The attributed hunk beside it is unaffected.
    let (h02, _, _) = hunk_with(&b, "+row-02 (change.apply)");
    assert!(h02.attributed && h02.auto_accept_eligible);
    let ok = fx
        .decide(
            "notes.txt",
            h02.index,
            "AUTO_ACCEPT",
            &rev,
            &h02.hunk_digest,
            "",
        )
        .await
        .unwrap();
    assert_eq!(ok.result, "ACCEPTED");
    // A person who looked at it may accept it themselves.
    let manual = fx
        .decide("notes.txt", idx, "ACCEPT", &rev, &digest, "")
        .await
        .unwrap();
    assert_eq!(manual.result, "ACCEPTED");
    assert!(manual.attributed_call_id.is_empty());
    // Rejecting it names no call to the agent.
    let rej = fx
        .decide("notes.txt", idx, "REJECT", &rev, &digest, "not mine")
        .await
        .unwrap();
    assert!(rej.agent_notice.contains("could not attribute"), "{rej:?}");
    assert!(!fx.text("notes.txt").contains("(stranger)"));
    fx.release();
    wait_task(&mut fx.c, &fx.task, 120).await;
}

/// `SIGKILL` right after the revert: the revert and the decision are one
/// transaction, so the restarted Core holds both exactly once; the same
/// command id replays; the resumed run is told once.
#[tokio::test]
async fn qual_px_103_sigkill_after_the_revert_recovers_to_one_consistent_outcome() {
    let steps = script_three_writers();
    let hold = steps.len();
    let mut fx = fixture(steps).await;
    fx.held(hold + 1).await;
    let b = fx.bundle().await;
    let (h, _, rev) = hunk_with(&b, "+row-12 (shell)");
    let (idx, digest, rev) = (h.index, h.hunk_digest.clone(), rev.to_owned());
    let before = fx.text("notes.txt");
    let cmd = fx.id();
    let (ack, replayed) = fx
        .decide_with(
            cmd.clone(),
            "notes.txt",
            idx,
            "REJECT",
            &rev,
            &digest,
            "no shell edits",
        )
        .await
        .unwrap();
    assert!(!replayed && ack.result == "REJECTED");
    let expected = before.replace("row-12 (shell)\n", "row-12\n");
    assert_eq!(fx.text("notes.txt"), expected);

    // Hard kill, before the agent was told.
    fx.restart().await;
    assert_eq!(fx.text("notes.txt"), expected, "reverted once, not twice");
    let events = fx.events().await;
    assert_eq!(of_type(&events, "HunkDecided").len(), 1);
    assert_eq!(of_type(&events, "ChangesReverted").len(), 1);
    assert!(of_type(&events, "HunkNoticeDelivered").is_empty());
    // The same command, retried by the client, replays: nothing is written.
    let (again, replayed) = fx
        .decide_with(
            cmd,
            "notes.txt",
            idx,
            "REJECT",
            &rev,
            &digest,
            "no shell edits",
        )
        .await
        .unwrap();
    assert!(replayed && again.replayed, "{again:?}");
    assert_eq!(again.decision_id, ack.decision_id);
    assert_eq!(fx.text("notes.txt"), expected);
    assert_eq!(of_type(&fx.events().await, "ChangesReverted").len(), 1);

    // The run resumes; it is told exactly once.
    fx.release();
    let (task, g, id) = (fx.task.clone(), fx.g, 0x90);
    start_task(&mut fx.c, &task, g, id, MODEL).await;
    let end = wait_task(&mut fx.c, &fx.task, 120).await;
    assert!(!end.loop_alive, "{end:?}");
    let events = fx.events().await;
    assert_eq!(
        of_type(&events, "HunkNoticeDelivered").len(),
        1,
        "told once"
    );
    assert_eq!(of_type(&events, "ChangesReverted").len(), 1);
    assert_eq!(fx.text("notes.txt"), expected);
}
