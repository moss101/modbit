# PX-168 — lint policy

Classification at start: NOT-FOUND for unwrap/expect, lock-across-await and generic spawn policy (only a `git` spawn rule existed in architecture-lint).
Change: `clippy.toml` (test exemption in configuration), `tools/lint-policy/clippy.toml` (disallowed spawns), `tools/lint_policy.sh`, a CI step in the supply-chain job, and a file-level `#![allow(...)]` with a reason line on each of 94 files that already had expect/unwrap sites or spawn processes (baselined, not rewritten; new sites in other files fail).
Honest residue: the 94 baselined files are not cleaned; inside them a new `expect` is not caught. Linux-only code (guest, execd) could not be linted locally; the CI run is the first lint of those files and may surface more sites to baseline.
