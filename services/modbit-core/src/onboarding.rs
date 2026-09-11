//! Onboarding (REQ-PX-022, docs/39 "Onboarding"): provider setup, explicit
//! scoped repository trust, and starter tasks for the detected stack.
//!
//! Three rules hold here. A credential handed to the Core is held in memory
//! only — never on the log, never in the object store, never in a file, never
//! in a view — and the renderer never touches it: Electron main sends it from
//! the OS keychain. A profile that has not set up a provider cannot start a
//! task, because the Core has no endpoint to start it on. And a desktop task
//! on a repository the user has not trusted does not start: trust is an event
//! on the session, scoped to exactly the root the user named.

use modbit_domain::SessionId;
use modbit_protocol::v1 as wire;
use modbit_providers::{Endpoint, ProviderKind, SecretHandle};

use crate::server::Core;

/// Register a provider from a credential the user supplied.
pub(crate) fn configure_provider(
    core: &Core,
    p: &wire::ConfigureProvider,
) -> Result<wire::ProviderConfigured, (String, String)> {
    let (kind, name, default_base, models) = match p.provider.as_str() {
        "openai" => (
            ProviderKind::OpenAi,
            "openai",
            "https://api.openai.com",
            modbit_providers::default_openai_models(),
        ),
        "anthropic" => (
            ProviderKind::Anthropic,
            "anthropic",
            "https://api.anthropic.com",
            modbit_providers::default_anthropic_models(),
        ),
        other => {
            return Err((
                "UNKNOWN_PROVIDER".into(),
                format!("`{other}` is not a provider this build knows (openai, anthropic)"),
            ));
        }
    };
    // An empty key and an empty endpoint is a withdrawal: the endpoint is
    // forgotten, credential included, as when a live test call failed.
    if p.api_key.is_empty() && p.base_url.trim().is_empty() {
        core.gateway.remove_endpoint(name);
        return Ok(wire::ProviderConfigured {
            endpoint: name.into(),
            credential_available: false,
            models: vec![],
        });
    }
    let base_url = if p.base_url.trim().is_empty() {
        default_base.to_owned()
    } else {
        p.base_url.trim().trim_end_matches('/').to_owned()
    };
    let credential = if p.api_key.is_empty() {
        // A compatible local endpoint may need no credential; the provider's
        // own endpoint always does, and a missing key is reported as such
        // by the live test call rather than hidden here.
        SecretHandle::None
    } else {
        SecretHandle::Inline(p.api_key.clone())
    };
    let credential_available =
        !matches!(credential, SecretHandle::None) || base_url != default_base;
    core.gateway.configure_endpoint(Endpoint {
        name: name.into(),
        kind,
        base_url,
        credential,
        models: models.clone(),
        max_retries: 3,
    });
    Ok(wire::ProviderConfigured {
        endpoint: name.into(),
        credential_available,
        models: models.into_iter().map(|m| m.model).collect(),
    })
}

/// Whether a session has trusted a root, read from the log.
pub(crate) fn is_trusted(
    store: &modbit_event_store::EventStore,
    session_id: SessionId,
    workspace_root: &str,
) -> bool {
    let wanted = canonical(workspace_root);
    let mut after = 0u64;
    loop {
        let Ok(batch) = store.read_session(&session_id, after, 512) else {
            return false;
        };
        if batch.is_empty() {
            return false;
        }
        for ev in &batch {
            after = ev.offset;
            if ev.envelope.event_type != "RepositoryTrusted" {
                continue;
            }
            if let Ok(payload) = store.payload(&ev.envelope)
                && payload["workspace_root"]
                    .as_str()
                    .is_some_and(|r| canonical(r) == wanted)
            {
                return true;
            }
        }
    }
}

fn canonical(root: &str) -> String {
    std::path::Path::new(root)
        .canonicalize()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| root.to_owned())
}

/// The starter tasks for a repository, from the stack the verification plan
/// detects. Each is a real goal the direct baseline can run; none pretends
/// to know the repository better than the index does.
pub(crate) fn starter_tasks(root: &str) -> (Vec<String>, Vec<wire::StarterTaskView>) {
    let path = std::path::Path::new(root);
    let plan = modbit_verification::plan::derive(
        path,
        &[],
        &modbit_verification::plan::configured_commands(path),
    );
    let mut stacks = plan.stacks.clone();
    if stacks.is_empty() {
        // No build system declares the stack: the files do. A bounded walk
        // of the tree, not a read of it.
        let mut counts: std::collections::BTreeMap<&str, u32> = std::collections::BTreeMap::new();
        let mut stack_dirs = vec![path.to_path_buf()];
        let mut seen = 0u32;
        while let Some(d) = stack_dirs.pop() {
            for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
                seen += 1;
                if seen > 2_000 {
                    break;
                }
                let p = e.path();
                let name = e.file_name().to_string_lossy().into_owned();
                if p.is_dir() {
                    if !name.starts_with('.') && name != "node_modules" && name != "target" {
                        stack_dirs.push(p);
                    }
                    continue;
                }
                let stack = match p.extension().and_then(|x| x.to_str()) {
                    Some("rs") => "rust",
                    Some("ts" | "tsx" | "js" | "mjs") => "node",
                    Some("py") => "python",
                    _ => continue,
                };
                *counts.entry(stack).or_default() += 1;
            }
        }
        let mut by_count: Vec<(&str, u32)> = counts.into_iter().collect();
        by_count.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
        stacks.extend(by_count.into_iter().map(|(s, _)| s.to_owned()));
    }
    if stacks.is_empty() {
        stacks.push("text".into());
    }
    let mut tasks = Vec::new();
    let task = |id: &str, title: &str, goal: &str, stack: &str| wire::StarterTaskView {
        id: id.into(),
        title: title.into(),
        goal_text: goal.into(),
        stack: stack.into(),
    };
    for stack in &stacks {
        match stack.as_str() {
            "rust" => {
                tasks.push(task(
                    "rust-add-test",
                    "Add a test for an existing function",
                    "Pick one public function in this crate that has no test and add a focused unit test for it; run cargo test to prove it passes.",
                    "rust",
                ));
                tasks.push(task(
                    "rust-explain",
                    "Explain how the crate is organised",
                    "Explain how the modules in this crate depend on each other and where the entry point is; do not change any file.",
                    "rust",
                ));
            }
            "node" => {
                tasks.push(task(
                    "node-add-test",
                    "Add a test for an existing module",
                    "Pick one exported function that has no test and add a focused test for it; run the test suite to prove it passes.",
                    "node",
                ));
                tasks.push(task(
                    "node-explain",
                    "Explain the package layout",
                    "Explain what this package exports and how its modules fit together; do not change any file.",
                    "node",
                ));
            }
            "python" => {
                tasks.push(task(
                    "python-add-test",
                    "Add a test for an existing function",
                    "Pick one function that has no test and add a focused pytest test for it; run pytest to prove it passes.",
                    "python",
                ));
                tasks.push(task(
                    "python-explain",
                    "Explain the service",
                    "Explain what this service does and how a request flows through it; do not change any file.",
                    "python",
                ));
            }
            _ => {
                tasks.push(task(
                    "text-explain",
                    "Summarise what is here",
                    "Read the files in this repository and summarise what they are for; do not change any file.",
                    "text",
                ));
            }
        }
    }
    (stacks, tasks)
}
