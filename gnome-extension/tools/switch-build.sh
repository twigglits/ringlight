#!/usr/bin/env bash
# Swap the installed GNOME extension between the published EGO v3 (which draws
# nothing — a D-Bus shim, useful only as a performance floor) and the repo's
# current build. GNOME Shell cannot reload an extension under Wayland, so each
# swap needs a log out and back in to take effect.
#
#   tools/switch-build.sh v3      # published baseline, no glow, no panel icon
#   tools/switch-build.sh current # this repo
#   tools/switch-build.sh status
set -euo pipefail

EXT="$HOME/.local/share/gnome-shell/extensions/ringlight-cursor@ringlight"
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo="$(dirname "$here")"
baseline="$here/ego-v3"

case "${1:-status}" in
v3)
    [ -f "$baseline/extension.js" ] || { echo "missing $baseline"; exit 1; }
    rm -f "$EXT"/*.js
    cp "$baseline/extension.js" "$baseline/metadata.json" "$EXT/"
    echo "installed: EGO v3 baseline (draws nothing). Log out and back in."
    ;;
current)
    rm -f "$EXT"/*.js
    cp "$repo"/*.js "$repo/metadata.json" "$EXT/"
    mkdir -p "$EXT/schemas"
    cp "$repo"/schemas/*.gschema.xml "$EXT/schemas/"
    glib-compile-schemas "$EXT/schemas/"
    # EGO serves 4; a lower number here gets silently reinstalled over at login.
    sed -i 's/"version": *[0-9]*/"version": 10/' "$EXT/metadata.json"
    echo "installed: current repo build. Log out and back in."
    ;;
status)
    if [ -f "$EXT/glow.js" ]; then
        echo "installed: current repo build ($(wc -l < "$EXT/extension.js") lines)"
    else
        echo "installed: EGO v3 baseline ($(wc -l < "$EXT/extension.js") lines, draws nothing)"
    fi
    grep -o '"version": *[0-9]*' "$EXT/metadata.json"
    ;;
*)
    echo "usage: $0 {v3|current|status}"; exit 1
    ;;
esac
