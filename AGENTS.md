# Working on OpenCull

OpenCull is an offline photo-culling desktop application: React/TypeScript in `src/`, Rust/Tauri in `src-tauri/src/`. Read `README.md` and `docs/HOW-IT-WORKS.md` before changing behavior.

## Branches and review

- Work directly on `dev`. Do not create feature branches or worktrees unless the user explicitly asks.
- Keep `main` as the release branch; merge or publish to it only when requested.
- Commit focused, verified changes on `dev`; do not mix unrelated formatting or dependency work.
- Inspect Git status first and preserve unrelated edits. Never reset or stash someone else's work.
- Existing work branches are archived under `archive/<date>/...` tags before their local or origin branch names are removed. Preserve distinct local and remote tips. Never delete `dev`, `main`, or upstream refs as part of cleanup.
- Do not force-push shared branches. Resolve conflicts deliberately and rerun affected checks.

## Verification

- `npm ci` installs the locked dependencies. Use Node 22.12+ (CI uses Node 22).
- `npm run test:web` runs frontend tests; `npm run test:rust` runs Rust library tests; `npm test` runs both.
- `npm run build` checks TypeScript and builds the frontend. There is no lint script: do not claim one passed.
- For desktop changes, also run `npm run tauri build -- --debug --no-bundle` and relevant runtime checks. Linux requires GTK/WebKitGTK and the Tauri prerequisites; see the README and CI.
- Add focused regression tests for behavior, using existing Vitest and inline Rust test conventions. Do not weaken checks to make a branch pass.
- Documentation-only work needs diff/link review, not an application rebuild.
- PR descriptions explain the problem, resulting behavior, validation, platform limitations, and any prerequisites. Report blocked checks honestly.

## Product boundaries

- Keep photos private: processing stays local; no remote font or media requests.
- Preserve original files. Card review, rejecting, and copying must never delete sources. Source deletion must be a distinct explicitly confirmed action.
- Preserve sidecar compatibility, RAW/JPEG associations, keyboard navigation, and existing normal-folder behavior.
- Settings belong behind the shared preferences interface. Retain existing defaults in upstream-facing features; personal defaults belong in local configuration.
- Support Linux without introducing macOS/Windows regressions. Test on available platforms and state what was not tested.
- Keep browser demo and native Tauri paths working. Use semantic theme tokens, accessible labels, and the existing UI component system.
