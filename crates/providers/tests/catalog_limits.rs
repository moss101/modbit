//! What a catalog entry means for a dispatch (audit G; FIX-14): the output
//! budget and timeout come from the entry rather than from one constant for
//! every model, effort and tier are the entry's defaults and reach the wire,
//! a request for structured output is refused where the adapter has none
//! instead of degrading to text, and Anthropic thinking is held under the
//! request's own `max_tokens`.

use modbit_providers::anthropic;
use modbit_providers::gateway::{
    BASE_REQUEST_TIMEOUT_MS, DEFAULT_OUTPUT_BUDGET_TOKENS, TIMEOUT_MS_PER_EXTRA_OUTPUT_TOKEN,
};
use modbit_providers::{
    Endpoint, Message, ModelCapability, ModelPolicy, ModelRequest, ProviderGateway, ProviderKind,
    Requirements, Role, RouteError, SecretHandle, default_anthropic_models, default_openai_models,
    endpoints_from, openai, parse_models_spec,
};
use serde_json::json;

fn entry(spec: &str) -> ModelCapability {
    parse_models_spec(spec).unwrap().remove(0)
}

#[test]
fn the_output_budget_and_timeout_come_from_the_entry_not_a_constant() {
    // No keys: the default budget, a timeout that grows with it.
    let big = entry("m=1/2;out=64000");
    assert_eq!(big.output_budget(), DEFAULT_OUTPUT_BUDGET_TOKENS);
    assert_eq!(
        big.timeout_ms(),
        BASE_REQUEST_TIMEOUT_MS
            + u64::from(DEFAULT_OUTPUT_BUDGET_TOKENS - 4096) * TIMEOUT_MS_PER_EXTRA_OUTPUT_TOKEN
    );
    assert!(
        big.output_budget() > 4096 && big.timeout_ms() > 120_000,
        "the old flat 4096 / 120 s are gone for a model that can write more"
    );
    // A small model is not asked for more than it can give.
    let small = entry("m=1/2;out=2048");
    assert_eq!(small.output_budget(), 2048);
    assert_eq!(small.timeout_ms(), BASE_REQUEST_TIMEOUT_MS);
    // Explicit values win, the budget never above the ceiling.
    let tuned = entry("m=1/2;out=32768;budget=24000;timeout=900000");
    assert_eq!(
        (tuned.output_budget(), tuned.timeout_ms()),
        (24_000, 900_000)
    );
    let capped = entry("m=1/2;out=8192;budget=50000");
    assert_eq!(capped.output_budget(), 8192);
    // The built-in catalogs derive from the same rule.
    for m in default_openai_models()
        .into_iter()
        .chain(default_anthropic_models())
    {
        assert!(m.output_budget() >= 4096.min(m.max_output_tokens), "{m:?}");
        assert!(m.output_budget() <= m.max_output_tokens, "{m:?}");
        assert!(m.timeout_ms() >= BASE_REQUEST_TIMEOUT_MS, "{m:?}");
    }
}

#[test]
fn effort_and_tier_are_the_entrys_defaults_and_only_for_a_model_that_reasons() {
    let m = entry("m=1/2;reasoning=true;effort=high;tier=priority");
    assert_eq!(
        m.execution_preference(),
        (Some("high".into()), Some("priority".into()))
    );
    // The tier needs no reasoning; an effort without it is a configuration
    // error, not something silently dropped.
    assert_eq!(
        entry("m=1/2;tier=flex").execution_preference(),
        (None, Some("flex".into()))
    );
    let err = parse_models_spec("m=1/2;effort=high").unwrap_err();
    assert!(err.contains("effort needs reasoning=true"), "{err}");
    assert!(parse_models_spec("m=1/2;reasoning=true;effort=extreme").is_err());
    assert!(parse_models_spec("m=1/2;tier=a b").is_err());
    // An entry that gained an effort some other way (a catalog from disk)
    // still never sends it to a model with no reasoning.
    let mut forced = entry("m=1/2");
    forced.default_reasoning_effort = Some("high".into());
    assert_eq!(forced.execution_preference().0, None);
    // Nothing configured, nothing sent.
    assert_eq!(entry("m=1/2").execution_preference(), (None, None));
}

fn chat(endpoint: &str, model: &str, response_format: Option<&str>) -> ModelRequest {
    ModelRequest {
        request_id: "r".into(),
        model_policy: ModelPolicy {
            endpoint: endpoint.into(),
            model: model.into(),
            reasoning_effort: None,
            service_tier: None,
        },
        messages: vec![Message::text(Role::User, "give me json")],
        tool_projection: vec![],
        response_format: response_format.map(str::to_owned),
        cache_key: None,
        cache_breakpoints: vec![],
        max_output_tokens: 64,
        timeout_ms: 1000,
        policy_tags: vec![],
    }
}

fn gateway(kind: ProviderKind, models: Vec<ModelCapability>) -> ProviderGateway {
    ProviderGateway::new(vec![Endpoint {
        name: "ep".into(),
        kind,
        base_url: "http://127.0.0.1:1".into(),
        credential: SecretHandle::None,
        models,
        max_retries: 0,
        auth: Default::default(),
        extra_body: Default::default(),
        max_concurrency: 0,
    }])
}

/// The catalog claimed structured output for every model, Anthropic's
/// included, whose adapter ignores `response_format`: the route check passed
/// and the request silently came back as text. Now the catalog does not claim
/// it and, whatever an entry says, the route refuses it for that wire.
#[test]
fn structured_output_is_claimed_and_accepted_only_where_the_adapter_implements_it() {
    assert!(
        default_anthropic_models()
            .iter()
            .all(|m| !m.structured_output),
        "no Anthropic entry claims a JSON mode the adapter lacks"
    );
    assert!(
        default_openai_models().iter().all(|m| m.structured_output),
        "the OpenAI adapter implements json_object"
    );
    assert!(ProviderKind::OpenAi.implements_structured_output());
    assert!(!ProviderKind::Anthropic.implements_structured_output());

    // OpenAI wire: a JSON request is routed, and the body asks for it.
    let openai_gw = gateway(ProviderKind::OpenAi, default_openai_models());
    let json_request = chat("ep", "gpt-5-mini", Some("json_object"));
    assert!(
        openai_gw
            .route(&json_request, &Requirements::default())
            .is_ok()
    );
    assert_eq!(
        openai::request_body(&json_request)["response_format"],
        json!({"type": "json_object"})
    );

    // Anthropic wire, honest catalog: refused with a typed mismatch.
    let anthropic_gw = gateway(ProviderKind::Anthropic, default_anthropic_models());
    let json_request = chat("ep", "claude-sonnet-5", Some("json_object"));
    let refused = anthropic_gw
        .route(&json_request, &Requirements::default())
        .unwrap_err();
    assert_eq!(
        refused,
        RouteError::CapabilityMismatch {
            model: "claude-sonnet-5".into(),
            capability: "structured_output".into()
        }
    );
    // Asked for by the caller's requirements rather than the request: same.
    let needs = Requirements {
        structured_output: true,
        ..Requirements::default()
    };
    assert!(
        anthropic_gw
            .route(&chat("ep", "claude-sonnet-5", None), &needs)
            .is_err()
    );
    // A plain request on the same endpoint is untouched.
    assert!(
        anthropic_gw
            .route(
                &chat("ep", "claude-sonnet-5", None),
                &Requirements::default()
            )
            .is_ok()
    );

    // Even a catalog entry that over-claims is refused for the wire, so a
    // configured or signed entry cannot make the claim true.
    let mut lying = default_anthropic_models();
    lying[1].structured_output = true;
    let lying_gw = gateway(ProviderKind::Anthropic, lying);
    assert!(
        lying_gw
            .route(
                &chat("ep", "claude-sonnet-5", Some("json_object")),
                &Requirements::default()
            )
            .is_err()
    );
}

/// A configured Anthropic catalog cannot claim structured output either:
/// `MODBIT_ANTHROPIC_MODELS` entries are registered without it.
#[test]
fn a_configured_anthropic_catalog_is_registered_without_structured_output() {
    let env = |name: &str| match name {
        "ANTHROPIC_API_KEY" => Some("k".to_owned()),
        "MODBIT_ANTHROPIC_MODELS" => Some("claude-x=1/2;out=64000".to_owned()),
        "MODBIT_ANTHROPIC_MAX_CONCURRENCY" => Some("3".to_owned()),
        _ => None,
    };
    let endpoints = endpoints_from(env);
    let ep = endpoints.iter().find(|e| e.name == "anthropic").unwrap();
    assert!(ep.models.iter().all(|m| !m.structured_output));
    assert_eq!(ep.max_concurrency, 3);
    // A malformed concurrency registers nothing rather than guessing.
    let bad = |name: &str| match name {
        "OPENAI_API_KEY" => Some("k".to_owned()),
        "MODBIT_OPENAI_MAX_CONCURRENCY" => Some("0".to_owned()),
        _ => None,
    };
    assert!(endpoints_from(bad).is_empty());
}

fn thinking_request(effort: &str, max_output_tokens: u32) -> ModelRequest {
    let mut r = chat("ep", "claude-sonnet-5", None);
    r.model_policy.reasoning_effort = Some(effort.into());
    r.max_output_tokens = max_output_tokens;
    r
}

/// The API refuses a thinking budget that is not smaller than `max_tokens`
/// (400): the loop's old 4096 with effort `high` (budget 16384) was exactly
/// that. The budget is now held to half of `max_tokens`, and a request with
/// no room for the minimum goes without a thinking block.
#[test]
fn the_thinking_budget_stays_under_max_tokens_and_leaves_room_for_the_answer() {
    let b = |effort: &str, max: u32| anthropic::request_body(&thinking_request(effort, max));
    // Room to spare: the effort's own budget.
    assert_eq!(
        b("high", 64_000)["thinking"],
        json!({"type": "enabled", "budget_tokens": 16_384})
    );
    assert_eq!(
        b("low", 8192)["thinking"],
        json!({"type": "enabled", "budget_tokens": 1024})
    );
    // The old loop's cap: held to half, never the 400-producing 16384.
    let tight = b("high", 4096);
    assert_eq!(
        tight["thinking"],
        json!({"type": "enabled", "budget_tokens": 2048})
    );
    assert!(
        tight["thinking"]["budget_tokens"].as_u64().unwrap()
            < tight["max_tokens"].as_u64().unwrap()
    );
    // Too small a cap for the 1024 minimum: no thinking block at all.
    assert!(b("medium", 1500).get("thinking").is_none());
    // No effort: nothing.
    assert!(
        anthropic::request_body(&chat("ep", "claude-sonnet-5", None))
            .get("thinking")
            .is_none()
    );
}

/// Effort and tier reach the OpenAI body exactly as the policy carries them.
#[test]
fn effort_and_tier_reach_the_wire() {
    let mut r = chat("ep", "gpt-5-mini", None);
    r.model_policy.reasoning_effort = Some("low".into());
    r.model_policy.service_tier = Some("flex".into());
    let body = openai::request_body(&r);
    assert_eq!(body["reasoning_effort"], "low");
    assert_eq!(body["service_tier"], "flex");
    let body = anthropic::request_body(&r);
    assert_eq!(body["service_tier"], "flex");
}
