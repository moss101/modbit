//! The live mode of the paired trials (PX-114, PX-136 harness side): a run
//! against a real provider the owner configures through `MODBIT_LIVE_*`
//! environment variables, or no run at all.
//!
//! The mode exists to be refused. It never invents a key, never falls back to
//! a scripted provider and never writes a report it did not measure:
//!
//! - `MODBIT_LIVE=1` must be set explicitly (a stray key in the environment
//!   does not start a paid run);
//! - `MODBIT_LIVE_API_KEY` and `MODBIT_LIVE_MODEL` must be present and
//!   non-blank, and the key must not be a recognisable placeholder;
//! - a loopback `MODBIT_LIVE_BASE_URL` is refused, because a local server is
//!   a stand-in and its numbers would be reported as live
//!   (`MODBIT_LIVE_ALLOW_LOOPBACK=1` is the owner's explicit override, for a
//!   gateway that really runs on this host).
//!
//! The key is handed to the Core process the CLI spawns and appears nowhere
//! else: not in an error, not in the report, not in a debug print.

use std::path::PathBuf;

use crate::projection_trial::RunConfig;

/// Why a live run was refused.
#[derive(Clone, PartialEq, Eq)]
pub enum LiveRefused {
    /// `MODBIT_LIVE=1` is not set: live mode was not asked for.
    NotRequested,
    /// A required variable is missing or blank.
    Missing(&'static str),
    /// The key is a placeholder, not a credential.
    PlaceholderKey,
    /// The base URL points at this host.
    LoopbackBaseUrl,
    /// A variable has a value the harness cannot use.
    Invalid(&'static str, String),
}

// The key is never part of a refusal; `Debug` is written out so a derive can
// never leak a field added later.
impl std::fmt::Debug for LiveRefused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(self, f)
    }
}

impl std::fmt::Display for LiveRefused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotRequested => write!(f, "LIVE: NOT RUN (MODBIT_LIVE=1 is not set)"),
            Self::Missing(v) => write!(
                f,
                "LIVE: NOT RUN ({v} is not set or is blank; nothing is recorded without a provider)"
            ),
            Self::PlaceholderKey => write!(
                f,
                "LIVE: NOT RUN (MODBIT_LIVE_API_KEY is a placeholder, not a credential)"
            ),
            Self::LoopbackBaseUrl => write!(
                f,
                "LIVE: NOT RUN (MODBIT_LIVE_BASE_URL is a loopback address, which is a stand-in; set MODBIT_LIVE_ALLOW_LOOPBACK=1 only for a real gateway on this host)"
            ),
            Self::Invalid(v, why) => write!(f, "LIVE: NOT RUN ({v}: {why})"),
        }
    }
}

/// What a live run needs, validated.
#[derive(Clone)]
pub struct LiveConfig {
    /// The run configuration for the harness (`live` is true).
    pub run: RunConfig,
    /// Repeats per task and arm.
    pub repeats: u32,
    /// Where the report goes.
    pub out: PathBuf,
    /// The spend cap in dollars (`MODBIT_LIVE_MAX_COST_USD`); `None` when the
    /// owner set none. The live workflow always sets one.
    pub max_cost_usd: Option<f64>,
}

impl std::fmt::Debug for LiveConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LiveConfig")
            .field("model", &self.run.model)
            .field("repeats", &self.repeats)
            .field("out", &self.out)
            .field("max_cost_usd", &self.max_cost_usd)
            .finish_non_exhaustive()
    }
}

pub(crate) fn is_placeholder(key: &str) -> bool {
    let k = key.trim().to_ascii_lowercase();
    k.len() < 12
        || [
            "test",
            "dummy",
            "fake",
            "scripted",
            "placeholder",
            "changeme",
            "xxxx",
            "your-key",
            "your_key",
        ]
        .iter()
        .any(|p| k.contains(p))
}

pub(crate) fn is_loopback(url: &str) -> bool {
    let rest = url.split("://").nth(1).unwrap_or(url);
    let host = rest
        .split(['/', '?'])
        .next()
        .unwrap_or_default()
        .rsplit('@')
        .next()
        .unwrap_or_default();
    let host = if let Some(bracketed) = host.strip_prefix('[') {
        bracketed.split(']').next().unwrap_or_default()
    } else {
        host.split(':').next().unwrap_or_default()
    };
    matches!(host, "localhost" | "127.0.0.1" | "::1" | "0.0.0.0")
        || host.starts_with("127.")
        || host.ends_with(".localhost")
}

/// Validate the environment for a live run. `var` reads one variable (the
/// process environment in production, a map in tests).
///
/// # Errors
/// The first reason the run is refused.
pub fn live_config(var: &dyn Fn(&str) -> Option<String>) -> Result<LiveConfig, LiveRefused> {
    let get = |name: &'static str| -> Result<String, LiveRefused> {
        var(name)
            .map(|v| v.trim().to_owned())
            .filter(|v| !v.is_empty())
            .ok_or(LiveRefused::Missing(name))
    };
    if var("MODBIT_LIVE").as_deref().map(str::trim) != Some("1") {
        return Err(LiveRefused::NotRequested);
    }
    let key = get("MODBIT_LIVE_API_KEY")?;
    let model = get("MODBIT_LIVE_MODEL")?;
    if is_placeholder(&key) {
        return Err(LiveRefused::PlaceholderKey);
    }
    let provider = var("MODBIT_LIVE_PROVIDER").unwrap_or_else(|| "openai".into());
    let key_var = match provider.trim() {
        "openai" => "OPENAI_API_KEY",
        "anthropic" => "ANTHROPIC_API_KEY",
        other => {
            return Err(LiveRefused::Invalid(
                "MODBIT_LIVE_PROVIDER",
                format!("`{other}` is not openai or anthropic"),
            ));
        }
    };
    let mut env = vec![(key_var.to_owned(), key)];
    if let Some(base) = var("MODBIT_LIVE_BASE_URL").filter(|b| !b.trim().is_empty()) {
        let allow = var("MODBIT_LIVE_ALLOW_LOOPBACK").as_deref().map(str::trim) == Some("1");
        if is_loopback(&base) && !allow {
            return Err(LiveRefused::LoopbackBaseUrl);
        }
        let base_var = if key_var == "OPENAI_API_KEY" {
            "MODBIT_OPENAI_BASE_URL"
        } else {
            "MODBIT_ANTHROPIC_BASE_URL"
        };
        env.push((base_var.to_owned(), base.trim().to_owned()));
    }
    // The catalog (priced models), the auth scheme and the extra request body
    // of a compatible gateway are the Core's configuration for the family;
    // forwarded when present, never invented.
    let prefix = if key_var == "OPENAI_API_KEY" {
        "MODBIT_OPENAI"
    } else {
        "MODBIT_ANTHROPIC"
    };
    for suffix in ["MODELS", "AUTH", "EXTRA_BODY"] {
        let name = format!("{prefix}_{suffix}");
        if let Some(v) = var(&name).filter(|v| !v.trim().is_empty()) {
            env.push((name, v.trim().to_owned()));
        }
    }
    let max_cost_usd = match var("MODBIT_LIVE_MAX_COST_USD").filter(|v| !v.trim().is_empty()) {
        None => None,
        Some(v) => Some(
            v.trim()
                .parse::<f64>()
                .ok()
                .filter(|c| c.is_finite() && *c > 0.0)
                .ok_or_else(|| {
                    LiveRefused::Invalid(
                        "MODBIT_LIVE_MAX_COST_USD",
                        "a positive number of dollars".into(),
                    )
                })?,
        ),
    };
    let repeats = match var("MODBIT_LIVE_REPEATS") {
        None => 1,
        Some(r) => r
            .trim()
            .parse::<u32>()
            .ok()
            .filter(|n| *n >= 1)
            .ok_or_else(|| {
                LiveRefused::Invalid("MODBIT_LIVE_REPEATS", "a whole number of at least 1".into())
            })?,
    };
    let exe = |name: &str| format!("target/debug/{name}{}", std::env::consts::EXE_SUFFIX);
    Ok(LiveConfig {
        run: RunConfig {
            cli: var("MODBIT_LIVE_CLI")
                .map_or_else(|| PathBuf::from(exe("modbit-cli")), PathBuf::from),
            core: var("MODBIT_LIVE_CORE")
                .map_or_else(|| PathBuf::from(exe("modbit-core")), PathBuf::from),
            model,
            env,
            max_turns: 40,
            live: true,
        },
        repeats,
        out: var("MODBIT_LIVE_OUT")
            .map_or_else(|| PathBuf::from("px-114-live-report.json"), PathBuf::from),
        max_cost_usd,
    })
}
