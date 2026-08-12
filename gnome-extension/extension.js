import GLib from 'gi://GLib';
import GObject from 'gi://GObject';
import St from 'gi://St';

import {Extension} from 'resource:///org/gnome/shell/extensions/extension.js';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import * as PanelMenu from 'resource:///org/gnome/shell/ui/panelMenu.js';
import * as PopupMenu from 'resource:///org/gnome/shell/ui/popupMenu.js';
import {Slider} from 'resource:///org/gnome/shell/ui/slider.js';

import {CameraWatcher} from './camera.js';
import {drawGlow} from './glow.js';

const CURSOR_POLL_MS = 50;
const CURSOR_EPSILON = 2;

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
        this._mouse = [null, null];

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
        this._camera.start();

        this._settingsId = this._settings.connect('changed', () => this._apply());
        // Switching auto-mode on has to pick up a camera that is already in
        // use, or the glow waits for the next flip — which on first run means
        // it looks broken.
        this._autoId = this._settings.connect('changed::auto-mode', () => {
            if (this._settings.get_boolean('auto-mode'))
                this._settings.set_boolean('enabled', this._camera.active);
        });
        this._monitorsId = Main.layoutManager.connect('monitors-changed',
            () => this._resize());

        this._resize();
        this._apply();
    }

    disable() {
        if (this._cursorId)
            GLib.Source.remove(this._cursorId);
        this._cursorId = 0;

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
        this._monitor = monitor;
        this._area.set_position(monitor.x, monitor.y);
        this._area.set_size(monitor.width, monitor.height);
        this._area.queue_repaint();
    }

    _apply() {
        const on = this._settings.get_boolean('enabled');
        this._area.visible = on;

        const wantsCursor = on && this._settings.get_string('hole-size') !== 'off';
        if (wantsCursor && !this._cursorId) {
            this._cursorId = GLib.timeout_add(GLib.PRIORITY_DEFAULT, CURSOR_POLL_MS,
                () => this._trackCursor());
        } else if (!wantsCursor && this._cursorId) {
            GLib.Source.remove(this._cursorId);
            this._cursorId = 0;
            this._mouse = [null, null];
        }

        this._area.queue_repaint();
    }

    _trackCursor() {
        if (!this._monitor)
            return GLib.SOURCE_CONTINUE;
        const [x, y] = global.get_pointer();
        const mx = x - this._monitor.x;
        const my = y - this._monitor.y;
        const [px, py] = this._mouse;
        if (px === null ||
            Math.abs(px - mx) > CURSOR_EPSILON || Math.abs(py - my) > CURSOR_EPSILON) {
            this._mouse = [mx, my];
            this._area.queue_repaint();
        }
        return GLib.SOURCE_CONTINUE;
    }

    _repaint() {
        const cr = this._area.get_context();
        const [width, height] = this._area.get_surface_size();
        try {
            drawGlow(cr, width, height, {
                brightness: this._settings.get_double('brightness'),
                colorTemp: this._settings.get_double('color-temp'),
                glowSize: this._settings.get_string('glow-size'),
                holeSize: this._settings.get_string('hole-size'),
                mouseX: this._mouse[0],
                mouseY: this._mouse[1],
            });
        } finally {
            cr.$dispose();
        }
    }
}
