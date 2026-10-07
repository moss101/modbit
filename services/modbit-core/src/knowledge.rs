//! The read commands of the code graph and the index store (PX-110, PX-111;
//! docs/18): `GetImpact` (what a change could break: ranked non-test
//! dependents with their edge paths and confidence, and the tests that cover
//! them), `GetSymbolEdges` (callers, callees, references and implementors of a
//! symbol) and `GetIndexStatus` (how a workspace's indexes came to be: loaded
//! from the persisted store or derived, why, what the last refresh cost).
//! All three are derived views of the workspace as it is: the filesystem and
//! the revision stay canonical, and nothing here grants a tool or a write.

use std::path::PathBuf;

use modbit_domain::task::Task;
use modbit_protocol::v1 as wire;
use modbit_retrieval::refs::SymbolEdges;

use crate::server::Core;

type Failure = (String, String);

fn root_of(task: &Task) -> Result<PathBuf, Failure> {
    let root = task.workspace_root.as_deref().ok_or_else(|| {
        (
            "NO_WORKSPACE".to_owned(),
            "the task has no workspace root".to_owned(),
        )
    })?;
    std::path::Path::new(root).canonicalize().map_err(|e| {
        (
            "NO_WORKSPACE".to_owned(),
            format!("workspace root `{root}`: {e}"),
        )
    })
}

fn busy() -> Failure {
    (
        "INDEX_BUSY".to_owned(),
        "the index is being refreshed".to_owned(),
    )
}

/// `GetImpact`.
pub(crate) async fn impact(
    core: &Core,
    task: &Task,
    p: &wire::GetImpact,
) -> Result<wire::ImpactResult, Failure> {
    let root = root_of(task)?;
    let to_failure = |e: anyhow::Error| ("INDEX_UNAVAILABLE".to_owned(), format!("{e:#}"));
    let index = core.tools.index(&root).await.map_err(to_failure)?;
    let symbols = core.tools.symbols(&root).await.map_err(to_failure)?;
    let graph = core.tools.graph(&root).await.map_err(to_failure)?;
    let refs = core.tools.refs(&root).await.map_err(to_failure)?;
    let idx = index.try_lock().map_err(|_| busy())?;
    let sym = symbols.try_lock().map_err(|_| busy())?;
    let gr = graph.try_lock().map_err(|_| busy())?;
    let rg = refs.try_lock().map_err(|_| busy())?;
    let mut paths = p.paths.clone();
    if paths.is_empty() {
        paths = gr.changed_paths();
    }
    let depth = if p.depth == 0 { 2 } else { p.depth };
    let max = if p.max == 0 { 50 } else { p.max as usize };
    let sel = modbit_retrieval::select_impacted_with_refs(&idx, &sym, &gr, &rg, &paths, depth, max);
    Ok(impact_wire(&sel))
}

/// An impact selection as the wire message.
pub(crate) fn impact_wire(sel: &modbit_retrieval::ImpactSelection) -> wire::ImpactResult {
    wire::ImpactResult {
        changed: sel.changed.clone(),
        dependents: sel
            .dependents
            .iter()
            .map(|d| wire::ImpactedFile {
                path: d.path.clone(),
                rank: d.rank,
                distance: d.distance,
                confidence: d.confidence.clone(),
                edge_path: d
                    .edge_path
                    .iter()
                    .map(|s| wire::ImpactEdgeStep {
                        from: s.from.clone(),
                        to: s.to.clone(),
                        kind: s.kind.clone(),
                        confidence: s.confidence.clone(),
                        symbol: s.symbol.clone(),
                        line: s.line,
                    })
                    .collect(),
                reasons: d.reasons.clone(),
                tests: d.tests.clone(),
            })
            .collect(),
        tests: sel
            .tests
            .iter()
            .map(|t| wire::ImpactedTestView {
                path: t.path.clone(),
                reasons: t.reasons.clone(),
                distance: t.distance,
                covers: t.covers.clone(),
            })
            .collect(),
        revision: sel.revision,
        partial: sel.partial,
        partial_reason: sel.partial_reason.clone(),
        symbols: sel.symbols.clone(),
        limitation: sel.limitation.clone(),
        ambiguous_edges: sel.ambiguous_edges,
        unresolved_edges: sel.unresolved_edges,
    }
}

/// `GetSymbolEdges`.
pub(crate) async fn symbol_edges(
    core: &Core,
    task: &Task,
    p: &wire::GetSymbolEdges,
) -> Result<wire::SymbolEdges, Failure> {
    if p.symbol.trim().is_empty() {
        return Err(("BAD_PAYLOAD".to_owned(), "symbol required".to_owned()));
    }
    let root = root_of(task)?;
    let refs = core
        .tools
        .refs(&root)
        .await
        .map_err(|e| ("INDEX_UNAVAILABLE".to_owned(), format!("{e:#}")))?;
    let rg = refs.try_lock().map_err(|_| busy())?;
    let relation = if p.relation.is_empty() {
        "all"
    } else {
        p.relation.as_str()
    };
    let path = (!p.path.is_empty()).then_some(p.path.as_str());
    let max = if p.max == 0 { 100 } else { p.max as usize };
    Ok(edges_wire(&rg.symbol_edges(
        p.symbol.trim(),
        path,
        relation,
        max,
    )))
}

fn edges_wire(e: &SymbolEdges) -> wire::SymbolEdges {
    wire::SymbolEdges {
        symbol: e.symbol.clone(),
        definitions: e
            .definitions
            .iter()
            .map(|d| wire::SymbolDefinitionView {
                path: d.path.clone(),
                name: d.name.clone(),
                kind: d.kind.clone(),
                container: d.container.clone().unwrap_or_default(),
                line_start: d.line_start,
                line_end: 0,
            })
            .collect(),
        edges: e
            .edges
            .iter()
            .map(|x| wire::SymbolEdgeView {
                kind: x.kind.label().to_owned(),
                from_path: x.from_path.clone(),
                from_line: x.from_line,
                from_symbol: x.from_symbol.clone().unwrap_or_default(),
                to_path: x.to_path.clone(),
                to_symbol: x.to_symbol.clone(),
                confidence: x.confidence.label().to_owned(),
                revision: x.revision,
            })
            .collect(),
        revision: e.revision,
        partial: e.partial,
        partial_reason: e.partial_reason.clone(),
    }
}

/// `GetIndexStatus`.
pub(crate) async fn index_status(
    core: &Core,
    task: &Task,
    _p: &wire::GetIndexStatus,
) -> Result<wire::IndexStatusView, Failure> {
    use std::sync::atomic::Ordering;
    let root = root_of(task)?;
    let state = core
        .tools
        .index_state(&root)
        .await
        .map_err(|e| ("INDEX_UNAVAILABLE".to_owned(), format!("{e:#}")))?;
    let s = state.snapshot();
    let revision = core
        .tools
        .index(&root)
        .await
        .ok()
        .and_then(|i| i.try_lock().ok().map(|i| i.revision()))
        .unwrap_or(0);
    Ok(wire::IndexStatusView {
        workspace_root: root.display().to_string(),
        store_dir: s.store_dir.clone(),
        workspace_revision: revision,
        builds: s.builds,
        loads: s.loads,
        refreshes: s.refreshes,
        last_refresh_ms: s.last_refresh_ms,
        last_refresh_files: s.last_refresh_files,
        components: s
            .components
            .iter()
            .map(|c| wire::IndexComponentView {
                name: c.name.clone(),
                state: c.state.clone(),
                files: c.files,
                persisted_bytes: 0,
                load_ms: if c.state == "loaded" || c.state == "hydrated" {
                    c.ms
                } else {
                    0
                },
                build_ms: if c.state == "built" || c.state == "rebuilt" {
                    c.ms
                } else {
                    0
                },
                reason: c.reason.clone(),
            })
            .collect(),
        rebuild_reasons: s.rebuild_reasons.clone(),
        persisted_bytes: s.persisted_bytes,
        persisted_generation: s.generation,
        first_ready_ms: s.first_ready_ms,
        searches_indexed: state.searches_indexed.load(Ordering::Relaxed),
        searches_scanned: state.searches_scanned.load(Ordering::Relaxed),
        recomputed_files: s.recomputed_files,
        recomputed_sample: s.recomputed_sample.clone(),
    })
}

/// What the symbol graph says the task's change could break, as advisory text
/// for a verification stage (PX-110, docs/64 §6). It names the non-test files
/// that depend on what changed and the tests that cover them, each with how
/// sure the graph is. It is *advice about where to look*: nothing consumes it
/// to narrow, skip or reorder a check, and the mandatory set of the stage ran
/// whatever it says. `None` when nothing changed or the indexes are busy.
pub(crate) async fn advisory_impact(core: &Core, task: &Task) -> Option<String> {
    let root = root_of(task).ok()?;
    let index = core.tools.index(&root).await.ok()?;
    let symbols = core.tools.symbols(&root).await.ok()?;
    let graph = core.tools.graph(&root).await.ok()?;
    let refs = core.tools.refs(&root).await.ok()?;
    let idx = index.try_lock().ok()?;
    let sym = symbols.try_lock().ok()?;
    let gr = graph.try_lock().ok()?;
    let rg = refs.try_lock().ok()?;
    let changed: Vec<String> = gr
        .changed_paths()
        .into_iter()
        .filter(|p| !p.starts_with(".modbit"))
        .collect();
    if changed.is_empty() {
        return None;
    }
    let sel = modbit_retrieval::select_impacted_with_refs(&idx, &sym, &gr, &rg, &changed, 2, 12);
    let deps: Vec<String> = sel
        .dependents
        .iter()
        .map(|d| format!("{} ({}, {})", d.path, d.reasons.join("+"), d.confidence))
        .collect();
    let tests: Vec<&str> = sel.tests.iter().map(|t| t.path.as_str()).collect();
    Some(format!(
        "advisory impact (heuristic, from the symbol graph at revision {}; it never narrows the mandatory checks, all of which ran): {} file(s) changed; {} dependent file(s): {}; tests that cover them: {}{}\n",
        sel.revision,
        changed.len(),
        sel.dependents.len(),
        if deps.is_empty() {
            "none found".to_owned()
        } else {
            deps.join(", ")
        },
        if tests.is_empty() {
            "none found".to_owned()
        } else {
            tests.join(", ")
        },
        if sel.partial {
            format!(" [partial: {}]", sel.partial_reason)
        } else {
            String::new()
        },
    ))
}
