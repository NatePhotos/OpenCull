# CUDA memory experiment

Measured locally on 2026-10-16: Ryzen 5 7500F, RTX 3060 Ti 8 GB, driver 615.78.08, ONNX Runtime 1.28.0. This is a Python model-inference harness, not full-app RAM or end-to-end culling throughput.

Each fresh process keeps one session for each of the four bundled models. A chain runs face detection once, then judge, eyes and mouth for one or eight faces. CPU uses one intra/inter-op thread, CUDA uses FP32 with TF32 disabled. Standard runs already reuse host input buffers. GPU input/output transfers remain included. The detector uses a local JPEG at long edge 2048; the face models use seeded synthetic crops of the production shapes.

Three independent trials per configuration, five warmups after the initial chain and 60 timed chains per trial. Times below are medians of trial medians. RAM is maximum Linux process RSS high-water; VRAM is maximum per-process allocation sampled about every 100 ms, including setup. Brief unsampled VRAM peaks are possible. Background desktop applications remain running.

## Step-by-step results

| CUDA configuration | 1-face time ms | Peak RAM MiB | Peak VRAM MiB | 8-face time ms | Peak RAM MiB | Peak VRAM MiB |
|---|---:|---:|---:|---:|---:|---:|
| Default arena/workspace | 11.95 | 1,223 | 506 | 13.61 | 1,256 | 610 |
| Smaller convolution workspace | 11.88 | 1,223 | 506 | 13.44 | 1,256 | 610 |
| Above + heuristic algorithm | 11.91 | 1,223 | 506 | 13.61 | 1,258 | 610 |
| Above + requested-size arena growth | 11.91 | 1,220 | 464 | 13.42 | 1,256 | 558 |
| Above + explicit reusable GPU input buffers | 12.12 | 1,227 | 556 | 13.77 | 1,259 | 642 |
| CPU control | 54.60 | 326 | 0 | 81.31 | 504 | 0 |

The input-buffer variant uploads the inputs every iteration and returns every output to CPU. It does not win by caching inputs permanently on the GPU. Each variant passed output comparison with CPU (rtol/atol 0.001), and separate untimed profiles confirmed CUDA model-node execution with no CPU model-node execution events.

## Isolate the useful setting

A second, independent three-trial comparison removed the neutral workspace/algorithm changes:

| Workload | Default CUDA time | Requested-size arena time | Default VRAM | Requested-size VRAM |
|---|---:|---:|---:|---:|
| One photo, 1 face | 11.89 ms | 11.95 ms | 506 MiB | 464 MiB |
| One photo, 8 faces | 13.78 ms | 13.42 ms | 610 MiB | 558 MiB |

Requested-size arena growth saved **42–52 MiB of sampled peak VRAM (about 8%)**. System RAM changed by less than 2 MiB in this follow-up, and timing differences were small enough not to claim a speed improvement. Reusable GPU input buffers increased VRAM, so were omitted. Smaller workspaces and heuristic selection gave no meaningful memory saving and were also omitted.

The native feature therefore uses one bounded shared GPU worker, requested-size arena growth and the tested FP32 precision. Its actual memory/throughput still needs workload-specific measurement: the Python harness already had a single worker and does not quantify the benefit versus a multiworker CUDA app. Models themselves total about 11 MiB; RSS includes runtime libraries and allocations, and these measurements do not attribute each page to a particular cause.

The native Rust smoke test additionally compares CPU/CUDA for all four models, one/eight-face batches, and four concurrent callers. CUDA is opt-in; macOS keeps CPU inference and excludes the CUDA dependency.

## Reproduce

In a separate Linux Python environment, install `onnxruntime-gpu[cuda,cudnn]==1.28.0` and Pillow, with compatible NVIDIA drivers. This is only needed for the benchmark, not for running OpenCull.

```sh
python tools/benchmark_cuda.py --image /path/to/photo.jpg --output /tmp/opencull-benchmark
# Also test the workspace/algorithm and explicit input-buffer variants:
python tools/benchmark_cuda.py --image /path/to/photo.jpg --output /tmp/opencull-benchmark-all --all-variants
```

The tool writes local input fixtures, provider profiles and JSON timing/memory samples to the output directory. No photograph or fixture belongs in Git. For the native application use the [CUDA trial guide](CUDA-TRIAL.md).

Provider settings and their limitations are documented in the [official CUDA provider guide](https://onnxruntime.ai/docs/execution-providers/CUDA-ExecutionProvider.html). `gpu_mem_limit` limits a device arena, not total system RAM or total VRAM; this experiment does not set it.
