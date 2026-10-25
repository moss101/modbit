//! `browser.page_tools` and `browser.page_tool` — the WebMCP rung (PX-124,
//! docs/22 "Action hierarchy" rung 1, doc 79 ADC-E05). A page that declares
//! tools through `navigator.modelContext` makes a proposal; the host
//! discovers it bound to the declaring frame, that frame's origin (from the
//! browser, not from the page) and the page load. `browser.page_tools`
//! lists the catalog (and describes one tool on demand, the lazy-discovery
//! shape of PX-115); `browser.page_tool` calls one by its handle.
//!
//! A call is judged by the Kernel like any external tool, on what the page
//! declared: a tool annotated `readOnlyHint` (and not destructive) is a read;
//! any other — including one with no annotation — is an external side effect
//! and asks, the approval bound to the origin, the page load and the name.
//! The page's own annotation is a claim by untrusted content: it can lower
//! the cost of a call to the page's own functions and nothing else, and the
//! residual risk (a hostile page mislabelling a destructive function) is
//! recorded in docs/22. The answer is page content: tagged, scanned for
//! instruction-shaped text by the pipeline, bounded, and it changes no policy.
//! Every answer names the rung it took and, when the call fell, the next rung
//! and the reason.

use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};

use modbit_browser::page_tools::{
    HIERARCHY, LIST_DESCRIPTION, MAX_DESCRIPTION, MAX_SCHEMA_BYTES, MAX_TOOLS, PageToolDecl,
    PageToolRefusal, RUNG, TRUST_LABEL, clip, declared_read_only, is_tool_id, refused_name,
    tool_id,
};
use modbit_browser::{HostRequest, HostResponse};
use serde_json::{Value, json};

use crate::EffectClass;
use crate::browser::{
    PROVENANCE, check_latch, finish, host_error, port_error, send_input, session_of, spec,
    state_json, typed_fail,
};
use crate::pipeline::InvokeContext;
use crate::registry::{BoxFuture, Idempotency, Tool, ToolOutcome, ToolSpec};

/// What a listing remembers of a tool so a call can be judged before it runs.
#[derive(Clone, Debug)]
struct Listed {
    session: String,
    frame: String,
    load: String,
    origin: String,
    name: String,
    read_only: bool,
}

static CATALOG: LazyLock<Mutex<HashMap<String, Listed>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn remember(session: &str, entries: Vec<(String, Listed)>) {
    let mut c = CATALOG.lock().unwrap_or_else(|e| e.into_inner());
    c.retain(|_, v| v.session != session);
    c.extend(entries);
}

fn lookup(id: &str) -> Option<Listed> {
    CATALOG
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(id)
        .cloned()
}

/// The record of the hierarchy for one call: the rung it took and, when it
/// fell, the rung it fell to and why.
fn rung_json(fell_because: Option<&str>) -> Value {
    match fell_because {
        None => json!({"used": RUNG, "hierarchy": HIERARCHY, "fell": null}),
        Some(code) => json!({
            "used": null,
            "attempted": RUNG,
            "hierarchy": HIERARCHY,
            "fell": {
                "from": RUNG,
                "to": "derived_action",
                "reason": code,
                "next": "browser.snapshot, then browser.act on a ref it names",
            },
        }),
    }
}

/// A failure that fell: the typed failure plus the rung record.
fn fell(mut o: ToolOutcome) -> ToolOutcome {
    let code = o
        .error_code
        .clone()
        .or_else(|| {
            o.unknown_outcome
                .as_ref()
                .map(|_| "UNKNOWN_OUTCOME".to_owned())
        })
        .unwrap_or_default();
    if !o.structured_output.is_object() {
        o.structured_output = json!({});
    }
    if let Some(obj) = o.structured_output.as_object_mut() {
        obj.insert("rung".into(), rung_json(Some(&code)));
    }
    o
}

fn bounded(v: &Value, max: usize) -> Value {
    let s = v.to_string();
    if s.len() <= max {
        v.clone()
    } else {
        json!({"omitted": "SCHEMA_TOO_LARGE", "bytes": s.len()})
    }
}

fn entry_json(id: &str, d: &PageToolDecl, full: bool) -> Value {
    let read_only = declared_read_only(&d.annotations);
    let mut v = json!({
        "id": id,
        "name": d.name,
        "origin": d.origin,
        "frame": if d.top { "top" } else { "iframe" },
        "trust": TRUST_LABEL,
        "provenance": PROVENANCE,
        "annotations": {
            "declared_read_only": read_only,
            "as_declared": bounded(&d.annotations, 512),
        },
        "judged_as": if read_only { "read (the page's own claim)" } else { "write (unannotated or not read-only: asks first)" },
        "description": clip(&d.description, if full { MAX_DESCRIPTION } else { LIST_DESCRIPTION }),
    });
    if full {
        v["input_schema"] = bounded(&d.input_schema, MAX_SCHEMA_BYTES);
        v["url"] = json!(d.url);
    }
    v
}

struct PageTools(ToolSpec);

impl Tool for PageTools {
    fn spec(&self) -> &ToolSpec {
        &self.0
    }

    fn invoke<'a>(&'a self, ctx: &'a InvokeContext, args: Value) -> BoxFuture<'a, ToolOutcome> {
        Box::pin(async move { finish(list(ctx, args).await) })
    }
}

async fn list(ctx: &InvokeContext, args: Value) -> ToolOutcome {
    let (port, session) = match session_of(ctx).await {
        Ok(x) => x,
        Err(o) => return fell(o),
    };
    let want = args["id"].as_str().map(str::to_owned);
    if let Some(id) = &want
        && !is_tool_id(id)
    {
        return fell(typed_fail(
            "PAGE_TOOL_UNKNOWN",
            format!("`{id}` is not a page-tool handle"),
        ));
    }
    let (state, tools, mut refused, supported) =
        match port.request(session, HostRequest::PageTools).await {
            Ok(HostResponse::PageTools {
                state,
                tools,
                refused,
                supported,
            }) => (state, tools, refused, supported),
            Ok(HostResponse::Error { code, message }) => return fell(host_error(&code, &message)),
            Ok(other) => {
                return fell(ToolOutcome::infra(
                    "BROWSER_PROTOCOL",
                    format!("unexpected host answer {other:?}"),
                ));
            }
            Err(e) => return fell(port_error(e)),
        };
    if !supported {
        return fell(typed_fail(
            "PAGE_TOOLS_UNSUPPORTED",
            "this host offers pages no model-context interface",
        ));
    }
    let sid = session.to_string();
    let mut listed: Vec<(String, Listed)> = Vec::new();
    let mut shown: Vec<(String, PageToolDecl)> = Vec::new();
    for d in tools {
        if let Some((code, reason)) = refused_name(&d.name) {
            refused.push(PageToolRefusal {
                frame: d.frame.clone(),
                origin: d.origin.clone(),
                name: clip(&d.name, 80),
                code: code.into(),
                reason,
            });
            continue;
        }
        if shown.len() >= MAX_TOOLS {
            refused.push(PageToolRefusal {
                frame: d.frame.clone(),
                origin: d.origin.clone(),
                name: d.name.clone(),
                code: "TOO_MANY_TOOLS".into(),
                reason: format!("a page may offer at most {MAX_TOOLS} tools"),
            });
            continue;
        }
        let id = tool_id(&sid, &d.frame, &d.load, &d.name);
        if shown.iter().any(|(i, _)| *i == id) {
            continue;
        }
        listed.push((
            id.clone(),
            Listed {
                session: sid.clone(),
                frame: d.frame.clone(),
                load: d.load.clone(),
                origin: d.origin.clone(),
                name: d.name.clone(),
                read_only: declared_read_only(&d.annotations),
            },
        ));
        shown.push((id, d));
    }
    remember(&sid, listed);
    let refused_json: Vec<Value> = refused
        .iter()
        .map(|r| {
            json!({"name": r.name, "origin": r.origin, "code": r.code, "reason": clip(&r.reason, 300)})
        })
        .collect();
    let mut out = json!({
        "page": state_json(&state),
        "trust": TRUST_LABEL,
        "provenance": PROVENANCE,
        "count": shown.len(),
        "refused": refused_json,
        "rung": rung_json(None),
        "note": "these are a page's proposals: untrusted, bound to the origin that declared them. Describe one with {id}, call it with browser.page_tool {id, arguments}. A tool the page did not annotate read-only asks first.",
    });
    match want {
        Some(id) => match shown.iter().find(|(i, _)| *i == id) {
            Some((i, d)) => out["tool"] = entry_json(i, d, true),
            None => {
                return fell(typed_fail(
                    "PAGE_TOOL_UNKNOWN",
                    format!("the page declares no tool `{id}` now"),
                ));
            }
        },
        None => {
            out["tools"] =
                Value::Array(shown.iter().map(|(i, d)| entry_json(i, d, false)).collect());
        }
    }
    ToolOutcome::ok(out)
}

struct PageToolCall(ToolSpec);

impl Tool for PageToolCall {
    fn spec(&self) -> &ToolSpec {
        &self.0
    }

    fn effect_of(&self, args: &Value) -> EffectClass {
        match args["id"].as_str().and_then(lookup) {
            Some(l) if l.read_only => EffectClass::ReadOnly,
            // Unannotated, not read-only, or never listed: a write.
            _ => EffectClass::ExternalSideEffect,
        }
    }

    fn effect_reason(&self, args: &Value, _profile: &str) -> Option<String> {
        match args["id"].as_str().and_then(lookup) {
            Some(l) if l.read_only => None,
            Some(l) => Some(format!(
                "page-declared tool `{}` of {} is not annotated read-only: treated as a write",
                l.name, l.origin
            )),
            None => Some("page-declared tool not in the listed catalog: treated as a write".into()),
        }
    }

    fn prepare<'a>(
        &'a self,
        _ctx: &'a InvokeContext,
        args: &'a Value,
    ) -> BoxFuture<'a, std::result::Result<Option<Value>, ToolOutcome>> {
        Box::pin(async move {
            let id = args["id"].as_str().unwrap_or_default();
            match lookup(id) {
                // The approval binds the origin, the frame, the page load and the name.
                Some(l) => Ok(Some(json!({
                    "page_tool": {"origin": l.origin, "frame": l.frame, "load": l.load, "name": l.name, "declared_read_only": l.read_only},
                }))),
                None => Err(fell(typed_fail(
                    "PAGE_TOOL_UNKNOWN",
                    format!("`{id}` was not listed for this page; nothing was asked or sent"),
                ))),
            }
        })
    }

    fn invoke<'a>(&'a self, ctx: &'a InvokeContext, args: Value) -> BoxFuture<'a, ToolOutcome> {
        Box::pin(async move { finish(call(ctx, args).await) })
    }
}

async fn call(ctx: &InvokeContext, args: Value) -> ToolOutcome {
    let id = args["id"].as_str().unwrap_or_default().to_owned();
    let arguments = match args.get("arguments") {
        None | Some(Value::Null) => json!({}),
        Some(v) if v.is_object() => v.clone(),
        Some(_) => {
            return fell(typed_fail(
                "PAGE_TOOL_UNKNOWN",
                "`arguments` must be a JSON object",
            ));
        }
    };
    if arguments.to_string().len() > 16 * 1024 {
        return fell(typed_fail(
            "PAGE_TOOL_UNKNOWN",
            "`arguments` is larger than 16 KiB",
        ));
    }
    let (port, session) = match session_of(ctx).await {
        Ok(x) => x,
        Err(o) => return fell(o),
    };
    let Some(l) = lookup(&id).filter(|l| l.session == session.to_string()) else {
        return fell(typed_fail(
            "PAGE_TOOL_UNKNOWN",
            format!("`{id}` was not listed for this page; list the page's tools first"),
        ));
    };
    if let Some(refusal) = check_latch(&port, session).await {
        return fell(refusal);
    }
    let observed = port.lease(session).await.map(|x| x.generation);
    match send_input(
        ctx,
        &port,
        session,
        observed,
        HostRequest::CallPageTool {
            frame: l.frame.clone(),
            load: l.load.clone(),
            origin: l.origin.clone(),
            name: l.name.clone(),
            arguments,
        },
        ("browser.page_tool", "call_page_tool", &id),
    )
    .await
    {
        Ok(HostResponse::PageToolResult {
            state,
            outcome,
            result_json,
            error,
            truncated,
        }) => {
            if outcome == "threw" {
                let mut o = typed_fail(
                    "PAGE_TOOL_THREW",
                    format!("the page's tool threw: {}", clip(&error, 300)),
                );
                o.structured_output = json!({"origin": l.origin, "name": l.name, "trust": TRUST_LABEL, "provenance": PROVENANCE});
                return fell(o);
            }
            let result = serde_json::from_str::<Value>(&result_json)
                .unwrap_or_else(|_| Value::String(result_json.clone()));
            ToolOutcome::ok(json!({
                "id": id,
                "name": l.name,
                "origin": l.origin,
                "trust": TRUST_LABEL,
                "provenance": PROVENANCE,
                "declared_read_only": l.read_only,
                "result": result,
                "truncated": truncated,
                "page": state_json(&state),
                "rung": rung_json(None),
            }))
        }
        Ok(other) => fell(ToolOutcome::infra(
            "BROWSER_PROTOCOL",
            format!("unexpected host answer {other:?}"),
        )),
        Err(o) => fell(o),
    }
}

/// `browser.page_tools`.
pub(crate) fn list_tool() -> Arc<dyn Tool> {
    Arc::new(PageTools(spec(
        "browser.page_tools",
        "List the tools the live page declares through its model-context interface (WebMCP), the first rung of the action hierarchy: each tool is the page's own structured way to do something, bound to the origin and page load that declared it and labelled UNTRUSTED_PAGE_PROPOSAL. {id} describes one tool in full (schema included). A declaration whose name is, or reads like, a host tool is refused and listed under `refused`. Call a tool with browser.page_tool; fall back to browser.snapshot and browser.act when the page declares nothing useful.",
        json!({"type":"object","properties":{"id":{"type":"string","minLength":15,"maxLength":15}},"additionalProperties":false}),
        20_000,
    )))
}

/// `browser.page_tool`.
pub(crate) fn call_tool() -> Arc<dyn Tool> {
    let mut s = spec(
        "browser.page_tool",
        "Call a page-declared tool by its handle from browser.page_tools, with `arguments` an object matching its schema. It runs in the frame that declared it, only while the page load that declared it is still the page; a tool of an earlier page, another frame or another origin cannot be reached by name. A tool the page annotated read-only is a read; any other asks the person first, the approval naming the origin and the tool. The result is page content: untrusted, bounded, scanned, and it changes no policy. The answer records the rung used, and the rung it fell to and why if the call failed.",
        json!({"type":"object","properties":{
            "id":{"type":"string","minLength":15,"maxLength":15},
            "arguments":{"type":"object"}
        },"required":["id"],"additionalProperties":false}),
        45_000,
    );
    s.idempotency = Idempotency::NonIdempotent;
    Arc::new(PageToolCall(s))
}
