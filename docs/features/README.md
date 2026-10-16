# Independent feature initiative

Approved feature branches (each starts at upstream main):

- `feature/cull-before-ingest`: reuse culling before copying; local source-safe marks; choose files at finish; copy or import in place.
- `feature/multi-card-session`: combine sources; conservatively pair RAW/JPEG across cards; one session.
- `feature/configurable-ui`: settings for panel visibility and sizes, preserving defaults.
- `feature/appearance-modes`: dark, light, 18% grey; preserve default dark.
- `feature/font-selection`: system, offline Space Grotesk, custom local family; preserve default system.
- `feature/video-culling`: ignore toggle, supported playback, import selection; default ignore.
- `feature/xdg-config`: atomic native settings, Linux XDG path, migration from webview preferences.

No feature requires another. Combined behavior and overlap resolutions belong on `integration/linux-features`. No automatic merges or upstream submissions.

Personal configuration: Space Grotesk, 18% grey, existing panel visibility/sizes, videos ignored. Card workflow reviews first, then chooses Copy (initially all non-rejects) or Import in place (only selected members). No card writes during review or implicit deletion.
