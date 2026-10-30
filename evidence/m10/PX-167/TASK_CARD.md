# PX-167 — dependency advisory, license and unused-crate gates, and a SECURITY.md

Classification at start: NOT-FOUND (no deny.toml, no SECURITY.md, no supply-chain CI job).
First missing link: no gate ran RustSec, license or unused-dependency checks on the repository.

Change: `deny.toml` (advisories, yanked, license allow-list, wildcards, sources), `SECURITY.md`, a `supply-chain` CI job (cargo-deny, the ignore list with each reason, `tools/check_unused_crates.sh` against `tools/unused-crates-baseline.txt`), and a lockfile update that fixed six advisories (rustls, rustls-webpki, webpki-roots, tokio, zstd-sys, ctutils).

Honest residue: five advisories (quick-xml via object_store, zstd/zstd-safe via tantivy) have no semver-compatible fix; each is ignored in deny.toml with its reason and exposure analysis. 29 pre-existing unused dependencies are baselined, not removed.

Proof: mutation-check.txt (planted vulnerable and denied-license crates fail, unmodified tree passes); the job itself runs in CI on the PR.
