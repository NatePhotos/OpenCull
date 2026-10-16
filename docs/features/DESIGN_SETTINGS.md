# Design settings

The combined app groups Appearance, Interface font and Interface scale under
Settings → Design. Wisp is an Appearance theme; Home layout independently controls
the featured image. Layout retains view modes, panels and filmstrip
arrangement. Legacy interface-design preferences migrate to theme and featured-image settings.

`feature/settings-design-tab` is based on `dev` because these controls originate
from several independently retained feature branches.

Settings typography uses a shared reading hierarchy across themes: 1rem section
headings, 0.9375rem labels, and 0.875rem help text and controls, with unitless 1.5
line height. Controls have a 2.5rem minimum height; rows wrap on narrow windows.
The dialog grows to 48rem on desktop, while keeping a 1rem viewport margin.
Options use 1.25rem padding and visible card boundaries. The heading and tabs
stay visible while the content scrolls. Compact photo
overlays retain their own type scale.

Leave native font rasterization enabled. Do not force grayscale font smoothing
or disable antialiasing to simulate hinting: font outlines, the operating system
and the webview determine hinting. Use the Interface font and Interface scale
preferences when a display or installed font needs a different reading size.
