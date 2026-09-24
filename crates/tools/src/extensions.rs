//! The Extension System's package (REQ-EV-0138, REQ-EV-0225, REQ-EV-0240):
//! one governed surface for what a party outside the Core adds to it.
//!
//! An extension is a directory with a `modbit-extension.json` manifest and,
//! when its publisher signed it, a `modbit-extension.sig` beside it. The
//! manifest declares four kinds of thing, and each goes to the owner that
//! already governs its kind — the extension adds no runtime of its own:
//!
//! - `hooks` — typed lifecycle handlers, to the Hook Bus;
//! - `tools` — external tool servers, to the External Tool Hub, where they
//!   are reached as `external.<server>.<tool>` under the task's lease;
//! - `commands` — named instruction templates the person runs, which enter
//!   the task as the person's queued input;
//! - `providers` — OpenAI- or Anthropic-compatible model endpoints, to the
//!   Provider Gateway, their credential a handle in the Core's custody.
//!
//! Everything it would do is shown before it does any of it
//! (`capabilities`), with its publisher, its source and whether its
//! signature verifies against a key this Core trusts. Only a verified
//! extension is active on load; any other is quarantined — present, inert —
//! until the person trusts that exact manifest. A signature that does not
//! verify is a manifest changed after signing, and that is never trusted.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::hooks::HookSpec;

/// The file an extension declares itself in.
pub const EXTENSION_MANIFEST: &str = "modbit-extension.json";

/// The publisher's signature over the manifest's exact bytes.
pub const EXTENSION_SIGNATURE: &str = "modbit-extension.sig";

/// Bytes a command template may hold.
pub const MAX_TEMPLATE_BYTES: usize = 16 * 1024;

/// A named instruction template.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommandSpec {
    /// Name, unique within the extension.
    pub name: String,
    /// What it is for.
    #[serde(default)]
    pub description: String,
    /// The text; `{{arguments}}` is replaced by what the person passes.
    pub template: String,
}

impl CommandSpec {
    /// The instruction for `arguments`.
    #[must_use]
    pub fn expand(&self, arguments: &str) -> String {
        self.template.replace("{{arguments}}", arguments)
    }
}

/// One model an extension's provider serves.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderModel {
    /// Model id.
    pub model: String,
    /// Context window, tokens.
    pub context_tokens: u32,
    /// Output cap, tokens.
    #[serde(default = "default_max_output")]
    pub max_output_tokens: u32,
    /// Tool calling.
    #[serde(default)]
    pub tools: bool,
    /// Image input.
    #[serde(default)]
    pub vision: bool,
}

fn default_max_output() -> u32 {
    4_096
}

/// A model endpoint an extension adds.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderSpec {
    /// Name, unique within the extension; the endpoint is
    /// `ext.<extension>.<name>`.
    pub name: String,
    /// Wire family: `openai` | `anthropic`.
    pub kind: String,
    /// Base URL.
    pub base_url: String,
    /// The handle of a credential in the Core's custody, never a value.
    #[serde(default)]
    pub credential: Option<String>,
    /// Models served.
    pub models: Vec<ProviderModel>,
}

impl ProviderSpec {
    /// The endpoint name the gateway knows it by.
    #[must_use]
    pub fn endpoint(&self, extension: &str) -> String {
        format!("ext.{extension}.{}", self.name)
    }
}

/// An extension's declaration of itself.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExtensionManifest {
    /// Name.
    pub name: String,
    /// Version.
    pub version: String,
    /// Who publishes it.
    #[serde(default)]
    pub publisher: String,
    /// Where it comes from (a URL or a registry reference).
    #[serde(default)]
    pub source: String,
    /// Lifecycle hooks.
    #[serde(default)]
    pub hooks: Vec<HookSpec>,
    /// External tool servers.
    #[serde(default)]
    pub tools: Vec<modbit_mcp::ServerConfig>,
    /// Instruction templates.
    #[serde(default)]
    pub commands: Vec<CommandSpec>,
    /// Model endpoints.
    #[serde(default)]
    pub providers: Vec<ProviderSpec>,
}

fn simple_name(kind: &str, name: &str) -> Result<(), String> {
    let ok = !name.is_empty()
        && name.len() <= 64
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'));
    if ok {
        Ok(())
    } else {
        Err(format!(
            "`{name}` is not a {kind} name (1–64 of A–Z, a–z, 0–9, `-`, `_`)"
        ))
    }
}

impl ExtensionManifest {
    /// Parse and validate a manifest.
    ///
    /// # Errors
    /// Why it cannot be loaded.
    pub fn parse(json: &str) -> Result<Self, String> {
        let mut m: Self =
            serde_json::from_str(json).map_err(|e| format!("not an extension manifest: {e}"))?;
        simple_name("extension", &m.name)?;
        if m.version.trim().is_empty() {
            return Err("an extension names its version".into());
        }
        let mut seen = BTreeSet::new();
        for h in &m.hooks {
            h.validate()?;
            if !seen.insert(format!("hook:{}", h.name)) {
                return Err(format!("hook `{}` is declared twice", h.name));
            }
        }
        for t in &mut m.tools {
            // What only the host may say of a server — that it is trusted,
            // which of its tools are reads, which sites it serves — an
            // extension cannot say of its own (docs/16: server content is not
            // authority).
            if t.trust != modbit_mcp::Trust::Proposed
                || !t.read_only_tools.is_empty()
                || !t.sites.is_empty()
            {
                return Err(format!(
                    "tool server `{}`: an extension cannot declare its own trust, reads or sites; the host does",
                    t.name
                ));
            }
            t.validate()
                .map_err(|e| format!("tool server `{}`: {e:?}", t.name))?;
            if !seen.insert(format!("tool:{}", t.name)) {
                return Err(format!("tool server `{}` is declared twice", t.name));
            }
        }
        for c in &m.commands {
            simple_name("command", &c.name)?;
            if c.template.trim().is_empty() || c.template.len() > MAX_TEMPLATE_BYTES {
                return Err(format!(
                    "command `{}`: a template is 1..={MAX_TEMPLATE_BYTES} bytes",
                    c.name
                ));
            }
            if !seen.insert(format!("command:{}", c.name)) {
                return Err(format!("command `{}` is declared twice", c.name));
            }
        }
        for p in &m.providers {
            simple_name("provider", &p.name)?;
            if p.kind != "openai" && p.kind != "anthropic" {
                return Err(format!(
                    "provider `{}`: kind `{}` is neither `openai` nor `anthropic`",
                    p.name, p.kind
                ));
            }
            let url = p.base_url.trim();
            if !(url.starts_with("https://") || url.starts_with("http://")) {
                return Err(format!(
                    "provider `{}`: base_url must be an http(s) URL",
                    p.name
                ));
            }
            if p.models.is_empty() || p.models.iter().any(|m| m.model.trim().is_empty()) {
                return Err(format!("provider `{}` names no model", p.name));
            }
            if !seen.insert(format!("provider:{}", p.name)) {
                return Err(format!("provider `{}` is declared twice", p.name));
            }
        }
        Ok(m)
    }

    /// Everything the extension would do once active, in words a person can
    /// check before activating it.
    #[must_use]
    pub fn capabilities(&self) -> Vec<String> {
        let mut out = Vec::new();
        for h in &self.hooks {
            out.push(format!(
                "hook `{}`: runs `{}` {} ({}, fail {}, {} ms{})",
                h.name,
                h.command.join(" "),
                h.point.label(),
                h.mode.label(),
                h.fail_policy.label(),
                h.timeout_ms,
                if h.tools.is_empty() {
                    String::new()
                } else {
                    format!(", tools {}", h.tools.join(","))
                }
            ));
        }
        for t in &self.tools {
            let modbit_mcp::Transport::Stdio { command, args } = &t.transport;
            out.push(format!(
                "tool server `{}`: runs `{} {}`{}{}",
                t.name,
                command,
                args.join(" "),
                if t.requires.is_empty() {
                    String::new()
                } else {
                    format!(
                        "; requires {}",
                        t.requires.iter().cloned().collect::<Vec<_>>().join(",")
                    )
                },
                t.credential
                    .as_ref()
                    .map(|c| format!("; uses credential `{c}`"))
                    .unwrap_or_default()
            ));
        }
        for c in &self.commands {
            out.push(format!(
                "command `{}`: adds instructions to a task when the person runs it",
                c.name
            ));
        }
        for p in &self.providers {
            out.push(format!(
                "provider `{}`: sends model requests to {} ({}; models {}){}",
                p.endpoint(&self.name),
                p.base_url,
                p.kind,
                p.models
                    .iter()
                    .map(|m| m.model.clone())
                    .collect::<Vec<_>>()
                    .join(","),
                p.credential
                    .as_ref()
                    .map(|c| format!("; uses credential `{c}`"))
                    .unwrap_or_default()
            ));
        }
        out
    }
}

/// Whether a manifest's signature verifies.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SignatureStatus {
    /// Signed by a key this Core trusts, over exactly these bytes.
    Verified {
        /// The key.
        key_id: String,
    },
    /// No signature.
    Unsigned,
    /// Signed by a key this Core does not trust.
    UnknownKey {
        /// The key named.
        key_id: String,
    },
    /// A trusted key's signature that does not verify over these bytes: the
    /// manifest changed after it was signed.
    Invalid {
        /// The key named.
        key_id: String,
    },
}

impl SignatureStatus {
    /// Record label: `VERIFIED:<key>`, `UNSIGNED`, `UNKNOWN_KEY:<key>`,
    /// `INVALID:<key>`.
    #[must_use]
    pub fn label(&self) -> String {
        match self {
            Self::Verified { key_id } => format!("VERIFIED:{key_id}"),
            Self::Unsigned => "UNSIGNED".into(),
            Self::UnknownKey { key_id } => format!("UNKNOWN_KEY:{key_id}"),
            Self::Invalid { key_id } => format!("INVALID:{key_id}"),
        }
    }

    /// Why an extension with this signature is quarantined, or `None` when
    /// it may be active on load.
    #[must_use]
    pub fn quarantine_reason(&self) -> Option<String> {
        match self {
            Self::Verified { .. } => None,
            Self::Unsigned => Some(
                "unsigned: no publisher vouches for it; inert until the person trusts this manifest"
                    .into(),
            ),
            Self::UnknownKey { key_id } => Some(format!(
                "signed by `{key_id}`, a key this Core does not trust; inert until the person trusts this manifest"
            )),
            Self::Invalid { key_id } => Some(format!(
                "the signature by `{key_id}` does not verify: the manifest changed after it was signed; it cannot be trusted"
            )),
        }
    }
}

/// The signature file's shape.
#[derive(Deserialize)]
struct SignatureFile {
    key_id: String,
    signature_hex: String,
}

/// Verify `manifest` (its exact bytes) against the signature file's text,
/// when there is one, and the keys this Core trusts.
#[must_use]
pub fn verify(
    manifest: &[u8],
    signature: Option<&str>,
    trusted: &BTreeMap<String, [u8; 32]>,
) -> SignatureStatus {
    use ed25519_dalek::Verifier;
    let Some(text) = signature else {
        return SignatureStatus::Unsigned;
    };
    let Ok(sig) = serde_json::from_str::<SignatureFile>(text) else {
        return SignatureStatus::Invalid {
            key_id: String::new(),
        };
    };
    let Some(key) = trusted.get(&sig.key_id) else {
        return SignatureStatus::UnknownKey { key_id: sig.key_id };
    };
    let invalid = || SignatureStatus::Invalid {
        key_id: sig.key_id.clone(),
    };
    let Ok(vk) = ed25519_dalek::VerifyingKey::from_bytes(key) else {
        return invalid();
    };
    let Ok(bytes) = hex::decode(sig.signature_hex.trim()) else {
        return invalid();
    };
    let Ok(arr) = <[u8; 64]>::try_from(bytes.as_slice()) else {
        return invalid();
    };
    match vk.verify(manifest, &ed25519_dalek::Signature::from_bytes(&arr)) {
        Ok(()) => SignatureStatus::Verified {
            key_id: sig.key_id.clone(),
        },
        Err(_) => invalid(),
    }
}

/// Publisher keys from `"<key id>:<64 hex chars>,..."`; a malformed entry is
/// skipped, never trusted.
#[must_use]
pub fn trusted_keys(raw: &str) -> BTreeMap<String, [u8; 32]> {
    let mut out = BTreeMap::new();
    for pair in raw.split(',').filter(|s| !s.trim().is_empty()) {
        let Some((id, hex_key)) = pair.split_once(':') else {
            continue;
        };
        let Ok(bytes) = hex::decode(hex_key.trim()) else {
            continue;
        };
        if let Ok(key) = <[u8; 32]>::try_from(bytes.as_slice()) {
            out.insert(id.trim().to_owned(), key);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::Signer;

    #[test]
    fn a_manifest_is_validated_whole_and_says_what_it_would_do() {
        let m = ExtensionManifest::parse(
            r#"{"name":"kit","version":"1.0.0","publisher":"acme","source":"https://example.test/kit",
                "hooks":[{"name":"audit","point":"after_tool","command":["sh","audit.sh"]}],
                "tools":[{"name":"docs","transport":{"kind":"stdio","command":"docs-server","args":["--stdio"]},"requires":["network.egress"]}],
                "commands":[{"name":"review","template":"Review {{arguments}} for defects."}],
                "providers":[{"name":"local","kind":"openai","base_url":"http://127.0.0.1:9","models":[{"model":"m1","context_tokens":128000,"tools":true}]}]}"#,
        )
        .expect("valid");
        let caps = m.capabilities();
        assert_eq!(caps.len(), 4, "{caps:#?}");
        assert!(caps[1].contains("docs-server --stdio") && caps[1].contains("network.egress"));
        assert!(caps[3].contains("ext.kit.local") && caps[3].contains("http://127.0.0.1:9"));
        assert_eq!(
            m.commands[0].expand("src/lib.rs"),
            "Review src/lib.rs for defects."
        );
        for bad in [
            r#"{"name":"kit","version":""}"#,
            r#"{"name":"k it","version":"1"}"#,
            r#"{"name":"kit","version":"1","commands":[{"name":"x","template":""}]}"#,
            r#"{"name":"kit","version":"1","providers":[{"name":"p","kind":"grpc","base_url":"http://x","models":[{"model":"m","context_tokens":1}]}]}"#,
            r#"{"name":"kit","version":"1","providers":[{"name":"p","kind":"openai","base_url":"file:///x","models":[{"model":"m","context_tokens":1}]}]}"#,
            r#"{"name":"kit","version":"1","run_anything":true}"#,
            r#"{"name":"kit","version":"1","tools":[{"name":"d","transport":{"kind":"stdio","command":"x"},"trust":"TRUSTED"}]}"#,
            r#"{"name":"kit","version":"1","tools":[{"name":"d","transport":{"kind":"stdio","command":"x"},"read_only_tools":["delete_everything"]}]}"#,
        ] {
            assert!(ExtensionManifest::parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn only_a_trusted_key_over_the_exact_bytes_verifies() {
        let key = ed25519_dalek::SigningKey::from_bytes(&[9u8; 32]);
        let mut trusted = BTreeMap::new();
        trusted.insert("acme".to_owned(), key.verifying_key().to_bytes());
        let manifest = br#"{"name":"kit","version":"1"}"#;
        let sig = |bytes: &[u8], id: &str| {
            serde_json::json!({"key_id": id, "signature_hex": hex::encode(key.sign(bytes).to_bytes())})
                .to_string()
        };
        assert_eq!(
            verify(manifest, Some(&sig(manifest, "acme")), &trusted),
            SignatureStatus::Verified {
                key_id: "acme".into()
            }
        );
        assert_eq!(verify(manifest, None, &trusted), SignatureStatus::Unsigned);
        assert!(matches!(
            verify(manifest, Some(&sig(manifest, "stranger")), &trusted),
            SignatureStatus::UnknownKey { .. }
        ));
        let tampered = br#"{"name":"kit","version":"2"}"#;
        let status = verify(tampered, Some(&sig(manifest, "acme")), &trusted);
        assert!(matches!(status, SignatureStatus::Invalid { .. }));
        assert!(
            status
                .quarantine_reason()
                .unwrap()
                .contains("cannot be trusted")
        );
        assert_eq!(trusted_keys("acme:zz,bad,ok:00").len(), 0);
    }
}
