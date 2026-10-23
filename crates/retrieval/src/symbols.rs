//! tree-sitter AST / symbol index (docs/18 "Index inputs": tree-sitter AST
//! and symbol definitions; M3.3). Definitions of the Alpha languages
//! (docs/76: TypeScript/JavaScript, Python, Rust) are extracted with the
//! real grammars into typed symbols with byte spans and line ranges, bound
//! to the file content hash and the index revision. Unsupported languages
//! contribute no symbols and no error: structural claims are made only where
//! a grammar exists.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use tree_sitter::{Language, Node, Parser};

/// One definition.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Symbol {
    /// Root-relative path.
    pub path: String,
    /// Name.
    pub name: String,
    /// `function` | `method` | `class` | `struct` | `enum` | `interface` | `trait` | `impl` | `type` | `const` | `module` | `variable`.
    pub kind: String,
    /// Language label.
    pub language: String,
    /// Enclosing definition name, if any (e.g. the class of a method).
    pub container: Option<String>,
    /// 1-based first line.
    pub line_start: u32,
    /// 1-based last line.
    pub line_end: u32,
    /// Byte span of the whole definition.
    pub span: (u64, u64),
    /// Content hash of the file.
    pub content_hash: String,
    /// Index revision.
    pub index_revision: u64,
}

/// A changed file for `refresh`: path and, when it still exists, its text,
/// language and content hash (`None` removes the path).
pub type ChangedSymbols = (String, Option<(String, Option<String>, String)>);

/// A symbol query.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SymbolQuery {
    /// Name to match (exact unless `prefix`).
    pub name: Option<String>,
    /// Prefix match on the name.
    pub prefix: bool,
    /// Restrict to a kind.
    pub kind: Option<String>,
    /// Restrict to a path glob.
    pub path_glob: Option<String>,
    /// Ceiling (0 = the default of 200).
    pub max: usize,
}

impl Default for SymbolQuery {
    fn default() -> Self {
        Self {
            name: None,
            prefix: false,
            kind: None,
            path_glob: None,
            max: 200,
        }
    }
}

/// The symbol index of one workspace.
#[derive(Debug, Default)]
pub struct SymbolIndex {
    revision: u64,
    by_path: BTreeMap<String, Vec<Symbol>>,
}

/// Grammar for a language label.
fn grammar(language: &str, path: &str) -> Option<Language> {
    Some(match language {
        "rust" => tree_sitter_rust::LANGUAGE.into(),
        "python" => tree_sitter_python::LANGUAGE.into(),
        "javascript" => tree_sitter_javascript::LANGUAGE.into(),
        "typescript" if path.ends_with(".tsx") => tree_sitter_typescript::LANGUAGE_TSX.into(),
        "typescript" => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        _ => return None,
    })
}

/// Definition node kinds per language → symbol kind. The name is the `name`
/// field where the grammar has one; `impl_item` uses `type`.
fn kind_of(language: &str, node_kind: &str) -> Option<&'static str> {
    Some(match (language, node_kind) {
        ("rust", "function_item") | ("rust", "function_signature_item") => "function",
        ("rust", "struct_item") => "struct",
        ("rust", "enum_item") => "enum",
        ("rust", "trait_item") => "trait",
        ("rust", "impl_item") => "impl",
        ("rust", "type_item") => "type",
        ("rust", "const_item") | ("rust", "static_item") => "const",
        ("rust", "mod_item") => "module",
        ("python", "function_definition") => "function",
        ("python", "class_definition") => "class",
        ("javascript" | "typescript", "function_declaration") => "function",
        ("javascript" | "typescript", "generator_function_declaration") => "function",
        ("javascript" | "typescript", "class_declaration") => "class",
        ("javascript" | "typescript", "method_definition") => "method",
        ("typescript", "interface_declaration") => "interface",
        ("typescript", "type_alias_declaration") => "type",
        ("typescript", "enum_declaration") => "enum",
        ("typescript", "abstract_class_declaration") => "class",
        ("javascript" | "typescript", "variable_declarator") => "variable",
        _ => return None,
    })
}

/// Push the chunk spans of one definition: itself when it fits (or has no
/// members), else its outermost members, recursively.
fn expand_chunk(
    sym: &Symbol,
    all: &[Symbol],
    max_chunk_bytes: usize,
    label: &str,
    out: &mut Vec<(String, u64, u64)>,
) {
    let size = usize::try_from(sym.span.1.saturating_sub(sym.span.0)).unwrap_or(usize::MAX);
    let container_kind = matches!(
        sym.kind.as_str(),
        "class" | "struct" | "enum" | "trait" | "impl" | "interface" | "module"
    );
    if size <= max_chunk_bytes || !container_kind {
        out.push((label.to_owned(), sym.span.0, sym.span.1));
        return;
    }
    let mut members: Vec<&Symbol> = all
        .iter()
        .filter(|m| {
            m.container.is_some()
                && m.span != sym.span
                && m.span.0 >= sym.span.0
                && m.span.1 <= sym.span.1
        })
        .collect();
    members.sort_by_key(|m| (m.span.0, std::cmp::Reverse(m.span.1)));
    let mut cursor = sym.span.0;
    let mut outermost: Vec<&Symbol> = Vec::new();
    for m in members {
        if m.span.0 >= cursor {
            cursor = m.span.1;
            outermost.push(m);
        }
    }
    if outermost.is_empty() {
        out.push((label.to_owned(), sym.span.0, sym.span.1));
        return;
    }
    for m in outermost {
        expand_chunk(
            m,
            all,
            max_chunk_bytes,
            &format!("{}::{}", sym.name, m.name),
            out,
        );
    }
}

impl SymbolIndex {
    /// Build from `(path, text, language, content_hash)` at `revision`.
    pub fn build<'a>(
        files: impl Iterator<Item = (&'a str, &'a str, Option<&'a str>, &'a str)>,
        revision: u64,
    ) -> Self {
        let mut idx = Self {
            revision,
            by_path: BTreeMap::new(),
        };
        for (p, t, l, h) in files {
            idx.index_file(p, t, l, h, revision);
        }
        idx
    }

    /// Index revision.
    #[must_use]
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Symbols of one path (empty for unsupported or unindexed paths).
    #[must_use]
    pub fn symbols_in(&self, path: &str) -> &[Symbol] {
        self.by_path.get(path).map_or(&[], Vec::as_slice)
    }

    /// The byte spans a semantic index should embed for `path` (`name`,
    /// start, end), one per top-level definition. A definition that does not
    /// fit one chunk (`max_chunk_bytes`) and holds members — a Rust `impl`, a
    /// class, a module — is replaced by its members, named
    /// `Container::member`, so a large `impl` is indexed method by method and
    /// not by its first 4 KiB (FIX-12, audit N9). A large definition with no
    /// members stays whole; the chunker cuts it into line-bounded pieces.
    #[must_use]
    pub fn chunk_spans(&self, path: &str, max_chunk_bytes: usize) -> Vec<(String, u64, u64)> {
        let all = self.symbols_in(path);
        let mut out = Vec::new();
        for s in all.iter().filter(|s| s.container.is_none()) {
            expand_chunk(s, all, max_chunk_bytes, &s.name, &mut out);
        }
        out
    }

    /// Every indexed file with its symbols, in path order.
    pub fn files(&self) -> impl Iterator<Item = (&String, &[Symbol])> {
        self.by_path.iter().map(|(p, s)| (p, s.as_slice()))
    }

    /// Total symbols.
    #[must_use]
    pub fn len(&self) -> usize {
        self.by_path.values().map(Vec::len).sum()
    }

    /// Whether empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Refresh changed paths at `revision`; `None` removes the path.
    pub fn refresh(&mut self, changed: &[ChangedSymbols], revision: u64) {
        for (p, content) in changed {
            match content {
                Some((t, l, h)) => self.index_file(p, t, l.as_deref(), h, revision),
                None => {
                    self.by_path.remove(p);
                }
            }
        }
        self.revision = revision;
    }

    /// Query, bounded; results ordered by path then position.
    pub fn query(&self, q: &SymbolQuery) -> Result<Vec<Symbol>, String> {
        let glob = match &q.path_glob {
            Some(g) => Some(
                globset::Glob::new(g)
                    .map_err(|e| e.to_string())?
                    .compile_matcher(),
            ),
            None => None,
        };
        let max = if q.max == 0 { 200 } else { q.max };
        let mut out = Vec::new();
        'outer: for (path, syms) in &self.by_path {
            if let Some(g) = &glob
                && !g.is_match(path)
            {
                continue;
            }
            for s in syms {
                if let Some(k) = &q.kind
                    && &s.kind != k
                {
                    continue;
                }
                if let Some(n) = &q.name {
                    let hit = if q.prefix {
                        s.name.starts_with(n.as_str())
                    } else {
                        &s.name == n
                    };
                    if !hit {
                        continue;
                    }
                }
                out.push(s.clone());
                if out.len() >= max {
                    break 'outer;
                }
            }
        }
        Ok(out)
    }

    fn index_file(
        &mut self,
        path: &str,
        text: &str,
        language: Option<&str>,
        hash: &str,
        revision: u64,
    ) {
        self.by_path.remove(path);
        let symbols = extract_symbols(path, text, language, hash, revision);
        if !symbols.is_empty() {
            self.by_path.insert(path.to_owned(), symbols);
        }
    }

    /// An index from per-file symbols already extracted (PX-111: the
    /// persisted records, validated by content hash, skip the parse).
    #[must_use]
    pub fn from_parts(revision: u64, by_path: BTreeMap<String, Vec<Symbol>>) -> Self {
        Self {
            revision,
            by_path: by_path.into_iter().filter(|(_, v)| !v.is_empty()).collect(),
        }
    }

    /// Replace one file's symbols with ones already extracted; empty removes.
    pub fn set_file(&mut self, path: &str, symbols: Vec<Symbol>) {
        if symbols.is_empty() {
            self.by_path.remove(path);
        } else {
            self.by_path.insert(path.to_owned(), symbols);
        }
    }

    /// Move the index to `revision` after a set of [`Self::set_file`] calls.
    pub fn set_revision(&mut self, revision: u64) {
        self.revision = revision;
    }
}

/// The definitions of one file with the real tree-sitter grammar of its
/// language (none for an unsupported one), bound to its content hash and the
/// revision: the pure function the index and its persisted records share.
#[must_use]
pub fn extract_symbols(
    path: &str,
    text: &str,
    language: Option<&str>,
    hash: &str,
    revision: u64,
) -> Vec<Symbol> {
    {
        let Some(language) = language else {
            return vec![];
        };
        let Some(lang) = grammar(language, path) else {
            return vec![];
        };
        let mut parser = Parser::new();
        if parser.set_language(&lang).is_err() {
            return vec![];
        }
        let Some(tree) = parser.parse(text, None) else {
            return vec![];
        };
        let mut symbols = Vec::new();
        let mut stack: Vec<(Node, Option<String>)> = vec![(tree.root_node(), None)];
        while let Some((node, container)) = stack.pop() {
            let mut own: Option<String> = None;
            if let Some(kind) = kind_of(language, node.kind()) {
                let name_node = node
                    .child_by_field_name("name")
                    .or_else(|| node.child_by_field_name("type"));
                if let Some(n) = name_node
                    && let Ok(name) = n.utf8_text(text.as_bytes())
                    && !name.is_empty()
                {
                    let (kind, keep) = match (language, kind) {
                        // Only top-level and class-level variable declarations are symbols.
                        (_, "variable") => (
                            "variable",
                            node.parent().and_then(|p| p.parent()).is_some_and(|gp| {
                                gp.kind() == "program" || gp.kind() == "export_statement"
                            }),
                        ),
                        (_, k) => (k, true),
                    };
                    if keep {
                        let range = node.byte_range();
                        symbols.push(Symbol {
                            path: path.to_owned(),
                            name: name.to_owned(),
                            kind: kind.to_owned(),
                            language: language.to_owned(),
                            container: container.clone(),
                            line_start: node.start_position().row as u32 + 1,
                            line_end: node.end_position().row as u32 + 1,
                            span: (range.start as u64, range.end as u64),
                            content_hash: hash.to_owned(),
                            index_revision: revision,
                        });
                        if matches!(
                            kind,
                            "class" | "struct" | "enum" | "trait" | "impl" | "interface" | "module"
                        ) {
                            own = Some(name.to_owned());
                        }
                    }
                }
            }
            let next = own.or(container);
            let mut cursor = node.walk();
            let children: Vec<Node> = node.children(&mut cursor).collect();
            for c in children.into_iter().rev() {
                stack.push((c, next.clone()));
            }
        }
        symbols.sort_by_key(|s| s.span.0);
        symbols
    }
}
