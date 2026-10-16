# Feature guides

These guides describe the combined contribution and its shared settings, session and scan infrastructure.

- [Card review before importing](cull-before-ingest.md), [multi-card sessions](multi-card-session.md), and [subfolder selection](ingest-subfolders.md).
- [Saved sessions](RECENT_SESSIONS.md) and [video review](video-culling.md).
- [Cached AI analysis](AI_CACHE.md), [MobileCLIP grouping](MOBILECLIP_GROUPING.md), [tighter time grouping](TIGHTER_GROUPING.md), and [motion blur](MOTION_BLUR.md).
- [Face zoom](FACE_ZOOM.md) and [face outlines/spotlight](FACE_OVERLAYS.md).
- [Settings transfer](SETTINGS_TRANSFER.md), [native preferences](xdg-config.md), and [view preferences](view-mode-preferences.md).
- [Home layouts](HOME_LAYOUTS.md), [interface scale](INTERFACE_SCALE.md), and [first-launch guidance](FIRST_LAUNCH.md).
- [Experimental filmstrips](EXPERIMENTAL_FILMSTRIP.md), [crop workspace](CROP_WORKSPACE.md), and [interface transitions](INTERFACE_MOTION.md).
- [NVIDIA graphics](NVIDIA_GRAPHICS.md) and [CUDA setup](../CUDA-TRIAL.md).

Experimental controls remain in Settings. Card review does not write to source cards; deleting a saved session does not delete photos. Source deletion is a separate explicit action.

Some individual guides retain implementation history and historical validation measurements. The current code and automated checks are authoritative for supported options.
