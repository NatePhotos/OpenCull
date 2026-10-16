# Choose a specific folder on a card

Branch: `feature/ingest-subfolders`, based directly on upstream `a8c7f32`.

Opening a card now browses its folders without selecting the whole card.
Open `CARD → DCIM → 100CANON` to select that leaf folder. Camera directory
names no longer stop navigation. Mixed folders containing both photographs
and subfolders can also be browsed. **Use this folder** explicitly selects
the currently displayed parent when a whole directory is wanted.
The system **Browse Folder** picker follows the same browse-first rule.

This changes source selection only. It works independently of multi-card,
culling-session, and appearance features. Original files are untouched.

Validation: 31 frontend tests and TypeScript/Vite build passed. Browser runtime
check verified zero scan requests while opening CARD/DCIM, a single scan of
`/DCIM/100CANON` when selected, and an explicit DCIM scan with **Use this folder**.
No browser JavaScript errors were observed.
