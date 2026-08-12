import GLib from 'gi://GLib';

const POLL_SECONDS = 2;

// A full /proc sweep measures ~21ms on a 512-process desktop. This code runs
// inside the compositor, so doing that in one go drops a frame every poll. The
// sweep is spread over idle callbacks instead, each one stopping after this
// long. A time budget rather than a process count because fd counts per process
// are wildly uneven — a browser with 500 open fds costs as much as 100 daemons.
const SLICE_US = 2000;

function videoDevices() {
    const found = [];
    for (let i = 0; i < 10; i++) {
        const path = `/dev/video${i}`;
        if (GLib.file_test(path, GLib.FileTest.EXISTS))
            found.push(path);
    }
    return found;
}

function processIds() {
    const pids = [];
    let dir;
    try {
        dir = GLib.Dir.open('/proc', 0);
    } catch (e) {
        return pids;
    }
    let name;
    while ((name = dir.read_name()) !== null) {
        if (name[0] >= '1' && name[0] <= '9')
            pids.push(name);
    }
    dir.close();
    return pids;
}

/** True if any fd of `pid` points at one of `devices`. Exported for check-camera.js. */
export function pidHoldsDevice(pid, devices) {
    const fdDir = `/proc/${pid}/fd`;
    let dir;
    try {
        dir = GLib.Dir.open(fdDir, 0);
    } catch (e) {
        return false; // not ours to read, or the process is already gone
    }
    let fd;
    while ((fd = dir.read_name()) !== null) {
        let target;
        try {
            target = GLib.file_read_link(`${fdDir}/${fd}`);
        } catch (e) {
            continue;
        }
        if (devices.includes(target)) {
            dir.close();
            return true;
        }
    }
    dir.close();
    return false;
}

/**
 * Polls for "is any process holding a webcam open" and calls `onChange(bool)`
 * whenever the answer flips.
 *
 * Reads /proc rather than Shell.CameraMonitor on purpose: that one is fed by
 * PipeWire and so misses the browsers and conferencing apps that still open
 * /dev/video* directly, which is most of them.
 */
export class CameraWatcher {
    /**
     * `devicesFn` is the seam check-camera.js uses to drive a full sweep on a
     * machine with no webcam plugged in; leave it out in the extension.
     */
    constructor(onChange, devicesFn = videoDevices) {
        this._onChange = onChange;
        this._devicesFn = devicesFn;
        this._active = false;
        this._pollId = 0;
        this._idleId = 0;
        this._holder = null;
    }

    get active() {
        return this._active;
    }

    start() {
        if (this._pollId)
            return;
        this._poll();
        this._pollId = GLib.timeout_add_seconds(GLib.PRIORITY_LOW, POLL_SECONDS, () => {
            this._poll();
            return GLib.SOURCE_CONTINUE;
        });
    }

    stop() {
        if (this._pollId)
            GLib.Source.remove(this._pollId);
        if (this._idleId)
            GLib.Source.remove(this._idleId);
        this._pollId = 0;
        this._idleId = 0;
        this._holder = null;
        this._active = false;
    }

    destroy() {
        this.stop();
        this._onChange = null;
    }

    _poll() {
        if (this._idleId)
            return; // previous sweep still running

        // Re-detected every poll so a camera plugged in later is picked up.
        const devices = this._devicesFn();
        if (devices.length === 0) {
            this._holder = null;
            this._report(false);
            return;
        }

        // While a call is running the answer is already known: check the one
        // process that had the camera before sweeping every process again.
        if (this._holder !== null) {
            if (pidHoldsDevice(this._holder, devices)) {
                this._report(true);
                return;
            }
            this._holder = null;
        }

        // Listing /proc is itself up to ~4ms with a cold dentry cache, so it
        // waits for the first idle slice rather than running on the timer.
        let pids = null;
        let i = 0;
        this._idleId = GLib.idle_add(GLib.PRIORITY_LOW, () => {
            const deadline = GLib.get_monotonic_time() + SLICE_US;
            if (pids === null) {
                pids = processIds();
                return GLib.SOURCE_CONTINUE;
            }
            while (i < pids.length) {
                const pid = pids[i++];
                if (pidHoldsDevice(pid, devices)) {
                    this._idleId = 0;
                    this._holder = pid;
                    this._report(true);
                    return GLib.SOURCE_REMOVE;
                }
                if (GLib.get_monotonic_time() >= deadline)
                    break;
            }
            if (i < pids.length)
                return GLib.SOURCE_CONTINUE;

            this._idleId = 0;
            this._report(false);
            return GLib.SOURCE_REMOVE;
        });
    }

    _report(active) {
        if (active === this._active)
            return;
        this._active = active;
        this._onChange?.(active);
    }
}
