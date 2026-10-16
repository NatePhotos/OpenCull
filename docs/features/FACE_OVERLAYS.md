# Experimental face overlays

Settings → AI → Experimental Features contains two independent, initially disabled options:

- **Detected face outlines** draws every detected face, including background faces hidden from the Faces panel.
- **Face hover spotlight** dims the photo outside the hovered face. Keyboard focus on a face cell does the same. The photo stays at its existing zoom and pan; clicking still uses the normal face-zoom command.

Outlines and the spotlight follow the same image sheet as zoom and pan. All four corners are transformed through crop rotation and mirroring, and the overlay is clipped to the displayed photograph. An SVG over the existing previews adds no image download, second decoded photo or blur effect, and cannot intercept photo gestures.

Leaving or blurring the face clears its spotlight. Hiding the panel, turning the feature/AI off, changing photos, or replacing analysis cannot spotlight a stale face on the current image. Retained cells from a previous photo cannot target the new photo. These are display aids for Loupe and Stack; they are not photo edits and do not appear on exported files or video.

The two preferences are saved and included in Settings import/export. The transient hovered face is excluded.
