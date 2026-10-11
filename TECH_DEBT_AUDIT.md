# Tech Debt Audit: OpenCull
Generated: 2026-10-10 · Commit audited: `5fcff50` (Release 0.2.2)

## Scope and method

- **What was audited.** About 20.5k lines of first-party code: Rust 10.9k in `src-tauri/src`, plus 0.8k in examples; TypeScript/React 8.8k in `src/`.
- **How.** Everything was read in full, as three parallel reads (Rust core, Rust raw/AI, frontend). The most severe findings were then re-checked by hand against the source.
- **Tooling.**
  - `npm audit`: 7 high-severity advisories, all coming through the `shadcn` CLI. See F061.
  - `tsc --noEmit`: clean.
  - `cargo clippy`, `cargo test`, `cargo audit`: **not run**. Compiling and executing the third-party crate was not permitted in the review environment, and `cargo-audit` is not installed. Run them before acting on anything in this audit that depends on build behaviour.
- **Churn analysis.** Not meaningful. The repo has 14 commits, all on 2026-10-10, and the first one (`50c56e8`) adds the whole codebase. There is no history to find hotspots in.

## Executive summary

**Counts:** 4 Critical, 18 High, 34 Medium, 18 Low (74 findings, including F074, added from the maintainer's answers).

1. **Moving photos can lose them.** A move across drives copies the file without forcing it to disk, then deletes the original (F001). Ingest reports "verified" before the data is on disk (F002). This app moves irreplaceable files; this matters most.
2. **One unreadable sidecar wipes a Lightroom sidecar.** Any read error, not just "file missing", turns into "write a fresh sidecar over it", destroying develop settings (F003).
3. **The crash guard does nothing.** `panic = "abort"` in the release profile turns every decoder panic into an app crash, which defeats the `catch_unwind` in `pool.rs` (F004). Several memory-exhaustion paths on malformed or huge files make this reachable (F008, F009).
4. **Sidecar and companion-file handling is inconsistent, and loses data at the edges.** It covers transfer overwriting a foreign sidecar (F005), ingest editing sidecars under Skip/UniqueName (F006), RAW+JPEG pairs split on ingest (F012), and Ship leaving JPEGs behind on a move (F007).
5. **Failures are silent where they matter.** Failed sidecar writes go only to stderr (F010). Ship can be closed mid-run, which hides partial failure (F011). Trash failures get no message (F020).
6. **The same low-level operation is written many times.** Atomic write has 6 copies, sidecar naming has 4, and name-collision rules have 2 (F040–F042). This is the root cause of most of #4.
7. **The riskiest code has no tests.** `commands.rs` has none, the RAW parsers that read untrusted bytes have no fuzzing and no real-camera fixtures, and the frontend has zero tests (F050–F053).
8. **IPC and the `opencull://` protocol trust any path.** That is acceptable while the webview is clean, but there is no second layer of defence (F030, F031).
9. **The code is unusually well commented and careful.** The debt is concentrated in the data-safety layer, not spread everywhere. Most fixes are size S.

## Architectural mental model

OpenCull is a Tauri 2 desktop app.

**Rust side.** `lib.rs` builds one `Arc<App>` (in `commands.rs`) holding:
- the preview service `Previews`: a disk `Cache` plus a priority work `Pool`;
- an XMP `Writer` thread, which writes sidecars and optionally embeds marks into JPEGs;
- a capture-time cache;
- the AI face store.

**How pictures reach the UI.** Pictures never cross IPC. The webview loads `opencull://?p=<path>&v=&s=` URLs. `serve.rs` answers cache hits on two reader threads and sends misses to the pool. There, `preview::render` uses `raw/` to find the camera's embedded JPEG without decoding the RAW (TIFF IFD walk, BMFF box walk, Fuji header, brute-force scan as a fallback), then decodes, resizes and re-encodes it.

**IPC commands.**
- Folders: open (`scan.rs` pairs RAW+JPEG by stem; `times.rs` and `burst.rs` group by capture time).
- Streaming: metadata and AI over Channels.
- Marks: written through the Writer.
- Moving files: trash, transfer and Ship (`transfer.rs`), and card ingest (`ingest.rs`).
- Opening editors: `editors.rs`.

**AI.** `ai::analyze` decodes the largest preview at full resolution, then:
1. runs YuNet (`finder`), then the project's own judge model over face crops;
2. measures focus on eye patches;
3. classifies subject vs background;
4. computes a worst-subject score.

`series.rs` re-scores frames within a burst or pose by following faces across frames.

**Frontend.** One large zustand store (`src/lib/store.ts`) holds `photos[]` (never reordered) plus the index arrays `order` and `view`, parallel sparse AI arrays, prefs, and undo/redo. All IPC goes through the `Backend` interface in `src/lib/api.ts`, which has a dev-only mock. One global keyboard handler maps actions onto store calls.

**IPC contract.** The frontend agent checked all 40 registered commands against `api.ts`. There is **no drift**: names, camelCase args and serde renames all line up.

**Does this match the README?** Yes, with two exceptions:
- `HOW-IT-WORKS.md` describes only 4 of the 12 examples and omits half of the modules (F070).
- The README's claim that marks are written "where other programs look for them" is true, but the app also rewrites the user's original JPEGs **by default** (`embedInJpeg` defaults to `true`, `src/lib/store.ts:570`). That deserves to be stated plainly; see Open questions.

## Findings

Severity is calibrated to this app's stakes. Anything that can lose or corrupt a user's photo or edits is High or above.

| ID | Category | File:Line | Sev | Effort | Description | Recommendation |
|----|----------|-----------|-----|--------|-------------|----------------|
| F001 | Data safety | src-tauri/src/transfer.rs:172-180 | Critical | S | Cross-volume move: `std::fs::copy` to staging, size check, rename, then `remove_file(source)`. Nothing is ever synced to disk. If the drive is pulled or power is lost right after, both the source and the copy can be gone. This contradicts the module doc (transfer.rs:4-8). | `File::open(&staged)?.sync_all()` before the rename. Sync the destination directory before `remove_file(source)`. |
| F002 | Data safety | src-tauri/src/ingest.rs:398 | Critical | S | `verified_copy` calls `to.flush()`, which does nothing for a `File`, and never syncs. "Success" is shown before the data is on disk, and the module doc says the user may format the card next. | `to.sync_all()?` before `drop(to)`. |
| F003 | Data safety | src-tauri/src/xmp.rs:262 | Critical | S | `read_to_string(&path).unwrap_or_default()` treats *any* error (permissions, a flaky network share, UTF-16 XMP) as "no sidecar". It then writes `fresh(marks)` over the existing sidecar, destroying Lightroom/C1 develop settings. | Use the empty string only on `ErrorKind::NotFound`; return every other error. |
| F004 | Error handling | src-tauri/Cargo.toml:82 + src-tauri/src/pool.rs:244-248 | Critical | S | `panic = "abort"` means `catch_unwind` never catches anything in release. Any panic in jpeg-decoder, png, fast_image_resize or ort kills the app. The pool test at pool.rs:406 passes only because tests unwind. | Remove `panic = "abort"` (the size cost is small). Fix the pool.rs comment either way. |
| F005 | Data safety | src-tauri/src/transfer.rs:102-108, 123-127 | High | M | Sidecars are placed with `replace=true`, and `free_suffix` checks only the photo names. Shipping `A.ARW`+`A.xmp` into a folder that already holds `A.NEF`+`A.xmp` overwrites the NEF's sidecar. The test `ratings_replace_stale_ones_left_at_the_destination` (transfer.rs:347) locks this in. | Include sidecar target names in the `free_suffix` check. Replace only a sidecar whose same-stem photo this job just placed. |
| F006 | Data safety | src-tauri/src/ingest.rs:476-501 | High | S | After copying, `xmp::write(&target, marks)` edits whatever sidecar sits next to the target. Under Skip it edits a pre-existing destination sidecar. Under UniqueName the copied sidecar goes to `A_1.xmp` while the ratings land in a stranger's `A.xmp`. | Derive the sidecar name from the photo's final stem. Don't pass sidecars through `names.unique`. Skip the write when the sidecar was skipped. |
| F007 | Contract | src-tauri/src/commands.rs:596 | High | S | `ship` builds jobs with `companions: Vec::new()`, so a Ship-move of a RAW+JPEG pair leaves the JPEG behind. That contradicts scan.rs:54-56 ("moved together"); `transfer_photos` (commands.rs:560) does include companions. | Use `transfer_job(…, true)` in `ship`, or document "Ship sends RAWs only". |
| F008 | Robustness | src-tauri/src/raw/mod.rs:610 | High | S | `scan_for_jpegs` reads *the entire rest of the file* into a Vec for each candidate SOI that passes the frame check. Failed candidates re-read it. A multi-GB DNG pano with no declared preview means allocations of file size per candidate, then an OOM abort. | Search for EOI in 1MB chunks through `read_at`. Cap preview length (e.g. 64MB) and the candidate count. |
| F009 | Robustness | src-tauri/src/raw/jpeg.rs:29-35 | High | S | `is_picture` accepts up to 65535×65535. A header declaring that (12.9GB RGB) passes the gate to the decoder. Not built-and-run to confirm, but combined with F004 it is an OOM-abort path. | Cap the pixel count in `is_picture` (e.g. 200MP). Set `jpeg_decoder::Decoder::set_max_decoding_buffer_size`. |
| F010 | Error handling | src-tauri/src/xmp.rs:345-347 | High | S | A failed sidecar write (read-only folder, card full, share gone) goes only to `eprintln!`. The UI shows a rating that was never saved. `on_failure` fires only for JPEG embed failures. | Route sidecar failures through the same callback, and emit an event to the UI. |
| F011 | Error handling | src/components/cull/ShipDialog.tsx:118, 133 | High | S | Escape, an outside click or the X close Ship mid-run. `close` and `onOpenChange` never check `shipping`. On unmount, `setError` from a partial failure is lost, which breaks the stated invariant at :140 ("must never close looking like it worked"). | Ignore `onOpenChange(false)` and hide the X while `shipping`, or raise errors into the store's `notice`. |
| F012 | Data safety | src-tauri/src/ingest.rs:458-468 | High | M | The JPEG is named separately from its RAW. Under Skip, a taken JPEG name silently drops the JPEG while the photo still counts as Copied. Under UniqueName, `A.ARW` gets paired with a stranger's `A.JPG` and the real one becomes `A_1.JPG`. transfer.rs:71-79 already solved this. | Reserve one suffix for the RAW, its JPEG and its sidecar together (see F042). |
| F013 | Data safety | src-tauri/src/transfer.rs:178-180 | High | S | Copy and rename succeed, but deleting the source fails (locked card, read-only share). This is reported as a total failure; companions and sidecars aren't moved, and an untracked duplicate exists. | Return a distinct "copied, source kept" outcome. |
| F014 | Data integrity | src-tauri/src/xmp.rs:49-64, 231, 246-249 | High | S | `update` always rewrites rating, label and flag. Non-colour labels (Bridge "Approved", "To Do") parse to `Color::None` and are deleted on the next rating change; Bridge's `Rating="-1"` reject is rewritten. | Have `update` touch only the fields that changed, or carry the original label string through. |
| F015 | Data integrity | src-tauri/src/commands.rs:508; src-tauri/src/xmp.rs:86-96 | High | M | Sidecars are keyed by stem, so unpaired `A.ARW`+`A.DNG` (or `A.JPG`+`A.png`) in one folder share `A.xmp`. Rating one overwrites the other, and trashing one trashes the other's edits. | When scan finds a stem collision among unpaired files, use the `NAME.ext.xmp` form, or warn. |
| F016 | Correctness | src/lib/store.ts:620-655 | High | S | `openFolder`'s stale-reply check compares only the folder path. Opening the same path twice quickly (a double-click on a recent) means the second call's `closeFolder()` sees placeholder `id:-1`, so the first backend session leaks and its metadata/AI threads keep running. | Use a monotonic open-token instead of path equality. |
| F017 | Performance | src-tauri/src/commands.rs:964-966, 1011-1013 | High | S | `paths_exist`/`folders_exist` are sync commands, so they run on the main thread. A `stat` on a disconnected SMB share can freeze the UI for tens of seconds. | Make them `async` with `spawn_blocking`. |
| F018 | Durability | src-tauri/src/xmp.rs:270-276 | High | S | Sidecars are written temp-then-rename (good), but with no `sync_all`. A crash on some filesystems leaves a zero-length sidecar. | Write through a `File` and call `sync_all` before the rename (or use F040's helper). |
| F019 | Correctness | src-tauri/src/xmp.rs:143-145, 217-251 | High | S | `parse` reads `photoshop:Urgency="1"` as Pick. Un-picking removes `PickStatus` but leaves Urgency, so the Pick comes back on the next read. | Strip Urgency when the flag isn't Pick. |
| F020 | Error handling | src/lib/store.ts:931-939; src-tauri/src/commands.rs:513-515 | Medium | S | `confirmTrash` doesn't catch a rejection, and photos the backend reports as failed stay put with no notice. Rust also discards the reason with `.is_err()`. | Wrap in try/catch and send failures to `notice`; return `(path, reason)` pairs from Rust. |
| F021 | Data integrity | src-tauri/src/embed.rs:156-168 | Medium | M | Same-length in-place XMP overwrite inside the user's original JPEG, on by default. A crash mid-write leaves a half-written packet. Pixel data is safe. | Use the already-atomic `afresh` path when the packet is small, or document the risk. |
| F022 | Data integrity | src-tauri/src/embed.rs:211-217 | Medium | M | `afresh` (new file + rename) keeps permissions only. Finder tags, xattrs, birth time, ACLs and hard links are lost. | Copy xattrs and birth time (`fcopyfile`/`clonefile` on macOS), or document the loss. |
| F023 | Contract | src-tauri/src/embed.rs:144-149 | Medium | S | After an in-place edit the mtime is restored, so size and mtime are unchanged. rsync's quick check, backup tools and Lightroom's "changed on disk" detection all miss the new rating. | Decide deliberately; at minimum make it a setting. |
| F024 | Correctness | src-tauri/src/ingest.rs:476 | Medium | S | Ingest copies only one sidecar form (`NAME.xmp` *or* `NAME.ext.xmp`), while transfer copies both (transfer.rs:112). | Share one `xmp::sidecars_of` (F041). |
| F025 | Error handling | src-tauri/src/ingest.rs:483, 487, 500 | Medium | S | Sidecar copy and write errors during ingest are `.ok()`'d away, so ingest reports success while losing ratings. | Collect them into `sidecar_failed`, as transfer does. |
| F026 | Correctness | src-tauri/src/ai/mod.rs:216-220 + src-tauri/src/commands.rs:690 | Medium | S | Every `analyze` error is cached as `Record::default()` under the file's version. A transient I/O error is remembered as "no faces" until the file or VERSION changes. `preview::decode_upright` (preview.rs:185) has no PNG branch, so every PNG is cached as empty. | Cache content errors only; give `decode_upright` the PNG path. |
| F027 | Contract | src-tauri/src/ai/store.rs:18 | Medium | S | The hand-written `VERSION` covers only some of the thresholds and none of the model bytes. Swapping `judge.onnx` without bumping it serves stale verdicts. | Append `xxh3_64(finder::MODEL ++ judge::MODEL)` at startup. Group the rule constants. |
| F028 | Resource | src-tauri/src/ai/judge.rs:49-66 | Medium | S | All faces go through the judge in one uncapped batch. A crowd shot with hundreds of hits means hundreds of MB per worker, across 6 workers. | Chunk into batches of about 32. |
| F029 | Resource | src-tauri/src/ai/mod.rs:218 | Medium | S | AI decodes the largest preview at `u32::MAX`: about 180MB RGB for 60MP, with up to 6 workers, so a 1–1.5GB peak. | Size the worker count by available memory as well as cores. |
| F030 | Security | src-tauri/src/serve.rs:181-207, 215; src-tauri/src/preview.rs:433-436 | Medium | M | `opencull://?p=` accepts any absolute path; `full` on a `.png` returns raw bytes, and the response sends `Access-Control-Allow-Origin: *`. Any script in the webview can read images anywhere the user can. | Check `p` against open sessions, ingest scans and recent covers. Drop the `*` CORS header. |
| F031 | Security | src-tauri/src/commands.rs:469, 493, 555, 582, 886 | Medium | M | `move_to_trash`, `transfer_photos`, `ship`, `set_marks` (which rewrites any JPEG) and `ingest_copy` take arbitrary paths. Capabilities restrict plugins, not app commands. | Validate sources against the current session's `images`/`display`. |
| F032 | Performance | src-tauri/src/pool.rs:135, 189, 199, 211-222 | Medium | M | The queue is a `Vec`, scanned in full under the lock on every submit, pick and `busy()`. About 40k jobs for a 20k-photo folder makes draining O(n²). AI workers poll `busy()` every 60ms. | Use a key `HashMap` plus a `BinaryHeap` with lazy delete, and a live-count counter. |
| F033 | Performance | src-tauri/src/serve.rs:87-99, 117-142 | Medium | S | Two reader threads do all the disk work, including `lift_screen` reads from slow cards. One slow file stalls every cache hit queued behind it. | Send `lift_screen` through the pool, or add readers. |
| F034 | Performance | src-tauri/src/raw/mod.rs:473-482, 594 | Medium | S | The 24MB fallback scan reruns on every `probe_source` call: per preview tier, twice in `open_folder`, and again in AI. | Memoize `Probe` per (path, version), or at least cache "needs scan". |
| F035 | Performance | src-tauri/src/commands.rs:716-722; src-tauri/src/ai/store.rs:114 | Medium | S | The whole folder's AI map is reserialized every 150 results while holding `known`, so all workers stall. O(n²) bytes over a large folder. | Clone under the lock, serialize outside it. |
| F036 | Resource | src-tauri/src/ai/store.rs:103-121 | Medium | S | Cache file names hash `VERSION`, so each bump orphans every old file forever. Entries for deleted photos and folders are never pruned. | On startup, delete files from other versions; prune to the current scan on save. |
| F037 | Performance | src/components/cull/Dialogs.tsx:194 | Medium | S | `const s = useStore()` subscribes Settings to *every* store change (AI batches, a 1s warm poll). | Use `useShallow` over the toggle keys. |
| F038 | Performance | src/lib/store.ts:372, 391; src/components/cull/Filmstrip.tsx:75-99; src/components/cull/SceneView.tsx:35-44 | Medium | M | One mark triggers several full-folder passes: `photos` copy, refilter, Filmstrip rows, FilterBar recount, SceneView scenes. Noticeable at 10k+ with speed cull and a held key. | Derive a stable `groups` array, and key the memos on it instead of `photos`. |
| F039 | Consistency | src/lib/shortcuts.ts:238 | Medium | S | The global key handler hard-codes the list of modals that suppress shortcuts. Any new dialog not added there leaks P/X/digits to the photo behind it. | Check `document.querySelector('[role=dialog],[role=alertdialog]')`, as :250 already does for menus. |
| F040 | Architecture | transfer.rs:172, ingest.rs:381, embed.rs:177, xmp.rs:270, cache.rs:116, times.rs:61 (all src-tauri/src) | Medium | M | Atomic write/copy is implemented 6 times, with 3 temp-naming schemes; only embed syncs to disk. This is the root cause of F001, F002 and F018. | Write one `fsx::atomic_write`/`staged_copy(sync: bool)` helper and migrate all six. |
| F041 | Architecture | src-tauri/src/xmp.rs:86, 155; src-tauri/src/transfer.rs:112; src-tauri/src/commands.rs:506 | Medium | S | Sidecar naming is worked out in 4 places, and ingest has a 5th rule (F024). | Make `xmp::sidecars_of(path) -> Vec<PathBuf>` the single source. |
| F042 | Architecture | src-tauri/src/transfer.rs:59, 71-79; src-tauri/src/ingest.rs:303-330 | Medium | M | Two collision systems: `Conflict{KeepBoth,Skip}` + `free_suffix` (per group, case-sensitive) vs `Policy{UniqueName,Overwrite,Skip}` + `Names::unique` (per file, lowercased). The root cause of F005, F006 and F012. | Extract one "reserve a name group" function used by both. |
| F043 | Architecture | src/lib/store.ts:367-984 | Medium | M | A ~620-line `create()` closure mixes lifecycle, selection, marks, undo, filter, sort, AI, trash, transfer and recents. The pure helpers exist but are private. | Move the pure functions to `lib/cull-logic.ts`; keep the actions thin. This is a prerequisite for F052. |
| F044 | Architecture | src/components/cull/Loupe.tsx:130-655 | Medium | M | `Canvas` is a ~525-line component with 10 `useState`s and module-level mutable `lastLevel`/`lastSpot` (:61, :68). | Extract a `useZoom` hook. |
| F045 | Consistency | src/lib/store.ts:283-308; src/components/ingest/IngestView.tsx:149-151; src/components/cull/ShipDialog.tsx:151, 183; src/components/cull/Loupe.tsx:61-67; src/App.tsx:16, 33-36 | Medium | M | Prefs are persisted 5 different ways, one of them inside `setState` updaters, which StrictMode runs twice. | Route them through the store's `PERSISTED` list or a `usePersistentState` hook. |
| F046 | Type | src/lib/prefs.ts:8 | Medium | S | `JSON.parse(raw) as T`. A stale `fileKinds` blob is sent to Rust `Option<Kinds>` and fails, so folders won't open. | Add an optional validator or allowed-values list to `load()` for enum keys. |
| F047 | Contract | src-tauri/src/commands.rs:868-873; src-tauri/src/transfer.rs:43-45 | Medium | S | Ingest status is a `&'static str`. Failure lists are `"name: error"` strings, so the UI can't map them back to photos. | Use a serde-tagged enum and `{path, reason}` structs. |
| F048 | Consistency | src-tauri/src/ai/focus.rs:83-90 vs src-tauri/src/ai/score.rs:77 | Low | S | One focus ratio on two scales. A face shown as "100 sharp" gets only about half the focus credit in the photo score. | Derive both from one constant pair, or document why they differ. |
| F049 | Correctness | src-tauri/src/raw/mod.rs:280-284, 541-561 | Medium | M | Capture-time precedence is coded 3 times. For CR3 the quick and full paths can disagree, despite the doc saying they "can never disagree"; the equivalence test (mod.rs:800-826) has no CR3 case. | Add a CR3 test case and one shared `pick_date()`. |
| F050 | Test | src-tauri/src/commands.rs (whole file) | High | M | No tests. Trash sidecar/companion selection, Ship job construction (F007 would have been caught) and flush-before-move ordering are untested. | Extract job construction into pure functions and test them. |
| F051 | Test | src-tauri/src/raw/*.rs | High | M | Parsers of untrusted bytes, with only synthetic tests: no fuzz target, no real-camera fixtures, one malformed BMFF case. ORF/RW2/PEF/SRW are untried (HOW-IT-WORKS.md:73-75). | Add a `cargo-fuzz` target over `probe_source`, `capture_time_source`, `jpeg::layout` and `set_orientation`, and a corpus of the first 64KB of each format. |
| F052 | Test | package.json:12 | High | M | No frontend tests at all. Undo/redo, `refilter`/`neighbour`, `previousGroup`, shift-select across filters, `rankRun` and Ship's `resolve()` decide what gets marked and moved. | Add vitest for the pure functions (after F043); `api.ts` already supports injecting the mock. |
| F053 | Test | src-tauri/src/transfer.rs, src-tauri/src/ingest.rs, src-tauri/src/xmp.rs (test modules) | Medium | M | No tests for: cross-volume move, source-delete failure, foreign-sidecar collision, ingest Skip/UniqueName sidecars, a non-NotFound read error in `xmp::write`, or Urgency un-pick. | One test per finding F003, F005, F006, F012, F013, F019. |
| F054 | Test | .github/workflows/build.yml:78 | Low | S | CI runs only `cargo test --lib`: no clippy, no fmt, and the 12 `examples/` are never compiled. | Add `cargo clippy --all-targets -- -D warnings` and `cargo check --examples`. |
| F055 | Test | src-tauri/src/ai/mod.rs:59-110 | Medium | L | Every threshold says "MEASURED on N labelled faces", but neither the labelled set nor the scorer is in the repo. The thresholds can't be re-validated after a model change. | Commit the evaluation script and a small redistributable regression set. |
| F056 | Dependency | src-tauri/Cargo.toml:57 | Medium | M | `ort = "=2.0.0-rc.13"`: a prerelease, with default features, which (per Cargo.lock's ureq/lzma) download prebuilt ONNX Runtime at build time. That puts a network dependency and a supply chain outside crates.io into the build. | Document it in THIRD-PARTY-NOTICES. Consider `ORT_LIB_LOCATION`/vendoring. Track ort 2.0 stable. |
| F057 | Dependency | src-tauri/models/judge.onnx; THIRD-PARTY-NOTICES.md:27-37 | Medium | M | The 6MB model is GPL, but there is no hash, no export script, and no training code (`crops.py` is referenced at faces.rs:94 but absent). Under GPL, the preferred form for modifying weights is arguably the training pipeline. | Publish SHA-256s, the export script and a training-data statement. |
| F058 | Resource | src-tauri/src/lib.rs:178-190 | Low | S | Only ingest blocks quitting. Quitting mid Ship-move can leave a placed copy with its source still present, plus staging files. | Reuse the `Ingesting` guard for transfer and ship. |
| F059 | Resource | transfer.rs:172, ingest.rs:381, embed.rs:177, xmp.rs:271 (src-tauri/src) | Low | S | `.opencull-staging-*`/`.opencull-tmp` files left by a crash stay in users' photo folders forever, at RAW size. | Sweep stale staging files (dead pid) when a folder is opened or a copy starts. |
| F060 | Data safety | src-tauri/src/transfer.rs:160-167; src-tauri/src/ingest.rs:413-416 | Low | M | "Never overwrite" is an `exists()` check followed by `rename`, which on Unix silently replaces a file created in between. | Use `renameat2(RENAME_NOREPLACE)`/`renamex_np(RENAME_EXCL)`, or `hard_link`+unlink. |
| F061 | Dependency | package.json:24 | Low | S | The whole `shadcn` CLI is a runtime dependency only for `src/index.css:3` `@import "shadcn/tailwind.css"`. It is the source of all 7 `npm audit` highs (braces/micromatch via fast-glob). It is not shipped in the bundle, so the risk is low. | Vendor that one CSS file and drop the dependency. |
| F062 | Dependency | src-tauri/Cargo.toml:49, 58 | Low | S | `memmap2` and `ndarray` are not referenced anywhere in `src/` or `examples/` (verified by grep). | Remove both. |
| F063 | Dependency | package.json:36; tsconfig.json:24 | Low | S | `@types/node ^26` while CI uses Node 22. Plain `tsc` doesn't build `references`, so `vite.config.ts` is never type-checked. | Pin `@types/node` to `^22`; use `tsc -b`. |
| F064 | Error handling | src/lib/store.ts:531, 612; src/lib/faces.ts:34, 61, 93; src/components/cull/Dialogs.tsx:206, 218, 228 | Low | S | Empty `.catch(() => {})` calls. Settings shows success when `setCacheLimit`/`aiClearCache` failed. | Route these through `api.report` at minimum. |
| F065 | Correctness | src/components/ingest/IngestView.tsx:372-376, 403-408 | Low | S | Cancel unmounts immediately. If the copy had already finished, the orphaned promise then opens the destination folder anyway. | Track a `cancelled` ref. |
| F066 | Resource | src/components/cull/FacePanel.tsx:116-128; src/components/cull/Loupe.tsx:399-447; src/components/cull/Thumb.tsx:54 | Low | S | Image decodes are never aborted. There is no `pointercancel` handler (so a pan can get stuck), and a retry timer is not cleared on unmount. | `img.src=""` in cleanup, handle `pointercancel`, keep the timer in a ref. |
| F067 | Architecture | src-tauri/src/commands.rs (1,051 LOC), :640-727 | Low | M | The AI scheduler (threads, batching, saving) lives inline among the IPC adapters. | Move `ai_scan`'s worker loop into `ai/`. |
| F068 | Dead code | xmp.rs:304; pool.rs:83, 192; cache.rs:179; preview.rs:466; ai/score.rs:81, 87-95; src/components/ui/separator.tsx | Low | S | These are used only in tests or nowhere. `score::ranks` duplicates the TS `rankRun` and can drift from it. | Use `#[cfg(test)]` or delete; delete `separator.tsx`. |
| F069 | Consistency | src/lib/store.ts:901; src/components/ingest/IngestView.tsx:126; src/lib/faces.ts:82-94; src/lib/warm.ts:62 | Low | S | The generic `set(key, value)` can overwrite `photos`/`view`, and components call raw `useStore.setState`. | Narrow `set` to a UI-key union; add named actions. |
| F070 | Docs | docs/HOW-IT-WORKS.md:211-219, 248-258, 126 | Low | S | Says "three small tools" and lists 4 of 12 examples. The layout omits `ai/`, `ingest.rs`, `transfer.rs`, `embed.rs` and more. `examples/embed.rs` writes into its input file, unmentioned. The "12" vs "13" figure disagrees with ai/mod.rs:83. | Update the doc, and warn about `embed`. |
| F071 | Docs | src-tauri/src/transfer.rs:4-8; src-tauri/src/ingest.rs:5-7; src-tauri/src/pool.rs:8, 244 | Low | S | Module docs promise guarantees the code doesn't keep: F001/F005, F006, F004. pool.rs says "three rules" and lists four. | Fix alongside the underlying findings. |
| F072 | Docs | README.md:164-177 | Low | S | The shortcut table is accurate but omits redo, select-all, trash, clear colour, zoom, and settings. | Add the rows. |
| F073 | Security | src-tauri/src/editors.rs:170-172, 189-193 | Low | S | Paths go straight into editor args on Windows/Linux. A path starting with `-` would be read as an option, and children are never waited on. There is no shell, and the editor comes from a compiled-in table. | Reject paths that are not absolute. |

## Top 5: if you fix nothing else, fix these

### 1. F001 + F002 + F018 → F040: one sync-to-disk staging helper
Every place that "copies then trusts" goes through it.
```rust
// src-tauri/src/fsx.rs (new)
pub fn staged_copy(source: &Path, target: &Path, replace: bool) -> io::Result<()> {
    let staged = staging_name(target);            // one naming scheme
    let r = (|| {
        let len = std::fs::copy(source, &staged)?;
        let f = File::options().write(true).open(&staged)?;
        if len != std::fs::metadata(source)?.len() { return Err(short()); }
        if let Ok(m) = std::fs::metadata(source)?.modified() { f.set_modified(m).ok(); }
        f.sync_all()?;                            // ← missing today
        rename_no_replace(&staged, target, replace)?;   // F060
        sync_dir(target.parent().unwrap())         // ← so the rename survives power loss
    })();
    if r.is_err() { std::fs::remove_file(&staged).ok(); }
    r
}
```
Then in `transfer::place`: `fsx::staged_copy(source, target, replace)?; if moving { std::fs::remove_file(source)? }`. In `ingest::verified_copy`, add `to.sync_all()?` before `drop(to)`. `xmp::write` and `cache`/`times` call the same helper (with `sync: false` for the caches).

### 2. F003: don't overwrite a sidecar you couldn't read
```rust
// src-tauri/src/xmp.rs:262
let existing = match std::fs::read_to_string(&path) {
    Ok(s) => s,
    Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
    Err(e) => return Err(e),   // never clobber what we couldn't read
};
```
Pair it with F010, so this error actually reaches the user.

### 3. F004: make the crash guard real
```toml
# src-tauri/Cargo.toml
[profile.release]
codegen-units = 1
lto = true
opt-level = 3
-panic = "abort"
strip = true
```
Then add F008 (chunked EOI search with a length cap) and F009 (pixel cap plus `set_max_decoding_buffer_size`). An OOM is an abort whatever the panic strategy, so those caps are needed even after this change.

### 4. F041 + F042 → F005, F006, F007, F012, F024: one sidecar rule, one name-reservation rule
- Add `xmp::sidecars_of(photo) -> Vec<PathBuf>` and use it in trash, transfer and ingest.
- Add `names::reserve_group(dest, &[owner, companions…, sidecars…], policy) -> Option<String /*suffix*/>`. It returns one suffix for the whole group, and checks *every* target name, sidecars included.
- `transfer::free_suffix` and `ingest::Names::unique` both become thin calls to it.
- `ship` uses `transfer_job(…, true)` so companions are included.
- Update `ratings_replace_stale_ones_left_at_the_destination` so it replaces only a sidecar whose same-stem photo this job placed.

### 5. F010 + F011 + F020: surface every failure that touches a photo
- **xmp.rs:345.** Call `on_failure(path, err)` for sidecar errors as well as embed errors, then `app.emit("marks-failed", …)`. The store listens and shows a `notice`.
- **ShipDialog.tsx:118.**
  ```tsx
  onOpenChange={(o) => { if (!o && shippingRef.current) return; useStore.getState().set("shipOpen", o) }}
  ```
  Hide the X while `shipping`.
- **store.ts:931.** Wrap `moveToTrash` in try/catch. If `failed.size`, set `notice` with the reasons; Rust returns `(path, reason)` pairs.

## Quick wins (Low effort × Medium+ severity)
- [ ] F002: `to.sync_all()?` in `ingest::verified_copy`
- [ ] F003: NotFound-only fallback in `xmp::write`
- [ ] F004: drop `panic = "abort"`
- [ ] F007: include companions in `ship`, or document why not
- [ ] F009: cap pixel count in `Frame::is_picture`
- [ ] F010: surface sidecar write failures to the UI
- [ ] F011: block closing Ship while shipping
- [ ] F016: open-token instead of path check in `openFolder`
- [ ] F017: make `paths_exist`/`folders_exist` async
- [ ] F019: strip `photoshop:Urgency` on un-pick
- [ ] F025: collect ingest sidecar failures
- [ ] F028: chunk the judge batch
- [ ] F035: serialize the AI store outside the lock
- [ ] F037: `useShallow` in Settings
- [ ] F039: DOM-based modal check in the shortcut guard
- [ ] F062: remove `memmap2` and `ndarray`
- [ ] F074: default `embedInJpeg` to `false`
- [ ] F023: stop restoring mtime after writing into a JPEG

## Things that look bad but are actually fine

- **~41 `lock().unwrap()` calls in Rust.** With `panic=abort` there is no unwinding, so a poisoned mutex can't be seen; with F004 fixed, poisoning only follows a panic that is already being reported. The panic itself is the real risk, not these unwraps.
- **`editors.rs` spawning processes with paths from the frontend.** The editor comes from a compiled-in table (`EDITORS.iter().find`, editors.rs:179). There is no shell (`Command::args`), and macOS goes through `/usr/bin/open -a`. No injection is possible beyond F073's `-` prefix.
- **`opencull://` path traversal into the cache.** It can't happen. `cache.rs:76-82` hashes path+version+tier with xxh3_128 into `<2hex>/<30hex>.jpg`. The real exposure is reading *source* files (F030), not escaping the cache.
- **Ingest checking size rather than a checksum (ingest.rs:369-378).** A documented trade-off: re-reading every byte from a card costs minutes. The gap that matters is the missing sync to disk (F002), not the missing hash.
- **`embed.rs::afresh`.** It verifies byte for byte outside the packet, calls `sync_all`, renames atomically, and refuses files whose picture index would break. It is the most careful code in the repo; only F022 applies to it.
- **The RAW parsers' general robustness.** `read_at` is exact-or-None, IFD entries are capped at 1000, the directory walk is capped at 64 with a visited set (no cyclic-IFD loops), and every embedded range is `checked_add`'d and bounded by the file size (raw/mod.rs:355). There is no `unsafe`. The real risks are only the memory-exhaustion paths (F008, F009).
- **`ort = "=2.0.0-rc.13"` exact pin.** That is correct for a release candidate, since rc-to-rc versions break APIs. The debt is being on an rc at all (F056), not the pin.
- **Thread-local ONNX sessions rebuilt each scan.** `Session::run` needs `&mut`, and the scoped threads drop them at scan end. This costs load time but is not a leak.
- **zustand selectors.** Every selector except F037 returns a primitive or an existing reference, list cells are `memo`'d per index, and all three big lists are virtualized. The store is big, but it doesn't cause re-render storms.
- **Tauri `listen()` cleanup with the `stopped` flag (src/lib/api.ts:233-260, src/App.tsx:43-64).** It looks fiddly, but it correctly handles unmounting before the async `listen` resolves.
- **`.catch(() => 0)` on the conflict pre-checks (ShipDialog.tsx:233, IngestView.tsx:326).** On failure they fall through to the non-destructive policy (KeepBoth/UniqueName), so nothing is overwritten.
- **Hand-rolled FFT and days-from-civil maths (ai/key.rs:194-248, raw/mod.rs:330-336).** Both are small, tested, and avoid dependencies for a fixed 64×64 transform.
- **The IPC contract has no runtime validation.** All 40 commands, arg names and serde renames match `api.ts`, so adding zod-style validation would cost more than it buys.
- **No CI signing or notarisation.** The README is upfront about it, and it is a cost decision, not debt.

## Maintainer answers (2026-10-10)

The open questions were put to the maintainer. Their answers, and what each one changes in this audit:

| # | Question | Maintainer's answer | What it means for the audit |
|---|----------|---------------------|-----------------------------|
| 1 | Should Ship move RAW+JPEG together? | Yes, both should move. | **F007 is a confirmed bug.** Include companions in `ship` (`transfer_job(…, true)`). |
| 2 | Should JPEG embedding be on by default? | No. | **New finding F074.** Change the default to `false`. |
| 3 | Restore mtime after writing into a JPEG? | No. The modified date should update. | **F023 decided.** Remove the mtime restore in `embed.rs:144-149`. |
| 4 | Should a transfer replace a destination sidecar? | Never overwrite edits, but ratings may change. | **F005 and F014 decided.** Never replace a sidecar wholesale. If it belongs to the same photo, merge only rating, label and flag into it (F014's "touch only changed fields"). If it belongs to a different photo, give the incoming one a unique name. Rewrite the test `ratings_replace_stale_ones_left_at_the_destination` to assert this. |
| 5 | Keep `panic = "abort"`? | Turn crash recovery on if it helps safety. | **F004 decided.** Remove `panic = "abort"`. Still do F008 and F009, since running out of memory aborts either way. |
| 6 | Should `opencull://` serve files outside the open folder? | The home page should only show photos from that folder. | **F030 decided.** Allowlist the paths the backend has handed out: the open session's images, `folder_cover` results (HomeView.tsx:289) and ingest scan results. Reject everything else. Drop `Access-Control-Allow-Origin: *`. |
| 7 | Are the two focus scales deliberate? | Not intentional, but the results look right, so leave it. | **F048 downgraded to Low and accepted.** Add a comment at both sites noting that the two scales differ and the current rankings were judged correct, so nobody "fixes" one without re-checking rankings. |
| 8 | Publish the training photos? | No. A private way to add photos would be good, if it can be done safely. | **F055/F057 narrowed.** Publish the training and evaluation *code*, the model hashes and the export script, but no photos. For private photos: the evaluation script reads a folder path given on the command line (e.g. `cargo run --example evaluate -- ~/private-faces`). Labels live in a CSV inside that folder; it's listed in `.gitignore` and never committed, and the script prints only aggregate numbers. Anyone can then re-check the thresholds against their own photos without sharing them. Note: under GPL it is the training *code* that matters most to publish; withholding photos is normal. |
| 9 | Cache failures as "no faces"? | Photos that fail should be removed from AI analysis. | **F026 decided.** Don't cache a failure as an empty record. Store a distinct "couldn't analyse" state that the UI excludes from ranking and face filters. Cache it for content errors (a corrupt file), but not for I/O errors (a pulled card), so those retry on the next open. Either add PNG decoding to `decode_upright` or mark PNGs "not analysed" explicitly. |
| 10 | Checked AI results across OSes? | Not checked. | **Still open.** Soften the claim in HOW-IT-WORKS.md (and `ai/mod.rs:3-4`) to "should be close on every platform". Optionally add a test that runs both models on a few fixed synthetic images and compares the scores to saved values within a tolerance, in CI on all three OSes. |
| 11 | Re-sort after marking? | Only re-sort when the user refreshes. | **Feature, not debt.** Keep the current stable order. Add a "Re-sort" action (menu item plus rebindable shortcut) that calls the existing sort over `order`. Document the behaviour in the README. |

### Added finding

| ID | Category | File:Line | Sev | Effort | Description | Recommendation |
|----|----------|-----------|-----|--------|-------------|----------------|
| F074 | Data safety | src/lib/store.ts:570 | Medium | S | `embedInJpeg: load("embedInJpeg", true)`: rating a photo rewrites the user's original JPEG files by default. The maintainer wants this off. | Change the default to `false`. Users who already saved a preference keep it. With the default off, F021 and F022 affect only users who opt in. |

## Status on branch `tech-debt-fixes` (2026-10-11)

**Resolved:** every finding F001–F074 except those listed below. Each commit message names the findings it fixes.

**Partly done:**
- **F029:** AI workers are capped at 4, but not sized by available memory.
- **F042:** transfer and ingest use the same group-naming rule, but still as two implementations.
- **F045:** the loupe's remembered zoom level and the app's shortcut bindings still save prefs their own way.
- **F047:** commands still return `Result<_, String>`.
- **F057:** the model hashes are published; the export script and a training-data statement need the maintainer.
- **F069:** components still call `useStore.setState` directly in a few places.

**Needs the maintainer:**
- **X1:** HOW-IT-WORKS says 12 wrong "looking away" calls; ai/mod.rs says 13.
- **Q10:** whether AI results match on Windows and Linux.
- **F055:** the labelled sets and training code.

**Not verified:** the loupe, ingest and settings refactors type-check, build and pass the tests, but have not been tried in the running app.
