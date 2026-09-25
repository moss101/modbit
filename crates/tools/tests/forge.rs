//! QUAL-EV-0211, component level of the GitHub forge adapter: the adapter
//! called directly, over a real socket, against a server that answers with
//! GitHub's REST shapes. The integration level is the Core's `forge.*`
//! suites (`qual_px_006_…`, `qual_px_007_…`, `qual_px_010_…` in
//! `services/modbit-core/tests/surface_protocol.rs`); the real-API level
//! waits for a token and a test repository (DR-M6-002).

use std::sync::{Arc, Mutex};

use modbit_tools::forge::{ForgeConfig, read_issue};
use serde_json::json;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

/// A value in the token's place; not token-shaped, so only the held-value
/// rule of the redactor can find it.
const TOKEN: &str = "forge-token-qual-ev-0211-5d1e";

/// Every request's `(method path, headers)`; answers `/repos/acme/widgets/issues/7`
/// with an issue, anything else 404 the way GitHub does.
async fn github() -> (String, Arc<Mutex<Vec<(String, Vec<(String, String)>)>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let kept = Arc::clone(&seen);
    tokio::spawn(async move {
        loop {
            let Ok((mut sock, _)) = listener.accept().await else {
                return;
            };
            let kept = Arc::clone(&kept);
            tokio::spawn(async move {
                let mut buf = Vec::new();
                let mut tmp = [0u8; 4096];
                while !buf.windows(4).any(|w| w == b"\r\n\r\n") {
                    let n = sock.read(&mut tmp).await.unwrap_or(0);
                    if n == 0 {
                        return;
                    }
                    buf.extend_from_slice(&tmp[..n]);
                }
                let head = String::from_utf8_lossy(&buf).to_string();
                let mut lines = head.lines();
                let first = lines.next().unwrap_or_default().to_owned();
                let headers: Vec<(String, String)> = lines
                    .filter_map(|l| l.split_once(':'))
                    .map(|(k, v)| (k.trim().to_ascii_lowercase(), v.trim().to_owned()))
                    .collect();
                let path = first.split(' ').nth(1).unwrap_or_default().to_owned();
                kept.lock().unwrap().push((first, headers));
                let (status, body) = if path == "/repos/acme/widgets/issues/7" {
                    (
                        "200 OK",
                        json!({
                            "number": 7,
                            "title": "Totals are wrong for negative amounts",
                            "body": format!("Steps to reproduce. (An echoed credential: {TOKEN})"),
                            "state": "open",
                            "user": {"login": "reporter"},
                            "labels": [{"name": "bug"}],
                            "html_url": "https://github.test/acme/widgets/issues/7",
                            "updated_at": "2026-09-26T00:00:00Z",
                        }),
                    )
                } else {
                    (
                        "404 Not Found",
                        json!({"message": "Not Found", "documentation_url": "https://docs.github.com/rest"}),
                    )
                };
                let body = body.to_string();
                let _ = sock
                    .write_all(
                        format!(
                            "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                            body.len()
                        )
                        .as_bytes(),
                    )
                    .await;
            });
        }
    });
    (format!("http://127.0.0.1:{port}"), seen)
}

fn config(api_base: &str) -> ForgeConfig {
    ForgeConfig {
        kind: "github".into(),
        api_base: api_base.into(),
        web_host: "github.test".into(),
        token: Some(TOKEN.into()),
    }
}

/// The adapter reads an issue in GitHub's shape, with the token in the
/// `Authorization` header and nowhere in what it returns; GitHub's refusal
/// is typed; a URL on another host is refused before any byte is sent.
#[tokio::test]
async fn forge_adapter_reads_github_issues_directly_with_the_token_in_its_header_only() {
    let (base, seen) = github().await;
    let cfg = config(&base);

    let issue = read_issue(&cfg, "https://github.test/acme/widgets/issues/7")
        .await
        .expect("the issue is read");
    assert_eq!(issue["provenance"], "forge_issue");
    assert_eq!(issue["trust"], "UNTRUSTED_EXTERNAL_CONTENT");
    assert_eq!(
        (
            issue["number"].as_u64(),
            issue["title"].as_str(),
            issue["author"].as_str()
        ),
        (
            Some(7),
            Some("Totals are wrong for negative amounts"),
            Some("reporter")
        )
    );
    assert_eq!(issue["labels"], json!(["bug"]));
    assert!(
        !issue.to_string().contains(TOKEN),
        "a token the forge echoed back never reaches the caller: {issue}"
    );

    {
        let requests = seen.lock().unwrap();
        assert_eq!(requests.len(), 1);
        let (line, headers) = &requests[0];
        assert!(
            line.starts_with("GET /repos/acme/widgets/issues/7 "),
            "{line}"
        );
        let header = |name: &str| {
            headers
                .iter()
                .find(|(k, _)| k == name)
                .map(|(_, v)| v.clone())
        };
        assert_eq!(header("authorization"), Some(format!("Bearer {TOKEN}")));
        assert_eq!(
            header("accept").as_deref(),
            Some("application/vnd.github+json")
        );
        assert_eq!(
            header("x-github-api-version").as_deref(),
            Some("2022-11-28")
        );
    }

    let (code, message) = read_issue(&cfg, "https://github.test/acme/widgets/issues/404")
        .await
        .expect_err("GitHub answers 404");
    assert_eq!(code, "FORGE_NOT_FOUND", "{message}");

    let before = seen.lock().unwrap().len();
    let (code, _) = read_issue(&cfg, "https://elsewhere.example/acme/widgets/issues/7")
        .await
        .expect_err("another host");
    assert_eq!(code, "EGRESS_DENIED");
    assert_eq!(seen.lock().unwrap().len(), before, "nothing was sent");
}
