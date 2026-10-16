# Experimental MobileCLIP grouping

Settings → AI → MobileCLIP grouping → Download model, then enable **Group by
visual similarity**. It is off by default and needs OpenCull AI. Switching it
off immediately regroups cached face results using the existing time/face
geometry rules. The toggle participates in Storage settings import/export.

## Order of operations

1. Read capture times and orientation; create candidate runs using the tighter
   time rules (see TIGHTER_GROUPING.md).
2. Restore cached face analysis, or analyze missing faces. MobileCLIP does not
   invalidate or repeat face inference.
3. Look for MobileCLIP embeddings in a bounded RAM cache, then the disk cache.
   Keys include source path, file version, model SHA and preprocessing version.
4. For misses, reuse existing 1280px thumbnails, then 640px thumbnails with a
   short edge of at least 256px, then cached Screen and Fit 1600/2400/3400
   JPEGs. Lifted camera Screen JPEGs have their EXIF orientation applied once;
   generated previews already have upright pixels. Do not generate a
   JPEG just to decode it again. Otherwise use the existing embedded-preview
   selection and scaled decoder (libjpeg-turbo when built with turbo).
5. At most four decode workers prepare four inputs at a time. Centre crop,
   bilinear resize to 256×256, RGB floats in [0,1], no mean/std normalization.
   One shared CUDA worker processes prepared inputs when `npm run desktop`
   detects a compatible CUDA runtime; otherwise one CPU session uses two threads.
   CPU model loading is lazy, so cache hits and CUDA inference do not initialize
   an unnecessary CPU session.
6. Store normalized 512-value embeddings. Select **Loose (>0.70)**, **Normal (>0.80, default)**, or
   **Tight (>0.90)** in Settings. At similarity strictly above that cutoff, compare
   against both the preceding frame and group anchor to avoid chaining different
   compositions. Time span remains at most three seconds; orientation changes
   split groups. A visual mismatch can split even a single shutter burst.
7. Rank frames inside their resulting groups using existing face/motion scores.
   MobileCLIP changes group membership, not photo ratings or source files.

Closed sessions and changed preferences cancel stale work. Background grouping
stands aside for interactive previews according to the existing AI pause option.
Status and errors appear in the toolbar and AI settings. Missing models do not
silently substitute a different visual algorithm. Clearing AI cache removes
embeddings and face results but retains the downloaded model.

## Model and privacy

The download is **45,543,630 bytes**, FP32 vision-only MobileCLIP-S0 from
[Xenova's ONNX conversion](https://huggingface.co/Xenova/mobileclip_s0), pinned
at revision `757d59c9c6870a76a4b0306f05f5061bca15c39f`. SHA-256:
`17d3c037b1d488c10c50e09f6009ea5a198caef4e0e8f4ea5617b7cb2d067ac0`.
Downloads require an explicit button press and are checked before installation
and model loading. Photographs and embeddings stay local. The model is stored
under the app cache's `mobileclip-s0` directory; weights are not committed.
See [Apple's MobileCLIP project](https://github.com/apple-aiml-research/ml-mobileclip)
for model terms and architecture. On Linux x86_64 CUDA builds, MobileCLIP shares the existing bounded worker
with face analysis. Small shape/control operations remain on CPU as assigned
by ONNX Runtime; model compute uses CUDA. Face models retain their strict
CUDA-only placement checks. Failures report a CPU fallback, and AI settings
show the available inference backend. macOS and ordinary CPU builds do not
load CUDA. GPU transfers remain included; cached embeddings bypass both
backends.

## Measured decisions

On a Ryzen 5 7500F, eight local JPEGs, five timed passes each, the standalone
Python/libjpeg-turbo harness measured median decode+preprocessing of 139.8ms at
full resolution, 51.7ms at 1/8 scale, 1.32ms from an existing 512px JPEG preview,
and 0.16ms loading a disk embedding. Eight-photo scaled decoding took 424ms with
one worker, 216ms with two, and 114ms with four. Four bounded decode workers use the fastest measured batch configuration;
background work still yields to interactive preview requests. These are warm-filesystem standalone results, not native
app latency promises. Preview creation is excluded from reuse measurements.

FP32 inference measured 21.2ms with two threads versus 46.0ms for the quantized
model. Quantization also substantially changed similarity scores, so FP32 was
chosen. Smaller recompressed previews can shift scores near the threshold;
prefer 1280px existing previews and validate on more shoots before relying on
this experimental grouping. Embeddings retain the representation used on first
analysis until the source/version changes or AI cache is cleared.

Native Linux validation: the explicit model download and checksum verification
succeeded. The reference pair P1962550/P1962551, supplied as the same synthetic
200ms burst, grouped together with visual grouping off and split with it on.
A repeat request used two cached embeddings and zero inferences; switching off
restored the baseline. Stale revisions were rejected and AI cache clearing
retained the model. Native batch logs measured 272ms for two new embeddings and
38µs for the two RAM-cache hits (excluding command transport and model startup).

CUDA follow-up on the same Ryzen 5 7500F / RTX 3060 Ti, eight photos × five
passes, five warmups, fresh processes and transfers included: FP32 CPU (two
threads) median 24.69ms, p95 26.74ms; CUDA median 6.44ms, p95 8.01ms (3.8× faster
inference). CUDA process peak RSS was 1106MiB versus CPU 157MiB, with 218MiB
VRAM reported after warmup. These include Python/runtime overhead, not whole-app
RAM, and exclude image decoding. Model setup and the first CUDA inference cost
334ms + 519ms in this run; cached embeddings do not incur inference. Smaller
workspace/heuristic variants measured 5.73–5.89ms, but order/clock variation
prevents claiming those small differences as a win; the existing shared-worker
provider settings are retained.

An untimed provider profile confirmed CUDA Conv/MatMul execution; the CPU
operations were integer shape/control work. The first combined GPU test failed
while other applications filled almost all VRAM; after stopping the requested
Spotiverse process, roughly 5.3GiB was available for retesting. CUDA failures
remain visible and fall back safely rather than claiming GPU execution.

The initial thumbnail reuse guard excluded 640×359 images despite their short
edge exceeding the model's 256px input. It now accepts short edges ≥256px,
with a regression test for that exact size. Changing this preprocessing policy
versions the embeddings, so older representations are not silently reused.

## Visible GPU failures

A CUDA allocation failure displays **Not enough GPU memory (VRAM)** in the
workspace and AI settings, explains CPU fallback, and offers technical details.
It can be dismissed without blocking photo navigation. Device-busy failures
have a distinct **GPU busy or unavailable** message: they do not prove VRAM
exhaustion. Other provider failures show **GPU acceleration failed**. Supported
CUDA builds report current backend/failure state; CPU-only and unsupported
platforms show no CUDA error. Status updates while the page is open, so a
failure after startup cannot leave the settings claiming CUDA is active.


A live audit of the 1993-photo session found cached Screen and Fit previews for
1992 photos (mean 177.5/206.5KiB), while only one photo had an eligible cached
640/1280px thumbnail. The initial implementation checked only thumbnail tiers,
forcing large source reads from the SATA HDD despite the available NVMe cache.
Screen and Fit tiers are now included, with tests that reuse each tier while
the original source is absent and verify EXIF rotation. This preprocessing
policy uses a new cache version to avoid mixing old and new representations.

Native follow-up with the same 1861 comparison photos and zero embedding-cache
hits in both passes: the sum of eight grouping-batch times fell from 170.748s
to 10.614s (16.1×). The 235-photo batch fell from 22.426s to 1.949s. A four-second
sample saved 700 embeddings (175/s), reading 110MiB through file reads and
107.45MiB physically, versus the earlier 41 embeddings and 382.3MiB physical
reads. This is the measured benefit of the existing small NVMe preview JPEGs
versus large SATA source reads; it excludes opening/metadata time and is not an
isolated decoder benchmark. CUDA remained enabled with no fallback, and all
1993 face results were restored from cache. In the runtime log, `cached` refers
to embeddings, not preview JPEG hits: zero cached embeddings is compatible with
reusing cached JPEGs to compute new embeddings.


## Grouping strength

The three radio choices sit beside the experimental MobileCLIP controls in
Settings → AI. The saved `mobileClipGrouping` preference is included in
settings export/import and validated as `loose`, `normal` or `tight`. Older
settings without the preference use Normal. Changing strength cancels stale
grouping requests and recomputes group membership using existing embeddings;
it does not decode photographs, rerun inference, or invalidate embeddings.
Previous-frame and group-anchor comparisons both use the selected threshold.
Time and orientation safeguards remain. Backend enum validation rejects
unsupported presets; calls that omit the preset use Normal.
