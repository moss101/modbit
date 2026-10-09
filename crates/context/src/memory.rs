//! The memory segment of the Context Pack (PX-113; docs/18 "Context Pack",
//! docs/19 "Engineering Memory"): curated engineering memory selected for a
//! task enters the prompt as a labelled, budgeted segment, one entry per
//! item, each carrying the item's id and provenance (scope, source, author,
//! confidence, validation, age) so the Inspector can name what the model was
//! shown and why. Memory is prompt context, never recovery state
//! (MOD-STATE-001) and never authority: the entries are rendered as data.
//!
//! The packer takes candidates already ranked by relevance and scope
//! precedence (`modbit_memory::MemoryStore::select`, run by the Core) and
//! spends a token budget on them, best first, clipping an over-long entry
//! and counting what it left out — it never drops anything in silence.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::estimate_tokens;

/// Compiler version stamped on every memory pack.
pub const MEMORY_COMPILER_VERSION: &str = "memory-pack-v1";

/// The most tokens one entry may cost; a longer item is clipped and says so.
pub const MAX_ENTRY_TOKENS: u32 = 120;

/// The most entries a pack holds whatever the budget (a budget of tens of
/// thousands of tokens is not a reason to inject fifty rules).
pub const MAX_ENTRIES: usize = 24;

/// One selected memory item offered to the packer.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MemoryCandidate {
    /// The item's id (64 hex characters).
    pub memory_id: String,
    /// `kind:id` scope key.
    pub scope: String,
    /// Scope kind label (`user`, `agent_profile`, …).
    pub scope_kind: String,
    /// Record type label (`convention`, `decision`, …).
    pub record_type: String,
    /// The item's topic.
    pub topic: String,
    /// The item's content (untrusted-origin text).
    pub content: String,
    /// Where it came from (`user_stated`, …).
    pub source: String,
    /// Who recorded it.
    pub author: String,
    /// Confidence, `0.0..=1.0`.
    pub confidence: f32,
    /// When it was created (ms).
    pub created_at_ms: i64,
    /// When it expires (ms), if it does.
    pub expires_at_ms: Option<i64>,
    /// Whether an independent validation is recorded on it.
    pub validated: bool,
    /// The repository revision it was last validated against.
    pub last_validation_revision: Option<String>,
    /// Ranking score from the selection.
    pub score: f32,
    /// Why it was selected.
    pub reasons: Vec<String>,
    /// Curated items on the same scope, type and topic (unresolved).
    pub conflicts_with: Vec<String>,
}

/// The provenance an entry carries into the prompt and the Inspector.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MemoryProvenance {
    /// `kind:id` scope key.
    pub scope: String,
    /// Source label.
    pub source: String,
    /// Author label.
    pub author: String,
    /// Confidence.
    pub confidence: f32,
    /// Created (ms).
    pub created_at_ms: i64,
    /// Expiry (ms), if any.
    pub expires_at_ms: Option<i64>,
    /// Validated.
    pub validated: bool,
    /// Revision it was last validated against.
    pub last_validation_revision: Option<String>,
}

/// One packed entry.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MemoryEntry {
    /// Stable id of this entry (sha256 of the memory id and the packed text).
    pub entry_id: String,
    /// The memory item's id.
    pub memory_id: String,
    /// Scope kind label.
    pub scope_kind: String,
    /// Record type label.
    pub record_type: String,
    /// Topic.
    pub topic: String,
    /// The text the model sees (the content, clipped to the entry budget).
    pub text: String,
    /// Whether the content was clipped.
    pub clipped: bool,
    /// Provenance.
    pub provenance: MemoryProvenance,
    /// Why it was selected.
    pub reasons: Vec<String>,
    /// Unresolved same-key items.
    pub conflicts_with: Vec<String>,
    /// Score.
    pub score: f32,
    /// Estimated tokens of the entry as rendered.
    pub token_cost: u32,
}

/// The memory segment of one prompt.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MemoryPack {
    /// sha256 over the entry ids.
    pub pack_id: String,
    /// Entries, best first.
    pub entries: Vec<MemoryEntry>,
    /// Candidates that did not fit (count).
    pub omitted_count: usize,
    /// Their estimated tokens.
    pub omitted_tokens: u32,
    /// Ids of what did not fit (bounded).
    pub omitted_ids: Vec<String>,
    /// The budget.
    pub token_budget: u32,
    /// Tokens used.
    pub token_used: u32,
    /// Compiler version.
    pub compiler_version: String,
    /// Token estimator.
    pub token_estimator: String,
}

/// The cost of an entry as the prompt renders it: a header line naming the
/// item and its provenance, and the text. Kept in step with the compiler's
/// rendering by being derived from the same strings (`render_header`).
fn render_header(c: &MemoryCandidate) -> String {
    format!(
        "[memory {} scope={} type={} topic={:?} source={} author={} confidence={:.2} validated={}]",
        &c.memory_id[..c.memory_id.len().min(12)],
        c.scope,
        c.record_type,
        c.topic,
        c.source,
        c.author,
        c.confidence,
        c.validated,
    )
}

fn clip(text: &str, max_bytes: usize) -> (String, bool) {
    if text.len() <= max_bytes {
        return (text.to_owned(), false);
    }
    let mut cut = max_bytes;
    while !text.is_char_boundary(cut) {
        cut -= 1;
    }
    (
        format!("{}[+{} bytes clipped]", &text[..cut], text.len() - cut),
        true,
    )
}

/// Pack ranked candidates under `token_budget`: best score first (ties by
/// id, so the pack is deterministic), each entry clipped to
/// [`MAX_ENTRY_TOKENS`], until the budget or [`MAX_ENTRIES`] is spent.
#[must_use]
pub fn pack_memory(candidates: &[MemoryCandidate], token_budget: u32) -> MemoryPack {
    let mut order: Vec<&MemoryCandidate> = candidates.iter().collect();
    order.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.memory_id.cmp(&b.memory_id))
    });
    let mut entries: Vec<MemoryEntry> = Vec::new();
    let mut used = 0u32;
    let mut omitted_count = 0usize;
    let mut omitted_tokens = 0u32;
    let mut omitted_ids: Vec<String> = Vec::new();
    for c in order {
        let header = render_header(c);
        let header_tokens = estimate_tokens(&header);
        // Room for the clip marker (`[+N bytes clipped]`) inside the entry
        // budget.
        let body_budget = ((MAX_ENTRY_TOKENS.saturating_sub(header_tokens).max(16) as usize) * 4)
            .saturating_sub(28)
            .max(32);
        let (text, clipped) = clip(&c.content, body_budget);
        let cost = header_tokens + estimate_tokens(&text) + 1;
        if entries.len() >= MAX_ENTRIES || used + cost > token_budget {
            omitted_count += 1;
            omitted_tokens = omitted_tokens.saturating_add(cost);
            if omitted_ids.len() < 50 {
                omitted_ids.push(c.memory_id.clone());
            }
            continue;
        }
        used += cost;
        let mut h = Sha256::new();
        h.update(c.memory_id.as_bytes());
        h.update([0]);
        h.update(text.as_bytes());
        entries.push(MemoryEntry {
            entry_id: hex::encode(h.finalize()),
            memory_id: c.memory_id.clone(),
            scope_kind: c.scope_kind.clone(),
            record_type: c.record_type.clone(),
            topic: c.topic.clone(),
            text,
            clipped,
            provenance: MemoryProvenance {
                scope: c.scope.clone(),
                source: c.source.clone(),
                author: c.author.clone(),
                confidence: c.confidence,
                created_at_ms: c.created_at_ms,
                expires_at_ms: c.expires_at_ms,
                validated: c.validated,
                last_validation_revision: c.last_validation_revision.clone(),
            },
            reasons: c.reasons.clone(),
            conflicts_with: c.conflicts_with.clone(),
            score: c.score,
            token_cost: cost,
        });
    }
    let mut ids = Sha256::new();
    for e in &entries {
        ids.update(e.entry_id.as_bytes());
        ids.update([0]);
    }
    MemoryPack {
        pack_id: hex::encode(ids.finalize()),
        entries,
        omitted_count,
        omitted_tokens,
        omitted_ids,
        token_budget,
        token_used: used,
        compiler_version: MEMORY_COMPILER_VERSION.into(),
        token_estimator: crate::TOKEN_ESTIMATOR.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cand(n: usize, score: f32, content: &str) -> MemoryCandidate {
        MemoryCandidate {
            memory_id: format!("{n:064x}"),
            scope: "user:u".into(),
            scope_kind: "user".into(),
            record_type: "convention".into(),
            topic: format!("topic {n}"),
            content: content.into(),
            source: "user_stated".into(),
            author: "user:u".into(),
            confidence: 0.9,
            created_at_ms: 1,
            validated: true,
            score,
            ..Default::default()
        }
    }

    #[test]
    fn fifty_memories_stay_inside_the_budget_and_the_rest_is_counted() {
        let cands: Vec<MemoryCandidate> = (0..50)
            .map(|n| cand(n, 1.0 - n as f32 / 100.0, &"x".repeat(200)))
            .collect();
        let pack = pack_memory(&cands, 400);
        assert!(pack.token_used <= 400, "{}", pack.token_used);
        assert_eq!(pack.entries.len() + pack.omitted_count, 50);
        assert!(!pack.entries.is_empty() && pack.omitted_count > 0);
        // Best first.
        assert_eq!(pack.entries[0].memory_id, cands[0].memory_id);
        let sum: u32 = pack.entries.iter().map(|e| e.token_cost).sum();
        assert_eq!(sum, pack.token_used);
    }

    #[test]
    fn a_long_item_is_clipped_with_a_marker_and_provenance_survives() {
        let pack = pack_memory(&[cand(1, 0.5, &"y".repeat(5_000))], 10_000);
        let e = &pack.entries[0];
        assert!(e.clipped && e.text.contains("bytes clipped"), "{}", e.text);
        assert!(e.token_cost <= MAX_ENTRY_TOKENS + 2, "{}", e.token_cost);
        assert_eq!(e.provenance.source, "user_stated");
        assert_eq!(e.memory_id.len(), 64);
    }

    #[test]
    fn the_pack_is_deterministic_and_capped_by_entry_count() {
        let cands: Vec<MemoryCandidate> = (0..40).map(|n| cand(n, 0.5, "short")).collect();
        let a = pack_memory(&cands, 100_000);
        let mut rev = cands.clone();
        rev.reverse();
        let b = pack_memory(&rev, 100_000);
        assert_eq!(a.pack_id, b.pack_id);
        assert_eq!(a.entries.len(), MAX_ENTRIES);
        assert_eq!(a.omitted_count, 40 - MAX_ENTRIES);
    }
}
