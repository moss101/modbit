//! `modbit-retrieval` — exact, BM25, vector, AST and graph search.
//!
//! Canonical owner: context-engine (`docs/12_REPOSITORY_AND_MODULE_LAYOUT.md`,
//! `docs/81_ARCHITECTURE_GUARDRAILS_AND_FORBIDDEN_DUPLICATION.md`).
//! Dependency direction is enforced by `tools/architecture-lint`.
//!
//! M3.2 adds the BM25 lexical index (Tantivy) over the same file set.
//! M3.1 delivers the L0 layer of docs/18: the exact / regex / path index over
//! one workspace revision, refreshed incrementally from the paths a write
//! changed. Every hit is bound to the index revision it was found at.

pub mod index;
pub mod lexical;

pub use index::{Hit, IndexError, IndexStats, PathHit, RepositoryIndex, SearchOptions};
pub use lexical::{ChangedDoc, LexicalHit, LexicalIndex};
