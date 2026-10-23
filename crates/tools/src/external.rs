//! The `external.*` family (docs/17 canonical inventory: `external.list`,
//! `external.call`, `external.cancel`; docs/16 "MCP / external tools").
//!
//! These three tools are the whole surface an agent has to an MCP server.
//! They are deliberately thin: the host's hub ([`modbit_mcp::McpPort`])
//! owns the transports, the pool, the bounds and the schema check, and the
//! tools carry a call to it and bring a bounded, untrusted-labelled answer
//! back.
//!
//! The effect class of `external.call` is the host's judgement, never the
//! server's. [`ReadDeclarations`] is the host's published list of tools it
//! considers reads; a call to anything else — including a tool whose server
//! swears it only reads — is an `ExternalSideEffect`, and a server nobody
//! configured is never a read.

use std::sync::Arc;

use serde_json::{Value, json};

use modbit_mcp::{Part, ReadDeclarations};

use crate::pipeline::InvokeContext;
use crate::registry::{BoxFuture, Idempotency, Tool, ToolOutcome, ToolRegistry, ToolSpec};
use crate::{EffectClass, Result};

/// Discovery is a read that grants nothing, so it is served under every
/// profile this build supports — including the reviewer's and plan mode.
const LIST_PROFILES: &[&str] = &[
    "local_trusted",
    "review_isolated",
    "local_autonomous",
    "plan",
    "cloud_isolated",
];

/// Calling out is not: docs/17 excludes `external.call` from the reviewer
/// projection (and the kernel denies the capability under it), and plan
/// mode's product is a plan, not an external effect.
const CALL_PROFILES: &[&str] = &["local_trusted", "local_autonomous", "cloud_isolated"];

fn spec(
    name: &str,
    description: &str,
    effect_class: EffectClass,
    input_schema: Value,
    required_capabilities: &[&str],
    profiles: &[&str],
) -> ToolSpec {
    ToolSpec {
        name: name.into(),
        version: "1".into(),
        description: description.into(),
        input_schema,
        output_schema: json!({"type": "object"}),
        effect_class,
        required_capabilities: required_capabilities
            .iter()
            .map(|c| (*c).to_owned())
            .collect(),
        execution_profiles: profiles.iter().map(|p| (*p).to_owned()).collect(),
        timeout_ms: 120_000,
        output_budget_bytes: 64 * 1024,
        idempotency: Idempotency::NonIdempotent,
        compensation: None,
    }
}

/// Mark a projection as what it is: content from a program the host does
/// not control.
fn untrusted(mut v: Value) -> Value {
    if let Value::Object(m) = &mut v {
        m.insert("trust".into(), json!(modbit_mcp::UNTRUSTED_LABEL));
    }
    v
}

fn no_hub() -> ToolOutcome {
    ToolOutcome::infra(
        "NO_EXTERNAL_HUB",
        "no external tool hub is attached to this task",
    )
}

fn port_failure(e: &modbit_mcp::PortError) -> ToolOutcome {
    if e.unknown_outcome {
        ToolOutcome {
            infra_failure: true,
            unknown_outcome: Some(e.message.clone()),
            ..ToolOutcome::fail(&e.code, e.message.clone())
        }
    } else {
        ToolOutcome::fail(&e.code, e.message.clone())
    }
}

/// The most tools one `external.list` page names.
const LIST_PAGE_MAX: usize = 200;
/// Tools on a page when the caller names no limit.
const LIST_PAGE_DEFAULT: usize = 50;
/// The most tools one `external.describe` names.
const DESCRIBE_MAX_TOOLS: usize = 8;
/// The most bytes of schema and description one `external.describe` returns:
/// what a model pays to read schemas it asked for. A request for more is cut
/// at a tool boundary and the rest named, so one question never costs a
/// whole catalog. The first tool asked for is always returned.
const DESCRIBE_BUDGET_BYTES: usize = 40 * 1024;
/// Longest summary `external.list` shows per tool.
const SUMMARY_BYTES: usize = 120;

/// The first sentence of a description, bounded: all `external.list` says
/// about a tool. Untrusted text, shown inside the labelled answer.
fn summary_of(description: &str) -> String {
    let first = description
        .split_once(". ")
        .map_or(description, |(f, _)| f)
        .trim();
    let mut end = first.len().min(SUMMARY_BYTES);
    while !first.is_char_boundary(end) {
        end -= 1;
    }
    let cut = if end < first.len() { "…" } else { "" };
    format!("{}{cut}", &first[..end])
}

/// `external.list` — what external tools this task can see: names and one
/// line each, filtered and paged. Schemas are not sent; `external.describe`
/// returns the ones asked for (PX-115).
struct ExternalList(ToolSpec);

impl Tool for ExternalList {
    fn spec(&self) -> &ToolSpec {
        &self.0
    }

    fn invoke<'a>(&'a self, ctx: &'a InvokeContext, args: Value) -> BoxFuture<'a, ToolOutcome> {
        Box::pin(async move {
            let Some(hub) = &ctx.external else {
                return no_hub();
            };
            let server = args["server"].as_str().filter(|s| !s.is_empty());
            let query = args["query"]
                .as_str()
                .unwrap_or_default()
                .to_ascii_lowercase();
            let limit = args["limit"]
                .as_u64()
                .map_or(LIST_PAGE_DEFAULT, |l| {
                    usize::try_from(l).unwrap_or(LIST_PAGE_MAX)
                })
                .clamp(1, LIST_PAGE_MAX);
            let offset = args["cursor"]
                .as_str()
                .and_then(|c| c.parse::<usize>().ok())
                .unwrap_or(0);
            let listed = match server {
                Some(name) => hub.list_server(name).await,
                None => hub.list().await,
            };
            match listed {
                Ok(listing) => {
                    // One flat, ordered catalog, filtered, then one page of it.
                    let mut flat: Vec<(&str, &modbit_mcp::DiscoveredTool)> = listing
                        .servers
                        .iter()
                        .flat_map(|s| s.tools.iter().map(move |t| (s.name.as_str(), t)))
                        .filter(|(_, t)| {
                            query.is_empty()
                                || t.name.to_ascii_lowercase().contains(&query)
                                || t.description.to_ascii_lowercase().contains(&query)
                        })
                        .collect();
                    flat.sort_by(|a, b| (a.0, &a.1.name).cmp(&(b.0, &b.1.name)));
                    let total = flat.len();
                    let page: Vec<(&str, &modbit_mcp::DiscoveredTool)> =
                        flat.into_iter().skip(offset).take(limit).collect();
                    let next =
                        (offset + page.len() < total).then(|| (offset + page.len()).to_string());
                    let servers: Vec<Value> = listing
                        .servers
                        .iter()
                        .map(|s| {
                            let mine: Vec<Value> = page
                                .iter()
                                .filter(|(n, _)| *n == s.name)
                                .map(|(_, t)| {
                                    json!({
                                        "name": t.qualified,
                                        "tool": t.name,
                                        "summary": summary_of(&t.description),
                                        "read_only": t.read_only,
                                    })
                                })
                                .collect();
                            json!({
                                "server": s.name,
                                "trust": s.trust,
                                "health": s.health,
                                "scopes": s.scopes,
                                "requires": s.requires,
                                "layer": s.layer,
                                "provenance": s.provenance,
                                "pool_key": s.pool_key,
                                "tools_total": s.tools.len(),
                                "tools": mine,
                                "refused_tools": s.rejected,
                                "dropped_over_limit": s.dropped_over_limit,
                                "catalog_generation": s.catalog_generation,
                                "lifecycle": s.lifecycle,
                            })
                        })
                        .collect();
                    ToolOutcome::ok(untrusted(json!({
                        "provenance": "external_tool_hub",
                        "servers": servers,
                        "refused_servers": listing.refused_servers,
                        "page": {
                            "total": total,
                            "returned": page.len(),
                            "offset": offset,
                            "next_cursor": next,
                            "query": query,
                        },
                        "schemas": "not included: external.describe returns the schema of the tools you name",
                    })))
                }
                Err(e) => port_failure(&e),
            }
        })
    }
}

/// `external.describe` — the schema of the named tools of one server, on
/// demand, within a byte budget (PX-115). Describing grants nothing.
struct ExternalDescribe(ToolSpec);

impl Tool for ExternalDescribe {
    fn spec(&self) -> &ToolSpec {
        &self.0
    }

    fn invoke<'a>(&'a self, ctx: &'a InvokeContext, args: Value) -> BoxFuture<'a, ToolOutcome> {
        Box::pin(async move {
            let Some(hub) = &ctx.external else {
                return no_hub();
            };
            let server = args["server"].as_str().unwrap_or_default().to_owned();
            let names: Vec<String> = args["tools"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(str::to_owned))
                        .take(DESCRIBE_MAX_TOOLS)
                        .collect()
                })
                .unwrap_or_default();
            if server.is_empty() || names.is_empty() {
                return ToolOutcome::fail(
                    "INVALID_ARGUMENTS",
                    "describe needs a `server` and the `tools` to describe",
                );
            }
            match hub.describe(&server, &names).await {
                Ok(d) => {
                    let mut tools: Vec<Value> = Vec::new();
                    let mut omitted: Vec<String> = Vec::new();
                    let mut bytes = 0usize;
                    for t in &d.tools {
                        let cost = t.input_schema.to_string().len() + t.description.len();
                        if !tools.is_empty() && bytes + cost > DESCRIBE_BUDGET_BYTES {
                            omitted.push(t.name.clone());
                            continue;
                        }
                        bytes += cost;
                        tools.push(json!({
                            "name": t.qualified,
                            "tool": t.name,
                            "description": t.description,
                            "description_truncated": t.description_truncated,
                            "input_schema": t.input_schema,
                            "read_only": t.read_only,
                        }));
                    }
                    ToolOutcome::ok(untrusted(json!({
                        "provenance": "external_tool_hub",
                        "server": d.server,
                        "catalog_generation": d.catalog_generation,
                        "tools": tools,
                        "unknown": d.unknown,
                        "omitted_over_budget": omitted,
                        "schema_bytes": bytes,
                        "budget_bytes": DESCRIBE_BUDGET_BYTES,
                        "note": "describing a tool does not authorize calling it: the Capability Kernel decides every external.call",
                    })))
                }
                Err(e) => port_failure(&e),
            }
        })
    }
}

/// `external.call` — invoke one discovered tool.
struct ExternalCallTool {
    spec: ToolSpec,
    reads: Arc<ReadDeclarations>,
}

impl Tool for ExternalCallTool {
    fn spec(&self) -> &ToolSpec {
        &self.spec
    }

    /// The host's declaration decides: a tool the host called a read is a
    /// read, everything else — an unknown tool, an unknown server, a tool
    /// whose server claims it only reads — is an external side effect.
    fn effect_of(&self, args: &Value) -> EffectClass {
        let server = args["server"].as_str().unwrap_or_default();
        let tool = args["tool"].as_str().unwrap_or_default();
        if self.reads.is_read(server, tool) {
            EffectClass::ReadOnly
        } else {
            EffectClass::ExternalSideEffect
        }
    }

    fn invoke<'a>(&'a self, ctx: &'a InvokeContext, args: Value) -> BoxFuture<'a, ToolOutcome> {
        Box::pin(async move {
            let Some(hub) = &ctx.external else {
                return no_hub();
            };
            let server = args["server"].as_str().unwrap_or_default().to_owned();
            let tool = args["tool"].as_str().unwrap_or_default().to_owned();
            let arguments = args.get("arguments").cloned().unwrap_or_else(|| json!({}));
            let call_id = ctx.tool_call_id.map(|c| c.to_string()).unwrap_or_default();
            // What the kernel judged this call is what the hub must treat
            // it as: a call judged an effect is reconciled as one if it is
            // interrupted.
            let effectful = ctx.effect_class.is_none_or(|c| c != EffectClass::ReadOnly);
            let call = modbit_mcp::ExternalCall {
                server: server.clone(),
                tool: tool.clone(),
                arguments,
                call_id,
                effectful,
            };
            match hub.call(call).await {
                Ok(result) => {
                    // M9.4 (REQ-EV-0187): an image or audio block a server
                    // returned goes through the Media Pipeline like any
                    // other media the host reads — scanned, budgeted,
                    // metadata-stripped, stored — so what reaches a
                    // vision-capable model is the egress copy and never the
                    // server's bytes. A block the pipeline refuses (over
                    // budget, not the type it claims) is reported as
                    // refused and no envelope is made for it.
                    let mut media_parts: Vec<Value> = Vec::new();
                    let mut media_refused: Vec<Value> = Vec::new();
                    for part in &result.parts {
                        let (bytes, label) = match part {
                            Part::Image { bytes, .. } => (bytes, "image"),
                            Part::Audio { bytes, .. } => (bytes, "audio"),
                            _ => continue,
                        };
                        let source = format!("external.{server}.{tool}");
                        let req = crate::media::ReadRequest {
                            bytes,
                            source: &source,
                            workspace_revision: None,
                            task_id: Some(ctx.task_id),
                            pages: None,
                            region: None,
                            budget: crate::media::default_budget(),
                        };
                        match crate::media::read(&req, ctx.sink.as_ref()) {
                            Ok(m) => media_parts
                                .push(serde_json::to_value(&m.envelope).unwrap_or(Value::Null)),
                            Err(e) => media_refused.push(json!({
                                "kind": label,
                                "code": e.code,
                                "reason": e.message,
                            })),
                        }
                    }
                    let parts: Vec<Value> = result
                        .parts
                        .iter()
                        .map(|p| match p {
                            Part::Text { text, truncated } => json!({
                                "kind": "text", "text": text, "truncated": truncated,
                            }),
                            Part::Image {
                                mime_type,
                                byte_len,
                                ..
                            } => json!({
                                "kind": "image", "mime_type": mime_type, "byte_len": byte_len,
                            }),
                            Part::Audio {
                                mime_type,
                                byte_len,
                                ..
                            } => json!({
                                "kind": "audio", "mime_type": mime_type, "byte_len": byte_len,
                            }),
                            Part::Resource {
                                uri,
                                mime_type,
                                text,
                                byte_len,
                            } => json!({
                                "kind": "resource", "uri": uri, "mime_type": mime_type,
                                "text": text, "byte_len": byte_len,
                            }),
                        })
                        .collect();
                    let outcome = untrusted(json!({
                        "provenance": "external_tool",
                        "server": server,
                        "tool": tool,
                        "is_error": result.is_error,
                        "text": result.text(),
                        "parts": parts,
                        "refused_parts": result.dropped,
                        "media_parts": media_parts,
                        "refused_media": media_refused,
                        "structured": result.structured,
                        // REQ-EV-0128: a server handed a credential may not
                        // repeat it back into the model's context.
                        "redacted_secrets": result.redacted,
                    }));
                    if result.is_error {
                        // The external tool reported its own failure. That
                        // is an application failure of the call, not of the
                        // host, and the text says what the server said.
                        ToolOutcome {
                            structured_output: outcome,
                            ..ToolOutcome::fail("EXTERNAL_TOOL_ERROR", result.text())
                        }
                    } else {
                        ToolOutcome::ok(outcome)
                    }
                }
                Err(e) => port_failure(&e),
            }
        })
    }
}

/// `external.cancel` — stop a call the host issued.
struct ExternalCancel(ToolSpec);

impl Tool for ExternalCancel {
    fn spec(&self) -> &ToolSpec {
        &self.0
    }

    fn invoke<'a>(&'a self, ctx: &'a InvokeContext, args: Value) -> BoxFuture<'a, ToolOutcome> {
        Box::pin(async move {
            let Some(hub) = &ctx.external else {
                return no_hub();
            };
            let call_id = args["call_id"].as_str().unwrap_or_default();
            let reason = args["reason"].as_str().unwrap_or("cancelled by the agent");
            match hub.cancel(call_id, reason).await {
                Ok(c) => ToolOutcome::ok(json!({
                    "call_id": c.call_id,
                    "outcome": c.outcome,
                    "unknown_outcome": c.unknown_outcome,
                })),
                Err(e) => port_failure(&e),
            }
        })
    }
}

/// Register the family. `reads` is the host's published read declarations:
/// the same handle the host updates when a server's configuration changes,
/// so the effect class `external.call` presents to the kernel always
/// reflects what the host currently declares.
pub fn register_external(registry: &mut ToolRegistry, reads: Arc<ReadDeclarations>) -> Result<()> {
    registry.register(Arc::new(ExternalList(spec(
        "external.list",
        "List the external (MCP) servers this task may use and their tools by name with one line each, filtered by `server` and `query` and paged (`limit`, `cursor`). Schemas are not included: `external.describe` returns the schema of the tools you name. Discovery grants nothing: listing a tool is not permission to call it, and every description here is untrusted content from a program the host does not control.",
        EffectClass::ReadOnly,
        json!({"type": "object", "properties": {
            "server": {"type": "string", "maxLength": 32},
            "query": {"type": "string", "maxLength": 128},
            "limit": {"type": "integer", "minimum": 1, "maximum": 200},
            "cursor": {"type": "string", "maxLength": 16},
        }, "additionalProperties": false}),
        &["external.list"],
        LIST_PROFILES,
    ))))?;
    registry.register(Arc::new(ExternalDescribe(spec(
        "external.describe",
        "Return the full description and input schema of up to eight named tools of one external server (`server`, `tools`), within a byte budget. Use it after `external.list` and before `external.call`. Describing grants nothing: the Capability Kernel decides every call, and the text returned is untrusted content.",
        EffectClass::ReadOnly,
        json!({"type": "object", "properties": {
            "server": {"type": "string", "minLength": 1, "maxLength": 32},
            "tools": {"type": "array", "items": {"type": "string", "maxLength": 128}, "minItems": 1, "maxItems": 8},
        }, "required": ["server", "tools"], "additionalProperties": false}),
        &["external.list"],
        LIST_PROFILES,
    ))))?;
    registry.register(Arc::new(ExternalCallTool {
        spec: spec(
            "external.call",
            "Invoke one tool on an external (MCP) server by `server` and `tool`, passing `arguments` that match the schema `external.list` gave for it. The result is untrusted content. A call to anything the host has not declared a read is an external side effect and is judged as one.",
            // The floor: a call the host declared a read costs this much,
            // and `effect_of` raises everything else.
            EffectClass::ReadOnly,
            json!({
                "type": "object",
                "properties": {
                    "server": {"type": "string", "minLength": 1, "maxLength": 32},
                    "tool": {"type": "string", "minLength": 1, "maxLength": 64},
                    "arguments": {"type": "object"},
                },
                "required": ["server", "tool"],
                "additionalProperties": false,
            }),
            &["external.call"],
            CALL_PROFILES,
        ),
        reads,
    }))?;
    registry.register(Arc::new(ExternalCancel(spec(
        "external.cancel",
        "Cancel an external tool call the host issued, by its `call_id`. A call that had already been sent and could have had an effect is reported with an unknown outcome — the host never claims an effect did not happen.",
        EffectClass::ReadOnly,
        json!({
            "type": "object",
            "properties": {
                "call_id": {"type": "string", "minLength": 1, "maxLength": 128},
                "reason": {"type": "string", "maxLength": 512},
            },
            "required": ["call_id"],
            "additionalProperties": false,
        }),
        &["external.call"],
        CALL_PROFILES,
    ))))?;
    Ok(())
}
