//! The model side of compaction (REQ-PX-109, ADC-B03, docs/19, docs/30).
//!
//! `modbit_compaction` is pure: it extracts an epoch from a transcript range
//! and validates a summary it is handed. This module is what a Core needs
//! around it:
//!
//! * **Thresholds** that follow the routed model. The compaction trigger used
//!   to be a constant (12k transcript tokens counted as bytes over four);
//!   it is now a share of the model's context window less the output budget
//!   and the prompt that is not transcript, with the old numbers kept as the
//!   fallback when the catalog does not know the window, and
//!   `MODBIT_COMPACTION_TOKEN_BUDGET` still overriding it for a deployment or
//!   a test.
//! * **The summarizer role.** A model named by policy — an operator's
//!   `MODBIT_COMPACTION_SUMMARIZER` (`run`, `endpoint/model`, `off`) or, when
//!   the registry binds one, its `summarizer` role — never a hard-coded one,
//!   and none at all unless policy names one. It must pass the task's
//!   model policy (`models_allow`), the registry's governance and a
//!   `compaction.model_summary` permission that is not `DENY`.
//! * **The ladder.** model summary, validated against the extractive
//!   baseline, then the extractive epoch, then — when even that cannot be
//!   installed — the old context untouched. A summarizer that is absent,
//!   refused, slow, rate limited, malformed or caught inventing something
//!   costs the run nothing but the summary: the epoch is the extractive one,
//!   with the typed reason on the log. A placeholder is never installed.
//! * **The transcript pointer.** Every epoch, model-written or not, stores
//!   the exact text of the range it replaced as an object and says where, so
//!   the model can read any earlier detail back with `artifact.range`.

use std::sync::Arc;

use modbit_compaction::summary::{self, SummaryInput};
use modbit_compaction::{
    CompactionManifest, CompactionRequest, PreservedFact, SourceEntry, attach_fallback,
    attach_narrative, attach_transcript, compact, transcript_objects,
};
use modbit_domain::task::Task;
use modbit_providers::{
    ContentPart, Message, ModelEvent, ModelPolicy, ModelRequest, Requirements, Role, Usage,
};
use tokio_util::sync::CancellationToken;

use crate::server::Core;

/// The transcript budget when the catalog does not know the model's window:
/// what the loop used before it followed the model.
const FALLBACK_HARD_TOKENS: u32 = 12_000;
/// Share of the usable window (window less the output budget) at which a
/// bounded synchronous compaction runs, and at which a worker starts.
const HARD_PERCENT: u32 = 85;
const SOFT_PERCENT: u32 = 70;
/// A transcript budget never goes below this, however much of a small window
/// the rest of the prompt takes.
const MIN_TRANSCRIPT_TOKENS: u32 = 2_000;
/// After an epoch the next one waits for the transcript to grow by this
/// share of the hard budget beyond what the epoch left (hysteresis).
const REARM_PERCENT: u32 = 15;
/// The most tokens a model narrative may take.
const NARRATIVE_CAP_TOKENS: u32 = 6_000;
/// The most time a summarizer call gets unless the model's own timeout or the
/// deployment says less.
const SUMMARIZER_TIMEOUT_MS: u64 = 60_000;

/// Where a threshold came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BudgetSource {
    /// Derived from the routed model's context window.
    ModelWindow,
    /// `MODBIT_COMPACTION_TOKEN_BUDGET`.
    EnvOverride,
    /// The old constant: the catalog does not know the window.
    Fallback,
}

impl BudgetSource {
    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::ModelWindow => "MODEL_WINDOW",
            Self::EnvOverride => "ENV_OVERRIDE",
            Self::Fallback => "FALLBACK",
        }
    }
}

/// The compaction trigger for one model.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Thresholds {
    /// The model's context window, 0 when unknown.
    pub window: u32,
    /// Transcript tokens at which a bounded synchronous compaction runs.
    pub hard: u32,
    /// Transcript tokens at which a worker starts.
    pub soft: u32,
    pub source: BudgetSource,
}

impl Thresholds {
    /// Whether this trigger follows the model and so counts tokens with the
    /// calibrated estimator and re-arms with hysteresis; the override and the
    /// fallback keep the counting they were written with.
    pub(crate) fn model_aware(&self) -> bool {
        self.source == BudgetSource::ModelWindow
    }

    /// The token estimator in force, as the Inspector names it.
    pub(crate) fn estimator(&self) -> &'static str {
        if self.model_aware() {
            "tokens-v2+calibration"
        } else {
            "bytes/4"
        }
    }

    /// The transcript size an epoch must be followed by before the next one
    /// may start: where it left the transcript plus a share of the budget.
    pub(crate) fn rearm_floor(&self, left: u32) -> u32 {
        if self.model_aware() {
            left.saturating_add(self.hard * REARM_PERCENT / 100)
        } else {
            0
        }
    }
}

/// The thresholds for `endpoint/model`. `overhead` is what the rest of the
/// prompt (system, rules, epoch, tools, volatile state) took in the last
/// request, in the same estimator units as the transcript.
pub(crate) fn thresholds(core: &Core, endpoint: &str, model: &str, overhead: u32) -> Thresholds {
    if let Some(v) = std::env::var("MODBIT_COMPACTION_TOKEN_BUDGET")
        .ok()
        .and_then(|v| v.parse::<u32>().ok())
        .filter(|v| *v > 0)
    {
        return Thresholds {
            window: core
                .gateway
                .capability(endpoint, model)
                .map_or(0, |c| c.context_tokens),
            hard: v,
            soft: v * 3 / 4,
            source: BudgetSource::EnvOverride,
        };
    }
    let Some(cap) = core
        .gateway
        .capability(endpoint, model)
        .filter(|c| c.context_tokens > 0)
    else {
        return Thresholds {
            window: 0,
            hard: FALLBACK_HARD_TOKENS,
            soft: FALLBACK_HARD_TOKENS * 3 / 4,
            source: BudgetSource::Fallback,
        };
    };
    from_window(cap.context_tokens, cap.output_budget(), overhead)
}

/// The window arithmetic, apart from the catalog.
pub(crate) fn from_window(window: u32, output_budget: u32, overhead: u32) -> Thresholds {
    // Room for the prompt: the window less what the answer may take and a
    // margin for what the estimate misses.
    let usable = window
        .saturating_sub(output_budget)
        .saturating_sub(window / 50);
    let hard_prompt = u64::from(usable) * u64::from(HARD_PERCENT) / 100;
    let hard = u32::try_from(hard_prompt)
        .unwrap_or(u32::MAX)
        .saturating_sub(overhead)
        .max(MIN_TRANSCRIPT_TOKENS);
    let soft = u32::try_from(u64::from(hard) * u64::from(SOFT_PERCENT) / u64::from(HARD_PERCENT))
        .unwrap_or(u32::MAX);
    Thresholds {
        window,
        hard,
        soft,
        source: BudgetSource::ModelWindow,
    }
}

/// The model that writes summaries, and the registry role it is called in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Summarizer {
    pub endpoint: String,
    pub model: String,
    /// `solver` when policy says "the run's own model", `summarizer` when it
    /// names one (or the registry binds one).
    pub role: &'static str,
}

impl Summarizer {
    pub(crate) fn label(&self) -> String {
        format!("{}/{}", self.endpoint, self.model)
    }
}

/// Resolve the summarizer for a task, or the typed reason there is none.
pub(crate) fn resolve(
    core: &Core,
    task: &Task,
    run_endpoint: &str,
    run_model: &str,
) -> Result<Summarizer, String> {
    let config = core.tools.configurations.for_task(
        task.task_id,
        &core.data_dir,
        task.workspace_root.as_deref(),
    );
    if config
        .permissions
        .get("compaction.model_summary")
        .is_some_and(|p| p.value == modbit_policy::config::Permission::Deny)
    {
        return Err("POLICY_DENIED".into());
    }
    let named = std::env::var("MODBIT_COMPACTION_SUMMARIZER").unwrap_or_default();
    let named = named.trim();
    let chosen = match named {
        "off" => return Err("SUMMARIZER_OFF".into()),
        "run" => Summarizer {
            endpoint: run_endpoint.to_owned(),
            model: run_model.to_owned(),
            role: "solver",
        },
        "" => {
            let registry = crate::model_registry::for_task(core, task)
                .ok_or_else(|| "SUMMARIZER_NOT_CONFIGURED".to_owned())?;
            let needs = modbit_providers::registry::Needs {
                min_context_tokens: 0,
                execution_profile: Some(task.execution_profile.clone()),
                ..Default::default()
            };
            let entry = registry
                .bindings_for("summarizer", &needs)
                .into_iter()
                .next()
                .ok_or_else(|| "SUMMARIZER_NOT_CONFIGURED".to_owned())?;
            Summarizer {
                endpoint: entry.endpoint.clone(),
                model: entry.model.clone(),
                role: "summarizer",
            }
        }
        spec => {
            let (endpoint, model) = spec
                .split_once('/')
                .filter(|(e, m)| !e.is_empty() && !m.is_empty())
                .ok_or_else(|| "SUMMARIZER_MISCONFIGURED".to_owned())?;
            Summarizer {
                endpoint: endpoint.to_owned(),
                model: model.to_owned(),
                role: "summarizer",
            }
        }
    };
    if core
        .gateway
        .capability(&chosen.endpoint, &chosen.model)
        .is_none()
    {
        return Err("SUMMARIZER_UNKNOWN_MODEL".into());
    }
    let allowed: Option<Vec<String>> = config
        .models_allow
        .as_ref()
        .map(|r| r.value.iter().cloned().collect());
    if crate::routing::refused_by_model_policy(
        allowed.as_deref(),
        [(chosen.endpoint.clone(), chosen.model.clone())],
    )
    .is_some()
    {
        return Err("SUMMARIZER_NOT_ALLOWED".into());
    }
    Ok(chosen)
}

/// What the summarizer call cost, for the task's usage record.
#[derive(Clone, Debug)]
pub(crate) struct SummarizerUsage {
    pub endpoint: String,
    pub model: String,
    pub usage: Usage,
    pub reported: bool,
}

/// Everything the ladder needs, owned so a worker can run it.
pub(crate) struct Ladder {
    pub entries: Vec<SourceEntry>,
    pub previous: Option<CompactionManifest>,
    pub task_generation: u64,
    pub source_head_offset: u64,
    pub branch_generation: u64,
    pub target_tokens: u32,
    pub core_facts: Vec<PreservedFact>,
    pub known_paths: Vec<String>,
    pub summarizer: Result<Summarizer, String>,
}

/// What the ladder produced: the manifest to install and what the model
/// call, if any, cost.
pub(crate) struct Built {
    pub manifest: CompactionManifest,
    pub usage: Option<SummarizerUsage>,
}

fn summarizer_timeout(core: &Core, s: &Summarizer) -> std::time::Duration {
    let own = core
        .gateway
        .capability(&s.endpoint, &s.model)
        .map_or(SUMMARIZER_TIMEOUT_MS, |c| c.timeout_ms());
    let ms = std::env::var("MODBIT_COMPACTION_SUMMARIZER_TIMEOUT_MS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(own.min(SUMMARIZER_TIMEOUT_MS));
    std::time::Duration::from_millis(ms)
}

/// Ask the summarizer model for a structured summary of `entries`; the
/// answer is the model's, unvalidated.
async fn ask(
    core: &Arc<Core>,
    s: &Summarizer,
    ladder: &Ladder,
    budget_tokens: u32,
    cancel: &CancellationToken,
) -> (Result<String, (String, String)>, Option<SummarizerUsage>) {
    let Some(cap) = core.gateway.capability(&s.endpoint, &s.model) else {
        return (
            Err(("SUMMARIZER_UNKNOWN_MODEL".into(), String::new())),
            None,
        );
    };
    // The summarizer is given as much of the transcript as its own window
    // holds, a quarter of it per entry at most.
    let window_chars = usize::try_from(cap.context_tokens / 2)
        .unwrap_or(usize::MAX)
        .saturating_mul(4);
    let per_entry = (window_chars / ladder.entries.len().max(1)).clamp(300, 6_000);
    let input = summary::summarizer_input(
        &ladder.entries,
        &ladder.known_paths,
        budget_tokens,
        per_entry,
    );
    let timeout = summarizer_timeout(core, s);
    let request = ModelRequest {
        request_id: format!("compaction:{}", rand::random::<u32>()),
        model_policy: ModelPolicy {
            endpoint: s.endpoint.clone(),
            model: s.model.clone(),
            reasoning_effort: None,
            service_tier: None,
        },
        messages: vec![
            Message::text(Role::System, summary::SUMMARIZER_SYSTEM),
            Message {
                role: Role::User,
                parts: vec![ContentPart::Text { text: input }],
            },
        ],
        tool_projection: vec![],
        response_format: None,
        cache_key: None,
        cache_breakpoints: vec![],
        max_output_tokens: (budget_tokens.saturating_mul(2))
            .clamp(1_024, cap.output_budget().max(1_024)),
        timeout_ms: u64::try_from(timeout.as_millis()).unwrap_or(u64::MAX),
        policy_tags: vec![],
    };
    let needs = Requirements {
        role: Some(s.role.to_owned()),
        ..Default::default()
    };
    let stream = match core.gateway.stream(request, &needs, cancel.child_token()) {
        Ok(st) => st,
        Err(e) => {
            let code = match &e {
                modbit_providers::RouteError::RegistryRefused { code, .. } => (*code).to_owned(),
                _ => "ROUTE_REFUSED".to_owned(),
            };
            return (Err((format!("SUMMARIZER_{code}"), e.to_string())), None);
        }
    };
    let mut events = stream.events;
    let mut text = String::new();
    let mut usage = Usage::default();
    let mut reported = false;
    let mut error: Option<(String, String)> = None;
    // The gateway enforces the request's timeout; this bound is the Core's
    // own, so a stuck stream can never hold a compaction past it.
    let collect = async {
        while let Some(ev) = events.recv().await {
            match ev {
                ModelEvent::MessageDelta { text: t } => text.push_str(&t),
                ModelEvent::Usage { usage: u } => {
                    usage = u;
                    reported = true;
                }
                ModelEvent::Error { code, message, .. } => error = Some((code, message)),
                _ => {}
            }
        }
    };
    let timed_out = tokio::time::timeout(timeout + std::time::Duration::from_secs(2), collect)
        .await
        .is_err();
    let spent = Some(SummarizerUsage {
        endpoint: s.endpoint.clone(),
        model: s.model.clone(),
        usage,
        reported,
    });
    if timed_out {
        return (
            Err((
                "SUMMARIZER_TIMEOUT".into(),
                format!("no answer within {} ms", timeout.as_millis()),
            )),
            spent,
        );
    }
    if let Some((code, message)) = error {
        let reason = if code.contains("RATE") {
            "SUMMARIZER_RATE_LIMITED".to_owned()
        } else if code.contains("TIMEOUT") {
            "SUMMARIZER_TIMEOUT".to_owned()
        } else {
            format!("SUMMARIZER_{code}")
        };
        return (Err((reason, message)), spent);
    }
    if text.trim().is_empty() {
        return (Err(("SUMMARIZER_EMPTY".into(), String::new())), spent);
    }
    (Ok(text), spent)
}

/// Build the epoch for a range: the extractive baseline first, then the
/// model's summary on top of it when one is configured and it validates;
/// every failure keeps the baseline and says why. The transcript pointer is
/// attached either way.
pub(crate) async fn build(
    core: &Arc<Core>,
    task: &Task,
    ladder: Ladder,
    cancel: &CancellationToken,
) -> Built {
    let mut manifest = compact(&CompactionRequest {
        entries: &ladder.entries,
        previous: ladder.previous.as_ref(),
        task_generation: ladder.task_generation,
        source_head_offset: ladder.source_head_offset,
        branch_generation: ladder.branch_generation,
        compiler_version: modbit_prompt_compiler::COMPILER_VERSION,
        target_tokens: ladder.target_tokens,
        core_facts: &ladder.core_facts,
    });
    // The exact text of what this epoch replaces, stored once, content
    // addressed: harmless if the epoch never installs.
    let (transcript, index) = transcript_objects(&ladder.entries);
    let refs = {
        let store = core.store.lock().await;
        store
            .objects()
            .put(&transcript)
            .and_then(|t| store.objects().put(&index).map(|i| (t, i)))
            .ok()
    };
    if let Some((t, i)) = &refs {
        attach_transcript(&mut manifest, t, i);
    }
    let summarizer = match &ladder.summarizer {
        Ok(s) => s.clone(),
        Err(reason) => {
            attach_fallback(&mut manifest, reason);
            return Built {
                manifest,
                usage: None,
            };
        }
    };
    // The summary shares the window with the epoch segment; the user
    // messages alone may not take most of it.
    let budget = ladder.target_tokens.clamp(512, NARRATIVE_CAP_TOKENS);
    let user_tokens: u32 = ladder
        .entries
        .iter()
        .filter(|e| e.role == "user")
        .map(|e| modbit_compaction::estimate_tokens_v2(&e.text))
        .sum();
    if user_tokens > budget * 2 / 5 {
        attach_fallback(
            &mut manifest,
            summary::SummaryRejection::UserMessagesExceedBudget.code(),
        );
        return Built {
            manifest,
            usage: None,
        };
    }
    let (answer, usage) = ask(core, &summarizer, &ladder, budget, cancel).await;
    let reject = |manifest: &mut CompactionManifest, why: &str, detail: String| {
        eprintln!(
            "modbit-core: compaction summarizer {} for task {}: {why}: {detail}; the extractive epoch stands",
            summarizer.label(),
            task.task_id
        );
        attach_fallback(manifest, why);
    };
    let raw = match answer {
        Ok(raw) => raw,
        Err((code, detail)) => {
            reject(&mut manifest, &code, detail);
            return Built { manifest, usage };
        }
    };
    let parsed = match summary::parse(&raw) {
        Ok(p) => p,
        Err(e) => {
            reject(&mut manifest, e.code(), e.describe());
            return Built { manifest, usage };
        }
    };
    let input = SummaryInput {
        entries: &ladder.entries,
        core_facts: &ladder.core_facts,
        previous: ladder.previous.as_ref(),
        known_paths: &ladder.known_paths,
        budget_tokens: budget,
    };
    let narrative = match summary::validate(&parsed, &input) {
        Ok(_) => summary::render_narrative(&parsed, &summarizer.label()),
        Err(e) => {
            reject(&mut manifest, e.code(), e.describe());
            return Built { manifest, usage };
        }
    };
    // The Core's own segment is what keeps the user's words: if the budget
    // pushed one out of it, the summary does not stand in for it.
    if ladder
        .entries
        .iter()
        .filter(|e| e.role == "user")
        .any(|e| !manifest.projection.contains(e.text.trim()))
    {
        reject(
            &mut manifest,
            summary::SummaryRejection::UserMessagesExceedBudget.code(),
            "the epoch segment cannot hold every user message".into(),
        );
        return Built { manifest, usage };
    }
    let replaced: u32 = ladder
        .entries
        .iter()
        .map(|e| modbit_compaction::estimate_tokens_v2(&e.text))
        .sum();
    let after = modbit_compaction::estimate_tokens_v2(&manifest.projection)
        + modbit_compaction::estimate_tokens_v2(&narrative);
    if after >= replaced {
        let e = summary::SummaryRejection::NotSmaller {
            summary: after,
            replaced,
        };
        reject(&mut manifest, e.code(), e.describe());
        return Built { manifest, usage };
    }
    let files = parsed
        .files
        .iter()
        .map(|f| f.path.trim().trim_start_matches("./").to_owned())
        .collect();
    attach_narrative(&mut manifest, &summarizer.label(), narrative, files);
    Built { manifest, usage }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thresholds_follow_the_window_and_hold_a_floor() {
        let small = from_window(32_000, 4_096, 0);
        let large = from_window(400_000, 16_384, 0);
        assert!(small.hard < large.hard, "{small:?} {large:?}");
        assert_eq!(small.source, BudgetSource::ModelWindow);
        assert!(small.soft < small.hard && large.soft < large.hard);
        // 85 percent of (window - output - 2 percent margin).
        assert_eq!(large.hard, (400_000 - 16_384 - 8_000) * 85 / 100);
        // The rest of the prompt takes its share; the floor holds.
        assert!(from_window(32_000, 4_096, 10_000).hard < small.hard);
        assert_eq!(from_window(8_000, 4_096, 7_000).hard, MIN_TRANSCRIPT_TOKENS);
    }

    #[test]
    fn hysteresis_only_applies_to_the_model_aware_trigger() {
        let aware = from_window(200_000, 16_384, 0);
        assert!(aware.rearm_floor(5_000) > 5_000);
        let legacy = Thresholds {
            window: 0,
            hard: 12_000,
            soft: 9_000,
            source: BudgetSource::Fallback,
        };
        assert_eq!(legacy.rearm_floor(5_000), 0);
        assert_eq!(legacy.estimator(), "bytes/4");
    }
}
