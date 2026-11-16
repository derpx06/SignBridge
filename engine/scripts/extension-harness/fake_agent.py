"""A stand-in for the Voicio agent on D-Bus: drives the GNOME extension's capsule through its states."""
import gi, sys, math
gi.require_version('Gio', '2.0')
from gi.repository import Gio, GLib
XML = '''<node><interface name="io.github.voicio.Engine">
<method name="Start"/><method name="Stop"/><method name="Cancel"/><method name="OpenSettings"/><method name="Attach"/>
<method name="Attach2"><arg type="b" direction="out"/></method>
<method name="ShortcutBound"><arg type="b" direction="out"/></method><method name="Detach"/>
<method name="Status"><arg type="s" direction="out"/><arg type="d" direction="out"/><arg type="d" direction="out"/><arg type="s" direction="out"/></method>
<signal name="Preview"><arg type="s"/></signal><signal name="Done"><arg type="s"/></signal><signal name="StateChanged"><arg type="s"/></signal><signal name="Level"><arg type="d"/></signal><signal name="Error"><arg type="s"/></signal>
</interface></node>'''
loop = GLib.MainLoop(); conn = None
def call(inv, m):
    if m == 'Attach2': inv.return_value(GLib.Variant('(b)', (False,)))
    elif m == 'ShortcutBound': inv.return_value(GLib.Variant('(b)', (True,)))
    elif m == 'Status': inv.return_value(GLib.Variant('(sdds)', ('idle', 0.0, 0.0, 'x')))
    else: inv.return_value(None)
def emit(name, args=None, sig=''):
    conn.emit_signal(None, '/io/github/voicio/Engine', 'io.github.voicio.Engine', name, GLib.Variant('(' + sig + ')', args) if sig else None)
def shot(name): open('/tmp/vh-shot', 'w').write(name)
def levels(amp, jitter=0.6):
    n = [0]
    def f():
        n[0] += 1; emit('Level', (amp * (1 - jitter + jitter * abs(math.sin(n[0] * 0.7))),), 'd'); return True
    return GLib.timeout_add(40, f)
def script():
    ids = {}
    def at(t, fn): GLib.timeout_add(int(t * 1000), lambda: (fn(), False)[1])
    at(1, lambda: emit('StateChanged', ('listening',), 's'))
    at(1.2, lambda: ids.__setitem__('a', levels(0.0)))
    at(3, lambda: shot('1_listening_quiet'))
    at(3.5, lambda: (GLib.source_remove(ids['a']), ids.__setitem__('a', levels(0.9)), emit('Preview', ('Please send the report to Priya before noon',), 's')))
    at(6, lambda: shot('2_listening_voice'))
    at(6.5, lambda: (GLib.source_remove(ids['a']), emit('StateChanged', ('finishing',), 's')))
    at(7.5, lambda: shot('3_working'))
    at(8, lambda: (emit('Done', ('Please send the report to Priya before noon.',), 's'), emit('StateChanged', ('idle',), 's')))
    at(8.4, lambda: shot('4_done'))
    at(10, lambda: emit('Error', ('The microphone delivered no audio. Check the input device.',), 's'))
    at(10.8, lambda: shot('5_error'))
    at(12, loop.quit)
def acquired(c, name):
    global conn; conn = c
    c.register_object('/io/github/voicio/Engine', Gio.DBusNodeInfo.new_for_xml(XML).interfaces[0], lambda c, s, p, i, m, pa, inv: call(inv, m), None, None)
Gio.bus_own_name(Gio.BusType.SESSION, 'io.github.voicio.Engine', 0, acquired, lambda c, n: script(), None)
loop.run()
