# Provenance of the ZCode reference capture

| Field | Value |
|---|---|
| Upstream | https://github.com/zai-org/ZCode (Apache-2.0) |
| Revision | `3c5b227649371507ddc4eb46ac5133afe6db1cab` ("chore: sync ZCode source", 2026-10-20; README: v3.16.2) |
| Components | ZCode `packages/ui/src/components/ui/*` and `styles.css`, unmodified except the removal of six asset imports (`tools/zcode-reference/scripts/transform_styles.py`) |
| Toolchain of the specimen | React 19.2.7, Tailwind CSS 4.2.2, radix-ui 1.4.3, lucide-react 1.17.0, class-variance-authority 0.7.1, tailwind-merge 3.5.0, tw-animate-css 1.4.0, Vite 8.0.8 (all as locked by ZCode's `pnpm-lock.yaml`) |
| Engine | Electron 44.3.0 (the version `apps/desktop` pins), macOS arm64, content size 1100 x 900, device scale 2 |
| Themes | Zai Light (`html.theme-zai-light`) and Zai Dark (`html.dark.theme-zai-dark`), applied exactly as ZCode's `useTheme.ts` does |
| Captured | 2026-10-24 |
| Recipe | `tools/zcode-reference/` (rerun on 2026-10-24 from a clean work directory outside the repository: `zcode-computed.json`, `zcode-tokens-resolved.json` and `zcode-colors-rgba.json` came back byte-identical) |
| Not captured | the user's installed ZCode 3.16.3 (single-instance lock held by the owner's running instance; not driven, no owner data read) |

Files: `zcode-computed.json` (probe records: computed style and geometry per `data-slot` element, 8 sheets x 2 themes),
`zcode-tokens-resolved.json` (every custom property as the browser resolved it), `zcode-colors-rgba.json` (every `--color-*`
as canonical sRGB with alpha), `contrast-zai.txt` (WCAG ratios of ZCode's own pairs), `*-light.png` / `*-dark.png`
(screenshots; supplemental, never proof).
