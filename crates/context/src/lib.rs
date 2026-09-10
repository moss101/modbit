//! `modbit-context` — Context Pack compiler and provenance ledger (docs/18
//! "Context Pack"; M3.8).
//!
//! Canonical owner: context-engine (`docs/12_REPOSITORY_AND_MODULE_LAYOUT.md`,
//! `docs/81_ARCHITECTURE_GUARDRAILS_AND_FORBIDDEN_DUPLICATION.md`).
//! Dependency direction is enforced by `tools/architecture-lint`.
//!
//! The compiler packs retrieval candidates under an explicit token budget:
//! critical entries (task constraints, diagnostics) first, then the highest
//! marginal evidence utility; duplicates collapse on span containment; what
//! was left out is summarised. Every entry carries provenance (path,
//! revision, content hash, sources, reasons) and a token cost from a named
//! estimator. The ledger records every injected entry and, later, whether a
//! tool call actually used the path at the revision it was retrieved at.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Compiler version stamped on every pack.
pub const COMPILER_VERSION: &str = "context-pack-v1";
/// The token estimator (deterministic; not a model tokenizer).
pub const TOKEN_ESTIMATOR: &str = "bytes/4";

/// Estimated tokens of a text (ceil(bytes / 4)).
#[must_use]
pub fn estimate_tokens(text: &str) -> u32 {
    u32::try_from(text.len().div_ceil(4)).unwrap_or(u32::MAX)
}

/// A retrieval candidate offered to the packer.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Candidate {
    /// Root-relative path.
    pub path: String,
    /// 1-based line range of the excerpt (`None` = file head).
    pub lines: Option<(u32, u32)>,
    /// Byte span (`None` = file head).
    pub span: Option<(u64, u64)>,
    /// Retrieval score.
    pub score: f32,
    /// Sources that produced it.
    pub sources: Vec<String>,
    /// Retrieval reasons / boosts.
    pub reasons: Vec<String>,
    /// Content hash of the file.
    pub content_hash: Option<String>,
    /// The excerpt.
    pub text: String,
    /// Critical: packed before utility ordering (task constraint, diagnostic).
    pub critical: bool,
    /// Why it is critical, when it is.
    pub critical_reason: Option<String>,
    /// Whether the file has uncommitted changes in the worktree.
    pub fresh_in_worktree: bool,
}

/// Where an entry came from.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Provenance {
    /// Path.
    pub path: String,
    /// Workspace revision the excerpt was read at.
    pub workspace_revision: u64,
    /// Content hash of the file at that revision.
    pub content_hash: Option<String>,
    /// Retrieval sources.
    pub sources: Vec<String>,
    /// Retrieval reasons.
    pub retrieval_reasons: Vec<String>,
    /// sha256 of the excerpt.
    pub excerpt_hash: String,
}

/// A packed entry.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    /// Stable id (sha256 of path, span and excerpt hash).
    pub entry_id: String,
    /// `workspace:<path>`.
    pub source_ref: String,
    /// 1-based line range, when narrower than the file.
    pub lines: Option<(u32, u32)>,
    /// Byte span, when narrower than the file.
    pub span: Option<(u64, u64)>,
    /// Provenance.
    pub provenance: Provenance,
    /// Freshness: `fresh_in_worktree` when the file has uncommitted changes.
    pub freshness: String,
    /// Why it was packed (`critical:<reason>` | `utility`).
    pub reason: String,
    /// Score.
    pub score: f32,
    /// Estimated tokens.
    pub token_cost: u32,
    /// The excerpt.
    pub text: String,
}

/// What did not fit.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct OmittedSummary {
    /// Entries omitted.
    pub count: usize,
    /// Their estimated tokens.
    pub token_cost: u32,
    /// Critical entries omitted (the pack is then marked incomplete).
    pub critical_count: usize,
    /// Entries collapsed as duplicates of a packed span.
    pub duplicates: usize,
    /// Paths of the omitted entries (bounded).
    pub paths: Vec<String>,
}

/// The pack.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ContextPack {
    /// sha256 over the entries.
    pub pack_id: String,
    /// Workspace revision.
    pub workspace_revision: u64,
    /// Query / task fingerprint.
    pub fingerprint: String,
    /// Entries in pack order.
    pub entries: Vec<Entry>,
    /// What was left out.
    pub omitted_summary: OmittedSummary,
    /// Budget.
    pub token_budget: u32,
    /// Tokens used.
    pub token_used: u32,
    /// Whether every critical candidate fit.
    pub complete: bool,
    /// Compiler version.
    pub compiler_version: String,
    /// Token estimator.
    pub token_estimator: String,
}

fn sha(parts: &[&str]) -> String {
    let mut h = Sha256::new();
    for p in parts {
        h.update(p.as_bytes());
        h.update([0]);
    }
    hex::encode(h.finalize())
}

fn contains(outer: &Candidate, inner: &Candidate) -> bool {
    if outer.path != inner.path {
        return false;
    }
    match (outer.lines, inner.lines) {
        (None, _) => true,
        (Some(_), None) => false,
        (Some((a, b)), Some((c, d))) => a <= c && d <= b,
    }
}

/// Pack candidates under `token_budget` (docs/18: critical first, then the
/// highest marginal evidence utility; duplicates collapse on span identity
/// or containment).
#[must_use]
pub fn pack(
    candidates: &[Candidate],
    token_budget: u32,
    workspace_revision: u64,
    fingerprint: &str,
) -> ContextPack {
    let cost = |c: &Candidate| estimate_tokens(&c.text).max(1);
    let mut order: Vec<usize> = (0..candidates.len()).collect();
    order.sort_by(|&a, &b| {
        let (ca, cb) = (&candidates[a], &candidates[b]);
        cb.critical
            .cmp(&ca.critical)
            .then_with(|| {
                let ua = ca.score / cost(ca) as f32;
                let ub = cb.score / cost(cb) as f32;
                ub.partial_cmp(&ua).unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| ca.path.cmp(&cb.path))
            .then_with(|| ca.lines.cmp(&cb.lines))
    });
    let mut used = 0u32;
    let mut packed: Vec<usize> = Vec::new();
    let mut omitted = OmittedSummary::default();
    for i in order {
        let c = &candidates[i];
        if packed.iter().any(|&p| {
            contains(&candidates[p], c)
                || (candidates[p].path == c.path && candidates[p].lines == c.lines)
        }) {
            omitted.duplicates += 1;
            continue;
        }
        // A wider span absorbs the narrower packed entries it contains (their
        // tokens are refunded); the narrower ones become duplicates.
        let absorbed: Vec<usize> = packed
            .iter()
            .copied()
            .filter(|&p| contains(c, &candidates[p]))
            .collect();
        let refund: u32 = absorbed.iter().map(|&p| cost(&candidates[p])).sum();
        let tc = cost(c);
        if used - refund + tc > token_budget {
            omitted.count += 1;
            omitted.token_cost += tc;
            if c.critical {
                omitted.critical_count += 1;
            }
            if omitted.paths.len() < 20 && !omitted.paths.contains(&c.path) {
                omitted.paths.push(c.path.clone());
            }
            continue;
        }
        if !absorbed.is_empty() {
            packed.retain(|p| !absorbed.contains(p));
            omitted.duplicates += absorbed.len();
        }
        used = used - refund + tc;
        packed.push(i);
    }
    let entries: Vec<Entry> = packed
        .iter()
        .map(|&i| {
            let c = &candidates[i];
            let excerpt_hash = sha(&[&c.text]);
            let span = c.span.map_or(String::new(), |(a, b)| format!("{a}-{b}"));
            Entry {
                entry_id: sha(&[&c.path, &span, &excerpt_hash]),
                source_ref: format!("workspace:{}", c.path),
                lines: c.lines,
                span: c.span,
                provenance: Provenance {
                    path: c.path.clone(),
                    workspace_revision,
                    content_hash: c.content_hash.clone(),
                    sources: c.sources.clone(),
                    retrieval_reasons: c.reasons.clone(),
                    excerpt_hash,
                },
                freshness: if c.fresh_in_worktree {
                    "fresh_in_worktree".into()
                } else {
                    "committed".into()
                },
                reason: if c.critical {
                    format!(
                        "critical:{}",
                        c.critical_reason.as_deref().unwrap_or("unspecified")
                    )
                } else {
                    "utility".into()
                },
                score: c.score,
                token_cost: cost(c),
                text: c.text.clone(),
            }
        })
        .collect();
    let ids: Vec<&str> = entries.iter().map(|e| e.entry_id.as_str()).collect();
    ContextPack {
        pack_id: sha(&[
            &[fingerprint, &workspace_revision.to_string()][..],
            &ids[..],
        ]
        .concat()),
        workspace_revision,
        fingerprint: fingerprint.to_owned(),
        entries,
        omitted_summary: omitted.clone(),
        token_budget,
        token_used: used,
        complete: omitted.critical_count == 0,
        compiler_version: COMPILER_VERSION.into(),
        token_estimator: TOKEN_ESTIMATOR.into(),
    }
}

/// A later use of a ledger entry.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Use {
    /// Tool call that used the path.
    pub tool_call_id: String,
    /// Tool.
    pub tool_name: String,
    /// Workspace revision at the use.
    pub workspace_revision: u64,
}

/// One injected entry and its later use.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LedgerEntry {
    /// Pack.
    pub pack_id: String,
    /// Entry.
    pub entry_id: String,
    /// Path.
    pub path: String,
    /// Lines.
    pub lines: Option<(u32, u32)>,
    /// Revision retrieved at.
    pub workspace_revision: u64,
    /// Ordinal of the pack in this ledger.
    pub pack_ordinal: u64,
    /// The first use, when any.
    pub used: Option<Use>,
}

/// The Context Ledger of one task.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextLedger {
    /// Entries in injection order.
    pub entries: Vec<LedgerEntry>,
    /// Packs recorded.
    pub packs: u64,
}

impl ContextLedger {
    /// Record every entry of a pack as injected.
    pub fn record(&mut self, pack: &ContextPack) {
        self.packs += 1;
        for e in &pack.entries {
            self.entries.push(LedgerEntry {
                pack_id: pack.pack_id.clone(),
                entry_id: e.entry_id.clone(),
                path: e.provenance.path.clone(),
                lines: e.lines,
                workspace_revision: pack.workspace_revision,
                pack_ordinal: self.packs,
                used: None,
            });
        }
    }

    /// Mark the unused entries of `path` retrieved at `revision` as used by a
    /// tool call; a stale-revision record is not a use (docs/28). Returns how
    /// many entries were marked.
    pub fn mark_used(
        &mut self,
        path: &str,
        revision: u64,
        tool_call_id: &str,
        tool_name: &str,
    ) -> usize {
        let mut n = 0;
        for e in &mut self.entries {
            if e.used.is_none() && e.path == path && e.workspace_revision == revision {
                e.used = Some(Use {
                    tool_call_id: tool_call_id.to_owned(),
                    tool_name: tool_name.to_owned(),
                    workspace_revision: revision,
                });
                n += 1;
            }
        }
        n
    }

    /// Whether `path` has a retrieval record at exactly `revision`.
    #[must_use]
    pub fn has_record(&self, path: &str, revision: u64) -> bool {
        self.entries
            .iter()
            .any(|e| e.path == path && e.workspace_revision == revision)
    }

    /// (injected, used) counts.
    #[must_use]
    pub fn usage(&self) -> (usize, usize) {
        (
            self.entries.len(),
            self.entries.iter().filter(|e| e.used.is_some()).count(),
        )
    }
}
