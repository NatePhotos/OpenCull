# CUDA inference on Linux

CUDA support is experimental and limited to **Linux x86_64**. It accelerates
ONNX face analysis. It does not accelerate JPEG decoding or webview rendering;
those paths are configured separately. macOS, Windows, ARM and CPU-only builds
do not include the CUDA provider.

## Requirements

Install the normal Rust, Node and Tauri prerequisites from the README. CUDA
inference additionally needs:

- An NVIDIA GPU with a compatible driver.
- CUDA 13.x runtime libraries, including cuBLAS, cuBLASLt, cuRAND and the CUDA
  runtime.
- NVIDIA cuDNN 9.x libraries compatible with CUDA 13.

The full CUDA toolkit can provide these runtime libraries. `nvidia-smi` reports
driver support; it does not prove that the runtime libraries or cuDNN are
installed. Follow [NVIDIA's CUDA installation guide](https://docs.nvidia.com/cuda/cuda-installation-guide-linux/)
and the [cuDNN installation guide](https://docs.nvidia.com/deeplearning/cudnn/installation/latest/linux.html).
For custom installation paths, make the libraries visible to the system linker
or set `LD_LIBRARY_PATH` when starting OpenCull. Do not mix incompatible CUDA
or cuDNN versions.

The `ort` crate downloads the matching ONNX Runtime CUDA provider during a
CUDA-enabled build. You do not need TensorRT or the ONNX Python package to run
the desktop application.

## Run and build

After installing the repository's npm packages, use the desktop launcher:

```sh
# Detect a compatible CUDA installation automatically; otherwise use CPU.
npm run desktop

# Explicitly request CUDA, or force CPU inference.
npm run desktop -- --cuda
npm run desktop -- --cpu

# Build an installer with the same automatic selection.
npm run build:desktop
npm run build:desktop -- --cuda
```

Automatic selection checks for `nvidia-smi` and the required runtime libraries.
The CUDA-enabled native app probes the driver and initializes a device before
using it. If initialization or inference fails, OpenCull releases its CUDA
sessions and retries the current analysis on CPU. The launcher reports which
path it selected. A successful window launch alone does not prove CUDA
inference is active.

`--nvidia`, `--compatibility` and `--software` select Linux webview rendering
profiles; these are independent of CUDA inference. See
[NVIDIA rendering](features/NVIDIA_GRAPHICS.md).

Direct Cargo builds must opt in with `--features cuda`. To force a runtime
choice in an already CUDA-enabled build, set `OPENCULL_AI=cpu` or
`OPENCULL_AI=cuda`.

## Linux installers

Linux installers built by CI include the matching ONNX CUDA and shared
provider libraries next to the executable, as required by the statically
linked runtime. The host still supplies the NVIDIA driver, CUDA 13 and cuDNN 9;
the installers do not bundle those system libraries. The packaging wrapper
adds the provider files to Debian, RPM and AppImage packages and excludes the
host NVIDIA libraries from AppImage bundling.

## Native checks

```sh
node --test scripts/cuda-desktop.test.mjs scripts/dev-desktop.test.mjs
cargo test --manifest-path src-tauri/Cargo.toml --lib --features cuda
OPENCULL_AI=cuda cargo test --manifest-path src-tauri/Cargo.toml --lib \
  --features cuda concurrent_callers_share_cuda_models_and_match_cpu -- --ignored --test-threads=1 --nocapture
OPENCULL_AI=cuda cargo test --manifest-path src-tauri/Cargo.toml --lib \
  --features cuda registered_file_model_runs_on_cuda_and_matches_cpu -- --ignored --test-threads=1 --nocapture
OPENCULL_AI=cuda cargo run --manifest-path src-tauri/Cargo.toml --features cuda \
  --example ai_backend -- /path/to/photo.jpg
```

The explicit hardware test compares all four built-in models with CPU results
for batches of one and eight across four concurrent callers. The read-only
example prints three photo-analysis timings and any CPU fallback. Default tests
do not require NVIDIA hardware.

See the [CUDA memory experiment](CUDA-BENCHMARK.md) for measurements and their
limits.
