# Persistent AI analysis

Analysis is stored as JSON under the native application cache directory's `faces` folder. On Linux this is normally `$XDG_CACHE_HOME/org.opencull.desktop/faces` (or `~/.cache/org.opencull.desktop/faces`). macOS and Windows keep their native Tauri cache location. Preferences remain in the configuration directory.

The cache checks each source preview's path and file version, and keeps Basic and Advanced AI separate. Model/rule versions select the appropriate cache. A changed photo or a different rule set can require analysis again; reopening unchanged photos does not.

Results are indexed by source directories as well as the session root, so card, subfolder and multi-card sessions can share analysis of the same files. Existing parent-root cache files are read too. Partial scans preserve completed records, atomic replacement protects files during saving, and a damaged individual entry no longer discards the whole shoot's valid records. Save failures are reported to the application log.

The toolbar distinguishes **Checking AI cache**, **Loading cached AI**, and **AI scanning**. Its status tooltip gives cache-hit and pending counts; the log records the same counts without photo paths. When every photo is cached, the backend restores results without starting inference workers. Reading EXIF or preparing previews can still happen separately.
