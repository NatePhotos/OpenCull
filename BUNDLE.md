# Ingest & sessions bundle

Prerequisite: `ideas/gpu-performance`. This branch contains that bundle plus one curated ingest/session commit. Merge GPU first to review separately, or merge this branch to obtain both.

Includes culling cards before copying, in-place review, multi-card sessions, subfolder selection, resumable imports, session history and deleting saved sessions, optional video review, and Linux video streaming. Source deletion remains a separate explicit action. Deleting a saved session does not delete source photos.

## Shared changes kept together

Session restoration and video/card review share the store, native commands, preference hydration and scan protocol. Their earlier settings foundation is included: configurable controls, local preferences, fonts, and basic light/grey themes. The offline Space Grotesk font and its license are included. Later Wisp, interface animation, motion-blur and MobileCLIP features are excluded. Grouping stays at the earlier face/time behavior.

## Provenance

Shared frontend foundation: `e937d03`; session home/history controls: `c5403ea`; completed review/session libraries, ingest components and native modules: `d8dc8a8`. Original feature commits include `29612f1`, `6633231`, `0c4f95b`, `5fad15b` (card review), `aacac1a` (multi-card), `a9ae3c9` (subfolders), `b098294`, `a823f2f`, `c5403ea` (sessions), and `a8c291e`, `0bae754`, `7e2ef6a`, `25251bf`, `06fce92` (video). These are provenance references, not inherited commit history.

## Try or merge

```sh
git clone --branch ideas/ingest-sessions --single-branch https://github.com/wispdevon/OpenCull.git
cd OpenCull
npm ci
npm run desktop
```

Fetch and merge this branch into upstream to obtain its explicit GPU prerequisite and the ingest/session bundle. Do not merge the old restored feature tips as a substitute: their history is preserved for reference, not reorganized.

## Validation

Linux production frontend build; 69 frontend tests and 11 launcher tests passed; 296 native tests passed with CUDA enabled, one hardware benchmark ignored. No macOS/Windows runtime test has been performed on this curated subset. The complete 0.5.0 release is checked separately in CI.
