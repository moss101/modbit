//! The tool surface of the live retrieval agents (PX-137).
//!
//! BASELINE gets what an agent without Modbit's indexes has: `search` (a real
//! `rg` process over the pinned checkout) and `read_file` (line ranges).
//! TREATMENT gets exactly those two tools and, in addition, `retrieve`: the
//! real `modbit_retrieval` planner (structural profile, all levels) over
//! indexes built in this process. The treatment is never forced to call it;
//! the model decides, and the transcript records whether it did.
//!
//! Every argument is hostile (it is the model's text, and the model reads
//! repository text): `search` passes the pattern and glob as `--opt=value`
//! so they can never become flags, `read_file` refuses absolute paths and
//! `..` and checks the canonical path stays under the checkout root, and
//! every result is cut to [`TOOL_OUTPUT_CHARS`].

use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};

use modbit_retrieval::bench::IndexTimes;
use modbit_retrieval::planner::{PlanRequest, Sources, retrieve};
use modbit_retrieval::{
    EvidenceGraph, HashingEmbedder, LexicalIndex, RepositoryIndex, SemanticIndex, SymbolIndex,
};
use serde_json::{Value, json};

/// A tool result is cut to this many characters (the case is not bounded by
/// what one grep happens to print).
pub const TOOL_OUTPUT_CHARS: usize = 6_000;
/// Most lines one `read_file` call returns.
pub const READ_MAX_LINES: usize = 250;
/// Lines `read_file` returns when the model names only a start.
pub const READ_DEFAULT_LINES: usize = 200;
/// Matches per file `search` prints.
const SEARCH_MAX_COUNT: &str = "5";
/// Most paths `retrieve` returns.
const RETRIEVE_MAX_PATHS: usize = 12;

/// The tools of the two profiles.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolSurface {
    /// `search` + `read_file`.
    Baseline,
    /// `search` + `read_file` + `retrieve`.
    Treatment,
}

impl ToolSurface {
    /// The profile name used in transcripts and reports.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Baseline => "baseline",
            Self::Treatment => "treatment",
        }
    }
}

/// The function schemas offered to the model.
#[must_use]
pub fn schemas(surface: ToolSurface) -> Vec<Value> {
    let mut tools = vec![
        json!({"type": "function", "function": {
            "name": "search",
            "description": "Search the repository's files with ripgrep (a regular expression, one line per match as path:line:text, at most 5 matches per file). Use glob to restrict the files, for example *.rs or crates/ignore/**.",
            "parameters": {"type": "object", "properties": {
                "pattern": {"type": "string", "description": "Regular expression to search for."},
                "glob": {"type": "string", "description": "Optional glob that file paths must match."},
                "ignore_case": {"type": "boolean", "description": "Match case-insensitively."}
            }, "required": ["pattern"]}}}),
        json!({"type": "function", "function": {
            "name": "read_file",
            "description": "Read lines of a file given its path relative to the repository root. Returns numbered lines (at most 250 per call).",
            "parameters": {"type": "object", "properties": {
                "path": {"type": "string", "description": "Path relative to the repository root."},
                "start_line": {"type": "integer", "description": "First line, 1-based (default 1)."},
                "end_line": {"type": "integer", "description": "Last line, inclusive (default start_line + 199)."}
            }, "required": ["path"]}}}),
    ];
    if surface == ToolSurface::Treatment {
        tools.push(json!({"type": "function", "function": {
            "name": "retrieve",
            "description": "Search the repository with Modbit's retrieval planner: exact, symbol, BM25 and vector search fused, with import and co-change graph expansion. Returns the best-ranked files with line ranges and which methods found them.",
            "parameters": {"type": "object", "properties": {
                "query": {"type": "string", "description": "What to look for, in words or identifiers."},
                "max_results": {"type": "integer", "description": "Most files to return (default 8, at most 12)."}
            }, "required": ["query"]}}}));
    }
    tools
}

/// One tool result.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolOutput {
    /// The text handed back to the model (already cut).
    pub text: String,
    /// The tool refused or failed (the model sees the message).
    pub error: bool,
    /// The text was cut at [`TOOL_OUTPUT_CHARS`].
    pub truncated: bool,
}

fn cut(text: String) -> (String, bool) {
    if text.chars().count() <= TOOL_OUTPUT_CHARS {
        return (text, false);
    }
    let mut s: String = text.chars().take(TOOL_OUTPUT_CHARS).collect();
    s.push_str("\n[output truncated]");
    (s, true)
}

fn ok(text: String) -> ToolOutput {
    let (text, truncated) = cut(text);
    ToolOutput {
        text,
        error: false,
        truncated,
    }
}

fn err(text: impl Into<String>) -> ToolOutput {
    let (text, truncated) = cut(text.into());
    ToolOutput {
        text,
        error: true,
        truncated,
    }
}

/// The planner's indexes over the checkout, built in this process.
pub struct Retriever {
    index: RepositoryIndex,
    lexical: LexicalIndex,
    symbols: SymbolIndex,
    semantic: SemanticIndex,
    graph: EvidenceGraph,
    /// How long building them took, per index.
    pub build_times: IndexTimes,
}

impl Retriever {
    /// Build every index of the planner over `root`.
    ///
    /// # Errors
    /// An index could not be built.
    pub fn build(root: &Path) -> Result<Self, String> {
        let t0 = std::time::Instant::now();
        let ms = |t: std::time::Instant| t.elapsed().as_secs_f64() * 1000.0;
        let t = std::time::Instant::now();
        let index = RepositoryIndex::build(root, 1).map_err(|e| e.to_string())?;
        let exact_ms = ms(t);
        let t = std::time::Instant::now();
        let lexical = LexicalIndex::build(index.texts(), 1).map_err(|e| e.to_string())?;
        let lexical_ms = ms(t);
        let t = std::time::Instant::now();
        let symbols = SymbolIndex::build(index.texts_with_hash(), 1);
        let symbols_ms = ms(t);
        let t = std::time::Instant::now();
        let files: Vec<modbit_retrieval::FileSource> = index
            .texts()
            .map(|(p, text, _)| {
                let spans = symbols
                    .symbols_in(p)
                    .iter()
                    .filter(|s| s.container.is_none())
                    .map(|s| (s.name.clone(), s.span.0, s.span.1))
                    .collect::<Vec<_>>();
                (p, text, spans)
            })
            .collect();
        let semantic =
            SemanticIndex::build(Box::new(HashingEmbedder::default()), files.into_iter(), 1)?;
        let semantic_ms = ms(t);
        let t = std::time::Instant::now();
        let graph = EvidenceGraph::build(index.texts(), vec![], Default::default(), 1);
        let graph_ms = ms(t);
        Ok(Self {
            index,
            lexical,
            symbols,
            semantic,
            graph,
            build_times: IndexTimes {
                exact_ms,
                lexical_ms,
                symbols_ms,
                semantic_ms,
                graph_ms,
                total_ms: ms(t0),
            },
        })
    }

    /// Run the structural profile (every level; the intent is derived from
    /// the query, as the planner does for a caller that names none).
    #[must_use]
    pub fn search(&self, query: &str, max_paths: usize) -> String {
        let src = Sources {
            index: &self.index,
            lexical: &self.lexical,
            symbols: &self.symbols,
            semantic: &self.semantic,
            graph: &self.graph,
        };
        let plan = retrieve(
            &src,
            &PlanRequest {
                query: query.to_owned(),
                intent: String::new(),
                max_hits: max_paths * 3,
                min_paths: 0,
                diagnostics: vec![],
                external_diagnostics: vec![],
                max_level: None,
            },
        );
        let mut seen: Vec<&str> = Vec::new();
        let mut out = String::new();
        for hit in &plan.hits {
            if seen.contains(&hit.path.as_str()) {
                continue;
            }
            seen.push(&hit.path);
            let lines = hit
                .lines
                .map_or_else(String::new, |(a, b)| format!(" lines {a}-{b}"));
            let first = hit
                .lines
                .and_then(|(a, _)| {
                    self.index
                        .entry_text(&hit.path)
                        .and_then(|(text, _, _)| text.lines().nth(a.saturating_sub(1) as usize))
                })
                .map(|l| {
                    let l: String = l.trim().chars().take(140).collect();
                    format!("  | {l}")
                })
                .unwrap_or_default();
            out.push_str(&format!(
                "{}{} (found by: {}){}\n",
                hit.path,
                lines,
                hit.sources.join(", "),
                first
            ));
            if seen.len() >= max_paths {
                break;
            }
        }
        if out.is_empty() {
            "no results".to_owned()
        } else {
            out
        }
    }
}

/// The tools bound to one checkout.
pub struct ToolBox {
    root: PathBuf,
    canonical_root: PathBuf,
    rg: PathBuf,
    retriever: Option<Retriever>,
}

impl ToolBox {
    /// Bind the tools to `root`. `retriever` is `Some` for the treatment.
    ///
    /// # Errors
    /// The root cannot be resolved.
    pub fn new(root: &Path, rg: PathBuf, retriever: Option<Retriever>) -> Result<Self, String> {
        let canonical_root = std::fs::canonicalize(root)
            .map_err(|e| format!("resolving {}: {e}", root.display()))?;
        Ok(Self {
            root: root.to_path_buf(),
            canonical_root,
            rg,
            retriever,
        })
    }

    /// Whether the treatment's `retrieve` is bound.
    #[must_use]
    pub fn has_retriever(&self) -> bool {
        self.retriever.is_some()
    }

    /// Run one tool call.
    #[must_use]
    pub fn execute(&self, name: &str, args: &Value) -> ToolOutput {
        match name {
            "search" => self.search(args),
            "read_file" => self.read_file(args),
            "retrieve" => match &self.retriever {
                Some(r) => {
                    let Some(q) = args["query"].as_str().filter(|q| !q.trim().is_empty()) else {
                        return err("retrieve needs a non-empty `query`");
                    };
                    let n = args["max_results"]
                        .as_u64()
                        .map_or(8, |n| (n as usize).clamp(1, RETRIEVE_MAX_PATHS));
                    ok(r.search(q, n))
                }
                None => err("unknown tool `retrieve`"),
            },
            other => err(format!("unknown tool `{other}`")),
        }
    }

    fn search(&self, args: &Value) -> ToolOutput {
        let Some(pattern) = args["pattern"].as_str().filter(|p| !p.is_empty()) else {
            return err("search needs a non-empty `pattern`");
        };
        let mut cmd = Command::new(&self.rg);
        cmd.current_dir(&self.root).stdin(Stdio::null()).args([
            "--no-config",
            "--color=never",
            "--line-number",
            "--no-heading",
            "--with-filename",
            "--max-columns=200",
            "--max-columns-preview",
            &format!("--max-count={SEARCH_MAX_COUNT}"),
            "--glob=!.git",
        ]);
        if args["ignore_case"].as_bool() == Some(true) {
            cmd.arg("--ignore-case");
        }
        if let Some(g) = args["glob"].as_str().filter(|g| !g.trim().is_empty()) {
            cmd.arg(format!("--glob={g}"));
        }
        cmd.arg(format!("--regexp={pattern}")).arg("--").arg(".");
        let out = match cmd.output() {
            Ok(o) => o,
            Err(e) => return err(format!("could not run ripgrep: {e}")),
        };
        match out.status.code() {
            Some(0) => {
                let text = String::from_utf8_lossy(&out.stdout)
                    .lines()
                    .map(|l| {
                        let l = l.replace('\\', "/");
                        l.strip_prefix("./").map_or(l.clone(), str::to_owned)
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                ok(text)
            }
            Some(1) => ok("no matches".to_owned()),
            _ => err(format!(
                "ripgrep error: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            )),
        }
    }

    fn resolve(&self, rel: &str) -> Result<PathBuf, String> {
        let p = Path::new(rel);
        if p.is_absolute()
            || p.components().any(|c| {
                matches!(
                    c,
                    Component::ParentDir | Component::Prefix(_) | Component::RootDir
                )
            })
        {
            return Err(format!(
                "`{rel}` must be a path relative to the repository root without `..`"
            ));
        }
        let full = std::fs::canonicalize(self.root.join(p))
            .map_err(|_| format!("`{rel}` does not exist"))?;
        if !full.starts_with(&self.canonical_root) {
            return Err(format!("`{rel}` is outside the repository"));
        }
        if !full.is_file() {
            return Err(format!("`{rel}` is not a file"));
        }
        Ok(full)
    }

    fn read_file(&self, args: &Value) -> ToolOutput {
        let Some(rel) = args["path"].as_str() else {
            return err("read_file needs a `path`");
        };
        let full = match self.resolve(rel) {
            Ok(f) => f,
            Err(e) => return err(e),
        };
        let bytes = match std::fs::read(&full) {
            Ok(b) => b,
            Err(e) => return err(format!("reading `{rel}`: {e}")),
        };
        let text = String::from_utf8_lossy(&bytes);
        let lines: Vec<&str> = text.lines().collect();
        let start = args["start_line"]
            .as_u64()
            .map_or(1, |n| (n as usize).max(1));
        let end = args["end_line"]
            .as_u64()
            .map_or(start.saturating_add(READ_DEFAULT_LINES - 1), |n| {
                (n as usize).max(start)
            });
        let end = end
            .min(start.saturating_add(READ_MAX_LINES - 1))
            .min(lines.len());
        if start > lines.len() {
            return ok(format!(
                "`{rel}` has {} lines; nothing at line {start}",
                lines.len()
            ));
        }
        let mut out = format!("{rel} ({} lines), lines {start}-{end}:\n", lines.len());
        for (i, l) in lines.iter().enumerate().take(end).skip(start - 1) {
            out.push_str(&format!("{}: {l}\n", i + 1));
        }
        ok(out)
    }
}
