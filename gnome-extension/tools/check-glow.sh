#!/usr/bin/env bash
# Renders the glow and asserts its shape against the maths in glow.js.
# GNOME Shell cannot be restarted under Wayland, so this is the only check on
# the drawing that does not cost a logout.
#
#   gnome-extension/tools/check-glow.sh
#
# Needs gjs and ImageMagick. Sample points avoid x=640..960, which the pointer
# cut-out covers in the rendered frame.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
png="$(mktemp -t ringlight-glow-XXXXXX.png)"
trap 'rm -f "$png"' EXIT

gjs -m "$here/render-glow.js" "$png" >/dev/null

# Alpha at a pixel, 0.0-1.0.
alpha() {
    convert "$png" -crop "1x1+$1+$2" +repage -format "%[fx:a]" info:
}

fails=0
expect() { # name x y want tolerance
    local got
    got=$(alpha "$2" "$3")
    if awk -v g="$got" -v w="$4" -v t="$5" 'BEGIN{exit !((g-w<t)&&(w-g<t))}'; then
        printf 'ok   %-22s alpha=%.4f (want %.4f)\n' "$1" "$got" "$4"
    else
        printf 'FAIL %-22s alpha=%.4f (want %.4f +/- %.3f)\n' "$1" "$got" "$4" "$5"
        fails=$((fails + 1))
    fi
}

# Five stacked passes at brightness 1.0 composite to 1-prod(1-a_i).
expect "top edge"        200    0 0.998 0.01
expect "left edge"         0  500 0.998 0.01
expect "right edge"     1599  500 0.998 0.01
expect "bottom edge"     200  999 0.998 0.01
expect "corner"            0    0 0.999 0.01
# One base-width in from the top: pass 0 has fallen to zero, the four wider
# passes have not.
expect "falloff at width"  200 100 0.637 0.02
# Interior is untouched, and the pointer cut-out erases what it covers.
expect "interior"          200 500 0.000 0.005
expect "pointer cut-out"   800  60 0.000 0.005

if [ "$fails" -ne 0 ]; then
    echo "$fails check(s) failed"
    exit 1
fi
echo "all glow checks passed"
