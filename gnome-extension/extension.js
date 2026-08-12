import GObject from 'gi://GObject';
import St from 'gi://St';

import {Extension} from 'resource:///org/gnome/shell/extensions/extension.js';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import * as PanelMenu from 'resource:///org/gnome/shell/ui/panelMenu.js';
import * as PopupMenu from 'resource:///org/gnome/shell/ui/popupMenu.js';
import {Slider} from 'resource:///org/gnome/shell/ui/slider.js';

import {CameraWatcher} from './camera.js';
import {drawGlow} from './glow.js';

// The glow is painted onto a surface this many pixels on its long side and the
// actor is scaled up to fill the monitor. Cairo is a software rasteriser and
// this runs inside the compositor: a full 2560x1600 repaint measures 67ms, four
// frame budgets. At 512 it is under a millisecond, and the scale-up costs at
// most 0.016 of alpha against a full-size render, because every gradient here
// is far smoother than the pixel grid.
const RENDER_MAX = 512;

const Indicator = GObject.registerClass(
class RinglightIndicator extends PanelMenu.Button {
    _init(settings, openPrefs) {
        super._init(0.5, 'Ringlight');
        this._settings = settings;

        this.add_child(new St.Icon({
            icon_name: 'display-brightness-symbolic',
            style_class: 'system-status-icon',
        }));

        this._onOff = new PopupMenu.PopupSwitchMenuItem(
            'Ring Light', settings.get_boolean('enabled'));
        this._onOff.connect('toggled', (_i, on) => settings.set_boolean('enabled', on));
        this.menu.addMenuItem(this._onOff);

        this._auto = new PopupMenu.PopupSwitchMenuItem(
            'Follow camera', settings.get_boolean('auto-mode'));
        this._auto.connect('toggled', (_i, on) => settings.set_boolean('auto-mode', on));
        this.menu.addMenuItem(this._auto);

        this.menu.addMenuItem(new PopupMenu.PopupSeparatorMenuItem());

        const row = new PopupMenu.PopupBaseMenuItem({activate: false});
        row.add_child(new St.Icon({
            icon_name: 'display-brightness-symbolic',
            style_class: 'popup-menu-icon',
        }));
        this._slider = new Slider(settings.get_double('brightness'));
        this._slider.connect('notify::value',
            () => settings.set_double('brightness', this._slider.value));
        row.add_child(this._slider);
        this.menu.addMenuItem(row);

        this.menu.addMenuItem(new PopupMenu.PopupSeparatorMenuItem());
        const prefs = new PopupMenu.PopupMenuItem('Settings…');
        prefs.connect('activate', () => openPrefs());
        this.menu.addMenuItem(prefs);

        this._changedId = settings.connect('changed', () => this._sync());
    }

    _sync() {
        this._onOff.setToggleState(this._settings.get_boolean('enabled'));
        this._auto.setToggleState(this._settings.get_boolean('auto-mode'));
        const brightness = this._settings.get_double('brightness');
        if (Math.abs(this._slider.value - brightness) > 0.001)
            this._slider.value = brightness;
    }

    destroy() {
        if (this._changedId)
            this._settings.disconnect(this._changedId);
        this._changedId = 0;
        super.destroy();
    }
});

export default class RinglightExtension extends Extension {
    enable() {
        this._settings = this.getSettings();

        // A layer-shell surface is not an option here: mutter does not
        // implement zwlr_layer_shell_v1 and a plain toplevel cannot be
        // click-through, always-on-top and screen-sized under Wayland. Chrome
        // owned by the shell itself is the only thing that can.
        this._area = new St.DrawingArea({reactive: false});
        this._repaintId = this._area.connect('repaint', () => this._repaint());
        Main.layoutManager.addTopChrome(this._area, {
            affectsInputRegion: false,
            affectsStruts: false,
            trackFullscreen: false, // a video call in fullscreen still wants light
        });

        this._indicator = new Indicator(this._settings, () => this.openPreferences());
        Main.panel.addToStatusArea(this.uuid, this._indicator);

        this._camera = new CameraWatcher(active => {
            if (this._settings.get_boolean('auto-mode'))
                this._settings.set_boolean('enabled', active);
        });
        if (this._settings.get_boolean('auto-mode'))
            this._camera.start();

        this._settingsId = this._settings.connect('changed', () => this._apply());
        // Switching auto-mode on has to pick up a camera that is already in
        // use, or the glow waits for the next flip — which on first run means
        // it looks broken. Switching it off stops the /proc sweeps entirely:
        // nothing is watching the answer.
        this._autoId = this._settings.connect('changed::auto-mode', () => {
            if (this._settings.get_boolean('auto-mode')) {
                this._camera.start();
                this._settings.set_boolean('enabled', this._camera.active);
            } else {
                this._camera.stop();
            }
        });
        this._monitorsId = Main.layoutManager.connect('monitors-changed',
            () => this._resize());

        this._resize();
        this._apply();
    }

    disable() {
        // Signals first: their handlers reach for the camera watcher and the
        // actor, both of which are about to stop existing.
        if (this._settingsId)
            this._settings.disconnect(this._settingsId);
        if (this._autoId)
            this._settings.disconnect(this._autoId);
        this._settingsId = 0;
        this._autoId = 0;

        if (this._monitorsId)
            Main.layoutManager.disconnect(this._monitorsId);
        this._monitorsId = 0;

        this._camera?.destroy();
        this._camera = null;

        this._indicator?.destroy();
        this._indicator = null;

        if (this._area) {
            this._area.disconnect(this._repaintId);
            Main.layoutManager.removeChrome(this._area);
            this._area.destroy();
            this._area = null;
        }

        this._settings = null;
    }

    // ponytail: primary monitor only. A ring light is for the screen your face
    // is pointed at; add a per-monitor actor list if someone actually asks.
    _resize() {
        const monitor = Main.layoutManager.primaryMonitor;
        if (!monitor || !this._area)
            return;

        // Paint small, let the GPU stretch it. See RENDER_MAX.
        const k = Math.min(1, RENDER_MAX / Math.max(monitor.width, monitor.height));
        const w = Math.max(2, Math.round(monitor.width * k));
        const h = Math.max(2, Math.round(monitor.height * k));

        this._area.set_position(monitor.x, monitor.y);
        this._area.set_size(w, h);
        this._area.set_scale(monitor.width / w, monitor.height / h);
        this._area.queue_repaint();
    }

    _apply() {
        this._area.visible = this._settings.get_boolean('enabled');
        this._area.queue_repaint();
    }

    // Only ever runs on a settings change, a monitor change, or the first show.
    // Nothing in the glow follows the pointer or a clock, so there is no
    // repaint to schedule and no timer to leave running.
    _repaint() {
        const cr = this._area.get_context();
        const [width, height] = this._area.get_surface_size();
        try {
            if (width < 1 || height < 1)
                return;
            drawGlow(cr, width, height, {
                brightness: this._settings.get_double('brightness'),
                colorTemp: this._settings.get_double('color-temp'),
                glowSize: this._settings.get_string('glow-size'),
            });
        } finally {
            cr.$dispose();
        }
    }
}
