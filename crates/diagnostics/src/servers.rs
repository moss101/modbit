//! Server registry: which real language server serves a language on this
//! host and how to start it (docs/76 Alpha languages). Resolution is honest:
//! a language whose server cannot be found is unavailable, never faked.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::lsp::LspError;

/// How to start a server.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServerSpec {
    /// Display name.
    pub name: String,
    /// Executable.
    pub command: PathBuf,
    /// Arguments.
    pub args: Vec<String>,
    /// LSP `languageId` for documents.
    pub language_id: String,
    /// `initializationOptions`.
    pub init_options: Value,
}

/// A path without the Windows verbatim prefix (`\\?\`), which node and
/// `CreateProcess` working directories do not accept.
#[must_use]
pub fn plain(path: &Path) -> PathBuf {
    let s = path.to_string_lossy();
    if let Some(rest) = s.strip_prefix(r"\\?\UNC\") {
        PathBuf::from(format!(r"\\{rest}"))
    } else if let Some(rest) = s.strip_prefix(r"\\?\") {
        PathBuf::from(rest)
    } else {
        path.to_path_buf()
    }
}

fn which(name: &str) -> Option<PathBuf> {
    let exe = if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_owned()
    };
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect())
        .unwrap_or_default();
    if let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")) {
        dirs.push(PathBuf::from(home).join(".cargo").join("bin"));
    }
    dirs.into_iter().map(|d| d.join(&exe)).find(|p| p.is_file())
}

/// A `node_modules` directory holding `package`: `MODBIT_NODE_MODULES`, then
/// the workspace root and its parents, then the Modbit executable's tree.
fn node_modules_with(package: &str, root: &Path, hint: Option<&Path>) -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(h) = hint {
        candidates.push(h.to_path_buf());
    }
    if let Some(p) = std::env::var_os("MODBIT_NODE_MODULES") {
        candidates.push(PathBuf::from(p));
    }
    let mut dir = Some(root.to_path_buf());
    while let Some(d) = dir {
        candidates.push(d.join("node_modules"));
        dir = d.parent().map(Path::to_path_buf);
    }
    if let Ok(exe) = std::env::current_exe() {
        let mut d = exe.parent().map(Path::to_path_buf);
        while let Some(p) = d {
            candidates.push(p.join("node_modules"));
            d = p.parent().map(Path::to_path_buf);
        }
    }
    candidates
        .into_iter()
        .find(|c| c.join(package).is_dir())
        .map(|c| plain(&c))
}

/// Resolve the server for `language` (label as in the retrieval index).
pub fn resolve_server(language: &str, root: &Path) -> Result<ServerSpec, LspError> {
    resolve_server_in(language, root, None)
}

/// `resolve_server` with an explicit `node_modules` to try first.
pub fn resolve_server_in(
    language: &str,
    root: &Path,
    node_modules: Option<&Path>,
) -> Result<ServerSpec, LspError> {
    let unavailable = |detail: String| LspError::Unavailable {
        language: language.to_owned(),
        detail,
    };
    match language {
        "rust" => {
            let command = which("rust-analyzer").ok_or_else(|| {
                unavailable("rust-analyzer is not on PATH or in ~/.cargo/bin (rustup component add rust-analyzer)".into())
            })?;
            Ok(ServerSpec {
                name: "rust-analyzer".into(),
                command,
                args: vec![],
                language_id: "rust".into(),
                // No cargo check on save: native diagnostics only, bounded and fast.
                init_options: json!({"checkOnSave": false, "cachePriming": {"enable": false}}),
            })
        }
        "typescript" | "javascript" => {
            let node = which("node").ok_or_else(|| unavailable("node is not on PATH".into()))?;
            let nm = node_modules_with("typescript-language-server", root, node_modules)
                .ok_or_else(|| {
                    unavailable(
                        "typescript-language-server is not installed in any reachable node_modules"
                            .into(),
                    )
                })?;
            // tsserver comes from the workspace's own typescript when present, else the
            // same node_modules that holds the server.
            // TypeScript 5's tsserver.js (the native TypeScript 7 ships none): the
            // `typescript-tsserver` alias first, then a plain `typescript` install.
            let tsserver = node_modules_with("typescript-tsserver", root, node_modules)
                .map(|d| {
                    d.join("typescript-tsserver")
                        .join("lib")
                        .join("tsserver.js")
                })
                .filter(|p| p.is_file())
                .or_else(|| {
                    node_modules_with("typescript", root, node_modules)
                        .map(|d| d.join("typescript").join("lib").join("tsserver.js"))
                        .filter(|p| p.is_file())
                })
                .ok_or_else(|| {
                    unavailable("typescript is not installed in any reachable node_modules".into())
                })?;
            let cli = nm
                .join("typescript-language-server")
                .join("lib")
                .join("cli.mjs");
            Ok(ServerSpec {
                name: "typescript-language-server".into(),
                command: node,
                args: vec![cli.to_string_lossy().into_owned(), "--stdio".into()],
                language_id: if language == "typescript" {
                    "typescript".into()
                } else {
                    "javascript".into()
                },
                init_options: json!({"tsserver": {"path": tsserver.to_string_lossy()}}),
            })
        }
        "python" => {
            let node = which("node").ok_or_else(|| unavailable("node is not on PATH".into()))?;
            let nm = node_modules_with("pyright", root, node_modules).ok_or_else(|| {
                unavailable("pyright is not installed in any reachable node_modules".into())
            })?;
            let cli = nm.join("pyright").join("langserver.index.js");
            Ok(ServerSpec {
                name: "pyright".into(),
                command: node,
                args: vec![cli.to_string_lossy().into_owned(), "--stdio".into()],
                language_id: "python".into(),
                init_options: json!({}),
            })
        }
        other => Err(unavailable(format!(
            "`{other}` has no language server at the Alpha baseline (docs/76)"
        ))),
    }
}
