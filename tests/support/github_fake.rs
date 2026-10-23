//! A GitHub REST fake that follows GitHub's documented response shapes
//! (docs.github.com/en/rest: pulls, pull files, issue and review comments,
//! reviews, check runs, rate limiting, `Link` pagination, the diff media
//! type). It is the stand-in the wire-level tests of PX-125, PX-126 and
//! PX-127 run against (DR-M6-002): it is NOT GitHub, and nothing proved
//! against it is a proof against the real API. It exists so the adapter,
//! the Core and the cloud worker cross a real socket and parse real HTTP.
//!
//! Included by `#[path]` from each test crate that needs it (std +
//! `serde_json` only), so no crate or dependency is added for it.
#![allow(dead_code, clippy::too_many_arguments)]

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};

use serde_json::{Value, json};

/// A request the fake received.
#[derive(Clone, Debug)]
pub struct Seen {
    pub method: String,
    /// Path without the query.
    pub path: String,
    pub query: String,
    /// The `Authorization: Bearer` token matched.
    pub authorized: bool,
    pub accept: String,
    pub body: Value,
}

/// A fault the next matching requests meet instead of the route.
#[derive(Clone, Debug)]
pub enum Fault {
    /// `403` + `x-ratelimit-remaining: 0` + `x-ratelimit-reset` (primary limit).
    PrimaryRateLimit { reset_epoch_secs: u64 },
    /// `403` + `retry-after` and the documented secondary-limit message.
    SecondaryRateLimit { retry_after_secs: u64 },
    /// `429` + `retry-after`.
    TooManyRequests { retry_after_secs: u64 },
    /// `403 Resource not accessible by integration`.
    Forbidden,
    /// `500`.
    ServerError,
}

struct Armed {
    method: Option<String>,
    path_contains: String,
    fault: Fault,
    remaining: u32,
}

#[derive(Default)]
struct State {
    token: String,
    /// Reads without a token succeed (a public repository).
    anonymous_reads: bool,
    seen: Vec<Seen>,
    next_id: u64,
    /// `owner/repo` -> pulls by number.
    pulls: BTreeMap<(String, u64), Value>,
    /// `owner/repo#n` -> files.
    files: BTreeMap<(String, u64), Vec<Value>>,
    /// `owner/repo#n` -> issue (conversation) comments.
    issue_comments: BTreeMap<(String, u64), Vec<Value>>,
    /// `owner/repo#n` -> review (inline) comments.
    review_comments: BTreeMap<(String, u64), Vec<Value>>,
    reviews: BTreeMap<(String, u64), Vec<Value>>,
    issues: BTreeMap<(String, u64), Value>,
    /// sha -> check runs.
    check_runs: BTreeMap<(String, String), Vec<Value>>,
    armed: Vec<Armed>,
    port: u16,
    /// Pull-request creation answers the documented duplicate refusal.
    web_host: String,
}

/// The running fake.
#[derive(Clone)]
pub struct GithubFake {
    pub base: String,
    state: Arc<Mutex<State>>,
}

fn now_iso() -> String {
    // A fixed-shape UTC timestamp from the system clock (no chrono here).
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    iso_of(secs)
}

pub fn iso_of(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    // civil from days (Howard Hinnant)
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60
    )
}

fn user(login: &str) -> Value {
    json!({
        "login": login, "id": 1000 + login.len() as u64, "node_id": format!("U_{login}"),
        "type": "User", "site_admin": false,
        "html_url": format!("https://github.test/{login}"),
    })
}

impl GithubFake {
    /// Start the fake; every route needs `token` as a bearer unless
    /// [`Self::allow_anonymous_reads`] is set.
    pub fn start(token: &str) -> Self {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let state = Arc::new(Mutex::new(State {
            token: token.to_owned(),
            port,
            web_host: "github.test".into(),
            next_id: 5000,
            ..Default::default()
        }));
        let st = Arc::clone(&state);
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(s) = stream else { break };
                let st = Arc::clone(&st);
                std::thread::spawn(move || serve(s, &st));
            }
        });
        Self {
            base: format!("http://127.0.0.1:{port}"),
            state,
        }
    }

    pub fn allow_anonymous_reads(&self, on: bool) {
        self.state.lock().unwrap().anonymous_reads = on;
    }

    pub fn requests(&self) -> Vec<Seen> {
        self.state.lock().unwrap().seen.clone()
    }

    pub fn requests_matching(&self, method: &str, path_contains: &str) -> Vec<Seen> {
        self.requests()
            .into_iter()
            .filter(|r| r.method == method && r.path.contains(path_contains))
            .collect()
    }

    /// Arm a fault for the next `times` requests whose path contains
    /// `path_contains` (and whose method is `method`, when given).
    pub fn inject(&self, method: Option<&str>, path_contains: &str, fault: Fault, times: u32) {
        self.state.lock().unwrap().armed.push(Armed {
            method: method.map(str::to_owned),
            path_contains: path_contains.to_owned(),
            fault,
            remaining: times,
        });
    }

    pub fn clear_faults(&self) {
        self.state.lock().unwrap().armed.clear();
    }

    /// An open pull request as `GET /repos/{o}/{r}/pulls/{n}` documents it.
    #[allow(clippy::too_many_arguments)]
    pub fn add_pull(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
        title: &str,
        body: &str,
        head_ref: &str,
        head_sha: &str,
        base_ref: &str,
        author: &str,
    ) {
        let mut s = self.state.lock().unwrap();
        let port = s.port;
        let now = now_iso();
        let pr = json!({
            "url": format!("http://127.0.0.1:{port}/repos/{owner}/{repo}/pulls/{number}"),
            "id": 9000 + number, "node_id": format!("PR_{number}"),
            "html_url": format!("https://github.test/{owner}/{repo}/pull/{number}"),
            "diff_url": format!("https://github.test/{owner}/{repo}/pull/{number}.diff"),
            "patch_url": format!("https://github.test/{owner}/{repo}/pull/{number}.patch"),
            "issue_url": format!("http://127.0.0.1:{port}/repos/{owner}/{repo}/issues/{number}"),
            "number": number, "state": "open", "locked": false, "title": title,
            "user": user(author), "body": body,
            "created_at": now, "updated_at": now, "closed_at": null, "merged_at": null,
            "merge_commit_sha": null, "assignee": null, "assignees": [],
            "requested_reviewers": [], "requested_teams": [], "labels": [],
            "head": {"label": format!("{owner}:{head_ref}"), "ref": head_ref, "sha": head_sha,
                     "user": user(owner), "repo": {"name": repo, "full_name": format!("{owner}/{repo}")}},
            "base": {"label": format!("{owner}:{base_ref}"), "ref": base_ref,
                     "sha": "b".repeat(40), "user": user(owner), "repo": {"name": repo, "full_name": format!("{owner}/{repo}")}},
            "author_association": "OWNER", "draft": false, "merged": false,
            "mergeable": true, "rebaseable": true, "mergeable_state": "clean",
            "merged_by": null, "comments": 0, "review_comments": 0,
            "maintainer_can_modify": false, "commits": 1,
            "additions": 0, "deletions": 0, "changed_files": 0,
        });
        s.pulls.insert((format!("{owner}/{repo}"), number), pr);
    }

    pub fn set_pull_field(&self, owner: &str, repo: &str, number: u64, key: &str, value: Value) {
        let mut s = self.state.lock().unwrap();
        if let Some(p) = s.pulls.get_mut(&(format!("{owner}/{repo}"), number)) {
            p[key] = value;
        }
    }

    pub fn request_reviewer(&self, owner: &str, repo: &str, number: u64, login: &str) {
        let mut s = self.state.lock().unwrap();
        if let Some(p) = s.pulls.get_mut(&(format!("{owner}/{repo}"), number)) {
            p["requested_reviewers"]
                .as_array_mut()
                .unwrap()
                .push(user(login));
        }
    }

    pub fn add_review(&self, owner: &str, repo: &str, number: u64, login: &str, state: &str) {
        let mut s = self.state.lock().unwrap();
        s.next_id += 1;
        let id = s.next_id;
        let r = json!({"id": id, "node_id": format!("PRR_{id}"), "user": user(login),
            "body": "", "state": state, "html_url": format!("https://github.test/{owner}/{repo}/pull/{number}#pullrequestreview-{id}"),
            "submitted_at": now_iso(), "commit_id": "c".repeat(40), "author_association": "MEMBER"});
        s.reviews
            .entry((format!("{owner}/{repo}"), number))
            .or_default()
            .push(r);
    }

    /// A changed file of a pull request, in the documented shape. `patch`
    /// `None` models a binary or oversized file GitHub returns without one.
    pub fn add_pull_file(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
        filename: &str,
        status: &str,
        additions: u64,
        deletions: u64,
        patch: Option<&str>,
    ) {
        let mut s = self.state.lock().unwrap();
        let sha = format!("{:040x}", filename.len() * 7919 + additions as usize);
        let mut f = json!({
            "sha": sha, "filename": filename, "status": status,
            "additions": additions, "deletions": deletions, "changes": additions + deletions,
            "blob_url": format!("https://github.test/{owner}/{repo}/blob/{}/{filename}", "c".repeat(40)),
            "raw_url": format!("https://github.test/{owner}/{repo}/raw/{}/{filename}", "c".repeat(40)),
            "contents_url": format!("https://api.github.test/repos/{owner}/{repo}/contents/{filename}"),
        });
        if let Some(p) = patch {
            f["patch"] = json!(p);
        }
        s.files
            .entry((format!("{owner}/{repo}"), number))
            .or_default()
            .push(f);
        let key = (format!("{owner}/{repo}"), number);
        let (adds, dels, n) = s.files[&key].iter().fold((0, 0, 0), |a, f| {
            (
                a.0 + f["additions"].as_u64().unwrap_or(0),
                a.1 + f["deletions"].as_u64().unwrap_or(0),
                a.2 + 1,
            )
        });
        if let Some(p) = s.pulls.get_mut(&key) {
            p["additions"] = json!(adds);
            p["deletions"] = json!(dels);
            p["changed_files"] = json!(n);
        }
    }

    pub fn add_issue(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
        title: &str,
        body: &str,
        author: &str,
    ) {
        let mut s = self.state.lock().unwrap();
        let port = s.port;
        let now = now_iso();
        s.issues.insert(
            (format!("{owner}/{repo}"), number),
            json!({
                "url": format!("http://127.0.0.1:{port}/repos/{owner}/{repo}/issues/{number}"),
                "html_url": format!("https://github.test/{owner}/{repo}/issues/{number}"),
                "id": 7000 + number, "number": number, "state": "open", "title": title,
                "body": body, "user": user(author), "labels": [{"name": "bug"}],
                "comments": 0, "created_at": now, "updated_at": now, "closed_at": null,
                "author_association": "NONE",
            }),
        );
    }

    /// A conversation comment (the issue-comments API; a pull request's
    /// conversation lives there).
    pub fn add_issue_comment(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
        author: &str,
        body: &str,
    ) -> u64 {
        let mut s = self.state.lock().unwrap();
        push_issue_comment(&mut s, owner, repo, number, author, body)
    }

    /// An inline review comment on a pull request.
    pub fn add_review_comment(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
        author: &str,
        body: &str,
        path: &str,
        line: u64,
    ) -> u64 {
        let mut s = self.state.lock().unwrap();
        s.next_id += 1;
        let id = s.next_id;
        let port = s.port;
        let now = now_iso();
        let c = json!({
            "url": format!("http://127.0.0.1:{port}/repos/{owner}/{repo}/pulls/comments/{id}"),
            "pull_request_review_id": 1, "id": id, "node_id": format!("PRRC_{id}"),
            "diff_hunk": "@@ -1,2 +1,3 @@", "path": path, "position": line, "original_position": line,
            "commit_id": "c".repeat(40), "original_commit_id": "c".repeat(40),
            "user": user(author), "body": body, "created_at": now, "updated_at": now,
            "html_url": format!("https://github.test/{owner}/{repo}/pull/{number}#discussion_r{id}"),
            "pull_request_url": format!("http://127.0.0.1:{port}/repos/{owner}/{repo}/pulls/{number}"),
            "author_association": "MEMBER", "line": line, "side": "RIGHT",
        });
        s.review_comments
            .entry((format!("{owner}/{repo}"), number))
            .or_default()
            .push(c);
        id
    }

    /// Conversation comments as the fake holds them (what was posted).
    pub fn issue_comments(&self, owner: &str, repo: &str, number: u64) -> Vec<Value> {
        self.state
            .lock()
            .unwrap()
            .issue_comments
            .get(&(format!("{owner}/{repo}"), number))
            .cloned()
            .unwrap_or_default()
    }

    pub fn set_check_runs(&self, owner: &str, repo: &str, sha: &str, runs: Vec<Value>) {
        self.state
            .lock()
            .unwrap()
            .check_runs
            .insert((format!("{owner}/{repo}"), sha.to_owned()), runs);
    }

    /// A check run in the documented shape.
    pub fn check_run(
        id: u64,
        name: &str,
        head_sha: &str,
        status: &str,
        conclusion: Option<&str>,
        title: &str,
        summary: &str,
        text: &str,
    ) -> Value {
        json!({
            "id": id, "name": name, "head_sha": head_sha, "node_id": format!("CR_{id}"),
            "external_id": "", "url": format!("https://api.github.test/check-runs/{id}"),
            "html_url": format!("https://github.test/o/r/runs/{id}"),
            "details_url": format!("https://ci.example/runs/{id}"),
            "status": status, "conclusion": conclusion,
            "started_at": now_iso(), "completed_at": if status == "completed" { json!(now_iso()) } else { Value::Null },
            "output": {"title": title, "summary": summary, "text": text, "annotations_count": 0},
            "check_suite": {"id": 1}, "app": {"slug": "github-actions"}, "pull_requests": [],
        })
    }

    pub fn pull(&self, owner: &str, repo: &str, number: u64) -> Option<Value> {
        self.state
            .lock()
            .unwrap()
            .pulls
            .get(&(format!("{owner}/{repo}"), number))
            .cloned()
    }

    pub fn pulls_of(&self, owner: &str, repo: &str) -> Vec<Value> {
        let key = format!("{owner}/{repo}");
        self.state
            .lock()
            .unwrap()
            .pulls
            .iter()
            .filter(|((r, _), _)| *r == key)
            .map(|(_, p)| p.clone())
            .collect()
    }
}

fn push_issue_comment(
    s: &mut State,
    owner: &str,
    repo: &str,
    number: u64,
    author: &str,
    body: &str,
) -> u64 {
    s.next_id += 1;
    let id = s.next_id;
    let port = s.port;
    let now = now_iso();
    let c = json!({
        "url": format!("http://127.0.0.1:{port}/repos/{owner}/{repo}/issues/comments/{id}"),
        "html_url": format!("https://github.test/{owner}/{repo}/pull/{number}#issuecomment-{id}"),
        "issue_url": format!("http://127.0.0.1:{port}/repos/{owner}/{repo}/issues/{number}"),
        "id": id, "node_id": format!("IC_{id}"), "user": user(author), "created_at": now,
        "updated_at": now, "author_association": "MEMBER", "body": body,
    });
    s.issue_comments
        .entry((format!("{owner}/{repo}"), number))
        .or_default()
        .push(c);
    id
}

struct Reply {
    status: u16,
    headers: Vec<(String, String)>,
    content_type: String,
    body: String,
}

fn json_reply(status: u16, v: &Value) -> Reply {
    Reply {
        status,
        headers: vec![],
        content_type: "application/json; charset=utf-8".into(),
        body: v.to_string(),
    }
}

fn message(status: u16, text: &str) -> Reply {
    json_reply(
        status,
        &json!({"message": text, "documentation_url": "https://docs.github.com/rest", "status": status.to_string()}),
    )
}

fn parse_query(q: &str) -> BTreeMap<String, String> {
    q.split('&')
        .filter(|p| !p.is_empty())
        .filter_map(|p| p.split_once('=').or(Some((p, ""))))
        .map(|(k, v)| (k.to_owned(), v.to_owned()))
        .collect()
}

fn paginate(items: &[Value], query: &BTreeMap<String, String>, base: &str, path: &str) -> Reply {
    let per_page: usize = query
        .get("per_page")
        .and_then(|v| v.parse().ok())
        .unwrap_or(30)
        .clamp(1, 100);
    let page: usize = query
        .get("page")
        .and_then(|v| v.parse().ok())
        .unwrap_or(1)
        .max(1);
    let start = (page - 1) * per_page;
    let slice: Vec<Value> = items.iter().skip(start).take(per_page).cloned().collect();
    let last = items.len().div_ceil(per_page).max(1);
    let mut links = Vec::new();
    if page < last {
        links.push(format!(
            "<{base}{path}?per_page={per_page}&page={}>; rel=\"next\"",
            page + 1
        ));
        links.push(format!(
            "<{base}{path}?per_page={per_page}&page={last}>; rel=\"last\""
        ));
    }
    let mut r = json_reply(200, &Value::Array(slice));
    if !links.is_empty() {
        r.headers.push(("link".into(), links.join(", ")));
    }
    r
}

/// The unified diff GitHub's `application/vnd.github.diff` media type returns.
fn diff_of(files: &[Value]) -> String {
    let mut out = String::new();
    for f in files {
        let name = f["filename"].as_str().unwrap_or_default();
        out.push_str(&format!("diff --git a/{name} b/{name}\n"));
        match f["status"].as_str() {
            Some("added") => out.push_str("new file mode 100644\n--- /dev/null\n"),
            _ => out.push_str(&format!("--- a/{name}\n")),
        }
        out.push_str(&format!("+++ b/{name}\n"));
        out.push_str(f["patch"].as_str().unwrap_or_default());
        out.push('\n');
    }
    out
}

fn route(
    s: &mut State,
    method: &str,
    path: &str,
    query: &BTreeMap<String, String>,
    accept: &str,
    body: &Value,
    authorized: bool,
) -> Reply {
    let base = format!("http://127.0.0.1:{}", s.port);
    let seg: Vec<&str> = path.trim_matches('/').split('/').collect();
    let write = method != "GET";
    if !authorized && (write || !s.anonymous_reads) {
        return message(401, "Bad credentials");
    }
    match (method, seg.as_slice()) {
        ("GET", ["repos", o, r, "issues", n]) if n.parse::<u64>().is_ok() => {
            let n: u64 = n.parse().unwrap();
            if let Some(issue) = s.issues.get(&(format!("{o}/{r}"), n)) {
                return json_reply(200, issue);
            }
            // A pull request is an issue, too (the issues API serves it).
            if let Some(p) = s.pulls.get(&(format!("{o}/{r}"), n)) {
                let mut i = p.clone();
                i["pull_request"] = json!({"url": p["url"], "html_url": p["html_url"]});
                return json_reply(200, &i);
            }
            message(404, "Not Found")
        }
        ("GET", ["repos", o, r, "pulls", n]) if n.parse::<u64>().is_ok() => {
            let n: u64 = n.parse().unwrap();
            match s.pulls.get(&(format!("{o}/{r}"), n)) {
                Some(p) if accept.contains("application/vnd.github.diff") => {
                    let files = s
                        .files
                        .get(&(format!("{o}/{r}"), n))
                        .cloned()
                        .unwrap_or_default();
                    Reply {
                        status: 200,
                        headers: vec![],
                        content_type: "application/vnd.github.diff; charset=utf-8".into(),
                        body: {
                            let _ = p;
                            diff_of(&files)
                        },
                    }
                }
                Some(p) => json_reply(200, p),
                None => message(404, "Not Found"),
            }
        }
        ("GET", ["repos", o, r, "pulls", n, "files"]) if n.parse::<u64>().is_ok() => {
            let n: u64 = n.parse().unwrap();
            if !s.pulls.contains_key(&(format!("{o}/{r}"), n)) {
                return message(404, "Not Found");
            }
            let files = s
                .files
                .get(&(format!("{o}/{r}"), n))
                .cloned()
                .unwrap_or_default();
            paginate(&files, query, &base, path)
        }
        ("GET", ["repos", o, r, "pulls", n, "reviews"]) if n.parse::<u64>().is_ok() => {
            let n: u64 = n.parse().unwrap();
            let v = s
                .reviews
                .get(&(format!("{o}/{r}"), n))
                .cloned()
                .unwrap_or_default();
            paginate(&v, query, &base, path)
        }
        ("GET", ["repos", o, r, "pulls", n, "comments"]) if n.parse::<u64>().is_ok() => {
            let n: u64 = n.parse().unwrap();
            let v = s
                .review_comments
                .get(&(format!("{o}/{r}"), n))
                .cloned()
                .unwrap_or_default();
            paginate(&v, query, &base, path)
        }
        ("GET", ["repos", o, r, "issues", n, "comments"]) if n.parse::<u64>().is_ok() => {
            let n: u64 = n.parse().unwrap();
            let v = s
                .issue_comments
                .get(&(format!("{o}/{r}"), n))
                .cloned()
                .unwrap_or_default();
            paginate(&v, query, &base, path)
        }
        ("POST", ["repos", o, r, "issues", n, "comments"]) if n.parse::<u64>().is_ok() => {
            let n: u64 = n.parse().unwrap();
            let known = s.issues.contains_key(&(format!("{o}/{r}"), n))
                || s.pulls.contains_key(&(format!("{o}/{r}"), n));
            if !known {
                return message(404, "Not Found");
            }
            let text = body["body"].as_str().unwrap_or_default();
            if text.trim().is_empty() {
                return json_reply(
                    422,
                    &json!({"message": "Validation Failed", "errors": [{"resource": "IssueComment", "code": "missing_field", "field": "body"}], "documentation_url": "https://docs.github.com/rest"}),
                );
            }
            let id = push_issue_comment(s, o, r, n, "modbit-bot", text);
            let c = s.issue_comments[&(format!("{o}/{r}"), n)]
                .iter()
                .find(|c| c["id"] == id)
                .cloned()
                .unwrap();
            json_reply(201, &c)
        }
        ("POST", ["repos", o, r, "pulls"]) => {
            let head = body["head"].as_str().unwrap_or_default().to_owned();
            let dup = s.pulls.iter().any(|((repo, _), p)| {
                *repo == format!("{o}/{r}") && p["head"]["ref"] == head && p["state"] == "open"
            });
            if dup {
                return json_reply(
                    422,
                    &json!({"message": "Validation Failed", "errors": [{"resource": "PullRequest", "code": "custom", "message": format!("A pull request already exists for {o}:{head}.")}], "documentation_url": "https://docs.github.com/rest"}),
                );
            }
            let number = s
                .pulls
                .keys()
                .filter(|(repo, _)| *repo == format!("{o}/{r}"))
                .map(|(_, n)| *n)
                .max()
                .unwrap_or(0)
                + 1;
            let title = body["title"].as_str().unwrap_or_default().to_owned();
            let text = body["body"].as_str().unwrap_or_default().to_owned();
            let base_ref = body["base"].as_str().unwrap_or("main").to_owned();
            let port = s.port;
            let now = now_iso();
            let sha = format!("{:040x}", number * 0x1234_5678 + 0x0123_4567_89ab_cdef);
            let pr = json!({
                "url": format!("http://127.0.0.1:{port}/repos/{o}/{r}/pulls/{number}"),
                "id": 9000 + number, "node_id": format!("PR_{number}"),
                "html_url": format!("https://github.test/{o}/{r}/pull/{number}"),
                "number": number, "state": "open", "locked": false, "title": title,
                "user": user("modbit-bot"), "body": text, "created_at": now, "updated_at": now,
                "closed_at": null, "merged_at": null, "merge_commit_sha": null,
                "requested_reviewers": [], "labels": [],
                "head": {"label": format!("{o}:{head}"), "ref": head, "sha": sha, "repo": {"name": r, "full_name": format!("{o}/{r}")}},
                "base": {"label": format!("{o}:{base_ref}"), "ref": base_ref, "sha": "b".repeat(40), "repo": {"name": r, "full_name": format!("{o}/{r}")}},
                "draft": body["draft"].as_bool().unwrap_or(false), "merged": false,
                "mergeable": null, "mergeable_state": "unknown",
                "comments": 0, "review_comments": 0, "commits": 1, "additions": 0, "deletions": 0, "changed_files": 0,
            });
            s.pulls.insert((format!("{o}/{r}"), number), pr.clone());
            json_reply(201, &pr)
        }
        ("GET", ["repos", o, r, "pulls"]) => {
            let head = query
                .get("head")
                .and_then(|h| h.split_once(':').map(|(_, b)| b.to_owned()))
                .unwrap_or_default();
            let state = query.get("state").map_or("open", String::as_str);
            let list: Vec<Value> = s
                .pulls
                .iter()
                .filter(|((repo, _), p)| {
                    *repo == format!("{o}/{r}")
                        && (head.is_empty() || p["head"]["ref"] == head)
                        && (state == "all" || p["state"] == state)
                })
                .map(|(_, p)| p.clone())
                .collect();
            paginate(&list, query, &base, path)
        }
        ("PATCH", ["repos", o, r, "pulls", n]) if n.parse::<u64>().is_ok() => {
            let n: u64 = n.parse().unwrap();
            match s.pulls.get_mut(&(format!("{o}/{r}"), n)) {
                Some(p) => {
                    for k in ["title", "body", "state"] {
                        if let Some(v) = body.get(k) {
                            p[k] = v.clone();
                        }
                    }
                    json_reply(200, p)
                }
                None => message(404, "Not Found"),
            }
        }
        ("GET", ["repos", o, r, "commits", sha, "check-runs"]) => {
            let runs = s
                .check_runs
                .get(&(format!("{o}/{r}"), (*sha).to_owned()))
                .cloned()
                .unwrap_or_default();
            // The forge lists runs of other commits only when asked about
            // them; a commit with no run answers an empty list.
            json_reply(200, &json!({"total_count": runs.len(), "check_runs": runs}))
        }
        _ => message(404, "Not Found"),
    }
}

fn serve(mut stream: std::net::TcpStream, state: &Arc<Mutex<State>>) {
    let mut buf = Vec::new();
    let mut tmp = [0u8; 8192];
    let (head_end, len) = loop {
        let n = stream.read(&mut tmp).unwrap_or(0);
        if n == 0 {
            return;
        }
        buf.extend_from_slice(&tmp[..n]);
        if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            let head = String::from_utf8_lossy(&buf[..pos]).to_string();
            let len = head
                .lines()
                .find_map(|l| {
                    let (k, v) = l.split_once(':')?;
                    k.eq_ignore_ascii_case("content-length")
                        .then(|| v.trim().parse::<usize>().ok())
                        .flatten()
                })
                .unwrap_or(0);
            break (pos + 4, len);
        }
    };
    while buf.len() < head_end + len {
        let n = stream.read(&mut tmp).unwrap_or(0);
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&tmp[..n]);
    }
    let head = String::from_utf8_lossy(&buf[..head_end]).to_string();
    let mut lines = head.lines();
    let request_line = lines.next().unwrap_or_default().to_owned();
    let mut parts = request_line.split(' ');
    let method = parts.next().unwrap_or_default().to_owned();
    let target = parts.next().unwrap_or_default().to_owned();
    let headers: Vec<(String, String)> = lines
        .filter_map(|l| l.split_once(':'))
        .map(|(k, v)| (k.trim().to_ascii_lowercase(), v.trim().to_owned()))
        .collect();
    let header = |name: &str| {
        headers
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.clone())
            .unwrap_or_default()
    };
    let body: Value = if len > 0 {
        serde_json::from_slice(&buf[head_end..head_end + len]).unwrap_or_default()
    } else {
        Value::Null
    };
    let (path, query_text) = target.split_once('?').unwrap_or((target.as_str(), ""));
    let query = parse_query(query_text);
    let reply = {
        let mut s = state.lock().unwrap();
        let authorized = header("authorization") == format!("Bearer {}", s.token);
        s.seen.push(Seen {
            method: method.clone(),
            path: path.to_owned(),
            query: query_text.to_owned(),
            authorized,
            accept: header("accept"),
            body: body.clone(),
        });
        let armed = s.armed.iter_mut().find(|a| {
            a.remaining > 0
                && path.contains(&a.path_contains)
                && a.method.as_deref().is_none_or(|m| m == method)
        });
        if let Some(a) = armed {
            a.remaining -= 1;
            fault_reply(&a.fault)
        } else {
            route(
                &mut s,
                &method,
                path,
                &query,
                &header("accept"),
                &body,
                authorized,
            )
        }
    };
    let reason = match reply.status {
        200 => "OK",
        201 => "Created",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        422 => "Unprocessable Entity",
        429 => "Too Many Requests",
        500 => "Internal Server Error",
        _ => "Status",
    };
    let mut out = format!(
        "HTTP/1.1 {} {reason}\r\ncontent-type: {}\r\ncontent-length: {}\r\nconnection: close\r\nx-github-media-type: github.v3; format=json\r\nx-ratelimit-limit: 5000\r\n",
        reply.status,
        reply.content_type,
        reply.body.len()
    );
    if !reply
        .headers
        .iter()
        .any(|(k, _)| k.eq_ignore_ascii_case("x-ratelimit-remaining"))
    {
        out.push_str("x-ratelimit-remaining: 4999\r\n");
    }
    for (k, v) in &reply.headers {
        out.push_str(&format!("{k}: {v}\r\n"));
    }
    out.push_str("\r\n");
    out.push_str(&reply.body);
    let _ = stream.write_all(out.as_bytes());
    let _ = stream.flush();
}

fn fault_reply(f: &Fault) -> Reply {
    match f {
        Fault::PrimaryRateLimit { reset_epoch_secs } => {
            let mut r = message(
                403,
                "API rate limit exceeded for user ID 1. If you reach out to GitHub Support for help, please include the request ID.",
            );
            r.headers.push(("x-ratelimit-remaining".into(), "0".into()));
            r.headers
                .push(("x-ratelimit-reset".into(), reset_epoch_secs.to_string()));
            r
        }
        Fault::SecondaryRateLimit { retry_after_secs } => {
            let mut r = message(
                403,
                "You have exceeded a secondary rate limit. Please wait a few minutes before you try again.",
            );
            r.headers
                .push(("retry-after".into(), retry_after_secs.to_string()));
            r
        }
        Fault::TooManyRequests { retry_after_secs } => {
            let mut r = message(429, "Too Many Requests");
            r.headers
                .push(("retry-after".into(), retry_after_secs.to_string()));
            r
        }
        Fault::Forbidden => message(403, "Resource not accessible by integration"),
        Fault::ServerError => message(500, "Server Error"),
    }
}
