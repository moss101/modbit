//! Provider conformance probe (docs/15, REQ-EV-0028): one real streaming
//! call through the gateway, reported with the requested-vs-resolved route
//! record (REQ-EV-0112). Nothing here touches the event log; the agent
//! runtime (M2.7) is the production caller of the gateway.

use modbit_protocol::v1 as wire;
use modbit_providers::{
    Message, ModelEvent, ModelPolicy, ModelRequest, ProviderGateway, Requirements, Role,
    RouteError, ToolProjection,
};
use tokio_util::sync::CancellationToken;

/// Run the probe.
pub async fn probe(gw: &ProviderGateway, p: &wire::ProbeModel) -> wire::ModelProbed {
    let req = ModelRequest {
        request_id: format!("probe-{}", modbit_domain::EventId::new()),
        model_policy: ModelPolicy {
            endpoint: p.endpoint.clone(),
            model: p.model.clone(),
            reasoning_effort: None,
            service_tier: None,
        },
        messages: vec![
            Message::text(
                Role::System,
                "You are a conformance probe. Follow the user's instruction exactly.",
            ),
            Message::text(Role::User, p.prompt.clone()),
        ],
        tool_projection: if p.with_tools {
            vec![ToolProjection {
                name: "probe.echo".into(),
                description: "Echo the given text back to the harness.".into(),
                input_schema: serde_json::json!({"type":"object","properties":{"text":{"type":"string"}},"required":["text"]}),
            }]
        } else {
            vec![]
        },
        response_format: None,
        cache_key: None,
        max_output_tokens: 256,
        timeout_ms: if p.timeout_ms == 0 {
            30_000
        } else {
            p.timeout_ms
        },
        policy_tags: vec!["probe".into()],
    };
    let needs = Requirements {
        tools: p.with_tools,
        ..Default::default()
    };
    let mut out = wire::ModelProbed::default();
    let mut stream = match gw.stream(req, &needs, CancellationToken::new()) {
        Ok(s) => s,
        Err(e) => {
            out.status = "ROUTE_REFUSED".into();
            out.error_code = match &e {
                RouteError::UnknownEndpoint(_) => "UNKNOWN_ENDPOINT",
                RouteError::UnknownModel { .. } => "UNKNOWN_MODEL",
                RouteError::CapabilityMismatch { .. } => "CAPABILITY_MISMATCH",
                RouteError::MissingCredential(_) => "MISSING_CREDENTIAL",
                RouteError::PolicyBlocked { .. } => "POLICY_BLOCKED",
            }
            .into();
            out.error_message = e.to_string();
            return out;
        }
    };
    let mut text = String::new();
    while let Some(ev) = stream.events.recv().await {
        out.events += 1;
        match ev {
            ModelEvent::MessageDelta { text: t } => text.push_str(&t),
            ModelEvent::ToolCallComplete {
                name,
                arguments_json,
                ..
            } => {
                out.tool_call_name = name;
                out.tool_call_arguments_json = arguments_json;
            }
            ModelEvent::Usage { usage } => {
                out.input_tokens = usage.input_tokens;
                out.output_tokens = usage.output_tokens;
                out.cached_input_tokens = usage.cached_input_tokens;
            }
            ModelEvent::Completed { stop_reason } => {
                out.status = "COMPLETED".into();
                out.stop_reason = stop_reason;
            }
            ModelEvent::Error { code, message, .. } => {
                out.status = "ERROR".into();
                out.error_code = code;
                out.error_message = message;
            }
            _ => {}
        }
    }
    out.text = text;
    out.route_json =
        serde_json::to_string(&*stream.route.lock().expect("route")).unwrap_or_default();
    out
}
