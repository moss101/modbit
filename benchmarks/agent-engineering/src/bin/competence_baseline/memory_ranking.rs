//! `competence-baseline memory-ranking` (PX-186): the memory axis's ranking
//! comparison through the real product.
//!
//! ```text
//! competence-baseline memory-ranking --corpus <corpus.json> --fixtures <dir> --out <dir> \
//!   --endpoint <openai|anthropic> --model <id> [--current keyword-v1] \
//!   --candidate <version> [--cli <bin>] [--core <bin>] [--keep]
//! ```
//!
//! Five passes, one after another, each a fresh Core profile on a fresh copy
//! of the fixture at one path: the Core's default (no ranking named), the
//! current ranking, the candidate, and each of those again. The ranking is
//! the one a person would choose, `MODBIT_MEMORY_RANKING` in the Core's
//! environment; nothing else differs. A candidate the Core does not have
//! falls back to its default, the records say so, and the comparison is
//! refused. In each pass the corpus is seeded through the CLI
//! (`memory propose|promote`, oldest first; the items marked
//! `before_revision_move` are seeded, then the repository commits, then the
//! rest), and every task of the corpus is created and run for one turn. What
//! reached the first prompt is read from the Core's records only: the
//! ContextCompile step's memory record and the `MemoryUsed` events.
//! Every workspace path has the same length on every machine (the headers
//! of repository-scoped items carry it, so its length is part of each
//! item's token cost).
//!
//! Exit 0 when the comparison is written (whatever it decides); 1 when it is
//! refused or a pass could not run.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use modbit_bench_agent_engineering::ranking::{
    Corpus, PassObservation, Passes, compare_rankings, observe, render,
};
use modbit_bench_agent_engineering::{parse_events, sha256_hex};

use super::{Cli, commit_all, first_word_after, git, stop_profile_core};

/// The length of every pass's canonical workspace path, in bytes.
pub const WORKSPACE_PATH_BYTES: usize = 120;

struct Args {
    corpus: PathBuf,
    fixtures: PathBuf,
    out: PathBuf,
    endpoint: String,
    model: String,
    current: String,
    candidate: String,
    cli: PathBuf,
    core: PathBuf,
    keep: bool,
}

fn usage() -> ! {
    eprintln!(
        "usage: competence-baseline memory-ranking --corpus <corpus.json> --fixtures <dir> --out <dir> --endpoint <openai|anthropic> --model <id> [--current <version>] --candidate <version> [--cli <bin>] [--core <bin>] [--keep]"
    );
    std::process::exit(1)
}

fn parse() -> Args {
    let argv: Vec<String> = std::env::args().skip(2).collect();
    let sibling = |name: &str| -> PathBuf {
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(Path::to_path_buf))
            .map(|d| d.join(format!("{name}{}", std::env::consts::EXE_SUFFIX)))
            .unwrap_or_else(|| PathBuf::from(name))
    };
    let mut a = Args {
        corpus: PathBuf::new(),
        fixtures: PathBuf::new(),
        out: PathBuf::new(),
        endpoint: String::new(),
        model: String::new(),
        current: "keyword-v1".into(),
        candidate: String::new(),
        cli: sibling("modbit-cli"),
        core: sibling("modbit-core"),
        keep: false,
    };
    let mut i = 0;
    let value = |i: &mut usize| -> String {
        *i += 1;
        argv.get(*i).cloned().unwrap_or_else(|| usage())
    };
    while i < argv.len() {
        match argv[i].as_str() {
            "--corpus" => a.corpus = PathBuf::from(value(&mut i)),
            "--fixtures" => a.fixtures = PathBuf::from(value(&mut i)),
            "--out" => a.out = PathBuf::from(value(&mut i)),
            "--endpoint" => a.endpoint = value(&mut i),
            "--model" => a.model = value(&mut i),
            "--current" => a.current = value(&mut i),
            "--candidate" => a.candidate = value(&mut i),
            "--cli" => a.cli = PathBuf::from(value(&mut i)),
            "--core" => a.core = PathBuf::from(value(&mut i)),
            "--keep" => a.keep = true,
            _ => usage(),
        }
        i += 1;
    }
    if a.corpus.as_os_str().is_empty()
        || a.fixtures.as_os_str().is_empty()
        || a.out.as_os_str().is_empty()
        || a.endpoint.is_empty()
        || a.model.is_empty()
        || a.candidate.is_empty()
    {
        usage();
    }
    a
}

/// A scratch directory whose `w` child has a canonical path of exactly
/// [`WORKSPACE_PATH_BYTES`] bytes.
fn fixed_length_scratch(label: &str) -> Result<(PathBuf, PathBuf), String> {
    let tmp = std::env::temp_dir()
        .canonicalize()
        .map_err(|e| format!("temp dir: {e}"))?;
    let base = tmp.to_string_lossy().trim_start_matches(r"\\?\").to_owned();
    let sep = std::path::MAIN_SEPARATOR;
    let stem = format!("modbit-rank-{}-{label}-", std::process::id());
    // base + sep + stem + pad + sep + "w"
    let fixed = base.len() + 1 + stem.len() + 1 + 1;
    let pad = WORKSPACE_PATH_BYTES.checked_sub(fixed).ok_or_else(|| {
        format!("the temp dir {base} is too long for a {WORKSPACE_PATH_BYTES}-byte workspace path")
    })?;
    let scratch = PathBuf::from(format!("{base}{sep}{stem}{}", "p".repeat(pad)));
    let workspace = scratch.join("w");
    Ok((scratch, workspace))
}

#[allow(clippy::too_many_lines)]
fn pass(a: &Args, corpus: &Corpus, ranking: &str, label: &str) -> Result<PassObservation, String> {
    // Every pass works at the same path: a repository item's id is
    // content-addressed over its scope, which names the workspace, so the
    // same corpus seeded at the same path is the same store, and a ranking
    // that breaks ties by id breaks them alike in every pass.
    let (scratch, workspace) = fixed_length_scratch("ws")?;
    // The previous pass's workspace goes first (a Core that has not yet
    // exited may still hold it; it idle-exits within seconds). Each pass
    // has a profile of its own, so no pass can reach another's Core.
    let started = std::time::Instant::now();
    while workspace.exists() {
        let _ = std::fs::remove_dir_all(&workspace);
        if started.elapsed() > Duration::from_secs(90) {
            return Err(format!("{}: could not be removed", workspace.display()));
        }
        if workspace.exists() {
            std::thread::sleep(Duration::from_millis(500));
        }
    }
    let data_dir = scratch.join(format!("profile-{label}"));
    if data_dir.exists() {
        return Err(format!(
            "{}: a profile is already there",
            data_dir.display()
        ));
    }
    std::fs::create_dir_all(&data_dir).map_err(|e| e.to_string())?;
    modbit_bench_agent_engineering::copy_fixture(&a.fixtures.join(&corpus.fixture), &workspace)?;
    git(&workspace, &["init", "-q", "-b", "main"])?;
    git(&workspace, &["config", "core.autocrlf", "false"])?;
    commit_all(&workspace, "fixture")?;
    let workspace_abs = workspace
        .canonicalize()
        .map_err(|e| e.to_string())?
        .to_string_lossy()
        .trim_start_matches(r"\\?\")
        .to_owned();
    if workspace_abs.len() != WORKSPACE_PATH_BYTES {
        return Err(format!(
            "workspace path {workspace_abs} is {} bytes, not {WORKSPACE_PATH_BYTES}",
            workspace_abs.len()
        ));
    }
    let pass_dir = a.out.join("passes").join(label);
    std::fs::create_dir_all(&pass_dir).map_err(|e| e.to_string())?;
    let mut cli = Cli {
        bin: &a.cli,
        data_dir: data_dir.clone(),
        env: vec![
            (
                "MODBIT_CORE_BIN".into(),
                a.core.to_string_lossy().into_owned(),
            ),
            ("MODBIT_MEMORY_RANKING".into(), ranking.to_owned()),
            // The pass's Core exits soon after its last client.
            ("MODBIT_CORE_IDLE_EXIT_SECS".into(), "3".into()),
        ],
        log: std::fs::File::create(pass_dir.join("cli.log")).map_err(|e| e.to_string())?,
    };
    let short = Duration::from_secs(120);
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
            "Record the team's memory.",
        ],
        short,
    );
    let seeder = first_word_after(&r.stdout, "task ")
        .ok_or_else(|| format!("task create: {}{}", r.stdout, r.stderr))?;
    let mut ids: BTreeMap<String, String> = BTreeMap::new();
    let ordered = corpus
        .items
        .iter()
        .filter(|i| i.before_revision_move)
        .chain(corpus.items.iter().filter(|i| !i.before_revision_move));
    let mut moved = false;
    for it in ordered {
        if !it.before_revision_move && !moved {
            // The repository moves on: what was validated before is stale.
            std::fs::write(workspace.join("REVISION"), "2\n").map_err(|e| e.to_string())?;
            commit_all(&workspace, "the repository moves on")?;
            moved = true;
        }
        let confidence = it.confidence.map(|c| format!("{c}"));
        let mut argv: Vec<&str> = vec![
            "memory",
            "propose",
            "--session",
            &session,
            "--task",
            &seeder,
            "--scope",
            &it.scope,
            "--type",
            &it.record_type,
            "--topic",
            &it.topic,
        ];
        if let Some(c) = &confidence {
            argv.extend(["--confidence", c.as_str()]);
        }
        if it.sensitive {
            argv.push("--sensitive");
        }
        argv.push(&it.content);
        let r = cli.call(&argv, short);
        let id = first_word_after(&r.stdout, "proposed ")
            .ok_or_else(|| format!("memory propose {}: {}{}", it.key, r.stdout, r.stderr))?;
        let r = cli.call(
            &[
                "memory",
                "promote",
                "--session",
                &session,
                "--task",
                &seeder,
                &id,
            ],
            short,
        );
        if r.code != 0 {
            return Err(format!(
                "memory promote {}: {}{}",
                it.key, r.stdout, r.stderr
            ));
        }
        // A curated item's id is its proposal's (content-addressed).
        ids.insert(id, it.key.clone());
    }
    let mut tasks: Vec<(String, String)> = Vec::new();
    for q in &corpus.queries {
        let r = cli.call(
            &[
                "task",
                "create",
                "--session",
                &session,
                "--workspace",
                &workspace_abs,
                &q.goal,
            ],
            short,
        );
        let task = first_word_after(&r.stdout, "task ")
            .ok_or_else(|| format!("task create {}: {}{}", q.id, r.stdout, r.stderr))?;
        // One turn: the first prompt is what the selection is measured on.
        let r = cli.call(
            &[
                "task",
                "run",
                "--session",
                &session,
                "--task",
                &task,
                "--endpoint",
                &a.endpoint,
                "--model",
                &a.model,
                "--max-turns",
                "1",
                "--wait",
            ],
            Duration::from_secs(300),
        );
        if r.timed_out {
            return Err(format!("task run {} timed out", q.id));
        }
        tasks.push((q.id.clone(), task));
    }
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
    let events = parse_events(&ev.stdout);
    std::fs::write(
        pass_dir.join("events.sha256"),
        sha256_hex(ev.stdout.as_bytes()),
    )
    .map_err(|e| e.to_string())?;
    let objects = data_dir.join("core/objects");
    let read_object = |hash: &str| -> Option<Vec<u8>> {
        if hash.len() < 3 || !hash.chars().all(|c| c.is_ascii_hexdigit()) {
            return None;
        }
        std::fs::read(objects.join(&hash[..2]).join(&hash[2..])).ok()
    };
    let mut out = PassObservation {
        ranking: ranking.to_owned(),
        queries: BTreeMap::new(),
    };
    for (qid, task) in &tasks {
        if let Some(o) = observe(&events, task, &read_object, &ids) {
            out.queries.insert(qid.clone(), o);
        }
    }
    std::fs::write(
        pass_dir.join("pass.json"),
        serde_json::to_vec_pretty(&out).unwrap_or_default(),
    )
    .map_err(|e| e.to_string())?;
    stop_profile_core(&data_dir);
    Ok(out)
}

/// The subcommand.
pub fn main() -> i32 {
    let a = parse();
    let (corpus, digest) = match Corpus::load(&a.corpus) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("memory-ranking: {e}");
            return 1;
        }
    };
    if let Err(e) = std::fs::create_dir_all(&a.out) {
        eprintln!("memory-ranking: {}: {e}", a.out.display());
        return 1;
    }
    // Five independent profiles, one after another at one path: the Core's
    // default (no ranking named), then current, candidate, and each again.
    let plan = [
        ("", "p0"),
        (a.current.as_str(), "p1"),
        (a.candidate.as_str(), "p2"),
        (a.current.as_str(), "p3"),
        (a.candidate.as_str(), "p4"),
    ];
    let mut observed = Vec::new();
    for (ranking, label) in plan {
        let r = pass(&a, &corpus, ranking, label);
        match r {
            Ok(p) => observed.push(p),
            Err(e) => {
                eprintln!("memory-ranking: pass {label} ({ranking:?}): {e}");
                return 1;
            }
        }
    }
    if !a.keep
        && let Ok((scratch, _)) = fixed_length_scratch("ws")
    {
        let _ = std::fs::remove_dir_all(scratch);
    }
    let Ok::<[PassObservation; 5], _>(
        [default, current, candidate, current_again, candidate_again],
    ) = observed.try_into()
    else {
        eprintln!("memory-ranking: a pass is missing");
        return 1;
    };
    let passes = Passes {
        default,
        current: [current, current_again],
        candidate: [candidate, candidate_again],
    };
    let _ = std::fs::write(
        a.out.join("passes.json"),
        serde_json::to_vec_pretty(&passes).unwrap_or_default(),
    );
    match compare_rankings(&corpus, &digest, &passes) {
        Ok(mut c) => {
            c.environment = environment(&a);
            let bytes = serde_json::to_vec_pretty(&c).unwrap_or_default();
            let summary = render(&c);
            if std::fs::write(a.out.join("comparison.json"), &bytes).is_err()
                || std::fs::write(a.out.join("summary.md"), &summary).is_err()
            {
                eprintln!("memory-ranking: could not write {}", a.out.display());
                return 1;
            }
            println!("{summary}");
            0
        }
        Err(e) => {
            let _ = std::fs::write(
                a.out.join("refused.json"),
                serde_json::to_vec_pretty(&serde_json::json!({
                    "refused": e.to_string(),
                    "environment": environment(&a),
                }))
                .unwrap_or_default(),
            );
            eprintln!("memory-ranking: refused: {e}");
            1
        }
    }
}

/// Where the comparison ran: the revision, the digest of the Core and CLI
/// that ran, the OS.
fn environment(a: &Args) -> serde_json::Value {
    let digest = |p: &Path| sha256_hex(&std::fs::read(p).unwrap_or_default());
    serde_json::json!({
        "modbit_revision": super::cmd_output("git", &["rev-parse", "HEAD"]),
        "build_digest": sha256_hex(
            &[digest(&a.core).into_bytes(), digest(&a.cli).into_bytes()].concat()
        ),
        "os": format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH),
        "workspace_path_bytes": WORKSPACE_PATH_BYTES,
    })
}
