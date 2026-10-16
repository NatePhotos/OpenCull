# Home layouts

The home page defaults to Spacious: identity above a row of actions, a 2rem
gap between those rows and a shared responsive gutter for the header and recent
items. Existing live recent-folder previews are preserved.

Settings → Design switches to Classic, placing actions at the upper right
beside the OpenCull heading when there is room. Actions wrap on smaller windows
rather than clipping. The `homeLayout` preference is stored through shared
preferences, independently of the design theme and colour mode. Invalid or
missing values use Spacious. macOS window-control clearance is preserved.

Home layout includes “Show a featured image on home page”, independently of
Appearance → Theme (Dark, Light, 18% grey or Wisp), font and scale. This retains
the former RapidRAW split homepage without an interface-design selector. Legacy
preferences migrate automatically; theme and home choices remain available while culling.
