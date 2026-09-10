//! `modbit-retrieval` — exact, BM25, vector, AST and graph search.
//!
//! Canonical owner: context-engine (`docs/12_REPOSITORY_AND_MODULE_LAYOUT.md`,
//! `docs/81_ARCHITECTURE_GUARDRAILS_AND_FORBIDDEN_DUPLICATION.md`).
//! Dependency direction is enforced by `tools/architecture-lint`.
//!
//! M3.2 adds the BM25 lexical index (Tantivy) over the same file set; M3.3
//! the tree-sitter AST/symbol index for the Alpha languages; M3.5 the USearch
//! semantic chunk index with versioned embeddings and changed-chunk updates;
//! M3.6 the dependency / Git / test / runtime-evidence graph; M3.7 the L0–L3
//! retrieval planner with rank fusion; M3.9 the retrieval benchmark harness.
//! M3.1 delivers the L0 layer of docs/18: the exact / regex / path index over
//! one workspace revision, refreshed incrementally from the paths a write
//! changed. Every hit is bound to the index revision it was found at.

pub mod bench;
pub mod graph;
pub mod index;
pub mod lexical;
pub mod planner;
pub mod semantic;
pub mod symbols;

pub use graph::{CommitRecord, EvidenceGraph, GraphQuery, GraphView};
pub use index::{Hit, IndexError, IndexStats, PathHit, RepositoryIndex, SearchOptions};
pub use lexical::{ChangedDoc, LexicalHit, LexicalIndex};
pub use planner::{FusedHit, Level, PlanRequest, PlanResult, Sources};
pub use semantic::{Chunk, Embedder, FileSource, HashingEmbedder, SemanticHit, SemanticIndex};
pub use symbols::{ChangedSymbols, Symbol, SymbolIndex, SymbolQuery};
