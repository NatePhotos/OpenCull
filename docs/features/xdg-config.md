# Native configuration

Linux settings are stored at `$XDG_CONFIG_HOME/opencull/config.json`, or `~/.config/opencull/config.json` when XDG_CONFIG_HOME is unset or not absolute. Other platforms use the native Tauri application config directory. The file is a JSON object of existing preference keys. Unknown keys survive writes. Restart after editing manually.

Startup migrates webview preferences only into missing keys, then imports the application so stores see file values immediately. Existing file values win. Writes are serialized and use a synced temporary file and rename; malformed or unreadable files are left untouched and reported visibly. Changes remain in webview storage if native initialization fails. `session.*` manifests remain separate from preferences.

Personal settings, when the corresponding independent features are installed:

```json
{"theme":"grey","font":{"choice":"space-grotesk","family":""},"ignoreVideos":true}
```

Run `npm test`, `npm run build`, and desktop startup tests with an XDG override, missing directory, malformed file, and an unwritable parent. This feature does not require appearance or font branches.
