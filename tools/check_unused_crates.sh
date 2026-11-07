#!/usr/bin/env bash
# PX-167: fail on an unused dependency that is not in the recorded baseline.
# The baseline lists the existing findings (crate dep); shrink it, never grow it.
set -euo pipefail
cd "$(dirname "$0")/.."
found=$( { cargo machete 2>/dev/null || true; } | awk '/ -- /{c=$1;next} /^\t/{gsub(/^\t/,"");print c" "$0}' | sort)
new=$(comm -13 <(sort tools/unused-crates-baseline.txt) <(printf '%s\n' "$found" | sed '/^$/d') || true)
if [ -n "$new" ]; then
  echo "unused dependencies not in tools/unused-crates-baseline.txt:" >&2
  echo "$new" >&2
  exit 1
fi
echo "no new unused dependencies ($(grep -c . tools/unused-crates-baseline.txt) baselined)"
