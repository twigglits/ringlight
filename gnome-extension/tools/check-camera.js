#!/usr/bin/env -S gjs -m
// Exercises the /proc sweep in camera.js outside the shell, where a mistake in
// it would only show up as a desktop that stutters or a glow that never fires.
//
//   gjs -m gnome-extension/tools/check-camera.js
import GLib from 'gi://GLib';
import Gio from 'gi://Gio';
import System from 'system';

import {CameraWatcher, pidHoldsDevice} from '../camera.js';

let fails = 0;
function check(name, got, want) {
    if (got === want) {
        print(`ok   ${name}`);
    } else {
        printerr(`FAIL ${name}: got ${got}, want ${want}`);
        fails++;
    }
}

// A device this process demonstrably holds open, standing in for a webcam.
const held = Gio.File.new_for_path('/dev/null').read(null);
const self = GLib.file_read_link('/proc/self');

check('finds a device this process holds', pidHoldsDevice(self, ['/dev/null']), true);
check('ignores a device nothing holds', pidHoldsDevice(self, ['/dev/ringlight-nope']), false);
check('survives a pid that is gone', pidHoldsDevice('999999999', ['/dev/null']), false);

const loop = new GLib.MainLoop(null, false);

// Sweep that hits: /dev/null is open in this process, so the scan must
// short-circuit and report a device in use.
const hit = new CameraWatcher(() => {}, () => ['/dev/null']);
hit.start();

// Sweep that misses: /dev/zero exists so the watcher does not bail early, and
// nothing holds it open, so every process gets walked. This is the only path
// that runs the sliced loop to completion. A high-priority 1ms timer stands in
// for the compositor's frame clock: the longest gap between its ticks is how
// long the sweep locked the main loop out, which is the whole reason for
// slicing.
const miss = new CameraWatcher(() => {}, () => ['/dev/zero']);
let worstGap = 0;
let lastTick = 0;
GLib.timeout_add(GLib.PRIORITY_HIGH, 1, () => {
    const now = GLib.get_monotonic_time();
    if (lastTick)
        worstGap = Math.max(worstGap, (now - lastTick) / 1000);
    lastTick = now;
    return GLib.SOURCE_CONTINUE;
});
miss.start();

GLib.timeout_add_seconds(GLib.PRIORITY_DEFAULT, 5, () => {
    check('sweep finds a held device', hit.active, true);
    check('full sweep finds nothing holding /dev/zero', miss.active, false);
    // Slices run to 2ms. The bound here is deliberately looser: /proc reads and
    // SpiderMonkey collections put the occasional ~9ms outlier in a slice, and
    // no amount of slicing removes those. What this asserts is that the sweep
    // is sliced at all — unsliced it blocks for ~21ms every single poll.
    if (worstGap > 18) {
        printerr(`FAIL sweep locked the main loop out for ${worstGap.toFixed(1)}ms (limit 18ms)`);
        fails++;
    } else {
        print(`ok   full sweep stays sliced (worst gap ${worstGap.toFixed(1)}ms, unsliced is ~21ms)`);
    }
    hit.destroy();
    miss.destroy();
    loop.quit();
    return GLib.SOURCE_REMOVE;
});

loop.run();
held.close(null);

if (fails > 0) {
    printerr(`${fails} check(s) failed`);
    System.exit(1);
}
print('all camera checks passed');
