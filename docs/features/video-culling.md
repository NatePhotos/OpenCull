# Video culling

Ignore videos is enabled initially, in Settings → Culling and the card browser. Disable it to list camera video containers alongside photos, keep/reject/rate them, copy them, and play supported codecs in the loupe. Native playback controls provide pause, seek, and volume; their keyboard events stay with the player. Unsupported codecs show a message and remain selectable for import. No transcoding or codec downloads are performed.

Playback uses a bounded, token-protected loopback stream on Linux because WebKitGTK's media backend rejects custom asset schemes. Other platforms use Tauri's range-capable asset protocol, with an empty initial scope. Only a video already belonging to an open session is granted individual file access. Videos use a thumbnail icon and skip image preview warming, histogram processing, and face analysis. Normal folder/video sidecars follow existing mark behavior; integration with source-safe card review is separate.

Linux playback depends on system WebKitGTK/GStreamer codec support. Formats are recognized as containers, not promises that every codec inside them will decode.

Validation: `npm test`, `npm run build`, native desktop smoke checks with a playable video, unsupported video, video-only source folder, seeking, and source-copy preservation. No other feature branch is required. Tauri asset protocol reference: https://v2.tauri.app/reference/javascript/api/namespacecore/#convertfilesrc
