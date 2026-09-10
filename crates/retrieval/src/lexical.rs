//! BM25 lexical index (docs/18 "Concrete local indexing stack": Tantivy
//! index per repository snapshot family; M3.2). One document per searchable
//! file of the exact index; the default tokenizer (split on non-alphanumerics,
//! lower-case) so identifiers such as `compute_total` match `compute` and
//! `total`. Held in memory per Core process and refreshed per changed path.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use tantivy::collector::TopDocs;
use tantivy::query::QueryParser;
use tantivy::schema::{Field, STORED, STRING, Schema, TEXT, Value};
use tantivy::{Index, IndexReader, IndexWriter, TantivyDocument, Term};

/// A changed document for `refresh`: path and, when it still exists, its
/// text and language (`None` removes the document).
pub type ChangedDoc = (String, Option<(String, Option<String>)>);

/// One ranked lexical hit.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LexicalHit {
    /// Root-relative path.
    pub path: String,
    /// BM25 score.
    pub score: f32,
    /// Language label, if known.
    pub language: Option<String>,
    /// Index revision the document was indexed at.
    pub index_revision: u64,
}

/// The BM25 index of one workspace.
pub struct LexicalIndex {
    index: Index,
    writer: IndexWriter,
    reader: IndexReader,
    path: Field,
    text: Field,
    language: Field,
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

    /// Documents indexed.
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
        self.writer.delete_term(Term::from_field_text(self.path, p));
        let mut doc = TantivyDocument::default();
        doc.add_text(self.path, p);
        doc.add_text(self.text, t);
        doc.add_text(self.language, l.unwrap_or(""));
        self.writer.add_document(doc)?;
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

    /// BM25 search (Tantivy query syntax, lenient; terms are AND-ed), bounded.
    pub fn search(&self, query: &str, max_hits: usize) -> tantivy::Result<Vec<LexicalHit>> {
        let searcher = self.reader.searcher();
        let mut parser = QueryParser::for_index(&self.index, vec![self.text]);
        parser.set_conjunction_by_default();
        let (q, _errors) = parser.parse_query_lenient(query);
        let top = searcher.search(&q, &TopDocs::with_limit(max_hits.max(1)))?;
        let mut out = Vec::with_capacity(top.len());
        for (score, addr) in top {
            let doc: TantivyDocument = searcher.doc(addr)?;
            let path = doc
                .get_first(self.path)
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_owned();
            let language = doc
                .get_first(self.language)
                .and_then(|v| v.as_str())
                .filter(|l| !l.is_empty())
                .map(str::to_owned);
            let index_revision = self.docs.get(&path).copied().unwrap_or(self.revision);
            out.push(LexicalHit {
                path,
                score,
                language,
                index_revision,
            });
        }
        Ok(out)
    }
}
