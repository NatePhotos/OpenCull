# OpenCull interface design guidelines

These rules apply across Home, ingest, culling, settings, menus and dialogs. Wisp is the reference theme for readable interface contrast; other presets retain their visual character.

## Readability and surfaces

Use semantic tokens for background, panel, row, input, selected state, text and focus. Avoid a single grey for every level. Wisp separates an ink workspace (#111827), navy panels (#162033), raised rows (#1d293d), and controls (#2b3b55). Borders (#71839f) identify controls and setting boundaries. Primary text is #f3f6ff; secondary copy is #bac8df. Violet (#b7a6ff) marks primary actions and active controls. Light mode uses paper surfaces with dark ink and a deeper violet.

Normal text must reach 4.5:1 against its actual background; large text, icons and necessary control boundaries must reach 3:1. Test computed colours in every colour mode, including hover and selected states. Do not lower descriptive copy opacity until it becomes difficult to read. Colour never carries state alone: use position, border, checkmark, text or icon as well.

## Hierarchy and density

Keep identity and supporting text above a horizontal action row. Use an 8px spacing rhythm: 8 within groups, 16 between controls or rows, 24 between sections, 32 before image collections. Setting rows have their own surface, 1px visible border, 8px radius and 16px padding; title and description belong together, control aligned at the start. Do not put borders around every text fragment.

Titles use a clear weight and size hierarchy. Home headings can scale from 36–56px with bounded container sizing. Section headings use 22–32px. Setting labels use 14px semibold; explanatory text uses 13px with 1.6 line height. Retain the user's font preference and native interface scale. Avoid fixed heights on text blocks; allow wrapping and vertical scrolling.

Controls need a 40px hit area where practical, with a visible keyboard focus outline separated from their edge. Selected tabs need both a border and stronger text; checked switches need a contrasting thumb position and track. Disabled controls retain labels and never look active. Menus, dialogs and popovers use opaque readable surfaces. Dialog content stays within the viewport and scrolls on smaller windows.

## Photograph-first workspace

Photo pixels, marks and rating colours are never recoloured by the theme. Keep the loupe surround neutral; 18% grey is #767676. Native-aspect strips use displayed rotation and crop proportions. Keep selection rings, rank labels and resize edges visible over mixed photographs. Theme accents belong on interface controls, not as a tint over previews.

Recent folder contact sheets stay visible. Compact presets may reduce card dimensions without turning images into text-only entries. Bulk actions appear after the first selection. Removing recent history must clearly distinguish itself from deleting source photos. Errors use readable text, not just red borders. Loading retains useful previews where possible.

## Verification

Check Home, ingest, culling and all Settings tabs in dark, light and 18% grey, at small and large windows and at 100%, 150% and 200% native interface scale. Verify keyboard focus, scrolling, switch/tab state contrast and forced colours. Use screenshots plus measured contrast; do not infer accessibility from palette names.

Wisp is supplied by feature/wisp-theme and depends on feature/rapidraw-theme's preset infrastructure. It changes no personal default automatically and loads no remote fonts or assets.
