//! The fixed-revision verified-outcome baseline (REQ-EPR-000).

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// What one invocation is known to have cost. Every field is `None` until the
/// provider reports it: a dropped or cancelled stream leaves the cost unknown,
/// and unknown is never zero (docs/38 "Shared serialization and validation").
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Usage {
    /// Input tokens, when reported.
    pub input_tokens: Option<u64>,
    /// Output tokens, when reported.
    pub output_tokens: Option<u64>,
    /// Of the input tokens, the part the provider served from its cache.
    pub cached_input_tokens: Option<u64>,
    /// Invocations whose usage the provider never reported.
    pub unreported_invocations: u32,
}

impl Usage {
    /// Add one reported invocation.
    pub fn add_reported(&mut self, input: u64, output: u64, cached: u64) {
        self.input_tokens = Some(self.input_tokens.unwrap_or(0) + input);
        self.output_tokens = Some(self.output_tokens.unwrap_or(0) + output);
        self.cached_input_tokens = Some(self.cached_input_tokens.unwrap_or(0) + cached);
    }

    /// Note one invocation whose usage never arrived.
    pub fn add_unreported(&mut self) {
        self.unreported_invocations += 1;
    }

    /// Whether every invocation reported its usage.
    #[must_use]
    pub fn complete(&self) -> bool {
        self.unreported_invocations == 0
    }
}

/// What the user had to do during the task: the intervention signals docs/27
/// keeps separate from the model's own work.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Interventions {
    /// Typed questions the agent asked.
    pub questions_asked: u32,
    /// Questions the user answered.
    pub questions_answered: u32,
    /// Protected effects that needed an approval.
    pub approvals_requested: u32,
    /// Approvals the user granted.
    pub approvals_granted: u32,
    /// Approvals the user denied.
    pub approvals_denied: u32,
    /// Steering or follow-up inputs the user queued.
    pub steering_inputs: u32,
    /// Review decisions the user recorded.
    pub review_decisions: u32,
    /// Times the run stopped needing attention.
    pub attention_stops: u32,
}

impl Interventions {
    /// Whether the user had to do anything at all.
    #[must_use]
    pub fn any(&self) -> bool {
        self.questions_asked
            + self.approvals_requested
            + self.steering_inputs
            + self.review_decisions
            + self.attention_stops
            > 0
    }
}

/// One task's place in the baseline.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskOutcome {
    /// Task id.
    pub task_id: String,
    /// sha256 of the goal text, so the bundle carries no prose.
    pub goal_digest: String,
    /// Final task state.
    pub state: String,
    /// The completion verification's own verdict: `PASSED` | `NO_CHECKS` |
    /// `FAILED` | `NOT_RUN`.
    pub verification: String,
    /// Verified: a completion run passed with at least one check.
    pub verified: bool,
    /// Checks that passed and failed in that run.
    pub checks: (u32, u32),
    /// Model invocations.
    pub model_calls: u32,
    /// Retries the gateway performed before a first token, summed.
    pub retries: u32,
    /// Prompt-prefix cache units: (reused, rebuilt).
    pub cache_units: (u32, u32),
    /// Tool calls.
    pub tool_calls: u32,
    /// Usage, with unknowns preserved.
    pub usage: Usage,
    /// Wall time of the task, in milliseconds.
    pub wall_ms: u64,
    /// Time inside model invocations.
    pub model_ms: u64,
    /// Time inside tool calls.
    pub tool_ms: u64,
    /// What the user had to do.
    pub interventions: Interventions,
    /// The model the invocations routed to.
    pub model: String,
    /// Endpoint name.
    pub endpoint: String,
}

/// A published baseline: what the direct path did, pinned to what produced it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BaselineBundle {
    /// Schema version of this bundle.
    pub schema_version: u32,
    /// Build identity of the Core that produced it.
    pub build_digest: String,
    /// Repository revision the tasks ran against.
    pub repository_revision: String,
    /// Environment digest (toolchain and platform identity).
    pub environment_digest: String,
    /// When it was assembled (milliseconds since the epoch).
    pub created_at_ms: i64,
    /// Per task, in the order the tasks were created.
    pub tasks: Vec<TaskOutcome>,
    /// Tasks that reached a verified outcome.
    pub verified_tasks: usize,
    /// Tasks whose usage is incomplete, so their cost is not fully known.
    pub tasks_with_unknown_usage: usize,
    /// What this bundle is for, and what it is not.
    pub note: String,
    /// sha256 over every field above, so it can be referenced by digest.
    pub bundle_digest: String,
}

/// What a baseline is, carried with it.
pub const NOTE: &str = "A fixed-revision verified-outcome baseline of the direct single-model path (REQ-EPR-000). Any later routing change is compared against a bundle, never against a memory of one. Unknown cost stays unknown: a task whose provider never reported usage is counted in tasks_with_unknown_usage and its usage fields are null rather than zero.";

/// sha256 of a string, as the bundle uses it.
#[must_use]
pub fn digest_of(text: &str) -> String {
    hex::encode(Sha256::digest(text.as_bytes()))
}

/// The digest of a bundle's content, computed over its canonical JSON with the
/// digest field itself empty.
#[must_use]
pub fn bundle_digest(bundle: &BaselineBundle) -> String {
    let mut b = bundle.clone();
    b.bundle_digest = String::new();
    digest_of(&serde_json::to_string(&b).unwrap_or_default())
}

/// Assemble and seal a bundle.
#[must_use]
pub fn publish(
    build_digest: &str,
    repository_revision: &str,
    environment_digest: &str,
    created_at_ms: i64,
    tasks: Vec<TaskOutcome>,
) -> BaselineBundle {
    let verified_tasks = tasks.iter().filter(|t| t.verified).count();
    let tasks_with_unknown_usage = tasks.iter().filter(|t| !t.usage.complete()).count();
    let mut bundle = BaselineBundle {
        schema_version: 1,
        build_digest: build_digest.to_owned(),
        repository_revision: repository_revision.to_owned(),
        environment_digest: environment_digest.to_owned(),
        created_at_ms,
        tasks,
        verified_tasks,
        tasks_with_unknown_usage,
        note: NOTE.to_owned(),
        bundle_digest: String::new(),
    };
    bundle.bundle_digest = bundle_digest(&bundle);
    bundle
}
