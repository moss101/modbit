//! Hand-labelled cases over this repository as the corpus (REQ-PX-137).
//!
//! Author-written: each row is a question a developer might ask and the
//! file (or files) that answer it, chosen by reading the module's own header.
//! The labels are paths relative to the repository root. The corpus copies
//! `crates`, `services` and `apps/cli`, so a label must live under one of
//! them; `labels_missing_from` reports any that no longer exist, so a rename
//! fails the benchmark instead of silently scoring zero.

use std::path::Path;

use modbit_retrieval::bench::Case;

/// (id, question, labelled files).
const ROWS: [(&str, &str, &[&str]); 64] = [
    (
        "registry-sign",
        "sign and verify the model registry document",
        &["crates/providers/src/registry.rs"],
    ),
    (
        "sse-parser",
        "server sent events stream parser",
        &["crates/providers/src/sse.rs"],
    ),
    (
        "anthropic-adapter",
        "anthropic messages api adapter tool_use streaming",
        &["crates/providers/src/anthropic.rs"],
    ),
    (
        "openai-adapter",
        "openai compatible chat completions adapter",
        &["crates/providers/src/openai.rs"],
    ),
    (
        "cache-economics",
        "cache economics routing epochs",
        &["crates/providers/src/economics.rs"],
    ),
    (
        "request-profiler",
        "request profiler what a request needs",
        &["crates/providers/src/profiler.rs"],
    ),
    (
        "feasibility",
        "feasibility confidence adjusted quality lower bound",
        &["crates/providers/src/feasibility.rs"],
    ),
    (
        "plan-compiler",
        "conditional plan compiler slots",
        &["crates/providers/src/compiler.rs"],
    ),
    (
        "length-framing",
        "length prefixed framing for the local protocol",
        &["crates/protocol/src/framing.rs"],
    ),
    (
        "local-transport",
        "unix domain socket named pipe local transport endpoint",
        &["crates/protocol/src/local.rs"],
    ),
    (
        "credential-broker",
        "credential broker handle",
        &["crates/secrets/src/broker.rs"],
    ),
    (
        "redactor",
        "redact secrets from output",
        &["crates/secrets/src/redact.rs"],
    ),
    (
        "object-directory",
        "content addressed object directory sha256",
        &["crates/event-store/src/objects.rs"],
    ),
    (
        "migration-runner",
        "forward only checksummed migration runner",
        &["crates/event-store/src/migrations.rs"],
    ),
    (
        "hunk-apply",
        "unified diff parsing selective hunk apply",
        &["crates/git/src/diff.rs"],
    ),
    (
        "apply-back",
        "apply back worktree result to the user's checkout",
        &[
            "crates/git/src/apply.rs",
            "services/modbit-core/src/apply_back.rs",
        ],
    ),
    (
        "mcp-jsonrpc",
        "json-rpc framing mcp handshake",
        &["crates/mcp/src/protocol.rs"],
    ),
    (
        "mcp-call-table",
        "in-flight call table cancellation unknown outcome",
        &["crates/mcp/src/calls.rs"],
    ),
    (
        "guest-image",
        "signed versioned guest image",
        &["crates/sandbox/src/image.rs"],
    ),
    (
        "egress-broker",
        "egress broker allowlist network",
        &["crates/sandbox/src/egress.rs"],
    ),
    (
        "guest-auth",
        "hmac ephemeral guest credential authentication",
        &["crates/sandbox/src/auth.rs"],
    ),
    (
        "capability-kernel",
        "capability kernel approval policy decision",
        &["crates/policy/src/kernel.rs"],
    ),
    (
        "receipt-chain",
        "protected effect receipt hash chain",
        &["crates/policy/src/ledger.rs"],
    ),
    (
        "config-resolver",
        "configuration resolver admin user repository merge",
        &["crates/policy/src/config.rs"],
    ),
    (
        "acceptance-gate",
        "acceptance gate verdict reject",
        &[
            "crates/verification/src/gate.rs",
            "services/modbit-core/src/gate.rs",
        ],
    ),
    (
        "diff-invariants",
        "diff invariants checked per change transaction",
        &["crates/verification/src/invariants.rs"],
    ),
    (
        "language-tiers",
        "language tier conformance",
        &["crates/verification/src/tiers.rs"],
    ),
    (
        "verification-plan",
        "derived verification plan from repository configuration",
        &["crates/verification/src/plan.rs"],
    ),
    (
        "ci-evidence",
        "ci results as external evidence",
        &[
            "crates/verification/src/ci.rs",
            "services/modbit-core/src/ci_evidence.rs",
        ],
    ),
    (
        "bm25",
        "bm25 lexical index tantivy",
        &["crates/retrieval/src/lexical.rs"],
    ),
    (
        "trigram",
        "trigram prefilter regex search",
        &["crates/retrieval/src/trigram.rs"],
    ),
    (
        "symbol-index",
        "tree sitter symbol index",
        &["crates/retrieval/src/symbols.rs"],
    ),
    (
        "impact-selection",
        "impact based test selection",
        &["crates/retrieval/src/impact.rs"],
    ),
    (
        "rank-fusion",
        "rank fusion retrieval planner levels",
        &["crates/retrieval/src/planner.rs"],
    ),
    (
        "persisted-index",
        "persisted index store",
        &["crates/retrieval/src/persist.rs"],
    ),
    (
        "semantic-index",
        "semantic chunk vector index hnsw",
        &["crates/retrieval/src/semantic.rs"],
    ),
    (
        "compaction-epoch",
        "compaction epoch projection extractive facts",
        &["crates/compaction/src/lib.rs"],
    ),
    (
        "compaction-summary",
        "structured compaction summary citations validation",
        &["crates/compaction/src/summary.rs"],
    ),
    (
        "context-pack",
        "context pack token budget provenance ledger",
        &["crates/context/src/lib.rs"],
    ),
    (
        "hierarchical-budgets",
        "hierarchical budgets child holds",
        &["crates/core-runtime/src/budget.rs"],
    ),
    (
        "capacity-tickets",
        "capacity tickets",
        &["crates/core-runtime/src/capacity.rs"],
    ),
    (
        "write-conflict",
        "semantic write conflict detection",
        &["crates/core-runtime/src/conflict.rs"],
    ),
    (
        "plan-admission",
        "plan admission validates the cap",
        &["crates/core-runtime/src/admission.rs"],
    ),
    (
        "tool-projection",
        "tool projection schema bytes budget",
        &[
            "crates/core-runtime/src/projection.rs",
            "services/modbit-core/src/tool_projection.rs",
        ],
    ),
    (
        "injection-detection",
        "prompt injection detection untrusted web content",
        &["crates/browser/src/injection.rs"],
    ),
    (
        "lsp-client",
        "language server lsp client content-length framing",
        &["crates/diagnostics/src/lsp.rs"],
    ),
    (
        "instructions-layer",
        "agents.md claude.md native rules layer",
        &["crates/prompt-compiler/src/instructions.rs"],
    ),
    (
        "context-accounting",
        "context accounting by category",
        &["crates/prompt-compiler/src/accounting.rs"],
    ),
    (
        "skill-trust",
        "skill trust owner decision",
        &["crates/skills/src/trust.rs"],
    ),
    (
        "skill-import",
        "import another agent's configuration",
        &["crates/skills/src/import.rs"],
    ),
    (
        "otlp-export",
        "opentelemetry otlp export",
        &["crates/observability/src/otlp.rs"],
    ),
    (
        "component-health",
        "persisted component health",
        &["crates/observability/src/health.rs"],
    ),
    (
        "outcome-baseline",
        "outcome baseline bundle fixed revision",
        &[
            "crates/observability/src/baseline.rs",
            "services/modbit-core/src/baseline.rs",
        ],
    ),
    (
        "critique",
        "isolated review bounded revision critique",
        &["services/modbit-core/src/critique.rs"],
    ),
    (
        "escalation",
        "quality rejection escalation continuation stronger slot",
        &["services/modbit-core/src/escalation.rs"],
    ),
    (
        "side-question",
        "side question bounded model call snapshot",
        &["services/modbit-core/src/side.rs"],
    ),
    (
        "pause-resume",
        "pause and resume a task",
        &["services/modbit-core/src/pause.rs"],
    ),
    (
        "worktree-cleanup",
        "scheduled cleanup of worktrees",
        &["services/modbit-core/src/worktree_cleanup.rs"],
    ),
    (
        "file-browsing",
        "read only file browsing of a task's workspace",
        &["services/modbit-core/src/workspace_files.rs"],
    ),
    (
        "pull-request",
        "pull request opened from a reviewed result",
        &["services/modbit-core/src/pull_request.rs"],
    ),
    (
        "conversation-search",
        "full text search over conversations",
        &["services/modbit-core/src/conversation_search.rs"],
    ),
    (
        "typed-undo",
        "typed undo the inverse of one tool call",
        &["services/modbit-core/src/undo.rs"],
    ),
    (
        "workspace-revision",
        "workspace revision monotonic bound to git head",
        &["crates/workspace/src/revision.rs"],
    ),
    (
        "path-policy",
        "path normalization policy",
        &["crates/workspace/src/paths.rs"],
    ),
];

/// The cases.
#[must_use]
pub fn cases() -> Vec<Case> {
    ROWS.iter()
        .map(|(id, query, files)| Case {
            id: format!("repo-{id}"),
            query: (*query).to_owned(),
            intent: "hybrid".into(),
            relevant: files.iter().map(|f| (*f).to_owned()).collect(),
            impacted: vec![],
        })
        .collect()
}

/// Labels that do not exist under `repo_root`.
#[must_use]
pub fn labels_missing_from(repo_root: &Path) -> Vec<String> {
    ROWS.iter()
        .flat_map(|(_, _, files)| files.iter())
        .filter(|f| !repo_root.join(f).is_file())
        .map(|f| (*f).to_owned())
        .collect()
}
