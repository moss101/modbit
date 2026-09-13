//! `forge.*` — GitHub behind the External Tool Hub rules (PX-006; docs/17,
//! docs/23, docs/29). Reads (`forge.issue.read`, `forge.pr.comments.read`,
//! `forge.ci.status`) need the `network.egress` capability of the lease and
//! return untrusted, provenance-bound data; writes (`forge.pr.create`,
//! `forge.pr.update`) are `ExternalSideEffect`s — approval-bound and
//! receipted by the kernel — and need `secret.use` as well. The token never
//! travels in arguments: the host holds it (`ForgeConfig.token`) and this
//! module puts it in the `Authorization` header and nowhere else; a call
//! that carries one is refused before anything is sent. Egress is pinned to
//! the one API host the host configured; a URL naming any other host is
//! refused (`EGRESS_DENIED`). A create names an idempotency key: the host's
//! ledger answers a retry with the pull request it already recorded, and a
//! forge that refuses a duplicate is asked for the one it has, so one key
//! is one pull request however the call is retried.

use std::sync::Arc;

use serde_json::{Value, json};

use crate::pipeline::InvokeContext;
use crate::registry::{BoxFuture, Idempotency, Tool, ToolOutcome, ToolRegistry, ToolSpec};
use crate::{EffectClass, Result};

/// A refusal an effector step returns before the tool's outcome (boxed:
/// the outcome carries buffers).
type Refused = Box<ToolOutcome>;

/// The forge a host lets this family reach.
#[derive(Clone, Debug)]
pub struct ForgeConfig {
    /// `github` (the only forge yet).
    pub kind: String,
    /// API base, e.g. `https://api.github.com` (a test host in tests).
    pub api_base: String,
    /// Web host issue and pull-request URLs name, e.g. `github.com`.
    pub web_host: String,
    /// The token in the host's custody; `None` = reads only if the forge allows them.
    pub token: Option<String>,
}

impl ForgeConfig {
    /// The secret handle and egress target the lease names (docs/23).
    #[must_use]
    pub fn egress_target(&self) -> String {
        let host = self
            .api_base
            .trim_start_matches("https://")
            .trim_start_matches("http://")
            .trim_end_matches('/')
            .to_owned();
        if host.contains(':') {
            host
        } else {
            format!("{host}:443")
        }
    }
}

/// The host's record of forge effects by idempotency key (the task's log).
pub trait ForgeLedger: Send + Sync {
    /// What a key already produced.
    fn lookup<'a>(&'a self, key: &'a str) -> BoxFuture<'a, Option<Value>>;
    /// Record what a key produced, right after it did.
    fn record<'a>(&'a self, tool: &'a str, key: &'a str, value: &'a Value) -> BoxFuture<'a, ()>;
}

const PROFILES: &[&str] = &["local_trusted", "local_autonomous"];

fn spec(
    name: &str,
    description: &str,
    effect: EffectClass,
    input: Value,
    caps: &[&str],
    idem: Idempotency,
) -> ToolSpec {
    ToolSpec {
        name: name.into(),
        version: "1".into(),
        description: description.into(),
        input_schema: input,
        output_schema: json!({"type": "object"}),
        effect_class: effect,
        required_capabilities: caps.iter().map(|s| (*s).to_owned()).collect(),
        execution_profiles: PROFILES.iter().map(|s| (*s).to_owned()).collect(),
        timeout_ms: 60_000,
        output_budget_bytes: 64 * 1024,
        idempotency: idem,
    }
}

fn s(v: &Value, k: &str) -> String {
    v.get(k)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

/// A token-looking string, wherever it appears in the arguments.
fn looks_like_token(v: &str) -> bool {
    let t = v.trim();
    t.starts_with("ghp_")
        || t.starts_with("gho_")
        || t.starts_with("ghu_")
        || t.starts_with("ghs_")
        || t.starts_with("github_pat_")
        || t.to_ascii_lowercase().starts_with("bearer ")
        || t.to_ascii_lowercase().starts_with("token ")
}

/// Refuse a call that carries a credential in its arguments (docs/23: the
/// broker supplies the token; a model never does).
fn token_in_arguments(args: &Value) -> Option<String> {
    fn walk(v: &Value, path: &str, out: &mut Option<String>) {
        match v {
            Value::Object(m) => {
                for (k, x) in m {
                    let lk = k.to_ascii_lowercase();
                    if matches!(
                        lk.as_str(),
                        "token"
                            | "authorization"
                            | "api_key"
                            | "apikey"
                            | "secret"
                            | "password"
                            | "github_token"
                    ) {
                        *out = Some(format!("{path}{k}"));
                        return;
                    }
                    walk(x, &format!("{path}{k}."), out);
                    if out.is_some() {
                        return;
                    }
                }
            }
            Value::Array(a) => {
                for (i, x) in a.iter().enumerate() {
                    walk(x, &format!("{path}{i}."), out);
                    if out.is_some() {
                        return;
                    }
                }
            }
            Value::String(t) if looks_like_token(t) => {
                *out = Some(path.trim_end_matches('.').to_owned());
            }
            _ => {}
        }
    }
    let mut out = None;
    walk(args, "", &mut out);
    out
}

/// `owner/repo#number` from a web URL on the configured host, or the parts as given.
fn locate(
    cfg: &ForgeConfig,
    args: &Value,
    kind: &str,
) -> std::result::Result<(String, String, u64), Refused> {
    let url = s(args, "url");
    if !url.is_empty() {
        let Some(rest) = url
            .strip_prefix("https://")
            .or_else(|| url.strip_prefix("http://"))
        else {
            return Err(Box::new(ToolOutcome::fail(
                "BAD_URL",
                "url must be an https URL",
            )));
        };
        let (host, path) = rest.split_once('/').unwrap_or((rest, ""));
        if host != cfg.web_host {
            return Err(Box::new(ToolOutcome::fail(
                "EGRESS_DENIED",
                format!(
                    "`{host}` is not the configured forge host `{}`; egress denied",
                    cfg.web_host
                ),
            )));
        }
        let parts: Vec<&str> = path.split('/').collect();
        if parts.len() < 4 || parts[2] != kind {
            return Err(Box::new(ToolOutcome::fail(
                "BAD_URL",
                format!(
                    "expected https://{}/<owner>/<repo>/{kind}/<number>",
                    cfg.web_host
                ),
            )));
        }
        let number: u64 = parts[3]
            .parse()
            .map_err(|_| Box::new(ToolOutcome::fail("BAD_URL", "the number is not an integer")))?;
        return Ok((parts[0].to_owned(), parts[1].to_owned(), number));
    }
    let owner = s(args, "owner");
    let repo = s(args, "repo");
    let number = args.get("number").and_then(Value::as_u64).unwrap_or(0);
    if owner.is_empty() || repo.is_empty() {
        return Err(Box::new(ToolOutcome::fail(
            "BAD_ARGUMENTS",
            "owner and repo (or a url) are required",
        )));
    }
    for p in [&owner, &repo] {
        if p.contains('/') || p.contains("..") || p.contains('?') || p.contains('#') {
            return Err(Box::new(ToolOutcome::fail(
                "BAD_ARGUMENTS",
                "owner and repo are single path segments",
            )));
        }
    }
    Ok((owner, repo, number))
}

/// The forge, the token check, and the lease's egress: what every call does first.
fn gate<'a>(
    ctx: &'a InvokeContext,
    args: &Value,
    needs_token: bool,
) -> std::result::Result<&'a ForgeConfig, Refused> {
    if let Some(at) = token_in_arguments(args) {
        return Err(Box::new(ToolOutcome::fail(
            "TOKEN_IN_ARGUMENTS",
            format!(
                "a credential at `{at}` in the arguments; the broker supplies the token, arguments never carry one"
            ),
        )));
    }
    let Some(cfg) = ctx.forge.as_deref() else {
        return Err(Box::new(ToolOutcome::fail(
            "NO_FORGE",
            "no forge is configured for this Core (MODBIT_GITHUB_TOKEN or ConfigureForge)",
        )));
    };
    if needs_token && cfg.token.is_none() {
        return Err(Box::new(ToolOutcome::fail(
            "NO_FORGE_TOKEN",
            "the forge has no token in the Core's custody; a write needs one",
        )));
    }
    Ok(cfg)
}

async fn call(
    cfg: &ForgeConfig,
    method: reqwest::Method,
    path: &str,
    body: Option<Value>,
) -> std::result::Result<(u16, Value), Refused> {
    let url = format!(
        "{}/{}",
        cfg.api_base.trim_end_matches('/'),
        path.trim_start_matches('/')
    );
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| Box::new(ToolOutcome::infra("HTTP_CLIENT", e.to_string())))?;
    let mut req = client
        .request(method, &url)
        .header("Accept", "application/vnd.github+json")
        .header("User-Agent", "modbit-forge/1")
        .header("X-GitHub-Api-Version", "2022-11-28");
    if let Some(t) = &cfg.token {
        req = req.header("Authorization", format!("Bearer {t}"));
    }
    if let Some(b) = body {
        req = req.json(&b);
    }
    let resp = req.send().await.map_err(|e| {
        Box::new(ToolOutcome::infra(
            "FORGE_UNREACHABLE",
            redact(cfg, &chain(&e)),
        ))
    })?;
    let status = resp.status().as_u16();
    let text = resp
        .text()
        .await
        .map_err(|e| Box::new(ToolOutcome::infra("FORGE_READ", redact(cfg, &chain(&e)))))?;
    let value: Value = serde_json::from_str(&text).unwrap_or_else(|_| json!({"raw": text}));
    Ok((status, value))
}

/// An error with its causes (reqwest's `Display` omits the source that says
/// why a request failed).
fn chain(e: &dyn std::error::Error) -> String {
    let mut out = e.to_string();
    let mut cur = e.source();
    while let Some(c) = cur {
        out.push_str(": ");
        out.push_str(&c.to_string());
        cur = c.source();
    }
    out
}

/// The token never appears in an error, whatever the transport said.
fn redact(cfg: &ForgeConfig, text: &str) -> String {
    match &cfg.token {
        Some(t) if !t.is_empty() => text.replace(t.as_str(), "[redacted]"),
        _ => text.to_owned(),
    }
}

fn forge_error(status: u16, body: &Value) -> ToolOutcome {
    let message = body
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or("the forge refused the request")
        .to_owned();
    let code = match status {
        401 => "FORGE_UNAUTHORIZED",
        403 => "FORGE_FORBIDDEN",
        404 => "FORGE_NOT_FOUND",
        422 => "FORGE_UNPROCESSABLE",
        _ => "FORGE_ERROR",
    };
    ToolOutcome::fail(code, format!("{status}: {message}"))
}

fn untrusted(v: Value) -> Value {
    let mut v = v;
    if let Value::Object(m) = &mut v {
        m.insert("trust".into(), json!("UNTRUSTED_EXTERNAL_CONTENT"));
    }
    v
}

fn pr_view(p: &Value) -> Value {
    json!({
        "number": p["number"],
        "url": p["html_url"],
        "api_url": p["url"],
        "state": p["state"],
        "title": p["title"],
        "head": {"ref": p["head"]["ref"], "sha": p["head"]["sha"]},
        "base": {"ref": p["base"]["ref"]},
    })
}

macro_rules! tool {
    ($ty:ident, $spec:expr, |$ctx:ident, $args:ident| $body:expr) => {
        struct $ty(ToolSpec);
        impl Tool for $ty {
            fn spec(&self) -> &ToolSpec {
                &self.0
            }
            fn invoke<'a>(
                &'a self,
                $ctx: &'a InvokeContext,
                $args: Value,
            ) -> BoxFuture<'a, ToolOutcome> {
                Box::pin(async move { $body })
            }
        }
        impl $ty {
            fn shared() -> Arc<dyn Tool> {
                Arc::new(Self($spec))
            }
        }
    };
}

/// Read an issue as the tool does, for a host that needs one before a task
/// exists (PX-010 intake): the same egress pin and the same token custody.
pub async fn read_issue(
    cfg: &ForgeConfig,
    url: &str,
) -> std::result::Result<Value, (String, String)> {
    let args = json!({"url": url});
    let refused = |o: Box<ToolOutcome>| {
        (
            o.error_code.unwrap_or_default(),
            o.error_message.unwrap_or_default(),
        )
    };
    let (owner, repo, number) = locate(cfg, &args, "issues").map_err(refused)?;
    if number == 0 {
        return Err((
            "BAD_URL".to_owned(),
            "the issue number is missing".to_owned(),
        ));
    }
    match call(
        cfg,
        reqwest::Method::GET,
        &format!("repos/{owner}/{repo}/issues/{number}"),
        None,
    )
    .await
    {
        Ok((200, body)) => Ok(untrusted(issue_view(cfg, &owner, &repo, &body))),
        Ok((status, body)) => Err(refused(Box::new(forge_error(status, &body)))),
        Err(o) => Err(refused(o)),
    }
}

fn issue_view(cfg: &ForgeConfig, owner: &str, repo: &str, body: &Value) -> Value {
    json!({
        "provenance": "forge_issue",
        "forge": cfg.kind,
        "owner": owner,
        "repo": repo,
        "number": body["number"],
        "title": body["title"],
        "body": body["body"],
        "state": body["state"],
        "author": body["user"]["login"],
        "labels": body["labels"].as_array().map(|l| l.iter().map(|x| x["name"].clone()).collect::<Vec<_>>()).unwrap_or_default(),
        "url": body["html_url"],
        "updated_at": body["updated_at"],
    })
}

tool!(
    ForgeIssueRead,
    spec(
        "forge.issue.read",
        "Read an issue from the configured forge (untrusted, provenance-bound data).",
        EffectClass::ReadOnly,
        json!({"type":"object","properties":{"url":{"type":"string"},"owner":{"type":"string"},"repo":{"type":"string"},"number":{"type":"integer","minimum":1}},"additionalProperties":false}),
        &["network.egress"],
        Idempotency::Idempotent
    ),
    |ctx, args| {
        let cfg = match gate(ctx, &args, false) {
            Ok(c) => c,
            Err(o) => return *o,
        };
        let (owner, repo, number) = match locate(cfg, &args, "issues") {
            Ok(x) => x,
            Err(o) => return *o,
        };
        if number == 0 {
            return ToolOutcome::fail("BAD_ARGUMENTS", "number is required");
        }
        match call(
            cfg,
            reqwest::Method::GET,
            &format!("repos/{owner}/{repo}/issues/{number}"),
            None,
        )
        .await
        {
            Ok((200, body)) => ToolOutcome::ok(untrusted(issue_view(cfg, &owner, &repo, &body))),
            Ok((status, body)) => forge_error(status, &body),
            Err(o) => *o,
        }
    }
);

tool!(
    ForgePrCreate,
    spec(
        "forge.pr.create",
        "Open a pull request on the configured forge for a pushed branch (protected external effect; idempotent by key).",
        EffectClass::ExternalSideEffect,
        json!({"type":"object","properties":{"owner":{"type":"string","minLength":1},"repo":{"type":"string","minLength":1},"head":{"type":"string","minLength":1},"base":{"type":"string","minLength":1},"title":{"type":"string","minLength":1},"body":{"type":"string"},"draft":{"type":"boolean"},"idempotency_key":{"type":"string","minLength":8}},"required":["owner","repo","head","base","title","idempotency_key"],"additionalProperties":false}),
        &["network.egress", "secret.use"],
        Idempotency::NonIdempotent
    ),
    |ctx, args| {
        let cfg = match gate(ctx, &args, true) {
            Ok(c) => c,
            Err(o) => return *o,
        };
        let (owner, repo, _) = match locate(cfg, &args, "pull") {
            Ok(x) => x,
            Err(o) => return *o,
        };
        let key = s(&args, "idempotency_key");
        if let Some(l) = ctx.forge_ledger.as_ref()
            && let Some(prior) = l.lookup(&key).await
        {
            let mut v = prior;
            v["replayed"] = json!(true);
            return ToolOutcome::ok(v);
        }
        let head = s(&args, "head");
        let base = s(&args, "base");
        let body = json!({
            "title": s(&args, "title"),
            "body": s(&args, "body"),
            "head": head,
            "base": base,
            "draft": args.get("draft").and_then(Value::as_bool).unwrap_or(false),
        });
        let created = match call(
            cfg,
            reqwest::Method::POST,
            &format!("repos/{owner}/{repo}/pulls"),
            Some(body),
        )
        .await
        {
            Ok((201, pr)) => pr,
            Ok((422, err)) => {
                // The forge already has one for this head: find it rather
                // than count a refusal as a second pull request (a crash
                // between the create and its record reconciles here).
                let msg = err["message"].as_str().unwrap_or_default().to_owned()
                    + &err["errors"].to_string();
                if !msg.contains("already exists") {
                    return forge_error(422, &err);
                }
                match call(
                    cfg,
                    reqwest::Method::GET,
                    &format!(
                        "repos/{owner}/{repo}/pulls?state=open&head={owner}:{head}&base={base}"
                    ),
                    None,
                )
                .await
                {
                    Ok((200, Value::Array(list))) if !list.is_empty() => list[0].clone(),
                    Ok((status, b)) => return forge_error(status, &b),
                    Err(o) => return *o,
                }
            }
            Ok((status, err)) => return forge_error(status, &err),
            Err(o) => return *o,
        };
        let mut view = pr_view(&created);
        view["idempotency_key"] = json!(key);
        view["owner"] = json!(owner);
        view["repo"] = json!(repo);
        if let Some(l) = ctx.forge_ledger.as_ref() {
            l.record("forge.pr.create", &key, &view).await;
        }
        view["replayed"] = json!(false);
        ToolOutcome::ok(view)
    }
);

tool!(
    ForgePrUpdate,
    spec(
        "forge.pr.update",
        "Update a pull request's title, body or state on the configured forge (protected external effect; idempotent by key).",
        EffectClass::ExternalSideEffect,
        json!({"type":"object","properties":{"owner":{"type":"string","minLength":1},"repo":{"type":"string","minLength":1},"number":{"type":"integer","minimum":1},"title":{"type":"string"},"body":{"type":"string"},"state":{"type":"string","enum":["open","closed"]},"idempotency_key":{"type":"string","minLength":8}},"required":["owner","repo","number","idempotency_key"],"additionalProperties":false}),
        &["network.egress", "secret.use"],
        Idempotency::NonIdempotent
    ),
    |ctx, args| {
        let cfg = match gate(ctx, &args, true) {
            Ok(c) => c,
            Err(o) => return *o,
        };
        let (owner, repo, number) = match locate(cfg, &args, "pull") {
            Ok(x) => x,
            Err(o) => return *o,
        };
        let key = s(&args, "idempotency_key");
        if let Some(l) = ctx.forge_ledger.as_ref()
            && let Some(prior) = l.lookup(&key).await
        {
            let mut v = prior;
            v["replayed"] = json!(true);
            return ToolOutcome::ok(v);
        }
        let mut patch = serde_json::Map::new();
        for k in ["title", "body", "state"] {
            if let Some(v) = args.get(k) {
                patch.insert(k.into(), v.clone());
            }
        }
        if patch.is_empty() {
            return ToolOutcome::fail(
                "BAD_ARGUMENTS",
                "nothing to update: give a title, body or state",
            );
        }
        match call(
            cfg,
            reqwest::Method::PATCH,
            &format!("repos/{owner}/{repo}/pulls/{number}"),
            Some(Value::Object(patch)),
        )
        .await
        {
            Ok((200, pr)) => {
                let mut view = pr_view(&pr);
                view["idempotency_key"] = json!(key);
                view["owner"] = json!(owner);
                view["repo"] = json!(repo);
                if let Some(l) = ctx.forge_ledger.as_ref() {
                    l.record("forge.pr.update", &key, &view).await;
                }
                view["replayed"] = json!(false);
                ToolOutcome::ok(view)
            }
            Ok((status, err)) => forge_error(status, &err),
            Err(o) => *o,
        }
    }
);

tool!(
    ForgePrCommentsRead,
    spec(
        "forge.pr.comments.read",
        "Read a pull request's review and issue comments from the configured forge (untrusted data).",
        EffectClass::ReadOnly,
        json!({"type":"object","properties":{"url":{"type":"string"},"owner":{"type":"string"},"repo":{"type":"string"},"number":{"type":"integer","minimum":1}},"additionalProperties":false}),
        &["network.egress"],
        Idempotency::Idempotent
    ),
    |ctx, args| {
        let cfg = match gate(ctx, &args, false) {
            Ok(c) => c,
            Err(o) => return *o,
        };
        let (owner, repo, number) = match locate(cfg, &args, "pull") {
            Ok(x) => x,
            Err(o) => return *o,
        };
        if number == 0 {
            return ToolOutcome::fail("BAD_ARGUMENTS", "number is required");
        }
        let mut comments = Vec::new();
        for (kind, path) in [
            (
                "review",
                format!("repos/{owner}/{repo}/pulls/{number}/comments"),
            ),
            (
                "issue",
                format!("repos/{owner}/{repo}/issues/{number}/comments"),
            ),
        ] {
            match call(cfg, reqwest::Method::GET, &path, None).await {
                Ok((200, Value::Array(list))) => {
                    for c in list {
                        comments.push(json!({
                            "kind": kind,
                            "id": c["id"],
                            "author": c["user"]["login"],
                            "body": c["body"],
                            "path": c["path"],
                            "line": c["line"],
                            "created_at": c["created_at"],
                            "url": c["html_url"],
                        }));
                    }
                }
                Ok((status, b)) => return forge_error(status, &b),
                Err(o) => return *o,
            }
        }
        ToolOutcome::ok(untrusted(
            json!({"provenance": "forge_pr_comment", "owner": owner, "repo": repo, "number": number, "comments": comments}),
        ))
    }
);

tool!(
    ForgeCiStatus,
    spec(
        "forge.ci.status",
        "Read the check runs of a commit on the configured forge (untrusted data; never a verification result).",
        EffectClass::ReadOnly,
        json!({"type":"object","properties":{"owner":{"type":"string","minLength":1},"repo":{"type":"string","minLength":1},"ref":{"type":"string","minLength":1}},"required":["owner","repo","ref"],"additionalProperties":false}),
        &["network.egress"],
        Idempotency::Idempotent
    ),
    |ctx, args| {
        let cfg = match gate(ctx, &args, false) {
            Ok(c) => c,
            Err(o) => return *o,
        };
        let (owner, repo, _) = match locate(cfg, &args, "commit") {
            Ok(x) => x,
            Err(o) => return *o,
        };
        let r = s(&args, "ref");
        if r.contains('/') && !r.starts_with("refs/") || r.contains("..") {
            return ToolOutcome::fail(
                "BAD_ARGUMENTS",
                "ref must be a sha, a branch name or a refs/ path",
            );
        }
        match call(
            cfg,
            reqwest::Method::GET,
            &format!("repos/{owner}/{repo}/commits/{r}/check-runs"),
            None,
        )
        .await
        {
            Ok((200, body)) => {
                let checks: Vec<Value> = body["check_runs"]
                    .as_array()
                    .map(|a| a.iter().map(|c| json!({"name": c["name"], "status": c["status"], "conclusion": c["conclusion"], "url": c["html_url"], "head_sha": c["head_sha"]})).collect())
                    .unwrap_or_default();
                ToolOutcome::ok(untrusted(
                    json!({"provenance": "forge_ci", "owner": owner, "repo": repo, "ref": r, "total": body["total_count"], "checks": checks}),
                ))
            }
            Ok((status, b)) => forge_error(status, &b),
            Err(o) => *o,
        }
    }
);

/// Register the family.
pub fn register_forge(registry: &mut ToolRegistry) -> Result<()> {
    for t in [
        ForgeIssueRead::shared(),
        ForgePrCreate::shared(),
        ForgePrUpdate::shared(),
        ForgePrCommentsRead::shared(),
        ForgeCiStatus::shared(),
    ] {
        registry.register(t)?;
    }
    Ok(())
}
