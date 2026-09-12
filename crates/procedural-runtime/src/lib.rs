//! `modbit-procedural-runtime` — the embedded JavaScript isolate with no
//! ambient authority (docs/16 "Procedural Tool Runtime", M5.2).
//!
//! Canonical owner: tool-runtime (`docs/12_REPOSITORY_AND_MODULE_LAYOUT.md`,
//! `docs/81_ARCHITECTURE_GUARDRAILS_AND_FORBIDDEN_DUPLICATION.md`).
//! Dependency direction is enforced by `tools/architecture-lint`.
//!
//! A program runs in a fresh QuickJS runtime that has no network, filesystem,
//! process, timer or module access: the engine's core intrinsics only, no
//! `std`/`os` library, no module loader, no `require`, no `fetch`. The only
//! way out is `tools.<toolset>.<name>(args)`: an async function generated for
//! every binding the [`Host`] offers, which hands the call — name and JSON
//! arguments — to the host and resolves with what the host returns. The host
//! is the seam through which M5.3 routes every binding through the Tool
//! Registry and the Capability Kernel; nothing in the isolate authorizes
//! anything.
//!
//! Budgets are enforced by the host side of the engine: a CPU-time deadline
//! checked from the interpreter's interrupt handler (which also bounds the
//! number of interrupt polls, an instruction-proportional measure), a memory
//! ceiling, a stack ceiling, a count of binding calls, and byte ceilings on
//! the program's log and on the value it returns. An exhausted budget ends
//! the program with the budget named.

use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use rquickjs::prelude::{Async, Func};
use rquickjs::{AsyncContext, AsyncRuntime, CatchResultExt, Ctx, Promise, Value};
use serde::{Deserialize, Serialize};

/// A boxed future a [`Host`] returns.
pub type HostFuture<T> = Pin<Box<dyn Future<Output = T> + Send + 'static>>;

/// What a binding call came back with when it did not succeed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostError {
    /// Stable code (`TOOL_NOT_PROJECTED`, `POLICY_DENIED`, …).
    pub code: String,
    /// Human-readable reason.
    pub message: String,
}

/// The host behind `tools.*`: names the bindings a program may call and
/// serves each call. Authority lives here (and, through M5.3, in the Tool
/// Registry and the Capability Kernel), never in the isolate.
pub trait Host: Send + Sync {
    /// Tool names offered to the program, in `toolset.name` form
    /// (`fs.read`, `git.worktree.create`). The projection of the node.
    fn bindings(&self) -> Vec<String>;
    /// Serve one call: the tool name and its arguments as JSON. `Ok` carries
    /// the result as JSON; `Err` becomes a rejected promise carrying `code`
    /// and `message`.
    fn invoke(&self, name: &str, arguments_json: &str) -> HostFuture<Result<String, HostError>>;
}

/// The budgets a program runs under.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Budget {
    /// Wall-clock CPU budget, checked from the interpreter's interrupt handler
    /// (the program stops at the next interrupt poll after the deadline).
    pub cpu_time_ms: u64,
    /// Ceiling on interrupt polls — an instruction-proportional measure the
    /// engine offers, independent of the machine's speed. `0` = unbounded.
    pub max_interrupt_polls: u64,
    /// Ceiling on the runtime's heap, in bytes.
    pub memory_bytes: usize,
    /// Ceiling on the interpreter stack, in bytes.
    pub stack_bytes: usize,
    /// Ceiling on `tools.*` calls.
    pub max_tool_calls: u32,
    /// Ceiling on the serialized return value, in bytes.
    pub max_output_bytes: usize,
    /// Ceiling on `console.log` output, in bytes (further lines are dropped
    /// and the log marked truncated).
    pub max_log_bytes: usize,
}

impl Default for Budget {
    fn default() -> Self {
        Self {
            cpu_time_ms: 5_000,
            max_interrupt_polls: 0,
            memory_bytes: 64 * 1024 * 1024,
            stack_bytes: 1024 * 1024,
            max_tool_calls: 64,
            max_output_bytes: 256 * 1024,
            max_log_bytes: 64 * 1024,
        }
    }
}

/// Which budget a program exhausted.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Exhausted {
    /// The CPU deadline (or the interrupt-poll ceiling).
    CpuTime,
    /// The heap ceiling.
    Memory,
    /// The interpreter stack ceiling.
    Stack,
    /// The `tools.*` call ceiling.
    ToolCalls,
    /// The return value ceiling.
    Output,
}

/// How a program ended.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Status {
    /// The program returned; `value_json` carries the value.
    Completed,
    /// The program threw (or a binding rejected and the program did not
    /// catch it); `error` carries the message and stack.
    Failed,
    /// A budget ended the program.
    BudgetExhausted {
        /// The budget.
        budget: Exhausted,
    },
    /// The host cancelled the program (at the next interrupt poll, or at the
    /// next binding call while it was awaiting one).
    Cancelled,
}

/// A handle the host keeps to end a running program: set it and the program
/// stops at its next interrupt poll or binding call.
#[derive(Clone, Debug, Default)]
pub struct Cancel(Arc<AtomicBool>);

impl Cancel {
    /// A fresh, unset handle.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// End the program.
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }

    /// Whether it was set.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

/// One binding call the program made, in order.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CallRecord {
    /// Tool name.
    pub name: String,
    /// Arguments JSON as the program passed them.
    pub arguments_json: String,
    /// `Ok` result JSON or the host's error code.
    pub outcome: Result<String, String>,
}

/// What a run produced.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Outcome {
    /// How it ended.
    pub status: Status,
    /// The returned value as JSON (`Completed` only; `null` when the program
    /// returned nothing).
    pub value_json: Option<String>,
    /// The error text (`Failed` and `BudgetExhausted`).
    pub error: Option<String>,
    /// `console.log` lines, bounded by the log budget.
    pub log: Vec<String>,
    /// Whether log lines were dropped.
    pub log_truncated: bool,
    /// Every binding call, in order.
    pub calls: Vec<CallRecord>,
    /// Interrupt polls the interpreter made.
    pub interrupt_polls: u64,
    /// Wall-clock time the program ran, in milliseconds.
    pub elapsed_ms: u64,
    /// Peak heap the runtime reported, in bytes.
    pub memory_peak_bytes: u64,
}

/// The identity of the engine behind the isolate (for receipts and
/// benchmarks).
pub const ENGINE: &str = "quickjs-ng 0.16.2 via rquickjs 0.13.0";

/// The prelude that turns the raw host binding into `tools.*` and
/// `console.log`, then removes the raw binding from the global object so the
/// program sees exactly the generated surface. Bindings are frozen; a
/// rejected call throws an `Error` whose `code` is the host's.
const PRELUDE: &str = r#"
(() => {
  const invoke = globalThis.__modbit_invoke;
  const log = globalThis.__modbit_log;
  const names = globalThis.__modbit_bindings;
  delete globalThis.__modbit_invoke;
  delete globalThis.__modbit_log;
  delete globalThis.__modbit_bindings;
  const tools = {};
  for (const name of names) {
    const parts = name.split('.');
    let cur = tools;
    for (let i = 0; i < parts.length - 1; i++) {
      if (!(parts[i] in cur)) cur[parts[i]] = {};
      cur = cur[parts[i]];
    }
    cur[parts[parts.length - 1]] = async (args) => {
      const raw = await invoke(name, JSON.stringify(args === undefined ? {} : args));
      const r = JSON.parse(raw);
      if (r.ok) return r.value;
      const e = new Error(r.message);
      e.code = r.code;
      e.tool = name;
      throw e;
    };
  }
  const freeze = (o) => {
    for (const k of Object.keys(o)) if (typeof o[k] === 'object' && o[k] !== null) freeze(o[k]);
    return Object.freeze(o);
  };
  Object.defineProperty(globalThis, 'tools', { value: freeze(tools), writable: false, configurable: false, enumerable: true });
  const render = (a) => a.map((x) => typeof x === 'string' ? x : JSON.stringify(x)).join(' ');
  const console = Object.freeze({
    log: (...a) => log(render(a)),
    info: (...a) => log(render(a)),
    warn: (...a) => log(render(a)),
    error: (...a) => log(render(a)),
    debug: (...a) => log(render(a)),
  });
  Object.defineProperty(globalThis, 'console', { value: console, writable: false, configurable: false, enumerable: true });
})();
"#;

/// The names a program must not find on the global object: what the
/// engine's library flavours or a browser would offer and this isolate does
/// not. Checked by the isolate's own tests; listed here so the surface is
/// one place.
pub const DENIED_GLOBALS: &[&str] = &[
    "require",
    "process",
    "fetch",
    "XMLHttpRequest",
    "WebSocket",
    "std",
    "os",
    "bjson",
    "Worker",
    "setTimeout",
    "setInterval",
    "importScripts",
    "Deno",
    "Bun",
];

/// A caught JavaScript error as `Name: message` with its stack — the
/// engine's own `Display` says `Error:` whatever the constructor was.
fn describe(e: &rquickjs::CaughtError<'_>) -> String {
    match e {
        rquickjs::CaughtError::Exception(ex) => {
            let name = ex
                .get::<_, String>("name")
                .ok()
                .filter(|n| !n.is_empty())
                .unwrap_or_else(|| "Error".into());
            let mut out = name;
            if let Some(m) = ex.message() {
                out.push_str(": ");
                out.push_str(&m);
            }
            if let Some(stack) = ex.stack() {
                out.push('\n');
                out.push_str(&stack);
            }
            out
        }
        other => other.to_string(),
    }
}

struct Meter {
    calls: AtomicU32,
    call_budget_hit: AtomicBool,
    log_bytes: AtomicUsize,
    log_truncated: AtomicBool,
    log: Mutex<Vec<String>>,
    calls_made: Mutex<Vec<CallRecord>>,
}

/// Run `source` as the body of an async function (so `await` and `return`
/// work at the top level) under `budget`, with `host` behind `tools.*`.
/// Blocks the calling thread for the program's duration; the isolate lives
/// on a dedicated thread with its own single-threaded executor, so the
/// caller's executor is never stalled by the interpreter.
///
/// # Panics
/// Never for a program's behaviour; only if the isolate thread cannot be
/// spawned.
#[must_use]
pub fn run(source: &str, budget: &Budget, host: Arc<dyn Host>) -> Outcome {
    run_with(source, budget, host, Cancel::new())
}

/// [`run`] with a [`Cancel`] handle the host may set from another thread.
#[must_use]
pub fn run_with(source: &str, budget: &Budget, host: Arc<dyn Host>, cancel: Cancel) -> Outcome {
    let source = source.to_owned();
    let budget = budget.clone();
    let handle = std::thread::Builder::new()
        .name("modbit-isolate".into())
        // Room for the interpreter's own stack ceiling plus the host's frames.
        .stack_size(budget.stack_bytes + 4 * 1024 * 1024)
        .spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_time()
                .build()
                .expect("a current-thread executor for the isolate");
            rt.block_on(run_in_isolate(&source, &budget, host, cancel))
        })
        .expect("the isolate thread spawns");
    match handle.join() {
        Ok(o) => o,
        Err(_) => Outcome {
            status: Status::Failed,
            value_json: None,
            error: Some("the isolate thread panicked".into()),
            log: vec![],
            log_truncated: false,
            calls: vec![],
            interrupt_polls: 0,
            elapsed_ms: 0,
            memory_peak_bytes: 0,
        },
    }
}

/// The async form of [`run`] for callers on a multi-threaded executor: the
/// isolate still runs on its own thread; only the wait is asynchronous.
pub async fn run_async(
    source: String,
    budget: Budget,
    host: Arc<dyn Host>,
    cancel: Cancel,
) -> Outcome {
    tokio::task::spawn_blocking(move || run_with(&source, &budget, host, cancel))
        .await
        .unwrap_or_else(|_| Outcome {
            status: Status::Failed,
            value_json: None,
            error: Some("the isolate task was cancelled".into()),
            log: vec![],
            log_truncated: false,
            calls: vec![],
            interrupt_polls: 0,
            elapsed_ms: 0,
            memory_peak_bytes: 0,
        })
}

async fn run_in_isolate(
    source: &str,
    budget: &Budget,
    host: Arc<dyn Host>,
    cancel: Cancel,
) -> Outcome {
    let started = Instant::now();
    let polls = Arc::new(AtomicU32::new(0));
    let cpu_hit = Arc::new(AtomicBool::new(false));
    let cancel_hit = Arc::new(AtomicBool::new(false));
    // Time the program spent awaiting the host (an approval, a slow tool) is
    // not its CPU time: every binding call extends the deadline by what it
    // waited.
    let waited_ms = Arc::new(AtomicU64::new(0));
    let meter = Arc::new(Meter {
        calls: AtomicU32::new(0),
        call_budget_hit: AtomicBool::new(false),
        log_bytes: AtomicUsize::new(0),
        log_truncated: AtomicBool::new(false),
        log: Mutex::new(Vec::new()),
        calls_made: Mutex::new(Vec::new()),
    });
    let mut outcome = Outcome {
        status: Status::Failed,
        value_json: None,
        error: None,
        log: vec![],
        log_truncated: false,
        calls: vec![],
        interrupt_polls: 0,
        elapsed_ms: 0,
        memory_peak_bytes: 0,
    };
    let rt = match AsyncRuntime::new() {
        Ok(rt) => rt,
        Err(e) => {
            outcome.error = Some(format!("the engine could not start: {e}"));
            return outcome;
        }
    };
    rt.set_memory_limit(budget.memory_bytes).await;
    rt.set_max_stack_size(budget.stack_bytes).await;
    {
        let deadline = started + Duration::from_millis(budget.cpu_time_ms);
        let max_polls = budget.max_interrupt_polls;
        let polls = polls.clone();
        let cpu_hit = cpu_hit.clone();
        let cancel_hit = cancel_hit.clone();
        let cancel = cancel.clone();
        let waited_ms = waited_ms.clone();
        rt.set_interrupt_handler(Some(Box::new(move || {
            let n = u64::from(polls.fetch_add(1, Ordering::Relaxed)) + 1;
            if cancel.is_cancelled() {
                cancel_hit.store(true, Ordering::Relaxed);
                return true;
            }
            let deadline = deadline + Duration::from_millis(waited_ms.load(Ordering::Relaxed));
            let over = Instant::now() >= deadline || (max_polls > 0 && n > max_polls);
            if over {
                cpu_hit.store(true, Ordering::Relaxed);
            }
            over
        })))
        .await;
    }
    let ctx = match AsyncContext::full(&rt).await {
        Ok(c) => c,
        Err(e) => {
            outcome.error = Some(format!("the engine could not open a context: {e}"));
            return outcome;
        }
    };
    let bindings = host.bindings();
    let max_calls = budget.max_tool_calls;
    let max_log = budget.max_log_bytes;
    let max_output = budget.max_output_bytes;
    let program = format!("(async () => {{\n{source}\n}})()");
    let result: Result<Result<Option<String>, String>, String> = ctx
        .async_with(async |ctx: Ctx<'_>| {
            // The raw bindings; the prelude wraps and removes them.
            let install = (|| -> rquickjs::Result<()> {
                let globals = ctx.globals();
                globals.set("__modbit_bindings", bindings.clone())?;
                let host_for_invoke = host.clone();
                let meter_for_invoke = meter.clone();
                let waited_for_invoke = waited_ms.clone();
                let cancel_for_invoke = cancel.clone();
                globals.set(
                    "__modbit_invoke",
                    Func::from(Async(move |name: String, args: String| {
                        let host = host_for_invoke.clone();
                        let meter = meter_for_invoke.clone();
                        let waited_ms = waited_for_invoke.clone();
                        let cancel = cancel_for_invoke.clone();
                        async move {
                            if cancel.is_cancelled() {
                                return serde_json::json!({
                                    "ok": false,
                                    "code": "CANCELLED",
                                    "message": "the program was cancelled",
                                })
                                .to_string();
                            }
                            let n = meter.calls.fetch_add(1, Ordering::Relaxed) + 1;
                            if n > max_calls {
                                meter.call_budget_hit.store(true, Ordering::Relaxed);
                                return serde_json::json!({
                                    "ok": false,
                                    "code": "TOOL_CALL_BUDGET",
                                    "message": format!("the program's tool-call budget ({max_calls}) is spent"),
                                })
                                .to_string();
                            }
                            let t0 = Instant::now();
                            let r = host.invoke(&name, &args).await;
                            waited_ms.fetch_add(
                                u64::try_from(t0.elapsed().as_millis()).unwrap_or(u64::MAX),
                                Ordering::Relaxed,
                            );
                            let record = CallRecord {
                                name: name.clone(),
                                arguments_json: args.clone(),
                                outcome: match &r {
                                    Ok(v) => Ok(v.clone()),
                                    Err(e) => Err(e.code.clone()),
                                },
                            };
                            if let Ok(mut calls) = meter.calls_made.lock() {
                                calls.push(record);
                            }
                            match r {
                                Ok(value) => {
                                    // The host's JSON is spliced in verbatim; a
                                    // host that returns something that is not
                                    // JSON is reported, not trusted.
                                    match serde_json::from_str::<serde_json::Value>(&value) {
                                        Ok(v) => serde_json::json!({"ok": true, "value": v}).to_string(),
                                        Err(e) => serde_json::json!({
                                            "ok": false,
                                            "code": "HOST_RESULT_NOT_JSON",
                                            "message": format!("the host returned a result that is not JSON: {e}"),
                                        })
                                        .to_string(),
                                    }
                                }
                                Err(e) => serde_json::json!({"ok": false, "code": e.code, "message": e.message})
                                    .to_string(),
                            }
                        }
                    })),
                )?;
                let meter_for_log = meter.clone();
                globals.set(
                    "__modbit_log",
                    Func::from(move |line: String| {
                        let used = meter_for_log.log_bytes.load(Ordering::Relaxed);
                        if used + line.len() > max_log {
                            meter_for_log.log_truncated.store(true, Ordering::Relaxed);
                            return;
                        }
                        meter_for_log
                            .log_bytes
                            .fetch_add(line.len(), Ordering::Relaxed);
                        if let Ok(mut log) = meter_for_log.log.lock() {
                            log.push(line);
                        }
                    }),
                )?;
                let _: Value = ctx.eval(PRELUDE)?;
                Ok(())
            })();
            if let Err(e) = install {
                let caught = rquickjs::CaughtError::from_error(&ctx, e);
                return Err(format!("the isolate could not be prepared: {caught}"));
            }
            let promise: Promise<'_> = match ctx.eval::<Promise<'_>, _>(program).catch(&ctx) {
                Ok(p) => p,
                Err(e) => return Ok(Err(describe(&e))),
            };
            match promise.into_future::<Value<'_>>().await.catch(&ctx) {
                Ok(value) => {
                    if value.is_undefined() {
                        return Ok(Ok(None));
                    }
                    match ctx.json_stringify(value) {
                        Ok(Some(s)) => Ok(Ok(Some(s.to_string().unwrap_or_default()))),
                        Ok(None) => Ok(Ok(None)),
                        Err(e) => {
                            let caught = rquickjs::CaughtError::from_error(&ctx, e);
                            Ok(Err(format!("the return value is not serializable: {caught}")))
                        }
                    }
                }
                Err(e) => Ok(Err(describe(&e))),
            }
        })
        .await;
    // The engine's view of the heap after the run, before it is torn down.
    let usage = rt.memory_usage().await;
    outcome.memory_peak_bytes =
        u64::try_from(usage.malloc_size.max(usage.memory_used_size)).unwrap_or_default();
    outcome.interrupt_polls = u64::from(polls.load(Ordering::Relaxed));
    outcome.elapsed_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    outcome.log = meter.log.lock().map(|l| l.clone()).unwrap_or_default();
    outcome.log_truncated = meter.log_truncated.load(Ordering::Relaxed);
    outcome.calls = meter
        .calls_made
        .lock()
        .map(|c| c.clone())
        .unwrap_or_default();
    let cpu_hit = cpu_hit.load(Ordering::Relaxed);
    let calls_hit = meter.call_budget_hit.load(Ordering::Relaxed);
    match result {
        Err(setup) => {
            outcome.status = Status::Failed;
            outcome.error = Some(setup);
        }
        Ok(Ok(value)) => {
            if let Some(v) = &value
                && v.len() > max_output
            {
                outcome.status = Status::BudgetExhausted {
                    budget: Exhausted::Output,
                };
                outcome.error = Some(format!(
                    "the return value is {} bytes; the budget is {max_output}",
                    v.len()
                ));
            } else {
                outcome.status = Status::Completed;
                outcome.value_json = Some(value.unwrap_or_else(|| "null".into()));
            }
        }
        Ok(Err(error)) if cancel_hit.load(Ordering::Relaxed) || cancel.is_cancelled() => {
            outcome.status = Status::Cancelled;
            outcome.error = Some(format!("cancelled by the host: {error}"));
        }
        Ok(Err(error)) => {
            let lower = error.to_lowercase();
            let hit = if cpu_hit {
                Some(Exhausted::CpuTime)
            } else if lower.contains("out of memory") {
                Some(Exhausted::Memory)
            } else if lower.contains("stack overflow") || lower.contains("maximum call stack") {
                Some(Exhausted::Stack)
            } else if calls_hit && lower.contains("tool-call budget") {
                Some(Exhausted::ToolCalls)
            } else {
                None
            };
            match hit {
                Some(b) => {
                    outcome.status = Status::BudgetExhausted { budget: b };
                    outcome.error = Some(match b {
                        Exhausted::CpuTime => format!(
                            "the CPU budget ({} ms{}) ended the program after {} interrupt polls: {error}",
                            budget.cpu_time_ms,
                            if budget.max_interrupt_polls > 0 {
                                format!(", {} polls", budget.max_interrupt_polls)
                            } else {
                                String::new()
                            },
                            outcome.interrupt_polls
                        ),
                        _ => error,
                    });
                }
                None => {
                    outcome.status = Status::Failed;
                    outcome.error = Some(error);
                }
            }
        }
    }
    outcome
}
