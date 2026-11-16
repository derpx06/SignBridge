// Voicio Dictation: a floating pill at the bottom of the desktop shows what Voicio is doing, and the
// text is typed into the focused field through GNOME's own input method (the path the on-screen
// keyboard uses). All speech processing happens in the local voicio daemon over D-Bus; nothing
// here touches the network.

import Cairo from 'cairo';
import Clutter from 'gi://Clutter';
import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import GObject from 'gi://GObject';
import Graphene from 'gi://Graphene';
import Meta from 'gi://Meta';
import Shell from 'gi://Shell';
import St from 'gi://St';

import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import {Extension} from 'resource:///org/gnome/shell/extensions/extension.js';

const BUS_NAME = 'io.github.voicio.Engine';
const OBJECT_PATH = '/io/github/voicio/Engine';
const HOLD_MS = 400; // held longer than this = push-to-talk; shorter = tap to toggle
const MESSAGE_MS = 3500;

const EngineProxy = Gio.DBusProxy.makeProxyWrapper(`
<node>
  <interface name="io.github.voicio.Engine">
    <method name="Start"/>
    <method name="Stop"/>
    <method name="Cancel"/>
    <method name="OpenSettings"/>
    <method name="Attach"/>
    <method name="Attach2"><arg type="b" direction="out"/></method>
    <method name="ShortcutBound"><arg type="b" direction="out"/></method>
    <method name="Detach"/>
    <method name="Status">
      <arg type="s" direction="out"/><arg type="d" direction="out"/>
      <arg type="d" direction="out"/><arg type="s" direction="out"/>
    </method>
    <signal name="Preview"><arg type="s"/></signal>
    <signal name="Done"><arg type="s"/></signal>
    <signal name="StateChanged"><arg type="s"/></signal>
    <signal name="Level"><arg type="d"/></signal>
    <signal name="Error"><arg type="s"/></signal>
  </interface>
</node>`);

// The floating capsule (Capsule.dc.html) with honeycomb v2 (Honeycomb2.dc.html): an ink capsule,
// 40 px tall, with 37 soft pointy-top hexagons in rings of 1, 6, 12 and 18 (each cell bevelled from
// the top-left, an orange core) and the live words beside it. The same look as the app's own
// overlay: the voice lifts the rings from the centre out (60 ms apart, easing in ~200 ms), room
// noise turns the rim slate, working is an amber sweep on the rim, typed is a lime tick, a problem is
// calm coral. Nothing flashes or shakes. Never reactive and added without grabbing input, so it
// cannot steal focus from the text field being dictated into.
const COMB = 32;                    // the comb's box in px
const S = 92 / 11.6 * 0.32;         // the design's spacing unit at this size
const HEX = 0.9;                    // cell radius in units of S (0.82 in the spec; fuller at 32 px)
const TICK_MS = 40;
const TICK = new Set(['-1,-1', '-1,0', '-1,1', '0,0', '1,-1', '2,-2', '3,-3']);
const DONE_MS = 1200; // the tick holds, then the capsule slips away
// the small-size variant of Honeycomb2 (a 32 px comb): brighter rest, a clear orange core, inner
// rings that swell and warm with the voice, a gentle bevel (the same numbers as the app's painter)
const REST = [1, 0.92, 0.78, 0.62];
const VOICE = [1, 1, 0.9, 0.7];
const REST_K = [1.15, 0.96, 0.92, 0.9];
const VOICE_K = [1.22, 1.02, 0.96, 0.92];
const rgb = h => [(h >> 16 & 255) / 255, (h >> 8 & 255) / 255, (h & 255) / 255];
const CREAM_BEVEL = [rgb(0xFFFFFF), rgb(0xF1EADB), rgb(0xCDBFA7)];
const CORE = [rgb(0xFFC9AA), rgb(0xFF7A45), rgb(0xB8380B)];
const mixc = (a, b, t) => a.map((v, i) => v + (b[i] - v) * t);
const bevel = c => [mixc(c, [1, 1, 1], 0.4), c, mixc(c, [0, 0, 0], 0.25)];
const flat = st => [mixc(st[1], st[0], 0.5), st[1], mixc(st[1], st[2], 0.45)];
const CREAM = flat(CREAM_BEVEL);
const AMBER = bevel(rgb(0xFFC24B)), LIME = bevel(rgb(0xB8F06A)), CORAL = bevel(rgb(0xFF6B5B)), SLATE = bevel(rgb(0x9FB0C2));

// A rounded pointy-top hexagon of radius R at (x, y), corners cut back by 28 % of R.
const hexPath = (cr, x, y, R) => {
    const pts = [];
    for (let i = 0; i < 6; i++) {
        const a = Math.PI / 180 * (60 * i - 90);
        pts.push([x + R * Math.cos(a), y + R * Math.sin(a)]);
    }
    for (let i = 0; i < 6; i++) {
        const p = pts[i], prev = pts[(i + 5) % 6], next = pts[(i + 1) % 6];
        const a = [p[0] + (prev[0] - p[0]) * 0.28, p[1] + (prev[1] - p[1]) * 0.28];
        const b = [p[0] + (next[0] - p[0]) * 0.28, p[1] + (next[1] - p[1]) * 0.28];
        if (i === 0) cr.moveTo(a[0], a[1]); else cr.lineTo(a[0], a[1]);
        // a quadratic corner as a cubic
        cr.curveTo(a[0] + (p[0] - a[0]) * 2 / 3, a[1] + (p[1] - a[1]) * 2 / 3, b[0] + (p[0] - b[0]) * 2 / 3, b[1] + (p[1] - b[1]) * 2 / 3, b[0], b[1]);
    }
    cr.closePath();
};

const Pill = GObject.registerClass(
class VoicioPill extends St.BoxLayout {
    _init() {
        super._init({style_class: 'voicio-pill', reactive: false, can_focus: false,
            track_hover: false, opacity: 0, visible: false});
        this._lattice = new St.DrawingArea({style_class: 'voicio-lattice', width: COMB, height: COMB, y_align: Clutter.ActorAlign.CENTER});
        this._lattice.connect('repaint', area => this._draw(area));
        this._cells = [];
        for (let r = -3; r <= 3; r++) {
            for (let q = -3; q <= 3; q++) {
                const ring = Math.max(Math.abs(q), Math.abs(r), Math.abs(q + r));
                if (ring > 3)
                    continue;
                const x = S * Math.sqrt(3) * (q + r / 2), y = S * 1.5 * r;
                this._cells.push({x, y, ring, angle: Math.atan2(y, x), tick: TICK.has(`${q},${r}`)});
            }
        }
        this._rings = [0, 0, 0, 0];
        this._history = new Array(8).fill(0);
        this.add_child(this._lattice);
        this._label = new St.Label({style_class: 'voicio-text', y_align: Clutter.ActorAlign.CENTER});
        this._label.clutter_text.set_ellipsize(1 /* Pango.EllipsizeMode.START: keep the newest words */);
        this.add_child(this._label);
        // An action is only ever a hint: the capsule never takes focus or clicks.
        this._hint = new St.Label({style_class: 'voicio-hint', y_align: Clutter.ActorAlign.CENTER, visible: false});
        this.add_child(this._hint);
        this._target = 0;
        this._live = 0;
        this._room = 0;   // the room's loudness, the floor below which nothing counts as voice
        this._noise = 0;  // the same, smoothed, for the grey rim
        this._done = 0;
        this._mode = 'listening';
        this._phase = 0;
        this._timer = 0;
        this._hideTimer = 0;
        this._paint();
    }

    // Bottom-centre of the monitor the user is working on.
    _place() {
        const monitor = Main.layoutManager.monitors[global.display.get_current_monitor()] ??
            Main.layoutManager.primaryMonitor;
        const [, width] = this.get_preferred_width(-1);
        const [, height] = this.get_preferred_height(width);
        this.set_position(Math.round(monitor.x + (monitor.width - width) / 2),
            Math.round(monitor.y + monitor.height - height - Math.max(64, monitor.height * 0.07)));
    }

    _setHint(text) {
        this._hint.text = text;
        this._hint.visible = !!text;
    }

    show_(mode, text, hint = '') {
        this._cancelHide();
        for (const c of ['listening', 'finishing', 'error', 'done', 'warn'])
            this.remove_style_pseudo_class(c);
        this.add_style_pseudo_class(mode);
        this._mode = mode;
        this._label.text = text;
        this._label.visible = !!text;
        this._setHint(hint);
        // a message reads from its start; live words keep their newest end
        this._label.clutter_text.set_ellipsize(mode === 'error' || mode === 'warn' ? 3 : 1);
        this._startTimer();
        if (!this.visible) {
            this.visible = true;
            this.opacity = 0;
        }
        this._place();
        this.remove_all_transitions();
        if (this.opacity < 255) {
            // eases up on key-down: a gentle 200 ms arrival, no bounce
            this.set_pivot_point(0.5, 0.5);
            this.set_scale(0.88, 0.88);
        }
        this.ease({opacity: 255, scale_x: 1, scale_y: 1, duration: 200, mode: Clutter.AnimationMode.EASE_OUT_QUAD});
    }

    setText(text) {
        this._label.text = text;
        this._label.visible = !!text;
        this._place();
    }

    // Loudness 0..1. The tick below turns it into the core's swell.
    pushLevel(level) {
        this._target = Math.sqrt(Math.max(0, Math.min(1, level))); // sqrt: quiet speech still moves
    }

    _startTimer() {
        if (this._timer)
            return;
        this._timer = GLib.timeout_add(GLib.PRIORITY_DEFAULT, TICK_MS, () => {
            this._phase += TICK_MS / 1000;
            // The room: drops to any quieter moment at once, creeps up slowly, so speech never becomes it.
            const raw = this._target;
            this._room = raw < this._room ? this._room + (raw - this._room) * 0.25 : this._room + (raw - this._room) * 0.006;
            // Only what rises clearly above the room counts as voice, rescaled to 0..1.
            const voice = this._mode === 'listening'
                ? Math.pow(Math.max(0, Math.min(1, (raw - this._room - 0.07) / Math.max(0.3, 0.75 - this._room))), 1.15) : 0;
            // quick to rise, slow to settle
            this._live = voice > this._live ? this._live + (voice - this._live) * 0.7 : this._live * 0.84 + voice * 0.16;
            this._noise += ((this._mode === 'listening' ? this._room : 0) - this._noise) * 0.15;
            this._done += ((this._mode === 'done' ? 1 : 0) - this._done) * 0.3;
            // ring k follows the voice k x 60 ms later and eases there in about 200 ms
            this._history.unshift(this._mode === 'listening' ? Math.min(1, this._live * 1.6) : 0);
            this._history.length = 8;
            for (let k = 0; k < 4; k++)
                this._rings[k] += (this._history[Math.round(k * 1.5)] - this._rings[k]) * 0.45;
            if (this._mode !== 'listening')
                this._target = 0;
            // The ring: orange with the voice while listening, lime once typed. St takes one shadow,
            // so the drop shadow turns into the ring as the voice rises.
            const g = this._mode === 'listening' ? this._live : 0;
            const ring = this._mode === 'done'
                ? 'box-shadow: 0 0 6px 1px rgba(184, 240, 106, 0.22);'
                : g > 0.05
                    ? `box-shadow: 0 0 ${(6 * g).toFixed(1)}px ${(2 * g).toFixed(1)}px rgba(242, 84, 27, 0.14);`
                    : '';
            this.set_style(ring);
            this._paint();
            return GLib.SOURCE_CONTINUE;
        });
    }

    _stopTimer() {
        if (this._timer)
            GLib.source_remove(this._timer);
        this._timer = 0;
    }

    _paint() {
        this._lattice.queue_repaint();
    }

    // The look of one cell (scale, opacity, bevel stops), as in Honeycomb2.dc.html.
    _look(cell) {
        const g = cell.ring, t = this._phase;
        switch (this._mode) {
        case 'finishing': {
            const w = (Math.cos(cell.angle + 0.9 - t * 4) + 1) / 2;
            if (g === 0) return [0.92, 0.9, AMBER];
            return g === 3 ? [0.84 + 0.16 * w, 0.15 + 0.85 * w, AMBER] : [0.9, 0.45, AMBER];
        }
        case 'done': {
            const k = Math.min(1, this._done * 1.4);
            return cell.tick ? [0.88 + 0.16 * k, 0.28 + 0.72 * k, LIME] : [0.88, 0.28, CREAM];
        }
        case 'error':
            return [0.92, g >= 2 ? 0.6 : 0.95, CORAL];
        case 'warn':
            return [0.9, 0.6, AMBER];
        default: {
            const v = this._rings[g];
            const breath = 0.5 + 0.5 * Math.sin(2 * Math.PI * t / 2.4 - g * 0.5);
            let k = (REST_K[g] + (VOICE_K[g] - REST_K[g]) * v) * (0.985 + 0.03 * breath * (1 - v));
            let a = REST[g] + (VOICE[g] - REST[g]) * v;
            let stops = g === 0 ? CORE : g === 1 ? CREAM.map((c, i) => mixc(c, CORE[i], 0.3 + 0.55 * v)) : CREAM;
            if (g === 3) {
                // room noise: the rim turns slate, heard and ignored
                const hush = Math.max(0, Math.min(1, (this._noise - 0.2) / 0.4)) * Math.pow(1 - Math.max(v, this._rings[0]), 2);
                k += (0.92 - k) * hush;
                a += (0.8 - a) * hush;
                stops = stops.map((c, i) => mixc(c, SLATE[i], hush));
            }
            return [k, a, stops];
        }
        }
    }

    _draw(area) {
        const cr = area.get_context();
        const [w, h] = area.get_surface_size();
        cr.translate(w / 2, h / 2);
        for (const cell of this._cells) {
            const [k, alpha, stops] = this._look(cell);
            const R = S * HEX * k;
            const grad = new Cairo.LinearGradient(cell.x - R * 0.87, cell.y - R, cell.x + R * 0.87, cell.y + R);
            grad.addColorStopRGBA(0, ...stops[0], alpha);
            grad.addColorStopRGBA(0.55, ...stops[1], alpha);
            grad.addColorStopRGBA(1, ...stops[2], alpha);
            hexPath(cr, cell.x, cell.y, R);
            cr.setSource(grad);
            cr.fill();
        }
        cr.$dispose();
    }

    // The text went in: the cells fold into a tick, then the capsule leaves.
    done(words) {
        this._cancelHide();
        for (const c of ['listening', 'finishing', 'error', 'warn'])
            this.remove_style_pseudo_class(c);
        this.add_style_pseudo_class('done');
        this._mode = 'done';
        this._done = 0;
        this._label.text = words > 0 ? `Typed · ${words} ${words === 1 ? 'word' : 'words'}` : 'Typed';
        this._label.visible = true;
        this._setHint(words > 0 ? 'Ctrl+Z to undo' : '');
        this._place();
        this.hide_(DONE_MS);
    }

    hide_(delay = 0) {
        this._cancelHide();
        this._hideTimer = GLib.timeout_add(GLib.PRIORITY_DEFAULT, delay, () => {
            this._hideTimer = 0;
            this.ease({opacity: 0, scale_x: 0.94, scale_y: 0.94, duration: 240, mode: Clutter.AnimationMode.EASE_IN_QUAD,
                onComplete: () => {
                    this.visible = false;
                    this._stopTimer();
                    this._live = this._target = 0;
                }});
            return GLib.SOURCE_REMOVE;
        });
    }

    _cancelHide() {
        if (this._hideTimer)
            GLib.source_remove(this._hideTimer);
        this._hideTimer = 0;
    }

    vfunc_destroy() {
        this._cancelHide();
        this._stopTimer();
        super.vfunc_destroy();
    }
});

export default class VoicioExtension extends Extension {
    enable() {
        this._state = 'offline';
        this._preedit = false;
        this._lastText = '';
        // Kept for the extension's lifetime: mutter delivers its key events asynchronously, so
        // disposing it right after typing crashes the compositor (as GNOME's own OSK also avoids).
        const seat = Clutter.get_default_backend().get_default_seat();
        this._keyboard = seat.create_virtual_device(Clutter.InputDeviceType.KEYBOARD_DEVICE);

        this._pill = new Pill();
        Main.layoutManager.addTopChrome(this._pill, {trackFullscreen: false});

        this._watch = Gio.bus_watch_name(Gio.BusType.SESSION, BUS_NAME, Gio.BusNameWatcherFlags.NONE,
            () => this._connect(), () => this._disconnect());

        // The agent owns the shortcut (the desktop's shortcut service gives it press AND release, which
        // is what hold-to-talk needs). This binding is only a fallback if that service is unavailable.
        this._bound = false;
    }

    disable() {
        this._unbindFallback();
        if (this._fallbackTimer)
            GLib.source_remove(this._fallbackTimer);
        this._fallbackTimer = 0;
        this._stopHoldWatch();
        Gio.bus_unwatch_name(this._watch);
        this._disconnect();
        this._pill.destroy();
        this._pill = null;
        this._keyboard.run_dispose();
        this._keyboard = null;
    }

    _message(text) {
        this._ui()?.show_('error', text, 'Open Voicio to fix');
        this._ui()?.hide_(MESSAGE_MS);
    }

    _connect() {
        this._proxy = new EngineProxy(Gio.DBus.session, BUS_NAME, OBJECT_PATH, (proxy, error) => {
            if (error) {
                console.error(`Voicio: ${error.message}`);
                return;
            }
            // Tell the agent we type its text. Newer agents answer whether they draw the pill
            // themselves (then ours stays hidden); older ones do not know Attach2.
            proxy.Attach2Remote((result, attachError) => {
                if (attachError) {
                    proxy.AttachRemote();
                    return;
                }
                this._agentDrawsPill = !!result[0];
                if (this._agentDrawsPill)
                    this._pill?.hide_();
                this._scheduleFallbackCheck();
            });
            this.refreshStatus();
        });
        this._signals = [
            this._proxy.connectSignal('Preview', (_p, _s, [text]) => this._showPreview(text)),
            this._proxy.connectSignal('Done', (_p, _s, [text]) => this._insert(text)),
            this._proxy.connectSignal('StateChanged', (_p, _s, [state]) => this._setState(state)),
            this._proxy.connectSignal('Level', (_p, _s, [level]) => this._ui()?.pushLevel(level)),
            this._proxy.connectSignal('Error', (_p, _s, [message]) => {
                console.warn(`Voicio: ${message}`);
                this._message(message);
            }),
        ];
    }

    // If the desktop's shortcut service has not accepted the agent's shortcut a few seconds after we
    // connect, bind the key here instead (it can only toggle: GNOME gives extensions no key release).
    _scheduleFallbackCheck() {
        if (this._fallbackTimer)
            GLib.source_remove(this._fallbackTimer);
        this._fallbackTimer = GLib.timeout_add_seconds(GLib.PRIORITY_DEFAULT, 8, () => {
            this._fallbackTimer = 0;
            this._proxy?.ShortcutBoundRemote((result, error) => {
                if (!error && !result[0])
                    this._bindFallback();
            });
            return GLib.SOURCE_REMOVE;
        });
    }

    _bindFallback() {
        if (this._bound)
            return;
        const action = Main.wm.addKeybinding('dictate', this.getSettings(), Meta.KeyBindingFlags.IGNORE_AUTOREPEAT,
            Shell.ActionMode.NORMAL | Shell.ActionMode.POPUP | Shell.ActionMode.OVERVIEW, () => this._onShortcut());
        this._bound = action !== Meta.KeyBindingAction.NONE;
        if (!this._bound)
            this._message('The dictation shortcut is used by something else. Run `voicio toggle` instead.');
    }

    _unbindFallback() {
        if (!this._bound)
            return;
        Main.wm.removeKeybinding('dictate');
        this._bound = false;
    }

    // The pill only exists when the agent is not drawing its own.
    _ui() {
        return this._agentDrawsPill ? null : this._pill;
    }

    _disconnect() {
        this._agentDrawsPill = false;
        this._unbindFallback();
        this._proxy?.DetachRemote();
        this._clearPreview();
        for (const id of this._signals ?? [])
            this._proxy.disconnectSignal(id);
        this._signals = null;
        this._proxy = null;
        this._state = 'offline';
        this._ui()?.hide_();
    }

    _setState(state) {
        const previous = this._state;
        this._state = state;
        if (state === 'listening') {
            this._lastText = '';
            this._ui()?.show_('listening', ''); // a quiet room shows no text
        } else if (state === 'finishing') {
            this._ui()?.show_('finishing', 'Tidying…');
        } else if (state === 'idle' && previous !== 'idle') {
            // Done/Error just arrived and may be showing a message: let it linger, else fade out.
            if (!this._ui()?.has_style_pseudo_class('error')) {
                if (this._typed)
                    this._ui()?.done(this._typedWords ?? 0);
                else
                    this._ui()?.hide_(250);
            }
            this._typed = false;
        }
    }

    toggle() {
        if (!this._proxy) {
            this._message('Voicio is not running. Start it with: systemctl --user start voicio');
            return;
        }
        if (this._state === 'idle')
            this._proxy.StartRemote(() => {});
        else if (this._state === 'listening')
            this._proxy.StopRemote(() => {});
    }

    refreshStatus() {
        this._proxy?.StatusRemote((result, error) => {
            if (error)
                return;
            this._state = result[0];
        });
    }

    // Tap = toggle. Hold past HOLD_MS = push-to-talk: finish when the shortcut's modifiers are released.
    _onShortcut() {
        const wasIdle = this._state === 'idle';
        this.toggle();
        if (!wasIdle)
            return;
        const [, , mods] = global.get_pointer();
        const held = mods & (Clutter.ModifierType.MOD4_MASK | Clutter.ModifierType.MOD1_MASK |
            Clutter.ModifierType.CONTROL_MASK | Clutter.ModifierType.SHIFT_MASK | Clutter.ModifierType.SUPER_MASK);
        if (!held)
            return;
        const pressedAt = GLib.get_monotonic_time();
        this._stopHoldWatch();
        this._holdWatch = GLib.timeout_add(GLib.PRIORITY_DEFAULT, 50, () => {
            const [, , now] = global.get_pointer();
            if ((now & held) !== 0)
                return GLib.SOURCE_CONTINUE;
            this._holdWatch = 0;
            if ((GLib.get_monotonic_time() - pressedAt) / 1000 > HOLD_MS && this._state === 'listening')
                this._proxy?.StopRemote(() => {});
            return GLib.SOURCE_REMOVE;
        });
    }

    _stopHoldWatch() {
        if (this._holdWatch)
            GLib.source_remove(this._holdWatch);
        this._holdWatch = 0;
    }

    _showPreview(text) {
        this._lastText = text;
        // The pill shows the live words even in apps that cannot display preedit text.
        if (this._state === 'listening')
            this._ui()?.setText(text);
        if (!Main.inputMethod.currentFocus)
            return;
        // Underlined, uncommitted text: the app shows it live but nothing is inserted yet.
        const length = [...text].length;
        Main.inputMethod.set_preedit_text(text, length, length, Clutter.PreeditResetMode.CLEAR);
        this._preedit = true;
    }

    _clearPreview() {
        if (this._preedit && Main.inputMethod.currentFocus)
            Main.inputMethod.set_preedit_text(null, 0, 0, Clutter.PreeditResetMode.CLEAR);
        this._preedit = false;
    }

    _insert(text) {
        this._clearPreview();
        if (!text)
            return;
        this._typed = true;
        this._typedWords = text.split(/\s+/).filter(w => w).length;
        try {
            if (Main.inputMethod.currentFocus) {
                console.log(`Voicio: committing ${text.length} chars through the input method`);
                Main.inputMethod.commit(text);
            } else {
                console.log(`Voicio: no input-method focus; typing ${text.length} chars with the virtual keyboard`);
                this._typeWithVirtualKeyboard(text);
            }
        } catch (e) {
            console.error(`Voicio: insertion failed: ${e}`);
            St.Clipboard.get_default().set_text(St.ClipboardType.CLIPBOARD, text); // only now: typing failed
            this._message('Could not type the text. It is on the clipboard: press Ctrl+V.');
        }
    }

    _typeWithVirtualKeyboard(text) {
        let time = GLib.get_monotonic_time();
        for (const ch of text) {
            const keyval = ch === '\n' ? Clutter.KEY_Return : Clutter.unicode_to_keysym(ch.codePointAt(0));
            this._keyboard.notify_keyval(time++, keyval, Clutter.KeyState.PRESSED);
            this._keyboard.notify_keyval(time++, keyval, Clutter.KeyState.RELEASED);
        }
    }
}
