//! M3.4: the headless bridge against the real language servers on the real
//! fixture repositories: diagnostics for a seeded defect, document symbols,
//! references and definitions, normalized into Modbit records; an
//! unsupported language is reported unavailable, never faked.

use std::path::{Path, PathBuf};
use std::time::Duration;

use modbit_diagnostics::{LanguageServer, LspError, Position, resolve_server, resolve_server_in};

fn fixture_copy(name: &str) -> (tempfile::TempDir, PathBuf) {
    let src = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/repos")
        .join(name);
    let dir = tempfile::tempdir().unwrap();
    fn copy(src: &Path, dst: &Path) {
        std::fs::create_dir_all(dst).unwrap();
        for e in std::fs::read_dir(src).unwrap().flatten() {
            let p = e.path();
            let name = e.file_name();
            if name == "target" || name == "node_modules" {
                continue;
            }
            if p.is_dir() {
                copy(&p, &dst.join(&name));
            } else {
                let bytes = std::fs::read(&p).unwrap();
                let bytes = if p.extension().is_some_and(|x| {
                    x == "rs" || x == "ts" || x == "py" || x == "toml" || x == "json"
                }) {
                    String::from_utf8_lossy(&bytes)
                        .replace("\r\n", "\n")
                        .into_bytes()
                } else {
                    bytes
                };
                std::fs::write(dst.join(&name), bytes).unwrap();
            }
        }
    }
    copy(&src, dir.path());
    let root = dir.path().canonicalize().unwrap();
    (dir, root)
}

fn node_modules() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../node_modules")
        .canonicalize()
        .unwrap()
}

fn start(language: &str, root: &Path) -> LanguageServer {
    // The repository's own node_modules serves the node-based servers in tests.
    let nm = node_modules();
    let spec =
        resolve_server_in(language, root, Some(&nm)).unwrap_or_else(|e| panic!("{language}: {e}"));
    LanguageServer::spawn(
        &spec.name,
        &spec.command,
        &spec.args,
        root,
        spec.init_options,
        Duration::from_secs(60),
    )
    .unwrap()
}

#[test]
fn rust_analyzer_reports_a_seeded_type_mismatch_symbols_references_and_definitions() {
    let (_d, root) = fixture_copy("rust-cli");
    let lib = std::fs::read_to_string(root.join("src/lib.rs")).unwrap();
    let seeded = format!("{lib}\npub fn seeded() -> u32 {{ \"not a number\" }}\n");
    std::fs::write(root.join("src/lib.rs"), &seeded).unwrap();
    let mut s = start("rust", &root);
    s.open("src/lib.rs", "rust", &seeded).unwrap();
    let diags = s
        .diagnostics("src/lib.rs", Duration::from_secs(120))
        .unwrap();
    let mismatch = diags
        .iter()
        .find(|d| d.severity == "error" && d.message.contains("expected"))
        .unwrap_or_else(|| panic!("{diags:?}"));
    assert_eq!(mismatch.path, "src/lib.rs");
    assert!(
        mismatch.range.start.line as usize >= lib.lines().count(),
        "{mismatch:?}"
    );
    assert_eq!(mismatch.source, "rust-analyzer");
    let symbols = s
        .document_symbols("src/lib.rs", Duration::from_secs(30))
        .unwrap();
    let names: Vec<&str> = symbols.iter().map(|x| x.name.as_str()).collect();
    assert!(
        names.contains(&"parse_quantity") && names.contains(&"seeded"),
        "{names:?}"
    );
    assert!(
        symbols
            .iter()
            .any(|x| x.name == "parse_quantity" && x.kind == "function")
    );
    let def = symbols.iter().find(|x| x.name == "parse_quantity").unwrap();
    let (line, col) = seeded
        .lines()
        .enumerate()
        .find_map(|(i, l)| {
            l.find("fn parse_quantity")
                .map(|c| (i as u32, c as u32 + 3))
        })
        .unwrap();
    assert_eq!(
        (def.selection.start.line, def.selection.start.character),
        (line, col),
        "selectionRange points at the name: {def:?}"
    );
    let refs = s
        .references(
            "src/lib.rs",
            Position {
                line,
                character: col,
            },
            Duration::from_secs(60),
        )
        .unwrap();
    assert!(
        refs.iter().any(|r| r.path == "src/lib.rs")
            && refs.iter().any(|r| r.path == "tests/quantities.rs"),
        "{refs:?}"
    );
    let tests_src = std::fs::read_to_string(root.join("tests/quantities.rs")).unwrap();
    s.open("tests/quantities.rs", "rust", &tests_src).unwrap();
    let (line, col) = tests_src
        .lines()
        .enumerate()
        .find_map(|(i, l)| l.find("parse_quantity(").map(|c| (i as u32, c as u32 + 2)))
        .unwrap();
    let defs = s
        .definition(
            "tests/quantities.rs",
            Position {
                line,
                character: col,
            },
            Duration::from_secs(60),
        )
        .unwrap();
    assert_eq!(
        defs.iter().map(|d| d.path.as_str()).collect::<Vec<_>>(),
        vec!["src/lib.rs"],
        "{defs:?}"
    );
    s.shutdown();
}

#[test]
fn typescript_language_server_reports_a_seeded_type_error_and_resolves_definitions() {
    let (_d, root) = fixture_copy("ts-webapp");
    let seeded = "import { totalCents } from \"../src/cart\";\nconst wrong: number = \"text\";\nexport const t = totalCents(1, 2);\n";
    std::fs::create_dir_all(root.join("test")).unwrap();
    std::fs::write(root.join("test/seeded.ts"), seeded).unwrap();
    let mut s = start("typescript", &root);
    s.open("test/seeded.ts", "typescript", seeded).unwrap();
    let diags = s
        .diagnostics("test/seeded.ts", Duration::from_secs(90))
        .unwrap();
    let err = diags
        .iter()
        .find(|d| d.severity == "error" && d.range.start.line == 1)
        .unwrap_or_else(|| panic!("{diags:?}"));
    assert!(err.message.contains("not assignable"), "{err:?}");
    assert_eq!(err.code.as_deref(), Some("2322"));
    let defs = s
        .definition(
            "test/seeded.ts",
            Position {
                line: 2,
                character: 20,
            },
            Duration::from_secs(60),
        )
        .unwrap();
    assert_eq!(
        defs.iter().map(|d| d.path.as_str()).collect::<Vec<_>>(),
        vec!["src/cart.ts"],
        "{defs:?}"
    );
    let symbols = s
        .document_symbols("src/cart.ts", Duration::from_secs(30))
        .unwrap_or_default();
    let _ = symbols;
    s.shutdown();
}

#[test]
fn pyright_reports_a_seeded_error_and_resolves_references() {
    let (_d, root) = fixture_copy("python-service");
    let seeded = "from service import total_cents\n\nx: int = \"text\"\ny = total_cents(1, 2)\n";
    std::fs::write(root.join("seeded.py"), seeded).unwrap();
    let mut s = start("python", &root);
    s.open("seeded.py", "python", seeded).unwrap();
    let diags = s.diagnostics("seeded.py", Duration::from_secs(90)).unwrap();
    let err = diags
        .iter()
        .find(|d| d.severity == "error" && d.range.start.line == 2)
        .unwrap_or_else(|| panic!("{diags:?}"));
    assert_eq!(err.source, "Pyright");
    assert!(
        err.message.contains("assignable") || err.message.contains("incompatible"),
        "{err:?}"
    );
    let refs = s
        .references(
            "seeded.py",
            Position {
                line: 3,
                character: 6,
            },
            Duration::from_secs(60),
        )
        .unwrap();
    assert!(
        refs.iter().any(|r| r.path == "service.py") && refs.iter().any(|r| r.path == "seeded.py"),
        "{refs:?}"
    );
    s.shutdown();
}

#[test]
fn unsupported_language_is_reported_unavailable_not_faked() {
    let d = tempfile::tempdir().unwrap();
    let e = resolve_server("go", d.path()).unwrap_err();
    assert!(
        matches!(e, LspError::Unavailable { ref language, .. } if language == "go"),
        "{e}"
    );
}
