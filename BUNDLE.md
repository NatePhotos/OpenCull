# GPU & performance bundle

Base: upstream OpenCull 0.4.1 (`a8c7f32bb1bd1af89022147407ac16b2142110fb`). No other idea bundle is required.

Includes Linux NVIDIA renderer selection, the stable fallback compositor, automatic CUDA detection, a shared ONNX CUDA worker with CPU fallback, renderer diagnostics, desktop launch commands, and packaging both ONNX provider libraries beside the Linux executable. macOS and Windows retain their native rendering and CPU inference paths; CUDA is Linux x86-64 only.

Excludes MobileCLIP, motion blur, ingest/session changes, and Wisp. CUDA file-model support is infrastructure for consumers such as MobileCLIP; it does not download or run that model here.

## Provenance

Curated from `feature/cuda-inference` (`ae472cd`) and `feature/nvidia-gpu-acceleration` (`c53903a`), plus the final versions of GPU/runtime/build files at `a1aa61c`. Relevant later changes: fallback compositor `c845b36`, automatic NVIDIA selection `dbaaf37`, desktop commands `2ea2bd9`, Vite reuse `aad55c5`, automatic CUDA `876ade0`, provider packaging `b1ae8ee`, and cross-platform path fix `a1aa61c`. Unrelated ancestry is not part of this bundle.

## Try or merge

```sh
git clone --branch ideas/gpu-performance --single-branch https://github.com/wispdevon/OpenCull.git
cd OpenCull
npm ci
npm run desktop
```

In an upstream checkout, fetch the branch and merge it. Later idea bundles document their additional prerequisites. Use `npm run desktop -- --cpu` to force CPU inference; see `docs/CUDA-TRIAL.md` for CUDA/cuDNN requirements. NVIDIA drivers, CUDA 13 and cuDNN 9 remain host dependencies.

## Validation

Linux: TypeScript/production frontend build; 31 frontend tests; 11 launcher tests; 275 native library tests passed, one hardware benchmark ignored. CUDA-enabled native tests compiled with `--features turbo,cuda`. This curated bundle has not been run on macOS or Windows; the full 0.5.0 release has a separate cross-platform CI run.

## Follow-up cleanup

Published history is preserved. An append-only cleanup stack fixes branch-specific clone/build instructions and removes unused MobileCLIP download dependencies from GPU and Ingest. AI restores those dependencies because model installation needs them. The new branch tips retain the cumulative dependency order.

Native Linux desktop build also passed: `npm run tauri build -- --debug --no-bundle --features cuda`, using a separate temporary target so the existing development executable was unchanged.
