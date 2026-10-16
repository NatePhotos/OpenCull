# OpenCull — Ingest & sessions

Card review, multi-card sessions, subfolders, resumable imports, saved sessions and video review.

This is a curated contribution snapshot. Read [BUNDLE.md](BUNDLE.md) for its exact scope, prerequisites, source commits and validation. The idea branches are cumulative; they preserve required dependencies without the old integration history. Day-to-day development remains on `dev`; installers are on the [0.5.0 release](https://github.com/wispdevon/OpenCull/releases/tag/v0.5.0).

## Run this bundle

Use Node 22.12+ (the exact tested version is in `.nvmrc`), npm 11+, Rust and the [Tauri platform prerequisites](https://v2.tauri.app/start/prerequisites/). Native JPEG decoding also needs CMake and NASM on Intel/AMD; on macOS install `jpeg-turbo` and `pkgconf` with Homebrew.

```sh
git clone --branch ideas/ingest-sessions --single-branch https://github.com/wispdevon/OpenCull.git
cd OpenCull
npm ci
npm run desktop
```

## Build and check

```sh
npm test
npm run build
npm run build:desktop
```

`npm run dev` starts only the browser demo. `npm run desktop` launches the native app and reuses an existing OpenCull frontend when possible.

## NVIDIA inference

On Linux x86-64, the desktop launcher automatically enables CUDA when it finds a compatible NVIDIA driver, CUDA 13 runtime libraries and cuDNN 9. The installer includes the ONNX provider libraries; the NVIDIA runtimes remain host dependencies. Missing prerequisites use CPU inference. macOS and Windows use CPU AI. Graphics rendering and AI inference are separate.

```sh
npm run desktop -- --cpu
npm run desktop -- --cuda
```

See [CUDA setup](docs/CUDA-TRIAL.md) and the bundle guide for limitations. MobileCLIP controls exist only in AI and Interface bundles.

## Project

Based on [NatePhotos/OpenCull](https://github.com/NatePhotos/OpenCull), with changes from [wispdevon/OpenCull](https://github.com/wispdevon/OpenCull). Processing stays local; model installation and update checks use the network. Original feature branches and archive tags remain available. See [how OpenCull works](docs/HOW-IT-WORKS.md) and [LICENSE](LICENSE) (GPL-3.0-or-later).
