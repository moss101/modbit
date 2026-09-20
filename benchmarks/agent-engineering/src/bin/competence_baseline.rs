//! `competence-baseline`: run the internal competence suite through the real
//! product and write the immutable baseline bundle (PX-020, docs/63).
//!
//! Every trial is a fresh Core profile on a fresh copy of a fixture
//! repository, driven only through `modbit-cli` (session create, task
//! create, task run --wait, question answer, approval resolve, task status,
//! task economics, events tail). The Core takes its provider endpoints from
//! this process's environment exactly as in production
//! (`OPENAI_API_KEY` / `ANTHROPIC_API_KEY`, `MODBIT_<P>_BASE_URL`, `_MODELS`,
//! `_AUTH`, `_EXTRA_BODY`); no credential is read, printed or stored here.
//!
//! ```text
//! competence-baseline --suite benchmarks/agent-engineering/suites/internal/tasks.json \
//!   --fixtures tests/fixtures/repos --out evidence/m3/PX-020/internal \
//!   --endpoint openai --model glm-5.3-flash [--trials 3] [--max-turns 24] \
//!   [--task <id>]... [--trial-timeout-secs 900] [--keep] [--cli <bin>] [--core <bin>]
//! ```
//!
//! Exit 0 when the bundle validates and is written; 1 otherwise.

use std::collections::BTreeMap;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use modbit_bench_agent_engineering::{
    Bundle, Economics, Environment, EventLog, HARNESS_VERSION, Protocol, Suite, TaskSpec,
    TrialOutcome, count, metrics, parse_events, sha256_hex,
};

struct Args {
    suite: PathBuf,
    fixtures: PathBuf,
    out: PathBuf,
    trials: Option<u32>,
    endpoint: String,
    model: String,
    max_turns: Option<u32>,
    tasks: Vec<String>,
    trial_timeout: Duration,
    keep: bool,
    cli: PathBuf,
    core: PathBuf,
}

fn usage() -> ! {
    eprintln!(
        "usage: competence-baseline --suite <tasks.json> --fixtures <dir> --out <dir> --endpoint <openai|anthropic> [--model <id>] [--trials N] [--max-turns N] [--task <id>]... [--trial-timeout-secs N] [--keep] [--cli <bin>] [--core <bin>]"
    );
    std::process::exit(1)
}

fn parse_args() -> Args {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let mut a = Args {
        suite: PathBuf::from("benchmarks/agent-engineering/suites/internal/tasks.json"),
        fixtures: PathBuf::from("tests/fixtures/repos"),
        out: PathBuf::from("target/competence-baseline"),
        trials: None,
        endpoint: String::new(),
        model: String::new(),
        max_turns: None,
        tasks: Vec::new(),
        trial_timeout: Duration::from_secs(900),
        keep: false,
        cli: PathBuf::new(),
        core: PathBuf::new(),
    };
    let mut i = 0;
    let value = |i: &mut usize| -> String {
        *i += 1;
        argv.get(*i).cloned().unwrap_or_else(|| usage())
    };
    while i < argv.len() {
        match argv[i].as_str() {
            "--suite" => a.suite = PathBuf::from(value(&mut i)),
            "--fixtures" => a.fixtures = PathBuf::from(value(&mut i)),
            "--out" => a.out = PathBuf::from(value(&mut i)),
            "--trials" => a.trials = Some(value(&mut i).parse().unwrap_or_else(|_| usage())),
            "--endpoint" => a.endpoint = value(&mut i),
            "--model" => a.model = value(&mut i),
            "--max-turns" => a.max_turns = Some(value(&mut i).parse().unwrap_or_else(|_| usage())),
            "--task" => a.tasks.push(value(&mut i)),
            "--trial-timeout-secs" => {
                a.trial_timeout =
                    Duration::from_secs(value(&mut i).parse().unwrap_or_else(|_| usage()));
            }
            "--keep" => a.keep = true,
            "--cli" => a.cli = PathBuf::from(value(&mut i)),
            "--core" => a.core = PathBuf::from(value(&mut i)),
            _ => usage(),
        }
        i += 1;
    }
    if a.endpoint.is_empty() {
        usage();
    }
    if a.model.is_empty() {
        a.model = env_nonempty(&format!(
            "MODBIT_{}_LIVE_MODEL",
            a.endpoint.to_ascii_uppercase()
        ))
        .or_else(|| env_nonempty("MODBIT_LIVE_MODEL"))
        .unwrap_or_else(|| {
            eprintln!("competence-baseline: --model or MODBIT_LIVE_MODEL is required");
            std::process::exit(1)
        });
    }
    let sibling = |name: &str| -> PathBuf {
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(Path::to_path_buf))
            .map(|d| d.join(name))
            .unwrap_or_else(|| PathBuf::from(name))
    };
    if a.cli.as_os_str().is_empty() {
        a.cli = env_nonempty("MODBIT_CLI_BIN").map_or_else(|| sibling("modbit-cli"), PathBuf::from);
    }
    if a.core.as_os_str().is_empty() {
        a.core =
            env_nonempty("MODBIT_CORE_BIN").map_or_else(|| sibling("modbit-core"), PathBuf::from);
    }
    a
}

fn env_nonempty(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.trim().is_empty())
}

/// Output of one command.
struct Run {
    code: i32,
    stdout: String,
    stderr: String,
    timed_out: bool,
}

/// Run to completion or until `timeout`; a timed-out child is killed.
fn run(cmd: &mut Command, timeout: Duration) -> Run {
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            return Run {
                code: -1,
                stdout: String::new(),
                stderr: format!("spawn failed: {e}"),
                timed_out: false,
            };
        }
    };
    let mut out_pipe = child.stdout.take();
    let mut err_pipe = child.stderr.take();
    let reader = |pipe: &mut Option<std::process::ChildStdout>| {
        pipe.take().map(|mut p| {
            std::thread::spawn(move || {
                let mut s = String::new();
                let _ = std::io::Read::read_to_string(&mut p, &mut s);
                s
            })
        })
    };
    let err_reader = |pipe: &mut Option<std::process::ChildStderr>| {
        pipe.take().map(|mut p| {
            std::thread::spawn(move || {
                let mut s = String::new();
                let _ = std::io::Read::read_to_string(&mut p, &mut s);
                s
            })
        })
    };
    let out_t = reader(&mut out_pipe);
    let err_t = err_reader(&mut err_pipe);
    let started = Instant::now();
    let mut timed_out = false;
    let status = loop {
        match child.try_wait() {
            Ok(Some(st)) => break Some(st),
            Ok(None) => {
                if started.elapsed() > timeout {
                    timed_out = true;
                    let _ = child.kill();
                    break child.wait().ok();
                }
                std::thread::sleep(Duration::from_millis(200));
            }
            Err(_) => break None,
        }
    };
    let stdout = out_t.and_then(|t| t.join().ok()).unwrap_or_default();
    let stderr = err_t.and_then(|t| t.join().ok()).unwrap_or_default();
    Run {
        code: status.and_then(|s| s.code()).unwrap_or(-1),
        stdout,
        stderr,
        timed_out,
    }
}

fn cmd_output(program: &str, args: &[&str]) -> String {
    Command::new(program)
        .args(args)
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
        .unwrap_or_default()
}

fn git(dir: &Path, args: &[&str]) -> Result<String, String> {
    let o = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .map_err(|e| format!("git {}: {e}", args.join(" ")))?;
    if !o.status.success() {
        return Err(format!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&o.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&o.stdout).into_owned())
}

/// Copy a fixture without its build products, LF-normalized, and link its
/// installed modules so a configured command runs as a developer's would.
fn copy_fixture(src: &Path, dst: &Path) -> Result<(), String> {
    fn walk(src: &Path, dst: &Path) -> Result<(), String> {
        std::fs::create_dir_all(dst).map_err(|e| format!("{}: {e}", dst.display()))?;
        for e in std::fs::read_dir(src).map_err(|e| format!("{}: {e}", src.display()))? {
            let e = e.map_err(|e| e.to_string())?;
            let n = e.file_name();
            let name = n.to_string_lossy();
            if name == "target"
                || name == "node_modules"
                || name == ".vitest"
                || name.starts_with(".vite")
            {
                continue;
            }
            let p = e.path();
            if p.is_dir() {
                walk(&p, &dst.join(&n))?;
            } else {
                let bytes = std::fs::read(&p).map_err(|e| format!("{}: {e}", p.display()))?;
                let text = String::from_utf8_lossy(&bytes).replace("\r\n", "\n");
                std::fs::write(dst.join(&n), text).map_err(|e| e.to_string())?;
            }
        }
        Ok(())
    }
    walk(src, dst)?;
    let installed = src.join("node_modules");
    if installed.exists() {
        #[cfg(unix)]
        std::os::unix::fs::symlink(&installed, dst.join("node_modules"))
            .map_err(|e| e.to_string())?;
        #[cfg(windows)]
        std::os::windows::fs::symlink_dir(&installed, dst.join("node_modules"))
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn commit_all(dir: &Path, message: &str) -> Result<(), String> {
    git(dir, &["add", "-A"])?;
    git(
        dir,
        &[
            "-c",
            "user.name=bench",
            "-c",
            "user.email=bench@modbit",
            "commit",
            "-q",
            "--allow-empty",
            "-m",
            message,
        ],
    )?;
    Ok(())
}

fn first_word_after(text: &str, prefix: &str) -> Option<String> {
    text.lines()
        .find_map(|l| l.strip_prefix(prefix))
        .and_then(|rest| rest.split_whitespace().next())
        .map(str::to_owned)
}

fn kv(text: &str, key: &str) -> Option<String> {
    text.split_whitespace()
        .find_map(|tok| tok.strip_prefix(&format!("{key}=")))
        .map(str::to_owned)
}

struct Cli<'a> {
    bin: &'a Path,
    data_dir: PathBuf,
    env: Vec<(String, String)>,
    log: std::fs::File,
}

impl Cli<'_> {
    fn call(&mut self, args: &[&str], timeout: Duration) -> Run {
        let mut c = Command::new(self.bin);
        c.arg("--data-dir").arg(&self.data_dir).args(args);
        for (k, v) in &self.env {
            c.env(k, v);
        }
        let r = run(&mut c, timeout);
        let _ = writeln!(
            self.log,
            "$ modbit-cli {}\n[exit {}{}]\n{}{}",
            args.join(" "),
            r.code,
            if r.timed_out { " TIMEOUT" } else { "" },
            r.stdout,
            if r.stderr.is_empty() {
                String::new()
            } else {
                format!("[stderr]\n{}", r.stderr)
            }
        );
        r
    }
}

struct TrialResult {
    outcome: TrialOutcome,
}

#[allow(clippy::too_many_lines)]
fn run_trial(
    args: &Args,
    suite: &Suite,
    suite_path: &Path,
    task: &TaskSpec,
    trial: u32,
    max_turns: u32,
) -> Result<TrialResult, String> {
    let slug = task.id.replace('/', "__");
    let trial_dir = args.out.join("trials").join(format!("{slug}-{trial}"));
    std::fs::create_dir_all(&trial_dir).map_err(|e| e.to_string())?;
    let scratch = std::env::temp_dir().join(format!(
        "modbit-bench-{}-{slug}-{trial}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&scratch);
    let workspace = scratch.join("workspace");
    let data_dir = scratch.join("profile");
    std::fs::create_dir_all(&data_dir).map_err(|e| e.to_string())?;
    copy_fixture(&args.fixtures.join(&task.fixture), &workspace)?;
    git(&workspace, &["init", "-q", "-b", "main"])?;
    git(&workspace, &["config", "core.autocrlf", "false"])?;
    commit_all(&workspace, "fixture")?;
    for op in &task.setup {
        match op {
            modbit_bench_agent_engineering::FileOp::Write { path, content } => {
                let p = workspace.join(path);
                if let Some(parent) = p.parent() {
                    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                }
                std::fs::write(&p, content).map_err(|e| e.to_string())?;
            }
            modbit_bench_agent_engineering::FileOp::Delete { path } => {
                let _ = std::fs::remove_file(workspace.join(path));
            }
        }
    }
    if !task.setup.is_empty() {
        commit_all(&workspace, "task setup")?;
    }
    let protected_before: BTreeMap<String, String> = task
        .acceptance
        .protected_tests
        .iter()
        .map(|p| {
            (
                p.clone(),
                std::fs::read(workspace.join(p))
                    .map(|b| sha256_hex(&b))
                    .unwrap_or_default(),
            )
        })
        .collect();
    let workspace_abs = workspace
        .canonicalize()
        .map_err(|e| e.to_string())?
        .to_string_lossy()
        .trim_start_matches(r"\\?\")
        .to_owned();
    let flaky_state = scratch.join("flaky-state");
    let mut cli = Cli {
        bin: &args.cli,
        data_dir: data_dir.clone(),
        env: vec![
            (
                "MODBIT_CORE_BIN".into(),
                args.core.to_string_lossy().into_owned(),
            ),
            (
                "FIXTURE_FLAKY_STATE".into(),
                flaky_state.to_string_lossy().into_owned(),
            ),
        ],
        log: std::fs::File::create(trial_dir.join("cli.log")).map_err(|e| e.to_string())?,
    };
    let short = Duration::from_secs(120);
    let mut notes = Vec::new();
    let r = cli.call(&["session", "create"], short);
    let session = first_word_after(&r.stdout, "session ")
        .ok_or_else(|| format!("session create: {}{}", r.stdout, r.stderr))?;
    let r = cli.call(
        &[
            "task",
            "create",
            "--session",
            &session,
            "--workspace",
            &workspace_abs,
            &task.goal,
        ],
        short,
    );
    let task_id = first_word_after(&r.stdout, "task ")
        .ok_or_else(|| format!("task create: {}{}", r.stdout, r.stderr))?;
    let turns = max_turns.to_string();
    let run_args = [
        "task",
        "run",
        "--session",
        &session,
        "--task",
        &task_id,
        "--endpoint",
        &args.endpoint,
        "--model",
        &args.model,
        "--max-turns",
        &turns,
        "--wait",
    ];
    let started = Instant::now();
    let mut r = cli.call(&run_args, args.trial_timeout);
    let mut interactions = 0u32;
    let mut state = kv(&r.stdout, "state").unwrap_or_default();
    let mut timed_out = r.timed_out;
    // A Waiting task is answered by protocol: one fixed answer to a typed
    // question, a denial to any protected-effect approval, then a resume.
    while !timed_out && r.code == 2 && interactions < 4 {
        interactions += 1;
        let remaining = args.trial_timeout.saturating_sub(started.elapsed());
        if remaining.is_zero() {
            timed_out = true;
            break;
        }
        let q = cli.call(&["question", "list", "--task", &task_id], short);
        let pending_q: Vec<String> = q
            .stdout
            .lines()
            .filter(|l| l.starts_with("question ") && l.contains("answered=false"))
            .filter_map(|l| l.split_whitespace().nth(1).map(str::to_owned))
            .collect();
        if let Some(qid) = pending_q.first() {
            notes.push(format!("question {qid} answered by protocol"));
            r = cli.call(
                &[
                    "question",
                    "answer",
                    "--session",
                    &session,
                    "--task",
                    &task_id,
                    "--question",
                    qid,
                    "--endpoint",
                    &args.endpoint,
                    "--model",
                    &args.model,
                    "--wait",
                    &suite.protocol.question_answer,
                ],
                remaining,
            );
        } else {
            let a = cli.call(&["approval", "list", "--session", &session], short);
            let pending_a: Vec<String> = a
                .stdout
                .lines()
                .filter(|l| l.starts_with("approval ") && l.contains("status=PENDING"))
                .filter_map(|l| l.split_whitespace().nth(1).map(str::to_owned))
                .collect();
            for aid in &pending_a {
                notes.push(format!("approval {aid} denied by protocol"));
                let _ = cli.call(
                    &[
                        "approval",
                        "resolve",
                        "--session",
                        &session,
                        "--approval",
                        aid,
                        "deny",
                        "benchmark protocol: protected effects are denied",
                    ],
                    short,
                );
            }
            r = cli.call(&run_args, remaining);
        }
        state = kv(&r.stdout, "state").unwrap_or(state);
        timed_out = r.timed_out;
    }
    if timed_out {
        notes.push(format!(
            "trial timed out after {}s; task cancelled",
            args.trial_timeout.as_secs()
        ));
        let _ = cli.call(
            &["task", "cancel", "--session", &session, "--task", &task_id],
            short,
        );
        state = "TIMEOUT".into();
    }
    let st = cli.call(&["task", "status", "--task", &task_id], short);
    if let Some(s) = kv(&st.stdout, "state")
        && !timed_out
    {
        state = s;
    }
    let econ = cli.call(&["task", "economics", "--task", &task_id], short);
    let economics = parse_economics(&econ.stdout);
    let ev = cli.call(
        &[
            "events",
            "tail",
            "--session",
            &session,
            "--json",
            "--count",
            "200000",
        ],
        short,
    );
    let events_path = trial_dir.join("events.jsonl");
    std::fs::write(&events_path, &ev.stdout).map_err(|e| e.to_string())?;
    let events = parse_events(&ev.stdout);
    let counts = count(&events, &task_id);
    let diff = git(&workspace, &["diff", "HEAD"]).unwrap_or_default();
    std::fs::write(trial_dir.join("diff.patch"), &diff).map_err(|e| e.to_string())?;
    // Acceptance: hidden files placed only now, commands must all pass,
    // protected tests must be byte-identical (DI-3 scored by the harness too).
    let mut acceptance_log = String::new();
    for f in &task.acceptance.hidden_files {
        let src = Suite::hidden_source(suite_path, f);
        let dst = workspace.join(&f.path);
        if let Some(parent) = dst.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        std::fs::copy(&src, &dst).map_err(|e| format!("hidden {}: {e}", src.display()))?;
    }
    let mut acceptance_passed = true;
    for command in &task.acceptance.commands {
        let Some((program, rest)) = command.split_first() else {
            continue;
        };
        let mut c = Command::new(program);
        c.args(rest).current_dir(&workspace);
        c.env(
            "FIXTURE_FLAKY_STATE",
            scratch.join("acceptance-flaky-state"),
        );
        for (k, v) in &task.acceptance.env {
            c.env(k, v);
        }
        let res = run(&mut c, Duration::from_secs(600));
        acceptance_log.push_str(&format!(
            "$ {}\n[exit {}{}]\n{}{}\n",
            command.join(" "),
            res.code,
            if res.timed_out { " TIMEOUT" } else { "" },
            res.stdout,
            res.stderr
        ));
        if res.code != 0 {
            acceptance_passed = false;
        }
    }
    std::fs::write(trial_dir.join("acceptance.log"), &acceptance_log).map_err(|e| e.to_string())?;
    let test_integrity_ok = protected_before.iter().all(|(p, before)| {
        std::fs::read(workspace.join(p))
            .map(|b| sha256_hex(&b))
            .unwrap_or_default()
            == *before
    });
    let core_verified = (counts.ready_for_review || counts.completed)
        && !counts.failed
        && counts.regressions_attributed == 0
        && matches!(state.as_str(), "ReadyForReview" | "Completed");
    let verified_success = core_verified && acceptance_passed && test_integrity_ok;
    let outcome = TrialOutcome {
        task: task.id.clone(),
        trial,
        state,
        exit_code: r.code,
        core_verified,
        acceptance_passed,
        test_integrity_ok,
        verified_success,
        first_pass: verified_success && counts.repair_attempts == 0,
        first_candidate: verified_success
            && counts.repair_attempts <= 1
            && counts.repair_escalations == 0,
        counts: counts.clone(),
        economics,
        event_log: EventLog {
            file: format!("trials/{slug}-{trial}/events.jsonl"),
            sha256: sha256_hex(ev.stdout.as_bytes()),
            offsets: counts.offsets,
        },
        diff_sha256: sha256_hex(diff.as_bytes()),
        interactions,
        notes,
    };
    if !args.keep {
        let _ = std::fs::remove_dir_all(&scratch);
    }
    Ok(TrialResult { outcome })
}

fn parse_economics(text: &str) -> Economics {
    let num = |k: &str| kv(text, k).and_then(|v| v.parse::<u64>().ok()).unwrap_or(0);
    Economics {
        model_calls: u32::try_from(num("calls")).unwrap_or(u32::MAX),
        input_tokens: num("input_tokens"),
        cached_input_tokens: num("cached_input"),
        output_tokens: num("output_tokens"),
        cost_usd: kv(text, "cost_usd").and_then(|v| v.parse::<f64>().ok()),
        wall_ms: num("wall_ms"),
        model_ms: num("model_ms"),
        tool_ms: num("tool_ms"),
        tool_calls: u32::try_from(num("tool_calls")).unwrap_or(u32::MAX),
    }
}

fn base_host(endpoint: &str) -> String {
    let default = match endpoint {
        "anthropic" => "api.anthropic.com",
        _ => "api.openai.com",
    };
    env_nonempty(&format!(
        "MODBIT_{}_BASE_URL",
        endpoint.to_ascii_uppercase()
    ))
    .and_then(|u| {
        u.split("://")
            .nth(1)
            .map(|rest| rest.split('/').next().unwrap_or("").to_owned())
    })
    .unwrap_or_else(|| default.to_owned())
}

/// Prices from `MODBIT_<P>_MODELS` for the pinned model, when configured.
fn catalog_prices(endpoint: &str, model: &str) -> Option<(f64, f64)> {
    let spec = env_nonempty(&format!("MODBIT_{}_MODELS", endpoint.to_ascii_uppercase()))?;
    spec.split(',').map(str::trim).find_map(|entry| {
        let head = entry.split(';').next()?;
        let (m, prices) = head.split_once('=')?;
        if m.trim() != model {
            return None;
        }
        let (i, o) = prices.split_once('/')?;
        Some((i.trim().parse().ok()?, o.trim().parse().ok()?))
    })
}

fn now_rfc3339() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // Civil date from days since the epoch (proleptic Gregorian).
    let days = i64::try_from(secs / 86_400).unwrap_or(0);
    let (h, m, s) = ((secs % 86_400) / 3600, (secs % 3600) / 60, secs % 60);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let mo = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if mo <= 2 { y + 1 } else { y };
    format!("{y:04}-{mo:02}-{d:02}T{h:02}:{m:02}:{s:02}Z")
}

fn main() {
    let args = parse_args();
    let (suite, task_list_digest) = Suite::load(&args.suite).unwrap_or_else(|e| {
        eprintln!("competence-baseline: {e}");
        std::process::exit(1)
    });
    let trials = args.trials.unwrap_or(suite.protocol.trials);
    let max_turns = args.max_turns.unwrap_or(suite.protocol.max_turns);
    let selected: Vec<&TaskSpec> = suite
        .tasks
        .iter()
        .filter(|t| args.tasks.is_empty() || args.tasks.iter().any(|f| f == &t.id))
        .collect();
    if selected.is_empty() {
        eprintln!("competence-baseline: no task matches {:?}", args.tasks);
        std::process::exit(1);
    }
    for bin in [&args.cli, &args.core] {
        if !bin.is_file() {
            eprintln!("competence-baseline: binary not found: {}", bin.display());
            std::process::exit(1);
        }
    }
    std::fs::create_dir_all(&args.out).unwrap_or_else(|e| {
        eprintln!("competence-baseline: {}: {e}", args.out.display());
        std::process::exit(1)
    });
    let build_digest = {
        let core = std::fs::read(&args.core).unwrap_or_default();
        let cli = std::fs::read(&args.cli).unwrap_or_default();
        sha256_hex(
            &[
                sha256_hex(&core).into_bytes(),
                sha256_hex(&cli).into_bytes(),
            ]
            .concat(),
        )
    };
    let environment = Environment {
        modbit_revision: cmd_output("git", &["rev-parse", "HEAD"]),
        build_digest,
        harness_version: HARNESS_VERSION.into(),
        toolchains: [
            ("rustc", cmd_output("rustc", &["--version"])),
            ("cargo", cmd_output("cargo", &["--version"])),
            ("node", cmd_output("node", &["--version"])),
            ("python3", cmd_output("python3", &["--version"])),
            ("git", cmd_output("git", &["--version"])),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_owned(), v))
        .collect(),
        os: format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH),
    };
    let protocol = Protocol {
        trials_per_task: trials,
        max_turns,
        configuration: "direct".into(),
        endpoint: args.endpoint.clone(),
        model: args.model.clone(),
        base_url_host: base_host(&args.endpoint),
        catalog_prices_usd_per_mtok: catalog_prices(&args.endpoint, &args.model),
        gold_patch_access: false,
        images: vec![],
        question_answer: suite.protocol.question_answer.clone(),
        approvals: suite.protocol.approvals.clone(),
    };
    eprintln!(
        "competence-baseline: suite {} v{} ({} tasks, digest {}) — {} trials x {} tasks, endpoint {} at {} model {}, max_turns {}",
        suite.id,
        suite.version,
        selected.len(),
        &task_list_digest[..12],
        trials,
        selected.len(),
        protocol.endpoint,
        protocol.base_url_host,
        protocol.model,
        max_turns
    );
    let mut outcomes = Vec::new();
    for task in &selected {
        for trial in 1..=trials {
            let started = Instant::now();
            match run_trial(&args, &suite, &args.suite, task, trial, max_turns) {
                Ok(r) => {
                    let o = r.outcome;
                    eprintln!(
                        "competence-baseline: {} trial {}/{}: state={} core_verified={} acceptance={} integrity={} verified={} repairs={} interactions={} cost={:?} ({}s)",
                        task.id,
                        trial,
                        trials,
                        o.state,
                        o.core_verified,
                        o.acceptance_passed,
                        o.test_integrity_ok,
                        o.verified_success,
                        o.counts.repair_attempts,
                        o.interactions,
                        o.economics.cost_usd,
                        started.elapsed().as_secs()
                    );
                    outcomes.push(o);
                }
                Err(e) => {
                    eprintln!(
                        "competence-baseline: {} trial {trial}: harness error: {e}",
                        task.id
                    );
                    std::process::exit(1);
                }
            }
        }
    }
    let bundle = Bundle {
        schema: modbit_bench_agent_engineering::bundle::SCHEMA.into(),
        suite_id: suite.id.clone(),
        suite_version: suite.version.clone(),
        task_list_digest,
        tasks: selected.iter().map(|t| t.id.clone()).collect(),
        protocol,
        environment,
        metrics: metrics(&outcomes),
        trials: outcomes,
        generated_at: now_rfc3339(),
    };
    if let Err(e) = bundle.validate() {
        eprintln!("competence-baseline: bundle refused: {e}");
        std::process::exit(1);
    }
    let bytes = bundle.to_file_bytes();
    let digest = modbit_bench_agent_engineering::digest_of(&bytes);
    std::fs::write(args.out.join("baseline.json"), &bytes).expect("write baseline.json");
    std::fs::write(
        args.out.join("baseline.sha256"),
        format!("{digest}  baseline.json\n"),
    )
    .expect("write digest");
    let m = &bundle.metrics;
    let mut summary = format!(
        "# Competence baseline — {} v{}\n\n- task list digest: `{}`\n- bundle digest: `{digest}`\n- protocol: {} trials/task, max_turns {}, {} `{}` at {} ({})\n- Modbit revision: `{}`; build digest `{}`; harness {}\n\n| metric | value | 95% interval |\n|---|---|---|\n| verified success | {}/{} = {:.3} | {:.3}–{:.3} |\n| first-pass success (zero RepairAttempts) | {}/{} = {:.3} | {:.3}–{:.3} |\n| first-candidate success (at most one repair attempt, no escalation) | {}/{} = {:.3} | {:.3}–{:.3} |\n| repair loops (attempts→trials) | {:?} | — |\n| equivalent-hypothesis escalations | {} ({:.2}/trial) | — |\n| no-progress escalations | {} ({:.2}/trial) | — |\n| wrong-effect attempts blocked | {} ({:.2}/trial) | — |\n| evidence coverage | {}/{} = {:.3} | {:.3}–{:.3} |\n| cost | total {:.4} USD; per verified success {:?} | — |\n| edits without retrieval record | {} | must be 0 |\n| files outside original plan / expansions / questions per trial | {:.2} / {:.2} / {:.2} | — |\n| regressions attributed | {} | — |\n| flaky quarantines | {} ({:.2}/trial) | — |\n| DI-3 violations | {} | — |\n\n| task | verified | first-pass | states |\n|---|---|---|---|\n",
        bundle.suite_id,
        bundle.suite_version,
        bundle.task_list_digest,
        bundle.protocol.trials_per_task,
        bundle.protocol.max_turns,
        bundle.protocol.endpoint,
        bundle.protocol.model,
        bundle.protocol.base_url_host,
        bundle.protocol.configuration,
        bundle.environment.modbit_revision,
        &bundle.environment.build_digest[..12],
        bundle.environment.harness_version,
        m.verified_success.successes,
        m.verified_success.n,
        m.verified_success.rate,
        m.verified_success.ci95.0,
        m.verified_success.ci95.1,
        m.first_pass_success.successes,
        m.first_pass_success.n,
        m.first_pass_success.rate,
        m.first_pass_success.ci95.0,
        m.first_pass_success.ci95.1,
        m.first_candidate_success.successes,
        m.first_candidate_success.n,
        m.first_candidate_success.rate,
        m.first_candidate_success.ci95.0,
        m.first_candidate_success.ci95.1,
        m.repair_loops,
        m.equivalent_hypothesis_escalations.0,
        m.equivalent_hypothesis_escalations.1,
        m.no_progress_escalations.0,
        m.no_progress_escalations.1,
        m.wrong_effect_attempts_blocked.0,
        m.wrong_effect_attempts_blocked.1,
        m.evidence_coverage.successes,
        m.evidence_coverage.n,
        m.evidence_coverage.rate,
        m.evidence_coverage.ci95.0,
        m.evidence_coverage.ci95.1,
        m.cost_and_time.total_cost_usd,
        m.cost_and_time.cost_per_verified_success_usd,
        m.retrieval_discipline_edits_without_record,
        m.scope_discipline.files_outside_original_plan_per_trial,
        m.scope_discipline.scope_expansions_per_trial,
        m.scope_discipline.scope_questions_per_trial,
        m.regression_attribution,
        m.flaky_check_rate.0,
        m.flaky_check_rate.1,
        m.test_integrity_violations,
    );
    for (task, tm) in &m.per_task {
        summary.push_str(&format!(
            "| {task} | {}/{} | {}/{} | {} |\n",
            tm.verified_success.successes,
            tm.verified_success.n,
            tm.first_pass_success.successes,
            tm.first_pass_success.n,
            tm.states.join(", ")
        ));
    }
    std::fs::write(args.out.join("summary.md"), &summary).expect("write summary");
    print!("{summary}");
    println!(
        "\nbundle: {} (sha256 {digest})",
        args.out.join("baseline.json").display()
    );
}
