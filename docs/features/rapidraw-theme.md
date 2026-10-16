# RapidRAW inspired design preset

Branch: `feature/rapidraw-theme`, based directly on upstream `a8c7f32`.
No other feature branch is required. Select **Settings → Culling → Design theme
→ RapidRAW inspired**. OpenCull's existing design remains the default.

The reference is RapidRAW's first library screen at
[`MainLibrary.tsx`](https://github.com/CyberTimon/RapidRAW/blob/e90a9bba7422ed833892e4945f06da35a65470d2/src/components/panel/MainLibrary.tsx):
a split image/action layout, prominent identity, neutral surfaces, and direct
session actions. This is an original implementation using OpenCull's existing
folder, settings, keyboard, card detection, and recent-folder controls.
No RapidRAW code, photographs, or logos are distributed.

The image panel uses the latest available recent folder's local preview, or
bundled original SVG artwork. Small windows show the action panel alone.
Space Grotesk is bundled offline under its SIL Open Font License. The preset
includes that font, with an option to use the normal interface font instead.
Selecting another design removes the preset font override without changing
the separate interface-font preference.

Preferences `visualTheme` and `visualThemeFont` use the shared preferences API,
so they also persist through the optional XDG configuration feature.
With appearance modes installed, dark, light, and 18% grey remain independent
of the design preset. Rating colours and photograph pixels are unchanged.

Validation: 33 frontend tests and production TypeScript/Vite build passed.
Chromium runtime checks cover selecting the theme, persistence after reload,
font override opt-out, returning to the default design, and desktop/small
window layouts with no horizontal overflow or JavaScript errors.
