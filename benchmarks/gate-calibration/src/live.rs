//! The live mode of the widened calibration (PX-135, QUAL-PX-135 "a live
//! compatible gateway for the candidate-producing runs").
//!
//! Offline, the candidates are templates of known failure classes. Live, a
//! real model produces the candidates: for every lineage the seeded fixture
//! is materialised, the Core is driven through `modbit-cli` with the lineage's
//! goal (the specification in words and the name of the failing visible test,
//! never an oracle or a label), and whatever the model left in the tree
//! becomes a candidate. That candidate goes through exactly the same
//! previous-gate and widened-gate measurement and the same hidden oracle as
//! an offline one, and the Core's retained events (the model requests among
//! them) are searched for oracle needles.
//!
//! The mode exists to be refused. It uses the repository's `MODBIT_LIVE_*`
//! pattern (`modbit_bench_context_economics::live`): `MODBIT_LIVE=1`, a real
//! key and model, a non-loopback gateway unless the owner says otherwise. It
//! never invents a key, never falls back to a scripted provider and never
//! writes a report it did not measure. **This module's Core-driving path has
//! not been run against a provider** (no credential was available when it was
//! written); its pure parts (the refusal, the goal text, the tree diff, the
//! leak search over events) are tested.

use std::path::{Path, PathBuf};
use std::process::Command;

use modbit_policy::AssurancePolicy;
use serde::{Deserialize, Serialize};

pub use modbit_bench_context_economics::live::{LiveConfig, LiveRefused, live_config};

use crate::widened::{PairRow, WidenedCase, WidenedCorpus, adversarial_config};
use crate::{AcceptanceCase, FileOp, find_leaks, run_case};

/// One candidate-producing run and what became of it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LiveRow {
    /// The previous and widened gates and the oracle on the candidate.
    pub row: PairRow,
    /// The Core task's final state.
    pub task_state: String,
    /// Files the model changed (paths only).
    pub changed: Vec<String>,
    /// Needles found in the goal or the retained events.
    pub leaks: Vec<String>,
}

/// A live run's report: only runs that happened.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LiveReport {
    /// The model id.
    pub model: String,
    /// Runs per lineage.
    pub repeats: u32,
    /// Every run.
    pub rows: Vec<LiveRow>,
    /// Always `true`: this report was measured against a live provider.
    pub live: bool,
}

fn word_after<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    text.split_whitespace()
        .skip_while(|w| *w != key)
        .nth(1)
        .or_else(|| {
            text.split_whitespace()
                .find_map(|w| w.strip_prefix(&format!("{key}=")))
        })
}

fn field<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    text.split_whitespace()
        .find_map(|w| w.strip_prefix(&format!("{key}=")))
}

fn cli(cfg: &LiveConfig, data_dir: &Path, args: &[&str]) -> Result<String, String> {
    let out = Command::new(&cfg.run.cli)
        .arg("--data-dir")
        .arg(data_dir)
        .args(args)
        .env("MODBIT_CORE_BIN", &cfg.run.core)
        .envs(cfg.run.env.iter().map(|(k, v)| (k, v)))
        .output()
        .map_err(|e| format!("spawning modbit-cli: {e}"))?;
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    // `task status` exits non-zero for a task that is not done; its output
    // is still the answer.
    if !out.status.success() && !args.starts_with(&["task", "status"]) {
        return Err(format!(
            "modbit-cli {args:?}: {}{}",
            text,
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    Ok(text)
}

fn read_tree(root: &Path) -> std::collections::BTreeMap<String, String> {
    fn walk(dir: &Path, root: &Path, out: &mut std::collections::BTreeMap<String, String>) {
        let Ok(rd) = std::fs::read_dir(dir) else {
            return;
        };
        for e in rd.flatten() {
            let p = e.path();
            let name = e.file_name().to_string_lossy().into_owned();
            if p.is_dir() {
                if !matches!(
                    name.as_str(),
                    "target"
                        | "node_modules"
                        | ".git"
                        | "__pycache__"
                        | ".pytest_cache"
                        | ".modbit"
                ) {
                    walk(&p, root, out);
                }
            } else if let Ok(t) = std::fs::read_to_string(&p) {
                let rel = p
                    .strip_prefix(root)
                    .unwrap_or(&p)
                    .to_string_lossy()
                    .replace('\\', "/");
                out.insert(rel, t);
            }
        }
    }
    let mut out = std::collections::BTreeMap::new();
    walk(root, root, &mut out);
    out
}

/// What changed between a seeded tree and the tree the model left: a write
/// per new or modified text file, a delete per removed one. Build products,
/// caches and the verification configuration are not part of a candidate.
#[must_use]
pub fn tree_diff(before: &Path, after: &Path) -> Vec<FileOp> {
    let b = read_tree(before);
    let a = read_tree(after);
    let noise = |p: &str| {
        p.ends_with(".xml") && p.contains(".modbit-")
            || p.starts_with(".modbit-")
            || p == "Cargo.lock"
    };
    let mut ops = Vec::new();
    for (path, content) in &a {
        if noise(path) {
            continue;
        }
        if b.get(path) != Some(content) {
            ops.push(FileOp::Write {
                path: path.clone(),
                content: content.clone(),
            });
        }
    }
    for path in b.keys() {
        if !a.contains_key(path) && !noise(path) {
            ops.push(FileOp::Delete { path: path.clone() });
        }
    }
    ops
}

fn copy_dir(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for e in std::fs::read_dir(src)? {
        let e = e?;
        let name = e.file_name();
        if matches!(name.to_str(), Some("target" | "node_modules" | ".git")) {
            continue;
        }
        let p = e.path();
        if p.is_dir() {
            copy_dir(&p, &dst.join(&name))?;
        } else {
            std::fs::copy(&p, dst.join(&name))?;
        }
    }
    Ok(())
}

/// One lineage task: the fixture with the seed applied, and its goal.
fn materialise(repo_root: &Path, case: &AcceptanceCase, into: &Path) -> std::io::Result<()> {
    copy_dir(
        &repo_root.join("tests/fixtures/repos").join(&case.fixture),
        into,
    )?;
    for op in &case.seed {
        if let FileOp::Write { path, content } = op {
            let p = into.join(path);
            if let Some(parent) = p.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(p, content)?;
        }
    }
    Ok(())
}

/// Produce one candidate through the real Core, or say why not.
fn produce(
    cfg: &LiveConfig,
    repo_root: &Path,
    wc: &WidenedCase,
    work: &Path,
) -> Result<(Vec<FileOp>, String, String), String> {
    let repo = work.join("task");
    let data = work.join("data");
    let _ = std::fs::remove_dir_all(work);
    std::fs::create_dir_all(&data).map_err(|e| e.to_string())?;
    materialise(repo_root, &wc.case, &repo).map_err(|e| e.to_string())?;
    let pristine = work.join("pristine");
    materialise(repo_root, &wc.case, &pristine).map_err(|e| e.to_string())?;
    let session = cli(cfg, &data, &["session", "create"])?;
    let session = word_after(&session, "session")
        .ok_or("no session id")?
        .to_owned();
    let root = repo.canonicalize().map_err(|e| e.to_string())?;
    let root = root
        .to_string_lossy()
        .trim_start_matches(r"\\?\")
        .to_owned();
    let created = cli(
        cfg,
        &data,
        &[
            "task",
            "create",
            "--session",
            &session,
            "--workspace",
            &root,
            &wc.goal,
        ],
    )?;
    let tid = word_after(&created, "task").ok_or("no task id")?.to_owned();
    let turns = cfg.run.max_turns.to_string();
    cli(
        cfg,
        &data,
        &[
            "task",
            "run",
            "--session",
            &session,
            "--task",
            &tid,
            "--model",
            &cfg.run.model,
            "--max-turns",
            &turns,
            "--wait",
        ],
    )?;
    let status = cli(cfg, &data, &["task", "status", "--task", &tid])?;
    let state = field(&status, "state").unwrap_or("unknown").to_owned();
    let events = cli(
        cfg,
        &data,
        &[
            "events",
            "tail",
            "--session",
            &session,
            "--count",
            "100000",
            "--json",
        ],
    )?;
    Ok((tree_diff(&pristine, &repo), state, events))
}

/// Run the live candidate-producing mode.
///
/// # Errors
/// A run that could not be made, or a case the harness refused.
pub async fn run_live(
    cfg: &LiveConfig,
    repo_root: &Path,
    corpora: &Path,
    work: &Path,
    corpus: &WidenedCorpus,
    languages: &[String],
) -> Result<LiveReport, String> {
    let policy = AssurancePolicy::default();
    let adv = adversarial_config(corpus);
    let mut seen = std::collections::BTreeSet::new();
    let mut rows = Vec::new();
    for wc in &corpus.cases {
        if !languages.is_empty() && !languages.contains(&wc.language) {
            continue;
        }
        if !seen.insert(wc.case.lineage.clone()) {
            continue;
        }
        for repeat in 0..cfg.repeats {
            let id = format!("live-{}-{repeat}", wc.case.lineage.replace('/', "-"));
            let dir = work.join(&id);
            let (candidate, state, events) = produce(cfg, repo_root, wc, &dir.join("produce"))?;
            let mut leaks = find_leaks(
                &[("goal".into(), wc.goal.clone()), ("events".into(), events)],
                &adv.needles,
            );
            let case = AcceptanceCase {
                id: id.clone(),
                candidate,
                // The candidate's label is the oracle's, not ours.
                declared: String::new(),
                write_set: vec![],
                ..wc.case.clone()
            };
            let changed: Vec<String> = case
                .candidate
                .iter()
                .map(|op| match op {
                    FileOp::Write { path, .. } | FileOp::Delete { path } => path.clone(),
                })
                .collect();
            // The plan's write set is the files the model touched: a live
            // candidate has no separate plan.
            let case = AcceptanceCase {
                write_set: changed.clone(),
                ..case
            };
            let run = run_case(
                &case,
                repo_root,
                &corpus_oracles(corpora),
                &dir,
                &policy,
                Some(&adv),
            )
            .await
            .map_err(|r| format!("{r:?}"))?;
            if let Some(w) = &run.widened {
                leaks.extend(w.leaks.iter().cloned());
            }
            let wc_run = WidenedCase {
                case: case.clone(),
                language: wc.language.clone(),
                class: "live".into(),
                flavor: repeat.to_string(),
                goal: wc.goal.clone(),
            };
            let row = crate::widened::pair_row(&wc_run, &wc.language, &run)
                .map_err(|r| format!("{r:?}"))?;
            rows.push(LiveRow {
                row,
                task_state: state,
                changed,
                leaks,
            });
        }
    }
    Ok(LiveReport {
        model: cfg.run.model.clone(),
        repeats: cfg.repeats,
        rows,
        live: true,
    })
}

fn corpus_oracles(corpora: &Path) -> PathBuf {
    corpora.join("widened")
}
