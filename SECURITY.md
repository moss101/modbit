# Security policy

## Reporting a vulnerability

Report privately to the maintainers (security contact: mohsinmustafa@gmail.com) with a description, affected version or commit, and reproduction steps. Please do not open a public issue for an undisclosed vulnerability. You will get an acknowledgement and a remediation plan; fixes ship with an advisory note naming the affected versions.

## Supported versions

Only the latest release and `main` receive fixes.

## Supply-chain gates

CI runs `cargo deny check` (`deny.toml`) on every change: RustSec advisories, yanked crates, license allow-list, wildcard dependencies and unknown registries. Every advisory ignore is listed in `deny.toml` with its reason and is reviewed when the upstream crate releases a fix.
