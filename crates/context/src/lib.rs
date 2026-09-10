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
    /// The bytes were re-read from the active revision and differed from the
    /// index (read-through hydration; docs/18 "Index freshness").
    #[serde(default)]
    pub rehydrated: bool,
    /// Symbol signatures of the file (`kind name Lstart-end`), for a stub.
    #[serde(default)]
    pub signatures: Vec<String>,
}

/// A signature-only stub of a candidate that did not fit as an entry: the
/// handle (`source_ref`, lines) hydrates on request through `fs.read` and
/// keeps its provenance.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Stub {
    /// Stable id (sha256 of path, span and `stub`).
    pub entry_id: String,
    /// `workspace:<path>`.
    pub source_ref: String,
    /// 1-based line range of the stubbed excerpt, when narrower than the file.
    pub lines: Option<(u32, u32)>,
    /// Symbol signatures.
    pub signatures: Vec<String>,
    /// Provenance.
    pub provenance: Provenance,
    /// Score.
    pub score: f32,
    /// Estimated tokens of the stub text.
    pub token_cost: u32,
    /// Estimated tokens the hydrated excerpt would cost.
    pub hydrated_token_cost: u32,
    /// How to hydrate (`fs.read <path>`; the ledger records the use).
    pub hydrate: String,
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
    /// Signature-only stubs for lower-ranked candidates, within the budget.
    pub stubs: Vec<Stub>,
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

fn freshness_of(c: &Candidate) -> String {
    if c.rehydrated {
        "rehydrated_from_active_revision".into()
    } else if c.fresh_in_worktree {
        "fresh_in_worktree".into()
    } else {
        "committed".into()
    }
}

/// The text of a stub: the path, lines and symbol signatures.
fn stub_text(c: &Candidate) -> String {
    let mut t = match c.lines {
        Some((a, b)) => format!("{} L{a}-{b}", c.path),
        None => c.path.clone(),
    };
    for sig in &c.signatures {
        t.push('\n');
        t.push_str(sig);
    }
    t
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

/// What one packed entry inherited from the spans it absorbed.
#[derive(Clone, Debug, Default)]
struct Merged {
    sources: Vec<String>,
    reasons: Vec<String>,
    critical_reason: Option<String>,
}

impl Merged {
    fn absorb(&mut self, c: &Candidate) {
        for s in &c.sources {
            if !self.sources.contains(s) {
                self.sources.push(s.clone());
            }
        }
        for r in &c.reasons {
            if !self.reasons.contains(r) {
                self.reasons.push(r.clone());
            }
        }
        if c.critical && self.critical_reason.is_none() {
            self.critical_reason = Some(
                c.critical_reason
                    .clone()
                    .unwrap_or_else(|| "unspecified".into()),
            );
        }
    }

    fn take(&mut self, other: Self) {
        for s in other.sources {
            if !self.sources.contains(&s) {
                self.sources.push(s);
            }
        }
        for r in other.reasons {
            if !self.reasons.contains(&r) {
                self.reasons.push(r);
            }
        }
        self.critical_reason = self.critical_reason.take().or(other.critical_reason);
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
    // A slice of the budget is held back for signature stubs of what does
    // not fit (REQ-EV-0003): entries pack up to `entry_budget`, stubs then
    // use whatever is left of the whole budget.
    let reserve = (token_budget / 8).max(16).min(token_budget / 2);
    let entry_budget = token_budget - reserve;
    let mut used = 0u32;
    let mut packed: Vec<usize> = Vec::new();
    let mut omitted = OmittedSummary::default();
    let mut left_out: Vec<usize> = Vec::new();
    // Absorption must not lose provenance: when one span swallows another, the
    // survivor inherits the other's sources, reasons and critical mark, so a
    // selected range covered by a wider retrieval hit still reads as selected.
    let mut merged: std::collections::HashMap<usize, Merged> = std::collections::HashMap::new();
    for i in order {
        let c = &candidates[i];
        if let Some(&p) = packed.iter().find(|&&p| {
            contains(&candidates[p], c)
                || (candidates[p].path == c.path && candidates[p].lines == c.lines)
        }) {
            omitted.duplicates += 1;
            let entry = merged.entry(p).or_default();
            entry.absorb(c);
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
        if used - refund + tc > entry_budget {
            left_out.push(i);
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
            let mut carried = Merged::default();
            for &p in &absorbed {
                carried.absorb(&candidates[p]);
                if let Some(m) = merged.remove(&p) {
                    carried.take(m);
                }
            }
            merged.entry(i).or_default().take(carried);
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
            let m = merged.get(&i);
            let mut sources = c.sources.clone();
            let mut reasons = c.reasons.clone();
            if let Some(m) = m {
                for s in &m.sources {
                    if !sources.contains(s) {
                        sources.push(s.clone());
                    }
                }
                for r in &m.reasons {
                    if !reasons.contains(r) {
                        reasons.push(r.clone());
                    }
                }
            }
            let critical = c.critical || m.is_some_and(|m| m.critical_reason.is_some());
            let critical_reason = c
                .critical_reason
                .clone()
                .or_else(|| m.and_then(|m| m.critical_reason.clone()));
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
                    sources,
                    retrieval_reasons: reasons,
                    excerpt_hash,
                },
                freshness: freshness_of(c),
                reason: if critical {
                    format!(
                        "critical:{}",
                        critical_reason.as_deref().unwrap_or("unspecified")
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
    // Stubs (docs/18, REQ-EV-0003/0167): the left-out candidates in utility
    // order as signature-only handles, while they fit in what is left.
    let mut stubs: Vec<Stub> = Vec::new();
    for &i in &left_out {
        let c = &candidates[i];
        let text = stub_text(c);
        let tc = estimate_tokens(&text).max(1);
        if used + tc > token_budget {
            continue;
        }
        used += tc;
        let excerpt_hash = sha(&[&c.text]);
        let span = c.span.map_or(String::new(), |(a, b)| format!("{a}-{b}"));
        stubs.push(Stub {
            entry_id: sha(&[&c.path, &span, "stub"]),
            source_ref: format!("workspace:{}", c.path),
            lines: c.lines,
            signatures: c.signatures.clone(),
            provenance: Provenance {
                path: c.path.clone(),
                workspace_revision,
                content_hash: c.content_hash.clone(),
                sources: c.sources.clone(),
                retrieval_reasons: c.reasons.clone(),
                excerpt_hash,
            },
            score: c.score,
            token_cost: tc,
            hydrated_token_cost: cost(c),
            hydrate: format!("fs.read {}", c.path),
        });
    }
    let mut ids: Vec<&str> = entries.iter().map(|e| e.entry_id.as_str()).collect();
    ids.extend(stubs.iter().map(|s| s.entry_id.as_str()));
    ContextPack {
        pack_id: sha(&[
            &[fingerprint, &workspace_revision.to_string()][..],
            &ids[..],
        ]
        .concat()),
        workspace_revision,
        fingerprint: fingerprint.to_owned(),
        entries,
        stubs,
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
    /// Injected as a signature-only stub (hydration is the use).
    #[serde(default)]
    pub stub: bool,
    /// Content hash of the file the entry was read at.
    #[serde(default)]
    pub content_hash: Option<String>,
    /// The first use, when any.
    pub used: Option<Use>,
}

/// A direct retrieval of a path (a read or a language-service query) at a
/// revision: the retrieval record docs/28 §2 requires before an edit.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadRecord {
    /// Path.
    pub path: String,
    /// Workspace revision at the read.
    pub workspace_revision: u64,
    /// Content hash of the file at the read (`None` when unknown).
    #[serde(default)]
    pub content_hash: Option<String>,
    /// Tool call.
    pub tool_call_id: String,
    /// Tool (`fs.read`, `lsp.*`, `change.*` for the task's own write).
    pub tool_name: String,
}

/// The Context Ledger of one task.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ContextLedger {
    /// Entries in injection order.
    pub entries: Vec<LedgerEntry>,
    /// Packs recorded.
    pub packs: u64,
    /// Direct retrievals (reads, language-service queries).
    #[serde(default)]
    pub reads: Vec<ReadRecord>,
    /// The most recent pack, so the runtime can inject it into the prompt
    /// with its provenance (REQ-EV-0169).
    #[serde(default)]
    pub last_pack: Option<ContextPack>,
}

impl ContextLedger {
    /// Record every entry of a pack as injected.
    pub fn record(&mut self, pack: &ContextPack) {
        self.packs += 1;
        self.last_pack = Some(pack.clone());
        for e in &pack.entries {
            self.entries.push(LedgerEntry {
                pack_id: pack.pack_id.clone(),
                entry_id: e.entry_id.clone(),
                path: e.provenance.path.clone(),
                lines: e.lines,
                workspace_revision: pack.workspace_revision,
                pack_ordinal: self.packs,
                stub: false,
                content_hash: e.provenance.content_hash.clone(),
                used: None,
            });
        }
        for st in &pack.stubs {
            self.entries.push(LedgerEntry {
                pack_id: pack.pack_id.clone(),
                entry_id: st.entry_id.clone(),
                path: st.provenance.path.clone(),
                lines: st.lines,
                workspace_revision: pack.workspace_revision,
                pack_ordinal: self.packs,
                stub: true,
                content_hash: st.provenance.content_hash.clone(),
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

    /// Record a direct retrieval of `path` at `revision`.
    pub fn record_read(
        &mut self,
        path: &str,
        revision: u64,
        content_hash: Option<&str>,
        tool_call_id: &str,
        tool_name: &str,
    ) {
        self.reads.push(ReadRecord {
            path: path.to_owned(),
            workspace_revision: revision,
            content_hash: content_hash.map(str::to_owned),
            tool_call_id: tool_call_id.to_owned(),
            tool_name: tool_name.to_owned(),
        });
    }

    /// Whether `path` has a retrieval record for exactly the bytes it holds
    /// now (`content_hash`): a record of other bytes is stale (docs/28 §2).
    #[must_use]
    pub fn has_current_record(&self, path: &str, content_hash: &str) -> bool {
        self.entries.iter().any(|e| {
            e.path == path
                && self
                    .entries_hash(&e.entry_id)
                    .is_some_and(|h| h == content_hash)
        }) || self
            .reads
            .iter()
            .any(|r| r.path == path && r.content_hash.as_deref() == Some(content_hash))
    }

    /// Content hash a pack entry was read at (kept on the ledger entry).
    fn entries_hash(&self, entry_id: &str) -> Option<&str> {
        self.entries
            .iter()
            .find(|e| e.entry_id == entry_id)
            .and_then(|e| e.content_hash.as_deref())
    }

    /// Whether `path` has a retrieval record (a pack entry or a direct read)
    /// at exactly `revision` (docs/28 §2: a stale-revision record does not count).
    #[must_use]
    pub fn has_record(&self, path: &str, revision: u64) -> bool {
        self.entries
            .iter()
            .any(|e| e.path == path && e.workspace_revision == revision)
            || self
                .reads
                .iter()
                .any(|r| r.path == path && r.workspace_revision == revision)
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
