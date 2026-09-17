//! Bearer tokens for the Cloud API (docs/24 "Identity": short-lived access
//! token + rotating refresh token). An access token is a compact signed
//! claim set — tenant, principal, kind, expiry — under an HMAC-SHA256 key
//! only the API holds; it is verified without a database round trip, and
//! nothing in it is secret. The refresh token is opaque, stored hashed by
//! the store, and rotated on every use (a replay revokes the family).

use base64::Engine;
use hmac::{Hmac, KeyInit, Mac};
use modbit_domain::TenantId;
use modbit_event_store::cloud::Principal;
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

/// What an access token asserts.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Claims {
    /// Tenant.
    pub tenant_id: String,
    /// Principal.
    pub principal_id: String,
    /// `user` | `worker` | `service`.
    pub kind: String,
    /// Label.
    pub label: String,
    /// Expiry, ms since the epoch.
    pub exp_ms: i64,
}

impl Claims {
    /// The principal these claims name.
    #[must_use]
    pub fn principal(&self) -> Option<Principal> {
        Some(Principal {
            principal_id: uuid::Uuid::parse_str(&self.principal_id).ok()?,
            tenant_id: TenantId::parse(&self.tenant_id).ok()?,
            kind: self.kind.clone(),
            label: self.label.clone(),
        })
    }
}

/// The signing key.
#[derive(Clone)]
pub struct TokenKey(Vec<u8>);

impl TokenKey {
    /// From raw key bytes (32 or more).
    #[must_use]
    pub fn new(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }

    /// A fresh random key (tokens do not survive a restart; fine for a
    /// single development instance, wrong for a fleet).
    #[must_use]
    pub fn random() -> Self {
        let b: [u8; 32] = rand::random();
        Self(b.to_vec())
    }

    fn mac(&self) -> HmacSha256 {
        HmacSha256::new_from_slice(&self.0).expect("hmac accepts any key length")
    }

    /// Issue an access token for `p` valid for `ttl_ms`.
    #[must_use]
    pub fn issue(&self, p: &Principal, ttl_ms: i64) -> String {
        let claims = Claims {
            tenant_id: p.tenant_id.to_string(),
            principal_id: p.principal_id.to_string(),
            kind: p.kind.clone(),
            label: p.label.clone(),
            exp_ms: modbit_domain::Timestamp::now().millis() + ttl_ms,
        };
        let body = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(serde_json::to_vec(&claims).expect("claims json"));
        let mut mac = self.mac();
        mac.update(body.as_bytes());
        let sig =
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes());
        format!("mba_{body}.{sig}")
    }

    /// Verify a token: signature and expiry; the claims when valid.
    #[must_use]
    pub fn verify(&self, token: &str) -> Option<Claims> {
        let rest = token.strip_prefix("mba_")?;
        let (body, sig) = rest.split_once('.')?;
        let sig_bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(sig)
            .ok()?;
        let mut mac = self.mac();
        mac.update(body.as_bytes());
        mac.verify_slice(&sig_bytes).ok()?;
        let claims: Claims = serde_json::from_slice(
            &base64::engine::general_purpose::URL_SAFE_NO_PAD
                .decode(body)
                .ok()?,
        )
        .ok()?;
        if claims.exp_ms < modbit_domain::Timestamp::now().millis() {
            return None;
        }
        Some(claims)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_token_verifies_under_its_key_only_and_expires() {
        let key = TokenKey::random();
        let p = Principal {
            principal_id: uuid::Uuid::now_v7(),
            tenant_id: TenantId::new(),
            kind: "user".into(),
            label: "ada".into(),
        };
        let t = key.issue(&p, 60_000);
        let c = key.verify(&t).unwrap();
        assert_eq!(c.principal().unwrap(), p);
        assert!(TokenKey::random().verify(&t).is_none(), "another key");
        let expired = key.issue(&p, -1);
        assert!(key.verify(&expired).is_none());
        let mut forged = t.clone();
        forged.replace_range(4..5, if &t[4..5] == "a" { "b" } else { "a" });
        assert!(key.verify(&forged).is_none());
    }
}
