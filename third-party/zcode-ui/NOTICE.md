# Third-party notice: the ZCode design language in the Modbit desktop renderer

Modbit's desktop renderer adopts the design language of **ZCode** (https://github.com/zai-org/ZCode), which is licensed under the
**Apache License, Version 2.0**. The licence text is `LICENSE` in this directory. The decision to do so, its scope and its limits
are recorded in `docs/decisions/DR-PX-2026-10-24-016-zcode-design-language.md`.

| | |
|---|---|
| Upstream | ZCode, Copyright 2026 Z.AI Co., Ltd, Apache-2.0 |
| Revision studied and used | `3c5b227649371507ddc4eb46ac5133afe6db1cab` (2026-10-20) |
| What is used | Design-token names and values (colour tokens of the Zai Light and Zai Dark themes, the `text-ui-*` type scale, the radius hierarchy, the overlay shadows, the font stacks), the sizing and geometry rules of its `DESIGN.md`, and the visual recipes of its interface primitives re-expressed as CSS. Values that ZCode takes from the Tailwind CSS palette are the colours that palette resolves to (Tailwind CSS, MIT: `TAILWIND-LICENSE`). |
| What is not used | No ZCode source file, component, asset, image, font, string of copy, logo, wordmark or product name is part of Modbit. |
| Modifications (Apache-2.0 section 4(b)) | Every token value is stored as a machine-resolved sRGB literal in `packages/design-tokens/src/design-language.ts` (generated, not copied); Modbit's role layer, accessibility exceptions and CSS are Modbit's own work. Each changed or derived file says so in its header. |
| Trademarks (section 6) | ZCode, Z.ai and Zai are marks of their owner. Modbit does not use them, is not affiliated with or endorsed by Z.AI Co., Ltd, and shows no ZCode branding. |
| Reference capture | `evidence/zcode-ui/reference/` and `tools/zcode-reference/` render ZCode's unmodified primitives in a work directory outside this repository, to measure fidelity. They are dossier tooling and are not shipped. |

## Icons

When the renderer's icon table (PX-143) inlines path data from **Lucide** (https://lucide.dev), the licences are `LUCIDE-LICENSE`
(ISC for Lucide, and MIT for the icons derived from the Feather project, both reproduced there). No icon package is a dependency.

## Distribution

The installer and updater work of M10.2 includes this directory in the shipped notices and generates the SBOM entry
(docs/36, docs/70). Until then these files are the notice of record in the repository.
