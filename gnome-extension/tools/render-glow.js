#!/usr/bin/env -S gjs -m
// Renders glow.js to a PNG so the drawing can be checked without a live
// session. GNOME Shell cannot be restarted under Wayland, so this is the only
// way to see a change to the glow before logging out.
//
//   gjs -m gnome-extension/tools/render-glow.js /tmp/glow.png
//
// check-glow.sh samples the result and asserts the shape is right.
import Cairo from 'cairo';
import {drawGlow} from '../glow.js';

const [out = '/tmp/ringlight-glow.png'] = ARGV;
const WIDTH = 1600;
const HEIGHT = 1000;

const surface = new Cairo.ImageSurface(Cairo.Format.ARGB32, WIDTH, HEIGHT);
const cr = new Cairo.Context(surface);

drawGlow(cr, WIDTH, HEIGHT, {
    brightness: 1.0,
    colorTemp: 0.5,
    glowSize: 'medium',
});

cr.$dispose();
surface.flush();
surface.writeToPNG(out);
print(`wrote ${out} (${WIDTH}x${HEIGHT})`);
