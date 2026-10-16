# AI & grouping bundle

Prerequisite: `ideas/ingest-sessions`, which includes `ideas/gpu-performance`. This branch adds one curated AI commit to those two bundles. It carries three new commits beyond upstream 0.4.1, not MobileCLIP's 197 inherited commits.

Includes MobileCLIP-S0 whole-image grouping, Loose >0.70 / Normal >0.80 / Tight >0.90 similarity, three-second group limits, motion-blur detection and filtering, optional one-star + Reject for currently unrated blur detections, cached AI results and embeddings, cached preview JPEG reuse, scaled decoding, shared CUDA inference, CPU fallback, and VRAM errors. Model download and the experimental toggle are in Settings → AI. Settings transfer preserves the AI options.

## Why these prerequisites

The existing session store owns grouping, ratings, filters, cache restoration and stale-analysis cancellation. The native scan protocol handles card sessions and videos as well as ordinary photos. Splitting those protocols apart would create a new implementation rather than regroup the working changes. The GPU bundle supplies the shared ONNX worker and provider packaging. The ingest bundle supplies their existing session/settings foundation. Full Wisp and the later workspace redesign are not required.

## Provenance

AI/backend and store integration from `a1aa61c`. Feature sources include tighter time grouping `136ef9c`, motion blur `b652d03` and `f09fd60`, session AI cache `51c4e5d`, initial MobileCLIP `343036b`, shared CUDA/VRAM handling `6b50dcd`, cached preview reuse `2d0d023`, strength controls `818d56a`, thresholds `cbb4bae`. Thumbnail labels and settings import validation are kept with their AI fields. Original history remains on restored feature branches and archive tags.

## Try or merge

```sh
git clone --branch ideas/ai-grouping --single-branch https://github.com/wispdevon/OpenCull.git
cd OpenCull
npm ci
npm run desktop
```

Enable OpenCull AI, download the MobileCLIP model once, then enable visual grouping in Settings → AI. CUDA is automatic on supported Linux NVIDIA installations; `npm run desktop -- --cpu` overrides it. macOS and Windows use CPU inference. Fetch and merge the idea branch to bring its explicitly listed prerequisites together.

## Validation

Linux production frontend build; 87 frontend tests and 11 launcher tests passed; 312 native tests passed with CUDA enabled, two hardware benchmarks ignored. Tests cover grouping boundaries, motion rating/Reject preservation, caches, settings transfer and CPU/CUDA error handling. Cross-platform CI and native runtime measurements for the full release are separate from these curated-subset checks.

Hardware checks also passed on the local NVIDIA GPU: concurrent CUDA callers matched CPU outputs; real MobileCLIP embeddings were computed on CUDA in 692 ms for two reference RAW files, and a reopened embedding cache was read in 49 µs without recomputation. These are correctness smoke-test timings, not a throughput benchmark.
