//! BM25 lexical index (docs/18 "Concrete local indexing stack": Tantivy
//! index per repository snapshot family; M3.2). One document per line-window
//! chunk of each searchable file of the exact index, so a hit names the
//! region that matched (FIX-12, audit N6) and not just the file; the default
//! tokenizer (split on non-alphanumerics, lower-case) so identifiers such as
//! `compute_total` match `compute` and `total`. Held in memory per Core
//! process and refreshed per changed path.
//!
//! Query semantics (FIX-12, audit N4). A bare keyword query is precise: its
//! terms are AND-ed, and a short query (three content terms or fewer) that
//! has any conjunctive hit returns only those. Anything else is a
//! natural-language question, which an AND over every word answers with
//! nothing ("where do we compute the total of the cart"): it runs as an OR
//! of the content terms, with the stopword list below removed, identifier-like
//! terms weighted up, and the conjunction kept as a boost so the chunks that
//! hold every term still rank first. Explicit Tantivy syntax (quotes, `+`,
//! `-`, parentheses, `field:`, `AND`/`OR`/`NOT`) is passed through as the
//! caller wrote it.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use tantivy::collector::TopDocs;
use tantivy::query::{BooleanQuery, BoostQuery, Occur, Query, QueryParser, TermQuery};
use tantivy::schema::{Field, IndexRecordOption, STORED, STRING, Schema, TEXT, Value};
use tantivy::{Index, IndexReader, IndexWriter, TantivyDocument, Term};

/// A changed document for `refresh`: path and, when it still exists, its
/// text and language (`None` removes the document).
pub type ChangedDoc = (String, Option<(String, Option<String>)>);

/// Lines per indexed chunk.
pub const CHUNK_LINES: usize = 40;
/// Lines between the starts of two consecutive chunks (the overlap keeps a
/// match that straddles a boundary whole in at least one chunk).
pub const CHUNK_STRIDE: usize = 30;
/// A keyword query of at most this many content terms keeps its conjunctive
/// answer when it has one.
const PRECISE_TERMS: usize = 3;
/// A query of at least this many content terms must match at least two of
/// them in a chunk, so one common word cannot fill the list.
const MIN_TERMS_FOR_MATCH_FLOOR: usize = 4;
/// Weight of a term that came from an identifier-looking word.
const IDENTIFIER_WEIGHT: f32 = 2.0;
/// Weight of the conjunction clause that lifts chunks holding every term.
const CONJUNCTION_BOOST: f32 = 2.0;

/// Words dropped from a natural-language query before it is matched: English
/// function words that carry no retrieval signal and appear in every
/// comment. Deliberately small and free of code vocabulary (`not`, `use`,
/// `return`, `get`, `test` and `error` are searchable).
pub const STOPWORDS: &[&str] = &[
    "a", "an", "and", "are", "as", "at", "be", "been", "being", "by", "can", "could", "did", "do",
    "does", "for", "from", "had", "has", "have", "how", "i", "if", "in", "into", "is", "it", "its",
    "me", "my", "of", "on", "or", "our", "should", "so", "than", "that", "the", "their", "then",
    "there", "these", "this", "those", "to", "us", "was", "we", "were", "what", "when", "where",
    "which", "who", "whom", "whose", "why", "will", "with", "would", "you", "your",
];

/// One chunk of a file: a line window with its byte span.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LineChunk<'a> {
    /// 1-based inclusive line range.
    pub lines: (u32, u32),
    /// Byte span in the file.
    pub span: (u64, u64),
    /// The chunk text.
    pub text: &'a str,
}

/// Split a file into overlapping line windows (`CHUNK_LINES` lines, a new
/// window every `CHUNK_STRIDE` lines). A file of at most `CHUNK_LINES` lines
/// is one chunk covering the whole file.
#[must_use]
pub fn line_chunks(text: &str) -> Vec<LineChunk<'_>> {
    let mut starts: Vec<usize> = Vec::new();
    let mut offset = 0usize;
    for line in text.split_inclusive('\n') {
        starts.push(offset);
        offset += line.len();
    }
    let n = starts.len();
    if n == 0 {
        return vec![LineChunk {
            lines: (1, 1),
            span: (0, 0),
            text,
        }];
    }
    let mut out = Vec::new();
    let mut first = 0usize;
    loop {
        let last = (first + CHUNK_LINES).min(n);
        let from = starts[first];
        let to = if last == n { text.len() } else { starts[last] };
        out.push(LineChunk {
            lines: (
                u32::try_from(first + 1).unwrap_or(u32::MAX),
                u32::try_from(last).unwrap_or(u32::MAX),
            ),
            span: (from as u64, to as u64),
            text: &text[from..to],
        });
        if last == n {
            break;
        }
        first += CHUNK_STRIDE;
    }
    out
}

/// One ranked lexical hit: the best-matching chunk of a file.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LexicalHit {
    /// Root-relative path.
    pub path: String,
    /// BM25 score of the best chunk.
    pub score: f32,
    /// Language label, if known.
    pub language: Option<String>,
    /// Index revision the document was indexed at.
    pub index_revision: u64,
    /// 1-based line range of the matching chunk; `None` when the chunk is
    /// the whole file.
    #[serde(default)]
    pub lines: Option<(u32, u32)>,
    /// Byte span of the matching chunk; `None` when it is the whole file.
    #[serde(default)]
    pub span: Option<(u64, u64)>,
}

/// The BM25 index of one workspace.
pub struct LexicalIndex {
    index: Index,
    writer: IndexWriter,
    reader: IndexReader,
    path: Field,
    text: Field,
    language: Field,
    start_line: Field,
    end_line: Field,
    start_byte: Field,
    end_byte: Field,
    chunks: Field,
    revision: u64,
    docs: BTreeMap<String, u64>,
}

impl std::fmt::Debug for LexicalIndex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LexicalIndex")
            .field("revision", &self.revision)
            .field("docs", &self.docs.len())
            .finish()
    }
}

/// A query after analysis.
enum Plan {
    /// Explicit query syntax: handed to the parser as written.
    Raw(String),
    /// A keyword or natural-language query.
    Natural {
        /// The words that survived the stopword list, as the caller wrote
        /// them (an identifier stays whole for the conjunctive parse).
        cleaned: String,
        /// The query is one word: a name being looked up, never a question.
        single_word: bool,
        /// Distinct content terms with their weights.
        terms: Vec<(String, f32)>,
    },
}

fn has_syntax(query: &str) -> bool {
    query.chars().any(|c| {
        matches!(
            c,
            '"' | '(' | ')' | '^' | '~' | '[' | ']' | '{' | '}' | '\\'
        )
    }) || query.split_whitespace().any(|t| {
        (t.len() > 1 && t.starts_with(['+', '-']))
            || matches!(t, "AND" | "OR" | "NOT")
            || ["path:", "text:", "language:"]
                .iter()
                .any(|field| t.starts_with(field))
    })
}

/// Whether a word looks like an identifier or a path (`compute_total`,
/// `core.ready`, `a::b`, `EvidenceGraph`): its terms are the query's anchors.
fn identifier_like(word: &str) -> bool {
    word.contains('_')
        || word.contains("::")
        || word.contains('/')
        || word.trim_matches('.').contains('.')
        || word
            .chars()
            .zip(word.chars().skip(1))
            .any(|(a, b)| a.is_lowercase() && b.is_uppercase())
}

impl LexicalIndex {
    /// Build from `(path, text, language)` triples at `revision`.
    pub fn build<'a>(
        files: impl Iterator<Item = (&'a str, &'a str, Option<&'a str>)>,
        revision: u64,
    ) -> tantivy::Result<Self> {
        let mut sb = Schema::builder();
        let path = sb.add_text_field("path", STRING | STORED);
        let text = sb.add_text_field("text", TEXT);
        let language = sb.add_text_field("language", STRING | STORED);
        let start_line = sb.add_u64_field("start_line", STORED);
        let end_line = sb.add_u64_field("end_line", STORED);
        let start_byte = sb.add_u64_field("start_byte", STORED);
        let end_byte = sb.add_u64_field("end_byte", STORED);
        let chunks = sb.add_u64_field("chunks", STORED);
        let index = Index::create_in_ram(sb.build());
        let writer: IndexWriter = index.writer(15_000_000)?;
        let reader = index.reader()?;
        let mut lx = Self {
            index,
            writer,
            reader,
            path,
            text,
            language,
            start_line,
            end_line,
            start_byte,
            end_byte,
            chunks,
            revision,
            docs: BTreeMap::new(),
        };
        for (p, t, l) in files {
            lx.add(p, t, l, revision)?;
        }
        lx.writer.commit()?;
        lx.reader.reload()?;
        Ok(lx)
    }

    /// Index revision.
    #[must_use]
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Files indexed.
    #[must_use]
    pub fn len(&self) -> usize {
        self.docs.len()
    }

    /// Whether the index is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.docs.is_empty()
    }

    fn add(&mut self, p: &str, t: &str, l: Option<&str>, revision: u64) -> tantivy::Result<()> {
        // Every chunk of the path shares the path term, so one delete clears
        // all of them.
        self.writer.delete_term(Term::from_field_text(self.path, p));
        let chunks = line_chunks(t);
        let count = chunks.len() as u64;
        for c in chunks {
            let mut doc = TantivyDocument::default();
            doc.add_text(self.path, p);
            doc.add_text(self.text, c.text);
            doc.add_text(self.language, l.unwrap_or(""));
            doc.add_u64(self.start_line, u64::from(c.lines.0));
            doc.add_u64(self.end_line, u64::from(c.lines.1));
            doc.add_u64(self.start_byte, c.span.0);
            doc.add_u64(self.end_byte, c.span.1);
            doc.add_u64(self.chunks, count);
            self.writer.add_document(doc)?;
        }
        self.docs.insert(p.to_owned(), revision);
        Ok(())
    }

    /// Refresh the changed paths at `revision`: `None` text removes the document.
    pub fn refresh(&mut self, changed: &[ChangedDoc], revision: u64) -> tantivy::Result<()> {
        for (p, content) in changed {
            match content {
                Some((t, l)) => self.add(p, t, l.as_deref(), revision)?,
                None => {
                    self.writer.delete_term(Term::from_field_text(self.path, p));
                    self.docs.remove(p);
                }
            }
        }
        self.writer.commit()?;
        self.reader.reload()?;
        self.revision = revision;
        Ok(())
    }

    /// The index's own analysis of a word (the tokenizer the text was
    /// indexed with), so a query term is always an indexed term's shape.
    fn analyze(&self, word: &str) -> Vec<String> {
        let Ok(mut analyzer) = self.index.tokenizer_for_field(self.text) else {
            return vec![];
        };
        let mut stream = analyzer.token_stream(word);
        let mut out = Vec::new();
        while stream.advance() {
            out.push(stream.token().text.clone());
        }
        out
    }

    fn plan(&self, query: &str) -> Plan {
        if has_syntax(query) {
            return Plan::Raw(query.to_owned());
        }
        let words: Vec<&str> = query
            .split_whitespace()
            .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric() && c != '_'))
            .filter(|w| !w.is_empty())
            .collect();
        let analyzed: Vec<(&str, bool, Vec<String>)> = words
            .iter()
            .map(|w| (*w, identifier_like(w), self.analyze(w)))
            .collect();
        let mut cleaned: Vec<&str> = Vec::new();
        let mut terms: Vec<(String, f32)> = Vec::new();
        let push_term = |terms: &mut Vec<(String, f32)>, piece: &str, weight: f32| {
            if let Some(t) = terms.iter_mut().find(|(t, _)| t == piece) {
                t.1 = t.1.max(weight);
            } else {
                terms.push((piece.to_owned(), weight));
            }
        };
        for (word, identifier, pieces) in &analyzed {
            let weight = if *identifier { IDENTIFIER_WEIGHT } else { 1.0 };
            let mut any = false;
            for piece in pieces
                .iter()
                .map(String::as_str)
                .filter(|p| *identifier || !STOPWORDS.contains(p))
            {
                push_term(&mut terms, piece, weight);
                any = true;
            }
            if any {
                cleaned.push(word);
            }
        }
        if terms.is_empty() {
            // Only stopwords: a query of nothing but function words still
            // asks for something, so match them as written.
            for (word, identifier, pieces) in &analyzed {
                let weight = if *identifier { IDENTIFIER_WEIGHT } else { 1.0 };
                for piece in pieces {
                    push_term(&mut terms, piece, weight);
                }
                if !pieces.is_empty() {
                    cleaned.push(word);
                }
            }
        }
        Plan::Natural {
            cleaned: cleaned.join(" "),
            single_word: analyzed.len() == 1,
            terms,
        }
    }

    /// The OR query: every content term (identifier-like ones weighted up),
    /// plus the conjunction of all of them as a boosted clause so a chunk
    /// holding every term outranks one holding some.
    fn disjunction(&self, terms: &[(String, f32)]) -> Box<dyn Query> {
        let term_query = |t: &str| -> Box<dyn Query> {
            Box::new(TermQuery::new(
                Term::from_field_text(self.text, t),
                IndexRecordOption::WithFreqsAndPositions,
            ))
        };
        let mut clauses: Vec<(Occur, Box<dyn Query>)> = terms
            .iter()
            .map(|(t, w)| {
                let q: Box<dyn Query> = Box::new(BoostQuery::new(term_query(t), *w));
                (Occur::Should, q)
            })
            .collect();
        if terms.len() >= 2 {
            let all: Vec<Box<dyn Query>> = terms.iter().map(|(t, _)| term_query(t)).collect();
            clauses.push((
                Occur::Should,
                Box::new(BoostQuery::new(
                    Box::new(BooleanQuery::intersection(all)),
                    CONJUNCTION_BOOST,
                )),
            ));
        }
        let floor = if terms.len() >= MIN_TERMS_FOR_MATCH_FLOOR {
            2
        } else {
            1
        };
        Box::new(BooleanQuery::with_minimum_required_clauses(clauses, floor))
    }

    /// Run a query and keep the best chunk of each path, at most `max_hits`.
    fn collect(&self, q: &dyn Query, max_hits: usize) -> tantivy::Result<Vec<LexicalHit>> {
        let searcher = self.reader.searcher();
        let limit = max_hits.saturating_mul(8).clamp(32, 2000);
        let top = searcher.search(q, &TopDocs::with_limit(limit))?;
        let mut out: Vec<LexicalHit> = Vec::with_capacity(max_hits.min(top.len()));
        for (score, addr) in top {
            let doc: TantivyDocument = searcher.doc(addr)?;
            let path = doc
                .get_first(self.path)
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_owned();
            if out.iter().any(|h| h.path == path) {
                continue;
            }
            let language = doc
                .get_first(self.language)
                .and_then(|v| v.as_str())
                .filter(|l| !l.is_empty())
                .map(str::to_owned);
            let number = |f: Field| doc.get_first(f).and_then(|v| v.as_u64()).unwrap_or(0);
            let whole_file = number(self.chunks) <= 1;
            let lines = (!whole_file).then(|| {
                (
                    u32::try_from(number(self.start_line)).unwrap_or(u32::MAX),
                    u32::try_from(number(self.end_line)).unwrap_or(u32::MAX),
                )
            });
            let span = (!whole_file).then(|| (number(self.start_byte), number(self.end_byte)));
            let index_revision = self.docs.get(&path).copied().unwrap_or(self.revision);
            out.push(LexicalHit {
                path,
                score,
                language,
                index_revision,
                lines,
                span,
            });
            if out.len() >= max_hits {
                break;
            }
        }
        Ok(out)
    }

    /// BM25 search, bounded: one hit per file, naming its best chunk.
    ///
    /// A keyword query ANDs its terms and keeps that answer when it is short
    /// and non-empty; otherwise (a question, or nothing matched every term)
    /// it is an OR over the content terms with the conjunction as a boost
    /// (see the module docs). Explicit query syntax is lenient: a malformed
    /// query yields no hits, never an error.
    pub fn search(&self, query: &str, max_hits: usize) -> tantivy::Result<Vec<LexicalHit>> {
        let max = max_hits.max(1);
        let mut parser = QueryParser::for_index(&self.index, vec![self.text]);
        parser.set_conjunction_by_default();
        match self.plan(query) {
            Plan::Raw(q) => {
                let (q, _errors) = parser.parse_query_lenient(&q);
                self.collect(q.as_ref(), max)
            }
            Plan::Natural {
                cleaned,
                single_word,
                terms,
            } => {
                if terms.is_empty() {
                    return Ok(vec![]);
                }
                let (conjunctive, _errors) = parser.parse_query_lenient(&cleaned);
                let hits = self.collect(conjunctive.as_ref(), max)?;
                if single_word || (!hits.is_empty() && terms.len() <= PRECISE_TERMS) {
                    return Ok(hits);
                }
                self.collect(self.disjunction(&terms).as_ref(), max)
            }
        }
    }
}
