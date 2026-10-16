# Settings import and export

Settings → Storage → Settings backup exports a versioned `opencull-settings.json`
file. The desktop uses a native Save dialog; the browser demo downloads the file.
Preferences, themes, home layout, panel sizes, experimental options and keyboard
shortcuts are portable. Recent folders, session history, source paths, caches and
photos are excluded.

Import accepts version 1 OpenCull settings exports up to 1 MB. Known preference
names, types, enums, dimensions, fonts and shortcut bindings are validated before
any write. Unknown keys, malformed JSON and invalid values reject the entire
import. The UI shows a review and Cancel/Apply choice. Apply merges matching
preferences, keeps unspecified preferences and reloads the app.

On desktop, preferences are merged and written atomically under the existing
settings lock. A failed native write leaves the old file intact. The browser
cache is updated after native persistence succeeds. Settings file errors remain
visible; an unavailable native configuration must be repaired before import.
Export cannot overwrite the active configuration file.

Imports are JSON data only. They do not install fonts, execute commands, copy
photos or include CUDA/graphics launch environment settings. A custom font falls
back normally if it is not installed on the receiving computer.
