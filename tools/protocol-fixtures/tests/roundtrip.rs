//! Cross-language round trip, Rust side (M0.3 acceptance):
//! 1. the committed Rust fixtures equal a fresh encoding (determinism);
//! 2. every TypeScript-encoded fixture decodes in Rust to the same message and
//!    re-encodes to exactly the Rust bytes (TS -> Rust direction).
//!
//! The TS fixtures are produced by the `packages/surface-protocol` tests.

use protocol_fixtures::{fixture_dir, samples};

#[test]
fn committed_rust_fixtures_are_fresh() {
    let dir = fixture_dir("rust");
    for s in samples() {
        let on_disk = std::fs::read(dir.join(format!("{}.bin", s.name)))
            .unwrap_or_else(|e| panic!("{}: {e}; run `cargo run -p protocol-fixtures`", s.name));
        assert_eq!(
            on_disk, s.bytes,
            "{} is stale; run `cargo run -p protocol-fixtures`",
            s.name
        );
    }
}

#[test]
fn typescript_encoded_fixtures_decode_and_reencode_identically() {
    let dir = fixture_dir("ts");
    for s in samples() {
        let ts_bytes = std::fs::read(dir.join(format!("{}.bin", s.name))).unwrap_or_else(|e| {
            panic!(
                "{}: {e}; run `pnpm --filter @modbit/surface-protocol test`",
                s.name
            )
        });
        let reencoded = s
            .reencode(&ts_bytes)
            .unwrap_or_else(|e| panic!("{}: {e}", s.name));
        assert_eq!(
            reencoded, s.bytes,
            "{}: TS bytes decode to a different message",
            s.name
        );
        assert_eq!(
            ts_bytes, s.bytes,
            "{}: TS and Rust canonical encodings differ",
            s.name
        );
    }
}
