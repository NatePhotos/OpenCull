# Default and remembered view modes

Branch: `feature/view-mode-preferences`, directly based on upstream `a8c7f32`.
No other feature is required.

**Settings → Culling → View mode** provides a default opening view:
Filmstrip (Loupe), Stack (Scene), or Grid. **Remember last view** is enabled by
default. With it enabled, toolbar, keyboard, and other view changes persist
across folders and restarts. With it disabled, each folder opens in the chosen
default, while the current folder can still be switched freely.

First use still starts in Loupe. Invalid saved modes fall back to the valid
default. Changing the default does not interrupt the current culling view.
Filmstrip panel visibility remains its existing separate setting.

The shared preferences API stores `defaultViewMode`, `rememberViewMode`, and
`viewMode`. Their mode values are `loupe`, `scene`, or `grid`. With XDG settings
installed they are written into the native JSON configuration automatically.
In the combined integration, these controls live under **Settings → Layout**.

Validation: 35 standalone frontend tests and TypeScript/Vite production build.
Browser runtime checks cover every toolbar mode across folder changes and
page reloads, configured defaults with remembering disabled, and keyboard
changes saved to preferences. No JavaScript errors were observed.
