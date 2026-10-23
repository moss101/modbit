//! The goal-seeded pre-turn Context Pack (REQ-PX-108, ADC-B02, docs/18).
//!
//! The retrieval planner used to run only when the model called a context
//! tool, so the first model turn saw no code evidence at all. This step adds
//! a second *caller* of the same planner and the same pack compiler — there
//! is still one retrieval entry (`IndexPort`, the port every context tool
//! uses): before the first model turn of a task, after a steering input that
//! changes the goal and after a compaction epoch, the Core seeds the planner
//! with the goal text (and, as required paths, the files the goal names and
//! the user's selection), compiles a bounded pack, and leaves it in the
//! task's Context Ledger, where the prompt compiler already injects the
//! ledger's latest pack, with its provenance, in the volatile tail — and
//! drops or re-hydrates a fragment whose file has changed since (FIX-12).
//!
//! What the step may not do:
//!
//! * **Block the task.** It runs under a deadline. A failure — a busy or
//!   unbuildable index, a panic, a timeout — is a typed `DEGRADED` record on
//!   the log and the run goes on without a pack; the model can still ask for
//!   one with `context.pack`. A task whose goal matches nothing records an
//!   `EMPTY` pack with the reason, never padding.
//! * **Replace model-initiated retrieval.** The context tools stay projected
//!   and unchanged; this only adds what the first turn starts with.
//! * **Exceed its budget.** The pack uses the pack compiler's token budget,
//!   derived from the routed model's context window (a small share of it,
//!   bounded), and enters the prompt as a fixed-size fragment set however many
//!   turns follow.
//!
//! Every outcome is a `ContextPackRecorded` event on the task with the
//! trigger, the status, the budget and the digest of what the planner was
//! seeded with; the ledger rebuilds from it after a Core restart, so the pack
//! a restarted run injects is the one the log records.

use std::sync::Arc;

use modbit_domain::event::{Actor, AggregateType};
use modbit_domain::task::{Task, TaskEvent};
use sha2::Digest;

use crate::runtime::{Lineage, append, typed};
use crate::server::Core;

/// The most characters of the goal the planner is seeded with.
const SEED_CHARS: usize = 1_200;
/// The most files a goal may name as required paths.
const NAMED_PATH_CAP: usize = 8;
/// Hits the planner may return for the pack.
const MAX_HITS: usize = 24;
/// The pack budget when the model's window is unknown.
const DEFAULT_BUDGET: u32 = 4_000;
/// The pack's share of the model window, as a divisor, and its bounds.
const WINDOW_DIVISOR: u32 = 40;
const MIN_BUDGET: u32 = 1_500;
const MAX_BUDGET: u32 = 6_000;
/// How long the loop waits for the pack before it goes on without one.
const DEFAULT_DEADLINE_MS: u64 = 15_000;

/// The pre-turn pack's token budget for a model with `window` tokens of
/// context (`None` = the catalog does not know it).
#[must_use]
pub(crate) fn pack_budget(window: Option<u32>) -> u32 {
    if let Some(v) = std::env::var("MODBIT_PRETURN_PACK_TOKENS")
        .ok()
        .and_then(|v| v.parse::<u32>().ok())
        .filter(|v| *v > 0)
    {
        return v;
    }
    window.filter(|w| *w > 0).map_or(DEFAULT_BUDGET, |w| {
        (w / WINDOW_DIVISOR).clamp(MIN_BUDGET, MAX_BUDGET)
    })
}

/// Whether the step is switched off for this Core (a deployment that wants
/// the model to ask for its own context).
fn disabled() -> bool {
    std::env::var("MODBIT_PRETURN_PACK").is_ok_and(|v| matches!(v.as_str(), "off" | "0" | "false"))
}

fn deadline() -> std::time::Duration {
    std::time::Duration::from_millis(
        std::env::var("MODBIT_PRETURN_PACK_DEADLINE_MS")
            .ok()
            .and_then(|v| v.parse().ok())
            .filter(|v| *v > 0)
            .unwrap_or(DEFAULT_DEADLINE_MS),
    )
}

/// Files a text names that exist in the workspace: `src/lib.rs`,
/// `lib.rs:42`, `` `Cargo.toml` ``. A name that climbs out of the root or is
/// not a file there is not a path.
#[must_use]
pub(crate) fn named_paths(root: &std::path::Path, text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for raw in text.split(|c: char| {
        c.is_whitespace() || matches!(c, '"' | '\'' | '`' | '(' | ')' | '<' | '>' | ',' | ';')
    }) {
        let mut token = raw.trim_matches(|c: char| matches!(c, '.' | ':' | '!' | '?'));
        // `path:12` and `path:12:3`.
        while let Some((head, tail)) = token.rsplit_once(':') {
            if !tail.is_empty() && tail.chars().all(|c| c.is_ascii_digit()) {
                token = head;
            } else {
                break;
            }
        }
        let token = token.trim_start_matches("./");
        let looks_like_file = token.contains('/')
            || token.rsplit_once('.').is_some_and(|(stem, ext)| {
                !stem.is_empty()
                    && (1..=6).contains(&ext.len())
                    && ext.chars().all(char::is_alphanumeric)
            });
        if token.is_empty() || !looks_like_file || out.iter().any(|p| p == token) {
            continue;
        }
        let rel = std::path::Path::new(token);
        if rel
            .components()
            .any(|c| !matches!(c, std::path::Component::Normal(_)))
        {
            continue;
        }
        if root.join(rel).is_file() {
            out.push(token.to_owned());
            if out.len() >= NAMED_PATH_CAP {
                break;
            }
        }
    }
    out
}

/// What the planner is seeded with, and its digest.
struct Seed {
    query: String,
    required: Vec<String>,
    digest: String,
}

fn seed_of(root: &std::path::Path, text: &str) -> Seed {
    let query: String = text.chars().take(SEED_CHARS).collect();
    let required = named_paths(root, &query);
    let digest = hex::encode(sha2::Sha256::digest(
        format!("{query}\n{}", required.join("\n")).as_bytes(),
    ));
    Seed {
        query,
        required,
        digest,
    }
}

/// The pre-turn step's state for one run.
pub(crate) struct PreTurn {
    /// A `TASK_START` record is already on the log (this run is a resume, or
    /// the step ran): the first-turn pack is never made twice.
    seeded: bool,
    /// The text of a steering input that replaced the goal, until it is used.
    goal_change: Option<String>,
    /// A compaction epoch opened, until the pack is refreshed.
    compaction: bool,
}

impl PreTurn {
    /// Read from the log whether the task's first-turn pack exists.
    pub(crate) async fn load(core: &Core, task: &Task) -> Self {
        let store = core.store.lock().await;
        let mut seeded = false;
        let mut after = 0u64;
        'scan: loop {
            let Ok(events) = store.read_aggregate(task.task_id.as_bytes(), after, 5_000) else {
                break;
            };
            for e in &events {
                after = e.envelope.sequence;
                if e.envelope.event_type == "ContextPackRecorded"
                    && store
                        .payload(&e.envelope)
                        .is_ok_and(|p| p["trigger"] == "TASK_START")
                {
                    seeded = true;
                    break 'scan;
                }
            }
            if events.len() < 5_000 {
                break;
            }
        }
        Self {
            seeded,
            goal_change: None,
            compaction: false,
        }
    }

    /// A steering input replaced the goal: seed again from it.
    pub(crate) fn goal_changed(&mut self, text: String) {
        self.goal_change = Some(text);
    }

    /// A compaction epoch opened: the pack is refreshed against the
    /// transcript that is left.
    pub(crate) fn compaction_opened(&mut self) {
        self.compaction = true;
    }

    /// Run the step if a trigger is pending. `window` is the routed model's
    /// context window, when the catalog has it.
    pub(crate) async fn step(
        &mut self,
        core: &Arc<Core>,
        task: &Task,
        lt: Lineage,
        actor: &Actor,
        window: Option<u32>,
    ) {
        let trigger = if !self.seeded {
            "TASK_START"
        } else if self.goal_change.is_some() {
            "GOAL_CHANGE"
        } else if self.compaction {
            "COMPACTION"
        } else {
            return;
        };
        let steer = self.goal_change.take();
        self.seeded = true;
        self.compaction = false;
        if disabled() || task.workspace_root.is_none() {
            return;
        }
        let text = match steer {
            Some(s) => format!("{}\n{s}", task.goal_text),
            None => task.goal_text.clone(),
        };
        let budget = pack_budget(window);
        let job = tokio::spawn(seed_pack(
            Arc::clone(core),
            task.clone(),
            lt,
            actor.clone(),
            trigger,
            text,
            budget,
        ));
        // The run waits for the pack up to the deadline and no longer: the
        // job goes on and records its pack when it finishes, and the next
        // turn's prompt carries it.
        let reason = match tokio::time::timeout(deadline(), job).await {
            Ok(Ok(())) => return,
            Ok(Err(e)) => format!("WORKER_FAILED: {e}"),
            Err(_) => "TIMEOUT: the pack was not ready before the run's deadline; the run goes on and the pack joins a later turn".to_owned(),
        };
        record(
            core,
            task,
            lt,
            actor,
            Recorded {
                trigger,
                status: "DEGRADED",
                reason,
                budget,
                seed_digest: String::new(),
                pack: None,
            },
        )
        .await;
    }
}

/// What one record says.
struct Recorded {
    trigger: &'static str,
    status: &'static str,
    reason: String,
    budget: u32,
    seed_digest: String,
    /// `(pack_id, pack_ref, ledger_ref, revision, token_used, entries, stubs)`.
    pack: Option<(String, String, String, u64, u32, u32, u32)>,
}

async fn record(core: &Arc<Core>, task: &Task, lt: Lineage, actor: &Actor, r: Recorded) {
    let (pack_id, pack_ref, ledger_ref, revision, used, entries, stubs) =
        r.pack.unwrap_or_default();
    let mut store = core.store.lock().await;
    let _ = append(
        &mut store,
        core,
        lt,
        AggregateType::Task,
        *task.task_id.as_bytes(),
        vec![typed(
            "ContextPackRecorded",
            &TaskEvent::ContextPackRecorded {
                pack_id,
                pack_ref,
                ledger_ref,
                workspace_revision: revision,
                tool_call_id: format!("preturn:{}", r.trigger.to_ascii_lowercase()),
                token_used: used,
                entries,
                stubs,
                trigger: r.trigger.to_owned(),
                status: r.status.to_owned(),
                reason: r.reason,
                token_budget: r.budget,
                seed_digest: r.seed_digest,
            },
            actor.clone(),
        )],
    );
}

/// Compile the pack through the one retrieval entry and record the outcome.
/// Never panics out: whatever goes wrong is a typed `DEGRADED` record.
async fn seed_pack(
    core: Arc<Core>,
    task: Task,
    lt: Lineage,
    actor: Actor,
    trigger: &'static str,
    text: String,
    budget: u32,
) {
    let Some(root) = task.workspace_root.clone() else {
        return;
    };
    let seed = seed_of(std::path::Path::new(&root), &text);
    let degrade = |code: &str, detail: String| Recorded {
        trigger,
        status: "DEGRADED",
        reason: format!("{code}: {detail}"),
        budget,
        seed_digest: seed.digest.clone(),
        pack: None,
    };
    let outcome: Result<serde_json::Value, (String, String)> = async {
        let (ws, canonical) = core
            .tools
            .workspace(&root)
            .await
            .map_err(|e| ("NO_WORKSPACE".to_owned(), e.to_string()))?;
        // REQ-PX-116: a child reads only what its lease covers; so does its
        // pack.
        let lease = core
            .store
            .lock()
            .await
            .leases_for_task(&task.task_id)
            .ok()
            .and_then(|l| l.into_iter().next());
        let port = core
            .tools
            .index_port(
                &core.store,
                core.tenant_id,
                task.session_id,
                task.task_id,
                &ws,
                &canonical,
                lease.as_ref(),
                task.workspace_root.as_deref(),
            )
            .await
            .map_err(|e| ("INDEX_UNAVAILABLE".to_owned(), e.to_string()))?;
        let request = modbit_tools::SearchRequest {
            kind: "pack".into(),
            query: serde_json::json!({
                "query": seed.query,
                "token_budget": budget,
                "required_paths": seed.required,
                "corroborated": true,
            })
            .to_string(),
            case_insensitive: false,
            path_glob: None,
            max_hits: MAX_HITS,
            use_index: true,
        };
        tokio::task::spawn_blocking(move || port.search(&request))
            .await
            .map_err(|e| ("WORKER_FAILED".to_owned(), e.to_string()))?
    }
    .await;
    let recorded = match outcome {
        Err((code, detail)) => degrade(&code, detail),
        Ok(v) => {
            let count = |k: &str| {
                u32::try_from(v["pack"][k].as_array().map_or(0, Vec::len)).unwrap_or(u32::MAX)
            };
            let (entries, stubs) = (count("entries"), count("stubs"));
            let used =
                u32::try_from(v["pack"]["token_used"].as_u64().unwrap_or(0)).unwrap_or(u32::MAX);
            let s = |k: &str| v[k].as_str().unwrap_or_default().to_owned();
            if s("pack_ref").is_empty() || s("ledger_ref").is_empty() {
                degrade("STORE", "the pack could not be stored".into())
            } else {
                Recorded {
                    trigger,
                    status: if entries == 0 && stubs == 0 {
                        "EMPTY"
                    } else {
                        "PACKED"
                    },
                    reason: if entries == 0 && stubs == 0 {
                        "NO_MATCHES: nothing in the workspace matched the goal".into()
                    } else {
                        String::new()
                    },
                    budget,
                    seed_digest: seed.digest.clone(),
                    pack: Some((
                        v["pack"]["pack_id"].as_str().unwrap_or_default().to_owned(),
                        s("pack_ref"),
                        s("ledger_ref"),
                        v["pack"]["workspace_revision"].as_u64().unwrap_or(0),
                        used,
                        entries,
                        stubs,
                    )),
                }
            }
        }
    };
    record(&core, &task, lt, &actor, recorded).await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_budget_follows_the_window_inside_its_bounds() {
        assert_eq!(pack_budget(None), DEFAULT_BUDGET);
        assert_eq!(pack_budget(Some(0)), DEFAULT_BUDGET);
        assert_eq!(pack_budget(Some(32_000)), MIN_BUDGET);
        assert_eq!(pack_budget(Some(200_000)), 5_000);
        assert_eq!(pack_budget(Some(2_000_000)), MAX_BUDGET);
    }

    #[test]
    fn a_goal_names_only_files_that_exist_inside_the_workspace() {
        let d = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(d.path().join("src")).unwrap();
        std::fs::write(d.path().join("src/cart.rs"), "x").unwrap();
        std::fs::write(d.path().join("README.md"), "x").unwrap();
        let named = named_paths(
            d.path(),
            "Fix the total in `src/cart.rs:42:7`, see README.md. Also ../outside.rs and /etc/passwd and src/missing.rs, v1.2.",
        );
        assert_eq!(named, ["src/cart.rs", "README.md"]);
    }

    #[test]
    fn the_seed_digest_names_the_text_and_the_required_paths() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("a.rs"), "x").unwrap();
        let one = seed_of(d.path(), "fix a.rs");
        let two = seed_of(d.path(), "fix a.rs please");
        assert_ne!(one.digest, two.digest);
        assert_eq!(one.required, ["a.rs"]);
        assert_eq!(one.digest, seed_of(d.path(), "fix a.rs").digest);
    }
}
