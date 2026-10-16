# Interface scale

Saved desktop preference under Settings → Culling in the standalone feature, and Design in the combined app. Uses Tauri Webview.setZoom so pixels, rems, menus, controls, and pointer coordinate systems scale together. Supports 80–200%, default 100%, plus Reset. Uses shared preferences. Browser demos retain browser zoom controls rather than simulating page zoom with CSS transforms.

macOS requires 11+ (Tauri API requirement). Linux and Windows use their native webview zoom implementation. A narrowly scoped core:webview:allow-set-webview-zoom permission is added. Errors remain visible and unsuccessful changes are not persisted.

Settings stay within the viewport, tabs wrap, settings content scrolls on short windows, and the culling toolbar wraps rather than clipping controls as usable CSS space decreases.
