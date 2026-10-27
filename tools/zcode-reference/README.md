# ZCode reference harness (dossier tooling, never part of the product)

Renders the **unmodified** primitives and stylesheet of ZCode (https://github.com/zai-org/ZCode, Apache-2.0) in the Electron
that Modbit ships, probes them, and writes the retained reference capture under `evidence/zcode-ui/reference/`.
DR-PX-2026-10-24-016 defines what is compared and why; PX-141 and PX-142 read the capture.

The harness uses only ZCode's components (`button`, `input`, `dialog`, ...) and `styles.css`, copied by `setup.sh` into a work
directory **outside this repository** (they are ZCode's Apache-2.0 files, not Modbit code; Tailwind v4 also skips git-ignored
files when it scans for classes, so an in-tree ignored copy renders unstyled), plus the harness sources committed here: two
stubs (i18n and the key-cap badge, under `src/stub/` so Tailwind scans their classes), the Vite config and the specimen page `src/main.tsx`.

## Regenerate

```bash
git clone https://github.com/zai-org/ZCode /tmp/zcode && git -C /tmp/zcode checkout 3c5b227649371507ddc4eb46ac5133afe6db1cab
tools/zcode-reference/setup.sh /tmp/zcode /tmp/zcode-harness      # work directory outside the repository, empty
(cd /tmp/zcode-harness && npx vite --port 5199 --strictPort) &
node tools/zcode-reference/scripts/capture.mjs      # screenshots + zcode-computed.json (16 sheet/theme pairs)
node tools/zcode-reference/scripts/tokens.mjs       # zcode-tokens-resolved.json (every custom property, both themes)
node tools/zcode-reference/scripts/token-rgba.mjs   # zcode-colors-rgba.json (every --color-* as canonical sRGB + alpha)
python3 -I tools/zcode-reference/scripts/contrast.py > evidence/zcode-ui/reference/contrast-zai.txt
```

`setup.sh` refuses any upstream revision other than the pinned one. A different revision is a new capture and a reviewed
diff of `zcode-*.json`, never a silent update. The capture is macOS arm64, Electron 44.3.0 at device scale 2.
