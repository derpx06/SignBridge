"""Adds a TEST-ONLY screenshot hook (reads /tmp/vh-shot) to a copy of extension.js. usage: patch_hook.py FILE OUTDIR"""
import sys
p, out = sys.argv[1], sys.argv[2]
s = open(p).read()
hook = '''
        this._dbg = GLib.timeout_add(GLib.PRIORITY_DEFAULT, 200, () => {
            const f = Gio.File.new_for_path('/tmp/vh-shot');
            if (f.query_exists(null)) {
                const [, c] = f.load_contents(null);
                const name = new TextDecoder().decode(c).trim();
                f.delete(null);
                const stream = Gio.File.new_for_path('%s/' + name + '.png').replace(null, false, Gio.FileCreateFlags.NONE, null);
                new Shell.Screenshot().screenshot(false, stream, (o, res) => {
                    try { o.screenshot_finish(res); } catch (e) { console.error('shot failed ' + e); }
                    stream.close(null);
                });
            }
            return GLib.SOURCE_CONTINUE;
        });
''' % out
a = "        this._bound = false;\n    }\n\n    disable()"
assert a in s, 'anchor not found'
open(p, 'w').write(s.replace(a, "        this._bound = false;\n" + hook + "    }\n\n    disable()", 1))
