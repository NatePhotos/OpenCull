# NVIDIA AI trial

CUDA is experimental and available only on **Linux x86_64** in this fork. macOS, Windows, ARM and ordinary CPU builds exclude the CUDA runtime dependency. macOS has no CUDA option; use its normal desktop development command.

The combined, cross-platform test branch is `dev`. The same changes are merged into the fork's `main` when approved. Earlier CUDA work is preserved in [archive tags](BRANCH_ARCHIVE.md); new development happens directly on `dev`.

## Requirements

First install the normal Rust, Node and Tauri prerequisites from the README. For the CUDA trial also install:

- An NVIDIA GPU and a driver compatible with CUDA 13.
- CUDA **13.x** runtime libraries, including cuBLAS, cuBLASLt, cuRAND and CUDA runtime. A full CUDA toolkit is also suitable. `nvidia-smi`'s displayed CUDA version describes driver support; it does not prove these libraries are installed.
- NVIDIA **cuDNN 9.x** runtime libraries compatible with CUDA 13. cuDNN is a separate installation from the CUDA toolkit.

Use [NVIDIA's CUDA installation guide](https://docs.nvidia.com/cuda/cuda-installation-guide-linux/) and [cuDNN installation guide](https://docs.nvidia.com/deeplearning/cudnn/installation/latest/linux.html). Make custom installation directories discoverable by the system dynamic linker, or add their library directories to `LD_LIBRARY_PATH` for the launch. Avoid mixing CUDA major versions or incompatible cuDNN libraries.

The pinned Rust binding downloads an ONNX Runtime 1.28 CUDA 13 build on the first CUDA compilation. Set `ORT_CUDA_VERSION=13` if needed to override an incorrectly detected older toolkit. No TensorRT installation or ONNX Python package is needed to run the desktop app; the prebuilt archive may contain additional unused providers.

## Run

On `dev` or fork `main`, after the README's `npm ci` step:

```sh
# Native desktop, automatically selects CUDA on compatible Linux systems:
npm run desktop

# Force CUDA compilation, Linux x86_64 only:
npm run desktop -- --cuda

# Force CPU AI:
npm run desktop -- --cpu

# NVIDIA webview rendering profile, automatic AI selection:
npm run desktop -- --nvidia
```

The desktop launcher checks `nvidia-smi` and the installed 64-bit CUDA/cuDNN
libraries. When available, it compiles CUDA support automatically. Otherwise it
builds the CPU version. `npm run build:desktop` uses the same selection; pass
`--cuda` to include CUDA support when building on a machine without the runtime.
CUDA-enabled executables also probe library loading and device initialization at
launch; absent or unusable devices use CPU. `OPENCULL_AI=cpu` disables runtime
probing, and `OPENCULL_AI=cuda` explicitly requests CUDA. Direct Tauri/Cargo build
commands still require `--features cuda` to include CUDA support.

Enabling CUDA accelerates AI models; JPEG decoding and the webview renderer are separate.

Open a folder and enable OpenCull AI in Settings. The terminal reports `[ai] CUDA model ready` as each model loads. Four model sessions are shared by the whole process through one bounded worker queue, rather than duplicated per CPU analysis thread. The device arena grows by the requested amount, with TF32 disabled for the tested FP32 comparison. Explicit additional GPU input buffers and workspace changes were omitted after the benchmark showed no useful memory benefit.

If provider initialization or inference fails, the app reports `[ai] CUDA unavailable; continuing on CPU: ...`, releases its CUDA sessions, and retries the original input on CPU. Subsequent inference remains on CPU for that launch. A successful window launch alone does not prove CUDA inference is active.

Existing cached AI results may skip inference. For a fresh trial use a new folder or Settings → Storage → clear the face-analysis cache. This only removes cached analysis, not photos or sidecars.

## Native checks

```sh
node --test scripts/cuda-desktop.test.mjs scripts/dev-desktop.test.mjs
cd src-tauri
cargo test --lib --features cuda
OPENCULL_AI=cuda cargo test --lib --features cuda concurrent_callers_share_cuda_models_and_match_cpu -- --ignored --nocapture
OPENCULL_AI=cuda cargo run --features cuda --example ai_backend -- /path/to/photo.jpg
```

The explicit GPU test compares all four models with CPU results at batches of one and eight across four concurrent callers. The read-only example prints three actual photo-analysis timings and any CPU fallback. Neither writes photos or marks. Default tests do not require NVIDIA hardware.

See [CUDA memory experiment](CUDA-BENCHMARK.md) for the measured tradeoff. Benchmark results are not a guarantee of whole-app performance or memory usage on another machine.

## Linux installers

Linux release installers include the matching ONNX CUDA and shared provider
libraries beside the executable, as required by the statically linked runtime.
The host supplies NVIDIA drivers, CUDA 13 and cuDNN 9; installers do not bundle
those system runtimes. The desktop build wrapper applies `tauri.cuda.conf.json`
when CUDA is selected, including debug package paths. Direct Tauri packaging
uses `--features cuda --config src-tauri/tauri.cuda.conf.json` for release builds.
