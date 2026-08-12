import Gio from 'gi://Gio';

const DBUS_IFACE = `
<node>
  <interface name="com.github.ringlight.Cursor">
    <method name="GetPosition">
      <arg type="i" direction="out" name="x"/>
      <arg type="i" direction="out" name="y"/>
    </method>
  </interface>
</node>`;

export default class RinglightCursorExtension {
    _dbus = null;
    _ownerId = 0;

    enable() {
        this._dbus = Gio.DBusExportedObject.wrapJSObject(DBUS_IFACE, this);
        this._dbus.export(Gio.DBus.session, '/com/github/ringlight/Cursor');
        this._ownerId = Gio.bus_own_name(
            Gio.BusType.SESSION,
            'com.github.ringlight.Cursor',
            Gio.BusNameOwnerFlags.NONE,
            null, null, null,
        );
    }

    disable() {
        if (this._dbus) {
            this._dbus.unexport();
            this._dbus = null;
        }
        if (this._ownerId) {
            Gio.bus_unown_name(this._ownerId);
            this._ownerId = 0;
        }
    }

    GetPosition() {
        let [x, y] = global.get_pointer();
        return [x, y];
    }
}
