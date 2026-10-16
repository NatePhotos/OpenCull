# Interface & workspace bundle

Prerequisite: `ideas/ai-grouping`, which includes `ideas/ingest-sessions` and `ideas/gpu-performance`. This branch completes the four-bundle stack. It contains the full application at `a1aa61c` plus branch documentation, with four curated feature commits and four append-only cleanup/integration commits beyond upstream OpenCull 0.4.1.

Includes Wisp as a theme, featured home images and home layout controls, ranking palettes, compact settings, settings import/export, typography/scale, inline keyboard settings, first-run overview, resizable native-aspect filmstrips, face spotlight/outlines and face-targeted zoom, crop improvements, and experimental transitions/filmstrip hiding while cropping.

## Why it follows AI and sessions

The final settings page, filmstrip, home page and overlays control the session and AI fields introduced by the preceding bundles. Keeping this full interface intact requires those APIs. GPU/runtime is the independent base; this branch is the complete experience. These are cumulative integration bundles, not four independent feature patches. To contribute a smaller UI-only patch upstream, use the source references and extract that change separately.

## Provenance

Final application snapshot: `a1aa61c`. Sources include configurable UI `a4263b7`, local preferences `f919a33`/`d2e1259`, final Wisp/home/panel consolidation `18c02b7`, ranking palettes `59da368`, face interaction `c45022a`/`f232769`/`be10db6`/`f312435`, transitions `83dc060`, crop behavior `c64c2fe`, and experimental settings order `041f12d`. Every original feature branch and archive tag remains available. The four feature commits reorganize the snapshot; they do not rewrite the restored branches or main/dev.

## Try or merge

```sh
git clone --branch ideas/interface-workspace --single-branch https://github.com/wispdevon/OpenCull.git
cd OpenCull
npm ci
npm run desktop
```

Fetch and merge this branch for all four idea bundles. For everyday development use `dev`; for installers use the tagged 0.5.0 release. These idea branches are curated pull/contribution snapshots, not parallel release channels.

## Validation

This application snapshot has the full 98-test frontend suite, 11 launcher checks, production frontend build and 312 native tests (two hardware benchmarks ignored). The preceding AI bundle has identical native source. The 0.5.0 tag additionally runs Windows, Apple-silicon macOS and Linux build jobs; see the release workflow for their authoritative status.

## Follow-up cleanup

Published history is preserved. An append-only cleanup stack fixes branch-specific clone/build instructions and removes unused MobileCLIP download dependencies from GPU and Ingest. AI restores those dependencies because model installation needs them. The new branch tips retain the cumulative dependency order.
