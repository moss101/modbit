//! Verification the Core runs for itself after the model's edits (PX-145,
//! DR-PX-2026-10-25-017; docs/28 §4-§5, docs/64).
//!
//! The harness already refuses a further edit while a verification failure is
//! open without a recorded `repair.attempt`, and a repair attempt is concluded
//! by the next verification. Both used to wait on the model choosing to call
//! `verify.run`: a model that edited and carried on never learned that it had
//! broken the build until it tried to complete. Here the Core runs a TARGETED
//! stage at the end of any turn that wrote, reports what it found in a notice
//! the model reads at the start of its next turn, concludes the repair attempt
//! the failure belongs to, and sends the model back to the failure with the
//! lines of source the diagnostics name.
//!
//! This adds no second verification engine, plan, repair policy or
//! transcript: it calls the one stage runner (`run_verification`), the one
//! attempt-conclusion path (`conclude_repair`) and records the notice as an
//! event the transcript rebuild replays, so a resumed run is told exactly what
//! the live one was. It runs the plan's mandatory checks and nothing else; it
//! cannot select, skip or weaken one. A task whose plan has no mandatory check
//! is not verified here (an empty set is never a pass, and never a failure
//! either): the model's own `task.complete` still meets the gate.

use std::path::Path;
use std::sync::Arc;

use modbit_core_runtime::harness::{HarnessState, MAX_AUTO_VERIFICATIONS, RepairEscalation};
use modbit_domain::event::{Actor, AggregateType};
use modbit_domain::task::Task;
use modbit_verification::Stage;
use serde_json::json;

use crate::changes::{record, records_aggregate};
use crate::runtime::{Lineage, TranscriptEntry};
use crate::server::Core;

/// The event type of the notice a turn's automatic verification left.
pub(crate) const NOTICE_EVENT: &str = "AutoVerificationDelivered";

/// Longest notice, in characters; the full output stays behind its reference.
const MAX_NOTICE_CHARS: usize = 7_000;
/// Files whose lines are quoted for the model, and lines either side.
const MAX_SNIPPET_FILES: usize = 3;
const SNIPPET_CONTEXT: usize = 6;
/// Most of the raw output searched for locations.
const MAX_RAW_BYTES: usize = 256 * 1024;

/// What one automatic verification found.
pub(crate) struct AutoVerified {
    /// The notice for the model, already recorded on the log.
    pub notice: String,
    /// The repair policy's verdict when the verification concluded an attempt.
    pub escalation: Option<RepairEscalation>,
}

/// Whether the Core owes this turn's edits a verification.
///
/// Not for a task that has nothing to verify against (no workspace), a
/// disposable review or replay, a subagent (its parent verifies what it
/// integrates), or one that has used its allowance; and not when
/// `MODBIT_AUTO_VERIFY=0` turned it off for the process.
///
/// Not while the plan still names files the task has not written, unless the
/// turn asked for no tool at all (`idle`): a model in the middle of a
/// multi-file change is not told its half-made edit fails, which would only
/// open a repair obligation for a state it is on its way out of. The moment
/// it stops asking for tools, or has written what it said it would, or calls
/// `task.complete` (whose own COMPLETION run is not deferred), the Core looks.
pub(crate) fn due(task: &Task, state: &HarnessState, idle: bool) -> bool {
    use modbit_domain::task::TaskOrigin;
    owed(state, idle)
        && task.workspace_root.is_some()
        && !matches!(
            task.origin,
            TaskOrigin::Review | TaskOrigin::Replay | TaskOrigin::Subagent
        )
        && std::env::var("MODBIT_AUTO_VERIFY").map_or(true, |v| v.trim() != "0")
}

/// The part of [`due`] that reads only the harness state.
fn owed(state: &HarnessState, idle: bool) -> bool {
    state.unverified_write
        && state.baseline_recorded
        && state.plan.is_some()
        && state.capsule.is_none()
        && state.auto_verifications < MAX_AUTO_VERIFICATIONS
        && (idle || state.plan_edits_complete())
}

/// Run the TARGETED stage over the edits just made.
///
/// `None` when the plan holds no mandatory check (nothing was run).
#[allow(clippy::too_many_arguments)]
pub(crate) async fn after_edits(
    core: &Arc<Core>,
    task: &Task,
    lt: Lineage,
    lturn: Lineage,
    actor: &Actor,
    state: &mut HarnessState,
    ordinal: u32,
) -> Option<AutoVerified> {
    let (plan, _) = crate::runtime::verification_plan(core, task, state).await;
    if !plan.commands.iter().any(|c| c.mandatory) {
        // Nothing to run: leave the edit pending so a plan that gains a
        // check (a configured file the user adds) is still honoured.
        return None;
    }
    state.auto_verifications += 1;
    let before = state.open_verify_signatures();
    let (entry, ok, revision) =
        crate::runtime::run_verification(core, task, lturn, actor, state, Stage::Targeted, ordinal)
            .await;
    let observation = match &entry {
        TranscriptEntry::ToolResult { text, .. } => text.clone(),
        _ => String::new(),
    };
    // A repair attempt the failure belongs to is concluded by this
    // verification exactly as a `verify.run` of the model's would.
    let escalation = if state.pending_attempt.is_some() {
        crate::runtime::conclude_repair(core, task, lt, actor, state, &before, revision.clone())
            .await
    } else {
        None
    };
    let open = state.open_verify_signatures();
    let diagnosis = if open.is_empty() {
        String::new()
    } else {
        diagnosis_context(core, task, &observation).await
    };
    let notice = compose(state, &revision, ok, &open, &observation, &diagnosis);
    {
        let mut store = core.store.lock().await;
        let _ = crate::runtime::append(
            &mut store,
            core,
            lt,
            AggregateType::Workspace,
            records_aggregate(task.task_id),
            vec![record(
                NOTICE_EVENT,
                json!({
                    "notice": notice,
                    "ok": ok,
                    "candidate_revision": revision,
                    "open_failures": open,
                    "ordinal": state.auto_verifications,
                }),
                actor,
            )],
        );
    }
    Some(AutoVerified { notice, escalation })
}

/// The notice, from what the run found and what the repair policy holds.
fn compose(
    state: &HarnessState,
    revision: &str,
    ok: bool,
    open: &[String],
    observation: &str,
    diagnosis: &str,
) -> String {
    let mut text = format!(
        "[modbit: automatic verification] The Core ran the plan's mandatory checks after your edits (candidate {revision}); you did not have to call verify.run.\n"
    );
    text.push_str(&clip(observation.trim_end(), 3_800));
    text.push('\n');
    if ok && open.is_empty() {
        text.push_str(
            "result: every mandatory check passed at this revision. If the objective is met, finish with task.complete (the completion run repeats these checks); otherwise carry on.\n",
        );
        return clip(&text, MAX_NOTICE_CHARS);
    }
    if open.is_empty() {
        text.push_str(
            "result: the checks could not establish an outcome (see the status above); nothing is recorded as failing. Look at the output before you rely on it.\n",
        );
        return clip(&text, MAX_NOTICE_CHARS);
    }
    text.push_str(&format!(
        "result: {} open failure(s): {}\n",
        open.len(),
        open.join(", ")
    ));
    // What has been tried against these failures, so the next hypothesis is
    // a different one and not the same fix again.
    let mut tried = String::new();
    for sig in open {
        let bare = sig.strip_prefix("verify:").unwrap_or(sig);
        let prior: Vec<_> = state
            .repair_attempts
            .iter()
            .filter(|a| a.failure_signature == *sig || a.failure_signature == bare)
            .collect();
        for a in prior {
            tried.push_str(&format!(
                "  - attempt {} on {}: {} -> {}\n",
                a.attempt_ordinal,
                bare,
                clip(a.hypothesis.trim(), 160),
                a.outcome.as_deref().unwrap_or("not concluded"),
            ));
        }
    }
    if !tried.is_empty() {
        text.push_str("attempts already made on these failures:\n");
        text.push_str(&tried);
        text.push_str("A repeat of a hypothesis that did not work is refused; state a different cause, or revise the plan.\n");
    }
    if !diagnosis.is_empty() {
        text.push_str("source the diagnostics point at:\n");
        text.push_str(diagnosis);
    }
    text.push_str(&format!(
        "next: record repair.attempt (failure_signature, hypothesis = the cause you found, intended_fix) for one open failure, make the smallest change that removes that cause, and the Core verifies again when you stop editing. attempts used: {}/{} on the task. Never skip, disable or weaken a test or a check to get a pass.\n",
        state.repair_attempts.len(),
        state.repair_policy.max_attempts_per_task
    ));
    clip(&text, MAX_NOTICE_CHARS)
}

/// The lines around the first locations the failing output names, read from
/// the workspace as it is now. Data for the model to read, nothing more: a
/// path outside the workspace or under `node_modules`, `target` or `.git` is
/// not quoted.
async fn diagnosis_context(core: &Arc<Core>, task: &Task, observation: &str) -> String {
    let Some(root) = task.workspace_root.as_deref() else {
        return String::new();
    };
    // The observation names the raw output of each report; the locations are
    // in that output, not in the tail the model was shown.
    let mut raw = String::new();
    {
        let store = core.store.lock().await;
        for token in observation.split_whitespace() {
            if let Some(r) = token.strip_prefix("raw_output_ref=")
                && let Ok(bytes) = store.objects().get(r)
            {
                let take = bytes.len().min(MAX_RAW_BYTES);
                raw.push_str(&String::from_utf8_lossy(&bytes[..take]));
                raw.push('\n');
            }
        }
    }
    if raw.is_empty() {
        raw = observation.to_owned();
    }
    let mut out = String::new();
    let mut seen: Vec<String> = Vec::new();
    for (path, line) in locations(&raw) {
        if seen.len() >= MAX_SNIPPET_FILES {
            break;
        }
        if seen.contains(&path) {
            continue;
        }
        let Some(snippet) = snippet(Path::new(root), &path, line) else {
            continue;
        };
        seen.push(path.clone());
        out.push_str(&format!("--- {path} (line {line}) ---\n{snippet}\n"));
    }
    out
}

/// `(path, line)` pairs a compiler, type checker, test runner or interpreter
/// printed, in the order they appear. Heuristic by design: it only chooses
/// what to quote, and what it quotes is checked against the workspace.
pub(crate) fn locations(output: &str) -> Vec<(String, usize)> {
    let mut found = Vec::new();
    for raw_line in output.lines() {
        let line = raw_line.trim();
        // rustc: `--> src/main.rs:12:5`
        if let Some(rest) = line.strip_prefix("--> ")
            && let Some(loc) = path_line(rest)
        {
            found.push(loc);
            continue;
        }
        // python: `File "app/x.py", line 12, in f`
        if let Some(rest) = line.strip_prefix("File \"")
            && let Some((path, tail)) = rest.split_once("\", line ")
        {
            let n: String = tail.chars().take_while(char::is_ascii_digit).collect();
            if let Ok(n) = n.parse() {
                found.push((path.to_owned(), n));
                continue;
            }
        }
        // tsc: `src/a.ts(12,5): error TS2322: ...`
        if let Some(open) = line.find('(')
            && let Some(close) = line[open..].find(')')
            && line[open + close..].starts_with("): ")
        {
            let path = &line[..open];
            let first = line[open + 1..open + close].split(',').next().unwrap_or("");
            if let (true, Ok(n)) = (looks_like_path(path), first.parse::<usize>()) {
                found.push((path.to_owned(), n));
                continue;
            }
        }
        // gcc, go, eslint (unix), tsc (pretty), node: `path:12:5 ...`
        if let Some(loc) = path_line(line) {
            found.push(loc);
        }
    }
    found
}

/// `path:12` or `path:12:5` at the start of `s`, with a plausible path.
fn path_line(s: &str) -> Option<(String, usize)> {
    let mut parts = s.splitn(3, ':');
    let path = parts.next()?;
    let line: String = parts
        .next()?
        .trim()
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    let n: usize = line.parse().ok()?;
    looks_like_path(path).then(|| (path.trim_start_matches("./").to_owned(), n))
}

fn looks_like_path(p: &str) -> bool {
    let p = p.trim();
    !p.is_empty()
        && p.len() < 300
        && !p.contains(char::is_whitespace)
        && p.rsplit('/')
            .next()
            .is_some_and(|f| f.contains('.') && !f.starts_with('.'))
}

/// The numbered lines around `line` of the workspace-relative `rel`, or
/// `None` when it is not a regular file the workspace holds.
fn snippet(root: &Path, rel: &str, line: usize) -> Option<String> {
    let rel = rel.trim_start_matches("./");
    let p = Path::new(rel);
    if p.is_absolute()
        || p.components().any(|c| {
            matches!(
                c,
                std::path::Component::ParentDir | std::path::Component::Prefix(_)
            ) || matches!(c, std::path::Component::Normal(n) if
                n == "node_modules" || n == "target" || n == ".git" || n == "dist")
        })
    {
        return None;
    }
    let full = root.join(p);
    let meta = std::fs::symlink_metadata(&full).ok()?;
    if !meta.is_file() || meta.len() > 512 * 1024 {
        return None;
    }
    let text = std::fs::read_to_string(&full).ok()?;
    let lines: Vec<&str> = text.lines().collect();
    if line == 0 || line > lines.len() {
        return None;
    }
    let from = line.saturating_sub(SNIPPET_CONTEXT + 1);
    let to = (line + SNIPPET_CONTEXT).min(lines.len());
    Some(
        lines[from..to]
            .iter()
            .enumerate()
            .map(|(i, l)| {
                let n = from + i + 1;
                format!(
                    "{}{n:>5}  {}",
                    if n == line { ">" } else { " " },
                    clip(l, 200)
                )
            })
            .collect::<Vec<_>>()
            .join("\n"),
    )
}

fn clip(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_owned();
    }
    let cut: String = s.chars().take(max).collect();
    format!("{cut}…")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_half_made_multi_file_edit_is_not_verified_until_the_model_stops_or_finishes_it() {
        use modbit_core_runtime::harness::Plan;
        let mut state = HarnessState {
            baseline_recorded: true,
            ..Default::default()
        };
        state.plan = Some(Plan {
            outcome: "x".into(),
            expected_files: vec!["a.ts".into(), "b.ts".into(), "docs/".into()],
            ..Default::default()
        });
        assert!(!owed(&state, true), "nothing written yet");
        state.note_write("a.ts");
        assert!(!owed(&state, false), "b.ts and docs/ are still to come");
        assert!(owed(&state, true), "the model asked for no tool: look now");
        state.note_write("b.ts");
        assert!(!owed(&state, false), "a file under docs/ is still to come");
        state.note_write("docs/readme.md");
        assert!(owed(&state, false), "everything the plan named is written");
        // A plan that names nothing waits for nothing.
        state.plan.as_mut().unwrap().expected_files.clear();
        state.written_paths.clear();
        state.note_write("c.ts");
        assert!(owed(&state, false));
        // No baseline, no plan, a subagent capsule or an exhausted allowance: not owed.
        let mut s = state.clone();
        s.baseline_recorded = false;
        assert!(!owed(&s, true));
        let mut s = state.clone();
        s.plan = None;
        assert!(!owed(&s, true));
        let mut s = state.clone();
        s.auto_verifications = MAX_AUTO_VERIFICATIONS;
        assert!(!owed(&s, true));
        // A verification clears the debt.
        state.unverified_write = false;
        assert!(!owed(&state, true));
    }

    #[test]
    fn locations_are_read_from_the_formats_toolchains_print() {
        let out = "\
src/a.ts(12,5): error TS2322: Type 'string' is not assignable to type 'number'.
src/b.tsx:7:3 - error TS2304: Cannot find name 'x'.
 --> src/main.rs:30:9
  File \"app/util.py\", line 41, in helper
./cmd/main.go:9:2: undefined: foo
Found 4 errors in 3 files.
> pkg@1.0.0 build
";
        assert_eq!(
            locations(out),
            vec![
                ("src/a.ts".to_owned(), 12),
                ("src/b.tsx".to_owned(), 7),
                ("src/main.rs".to_owned(), 30),
                ("app/util.py".to_owned(), 41),
                ("cmd/main.go".to_owned(), 9),
            ]
        );
    }

    #[test]
    fn a_snippet_is_quoted_only_from_a_regular_file_inside_the_workspace() {
        let d = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(d.path().join("src")).unwrap();
        std::fs::create_dir_all(d.path().join("node_modules/x")).unwrap();
        let body: String = (1..=20).map(|i| format!("line {i}\n")).collect();
        std::fs::write(d.path().join("src/a.ts"), &body).unwrap();
        std::fs::write(d.path().join("node_modules/x/i.js"), &body).unwrap();
        let s = snippet(d.path(), "src/a.ts", 10).unwrap();
        assert!(s.contains(">   10  line 10"), "{s}");
        assert!(
            s.contains("    4  line 4") && s.contains("   16  line 16"),
            "{s}"
        );
        assert!(!s.contains("line 17"), "{s}");
        assert!(snippet(d.path(), "node_modules/x/i.js", 3).is_none());
        assert!(snippet(d.path(), "../etc/passwd", 1).is_none());
        assert!(snippet(d.path(), "/etc/passwd", 1).is_none());
        assert!(snippet(d.path(), "src/missing.ts", 1).is_none());
        assert!(snippet(d.path(), "src/a.ts", 99).is_none());
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink("/etc/hosts", d.path().join("src/link.ts")).unwrap();
            assert!(
                snippet(d.path(), "src/link.ts", 1).is_none(),
                "a link is not followed"
            );
        }
    }

    #[test]
    fn the_notice_names_the_failures_what_was_tried_and_the_next_step() {
        let mut state = HarnessState::default();
        state
            .repair_attempts
            .push(modbit_core_runtime::harness::RepairAttempt {
                attempt_ordinal: 1,
                failure_signature: "verify:build:node:abc".into(),
                hypothesis: "the import path is wrong".into(),
                hypothesis_fingerprint: "f".into(),
                evidence_refs: vec![],
                intended_fix: String::new(),
                start_revision: 3,
                outcome: Some("NO_EFFECT".into()),
                change_fingerprint: None,
                start_fingerprint: None,
            });
        let open = vec!["verify:build:node:abc".to_owned()];
        let n = compose(
            &state,
            "ws-rev-7",
            false,
            &open,
            "status: Failed\n",
            "--- src/a.ts (line 3) ---\n>    3  x\n",
        );
        assert!(n.contains("automatic verification"));
        assert!(n.contains("1 open failure(s): verify:build:node:abc"));
        assert!(n.contains("attempt 1 on build:node:abc: the import path is wrong -> NO_EFFECT"));
        assert!(n.contains("src/a.ts (line 3)"));
        assert!(n.contains("repair.attempt"));
        assert!(n.contains("Never skip, disable or weaken"));
        let pass = compose(&state, "ws-rev-8", true, &[], "status: Passed\n", "");
        assert!(pass.contains("every mandatory check passed"));
        assert!(!pass.contains("repair.attempt"));
        let unknown = compose(&state, "ws-rev-8", false, &[], "status: Unknown\n", "");
        assert!(unknown.contains("could not establish an outcome"));
    }
}
