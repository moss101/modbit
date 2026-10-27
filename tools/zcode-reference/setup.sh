#!/usr/bin/env bash
# Prepares the reference harness from a checkout of https://github.com/zai-org/ZCode at the pinned revision.
# usage: tools/zcode-reference/setup.sh /path/to/ZCode /path/to/an/empty/work/dir
# The work directory must be OUTSIDE this repository: Tailwind v4 skips git-ignored files when it scans for classes, and the
# upstream files copied here are Apache-2.0 ZCode files, not Modbit code, so they never enter the tree.
# Copies (never edits) the upstream primitives and stylesheet into the work directory next to the committed harness sources,
# then installs the pinned dependencies there.
set -euo pipefail
UP="${1:?path to a ZCode checkout}"
W="${2:?path to an empty work directory outside this repository}"
REV="3c5b227649371507ddc4eb46ac5133afe6db1cab"
[ "$(git -C "$UP" rev-parse HEAD)" = "$REV" ] || { echo "the checkout must be at $REV" >&2; exit 1; }
HERE="$(cd "$(dirname "$0")" && pwd)"
case "$(cd "$W" 2>/dev/null && pwd || echo "$W")" in "$HERE"/*|"$(git -C "$HERE" rev-parse --show-toplevel)"/*) echo "the work directory must be outside the repository" >&2; exit 1;; esac
mkdir -p "$W"
[ -z "$(ls -A "$W")" ] || { echo "the work directory must be empty" >&2; exit 1; }
cp -R "$HERE/harness/." "$W/"
mkdir -p "$W/src/components/ui" "$W/src/components/lib"
for f in button input textarea badge card tabs switch checkbox kbd separator tooltip dialog alert-dialog dropdown-menu popover select scroll-area label alert progress radio-group input-group button-group spinner command context-menu collapsible toast; do
  cp "$UP/packages/ui/src/components/ui/$f.tsx" "$W/src/components/ui/"
done
cp "$UP/packages/ui/src/components/lib/utils.ts" "$W/src/components/lib/utils.ts"
cp "$UP/packages/ui/lucide-colors.mjs" "$W/lucide-colors.mjs"
python3 -I "$HERE/scripts/transform_styles.py" "$UP/packages/ui/src/styles.css" "$W/src/styles.css"
(cd "$W" && npm install --no-audit --no-fund --loglevel=error)
echo "harness ready: (cd $W && npx vite --port 5199 --strictPort), then node $HERE/scripts/capture.mjs"
