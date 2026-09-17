//! Authentication at the boundary: the ephemeral guest credential (an HMAC
//! key the gateway mints per sandbox and hands the guest at admission; every
//! later frame carries an HMAC over its content, and a call id is a nonce
//! the guest never accepts twice) and the worker's bearer token to the
//! gateway (a signed claim set under the gateway's key: worker id, expiry;
//! the tenant and session a request acts for are checked against the cloud
//! store's lease, never taken from the token).

use hmac::{Hmac, KeyInit, Mac};
use prost::Message;
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

/// A fresh 32-byte credential.
#[must_use]
pub fn fresh_credential() -> Vec<u8> {
    let b: [u8; 32] = rand::random();
    b.to_vec()
}

fn mac(key: &[u8], parts: &[&[u8]]) -> Vec<u8> {
    let mut m = HmacSha256::new_from_slice(key).expect("any key length");
    for p in parts {
        m.update(p);
    }
    m.finalize().into_bytes().to_vec()
}

fn verify(key: &[u8], parts: &[&[u8]], tag: &[u8]) -> bool {
    let mut m = HmacSha256::new_from_slice(key).expect("any key length");
    for p in parts {
        m.update(p);
    }
    m.verify_slice(tag).is_ok()
}

/// Sign a call: the HMAC over the call with its `auth` cleared.
pub fn sign_call(credential: &[u8], call: &mut modbit_protocol::v1::GuestCall) {
    call.auth = Vec::new();
    let bytes = call.encode_to_vec();
    call.auth = mac(credential, &[&bytes]);
}

/// Verify a call's HMAC.
#[must_use]
pub fn verify_call(credential: &[u8], call: &modbit_protocol::v1::GuestCall) -> bool {
    let mut c = call.clone();
    let tag = std::mem::take(&mut c.auth);
    verify(credential, &[&c.encode_to_vec()], &tag)
}

/// Sign a reply.
pub fn sign_reply(credential: &[u8], reply: &mut modbit_protocol::v1::GuestReply) {
    reply.auth = Vec::new();
    let bytes = reply.encode_to_vec();
    reply.auth = mac(credential, &[&bytes]);
}

/// Verify a reply's HMAC.
#[must_use]
pub fn verify_reply(credential: &[u8], reply: &modbit_protocol::v1::GuestReply) -> bool {
    let mut r = reply.clone();
    let tag = std::mem::take(&mut r.auth);
    verify(credential, &[&r.encode_to_vec()], &tag)
}

/// What a worker's bearer token to the gateway asserts.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct WorkerClaims {
    /// The worker.
    pub worker_id: String,
    /// Expiry, ms since the epoch.
    pub exp_ms: i64,
}

/// The gateway's worker-token key.
#[derive(Clone)]
pub struct WorkerKey(Vec<u8>);

impl WorkerKey {
    /// From raw bytes.
    #[must_use]
    pub fn new(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }

    /// A random key (one process only).
    #[must_use]
    pub fn random() -> Self {
        Self(fresh_credential())
    }

    /// Issue a token: `mbw_<hex claims>.<hex hmac>`.
    #[must_use]
    pub fn issue(&self, claims: &WorkerClaims) -> String {
        let body = serde_json::to_vec(claims).expect("claims serialize");
        let tag = mac(&self.0, &[&body]);
        format!("mbw_{}.{}", hex::encode(&body), hex::encode(tag))
    }

    /// Verify a token as of `now_ms`.
    #[must_use]
    pub fn verify(&self, token: &str, now_ms: i64) -> Option<WorkerClaims> {
        let rest = token.strip_prefix("mbw_")?;
        let (body_hex, tag_hex) = rest.split_once('.')?;
        let body = hex::decode(body_hex).ok()?;
        let tag = hex::decode(tag_hex).ok()?;
        if !verify(&self.0, &[&body], &tag) {
            return None;
        }
        let claims: WorkerClaims = serde_json::from_slice(&body).ok()?;
        (claims.exp_ms > now_ms).then_some(claims)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use modbit_protocol::v1::{GuestCall, GuestHealth, guest_call};

    #[test]
    fn a_call_verifies_under_its_credential_only_and_a_change_breaks_it() {
        let k = fresh_credential();
        let mut c = GuestCall {
            call_id: "c1".into(),
            task_id: "t".into(),
            effect_id: String::new(),
            capability: "health".into(),
            auth: vec![],
            body: Some(guest_call::Body::Health(GuestHealth {})),
        };
        sign_call(&k, &mut c);
        assert!(verify_call(&k, &c));
        assert!(!verify_call(&fresh_credential(), &c));
        let mut tampered = c.clone();
        tampered.capability = "exec".into();
        assert!(!verify_call(&k, &tampered));
    }

    #[test]
    fn a_worker_token_verifies_until_it_expires() {
        let key = WorkerKey::random();
        let t = key.issue(&WorkerClaims {
            worker_id: "w1".into(),
            exp_ms: 1_000,
        });
        assert_eq!(key.verify(&t, 999).map(|c| c.worker_id), Some("w1".into()));
        assert!(key.verify(&t, 1_000).is_none(), "expired");
        assert!(WorkerKey::random().verify(&t, 0).is_none(), "another key");
        assert!(key.verify("mbw_zz.zz", 0).is_none());
    }
}
