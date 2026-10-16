# Multiple cards, one shoot

Ingest from Card now accumulates selected cards/folders in a removable source list. All selected sources are scanned recursively, including nested DCIM folders, and displayed in one combined selection with source labels. Overlapping folders do not duplicate photos. Removed/disconnected sources are reported before copying.

A cross-source RAW/JPEG pair requires one RAW and one JPEG with matching filename stem, camera make/model, and capture timestamp. Missing metadata, differing camera/time, or multiple candidates keep entries separate. Existing same-folder pairs remain intact. The JPEG supplies the preview and both files travel together under the existing collision/cancellation rules.

Standalone: copies the selected combined shoot through existing ingest and opens the destination. Integration with cull-before-ingest is implemented separately, so this feature has no prerequisite branch.

Validation: `npm test`, `npm run build`; Rust tests cover cross-card pairs, ambiguous/missing metadata, differing timestamps/cameras, overlapping roots, and disconnected sources. Native checks should include actual JPEG-only and RAW-only cards.
