import Adw from 'gi://Adw';
import Gio from 'gi://Gio';
import Gtk from 'gi://Gtk';

import {ExtensionPreferences} from 'resource:///org/gnome/Shell/Extensions/js/extensions/prefs.js';

const GLOW_SIZES = ['small', 'medium', 'large'];
const HOLE_SIZES = ['off', 'small', 'medium', 'large'];

function scaleRow(group, title, subtitle, settings, key) {
    const row = new Adw.ActionRow({title, subtitle});
    const scale = new Gtk.Scale({
        adjustment: new Gtk.Adjustment({lower: 0, upper: 1, step_increment: 0.05}),
        digits: 2,
        draw_value: true,
        hexpand: true,
        width_request: 220,
        valign: Gtk.Align.CENTER,
    });
    settings.bind(key, scale.adjustment, 'value', Gio.SettingsBindFlags.DEFAULT);
    row.add_suffix(scale);
    group.add(row);
}

function choiceRow(group, title, subtitle, labels, values, settings, key) {
    const row = new Adw.ComboRow({
        title,
        subtitle,
        model: Gtk.StringList.new(labels),
        selected: Math.max(0, values.indexOf(settings.get_string(key))),
    });
    row.connect('notify::selected', () => settings.set_string(key, values[row.selected]));
    group.add(row);
}

export default class RinglightPreferences extends ExtensionPreferences {
    fillPreferencesWindow(window) {
        const settings = this.getSettings();
        const page = new Adw.PreferencesPage();

        const light = new Adw.PreferencesGroup({title: 'Light'});
        light.add(this._switchRow('Follow camera',
            'Turn the glow on whenever an app opens the webcam',
            settings, 'auto-mode'));
        scaleRow(light, 'Brightness', null, settings, 'brightness');
        scaleRow(light, 'Colour', 'Warm amber at 0, cool white at 1',
            settings, 'color-temp');
        page.add(light);

        const shape = new Adw.PreferencesGroup({title: 'Shape'});
        choiceRow(shape, 'Glow size', 'How far the glow reaches in from the edge',
            ['Small', 'Medium', 'Large'], GLOW_SIZES, settings, 'glow-size');
        choiceRow(shape, 'Pointer cut-out', 'Clears the glow around the pointer',
            ['Off', 'Small', 'Medium', 'Large'], HOLE_SIZES, settings, 'hole-size');
        page.add(shape);

        window.add(page);
    }

    _switchRow(title, subtitle, settings, key) {
        const row = new Adw.SwitchRow({title, subtitle});
        settings.bind(key, row, 'active', Gio.SettingsBindFlags.DEFAULT);
        return row;
    }
}
