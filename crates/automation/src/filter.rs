//! Trigger filters (AUT-B02): which events a trigger lets through.
//!
//! The payload is untrusted. It is only ever *read* here, to answer yes or
//! no; matching never executes anything and never feeds back into the
//! definition, its profile or its budget (AUT-D04). Patterns are globs
//! (polynomial-time matching) and regular expressions compiled by the
//! linear-time engine under a size limit; a field the payload does not carry
//! cannot satisfy a filter (fail closed).

use serde_json::Value;

use crate::definition::{Filters, Trigger, compile_regex};

const MAX_FIELD_CHARS: usize = 4096;

enum Tok {
    Lit(char),
    One,
    Star,
    StarStar,
}

fn tokens(pattern: &str) -> Vec<Tok> {
    let chars: Vec<char> = pattern.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '*' if chars.get(i + 1) == Some(&'*') => {
                out.push(Tok::StarStar);
                i += 2;
            }
            '*' => {
                out.push(Tok::Star);
                i += 1;
            }
            '?' => {
                out.push(Tok::One);
                i += 1;
            }
            c => {
                out.push(Tok::Lit(c));
                i += 1;
            }
        }
    }
    out
}

/// Glob match: `*` is any run within one path segment, `**` any run, `?` one
/// character within a segment. Dynamic programming, so a hostile pattern or
/// text cannot make it exponential.
#[must_use]
pub fn glob_match(pattern: &str, text: &str) -> bool {
    let toks = tokens(&pattern.chars().take(256).collect::<String>());
    let text: Vec<char> = text.chars().take(MAX_FIELD_CHARS).collect();
    let (p, n) = (toks.len(), text.len());
    // m[i][j]: toks[i..] matches text[j..]; rows are kept as two vectors.
    let mut next = vec![false; n + 1];
    next[n] = true; // i == p
    for i in (0..p).rev() {
        let mut row = vec![false; n + 1];
        for j in (0..=n).rev() {
            row[j] = match &toks[i] {
                Tok::Lit(c) => j < n && text[j] == *c && next[j + 1],
                Tok::One => j < n && text[j] != '/' && next[j + 1],
                Tok::Star => next[j] || (j < n && text[j] != '/' && row[j + 1]),
                Tok::StarStar => next[j] || (j < n && row[j + 1]),
            };
        }
        next = row;
    }
    next[0]
}

fn str_of<'a>(payload: &'a Value, key: &str) -> Option<&'a str> {
    payload.get(key).and_then(Value::as_str)
}

fn strings_of(payload: &Value, key: &str) -> Vec<String> {
    payload
        .get(key)
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

fn clip(s: &str) -> String {
    s.chars().take(MAX_FIELD_CHARS).collect()
}

/// Apply filters to a normalized payload (`branch`, `labels`, `author`,
/// `paths`, `title`, `text`; a webhook body is tested by `fields`).
///
/// # Errors
/// The reason a stored pattern could not be used (it was validated at save
/// time, so this is a defect, and the caller treats it as no match).
pub fn apply(filters: &Filters, payload: &Value) -> Result<bool, String> {
    if !filters.branches.is_empty() {
        let Some(branch) = str_of(payload, "branch") else {
            return Ok(false);
        };
        if !filters.branches.iter().any(|g| glob_match(g, branch)) {
            return Ok(false);
        }
    }
    if !filters.labels.is_empty() {
        let have = strings_of(payload, "labels");
        if !filters.labels.iter().any(|l| have.contains(l)) {
            return Ok(false);
        }
    }
    if !filters.authors.is_empty() {
        let Some(author) = str_of(payload, "author") else {
            return Ok(false);
        };
        if !filters
            .authors
            .iter()
            .any(|a| a.eq_ignore_ascii_case(author))
        {
            return Ok(false);
        }
    }
    if !filters.paths.is_empty() {
        let have = strings_of(payload, "paths");
        if !have
            .iter()
            .any(|p| filters.paths.iter().any(|g| glob_match(g, p)))
        {
            return Ok(false);
        }
    }
    for (pattern, key) in [
        (&filters.title_regex, "title"),
        (&filters.text_regex, "text"),
    ] {
        if let Some(p) = pattern {
            let re = compile_regex(p)?;
            let Some(v) = str_of(payload, key) else {
                return Ok(false);
            };
            if !re.is_match(&clip(v)) {
                return Ok(false);
            }
        }
    }
    for f in &filters.fields {
        let Some(v) = payload.pointer(&f.pointer) else {
            return Ok(false);
        };
        let text = match v {
            Value::String(s) => clip(s),
            Value::Number(n) => n.to_string(),
            Value::Bool(b) => b.to_string(),
            _ => return Ok(false),
        };
        if let Some(eq) = &f.equals
            && &text != eq
        {
            return Ok(false);
        }
        if let Some(r) = &f.regex
            && !compile_regex(r)?.is_match(&text)
        {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Whether an event delivery satisfies a trigger: the source and event name
/// (and action) for an event trigger, the endpoint name for a webhook, then
/// the filters. A schedule or a manual trigger never matches a delivery.
///
/// # Errors
/// As [`apply`].
pub fn trigger_matches(
    trigger: &Trigger,
    source: &str,
    event: &str,
    payload: &Value,
) -> Result<bool, String> {
    match trigger {
        Trigger::Event {
            source: s,
            event: e,
            actions,
            filters,
            ..
        } => {
            if s != source || e != event {
                return Ok(false);
            }
            if !actions.is_empty() {
                let action = str_of(payload, "action").unwrap_or_default();
                if !actions.iter().any(|a| a == action) {
                    return Ok(false);
                }
            }
            apply(filters, payload)
        }
        Trigger::Webhook { name, filters, .. } => {
            if source != "webhook" || name != event {
                return Ok(false);
            }
            apply(filters, payload)
        }
        Trigger::Schedule { .. } | Trigger::Manual { .. } => Ok(false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::definition::FieldFilter;
    use serde_json::json;

    #[test]
    fn globs_follow_segment_rules_and_stay_polynomial() {
        assert!(glob_match("release/*", "release/1.2"));
        assert!(!glob_match("release/*", "release/1/2"));
        assert!(glob_match("release/**", "release/1/2"));
        assert!(glob_match("src/**/*.rs", "src/a/b/c.rs"));
        assert!(glob_match("m?in", "main"));
        assert!(!glob_match("m?in", "m/in"));
        let hostile = "*a".repeat(100) + "b";
        assert!(!glob_match(&hostile, &"a".repeat(2000)));
    }

    #[test]
    fn filters_gate_on_the_fields_the_payload_carries() {
        let f = Filters {
            branches: vec!["main".into(), "release/*".into()],
            labels: vec!["bug".into()],
            authors: vec!["Octo".into()],
            title_regex: Some("^fix".into()),
            ..Filters::default()
        };
        let ok = json!({"branch": "release/9", "labels": ["bug", "x"], "author": "octo", "title": "fix: it"});
        assert!(apply(&f, &ok).unwrap());
        for (k, v) in [
            ("branch", json!("dev")),
            ("labels", json!(["x"])),
            ("author", json!("mallory")),
            ("title", json!("chore")),
        ] {
            let mut p = ok.clone();
            p[k] = v;
            assert!(!apply(&f, &p).unwrap(), "{k}");
        }
        // A missing field cannot satisfy a filter.
        let mut p = ok.clone();
        p.as_object_mut().unwrap().remove("author");
        assert!(!apply(&f, &p).unwrap());
    }

    #[test]
    fn webhook_field_filters_use_pointers() {
        let f = Filters {
            fields: vec![
                FieldFilter {
                    pointer: "/ref".into(),
                    equals: Some("refs/heads/main".into()),
                    regex: None,
                },
                FieldFilter {
                    pointer: "/count".into(),
                    equals: None,
                    regex: Some("^[0-9]+$".into()),
                },
            ],
            ..Filters::default()
        };
        assert!(apply(&f, &json!({"ref": "refs/heads/main", "count": 12})).unwrap());
        assert!(!apply(&f, &json!({"ref": "refs/heads/dev", "count": 12})).unwrap());
        assert!(!apply(&f, &json!({"ref": "refs/heads/main"})).unwrap());
    }
}
