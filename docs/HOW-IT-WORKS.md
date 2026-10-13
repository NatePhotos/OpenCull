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

With **Write ratings and crops into JPEGs** turned on in Settings — it is off unless
asked for — a JPEG's marks are also written into the JPEG itself, in its XMP
packet, because that is where Lightroom, Bridge and others keep a JPEG's rating and
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
ranks the frames of each burst. It uses no part of the system it runs on:
three model files, run through ONNX Runtime, give the same answers on every
platform. One finds faces; one judges each face, and says when what was
found is not a face at all — a hand, a plate of food, the back of a head; the
third takes a closer look at the eyes alone.
MEASURED against the Mac app on 356 labelled faces: eyes 0.936 to 0.892,
gaze 0.962 to 0.880, background people caught 91% to 71% at the same cost. Of
what the finder wrongly boxes, 88% is thrown out, and with it 1.2% of real
faces, nearly all seen from behind; where the finder itself was unsure of a
box, less is asked of the judge before it goes. Who a photo is about is
decided for its faces together: each is weighed on how large it is beside
the largest, how sharp beside the sharpest, how far to the side it sits, and
how sure the finder and the judge were of it. In a test of 1,782 faces graded
by eye across 79 shoots, each shoot answered by weights fitted without it,
this leaves out 60% of the people in the background and 0.6% of real
subjects; the size rules it replaced left out 47% and 0.6%. In posed groups
it leaves out 1 face of 1,360. Looking away is called from two
answers. The judge's own looking-away score reads mostly the turn of the
head: a head held square with the eyes turned aside gets past it, and it
accuses people whose head is turned and whose eyes are on the lens. So each
face large enough to be accused is looked at once more: its two eyes are cut
from the full-size photo, each centred on the finder's own eye point, and a
small network shown only those — and told how far the head is turned, since
eyes swung back to the lens from a turned head are looking at the camera —
says whether they are on the camera. A face is looking away if that closer
look is sure of it, or if the score says so and the closer look goes some
way along with it. MEASURED with the whole app on faces none of it learned
from, against the score alone: of 101 looking away in 25 shoots held out
whole, 81% caught where 74% were, with 4.2% of 190 looking at the camera
accused where 2.6% were; on 185 graded by hand, each counted for as many as
its kind is common, 72% caught where 60% were, and 0.3% accused where 1.3%
were; on 646 faces in posed groups, 3 accused. It adds nothing that can be
measured to the time a photo takes.

What looking away costs a photo depends on who else is in it. Where nobody
is looking at the camera it is the photograph — a couple looking at each
other, a table of guests, a child with a toy — and costs nothing. Where
someone else is looking, it costs a little: enough to put a frame behind one
in which everyone looked, not enough to sink it, and far less than a blink.
MEASURED: a photographer shown 80 photos with faces flagged as looking away
called it a flaw in none of the 43 where everyone was flagged and in 3 of
the 30 where someone else was looking; and on 734 photos rated by hand, the
mark tells the good from the bad 81% of the time this way, where counting
every look away in full it told 77%. Bursts are still split by the clock
alone.

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
  look nothing alike. Here it is the clock alone.
- **RAW rendering and auto-exposure** — needs a real RAW decoder. Cameras that
  embed only a small preview (older bodies, mostly) therefore cannot be zoomed
  further than that preview goes.
- **HEIC** — nothing portable decodes it.
- **Adobe RGB previews without a colour profile.** A profile embedded in a
  preview is carried through. Cameras that mark Adobe RGB only in EXIF will
  show slightly muted.

Lens names are read from standard EXIF. Nikon and older Canon bodies keep
theirs somewhere private, so those show as unknown.


## Cropping and turning

A crop is an instruction, not a picture. Nothing is rendered from it: the
previews stay the whole frame, and a cropped or turned photo is shown by
placing that frame behind a window, in CSS. So framing a photo makes no
preview, and a photo with no framing is drawn by exactly the code that drew
it before.

It is saved in the photo's sidecar in the fields Adobe's programs use, so
Lightroom and Camera Raw open the photo cropped the same way and can still
change it: `crs:CropTop`, `CropLeft`, `CropBottom`, `CropRight`, `CropAngle`,
`CropConstrainToWarp` and `HasCrop`, and `tiff:Orientation` for a quarter
turn or a mirror. A crop made in those programs is read the same way and
shown here.

Three frames are kept apart (`framing.rs`, `src/lib/framing.ts`). Adobe
stores a crop against the picture as it is stored, turned no way at all — the
top-left and bottom-right corners of the turned rectangle, as fractions, and
the turn in degrees. MEASURED on four sidecars Adobe wrote, one for a photo
stored on its side: read that way each comes out at exactly the 3:2 it was
locked to, and read any other way none does. The previews are that picture
turned the way the file says; the view is that, turned further by the
photographer. The interface only ever deals in the view, and the conversion
is in one place, at the sidecar.

A sidecar an editing program has worked on holds saved snapshots, looks and
presets, each with its own copy of the same fields. MEASURED on about 15,000
sidecars from real shoots: none could not be followed; 1,252 of the first
3,000 held more than one block, and 206 held crop numbers only inside
snapshots. So the sidecar is walked as a document (`sidecar.rs`), and only
the fields on the photo's own description are read or changed. Every change
was made in memory to each of those sidecars and read back: none touched a
byte outside the photo's own fields. A sidecar that cannot be followed with
certainty is left alone, and that photo cannot be cropped here.

With **Write ratings and crops into JPEGs** on, a crop also goes into a
JPEG's own packet, which is the only place Lightroom reads a JPEG's crop
from. Which way up a JPEG is, is never written there: the file says that
itself.

A crop can be copied from one photo and pasted onto others. It is carried as
a share of the photo — where its middle is and how much it takes, across and
down — so it lands on the same part of a photo of any size, and is kept on
the picture if a tilt would take it off.

Not done, and worth knowing:

- Face analysis is not run again for a photo that has been turned, and faces
  a crop leaves out are still listed.
- The histogram and the dimensions in the EXIF panel are of the whole frame.
- Lightroom reads a DNG's settings from inside the DNG, so a crop saved
  beside one shows here and not there.
- A camera set to shoot a shape other than its sensor's (16:9, square) saves
  a JPEG and a preview of that shape beside a RAW of the whole sensor. A crop
  drawn on one would not be the same crop on the other, so such photos are
  not cropped here.
- That Lightroom opens a crop made here as it was drawn is by the arithmetic
  above and has not yet been looked at in Lightroom itself; nor has a crop
  on a mirrored photo, of which there was no real example to measure.


## Updates

As it opens, the app asks GitHub for `latest.json` from the newest published
release: a small file naming that release's version and, for each system,
where its update file is and what its signature is. If the version is newer
than the one running, the app offers it — on the home screen only, never over
a folder being culled — and, if asked to, downloads the file, checks the
signature, puts the new version in place of the old and reopens. A version
can be skipped: it is not offered again unasked, the one after it is, and
Check Now in Settings still finds it. Nothing is
sent but the request itself. It is the only thing the app uses the internet
for, it is done by the backend so the view has no leave to reach the network
at all, and Settings has a switch for it.

A download is installed only if it was signed with the project's update key.
The public half of that key is in `tauri.conf.json`; the private half is not
in this repository. The signature also records which version it was made for,
and the app requires that to match what `latest.json` claims, so an old
release cannot be passed off as a new one. This is a different thing from the
paid certificates the systems themselves ask about, which the project does
not have.

MEASURED on a Mac, against a stand-in server: a build of one version found a
signed build of the next, installed it, and was that version afterwards; the
same file with one byte changed was refused and nothing was replaced. The
Windows and Linux halves are the updater's own and have not been run here.

Making a release with updates in it:

- The build makes the update files only when it is given the key, as the
  `TAURI_SIGNING_PRIVATE_KEY` secret of the repository (`build.yml`). A build
  without it — a fork's pull request, or one made locally — makes ordinary
  installers and nothing else, and needs no key.
- `latest.json` is put on the release by the build. The app reads it from the
  release GitHub calls latest, so a release is offered to nobody while it is
  still a draft or marked as a pre-release.
- Lose the key and the versions already installed can never update
  themselves again: each would have to be replaced by hand, once, with a
  build that trusts a new key.


## Checking it works

The logic has unit tests:

```bash
npm test
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

Three small tools for looking at files directly:

```bash
cd src-tauri && cargo run --release --example probe -- /path/to/photo.ARW
```

`probe` says what previews a file holds; `bench` and `bench_fit` compare
decoders on it; `throughput` runs the whole pipeline over a folder. Add
`--features turbo-system` (or `turbo`) to run them with libjpeg-turbo.


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
  raw/         finding previews and EXIF inside files, by reading directories
  preview.rs   decode, shrink, turn upright, encode
  pool.rs      what gets made first: the photo on screen, then newest asked
  cache.rs     previews kept on disk
  serve.rs     hands previews to the web view by address
  scan.rs      listing folders, pairing RAW with JPEG
  xmp.rs       sidecar ratings, read and written
  burst.rs     which frames came from one press of the shutter
  update.rs    finding a newer version and putting it in place
  framing.rs   how a photo is turned and cropped, between sensor and view
  sidecar.rs   reading and changing a photo's own fields in a sidecar
  commands.rs  what the interface can ask for
src/
  lib/         state, shortcuts, the bridge to the backend
  components/  home screen, loupe, filmstrip, grid, toolbar, panels
```


## Only tried on a Mac

Everything above was built and measured on macOS. The code avoids anything
platform-specific — positioned reads, trash, and the preview addresses each
have a Windows form — but none of it has been run there.
