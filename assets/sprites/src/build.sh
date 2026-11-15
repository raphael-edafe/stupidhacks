#!/bin/sh
# Rebuild the game sprites from the pxl sources and copy the PNGs to assets/sprites/.
# Needs the pxl skill (Node >= 22.18; set NODE=/path/to/node if the system one is older).
# Usage: assets/sprites/src/build.sh [path/to/pxl.js]
set -e
here="$(cd "$(dirname "$0")" && pwd)"
pxl="${1:-$HOME/.claude/skills/pxl-pixel-art/bin/pxl.js}"
node="${NODE:-node}"
out="$here/out"
for f in player items mobs terrain flora sky; do
  "$node" "$pxl" build "$here/$f.px.js" --out "$out" --no-preview
done
dst="$here/.."
cp "$out"/anims/*.png "$dst"/
cp "$out"/sprites/*.png "$dst"/
echo "sprites copied to $dst"
