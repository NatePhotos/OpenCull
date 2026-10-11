# How OpenCull works

The detail behind the [README](../README.md): how previews are made and how
fast, what the face analysis measures, how to check a build, and what is not
here yet.

Where this says "the Mac app", it means the earlier Mac-only app OpenCull
began as a rewrite of. Many of the figures are comparisons against it, and
the two read and write the same sidecar files.

## How previews are made

No RAW is decoded. A camera writes finished JPEGs into every RAW it saves, and
says where they are in a directory at the front of the file. The app reads that
directory and lifts the pictures out.

Four sizes, each made the cheapest way that size allows:

| | For | How |
|---|---|---|
| Thumbnail | filmstrip, grid | cut from the camera's small screen-sized preview |
| Screen | the loupe, at once | that same preview's own bytes, handed over untouched |
| Fit | the loupe, sharp | the full-size preview, decoded at half size |
| Full | zoomed in | the full-size preview's own bytes, handed over untouched |

A JPEG straight from the camera holds no screen-sized preview, only itself,
so one is made for it (57 ms from a 30-megapixel file on a USB SSD) and kept.
It is what a held arrow key shows: small enough to be decoded faster than the
key repeats, where the loupe rendering is not.

Thumbnails and fit previews are kept on disk, so each is only ever made once.
A photo's orientation is corrected by telling the picture which way up it is,
rather than by re-encoding it.

A folder opens in the order it was shot, on its true first frame. Filenames
cannot give that order when two cameras were used — each numbers its own
frames — so the capture times are read as the folder is listed, by a route
much shorter than a full read of each file's metadata, and remembered for the
next time the folder is opened. If a folder is too slow to time within a
second and a half, it opens by filename instead and settles afterwards.

The loupe stacks them: thumbnail, then screen, then fit, each shown as it
arrives. The screen-sized one costs nothing to make, so a photo jumped to cold
shows a real picture in about a millisecond while its fit rendering is made.

While nothing else is going on, fit renderings for the two thousand photos
around the current one are made in the background, so that a jump lands on one
already made. This stops the moment anything is asked for, and runs on a third
of the machine's cores, kept apart from the ones the photo on screen uses.
It can be turned off in Settings.

Measured on an M-series Mac, over 3,002 Sony RAWs on a USB SSD, from scratch:

| | |
|---|---|
| Listing the folder | 0.29 s |
| Learning when every photo was taken | 0.075 ms a photo |
| Opening a 2,000-photo folder for the first time, in shooting order | 0.4 s |
| Finding a file's previews | 0.04 ms |
| Thumbnails | 2,580 a second |
| Loupe previews | 164 a second |
| A photo that was looked ahead to | 1 ms |
| A photo jumped to cold: something to look at | about 1 ms |
| A photo jumped to cold: the sharp rendering | about 60 ms |
| The same jump while the folder is being prepared | no slower |
| Preparing the folder in the background | about 70 photos a second |
| Any of them a second time | under 0.1 ms |

Without libjpeg-turbo the middle of that table is slower: 1,640 thumbnails
and 72 loupe previews a second, about 115 ms for a cold sharp rendering, and
23 photos a second in the background.

Sony ARW, Nikon NEF, Canon CR2 and CR3, Fuji RAF, Leica DNG and plain JPEG have
all been opened with it. ORF, RW2, PEF and SRW share the same structure and
should work, but no file of those kinds has been tried.


## What is here, and what is not

Here: opening folders, with or without subfolders; RAW + JPEG pairing, or
either kind alone (a pair is one photo with one sidecar, so a mark given to
it is on both files; shown alone, a RAW or a JPEG leaves the other where it
is); the
loupe with zoom and pan, and zooming to a box dragged out with Command held
(Control on Windows and Linux); filmstrip, grid and scene views; stars, flags
and colour labels, written as sidecars; filters and sorting; burst grouping by
capture time; EXIF panel and histogram; undo; move to trash; copying and moving
photos to another folder; Ship; rebindable shortcuts; recent folders; the Mac
app's settings, less the ones that belong to features not here.

A JPEG's marks are also written into the JPEG itself, in its XMP packet,
because that is where Lightroom, Bridge and others keep a JPEG's rating and
the only place they look for it; a sidecar is still written, so the app's own
record does not depend on it. It is the one place the app writes into a
photograph. Only the packet is touched, never the picture. Where the packet
has room it is written over at the same length and nothing else in the file
moves; where it has not, the file is written out afresh beside itself, every
byte outside the packet checked against the original, and only then swapped
in. MEASURED on 7 and 17 MB JPEGs: 6 to 10 ms in place, 14 ms written afresh,
the decoded picture and the EXIF identical afterwards. The file's date is put
back, so its previews stay good. Pick and reject flags are written the same
way, though Lightroom keeps its own flags in its catalogue and reads none
from a file. It can be turned off in Settings.

Ingest copies photos off a card: it lists cards and outside drives, shows what
is on them, and copies the chosen photos with their JPEGs and in-camera stars.
Every copy is checked for size before it is given its name, nothing is left
half written, and nothing is replaced unless that is asked for. Quitting is
refused while a copy runs. MEASURED reading a CFexpress card on a Mac, into the
internal drive: 5,320 photos listed in 0.36 s, capture times read at 1.3 ms a
photo, and copied at 1.1 GB/s one at a time and 1.3 GB/s four at a time. Finding
cards on Windows and Linux is written and has never been run.

Face analysis finds the faces in each photo and judges them — eyes shut,
looking away, how sharp, subject or background — then marks the photo and
ranks the frames of each burst. It uses no part of the system it runs on: two
model files, run through ONNX Runtime, on every platform. Results should be
very close everywhere, though a verdict right at a threshold may differ
between CPUs; only the Mac has been checked. One finds faces; the other judges each one, and says when what
was found is not a face at all — a hand, a plate of food, the back of a head.
MEASURED against the Mac app on 356 labelled faces: eyes 0.936 to 0.892,
gaze 0.962 to 0.880, background people caught 91% to 71% at the same cost. Of
what the finder wrongly boxes, 88% is thrown out, and with it 1.2% of real
faces, nearly all seen from behind. Looking away is called on the judge's
looking-away score or on pupils sitting to one side of a face held square:
on 615 labelled faces that catches 79% and wrongly accuses 5.4%, and on 1,360
faces known to be looking at the camera it wrongly accuses 12. Bursts are split
by the clock; within a burst, frames are grouped into poses by where the same
people stand (below).

A photo with nobody in it — a table, a tray of pastries, a cufflink, a
venue — is marked on how sharp its subject is, and the face panel shows that
part of the photo where the faces would be. The Mac app finds the subject
with Apple's map of where the eye goes; here it is what stands out from the
rest of the picture (the spectral residual) and is in focus, leaning to the
middle of the frame, in 16 ms a photo. Looked at on 100 photos with no faces,
with the choice drawn on each, it is on the subject in about nine of ten; it
misses a small figure in a wide landscape.

Within a burst each person is followed from frame to frame by where they
stand, and each frame is marked on how open their eyes are as a share of
their own best — narrow eyes wide open and wide eyes half shut measure alike
until they are set beside the same person a moment earlier. The face panel
shows that share for each face. Looked at by eye on 200 such comparisons:
under 60% nearly every one is a blink, half a blink or eyes cast down; over
80% the frames cannot be told apart, and nothing is taken off.

The same is done for a pose shot again. A group lined up and photographed
five times, a press at a time, is five bursts by the clock and one pose to
the photographer: frames taken within twenty seconds of one another with the
same people in the same places are compared, and ranked, as one, and are
one scene in Scene View and one group in the filmstrip. An upright photo
and one on its side are never grouped, however alike: turning the camera
is making another picture. "Intelligent grouping" in Settings turns poses
off, leaving only the frames of one press grouped. A folder still being scanned starts out grouped by the clock and
settles into poses as the scan reaches them. In a test
of 2,328 photos that is 512 poses holding 1,579 photos, the largest of 25
frames, and 61 subjects caught blinking whose eyes had not been called shut;
in one of 134 photos each taken with its own press, 28 poses where the
clock alone found 2 bursts. Someone who moves across the frame
between frames, turns their head away or wears dark glasses is not followed.

Ship opens photos in an editing program, copies or moves them, or both. It has
only been run on a Mac, and only its copying has been run at all: opening
Lightroom Classic and Aftershoot is the same call the Mac app makes, but
Lightroom (the cloud one), Capture One, and everything on Windows and Linux
were written without the program there to try. Nothing at a destination is
ever replaced, and nothing is left there half written.

Not here yet:

- **Burst grouping by picture** — the Mac app splits frames close in time that
  look nothing alike. Here bursts are by the clock alone; only poses within a
  burst are told apart, and only by where the people in them stand.
- **RAW rendering and auto-exposure** — needs a real RAW decoder. Cameras that
  embed only a small preview (older bodies, mostly) therefore cannot be zoomed
  further than that preview goes.
- **HEIC** — nothing portable decodes it.
- **Adobe RGB previews without a colour profile.** A profile embedded in a
  preview is carried through. Cameras that mark Adobe RGB only in EXIF will
  show slightly muted.

Lens names are read from standard EXIF. Nikon and older Canon bodies keep
theirs somewhere private, so those show as unknown.


## Checking it works

The logic has unit tests:

```bash
npm test
```

That runs the interface's tests (Vitest, `src/**/*.test.ts`) and then the
Rust tests. Among the latter, `raw::robust` feeds every file reader thousands
of damaged files. CI also runs `cargo clippy --all-targets -D warnings`.

For a longer, coverage-guided search of the same readers:

```bash
cargo install cargo-fuzz
cd src-tauri && cargo +nightly fuzz run parse
```

And the app can run itself against a real folder with no window on screen —
opening it, loading previews the way clicks would, and printing what happened:

```bash
npm run tauri build -- --debug --no-bundle
```

```bash
./src-tauri/target/debug/opencull --selftest /path/to/a/folder/of/photos
```

It never writes to the folder. This is the thing to run first on a new
machine: the parts most likely to break on another platform are the ones a
compiler cannot see.

It shows a window one point square in a corner of the screen, beneath
everything else. That is deliberate: a web view in a window that is fully
hidden has its scripts suspended by the system a few seconds in.

Small tools for looking at files and measuring the app directly, in
`src-tauri/examples`. Each says how to run it at its top:

```bash
cd src-tauri && cargo run --release --example probe -- /path/to/photo.ARW
```

| Tool | What it does |
|---|---|
| `probe` | Says what previews and details a file holds |
| `previews`, `screen` | Makes a file's previews and writes them out to look at |
| `bench`, `bench_fit`, `throughput` | Times decoders on a file, and the whole pipeline over a folder |
| `times` | Reads a folder's capture times the quick way, and times it |
| `ingest` | Times listing and copying a real card (into a temporary folder it removes) |
| `analyze`, `bursts`, `key` | Runs the face analysis, burst marking or subject finding over a list of photos, to CSV |
| `evaluate` | Checks the face analysis against your own labelled photos and prints only totals; see below |
| `embed` | Writes a rating into a JPEG. **It writes into the file it is given — use a copy.** |

Add `--features turbo-system` (or `turbo`) to run them with libjpeg-turbo.
Only `embed` writes anything to a photo.

### Checking the face analysis on your own photos

The thresholds in `src-tauri/src/ai` were measured on labelled photos that
are not in this repository. To check them after a change, on photos you
keep to yourself:

```bash
mkdir private-eval    # git ignores it
# put photos in it, and a labels.csv: file,eyes_shut,looking_away,soft
cd src-tauri && cargo run --release --example evaluate -- ../private-eval
```

It prints how many of each problem it caught and how many it raised
wrongly, and nothing about any one photo unless `--misses` is given.


## Building: the fast JPEG library

Previews are decoded and written by libjpeg-turbo, which is about two and a
half times as fast as the pure-Rust code the app can otherwise fall back on.
It is the one part of the build that is not plain Rust, so each platform has
one thing to install first:

- **macOS** — `brew install jpeg-turbo`
- **Windows, Linux** — CMake, and NASM on Intel and AMD machines. The library
  is then built from source as part of the app.

Either way it is built into the app; nothing extra is needed to *run* it.
Which of the two is used is set per platform in `src-tauri/tauri.*.conf.json`.

To build without it — on a machine with none of the above, say — clear the
feature list. The app works the same and makes new previews more slowly:

```bash
npm run tauri build -- --config '{"build":{"features":[]}}'
```

A folder can be named on the command line to open it straight away.


## Layout

```
src-tauri/src/
  raw/          finding previews and EXIF inside files, by reading directories
  preview.rs    decode, shrink, turn upright, encode
  pool.rs       what gets made first: the photo on screen, then newest asked
  cache.rs      previews kept on disk
  serve.rs      hands previews to the web view by address, for listed files only
  scan.rs       listing folders, pairing RAW with JPEG
  times.rs      capture times remembered per folder
  burst.rs      which frames came from one press of the shutter
  xmp.rs        sidecar ratings, read and written
  embed.rs      ratings written into a JPEG itself (off unless turned on)
  fsx.rs        writing files so a crash never leaves one half written
  transfer.rs   copy and move to another folder, and Ship
  ingest.rs     cards: finding them, listing them, copying off them
  editors.rs    opening photos in an editing program
  faces.rs      cutting faces out of a picture
  ai/           finding and judging faces, the folder scan, what it remembers
  commands.rs   what the interface can ask for
src-tauri/examples/  tools for measuring, above
src-tauri/fuzz/      the parsers' fuzz target
src/
  lib/          state, culling rules, shortcuts, the bridge to the backend
  components/   home screen, loupe, filmstrip, grid, toolbar, panels, ingest
```


## Measured on a Mac

Everything above was built and measured on macOS. The app has since been
tested and confirmed working on Windows and Linux, but the figures given here
are the Mac's.
