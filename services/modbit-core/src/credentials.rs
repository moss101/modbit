//! The credential broker as a client sees it (REQ-PX-130): the credentials
//! it holds, the grants, the counters and the audit — never a value — and
//! revocation. Refusals, rotations and revocations are also recorded as
//! security events on the task they concern.

use modbit_protocol::v1 as wire;
use prost::Message;

use crate::server::{Core, accept, reject};

/// `GetCredentialBroker`.
pub(crate) fn view(core: &Core, env: wire::CommandEnvelope) -> wire::CommandAck {
    let cid = env.command_id.clone();
    let Ok(p) = wire::GetCredentialBroker::decode(env.payload.as_slice()) else {
        return reject(cid, "BAD_PAYLOAD", "GetCredentialBroker");
    };
    let b = core.tools.broker();
    let limit = match p.audit_limit {
        0 => 50,
        n => n.min(500) as usize,
    };
    let stats = b.stats();
    accept(
        cid,
        false,
        wire::CredentialBrokerView {
            credentials: b
                .credentials()
                .into_iter()
                .map(|c| wire::CredentialView {
                    id: c.id.to_string(),
                    kind: c.kind.name().to_owned(),
                    audience: c.audience,
                    generation: c.generation,
                    revoked: c.revoked,
                    configured: c.configured,
                    delegated: c.delegated,
                    uses: c.uses,
                    refusals: c.refusals,
                    last_used_ms: c.last_used_ms,
                    registered_at_ms: c.registered_at_ms,
                })
                .collect(),
            grants: b
                .grants()
                .into_iter()
                .map(|g| wire::GrantView {
                    id: g.handle.id,
                    credential: g.handle.credential.to_string(),
                    principal: g.handle.principal,
                    audience: g.handle.audience,
                    purpose: g.handle.purpose,
                    expires_at_ms: g.handle.expires_at_ms,
                    max_uses: g.handle.max_uses.unwrap_or(0),
                    uses: g.uses,
                    revoked: g.revoked,
                    expired: g.expired,
                })
                .collect(),
            audit: b
                .audit_tail(limit)
                .into_iter()
                .map(|a| wire::CredentialAuditView {
                    at_ms: a.at_ms,
                    action: a.action.to_owned(),
                    credential: a.credential,
                    grant: a.grant,
                    principal: a.principal,
                    audience: a.audience,
                    purpose: a.purpose,
                    outcome: a.outcome,
                })
                .collect(),
            issued: stats.issued,
            used: stats.used,
            refused: stats.refused.into_iter().collect(),
        }
        .encode_to_vec(),
    )
}

/// `RevokeCredential`: by credential, by grant or by principal.
pub(crate) fn revoke(core: &Core, env: wire::CommandEnvelope) -> wire::CommandAck {
    let cid = env.command_id.clone();
    let Ok(p) = wire::RevokeCredential::decode(env.payload.as_slice()) else {
        return reject(cid, "BAD_PAYLOAD", "RevokeCredential");
    };
    let given = [&p.credential_id, &p.grant_id, &p.principal]
        .iter()
        .filter(|s| !s.is_empty())
        .count();
    if given != 1 {
        return reject(
            cid,
            "BAD_PAYLOAD",
            "name exactly one of credential_id, grant_id, principal",
        );
    }
    let b = core.tools.broker();
    let n = if !p.credential_id.is_empty() {
        let found = b
            .credentials()
            .into_iter()
            .find(|c| c.id.as_str() == p.credential_id);
        match found {
            Some(c) => u32::from(b.revoke_credential(&c.id, &p.reason)),
            None => return reject(cid, "UNKNOWN_CREDENTIAL", p.credential_id),
        }
    } else if !p.grant_id.is_empty() {
        if !b.revoke_grant(&p.grant_id) {
            return reject(cid, "UNKNOWN_GRANT", p.grant_id);
        }
        1
    } else {
        u32::try_from(b.revoke_principal(&p.principal)).unwrap_or(u32::MAX)
    };
    accept(
        cid,
        false,
        wire::CredentialRevoked { revoked: n }.encode_to_vec(),
    )
}
