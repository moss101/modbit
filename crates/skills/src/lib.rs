//! `modbit-skills` — skill packages, their registry, selector, compiler,
//! provenance and signing (docs/16 "Skills", docs/26 "Skill Registry and
//! Evolution", M5.5).
//!
//! Canonical owner: skills (`docs/12_REPOSITORY_AND_MODULE_LAYOUT.md`,
//! `docs/81_ARCHITECTURE_GUARDRAILS_AND_FORBIDDEN_DUPLICATION.md`).
//! Dependency direction is enforced by `tools/architecture-lint`.
//!
//! A skill is a portable package on disk: a `SKILL.md` whose front matter is
//! the manifest (name, version, description, compatibility, the tools it
//! needs, its capability ceiling, triggers, provenance, eval metadata) and
//! whose body is the instructions; optional `procedures/*.js` templates for
//! `proc.exec`; optional `resources/*` loaded by reference, never injected.
//! Its identity is the content hash over every file. Its lifecycle is
//! `incubator → evaluated → signed → enabled`: an `EVALUATION.json` beside
//! the manifest records a qualification; a `SIGNATURE.json` carries an
//! Ed25519 attestation of the content hash by a trusted key; a skill is
//! enabled by policy, never by its own say-so, and a skill cannot request
//! capabilities beyond the task's policy — the compiler intersects, it never
//! widens.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use ed25519_dalek::{Signature, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::Digest;

/// Why a package, a signature or a selection was refused.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
#[serde(tag = "code", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SkillError {
    /// The package directory has no `SKILL.md`.
    #[error("no SKILL.md in the package")]
    NoManifest,
    /// The front matter is missing or malformed.
    #[error("malformed front matter: {detail}")]
    MalformedManifest {
        /// What was wrong.
        detail: String,
    },
    /// A required field is missing or empty.
    #[error("manifest field `{field}` is missing or empty")]
    MissingField {
        /// Field.
        field: String,
    },
    /// The signature's key is not trusted.
    #[error("unknown signing key `{key_id}`")]
    UnknownKey {
        /// Key id.
        key_id: String,
    },
    /// The signature does not verify.
    #[error("the signature does not verify")]
    BadSignature,
    /// The attestation is for other content.
    #[error("the attestation names {attested}, the package is {actual}")]
    ContentMismatch {
        /// Attested identity.
        attested: String,
        /// Actual identity.
        actual: String,
    },
    /// The skill is not enabled.
    #[error("skill `{name}` is {lifecycle:?}, not enabled: {reason}")]
    NotEnabled {
        /// Skill.
        name: String,
        /// Its lifecycle state.
        lifecycle: Lifecycle,
        /// Why.
        reason: String,
    },
    /// No such skill.
    #[error("no skill named `{name}`")]
    Unknown {
        /// Name asked for.
        name: String,
    },
    /// A file or the package is larger than a skill may be (REQ-EV-0209:
    /// a package is instructions and small resources, not a payload).
    #[error("`{path}` is {bytes} bytes; the limit is {limit}")]
    Oversized {
        /// The file, or `<package>` for the whole.
        path: String,
        /// Its size.
        bytes: u64,
        /// The limit.
        limit: u64,
    },
    /// A file could not be read.
    #[error("cannot read `{path}`: {detail}")]
    Io {
        /// Path.
        path: String,
        /// Detail.
        detail: String,
    },
}

/// Compatibility the skill declares.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Compatibility {
    /// Modbit version range (informational; `*` or empty = any).
    #[serde(default)]
    pub modbit: String,
    /// Model families the skill was written for (empty = any).
    #[serde(default)]
    pub models: Vec<String>,
}

/// Provenance the skill carries.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Provenance {
    /// Where it came from (URL, path, importer).
    #[serde(default)]
    pub source: String,
    /// Author.
    #[serde(default)]
    pub author: String,
    /// License.
    #[serde(default)]
    pub license: String,
}

/// Evaluation metadata the skill names (the evaluation itself lives beside
/// the package as `EVALUATION.json`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvalMetadata {
    /// Benchmark id.
    #[serde(default)]
    pub benchmark: String,
    /// Task classes.
    #[serde(default)]
    pub task_classes: Vec<String>,
}

/// The manifest: `SKILL.md` front matter.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillManifest {
    /// Name (`[a-z0-9-]+`).
    #[serde(default)]
    pub name: String,
    /// Version.
    #[serde(default)]
    pub version: String,
    /// One line.
    #[serde(default)]
    pub description: String,
    /// Compatibility.
    #[serde(default)]
    pub compatibility: Compatibility,
    /// Tools (names or toolsets) the skill uses.
    #[serde(default)]
    pub required_tools: Vec<String>,
    /// Capability ids the skill may use at most.
    #[serde(default)]
    pub capability_ceiling: Vec<String>,
    /// Whether the model may select it (`false` = user/system only).
    #[serde(default = "default_true")]
    pub model_invocable: bool,
    /// Phrases that select it for a task goal.
    #[serde(default)]
    pub triggers: Vec<String>,
    /// Provenance.
    #[serde(default)]
    pub provenance: Provenance,
    /// Eval metadata.
    #[serde(default)]
    pub eval: EvalMetadata,
}

fn default_true() -> bool {
    true
}

/// A procedure template (`procedures/<name>.js`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Procedure {
    /// File name without extension.
    pub name: String,
    /// The program source.
    pub source: String,
    /// Content hash.
    pub hash: String,
}

/// A resource loaded by reference (`resources/<path>`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceRef {
    /// Path inside `resources/`.
    pub path: String,
    /// Content hash.
    pub hash: String,
    /// Size in bytes.
    pub bytes: u64,
}

/// A package as loaded from disk.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillPackage {
    /// The manifest.
    pub manifest: SkillManifest,
    /// The instructions (the `SKILL.md` body).
    pub instructions: String,
    /// Procedure templates.
    pub procedures: Vec<Procedure>,
    /// Resources by reference.
    pub resources: Vec<ResourceRef>,
    /// Content hash over every file but the attestations (path-sorted
    /// `sha256(path, sha256(bytes))`).
    pub content_hash: String,
    /// Where it was loaded from.
    pub root: PathBuf,
}

/// The lifecycle (docs/16): `incubator → evaluated → signed → enabled`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Lifecycle {
    /// Loaded, nothing attested.
    Incubator,
    /// An evaluation record with disposition PROMOTE is present.
    Evaluated,
    /// A trusted key attests the content hash.
    Signed,
    /// Signed (or, by a development policy, unsigned) and enabled by policy.
    Enabled,
}

/// The record `EVALUATION.json` carries (docs/26 `SkillQualification`,
/// the fields the registry reads).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Evaluation {
    /// Benchmark version.
    #[serde(default)]
    pub benchmark_version: String,
    /// Content hash the evaluation was of.
    #[serde(default)]
    pub content_hash: String,
    /// `PROMOTE` | `REJECT`.
    #[serde(default)]
    pub disposition: String,
    /// Verified-completion delta against the baseline, basis points.
    #[serde(default)]
    pub verified_completion_delta_bp: i64,
    /// Safety failures.
    #[serde(default)]
    pub safety_failures: u32,
}

/// What a signer attests: the exact content hash under a name and version.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Attestation {
    /// Skill name.
    pub name: String,
    /// Version.
    pub version: String,
    /// Content hash attested.
    pub content_hash: String,
    /// When.
    pub signed_at_ms: i64,
}

/// `SIGNATURE.json`: an attestation with the signature over its exact bytes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedSkill {
    /// Which trusted key signed it.
    pub key_id: String,
    /// Ed25519 signature over `attestation_json`, hex.
    pub signature_hex: String,
    /// The attestation, as the exact bytes that were signed.
    pub attestation_json: String,
}

/// The policy that enables skills (task/user policy; docs/16 "a skill cannot
/// request capabilities beyond task/user policy").
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillPolicy {
    /// Enable signed skills.
    pub enable_signed: bool,
    /// Enable unsigned skills too (development only).
    pub enable_incubator: bool,
}

impl Default for SkillPolicy {
    fn default() -> Self {
        Self {
            enable_signed: true,
            enable_incubator: false,
        }
    }
}

/// A skill in the registry with its lifecycle decided.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegisteredSkill {
    /// The package.
    pub package: SkillPackage,
    /// Lifecycle.
    pub lifecycle: Lifecycle,
    /// The verified attestation, when signed.
    pub attestation: Option<Attestation>,
    /// The evaluation, when present.
    pub evaluation: Option<Evaluation>,
    /// Why it is not further along.
    pub note: String,
}

/// The registry: every package found under the roots, with its lifecycle.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillRegistry {
    /// Skills, by name (the first root wins a name).
    pub skills: Vec<RegisteredSkill>,
    /// Packages that could not be loaded, with why.
    pub rejected: Vec<(String, SkillError)>,
}

fn sha_hex(bytes: &[u8]) -> String {
    hex::encode(sha2::Sha256::digest(bytes))
}

/// Parse `SKILL.md`: `---` front matter of `key: value` and `key: [a, b]`
/// lines (`outer.inner: value` nests one level), then the body.
///
/// # Errors
/// Missing or malformed front matter, or a missing required field.
pub fn parse_skill_md(text: &str) -> Result<(SkillManifest, String), SkillError> {
    let text = text.trim_start_matches('\u{feff}');
    let mut lines = text.lines();
    if lines.next().map(str::trim) != Some("---") {
        return Err(SkillError::MalformedManifest {
            detail: "SKILL.md must start with a `---` front matter fence".into(),
        });
    }
    let mut front: Vec<&str> = Vec::new();
    let mut closed = false;
    for l in lines.by_ref() {
        if l.trim() == "---" {
            closed = true;
            break;
        }
        front.push(l);
    }
    if !closed {
        return Err(SkillError::MalformedManifest {
            detail: "unterminated front matter".into(),
        });
    }
    let body: String = lines.collect::<Vec<_>>().join("\n").trim().to_owned();
    let mut map: serde_json::Map<String, serde_json::Value> = serde_json::Map::new();
    for raw in front {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((k, v)) = line.split_once(':') else {
            return Err(SkillError::MalformedManifest {
                detail: format!("expected `key: value`, got `{line}`"),
            });
        };
        let key = k.trim();
        let value = parse_scalar(v.trim());
        if let Some((outer, inner)) = key.split_once('.') {
            let entry = map
                .entry(outer.to_owned())
                .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
            if let serde_json::Value::Object(o) = entry {
                o.insert(inner.to_owned(), value);
            }
        } else {
            map.insert(key.to_owned(), value);
        }
    }
    let manifest: SkillManifest =
        serde_json::from_value(serde_json::Value::Object(map)).map_err(|e| {
            SkillError::MalformedManifest {
                detail: e.to_string(),
            }
        })?;
    for (field, empty) in [
        ("name", manifest.name.trim().is_empty()),
        ("version", manifest.version.trim().is_empty()),
        ("description", manifest.description.trim().is_empty()),
    ] {
        if empty {
            return Err(SkillError::MissingField {
                field: field.into(),
            });
        }
    }
    if !manifest
        .name
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return Err(SkillError::MalformedManifest {
            detail: format!("name `{}` is not [a-z0-9-]+", manifest.name),
        });
    }
    Ok((manifest, body))
}

fn parse_scalar(v: &str) -> serde_json::Value {
    if let Some(inner) = v.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
        let items: Vec<serde_json::Value> = inner
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| serde_json::Value::String(unquote(s).to_owned()))
            .collect();
        return serde_json::Value::Array(items);
    }
    match v {
        "true" => serde_json::Value::Bool(true),
        "false" => serde_json::Value::Bool(false),
        _ => serde_json::Value::String(unquote(v).to_owned()),
    }
}

fn unquote(s: &str) -> &str {
    s.strip_prefix('"')
        .and_then(|x| x.strip_suffix('"'))
        .or_else(|| s.strip_prefix('\'').and_then(|x| x.strip_suffix('\'')))
        .unwrap_or(s)
}

fn read(path: &Path) -> Result<Vec<u8>, SkillError> {
    std::fs::read(path).map_err(|e| SkillError::Io {
        path: path.display().to_string(),
        detail: e.to_string(),
    })
}

/// The files that attest a package and are therefore not part of what they
/// attest.
const ATTESTATION_FILES: &[&str] = &["SIGNATURE.json", "EVALUATION.json"];

/// The most one file of a package may be.
pub const MAX_FILE_BYTES: u64 = 1024 * 1024;
/// The most a whole package may be.
pub const MAX_PACKAGE_BYTES: u64 = 4 * 1024 * 1024;

/// Load a package from its directory.
///
/// # Errors
/// No `SKILL.md`, a malformed manifest, or an unreadable file.
pub fn load_package(dir: &Path) -> Result<SkillPackage, SkillError> {
    let manifest_path = dir.join("SKILL.md");
    if !manifest_path.is_file() {
        return Err(SkillError::NoManifest);
    }
    let text = String::from_utf8_lossy(&read(&manifest_path)?).into_owned();
    let (manifest, instructions) = parse_skill_md(&text)?;
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();
    collect_files(dir, dir, &mut files)?;
    files.sort_by(|a, b| a.0.cmp(&b.0));
    let mut total: u64 = 0;
    for (p, bytes) in &files {
        let n = bytes.len() as u64;
        if n > MAX_FILE_BYTES {
            return Err(SkillError::Oversized {
                path: p.clone(),
                bytes: n,
                limit: MAX_FILE_BYTES,
            });
        }
        total += n;
    }
    if total > MAX_PACKAGE_BYTES {
        return Err(SkillError::Oversized {
            path: "<package>".into(),
            bytes: total,
            limit: MAX_PACKAGE_BYTES,
        });
    }
    let mut hasher = sha2::Sha256::new();
    for (p, bytes) in &files {
        if ATTESTATION_FILES.contains(&p.as_str()) {
            continue;
        }
        hasher.update(p.as_bytes());
        hasher.update([0u8]);
        hasher.update(sha_hex(bytes).as_bytes());
        hasher.update([0u8]);
    }
    let content_hash = hex::encode(hasher.finalize());
    let mut procedures = Vec::new();
    let mut resources = Vec::new();
    for (p, bytes) in &files {
        if let Some(name) = p
            .strip_prefix("procedures/")
            .and_then(|n| n.strip_suffix(".js"))
        {
            procedures.push(Procedure {
                name: name.to_owned(),
                source: String::from_utf8_lossy(bytes).into_owned(),
                hash: sha_hex(bytes),
            });
        } else if let Some(rp) = p.strip_prefix("resources/") {
            resources.push(ResourceRef {
                path: rp.to_owned(),
                hash: sha_hex(bytes),
                bytes: bytes.len() as u64,
            });
        }
    }
    Ok(SkillPackage {
        manifest,
        instructions,
        procedures,
        resources,
        content_hash,
        root: dir.to_path_buf(),
    })
}

fn collect_files(
    root: &Path,
    dir: &Path,
    out: &mut Vec<(String, Vec<u8>)>,
) -> Result<(), SkillError> {
    let entries = std::fs::read_dir(dir).map_err(|e| SkillError::Io {
        path: dir.display().to_string(),
        detail: e.to_string(),
    })?;
    for e in entries.flatten() {
        let path = e.path();
        if path.is_dir() {
            collect_files(root, &path, out)?;
        } else if path.is_file() {
            let rel = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            out.push((rel, read(&path)?));
        }
    }
    Ok(())
}

/// Verify a signature against the trusted keys and the package it sits
/// beside.
///
/// # Errors
/// An unknown key, a bad signature, or an attestation for other content.
pub fn verify(
    signed: &SignedSkill,
    package: &SkillPackage,
    trusted: &BTreeMap<String, [u8; 32]>,
) -> Result<Attestation, SkillError> {
    let Some(key) = trusted.get(&signed.key_id) else {
        return Err(SkillError::UnknownKey {
            key_id: signed.key_id.clone(),
        });
    };
    let verifying = VerifyingKey::from_bytes(key).map_err(|_| SkillError::BadSignature)?;
    let raw = hex::decode(&signed.signature_hex).map_err(|_| SkillError::BadSignature)?;
    let bytes: [u8; 64] = raw.try_into().map_err(|_| SkillError::BadSignature)?;
    verifying
        .verify_strict(
            signed.attestation_json.as_bytes(),
            &Signature::from_bytes(&bytes),
        )
        .map_err(|_| SkillError::BadSignature)?;
    let attestation: Attestation = serde_json::from_str(&signed.attestation_json).map_err(|e| {
        SkillError::MalformedManifest {
            detail: format!("attestation: {e}"),
        }
    })?;
    if attestation.content_hash != package.content_hash
        || attestation.name != package.manifest.name
        || attestation.version != package.manifest.version
    {
        return Err(SkillError::ContentMismatch {
            attested: format!(
                "{}@{} {}",
                attestation.name, attestation.version, attestation.content_hash
            ),
            actual: format!(
                "{}@{} {}",
                package.manifest.name, package.manifest.version, package.content_hash
            ),
        });
    }
    Ok(attestation)
}

/// Sign a package's attestation with a key (for tooling and tests; the Core
/// only verifies).
#[must_use]
pub fn sign(
    package: &SkillPackage,
    key_id: &str,
    signing_key: &ed25519_dalek::SigningKey,
    signed_at_ms: i64,
) -> SignedSkill {
    use ed25519_dalek::Signer;
    let attestation = Attestation {
        name: package.manifest.name.clone(),
        version: package.manifest.version.clone(),
        content_hash: package.content_hash.clone(),
        signed_at_ms,
    };
    let attestation_json = serde_json::to_string(&attestation).unwrap_or_default();
    let signature = signing_key.sign(attestation_json.as_bytes());
    SignedSkill {
        key_id: key_id.into(),
        signature_hex: hex::encode(signature.to_bytes()),
        attestation_json,
    }
}

/// Trusted skill-signing keys from `MODBIT_SKILL_KEYS="<key id>:<64 hex>,..."`
/// (the model registry's convention); a malformed entry is skipped, never
/// trusted.
#[must_use]
pub fn trusted_keys_from_env(raw: &str) -> BTreeMap<String, [u8; 32]> {
    let mut out = BTreeMap::new();
    for entry in raw.split(',') {
        let Some((id, hexkey)) = entry.trim().split_once(':') else {
            continue;
        };
        let Ok(bytes) = hex::decode(hexkey.trim()) else {
            continue;
        };
        let Ok(key) = <[u8; 32]>::try_from(bytes) else {
            continue;
        };
        if !id.trim().is_empty() {
            out.insert(id.trim().to_owned(), key);
        }
    }
    out
}

impl SkillRegistry {
    /// Discover every package under `roots` (each root holds one directory
    /// per skill) and decide each lifecycle under `policy` with `trusted`
    /// keys. The first root that carries a name wins it (a project skill
    /// shadows a user skill of the same name).
    #[must_use]
    pub fn discover(
        roots: &[PathBuf],
        trusted: &BTreeMap<String, [u8; 32]>,
        policy: &SkillPolicy,
    ) -> Self {
        let mut reg = Self::default();
        for root in roots {
            let Ok(entries) = std::fs::read_dir(root) else {
                continue;
            };
            let mut dirs: Vec<PathBuf> = entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.is_dir())
                .collect();
            dirs.sort();
            for dir in dirs {
                match load_package(&dir) {
                    Ok(package) => {
                        if reg
                            .skills
                            .iter()
                            .any(|s| s.package.manifest.name == package.manifest.name)
                        {
                            continue;
                        }
                        reg.skills.push(register(package, trusted, policy));
                    }
                    Err(SkillError::NoManifest) => {}
                    Err(e) => reg.rejected.push((dir.display().to_string(), e)),
                }
            }
        }
        reg
    }

    /// A skill by name.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&RegisteredSkill> {
        self.skills.iter().find(|s| s.package.manifest.name == name)
    }
}

fn register(
    package: SkillPackage,
    trusted: &BTreeMap<String, [u8; 32]>,
    policy: &SkillPolicy,
) -> RegisteredSkill {
    let mut lifecycle = Lifecycle::Incubator;
    let mut notes: Vec<String> = Vec::new();
    let evaluation: Option<Evaluation> = std::fs::read(package.root.join("EVALUATION.json"))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok());
    match &evaluation {
        Some(ev) if ev.content_hash == package.content_hash && ev.disposition == "PROMOTE" => {
            lifecycle = Lifecycle::Evaluated;
        }
        Some(ev) => notes.push(format!(
            "evaluation is of {} with disposition {}; the package is {}",
            ev.content_hash, ev.disposition, package.content_hash
        )),
        None => notes.push("no evaluation record".into()),
    }
    let mut attestation = None;
    match std::fs::read(package.root.join("SIGNATURE.json")) {
        Ok(bytes) => match serde_json::from_slice::<SignedSkill>(&bytes) {
            Ok(signed) => match verify(&signed, &package, trusted) {
                Ok(a) => {
                    attestation = Some(a);
                    lifecycle = Lifecycle::Signed;
                }
                Err(e) => notes.push(format!("signature refused: {e}")),
            },
            Err(e) => notes.push(format!("SIGNATURE.json malformed: {e}")),
        },
        Err(_) => notes.push("no signature".into()),
    }
    let enable = match lifecycle {
        Lifecycle::Signed => policy.enable_signed,
        Lifecycle::Incubator | Lifecycle::Evaluated => policy.enable_incubator,
        Lifecycle::Enabled => true,
    };
    if enable {
        lifecycle = Lifecycle::Enabled;
        notes.clear();
    } else if lifecycle == Lifecycle::Signed {
        notes.push("signed skills are not enabled by policy".into());
    } else {
        notes.push("unsigned skills are not enabled by policy".into());
    }
    RegisteredSkill {
        package,
        lifecycle,
        attestation,
        evaluation,
        note: notes.join("; "),
    }
}

/// Why a skill was selected.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SelectionReason {
    /// Named by the task or the user.
    Explicit,
    /// A trigger phrase matched the goal.
    Trigger {
        /// The phrase.
        phrase: String,
    },
}

/// A selected skill.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Selection {
    /// Skill name.
    pub name: String,
    /// Version.
    pub version: String,
    /// Content hash.
    pub content_hash: String,
    /// Why.
    pub reason: SelectionReason,
}

/// A skill asked for but not usable, with why.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rejection {
    /// Skill name.
    pub name: String,
    /// Why.
    pub error: SkillError,
}

/// Select skills for a task: the explicit names first (refused with a
/// reason when unknown or not enabled), then every enabled, model-invocable
/// skill one of whose trigger phrases occurs in the goal.
#[must_use]
pub fn select(
    registry: &SkillRegistry,
    goal: &str,
    explicit: &[String],
) -> (Vec<Selection>, Vec<Rejection>) {
    let mut selected: Vec<Selection> = Vec::new();
    let mut rejected: Vec<Rejection> = Vec::new();
    for name in explicit {
        match registry.get(name) {
            Some(s) if s.lifecycle == Lifecycle::Enabled => selected.push(Selection {
                name: s.package.manifest.name.clone(),
                version: s.package.manifest.version.clone(),
                content_hash: s.package.content_hash.clone(),
                reason: SelectionReason::Explicit,
            }),
            Some(s) => rejected.push(Rejection {
                name: name.clone(),
                error: SkillError::NotEnabled {
                    name: name.clone(),
                    lifecycle: s.lifecycle,
                    reason: s.note.clone(),
                },
            }),
            None => rejected.push(Rejection {
                name: name.clone(),
                error: SkillError::Unknown { name: name.clone() },
            }),
        }
    }
    let goal_lower = goal.to_lowercase();
    for s in &registry.skills {
        if s.lifecycle != Lifecycle::Enabled
            || !s.package.manifest.model_invocable
            || selected.iter().any(|x| x.name == s.package.manifest.name)
        {
            continue;
        }
        if let Some(phrase) = s
            .package
            .manifest
            .triggers
            .iter()
            .find(|t| !t.trim().is_empty() && goal_lower.contains(&t.trim().to_lowercase()))
        {
            selected.push(Selection {
                name: s.package.manifest.name.clone(),
                version: s.package.manifest.version.clone(),
                content_hash: s.package.content_hash.clone(),
                reason: SelectionReason::Trigger {
                    phrase: phrase.clone(),
                },
            });
        }
    }
    (selected, rejected)
}

/// What a skill compiles to for one turn: minimal instructions, the tools it
/// needs that the turn projects (never more), and what is loaded by
/// reference.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompiledSkill {
    /// Skill name.
    pub name: String,
    /// Version.
    pub version: String,
    /// Content hash.
    pub content_hash: String,
    /// The instructions injected (bounded).
    pub instructions: String,
    /// Whether the instructions were cut at the budget.
    pub instructions_truncated: bool,
    /// Hash of the injected instructions.
    pub instructions_hash: String,
    /// Required tools the turn projects.
    pub tool_projection: Vec<String>,
    /// Required tools the turn does not project (told, not granted).
    pub tools_unavailable: Vec<String>,
    /// Procedure templates, by reference (name, hash).
    pub procedures: Vec<(String, String)>,
    /// Resources, by reference.
    pub resources: Vec<ResourceRef>,
}

/// Compile a skill against the tools the task may use at most (its policy
/// surface, or a turn's projection) under a byte budget for the injected
/// instructions. The compiler intersects the skill's tools with what it is
/// given — a skill never widens what the task may do (docs/16).
#[must_use]
pub fn compile(skill: &SkillPackage, projection: &[String], budget_bytes: usize) -> CompiledSkill {
    let mut instructions = format!(
        "# Skill: {} v{}\n{}\n\n{}",
        skill.manifest.name, skill.manifest.version, skill.manifest.description, skill.instructions
    );
    let mut truncated = false;
    if instructions.len() > budget_bytes {
        let mut cut = budget_bytes;
        while !instructions.is_char_boundary(cut) {
            cut -= 1;
        }
        instructions.truncate(cut);
        instructions.push_str(
            "\n… (skill instructions cut at the budget; the rest is in the package by reference)",
        );
        truncated = true;
    }
    let mut tool_projection = Vec::new();
    let mut tools_unavailable = Vec::new();
    for t in &skill.manifest.required_tools {
        let matching: Vec<String> = projection
            .iter()
            .filter(|p| *p == t || p.starts_with(&format!("{t}.")))
            .cloned()
            .collect();
        if matching.is_empty() {
            tools_unavailable.push(t.clone());
        } else {
            tool_projection.extend(matching);
        }
    }
    tool_projection.sort();
    tool_projection.dedup();
    if !tools_unavailable.is_empty() {
        instructions.push_str(&format!(
            "\n\n(This skill also names {} which this task's policy does not offer; work without them.)",
            tools_unavailable.join(", ")
        ));
    }
    if !skill.procedures.is_empty() {
        instructions.push_str("\n\nProcedure templates of this skill you may run with proc.exec: ");
        instructions.push_str(
            &skill
                .procedures
                .iter()
                .map(|p| p.name.as_str())
                .collect::<Vec<_>>()
                .join(", "),
        );
    }
    CompiledSkill {
        name: skill.manifest.name.clone(),
        version: skill.manifest.version.clone(),
        content_hash: skill.content_hash.clone(),
        instructions_hash: sha_hex(instructions.as_bytes()),
        instructions,
        instructions_truncated: truncated,
        tool_projection,
        tools_unavailable,
        procedures: skill
            .procedures
            .iter()
            .map(|p| (p.name.clone(), p.hash.clone()))
            .collect(),
        resources: skill.resources.clone(),
    }
}
