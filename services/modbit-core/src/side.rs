//! Side questions (REQ-EV-0261): one bounded model call over a snapshot of
//! the task (goal, plan, recent transcript). Nothing is appended to the log
//! and no tool is projected, so the main task state and cursor are unchanged.

use std::sync::Arc;

use modbit_core_runtime::Budgets;
use modbit_domain::task::Task;
use modbit_protocol::v1 as wire;
use modbit_providers::{Message, ModelEvent, ModelPolicy, ModelRequest, Requirements, Role, Usage};
use tokio_util::sync::CancellationToken;

use crate::runtime::rebuild;
use crate::server::Core;

/// Recent transcript messages carried in the snapshot.
const SNAPSHOT_MESSAGES: usize = 12;

/// Answer `text` about `task`; returns the answer or a typed error code + message.
pub(crate) async fn ask(
    core: &Arc<Core>,
    task: &Task,
    text: &str,
    endpoint: &str,
    model: &str,
) -> Result<wire::SideAnswer, (String, String)> {
    let endpoints = core.gateway.endpoints();
    let endpoint = if endpoint.is_empty() {
        std::env::var("MODBIT_DEFAULT_ENDPOINT")
            .ok()
            .or_else(|| endpoints.first().map(|e| e.name.clone()))
            .ok_or_else(|| ("NO_PROVIDER".to_owned(), "no provider endpoint".to_owned()))?
    } else {
        endpoint.to_owned()
    };
    let model = if model.is_empty() {
        std::env::var("MODBIT_DEFAULT_MODEL").ok().or_else(|| {
            endpoints
                .iter()
                .find(|e| e.name == endpoint)
                .and_then(|e| e.models.first())
                .map(|m| m.model.clone())
        })
    } else {
        Some(model.to_owned())
    }
    .ok_or_else(|| {
        (
            "NO_MODEL".to_owned(),
            format!("endpoint `{endpoint}` serves no model"),
        )
    })?;
    let before = core
        .store
        .lock()
        .await
        .last_offset()
        .map_err(|e| ("STORE".to_owned(), e.to_string()))?;
    let (transcript, state, _, _) = rebuild(core, task, Budgets::default()).await;
    let recent: Vec<Message> = transcript
        .iter()
        .rev()
        .take(SNAPSHOT_MESSAGES)
        .rev()
        .cloned()
        .collect();
    let plan = state
        .plan
        .as_ref()
        .map(|p| serde_json::to_string(p).unwrap_or_default())
        .unwrap_or_else(|| "(no plan recorded)".into());
    let mut messages = vec![Message::text(
        Role::System,
        format!(
            "You are answering a side question about a task. Answer from the snapshot only; you cannot act, and nothing you say changes the task.\nTask goal: {}\nPlan: {plan}",
            task.goal_text
        ),
    )];
    messages.extend(recent.iter().cloned());
    messages.push(Message::text(Role::User, format!("[SIDE QUESTION] {text}")));
    let request = ModelRequest {
        request_id: format!("side-{}-{}", task.task_id, before),
        model_policy: ModelPolicy {
            endpoint,
            model,
            reasoning_effort: None,
            service_tier: None,
        },
        messages,
        tool_projection: vec![],
        response_format: None,
        cache_key: None,
        max_output_tokens: 2048,
        timeout_ms: 60_000,
        policy_tags: vec!["side_question".into()],
    };
    let stream = core
        .gateway
        .stream(request, &Requirements::default(), CancellationToken::new())
        .map_err(|e| ("ROUTE_REFUSED".to_owned(), e.to_string()))?;
    let mut events = stream.events;
    let mut answer = String::new();
    let mut usage = Usage::default();
    let mut error = None;
    while let Some(ev) = events.recv().await {
        match ev {
            ModelEvent::MessageDelta { text } => answer.push_str(&text),
            ModelEvent::Usage { usage: u } => usage = u,
            ModelEvent::Error { code, message, .. } => error = Some((code, message)),
            _ => {}
        }
    }
    if let Some(e) = error {
        return Err(e);
    }
    let route_json = stream
        .route
        .lock()
        .map(|r| serde_json::to_string(&*r).unwrap_or_default())
        .unwrap_or_default();
    let after = core
        .store
        .lock()
        .await
        .last_offset()
        .map_err(|e| ("STORE".to_owned(), e.to_string()))?;
    debug_assert_eq!(before, after, "a side question appends nothing");
    Ok(wire::SideAnswer {
        text: answer,
        route_json,
        input_tokens: usage.input_tokens,
        output_tokens: usage.output_tokens,
        last_offset: after,
        snapshot_messages: recent.len() as u32,
    })
}
