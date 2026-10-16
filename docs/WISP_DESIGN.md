# Wisp design guidelines

Select Wisp under Settings → Design → Appearance → Theme. It uses contrasting
neutral panels, violet controls and an 18% grey photographic surround. Selecting
a theme leaves font, scale, home layout and culling preferences unchanged.

The featured home image is an independent checkbox under Home layout. The
[included Wisp preset](../src/lib/presets/wisp.json) remains available through
Settings → Storage import; it also includes Space Grotesk, 125% scale and saved
culling options, without source paths or history. Settings export preserves your
current choices. Older interface-design settings are migrated automatically.

- Use semantic foreground, muted-foreground, background, card, border and primary
  tokens. Never dim an entire readable row with opacity or hard-code grey text.
- Maintain at least 4.5:1 contrast for labels, values and help text against their
  actual surfaces, and 3:1 for meaningful icons and control boundaries.
- Group related metadata in bordered sections with a distinct surface. Use a
  visible divider beneath the section heading, with 1rem internal padding and
  1rem between sections. Avoid faint dividers on an undifferentiated grey panel.
- Metadata headings use 0.875rem semibold, labels 0.8125rem, and values 0.875rem,
  all with 1.5 line height. Use sentence case and brighter headings rather than
  tiny uppercase text. Keep values monospaced for numbers and filenames.
- Use primary-coloured icons as navigation cues alongside text. A label remains
  understandable without its icon or colour.
- Settings options use 1.25rem padding and visible card boundaries. Keep controls
  separate from help text, wrap long labels and values, and let content scroll
  without losing the dialog heading or tabs.
- Support interface scaling through relative units, not CSS transforms. Check
  long camera/lens names and file paths, narrow panels, keyboard focus and all
  three appearance modes. Respect forced colours and native font rendering.

The EXIF panel applies this hierarchy to Exposure, Equipment, Date & Time and
File Details. Other themes retain their existing metadata styling.
