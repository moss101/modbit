//! Full-text search over conversations (REQ-PX-042; docs/65 AFW-B10):
//! `SearchConversations`.
//!
//! The index is a projection of the canonical event log and nothing more. A
//! conversation's rows are the atoms `transcript` folds out of its log
//! (user messages, completed assistant messages, tool cards with their result
//! text, approval and question cards), so a hit names a row id that
//! `GetTranscript` serves in every density. They are tokenised into an
//! in-memory inverted index that
//!
//! * is **rebuilt from the log**: a conversation is (re)indexed whenever it has
//!   no index or the log has moved past the offset its index was folded to,
//!   so a restarted Core, a deleted cache and a never-used Core all arrive at
//!   the same index for the same log (the results carry a digest that says
//!   so);
//! * is **bounded**: a row's indexed text, a conversation's indexed text and
//!   the whole index have byte limits (`MODBIT_SEARCH_ROW_BYTES`,
//!   `MODBIT_SEARCH_TASK_BYTES`, `MODBIT_SEARCH_INDEX_BYTES`); past the total
//!   the least recently searched conversations leave the index and are
//!   rebuilt when next needed; the query, the terms, the page and the
//!   snippets are bounded too;
//! * is **scoped**: only the conversations of the session the command names
//!   are searched, a conversation of another session is refused, never
//!   answered, and the index keys every conversation by its task;
//! * is **data**: a model's text, a tool's output and a repository's words
//!   are indexed as text and returned as plain-text snippets with their row
//!   ids. Nothing in them is read as an instruction, and a search appends
//!   nothing to the log.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Mutex;

use modbit_domain::task::Task;
use modbit_domain::{SessionId, TaskId};
use modbit_protocol::v1 as wire;
use sha2::{Digest, Sha256};

use crate::server::Core;
use crate::transcript::Refusal;

const DEFAULT_PAGE: usize = 20;
const MAX_PAGE: usize = 100;
const DEFAULT_SNIPPETS: usize = 3;
const MAX_SNIPPETS: usize = 10;
const MAX_QUERY_CHARS: usize = 256;
const MAX_TERMS: usize = 16;
const MAX_TOKEN_CHARS: usize = 64;
const SNIPPET_CHARS: usize = 240;
/// Characters of context before the first match in a snippet.
const SNIPPET_LEAD: usize = 80;
const DEFAULT_ROW_BYTES: usize = 32 * 1024;
const DEFAULT_TASK_BYTES: usize = 2 * 1024 * 1024;
const DEFAULT_INDEX_BYTES: usize = 32 * 1024 * 1024;

fn env_bytes(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|v| v.trim().parse::<usize>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(default)
}

/// One indexed transcript row.
struct IndexedRow {
    row_id: String,
    kind: i32,
    source: wire::SnippetSource,
    offset: u64,
    turn_id: String,
    text: String,
}

/// The index of one conversation.
struct TaskIndex {
    session: SessionId,
    /// The offset of the conversation's latest event when it was folded.
    as_of: u64,
    rows: Vec<IndexedRow>,
    /// Token -> the rows (by position) it occurs in, ascending.
    postings: BTreeMap<String, Vec<u32>>,
    bytes: usize,
    truncated: bool,
    digest: String,
    used: u64,
}

#[derive(Default)]
struct Inner {
    tasks: HashMap<TaskId, TaskIndex>,
    bytes: usize,
    clock: u64,
}

/// The Core's conversation index: derived, bounded, rebuilt from the log.
pub(crate) struct Index {
    inner: Mutex<Inner>,
    row_bytes: usize,
    task_bytes: usize,
    budget: usize,
}

impl Index {
    pub(crate) fn from_env() -> Self {
        Self {
            inner: Mutex::new(Inner::default()),
            row_bytes: env_bytes("MODBIT_SEARCH_ROW_BYTES", DEFAULT_ROW_BYTES),
            task_bytes: env_bytes("MODBIT_SEARCH_TASK_BYTES", DEFAULT_TASK_BYTES),
            budget: env_bytes("MODBIT_SEARCH_INDEX_BYTES", DEFAULT_INDEX_BYTES),
        }
    }
}

// ------------------------------------------------------------------ tokens

/// The searchable words of `text`: maximal runs of alphanumeric characters,
/// lowercased, with their byte range in `text`.
fn tokens(text: &str) -> Vec<(usize, usize, String)> {
    let mut out = Vec::new();
    let mut start: Option<usize> = None;
    for (i, c) in text.char_indices() {
        if c.is_alphanumeric() {
            if start.is_none() {
                start = Some(i);
            }
        } else if let Some(s) = start.take() {
            out.push((s, i, normal(&text[s..i])));
        }
    }
    if let Some(s) = start {
        out.push((s, text.len(), normal(&text[s..])));
    }
    out
}

fn normal(word: &str) -> String {
    word.chars()
        .take(MAX_TOKEN_CHARS)
        .flat_map(char::to_lowercase)
        .collect()
}

/// A parsed query: the words, the phrases (each a run of words that must
/// appear together, in order) and whether the last word also matches as a
/// prefix.
struct Query {
    terms: Vec<String>,
    phrases: Vec<Vec<String>>,
    /// The last bare word of the query (not inside a phrase) matches as a
    /// prefix: a person is typing it.
    prefix: Option<String>,
}

impl Query {
    fn parse(text: &str) -> Result<Self, Refusal> {
        if text.chars().count() > MAX_QUERY_CHARS {
            return Err((
                "BAD_PAYLOAD",
                format!("a query is at most {MAX_QUERY_CHARS} characters"),
            ));
        }
        let mut terms: Vec<String> = Vec::new();
        let mut phrases: Vec<Vec<String>> = Vec::new();
        let mut last_bare: Option<String> = None;
        let mut rest = text;
        let mut in_quote = false;
        loop {
            let (chunk, next) = match rest.find('"') {
                Some(i) => (&rest[..i], Some(&rest[i + 1..])),
                None => (rest, None),
            };
            let words: Vec<String> = tokens(chunk).into_iter().map(|t| t.2).collect();
            if in_quote {
                if !words.is_empty() {
                    terms.extend(words.iter().cloned());
                    phrases.push(words);
                }
            } else {
                if let Some(w) = words.last() {
                    last_bare = Some(w.clone());
                }
                terms.extend(words);
            }
            match next {
                Some(n) => {
                    rest = n;
                    in_quote = !in_quote;
                }
                None => break,
            }
        }
        if terms.is_empty() {
            return Err((
                "BAD_PAYLOAD",
                "the query has no searchable word (letters or digits)".into(),
            ));
        }
        if terms.len() > MAX_TERMS {
            return Err((
                "BAD_PAYLOAD",
                format!("a query has at most {MAX_TERMS} words"),
            ));
        }
        // The prefix is the very last word of the query, and only when it was
        // typed outside quotes.
        let prefix =
            last_bare.filter(|w| terms.last() == Some(w) && !text.trim_end().ends_with('"'));
        Ok(Self {
            terms,
            phrases,
            prefix,
        })
    }

    fn normalised(&self) -> String {
        let mut out = self.terms.join(" ");
        for p in &self.phrases {
            out.push_str(&format!(" \"{}\"", p.join(" ")));
        }
        out
    }

    /// Whether the token `t` is one of the query's words.
    fn is_term(&self, t: &str) -> bool {
        self.terms.iter().any(|x| x == t)
            || self.prefix.as_deref().is_some_and(|p| t.starts_with(p))
    }
}

// ------------------------------------------------------------------- build

fn source_of(kind: wire::TranscriptRowKind) -> wire::SnippetSource {
    match kind {
        wire::TranscriptRowKind::UserMessage => wire::SnippetSource::User,
        wire::TranscriptRowKind::AssistantMessage => wire::SnippetSource::Assistant,
        wire::TranscriptRowKind::ToolCard => wire::SnippetSource::Tool,
        wire::TranscriptRowKind::ApprovalCard => wire::SnippetSource::Approval,
        _ => wire::SnippetSource::Unspecified,
    }
}

fn object_text(store: &modbit_event_store::EventStore, hash: &str, max: usize) -> Option<String> {
    if hash.is_empty() {
        return None;
    }
    let (data, _, _) = store.objects().read_range(hash, 0, max as u64).ok()?;
    Some(String::from_utf8_lossy(&data).into_owned())
}

fn cut(mut s: String, max: usize) -> String {
    if s.len() > max {
        let mut end = max;
        while !s.is_char_boundary(end) {
            end -= 1;
        }
        s.truncate(end);
    }
    s
}

impl Index {
    /// Fold `task`'s log into an index, bounded. Pure of the cache: the same
    /// log always gives the same index (and the same digest).
    fn build(
        &self,
        store: &modbit_event_store::EventStore,
        task: &Task,
        as_of: u64,
    ) -> Result<TaskIndex, Refusal> {
        let atoms = crate::transcript::atoms(store, task)?;
        let mut rows: Vec<IndexedRow> = Vec::new();
        let mut text_bytes = 0usize;
        let mut truncated = false;
        for a in atoms {
            let Ok(kind) = wire::TranscriptRowKind::try_from(a.kind) else {
                continue;
            };
            let source = source_of(kind);
            if source == wire::SnippetSource::Unspecified {
                continue;
            }
            if !a.hints.as_ref().is_some_and(|h| h.renderable) {
                continue;
            }
            // The row's whole text where the transcript row carries only a
            // preview: a completed message, a tool's result.
            let mut text = a.text.clone();
            if kind == wire::TranscriptRowKind::AssistantMessage
                && !a.text_ref.is_empty()
                && let Some(full) = object_text(store, &a.text_ref, self.row_bytes)
            {
                text = full;
            }
            if let Some(wire::transcript_row::Facts::Tool(t)) = &a.facts
                && let Some(result) = object_text(store, &t.result_ref, self.row_bytes)
                && !result.trim().is_empty()
            {
                text.push('\n');
                text.push_str(&result);
            }
            let text = cut(text, self.row_bytes);
            if text.trim().is_empty() {
                continue;
            }
            if text_bytes + text.len() > self.task_bytes {
                truncated = true;
                break;
            }
            text_bytes += text.len();
            rows.push(IndexedRow {
                row_id: a.row_id,
                kind: a.kind,
                source,
                offset: a.offset,
                turn_id: a.turn_id,
                text,
            });
        }
        let mut postings: BTreeMap<String, Vec<u32>> = BTreeMap::new();
        for (i, r) in rows.iter().enumerate() {
            for (_, _, t) in tokens(&r.text) {
                let list = postings.entry(t).or_default();
                if list.last() != Some(&(i as u32)) {
                    list.push(i as u32);
                }
            }
        }
        let mut h = Sha256::new();
        let mut bytes = 64usize;
        for r in &rows {
            h.update(r.row_id.as_bytes());
            h.update([0]);
            h.update(r.offset.to_be_bytes());
            h.update(r.kind.to_be_bytes());
            h.update(r.text.as_bytes());
            h.update([0]);
            bytes += r.row_id.len() + r.text.len() + r.turn_id.len() + 64;
        }
        for (t, list) in &postings {
            h.update(t.as_bytes());
            h.update([0]);
            for n in list {
                h.update(n.to_be_bytes());
            }
            bytes += t.len() + 48 + list.len() * 4;
        }
        Ok(TaskIndex {
            session: task.session_id,
            as_of,
            rows,
            postings,
            bytes,
            truncated,
            digest: hex::encode(h.finalize()),
            used: 0,
        })
    }
}

// ------------------------------------------------------------------ search

/// Rows of `ix` that satisfy `q`: every word present (the last as a prefix
/// when it was typed bare) and every phrase in sequence.
fn matching_rows(ix: &TaskIndex, q: &Query) -> Vec<u32> {
    let mut acc: Option<BTreeSet<u32>> = None;
    for term in &q.terms {
        let is_prefix = q.prefix.as_deref() == Some(term.as_str()) && q.terms.last() == Some(term);
        let rows: BTreeSet<u32> = if is_prefix {
            ix.postings
                .range(term.clone()..)
                .take_while(|(t, _)| t.starts_with(term.as_str()))
                .flat_map(|(_, v)| v.iter().copied())
                .collect()
        } else {
            ix.postings
                .get(term)
                .map(|v| v.iter().copied().collect())
                .unwrap_or_default()
        };
        acc = Some(match acc {
            None => rows,
            Some(prev) => prev.intersection(&rows).copied().collect(),
        });
        if acc.as_ref().is_some_and(BTreeSet::is_empty) {
            return vec![];
        }
    }
    let candidates = acc.unwrap_or_default();
    candidates
        .into_iter()
        .filter(|i| {
            if q.phrases.is_empty() {
                return true;
            }
            let toks: Vec<String> = tokens(&ix.rows[*i as usize].text)
                .into_iter()
                .map(|t| t.2)
                .collect();
            q.phrases
                .iter()
                .all(|p| toks.windows(p.len()).any(|w| w == p.as_slice()))
        })
        .collect()
}

/// The matched ranges of a row's text: its query words, and each phrase as
/// one range from its first word to its last.
fn ranges_of(text: &str, q: &Query) -> (Vec<(usize, usize)>, u32) {
    let toks = tokens(text);
    let mut spans: Vec<(usize, usize)> = Vec::new();
    let mut occurrences = 0u32;
    for (s, e, t) in &toks {
        if q.is_term(t) {
            spans.push((*s, *e));
            occurrences += 1;
        }
    }
    for p in &q.phrases {
        for (i, w) in toks.windows(p.len()).enumerate() {
            if w.iter().map(|t| &t.2).eq(p.iter()) {
                spans.push((toks[i].0, toks[i + p.len() - 1].1));
                occurrences += 1;
            }
        }
    }
    spans.sort_unstable();
    let mut ranges: Vec<(usize, usize)> = Vec::new();
    for (s, e) in spans {
        match ranges.last_mut() {
            Some(last) if s <= last.1 => last.1 = last.1.max(e),
            _ => ranges.push((s, e)),
        }
    }
    (ranges, occurrences)
}

fn snippet_of(row: &IndexedRow, q: &Query) -> wire::ConversationSnippet {
    let (ranges, occurrences) = ranges_of(&row.text, q);
    let first = ranges.first().map_or(0, |r| r.0);
    // The window: SNIPPET_LEAD characters before the first match, then up to
    // SNIPPET_CHARS characters, on character boundaries.
    let chars: Vec<(usize, char)> = row.text.char_indices().collect();
    let first_ci = chars.iter().position(|(b, _)| *b >= first).unwrap_or(0);
    let start_ci = first_ci.saturating_sub(SNIPPET_LEAD);
    let end_ci = (start_ci + SNIPPET_CHARS).min(chars.len());
    let start_b = chars.get(start_ci).map_or(row.text.len(), |c| c.0);
    let end_b = chars.get(end_ci).map_or(row.text.len(), |c| c.0);
    // Control characters become spaces of the same byte length, so byte
    // ranges stay valid and no snippet carries a terminal escape.
    let mut text = String::with_capacity(end_b - start_b);
    for c in row.text[start_b..end_b].chars() {
        if c.is_control() {
            text.extend(std::iter::repeat_n(' ', c.len_utf8()));
        } else {
            text.push(c);
        }
    }
    let matches = ranges
        .iter()
        .filter(|(s, e)| *s >= start_b && *e <= end_b)
        .map(|(s, e)| wire::MatchRange {
            start: (s - start_b) as u32,
            end: (e - start_b) as u32,
        })
        .collect();
    wire::ConversationSnippet {
        row_id: row.row_id.clone(),
        kind: row.kind,
        source: row.source as i32,
        offset: row.offset,
        turn_id: row.turn_id.clone(),
        text,
        cut_before: start_b > 0,
        cut_after: end_b < row.text.len(),
        matches,
        score: occurrences,
    }
}

/// Run `req` over the session's conversations.
///
/// # Errors
/// `BAD_PAYLOAD`, `UNKNOWN_SESSION`, `UNKNOWN_TASK`, `WRONG_SESSION`.
pub(crate) async fn search(
    core: &Core,
    req: &wire::SearchConversations,
    envelope_session: Option<[u8; 16]>,
) -> Result<wire::ConversationSearchResults, Refusal> {
    let Some(session) = req
        .session_id
        .as_ref()
        .and_then(crate::server::id16)
        .map(SessionId::from_bytes)
    else {
        return Err(("BAD_PAYLOAD", "session_id required".into()));
    };
    if let Some(named) = envelope_session
        && named != *session.as_bytes()
    {
        return Err((
            "WRONG_SESSION",
            "the command names a different session than the search".into(),
        ));
    }
    let q = Query::parse(&req.query)?;
    let page = match req.limit as usize {
        0 => DEFAULT_PAGE,
        n => n.min(MAX_PAGE),
    };
    let max_snippets = match req.max_snippets as usize {
        0 => DEFAULT_SNIPPETS,
        n => n.min(MAX_SNIPPETS),
    };
    let only = match req.task_id.as_ref().filter(|i| !i.value.is_empty()) {
        Some(i) => Some(TaskId::from_bytes(
            crate::server::id16(i).ok_or(("BAD_PAYLOAD", "task_id must be 16 bytes".to_owned()))?,
        )),
        None => None,
    };
    let (tasks, digests, tip) = {
        let store = core.store.lock().await;
        match store.session(&session) {
            Ok(Some(_)) => {}
            Ok(None) => return Err(("UNKNOWN_SESSION", session.to_string())),
            Err(e) => return Err(("STORE_ERROR", e.to_string())),
        }
        let tasks = store
            .session_tasks(&session)
            .map_err(|e| ("STORE_ERROR", e.to_string()))?;
        let digests: HashMap<TaskId, modbit_event_store::digest::TaskDigest> = store
            .task_digests(&session)
            .map_err(|e| ("STORE_ERROR", e.to_string()))?
            .into_iter()
            .map(|d| (d.task_id, d))
            .collect();
        (tasks, digests, store.last_offset().unwrap_or(0))
    };
    if let Some(t) = only {
        match core.store.lock().await.task(&t) {
            Ok(Some(task)) if task.session_id != session => {
                return Err((
                    "WRONG_SESSION",
                    "the task belongs to another session; a conversation is searched inside its own"
                        .into(),
                ));
            }
            Ok(Some(_)) => {}
            Ok(None) => return Err(("UNKNOWN_TASK", t.to_string())),
            Err(e) => return Err(("STORE_ERROR", e.to_string())),
        }
    }
    let mut hits: Vec<wire::ConversationHit> = Vec::new();
    let mut considered = 0u32;
    let mut rebuilt = 0u32;
    let mut truncated_tasks = 0u32;
    let mut rows_seen = 0u32;
    let mut digest_parts: Vec<(String, String)> = Vec::new();
    for task in tasks {
        if only.is_some_and(|t| t != task.task_id) {
            continue;
        }
        let Some(d) = digests.get(&task.task_id) else {
            continue;
        };
        if d.archived && !req.include_archived {
            continue;
        }
        considered += 1;
        // The conversation's index: the cached one when the log has not moved
        // past it, else rebuilt from the log (store held only for the read).
        let cached = {
            let mut inner = core.conversation_index.inner.lock().expect("index");
            inner.clock += 1;
            let clock = inner.clock;
            match inner.tasks.get_mut(&task.task_id) {
                Some(ix) if ix.as_of == d.last_offset && ix.session == session => {
                    ix.used = clock;
                    true
                }
                _ => false,
            }
        };
        if !cached {
            let fresh = {
                let store = core.store.lock().await;
                core.conversation_index
                    .build(&store, &task, d.last_offset)?
            };
            rebuilt += 1;
            let mut inner = core.conversation_index.inner.lock().expect("index");
            if let Some(old) = inner.tasks.remove(&task.task_id) {
                inner.bytes -= old.bytes;
            }
            inner.clock += 1;
            let mut fresh = fresh;
            fresh.used = inner.clock;
            inner.bytes += fresh.bytes;
            inner.tasks.insert(task.task_id, fresh);
            // Past the budget the least recently used conversations leave
            // (never the one just built).
            while inner.bytes > core.conversation_index.budget {
                let victim = inner
                    .tasks
                    .iter()
                    .filter(|(id, _)| **id != task.task_id)
                    .min_by_key(|(_, ix)| ix.used)
                    .map(|(id, _)| *id);
                let Some(v) = victim else { break };
                if let Some(old) = inner.tasks.remove(&v) {
                    inner.bytes -= old.bytes;
                }
            }
        }
        let title = crate::transcript::title_of(&task);
        let title_toks: Vec<String> = tokens(&title).into_iter().map(|t| t.2).collect();
        let title_matched = q.terms.iter().all(|t| {
            title_toks.iter().any(|x| {
                x == t || (q.prefix.as_deref() == Some(t.as_str()) && x.starts_with(t.as_str()))
            })
        }) && q
            .phrases
            .iter()
            .all(|p| title_toks.windows(p.len()).any(|w| w == p.as_slice()));
        let (matched, snippets, score, tdigest, was_truncated) = {
            let inner = core.conversation_index.inner.lock().expect("index");
            let Some(ix) = inner.tasks.get(&task.task_id) else {
                continue;
            };
            rows_seen += ix.rows.len() as u32;
            let rows = matching_rows(ix, &q);
            let mut scored: Vec<(u32, u32)> = rows
                .iter()
                .map(|i| (ranges_of(&ix.rows[*i as usize].text, &q).1, *i))
                .collect();
            // Best rows first; ties keep log order.
            scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
            let score = scored.iter().map(|s| s.0.min(5)).sum::<u32>();
            let snippets: Vec<wire::ConversationSnippet> = scored
                .iter()
                .take(max_snippets)
                .map(|(_, i)| snippet_of(&ix.rows[*i as usize], &q))
                .collect();
            (
                rows.len() as u32,
                snippets,
                score,
                ix.digest.clone(),
                ix.truncated,
            )
        };
        digest_parts.push((task.task_id.to_string(), tdigest));
        if was_truncated {
            truncated_tasks += 1;
        }
        if matched == 0 && !title_matched {
            continue;
        }
        hits.push(wire::ConversationHit {
            task_id: Some(crate::server::wire_id(task.task_id.as_bytes())),
            title,
            status_class: 0,
            status_label: String::new(),
            archived: d.archived,
            title_matched,
            matched_rows: matched,
            score: score + if title_matched { 100 } else { 0 },
            last_offset: d.last_offset,
            snippets,
        });
    }
    hits.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then(b.last_offset.cmp(&a.last_offset))
    });
    let total = hits.len();
    hits.truncate(page);
    // The status class is the header projection's (AFW-B02), never computed
    // here: a hit shows the class the agent list shows.
    {
        let store = core.store.lock().await;
        let headers = crate::transcript::headers(core, &store, session, true)?;
        for hit in &mut hits {
            let Some(h) = headers.headers.iter().find(|h| h.task_id == hit.task_id) else {
                continue;
            };
            hit.status_class = h.status_class;
            hit.status_label.clone_from(&h.status_label);
        }
    }
    digest_parts.sort();
    let mut h = Sha256::new();
    for (t, d) in &digest_parts {
        h.update(t.as_bytes());
        h.update(d.as_bytes());
    }
    let index_bytes = core.conversation_index.inner.lock().expect("index").bytes as u64;
    Ok(wire::ConversationSearchResults {
        session_id: req.session_id.clone(),
        query: q.normalised(),
        hits,
        total_hits: total as u32,
        has_more: total > page,
        tasks_considered: considered,
        tasks_rebuilt: rebuilt,
        rows_indexed: rows_seen,
        index_bytes,
        index_budget_bytes: core.conversation_index.budget as u64,
        tasks_truncated: truncated_tasks,
        index_digest: hex::encode(h.finalize()),
        as_of_offset: tip,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_query_is_words_phrases_and_a_typed_prefix() {
        let q = Query::parse("Fix the \"null pointer\" crash").unwrap();
        assert_eq!(q.terms, ["fix", "the", "null", "pointer", "crash"]);
        assert_eq!(q.phrases, [vec!["null".to_owned(), "pointer".to_owned()]]);
        assert_eq!(q.prefix.as_deref(), Some("crash"));
        let q = Query::parse("\"null pointer\"").unwrap();
        assert_eq!(q.prefix, None, "a closed phrase is not a prefix");
        assert!(Query::parse("   ... ").is_err());
        assert!(Query::parse(&"a ".repeat(20)).is_err());
        assert!(Query::parse(&"a".repeat(300)).is_err());
    }

    #[test]
    fn matches_are_byte_ranges_of_the_snippet_and_controls_are_blanked() {
        let row = IndexedRow {
            row_id: "msg:1".into(),
            kind: wire::TranscriptRowKind::AssistantMessage as i32,
            source: wire::SnippetSource::Assistant,
            offset: 7,
            turn_id: String::new(),
            text: format!("{}needle\u{1b}[31m red tail", "x ".repeat(200)),
        };
        let q = Query::parse("needle").unwrap();
        let s = snippet_of(&row, &q);
        assert!(s.cut_before && !s.cut_after);
        assert!(!s.text.contains('\u{1b}'));
        let m = &s.matches[0];
        assert_eq!(&s.text[m.start as usize..m.end as usize], "needle");
        assert!(s.text.chars().count() <= SNIPPET_CHARS);
    }
}
