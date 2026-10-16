# Settings layout guidelines

Use the same spacing rules in every theme; themes change colours, not the form's
geometry. All sizes below use rems so the saved interface scale applies.

- Dialog: cap width at 48rem, keep 1rem clear of each viewport edge, use 1rem
  outer padding on narrow windows and 1.5rem on desktop. Keep the heading and
  tabs visible; scroll the content area.
- Cards: give each independent option or related group one boundary with
  1.25rem padding. Leave 1rem between cards. Do not add a second padded card
  inside a card. Nested fieldsets retain their legend and radio-group semantics
  with zero padding, zero border and no separate background.
- Fieldset headings: use 0.75rem (12px at 100%) top padding before the first
  content row. A legend adds no bottom margin, and the first child adds no top
  margin. Do not stack legend spacing, card padding and paragraph margins into
  a large empty band, as happened in Graphics acceleration.
- Within a group: use 0.5rem between a group label and its controls, 0.75rem
  between related rows and 0.375rem between a setting title and its explanation.
  Avoid spacer elements, fixed row heights or margins added to both neighbours.
- Typography: section headings 1rem semibold, labels 0.9375rem medium,
  descriptions and controls 0.875rem with 1.5 line height. Use the shared
  foreground and muted-foreground tokens rather than opacity.
  Experimental options follow exactly this hierarchy. Use explicit label and
  hint classes: a generic nested-span rule must never turn a title into body
  text. Align the switch with the first title line, keep its width fixed, and
  wrap the description in the remaining column. Section headings stay on the
  card boundary with the same inset as every other section.
- Controls: align switches with the setting title and reserve their width.
  Panel titles reuse their toolbar icon (left panel, bottom filmstrip, right
  face panel and info). Icons supplement the label and are hidden from screen readers.
  Text inputs and selects have a 2.5rem minimum height. Radio options wrap as
  complete label/control pairs. Long explanations wrap within the card.
- Check the rendered result in all themes, on short and narrow windows and at
  larger interface scales. No horizontal clipping, compounded nested padding,
  lost focus indicators or inaccessible controls.

View mode demonstrates this structure: one outer card, a compact semantic group
for the three opening-view radios, then the Remember last view checkbox and its
description. The inner group adds no extra inset.

Keyboard shortcuts are edited directly in the Keyboard tab. Home and workspace
keyboard buttons open that same tab. Use responsive section cards, preserve
key-conflict handling, cancel capture with Escape, and stop capture when leaving
the tab or closing Settings. Do not open another modal for the editor.

Experimental sections always come before ordinary sections in their Settings tab.
Use the shared `experimental-section` class: red legend and border, with normal
foreground labels and muted descriptions. Keep the word “Experimental” visible
so colour is not the only status cue. The accent has separate light and dark
values; retain the same spacing and typography as ordinary sections. New
experimental options must also be included in settings import/export.

Appearance places Ranking colours beside Theme, wrapping to a full row when
space is limited. Keep the classic blue–grey–red palette as the default. The
optional red–green palette makes rank 1 green, intermediate frames yellow and
the lowest rank red. Preserve numeric ranks and problem labels. Pick/Reject
status uses the same green tick/red cross circles in grid, filmstrip and group
previews; reviewed-group ticks retain their distinct accessible label.

Appearance and home layout are independent. Wisp is a Theme alongside Dark, Light and 18% grey; do not add a separate interface-design selector. Home layout owns “Show a featured image on home page”. Keep Theme and Ranking colours on separate rows. Ranking choices show palette dots with accessible names and a radio selection indicator. Retain numeric ranks so colour is never the only cue. Legacy settings imports migrate without resetting other preferences.

Panel visibility switches belong in one shared section with compact divided rows. Keep their icons and labels; put toolbar/keyboard guidance once above the list rather than repeating it below every switch.
