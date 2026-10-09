//! A larger, deterministic synthetic repository for the retrieval benchmark
//! (REQ-PX-137), with sixty hand-labelled cases.
//!
//! The feature table below is **author-written**: each row names a feature,
//! the three words that identify it, the sentence its source file opens with
//! and the question a developer would ask to find it. The label for a case is
//! the file the generator wrote for that feature. The rest of the repository
//! is filler written from a vocabulary that cannot collide with a feature
//! keyword, plus decoys: files that mention one or two of a feature's
//! keywords in passing, and a test file per feature that mentions all three,
//! so a tool that only counts words cannot find the answer by luck. The
//! generator is seeded and platform-independent: the same bytes on every run.

use std::path::Path;

use modbit_retrieval::bench::Case;

/// (slug, area, three keywords, opening doc sentence, developer question).
const FEATURES: [(&str, &str, [&str; 3], &str, &str); 60] = [
    (
        "retry_backoff",
        "net",
        ["backoff", "jitter", "retry"],
        "Exponential backoff with jitter between retry attempts.",
        "where do failed requests wait with exponential backoff and jitter before another retry",
    ),
    (
        "token_bucket",
        "net",
        ["bucket", "refill", "throttle"],
        "Token bucket that refills at a fixed rate to throttle callers.",
        "throttle callers with a refilling token bucket",
    ),
    (
        "oauth_device",
        "auth",
        ["device", "verification", "polling"],
        "OAuth device flow that shows a verification code while polling.",
        "device authorization flow polling for the verification result",
    ),
    (
        "csv_export",
        "export",
        ["csv", "delimiter", "quoting"],
        "Writes rows as csv with a configurable delimiter and RFC quoting.",
        "write csv rows with quoting and a custom delimiter",
    ),
    (
        "lru_cache",
        "cache",
        ["eviction", "recency", "capacity"],
        "Cache that evicts by recency once capacity is reached.",
        "evict the least recently used entry when the cache capacity is reached",
    ),
    (
        "bloom_filter",
        "cache",
        ["bloom", "bitset", "membership"],
        "Bloom filter membership test over a packed bitset.",
        "probabilistic membership test using a bloom filter bitset",
    ),
    (
        "cron_parser",
        "sched",
        ["cron", "expression", "weekday"],
        "Parses a cron expression including weekday and step fields.",
        "parse a cron expression with weekday fields",
    ),
    (
        "ring_buffer",
        "collections",
        ["wraparound", "overwrite", "circular"],
        "Fixed circular buffer that overwrites the oldest item on wraparound.",
        "circular buffer that overwrites the oldest item after wraparound",
    ),
    (
        "jwt_verify",
        "auth",
        ["jwt", "signature", "audience"],
        "Verifies a jwt signature and checks the audience claim.",
        "verify a jwt signature and the audience claim",
    ),
    (
        "merkle_tree",
        "integrity",
        ["merkle", "inclusion", "proof"],
        "Builds a merkle tree and produces an inclusion proof.",
        "produce a merkle inclusion proof for a leaf",
    ),
    (
        "diff_myers",
        "text",
        ["myers", "shortest", "edit"],
        "Myers shortest edit script between two sequences.",
        "shortest edit script between two sequences using the myers algorithm",
    ),
    (
        "semver_range",
        "pkg",
        ["semver", "caret", "prerelease"],
        "Resolves semver caret ranges including prerelease tags.",
        "resolve a semver caret range with prerelease tags",
    ),
    (
        "rate_window",
        "net",
        ["sliding", "window", "counter"],
        "Sliding window counter for request rates.",
        "count requests per sliding window",
    ),
    (
        "utf8_validate",
        "text",
        ["utf8", "continuation", "overlong"],
        "Validates utf8 and rejects overlong sequences and stray continuation bytes.",
        "reject overlong utf8 sequences and stray continuation bytes",
    ),
    (
        "gzip_stream",
        "io",
        ["gzip", "deflate", "crc"],
        "Streams gzip deflate blocks and verifies the crc at the end.",
        "stream gzip deflate blocks and verify the crc",
    ),
    (
        "dns_resolver",
        "net",
        ["dns", "resolver", "ttl"],
        "Caching dns resolver that honours record ttl.",
        "caching dns resolver honouring the record ttl",
    ),
    (
        "tls_handshake",
        "net",
        ["handshake", "ciphersuite", "alpn"],
        "Negotiates a tls handshake and picks the ciphersuite and alpn protocol.",
        "negotiate the tls handshake ciphersuite and alpn protocol",
    ),
    (
        "websocket_frame",
        "net",
        ["websocket", "opcode", "masking"],
        "Encodes websocket frames with opcode and client masking.",
        "encode websocket frames with opcode and masking",
    ),
    (
        "btree_page",
        "storage",
        ["btree", "page", "overflow"],
        "Splits a btree page when a node overflows.",
        "split a btree page when the node overflows",
    ),
    (
        "wal_replay",
        "storage",
        ["wal", "replay", "checkpoint"],
        "Replays the wal after a crash up to the last checkpoint.",
        "replay the wal after a crash from the last checkpoint",
    ),
    (
        "snapshot_isolation",
        "storage",
        ["snapshot", "isolation", "visibility"],
        "Snapshot isolation with row visibility rules.",
        "row visibility rules for snapshot isolation",
    ),
    (
        "vector_clock",
        "dist",
        ["vector", "clock", "happens"],
        "Vector clock that decides happens-before between replicas.",
        "decide happens-before between replicas with a vector clock",
    ),
    (
        "raft_election",
        "dist",
        ["raft", "election", "term"],
        "Raft leader election that bumps the term on timeout.",
        "raft leader election and the term bump on timeout",
    ),
    (
        "consistent_hash",
        "dist",
        ["consistent", "vnode", "rebalance"],
        "Consistent hashing with virtual nodes and minimal rebalance.",
        "rebalance keys with consistent hashing and virtual nodes",
    ),
    (
        "circuit_breaker",
        "net",
        ["breaker", "halfopen", "tripped"],
        "Circuit breaker with a half-open probe after it has tripped.",
        "circuit breaker half-open probe after it tripped",
    ),
    (
        "feature_flag",
        "config",
        ["flag", "rollout", "bucketing"],
        "Feature flag percentage rollout with stable bucketing.",
        "percentage rollout of a feature flag with stable bucketing",
    ),
    (
        "locale_plural",
        "i18n",
        ["locale", "plural", "cardinal"],
        "Chooses the plural form of a message by locale and cardinal rules.",
        "choose the plural form of a message for a locale",
    ),
    (
        "timezone_shift",
        "time",
        ["timezone", "offset", "daylight"],
        "Converts instants between timezone offsets across daylight changes.",
        "convert between timezones across a daylight saving change",
    ),
    (
        "markdown_table",
        "text",
        ["markdown", "table", "alignment"],
        "Renders a markdown table with column alignment.",
        "render a markdown table with column alignment",
    ),
    (
        "html_sanitize",
        "web",
        ["sanitize", "allowlist", "attribute"],
        "Sanitizes html with a tag and attribute allowlist.",
        "sanitize html using an allowlist of tags and attributes",
    ),
    (
        "sql_planner",
        "db",
        ["planner", "join", "selectivity"],
        "Query planner that orders a join by selectivity.",
        "order a join by selectivity in the query planner",
    ),
    (
        "index_scan",
        "db",
        ["covering", "scan", "rowid"],
        "Covering index scan that avoids the rowid lookup.",
        "covering index scan without a rowid lookup",
    ),
    (
        "pagination_cursor",
        "api",
        ["pagination", "cursor", "opaque"],
        "Opaque cursor pagination for list endpoints.",
        "opaque cursor pagination for a list endpoint",
    ),
    (
        "webhook_signature",
        "api",
        ["webhook", "hmac", "timestamp"],
        "Checks the webhook hmac and rejects a stale timestamp.",
        "check the webhook hmac and reject a stale timestamp",
    ),
    (
        "idempotency_key",
        "api",
        ["idempotency", "dedupe", "storedresponse"],
        "Idempotency key store that dedupes and returns the storedresponse.",
        "dedupe a request by idempotency key and return the stored response",
    ),
    (
        "password_hash",
        "auth",
        ["argon", "salt", "memory"],
        "Password hashing with argon, a random salt and a memory cost.",
        "hash a password with argon salt and memory cost",
    ),
    (
        "totp_code",
        "auth",
        ["totp", "drift", "authenticator"],
        "Time based one time codes for an authenticator with clock drift.",
        "validate a totp code from an authenticator allowing clock drift",
    ),
    (
        "csrf_token",
        "web",
        ["csrf", "double", "submit"],
        "Double submit cookie csrf protection.",
        "double submit cookie csrf protection",
    ),
    (
        "cors_preflight",
        "web",
        ["cors", "preflight", "origin"],
        "Answers a cors preflight by checking the origin.",
        "answer a cors preflight request by checking the origin",
    ),
    (
        "image_resize",
        "media",
        ["resize", "lanczos", "aspect"],
        "Image resize with lanczos filtering and aspect preserved.",
        "resize an image with lanczos filtering keeping the aspect",
    ),
    (
        "audio_resample",
        "media",
        ["resample", "sinc", "channel"],
        "Audio resample using a sinc kernel per channel.",
        "resample audio with a sinc kernel per channel",
    ),
    (
        "pdf_xref",
        "doc",
        ["xref", "stream", "subsection"],
        "Reads a pdf xref stream and its subsection entries.",
        "read the pdf xref stream subsections",
    ),
    (
        "zip_central",
        "io",
        ["zip", "central", "directory"],
        "Reads the zip central directory from the end of the archive.",
        "read the zip central directory",
    ),
    (
        "tar_header",
        "io",
        ["tar", "ustar", "checksum"],
        "Parses a ustar tar header and verifies its checksum.",
        "verify the checksum of a ustar tar header",
    ),
    (
        "mime_sniff",
        "web",
        ["mime", "sniff", "magic"],
        "Sniffs the mime type from magic bytes.",
        "sniff the mime type from magic bytes",
    ),
    (
        "url_normalize",
        "web",
        ["normalize", "percent", "dotsegment"],
        "Normalizes a url by decoding percent escapes and removing dotsegment parts.",
        "normalize a url removing dot segments and decoding percent escapes",
    ),
    (
        "diff3_merge",
        "vcs",
        ["merge", "conflict", "ancestor"],
        "Three way merge that marks a conflict against the common ancestor.",
        "three way merge conflict against the common ancestor",
    ),
    (
        "blame_walk",
        "vcs",
        ["blame", "ancestry", "hunk"],
        "Blame that walks ancestry and attributes each hunk.",
        "blame walk attributing each hunk through ancestry",
    ),
    (
        "pack_index",
        "vcs",
        ["packfile", "delta", "fanout"],
        "Reads a packfile index through its fanout table and resolves delta chains.",
        "resolve a delta chain from a packfile using the fanout table",
    ),
    (
        "glob_match",
        "fs",
        ["glob", "brace", "doublestar"],
        "Glob matching with brace expansion and doublestar.",
        "glob matching with brace expansion and doublestar",
    ),
    (
        "file_watch",
        "fs",
        ["watcher", "debounce", "inotify"],
        "File watcher that debounces inotify events.",
        "debounce inotify events in the file watcher",
    ),
    (
        "lockfile_stale",
        "fs",
        ["lockfile", "stale", "pidcheck"],
        "Lockfile that is stolen when stale after a pidcheck.",
        "steal a stale lockfile after a pidcheck",
    ),
    (
        "tempdir_sweep",
        "fs",
        ["tempdir", "sweep", "expiry"],
        "Sweeps the tempdir for entries past their expiry.",
        "sweep the tempdir for expired entries",
    ),
    (
        "metrics_histogram",
        "obs",
        ["histogram", "quantile", "sketch"],
        "Histogram sketch that answers quantile queries.",
        "answer quantile queries from a histogram sketch",
    ),
    (
        "trace_sampling",
        "obs",
        ["sampler", "parentbased", "head"],
        "Head sampler with a parentbased decision.",
        "parentbased head sampler for traces",
    ),
    (
        "log_rotation",
        "obs",
        ["rotation", "retention", "compress"],
        "Log rotation with retention and compress of old files.",
        "rotate logs keeping retention and compressing old files",
    ),
    (
        "health_probe",
        "ops",
        ["liveness", "readiness", "probe"],
        "Liveness and readiness probe endpoints.",
        "liveness and readiness probe endpoints",
    ),
    (
        "graceful_shutdown",
        "ops",
        ["drain", "sigterm", "grace"],
        "Drains connections on sigterm within a grace period.",
        "drain connections on sigterm within the grace period",
    ),
    (
        "config_layers",
        "config",
        ["overlay", "precedence", "schema"],
        "Configuration overlay with precedence checked against the schema.",
        "configuration overlay precedence validated against the schema",
    ),
    (
        "secret_rotation",
        "sec",
        ["keyring", "rotate", "retire"],
        "Rotate a key in the keyring and retire the old one.",
        "rotate a key in the keyring and retire the old key",
    ),
];

/// A deterministic generator (splitmix64).
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

const CONSONANTS: [&str; 12] = ["k", "m", "p", "t", "v", "z", "r", "l", "n", "s", "b", "d"];
const VOWELS: [&str; 5] = ["a", "e", "i", "o", "u"];
/// English words every repository uses; they appear in queries too, so a tool
/// that ranks by common words alone is penalised.
const COMMON: [&str; 40] = [
    "request", "failed", "wait", "another", "value", "result", "error", "state", "load", "write",
    "read", "find", "list", "name", "path", "size", "time", "data", "check", "build", "range",
    "field", "entry", "event", "cache", "index", "stream", "count", "limit", "order", "frame",
    "block", "table", "token", "version", "update", "remove", "return", "filter", "handle",
];

fn pseudo_word(rng: &mut Rng) -> String {
    let syllables = 2 + rng.below(3);
    let mut w = String::new();
    for _ in 0..syllables {
        w.push_str(CONSONANTS[rng.below(CONSONANTS.len())]);
        w.push_str(VOWELS[rng.below(VOWELS.len())]);
    }
    w
}

fn all_keywords() -> Vec<&'static str> {
    FEATURES.iter().flat_map(|f| f.2).collect()
}

fn filler_text(rng: &mut Rng, lines: usize, keywords: &[&str]) -> String {
    let mut out = String::new();
    for _ in 0..lines {
        let mut line = String::from("// ");
        for _ in 0..(6 + rng.below(8)) {
            let w = if rng.below(4) == 0 {
                COMMON[rng.below(COMMON.len())].to_owned()
            } else {
                pseudo_word(rng)
            };
            // A pseudo word must never be a feature keyword.
            if keywords.contains(&w.as_str()) {
                line.push_str("filler");
            } else {
                line.push_str(&w);
            }
            line.push(' ');
        }
        out.push_str(line.trim_end());
        out.push('\n');
    }
    out
}

/// Write the repository under `dir`: `files` files in all (at least 130),
/// and return the cases. The feature files and their tests are fixed; the
/// rest is filler and decoys.
///
/// # Errors
/// An I/O failure writing the tree.
pub fn write_repo(dir: &Path, files: usize) -> std::io::Result<Vec<Case>> {
    let keywords = all_keywords();
    let mut rng = Rng(0x05EE_D137);
    let mut written = 0usize;
    let put = |rel: &str, body: &str| -> std::io::Result<()> {
        let path = dir.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, body)
    };
    let mut cases = Vec::new();
    for (slug, area, kws, doc, query) in FEATURES {
        let file = format!("src/{area}/{slug}.rs");
        let mut body = format!("//! {doc}\n//!\n//! {} {} {}.\n\n", kws[0], kws[1], kws[2]);
        body.push_str(&format!("pub fn {slug}_apply(input: &[u8]) -> usize {{\n"));
        for k in kws {
            body.push_str(&format!("    // {k} handling for {slug}\n"));
        }
        body.push_str(&filler_text(&mut rng, 12, &keywords));
        body.push_str("    input.len()\n}\n");
        put(&file, &body)?;
        put(
            &format!("tests/{slug}_test.rs"),
            &format!(
                "#[test]\nfn {slug}_works() {{\n    // {} {} {}\n    assert!(true);\n}}\n",
                kws[0], kws[1], kws[2]
            ),
        )?;
        written += 2;
        cases.push(Case {
            id: format!("syn-{slug}"),
            query: query.to_owned(),
            intent: "hybrid".into(),
            relevant: vec![file],
            impacted: vec![],
        });
    }
    // Decoys: each feature is mentioned in passing, one or two keywords at a
    // time, in six files elsewhere.
    for (i, (slug, _, kws, _, _)) in FEATURES.iter().enumerate() {
        for d in 0..6 {
            if written >= files {
                break;
            }
            let dir_no = (i * 6 + d) % 40;
            let mention: Vec<&str> = if d % 2 == 0 {
                vec![kws[d % 3]]
            } else {
                vec![kws[d % 3], kws[(d + 1) % 3]]
            };
            let mut body = filler_text(&mut rng, 20, &keywords);
            body.push_str(&format!(
                "// see also {} in the other module\n",
                mention.join(" and ")
            ));
            put(&format!("lib/m{dir_no:02}/note_{slug}_{d}.rs"), &body)?;
            written += 1;
        }
    }
    let mut n = 0usize;
    while written < files {
        let lines = 24 + rng.below(40);
        let body = filler_text(&mut rng, lines, &keywords);
        put(&format!("lib/m{:02}/f{n:05}.rs", n % 40), &body)?;
        written += 1;
        n += 1;
    }
    Ok(cases)
}

/// Rows of the author-written feature table (for the fixture self-checks).
#[must_use]
pub fn feature_count() -> usize {
    FEATURES.len()
}

/// Whether every feature keyword is unique to its row.
#[must_use]
pub fn keywords_are_unique() -> bool {
    let all = all_keywords();
    let mut sorted = all.clone();
    sorted.sort_unstable();
    sorted.dedup();
    sorted.len() == all.len()
}
