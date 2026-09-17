//! Sandbox policy (docs/21 "Sandbox substrate boundary": deny-by-default
//! network, explicit egress by capability, protected filesystem paths,
//! resource quotas; REQ-EV-0287, REQ-EV-0290). A [`SandboxSpec`] says what a
//! task is entitled to; [`compile`] turns it into the [`CompiledPolicy`] the
//! backend configures the substrate with and the guest enforces inside.
//! Everything not granted is refused.

use std::path::{Path, PathBuf};

use modbit_domain::{SessionId, TaskId, TenantId};

/// One permitted egress destination.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct EgressRule {
    /// Host name or address.
    pub host: String,
    /// Port.
    pub port: u16,
    /// The capability that granted it (audit).
    pub capability: String,
}

/// Network policy: nothing unless granted.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct NetworkPolicy {
    /// Permitted destinations; empty = no network interface at all.
    pub egress: Vec<EgressRule>,
}

/// Resource bounds.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Resources {
    /// Virtual CPUs.
    pub vcpus: u32,
    /// Memory in MiB.
    pub memory_mib: u32,
    /// Workspace disk in MiB.
    pub workspace_mib: u32,
    /// Processes at once inside the guest.
    pub max_processes: u32,
    /// Output bytes per exec stream.
    pub max_output_bytes: u64,
    /// Ceiling for one exec.
    pub exec_timeout_ms: u64,
}

impl Default for Resources {
    fn default() -> Self {
        Self {
            vcpus: 1,
            memory_mib: 512,
            workspace_mib: 256,
            max_processes: 64,
            max_output_bytes: 1024 * 1024,
            exec_timeout_ms: 300_000,
        }
    }
}

/// What a task is entitled to inside its sandbox.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SandboxSpec {
    /// Tenant.
    pub tenant_id: TenantId,
    /// Session.
    pub session_id: SessionId,
    /// Task.
    pub task_id: TaskId,
    /// The workspace directory on the gateway's host the sandbox starts
    /// from (its content becomes the guest's `/workspace`).
    pub workspace_source: PathBuf,
    /// Paths (guest-relative, under `/workspace`) the guest may never write.
    pub protected_paths: Vec<String>,
    /// Network grants.
    pub network: NetworkPolicy,
    /// Bounds.
    pub resources: Resources,
}

/// Where the guest's workspace is mounted.
pub const GUEST_WORKSPACE: &str = "/workspace";

/// Paths of the guest's own root that are never written, whatever the
/// spec says (the control surface, the init, the devices the substrate
/// exposes) — docs/21 "protected filesystem paths".
pub const GUEST_CONTROL_PATHS: &[&str] = &["/init", "/etc", "/proc", "/sys", "/dev", "/run/modbit"];

/// The policy a backend configures and a guest enforces.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CompiledPolicy {
    /// The spec it came from.
    pub spec: SandboxSpec,
    /// Guest-side: the writable tree.
    pub workspace_root: String,
    /// Guest-side: never written (absolute guest paths; the control paths
    /// and the spec's protected paths under the workspace).
    pub protected_paths: Vec<String>,
    /// Guest-side: readable beyond the workspace.
    pub readable_roots: Vec<String>,
    /// Substrate-side: whether the guest gets a network interface at all.
    pub network_interface: bool,
    /// Substrate-side: the egress allow-list (empty with no interface).
    pub egress: Vec<EgressRule>,
}

/// Compile a spec. Refuses a protected path that escapes the workspace.
pub fn compile(spec: &SandboxSpec) -> crate::Result<CompiledPolicy> {
    let mut protected: Vec<String> = GUEST_CONTROL_PATHS
        .iter()
        .map(|s| (*s).to_owned())
        .collect();
    for p in &spec.protected_paths {
        let rel = p.trim_start_matches('/');
        if rel.is_empty() || rel.split('/').any(|seg| seg == "..") {
            return Err(crate::SandboxError::Unsupported(format!(
                "protected path `{p}` is not a path under the workspace"
            )));
        }
        protected.push(format!("{GUEST_WORKSPACE}/{rel}"));
    }
    Ok(CompiledPolicy {
        spec: spec.clone(),
        workspace_root: GUEST_WORKSPACE.to_owned(),
        protected_paths: protected,
        readable_roots: vec!["/usr".into(), "/lib".into(), "/bin".into(), "/tmp".into()],
        network_interface: !spec.network.egress.is_empty(),
        egress: spec.network.egress.clone(),
    })
}

impl CompiledPolicy {
    /// The guest-side policy on the wire.
    #[must_use]
    pub fn guest_policy(&self) -> modbit_protocol::v1::GuestPolicy {
        modbit_protocol::v1::GuestPolicy {
            workspace_root: self.workspace_root.clone(),
            protected_paths: self.protected_paths.clone(),
            readable_roots: self.readable_roots.clone(),
            max_output_bytes: self.spec.resources.max_output_bytes,
            max_processes: self.spec.resources.max_processes,
            exec_timeout_ms: self.spec.resources.exec_timeout_ms,
        }
    }
}

/// Guest-side path decision (shared by the guest and by the reference
/// backend's tests): a write is allowed only under the workspace root and
/// never on a protected path or under one; a read is allowed under the
/// workspace and the readable roots. Paths are normalized lexically (no
/// symlink resolution: the guest resolves on its own filesystem before
/// asking).
pub fn write_allowed(
    policy: &modbit_protocol::v1::GuestPolicy,
    path: &str,
) -> std::result::Result<(), (&'static str, String)> {
    let p = normalize(path);
    if !p.starts_with(&format!("{}/", policy.workspace_root.trim_end_matches('/')))
        && p != policy.workspace_root
    {
        return Err((
            "OUTSIDE_WORKSPACE",
            format!("`{path}` is outside {}", policy.workspace_root),
        ));
    }
    for prot in &policy.protected_paths {
        let prot = prot.trim_end_matches('/');
        if p == prot || p.starts_with(&format!("{prot}/")) {
            return Err(("PROTECTED_PATH", format!("`{path}` is protected ({prot})")));
        }
    }
    Ok(())
}

/// Read decision.
pub fn read_allowed(
    policy: &modbit_protocol::v1::GuestPolicy,
    path: &str,
) -> std::result::Result<(), (&'static str, String)> {
    let p = normalize(path);
    let mut roots: Vec<&str> = vec![policy.workspace_root.as_str()];
    roots.extend(policy.readable_roots.iter().map(String::as_str));
    if roots.iter().any(|r| {
        let r = r.trim_end_matches('/');
        p == r || p.starts_with(&format!("{r}/"))
    }) {
        Ok(())
    } else {
        Err(("OUTSIDE_WORKSPACE", format!("`{path}` is not readable")))
    }
}

/// Lexical normalization: absolute, no `.`/`..`, single slashes.
#[must_use]
pub fn normalize(path: &str) -> String {
    let mut out: Vec<&str> = Vec::new();
    for seg in path.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                out.pop();
            }
            s => out.push(s),
        }
    }
    format!("/{}", out.join("/"))
}

/// Host-side helper: a guest path under the workspace mapped onto the
/// host directory that backs it (the reference backend).
#[must_use]
pub fn map_to_host(
    workspace_host: &Path,
    guest_workspace: &str,
    guest_path: &str,
) -> Option<PathBuf> {
    let p = normalize(guest_path);
    let rel = p.strip_prefix(guest_workspace.trim_end_matches('/'))?;
    Some(workspace_host.join(rel.trim_start_matches('/')))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> SandboxSpec {
        SandboxSpec {
            tenant_id: TenantId::from_bytes([1; 16]),
            session_id: SessionId::from_bytes([2; 16]),
            task_id: TaskId::from_bytes([3; 16]),
            workspace_source: PathBuf::from("/tmp/ws"),
            protected_paths: vec![".git/hooks".into(), "secrets".into()],
            network: NetworkPolicy::default(),
            resources: Resources::default(),
        }
    }

    #[test]
    fn a_policy_has_no_network_unless_granted_and_protects_control_paths() {
        let c = compile(&spec()).unwrap();
        assert!(!c.network_interface);
        let g = c.guest_policy();
        assert!(write_allowed(&g, "/workspace/src/main.rs").is_ok());
        assert_eq!(
            write_allowed(&g, "/workspace/.git/hooks/pre-commit")
                .unwrap_err()
                .0,
            "PROTECTED_PATH"
        );
        assert_eq!(
            write_allowed(&g, "/workspace/secrets").unwrap_err().0,
            "PROTECTED_PATH"
        );
        assert_eq!(
            write_allowed(&g, "/workspace/../etc/passwd").unwrap_err().0,
            "OUTSIDE_WORKSPACE"
        );
        assert_eq!(
            write_allowed(&g, "/init").unwrap_err().0,
            "OUTSIDE_WORKSPACE"
        );
        assert_eq!(
            write_allowed(&g, "/etc/hostname").unwrap_err().0,
            "OUTSIDE_WORKSPACE"
        );
        assert!(read_allowed(&g, "/usr/bin/sh").is_ok());
        assert_eq!(
            read_allowed(&g, "/root/.ssh/id_ed25519").unwrap_err().0,
            "OUTSIDE_WORKSPACE"
        );
        let mut s = spec();
        s.network.egress.push(EgressRule {
            host: "api.github.com".into(),
            port: 443,
            capability: "forge.read".into(),
        });
        assert!(compile(&s).unwrap().network_interface);
        let mut bad = spec();
        bad.protected_paths.push("../x".into());
        assert!(compile(&bad).is_err());
    }

    #[test]
    fn paths_normalize_lexically_and_map_onto_the_host() {
        assert_eq!(normalize("/workspace//a/./b/../c"), "/workspace/a/c");
        assert_eq!(normalize("../../etc"), "/etc");
        assert_eq!(
            map_to_host(Path::new("/host/ws"), "/workspace", "/workspace/a/b"),
            Some(PathBuf::from("/host/ws/a/b"))
        );
        assert_eq!(
            map_to_host(Path::new("/host/ws"), "/workspace", "/etc/passwd"),
            None
        );
    }
}
