//! Symbol reference, call and implementor edges (PX-110; docs/18 L2/L3
//! "AST/LSP/dependency graph expansion", REQ-EV-0157): the code graph below
//! the file level.
//!
//! Two layers, deliberately separate:
//!
//! 1. **Facts** ([`FileFacts`]) — a pure function of one file's bytes: the
//!    identifier occurrences of the real tree-sitter parse (each a reference or
//!    a call, with its qualifier and the definition it sits in), the
//!    inheritance edges the language gives (Rust `impl Trait for Type`,
//!    TypeScript/JavaScript `extends`/`implements`, Python base classes) and
//!    the names the file imports. Facts are keyed by the file's content hash,
//!    so they are cached and persisted as such (PX-111) and never go stale.
//! 2. **Resolution** ([`RefGraph`]) — occurrences resolved to definitions by
//!    name and import, over the symbol index and the workspace's path set.
//!    Every edge carries a confidence class and the revision it holds at:
//!    `resolved` (an import binding, a qualified path, the same file or a
//!    qualifier naming the defining type said so), `ambiguous` (only the name
//!    says so, perhaps of several definitions) — and an occurrence whose name no
//!    workspace definition carries is `unresolved` and is counted, not made
//!    into an edge. An LSP is an upgrade where a Tier A server is available
//!    and never a requirement.
//!
//! Resolution depends on other files (a definition added or removed
//! elsewhere changes what a name means), so a refresh re-resolves the changed
//! files and every file that mentions a name the changed files defined or no
//! longer define, or that imports one of them: correct at the new revision,
//! incremental in cost. The graph is derived and rebuildable; the filesystem
//! and the revision stay canonical.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use serde::{Deserialize, Serialize};
use tree_sitter::{Language, Node, Parser};

use crate::graph::resolve_import;
use crate::symbols::SymbolIndex;

/// Version of the facts format; part of every persisted fact record.
pub const FACTS_VERSION: u32 = 1;

/// Occurrences kept per file (a generated file cannot make a graph without a
/// bound).
const MAX_OCCURRENCES_PER_FILE: usize = 20_000;
/// Candidate definitions one ambiguous occurrence is linked to.
const MAX_AMBIGUOUS_TARGETS: usize = 6;
/// Edges expanded by one impact or symbol query before it says it is partial.
pub const EXPANSION_CAP: usize = 5_000;

/// What an occurrence is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OccKind {
    /// The name is mentioned (a type, a value, a function passed along).
    Reference,
    /// The name is called (or constructed).
    Call,
}

/// One identifier occurrence in a file.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Occurrence {
    /// The identifier.
    pub name: String,
    /// What it is qualified by, as written: the path before `::`, the
    /// receiver before `.`, the object before an attribute.
    pub qualifier: Option<String>,
    /// 1-based line.
    pub line: u32,
    /// Reference or call.
    pub kind: OccKind,
    /// The definition it sits in (`Container::name` or `name`).
    pub enclosing: Option<String>,
}

/// An inheritance edge as written in the source.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Heritage {
    /// The implementing (or extending) type.
    pub name: String,
    /// The interface, trait or base class.
    pub base: String,
    /// `implements` (a trait or interface) or `extends` (a base class).
    pub relation: String,
    /// 1-based line.
    pub line: u32,
}

/// A name a file imports.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Binding {
    /// The name in this file.
    pub local: String,
    /// The module or path imported from, as written (`crate::a::b::C`,
    /// `./util`, `pkg.mod`).
    pub spec: String,
    /// The name imported from it; `None` when the binding is the module
    /// itself (`import * as ns`, `use a::b;`, `import pkg`).
    pub imported: Option<String>,
    /// 1-based line.
    pub line: u32,
}

/// What one file says about references, a pure function of its bytes.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileFacts {
    /// Language label.
    pub language: String,
    /// sha256 of the bytes the facts were read from.
    pub content_hash: String,
    /// Identifier occurrences.
    pub occurrences: Vec<Occurrence>,
    /// Inheritance edges.
    pub heritage: Vec<Heritage>,
    /// Imported names.
    pub bindings: Vec<Binding>,
    /// The occurrence cap cut this file short.
    #[serde(default)]
    pub truncated: bool,
}

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

/// Whether a language has reference facts (a grammar).
#[must_use]
pub fn supports(language: &str) -> bool {
    matches!(language, "rust" | "python" | "javascript" | "typescript")
}

fn text_of<'a>(n: Node<'_>, src: &'a str) -> &'a str {
    n.utf8_text(src.as_bytes()).unwrap_or("")
}

fn last_segment(s: &str) -> &str {
    s.rsplit([':', '.', '/'])
        .find(|p| !p.is_empty())
        .unwrap_or(s)
}

fn is_ident_kind(language: &str, kind: &str) -> bool {
    match language {
        "rust" => matches!(kind, "identifier" | "type_identifier" | "field_identifier"),
        "python" => kind == "identifier",
        _ => matches!(
            kind,
            "identifier"
                | "type_identifier"
                | "property_identifier"
                | "shorthand_property_identifier"
        ),
    }
}

fn is_field(parent: Node<'_>, node: Node<'_>, field: &str) -> bool {
    parent
        .child_by_field_name(field)
        .is_some_and(|n| n.id() == node.id())
}

/// Whether the identifier sits in an import, an attribute or another place
/// where a name is declared or configured rather than used.
fn in_declaration_context(language: &str, node: Node<'_>) -> bool {
    let mut cur = node.parent();
    let mut hops = 0;
    while let Some(n) = cur {
        let k = n.kind();
        let hit = match language {
            "rust" => matches!(
                k,
                "use_declaration" | "attribute_item" | "inner_attribute_item"
            ),
            "python" => matches!(
                k,
                "import_statement" | "import_from_statement" | "future_import_statement"
            ),
            _ => matches!(k, "import_statement" | "export_specifier"),
        };
        if hit {
            return true;
        }
        hops += 1;
        if hops > 8 {
            return false;
        }
        cur = n.parent();
    }
    false
}

/// Nodes whose `name` field (or binding position) defines a name rather
/// than using it.
fn defines_name(language: &str, parent: Node<'_>, node: Node<'_>) -> bool {
    if is_field(parent, node, "name") {
        // `name:` of a call-like construct or a path is a use; everything
        // else with a `name` field is a declaration.
        return !matches!(
            parent.kind(),
            "struct_expression"
                | "keyword_argument"
                | "scoped_identifier"
                | "scoped_type_identifier"
        );
    }
    let pk = parent.kind();
    match language {
        "rust" => match pk {
            "parameter" | "let_declaration" | "for_expression" => is_field(parent, node, "pattern"),
            "closure_parameters" | "tuple_pattern" | "struct_pattern" | "field_pattern"
            | "ref_pattern" | "mut_pattern" | "or_pattern" | "slice_pattern"
            | "captured_pattern" => true,
            // `Some(x)`: the head is a path to a constructor, the rest bind.
            "tuple_struct_pattern" => !is_field(parent, node, "type"),
            _ => false,
        },
        "python" => match pk {
            "parameters" | "global_statement" | "nonlocal_statement" | "pattern_list"
            | "tuple_pattern" | "as_pattern_target" | "dotted_name" | "aliased_import" => true,
            "keyword_argument" => is_field(parent, node, "name"),
            "default_parameter" | "typed_parameter" | "typed_default_parameter" => {
                !is_field(parent, node, "value") && !is_field(parent, node, "type")
            }
            "for_statement" => is_field(parent, node, "left"),
            _ => false,
        },
        _ => match pk {
            "required_parameter" | "optional_parameter" => is_field(parent, node, "pattern"),
            "formal_parameters" | "array_pattern" | "object_pattern" | "rest_pattern"
            | "labeled_statement" => true,
            // An object literal's key is not a use of that name.
            "pair" => is_field(parent, node, "key"),
            _ => false,
        },
    }
}

/// The enclosing definition's label for a node.
fn enclosing_label(language: &str, node: Node<'_>, src: &str) -> Option<String> {
    let mut cur = node.parent();
    let mut inner: Option<String> = None;
    while let Some(n) = cur {
        let k = n.kind();
        let is_fn = match language {
            "rust" => k == "function_item",
            "python" => k == "function_definition",
            _ => {
                matches!(
                    k,
                    "function_declaration"
                        | "method_definition"
                        | "generator_function_declaration"
                        | "function_expression"
                ) || (k == "variable_declarator"
                    && n.child_by_field_name("value").is_some_and(|v| {
                        matches!(v.kind(), "arrow_function" | "function_expression")
                    }))
            }
        };
        let is_container = match language {
            "rust" => matches!(k, "impl_item" | "trait_item"),
            "python" => k == "class_definition",
            _ => matches!(k, "class_declaration" | "abstract_class_declaration"),
        };
        if is_fn && inner.is_none() {
            let name = n
                .child_by_field_name("name")
                .map(|x| text_of(x, src).to_owned())
                .filter(|s| !s.is_empty());
            if name.is_some() {
                inner = name;
            }
        } else if is_container {
            let cname = n
                .child_by_field_name("type")
                .or_else(|| n.child_by_field_name("name"))
                .map(|x| type_name(x, src))
                .filter(|s| !s.is_empty());
            return match (cname, inner) {
                (Some(c), Some(i)) => Some(format!("{c}::{i}")),
                (None, Some(i)) => Some(i),
                (Some(c), None) => Some(c),
                (None, None) => None,
            };
        }
        cur = n.parent();
    }
    inner
}

/// The bare name of a type node: `Vec<T>` is `Vec`, `a::B` is `B`.
fn type_name(n: Node<'_>, src: &str) -> String {
    match n.kind() {
        "generic_type" | "generic_type_with_turbofish" => n
            .child_by_field_name("type")
            .or_else(|| n.child_by_field_name("name"))
            .map(|t| type_name(t, src))
            .unwrap_or_default(),
        "scoped_type_identifier" | "scoped_identifier" => n
            .child_by_field_name("name")
            .map(|t| text_of(t, src).to_owned())
            .unwrap_or_default(),
        "reference_type" | "pointer_type" => n
            .child_by_field_name("type")
            .map(|t| type_name(t, src))
            .unwrap_or_default(),
        _ => {
            let t = text_of(n, src);
            // `Foo<T>` written through a grammar without generic nodes.
            let t = t.split(['<', '(']).next().unwrap_or(t).trim();
            last_segment(t).to_owned()
        }
    }
}

/// The callee identifier of a call, its qualifier and the byte offset the
/// identifier node starts at.
fn callee_of(language: &str, call: Node<'_>, src: &str) -> Option<(String, Option<String>, usize)> {
    let f = match (language, call.kind()) {
        ("javascript" | "typescript", "new_expression") => {
            call.child_by_field_name("constructor")?
        }
        _ => call.child_by_field_name("function")?,
    };
    callee_of_expr(f, src)
}

fn callee_of_expr(f: Node<'_>, src: &str) -> Option<(String, Option<String>, usize)> {
    match f.kind() {
        "identifier" | "type_identifier" => {
            Some((text_of(f, src).to_owned(), None, f.start_byte()))
        }
        "scoped_identifier" => {
            let name = f.child_by_field_name("name")?;
            let path = f
                .child_by_field_name("path")
                .map(|p| text_of(p, src).to_owned());
            Some((text_of(name, src).to_owned(), path, name.start_byte()))
        }
        "generic_function" => callee_of_expr(f.child_by_field_name("function")?, src),
        "field_expression" => {
            let name = f.child_by_field_name("field")?;
            let recv = f
                .child_by_field_name("value")
                .map(|p| text_of(p, src).to_owned());
            Some((text_of(name, src).to_owned(), recv, name.start_byte()))
        }
        "member_expression" => {
            let name = f.child_by_field_name("property")?;
            let recv = f
                .child_by_field_name("object")
                .map(|p| text_of(p, src).to_owned());
            Some((text_of(name, src).to_owned(), recv, name.start_byte()))
        }
        "attribute" => {
            let name = f.child_by_field_name("attribute")?;
            let recv = f
                .child_by_field_name("object")
                .map(|p| text_of(p, src).to_owned());
            Some((text_of(name, src).to_owned(), recv, name.start_byte()))
        }
        "parenthesized_expression" | "await_expression" => callee_of_expr(f.named_child(0)?, src),
        _ => None,
    }
}

/// Qualifier of an identifier that is not a callee: its scoped path or the
/// receiver it is a field of.
fn qualifier_of(parent: Node<'_>, node: Node<'_>, src: &str) -> Option<String> {
    match parent.kind() {
        "scoped_identifier" | "scoped_type_identifier"
            if parent
                .child_by_field_name("name")
                .is_some_and(|n| n.id() == node.id()) =>
        {
            parent
                .child_by_field_name("path")
                .map(|p| text_of(p, src).to_owned())
        }
        "field_expression"
            if parent
                .child_by_field_name("field")
                .is_some_and(|n| n.id() == node.id()) =>
        {
            parent
                .child_by_field_name("value")
                .map(|p| text_of(p, src).to_owned())
        }
        "member_expression"
            if parent
                .child_by_field_name("property")
                .is_some_and(|n| n.id() == node.id()) =>
        {
            parent
                .child_by_field_name("object")
                .map(|p| text_of(p, src).to_owned())
        }
        "attribute"
            if parent
                .child_by_field_name("attribute")
                .is_some_and(|n| n.id() == node.id()) =>
        {
            parent
                .child_by_field_name("object")
                .map(|p| text_of(p, src).to_owned())
        }
        _ => None,
    }
}

fn flatten_rust_use(n: Node<'_>, src: &str, prefix: &str, out: &mut Vec<Binding>) {
    let join = |p: &str, s: &str| {
        if p.is_empty() {
            s.to_owned()
        } else {
            format!("{p}::{s}")
        }
    };
    let line = n.start_position().row as u32 + 1;
    match n.kind() {
        "identifier" | "crate" | "self" | "super" => {
            let name = text_of(n, src);
            out.push(Binding {
                local: name.to_owned(),
                spec: join(prefix, name),
                imported: Some(name.to_owned()),
                line,
            });
        }
        "scoped_identifier" => {
            let full = join(prefix, text_of(n, src));
            let name = n
                .child_by_field_name("name")
                .map(|x| text_of(x, src).to_owned())
                .unwrap_or_default();
            out.push(Binding {
                local: name.clone(),
                spec: full,
                imported: Some(name),
                line,
            });
        }
        "use_as_clause" => {
            let path = n
                .child_by_field_name("path")
                .map(|p| text_of(p, src))
                .unwrap_or("");
            let alias = n
                .child_by_field_name("alias")
                .map(|p| text_of(p, src))
                .unwrap_or("");
            let imported = last_segment(path).to_owned();
            out.push(Binding {
                local: alias.to_owned(),
                spec: join(prefix, path),
                imported: Some(imported),
                line,
            });
        }
        "scoped_use_list" => {
            let path = n
                .child_by_field_name("path")
                .map(|p| text_of(p, src))
                .unwrap_or("");
            let new_prefix = join(prefix, path);
            if let Some(list) = n.child_by_field_name("list") {
                flatten_rust_use(list, src, &new_prefix, out);
            }
        }
        "use_list" => {
            let mut c = n.walk();
            for ch in n.named_children(&mut c) {
                flatten_rust_use(ch, src, prefix, out);
            }
        }
        _ => {}
    }
}

fn py_dotted(n: Node<'_>, src: &str) -> String {
    text_of(n, src).to_owned()
}

fn collect_bindings_and_heritage(
    language: &str,
    node: Node<'_>,
    src: &str,
    bindings: &mut Vec<Binding>,
    heritage: &mut Vec<Heritage>,
) {
    let line = node.start_position().row as u32 + 1;
    match (language, node.kind()) {
        ("rust", "use_declaration") => {
            if let Some(arg) = node.child_by_field_name("argument") {
                flatten_rust_use(arg, src, "", bindings);
            }
        }
        ("rust", "impl_item") => {
            if let (Some(tr), Some(ty)) = (
                node.child_by_field_name("trait"),
                node.child_by_field_name("type"),
            ) {
                let base = type_name(tr, src);
                let name = type_name(ty, src);
                if !base.is_empty() && !name.is_empty() {
                    heritage.push(Heritage {
                        name,
                        base,
                        relation: "implements".into(),
                        line,
                    });
                }
            }
        }
        ("python", "import_statement") => {
            let mut c = node.walk();
            for ch in node.children_by_field_name("name", &mut c) {
                match ch.kind() {
                    "dotted_name" => {
                        let m = py_dotted(ch, src);
                        let local = m.split('.').next().unwrap_or(&m).to_owned();
                        // `import a.b` binds `a`; the module is `a.b`.
                        bindings.push(Binding {
                            local,
                            spec: m,
                            imported: None,
                            line,
                        });
                    }
                    "aliased_import" => {
                        let m = ch
                            .child_by_field_name("name")
                            .map(|x| py_dotted(x, src))
                            .unwrap_or_default();
                        let alias = ch
                            .child_by_field_name("alias")
                            .map(|x| text_of(x, src).to_owned())
                            .unwrap_or_default();
                        bindings.push(Binding {
                            local: alias,
                            spec: m,
                            imported: None,
                            line,
                        });
                    }
                    _ => {}
                }
            }
        }
        ("python", "import_from_statement") => {
            let module = node
                .child_by_field_name("module_name")
                .map(|m| py_dotted(m, src))
                .unwrap_or_default();
            let mut c = node.walk();
            for ch in node.children_by_field_name("name", &mut c) {
                match ch.kind() {
                    "dotted_name" => {
                        let n = py_dotted(ch, src);
                        bindings.push(Binding {
                            local: n.clone(),
                            spec: module.clone(),
                            imported: Some(n),
                            line,
                        });
                    }
                    "aliased_import" => {
                        let n = ch
                            .child_by_field_name("name")
                            .map(|x| py_dotted(x, src))
                            .unwrap_or_default();
                        let alias = ch
                            .child_by_field_name("alias")
                            .map(|x| text_of(x, src).to_owned())
                            .unwrap_or_default();
                        bindings.push(Binding {
                            local: alias,
                            spec: module.clone(),
                            imported: Some(n),
                            line,
                        });
                    }
                    _ => {}
                }
            }
        }
        ("python", "class_definition") => {
            if let (Some(name), Some(sup)) = (
                node.child_by_field_name("name"),
                node.child_by_field_name("superclasses"),
            ) {
                let mut c = sup.walk();
                for b in sup.named_children(&mut c) {
                    let base = match b.kind() {
                        "identifier" => text_of(b, src).to_owned(),
                        "attribute" => b
                            .child_by_field_name("attribute")
                            .map(|a| text_of(a, src).to_owned())
                            .unwrap_or_default(),
                        _ => continue,
                    };
                    if !base.is_empty() && base != "object" {
                        heritage.push(Heritage {
                            name: text_of(name, src).to_owned(),
                            base,
                            relation: "extends".into(),
                            line,
                        });
                    }
                }
            }
        }
        ("javascript" | "typescript", "import_statement") => {
            let spec = node
                .child_by_field_name("source")
                .map(|s| {
                    text_of(s, src)
                        .trim_matches(|c| c == '"' || c == '\'')
                        .to_owned()
                })
                .unwrap_or_default();
            let mut c = node.walk();
            for clause in node.named_children(&mut c) {
                if clause.kind() != "import_clause" {
                    continue;
                }
                let mut cc = clause.walk();
                for item in clause.named_children(&mut cc) {
                    match item.kind() {
                        "identifier" => bindings.push(Binding {
                            local: text_of(item, src).to_owned(),
                            spec: spec.clone(),
                            imported: Some("default".into()),
                            line,
                        }),
                        "namespace_import" => {
                            let mut nc = item.walk();
                            if let Some(id) = item
                                .named_children(&mut nc)
                                .find(|x| x.kind() == "identifier")
                            {
                                bindings.push(Binding {
                                    local: text_of(id, src).to_owned(),
                                    spec: spec.clone(),
                                    imported: None,
                                    line,
                                });
                            }
                        }
                        "named_imports" => {
                            let mut nc = item.walk();
                            for sp in item.named_children(&mut nc) {
                                if sp.kind() != "import_specifier" {
                                    continue;
                                }
                                let name = sp
                                    .child_by_field_name("name")
                                    .map(|x| text_of(x, src).to_owned())
                                    .unwrap_or_default();
                                let alias = sp
                                    .child_by_field_name("alias")
                                    .map(|x| text_of(x, src).to_owned());
                                bindings.push(Binding {
                                    local: alias.unwrap_or_else(|| name.clone()),
                                    spec: spec.clone(),
                                    imported: Some(name),
                                    line,
                                });
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
        (
            "javascript" | "typescript",
            "class_declaration" | "abstract_class_declaration" | "class",
        ) => {
            let Some(name) = node
                .child_by_field_name("name")
                .map(|n| text_of(n, src).to_owned())
            else {
                return;
            };
            let mut c = node.walk();
            for h in node.named_children(&mut c) {
                if h.kind() != "class_heritage" {
                    continue;
                }
                let mut hc = h.walk();
                let clauses: Vec<Node> = h.named_children(&mut hc).collect();
                for cl in clauses {
                    match cl.kind() {
                        "extends_clause" => {
                            let mut xc = cl.walk();
                            for v in cl.named_children(&mut xc) {
                                if matches!(v.kind(), "identifier" | "type_identifier") {
                                    heritage.push(Heritage {
                                        name: name.clone(),
                                        base: text_of(v, src).to_owned(),
                                        relation: "extends".into(),
                                        line,
                                    });
                                } else if v.kind() == "member_expression"
                                    && let Some(p) = v.child_by_field_name("property")
                                {
                                    heritage.push(Heritage {
                                        name: name.clone(),
                                        base: text_of(p, src).to_owned(),
                                        relation: "extends".into(),
                                        line,
                                    });
                                }
                            }
                        }
                        "implements_clause" => {
                            let mut xc = cl.walk();
                            for v in cl.named_children(&mut xc) {
                                let b = type_name(v, src);
                                if !b.is_empty() {
                                    heritage.push(Heritage {
                                        name: name.clone(),
                                        base: b,
                                        relation: "implements".into(),
                                        line,
                                    });
                                }
                            }
                        }
                        // JavaScript: `class A extends B` has the base directly.
                        "identifier" | "type_identifier" => {
                            heritage.push(Heritage {
                                name: name.clone(),
                                base: text_of(cl, src).to_owned(),
                                relation: "extends".into(),
                                line,
                            });
                        }
                        "member_expression" => {
                            if let Some(p) = cl.child_by_field_name("property") {
                                heritage.push(Heritage {
                                    name: name.clone(),
                                    base: text_of(p, src).to_owned(),
                                    relation: "extends".into(),
                                    line,
                                });
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
        ("typescript", "interface_declaration") => {
            let Some(name) = node
                .child_by_field_name("name")
                .map(|n| text_of(n, src).to_owned())
            else {
                return;
            };
            let mut c = node.walk();
            for h in node.named_children(&mut c) {
                if h.kind() != "extends_type_clause" {
                    continue;
                }
                let mut hc = h.walk();
                for v in h.named_children(&mut hc) {
                    let b = type_name(v, src);
                    if !b.is_empty() {
                        heritage.push(Heritage {
                            name: name.clone(),
                            base: b,
                            relation: "extends".into(),
                            line,
                        });
                    }
                }
            }
        }
        _ => {}
    }
}

/// Extract the facts of one file with the real tree-sitter grammar of its
/// language; an unsupported language has none.
#[must_use]
pub fn extract(path: &str, language: &str, text: &str, content_hash: &str) -> FileFacts {
    let mut facts = FileFacts {
        language: language.to_owned(),
        content_hash: content_hash.to_owned(),
        ..FileFacts::default()
    };
    let Some(lang) = grammar(language, path) else {
        return facts;
    };
    let mut parser = Parser::new();
    if parser.set_language(&lang).is_err() {
        return facts;
    }
    let Some(tree) = parser.parse(text, None) else {
        return facts;
    };
    let mut call_sites: HashMap<usize, (bool, Option<String>)> = HashMap::new();
    let mut seen: HashSet<(String, Option<String>, u32, OccKind)> = HashSet::new();
    let mut stack: Vec<Node> = vec![tree.root_node()];
    while let Some(node) = stack.pop() {
        collect_bindings_and_heritage(
            language,
            node,
            text,
            &mut facts.bindings,
            &mut facts.heritage,
        );
        // Calls first: mark the callee so the identifier visit says `Call`.
        let is_call = match language {
            "rust" => node.kind() == "call_expression",
            "python" => node.kind() == "call",
            _ => matches!(node.kind(), "call_expression" | "new_expression"),
        };
        if is_call && let Some((_, q, at)) = callee_of(language, node, text) {
            call_sites.insert(at, (true, q));
        }
        // Rust struct literals construct their type.
        if language == "rust"
            && node.kind() == "struct_expression"
            && let Some(n) = node.child_by_field_name("name")
        {
            let (name, q, at) = match n.kind() {
                "scoped_type_identifier" | "scoped_identifier" => {
                    let nm = n.child_by_field_name("name");
                    (
                        nm.map(|x| text_of(x, text).to_owned()).unwrap_or_default(),
                        n.child_by_field_name("path")
                            .map(|p| text_of(p, text).to_owned()),
                        nm.map_or(n.start_byte(), |x| x.start_byte()),
                    )
                }
                _ => (text_of(n, text).to_owned(), None, n.start_byte()),
            };
            if !name.is_empty() {
                call_sites.insert(at, (true, q));
            }
        }
        if is_ident_kind(language, node.kind())
            && facts.occurrences.len() < MAX_OCCURRENCES_PER_FILE
            && let Some(parent) = node.parent()
            && !defines_name(language, parent, node)
            && !in_declaration_context(language, node)
        {
            let name = text_of(node, text);
            // A struct field read is not a use of a definition.
            let plain_field = language == "rust"
                && node.kind() == "field_identifier"
                && !call_sites.contains_key(&node.start_byte());
            if !name.is_empty() && !plain_field {
                let is_call_site = call_sites.get(&node.start_byte()).cloned();
                // Qualifier chains: only the final segment is an occurrence
                // of its own; the earlier ones are mentioned by it.
                let mut qualifier = match &is_call_site {
                    Some((_, q)) => q.clone(),
                    None => qualifier_of(parent, node, text),
                };
                let mut kind = if is_call_site.is_some() {
                    OccKind::Call
                } else {
                    OccKind::Reference
                };
                // A path segment in the middle of `a::b::c` is the qualifier of
                // `c`, not an occurrence.
                let mut mid_path = matches!(
                    parent.kind(),
                    "scoped_identifier" | "scoped_type_identifier"
                ) && parent
                    .child_by_field_name("name")
                    .is_none_or(|n| n.id() != node.id());
                // A macro's arguments are a token tree, not parsed
                // expressions (`println!("{}", f(x))`): read the call shape
                // off the tokens, so a call inside a macro is still a call.
                if language == "rust"
                    && parent.kind() == "token_tree"
                    && node.kind() == "identifier"
                {
                    let next = node.next_sibling();
                    let prev = node.prev_sibling();
                    if next.is_some_and(|n| n.kind() == "::") {
                        mid_path = true;
                    } else {
                        let mut qual: Option<String> = None;
                        if let Some(p) = prev {
                            if p.kind() == "::" {
                                let mut segs: Vec<String> = Vec::new();
                                let mut cur = p;
                                while let Some(before) = cur.prev_sibling() {
                                    if before.kind() != "identifier" {
                                        break;
                                    }
                                    segs.push(text_of(before, text).to_owned());
                                    match before.prev_sibling() {
                                        Some(pp) if pp.kind() == "::" => cur = pp,
                                        _ => break,
                                    }
                                }
                                segs.reverse();
                                if !segs.is_empty() {
                                    qual = Some(segs.join("::"));
                                }
                            } else if p.kind() == "."
                                && let Some(b) = p.prev_sibling()
                                && b.kind() == "identifier"
                            {
                                qual = Some(text_of(b, text).to_owned());
                            }
                        }
                        let is_call = next.is_some_and(|n| {
                            n.kind() == "token_tree" && text_of(n, text).starts_with('(')
                        });
                        qualifier = qual;
                        kind = if is_call {
                            OccKind::Call
                        } else {
                            OccKind::Reference
                        };
                    }
                }
                if !mid_path {
                    let line = node.start_position().row as u32 + 1;
                    if seen.insert((name.to_owned(), qualifier.clone(), line, kind)) {
                        facts.occurrences.push(Occurrence {
                            name: name.to_owned(),
                            qualifier,
                            line,
                            kind,
                            enclosing: enclosing_label(language, node, text),
                        });
                    }
                }
            }
        } else if facts.occurrences.len() >= MAX_OCCURRENCES_PER_FILE {
            facts.truncated = true;
        }
        let mut c = node.walk();
        let children: Vec<Node> = node.children(&mut c).collect();
        for ch in children.into_iter().rev() {
            stack.push(ch);
        }
    }
    facts
}

/// Confidence class of an edge.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    /// Said by an import binding, a qualified path, the same file or the
    /// defining type's own name.
    Resolved,
    /// Said only by a name, perhaps shared by several definitions.
    Ambiguous,
    /// No workspace definition carries the name.
    Unresolved,
}

impl Confidence {
    /// Stable label.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Confidence::Resolved => "resolved",
            Confidence::Ambiguous => "ambiguous",
            Confidence::Unresolved => "unresolved",
        }
    }

    /// The weight an edge of this class carries in ranking.
    #[must_use]
    pub fn weight(self) -> f64 {
        match self {
            Confidence::Resolved => 1.0,
            Confidence::Ambiguous => 0.5,
            Confidence::Unresolved => 0.1,
        }
    }
}

/// What an edge is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EdgeKind {
    /// A mention of the target.
    Reference,
    /// A call (or construction) of the target.
    Call,
    /// The source type implements the target interface or trait.
    Implements,
    /// The source type extends the target base class or interface.
    Extends,
}

impl EdgeKind {
    /// Stable label.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            EdgeKind::Reference => "reference",
            EdgeKind::Call => "call",
            EdgeKind::Implements => "implements",
            EdgeKind::Extends => "extends",
        }
    }
}

/// One resolved edge from a source location to a definition.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Edge {
    /// Kind.
    pub kind: EdgeKind,
    /// The file the edge starts in.
    pub from_path: String,
    /// 1-based line in it.
    pub from_line: u32,
    /// The definition it starts in (`Container::name`), when there is one.
    pub from_symbol: Option<String>,
    /// The file defining the target.
    pub to_path: String,
    /// The target's name.
    pub to_symbol: String,
    /// The target's container, when it is a member.
    pub to_container: Option<String>,
    /// 1-based first line of the target definition.
    pub to_line: u32,
    /// Confidence class.
    pub confidence: Confidence,
    /// How many definitions the name could mean (1 for a resolved edge).
    pub candidates: u32,
    /// The revision the edge holds at.
    pub revision: u64,
}

/// A definition the graph resolves to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Def {
    /// File.
    pub path: String,
    /// Name.
    pub name: String,
    /// Container (an impl's type, a class), when a member.
    pub container: Option<String>,
    /// Kind (`function`, `method`, `class`, `struct`, `trait`, ...).
    pub kind: String,
    /// 1-based first line.
    pub line: u32,
}

/// Kinds a reference can point at (an `impl` block is its type, not a target).
fn is_target_kind(kind: &str) -> bool {
    !matches!(kind, "impl" | "module")
}

fn is_base_kind(kind: &str) -> bool {
    matches!(
        kind,
        "trait" | "interface" | "class" | "struct" | "enum" | "type"
    )
}

fn stem_of(path: &str) -> String {
    let file = path.rsplit('/').next().unwrap_or(path);
    let stem = file.rsplit_once('.').map_or(file, |(s, _)| s);
    if matches!(stem, "mod" | "index" | "lib" | "__init__") {
        let dir = path.rsplit_once('/').map_or("", |(d, _)| d);
        return dir.rsplit('/').next().unwrap_or("").to_owned();
    }
    stem.to_owned()
}

fn dir_of(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(d, _)| d)
}

/// The resolved reference graph of one workspace.
#[derive(Debug, Default)]
pub struct RefGraph {
    revision: u64,
    facts: BTreeMap<String, FileFacts>,
    paths: BTreeSet<String>,
    defs_by_name: BTreeMap<String, Vec<Def>>,
    defs_by_file: BTreeMap<String, Vec<Def>>,
    /// Occurrence and heritage names → files that mention them.
    mentions: BTreeMap<String, BTreeSet<String>>,
    /// Import-binding module stems → files that import them.
    importers_of_stem: BTreeMap<String, BTreeSet<String>>,
    edges: BTreeMap<String, Vec<Edge>>,
    /// Target name → files holding an edge to it.
    incoming: BTreeMap<String, BTreeSet<String>>,
    unresolved: BTreeMap<String, u32>,
    /// Names a file defined before its last refresh and no longer defines.
    removed: BTreeMap<String, BTreeSet<String>>,
    /// Occurrences resolved or refused in the last resolution pass.
    resolutions: u64,
}

impl RefGraph {
    /// Build from facts at `revision`: every file's facts, the symbol index
    /// the definitions come from, and (for import resolution) the workspace
    /// path set — taken from the facts' keys plus `extra_paths`.
    #[must_use]
    pub fn from_facts(
        facts: BTreeMap<String, FileFacts>,
        symbols: &SymbolIndex,
        extra_paths: impl IntoIterator<Item = String>,
        revision: u64,
    ) -> Self {
        let mut g = Self {
            revision,
            ..Self::default()
        };
        g.paths = facts.keys().cloned().chain(extra_paths).collect();
        for (path, syms) in symbols.files() {
            g.add_defs(path, syms);
        }
        for (p, f) in facts {
            g.index_mentions(&p, &f);
            g.facts.insert(p, f);
        }
        let all: Vec<String> = g.facts.keys().cloned().collect();
        for p in all {
            g.resolve_file(&p);
        }
        g
    }

    /// Revision.
    #[must_use]
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Files with facts.
    #[must_use]
    pub fn file_count(&self) -> usize {
        self.facts.len()
    }

    /// Edges in the graph.
    #[must_use]
    pub fn edge_count(&self) -> usize {
        self.edges.values().map(Vec::len).sum()
    }

    /// Occurrences with no workspace definition, in total.
    #[must_use]
    pub fn unresolved_count(&self) -> u64 {
        self.unresolved.values().map(|n| u64::from(*n)).sum()
    }

    /// The facts of a file.
    #[must_use]
    pub fn facts(&self, path: &str) -> Option<&FileFacts> {
        self.facts.get(path)
    }

    /// Every file's facts (for persistence).
    pub fn all_facts(&self) -> impl Iterator<Item = (&String, &FileFacts)> {
        self.facts.iter()
    }

    /// Whether a file is in the graph.
    #[must_use]
    pub fn contains(&self, path: &str) -> bool {
        self.facts.contains_key(path)
    }

    /// Definitions in a file.
    #[must_use]
    pub fn defs_in(&self, path: &str) -> &[Def] {
        self.defs_by_file.get(path).map_or(&[], Vec::as_slice)
    }

    /// Definitions of a name.
    #[must_use]
    pub fn defs_named(&self, name: &str) -> &[Def] {
        self.defs_by_name.get(name).map_or(&[], Vec::as_slice)
    }

    /// Names `path` defined before its last refresh and no longer defines.
    #[must_use]
    pub fn removed_names(&self, path: &str) -> Vec<String> {
        self.removed
            .get(path)
            .map(|s| s.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// Edges starting in a file.
    #[must_use]
    pub fn edges_from(&self, path: &str) -> &[Edge] {
        self.edges.get(path).map_or(&[], Vec::as_slice)
    }

    /// Files that mention a name (occurrence or inheritance), for a query's
    /// candidate set.
    #[must_use]
    pub fn files_mentioning(&self, name: &str) -> Vec<&String> {
        self.mentions
            .get(name)
            .map(|s| s.iter().collect())
            .unwrap_or_default()
    }

    /// Edges whose target is a definition named `name` (in `to_path`, when
    /// given), of the given kinds, in path and line order.
    #[must_use]
    pub fn edges_to(&self, name: &str, to_path: Option<&str>, kinds: &[EdgeKind]) -> Vec<&Edge> {
        let mut out: Vec<&Edge> = Vec::new();
        for from in self.incoming.get(name).into_iter().flatten() {
            for e in self.edges_from(from) {
                if e.to_symbol == name
                    && kinds.contains(&e.kind)
                    && to_path.is_none_or(|p| e.to_path == p)
                {
                    out.push(e);
                }
            }
        }
        out.sort_by(|a, b| {
            (&a.from_path, a.from_line, a.kind).cmp(&(&b.from_path, b.from_line, b.kind))
        });
        out
    }

    /// Edges starting in the definition `name` of `def_path` (its callees and
    /// references).
    #[must_use]
    pub fn edges_in_def(&self, def_path: &str, name: &str, kinds: &[EdgeKind]) -> Vec<&Edge> {
        self.edges_from(def_path)
            .iter()
            .filter(|e| {
                kinds.contains(&e.kind)
                    && e.from_symbol
                        .as_deref()
                        .is_some_and(|s| s == name || s.rsplit("::").next() == Some(name))
            })
            .collect()
    }

    fn add_defs(&mut self, path: &str, syms: &[crate::symbols::Symbol]) {
        let defs: Vec<Def> = syms
            .iter()
            .filter(|s| is_target_kind(&s.kind))
            .map(|s| Def {
                path: path.to_owned(),
                name: s.name.clone(),
                container: s.container.clone(),
                kind: s.kind.clone(),
                line: s.line_start,
            })
            .collect();
        for d in &defs {
            self.defs_by_name
                .entry(d.name.clone())
                .or_default()
                .push(d.clone());
        }
        if !defs.is_empty() {
            self.defs_by_file.insert(path.to_owned(), defs);
        }
    }

    fn drop_defs(&mut self, path: &str) -> Vec<Def> {
        let old = self.defs_by_file.remove(path).unwrap_or_default();
        for d in &old {
            if let Some(v) = self.defs_by_name.get_mut(&d.name) {
                v.retain(|x| x.path != path);
                if v.is_empty() {
                    self.defs_by_name.remove(&d.name);
                }
            }
        }
        old
    }

    /// What resolution can see of a definition: its name, container and
    /// kind. Where it sits in its file is not part of it.
    fn signature(d: &Def) -> (String, Option<String>, String) {
        (d.name.clone(), d.container.clone(), d.kind.clone())
    }

    /// A definition that only moved within its file leaves every dependent's
    /// resolution as it was; only the line its edges point at changes.
    fn patch_target_lines(&mut self, path: &str, names: &BTreeSet<String>) {
        let defs: Vec<Def> = self.defs_by_file.get(path).cloned().unwrap_or_default();
        for name in names {
            let froms: Vec<String> = self
                .incoming
                .get(name)
                .map(|s| s.iter().cloned().collect())
                .unwrap_or_default();
            for from in froms {
                let Some(list) = self.edges.get_mut(&from) else {
                    continue;
                };
                for e in list
                    .iter_mut()
                    .filter(|e| e.to_path == path && &e.to_symbol == name)
                {
                    if let Some(d) = defs
                        .iter()
                        .find(|d| &d.name == name && d.container == e.to_container)
                    {
                        e.to_line = d.line;
                    }
                }
            }
        }
    }

    fn index_mentions(&mut self, path: &str, f: &FileFacts) {
        for o in &f.occurrences {
            self.mentions
                .entry(o.name.clone())
                .or_default()
                .insert(path.to_owned());
        }
        for h in &f.heritage {
            self.mentions
                .entry(h.base.clone())
                .or_default()
                .insert(path.to_owned());
            self.mentions
                .entry(h.name.clone())
                .or_default()
                .insert(path.to_owned());
        }
        for b in &f.bindings {
            let stem = last_segment(&b.spec).to_owned();
            self.importers_of_stem
                .entry(stem)
                .or_default()
                .insert(path.to_owned());
            if let Some(i) = &b.imported {
                self.importers_of_stem
                    .entry(last_segment(i).to_owned())
                    .or_default()
                    .insert(path.to_owned());
            }
        }
    }

    fn drop_mentions(&mut self, path: &str) {
        let Some(f) = self.facts.get(path).cloned() else {
            return;
        };
        let mut names: Vec<String> = f.occurrences.iter().map(|o| o.name.clone()).collect();
        for h in &f.heritage {
            names.push(h.base.clone());
            names.push(h.name.clone());
        }
        for n in names {
            if let Some(s) = self.mentions.get_mut(&n) {
                s.remove(path);
                if s.is_empty() {
                    self.mentions.remove(&n);
                }
            }
        }
        for b in &f.bindings {
            let mut stems = vec![last_segment(&b.spec).to_owned()];
            if let Some(i) = &b.imported {
                stems.push(last_segment(i).to_owned());
            }
            for stem in stems {
                if let Some(s) = self.importers_of_stem.get_mut(&stem) {
                    s.remove(path);
                    if s.is_empty() {
                        self.importers_of_stem.remove(&stem);
                    }
                }
            }
        }
    }

    fn drop_edges(&mut self, path: &str) {
        if let Some(old) = self.edges.remove(path) {
            let names: BTreeSet<String> = old.into_iter().map(|e| e.to_symbol).collect();
            for n in names {
                if let Some(s) = self.incoming.get_mut(&n) {
                    s.remove(path);
                    if s.is_empty() {
                        self.incoming.remove(&n);
                    }
                }
            }
        }
        self.unresolved.remove(path);
    }

    /// Refresh changed files at `revision`: `Some(facts)` re-reads a file,
    /// `None` removes it; `symbols` is already refreshed. Re-resolves the
    /// changed files and every file whose resolution could have moved: those
    /// mentioning a name a changed file defined or defines now, and those
    /// importing a changed file's module. Returns the paths re-resolved.
    pub fn refresh(
        &mut self,
        changed: Vec<(String, Option<FileFacts>)>,
        symbols: &SymbolIndex,
        revision: u64,
    ) -> Vec<String> {
        let mut affected_names: BTreeSet<String> = BTreeSet::new();
        let mut affected_stems: BTreeSet<String> = BTreeSet::new();
        let mut redo: BTreeSet<String> = BTreeSet::new();
        let mut moved: Vec<(String, BTreeSet<String>)> = Vec::new();
        for (path, facts) in changed {
            let was_known = self.facts.contains_key(&path);
            let before_defs = self.drop_defs(&path);
            self.drop_edges(&path);
            self.drop_mentions(&path);
            self.facts.remove(&path);
            self.add_defs(&path, symbols.symbols_in(&path));
            let before: BTreeSet<String> = before_defs.iter().map(|d| d.name.clone()).collect();
            let after_defs: Vec<Def> = self.defs_by_file.get(&path).cloned().unwrap_or_default();
            let after: BTreeSet<String> = after_defs.iter().map(|x| x.name.clone()).collect();
            let removed: BTreeSet<String> = before.difference(&after).cloned().collect();
            if removed.is_empty() {
                self.removed.remove(&path);
            } else {
                self.removed.insert(path.clone(), removed.clone());
            }
            // Only a definition that appeared, vanished or changed what
            // resolution can see (name, container, kind) can change what a
            // mention elsewhere means.
            let sig_before: BTreeSet<_> = before_defs.iter().map(Self::signature).collect();
            let sig_after: BTreeSet<_> = after_defs.iter().map(Self::signature).collect();
            for (name, ..) in sig_before.symmetric_difference(&sig_after) {
                affected_names.insert(name.clone());
            }
            let unchanged_names: BTreeSet<String> = sig_before
                .intersection(&sig_after)
                .map(|(n, ..)| n.clone())
                .collect();
            moved.push((path.clone(), unchanged_names));
            // A file that joins or leaves the workspace can change what an
            // import elsewhere resolves to; one that is only edited cannot.
            if !was_known || facts.is_none() {
                affected_stems.insert(stem_of(&path));
            }
            match facts {
                Some(f) => {
                    self.paths.insert(path.clone());
                    self.index_mentions(&path, &f);
                    self.facts.insert(path.clone(), f);
                    redo.insert(path);
                }
                None => {
                    self.paths.remove(&path);
                    self.defs_by_file.remove(&path);
                }
            }
        }
        for n in &affected_names {
            if let Some(files) = self.mentions.get(n) {
                redo.extend(files.iter().cloned());
            }
        }
        for s in &affected_stems {
            if let Some(files) = self.importers_of_stem.get(s) {
                redo.extend(files.iter().cloned());
            }
        }
        self.revision = revision;
        // An edge that survived the change holds at the new revision too.
        for e in self.edges.values_mut().flatten() {
            e.revision = revision;
        }
        let redo: Vec<String> = redo
            .into_iter()
            .filter(|p| self.facts.contains_key(p))
            .collect();
        for p in &redo {
            self.drop_edges(p);
            self.resolve_file(p);
        }
        for (path, names) in &moved {
            self.patch_target_lines(path, names);
        }
        redo
    }

    fn bind_target(&self, from: &str, language: &str, b: &Binding) -> Option<String> {
        // `from pkg import mod` may bind a module itself.
        if language == "python"
            && let Some(i) = &b.imported
            && let Some(p) = resolve_import(
                language,
                from,
                &format!("{}.{i}", b.spec.trim_end_matches('.')),
                &self.paths,
            )
            && p != from
        {
            return Some(p);
        }
        let spec = if language == "python" && b.spec.chars().all(|c| c == '.') {
            // `from . import x`: the package itself.
            b.spec.clone()
        } else {
            b.spec.clone()
        };
        resolve_import(language, from, &spec, &self.paths).filter(|p| p != from)
    }

    fn resolve_file(&mut self, path: &str) {
        let Some(f) = self.facts.get(path).cloned() else {
            return;
        };
        let language = f.language.clone();
        let mut bindings: HashMap<&str, (&Binding, Option<String>)> = HashMap::new();
        for b in &f.bindings {
            bindings.insert(b.local.as_str(), (b, self.bind_target(path, &language, b)));
        }
        let mut out: Vec<Edge> = Vec::new();
        let mut unresolved = 0u32;
        let revision = self.revision;
        let file_defs: Vec<Def> = self.defs_by_file.get(path).cloned().unwrap_or_default();
        let push = |out: &mut Vec<Edge>,
                    kind: EdgeKind,
                    line: u32,
                    from: Option<&String>,
                    d: &Def,
                    conf: Confidence,
                    cands: usize| {
            out.push(Edge {
                kind,
                from_path: path.to_owned(),
                from_line: line,
                from_symbol: from.cloned(),
                to_path: d.path.clone(),
                to_symbol: d.name.clone(),
                to_container: d.container.clone(),
                to_line: d.line,
                confidence: conf,
                candidates: u32::try_from(cands).unwrap_or(u32::MAX),
                revision,
            });
        };
        for o in &f.occurrences {
            let kind = match o.kind {
                OccKind::Call => EdgeKind::Call,
                OccKind::Reference => EdgeKind::Reference,
            };
            let (targets, conf) = self.resolve_name(
                path,
                &language,
                &o.name,
                o.qualifier.as_deref(),
                o.enclosing.as_deref(),
                &file_defs,
                &bindings,
                matches!(o.kind, OccKind::Call),
            );
            if targets.is_empty() {
                unresolved += 1;
                continue;
            }
            // A mention of a definition from inside that very definition is a
            // recursion or a self-name, not a dependency worth an edge.
            let n = targets.len();
            for d in targets.iter().take(MAX_AMBIGUOUS_TARGETS) {
                if d.path == path
                    && o.enclosing
                        .as_deref()
                        .is_some_and(|e| e == d.name || e.ends_with(&format!("::{}", d.name)))
                    && d.line == o.line
                {
                    continue;
                }
                push(&mut out, kind, o.line, o.enclosing.as_ref(), d, conf, n);
            }
        }
        for h in &f.heritage {
            let kind = if h.relation == "implements" {
                EdgeKind::Implements
            } else {
                EdgeKind::Extends
            };
            let (targets, conf) = self.resolve_name(
                path, &language, &h.base, None, None, &file_defs, &bindings, false,
            );
            let targets: Vec<Def> = targets
                .into_iter()
                .filter(|d| is_base_kind(&d.kind))
                .collect();
            if targets.is_empty() {
                unresolved += 1;
                continue;
            }
            let n = targets.len();
            let implementor = h.name.clone();
            for d in targets.iter().take(MAX_AMBIGUOUS_TARGETS) {
                push(&mut out, kind, h.line, Some(&implementor), d, conf, n);
            }
        }
        out.sort_by(|a, b| {
            (a.from_line, a.kind, &a.to_path, &a.to_symbol).cmp(&(
                b.from_line,
                b.kind,
                &b.to_path,
                &b.to_symbol,
            ))
        });
        out.dedup();
        let names: BTreeSet<String> = out.iter().map(|e| e.to_symbol.clone()).collect();
        for n in names {
            self.incoming.entry(n).or_default().insert(path.to_owned());
        }
        if !out.is_empty() {
            self.edges.insert(path.to_owned(), out);
        }
        if unresolved > 0 {
            self.unresolved.insert(path.to_owned(), unresolved);
        }
        self.resolutions += 1;
    }

    #[allow(clippy::too_many_arguments)]
    fn resolve_name(
        &self,
        from: &str,
        language: &str,
        name: &str,
        qualifier: Option<&str>,
        enclosing: Option<&str>,
        file_defs: &[Def],
        bindings: &HashMap<&str, (&Binding, Option<String>)>,
        is_call: bool,
    ) -> (Vec<Def>, Confidence) {
        let named: &[Def] = self.defs_by_name.get(name).map_or(&[], Vec::as_slice);
        if named.is_empty() {
            return (vec![], Confidence::Unresolved);
        }
        let in_file = |defs: &[Def], file: &str| -> Vec<Def> {
            defs.iter().filter(|d| d.path == file).cloned().collect()
        };
        let one_or_ambiguous = |mut v: Vec<Def>, conf: Confidence| -> (Vec<Def>, Confidence) {
            if v.is_empty() {
                (v, Confidence::Unresolved)
            } else if v.len() == 1 {
                (v, conf)
            } else {
                v.sort_by(|a, b| (&a.path, a.line).cmp(&(&b.path, b.line)));
                (v, Confidence::Ambiguous)
            }
        };
        // A qualifier says where the name lives.
        if let Some(q) = qualifier {
            let first = q.split(['.', ':']).find(|s| !s.is_empty()).unwrap_or(q);
            let q_last = last_segment(q);
            if matches!(first, "self" | "Self" | "this" | "cls" | "super") {
                let mut mine = in_file(named, from);
                // A method of the type the call sits in.
                let container = enclosing.and_then(|e| e.split_once("::").map(|(c, _)| c));
                if let Some(c) = container {
                    let by_container: Vec<Def> = mine
                        .iter()
                        .filter(|d| d.container.as_deref() == Some(c))
                        .cloned()
                        .collect();
                    if !by_container.is_empty() {
                        mine = by_container;
                    }
                }
                if !mine.is_empty() {
                    return one_or_ambiguous(mine, Confidence::Resolved);
                }
                // An inherited member: defined in a base, found by name.
                return (
                    named.iter().take(MAX_AMBIGUOUS_TARGETS).cloned().collect(),
                    Confidence::Ambiguous,
                );
            }
            if let Some((b, target)) = bindings.get(first) {
                if let Some(t) = target {
                    let v = in_file(named, t);
                    if !v.is_empty() {
                        // `ns.f` / `mod::f` through a module import; `Type::f`
                        // through an imported type (its members live in the
                        // file that defines it, which `t` is).
                        return one_or_ambiguous(v, Confidence::Resolved);
                    }
                }
                // A name imported from elsewhere under that qualifier.
                if let Some(imp) = &b.imported
                    && let Some(defs) = self.defs_by_name.get(imp)
                {
                    let members: Vec<Def> = named
                        .iter()
                        .filter(|d| defs.iter().any(|t| d.container.as_deref() == Some(&t.name)))
                        .cloned()
                        .collect();
                    if !members.is_empty() {
                        return one_or_ambiguous(members, Confidence::Resolved);
                    }
                }
            }
            // `crate::foo::f` / `pkg.sub.f`: a path to a module file.
            if let Some(p) = resolve_import(language, from, &q.replace('.', "::"), &self.paths)
                .or_else(|| resolve_import(language, from, q, &self.paths))
            {
                let v = in_file(named, &p);
                if !v.is_empty() {
                    return one_or_ambiguous(v, Confidence::Resolved);
                }
            }
            // `Type::f` / `Type.f` where the qualifier names the container.
            let by_type: Vec<Def> = named
                .iter()
                .filter(|d| d.container.as_deref() == Some(q_last))
                .cloned()
                .collect();
            if !by_type.is_empty() {
                return one_or_ambiguous(by_type, Confidence::Resolved);
            }
            // A module qualifier by file stem.
            let by_stem: Vec<Def> = named
                .iter()
                .filter(|d| stem_of(&d.path) == q_last)
                .cloned()
                .collect();
            if !by_stem.is_empty() {
                return one_or_ambiguous(by_stem, Confidence::Resolved);
            }
            // The receiver is a variable: only the name speaks, and only
            // members and functions can be meant by `x.name(..)`.
            let members: Vec<Def> = named
                .iter()
                .filter(|d| {
                    if is_call {
                        matches!(d.kind.as_str(), "method" | "function")
                    } else {
                        true
                    }
                })
                .take(MAX_AMBIGUOUS_TARGETS)
                .cloned()
                .collect();
            return (members, Confidence::Ambiguous);
        }
        // An unqualified name: an import binding first.
        if let Some((b, target)) = bindings.get(name) {
            let wanted = b
                .imported
                .as_deref()
                .filter(|i| *i != "default")
                .unwrap_or(name);
            let wanted_defs: &[Def] = self.defs_by_name.get(wanted).map_or(&[], Vec::as_slice);
            if let Some(t) = target {
                let v = in_file(wanted_defs, t);
                if !v.is_empty() {
                    return one_or_ambiguous(v, Confidence::Resolved);
                }
                // A default export has its own name.
                if b.imported.as_deref() == Some("default") {
                    let v: Vec<Def> = self
                        .defs_by_file
                        .get(t)
                        .map(|ds| {
                            ds.iter()
                                .filter(|d| d.container.is_none())
                                .cloned()
                                .collect()
                        })
                        .unwrap_or_default();
                    if v.len() == 1 {
                        return (v, Confidence::Resolved);
                    }
                }
            }
            // Re-exported by the module it names: the import still says
            // which name, so a unique definition of it is resolved.
            if wanted_defs.len() == 1 {
                return (wanted_defs.to_vec(), Confidence::Resolved);
            }
            if !wanted_defs.is_empty() {
                return (
                    wanted_defs
                        .iter()
                        .take(MAX_AMBIGUOUS_TARGETS)
                        .cloned()
                        .collect(),
                    Confidence::Ambiguous,
                );
            }
        }
        // The same file.
        let mine = in_file(named, from);
        let mine_top: Vec<Def> = mine
            .iter()
            .filter(|d| d.container.is_none())
            .cloned()
            .collect();
        if !mine_top.is_empty() {
            return one_or_ambiguous(mine_top, Confidence::Resolved);
        }
        let _ = file_defs;
        if !mine.is_empty() && !is_call {
            return one_or_ambiguous(mine, Confidence::Resolved);
        }
        // A unique top-level definition elsewhere in the same directory (a
        // module's siblings through `use super::*`, a Python package).
        let dir = dir_of(from);
        let siblings: Vec<Def> = named
            .iter()
            .filter(|d| d.container.is_none() && dir_of(&d.path) == dir && d.path != from)
            .cloned()
            .collect();
        if siblings.len() == 1 {
            return (siblings, Confidence::Ambiguous);
        }
        // Only the name speaks.
        let global: Vec<Def> = named
            .iter()
            .filter(|d| d.container.is_none() || !is_call)
            .take(MAX_AMBIGUOUS_TARGETS)
            .cloned()
            .collect();
        if global.is_empty() {
            return (vec![], Confidence::Unresolved);
        }
        (global, Confidence::Ambiguous)
    }
}

// ---- Queries ---------------------------------------------------------------

/// What a symbol query answers.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SymbolEdges {
    /// The symbol asked about.
    pub symbol: String,
    /// Its definitions in the workspace.
    pub definitions: Vec<DefView>,
    /// The edges, in path and line order.
    pub edges: Vec<Edge>,
    /// Graph revision.
    pub revision: u64,
    /// A cap cut the answer short.
    pub partial: bool,
    /// Why.
    pub partial_reason: String,
}

/// A definition as a client shows it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DefView {
    /// File.
    pub path: String,
    /// Name.
    pub name: String,
    /// Kind.
    pub kind: String,
    /// Container.
    pub container: Option<String>,
    /// First line.
    pub line_start: u32,
}

impl RefGraph {
    /// Edges of one relation around a symbol: `references` (references and
    /// calls to it), `callers`, `callees` (what its definition calls),
    /// `implementors` (types implementing or extending it), `implemented_by`
    /// (what it implements or extends) or `all`.
    #[must_use]
    pub fn symbol_edges(
        &self,
        symbol: &str,
        path: Option<&str>,
        relation: &str,
        max: usize,
    ) -> SymbolEdges {
        let max = if max == 0 { 100 } else { max };
        let defs: Vec<&Def> = self
            .defs_named(symbol)
            .iter()
            .filter(|d| path.is_none_or(|p| d.path == p))
            .collect();
        let want = |r: &str| relation == "all" || relation.is_empty() || relation == r;
        let mut edges: Vec<Edge> = Vec::new();
        let to_path = path.filter(|_| !defs.is_empty());
        if want("references") {
            edges.extend(
                self.edges_to(symbol, to_path, &[EdgeKind::Reference, EdgeKind::Call])
                    .into_iter()
                    .cloned(),
            );
        } else if want("callers") {
            edges.extend(
                self.edges_to(symbol, to_path, &[EdgeKind::Call])
                    .into_iter()
                    .cloned(),
            );
        }
        if want("callees") {
            for d in &defs {
                edges.extend(
                    self.edges_in_def(&d.path, &symbol_label(d), &[EdgeKind::Call])
                        .into_iter()
                        .cloned(),
                );
            }
        }
        if want("implementors") {
            edges.extend(
                self.edges_to(symbol, to_path, &[EdgeKind::Implements, EdgeKind::Extends])
                    .into_iter()
                    .cloned(),
            );
        }
        if want("implemented_by") {
            for d in &defs {
                edges.extend(
                    self.edges_from(&d.path)
                        .iter()
                        .filter(|e| {
                            matches!(e.kind, EdgeKind::Implements | EdgeKind::Extends)
                                && e.from_symbol.as_deref() == Some(symbol)
                        })
                        .cloned(),
                );
            }
        }
        edges.sort_by(|a, b| {
            (&a.from_path, a.from_line, a.kind, &a.to_path).cmp(&(
                &b.from_path,
                b.from_line,
                b.kind,
                &b.to_path,
            ))
        });
        edges.dedup();
        let partial = edges.len() > max;
        edges.truncate(max);
        SymbolEdges {
            symbol: symbol.to_owned(),
            definitions: defs
                .iter()
                .map(|d| DefView {
                    path: d.path.clone(),
                    name: d.name.clone(),
                    kind: d.kind.clone(),
                    container: d.container.clone(),
                    line_start: d.line,
                })
                .collect(),
            edges,
            revision: self.revision,
            partial,
            partial_reason: if partial {
                format!("more than {max} edges; the answer is cut at {max}")
            } else {
                String::new()
            },
        }
    }

    /// All files holding an edge to any definition in `path`, with the best
    /// edge per file: `(from_path, edge)` pairs, one entry per source file
    /// and target symbol.
    #[must_use]
    pub fn dependents_of_file(&self, path: &str, only_symbols: Option<&[String]>) -> Vec<&Edge> {
        let mut out = Vec::new();
        let mut seen_names: BTreeSet<&str> = BTreeSet::new();
        for d in self.defs_in(path) {
            if only_symbols.is_some_and(|s| !s.contains(&d.name)) {
                continue;
            }
            if !seen_names.insert(d.name.as_str()) {
                continue;
            }
            for from in self.incoming.get(&d.name).into_iter().flatten() {
                if from == path {
                    continue;
                }
                for e in self.edges_from(from) {
                    if e.to_symbol == d.name && e.to_path == path {
                        out.push(e);
                    }
                }
            }
        }
        out
    }

    /// Files holding an occurrence whose name no definition carries any
    /// longer, among the names `path` used to define (a rename or a removal
    /// the dependents have not followed).
    #[must_use]
    pub fn dangling_dependents(&self, path: &str) -> Vec<(String, String, u32)> {
        let mut out = Vec::new();
        for name in self.removed.get(path).into_iter().flatten() {
            if self.defs_by_name.contains_key(name) {
                continue;
            }
            for file in self.mentions.get(name).into_iter().flatten() {
                if file == path {
                    continue;
                }
                if let Some(f) = self.facts.get(file)
                    && let Some(o) = f.occurrences.iter().find(|o| &o.name == name)
                {
                    out.push((file.clone(), name.clone(), o.line));
                }
            }
        }
        out.sort();
        out.dedup();
        out
    }
}

fn symbol_label(d: &Def) -> String {
    match &d.container {
        Some(c) => format!("{c}::{}", d.name),
        None => d.name.clone(),
    }
}
