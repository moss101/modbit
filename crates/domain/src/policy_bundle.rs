//! The signed, versioned, tenant-scoped policy bundle (PX-129; docs/24
//! "Execution policy distribution"): what the control plane publishes and
//! the Cloud API, a Cloud Core Worker and a client each verify before they
//! use it. Pure: a document, a signature, and the rules a bundle must meet —
//! signed by a key the organisation registered, written for the tenant that
//! reads it, within its freshness window, and a generation no lower than the
//! last one accepted (so a rollback is refused, not merely discouraged).
//!
//! The signature covers the document's exact text under a domain tag, so a
//! bundle is not another message's signature and re-serialising it changes
//! nothing that was signed. The payload is the administrator layer of the
//! configuration resolver (`admin-config.json`); the only keys a cloud bundle
//! may carry are the ones an administrator layer owns — never the device
//! layer's, which only the machine itself sets.

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};

/// The bundle document's `kind`.
pub const KIND: &str = "modbit-policy-bundle";

/// The document schema this build reads.
pub const SCHEMA_VERSION: u32 = 1;

/// The most bytes a bundle document may have.
pub const MAX_DOCUMENT_BYTES: usize = 256 * 1024;

/// The signing domain: a bundle's signature is over this tag, then the text.
const DOMAIN_TAG: &[u8] = b"modbit-policy-bundle/1\n";

/// How far in the future a bundle's `issued_at_ms` may be (clock skew).
pub const SKEW_MS: i64 = 5 * 60 * 1000;

/// The keys an administrator layer owns (the device layer's `device` is not
/// among them).
pub const ADMIN_KEYS: &[&str] = &[
    "permissions",
    "network_allow",
    "models_allow",
    "mcp_servers",
    "mcp_deny",
    "hooks",
    "rules",
    "review_comment_authors",
];

/// A bundle as published: the document's exact text and the signature over it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignedBundle {
    /// The id of the organisation key that signed it.
    pub key_id: String,
    /// The document, as signed (JSON text).
    pub document: String,
    /// Hex Ed25519 signature over the domain tag and `document`.
    pub signature: String,
}

/// The document a signature covers.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BundleDocument {
    /// [`KIND`].
    pub kind: String,
    /// [`SCHEMA_VERSION`].
    pub schema_version: u32,
    /// The tenant it is written for.
    pub tenant_id: String,
    /// Monotonic per tenant; the next bundle's is greater.
    pub generation: u64,
    /// When it was issued (ms since the epoch).
    pub issued_at_ms: i64,
    /// When it stops being valid (ms since the epoch).
    pub expires_at_ms: i64,
    /// The lowest surface protocol major that may use it (the compatibility range).
    pub min_protocol_major: u32,
    /// The administrator layer: `admin-config.json`'s content.
    pub admin_config: serde_json::Value,
}

/// Why a bundle was refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BundleRefusal {
    /// The envelope or the document does not parse, or is too large.
    Malformed(String),
    /// The signing key is not one the organisation registered.
    UnknownKey(String),
    /// The signature does not verify over the document.
    BadSignature,
    /// Not a kind or a schema this build reads.
    Unsupported(String),
    /// Written for another tenant.
    WrongTenant,
    /// Issued in the future beyond the skew.
    NotYetValid,
    /// Past its expiry.
    Expired,
    /// A generation lower than the one already accepted.
    StaleGeneration {
        /// The lowest generation that would be accepted.
        floor: u64,
        /// The one presented.
        presented: u64,
    },
    /// Needs a newer protocol than this build speaks.
    Incompatible {
        /// The bundle's minimum.
        needs: u32,
        /// This build's.
        have: u32,
    },
    /// The administrator layer carries something it may not.
    ForbiddenContent(String),
}

impl BundleRefusal {
    /// A stable code.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::Malformed(_) => "BUNDLE_MALFORMED",
            Self::UnknownKey(_) => "BUNDLE_UNTRUSTED_KEY",
            Self::BadSignature => "BUNDLE_BAD_SIGNATURE",
            Self::Unsupported(_) => "BUNDLE_UNSUPPORTED",
            Self::WrongTenant => "BUNDLE_WRONG_TENANT",
            Self::NotYetValid => "BUNDLE_NOT_YET_VALID",
            Self::Expired => "BUNDLE_EXPIRED",
            Self::StaleGeneration { .. } => "BUNDLE_STALE_GENERATION",
            Self::Incompatible { .. } => "BUNDLE_INCOMPATIBLE",
            Self::ForbiddenContent(_) => "BUNDLE_FORBIDDEN_CONTENT",
        }
    }
}

impl std::fmt::Display for BundleRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Malformed(w) => write!(f, "the bundle is malformed: {w}"),
            Self::UnknownKey(k) => write!(f, "`{k}` is not a key this organisation registered"),
            Self::BadSignature => write!(f, "the signature does not verify over the document"),
            Self::Unsupported(w) => write!(f, "unsupported: {w}"),
            Self::WrongTenant => write!(f, "the bundle is written for another tenant"),
            Self::NotYetValid => write!(f, "the bundle is issued in the future"),
            Self::Expired => write!(f, "the bundle has expired"),
            Self::StaleGeneration { floor, presented } => write!(
                f,
                "generation {presented} is below {floor}; a bundle never moves policy backwards"
            ),
            Self::Incompatible { needs, have } => write!(
                f,
                "the bundle needs protocol major {needs}; this build speaks {have}"
            ),
            Self::ForbiddenContent(w) => write!(f, "the administrator layer may not carry {w}"),
        }
    }
}

impl std::error::Error for BundleRefusal {}

/// Sign a document under `key`.
///
/// # Errors
/// [`BundleRefusal::Malformed`] when the document is not serialisable or is
/// over [`MAX_DOCUMENT_BYTES`].
pub fn sign(
    document: &BundleDocument,
    key_id: &str,
    key: &SigningKey,
) -> Result<SignedBundle, BundleRefusal> {
    let text =
        serde_json::to_string(document).map_err(|e| BundleRefusal::Malformed(e.to_string()))?;
    if text.len() > MAX_DOCUMENT_BYTES {
        return Err(BundleRefusal::Malformed(format!(
            "the document is {} bytes; at most {MAX_DOCUMENT_BYTES}",
            text.len()
        )));
    }
    let signature = key.sign(&[DOMAIN_TAG, text.as_bytes()].concat());
    Ok(SignedBundle {
        key_id: key_id.to_owned(),
        document: text,
        signature: hex::encode(signature.to_bytes()),
    })
}

/// What a verifier needs besides the bundle.
pub struct Expect<'a> {
    /// The organisation's registered keys: `(key_id, public key)`.
    pub keys: &'a [(String, [u8; 32])],
    /// The tenant reading the bundle.
    pub tenant_id: &'a str,
    /// Now (ms since the epoch).
    pub now_ms: i64,
    /// The lowest generation accepted (inclusive).
    pub min_generation: u64,
    /// The surface protocol major this build speaks.
    pub protocol_major: u32,
}

/// Verify a bundle against `expect`; the document when it meets every rule.
///
/// # Errors
/// The first rule it breaks, as a [`BundleRefusal`].
pub fn verify(signed: &SignedBundle, expect: &Expect<'_>) -> Result<BundleDocument, BundleRefusal> {
    if signed.document.len() > MAX_DOCUMENT_BYTES {
        return Err(BundleRefusal::Malformed("the document is too large".into()));
    }
    let Some((_, public)) = expect.keys.iter().find(|(id, _)| *id == signed.key_id) else {
        return Err(BundleRefusal::UnknownKey(signed.key_id.clone()));
    };
    let key = VerifyingKey::from_bytes(public)
        .map_err(|_| BundleRefusal::Malformed("the registered key is not an Ed25519 key".into()))?;
    let sig_bytes = hex::decode(&signed.signature)
        .map_err(|_| BundleRefusal::Malformed("the signature is not hex".into()))?;
    let sig = Signature::from_slice(&sig_bytes)
        .map_err(|_| BundleRefusal::Malformed("the signature is not 64 bytes".into()))?;
    key.verify(&[DOMAIN_TAG, signed.document.as_bytes()].concat(), &sig)
        .map_err(|_| BundleRefusal::BadSignature)?;
    let doc: BundleDocument = serde_json::from_str(&signed.document)
        .map_err(|e| BundleRefusal::Malformed(e.to_string()))?;
    if doc.kind != KIND {
        return Err(BundleRefusal::Unsupported(format!("kind `{}`", doc.kind)));
    }
    if doc.schema_version != SCHEMA_VERSION {
        return Err(BundleRefusal::Unsupported(format!(
            "schema {}",
            doc.schema_version
        )));
    }
    if doc.tenant_id != expect.tenant_id {
        return Err(BundleRefusal::WrongTenant);
    }
    if doc.issued_at_ms > expect.now_ms + SKEW_MS {
        return Err(BundleRefusal::NotYetValid);
    }
    if doc.expires_at_ms <= expect.now_ms {
        return Err(BundleRefusal::Expired);
    }
    if doc.min_protocol_major > expect.protocol_major {
        return Err(BundleRefusal::Incompatible {
            needs: doc.min_protocol_major,
            have: expect.protocol_major,
        });
    }
    if doc.generation < expect.min_generation || doc.generation == 0 {
        return Err(BundleRefusal::StaleGeneration {
            floor: expect.min_generation.max(1),
            presented: doc.generation,
        });
    }
    match doc.admin_config.as_object() {
        None => {
            return Err(BundleRefusal::Malformed(
                "admin_config must be an object".into(),
            ));
        }
        Some(m) => {
            if let Some(k) = m.keys().find(|k| !ADMIN_KEYS.contains(&k.as_str())) {
                return Err(BundleRefusal::ForbiddenContent(format!("`{k}`")));
            }
        }
    }
    Ok(doc)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(seed: u8) -> SigningKey {
        SigningKey::from_bytes(&[seed; 32])
    }

    fn doc(generation: u64) -> BundleDocument {
        BundleDocument {
            kind: KIND.into(),
            schema_version: SCHEMA_VERSION,
            tenant_id: "t-1".into(),
            generation,
            issued_at_ms: 1_000,
            expires_at_ms: 10_000_000,
            min_protocol_major: 1,
            admin_config: serde_json::json!({"permissions": {"task.handoff": "DENY"}}),
        }
    }

    fn expect<'a>(keys: &'a [(String, [u8; 32])], floor: u64) -> Expect<'a> {
        Expect {
            keys,
            tenant_id: "t-1",
            now_ms: 5_000,
            min_generation: floor,
            protocol_major: 1,
        }
    }

    #[test]
    fn a_bundle_verifies_only_under_its_key_tenant_window_and_generation() {
        let k = key(1);
        let keys = vec![("org-1".to_owned(), k.verifying_key().to_bytes())];
        let signed = sign(&doc(3), "org-1", &k).unwrap();
        assert_eq!(verify(&signed, &expect(&keys, 3)).unwrap().generation, 3);
        // another organisation key
        let other = vec![("org-1".to_owned(), key(2).verifying_key().to_bytes())];
        assert_eq!(
            verify(&signed, &expect(&other, 1)),
            Err(BundleRefusal::BadSignature)
        );
        assert!(matches!(
            verify(&signed, &expect(&[], 1)),
            Err(BundleRefusal::UnknownKey(_))
        ));
        // tampered text keeps the old signature
        let mut tampered = signed.clone();
        tampered.document = tampered.document.replace("DENY", "ALLOW");
        assert_eq!(
            verify(&tampered, &expect(&keys, 1)),
            Err(BundleRefusal::BadSignature)
        );
        // a lower generation than accepted
        assert_eq!(
            verify(&signed, &expect(&keys, 4)),
            Err(BundleRefusal::StaleGeneration {
                floor: 4,
                presented: 3
            })
        );
        // another tenant, past expiry, the future
        let mut e = expect(&keys, 1);
        e.tenant_id = "t-2";
        assert_eq!(verify(&signed, &e), Err(BundleRefusal::WrongTenant));
        let mut e = expect(&keys, 1);
        e.now_ms = 20_000_000;
        assert_eq!(verify(&signed, &e), Err(BundleRefusal::Expired));
        let mut e = expect(&keys, 1);
        e.now_ms = -1_000_000;
        assert_eq!(verify(&signed, &e), Err(BundleRefusal::NotYetValid));
        // the device layer is not an administrator's to set
        let mut d = doc(1);
        d.admin_config = serde_json::json!({"device": {"sandbox_required": false}});
        let signed = sign(&d, "org-1", &k).unwrap();
        assert!(matches!(
            verify(&signed, &expect(&keys, 1)),
            Err(BundleRefusal::ForbiddenContent(_))
        ));
        // a signature over the text alone (no domain tag) is not a bundle's
        let text = serde_json::to_string(&doc(1)).unwrap();
        let bare = SignedBundle {
            key_id: "org-1".into(),
            signature: hex::encode(k.sign(text.as_bytes()).to_bytes()),
            document: text,
        };
        assert_eq!(
            verify(&bare, &expect(&keys, 1)),
            Err(BundleRefusal::BadSignature)
        );
    }
}
