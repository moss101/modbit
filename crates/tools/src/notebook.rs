//! Jupyter notebooks as structure (docs/25 "Notebook reads preserve cell
//! identity/type/outputs; edits are revision-bound cell operations rather
//! than whole-file string replacement"; REQ-EV-0186): `fs.read` of an
//! `.ipynb` returns the cells with their stable ids, and `change.apply`'s
//! `notebook_cell` op rewrites one cell's source by id, leaving every other
//! cell and the notebook metadata byte-identical in Jupyter's own canonical
//! form (`indent=1, sort_keys=True`, trailing newline) so the Git diff is
//! the edited cell and nothing else. The output policy is fixed and said:
//! the edited cell's outputs and execution count are cleared (they were
//! produced by other source); other cells keep theirs. An ambiguous target
//! (a missing or duplicated id) or a truncated / malformed notebook is a
//! refusal, never a best effort.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Why a notebook operation was refused.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotebookError {
    /// `NOTEBOOK_MALFORMED` | `NOTEBOOK_UNSUPPORTED` | `NOTEBOOK_AMBIGUOUS_CELL` | `NOTEBOOK_NO_SUCH_CELL`.
    pub code: &'static str,
    /// Detail.
    pub message: String,
}

fn refuse(code: &'static str, message: impl Into<String>) -> NotebookError {
    NotebookError {
        code,
        message: message.into(),
    }
}

/// One cell as the model sees it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CellView {
    /// Stable id (nbformat 4.5+); `None` when the notebook carries none —
    /// such a cell cannot be targeted.
    pub id: Option<String>,
    /// 0-based position.
    pub index: usize,
    /// `code` | `markdown` | `raw`.
    pub cell_type: String,
    /// Source text (joined).
    pub source: String,
    /// Execution count, when any.
    pub execution_count: Option<i64>,
    /// Number of outputs.
    pub outputs: usize,
    /// Output types (`stream`, `execute_result`, `display_data`, `error`).
    pub output_types: Vec<String>,
}

/// The notebook as structure.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotebookView {
    /// nbformat major.
    pub nbformat: i64,
    /// nbformat minor.
    pub nbformat_minor: i64,
    /// Kernel display name, when declared.
    pub kernel: Option<String>,
    /// Language, when declared.
    pub language: Option<String>,
    /// Cells.
    pub cells: Vec<CellView>,
    /// Ids that occur more than once (those cells cannot be targeted).
    pub duplicate_ids: Vec<String>,
    /// Whether every cell carries a stable id.
    pub all_cells_addressable: bool,
}

fn parse(bytes: &[u8]) -> Result<Value, NotebookError> {
    let v: Value = serde_json::from_slice(bytes)
        .map_err(|e| refuse("NOTEBOOK_MALFORMED", format!("not a JSON notebook: {e}")))?;
    let major = v["nbformat"].as_i64().unwrap_or(0);
    if major != 4 {
        return Err(refuse(
            "NOTEBOOK_UNSUPPORTED",
            format!("nbformat {major} is not supported (4 is)"),
        ));
    }
    if !v["cells"].is_array() {
        return Err(refuse("NOTEBOOK_MALFORMED", "no `cells` array"));
    }
    Ok(v)
}

fn source_text(cell: &Value) -> String {
    match &cell["source"] {
        Value::String(s) => s.clone(),
        Value::Array(a) => a
            .iter()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>()
            .join(""),
        _ => String::new(),
    }
}

/// Read a notebook as structure.
///
/// # Errors
/// Malformed or unsupported notebook.
pub fn read(bytes: &[u8]) -> Result<NotebookView, NotebookError> {
    let v = parse(bytes)?;
    let cells = v["cells"].as_array().cloned().unwrap_or_default();
    let mut seen: Vec<String> = Vec::new();
    let mut duplicates: Vec<String> = Vec::new();
    let mut views = Vec::with_capacity(cells.len());
    for (index, cell) in cells.iter().enumerate() {
        let id = cell["id"].as_str().map(str::to_owned);
        if let Some(i) = &id {
            if seen.contains(i) {
                if !duplicates.contains(i) {
                    duplicates.push(i.clone());
                }
            } else {
                seen.push(i.clone());
            }
        }
        let outputs = cell["outputs"].as_array().cloned().unwrap_or_default();
        views.push(CellView {
            id,
            index,
            cell_type: cell["cell_type"].as_str().unwrap_or("").to_owned(),
            source: source_text(cell),
            execution_count: cell["execution_count"].as_i64(),
            outputs: outputs.len(),
            output_types: outputs
                .iter()
                .filter_map(|o| o["output_type"].as_str().map(str::to_owned))
                .collect(),
        });
    }
    let all_cells_addressable = views.iter().all(|c| c.id.is_some()) && duplicates.is_empty();
    Ok(NotebookView {
        nbformat: v["nbformat"].as_i64().unwrap_or(0),
        nbformat_minor: v["nbformat_minor"].as_i64().unwrap_or(0),
        kernel: v["metadata"]["kernelspec"]["display_name"]
            .as_str()
            .map(str::to_owned),
        language: v["metadata"]["language_info"]["name"]
            .as_str()
            .map(str::to_owned),
        cells: views,
        duplicate_ids: duplicates,
        all_cells_addressable,
    })
}

/// Jupyter's line convention: every line keeps its `\n` except the last.
fn source_lines(text: &str) -> Value {
    if text.is_empty() {
        return Value::Array(vec![]);
    }
    let mut lines: Vec<Value> = Vec::new();
    let mut rest = text;
    while let Some(i) = rest.find('\n') {
        lines.push(Value::String(rest[..=i].to_owned()));
        rest = &rest[i + 1..];
    }
    if !rest.is_empty() {
        lines.push(Value::String(rest.to_owned()));
    }
    Value::Array(lines)
}

/// Sort object keys recursively (Jupyter writes `sort_keys=True`).
fn sorted(v: &Value) -> Value {
    match v {
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            let mut out = serde_json::Map::new();
            for k in keys {
                out.insert(k.clone(), sorted(&map[k]));
            }
            Value::Object(out)
        }
        Value::Array(a) => Value::Array(a.iter().map(sorted).collect()),
        other => other.clone(),
    }
}

/// Jupyter's canonical bytes: `indent=1`, sorted keys, a trailing newline.
///
/// # Errors
/// Serialization failure (not expected for JSON values).
pub fn canonical_bytes(v: &Value) -> Result<Vec<u8>, NotebookError> {
    let mut out = Vec::new();
    let formatter = serde_json::ser::PrettyFormatter::with_indent(b" ");
    let mut ser = serde_json::Serializer::with_formatter(&mut out, formatter);
    serde::Serialize::serialize(&sorted(v), &mut ser)
        .map_err(|e| refuse("NOTEBOOK_MALFORMED", e.to_string()))?;
    out.push(b'\n');
    Ok(out)
}

/// Rewrite one cell's source by its stable id; every other cell and the
/// metadata are kept; the edited cell's outputs and execution count are
/// cleared (fixed policy, stated in the result).
///
/// # Errors
/// Malformed notebook, missing or duplicated id.
pub fn edit_cell(bytes: &[u8], cell_id: &str, new_source: &str) -> Result<Vec<u8>, NotebookError> {
    let mut v = parse(bytes)?;
    let cells = v["cells"]
        .as_array_mut()
        .ok_or_else(|| refuse("NOTEBOOK_MALFORMED", "no `cells` array"))?;
    let matches: Vec<usize> = cells
        .iter()
        .enumerate()
        .filter(|(_, c)| c["id"].as_str() == Some(cell_id))
        .map(|(i, _)| i)
        .collect();
    match matches.len() {
        0 => {
            return Err(refuse(
                "NOTEBOOK_NO_SUCH_CELL",
                format!("no cell with id `{cell_id}` (read the notebook for the ids)"),
            ));
        }
        1 => {}
        n => {
            return Err(refuse(
                "NOTEBOOK_AMBIGUOUS_CELL",
                format!(
                    "{n} cells carry id `{cell_id}`; the notebook must be repaired before a cell can be targeted"
                ),
            ));
        }
    }
    let cell = &mut cells[matches[0]];
    let Some(obj) = cell.as_object_mut() else {
        return Err(refuse("NOTEBOOK_MALFORMED", "cell is not an object"));
    };
    obj.insert("source".into(), source_lines(new_source));
    if obj.get("cell_type").and_then(Value::as_str) == Some("code") {
        obj.insert("outputs".into(), Value::Array(vec![]));
        obj.insert("execution_count".into(), Value::Null);
    }
    canonical_bytes(&v)
}

#[cfg(test)]
mod tests {
    use super::*;

    const NB: &str = r##"{"cells":[{"cell_type":"markdown","id":"intro","metadata":{},"source":["# Title\n","text"]},{"cell_type":"code","execution_count":3,"id":"calc","metadata":{"tags":["x"]},"outputs":[{"name":"stdout","output_type":"stream","text":["4\n"]}],"source":["print(2+2)"]},{"cell_type":"code","execution_count":4,"id":"keep","metadata":{},"outputs":[{"data":{"text/plain":["9"]},"execution_count":4,"metadata":{},"output_type":"execute_result"}],"source":["3*3"]}],"metadata":{"kernelspec":{"display_name":"Python 3","language":"python","name":"python3"},"language_info":{"name":"python"}},"nbformat":4,"nbformat_minor":5}"##;

    #[test]
    fn a_notebook_reads_as_cells_with_ids_and_an_edit_touches_one_cell_only() {
        let view = read(NB.as_bytes()).unwrap();
        assert_eq!(view.cells.len(), 3);
        assert_eq!(view.cells[1].id.as_deref(), Some("calc"));
        assert_eq!(view.cells[1].source, "print(2+2)");
        assert_eq!(view.cells[1].execution_count, Some(3));
        assert_eq!(view.cells[1].output_types, ["stream"]);
        assert_eq!(view.cells[0].source, "# Title\ntext");
        assert_eq!(view.kernel.as_deref(), Some("Python 3"));
        assert!(view.all_cells_addressable);
        let edited = edit_cell(NB.as_bytes(), "calc", "print(2 + 2)\nprint('again')").unwrap();
        let after: Value = serde_json::from_slice(&edited).unwrap();
        assert_eq!(
            after["cells"][1]["source"],
            serde_json::json!(["print(2 + 2)\n", "print('again')"])
        );
        assert_eq!(after["cells"][1]["outputs"], serde_json::json!([]));
        assert!(after["cells"][1]["execution_count"].is_null());
        assert_eq!(
            after["cells"][1]["metadata"]["tags"],
            serde_json::json!(["x"])
        );
        // The other cells and the metadata are exactly what they were.
        let before: Value = serde_json::from_str(NB).unwrap();
        assert_eq!(after["cells"][0], before["cells"][0]);
        assert_eq!(after["cells"][2], before["cells"][2]);
        assert_eq!(after["metadata"], before["metadata"]);
        // Canonical form: 1-space indent, sorted keys, trailing newline;
        // idempotent, so a second edit of the same cell diffs only there.
        let text = String::from_utf8(edited.clone()).unwrap();
        assert!(text.starts_with("{\n \"cells\": [\n"), "{text}");
        assert!(text.ends_with("}\n"));
        assert_eq!(canonical_bytes(&after).unwrap(), edited);
        // Refusals.
        assert_eq!(
            edit_cell(NB.as_bytes(), "nope", "x").unwrap_err().code,
            "NOTEBOOK_NO_SUCH_CELL"
        );
        let dup = NB.replace("\"id\":\"keep\"", "\"id\":\"calc\"");
        assert_eq!(
            edit_cell(dup.as_bytes(), "calc", "x").unwrap_err().code,
            "NOTEBOOK_AMBIGUOUS_CELL"
        );
        assert!(!read(dup.as_bytes()).unwrap().all_cells_addressable);
        assert_eq!(read(dup.as_bytes()).unwrap().duplicate_ids, ["calc"]);
        assert_eq!(
            edit_cell(&NB.as_bytes()[..200], "calc", "x")
                .unwrap_err()
                .code,
            "NOTEBOOK_MALFORMED"
        );
        assert_eq!(
            read(br#"{"nbformat":3,"cells":[]}"#).unwrap_err().code,
            "NOTEBOOK_UNSUPPORTED"
        );
    }
}
