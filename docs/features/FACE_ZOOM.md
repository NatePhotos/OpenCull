# Face zoom target

Z toggles centre zoom; Space toggles face zoom, falling back to centre when no face is available. Settings → Zoom → Prefer highest-scoring face selects the sharpest scored subject instead of the first visible main subject. Saved custom bindings follow the renamed actions.


The existing per-face numeric score is sharpness. The target is the highest scored visible subject, falling back to a scored background face if necessary. Unknown scores, invalid geometry and faces whose centres are outside the crop are excluded. With AI off, no analysed faces or missing dimensions, the usual centre target is used immediately; finishing analysis never moves the view by itself.

This preference is saved and included in Settings import/export.

Face targets are placed at 28% of the view height, horizontally centred, to keep the person's body in view. This applies to face-panel clicks and the face-targeted Space option. The picture is still clamped at its edges; normal photo clicks keep their existing placement.
