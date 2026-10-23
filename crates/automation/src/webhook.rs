//! Signed webhooks (AUT-B04): an HMAC-SHA256 over the exact body with a
//! timestamp and a nonce, so a forged request fails the signature, a stale
//! one fails the window, and a replayed one fails the nonce check the caller
//! keeps in its own store (the Cloud API keeps it in its database; an
//! in-memory cache is provided for tests and single-process callers).
//!
//! Header format: `t=<unix seconds>,n=<nonce>,v1=<hex hmac>`. The signed
//! text is `v1.<t>.<nonce>.` followed by the body bytes, so a body cannot be
//! moved under another timestamp or nonce.

use std::collections::BTreeMap;

use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;
use subtle::ConstantTimeEq;

/// The request header carrying the signature.
pub const SIGNATURE_HEADER: &str = "x-modbit-signature";
/// How far a timestamp may be from the receiver's clock, either way.
pub const DEFAULT_WINDOW_SECONDS: i64 = 300;
/// The longest nonce accepted.
pub const MAX_NONCE_BYTES: usize = 64;

/// Why a delivery was refused. The code is stable and goes in the audit row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// No header, or one that does not parse.
    Malformed,
    /// The nonce is empty, too long or not plain.
    BadNonce,
    /// The timestamp is older than the window.
    Stale,
    /// The timestamp is further in the future than the window.
    Future,
    /// The signature does not match the body under the secret.
    BadSignature,
    /// The nonce was already used inside the window.
    Replayed,
}

impl Refusal {
    /// The audit code.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::Malformed => "SIGNATURE_MALFORMED",
            Self::BadNonce => "SIGNATURE_BAD_NONCE",
            Self::Stale => "SIGNATURE_STALE",
            Self::Future => "SIGNATURE_FUTURE",
            Self::BadSignature => "SIGNATURE_INVALID",
            Self::Replayed => "WEBHOOK_REPLAYED",
        }
    }
}

/// A delivery that passed the signature and window checks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Verified {
    /// The signed timestamp (unix seconds).
    pub timestamp_s: i64,
    /// The signed nonce; the caller records it to refuse a replay.
    pub nonce: String,
}

fn mac(secret: &[u8], timestamp_s: i64, nonce: &str, body: &[u8]) -> Option<Vec<u8>> {
    let mut m = Hmac::<Sha256>::new_from_slice(secret).ok()?;
    m.update(format!("v1.{timestamp_s}.{nonce}.").as_bytes());
    m.update(body);
    Some(m.finalize().into_bytes().to_vec())
}

/// Build the header value for a body (a sender, and the tests).
#[must_use]
pub fn sign(secret: &[u8], timestamp_s: i64, nonce: &str, body: &[u8]) -> String {
    let sig = mac(secret, timestamp_s, nonce, body).unwrap_or_default();
    format!("t={timestamp_s},n={nonce},v1={}", hex::encode(sig))
}

fn plain_nonce(n: &str) -> bool {
    !n.is_empty()
        && n.len() <= MAX_NONCE_BYTES
        && n.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// Verify a delivery: the header parses, the nonce is plain, the timestamp
/// is inside the window, and the signature matches the exact body in
/// constant time. Replay is the caller's check on [`Verified::nonce`].
///
/// # Errors
/// The typed refusal.
pub fn verify(
    secret: &[u8],
    header: &str,
    body: &[u8],
    now_s: i64,
    window_s: i64,
) -> Result<Verified, Refusal> {
    let mut t = None;
    let mut n = None;
    let mut v1 = None;
    for part in header.split(',') {
        let Some((k, v)) = part.trim().split_once('=') else {
            return Err(Refusal::Malformed);
        };
        match k {
            "t" => t = v.parse::<i64>().ok(),
            "n" => n = Some(v.to_owned()),
            "v1" => v1 = Some(v.to_owned()),
            _ => return Err(Refusal::Malformed),
        }
    }
    let (Some(t), Some(n), Some(v1)) = (t, n, v1) else {
        return Err(Refusal::Malformed);
    };
    if !plain_nonce(&n) {
        return Err(Refusal::BadNonce);
    }
    let given = hex::decode(&v1).map_err(|_| Refusal::Malformed)?;
    let expected = mac(secret, t, &n, body).ok_or(Refusal::Malformed)?;
    // The signature is checked before the window so a forged request learns
    // nothing about the receiver's clock.
    if given.len() != expected.len() || !bool::from(given.ct_eq(&expected)) {
        return Err(Refusal::BadSignature);
    }
    if now_s - t > window_s {
        return Err(Refusal::Stale);
    }
    if t - now_s > window_s {
        return Err(Refusal::Future);
    }
    Ok(Verified {
        timestamp_s: t,
        nonce: n,
    })
}

/// A bounded in-memory nonce cache: the nonces seen inside the window, per
/// endpoint. A durable caller (the Cloud API) keeps the same facts in its
/// database; this is for tests and single-process receivers.
#[derive(Debug, Default)]
pub struct ReplayCache {
    seen: BTreeMap<(String, String), i64>,
}

impl ReplayCache {
    /// Record `nonce` for `endpoint` at `now_s`; `Err(Replayed)` when it was
    /// already recorded inside the window. Old entries are dropped.
    ///
    /// # Errors
    /// [`Refusal::Replayed`].
    pub fn check_and_record(
        &mut self,
        endpoint: &str,
        nonce: &str,
        now_s: i64,
        window_s: i64,
    ) -> Result<(), Refusal> {
        self.seen.retain(|_, at| now_s - *at <= window_s * 2);
        let key = (endpoint.to_owned(), nonce.to_owned());
        if self.seen.contains_key(&key) {
            return Err(Refusal::Replayed);
        }
        self.seen.insert(key, now_s);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: &[u8] = b"s3cret";

    #[test]
    fn a_good_signature_verifies_and_a_changed_byte_does_not() {
        let body = br#"{"ref":"refs/heads/main"}"#;
        let h = sign(SECRET, 1_000, "abc123", body);
        let v = verify(SECRET, &h, body, 1_010, 300).unwrap();
        assert_eq!(v.nonce, "abc123");
        assert_eq!(
            verify(SECRET, &h, br#"{"ref":"refs/heads/dev"}"#, 1_010, 300),
            Err(Refusal::BadSignature)
        );
        assert_eq!(
            verify(b"other", &h, body, 1_010, 300),
            Err(Refusal::BadSignature)
        );
    }

    #[test]
    fn the_window_bounds_both_directions_and_the_timestamp_is_signed() {
        let body = b"{}";
        let h = sign(SECRET, 1_000, "n1", body);
        assert_eq!(verify(SECRET, &h, body, 1_301, 300), Err(Refusal::Stale));
        assert_eq!(verify(SECRET, &h, body, 699, 300), Err(Refusal::Future));
        // Moving the body under a fresh timestamp breaks the signature.
        let moved = h.replace("t=1000", "t=5000");
        assert_eq!(
            verify(SECRET, &moved, body, 5_000, 300),
            Err(Refusal::BadSignature)
        );
    }

    #[test]
    fn malformed_headers_and_nonces_are_refused() {
        for bad in [
            "",
            "garbage",
            "t=1,n=a",
            "t=x,n=a,v1=00",
            "t=1,n=a,v1=zz",
            "t=1,n=,v1=00",
        ] {
            assert!(verify(SECRET, bad, b"", 1, 300).is_err(), "{bad}");
        }
        let h = sign(SECRET, 1, &"n".repeat(65), b"");
        assert_eq!(verify(SECRET, &h, b"", 1, 300), Err(Refusal::BadNonce));
    }

    #[test]
    fn a_nonce_is_good_once_inside_the_window() {
        let mut c = ReplayCache::default();
        assert!(c.check_and_record("ci", "n1", 100, 300).is_ok());
        assert_eq!(
            c.check_and_record("ci", "n1", 150, 300),
            Err(Refusal::Replayed)
        );
        assert!(c.check_and_record("other", "n1", 150, 300).is_ok());
        // After the window (twice over) the entry is forgotten; the timestamp
        // check is what refuses such an old delivery.
        assert!(c.check_and_record("ci", "n1", 1_000, 300).is_ok());
    }
}
