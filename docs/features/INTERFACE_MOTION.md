# Experimental interface transitions

Settings → Design → Interface transitions is off by default and is included in settings exports. It gently fades photo changes and crop entry/exit, fades face spotlights, and animates panel and filmstrip opening/closing. It respects the system's reduced-motion preference. Resizing panels remains immediate.

Photo transitions animate the existing view's opacity from 82% to 100%; they do not add a second photo renderer, fade to blank, or restart on zoom. This keeps the Linux WebKit compositor workaround intact. Face spotlight transitions use one SVG mask over the existing photo.
