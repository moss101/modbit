//! The composer's model catalog read (REQ-PX-056; docs/65 AFW-D13, AFW-D14).
//!
//! `ListModelVariants` answers one question for the model picker: which models
//! this Core can dispatch, which (effort, tier) variants each offers, and
//! whether a manual pin of it would be allowed. It is a view over the provider
//! gateway's catalog and the policy checks `SetExecutionPreference` already
//! makes; it records nothing, routes nothing and adds no registry of its own.

use std::sync::Arc;

use modbit_domain::TaskId;
use modbit_protocol::v1 as wire;
use prost::Message;

use crate::server::{Core, accept, reject};

/// The efforts a reasoning model can be asked for, cheapest first
/// (`modbit_domain::mode::valid_effort`).
const EFFORTS: [&str; 3] = ["low", "medium", "high"];

fn effort_rank(effort: &str) -> usize {
    EFFORTS.iter().position(|e| *e == effort).unwrap_or(1)
}

/// The variants a catalog entry offers: one per effort for a model that
/// exposes reasoning, else the single variant the entry has. The tier is the
/// entry's own: the catalog names no others.
fn variants_of(reasoning: bool, default_effort: &str, tier: &str) -> Vec<wire::ModelVariantView> {
    if !reasoning {
        return vec![wire::ModelVariantView {
            effort: String::new(),
            service_tier: tier.to_owned(),
            label: "Standard".to_owned(),
            is_default: true,
            raises_cost: false,
        }];
    }
    // With no default of its own the provider's default is taken as medium.
    let default = if default_effort.is_empty() {
        "medium"
    } else {
        default_effort
    };
    EFFORTS
        .iter()
        .map(|e| wire::ModelVariantView {
            effort: (*e).to_owned(),
            service_tier: tier.to_owned(),
            label: format!("{} effort", capitalize(e)),
            is_default: *e == default,
            raises_cost: effort_rank(e) > effort_rank(default),
        })
        .collect()
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    c.next()
        .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
        .unwrap_or_default()
}

pub(crate) async fn list_variants(
    core: &Arc<Core>,
    cid: Option<wire::Id>,
    payload: &[u8],
) -> wire::CommandAck {
    let Ok(p) = wire::ListModelVariants::decode(payload) else {
        return reject(cid, "BAD_PAYLOAD", "ListModelVariants");
    };
    // The task's configured allow list applies on top of the organisation
    // policy, exactly as it does to a pin.
    let allow: Option<Vec<String>> = match p.task_id.as_ref() {
        None => None,
        Some(id) => {
            let Ok(bytes) = <[u8; 16]>::try_from(id.value.as_slice()) else {
                return reject(cid, "BAD_PAYLOAD", "task_id must be 16 bytes");
            };
            let task_id = TaskId::from_bytes(bytes);
            let task = match core.store.lock().await.task(&task_id) {
                Ok(Some(t)) => t,
                Ok(None) => return reject(cid, "UNKNOWN_TASK", task_id.to_string()),
                Err(e) => return reject(cid, crate::server::error_code(&e), e.to_string()),
            };
            crate::routing::model_policy(core, &task)
        }
    };
    let gw = &core.gateway;
    let mut models = Vec::new();
    for ep in gw.endpoints() {
        let credential_available = matches!(ep.credential, modbit_providers::SecretHandle::None)
            || gw.credential_configured(&ep.name);
        for m in &ep.models {
            let org_block = gw
                .policy()
                .blocking_rule(&ep.name, ep.kind, &m.model)
                .unwrap_or_default();
            let list_block = crate::routing::refused_by_model_policy(
                allow.as_deref(),
                [(ep.name.clone(), m.model.clone())],
            )
            .map(|(_, why)| why)
            .unwrap_or_default();
            let pin_refusal_code = if !org_block.is_empty() {
                "POLICY_BLOCKED"
            } else if !list_block.is_empty() {
                "MODEL_NOT_ALLOWED"
            } else if !credential_available {
                "NO_CREDENTIAL"
            } else {
                ""
            };
            let blocked_by_policy = if org_block.is_empty() {
                list_block
            } else {
                org_block
            };
            let (effort, tier) = m.execution_preference();
            let (effort, tier) = (effort.unwrap_or_default(), tier.unwrap_or_default());
            models.push(wire::ModelVariantsEntry {
                endpoint: ep.name.clone(),
                model: m.model.clone(),
                provider: format!("{:?}", ep.kind).to_lowercase(),
                reasoning: m.reasoning,
                vision: m.vision,
                context_tokens: m.context_tokens,
                variants: variants_of(m.reasoning, &effort, &tier),
                default_effort: effort,
                default_service_tier: tier,
                credential_available,
                blocked_by_policy,
                pin_allowed: pin_refusal_code.is_empty(),
                pin_refusal_code: pin_refusal_code.to_owned(),
            });
        }
    }
    accept(
        cid,
        false,
        wire::ModelVariantList {
            models,
            objectives: ["COST", "BALANCE", "INTELLIGENCE"]
                .map(str::to_owned)
                .to_vec(),
            default_objective: "BALANCE".to_owned(),
        }
        .encode_to_vec(),
    )
}
