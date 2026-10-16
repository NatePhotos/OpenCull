# Cull before importing

Ingest from Card → select a source → Start Culling opens the existing workspace directly on the card. Review keeps ratings, flags, colours and crops in a local saved session; source XMP and embedded JPEG writes are bypassed even when normally enabled. Resume card session on Home restores the latest session; disconnected cards must be reconnected.

Finish review initially selects all non-rejects, with All/None and individual selection. Copy selected files uses the existing staged, size-checked transfer path with collision renaming and writes marks/crops only at the destination. A RAW and its JPEG travel together. Completed copies are recorded for retry after cancellation or failure; retries retain the chosen destination. Import in place opens only chosen members without copying or source writes. Sessions live in the native application data directory, separately from configuration.

Move and Ship cannot bypass this workflow. Deleting remains an explicit source action and always asks during card review. Reject never deletes. Ordinary folder culling retains its existing sidecar behavior.

Validation: frontend tests include local-only marks/crops/undo and forced deletion confirmation. Rust tests check unchanged source bytes, destination sidecars, paired copying, source-destination refusal, and session persistence. Run `npm test`, `npm run build`, and a native desktop smoke test. This branch works independently; integration with multi-card selection is separate.
