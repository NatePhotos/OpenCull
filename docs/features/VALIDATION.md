# Linux feature integration validation

The seven feature branches start independently at upstream release 0.4.1. Each has its own fork PR; none requires the others. `integration/linux-features` combines them and resolves their overlapping settings, session, and ingest interfaces. General Node/toolchain fixes are committed directly to fork `main` and merged into integration.

## Review units

| Change | Branch | Fork PR |
|---|---|---|
| Agent workflow | `docs/agent-workflow` | [#1](https://github.com/wispdevon/OpenCull/pull/1) |
| Workspace controls | `feature/configurable-ui` | [#2](https://github.com/wispdevon/OpenCull/pull/2) |
| Offline fonts | `feature/font-selection` | [#3](https://github.com/wispdevon/OpenCull/pull/3) |
| Dark/light/18% grey | `feature/appearance-modes` | [#4](https://github.com/wispdevon/OpenCull/pull/4) |
| XDG configuration | `feature/xdg-config` | [#5](https://github.com/wispdevon/OpenCull/pull/5) |
| Cull before import | `feature/cull-before-ingest` | [#6](https://github.com/wispdevon/OpenCull/pull/6) |
| Multi-card sources | `feature/multi-card-session` | [#7](https://github.com/wispdevon/OpenCull/pull/7) |
| Video playback | `feature/video-culling` | [#8](https://github.com/wispdevon/OpenCull/pull/8) |
| RapidRAW inspired design | `feature/rapidraw-theme` | [#10](https://github.com/wispdevon/OpenCull/pull/10) |
| Precise card subfolders | `feature/ingest-subfolders` | [#11](https://github.com/wispdevon/OpenCull/pull/11) |
| Larger headings and review guidance | `feature/ui-changes` | [#12](https://github.com/wispdevon/OpenCull/pull/12) |
| Default and remembered views | `feature/view-mode-preferences` | [#13](https://github.com/wispdevon/OpenCull/pull/13) |

The design and subfolder branches also start directly from upstream. The UI
branch starts from `feature/cull-before-ingest`, because its instruction text
describes that workflow; its PR targets that branch to isolate the UI diff.

## Additional welcome, folder, and heading checks

The latest integration passes 52 frontend tests and a Linux debug desktop
build. Standalone design/subfolder/UI branches pass their frontend tests and
TypeScript/Vite builds. No Rust logic changed in these additions.

The in-app browser remained unavailable, so a separate headless Chromium
instance checked the rendered screens. No existing user browser was controlled.
Theme checks covered selection, reload persistence, offline font application,
font opt-out, restoration of the default design, and 1440px/600px layouts without
horizontal overflow or JavaScript errors. Combined light/dark/grey modes keep
the expected native colour scheme and grey's `#767676` photo canvas.

Folder checks observed no photo scan when navigating CARD or DCIM, a scan of
only `DCIM/100CANON` after selecting it, and an explicit parent selection with
**Use this folder**. Combined multi-source selection retained earlier sources.
Heading checks verified a 24px ingest heading, 14px instruction immediately
below it, no duplicate sidebar instruction, and 20px shared dialog titles.

View preferences add a default opening view and an enabled-by-default
**Remember last view** control under **Settings → Layout**. The standalone
branch passes 35 frontend tests and a TypeScript/Vite build. Combined browser
checks verify Filmstrip/Loupe, Stack/Scene, and Grid across folder changes and
reloads, explicit defaults with remembering disabled, and keyboard changes
written to preferences. The runtime check follows the UI's nonblocking folder
opening path and waits for the listing rather than for background metadata.

The updated executable opened successfully in the user's Wayland session with
`WEBKIT_DISABLE_DMABUF_RENDERER=1`. An unmodified launch previously exited with
a Wayland protocol error. The previous app window was left running so its
pending card selection was preserved.

## Verified locally

- Clean-clone quickstart: a fresh remote clone of `integration/linux-features`
  installed with pinned `npm ci` and passed `npm run check` (52 frontend tests,
  production frontend build, and 283 Rust tests). The clone contained no local
  patches or dependency symlinks. Cargo reused cached native dependencies.
- `npm run dev:desktop` passed its Node guard, started Vite on port 1420, and
  opened the real desktop app in the user's Wayland session. This uses the
  development URL and Rust watcher rather than a packaged frontend. The Linux
  graphics workaround was supplied only to that local process's environment.
- Platform review: Linux video streaming and XDG path code/tests are guarded by
  Linux target configuration. macOS uses its native app-config location,
  asset-protocol video playback, and `turbo-system` JPEG feature. macOS setup/CI
  now explicitly installs Homebrew `jpeg-turbo` and `pkgconf`. Native macOS
  verification is tracked by [the cross-platform run](https://github.com/wispdevon/OpenCull/actions/runs/38796608300);
  it was still in progress when the local desktop development session opened.
- Toolchain: Node 22.23.3, npm 11.21.1. Clean `npm ci`, then `npm run check`: 46 frontend tests, frontend production build, and 283 Rust library tests pass on the combined branch. Fork main separately passes its 31 frontend tests, production build, and 268 Rust tests.
- Desktop: `npm run build:desktop -- --debug --no-bundle` succeeds on Linux with libjpeg-turbo enabled.
- Native self-test: opens files, loads previews, transfers without changing originals, and exercises the new features through real Tauri IPC and the existing store.
- Separate synthetic RAW/JPEG cards pair into shared entries. Local marks survive save/load; reviewing creates no source sidecars. Selected-only copying includes companions, and import in place retains only selected members and their marks.
- MP4 metadata, playback, pause, and seeking work on Linux. WebKitGTK cannot play the custom `asset:` scheme directly, even with a working H.264 decoder. The Linux feature uses a bounded, token-protected loopback HTTP stream, with range/authorization tests. Other systems retain Tauri's scoped asset protocol.
- Offline Space Grotesk loads. Theme controls change the actual root theme; dark/light/grey appearances and layout controls render. Native screenshots were inspected outside the repository.
- A malformed config remains byte-for-byte intact and displays a recovery banner while the app remains usable. An unwritable config root creates no partial file and displays the same recovery path.
- SHA-256 hashes and file membership of all nine synthetic source files remain unchanged after native checks. Test photos, videos, snapshots, and session data are outside the repository.

## Reproducing native feature checks

Use an isolated XDG config/data/cache directory. The combined native `--selftest <photo-folder>` runs baseline checks and the additional integration checks. Supply two or more still images; include a playable MP4 to test video. An optional `selfTestSources` array in the temporary config can point at separate RAW and JPEG source folders to verify cross-card pairing. This option is read only by the integration self-test, not normal application flows.

The browser plugin reported `Browser is not available: iab`; rendered validation used the real Linux GTK/WebKit application under a private X display instead. No external browser or existing user browser session was controlled.

These fixtures validate session, pairing, copying, persistence, and media delivery. They are synthetic previews with RAW filenames, not real sensor-data captures. Original RAW parsing retains its existing Rust tests. Physical card-reader behavior and macOS/Windows runtime behavior still need reports from those systems; the fork's cross-platform build is separate from local Linux validation.

## Personal configuration

`$XDG_CONFIG_HOME/opencull/config.json` (normally `~/.config/opencull/config.json`):

```json
{
  "theme": "grey",
  "font": { "choice": "space-grotesk", "family": "" },
  "ignoreVideos": true
}
```

Existing panel defaults remain unchanged. These are personal values; standalone feature branches preserve the original first-run defaults.


## October 16 follow-up

- Combined frontend: 57 tests pass; TypeScript/Vite production build passes.
- Combined Rust library: 285 tests pass, including retained session history and legacy session compatibility.
- Browser runtime: mixed native-aspect widths, fixed full-width stack filmstrip geometry, and the experimental 320ms cubic click-zoom transition verified after a fresh dev-server launch.
- Linux native preview fix: WebKitGTK dev-origin CORS allowlist enables opencull://localhost previews. Reproduced failure with the DMABUF flag both set and unset; verified JPEG and real RW2 preview dimensions after the fix.
- Stack placement lives under Layout in the combined app. Standalone branch uses Culling.
- New standalone branches: feature/recent-sessions (depends on cull-before-ingest), feature/stack-filmstrip-layout, feature/experimental-filmstrip-aspect, feature/experimental-smooth-zoom.
- Native macOS runtime remains unverified locally.


## Interface scale and compact home follow-up

- Interface scale uses native webview page zoom, with a saved 80–200% range and Reset. Located under Settings → Layout in the combined app. Native Linux inspection verified 150% (960×600 CSS viewport) and 200% (720×450), bounded Settings dialogs, no horizontal overflow or errors, and 200% retained after reload.
- Typography uses clamped container-relative heading sizes with rem bounds and fixed fallbacks. Identity stays above the horizontal toolbar in both visual themes.
- RapidRAW recent-folder contact sheets remain visible. Its session cards measured 304px versus 437px in the default preset at the same 1440px viewport; card layouts and toolbar hierarchy verified in browser runtime.
- User rejected the monochrome mountain demo. demo/rapidraw-welcome is excluded from integration and the app continues to use its original opening page.
- Frontend suite: 58 tests pass; combined production frontend build passes. Native macOS scaling has not been run locally.

## Contrast, strips, history deletion and graphics follow-up

- 61 frontend tests pass; production frontend build passes. 288 Rust tests pass, including session removal/promotion, legacy cleanup and graphics-profile choice.
- Wisp's measured setting title/detail contrast meets 4.5:1, and row boundaries meet 3:1, in dark, light and grey. Active tabs have explicit boundaries. Guidelines are in DESIGN.md.
- Browser runtime verifies session deletion, bulk controls hidden before selection, source-history separation, whole-window OpenCull layout after theme switching, both strip resize controls, and hide/show for both classic stack strips.
- Native aspect covers both the active-frame and scene overview strips. Decoded upright JPEG dimensions supply early proportions; full camera dimensions remain separate and supersede the preview ratio.
- This NVIDIA/Wayland test hit protocol error 71 with direct DMA-BUF. The narrower FORCE_SHM/DISABLE_GBM transport keeps the accelerated renderer enabled. An isolated CSS-only WebKit animation used the RTX 3060 Ti with 53 MiB allocated; no WebGL probe was required. NVIDIA profile requests hardware compositing/WebGL/available 2D canvas acceleration and uses this transport. CPU decoding remains separate. No zoom-performance gain is claimed without representative timings.
- The earlier combined macOS Apple-silicon build passed. Fork main's new macOS dev startup smoke passed, checking the native process and Vite frontend. The final pre-CUDA combined macOS run passed tests, desktop development startup and build; Intel Mac runtime is not covered.


## Final native NVIDIA check

The actual OpenCull WebKit process (not an isolated WebGL probe) was observed using the RTX 3060 Ti with a 311 MiB graphics allocation while rendering real RAW previews. No failed previews were found in the inspected sample. This uses FORCE_SHM/DISABLE_GBM presentation, keeping hardware compositing enabled. The broad WEBKIT_DISABLE_DMABUF_RENDERER flag is cleared only for this opt-in profile. Native Linux desktop is running via npm run dev:desktop:nvidia. GPU acceleration does not imply CPU image decoding or AI inference has moved to GPU.

Additional feature PRs: [Wisp theme](https://github.com/wispdevon/OpenCull/pull/19), [resizable stack strips](https://github.com/wispdevon/OpenCull/pull/20), [NVIDIA rendering](https://github.com/wispdevon/OpenCull/pull/21). Wisp depends on the RapidRAW preset infrastructure; the resize and NVIDIA branches start from upstream. The combined macOS dev-startup smoke at source fb2a83a passed; the final pre-CUDA combined run at 790b38b passed macOS tests, desktop development startup and build ([run](https://github.com/wispdevon/OpenCull/actions/runs/38804193750)).


## Shared dev branch and CUDA trial

- The remote integration branch was renamed to `dev` for cross-platform testing. Fork `main` receives the approved combined work; all 20 feature branch heads were retained during the rename. The discarded mountain demo remains excluded.
- The `dev` README clones `wispdevon/OpenCull` with `--branch dev`. The `main` README uses a normal clone of the same fork without a branch flag.
- The combined branch passes 61 frontend tests, TypeScript/Vite build, two launcher platform guards and 289 Rust library tests with the ordinary CPU build.
- CUDA is an optional Linux x86_64 dependency, excluded on macOS/Windows/ARM even when its Cargo feature is requested. macOS dependency-graph inspection confirmed no CUDA crate/provider feature; CI additionally tests this on macOS.
- Native CUDA test: four concurrent callers share four model sessions; all four models match CPU within the stated tolerance at batches one and eight. A real-photo native analysis used all four CUDA sessions; a separate process with no visible GPU successfully retried on CPU.
- Requested-size device-arena growth saved about 8% of sampled peak VRAM, with no meaningful system-RAM reduction. Workspace changes and explicit reusable GPU input buffers were omitted after measurement. See [experiment](../CUDA-BENCHMARK.md) and [trial requirements/commands](../CUDA-TRIAL.md).
- The standalone CUDA branch passed a debug desktop build and its 269 Rust/31 frontend tests. The combined CUDA debug desktop build and native concurrent GPU comparison also passed. The latest main CI reports its exact source commit; local checks cannot verify an Intel Mac or Windows NVIDIA runtime.
- [CUDA feature PR](https://github.com/wispdevon/OpenCull/pull/22). The CUDA launcher is independent from the Linux NVIDIA renderer profile; normal launches remain CPU inference.
