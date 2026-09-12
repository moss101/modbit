# Task Card — IMP-EV-0186 Structured notebook read/edit

## Identity

- Task ID: IMP-EV-0186
- Milestone: M5 Procedural runtime and skills (P0)
- Requirement: REQ-EV-0186; owner label: Artifact/Notebook Adapter; subsystem: media
- Qualification: `QUAL-EV-0186` — Real ipynb read/edit preserves unrelated cells and execution metadata policy.
- Evidence tier: production-equivalent (real fixtures through the production tool and the real Core against a scripted OpenAI-compatible server)

## Goal

Represent notebook cells structurally; edits target stable cell IDs and reject ambiguous/truncated state.

## Existing-code audit

- classification: MISSING before: `.ipynb` was read as opaque JSON text and edited by string replacement.
- production entry points: `crates/tools/src/notebook.rs` (`read` → `NotebookView` with nbformat, kernel, cells with stable id/type/source/execution_count/output types, `all_cells_addressable`; `edit_cell` rewriting one cell, clearing the edited code cell's outputs and execution count, `canonical_bytes` in Jupyter's canonical form; refusals `NOTEBOOK_NO_SUCH_CELL` / `NOTEBOOK_AMBIGUOUS_CELL` / `NOTEBOOK_MALFORMED` / `NOTEBOOK_UNSUPPORTED`); `crates/tools/src/direct.rs` (`fs.read` returns the `notebook` view plus `edit_hint`; `change.apply op=notebook_cell` with `cell_id` and `source` through the Change Engine's revision-bound atomic replace).
- proof: on the real Core the fixture `totals.ipynb` reads as three addressable cells; `change.apply op=notebook_cell cell_id=total` lands one `FileChanged` with a workspace revision, Git's `-U0` diff removes exactly the cell's execution count, its outputs block and its one changed source line and adds their replacements, the other cells (including tagged metadata and outputs) and notebook metadata are byte-identical; an unknown id is refused `NOTEBOOK_NO_SUCH_CELL` with no write; unit tests cover ambiguous ids, truncated JSON and unsupported nbformat.

## Limitations

- Cell insertion, deletion and reordering are not yet operations; a notebook whose cells lack ids is readable but not addressable (`all_cells_addressable: false`). Canonical form is Jupyter's (indent 1, sorted keys), so a notebook saved by another tool in a different layout diffs whole on its first edit.

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `qual_ev_0186_a_notebook_edit_targets_one_cell_by_id_and_keeps_the_rest`
- `a_notebook_reads_as_cells_with_ids_and_an_edit_touches_one_cell_only`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
