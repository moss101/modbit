#!/bin/bash
# Run the macOS actuator's real-permission tests (PX-071), or SKIP them loudly.
#
# Those tests drive a real AppKit application through the real Accessibility
# API, the real window server and real synthesised events. They need what a CI
# runner and a locked or ungranted Mac do not have, so they are #[ignore]d in
# a normal `cargo test` (libtest lists them as ignored - never as passed) and
# this script is the one place that decides whether they can run:
#
#   services/modbit-actuator-macos/scripts/ax-tests.sh
#
# * Permissions are checked by the helper itself (`--check-permissions`), so
#   the answer is the answer for the process that will run the tests.
# * When they cannot run it prints SKIPPED, with the exact count and the
#   one-time owner steps, writes a GitHub annotation under Actions, and exits 0
#   - unless MODBIT_REQUIRE_AX_TESTS=1, which turns a skip into a failure (use
#   it on a machine that is known to be granted).
#
# One-time owner steps (System Settings > Privacy & Security):
#   1. Accessibility                       -> add the app that runs `cargo`
#                                             (Terminal, iTerm, the Electron app, ...)
#   2. Screen & System Audio Recording     -> add the same app
#   3. Unlock the screen and keep it awake while the tests run.
#   Then quit and reopen that app once (macOS applies a grant to new processes).
set -uo pipefail

root="$(cd "$(dirname "$0")/../../.." && pwd)"
cd "$root"
n=$(grep -c '^#\[ignore' services/modbit-actuator-macos/tests/ax_fixture.rs || true)

skip() {
  echo "" >&2
  echo "################################################################" >&2
  echo "# SKIPPED: $n macOS actuator tests did NOT run (they are not passed)" >&2
  echo "# reason : $1" >&2
  echo "# steps  : System Settings > Privacy & Security > Accessibility AND" >&2
  echo "#          Screen & System Audio Recording > add the app that runs cargo;" >&2
  echo "#          unlock the screen; reopen that app; run this script again." >&2
  echo "################################################################" >&2
  if [ -n "${GITHUB_ACTIONS:-}" ]; then
    echo "::warning title=macOS actuator AX tests SKIPPED::$n tests did not run: $1"
  fi
  if [ "${MODBIT_REQUIRE_AX_TESTS:-}" = "1" ]; then
    echo "MODBIT_REQUIRE_AX_TESTS=1: a skip is a failure." >&2
    exit 1
  fi
  exit 0
}

if [ "$(uname -s)" != "Darwin" ]; then
  skip "not macOS"
fi
if ! command -v swiftc >/dev/null 2>&1; then
  skip "swiftc is not installed (the fixture application is built with it)"
fi

cargo build -q -p modbit-actuator-macos || { echo "building the helper failed" >&2; exit 1; }
target="${CARGO_TARGET_DIR:-$root/target}"
perms="$("$target/debug/modbit-actuator-macos" --check-permissions)"
echo "helper permissions: $perms"
case "$perms" in *'"accessibility":true'*) ;; *) skip "Accessibility is not granted to this process" ;; esac
case "$perms" in *'"screen_recording":true'*) ;; *) skip "Screen Recording is not granted to this process" ;; esac
case "$perms" in *'"secure_desktop":false'*) ;; *) skip "the screen is locked (or a secure prompt is up)" ;; esac

exec cargo test -p modbit-actuator-macos --test ax_fixture -- --ignored --test-threads=1 --nocapture
