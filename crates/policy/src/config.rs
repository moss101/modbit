//! Typed `ConfigurationResolver` (REQ-EV-0039, docs/23): merges admin,
//! project and user configuration with domain-specific merge laws and records
//! provenance for every resolved decision. Lower authority can narrow but
//! never widen what higher authority decided.
//!
//! Sections and their laws:
//! - `permissions`: capability → Allow | Ask | Deny. Higher authority wins on
//!   conflict; a lower layer may only tighten (Allow→Ask→Deny), never loosen.
//! - `network`: egress allow-list of hosts. Effective = intersection of every
//!   layer that states one (a lower layer cannot add hosts).
//! - `models`: allowed model labels. Same intersection law.
//! - `mcp_servers`: named servers. Lower layers may add servers unless a
//!   higher layer denied the name; higher deny always wins.
//! - `hooks` and `rules`: ordered lists, higher authority first; a lower layer
//!   cannot remove or disable a higher layer's entry.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

/// Authority levels, highest first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Authority {
    /// Organization administrator.
    Admin,
    /// Repository / project configuration.
    Project,
    /// User preferences.
    User,
}

impl Authority {
    /// Every level in precedence order.
    pub const ALL: [Authority; 3] = [Authority::Admin, Authority::Project, Authority::User];
}

/// Permission decision, from least to most restrictive.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Permission {
    /// Allowed without asking.
    Allow,
    /// Requires approval.
    Ask,
    /// Denied.
    Deny,
}

/// One layer's declarations. Every field is optional: absent means "no opinion".
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Layer {
    /// Capability id → decision.
    #[serde(default)]
    pub permissions: BTreeMap<String, Permission>,
    /// Egress allow-list; `Some(empty)` means "no egress".
    #[serde(default)]
    pub network_allow: Option<BTreeSet<String>>,
    /// Allowed model labels.
    #[serde(default)]
    pub models_allow: Option<BTreeSet<String>>,
    /// MCP servers this layer adds.
    #[serde(default)]
    pub mcp_servers: BTreeMap<String, String>,
    /// MCP server names this layer denies.
    #[serde(default)]
    pub mcp_deny: BTreeSet<String>,
    /// Hooks (ordered).
    #[serde(default)]
    pub hooks: Vec<String>,
    /// Rules (ordered).
    #[serde(default)]
    pub rules: Vec<String>,
}

/// Where a resolved value came from.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Provenance {
    /// Layer that decided.
    pub decided_by: Authority,
    /// Layers whose contrary opinion was overridden or narrowed, with why.
    pub overridden: Vec<(Authority, String)>,
}

/// A resolved value with provenance.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Resolved<T> {
    /// The value.
    pub value: T,
    /// Provenance.
    pub provenance: Provenance,
}

/// Fully resolved configuration.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedConfig {
    /// Permissions.
    pub permissions: BTreeMap<String, Resolved<Permission>>,
    /// Egress allow-list (`None` = no layer restricted egress).
    pub network_allow: Option<Resolved<BTreeSet<String>>>,
    /// Allowed models (`None` = unrestricted).
    pub models_allow: Option<Resolved<BTreeSet<String>>>,
    /// MCP servers by name.
    pub mcp_servers: BTreeMap<String, Resolved<String>>,
    /// Hooks in effective order.
    pub hooks: Vec<Resolved<String>>,
    /// Rules in effective order.
    pub rules: Vec<Resolved<String>>,
    /// Attempts by a lower layer to widen a higher decision (rejected, kept for audit).
    pub rejected_widenings: Vec<String>,
}

/// Resolve layers deterministically. `layers` may omit levels.
#[must_use]
pub fn resolve(layers: &BTreeMap<Authority, Layer>) -> ResolvedConfig {
    let mut out = ResolvedConfig::default();
    for level in Authority::ALL {
        let Some(layer) = layers.get(&level) else {
            continue;
        };
        // permissions: tighten-only for lower layers
        for (cap, decision) in &layer.permissions {
            match out.permissions.get_mut(cap) {
                None => {
                    out.permissions.insert(
                        cap.clone(),
                        Resolved {
                            value: *decision,
                            provenance: Provenance {
                                decided_by: level,
                                overridden: vec![],
                            },
                        },
                    );
                }
                Some(existing) => {
                    if *decision > existing.value {
                        existing.provenance.overridden.push((
                            existing.provenance.decided_by,
                            format!(
                                "{:?} tightened by {level:?} to {decision:?}",
                                existing.value
                            ),
                        ));
                        existing.value = *decision;
                        existing.provenance.decided_by = level;
                    } else if *decision < existing.value {
                        out.rejected_widenings.push(format!("{level:?} tried to widen permission `{cap}` from {:?} to {decision:?} (decided by {:?})", existing.value, existing.provenance.decided_by));
                    }
                }
            }
        }
        // network / models: intersection
        for (field, mine, target) in [
            ("network", &layer.network_allow, &mut out.network_allow),
            ("models", &layer.models_allow, &mut out.models_allow),
        ] {
            let Some(set) = mine else { continue };
            match target {
                None => {
                    *target = Some(Resolved {
                        value: set.clone(),
                        provenance: Provenance {
                            decided_by: level,
                            overridden: vec![],
                        },
                    })
                }
                Some(existing) => {
                    let widened: Vec<_> = set.difference(&existing.value).cloned().collect();
                    if !widened.is_empty() {
                        out.rejected_widenings.push(format!("{level:?} tried to add {field} entries {widened:?} beyond {:?}'s allow-list", existing.provenance.decided_by));
                    }
                    let narrowed: BTreeSet<_> = existing.value.intersection(set).cloned().collect();
                    if narrowed != existing.value {
                        existing.provenance.overridden.push((
                            existing.provenance.decided_by,
                            format!("narrowed by {level:?}"),
                        ));
                        existing.value = narrowed;
                        existing.provenance.decided_by = level;
                    }
                }
            }
        }
        // mcp servers: higher deny wins; lower may add
        for (name, endpoint) in &layer.mcp_servers {
            let denied_by = Authority::ALL
                .iter()
                .copied()
                .take_while(|l| *l < level)
                .find(|l| layers.get(l).is_some_and(|hl| hl.mcp_deny.contains(name)));
            if let Some(d) = denied_by {
                out.rejected_widenings.push(format!(
                    "{level:?} tried to add MCP server `{name}` denied by {d:?}"
                ));
                continue;
            }
            out.mcp_servers.entry(name.clone()).or_insert(Resolved {
                value: endpoint.clone(),
                provenance: Provenance {
                    decided_by: level,
                    overridden: vec![],
                },
            });
        }
        for name in &layer.mcp_deny {
            if let Some(existing) = out.mcp_servers.get(name)
                && existing.provenance.decided_by < level
            {
                out.rejected_widenings.push(format!(
                    "{level:?} tried to remove MCP server `{name}` added by {:?}",
                    existing.provenance.decided_by
                ));
            } else {
                out.mcp_servers.remove(name);
            }
        }
        // hooks / rules: append in authority order
        for h in &layer.hooks {
            out.hooks.push(Resolved {
                value: h.clone(),
                provenance: Provenance {
                    decided_by: level,
                    overridden: vec![],
                },
            });
        }
        for r in &layer.rules {
            out.rules.push(Resolved {
                value: r.clone(),
                provenance: Provenance {
                    decided_by: level,
                    overridden: vec![],
                },
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(items: &[&str]) -> BTreeSet<String> {
        items.iter().map(|s| (*s).to_owned()).collect()
    }

    fn layers() -> BTreeMap<Authority, Layer> {
        let mut m = BTreeMap::new();
        m.insert(
            Authority::Admin,
            Layer {
                permissions: [
                    ("git.push".into(), Permission::Ask),
                    ("fs.write".into(), Permission::Allow),
                    ("secret.use".into(), Permission::Deny),
                ]
                .into(),
                network_allow: Some(set(&["api.github.com", "pypi.org"])),
                models_allow: Some(set(&["m-large", "m-small"])),
                mcp_deny: set(&["shadow-mcp"]),
                hooks: vec!["admin-audit".into()],
                rules: vec!["no-force-push".into()],
                ..Default::default()
            },
        );
        m.insert(
            Authority::Project,
            Layer {
                permissions: [
                    ("git.push".into(), Permission::Deny),
                    ("fs.write".into(), Permission::Ask),
                ]
                .into(),
                network_allow: Some(set(&["api.github.com", "evil.example"])),
                mcp_servers: [("repo-mcp".into(), "unix:///tmp/repo".into())].into(),
                hooks: vec!["project-lint".into()],
                ..Default::default()
            },
        );
        m.insert(
            Authority::User,
            Layer {
                permissions: [
                    ("git.push".into(), Permission::Allow),
                    ("secret.use".into(), Permission::Ask),
                    ("browser.open".into(), Permission::Allow),
                ]
                .into(),
                models_allow: Some(set(&["m-large", "m-huge"])),
                mcp_servers: [
                    ("shadow-mcp".into(), "http://x".into()),
                    ("my-mcp".into(), "unix:///tmp/my".into()),
                ]
                .into(),
                mcp_deny: set(&["repo-mcp"]),
                rules: vec!["prefer-tabs".into()],
                ..Default::default()
            },
        );
        m
    }

    #[test]
    fn lower_authority_narrows_but_never_widens_and_everything_has_provenance() {
        let r = resolve(&layers());
        assert_eq!(
            r.permissions["git.push"].value,
            Permission::Deny,
            "project tightened admin's Ask; user's Allow rejected"
        );
        assert_eq!(
            r.permissions["git.push"].provenance.decided_by,
            Authority::Project
        );
        assert_eq!(r.permissions["fs.write"].value, Permission::Ask);
        assert_eq!(r.permissions["secret.use"].value, Permission::Deny);
        assert_eq!(
            r.permissions["browser.open"].provenance.decided_by,
            Authority::User,
            "new capability may be set by the user"
        );
        assert_eq!(
            r.network_allow.as_ref().unwrap().value,
            set(&["api.github.com"]),
            "intersection; evil.example rejected"
        );
        assert_eq!(r.models_allow.as_ref().unwrap().value, set(&["m-large"]));
        assert!(!r.mcp_servers.contains_key("shadow-mcp"), "admin deny wins");
        assert!(
            r.mcp_servers.contains_key("repo-mcp"),
            "user cannot remove project's server"
        );
        assert!(r.mcp_servers.contains_key("my-mcp"));
        assert_eq!(
            r.hooks.iter().map(|h| h.value.as_str()).collect::<Vec<_>>(),
            ["admin-audit", "project-lint"]
        );
        assert_eq!(
            r.rules.iter().map(|h| h.value.as_str()).collect::<Vec<_>>(),
            ["no-force-push", "prefer-tabs"]
        );
        assert_eq!(r.rejected_widenings.len(), 6, "{:#?}", r.rejected_widenings);
        for w in &r.rejected_widenings {
            assert!(w.contains("tried to"), "{w}");
        }
    }

    #[test]
    fn resolution_is_deterministic_and_order_independent() {
        let a = resolve(&layers());
        let mut reversed = BTreeMap::new();
        for (k, v) in layers().into_iter().rev() {
            reversed.insert(k, v);
        }
        let b = resolve(&reversed);
        assert_eq!(a, b);
        assert_eq!(
            serde_json::to_string(&a).unwrap(),
            serde_json::to_string(&b).unwrap()
        );
    }

    #[test]
    fn missing_layers_and_empty_allow_lists_behave() {
        let mut only_user = BTreeMap::new();
        only_user.insert(
            Authority::User,
            Layer {
                network_allow: Some(BTreeSet::new()),
                permissions: [("fs.write".into(), Permission::Allow)].into(),
                ..Default::default()
            },
        );
        let r = resolve(&only_user);
        assert_eq!(
            r.network_allow.unwrap().value,
            BTreeSet::new(),
            "user may declare no egress for themselves"
        );
        assert_eq!(r.permissions["fs.write"].value, Permission::Allow);
        assert!(r.models_allow.is_none());
        assert!(r.rejected_widenings.is_empty());
    }
}
