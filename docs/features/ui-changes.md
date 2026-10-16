# Clearer UI headings and ingest instructions

Branch: `feature/ui-changes`. Prerequisite: `feature/cull-before-ingest`.
This branch starts from that feature because the moved **Review first**
instruction describes its cull-before-copy workflow. Review its PR against
`feature/cull-before-ingest` to see only the UI changes.

- Ingest uses a semantic 24px heading. The 14px **Review first** instruction
  sits immediately below it, above the whole workspace, rather than in the
  destination sidebar. Header height follows the wrapped instruction text.
- Home branding increases to 32px, recent-folder headings to 18px, the current
  culling folder to 16px, and shared dialog/confirmation titles to 20px.
- Source section labels increase to 14px, with 12px card/drive subheadings.
- The unused destination sidebar is removed during card review. Previews use
  the available width, with **Start Culling** in a bottom action bar. In the
  combined app, the video inclusion toggle remains in Settings.

These changes are independent of the optional design theme. Actions, photo
selection, keyboard shortcuts, and file operations retain their behavior.

Validation: frontend tests, TypeScript/Vite build, and browser runtime checks
of the ingest heading/instruction placement and shared dialog title sizes.
