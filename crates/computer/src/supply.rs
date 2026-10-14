//! The actuator's supply chain (PX-072; docs/66 CUC-E01, CUC-E03, CUC-E04).
//!
//! The helper ships INSIDE the signed application bundle and updates only
//! with the application (M10.2); there is no run-time download path anywhere
//! in this module or this crate. What the Core does with a bundled helper:
//!
//! * **A hash-pinned manifest.** `actuator-manifest.json` is a signed
//!   envelope - the same `{payload, signature, key_id}` shape, ed25519 and
//!   key-id rule as the M10.2 update manifest (`apps/desktop/src/main/update/
//!   manifest.ts`), so the release pipeline signs it with the tooling it
//!   already has. The payload lists each bundled helper: path (inside the
//!   manifest's directory), sha256, size, version, platform, architecture,
//!   the minimum Core version, the protocol major it speaks, whether its code
//!   signature must verify (and against which designated requirement), and
//!   the version to roll back to.
//! * **Verification before every spawn.** The signature of the manifest, the
//!   path (no `..`, no absolute path, no symlink out), the size and sha256 of
//!   the file, the executable bit, the Core and protocol versions and, where
//!   required, `codesign --verify` (macOS) - immediately before the process
//!   is created. A modified binary is never executed.
//! * **Process identity.** After the spawn and BEFORE the Core sends its hello
//!   (so an unverified image never receives the per-launch token), the pid's
//!   executable path and the file's sha256 are checked against the manifest
//!   entry. A signal is delivered to a bare pid only after its executable path
//!   is verified ([`terminate_verified`]).
//! * **Rollback.** When the newest entry does not verify, does not fit this
//!   Core or is refused at the handshake, the entry named by `rollback_to` is
//!   tried, and so on down the chain; every rejection is a typed reason that is
//!   kept and reported. When nothing verifies nothing is launched.
//!
//! What this module does not do: apply an update. Replacing the bundle
//! (staging, the previous version kept, recovery from a crash mid-update,
//! waiting for a live control session) is the application updater's (M10.2);
//! the interface it needs from here is "verify this directory" ([`plan`]).

use std::fmt;
use std::path::{Path, PathBuf};
use std::process::Command;

use base64::Engine as _;
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// The manifest schema this Core reads.
pub const SCHEMA: u32 = 1;
/// The `kind` every actuator manifest carries.
pub const KIND: &str = "modbit-actuator-manifest";

/// ASN.1 prefix of an ed25519 SubjectPublicKeyInfo (RFC 8410): the DER of a
/// public key is this followed by the 32 key bytes.
const SPKI_PREFIX: [u8; 12] = [
    0x30, 0x2a, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x03, 0x21, 0x00,
];

/// One bundled helper.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HelperEntry {
    /// Helper name, e.g. `modbit-actuator-macos`.
    pub id: String,
    /// Its version, `MAJOR.MINOR.PATCH[-pre]`.
    pub version: String,
    /// Path of the executable relative to the manifest's directory.
    pub path: String,
    /// sha256 of the executable, lowercase hex.
    pub sha256: String,
    /// Size of the executable in bytes.
    pub size: u64,
    /// `macos` | `linux` | `windows`.
    pub platform: String,
    /// `arm64` | `x86_64` | `universal`.
    pub arch: String,
    /// The oldest Core version that may launch it.
    pub min_core_version: String,
    /// The actuator protocol major it speaks.
    pub protocol_major: u32,
    /// `codesign --verify` must pass before it is launched.
    #[serde(default)]
    pub require_signature: bool,
    /// A codesign requirement the signature must satisfy, e.g.
    /// `anchor apple generic and certificate leaf[subject.OU] = "TEAMID"`.
    #[serde(default)]
    pub designated_requirement: Option<String>,
    /// The version of the entry to fall back to.
    #[serde(default)]
    pub rollback_to: Option<String>,
}

/// A reference to an SBOM document released with the build.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SbomRef {
    /// Document name.
    pub name: String,
    /// Its sha256.
    pub sha256: String,
}

/// The signed payload.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    /// [`SCHEMA`].
    pub schema: u32,
    /// [`KIND`].
    pub kind: String,
    /// The bundled helpers.
    pub helpers: Vec<HelperEntry>,
    /// SBOM documents.
    #[serde(default)]
    pub sbom: Vec<SbomRef>,
}

/// The envelope on disk (the M10.2 shape).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignedManifest {
    /// The exact JSON text that was signed.
    pub payload: String,
    /// base64 ed25519 signature over the payload's UTF-8 bytes.
    pub signature: String,
    /// First 16 hex characters of sha256(SPKI DER of the signing key).
    pub key_id: String,
}

/// A trusted signing key (the M10.2 `PinnedKey` shape).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PinnedKey {
    /// Declared key id; it must equal the id computed from the key.
    pub key_id: String,
    /// SPKI PEM of the ed25519 public key.
    pub public_key_pem: String,
    /// A revoked key verifies nothing.
    #[serde(default)]
    pub revoked: bool,
}

/// How to find and trust the bundled helper.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SupplyConfig {
    /// `actuator-manifest.json`.
    pub manifest: PathBuf,
    /// A JSON array of [`PinnedKey`], read at every launch (a rotated key
    /// takes effect with the next spawn).
    pub keys_file: Option<PathBuf>,
    /// Keys given directly (tests, embedded builds).
    pub keys: Vec<PinnedKey>,
    /// Accept a manifest with no signature (a developer build). Never set by
    /// the packaged app.
    pub allow_unsigned: bool,
    /// The Core's own version, for `min_core_version`.
    pub core_version: String,
}

/// Why a helper was not launched. Every reason has a stable code.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Reject {
    /// The path leaves the manifest's directory.
    PathEscapes(String),
    /// The file is not there or not readable.
    Missing(String),
    /// Not a regular file (a directory, a device, a symlink out of the bundle).
    NotRegularFile(String),
    /// The size differs from the manifest.
    SizeMismatch {
        /// Pinned.
        want: u64,
        /// Found.
        got: u64,
    },
    /// The sha256 differs from the manifest.
    DigestMismatch {
        /// Pinned.
        want: String,
        /// Found.
        got: String,
    },
    /// Not executable.
    NotExecutable(String),
    /// This Core is older than the entry's minimum.
    CoreTooOld {
        /// Needed.
        needs: String,
        /// This Core's.
        have: String,
    },
    /// The entry speaks another protocol major.
    ProtocolMismatch {
        /// The helper's.
        helper: u32,
        /// This Core's.
        core: u32,
    },
    /// `codesign` could not be run where a signature is required.
    CodesignUnavailable(String),
    /// `codesign --verify` failed or the requirement was not met.
    SignatureCheckFailed(String),
    /// The running process is not the verified file.
    ProcessIdentity(String),
    /// The helper refused this Core at the handshake.
    Refused(String),
    /// The helper started and failed its handshake.
    Handshake(String),
    /// The helper could not be started.
    Spawn(String),
}

impl Reject {
    /// The stable code of the reason.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::PathEscapes(_) => "PATH_ESCAPES_BUNDLE",
            Self::Missing(_) => "HELPER_MISSING",
            Self::NotRegularFile(_) => "NOT_A_REGULAR_FILE",
            Self::SizeMismatch { .. } => "SIZE_MISMATCH",
            Self::DigestMismatch { .. } => "DIGEST_MISMATCH",
            Self::NotExecutable(_) => "NOT_EXECUTABLE",
            Self::CoreTooOld { .. } => "CORE_TOO_OLD",
            Self::ProtocolMismatch { .. } => "PROTOCOL_MISMATCH",
            Self::CodesignUnavailable(_) => "CODESIGN_UNAVAILABLE",
            Self::SignatureCheckFailed(_) => "SIGNATURE_CHECK_FAILED",
            Self::ProcessIdentity(_) => "PROCESS_IDENTITY_MISMATCH",
            Self::Refused(_) => "HELPER_REFUSED",
            Self::Handshake(_) => "HANDSHAKE_FAILED",
            Self::Spawn(_) => "SPAWN_FAILED",
        }
    }
}

impl fmt::Display for Reject {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: ", self.code())?;
        match self {
            Self::PathEscapes(s)
            | Self::Missing(s)
            | Self::NotRegularFile(s)
            | Self::NotExecutable(s)
            | Self::CodesignUnavailable(s)
            | Self::SignatureCheckFailed(s)
            | Self::ProcessIdentity(s)
            | Self::Refused(s)
            | Self::Handshake(s)
            | Self::Spawn(s) => write!(f, "{s}"),
            Self::SizeMismatch { want, got } => write!(f, "{got} bytes, the manifest pins {want}"),
            Self::DigestMismatch { want, got } => write!(f, "sha256 {got}, the manifest pins {want}"),
            Self::CoreTooOld { needs, have } => write!(f, "needs Core {needs}, this is {have}"),
            Self::ProtocolMismatch { helper, core } => {
                write!(f, "the helper speaks protocol {helper}, this Core {core}")
            }
        }
    }
}

/// One rejected candidate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Attempt {
    /// The entry's version.
    pub version: String,
    /// Why it was not used.
    pub reason: Reject,
}

/// Why no helper could be chosen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SupplyError {
    /// The manifest file is missing or unreadable.
    ManifestUnreadable(String),
    /// Not the JSON of an actuator manifest.
    ManifestMalformed(String),
    /// A manifest with no signature, where one is required.
    ManifestUnsigned,
    /// Signed by a key this build does not pin.
    SignatureUnknownKey(String),
    /// Signed by a revoked key.
    SignatureKeyRevoked(String),
    /// The signature does not match the payload.
    SignatureInvalid,
    /// The trusted-keys file is unreadable or malformed.
    KeysUnreadable(String),
    /// No entry for this platform and architecture.
    NoEntry(String),
    /// Entries exist and none verified (or none could be launched).
    NoVerifiedHelper(Vec<Attempt>),
}

impl SupplyError {
    /// The stable code.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::ManifestUnreadable(_) => "MANIFEST_UNREADABLE",
            Self::ManifestMalformed(_) => "MANIFEST_MALFORMED",
            Self::ManifestUnsigned => "MANIFEST_UNSIGNED",
            Self::SignatureUnknownKey(_) => "SIGNATURE_UNKNOWN_KEY",
            Self::SignatureKeyRevoked(_) => "SIGNATURE_KEY_REVOKED",
            Self::SignatureInvalid => "SIGNATURE_INVALID",
            Self::KeysUnreadable(_) => "KEYS_UNREADABLE",
            Self::NoEntry(_) => "NO_ENTRY_FOR_PLATFORM",
            Self::NoVerifiedHelper(_) => "NO_VERIFIED_HELPER",
        }
    }
}

impl fmt::Display for SupplyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: ", self.code())?;
        match self {
            Self::ManifestUnreadable(s)
            | Self::ManifestMalformed(s)
            | Self::SignatureUnknownKey(s)
            | Self::SignatureKeyRevoked(s)
            | Self::KeysUnreadable(s)
            | Self::NoEntry(s) => write!(f, "{s}"),
            Self::ManifestUnsigned => write!(f, "the manifest carries no signature"),
            Self::SignatureInvalid => write!(f, "the signature does not match the payload"),
            Self::NoVerifiedHelper(a) => {
                let parts: Vec<String> =
                    a.iter().map(|x| format!("{} -> {}", x.version, x.reason)).collect();
                write!(f, "no bundled helper could be used ({})", parts.join("; "))
            }
        }
    }
}

impl std::error::Error for SupplyError {}

/// A helper that passed verification just now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Verified {
    /// Its version.
    pub version: String,
    /// The canonical path to execute.
    pub path: PathBuf,
    /// Its sha256.
    pub sha256: String,
    /// The protocol major it declared.
    pub protocol_major: u32,
}

/// What a launch of a bundled helper settled on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SupplyReport {
    /// The helper in use.
    pub used: Verified,
    /// Candidates rejected on the way, newest first. Non-empty means the
    /// launch ROLLED BACK.
    pub rejected: Vec<Attempt>,
}

// ------------------------------------------------------------------ keys and signatures

/// The key id of a public key: first 16 hex characters of sha256(SPKI DER).
#[must_use]
pub fn key_id_of(public: &[u8; 32]) -> String {
    let mut der = SPKI_PREFIX.to_vec();
    der.extend_from_slice(public);
    hex::encode(Sha256::digest(&der))[..16].to_owned()
}

/// The SPKI PEM of a public key.
#[must_use]
pub fn pem_of(public: &[u8; 32]) -> String {
    let mut der = SPKI_PREFIX.to_vec();
    der.extend_from_slice(public);
    let b64 = base64::engine::general_purpose::STANDARD.encode(der);
    let lines: Vec<String> = b64
        .as_bytes()
        .chunks(64)
        .map(|c| String::from_utf8_lossy(c).into_owned())
        .collect();
    format!("-----BEGIN PUBLIC KEY-----\n{}\n-----END PUBLIC KEY-----\n", lines.join("\n"))
}

/// A [`PinnedKey`] for a signing key.
#[must_use]
pub fn pinned_for(sk: &SigningKey) -> PinnedKey {
    let public = sk.verifying_key().to_bytes();
    PinnedKey {
        key_id: key_id_of(&public),
        public_key_pem: pem_of(&public),
        revoked: false,
    }
}

fn verifying_key(k: &PinnedKey) -> Result<VerifyingKey, SupplyError> {
    let body: String = k
        .public_key_pem
        .lines()
        .filter(|l| !l.starts_with("-----"))
        .collect();
    let der = base64::engine::general_purpose::STANDARD
        .decode(body.trim())
        .map_err(|e| SupplyError::KeysUnreadable(format!("key {}: {e}", k.key_id)))?;
    if der.len() != 44 || der[..12] != SPKI_PREFIX {
        return Err(SupplyError::KeysUnreadable(format!(
            "key {} is not an ed25519 SPKI public key",
            k.key_id
        )));
    }
    let mut raw = [0u8; 32];
    raw.copy_from_slice(&der[12..]);
    if key_id_of(&raw) != k.key_id {
        return Err(SupplyError::KeysUnreadable(format!(
            "key {} does not match its declared id",
            k.key_id
        )));
    }
    VerifyingKey::from_bytes(&raw)
        .map_err(|e| SupplyError::KeysUnreadable(format!("key {}: {e}", k.key_id)))
}

/// Sign a manifest the way the release pipeline does.
///
/// # Panics
/// Only if the manifest cannot be serialised (it is plain data).
#[must_use]
pub fn sign_manifest(m: &Manifest, sk: &SigningKey) -> SignedManifest {
    let payload = serde_json::to_string(m).expect("a manifest serialises");
    let sig = sk.sign(payload.as_bytes());
    SignedManifest {
        payload,
        signature: base64::engine::general_purpose::STANDARD.encode(sig.to_bytes()),
        key_id: key_id_of(&sk.verifying_key().to_bytes()),
    }
}

/// Verify the envelope against the pinned keys, then parse the payload.
///
/// # Errors
/// The typed reason the manifest is not trusted.
pub fn verify_manifest(s: &SignedManifest, pinned: &[PinnedKey]) -> Result<Manifest, SupplyError> {
    let Some(key) = pinned.iter().find(|k| k.key_id == s.key_id) else {
        return Err(SupplyError::SignatureUnknownKey(format!(
            "key {} is not pinned in this build",
            s.key_id
        )));
    };
    if key.revoked {
        return Err(SupplyError::SignatureKeyRevoked(format!("key {} is revoked", s.key_id)));
    }
    let vk = verifying_key(key)?;
    let sig_bytes = base64::engine::general_purpose::STANDARD
        .decode(s.signature.trim())
        .map_err(|_| SupplyError::SignatureInvalid)?;
    let sig_arr: [u8; 64] = sig_bytes.try_into().map_err(|_| SupplyError::SignatureInvalid)?;
    vk.verify(s.payload.as_bytes(), &Signature::from_bytes(&sig_arr))
        .map_err(|_| SupplyError::SignatureInvalid)?;
    parse_payload(&s.payload)
}

fn parse_payload(payload: &str) -> Result<Manifest, SupplyError> {
    let m: Manifest = serde_json::from_str(payload)
        .map_err(|e| SupplyError::ManifestMalformed(e.to_string()))?;
    if m.schema != SCHEMA || m.kind != KIND {
        return Err(SupplyError::ManifestMalformed(format!(
            "schema {} kind {:?}, expected schema {SCHEMA} kind {KIND:?}",
            m.schema, m.kind
        )));
    }
    Ok(m)
}

/// Read the manifest the config names and decide whether to trust it.
///
/// # Errors
/// The typed reason.
pub fn load_manifest(cfg: &SupplyConfig) -> Result<Manifest, SupplyError> {
    let text = std::fs::read_to_string(&cfg.manifest)
        .map_err(|e| SupplyError::ManifestUnreadable(format!("{}: {e}", cfg.manifest.display())))?;
    let value: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| SupplyError::ManifestMalformed(e.to_string()))?;
    let signed = value.get("signature").is_some() || value.get("payload").is_some();
    if !signed {
        if cfg.allow_unsigned {
            return parse_payload(&text);
        }
        return Err(SupplyError::ManifestUnsigned);
    }
    let envelope: SignedManifest = serde_json::from_value(value)
        .map_err(|e| SupplyError::ManifestMalformed(e.to_string()))?;
    let mut keys = cfg.keys.clone();
    if let Some(p) = &cfg.keys_file {
        let t = std::fs::read_to_string(p)
            .map_err(|e| SupplyError::KeysUnreadable(format!("{}: {e}", p.display())))?;
        let more: Vec<PinnedKey> =
            serde_json::from_str(&t).map_err(|e| SupplyError::KeysUnreadable(e.to_string()))?;
        keys.extend(more);
    }
    verify_manifest(&envelope, &keys)
}

// ------------------------------------------------------------------ versions

fn parse_version(v: &str) -> Option<([u64; 3], Option<String>)> {
    let (core, pre) = match v.split_once('-') {
        Some((c, p)) => (c, Some(p.to_owned())),
        None => (v, None),
    };
    let mut it = core.split('.');
    let n = [
        it.next()?.parse().ok()?,
        it.next()?.parse().ok()?,
        it.next()?.parse().ok()?,
    ];
    it.next().is_none().then_some((n, pre))
}

/// `MAJOR.MINOR.PATCH[-pre]` ordering; a release outranks its pre-release.
/// An unparseable version sorts lowest.
#[must_use]
pub fn compare_versions(a: &str, b: &str) -> std::cmp::Ordering {
    use std::cmp::Ordering::{Equal, Greater, Less};
    match (parse_version(a), parse_version(b)) {
        (None, None) => Equal,
        (None, Some(_)) => Less,
        (Some(_), None) => Greater,
        (Some((x, xp)), Some((y, yp))) => x.cmp(&y).then_with(|| match (xp, yp) {
            (None, None) => Equal,
            (None, Some(_)) => Greater,
            (Some(_), None) => Less,
            (Some(p), Some(q)) => p.cmp(&q),
        }),
    }
}

// ------------------------------------------------------------------ platform

/// This operating system's name in the manifest.
#[must_use]
pub fn this_platform() -> &'static str {
    if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else {
        "linux"
    }
}

/// This architecture's name in the manifest.
#[must_use]
pub fn this_arch() -> &'static str {
    if cfg!(target_arch = "aarch64") {
        "arm64"
    } else {
        "x86_64"
    }
}

// ------------------------------------------------------------------ verification

fn sha256_file(p: &Path) -> std::io::Result<(String, u64)> {
    use std::io::Read;
    let mut f = std::fs::File::open(p)?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 16];
    let mut n = 0u64;
    loop {
        let r = f.read(&mut buf)?;
        if r == 0 {
            break;
        }
        h.update(&buf[..r]);
        n += r as u64;
    }
    Ok((hex::encode(h.finalize()), n))
}

fn resolve_inside(dir: &Path, rel: &str) -> Result<PathBuf, Reject> {
    let p = Path::new(rel);
    if p.is_absolute()
        || p.components().any(|c| {
            matches!(
                c,
                std::path::Component::ParentDir
                    | std::path::Component::RootDir
                    | std::path::Component::Prefix(_)
            )
        })
    {
        return Err(Reject::PathEscapes(format!("{rel:?} is not a path inside the bundle")));
    }
    let root = std::fs::canonicalize(dir)
        .map_err(|e| Reject::Missing(format!("{}: {e}", dir.display())))?;
    let full = std::fs::canonicalize(root.join(p))
        .map_err(|e| Reject::Missing(format!("{}: {e}", root.join(p).display())))?;
    if !full.starts_with(&root) {
        return Err(Reject::PathEscapes(format!(
            "{} resolves outside {}",
            full.display(),
            root.display()
        )));
    }
    Ok(full)
}

/// Check one entry NOW, right before it is launched.
///
/// # Errors
/// The first typed reason it must not be launched.
pub fn verify_entry(e: &HelperEntry, dir: &Path, cfg: &SupplyConfig) -> Result<Verified, Reject> {
    if compare_versions(&cfg.core_version, &e.min_core_version) == std::cmp::Ordering::Less {
        return Err(Reject::CoreTooOld {
            needs: e.min_core_version.clone(),
            have: cfg.core_version.clone(),
        });
    }
    if e.protocol_major != crate::actuator::PROTOCOL_MAJOR {
        return Err(Reject::ProtocolMismatch {
            helper: e.protocol_major,
            core: crate::actuator::PROTOCOL_MAJOR,
        });
    }
    let path = resolve_inside(dir, &e.path)?;
    let meta = std::fs::metadata(&path).map_err(|err| Reject::Missing(format!("{}: {err}", path.display())))?;
    if !meta.is_file() {
        return Err(Reject::NotRegularFile(path.display().to_string()));
    }
    if meta.len() != e.size {
        return Err(Reject::SizeMismatch { want: e.size, got: meta.len() });
    }
    let (digest, _) = sha256_file(&path).map_err(|err| Reject::Missing(format!("{}: {err}", path.display())))?;
    if !digest.eq_ignore_ascii_case(&e.sha256) {
        return Err(Reject::DigestMismatch { want: e.sha256.to_ascii_lowercase(), got: digest });
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if meta.permissions().mode() & 0o111 == 0 {
            return Err(Reject::NotExecutable(path.display().to_string()));
        }
    }
    if e.require_signature {
        codesign_verify(&path, e.designated_requirement.as_deref())?;
    }
    Ok(Verified {
        version: e.version.clone(),
        path,
        sha256: digest,
        protocol_major: e.protocol_major,
    })
}

/// `codesign --verify --strict [-R=<requirement>] <path>` (macOS). Fails
/// closed: where a signature is required and cannot be checked, the helper is
/// not launched.
fn codesign_verify(path: &Path, requirement: Option<&str>) -> Result<(), Reject> {
    if !cfg!(target_os = "macos") {
        return Err(Reject::CodesignUnavailable(
            "a code signature is required and can be checked only on macOS".into(),
        ));
    }
    let mut cmd = Command::new("/usr/bin/codesign");
    cmd.args(["--verify", "--strict"]);
    if let Some(r) = requirement {
        cmd.arg("-R").arg(format!("={r}"));
    }
    cmd.arg(path);
    let out = cmd
        .output()
        .map_err(|e| Reject::CodesignUnavailable(format!("/usr/bin/codesign: {e}")))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(Reject::SignatureCheckFailed(
            String::from_utf8_lossy(&out.stderr).trim().to_owned(),
        ))
    }
}

/// The entries to try, newest first, following `rollback_to`. Nothing is
/// verified here; each is verified just before it is launched.
///
/// # Errors
/// No entry for this platform.
pub fn candidates(m: &Manifest) -> Result<Vec<HelperEntry>, SupplyError> {
    let fits = |e: &&HelperEntry| {
        e.platform == this_platform() && (e.arch == this_arch() || e.arch == "universal")
    };
    let mut mine: Vec<&HelperEntry> = m.helpers.iter().filter(fits).collect();
    if mine.is_empty() {
        return Err(SupplyError::NoEntry(format!(
            "the manifest has no helper for {}/{}",
            this_platform(),
            this_arch()
        )));
    }
    mine.sort_by(|a, b| compare_versions(&b.version, &a.version));
    // The newest entry starts the chain; `rollback_to` names each next one.
    let mut chain: Vec<HelperEntry> = Vec::new();
    let mut cur = Some(mine[0]);
    while let Some(e) = cur {
        if chain.iter().any(|c| c.version == e.version) {
            break;
        }
        chain.push(e.clone());
        cur = e
            .rollback_to
            .as_deref()
            .and_then(|v| mine.iter().copied().find(|x| x.version == v));
    }
    Ok(chain)
}

/// The directory a manifest's relative paths are inside.
#[must_use]
pub fn bundle_dir(cfg: &SupplyConfig) -> PathBuf {
    cfg.manifest
        .parent()
        .map_or_else(|| PathBuf::from("."), Path::to_path_buf)
}

/// Verify what a bundle ships, without launching it (the entry point of the
/// application updater and of the release gate): the manifest is trusted and
/// every entry for this platform is checked.
///
/// # Errors
/// The manifest's trust failure; per-entry results are in the `Ok`.
pub fn audit(cfg: &SupplyConfig) -> Result<Vec<(String, Result<Verified, Reject>)>, SupplyError> {
    let m = load_manifest(cfg)?;
    let dir = bundle_dir(cfg);
    Ok(m.helpers
        .iter()
        .filter(|e| e.platform == this_platform())
        .map(|e| (e.version.clone(), verify_entry(e, &dir, cfg)))
        .collect())
}

// ------------------------------------------------------------------ process identity

/// The executable path of a running process, where the platform can say.
#[must_use]
pub fn process_exe(pid: u32) -> Option<PathBuf> {
    #[cfg(target_os = "linux")]
    {
        std::fs::read_link(format!("/proc/{pid}/exe")).ok()
    }
    #[cfg(target_os = "macos")]
    {
        // `ps -o comm=` prints the executable's full path on macOS.
        let out = Command::new("/bin/ps")
            .args(["-p", &pid.to_string(), "-o", "comm="])
            .output()
            .ok()?;
        let s = String::from_utf8_lossy(&out.stdout).trim().to_owned();
        (out.status.success() && !s.is_empty()).then(|| PathBuf::from(s))
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        let _ = pid;
        None
    }
}

/// The running process `pid` is the verified helper: same executable path,
/// and the file at that path still has the pinned digest.
///
/// # Errors
/// What differs.
pub fn check_process(pid: u32, v: &Verified) -> Result<(), Reject> {
    let Some(exe) = process_exe(pid) else {
        return Err(Reject::ProcessIdentity(format!(
            "the executable of process {pid} cannot be determined"
        )));
    };
    let exe = std::fs::canonicalize(&exe).unwrap_or(exe);
    if exe != v.path {
        return Err(Reject::ProcessIdentity(format!(
            "process {pid} runs {}, the manifest pins {}",
            exe.display(),
            v.path.display()
        )));
    }
    let (digest, _) = sha256_file(&exe)
        .map_err(|e| Reject::ProcessIdentity(format!("{}: {e}", exe.display())))?;
    if digest != v.sha256 {
        return Err(Reject::ProcessIdentity(format!(
            "{} now hashes to {digest}, the manifest pins {}",
            exe.display(),
            v.sha256
        )));
    }
    Ok(())
}

/// Signal a process only if its executable is the expected one (CUC-E03).
/// Returns whether the signal was sent.
///
/// # Errors
/// The identity does not match, or the signal could not be sent.
pub fn terminate_verified(pid: u32, expected: &Path) -> Result<(), Reject> {
    let want = std::fs::canonicalize(expected).unwrap_or_else(|_| expected.to_path_buf());
    let Some(exe) = process_exe(pid) else {
        return Err(Reject::ProcessIdentity(format!(
            "the executable of process {pid} cannot be determined; not signalled"
        )));
    };
    let exe = std::fs::canonicalize(&exe).unwrap_or(exe);
    if exe != want {
        return Err(Reject::ProcessIdentity(format!(
            "process {pid} runs {}, not {}; not signalled",
            exe.display(),
            want.display()
        )));
    }
    #[cfg(unix)]
    {
        let st = Command::new("/bin/kill")
            .args(["-TERM", &pid.to_string()])
            .status()
            .map_err(|e| Reject::ProcessIdentity(format!("kill: {e}")))?;
        if st.success() {
            Ok(())
        } else {
            Err(Reject::ProcessIdentity(format!("kill -TERM {pid} failed")))
        }
    }
    #[cfg(not(unix))]
    {
        Err(Reject::ProcessIdentity("signalling by pid is not supported here".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cmp::Ordering;

    fn key(seed: u8) -> SigningKey {
        SigningKey::from_bytes(&[seed; 32])
    }

    fn entry(version: &str, path: &str) -> HelperEntry {
        HelperEntry {
            id: "modbit-actuator-macos".into(),
            version: version.into(),
            path: path.into(),
            sha256: "0".repeat(64),
            size: 0,
            platform: this_platform().into(),
            arch: "universal".into(),
            min_core_version: "0.0.0".into(),
            protocol_major: crate::actuator::PROTOCOL_MAJOR,
            require_signature: false,
            designated_requirement: None,
            rollback_to: None,
        }
    }

    fn manifest(h: Vec<HelperEntry>) -> Manifest {
        Manifest { schema: SCHEMA, kind: KIND.into(), helpers: h, sbom: vec![] }
    }

    #[test]
    fn versions_order_like_semver_and_a_release_outranks_its_prerelease() {
        assert_eq!(compare_versions("1.2.3", "1.2.10"), Ordering::Less);
        assert_eq!(compare_versions("2.0.0", "1.99.99"), Ordering::Greater);
        assert_eq!(compare_versions("1.0.0", "1.0.0-rc1"), Ordering::Greater);
        assert_eq!(compare_versions("1.0.0-rc1", "1.0.0-rc2"), Ordering::Less);
        assert_eq!(compare_versions("garbage", "0.0.1"), Ordering::Less);
    }

    #[test]
    fn a_signed_manifest_verifies_and_every_tamper_or_trust_failure_is_typed() {
        let sk = key(7);
        let pinned = vec![pinned_for(&sk)];
        let m = manifest(vec![entry("1.0.0", "h")]);
        let signed = sign_manifest(&m, &sk);
        assert_eq!(verify_manifest(&signed, &pinned).unwrap(), m);
        // payload edited after signing
        let mut t = signed.clone();
        t.payload = t.payload.replace("1.0.0", "9.9.9");
        assert_eq!(verify_manifest(&t, &pinned), Err(SupplyError::SignatureInvalid));
        // signed by a key that is not pinned
        let other = sign_manifest(&m, &key(8));
        assert!(matches!(verify_manifest(&other, &pinned), Err(SupplyError::SignatureUnknownKey(_))));
        // revoked
        let mut revoked = pinned.clone();
        revoked[0].revoked = true;
        assert!(matches!(verify_manifest(&signed, &revoked), Err(SupplyError::SignatureKeyRevoked(_))));
        // a pinned key whose declared id lies about the key
        let mut liar = pinned.clone();
        liar[0].public_key_pem = pem_of(&key(9).verifying_key().to_bytes());
        assert!(matches!(verify_manifest(&signed, &liar), Err(SupplyError::KeysUnreadable(_))));
        // a wrong kind
        let mut bad = m.clone();
        bad.kind = "something-else".into();
        assert!(matches!(verify_manifest(&sign_manifest(&bad, &sk), &pinned), Err(SupplyError::ManifestMalformed(_))));
    }

    #[test]
    fn the_key_id_rule_is_the_update_manifests() {
        // sha256 of the SPKI DER, first 16 hex characters (manifest.ts keyIdOf).
        let sk = key(1);
        let id = key_id_of(&sk.verifying_key().to_bytes());
        assert_eq!(id.len(), 16);
        assert!(id.chars().all(|c| c.is_ascii_hexdigit()));
        assert_eq!(pinned_for(&sk).key_id, id);
        assert!(pem_of(&sk.verifying_key().to_bytes()).starts_with("-----BEGIN PUBLIC KEY-----\nMCowBQYDK2VwAyEA"));
    }

    #[test]
    fn candidates_follow_the_rollback_chain_and_stop_at_a_loop() {
        let mut a = entry("3.0.0", "a");
        let mut b = entry("2.0.0", "b");
        let mut c = entry("1.0.0", "c");
        a.rollback_to = Some("2.0.0".into());
        b.rollback_to = Some("1.0.0".into());
        c.rollback_to = Some("3.0.0".into()); // a loop
        let chain = candidates(&manifest(vec![c.clone(), a.clone(), b.clone()])).unwrap();
        assert_eq!(chain.iter().map(|e| e.version.as_str()).collect::<Vec<_>>(), ["3.0.0", "2.0.0", "1.0.0"]);
        // An entry that is not on the newest entry's chain is never launched.
        a.rollback_to = None;
        let chain = candidates(&manifest(vec![a, b, c])).unwrap();
        assert_eq!(chain.len(), 1);
        let none = candidates(&manifest(vec![]));
        assert!(matches!(none, Err(SupplyError::NoEntry(_))));
    }

    #[test]
    fn a_path_may_not_leave_the_bundle() {
        let d = tempfile::tempdir().unwrap();
        assert!(matches!(resolve_inside(d.path(), "../x"), Err(Reject::PathEscapes(_))));
        assert!(matches!(resolve_inside(d.path(), "/bin/sh"), Err(Reject::PathEscapes(_))));
        assert!(matches!(resolve_inside(d.path(), "a/../../x"), Err(Reject::PathEscapes(_))));
        assert!(matches!(resolve_inside(d.path(), "missing"), Err(Reject::Missing(_))));
        std::fs::write(d.path().join("ok"), b"x").unwrap();
        assert!(resolve_inside(d.path(), "ok").is_ok());
        #[cfg(unix)]
        {
            let outside = tempfile::tempdir().unwrap();
            std::fs::write(outside.path().join("evil"), b"x").unwrap();
            std::os::unix::fs::symlink(outside.path().join("evil"), d.path().join("link")).unwrap();
            assert!(matches!(resolve_inside(d.path(), "link"), Err(Reject::PathEscapes(_))));
        }
    }

    #[test]
    fn size_digest_core_and_protocol_are_each_checked() {
        let d = tempfile::tempdir().unwrap();
        let f = d.path().join("helper");
        std::fs::write(&f, b"#!/bin/sh\nexit 0\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let (digest, size) = sha256_file(&f).unwrap();
        let cfg = SupplyConfig { manifest: d.path().join("m.json"), keys_file: None, keys: vec![], allow_unsigned: true, core_version: "1.0.0".into() };
        let mut e = entry("1.0.0", "helper");
        e.sha256 = digest.clone();
        e.size = size;
        assert!(verify_entry(&e, d.path(), &cfg).is_ok());
        let mut x = e.clone();
        x.size += 1;
        assert!(matches!(verify_entry(&x, d.path(), &cfg), Err(Reject::SizeMismatch { .. })));
        let mut x = e.clone();
        x.sha256 = "ab".repeat(32);
        assert!(matches!(verify_entry(&x, d.path(), &cfg), Err(Reject::DigestMismatch { .. })));
        let mut x = e.clone();
        x.min_core_version = "2.0.0".into();
        assert!(matches!(verify_entry(&x, d.path(), &cfg), Err(Reject::CoreTooOld { .. })));
        let mut x = e.clone();
        x.protocol_major = 2;
        assert!(matches!(verify_entry(&x, d.path(), &cfg), Err(Reject::ProtocolMismatch { .. })));
        // same size, one byte different: the digest, not the size, catches it
        std::fs::write(&f, b"#!/bin/sh\nexit 1\n").unwrap();
        assert!(matches!(verify_entry(&e, d.path(), &cfg), Err(Reject::DigestMismatch { .. })));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::write(&f, b"#!/bin/sh\nexit 0\n").unwrap();
            std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o644)).unwrap();
            assert!(matches!(verify_entry(&e, d.path(), &cfg), Err(Reject::NotExecutable(_))));
        }
    }

    #[test]
    fn a_required_signature_that_cannot_be_checked_fails_closed() {
        if cfg!(target_os = "macos") {
            return;
        }
        let r = codesign_verify(Path::new("/x"), None);
        assert!(matches!(r, Err(Reject::CodesignUnavailable(_))));
    }

    #[test]
    fn manifests_without_a_signature_need_an_explicit_developer_opt_in() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("actuator-manifest.json");
        std::fs::write(&p, serde_json::to_string(&manifest(vec![entry("1.0.0", "h")])).unwrap()).unwrap();
        let mut cfg = SupplyConfig { manifest: p, keys_file: None, keys: vec![], allow_unsigned: false, core_version: "1.0.0".into() };
        assert_eq!(load_manifest(&cfg), Err(SupplyError::ManifestUnsigned));
        cfg.allow_unsigned = true;
        assert!(load_manifest(&cfg).is_ok());
        // An envelope is never treated as an unsigned manifest.
        let sk = key(3);
        let env = sign_manifest(&manifest(vec![entry("1.0.0", "h")]), &sk);
        std::fs::write(&cfg.manifest, serde_json::to_string(&env).unwrap()).unwrap();
        assert!(matches!(load_manifest(&cfg), Err(SupplyError::SignatureUnknownKey(_))));
        cfg.keys = vec![pinned_for(&sk)];
        assert!(load_manifest(&cfg).is_ok());
    }
}
