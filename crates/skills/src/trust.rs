//! Owner trust for skills (REQ-PX-105, docs/79 ADC-A08): how a skill a
//! person wrote, or imported, becomes allowed to reach a model without a
//! signing key.
//!
//! A trust decision is `name@content-hash`, held per profile in
//! `<profile>/skills/trusted.json`. It binds the exact bytes the owner
//! reviewed: the content hash covers every file of the package, so one
//! changed byte is a different hash and the skill is untrusted again. Only
//! the owner's command writes it — never a skill's own text, never an
//! environment flag.
//!
//! A System scope holds skills an administrator provisions (above the
//! user's and every project's, never shadowed) and the right to forbid: a
//! `forbidden.json` there names skills (`name`, or `name@hash`) that no
//! scope may enable, whatever the owner trusted.
//!
//! Reads fail closed: a trust file that cannot be read or parsed trusts
//! nothing; a forbidden list that cannot be read forbids nothing it cannot
//! name but is reported, so the inventory says why.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::SkillError;

/// The profile's trust file, under `<profile>/skills/`.
pub const TRUST_FILE: &str = "trusted.json";
/// The System scope's prohibitions, under the system skills root.
pub const FORBIDDEN_FILE: &str = "forbidden.json";

/// One owner decision.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrustRecord {
    /// Skill name.
    pub name: String,
    /// The content hash the owner reviewed.
    pub content_hash: String,
    /// When the decision was made.
    pub decided_at_ms: i64,
    /// Who decided (the client that carried the command).
    #[serde(default)]
    pub decided_by: String,
}

/// What the registry needs to decide trust: the owner's decisions and the
/// System scope's prohibitions.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TrustContext {
    /// `(name, content hash)` the owner trusts.
    pub owner: BTreeSet<(String, String)>,
    /// `name` or `name@hash` the System scope forbids.
    pub forbidden: BTreeSet<String>,
}

impl TrustContext {
    /// Whether the owner trusts exactly this content.
    #[must_use]
    pub fn is_trusted(&self, name: &str, hash: &str) -> bool {
        self.owner.contains(&(name.to_owned(), hash.to_owned()))
    }

    /// Whether the owner once trusted this name at some other content.
    #[must_use]
    pub fn trusted_other_content(&self, name: &str, hash: &str) -> bool {
        self.owner.iter().any(|(n, h)| n == name && h != hash)
    }

    /// Whether the System scope forbids this skill.
    #[must_use]
    pub fn is_forbidden(&self, name: &str, hash: &str) -> bool {
        self.forbidden.contains(name) || self.forbidden.contains(&format!("{name}@{hash}"))
    }

    /// Read both files. A file that cannot be read or parsed is a problem
    /// (returned, so the inventory can say why) and contributes nothing.
    #[must_use]
    pub fn load(
        profile_skills: &Path,
        system_root: Option<&Path>,
    ) -> (Self, Vec<(String, String)>) {
        let mut ctx = Self::default();
        let mut problems = Vec::new();
        let trust_path = profile_skills.join(TRUST_FILE);
        match read_records(&trust_path) {
            Ok(records) => {
                ctx.owner = records
                    .into_iter()
                    .map(|r| (r.name, r.content_hash))
                    .collect();
            }
            Err(e) => problems.push((trust_path.display().to_string(), e.to_string())),
        }
        if let Some(root) = system_root {
            let path = root.join(FORBIDDEN_FILE);
            match read_names(&path) {
                Ok(names) => ctx.forbidden = names,
                Err(e) => problems.push((path.display().to_string(), e.to_string())),
            }
        }
        (ctx, problems)
    }
}

fn io(path: &Path, e: impl std::fmt::Display) -> SkillError {
    SkillError::Io {
        path: path.display().to_string(),
        detail: e.to_string(),
    }
}

/// The owner's decisions in `path` (none when the file does not exist).
///
/// # Errors
/// The file exists and cannot be read or parsed.
pub fn read_records(path: &Path) -> Result<Vec<TrustRecord>, SkillError> {
    match std::fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(|e| io(path, e)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(vec![]),
        Err(e) => Err(io(path, e)),
    }
}

/// A list of names (`name` or `name@hash`) in `path` (none when absent).
///
/// # Errors
/// The file exists and cannot be read or parsed.
pub fn read_names(path: &Path) -> Result<BTreeSet<String>, SkillError> {
    match std::fs::read(path) {
        Ok(bytes) => serde_json::from_slice::<Vec<String>>(&bytes)
            .map(|v| v.into_iter().collect())
            .map_err(|e| io(path, e)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(BTreeSet::new()),
        Err(e) => Err(io(path, e)),
    }
}

/// A skill name is `[a-z0-9-]+`, the manifest's rule.
fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// A content hash is 64 lowercase hex digits.
#[must_use]
pub fn valid_hash(hash: &str) -> bool {
    hash.len() == 64
        && hash
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// Write `records` atomically: a temporary file in the same directory,
/// synced, then renamed over the trust file, so a crash leaves the old
/// decisions or the new ones, never half of either.
fn write_records(dir: &Path, records: &[TrustRecord]) -> Result<(), SkillError> {
    use std::io::Write;
    std::fs::create_dir_all(dir).map_err(|e| io(dir, e))?;
    let path = dir.join(TRUST_FILE);
    let tmp: PathBuf = dir.join(format!("{TRUST_FILE}.tmp"));
    let bytes = serde_json::to_vec_pretty(records).map_err(|e| io(&path, e))?;
    {
        let mut f = std::fs::File::create(&tmp).map_err(|e| io(&tmp, e))?;
        f.write_all(&bytes).map_err(|e| io(&tmp, e))?;
        f.sync_all().map_err(|e| io(&tmp, e))?;
    }
    std::fs::rename(&tmp, &path).map_err(|e| io(&path, e))
}

/// Record the owner's decision that `name` at `content_hash` may reach a
/// model. Idempotent: the same decision twice leaves one record. Returns the
/// number of decisions held for the name afterwards.
///
/// # Errors
/// A malformed name or hash, or the file cannot be read or written.
pub fn record_trust(
    profile_skills: &Path,
    name: &str,
    content_hash: &str,
    now_ms: i64,
    decided_by: &str,
) -> Result<usize, SkillError> {
    if !valid_name(name) {
        return Err(SkillError::MalformedManifest {
            detail: format!("skill name `{name}` is not [a-z0-9-]+"),
        });
    }
    if !valid_hash(content_hash) {
        return Err(SkillError::MalformedManifest {
            detail: "a content hash is 64 lowercase hex digits".into(),
        });
    }
    let mut records = read_records(&profile_skills.join(TRUST_FILE))?;
    if !records
        .iter()
        .any(|r| r.name == name && r.content_hash == content_hash)
    {
        records.push(TrustRecord {
            name: name.to_owned(),
            content_hash: content_hash.to_owned(),
            decided_at_ms: now_ms,
            decided_by: decided_by.to_owned(),
        });
        records.sort_by(|a, b| (&a.name, &a.content_hash).cmp(&(&b.name, &b.content_hash)));
        write_records(profile_skills, &records)?;
    }
    Ok(records.iter().filter(|r| r.name == name).count())
}

/// Withdraw trust: one hash of the name, or every hash when `content_hash`
/// is `None`. Returns how many decisions were removed.
///
/// # Errors
/// The file cannot be read or written.
pub fn withdraw_trust(
    profile_skills: &Path,
    name: &str,
    content_hash: Option<&str>,
) -> Result<usize, SkillError> {
    let mut records = read_records(&profile_skills.join(TRUST_FILE))?;
    let before = records.len();
    records.retain(|r| !(r.name == name && content_hash.is_none_or(|h| r.content_hash == h)));
    let removed = before - records.len();
    if removed > 0 {
        write_records(profile_skills, &records)?;
    }
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;

    const H: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    #[test]
    fn a_decision_is_bound_to_its_hash_and_withdrawable() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            record_trust(dir.path(), "lint-fix", H, 1, "cli").unwrap(),
            1
        );
        assert_eq!(
            record_trust(dir.path(), "lint-fix", H, 2, "cli").unwrap(),
            1
        );
        let (ctx, problems) = TrustContext::load(dir.path(), None);
        assert!(problems.is_empty());
        assert!(ctx.is_trusted("lint-fix", H));
        let other = "f".repeat(64);
        assert!(
            !ctx.is_trusted("lint-fix", &other),
            "another hash is another skill"
        );
        assert!(ctx.trusted_other_content("lint-fix", &other));
        assert!(!ctx.trusted_other_content("lint-fix", H));
        assert_eq!(withdraw_trust(dir.path(), "lint-fix", Some(H)).unwrap(), 1);
        assert!(
            !TrustContext::load(dir.path(), None)
                .0
                .is_trusted("lint-fix", H)
        );
    }

    #[test]
    fn a_malformed_decision_is_refused_and_an_unreadable_file_trusts_nothing() {
        let dir = tempfile::tempdir().unwrap();
        assert!(record_trust(dir.path(), "Bad Name", H, 1, "cli").is_err());
        assert!(record_trust(dir.path(), "ok", "abc", 1, "cli").is_err());
        std::fs::write(dir.path().join(TRUST_FILE), "{ not json").unwrap();
        let (ctx, problems) = TrustContext::load(dir.path(), None);
        assert!(ctx.owner.is_empty());
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(
            record_trust(dir.path(), "ok", H, 1, "cli").is_err(),
            "a damaged file is not overwritten"
        );
    }

    #[test]
    fn the_system_scope_forbids_by_name_or_by_content() {
        let sys = tempfile::tempdir().unwrap();
        std::fs::write(
            sys.path().join(FORBIDDEN_FILE),
            serde_json::to_vec(&["rm-everything", &format!("pinned@{H}")]).unwrap(),
        )
        .unwrap();
        let profile = tempfile::tempdir().unwrap();
        let (ctx, problems) = TrustContext::load(profile.path(), Some(sys.path()));
        assert!(problems.is_empty());
        assert!(ctx.is_forbidden("rm-everything", H));
        assert!(ctx.is_forbidden("pinned", H));
        assert!(!ctx.is_forbidden("pinned", &"e".repeat(64)));
        assert!(!ctx.is_forbidden("other", H));
    }
}
