//! Runner adapters (docs/64 §2): structured reporters first, a conservative
//! `configured_command` fallback labelled HEURISTIC.

use crate::report::{
    CheckKind, CheckResult, CheckStatus, Confidence, Location, ParserInfo, ReportStatus,
    RunnerFamily, TestReport, normalize_message,
};

/// Raw process outcome an adapter parses.
#[derive(Clone, Debug, Default)]
pub struct RawRun {
    /// Exit code (`None` = killed / unknown).
    pub exit_code: Option<i32>,
    /// stdout.
    pub stdout: String,
    /// stderr.
    pub stderr: String,
    /// Structured reporter payload when the runner wrote one to a file.
    pub reporter_file: Option<String>,
    /// Timed out.
    pub timed_out: bool,
    /// Cancelled.
    pub cancelled: bool,
}

/// Which adapter to use for an argv.
#[must_use]
pub fn detect(argv: &[String]) -> RunnerFamily {
    let joined = argv.join(" ");
    if argv.first().is_some_and(|a| a == "cargo") && argv.get(1).is_some_and(|a| a == "test") {
        RunnerFamily::Cargo
    } else if joined.contains("vitest") {
        RunnerFamily::Vitest
    } else if joined.contains("jest") {
        RunnerFamily::Jest
    } else if joined.contains("pytest") {
        RunnerFamily::Pytest
    } else {
        RunnerFamily::ConfiguredCommand
    }
}

fn excerpt(s: &str) -> Option<String> {
    let t = s.trim();
    if t.is_empty() {
        None
    } else {
        Some(t.chars().take(600).collect())
    }
}

/// Remove ANSI escape sequences (runners colorize under CI settings).
#[must_use]
pub fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            if chars.peek() == Some(&'[') {
                chars.next();
                for n in chars.by_ref() {
                    if n.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
            continue;
        }
        out.push(c);
    }
    out
}

/// Parse into the report's `checks`/`parser`; the caller fills identity fields.
pub fn parse(family: RunnerFamily, raw: &RawRun, report: &mut TestReport) {
    let raw = &RawRun {
        stdout: strip_ansi(&raw.stdout),
        stderr: strip_ansi(&raw.stderr),
        ..raw.clone()
    };
    let combined = format!("{}{}", raw.stdout, raw.stderr);
    let (adapter, confidence) = match family {
        RunnerFamily::Cargo => {
            report.checks = parse_cargo(&raw.stdout, &raw.stderr);
            ("cargo-libtest", Confidence::Structured)
        }
        RunnerFamily::Vitest | RunnerFamily::Jest => match raw.reporter_file.as_deref() {
            Some(json) => {
                report.checks = parse_vitest_json(json);
                ("vitest-json", Confidence::Structured)
            }
            None => ("vitest-json", Confidence::Heuristic),
        },
        RunnerFamily::Pytest => match raw.reporter_file.as_deref() {
            Some(xml) => {
                report.checks = parse_junit_xml(xml);
                ("pytest-junit", Confidence::Structured)
            }
            None => ("pytest-junit", Confidence::Heuristic),
        },
        RunnerFamily::ConfiguredCommand => ("configured_command", Confidence::Heuristic),
    };
    if confidence == Confidence::Heuristic {
        // One command-level check; UNKNOWN when the outcome cannot be established.
        let status = if raw.timed_out {
            CheckStatus::Timeout
        } else if raw.cancelled {
            CheckStatus::Error
        } else {
            match raw.exit_code {
                Some(0) => CheckStatus::Pass,
                Some(_) => CheckStatus::Fail,
                None => CheckStatus::Unknown,
            }
        };
        report.checks = vec![CheckResult {
            check_id: format!("configured_command:{}", report.runner.argv.join(" ")),
            kind: CheckKind::Command,
            status,
            duration_ms: 0,
            location: Location::default(),
            error_class: match status {
                CheckStatus::Fail => Some("NON_ZERO_EXIT".into()),
                CheckStatus::Timeout => Some("TIMEOUT".into()),
                CheckStatus::Unknown => Some("UNKNOWN_OUTCOME".into()),
                _ => None,
            },
            message_fingerprint: match status {
                CheckStatus::Pass => None,
                _ => Some(normalize_message(&format!(
                    "exit={:?} {}",
                    raw.exit_code,
                    combined.lines().rev().take(3).collect::<Vec<_>>().join(" ")
                ))),
            },
            message_excerpt: if status == CheckStatus::Pass {
                None
            } else {
                excerpt(
                    &combined
                        .chars()
                        .rev()
                        .take(600)
                        .collect::<String>()
                        .chars()
                        .rev()
                        .collect::<String>(),
                )
            },
            output_range: Some((0, combined.len())),
        }];
    }
    report.parser = ParserInfo {
        adapter: adapter.into(),
        adapter_version: "1".into(),
        confidence,
    };
    if raw.timed_out {
        report.status = ReportStatus::Timeout;
    } else if raw.cancelled {
        report.status = ReportStatus::Cancelled;
    }
    report.exit_code = raw.exit_code;
    report.finalize();
}

/// libtest human output: `test name ... ok|FAILED|ignored`, then `---- name stdout ----` blocks.
#[must_use]
pub fn parse_cargo(stdout: &str, stderr: &str) -> Vec<CheckResult> {
    let mut checks: Vec<CheckResult> = Vec::new();
    let mut offset = 0usize;
    // Failure detail blocks keyed by test name.
    let mut details: std::collections::HashMap<String, (String, usize, usize)> =
        std::collections::HashMap::new();
    let mut current: Option<(String, usize, String)> = None;
    for line in stdout.lines() {
        let start = offset;
        offset += line.len() + 1;
        if let Some(rest) = line.strip_prefix("---- ")
            && let Some(name) = rest.strip_suffix(" stdout ----")
        {
            if let Some((n, s, body)) = current.take() {
                details.insert(n, (body, s, start));
            }
            current = Some((name.to_owned(), start, String::new()));
            continue;
        }
        if line.starts_with("failures:") || line.starts_with("test result:") {
            if let Some((n, s, body)) = current.take() {
                details.insert(n, (body, s, start));
            }
            continue;
        }
        if let Some((_, _, body)) = current.as_mut() {
            body.push_str(line);
            body.push('\n');
        }
    }
    if let Some((n, s, body)) = current.take() {
        details.insert(n, (body, s, offset));
    }
    // Binary/file context: cargo prints `Running tests/quantities.rs (...)` on
    // stderr before each binary's `running N tests` block on stdout, in order.
    let binaries: Vec<String> = stderr
        .lines()
        .map(str::trim)
        .filter_map(|l| {
            if let Some(rest) = l.strip_prefix("Running ") {
                rest.split(" (").next().map(|s| s.trim().to_owned())
            } else if l.starts_with("Doc-tests ") {
                Some("doc-tests".into())
            } else {
                None
            }
        })
        .collect();
    let mut block: usize = 0;
    let mut file: Option<String> = None;
    for line in stdout.lines() {
        let t = line.trim();
        if t.starts_with("running ") && t.ends_with(" tests") || t == "running 1 test" {
            file = binaries.get(block).cloned();
            block += 1;
            continue;
        }
        if let Some(rest) = t.strip_prefix("test ")
            && let Some((name, outcome)) = rest.rsplit_once(" ... ")
        {
            let name = name.trim();
            if name.is_empty() || name.starts_with("result:") {
                continue;
            }
            let status = match outcome.trim() {
                "ok" => CheckStatus::Pass,
                "FAILED" => CheckStatus::Fail,
                "ignored" => CheckStatus::Skip,
                o if o.starts_with("ignored") => CheckStatus::Skip,
                _ => CheckStatus::Unknown,
            };
            let path = file.clone();
            let detail = details.get(name);
            let panic_line = detail.and_then(|(b, _, _)| {
                b.lines()
                    .find(|l| l.contains("panicked at") || l.contains("assertion"))
            });
            let error_class = if status == CheckStatus::Fail {
                Some(
                    detail
                        .and_then(|(b, _, _)| {
                            if b.contains("assertion `left == right` failed") {
                                Some("assertion_eq")
                            } else if b.contains("assertion failed") {
                                Some("assertion")
                            } else if b.contains("panicked at") {
                                Some("panic")
                            } else {
                                None
                            }
                        })
                        .unwrap_or("test_failure")
                        .to_owned(),
                )
            } else {
                None
            };
            let line_no = panic_line.and_then(|l| {
                l.split("panicked at ").nth(1).and_then(|loc| {
                    loc.trim()
                        .trim_end_matches(':')
                        .rsplit(':')
                        .nth(1)
                        .and_then(|n| n.parse::<u32>().ok())
                })
            });
            checks.push(CheckResult {
                check_id: format!("cargo:{}::{}", path.clone().unwrap_or_default(), name),
                kind: CheckKind::Test,
                status,
                duration_ms: 0,
                location: Location {
                    path,
                    line: line_no,
                    symbol: Some(name.to_owned()),
                },
                error_class,
                message_fingerprint: detail.map(|(b, _, _)| {
                    normalize_message(
                        &b.lines()
                            .filter(|l| !l.trim().is_empty())
                            .take(4)
                            .collect::<Vec<_>>()
                            .join(" "),
                    )
                }),
                message_excerpt: detail.and_then(|(b, _, _)| excerpt(b)),
                output_range: detail.map(|(_, s, e)| (*s, *e)),
            });
        }
    }
    if checks.is_empty()
        && (stderr.contains("error[E") || stderr.contains("error: could not compile"))
    {
        checks.push(CheckResult {
            check_id: "cargo:build".into(),
            kind: CheckKind::Build,
            status: CheckStatus::Fail,
            duration_ms: 0,
            location: Location::default(),
            error_class: Some("compile_error".into()),
            message_fingerprint: Some(normalize_message(
                stderr
                    .lines()
                    .find(|l| l.contains("error"))
                    .unwrap_or_default(),
            )),
            message_excerpt: excerpt(stderr),
            output_range: None,
        });
    }
    checks
}

/// vitest/jest JSON reporter.
#[must_use]
pub fn parse_vitest_json(json: &str) -> Vec<CheckResult> {
    let v: serde_json::Value = match serde_json::from_str(json) {
        Ok(v) => v,
        Err(_) => return vec![],
    };
    let mut out = Vec::new();
    for file in v["testResults"].as_array().into_iter().flatten() {
        let name = file["name"].as_str().unwrap_or_default();
        let rel = name
            .rsplit("/test/")
            .next()
            .map(|s| format!("test/{s}"))
            .unwrap_or_else(|| name.to_owned());
        for a in file["assertionResults"].as_array().into_iter().flatten() {
            let full = a["fullName"].as_str().unwrap_or_default();
            let status = match a["status"].as_str() {
                Some("passed") => CheckStatus::Pass,
                Some("failed") => CheckStatus::Fail,
                Some("skipped") | Some("pending") | Some("todo") => CheckStatus::Skip,
                _ => CheckStatus::Unknown,
            };
            let msgs: Vec<String> = a["failureMessages"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|m| m.as_str().map(str::to_owned))
                .collect();
            let first = msgs.first().cloned().unwrap_or_default();
            let error_class = if status == CheckStatus::Fail {
                Some(first.split(':').next().unwrap_or("Error").trim().to_owned())
            } else {
                None
            };
            let line = first
                .lines()
                .find(|l| l.contains(&rel))
                .and_then(|l| l.rsplit(':').nth(1).and_then(|n| n.parse::<u32>().ok()));
            out.push(CheckResult {
                check_id: format!("vitest:{rel}::{full}"),
                kind: CheckKind::Test,
                status,
                duration_ms: a["duration"].as_f64().unwrap_or(0.0) as u64,
                location: Location {
                    path: Some(rel.clone()),
                    line,
                    symbol: Some(full.to_owned()),
                },
                error_class,
                message_fingerprint: if msgs.is_empty() {
                    None
                } else {
                    Some(normalize_message(first.lines().next().unwrap_or_default()))
                },
                message_excerpt: excerpt(&first),
                output_range: None,
            });
        }
    }
    out
}

/// pytest JUnit XML (minimal parser, no external dependency).
#[must_use]
pub fn parse_junit_xml(xml: &str) -> Vec<CheckResult> {
    let mut out = Vec::new();
    let mut rest = xml;
    while let Some(i) = rest.find("<testcase") {
        rest = &rest[i..];
        let end_tag = rest.find('>').unwrap_or(rest.len());
        let head = &rest[..end_tag];
        let self_closing = head.ends_with('/');
        let attr = |k: &str| -> Option<String> {
            let pat = format!(" {k}=\"");
            let s = head.find(&pat)? + pat.len();
            let e = head[s..].find('"')? + s;
            Some(unescape(&head[s..e]))
        };
        let classname = attr("classname").unwrap_or_default();
        let name = attr("name").unwrap_or_default();
        let time_ms = attr("time")
            .and_then(|t| t.parse::<f64>().ok())
            .map(|t| (t * 1000.0) as u64)
            .unwrap_or(0);
        let (body, consumed) = if self_closing {
            ("", end_tag + 1)
        } else {
            let close = rest
                .find("</testcase>")
                .map(|c| c + "</testcase>".len())
                .unwrap_or(rest.len());
            (
                &rest[end_tag + 1..close.saturating_sub("</testcase>".len()).max(end_tag + 1)],
                close,
            )
        };
        let (status, error_class, message) = if body.contains("<failure") {
            (
                CheckStatus::Fail,
                "AssertionError",
                extract_msg(body, "failure"),
            )
        } else if body.contains("<error") {
            (CheckStatus::Error, "Error", extract_msg(body, "error"))
        } else if body.contains("<skipped") {
            (CheckStatus::Skip, "", String::new())
        } else {
            (CheckStatus::Pass, "", String::new())
        };
        let path = format!("{}.py", classname.replace('.', "/"));
        let canonical_name = name.split('[').next().unwrap_or(&name).to_owned();
        out.push(CheckResult {
            check_id: format!(
                "pytest:{path}::{canonical_name}{}",
                if name.contains('[') { "[param]" } else { "" }
            ),
            kind: CheckKind::Test,
            status,
            duration_ms: time_ms,
            location: Location {
                path: Some(path),
                line: body.lines().find_map(|l| {
                    l.split(".py:")
                        .nth(1)
                        .and_then(|r| r.split(':').next())
                        .and_then(|n| n.parse().ok())
                }),
                symbol: Some(name.clone()),
            },
            error_class: if error_class.is_empty() {
                None
            } else {
                Some(
                    message
                        .split(':')
                        .next()
                        .unwrap_or(error_class)
                        .trim()
                        .to_owned(),
                )
            },
            message_fingerprint: if message.is_empty() {
                None
            } else {
                Some(normalize_message(
                    message.lines().next().unwrap_or_default(),
                ))
            },
            message_excerpt: excerpt(&message),
            output_range: None,
        });
        rest = &rest[consumed..];
    }
    out
}

fn extract_msg(body: &str, tag: &str) -> String {
    let open = format!("<{tag}");
    let Some(s) = body.find(&open) else {
        return String::new();
    };
    let seg = &body[s..];
    let msg_attr = seg.find("message=\"").map(|i| {
        let a = i + "message=\"".len();
        let e = seg[a..].find('"').map(|x| x + a).unwrap_or(seg.len());
        unescape(&seg[a..e])
    });
    let text = seg.find('>').map(|i| {
        let a = i + 1;
        let e = seg[a..]
            .find(&format!("</{tag}>"))
            .map(|x| x + a)
            .unwrap_or(seg.len());
        unescape(seg[a..e].trim())
    });
    match (msg_attr, text) {
        (Some(m), Some(t)) if !t.is_empty() => format!("{m}\n{t}"),
        (Some(m), _) => m,
        (None, Some(t)) => t,
        _ => String::new(),
    }
}

fn unescape(s: &str) -> String {
    s.replace("&quot;", "\"")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}
