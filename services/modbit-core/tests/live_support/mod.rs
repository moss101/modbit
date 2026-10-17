//! The harness of the live-gateway legs of the Core qualifications
//! (`live_core.rs`, `.github/workflows/live-core.yml`).
//!
//! A leg drives the REAL Core (the `modbit-core` binary over its real socket,
//! real SQLite log, real git repository) with the REAL provider adapter
//! pointed at the gateway the repository variables name (DR-M9-002). This
//! module is what is common to every leg:
//!
//! * [`LiveEnv::from_vars`] refuses unless the run is explicitly live
//!   (`MODBIT_LIVE=1`), the credential, the model and its priced catalog entry
//!   are present, and a spend cap is set. A leg that cannot get them FAILS; it
//!   never skips silently as a pass.
//! * [`Leg`] is one leg's result record (run id, model, endpoint, counts,
//!   cost, every named check). It is written when the leg ends, whether it
//!   passed, failed an assertion or panicked, so a failure is retained and
//!   reported, never lost.
//! * The spend cap is the one of the `live-evals` workflows: a
//!   [`SpendMeter`] charged from the usage the gateway returned, checked
//!   before each leg and between its phases; a leg the cap would not cover is
//!   refused, and a leg that was stopped by it does not pass.
//!
//! Nothing here is a model: the offline tests of this module and of the legs
//! point the same code at a scripted OpenAI-compatible server, which proves
//! the mechanics and says nothing about a model.
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::Instant;

use modbit_bench_context_economics::live::live_config;
use modbit_bench_context_economics::spend::{Price, SpendMeter, price_from_catalog};
use serde_json::{Value, json};

/// What a live run is configured with, validated.
pub struct LiveEnv {
    /// The model id the legs start tasks on.
    pub model: String,
    /// The gateway's base URL (the endpoint recorded with every result).
    pub endpoint: String,
    /// The catalog spec the Core is given (`MODBIT_OPENAI_MODELS`).
    pub catalog: String,
    /// The model's list price.
    pub price: Price,
    /// The model's context window in the catalog (`ctx=`), when it names one.
    pub window: Option<u64>,
    /// The environment the Core process is spawned with.
    pub core_env: Vec<(String, String)>,
    /// Where result records are written.
    pub results_dir: PathBuf,
    /// The workflow run (or a local stamp).
    pub run_id: String,
    /// The spend account of this process.
    pub meter: SpendMeter,
    /// False for the offline tests (a scripted stand-in, not a gateway).
    pub live: bool,
}

/// `ctx=NNN` of one model's catalog entry.
#[must_use]
pub fn catalog_window(spec: &str, model: &str) -> Option<u64> {
    for entry in spec.split(',').map(str::trim) {
        let mut parts = entry.split(';');
        let head = parts.next().unwrap_or_default();
        if head.split_once('=').map(|(n, _)| n.trim()) != Some(model) {
            continue;
        }
        // The catalog's own default when the entry names none.
        return Some(parts.find_map(|p| p.trim().strip_prefix("ctx=")?.parse().ok()).unwrap_or(128_000));
    }
    None
}

impl LiveEnv {
    /// Validate the environment. `var` reads one variable (the process
    /// environment in production, a map in tests).
    ///
    /// # Errors
    /// The first reason the run is refused, in words.
    pub fn from_vars(var: &dyn Fn(&str) -> Option<String>) -> Result<Self, String> {
        let cfg = live_config(var).map_err(|e| e.to_string())?;
        let cap = cfg.max_cost_usd.ok_or_else(|| {
            "LIVE: NOT RUN (MODBIT_LIVE_MAX_COST_USD is not set: a live leg spends money and runs only under a hard cap)".to_owned()
        })?;
        let catalog = var("MODBIT_OPENAI_MODELS")
            .map(|v| v.trim().to_owned())
            .filter(|v| !v.is_empty())
            .ok_or_else(|| {
                "LIVE: NOT RUN (MODBIT_OPENAI_MODELS is not set: without a priced catalog entry no cost can be charged)".to_owned()
            })?;
        let price = price_from_catalog(&catalog, &cfg.run.model)?;
        let meter = SpendMeter::new(cap)?;
        let mut core_env = cfg.run.env.clone();
        // One family only: a stray Anthropic credential in the process
        // environment must not become a second endpoint of the Core.
        core_env.push(("ANTHROPIC_API_KEY".into(), String::new()));
        let run_id = var("GITHUB_RUN_ID")
            .filter(|v| !v.trim().is_empty())
            .or_else(|| var("MODBIT_LIVE_RUN_ID").filter(|v| !v.trim().is_empty()))
            .unwrap_or_else(|| {
                format!(
                    "local-{}",
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map_or(0, |d| d.as_millis())
                )
            });
        Ok(Self {
            window: catalog_window(&catalog, &cfg.run.model),
            endpoint: var("MODBIT_LIVE_BASE_URL")
                .map(|v| v.trim().to_owned())
                .filter(|v| !v.is_empty())
                .unwrap_or_else(|| "(the provider's own endpoint)".into()),
            model: cfg.run.model.clone(),
            catalog,
            price,
            core_env,
            results_dir: var("MODBIT_LIVE_RESULTS_DIR")
                .filter(|v| !v.trim().is_empty())
                .map_or_else(|| PathBuf::from("target/live-core-results"), PathBuf::from),
            run_id,
            meter,
            live: true,
        })
    }

    /// The environment of this process, validated once. A live test calls
    /// this first; if the run is not live it PANICS, so a `--ignored` run
    /// without the credential is a failed job and never a quiet pass.
    #[must_use]
    pub fn from_process() -> Arc<Self> {
        static ENV: OnceLock<Result<Arc<LiveEnv>, String>> = OnceLock::new();
        match ENV.get_or_init(|| Self::from_vars(&|k| std::env::var(k).ok()).map(Arc::new)) {
            Ok(env) => Arc::clone(env),
            Err(why) => panic!("{why}"),
        }
    }

    /// The Core's environment as the pairs `CoreProcess::spawn_with_env` takes.
    #[must_use]
    pub fn core_env_refs(&self) -> Vec<(&str, &str)> {
        self.core_env
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect()
    }

    /// The Core's environment plus `extra` (a later pair wins in the child).
    #[must_use]
    pub fn core_env_with<'a>(&'a self, extra: &[(&'a str, &'a str)]) -> Vec<(&'a str, &'a str)> {
        let mut v = self.core_env_refs();
        v.extend_from_slice(extra);
        v
    }
}

/// One named assertion of a leg.
struct Check {
    name: String,
    ok: bool,
    detail: String,
}

/// One leg's result record. Created with [`Leg::begin`], ended with
/// [`Leg::finish`]; written on drop either way.
pub struct Leg {
    env: Arc<LiveEnv>,
    row: &'static str,
    name: &'static str,
    started: Instant,
    started_at_ms: u128,
    counts: serde_json::Map<String, Value>,
    checks: Vec<Check>,
    notes: Vec<String>,
    calls: u64,
    input_tokens: u64,
    output_tokens: u64,
    cost_usd: f64,
    stopped_by_cap: bool,
    finished: bool,
}

impl Leg {
    /// Start a leg. The spend cap must cover `reserve_usd` more.
    ///
    /// # Panics
    /// The cap would be exceeded: the leg is refused and its record says so.
    #[must_use]
    pub fn begin(
        env: &Arc<LiveEnv>,
        row: &'static str,
        name: &'static str,
        reserve_usd: f64,
    ) -> Self {
        let mut leg = Self {
            env: Arc::clone(env),
            row,
            name,
            started: Instant::now(),
            started_at_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_millis()),
            counts: serde_json::Map::new(),
            checks: Vec::new(),
            notes: Vec::new(),
            calls: 0,
            input_tokens: 0,
            output_tokens: 0,
            cost_usd: 0.0,
            stopped_by_cap: false,
            finished: false,
        };
        leg.admit(reserve_usd);
        leg
    }

    /// The environment.
    #[must_use]
    pub fn env(&self) -> &Arc<LiveEnv> {
        &self.env
    }

    /// May `reserve_usd` more be spent? Panics (and records why) if not.
    pub fn admit(&mut self, reserve_usd: f64) {
        if let Err(e) = self.env.meter.admit(reserve_usd) {
            self.stopped_by_cap = true;
            self.notes.push(e.to_string());
            panic!("{}/{}: {e}", self.row, self.name);
        }
    }

    /// A number the result reports (a count, a latency, a token total).
    pub fn count(&mut self, key: &str, value: impl Into<Value>) {
        self.counts.insert(key.to_owned(), value.into());
    }

    /// A line of context that is not an assertion (what the gateway cannot
    /// give, what the model did).
    pub fn note(&mut self, text: impl Into<String>) {
        self.notes.push(text.into());
    }

    /// Charge model usage the Core's economics view reported for a task (the
    /// gateway's own usage frames, priced at the catalog list price).
    pub fn charge(&mut self, calls: u64, input_tokens: u64, output_tokens: u64) {
        self.calls += calls;
        self.input_tokens += input_tokens;
        self.output_tokens += output_tokens;
        let usd = self.env.price.cost_usd(input_tokens, output_tokens);
        self.cost_usd += usd;
        self.env.meter.charge_usd(usd, input_tokens, output_tokens);
    }

    /// One assertion. A failed one is recorded and PANICS with its detail.
    pub fn check(&mut self, name: &str, ok: bool, detail: impl std::fmt::Display) {
        let detail = detail.to_string();
        self.checks.push(Check {
            name: name.to_owned(),
            ok,
            detail: detail.clone(),
        });
        assert!(
            ok,
            "{}/{}: check `{name}` failed: {detail}",
            self.row, self.name
        );
    }

    /// The leg ran to its end: every check was made and none failed.
    pub fn finish(&mut self) {
        self.finished = true;
    }

    /// The record as JSON.
    #[must_use]
    pub fn record(&self, passed: bool) -> Value {
        json!({
            "row": self.row,
            "leg": self.name,
            "run_id": self.env.run_id,
            "model": self.env.model,
            "endpoint": self.env.endpoint,
            "live": self.env.live,
            "started_at_ms": self.started_at_ms,
            "elapsed_s": self.started.elapsed().as_secs_f64(),
            "passed": passed,
            "stopped_by_cap": self.stopped_by_cap,
            "model_calls": self.calls,
            "input_tokens": self.input_tokens,
            "output_tokens": self.output_tokens,
            "cost_usd": self.cost_usd,
            "counts": self.counts,
            "checks": self.checks.iter().map(|c| json!({"name": c.name, "ok": c.ok, "detail": c.detail})).collect::<Vec<_>>(),
            "notes": self.notes,
        })
    }

    /// Where this leg's record is written.
    #[must_use]
    pub fn path(&self) -> PathBuf {
        self.env
            .results_dir
            .join(format!("{}-{}.json", self.row, self.name))
    }
}

impl Drop for Leg {
    fn drop(&mut self) {
        let passed = self.finished
            && !std::thread::panicking()
            && !self.stopped_by_cap
            && self.checks.iter().all(|c| c.ok);
        let mut record = self.record(passed);
        if !passed && !self.checks.iter().any(|c| !c.ok) {
            record["failure"] = json!(if self.stopped_by_cap {
                "the spend cap stopped the leg"
            } else {
                "the leg ended before it finished (a panic or an early return); see the job log"
            });
        }
        let path = self.path();
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(
            &path,
            serde_json::to_string_pretty(&record).unwrap_or_default(),
        );
    }
}

/// The words of a text (whitespace separated).
#[must_use]
pub fn word_count(text: &str) -> usize {
    text.split_whitespace().count()
}

/// The nearest-rank percentile of `values` (milliseconds etc.).
#[must_use]
pub fn percentile(values: &[u64], p: f64) -> u64 {
    if values.is_empty() {
        return 0;
    }
    let mut v = values.to_vec();
    v.sort_unstable();
    let rank = ((p / 100.0) * v.len() as f64).ceil().max(1.0) as usize;
    v[rank.min(v.len()) - 1]
}

/// A path shown relative to a base (for result records).
#[must_use]
pub fn rel(base: &Path, p: &Path) -> String {
    p.strip_prefix(base)
        .unwrap_or(p)
        .to_string_lossy()
        .replace('\\', "/")
}
