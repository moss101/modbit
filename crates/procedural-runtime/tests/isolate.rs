//! The isolate as specified in docs/16 "Procedural Tool Runtime" (M5.2): no
//! ambient authority, `tools.*` as the only way out, budgets enforced by the
//! host side of the engine.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use modbit_procedural_runtime::{
    Budget, Cancel, DENIED_GLOBALS, Exhausted, Host, HostError, HostFuture, Outcome, Status, run,
    run_async, run_with,
};

/// A host that offers a few bindings and answers from a table; it records
/// every call and can be told to refuse a name.
struct TableHost {
    bindings: Vec<String>,
    refuse: Option<(String, HostError)>,
    delay_ms: u64,
    calls: Mutex<Vec<(String, String)>>,
    served: AtomicU32,
}

impl TableHost {
    fn new(bindings: &[&str]) -> Self {
        Self {
            bindings: bindings.iter().map(|s| (*s).to_owned()).collect(),
            refuse: None,
            delay_ms: 0,
            calls: Mutex::new(vec![]),
            served: AtomicU32::new(0),
        }
    }
}

impl Host for TableHost {
    fn bindings(&self) -> Vec<String> {
        self.bindings.clone()
    }

    fn invoke(&self, name: &str, arguments_json: &str) -> HostFuture<Result<String, HostError>> {
        self.calls
            .lock()
            .unwrap()
            .push((name.to_owned(), arguments_json.to_owned()));
        self.served.fetch_add(1, Ordering::Relaxed);
        let refused = self
            .refuse
            .as_ref()
            .filter(|(n, _)| n == name)
            .map(|(_, e)| e.clone());
        let name = name.to_owned();
        let args: serde_json::Value = serde_json::from_str(arguments_json).unwrap_or_default();
        let delay = self.delay_ms;
        Box::pin(async move {
            if delay > 0 {
                tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
            }
            if let Some(e) = refused {
                return Err(e);
            }
            Ok(match name.as_str() {
                "fs.read" => serde_json::json!({
                    "path": args["path"],
                    "content": format!("contents of {}", args["path"].as_str().unwrap_or("?")),
                })
                .to_string(),
                "search.exact" => {
                    serde_json::json!({"hits": [{"path": "a.rs", "line": 3}]}).to_string()
                }
                "test.run" => serde_json::json!({"status": "PASSED", "checks": 2}).to_string(),
                _ => serde_json::json!({"echo": args}).to_string(),
            })
        })
    }
}

fn budget() -> Budget {
    Budget {
        cpu_time_ms: 2_000,
        ..Budget::default()
    }
}

#[test]
fn the_isolate_has_no_ambient_authority() {
    let host = Arc::new(TableHost::new(&["fs.read"]));
    // Every name a library flavour or a browser would give a script is
    // absent; the raw host binding the prelude wrapped is gone too; the
    // program cannot import a module because there is no loader.
    let probes = DENIED_GLOBALS
        .iter()
        .map(|g| format!("[{g:?}, typeof globalThis[{g:?}]]"))
        .collect::<Vec<_>>()
        .join(",");
    let source = format!(
        r#"
        const found = [{probes}].filter(([, t]) => t !== 'undefined').map(([n]) => n);
        const raw = [typeof globalThis.__modbit_invoke, typeof globalThis.__modbit_log, typeof globalThis.__modbit_bindings];
        let imported = 'not attempted';
        try {{ await import('fs'); imported = 'IMPORTED'; }} catch (e) {{ imported = 'refused: ' + e.constructor.name; }}
        let dyn = 'not attempted';
        try {{ new Function('return require')()('fs'); dyn = 'REACHED'; }} catch (e) {{ dyn = 'refused: ' + e.constructor.name; }}
        // The generated surface is frozen: neither replaced nor extended.
        let replaced = 'unchanged';
        try {{ globalThis.tools = {{}}; }} catch (e) {{}}
        try {{ tools.fs = null; }} catch (e) {{}}
        try {{ tools.change = {{ apply: () => 1 }}; }} catch (e) {{}}
        if (typeof tools.fs.read !== 'function' || 'change' in tools) replaced = 'CHANGED';
        return {{ found, raw, imported, dyn, replaced, has_tools: typeof tools.fs.read }};
        "#
    );
    let out = run(&source, &budget(), host);
    assert_eq!(out.status, Status::Completed, "{out:#?}");
    let v: serde_json::Value = serde_json::from_str(out.value_json.as_deref().unwrap()).unwrap();
    assert_eq!(v["found"], serde_json::json!([]), "{v}");
    assert_eq!(
        v["raw"],
        serde_json::json!(["undefined", "undefined", "undefined"])
    );
    assert!(
        v["imported"].as_str().unwrap().starts_with("refused"),
        "{v}"
    );
    assert!(v["dyn"].as_str().unwrap().starts_with("refused"), "{v}");
    assert_eq!(v["replaced"], "unchanged");
    assert_eq!(v["has_tools"], "function");
}

#[test]
fn tools_bindings_are_the_only_way_out_and_every_call_reaches_the_host() {
    let host = Arc::new(TableHost::new(&["fs.read", "search.exact", "test.run"]));
    let source = r#"
        const hits = await tools.search.exact({ query: "SessionStore" });
        const file = await tools.fs.read({ path: hits.hits[0].path });
        console.log("read", file.path, file.content.length);
        const [a, b] = await Promise.all([tools.fs.read({ path: "x" }), tools.fs.read({ path: "y" })]);
        const test = await tools.test.run({ target: "session-store" });
        // A tool the host did not offer is not a function at all.
        const missing = typeof (tools.change && tools.change.apply);
        return { first: hits.hits[0].line, content: file.content, both: [a.content, b.content], test, missing };
    "#;
    let out = run(source, &budget(), host.clone());
    assert_eq!(out.status, Status::Completed, "{out:#?}");
    let v: serde_json::Value = serde_json::from_str(out.value_json.as_deref().unwrap()).unwrap();
    assert_eq!(v["first"], 3);
    assert_eq!(v["content"], "contents of a.rs");
    assert_eq!(
        v["both"],
        serde_json::json!(["contents of x", "contents of y"])
    );
    assert_eq!(v["test"]["status"], "PASSED");
    assert_eq!(v["missing"], "undefined");
    assert_eq!(out.log, vec!["read a.rs 16".to_owned()]);
    let calls = host.calls.lock().unwrap().clone();
    assert_eq!(
        calls.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>(),
        ["search.exact", "fs.read", "fs.read", "fs.read", "test.run"]
    );
    assert_eq!(calls[1].1, r#"{"path":"a.rs"}"#);
    assert_eq!(out.calls.len(), 5);
    assert_eq!(
        out.calls[4]
            .outcome
            .as_deref()
            .map(|s| s.contains("PASSED")),
        Ok(true)
    );
}

#[test]
fn a_host_refusal_is_a_typed_error_the_program_may_catch_and_never_an_effect() {
    let mut host = TableHost::new(&["fs.read", "change.apply"]);
    host.refuse = Some((
        "change.apply".into(),
        HostError {
            code: "POLICY_DENIED".into(),
            message: "the kernel denied the write".into(),
        },
    ));
    let host = Arc::new(host);
    let source = r#"
        let caught = null;
        try { await tools.change.apply({ path: "a.rs", op: "replace", content: "x" }); }
        catch (e) { caught = { code: e.code, tool: e.tool, message: e.message, is_error: e instanceof Error }; }
        const after = await tools.fs.read({ path: "still.works" });
        return { caught, after: after.content };
    "#;
    let out = run(source, &budget(), host.clone());
    assert_eq!(out.status, Status::Completed, "{out:#?}");
    let v: serde_json::Value = serde_json::from_str(out.value_json.as_deref().unwrap()).unwrap();
    assert_eq!(
        v["caught"],
        serde_json::json!({"code": "POLICY_DENIED", "tool": "change.apply", "message": "the kernel denied the write", "is_error": true})
    );
    assert_eq!(v["after"], "contents of still.works");
    assert_eq!(out.calls[0].outcome, Err("POLICY_DENIED".into()));
    // Uncaught, the same refusal fails the program with the code in the error.
    let out = run(
        "await tools.change.apply({ path: 'a.rs', op: 'delete' }); return 'unreachable';",
        &budget(),
        host,
    );
    assert_eq!(out.status, Status::Failed, "{out:#?}");
    assert!(
        out.error
            .as_deref()
            .unwrap()
            .contains("the kernel denied the write"),
        "{out:#?}"
    );
    assert_eq!(out.value_json, None);
}

#[test]
fn the_cpu_budget_ends_a_spinning_program() {
    let host = Arc::new(TableHost::new(&["fs.read"]));
    let b = Budget {
        cpu_time_ms: 300,
        ..Budget::default()
    };
    let started = std::time::Instant::now();
    let out = run("let n = 0; while (true) { n++; } return n;", &b, host);
    assert_eq!(
        out.status,
        Status::BudgetExhausted {
            budget: Exhausted::CpuTime
        },
        "{out:#?}"
    );
    assert!(
        started.elapsed().as_millis() < 5_000,
        "{:?}",
        started.elapsed()
    );
    assert!(out.interrupt_polls > 0);
    assert!(out.error.as_deref().unwrap().contains("300 ms"), "{out:#?}");
    // The instruction-proportional ceiling works the same way.
    let b = Budget {
        cpu_time_ms: 60_000,
        max_interrupt_polls: 3,
        ..Budget::default()
    };
    let out = run(
        "let n = 0; while (true) { n++; } return n;",
        &b,
        Arc::new(TableHost::new(&[])),
    );
    assert_eq!(
        out.status,
        Status::BudgetExhausted {
            budget: Exhausted::CpuTime
        },
        "{out:#?}"
    );
    assert!(out.interrupt_polls <= 4, "{out:#?}");
}

#[test]
fn the_memory_and_stack_budgets_end_a_program_that_grows_without_bound() {
    let host = Arc::new(TableHost::new(&[]));
    let b = Budget {
        memory_bytes: 8 * 1024 * 1024,
        ..budget()
    };
    let out = run(
        "const a = []; for (;;) { a.push(new Array(4096).fill('x'.repeat(64))); } return a.length;",
        &b,
        host.clone(),
    );
    assert_eq!(
        out.status,
        Status::BudgetExhausted {
            budget: Exhausted::Memory
        },
        "{out:#?}"
    );
    let out = run(
        "function f(n) { return f(n + 1) + 1; } return f(0);",
        &budget(),
        host,
    );
    assert_eq!(
        out.status,
        Status::BudgetExhausted {
            budget: Exhausted::Stack
        },
        "{out:#?}"
    );
}

#[test]
fn the_tool_call_output_and_log_budgets_are_enforced() {
    let host = Arc::new(TableHost::new(&["fs.read"]));
    let b = Budget {
        max_tool_calls: 3,
        ..budget()
    };
    let out = run(
        "for (let i = 0; i < 5; i++) { await tools.fs.read({ path: 'p' + i }); } return 'done';",
        &b,
        host.clone(),
    );
    assert_eq!(
        out.status,
        Status::BudgetExhausted {
            budget: Exhausted::ToolCalls
        },
        "{out:#?}"
    );
    assert_eq!(
        host.served.load(Ordering::Relaxed),
        3,
        "the fourth call never reached the host"
    );
    assert_eq!(out.calls.len(), 3);
    // The program may catch the budget error and stop gracefully.
    let out = run(
        "let n = 0; try { for (;;) { await tools.fs.read({ path: 'p' }); n++; } } catch (e) { return { n, code: e.code }; }",
        &b,
        Arc::new(TableHost::new(&["fs.read"])),
    );
    assert_eq!(out.status, Status::Completed, "{out:#?}");
    assert_eq!(
        out.value_json.as_deref(),
        Some(r#"{"n":3,"code":"TOOL_CALL_BUDGET"}"#)
    );
    // Output ceiling.
    let b = Budget {
        max_output_bytes: 1_000,
        ..budget()
    };
    let out = run(
        "return 'x'.repeat(5000);",
        &b,
        Arc::new(TableHost::new(&[])),
    );
    assert_eq!(
        out.status,
        Status::BudgetExhausted {
            budget: Exhausted::Output
        },
        "{out:#?}"
    );
    assert_eq!(out.value_json, None);
    // Log ceiling: lines past it are dropped, the log says so, the program
    // still completes.
    let b = Budget {
        max_log_bytes: 40,
        ..budget()
    };
    let out = run(
        "for (let i = 0; i < 10; i++) console.log('line', i, { i }); return 'ok';",
        &b,
        Arc::new(TableHost::new(&[])),
    );
    assert_eq!(out.status, Status::Completed, "{out:#?}");
    assert!(out.log_truncated);
    assert!(out.log.len() < 10 && !out.log.is_empty(), "{:?}", out.log);
    assert_eq!(out.log[0], r#"line 0 {"i":0}"#);
}

#[test]
fn a_thrown_error_fails_the_program_with_its_message_and_stack() {
    let out = run(
        "function boom() { throw new TypeError('bad input'); }\nboom();",
        &budget(),
        Arc::new(TableHost::new(&[])),
    );
    assert_eq!(out.status, Status::Failed, "{out:#?}");
    let e = out.error.as_deref().unwrap();
    assert!(e.contains("bad input") && e.contains("boom"), "{e}");
    let out = run("return {", &budget(), Arc::new(TableHost::new(&[])));
    assert_eq!(out.status, Status::Failed, "{out:#?}");
    assert!(
        out.error.as_deref().unwrap().contains("SyntaxError"),
        "{out:#?}"
    );
    // Returning nothing is `null`, not a failure.
    let out = run("const x = 1;", &budget(), Arc::new(TableHost::new(&[])));
    assert_eq!(out.status, Status::Completed);
    assert_eq!(out.value_json.as_deref(), Some("null"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_async_form_runs_on_its_own_thread_and_awaits_real_host_futures() {
    let mut host = TableHost::new(&["fs.read"]);
    host.delay_ms = 100;
    let host = Arc::new(host);
    let started = std::time::Instant::now();
    let out: Outcome = run_async(
        "const [a, b, c] = await Promise.all([1, 2, 3].map((i) => tools.fs.read({ path: 'p' + i }))); return [a.path, b.path, c.path];"
            .into(),
        budget(),
        host,
        Cancel::new(),
    )
    .await;
    assert_eq!(out.status, Status::Completed, "{out:#?}");
    assert_eq!(out.value_json.as_deref(), Some(r#"["p1","p2","p3"]"#));
    // Three concurrent 100 ms host futures take about 100 ms inside the
    // isolate, not 300 (the wall clock also carries the thread and engine
    // start-up, bounded loosely for slow runners).
    assert!(out.elapsed_ms >= 100 && out.elapsed_ms < 260, "{out:#?}");
    assert!(started.elapsed().as_secs() < 5, "{:?}", started.elapsed());
}

#[test]
fn time_awaiting_the_host_is_not_cpu_time_and_the_host_can_cancel() {
    // A 150 ms CPU budget survives a 400 ms host wait: the deadline moves by
    // what the program waited for the host.
    let mut host = TableHost::new(&["fs.read"]);
    host.delay_ms = 400;
    let b = Budget {
        cpu_time_ms: 150,
        ..Budget::default()
    };
    let out = run(
        "const a = await tools.fs.read({ path: 'slow' }); let n = 0; for (let i = 0; i < 1000; i++) n += i; return [a.path, n];",
        &b,
        Arc::new(host),
    );
    assert_eq!(out.status, Status::Completed, "{out:#?}");
    assert_eq!(out.value_json.as_deref(), Some(r#"["slow",499500]"#));
    // The host ends a spinning program at its next interrupt poll.
    let cancel = Cancel::new();
    let flag = cancel.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(200));
        flag.cancel();
    });
    let started = std::time::Instant::now();
    let out = run_with(
        "for (;;) {}",
        &Budget {
            cpu_time_ms: 30_000,
            ..Budget::default()
        },
        Arc::new(TableHost::new(&[])),
        cancel,
    );
    assert_eq!(out.status, Status::Cancelled, "{out:#?}");
    assert!(started.elapsed().as_secs() < 5, "{:?}", started.elapsed());
    // A program awaiting the host when cancelled ends at its next binding
    // call, with the code the program could see.
    let cancel = Cancel::new();
    let flag = cancel.clone();
    let mut host = TableHost::new(&["fs.read"]);
    host.delay_ms = 300;
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(100));
        flag.cancel();
    });
    let out = run_with(
        "await tools.fs.read({ path: 'a' }); await tools.fs.read({ path: 'b' }); return 'unreachable';",
        &Budget::default(),
        Arc::new(host),
        cancel,
    );
    assert_eq!(out.status, Status::Cancelled, "{out:#?}");
    assert!(
        out.error.as_deref().unwrap().contains("cancelled"),
        "{out:#?}"
    );
}
