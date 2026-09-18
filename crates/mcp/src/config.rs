//! What the host knows about an MCP server, and the key a transport to it
//! may be shared under (REQ-EV-0193 "workspace-scoped MCP transport pool":
//! share healthy transports by normalized config fingerprint while
//! isolating sessions and tenants).
//!
//! A configuration is host state. It is written by the resolved
//! configuration layers (`modbit-policy::config`, where a project layer may
//! add a server and an admin layer may deny one) or proposed through the
//! conversational path — never by a server, and never by the model on its
//! own authority.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// How the host reaches a server. Only the stdio transport is built: the
/// host starts the server as a child process and speaks newline-delimited
/// JSON-RPC over its pipes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Transport {
    /// A child process the host starts.
    Stdio {
        /// Program.
        command: String,
        /// Arguments.
        #[serde(default)]
        args: Vec<String>,
    },
}

/// Whether a configured server may be reached at all (REQ-EV-0224: a
/// proposal is inert until the host trusts it).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Trust {
    /// Proposed by a client or an agent; never started, never listed to the
    /// model, never callable.
    #[default]
    Proposed,
    /// A person or an admin layer trusted it; the hub may start it.
    Trusted,
}

/// One configured MCP server.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerConfig {
    /// Normalized name; the tools it declares live under
    /// `external.<name>.<tool>`.
    pub name: String,
    /// How to reach it.
    pub transport: Transport,
    /// Non-secret environment the child process gets.
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    /// The name of a credential in the host's broker, when the server needs
    /// one. Never a value: the host resolves the handle at connect time and
    /// the value never enters this structure, a log, a projection or an
    /// argument.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credential: Option<String>,
    /// Scopes the host grants this server (display and audit; the kernel
    /// decides authority).
    #[serde(default)]
    pub scopes: BTreeSet<String>,
    /// Tools the host declares are reads. Only this can make a call a read;
    /// a server saying so about itself cannot (`docs/16`: server content is
    /// not authority).
    #[serde(default)]
    pub read_only_tools: BTreeSet<String>,
    /// Trust state.
    #[serde(default)]
    pub trust: Trust,
    /// Which configuration layer put it here (`admin`, `project`, `user`,
    /// `proposal`) — the audit answer to "who added this server".
    #[serde(default)]
    pub layer: String,
}

/// A configuration that cannot be accepted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConfigError {
    /// Stable code.
    pub code: &'static str,
    /// What is wrong.
    pub message: String,
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

fn err(code: &'static str, message: impl Into<String>) -> ConfigError {
    ConfigError {
        code,
        message: message.into(),
    }
}

/// Longest server name the host accepts.
pub const MAX_SERVER_NAME_BYTES: usize = 32;
/// Most arguments a configured command may carry.
pub const MAX_COMMAND_ARGS: usize = 32;
/// Most environment entries a configured server may carry.
pub const MAX_ENV_ENTRIES: usize = 32;

/// Normalize a server name to `[a-z0-9][a-z0-9_-]*`, lowercased.
///
/// # Errors
/// `SERVER_NAME_INVALID` when it is empty, too long, does not start with an
/// alphanumeric, or carries a character outside the set. A name is part of
/// every tool name the model sees, so it may not carry a dot (which would
/// forge a namespace) or whitespace.
pub fn normalize_server_name(raw: &str) -> Result<String, ConfigError> {
    let name = raw.trim().to_ascii_lowercase();
    if name.is_empty() || name.len() > MAX_SERVER_NAME_BYTES {
        return Err(err(
            "SERVER_NAME_INVALID",
            format!("server name must be 1..={MAX_SERVER_NAME_BYTES} bytes"),
        ));
    }
    if !name
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_alphanumeric())
    {
        return Err(err(
            "SERVER_NAME_INVALID",
            format!("server name `{raw}` must start with a letter or digit"),
        ));
    }
    if let Some(bad) = name
        .chars()
        .find(|c| !(c.is_ascii_alphanumeric() || *c == '-' || *c == '_'))
    {
        return Err(err(
            "SERVER_NAME_INVALID",
            format!("server name `{raw}` carries `{bad}`; allowed: a-z 0-9 - _"),
        ));
    }
    Ok(name)
}

/// Whether an environment variable name looks like it carries a secret. A
/// secret reaches a server through the broker-held credential handle, never
/// through configuration that is written to disk and echoed in a view.
#[must_use]
pub fn secret_shaped_env(name: &str) -> bool {
    let n = name.to_ascii_lowercase().replace(['_', '-'], "");
    [
        "secret",
        "password",
        "passwd",
        "apikey",
        "token",
        "privatekey",
        "credential",
        "accesskey",
        "auth",
    ]
    .iter()
    .any(|needle| n.contains(needle))
}

impl ServerConfig {
    /// A trusted stdio server (the shape most configurations take).
    #[must_use]
    pub fn stdio(name: &str, command: &str, args: &[&str]) -> Self {
        Self {
            name: name.to_owned(),
            transport: Transport::Stdio {
                command: command.to_owned(),
                args: args.iter().map(|a| (*a).to_owned()).collect(),
            },
            env: BTreeMap::new(),
            credential: None,
            scopes: BTreeSet::new(),
            read_only_tools: BTreeSet::new(),
            trust: Trust::Trusted,
            layer: String::new(),
        }
    }

    /// Check and normalize. The host calls this before a configuration can
    /// enter the hub, from any source.
    ///
    /// # Errors
    /// `SERVER_NAME_INVALID`, `COMMAND_EMPTY`, `COMMAND_TOO_MANY_ARGS`,
    /// `ENV_TOO_MANY`, `ENV_NAME_INVALID` or `SECRET_IN_CONFIG` (a
    /// secret-shaped environment entry: a credential is a broker handle,
    /// not configuration).
    pub fn validate(&mut self) -> Result<(), ConfigError> {
        self.name = normalize_server_name(&self.name)?;
        match &self.transport {
            Transport::Stdio { command, args } => {
                if command.trim().is_empty() {
                    return Err(err("COMMAND_EMPTY", "stdio transport needs a command"));
                }
                if args.len() > MAX_COMMAND_ARGS {
                    return Err(err(
                        "COMMAND_TOO_MANY_ARGS",
                        format!("{} arguments exceeds {MAX_COMMAND_ARGS}", args.len()),
                    ));
                }
            }
        }
        if self.env.len() > MAX_ENV_ENTRIES {
            return Err(err(
                "ENV_TOO_MANY",
                format!(
                    "{} environment entries exceeds {MAX_ENV_ENTRIES}",
                    self.env.len()
                ),
            ));
        }
        for key in self.env.keys() {
            if key.is_empty() || key.contains('=') || key.contains('\0') {
                return Err(err(
                    "ENV_NAME_INVALID",
                    format!("`{key}` is not a variable name"),
                ));
            }
            if secret_shaped_env(key) {
                return Err(err(
                    "SECRET_IN_CONFIG",
                    format!(
                        "environment entry `{key}` looks like a secret; a server's credential is a broker handle (`credential`), never configuration"
                    ),
                ));
            }
        }
        if let Some(handle) = &self.credential
            && handle.trim().is_empty()
        {
            return Err(err("CREDENTIAL_HANDLE_EMPTY", "credential handle is empty"));
        }
        Ok(())
    }

    /// Whether the hub may start this server.
    #[must_use]
    pub fn startable(&self) -> bool {
        self.trust == Trust::Trusted
    }

    /// The effect floor the host declares for one of this server's tools:
    /// a read only when the host itself said so.
    #[must_use]
    pub fn host_declares_read(&self, tool: &str) -> bool {
        self.read_only_tools.contains(tool)
    }
}

/// The normalized fingerprint of a configuration: everything that decides
/// what process is started and how it behaves. Two configurations with the
/// same fingerprint are the same server, whoever wrote them; one byte of
/// difference is a different server and a different transport.
///
/// The credential *handle name* takes part (a server reached with a
/// different credential is a different server); no credential *value* is
/// ever seen here.
#[must_use]
pub fn fingerprint(cfg: &ServerConfig) -> String {
    let mut h = Sha256::new();
    h.update(b"modbit-mcp/1\n");
    h.update(cfg.name.as_bytes());
    h.update(b"\n");
    match &cfg.transport {
        Transport::Stdio { command, args } => {
            h.update(b"stdio\n");
            h.update(command.as_bytes());
            h.update(b"\n");
            for a in args {
                h.update(a.as_bytes());
                h.update(b"\x1f");
            }
        }
    }
    h.update(b"\nenv\n");
    for (k, v) in &cfg.env {
        h.update(k.as_bytes());
        h.update(b"=");
        h.update(v.as_bytes());
        h.update(b"\x1f");
    }
    h.update(b"\ncredential\n");
    h.update(cfg.credential.as_deref().unwrap_or("").as_bytes());
    h.update(b"\nscopes\n");
    for s in &cfg.scopes {
        h.update(s.as_bytes());
        h.update(b"\x1f");
    }
    hex::encode(h.finalize())
}

/// The key a transport is pooled under: the tenant that owns it, the
/// workspace it serves and the configuration fingerprint. Sessions of the
/// same tenant and workspace share one transport; a different tenant, a
/// different workspace or a changed configuration never does.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct PoolKey {
    /// Owning tenant.
    pub tenant: String,
    /// Canonical workspace root the server serves (`<none>` when the task
    /// has no workspace).
    pub workspace: String,
    /// Configuration fingerprint.
    pub fingerprint: String,
}

impl PoolKey {
    /// The key for `cfg` under `tenant` and `workspace`.
    #[must_use]
    pub fn new(tenant: &str, workspace: Option<&str>, cfg: &ServerConfig) -> Self {
        Self {
            tenant: tenant.to_owned(),
            workspace: workspace.unwrap_or("<none>").to_owned(),
            fingerprint: fingerprint(cfg),
        }
    }
}

impl std::fmt::Display for PoolKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}/{}/{}", self.tenant, self.workspace, self.fingerprint)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> ServerConfig {
        ServerConfig::stdio("Docs", "/usr/bin/docs-mcp", &["--root", "/w"])
    }

    #[test]
    fn a_name_is_normalized_and_a_forged_namespace_is_refused() {
        let mut c = cfg();
        c.validate().expect("valid");
        assert_eq!(c.name, "docs");
        for bad in ["", "a.b", "has space", "-leading", &"x".repeat(33)] {
            assert_eq!(
                normalize_server_name(bad).unwrap_err().code,
                "SERVER_NAME_INVALID",
                "`{bad}` must be refused"
            );
        }
    }

    #[test]
    fn a_secret_shaped_environment_entry_is_refused_in_configuration() {
        for key in [
            "GITHUB_TOKEN",
            "api_key",
            "MY_SECRET",
            "db-password",
            "AWS_ACCESS_KEY_ID",
        ] {
            let mut c = cfg();
            c.env.insert(key.into(), "value".into());
            let e = c.validate().expect_err("refused");
            assert_eq!(e.code, "SECRET_IN_CONFIG", "`{key}` must be refused");
        }
        let mut ok = cfg();
        ok.env.insert("DOCS_ROOT".into(), "/w".into());
        ok.credential = Some("docs-api".into());
        ok.validate()
            .expect("a broker handle is how a secret is named");
    }

    #[test]
    fn the_fingerprint_separates_what_makes_a_different_server() {
        let mut base = cfg();
        base.validate().expect("valid");
        let same = {
            let mut c = ServerConfig::stdio("docs", "/usr/bin/docs-mcp", &["--root", "/w"]);
            c.validate().expect("valid");
            c
        };
        assert_eq!(
            fingerprint(&base),
            fingerprint(&same),
            "same config, same server"
        );

        for mutate in [
            (|c: &mut ServerConfig| c.name = "other".into()) as fn(&mut ServerConfig),
            |c: &mut ServerConfig| {
                c.transport = Transport::Stdio {
                    command: "/usr/bin/other".into(),
                    args: vec![],
                }
            },
            |c: &mut ServerConfig| {
                c.env.insert("DOCS_ROOT".into(), "/elsewhere".into());
            },
            |c: &mut ServerConfig| c.credential = Some("other-handle".into()),
            |c: &mut ServerConfig| {
                c.scopes.insert("repo:write".into());
            },
        ] {
            let mut c = base.clone();
            mutate(&mut c);
            assert_ne!(
                fingerprint(&base),
                fingerprint(&c),
                "a changed config is a different server"
            );
        }
    }

    #[test]
    fn a_pool_key_isolates_tenants_and_workspaces() {
        let mut c = cfg();
        c.validate().expect("valid");
        let a = PoolKey::new("t1", Some("/w"), &c);
        assert_eq!(
            a,
            PoolKey::new("t1", Some("/w"), &c),
            "same tenant and workspace share"
        );
        assert_ne!(a, PoolKey::new("t2", Some("/w"), &c), "tenants never share");
        assert_ne!(
            a,
            PoolKey::new("t1", Some("/other"), &c),
            "workspaces never share"
        );
        assert_ne!(a, PoolKey::new("t1", None, &c));
    }

    #[test]
    fn a_proposal_is_not_startable() {
        let mut c = cfg();
        c.trust = Trust::Proposed;
        assert!(!c.startable());
        c.trust = Trust::Trusted;
        assert!(c.startable());
    }

    #[test]
    fn only_the_host_can_call_a_tool_a_read() {
        let mut c = cfg();
        c.read_only_tools.insert("search".into());
        assert!(c.host_declares_read("search"));
        assert!(!c.host_declares_read("deploy"));
    }
}

/// Which external tools the **host** has declared reads, shared between the
/// host's configuration and the registered `external.call` tool.
///
/// The tool layer must know a call's effect class before the call is made
/// and without any host context (`Tool::effect_of` sees only the arguments),
/// so the host publishes its declarations here and the tool reads them. Two
/// properties make that safe:
///
/// - **Unknown is not a read.** A server or a tool nobody declared is an
///   external side effect, so a server the host has never configured cannot
///   be called cheaply.
/// - **Declarations intersect.** When more than one configuration names the
///   same server and tool, it is a read only if every one of them says so;
///   a single configuration that does not makes it an effect for everybody.
///   Merging can only ever take a read away, never grant one.
#[derive(Debug, Default)]
pub struct ReadDeclarations {
    reads: std::sync::Mutex<BTreeMap<(String, String), bool>>,
}

impl ReadDeclarations {
    /// Empty: nothing is a read.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Publish what `cfg` declares. Called by the host whenever a server's
    /// configuration enters or changes.
    pub fn publish(&self, cfg: &ServerConfig) {
        let mut reads = self.reads.lock().unwrap_or_else(|e| e.into_inner());
        for tool in &cfg.read_only_tools {
            // Present means already decided (a denial stays a denial).
            reads
                .entry((cfg.name.clone(), tool.clone()))
                .or_insert(true);
        }
    }

    /// Record that this configuration does *not* call `tool` a read, which
    /// takes the read away from every configuration that did.
    pub fn deny(&self, server: &str, tool: &str) {
        self.reads
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert((server.to_owned(), tool.to_owned()), false);
    }

    /// Publish a whole configuration set, replacing what was there: a tool
    /// is a read only when every server configuration of that name declares
    /// it one.
    pub fn republish(&self, configs: &[ServerConfig]) {
        let mut next: BTreeMap<(String, String), bool> = BTreeMap::new();
        for cfg in configs {
            for tool in &cfg.read_only_tools {
                next.entry((cfg.name.clone(), tool.clone())).or_insert(true);
            }
        }
        // A configuration for the same server that does not name the tool
        // takes the read away.
        for cfg in configs {
            for ((server, tool), value) in &mut next {
                if *server == cfg.name && !cfg.read_only_tools.contains(tool) {
                    *value = false;
                }
            }
        }
        *self.reads.lock().unwrap_or_else(|e| e.into_inner()) = next;
    }

    /// Whether the host declared `tool` on `server` a read.
    #[must_use]
    pub fn is_read(&self, server: &str, tool: &str) -> bool {
        self.reads
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&(server.to_owned(), tool.to_owned()))
            .copied()
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod read_declaration_tests {
    use super::*;

    fn server(name: &str, reads: &[&str]) -> ServerConfig {
        let mut c = ServerConfig::stdio(name, "/bin/x", &[]);
        c.read_only_tools = reads.iter().map(|r| (*r).to_owned()).collect();
        c.validate().expect("valid");
        c
    }

    #[test]
    fn an_undeclared_tool_is_never_a_read() {
        let d = ReadDeclarations::new();
        assert!(!d.is_read("docs", "search"), "unknown is not a read");
        d.publish(&server("docs", &["search"]));
        assert!(d.is_read("docs", "search"));
        assert!(!d.is_read("docs", "deploy"));
        assert!(!d.is_read("other", "search"));
    }

    #[test]
    fn declarations_can_only_take_a_read_away() {
        let d = ReadDeclarations::new();
        d.publish(&server("docs", &["search"]));
        d.deny("docs", "search");
        assert!(!d.is_read("docs", "search"));
        d.publish(&server("docs", &["search"]));
        assert!(
            !d.is_read("docs", "search"),
            "a later declaration cannot give back what a denial took"
        );
    }

    #[test]
    fn republishing_intersects_configurations_of_the_same_server() {
        let d = ReadDeclarations::new();
        d.republish(&[
            server("docs", &["search", "list"]),
            server("docs", &["search"]),
        ]);
        assert!(d.is_read("docs", "search"), "both configurations agree");
        assert!(
            !d.is_read("docs", "list"),
            "one configuration not calling it a read takes it away"
        );
    }
}
