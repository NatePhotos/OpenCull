# Try every OpenCull feature on macOS

Share this link: https://github.com/wispdevon/OpenCull/tree/dev

The `dev` branch already combines the approved feature branches across platforms. Do not merge branches by hand or use fork main for the combined preview. Experimental features are available in Settings and remain opt-in. The discarded mountain welcome-page demo is excluded.

## Prerequisites

Install Xcode Command Line Tools (`xcode-select --install`), Homebrew, Rust stable from https://rustup.rs, and Node 22.23.3 (for example with nvm). Then:

```sh
brew install jpeg-turbo pkgconf
git clone --branch dev --single-branch https://github.com/wispdevon/OpenCull.git
cd OpenCull
nvm install
nvm use
npm install --global npm@11.21.1
npm ci
npm run desktop
```

If Node 22.23.3 and npm 11.21.1 are already installed, omit the nvm and global npm steps. The first Rust compile can take several minutes. Keep the terminal open. This starts the Vite frontend and native Tauri backend together; it does not create an installer. A browser-only preview cannot ingest actual cards or exercise native image decoding.

macOS automatically uses the Homebrew libjpeg-turbo configuration. Do not set Linux WebKit, DMA-BUF, CUDA or NVIDIA flags on a Mac. The NVIDIA profile commands intentionally apply only to Linux. Settings use the native macOS application-config directory; no personal Linux config is needed.

## Things to try

- Ingest from Card: navigate to a DCIM subfolder, add multiple cards to one review session, cull before copying, and resume through Recent Sessions.
- Settings → Design: choose Wisp, OpenCull or RapidRAW inspired design, and Spacious or Classic action placement. Wisp separates setting rows and active controls with stronger contrast.
- Settings → Design: choose colour mode, font and interface scale (80–200%).
- Settings → Layout: default/remembered view and panel visibility. Stack view puts groups in the full-width bottom row and active stack frames above. Drag each strip's top edge to resize; F hides both stack strips. Resize edges support arrow keys as well.
- Settings → Culling → Experimental Features: native-aspect strips, including the stack overview; decoded JPEG dimensions allow early sizing before complete metadata.
- Settings → Zoom → Experimental Features: eased click zoom. This affects motion curves, not the speed of CPU image decoding.
- Select one recent item to reveal bulk controls. Deleting a session removes saved review history, never source photos.

Use a copy of a small shoot for an initial trial. `npm run check` runs frontend tests/build and Rust tests. Update with `git pull --ff-only`, `npm ci`, then `npm run desktop`.

## Verification boundaries

The combined snapshot before the CUDA addition passed macOS Apple-silicon CI tests, desktop development startup and build. The final branch's CI additionally starts `desktop`, checks Vite and the native process, and observes them through startup. This is a startup smoke test, not a visual, card-reader or performance test on the recipient's Mac. Intel macOS is not covered by the Apple-silicon runner. See the latest integration Build workflow for the exact commit and result.

The opt-in CUDA dependency is target-scoped to Linux x86_64. CI additionally checks the macOS dependency graph and tests a build with the `cuda` feature requested: the NVIDIA dependency must remain absent and inference stays on CPU. The README on fork `main` clones the default branch; this trial guide intentionally selects `dev`.
