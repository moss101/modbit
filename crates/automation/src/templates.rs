//! The reference definitions Modbit ships (AUT-C03): real data a person can
//! copy, validate and enable, not examples in prose. Each is parsed and
//! validated by the same code as any other definition, so a template that
//! stopped being valid fails this crate's tests.

/// One shipped definition: its id, and its document.
pub struct Template {
    /// Stable id (`default-branch-drift`).
    pub id: &'static str,
    /// The definition document.
    pub json: &'static str,
}

/// Every shipped definition.
pub const TEMPLATES: &[Template] = &[Template {
    id: "default-branch-drift",
    json: include_str!("../templates/default-branch-drift.json"),
}];

/// The template with `id`.
#[must_use]
pub fn template(id: &str) -> Option<&'static Template> {
    TEMPLATES.iter().find(|t| t.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::definition::{Effects, parse_and_validate};

    #[test]
    fn every_shipped_definition_is_valid_and_read_only() {
        for t in TEMPLATES {
            let d = parse_and_validate(t.json).unwrap_or_else(|e| panic!("{}: {e:?}", t.id));
            assert_eq!(d.name, t.id);
            assert!(d.profile.is_read_only(), "{} must be read-only", t.id);
            assert_eq!(d.profile.effects, Effects::ReadOnly);
            // Bounded: every shipped definition carries explicit limits.
            assert!(d.limits.max_turns <= 40 && d.limits.deadline_minutes <= 30);
        }
    }

    #[test]
    fn the_drift_report_uses_only_read_only_git_tools() {
        let d = parse_and_validate(template("default-branch-drift").unwrap().json).unwrap();
        assert!(d.prompt.contains("git.status") && d.prompt.contains("git.diff"));
        assert!(d.prompt.contains("Do not fetch"));
        assert!(d.profile.capabilities.is_empty() && d.profile.hosts.is_empty());
    }
}
