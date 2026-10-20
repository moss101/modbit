#!/bin/bash
# Sign the macOS actuator helper (PX-072; docs/66 CUC-E01, CUC-E04).
#
#   MODBIT_SIGNING_IDENTITY="Developer ID Application: Example Inc (TEAMID1234)" \
#     scripts/sign-actuator-macos.sh path/to/modbit-actuator-macos
#
# What it does: hardened-runtime code signature with a secure timestamp, then
# `codesign --verify --strict`, then prints the designated requirement the
# manifest should pin (`designated_requirement`). Notarisation is NOT done
# here: the helper ships inside the app bundle and is notarised with the app
# (`xcrun notarytool submit` on the signed .app / .dmg, then `xcrun stapler
# staple`), which is the M10.2 pipeline's step. Order in the pipeline:
#
#   1. cargo build --release -p modbit-actuator-macos   (per architecture; lipo for universal)
#   2. this script                                       (needs the Developer ID identity)
#   3. modbit-actuator-manifest build / sign             (hashes the SIGNED file)
#   4. bundle the helper + actuator-manifest.json + trusted-keys.json under Resources/actuator/
#   5. sign + notarise the app (M10.2)
#   6. modbit-actuator-manifest audit                    (the release gate)
#
# It fails, loudly and before touching anything, without an identity. The
# identity "-" (ad-hoc) is accepted ONLY to prove the plumbing on a developer
# machine; an ad-hoc signature satisfies no Apple-anchored requirement and the
# packaged app must never ship one.
set -euo pipefail

bin="${1:-}"
if [ -z "$bin" ] || [ ! -f "$bin" ]; then
  echo "usage: MODBIT_SIGNING_IDENTITY=<identity> $0 <path to the helper binary>" >&2
  exit 2
fi
if [ -z "${MODBIT_SIGNING_IDENTITY:-}" ]; then
  echo "sign-actuator-macos: MODBIT_SIGNING_IDENTITY is not set." >&2
  echo "  It names the Developer ID Application identity (for example" >&2
  echo "  \"Developer ID Application: Example Inc (TEAMID1234)\") that signs the helper." >&2
  echo "  List the identities in this keychain:  security find-identity -v -p codesigning" >&2
  echo "  Nothing was signed." >&2
  exit 3
fi
if [ "$(uname -s)" != "Darwin" ]; then
  echo "sign-actuator-macos: code signing needs macOS." >&2
  exit 4
fi

if [ "$MODBIT_SIGNING_IDENTITY" = "-" ]; then
  echo "sign-actuator-macos: AD-HOC signature (developer build only; no Apple identity)." >&2
  /usr/bin/codesign --force --sign - "$bin"
  echo "signed ad-hoc: $bin"
else
  /usr/bin/codesign --force --options runtime --timestamp \
    --sign "$MODBIT_SIGNING_IDENTITY" "$bin"
  echo "signed: $bin"
fi
/usr/bin/codesign --verify --strict --verbose=2 "$bin"
echo "designated requirement to pin in the manifest:"
/usr/bin/codesign -d -r- "$bin" 2>&1 | sed -n 's/^designated => //p'
