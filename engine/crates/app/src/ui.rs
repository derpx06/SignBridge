//! Slint UI on the main thread (required on macOS): tray icon + menu where the desktop has a tray
//! (not GNOME, whose extension shows the indicator), a floating capsule that never takes focus
//! (Windows, macOS, X11), and the app window. Windows are created on demand and dropped when
//! hidden, so they cost no memory while unused. Text insertion and the global shortcut also live
//! on this thread, as macOS requires.
//!
//! The look is the approved "Paper & Ink" canvas (design/*.dc.html): paper #F3EFE6, a white card,
//! ink #16130F, orange #F2541B, Bricolage Grotesque headings and Inter text (both bundled, SIL OFL).
//!
//! Other threads talk to it only through `invoke_from_event_loop`; without a display the calls are
//! no-ops and the agent runs headless.

use crate::agent::{Cmd, Msg};
use crate::autostart;
use crate::config::Config;
use crate::desktop::Desktop;
use crate::inject::{Delivered, Injector};
use crate::pill::model::{Bands, Event as PillEvent, Model as PillModel, NoticeKind};
use crate::pill::paint::{self, Target};
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};

slint::slint! {
    import { ScrollView, Palette } from "std-widgets.slint";
    import "../ui/fonts/Inter-Regular.ttf";
    import "../ui/fonts/Inter-Medium.ttf";
    import "../ui/fonts/Inter-SemiBold.ttf";
    import "../ui/fonts/Inter-Bold.ttf";
    import "../ui/fonts/BricolageGrotesque-ExtraBold.ttf";

    // One dictation on History / Home.
    export struct HistoryRow {
        text: string,
        when: string, // "10:42" or "2 min ago"
        day: string, // group heading: TODAY, YESTERDAY, 12 NOV
        line: string, // the text on one line (Home's recent list)
        first: bool, // first row of its day: draws the heading
        kind: string, // Note | List | Email
        meta: string, // "10:42 · 19 words"
        index: int,
    }

    export struct DictRow {
        phrase: string,
        output: string,
    }

    // One of the three packs (from models::Tier).
    export struct PartRow {
        name: string,
        size: string,
    }

    export struct PackRow {
        name: string, // Lite | Balanced | Max
        line: string,
        menu-sub: string, // sidebar menu: "Fastest · 315 MB memory"
        chip-sub: string, // setup chip menu: "Fastest · 137 MB download"
        download: string,
        memory: string,
        facts: string, // "137 MB · 315 MB memory"
        parts: [PartRow],
        acc: int,
        spd: int,
        have: bool, // every file is installed
        get: string, // "Get 1.2 GB" when files are missing
        busy: bool,
        progress: float,
    }

    // One speech model in the "Language models" list (model first).
    export struct LangRow {
        id: string,
        mark: string,
        tint: color,
        name: string,
        sub: string, // "Hindi · by AI4Bharat"
        size: string,
        experimental: bool,
        installed: bool,
        in-use: bool,
        builtin: bool, // included in every pack
        busy: bool,
        progress: float,
        status: string,
        added: bool, // setup: chosen to download
    }

    // One file being fetched in setup's Download step.
    export struct DlRow {
        id: string,
        name: string,
        detail: string,
        glyph: string,
        tint: color,
        progress: float,
        state: string,
    }

    export struct CreditRow {
        name: string,
        detail: string,
    }

    // One optional cloud transcription provider.
    export struct CloudRow {
        id: string,
        title: string, // the provider (company) first: "Groq", "Google Gemini"
        name: string,
        maker: string,
        mark: string,
        tint: color,
        price: string,
        speed: string,
        host: string,
        models: [string],
        model-index: int,
        needs-region: bool,
        has-key: bool,
        slow: bool,
        live-supported: bool, // words can stream in while speaking
        live-experimental: bool,
        live-on: bool,
    }

    export component Tray inherits SystemTrayIcon {
        in property <string> state: "idle";
        in property <string> models: "Speech models not loaded";
        callback toggle();
        callback cancel();
        callback settings();
        callback quit();
        icon: state == "listening" ? @image-url("../ui/mic-listening.svg")
            : state == "finishing" ? @image-url("../ui/mic-finishing.svg")
            : @image-url("../ui/mic-idle.svg");
        title: "Voicio";
        tooltip: state == "listening" ? "Voicio: listening" : state == "finishing" ? "Voicio: finishing" : "Voicio: ready";
        clicked => { root.toggle(); }
        Menu {
            MenuItem {
                title: root.state == "listening" ? "Finish dictation" : "Start dictation";
                enabled: root.state != "finishing";
                activated => { root.toggle(); }
            }
            MenuItem {
                title: "Cancel";
                enabled: root.state != "idle";
                activated => { root.cancel(); }
            }
            MenuSeparator { }
            MenuItem { title: root.models; enabled: false; }
            MenuItem { title: "Settings…"; activated => { root.settings(); } }
            MenuItem { title: "Quit Voicio"; activated => { root.quit(); } }
        }
    }

    // The floating capsule (Capsule.dc.html). `pill::paint` draws the ink capsule and the comb into
    // `frame`; the words sit inside it, 60 pt from its left edge. Settled words are cream, the last
    // few fade in; a caret blinks while it listens.
    export component Overlay inherits Window {
        in property <image> frame;
        in property <string> settled;
        in property <string> fresh;
        in property <float> fresh-opacity: 1.0;
        in property <string> caption; // "Tidying…", "Typed · 14 words", "Warming up"
        in property <color> caption-color: #EFE7D7;
        in property <string> hint; // "Ctrl+Z to undo" / "Open Voicio to fix"
        in property <length> capsule-width: 64px;
        in property <string> notice-text;
        in property <color> notice-color: #FFC24B;
        out property <length> notice-width: notice-label.preferred-width + hint-label.preferred-width + (root.hint != "" ? 14px : 0px) + 4px;
        out property <length> caption-width: caption-label.preferred-width + hint-label.preferred-width + (root.hint != "" ? 14px : 0px) + 4px;
        in property <bool> show-words: false;
        in property <bool> show-caret: false;
        in property <float> caret-phase: 1.0;
        out property <bool> dark: Palette.color-scheme == ColorScheme.dark;
        out property <length> words-width: settled-text.preferred-width + fresh-text.preferred-width + 6px;
        no-frame: true;
        always-on-top: true;
        background: transparent;
        default-font-family: "Inter";
        width: 400px;
        height: 64px;
        title: "Voicio";
        Image {
            source: root.frame;
            image-fit: fill;
            width: 400px;
            height: 64px;
            x: 0px;
            y: 0px;
        }
        // The text region inside the capsule: right of the comb, left of the 14 pt padding.
        Rectangle {
            x: (parent.width - root.capsule-width) / 2 + 52px;
            y: 26px - 11px;
            width: max(0px, root.capsule-width - 68px);
            height: 22px;
            clip: true;
            HorizontalLayout {
                visible: root.notice-text != "";
                spacing: 14px;
                alignment: start;
                notice-label := Text {
                    text: root.notice-text;
                    color: root.notice-color;
                    font-size: 13px;
                    font-weight: 500;
                    vertical-alignment: center;
                }
            }
            HorizontalLayout {
                visible: !root.show-words && root.notice-text == "";
                spacing: 14px;
                alignment: start;
                caption-label := Text {
                    text: root.caption;
                    color: root.caption-color;
                    font-size: 13px;
                    font-weight: 600;
                    vertical-alignment: center;
                }
            }
            hint-label := Text {
                // a long message keeps the room: the hint is dropped rather than cut off
                visible: root.hint != "" && !root.show-words && self.x + self.preferred-width <= parent.width;
                x: (root.notice-text != "" ? notice-label.preferred-width : caption-label.preferred-width) + 14px;
                text: root.hint;
                color: #F7F3EA.with-alpha(0.62);
                font-size: 12px;
                font-weight: 600;
                vertical-alignment: center;
                height: parent.height;
            }
            HorizontalLayout {
                visible: root.show-words && root.notice-text == "";
                // keep the newest words in view: the line slides left once it is wider than the region
                x: min(0px, parent.width - self.preferred-width);
                spacing: 0px;
                height: parent.height;
                settled-text := Text {
                    text: root.settled;
                    color: #EFE7D7;
                    font-size: 14px;
                    font-weight: 500;
                    vertical-alignment: center;
                }
                fresh-text := Text {
                    text: root.fresh;
                    color: #EFE7D7;
                    opacity: 0.45 + 0.55 * root.fresh-opacity;
                    font-size: 14px;
                    font-weight: 500;
                    vertical-alignment: center;
                }
                Rectangle {
                    width: 6px;
                    Rectangle {
                        x: 3px;
                        y: (parent.height - self.height) / 2;
                        width: 2px;
                        height: 15px;
                        border-radius: 1px;
                        background: #EFE7D7;
                        opacity: root.show-caret ? 0.6 * root.caret-phase : 0.0;
                    }
                }
            }
            // older words dissolve into the capsule at the left edge
            Rectangle {
                x: 0px;
                width: 14px;
                visible: root.show-words && settled-text.preferred-width + fresh-text.preferred-width + 6px > parent.width + 3px;
                background: @linear-gradient(90deg, #16130F 0%, rgba(22, 19, 15, 0) 100%);
            }
        }
    }

    // ---- "Paper & Ink" design tokens (light only) ---------------------------------------------
    global Theme {
        out property <color> page: #F3EFE6;
        out property <color> card: #FFFFFF;
        out property <color> soft: #F7F3EA;
        out property <color> ink: #16130F;
        out property <color> muted: rgb(94, 87, 75);
        out property <color> body: #3D372E;
        out property <color> line: #EFE9DD;
        out property <color> line2: #E8E2D6;
        out property <color> track: #E3DDD0;
        out property <color> accent: #F2541B;
        out property <color> link: #B83A0E;
        out property <color> placeholder: rgb(110, 103, 91);
        out property <color> green: rgb(78, 154, 19);
        out property <color> green-soft: #EAF8D2;
        out property <color> green-ink: #2F5B0B;
        out property <color> blue-soft: #E2F0FF;
        out property <color> blue-ink: #1F4E8C;
        out property <color> peach-soft: #FFE3D3;
        out property <color> yellow-soft: #FFF0B8;
        out property <color> yellow-ink: #6B4E00;
        out property <color> danger: #B42318;
        out property <color> danger-soft: #FFE4DE;
        out property <string> head: "Bricolage Grotesque";
    }

    // 24x24 line icon; pass SVG path data.
    component Glyph inherits Path {
        in property <string> d;
        in property <color> tint: Theme.ink;
        in property <length> size: 15px;
        in property <length> weight: 2.2px;
        commands: root.d;
        stroke: root.tint;
        stroke-width: root.weight;
        stroke-line-cap: round;
        stroke-line-join: round;
        fill: transparent;
        width: root.size;
        height: root.size;
        viewbox-width: 24;
        viewbox-height: 24;
    }

    // The honeycomb v2 (Honeycomb2.dc.html): 37 soft pointy-top hexagons in rings of 1, 6, 12 and
    // 18, so the whole mark is a hexagon. `size` is the comb's box. `look`: 0 brand (orange core,
    // cream cells fading outward), 1 microphone (green, the inner rings swell with `level`),
    // 2 home hero (orange / peach / ink), 3 download (cells light up to `fill`), 4 tick (lime).
    component Comb inherits Rectangle {
        in property <length> size: 32px;
        in property <int> look: 0;
        in property <float> level: 0;
        in property <float> fill: 0;
        property <length> s: root.size / 11.6;
        width: root.size;
        height: root.size;
        for i in 37: Path {
            property <int> r: i < 4 ? -3 : i < 9 ? -2 : i < 15 ? -1 : i < 22 ? 0 : i < 28 ? 1 : i < 33 ? 2 : 3;
            property <int> start: r == -3 ? 0 : r == -2 ? 4 : r == -1 ? 9 : r == 0 ? 15 : r == 1 ? 22 : r == 2 ? 28 : 33;
            property <int> q: Math.max(-3, -3 - r) + (i - start);
            property <int> g: Math.max(Math.abs(q), Math.max(Math.abs(r), Math.abs(q + r)));
            property <bool> on-tick: (q == -1 && r == -1) || (q == -1 && r == 0) || (q == -1 && r == 1) || (q == 0 && r == 0) || (q == 1 && r == -1) || (q == 2 && r == -2) || (q == 3 && r == -3);
            property <float> k: root.look == 1 && g <= 1 ? 0.92 + 0.2 * root.level : root.look == 4 ? (on-tick ? 1.04 : 0.88) : 1;
            property <length> rad: root.s * 0.82 * k;
            commands: "M-0.242 -0.860 Q0.000 -1.000 0.242 -0.860 L0.624 -0.640 Q0.866 -0.500 0.866 -0.220 L0.866 0.220 Q0.866 0.500 0.624 0.640 L0.242 0.860 Q0.000 1.000 -0.242 0.860 L-0.624 0.640 Q-0.866 0.500 -0.866 0.220 L-0.866 -0.220 Q-0.866 -0.500 -0.624 -0.640 Z";
            viewbox-x: -1;
            viewbox-y: -1;
            viewbox-width: 2;
            viewbox-height: 2;
            width: 2 * rad;
            height: 2 * rad;
            x: root.size / 2 + root.s * 1.7320508 * (q + r / 2) - rad;
            y: root.size / 2 + root.s * 1.5 * r - rad;
            stroke-width: 0px;
            fill: root.look == 0 ? (g == 0 ? @linear-gradient(135deg, #FFC9AA 0%, #FF7A45 45%, #B8380B 100%) : @linear-gradient(135deg, #FFFFFF 0%, #F1EADB 55%, #CDBFA7 100%))
                : root.look == 1 ? (g == 0 ? #4E9A13 : g <= 2 ? #8BCB45 : #16130F)
                : root.look == 2 ? (g == 0 ? #F2541B : g <= 2 ? #FF9466 : #16130F)
                : root.look == 3 ? (i / 37 < root.fill ? #F2541B : #16130F)
                : (on-tick ? #B8F06A : #F7F3EA);
            opacity: root.look == 0 ? (g == 0 ? 1 : g == 1 ? 0.92 : g == 2 ? 0.62 : 0.32)
                : root.look == 1 ? (g <= 1 ? 1 : g == 2 ? 0.6 : 0.15)
                : root.look == 2 ? (g <= 1 ? 1 : g == 2 ? 0.55 : 0.18)
                : root.look == 3 ? (i / 37 < root.fill ? 1 : 0.1)
                : (on-tick ? 1 : 0.22);
        }
    }

    // The app mark: the comb on an ink tile.
    component Mark inherits Rectangle {
        in property <length> size: 40px;
        width: root.size;
        height: root.size;
        Image { source: @image-url("../ui/voicio.svg"); width: root.size; height: root.size; }
    }

    // A button. Primary = ink, otherwise the given fill.
    component Btn inherits Rectangle {
        in property <string> text;
        in property <bool> primary: false;
        in property <bool> enabled: true;
        in property <string> icon;
        in property <color> fill: Theme.soft;
        in property <color> ink: Theme.ink;
        in property <length> h: 48px;
        in property <length> r: 16px;
        in property <length> pad: 20px;
        in property <length> fs: 15px;
        in property <int> fw: 600;
        callback clicked();
        height: root.h;
        min-width: label.preferred-width;
        horizontal-stretch: 0;
        border-radius: root.r;
        opacity: root.enabled ? 1.0 : 0.45;
        background: root.primary ? (ta.pressed ? #000000 : ta.has-hover && root.enabled ? #2A251E : Theme.ink)
            : (ta.pressed ? root.fill.darker(0.08) : ta.has-hover && root.enabled ? root.fill.darker(0.04) : root.fill);
        animate background { duration: 120ms; }
        label := HorizontalLayout {
            padding-left: root.pad;
            padding-right: root.pad;
            spacing: 8px;
            alignment: center;
            if root.icon != "": Glyph { d: root.icon; tint: root.primary ? #FFFFFF : root.ink; size: 16px; weight: 2.4px; y: (parent.height - self.height) / 2; }
            Text {
                text: root.text;
                vertical-alignment: center;
                font-size: root.fs;
                font-weight: root.primary ? 700 : root.fw;
                color: root.primary ? #FFFFFF : root.ink;
            }
        }
        ta := TouchArea {
            enabled: root.enabled;
            mouse-cursor: root.enabled ? MouseCursor.pointer : MouseCursor.default;
            clicked => { root.clicked(); }
        }
    }

    // A square icon button (copy, delete, remove).
    component IconBtn inherits Rectangle {
        in property <string> icon;
        in property <color> tint: Theme.ink;
        in property <length> size: 40px;
        callback clicked();
        width: root.size;
        height: root.size;
        border-radius: root.size * 0.3;
        background: ta.has-hover ? Theme.line : #FFFFFF;
        Glyph { d: root.icon; tint: root.tint; size: root.size * 0.4; weight: 2px; x: (parent.width - self.width) / 2; y: (parent.height - self.height) / 2; }
        ta := TouchArea { mouse-cursor: MouseCursor.pointer; clicked => { root.clicked(); } }
    }

    component Chip inherits Rectangle {
        in property <string> text;
        in property <color> fg: Theme.ink;
        in property <color> bg: #FFFFFF;
        in property <length> fs: 12px;
        in property <length> padx: 11px;
        in property <length> h: 24px;
        height: root.h;
        border-radius: root.h / 2;
        background: root.bg;
        horizontal-stretch: 0;
        min-width: t.preferred-width + 2 * root.padx;
        t := Text { x: root.padx; text: root.text; font-size: root.fs; font-weight: 700; color: root.fg; vertical-alignment: center; height: parent.height; }
    }

    // A keyboard key: white face, ink ring and an ink "drop" below it.
    component KeyCap inherits Rectangle {
        in property <string> label;
        in property <length> fs: 18px;
        in property <length> ring: 2px;
        in property <length> drop: 4px;
        in property <length> r: 14px;
        in property <length> padx: 14px;
        in property <bool> capturing: false;
        callback clicked();
        min-width: t.preferred-width + 2 * root.padx;
        horizontal-stretch: 0;
        Rectangle { y: root.drop; width: parent.width + 2 * root.ring; x: -root.ring; height: parent.height + root.ring; border-radius: root.r + root.ring; background: Theme.ink; }
        Rectangle {
            x: -root.ring; y: -root.ring; width: parent.width + 2 * root.ring; height: parent.height + 2 * root.ring;
            border-radius: root.r + root.ring; background: Theme.ink;
            Rectangle { x: root.ring; y: root.ring; width: parent.width - 2 * root.ring; height: parent.height - 2 * root.ring; border-radius: root.r; background: root.capturing ? #FFF1E8 : ta.has-hover ? #FBF8F2 : #FFFFFF; }
        }
        t := Text {
            text: root.capturing ? "Press a key…" : root.label;
            font-family: root.capturing ? "Inter" : Theme.head;
            font-size: root.capturing ? max(13px, root.fs * 0.32) : root.fs;
            font-weight: 800;
            letter-spacing: root.capturing ? 0px : -root.fs * 0.03;
            color: Theme.ink;
            horizontal-alignment: center;
            vertical-alignment: center;
            width: parent.width;
            height: parent.height;
        }
        ta := TouchArea { mouse-cursor: MouseCursor.pointer; clicked => { root.clicked(); } }
    }

    // On/off switch, 52 x 30 (Settings.dc.html).
    component Toggle inherits Rectangle {
        in-out property <bool> checked;
        callback toggled(bool);
        width: 52px;
        height: 30px;
        border-radius: 15px;
        horizontal-stretch: 0;
        background: root.checked ? Theme.accent : Theme.track;
        animate background { duration: 160ms; }
        Rectangle {
            x: root.checked ? 25px : 3px;
            y: 3px;
            width: 24px;
            height: 24px;
            border-radius: 12px;
            background: #FFFFFF;
            drop-shadow-blur: 3px;
            drop-shadow-offset-y: 1px;
            drop-shadow-color: rgba(0, 0, 0, 0.2);
            animate x { duration: 180ms; easing: cubic-bezier(0.3, 1.4, 0.5, 1); }
        }
        TouchArea { mouse-cursor: MouseCursor.pointer; clicked => { root.checked = !root.checked; root.toggled(root.checked); } }
    }

    // Segmented choice (Settings.dc.html): a sand track, the choice on a white thumb.
    component Seg inherits Rectangle {
        in property <[string]> options;
        in-out property <int> index;
        callback picked(int);
        height: 48px;
        border-radius: 14px;
        background: Theme.page;
        property <length> seg: (self.width - 8px - (root.options.length - 1) * 4px) / root.options.length;
        for o[i] in root.options: Rectangle {
            x: 4px + i * (root.seg + 4px);
            y: 4px;
            width: root.seg;
            height: parent.height - 8px;
            border-radius: 11px;
            background: root.index == i ? #FFFFFF : transparent;
            drop-shadow-blur: root.index == i ? 6px : 0px;
            drop-shadow-offset-y: 2px;
            drop-shadow-color: rgba(60, 40, 10, 0.12);
            Text { text: o; horizontal-alignment: center; vertical-alignment: center; font-size: 13px; font-weight: 700; color: Theme.ink; width: parent.width; height: parent.height; }
            TouchArea { mouse-cursor: MouseCursor.pointer; clicked => { root.index = i; root.picked(i); } }
        }
    }

    // A text field: white box, 2px sand border (ink while typing), placeholder.
    component Field inherits Rectangle {
        in property <string> placeholder;
        in-out property <string> text <=> input.text;
        in property <bool> multiline: false;
        in property <bool> password: false;
        in property <bool> search: false;
        in property <length> fs: 15px;
        callback edited(string);
        callback accepted(string);
        min-height: 48px;
        border-radius: root.search ? 16px : 14px;
        background: root.search ? Theme.soft : #FFFFFF;
        border-width: root.search ? 0px : 2px;
        border-color: input.has-focus ? Theme.ink : Theme.line2;
        if root.search: Glyph { x: 16px; y: (parent.height - self.height) / 2; d: "M11 4a7 7 0 1 1 0 14 7 7 0 0 1 0-14z M20 20l-4-4"; tint: Theme.muted; size: 16px; weight: 2.4px; }
        property <length> left: root.search ? 42px : 14px;
        Text {
            x: root.left;
            y: root.multiline ? 16px : 0px;
            width: parent.width - root.left - 14px;
            height: root.multiline ? self.preferred-height : parent.height;
            visible: input.text == "";
            text: root.placeholder;
            color: Theme.placeholder;
            font-size: root.fs;
            wrap: word-wrap;
            vertical-alignment: root.multiline ? top : center;
        }
        input := TextInput {
            x: root.left;
            y: root.multiline ? 16px : 0px;
            width: parent.width - root.left - 14px;
            height: root.multiline ? parent.height - 32px : parent.height;
            single-line: !root.multiline;
            wrap: root.multiline ? word-wrap : no-wrap;
            input-type: root.password ? InputType.password : InputType.text;
            font-size: root.fs;
            color: Theme.ink;
            selection-background-color: #FFD7C0;
            vertical-alignment: root.multiline ? top : center;
            edited => { root.edited(self.text); }
            accepted => { root.accepted(self.text); }
        }
    }

    // Five accuracy / speed segments.
    component Segs inherits HorizontalLayout {
        in property <int> n;
        in property <color> fill;
        spacing: 4px;
        for i in 5: Rectangle { height: 8px; border-radius: 4px; background: i < root.n ? root.fill : rgba(22, 19, 15, 0.1); }
    }

    component H1 inherits Text {
        font-family: Theme.head;
        font-size: 44px;
        font-weight: 800;
        letter-spacing: -1.4px;
        color: Theme.ink;
        wrap: word-wrap;
    }

    component H2 inherits Text {
        font-family: Theme.head;
        font-size: 24px;
        font-weight: 800;
        letter-spacing: -0.4px;
        color: Theme.ink;
    }

    component Link inherits Text {
        callback clicked();
        font-size: 14px;
        font-weight: 600;
        color: lta.has-hover ? #8A2A08 : Theme.link;
        lta := TouchArea { mouse-cursor: MouseCursor.pointer; clicked => { root.clicked(); } }
    }

    // Eyebrow over a setup title: "STEP 2 · MICROPHONE".
    component Eyebrow inherits Text {
        font-size: 13px;
        font-weight: 700;
        letter-spacing: 1.2px;
        color: Theme.link;
    }

    // A small tinted icon square (sidebar, setup points, settings groups).
    component Tile inherits Rectangle {
        in property <string> icon;
        in property <length> size: 28px;
        in property <length> glyph: 15px;
        in property <color> tint: Theme.ink;
        width: root.size;
        height: root.size;
        border-radius: root.size * 0.32;
        Glyph { d: root.icon; size: root.glyph; tint: root.tint; x: (parent.width - self.width) / 2; y: (parent.height - self.height) / 2; }
    }

    // A badge with two letters (language models, cloud providers).
    component Badge inherits Rectangle {
        in property <string> mark;
        in property <length> size: 46px;
        width: root.size;
        height: root.size;
        border-radius: 14px;
        Text { text: root.mark; font-size: 16px; font-weight: 800; color: Theme.ink; horizontal-alignment: center; vertical-alignment: center; width: parent.width; height: parent.height; }
    }

    component Bar inherits Rectangle {
        in property <float> value;
        in property <color> fill: Theme.accent;
        height: 6px;
        border-radius: 3px;
        background: Theme.line2;
        clip: true;
        Rectangle { x: 0px; width: parent.width * Math.max(0, Math.min(1, root.value)); height: parent.height; border-radius: 3px; background: root.fill; animate width { duration: 200ms; } }
    }

    // The sidebar (Sidebar.dc.html): mark, six pages with tinted icon squares, and the pack switcher.
    component Sidebar inherits Rectangle {
        in-out property <int> page;
        in property <[PackRow]> packs;
        in property <int> pack-index;
        in property <string> pack-sub;
        in-out property <bool> menu-open;
        callback pick-pack(int);
        width: 248px;
        background: Theme.page;
        VerticalLayout {
            padding-top: 24px;
            padding-left: 16px;
            padding-right: 16px;
            padding-bottom: 18px;
            spacing: 4px;
            HorizontalLayout {
                padding-left: 8px;
                padding-bottom: 28px;
                spacing: 10px;
                alignment: start;
                Mark { size: 40px; }
                Text { text: "voicio"; font-family: Theme.head; font-weight: 800; font-size: 25px; letter-spacing: -0.8px; color: Theme.ink; vertical-alignment: center; }
            }
            for item[i] in [
                { id: 0, label: "Home", tint: #FFD8C2, d: "M3 11l9-7 9 7 M5 10v10h14V10" },
                { id: 1, label: "History", tint: #D9ECFF, d: "M12 3a9 9 0 1 1 0 18 9 9 0 0 1 0-18z M12 7v5l3 2" },
                { id: 5, label: "Dictionary", tint: #E6F7C4, d: "M5 4h11a3 3 0 0 1 3 3v13H8a3 3 0 0 1-3-3z M5 17a3 3 0 0 1 3-3h11" },
                { id: 2, label: "Models", tint: #FFE7A8, d: "M12 3l8 4.5v9L12 21l-8-4.5v-9z M4 7.5l8 4.5 8-4.5 M12 12v9" },
                { id: 3, label: "Settings", tint: #EBE4FF, d: "M4 7h10 M18 7h2 M4 17h4 M12 17h8 M16 5v4 M10 15v4" },
                { id: 4, label: "About", tint: #E8E2D6, d: "M12 3a9 9 0 1 1 0 18 9 9 0 0 1 0-18z M12 11v6 M12 7.5v.5" },
            ]: Rectangle {
                height: 46px;
                border-radius: 14px;
                background: root.page == item.id ? #FFFFFF : nta.has-hover ? rgba(255, 255, 255, 0.5) : transparent;
                drop-shadow-blur: root.page == item.id ? 14px : 0px;
                drop-shadow-offset-y: 4px;
                drop-shadow-color: rgba(60, 40, 10, 0.08);
                HorizontalLayout {
                    padding-left: 10px;
                    spacing: 12px;
                    alignment: start;
                    Tile { icon: item.d; background: item.tint; y: (parent.height - self.height) / 2; }
                    Text { text: item.label; font-size: 15px; font-weight: root.page == item.id ? 700 : 500; color: Theme.ink; vertical-alignment: center; }
                }
                nta := TouchArea { mouse-cursor: MouseCursor.pointer; clicked => { root.page = item.id; root.menu-open = false; } }
            }
            Rectangle { vertical-stretch: 1; min-height: 24px; }
            switcher := Rectangle {
                height: 62px;
                border-radius: 18px;
                background: #FFFFFF;
                drop-shadow-blur: 14px;
                drop-shadow-offset-y: 4px;
                drop-shadow-color: rgba(60, 40, 10, 0.08);
                HorizontalLayout {
                    padding-left: 12px;
                    padding-right: 12px;
                    spacing: 10px;
                    Rectangle {
                        width: 36px; height: 36px; y: (parent.height - self.height) / 2; border-radius: 11px;
                        background: root.pack-index == 0 ? #C5F07A : root.pack-index == 1 ? #FFB38A : #9CCBFF;
                        Glyph { d: "M12 3a3 3 0 0 1 3 3v5a3 3 0 0 1-6 0V6a3 3 0 0 1 3-3z M5 11a7 7 0 0 0 14 0 M12 18v3"; size: 16px; weight: 2.4px; x: 10px; y: 10px; }
                    }
                    VerticalLayout {
                        alignment: center;
                        spacing: 2px;
                        horizontal-stretch: 1; preferred-width: 100px; min-width: 0px;
                        Text { text: (root.packs.length > root.pack-index ? root.packs[root.pack-index].name : "") + " pack"; font-size: 14px; font-weight: 700; color: Theme.ink; }
                        Text { text: root.pack-sub; font-size: 12px; color: Theme.muted; overflow: elide; }
                    }
                    Glyph { d: root.menu-open ? "M7 10l5 5 5-5" : "M7 14l5-5 5 5"; size: 16px; weight: 2.4px; y: (parent.height - self.height) / 2; }
                }
                TouchArea { mouse-cursor: MouseCursor.pointer; clicked => { root.menu-open = !root.menu-open; } }
            }
        }
        if root.menu-open: Rectangle {
            x: 16px;
            width: parent.width - 32px;
            height: menu.preferred-height;
            y: parent.height - 18px - 62px - 8px - self.height;
            border-radius: 18px;
            background: #FFFFFF;
            drop-shadow-blur: 34px;
            drop-shadow-offset-y: 14px;
            drop-shadow-color: rgba(60, 40, 10, 0.16);
            menu := VerticalLayout {
                padding: 6px;
                spacing: 2px;
                Text { text: "SWITCH PACK"; font-size: 11px; font-weight: 700; letter-spacing: 1px; color: Theme.muted; height: 28px; vertical-alignment: center; x: 10px; }
                for p[i] in root.packs: Rectangle {
                    min-height: 48px;
                    border-radius: 12px;
                    background: i == root.pack-index ? Theme.soft : mta.has-hover ? #FBF8F2 : transparent;
                    HorizontalLayout {
                        padding-left: 10px;
                        padding-right: 10px;
                        spacing: 10px;
                        Rectangle { width: 14px; height: 14px; y: (parent.height - self.height) / 2; border-radius: 5px; background: i == 0 ? #C5F07A : i == 1 ? #FFB38A : #9CCBFF; }
                        VerticalLayout {
                            alignment: center;
                            spacing: 1px;
                            horizontal-stretch: 1; preferred-width: 100px; min-width: 0px;
                            Text { text: p.name; font-size: 13px; font-weight: 700; color: Theme.ink; }
                            Text { text: p.menu-sub; font-size: 11px; color: Theme.muted; wrap: word-wrap; }
                        }
                        Chip {
                            y: (parent.height - self.height) / 2;
                            h: 22px;
                            fs: 11px;
                            padx: 8px;
                            text: i == root.pack-index ? "In use" : p.busy ? Math.round(p.progress * 100) + "%" : p.have ? "Switch" : "Get " + p.get;
                            bg: i == root.pack-index ? Theme.ink : p.have ? #EFE9DD : Theme.blue-soft;
                            fg: i == root.pack-index ? #FFFFFF : p.have ? Theme.body : Theme.blue-ink;
                        }
                    }
                    mta := TouchArea { mouse-cursor: MouseCursor.pointer; clicked => { root.menu-open = false; root.pick-pack(i); } }
                }
                Rectangle {
                    height: 44px;
                    Rectangle { y: 4px; height: 1px; background: Theme.line; }
                    Text { x: 10px; y: 4px; height: 40px; text: "All models and languages"; font-size: 13px; font-weight: 700; color: Theme.link; vertical-alignment: center; }
                    TouchArea { mouse-cursor: MouseCursor.pointer; clicked => { root.menu-open = false; root.page = 2; } }
                }
                Rectangle {
                    height: 36px;
                    Text { x: 10px; height: 36px; text: "Use a cloud provider"; font-size: 13px; font-weight: 600; color: Theme.body; vertical-alignment: center; }
                    TouchArea { mouse-cursor: MouseCursor.pointer; clicked => { root.menu-open = false; root.page = 3; } }
                }
            }
        }
    }

    // A white card on the soft surface: one setting per row (Settings.dc.html).
    component Group inherits Rectangle {
        in property <string> title;
        in property <string> note;
        in property <string> icon;
        in property <color> tint;
        border-radius: 24px;
        background: Theme.soft;
        VerticalLayout {
            padding: 8px;
            HorizontalLayout {
                padding-left: 16px;
                padding-top: 14px;
                padding-bottom: 8px;
                padding-right: 16px;
                spacing: 10px;
                alignment: start;
                Tile { icon: root.icon; background: root.tint; }
                Text { text: root.title; font-family: Theme.head; font-size: 20px; font-weight: 800; color: Theme.ink; vertical-alignment: center; }
                if root.note != "": Text { text: root.note; font-size: 13px; font-weight: 600; color: Theme.muted; vertical-alignment: center; }
            }
            Rectangle {
                background: #FFFFFF;
                border-radius: 18px;
                VerticalLayout { @children }
            }
        }
    }

    // One row of a Group: title and hint on the left, a control on the right.
    component Row inherits Rectangle {
        in property <string> title;
        in property <string> hint;
        in property <bool> last: false;
        min-height: 72px;
        HorizontalLayout {
            padding: 16px;
            spacing: 14px;
            VerticalLayout {
                alignment: center;
                spacing: 3px;
                horizontal-stretch: 1; preferred-width: 100px; min-width: 0px;
                Text { text: root.title; font-size: 15px; font-weight: 700; color: Theme.ink; }
                if root.hint != "": Text { text: root.hint; font-size: 13px; color: Theme.muted; wrap: word-wrap; }
            }
            @children
        }
        if !root.last: Rectangle { y: parent.height - 1.5px; height: 1.5px; background: Theme.page; }
    }


    // A card with a gradient and an optional radial glow. Slint's software renderer fills gradients
    // as plain rectangles (no rounded corners), so the four corners are masked with quarter circles
    // in the colour the gradient has there, over the colour `behind` the card.
    component GradCard inherits Rectangle {
        in property <color> from;
        in property <color> to;
        in property <angle> angle: 150deg;
        in property <bool> has-glow: false;
        in property <color> glow;
        in property <float> gx: 0.5;
        in property <float> gy: 0.5;
        in property <float> gsize: 1.3;
        in property <length> r: 26px;
        in property <color> behind: #FFFFFF;
        in property <length> ring: 0px;
        in property <color> ring-color: transparent;
        clip: true;
        background: @linear-gradient(root.angle, root.from 0%, root.to 100%);
        if root.has-glow: Rectangle {
            width: parent.width * root.gsize;
            height: self.width;
            x: parent.width * root.gx - self.width / 2;
            y: parent.height * root.gy - self.height / 2;
            background: @radial-gradient(circle, root.glow 0%, root.glow.transparentize(1) 55%);
        }
        @children
        for k in 4: Rectangle {
            property <float> cx: (k == 1 || k == 2) ? 1 : 0;
            property <float> cy: k >= 2 ? 1 : 0;
            property <float> w: root.width / 1px;
            property <float> h: root.height / 1px;
            property <float> span: Math.abs(w * Math.sin(root.angle)) + Math.abs(h * Math.cos(root.angle));
            property <float> t: Math.max(0, Math.min(1, ((cx - 0.5) * w * Math.sin(root.angle) - (cy - 0.5) * h * Math.cos(root.angle)) / Math.max(1, span) + 0.5));
            property <color> lin: root.to.mix(root.from, t);
            property <float> dist: Math.sqrt(Math.pow((cx - root.gx) * w, 2) + Math.pow((cy - root.gy) * h, 2));
            property <float> ga: Math.max(0, 1 - dist / Math.max(1, w * root.gsize * 0.707 * 0.65));
            x: cx == 0 ? 0px : parent.width - root.r;
            y: cy == 0 ? 0px : parent.height - root.r;
            width: root.r;
            height: root.r;
            clip: true;
            background: root.behind;
            Rectangle {
                x: cx == 0 ? 0px : -root.r;
                y: cy == 0 ? 0px : -root.r;
                width: 2 * root.r;
                height: 2 * root.r;
                border-radius: root.r;
                background: root.has-glow ? root.glow.mix(lin, ga) : lin;
            }
        }
        if root.ring > 0px: Rectangle {
            border-radius: root.r;
            border-width: root.ring;
            border-color: root.ring-color;
            background: transparent;
        }
    }

    // The pack choice card used on Home.
    component PackCard inherits GradCard {
        in property <PackRow> pack;
        in property <int> index;
        in property <bool> selected;
        callback clicked();
        min-height: 138px;
        r: 22px;
        from: root.index == 0 ? #F3FCE4 : root.index == 1 ? #FFF1E8 : #EEF6FF;
        to: root.index == 0 ? #DDF3B8 : root.index == 1 ? #FFD7C0 : #CFE5FF;
        ring: root.selected ? 2.5px : 0px;
        ring-color: Theme.ink;
        VerticalLayout {
            padding: 18px;
            spacing: 10px;
            alignment: start;
            HorizontalLayout {
                Text { text: root.pack.name; font-family: Theme.head; font-weight: 800; font-size: 22px; color: Theme.ink; horizontal-stretch: 1; preferred-width: 100px; min-width: 0px; }
                Chip {
                    h: 22px; fs: 11px; padx: 10px;
                    text: root.selected ? "In use" : root.pack.busy ? Math.round(root.pack.progress * 100) + "%" : root.pack.have ? "Switch" : "Get " + root.pack.get;
                    bg: root.selected ? Theme.ink : #FFFFFF;
                    fg: root.selected ? #FFFFFF : Theme.ink;
                    y: (parent.height - self.height) / 2;
                }
            }
            Text { text: root.pack.line; font-size: 14px; color: Theme.body; wrap: word-wrap; }
            Text { text: root.pack.facts; font-size: 12px; font-weight: 600; color: Theme.body; }
        }
        TouchArea { mouse-cursor: MouseCursor.pointer; clicked => { root.clicked(); } }
    }

    // A history / recent row's kind chip: Note, List or Email.
    component KindChip inherits Rectangle {
        in property <string> kind;
        in property <length> w: 66px;
        width: root.w;
        height: 26px;
        border-radius: 11px;
        background: root.kind == "List" ? Theme.yellow-soft : root.kind == "Email" ? Theme.peach-soft : Theme.blue-soft;
        Text { text: root.kind; font-size: 12px; font-weight: 700; color: root.kind == "List" ? Theme.yellow-ink : root.kind == "Email" ? #8A2A08 : Theme.blue-ink; horizontal-alignment: center; vertical-alignment: center; width: parent.width; height: parent.height; }
    }


    // One provider in the Cloud manager's grid: badge, name; a dot for the default (orange) or a saved key (green).
    component ProviderTile inherits Rectangle {
        in property <[CloudRow]> rows;
        in property <int> n;
        in property <int> limit;
        in property <int> selected;
        in property <string> default;
        callback clicked();
        property <bool> real: root.n < root.limit;
        horizontal-stretch: 1;
        preferred-width: 100px;
        min-width: 0px;
        height: 56px;
        border-radius: 13px;
        background: root.n == root.selected ? Theme.soft : root.real ? (pta.has-hover ? #FBF8F2 : #FFFFFF) : transparent;
        border-width: root.real ? 1.5px : 0px;
        border-color: root.n == root.selected ? Theme.ink : Theme.line;
        if root.real: HorizontalLayout {
            padding-left: 6px;
            padding-right: 8px;
            spacing: 8px;
            Badge { mark: root.rows[root.n].mark; background: root.rows[root.n].tint; size: 30px; border-radius: 9px; y: (parent.height - self.height) / 2; }
            VerticalLayout {
                alignment: center;
                horizontal-stretch: 1;
                preferred-width: 100px;
                min-width: 0px;
                Text { text: root.rows[root.n].title; font-size: 13px; font-weight: 700; color: Theme.ink; overflow: elide; }
                Text { text: root.rows[root.n].speed; font-size: 11px; color: Theme.muted; overflow: elide; }
                Text { text: root.rows[root.n].models.length > root.rows[root.n].model-index ? root.rows[root.n].models[root.rows[root.n].model-index] : ""; font-size: 10px; font-weight: 600; color: Theme.body; overflow: elide; }
            }
            if root.rows[root.n].id == root.default: Rectangle { width: 8px; height: 8px; y: (parent.height - self.height) / 2; border-radius: 4px; background: Theme.accent; }
            if root.rows[root.n].has-key && root.rows[root.n].id != root.default: Rectangle { width: 8px; height: 8px; y: (parent.height - self.height) / 2; border-radius: 4px; background: Theme.green; }
        }
        pta := TouchArea { enabled: root.real; mouse-cursor: MouseCursor.pointer; clicked => { root.clicked(); } }
    }

    // Settings › Cloud: provider first. Pick what the cloud is used for, then a provider: its model,
    // price and speed, the host audio goes to, a key that is tested before it is kept, and whether it
    // is the default.
    component CloudPanel inherits VerticalLayout {
        in property <[CloudRow]> rows;
        in-out property <int> index;
        in property <string> default;
        in-out property <int> mode;
        in-out property <bool> consent;
        in-out property <string> region;
        in property <string> status;
        in property <string> status-kind;
        in property <string> excluded;
        in property <int> featured: 10;
        callback pick(int);
        callback model(int, int);
        callback add-key(int, string, string);
        callback set-default(int);
        callback remove-key(int);
        callback mode-changed(int);
        callback live(int, bool);
        property <bool> has: root.rows.length > root.index;
        padding: 16px;
        spacing: 14px;
        VerticalLayout {
            spacing: 3px;
            Text { text: root.mode == 0 ? "Off: your voice stays here" : "Use cloud for"; font-size: 15px; font-weight: 700; color: Theme.ink; }
            Text {
                text: root.mode == 0 ? "Turn on to use your own API key. Audio goes only to the provider you pick."
                    : root.mode == 1 ? "Every dictation goes to your default provider. If it fails, this computer does the work."
                    : "Only when the speech pack is not downloaded yet.";
                font-size: 13px; color: Theme.muted; wrap: word-wrap;
            }
        }
        Seg { options: ["Off", "All dictations", "Pack missing"]; index <=> root.mode; picked(m) => { root.mode-changed(m); } }
        // the featured providers first, the rest under "More providers"
        for row[k] in Math.ceil(Math.min(root.featured, root.rows.length) / 2): HorizontalLayout {
            spacing: 6px;
            for j in 2: ProviderTile {
                rows: root.rows; n: k * 2 + j; limit: Math.min(root.featured, root.rows.length); selected: root.index; default: root.default;
                clicked => { root.index = self.n; root.consent = false; root.pick(self.n); }
            }
        }
        if root.rows.length > root.featured: Text { text: "More providers"; font-size: 12px; font-weight: 700; letter-spacing: 0.8px; color: Theme.muted; }
        for row[k] in Math.ceil(Math.max(0, root.rows.length - root.featured) / 2): HorizontalLayout {
            spacing: 6px;
            for j in 2: ProviderTile {
                rows: root.rows; n: root.featured + k * 2 + j; limit: root.rows.length; selected: root.index; default: root.default;
                clicked => { root.index = self.n; root.consent = false; root.pick(self.n); }
            }
        }
        if root.has: Rectangle {
            border-radius: 18px;
            background: Theme.soft;
            min-height: pd.preferred-height;
            pd := VerticalLayout {
                padding: 16px;
                spacing: 12px;
                alignment: start;
                HorizontalLayout {
                    spacing: 12px;
                    Badge { mark: root.rows[root.index].mark; background: root.rows[root.index].tint; size: 44px; y: (parent.height - self.height) / 2; }
                    VerticalLayout {
                        alignment: center;
                        spacing: 2px;
                        horizontal-stretch: 1; preferred-width: 100px; min-width: 0px;
                        Text { text: root.rows[root.index].title; font-size: 16px; font-weight: 700; color: Theme.ink; }
                        Text { text: root.rows[root.index].name + " · " + root.rows[root.index].host; font-size: 12px; color: Theme.muted; overflow: elide; }
                    }
                    if root.rows[root.index].id == root.default: Chip { text: "Default"; bg: Theme.ink; fg: #FFFFFF; y: (parent.height - self.height) / 2; }
                    if root.rows[root.index].has-key && root.rows[root.index].id != root.default: Chip { text: "Connected"; bg: Theme.green-soft; fg: Theme.green-ink; y: (parent.height - self.height) / 2; }
                }
                Text { text: root.rows[root.index].price + (root.rows[root.index].speed != "" ? " · " + root.rows[root.index].speed : ""); font-size: 13px; color: Theme.body; wrap: word-wrap; }
                if root.rows[root.index].models.length > 0: VerticalLayout {
                    spacing: 6px;
                    Text { text: "Model"; font-size: 13px; font-weight: 700; color: Theme.ink; }
                    for mrow[mk] in Math.ceil(root.rows[root.index].models.length / 2): HorizontalLayout {
                        spacing: 6px;
                        for mj in 2: Rectangle {
                            property <int> m: mk * 2 + mj;
                            horizontal-stretch: 1; preferred-width: 100px; min-width: 0px;
                            height: 36px;
                            border-radius: 11px;
                            background: m == root.rows[root.index].model-index ? #FFFFFF : transparent;
                            border-width: m < root.rows[root.index].models.length ? 1.5px : 0px;
                            border-color: m == root.rows[root.index].model-index ? Theme.ink : Theme.line2;
                            if m < root.rows[root.index].models.length: Text { x: 10px; width: parent.width - 20px; height: parent.height; text: root.rows[root.index].models[m]; font-size: 12px; font-weight: 600; color: Theme.ink; vertical-alignment: center; overflow: elide; }
                            TouchArea { enabled: m < root.rows[root.index].models.length; mouse-cursor: MouseCursor.pointer; clicked => { root.model(root.index, m); } }
                        }
                    }
                }
                if root.rows[root.index].live-supported: HorizontalLayout {
                    spacing: 12px;
                    VerticalLayout {
                        alignment: center;
                        spacing: 2px;
                        horizontal-stretch: 1; preferred-width: 100px; min-width: 0px;
                        HorizontalLayout {
                            spacing: 8px;
                            alignment: start;
                            Text { text: "Live"; font-size: 13px; font-weight: 700; color: Theme.ink; }
                            if root.rows[root.index].live-experimental: Chip { text: "Experimental"; bg: Theme.yellow-soft; fg: Theme.yellow-ink; fs: 11px; h: 20px; padx: 8px; }
                        }
                        Text { text: "Words appear while you speak (streams audio to " + root.rows[root.index].host + ")"; font-size: 12px; color: Theme.muted; wrap: word-wrap; }
                    }
                    Toggle {
                        checked: root.rows[root.index].live-on;
                        y: (parent.height - self.height) / 2;
                        toggled(on) => { root.live(root.index, on); }
                    }
                }
                if root.rows[root.index].needs-region: VerticalLayout {
                    spacing: 6px;
                    Text { text: "Region"; font-size: 13px; font-weight: 700; color: Theme.ink; }
                    Field { placeholder: "For example westeurope"; text <=> root.region; height: 44px; }
                }
                if !root.rows[root.index].has-key: VerticalLayout {
                    spacing: 10px;
                    key := Field { placeholder: "Paste your API key (kept only on this computer)"; password: true; height: 44px; }
                    HorizontalLayout {
                        spacing: 10px;
                        Rectangle {
                            width: 20px; height: 20px; y: 0px; border-radius: 5px;
                            background: root.consent ? Theme.ink : #FFFFFF;
                            border-width: 2px; border-color: Theme.ink;
                            if root.consent: Glyph { d: "M5 12l5 5 9-10"; tint: #FFFFFF; size: 14px; weight: 3px; x: 1px; y: 1px; }
                            TouchArea { mouse-cursor: MouseCursor.pointer; clicked => { root.consent = !root.consent; } }
                        }
                        Text { text: "I agree that my recordings are sent to " + root.rows[root.index].host + " when cloud is on."; font-size: 13px; color: Theme.ink; wrap: word-wrap; horizontal-stretch: 1; preferred-width: 100px; min-width: 0px; }
                    }
                    HorizontalLayout {
                        alignment: start;
                        Btn {
                            text: root.status-kind == "busy" ? "Testing…" : "Add and test key";
                            primary: true; h: 42px; r: 13px; pad: 16px; fs: 13px;
                            enabled: key.text != "" && root.consent && root.status-kind != "busy" && (!root.rows[root.index].needs-region || root.region != "");
                            clicked => { root.add-key(root.index, key.text, root.region); key.text = ""; }
                        }
                    }
                }
                if root.rows[root.index].has-key: HorizontalLayout {
                    spacing: 10px;
                    alignment: start;
                    if root.rows[root.index].id != root.default: Btn { text: "Make default"; primary: true; h: 42px; r: 13px; pad: 16px; fs: 13px; clicked => { root.set-default(root.index); } }
                    Btn { text: "Remove key"; fill: #FFFFFF; ink: Theme.danger; h: 42px; r: 13px; pad: 16px; fs: 13px; clicked => { root.remove-key(root.index); } }
                }
                if root.rows[root.index].slow: Text { text: "Slower: results arrive a few seconds after you stop."; font-size: 12px; color: Theme.muted; }
                if root.status != "": Text {
                    text: root.status;
                    font-size: 13px;
                    font-weight: 700;
                    wrap: word-wrap;
                    color: root.status-kind == "error" ? Theme.danger : root.status-kind == "ok" ? Theme.green-ink : Theme.muted;
                }
            }
        }
        // a wrapped Text's height is not known through layouts, which inflated the scroll length: size it by length instead
        if root.excluded != "": Rectangle {
            height: ceil(root.excluded.character-count / 42) * 16px + 4px;
            Text { width: parent.width; height: parent.height; text: root.excluded; font-size: 12px; color: Theme.muted; wrap: word-wrap; vertical-alignment: top; }
        }
    }

    // The setup's header (OnbHeader.dc.html): brand, six progress pills and the "Pack:" chip.
    component OnbHeader inherits Rectangle {
        in property <int> step; // 0..5
        in property <[PackRow]> packs;
        in-out property <int> pack;
        in property <int> recommended;
        in-out property <bool> open;
        callback picked(int);
        height: 84px;
        HorizontalLayout {
            padding-left: 28px;
            padding-right: 28px;
            Text { text: "voicio"; font-family: Theme.head; font-weight: 800; font-size: 24px; letter-spacing: -0.6px; color: Theme.ink; vertical-alignment: center; }
            Rectangle { horizontal-stretch: 1; preferred-width: 100px; min-width: 0px; }
            HorizontalLayout {
                spacing: 6px;
                alignment: center;
                for label[i] in ["Welcome", "Microphone", "Shortcut", "Pack", "Download", "Ready"]: Rectangle {
                    y: (parent.height - self.height) / 2;
                    height: 36px;
                    width: i == root.step ? 36px + lbl.preferred-width + 14px - 5px : 36px;
                    border-radius: 18px;
                    background: i == root.step ? Theme.ink : transparent;
                    Rectangle {
                        x: 5px; y: 5px; width: 26px; height: 26px; border-radius: 13px;
                        background: i == root.step ? Theme.accent : i < root.step ? Theme.ink : Theme.line2;
                        Text { text: i < root.step ? "✓" : (i + 1); font-size: 12px; font-weight: 800; color: i == root.step ? Theme.ink : i < root.step ? #F5F1E8 : Theme.muted; horizontal-alignment: center; vertical-alignment: center; width: parent.width; height: parent.height; }
                    }
                    lbl := Text { x: 39px; visible: i == root.step; text: label; font-size: 13px; font-weight: 700; color: #F5F1E8; vertical-alignment: center; height: parent.height; }
                }
            }
            Rectangle { horizontal-stretch: 1; preferred-width: 100px; min-width: 0px; }
            chip := Rectangle {
                y: (parent.height - self.height) / 2;
                height: 44px;
                width: chip-row.preferred-width;
                border-radius: 22px;
                background: #FFFFFF;
                border-width: 1.5px;
                border-color: Theme.ink;
                chip-row := HorizontalLayout {
                    padding-left: 12px;
                    padding-right: 14px;
                    spacing: 8px;
                    Rectangle { width: 12px; height: 12px; y: (parent.height - self.height) / 2; border-radius: 4px; background: root.pack == 0 ? #B8E986 : root.pack == 1 ? #FF8A5B : rgb(142, 197, 255); }
                    Text { text: "Pack: " + (root.packs.length > root.pack ? root.packs[root.pack].name : ""); font-size: 14px; font-weight: 600; color: Theme.ink; vertical-alignment: center; }
                    Glyph { d: "M7 10l5 5 5-5"; size: 14px; weight: 2.6px; y: (parent.height - self.height) / 2; }
                }
                TouchArea { mouse-cursor: MouseCursor.pointer; clicked => { root.open = !root.open; } }
            }
        }
    }

    // The chip's menu; drawn by the window so it floats above the step's card.
    component PackMenu inherits Rectangle {
        in property <[PackRow]> packs;
        in property <int> pack;
        in property <int> recommended;
        callback picked(int);
        width: 280px;
        height: col.preferred-height;
        border-radius: 16px;
        background: #FFFFFF;
        border-width: 1.5px;
        border-color: Theme.ink;
        drop-shadow-blur: 40px;
        drop-shadow-offset-y: 18px;
        drop-shadow-color: rgba(0, 0, 0, 0.18);
        col := VerticalLayout {
            padding: 6px;
            spacing: 2px;
            for p[i] in root.packs: Rectangle {
                min-height: 52px;
                border-radius: 11px;
                background: i == root.pack ? #F5F1E8 : ta.has-hover ? #FBF8F2 : transparent;
                HorizontalLayout {
                    padding-left: 10px;
                    padding-right: 10px;
                    spacing: 10px;
                    Rectangle { width: 12px; height: 12px; y: (parent.height - self.height) / 2; border-radius: 4px; background: i == 0 ? #B8E986 : i == 1 ? #FF8A5B : rgb(142, 197, 255); }
                    VerticalLayout {
                        alignment: center;
                        spacing: 1px;
                        Text { text: p.name; font-size: 14px; font-weight: 700; color: Theme.ink; }
                        Text { text: p.chip-sub; font-size: 12px; color: Theme.muted; wrap: word-wrap; }
                    }
                }
                ta := TouchArea { mouse-cursor: MouseCursor.pointer; clicked => { root.picked(i); } }
            }
            Rectangle {
                height: lbl.preferred-height + 16px;
                Rectangle { height: 1px; y: 0px; background: Theme.line2; }
                lbl := Text { x: 10px; y: 8px; width: parent.width - 20px; text: "You can change this at any step, and later in Models."; font-size: 12px; color: Theme.muted; wrap: word-wrap; }
            }
        }
    }

    // The setup's white card with a text column and an illustration panel (Onb*.dc.html).
    component OnbCard inherits Rectangle {
        border-radius: 30px;
        background: #FFFFFF;
        drop-shadow-blur: 40px;
        drop-shadow-offset-y: 18px;
        drop-shadow-color: rgba(60, 40, 10, 0.06);
    }

    // An illustration panel: a soft gradient with a radial glow (the glow is drawn as a large
    // radial blob, since Slint's radial gradients are centred circles).
    component Panel inherits GradCard {
        r: 24px;
        angle: 160deg;
        has-glow: true;
        gy: 0.45;
    }

    // A "Back" / "Continue" footer.
    component Footer inherits HorizontalLayout {
        in property <bool> back: true;
        in property <string> next: "Continue";
        in property <bool> next-enabled: true;
        callback go-back();
        callback go-next();
        spacing: 12px;
        if root.back: Btn { text: "Back"; y: (parent.height - self.height) / 2; clicked => { root.go-back(); } }
        Rectangle { horizontal-stretch: 1; preferred-width: 100px; min-width: 0px; }
        if root.next != "": Btn { text: root.next; primary: true; enabled: root.next-enabled; h: 52px; r: 18px; pad: 26px; fs: 16px; clicked => { root.go-next(); } }
    }

    // The app window.
    export component SettingsWindow inherits Window {
        title: "Voicio";
        icon: @image-url("../ui/voicio.png");
        preferred-width: 1280px;
        preferred-height: 860px;
        min-width: 960px;
        min-height: 720px;
        background: Theme.page;
        default-font-family: "Inter";
        in property <int> scheme: 0; // kept for `ui-snapshot`; the design is light only
        // ---- app state
        in-out property <int> page: 0;
        // read by the scroll regression test: the page's content height against the visible height
        out property <length> page-content-h: page-scroll.viewport-height;
        out property <length> page-view-h: page-scroll.height;
        in-out property <length> page-y <=> page-scroll.viewport-y;
        changed page => { root.page-y = 0px; }
        function scroll-by(d: length) {
            root.page-y = min(0px, max(-(page-scroll.viewport-height - page-scroll.height), root.page-y - d));
        }
        in property <string> state: "idle";
        in property <string> version;
        in property <string> models;
        in property <string> health-problem;
        in property <string> health-fix;
        in property <string> health-action;
        in property <string> health-note;
        callback health-act();
        callback health-check();
        // ---- the talk key
        in property <string> key-label: "F9";
        in-out property <int> key-mode: 0; // 0 tap or hold, 1 tap, 2 hold
        // what the key really does now: the GNOME fallback binding can only toggle, maybe on another key
        in property <string> bound-label: root.key-label;
        in property <int> bound-mode: root.key-mode;
        in-out property <bool> capturing: false;
        in property <string> key-note;
        callback key-captured(string, bool, bool, bool, bool);
        callback key-set(string);
        callback toggle();
        // ---- packs
        in property <[PackRow]> packs;
        in property <int> pack-index;
        in property <string> pack-sub;
        in-out property <bool> menu-open;
        callback pick-pack(int);
        // ---- history
        in property <[HistoryRow]> history;
        in property <[HistoryRow]> recent;
        in property <int> history-total;
        in-out property <int> history-filter: 0;
        in property <string> history-subtitle;
        callback history-search(string);
        callback history-filter-changed(int);
        callback history-delete(int);
        callback clear-history();
        callback copy(string);
        // ---- dictionary
        in property <[DictRow]> dict-rows;
        callback dict-add(string, string);
        callback dict-remove(int);
        // ---- models
        in property <[LangRow]> lang-rows;
        in property <bool> models-busy;
        in property <string> models-message;
        callback lang-use(string);
        callback install(string);
        callback cancel-install();
        callback remove(string);
        // ---- settings (each change applies at once)
        in-out property <bool> live-text;
        in-out property <bool> cleanup;
        in-out property <bool> voice-commands;
        in-out property <bool> noise-reduction;
        in-out property <bool> lists-on;
        in-out property <bool> email-on;
        in-out property <bool> launch-at-login;
        in-out property <bool> pause-media;
        in-out property <int> styling-index: 2;
        in-out property <int> cpu-index: 1; // 0 light, 1 balanced, 2 fast
        in-out property <int> history-days: 30;
        in property <[string]> mic-names;
        in-out property <int> mic-index: 0;
        callback mic-picked(int);
        callback apply();
        // ---- cloud
        in property <[CloudRow]> cloud-rows;
        in-out property <int> cloud-index: 0;
        in property <string> cloud-default; // provider id in use, "" = none
        in-out property <int> cloud-mode: 0; // 0 off, 1 all dictations, 2 only when the pack is missing
        in-out property <bool> cloud-consent: false;
        in-out property <string> cloud-region;
        in property <string> cloud-status;
        in property <string> cloud-status-kind; // "" | busy | ok | error
        in property <string> cloud-excluded;
        in property <int> cloud-featured: 10;
        callback cloud-pick(int);
        callback cloud-model(int, int);
        callback cloud-add-key(int, string, string);
        callback cloud-set-default(int);
        callback cloud-remove-key(int);
        callback cloud-mode-changed(int);
        callback cloud-live(int, bool);
        // ---- about
        in property <[CreditRow]> credits;
        callback open-url(string);
        // ---- first-run setup (onb-step -1 = closed; 0..5)
        in-out property <int> onb-step: -1;
        in-out property <int> onb-pack: 1;
        in property <int> recommended-pack: 1;
        in-out property <bool> chip-open: false;
        in property <string> mic-state: "waiting"; // waiting | none | quiet | ok
        in property <float> mic-level;
        in property <[float]> mic-bars;
        in property <string> mem-note;
        in property <[LangRow]> onb-langs;
        callback onb-lang-toggle(string);
        in property <string> onb-total;
        in property <[DlRow]> dl-rows;
        in property <float> dl-fraction;
        in property <string> dl-left;
        in property <string> dl-error;
        in property <bool> dl-done;
        in property <bool> dl-paused;
        callback dl-pause();
        in property <string> summary-mic;
        in property <string> summary-pack;
        in-out property <bool> onb-login: true;
        callback onb-enter(int);
        callback onb-finish();

        property <string> mode-text: root.key-mode == 0 ? "tap or hold" : root.key-mode == 1 ? "tap" : "hold";
        function go(n: int) {
            root.chip-open = false;
            root.onb-step = n;
            root.onb-enter(n);
        }

        // The key capture: while `capturing`, the next key press becomes the talk key.
        key-scope := FocusScope {
            enabled: root.capturing;
            key-pressed(event) => {
                if !root.capturing { return reject; }
                root.key-captured(event.text, event.modifiers.control, event.modifiers.alt, event.modifiers.shift, event.modifiers.meta);
                accept
            }
        }
        function capture() {
            root.capturing = true;
            key-scope.focus();
        }

        HorizontalLayout {
            Sidebar {
                page <=> root.page;
                packs: root.packs;
                pack-index: root.pack-index;
                pack-sub: root.pack-sub;
                menu-open <=> root.menu-open;
                pick-pack(i) => { root.pick-pack(i); }
            }
            Rectangle {
                horizontal-stretch: 1; preferred-width: 100px; min-width: 0px;
                Rectangle {
                    x: 0px;
                    y: 12px;
                    width: parent.width - 12px;
                    height: parent.height - 24px;
                    border-radius: 28px;
                    background: #FFFFFF;
                    drop-shadow-blur: 40px;
                    drop-shadow-offset-y: 18px;
                    drop-shadow-color: rgba(60, 40, 10, 0.06);
                    clip: true;
                    // keyboard scrolling: PageUp/PageDown/arrows/Home/End move the page when no field is typing
                    page-keys := FocusScope {
                        width: parent.width;
                        height: parent.height;
                        init => { self.focus(); }
                        key-pressed(e) => {
                            if e.text == Key.PageDown { root.scroll-by(page-scroll.height * 0.9); return accept; }
                            if e.text == Key.PageUp { root.scroll-by(-page-scroll.height * 0.9); return accept; }
                            if e.text == Key.DownArrow { root.scroll-by(48px); return accept; }
                            if e.text == Key.UpArrow { root.scroll-by(-48px); return accept; }
                            if e.text == Key.Home { root.page-y = 0px; return accept; }
                            if e.text == Key.End { root.page-y = -(page-scroll.viewport-height - page-scroll.height); return accept; }
                            reject
                        }
                    page-scroll := ScrollView {
                        width: parent.width;
                        viewport-height: max(page-body.preferred-height, self.height);
                        viewport-width: self.width;
                        height: parent.height;
                        // one column holds the active page; the viewport is its height, so tall pages scroll
                        page-body := VerticalLayout {
                        width: page-scroll.width;
                        alignment: start;

                        // ---------------- Home (Main.dc.html)
                        if root.page == 0: VerticalLayout {
                            padding-top: 40px;
                            padding-left: 44px;
                            padding-right: 44px;
                            padding-bottom: 44px;
                            spacing: 30px;
                            alignment: start;
                            HorizontalLayout {
                                spacing: 16px;
                                VerticalLayout {
                                    spacing: 10px;
                                    horizontal-stretch: 1; preferred-width: 100px; min-width: 0px;
                                    HorizontalLayout {
                                        alignment: start;
                                        H1 { text: "Say it."; font-size: 46px; letter-spacing: -1.6px; wrap: no-wrap; }
                                        H1 { text: " It's typed."; font-size: 46px; letter-spacing: -1.6px; color: Theme.accent; wrap: no-wrap; }
                                    }
                                    Text { text: "Voicio works in every app on this computer, and keeps your voice on it."; font-size: 16px; color: Theme.muted; wrap: word-wrap; }
                                }
                                VerticalLayout {
                                    alignment: start;
                                    Rectangle {
                                        height: 34px;
                                        width: st.preferred-width + 30px + 16px;
                                        border-radius: 17px;
                                        background: root.state == "listening" ? Theme.peach-soft : root.state == "finishing" ? #FFF3D6 : root.health-problem != "" ? Theme.danger-soft : Theme.green-soft;
                                        Rectangle { x: 14px; y: 13px; width: 8px; height: 8px; border-radius: 4px; background: root.state == "listening" ? Theme.accent : root.state == "finishing" ? #C98A00 : root.health-problem != "" ? Theme.danger : Theme.green; }
                                        st := Text { x: 30px; text: root.state == "listening" ? "Listening" : root.state == "finishing" ? "Writing it up" : root.health-problem != "" ? "Needs attention" : "Ready"; font-size: 13px; font-weight: 700; color: root.state == "listening" ? #8A2A08 : root.state == "finishing" ? #6B4E00 : root.health-problem != "" ? Theme.danger : Theme.green-ink; vertical-alignment: center; height: parent.height; }
                                    }
                                }
                            }
                            if root.health-problem != "": Rectangle {
                                border-radius: 20px;
                                background: Theme.danger-soft;
                                min-height: hb.preferred-height;
                                hb := HorizontalLayout {
                                    padding: 16px;
                                    padding-left: 18px;
                                    spacing: 14px;
                                    VerticalLayout {
                                        alignment: center;
                                        spacing: 3px;
                                        horizontal-stretch: 1; preferred-width: 100px; min-width: 0px;
                                        Text { text: root.health-problem; font-size: 15px; font-weight: 700; color: Theme.ink; wrap: word-wrap; }
                                        Text { text: root.health-fix; font-size: 13px; color: Theme.body; wrap: word-wrap; }
                                    }
                                    if root.health-action != "": Btn { text: root.health-action; primary: true; h: 44px; r: 14px; pad: 18px; fs: 14px; y: (parent.height - self.height) / 2; clicked => { root.health-act(); } }
                                }
                            }
                            HorizontalLayout {
                                spacing: 18px;
                                height: 250px;
                                // the key card: peach gradient with the honeycomb
                                GradCard {
                                    horizontal-stretch: 1; preferred-width: 100px; min-width: 0px;
                                    r: 26px;
                                    angle: 140deg;
                                    from: #FFEDE2;
                                    to: rgb(255, 217, 195);
                                    has-glow: true;
                                    glow: #FFC6A2;
                                    gx: 0.85;
                                    gy: 0.2;
                                    gsize: 1.4;
                                    Comb { look: 2; size: 170px; x: parent.width - self.width - 16px; y: 20px; }
                                    VerticalLayout {
                                        padding: 28px;
                                        spacing: 20px;
                                        VerticalLayout {
                                            spacing: 14px;
                                            Text { text: "YOUR KEY"; font-size: 12px; font-weight: 700; letter-spacing: 1.4px; color: #8A2A08; }
                                            HorizontalLayout {
                                                spacing: 14px;
                                                alignment: start;
                                                KeyCap { label: root.bound-label; fs: 32px; r: 18px; padx: 22px; height: 58px; drop: 6px; capturing: root.capturing; clicked => { root.capture(); } }
                                                Text { text: root.bound-mode == 0 ? "Tap to start, tap again to stop.\nOr hold while you speak." : root.bound-mode == 1 ? "Tap to start.\nTap again to stop." : "Hold while you speak.\nLet go to type."; font-size: 15px; color: Theme.body; vertical-alignment: center; }
                                            }
                                            if root.key-note != "": Text { text: root.key-note; font-size: 12px; color: #8A2A08; wrap: word-wrap; }
                                        }
                                        Rectangle { vertical-stretch: 1; }
                                        HorizontalLayout {
                                            spacing: 10px;
                                            alignment: start;
                                            Btn { primary: true; text: root.state == "listening" ? "Stop and type" : "Start talking"; icon: "M12 3a3 3 0 0 1 3 3v5a3 3 0 0 1-6 0V6a3 3 0 0 1 3-3z M5 11a7 7 0 0 0 14 0 M12 18v3"; h: 48px; r: 16px; pad: 22px; fs: 15px; enabled: root.state != "finishing"; clicked => { root.toggle(); } }
                                            Btn { text: "Change key"; fill: #FFFFFF; fs: 14px; pad: 18px; clicked => { root.page = 3; } }
                                        }
                                    }
                                }
                                Rectangle {
                                    horizontal-stretch: 1; preferred-width: 100px; min-width: 0px;
                                    border-radius: 26px;
                                    background: Theme.soft;
                                    VerticalLayout {
                                        padding: 24px;
                                        spacing: 10px;
                                        Text { text: "Try it here"; font-family: Theme.head; font-weight: 800; font-size: 21px; color: Theme.ink; }
                                        Field { multiline: true; vertical-stretch: 1; border-radius: 18px; placeholder: "Click here, press " + root.key-label + " and say: remind me to call the bank at four about my card."; }
                                        Text { text: "Just for practice. Nothing here is saved."; font-size: 12px; color: Theme.muted; }
                                    }
                                }
                            }
                            VerticalLayout {
                                spacing: 14px;
                                HorizontalLayout {
                                    H2 { text: "Speech pack"; horizontal-stretch: 1; preferred-width: 100px; min-width: 0px; }
                                    Link { text: "All models"; clicked => { root.page = 2; } }
                                }
                                HorizontalLayout {
                                    spacing: 14px;
                                    for p[i] in root.packs: PackCard { horizontal-stretch: 1; preferred-width: 100px; min-width: 0px; pack: p; index: i; selected: i == root.pack-index; clicked => { root.pick-pack(i); } }
                                }
                            }
                            VerticalLayout {
                                spacing: 14px;
                                HorizontalLayout {
                                    H2 { text: "Recently typed"; horizontal-stretch: 1; preferred-width: 100px; min-width: 0px; }
                                    Link { text: "History"; clicked => { root.page = 1; } }
                                }
                                if root.recent.length == 0: Rectangle {
                                    height: 64px;
                                    border-radius: 18px;
                                    background: Theme.soft;
                                    Text { x: 18px; text: "Nothing yet. What you dictate shows here, so you can copy it again."; font-size: 14px; color: Theme.muted; vertical-alignment: center; height: parent.height; }
                                }
                                for r in root.recent: Rectangle {
                                    height: 64px;
                                    border-radius: 18px;
                                    background: Theme.soft;
                                    HorizontalLayout {
                                        padding-left: 12px;
                                        padding-right: 14px;
                                        spacing: 14px;
                                        KindChip { kind: r.kind; w: 64px; y: (parent.height - self.height) / 2; }
                                        Text { text: r.line; font-size: 14px; color: Theme.ink; overflow: elide; vertical-alignment: center; horizontal-stretch: 1; min-width: 0px; preferred-width: 1px; }
                                        Text { text: r.when; font-size: 12px; color: Theme.muted; vertical-alignment: center; }
                                        IconBtn { icon: "M8 8h12v12H8z M16 8V5a1 1 0 0 0-1-1H5a1 1 0 0 0-1 1v10a1 1 0 0 0 1 1h3"; y: (parent.height - self.height) / 2; clicked => { root.copy(r.text); } }
                                    }
                                }
                            }
                        }

                        // ---------------- History (History.dc.html)
                        if root.page == 1: VerticalLayout {
                            padding-top: 40px;
                            padding-left: 44px;
                            padding-right: 44px;
                            padding-bottom: 44px;
                            spacing: 22px;
                            alignment: start;
                            HorizontalLayout {
                                spacing: 16px;
                                VerticalLayout {
                                    spacing: 8px;
                                    horizontal-stretch: 1; preferred-width: 100px; min-width: 0px;
                                    H1 { text: "History"; }
                                    Text { text: root.history-subtitle; font-size: 15px; color: Theme.muted; wrap: word-wrap; }
                                }
                                VerticalLayout { alignment: end; Field { search: true; width: 320px; height: 48px; placeholder: "Search what you said"; edited(t) => { root.history-search(t); } } }
                            }
                            HorizontalLayout {
                                spacing: 8px;
                                alignment: start;
                                for f[i] in ["All", "Today", "Lists", "Emails"]: Rectangle {
                                    height: 40px;
                                    width: ft.preferred-width + 32px + 4px;
                                    border-radius: 20px;
                                    background: root.history-filter == i ? Theme.ink : #FFFFFF;
                                    border-width: 2px;
                                    border-color: root.history-filter == i ? Theme.ink : Theme.line;
                                    ft := Text { text: f; font-size: 14px; font-weight: 700; color: root.history-filter == i ? #FFFFFF : Theme.ink; horizontal-alignment: center; vertical-alignment: center; width: parent.width; height: parent.height; }
                                    TouchArea { mouse-cursor: MouseCursor.pointer; clicked => { root.history-filter = i; root.history-filter-changed(i); } }
                                }
                                Rectangle { horizontal-stretch: 1; preferred-width: 100px; min-width: 0px; }
                                if root.history-total > 0: Link { text: "Clear all"; color: Theme.muted; vertical-alignment: center; clicked => { root.clear-history(); } }
                            }
                            if root.history-total == 0: Rectangle {
                                height: 120px;
                                border-radius: 20px;
                                background: Theme.soft;
                                VerticalLayout {
                                    alignment: center;
                                    spacing: 6px;
                                    Text { text: "No dictations yet"; font-size: 15px; font-weight: 700; color: Theme.ink; horizontal-alignment: center; }
                                    Text { text: "Everything you dictate appears here, so you can copy it again."; font-size: 14px; color: Theme.muted; horizontal-alignment: center; }
                                }
                            }
                            if root.history-total > 0 && root.history.length == 0: Text { text: "Nothing matches."; font-size: 14px; color: Theme.muted; }
                            VerticalLayout {
                            spacing: 10px;
                            for r[i] in root.history: VerticalLayout {
                                spacing: 10px;
                                padding-top: r.first && i > 0 ? 12px : 0px;
                                if r.first: Text { text: r.day; font-size: 13px; font-weight: 700; letter-spacing: 1px; color: Theme.muted; }
                                Rectangle {
                                    border-radius: 20px;
                                    background: Theme.soft;
                                    min-height: art.preferred-height;
                                    art := HorizontalLayout {
                                        padding: 16px;
                                        spacing: 14px;
                                        VerticalLayout { alignment: start; KindChip { kind: r.kind; } }
                                        VerticalLayout {
                                            spacing: 6px;
                                            horizontal-stretch: 1; preferred-width: 100px; min-width: 0px;
                                            Text { text: r.text; font-size: 15px; color: Theme.ink; wrap: word-wrap; }
                                            Text { text: r.meta; font-size: 12px; color: Theme.muted; }
                                        }
                                        VerticalLayout {
                                            alignment: start;
                                            HorizontalLayout {
                                                spacing: 6px;
                                                IconBtn { icon: "M8 8h12v12H8z M16 8V5a1 1 0 0 0-1-1H5a1 1 0 0 0-1 1v10a1 1 0 0 0 1 1h3"; clicked => { root.copy(r.text); } }
                                                IconBtn { icon: "M4 7h16 M10 11v6 M14 11v6 M6 7l1 13h10l1-13 M9 7V4h6v3"; tint: Theme.danger; clicked => { root.history-delete(r.index); } }
                                            }
                                        }
                                    }
                                }
                            }
                            }
                        }

                        // ---------------- Dictionary (Dictionary.dc.html)
                        if root.page == 5: VerticalLayout {
                            padding-top: 40px;
                            padding-left: 44px;
                            padding-right: 44px;
                            padding-bottom: 44px;
                            spacing: 24px;
                            alignment: start;
                            VerticalLayout {
                                spacing: 8px;
                                H1 { text: "Dictionary"; }
                                Text { text: "Teach Voicio names and words it gets wrong. It fixes them every time."; font-size: 15px; color: Theme.muted; }
                            }
                            GradCard {
                                r: 22px;
                                from: Theme.soft;
                                to: Theme.soft;
                                has-glow: true;
                                glow: #FFE3D3;
                                gx: 0;
                                gy: 0;
                                gsize: 1.2;
                                height: 112px;
                                HorizontalLayout {
                                    padding: 16px;
                                    spacing: 10px;
                                    VerticalLayout {
                                        spacing: 6px;
                                        horizontal-stretch: 1; preferred-width: 100px; min-width: 0px;
                                        Text { text: "When I say"; font-size: 13px; font-weight: 700; color: Theme.ink; }
                                        phrase := Field { placeholder: "e.g. voice io"; height: 48px; }
                                    }
                                    VerticalLayout { alignment: end; horizontal-stretch: 0; width: 20px; Rectangle { height: 48px; Glyph { d: "M5 12h14 M13 6l6 6-6 6"; size: 20px; weight: 2.4px; tint: Theme.muted; y: 14px; } } }
                                    VerticalLayout {
                                        spacing: 6px;
                                        horizontal-stretch: 1; preferred-width: 100px; min-width: 0px;
                                        Text { text: "Type"; font-size: 13px; font-weight: 700; color: Theme.ink; }
                                        output := Field { placeholder: "e.g. Voicio"; height: 48px; accepted => { if phrase.text != "" { root.dict-add(phrase.text, output.text); phrase.text = ""; output.text = ""; } } }
                                    }
                                    VerticalLayout {
                                        alignment: end;
                                        Btn { text: "Add"; primary: true; h: 48px; r: 14px; pad: 20px; fs: 15px; clicked => { root.dict-add(phrase.text, output.text); phrase.text = ""; output.text = ""; } }
                                    }
                                }
                            }
                            if root.dict-rows.length == 0: Text { text: "No words yet. Add the first one above."; font-size: 14px; color: Theme.muted; }
                            VerticalLayout {
                            spacing: 10px;
                            // two columns of rule chips
                            for pair[k] in Math.ceil(root.dict-rows.length / 2): HorizontalLayout {
                                spacing: 10px;
                                for j in 2: Rectangle {
                                    property <int> n: k * 2 + j;
                                    horizontal-stretch: 1; preferred-width: 100px; min-width: 0px;
                                    height: 60px;
                                    border-radius: 18px;
                                    background: n < root.dict-rows.length ? Theme.soft : transparent;
                                    if n < root.dict-rows.length: HorizontalLayout {
                                        padding-left: 16px;
                                        padding-right: 10px;
                                        spacing: 8px;
                                        Rectangle {
                                            horizontal-stretch: 0;
                                            width: min(from-t.preferred-width, 200px);
                                            from-t := Text { text: root.dict-rows[n].phrase; font-size: 14px; color: Theme.muted; vertical-alignment: center; overflow: elide; width: parent.width; height: parent.height; }
                                            Rectangle { y: parent.height / 2; height: 1px; width: min(from-t.preferred-width, parent.width); background: rgba(94, 87, 75, 0.5); }
                                        }
                                        Glyph { d: "M5 12h14 M13 6l6 6-6 6"; size: 14px; weight: 2.4px; tint: Theme.muted; y: (parent.height - self.height) / 2; }
                                        Chip {
                                            text: root.dict-rows[n].output == "" ? "(nothing)" : root.dict-rows[n].output;
                                            bg: Math.mod(n, 4) == 0 ? #FFE3D3 : Math.mod(n, 4) == 1 ? #EAF8D2 : Math.mod(n, 4) == 2 ? #E2F0FF : #FFF0B8;
                                            fs: 14px; padx: 10px; h: 28px; y: (parent.height - self.height) / 2;
                                        }
                                        Rectangle { horizontal-stretch: 1; preferred-width: 100px; min-width: 0px; }
                                        IconBtn { icon: "M6 6l12 12 M18 6L6 18"; size: 36px; y: (parent.height - self.height) / 2; clicked => { root.dict-remove(n); } }
                                    }
                                }
                            }
                            }
                            Rectangle {
                                border-radius: 20px;
                                background: Theme.blue-soft;
                                min-height: tip.preferred-height;
                                tip := HorizontalLayout {
                                    padding-top: 16px;
                                    padding-bottom: 16px;
                                    padding-left: 18px;
                                    padding-right: 18px;
                                    spacing: 14px;
                                    Tile { size: 40px; glyph: 18px; background: #FFFFFF; tint: Theme.blue-ink; icon: "M12 3l2.5 5.5L20 9l-4 4 1 6-5-3-5 3 1-6-4-4 5.5-.5z"; y: (parent.height - self.height) / 2; }
                                    Text { text: "Tip: write the left side the way Voicio mishears it, for example “pre ya” for Priya. Copy it straight from History."; font-size: 14px; color: Theme.ink; wrap: word-wrap; vertical-alignment: center; horizontal-stretch: 1; preferred-width: 100px; min-width: 0px; }
                                }
                            }
                        }

                        // ---------------- Models (Models.dc.html)
                        if root.page == 2: VerticalLayout {
                            padding-top: 40px;
                            padding-left: 44px;
                            padding-right: 44px;
                            padding-bottom: 44px;
                            spacing: 30px;
                            alignment: start;
                            VerticalLayout {
                                spacing: 8px;
                                H1 { text: "Models"; }
                                Text { text: "Everything runs on this computer. Switch packs or add a language in one click."; font-size: 15px; color: Theme.muted; wrap: word-wrap; }
                            }
                            if root.models-message != "": Text { text: root.models-message; font-size: 14px; color: Theme.danger; wrap: word-wrap; }
                            VerticalLayout {
                                spacing: 14px;
                                H2 { text: "Speech pack"; }
                                HorizontalLayout {
                                    spacing: 14px;
                                    for p[i] in root.packs: GradCard {
                                        horizontal-stretch: 1; preferred-width: 100px; min-width: 0px;
                                        r: 26px;
                                        from: i == 0 ? #F3FCE4 : i == 1 ? #FFF1E8 : #EEF6FF;
                                        to: i == 0 ? #DDF3B8 : i == 1 ? #FFD7C0 : #CFE5FF;
                                        ring: i == root.pack-index ? 3px : 0px;
                                        ring-color: Theme.ink;
                                        min-height: pc.preferred-height;
                                        pc := VerticalLayout {
                                            padding: 22px;
                                            spacing: 12px;
                                            HorizontalLayout {
                                                Text { text: p.name; font-family: Theme.head; font-size: 28px; font-weight: 800; letter-spacing: -0.6px; color: Theme.ink; horizontal-stretch: 1; preferred-width: 100px; min-width: 0px; }
                                                if i == root.pack-index: Chip { text: "In use"; bg: Theme.ink; fg: #FFFFFF; h: 26px; y: (parent.height - self.height) / 2; }
                                            }
                                            Text { text: p.line; font-size: 14px; color: Theme.body; wrap: word-wrap; min-height: 40px; }
                                            Rectangle { height: 1.5px; background: rgba(22, 19, 15, 0.1); }
                                            for m in p.parts: HorizontalLayout {
                                                Text { text: m.name; font-size: 13px; font-weight: 600; color: Theme.ink; horizontal-stretch: 1; preferred-width: 100px; min-width: 0px; }
                                                Text { text: m.size; font-size: 13px; color: Theme.body; }
                                            }
                                            Rectangle { vertical-stretch: 1; }
                                            if p.busy: Bar { value: p.progress; }
                                            Rectangle {
                                                height: 46px;
                                                border-radius: 15px;
                                                background: i == root.pack-index ? rgba(255, 255, 255, 0.7) : p.have || p.busy ? (bta.has-hover ? #FBF8F2 : #FFFFFF) : (bta.has-hover ? #2A251E : Theme.ink);
                                                Text {
                                                    text: i == root.pack-index ? (p.have ? "Using this pack" : p.busy ? "Downloading " + Math.round(p.progress * 100) + "%" : "Download " + p.get) : p.busy ? "Downloading " + Math.round(p.progress * 100) + "%" : p.have ? "Use " + p.name : "Download " + p.get;
                                                    font-size: 14px; font-weight: 700;
                                                    color: i == root.pack-index ? Theme.body : p.have || p.busy ? Theme.ink : #FFFFFF;
                                                    horizontal-alignment: center; vertical-alignment: center; width: parent.width; height: parent.height;
                                                }
                                                bta := TouchArea {
                                                    mouse-cursor: MouseCursor.pointer;
                                                    clicked => { if p.busy { root.cancel-install(); } else if i != root.pack-index || !p.have { root.pick-pack(i); } }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            VerticalLayout {
                                spacing: 14px;
                                HorizontalLayout {
                                    spacing: 12px;
                                    H2 { text: "Language models"; }
                                    Text { text: "Smart clean-up works in English. Other languages get punctuation from their own model."; font-size: 13px; color: Theme.muted; horizontal-alignment: right; vertical-alignment: center; wrap: word-wrap; horizontal-stretch: 1; preferred-width: 100px; min-width: 0px; }
                                }
                                for l in root.lang-rows: Rectangle {
                                    border-radius: 20px;
                                    background: Theme.soft;
                                    min-height: lr.preferred-height;
                                    lr := VerticalLayout {
                                        padding-top: 12px;
                                        padding-bottom: 12px;
                                        padding-left: 14px;
                                        padding-right: 14px;
                                        spacing: 8px;
                                        HorizontalLayout {
                                            spacing: 14px;
                                            Badge { mark: l.mark; background: l.tint; y: (parent.height - self.height) / 2; }
                                            VerticalLayout {
                                                alignment: center;
                                                spacing: 2px;
                                                horizontal-stretch: 1; preferred-width: 100px; min-width: 0px;
                                                HorizontalLayout {
                                                    spacing: 8px;
                                                    alignment: start;
                                                    Text { text: l.name; font-size: 15px; font-weight: 700; color: Theme.ink; overflow: elide; min-width: 0px; }
                                                    if l.experimental: Chip { text: "Experimental"; bg: Theme.yellow-soft; fg: Theme.yellow-ink; fs: 11px; h: 20px; padx: 8px; }
                                                }
                                                Text { text: l.sub; font-size: 13px; color: Theme.body; wrap: word-wrap; }
                                            }
                                            Text { text: l.size; font-size: 13px; color: Theme.body; vertical-alignment: center; min-width: 150px; }
                                            if l.installed && !l.builtin && !l.in-use && !l.busy: IconBtn { icon: "M4 7h16 M10 11v6 M14 11v6 M6 7l1 13h10l1-13 M9 7V4h6v3"; tint: Theme.danger; size: 42px; y: (parent.height - self.height) / 2; clicked => { root.remove(l.id); } }
                                            Rectangle {
                                                width: max(104px, lb.preferred-width + 32px);
                                                height: 42px;
                                                y: (parent.height - self.height) / 2;
                                                border-radius: 13px;
                                                background: l.in-use ? Theme.ink : l.busy || l.installed ? (lta.has-hover ? #FBF8F2 : #FFFFFF) : (lta.has-hover ? #E5480F : Theme.accent);
                                                opacity: !l.busy && !l.installed && root.models-busy ? 0.5 : 1;
                                                lb := Text { text: l.in-use ? "In use" : l.busy ? "Cancel" : l.installed ? "Use" : "Download"; font-size: 13px; font-weight: 700; color: l.in-use ? #FFFFFF : Theme.ink; horizontal-alignment: center; vertical-alignment: center; width: parent.width; height: parent.height; }
                                                lta := TouchArea {
                                                    mouse-cursor: MouseCursor.pointer;
                                                    clicked => {
                                                        if l.busy { root.cancel-install(); }
                                                        else if l.installed && !l.in-use { root.lang-use(l.id); }
                                                        else if !l.installed && !root.models-busy { root.install(l.id); }
                                                    }
                                                }
                                            }
                                        }
                                        if l.busy: HorizontalLayout {
                                            padding-left: 60px;
                                            spacing: 12px;
                                            Bar { value: l.progress; horizontal-stretch: 1; preferred-width: 100px; min-width: 0px; y: (parent.height - self.height) / 2; }
                                            Text { text: l.status; font-size: 12px; font-weight: 600; color: Theme.body; }
                                        }
                                    }
                                }
                            }
                        }

                        // ---------------- Settings (Settings.dc.html): changes apply at once
                        if root.page == 3: VerticalLayout {
                            padding-top: 40px;
                            padding-left: 44px;
                            padding-right: 44px;
                            padding-bottom: 44px;
                            spacing: 26px;
                            alignment: start;
                            VerticalLayout {
                                spacing: 8px;
                                H1 { text: "Settings"; }
                                Text { text: "Changes apply right away."; font-size: 15px; color: Theme.muted; }
                            }
                            // height-for-width does not travel up through layouts: size the row by its columns,
                            // or the page claims thousands of pixels of empty scroll
                            HorizontalLayout {
                                height: max(col-a.preferred-height, col-b.preferred-height);
                                spacing: 18px;
                                alignment: stretch;
                                col-a := VerticalLayout {
                                    spacing: 18px;
                                    horizontal-stretch: 1; preferred-width: 100px; min-width: 0px;
                                    alignment: start;
                                    Group {
                                        title: "Shortcut";
                                        tint: #E2F0FF;
                                        icon: "M3 7h18v11H3z M7 11h1 M11 11h1 M15 11h2 M7 14h10";
                                        Row {
                                            title: "Talk key";
                                            hint: root.key-note != "" ? root.key-note : "Click, then press the key you want.";
                                            KeyCap { label: root.key-label; height: 46px; fs: 18px; r: 14px; drop: 4px; padx: 18px; capturing: root.capturing; min-width: 76px; y: (parent.height - self.height) / 2 - 2px; clicked => { root.capture(); } }
                                        }
                                        VerticalLayout {
                                            padding: 16px;
                                            spacing: 10px;
                                            VerticalLayout {
                                                spacing: 3px;
                                                Text { text: "How it works"; font-size: 15px; font-weight: 700; color: Theme.ink; }
                                                Text { text: root.key-mode == 0 ? "Tap to start and stop, or hold while you speak." : root.key-mode == 1 ? "Tap once to start, again to stop." : "Listens only while the key is down."; font-size: 13px; color: Theme.muted; }
                                            }
                                            Seg { options: ["Tap or hold", "Tap", "Hold"]; index <=> root.key-mode; picked => { root.apply(); } }
                                        }
                                    }
                                    Group {
                                        title: "Speed and memory";
                                        tint: #EAF8D2;
                                        icon: "M13 3L5 14h6l-1 7 8-11h-6z";
                                        VerticalLayout {
                                            padding: 16px;
                                            spacing: 10px;
                                            VerticalLayout {
                                                spacing: 3px;
                                                Text { text: "Processor use"; font-size: 15px; font-weight: 700; color: Theme.ink; }
                                                Text { text: root.cpu-index == 0 ? "One core. Coolest and quietest." : root.cpu-index == 1 ? "Two cores. Quick with a light load. Recommended." : "All cores. Text appears soonest."; font-size: 13px; color: Theme.muted; }
                                            }
                                            Seg { options: ["Light", "Balanced", "Fast"]; index <=> root.cpu-index; picked => { root.apply(); } }
                                        }
                                        Rectangle { height: 1.5px; background: Theme.page; }
                                        Row {
                                            title: "When memory runs low";
                                            hint: "The capsule tells you, and Voicio skips clean-up instead of slowing your computer.";
                                            last: true;
                                            Chip { text: "Automatic"; bg: Theme.green-soft; fg: Theme.green-ink; h: 28px; padx: 12px; y: (parent.height - self.height) / 2; }
                                        }
                                    }
                                    Group {
                                        title: "Microphone and history";
                                        tint: #FFF0B8;
                                        icon: "M12 3a3 3 0 0 1 3 3v5a3 3 0 0 1-6 0V6a3 3 0 0 1 3-3z M5 11a7 7 0 0 0 14 0 M12 18v3";
                                        VerticalLayout {
                                            padding: 16px;
                                            spacing: 10px;
                                            Text { text: "Microphone"; font-size: 15px; font-weight: 700; color: Theme.ink; }
                                            for m[i] in root.mic-names: Rectangle {
                                                height: 44px;
                                                border-radius: 12px;
                                                background: root.mic-index == i ? Theme.soft : transparent;
                                                border-width: 2px;
                                                border-color: root.mic-index == i ? Theme.ink : Theme.line;
                                                HorizontalLayout {
                                                    padding-left: 12px;
                                                    padding-right: 12px;
                                                    spacing: 10px;
                                                    Rectangle {
                                                        width: 18px; height: 18px; y: (parent.height - self.height) / 2; border-radius: 9px;
                                                        border-width: 2px; border-color: root.mic-index == i ? Theme.ink : #C9C1B2;
                                                        Rectangle { x: 4px; y: 4px; width: 6px; height: 6px; border-radius: 3px; background: root.mic-index == i ? Theme.accent : transparent; }
                                                    }
                                                    Text { text: m; font-size: 14px; font-weight: 600; color: Theme.ink; vertical-alignment: center; overflow: elide; horizontal-stretch: 1; preferred-width: 100px; min-width: 0px; }
                                                }
                                                TouchArea { mouse-cursor: MouseCursor.pointer; clicked => { root.mic-index = i; root.mic-picked(i); } }
                                            }
                                        }
                                        Rectangle { height: 1.5px; background: Theme.page; }
                                        Row {
                                            title: "Keep history";
                                            hint: root.history-days == 0 ? "Nothing is written to disk." : "Text only, never audio, for " + root.history-days + " days.";
                                            HorizontalLayout {
                                                spacing: 6px;
                                                y: (parent.height - self.height) / 2;
                                                height: 40px;
                                                for d in [0, 7, 30, 90]: Rectangle {
                                                    width: 46px;
                                                    border-radius: 12px;
                                                    background: root.history-days == d ? Theme.ink : Theme.page;
                                                    Text { text: d == 0 ? "Off" : d + "d"; font-size: 13px; font-weight: 700; color: root.history-days == d ? #FFFFFF : Theme.ink; horizontal-alignment: center; vertical-alignment: center; width: parent.width; height: parent.height; }
                                                    TouchArea { mouse-cursor: MouseCursor.pointer; clicked => { root.history-days = d; root.apply(); } }
                                                }
                                            }
                                        }
                                        Row {
                                            title: "Start Voicio when I log in";
                                            hint: "So your key works right after you sign in.";
                                            last: true;
                                            Toggle { checked <=> root.launch-at-login; y: (parent.height - self.height) / 2; toggled => { root.apply(); } }
                                        }
                                    }
                                }
                                col-b := VerticalLayout {
                                    spacing: 18px;
                                    horizontal-stretch: 1; preferred-width: 100px; min-width: 0px;
                                    alignment: start;
                                    Group {
                                        title: "Writing";
                                        tint: #FFE3D3;
                                        icon: "M4 7h16 M4 12h10 M4 17h13";
                                        Row { title: "Show words as I talk"; hint: "Live preview in the capsule. Off saves the most power."; Toggle { checked <=> root.live-text; y: (parent.height - self.height) / 2; toggled => { root.apply(); } } }
                                        Row { title: "Clean up my text"; hint: "Punctuation, fillers and numbers fixed when you finish."; Toggle { checked <=> root.cleanup; y: (parent.height - self.height) / 2; toggled => { root.apply(); } } }
                                        Row { title: "Spoken commands"; hint: "Start with “as a list”, “email mode” or “make it formal”."; Toggle { checked <=> root.voice-commands; y: (parent.height - self.height) / 2; toggled => { root.apply(); } } }
                                        Row { title: "Ignore background noise"; hint: "Fans, hum and hiss are filtered out."; Toggle { checked <=> root.noise-reduction; y: (parent.height - self.height) / 2; toggled => { root.apply(); } } }
                                        Row { title: "Pause music while I talk"; hint: "Pauses players that are playing, and resumes them when you finish."; Toggle { checked <=> root.pause-media; y: (parent.height - self.height) / 2; toggled => { root.apply(); } } }
                                        VerticalLayout {
                                            padding: 16px;
                                            spacing: 10px;
                                            VerticalLayout {
                                                spacing: 3px;
                                                Text { text: "Tone"; font-size: 15px; font-weight: 700; color: Theme.ink; }
                                                Text { text: root.styling-index == 0 ? "All lowercase, as you said it." : root.styling-index == 1 ? "Your phrasing; I and contractions capitalised." : root.styling-index == 2 ? "Standard written English." : "Full sentences, contractions written out."; font-size: 13px; color: Theme.muted; }
                                            }
                                            Seg { options: ["Casual", "Relaxed", "Standard", "Formal"]; index <=> root.styling-index; picked => { root.apply(); } }
                                        }
                                        Rectangle { height: 1.5px; background: Theme.page; }
                                        Row { title: "Always make lists"; hint: "Turn spoken lists into bullet points."; Toggle { checked <=> root.lists-on; y: (parent.height - self.height) / 2; toggled => { root.apply(); } } }
                                        Row { title: "Always write as an email"; hint: "Greeting, body and sign-off laid out."; last: true; Toggle { checked <=> root.email-on; y: (parent.height - self.height) / 2; toggled => { root.apply(); } } }
                                    }
                                    Group {
                                        title: "Cloud";
                                        note: "optional";
                                        tint: #EBE4FF;
                                        icon: "M7 18a5 5 0 0 1-.6-9.96A6 6 0 0 1 18 9a4.5 4.5 0 0 1-.5 9z";
                                        CloudPanel {
                                            rows: root.cloud-rows;
                                            index <=> root.cloud-index;
                                            default: root.cloud-default;
                                            mode <=> root.cloud-mode;
                                            consent <=> root.cloud-consent;
                                            region <=> root.cloud-region;
                                            status: root.cloud-status;
                                            status-kind: root.cloud-status-kind;
                                            excluded: root.cloud-excluded;
                                            featured: root.cloud-featured;
                                            pick(i) => { root.cloud-pick(i); }
                                            model(i, m) => { root.cloud-model(i, m); }
                                            add-key(i, k, r) => { root.cloud-add-key(i, k, r); }
                                            set-default(i) => { root.cloud-set-default(i); }
                                            remove-key(i) => { root.cloud-remove-key(i); }
                                            mode-changed(m) => { root.cloud-mode-changed(m); }
                                            live(i, on) => { root.cloud-live(i, on); }
                                        }
                                    }
                                }
                            }
                        }

                        // ---------------- About (About.dc.html)
                        if root.page == 4: VerticalLayout {
                            padding-top: 40px;
                            padding-left: 44px;
                            padding-right: 44px;
                            padding-bottom: 44px;
                            spacing: 22px;
                            alignment: start;
                            GradCard {
                                r: 26px;
                                from: Theme.soft;
                                to: Theme.soft;
                                has-glow: true;
                                glow: #FFD7C0;
                                gx: 1;
                                gy: 0;
                                gsize: 1.2;
                                height: 160px;
                                HorizontalLayout {
                                    padding: 28px;
                                    spacing: 24px;
                                    Image { source: @image-url("../ui/voicio.svg"); width: 104px; height: 104px; y: (parent.height - self.height) / 2; }
                                    VerticalLayout {
                                        alignment: center;
                                        spacing: 6px;
                                        horizontal-stretch: 1; preferred-width: 100px; min-width: 0px;
                                        H1 { text: "voicio"; }
                                        Text { text: "Version " + root.version + " · beta · free and open source (GPL-3.0)"; font-size: 15px; color: Theme.body; }
                                    }
                                    HorizontalLayout {
                                        spacing: 10px;
                                        alignment: center;
                                        Btn { text: "Run setup again"; primary: true; h: 46px; r: 15px; pad: 18px; fs: 14px; y: (parent.height - self.height) / 2; clicked => { root.go(0); } }
                                        Btn { text: "Privacy policy"; fill: #FFFFFF; h: 46px; r: 15px; pad: 18px; fs: 14px; y: (parent.height - self.height) / 2; clicked => { root.open-url("https://github.com/derpx06/voicio/blob/main/PRIVACY.md"); } }
                                    }
                                }
                            }
                            HorizontalLayout {
                                spacing: 14px;
                                Rectangle {
                                    horizontal-stretch: 1; preferred-width: 100px; min-width: 0px;
                                    border-radius: 22px;
                                    background: Theme.green-soft;
                                    min-height: pv.preferred-height;
                                    pv := VerticalLayout {
                                        padding: 22px;
                                        spacing: 10px;
                                        alignment: start;
                                        Text { text: "Your privacy"; font-family: Theme.head; font-size: 20px; font-weight: 800; color: Theme.ink; }
                                        Text { text: "Speech is turned into text on this computer. No account, no analytics. Audio only leaves if you switch on a cloud provider yourself."; font-size: 14px; color: #23400A; wrap: word-wrap; }
                                    }
                                }
                                Rectangle {
                                    horizontal-stretch: 1; preferred-width: 100px; min-width: 0px;
                                    border-radius: 22px;
                                    background: Theme.blue-soft;
                                    min-height: hc.preferred-height;
                                    hc := VerticalLayout {
                                        padding: 22px;
                                        spacing: 10px;
                                        alignment: start;
                                        Text { text: "Something not working?"; font-family: Theme.head; font-size: 20px; font-weight: 800; color: Theme.ink; }
                                        Text { text: "A health check looks at your microphone, shortcut, typing and models, and tells you what to fix."; font-size: 14px; color: #173A68; wrap: word-wrap; }
                                        if root.health-note != "": Text { text: root.health-note; font-size: 14px; font-weight: 700; color: root.health-problem != "" ? Theme.danger : Theme.green-ink; wrap: word-wrap; }
                                        HorizontalLayout {
                                            alignment: start;
                                            spacing: 10px;
                                            Btn { text: "Run health check"; fill: #FFFFFF; h: 42px; r: 13px; pad: 16px; fs: 13px; fw: 700; clicked => { root.health-check(); } }
                                            if root.health-action != "": Btn { text: root.health-action; primary: true; h: 42px; r: 13px; pad: 16px; fs: 13px; clicked => { root.health-act(); } }
                                        }
                                    }
                                }
                            }
                            Rectangle {
                                border-radius: 22px;
                                background: Theme.soft;
                                min-height: cr.preferred-height;
                                cr := VerticalLayout {
                                    padding: 22px;
                                    spacing: 12px;
                                    Text { text: "Built on"; font-family: Theme.head; font-size: 20px; font-weight: 800; color: Theme.ink; }
                                    for row[k] in Math.ceil(root.credits.length / 3): HorizontalLayout {
                                        spacing: 8px;
                                        for j in 3: Rectangle {
                                            property <int> n: k * 3 + j;
                                            horizontal-stretch: 1; preferred-width: 100px; min-width: 0px;
                                            height: 58px;
                                            border-radius: 16px;
                                            background: n < root.credits.length ? #FFFFFF : transparent;
                                            if n < root.credits.length: VerticalLayout {
                                                padding-left: 14px;
                                                padding-right: 14px;
                                                alignment: center;
                                                spacing: 2px;
                                                Text { text: root.credits[n].name; font-size: 14px; font-weight: 700; color: Theme.ink; overflow: elide; }
                                                Text { text: root.credits[n].detail; font-size: 12px; color: Theme.muted; overflow: elide; }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        }
                    }
                    }
                }
            }
        }

        // ---------------- First-run setup (Onb1..Onb6): a full-window layer, six steps
        if root.onb-step >= 0: Rectangle {
            width: parent.width;
            height: parent.height;
            background: Theme.page;
            TouchArea { } // nothing below reacts while setup is open
            VerticalLayout {
                OnbHeader {
                    step: root.onb-step;
                    packs: root.packs;
                    pack: root.onb-pack;
                    recommended: root.recommended-pack;
                    open <=> root.chip-open;
                }
                HorizontalLayout {
                    alignment: center;
                    vertical-stretch: 1;
                    padding-top: 8px;
                    padding-left: 24px;
                    padding-right: 24px;
                    padding-bottom: 24px;
                    OnbCard {
                        width: min(1072px, parent.width - 48px);
                        // ---- 1 Welcome
                        if root.onb-step == 0: HorizontalLayout {
                            padding: 18px;
                            spacing: 18px;
                            VerticalLayout {
                                horizontal-stretch: 1; preferred-width: 100px; min-width: 0px;
                                padding-top: 38px;
                                padding-left: 30px;
                                padding-right: 30px;
                                padding-bottom: 26px;
                                spacing: 22px;
                                VerticalLayout {
                                    H1 { text: "Hi. Let's get you"; font-size: 52px; letter-spacing: -1.8px; }
                                    H1 { text: "talking."; font-size: 52px; letter-spacing: -1.8px; color: Theme.accent; }
                                }
                                Text { text: "Press one key, say what you mean, and Voicio types it into whatever app you're in. Setup takes about two minutes."; font-size: 17px; color: Theme.body; wrap: word-wrap; }
                                for p in [
                                    { t: "Works in every app", x: "Email, chat, docs, code: anywhere you can type.", d: "M4 6h16v12H4z M4 10h16", c: #FFE3D3 },
                                    { t: "Stays on this computer", x: "Speech is recognised here. No account, no upload.", d: "M6 11h12v9H6z M9 11V8a3 3 0 0 1 6 0v3", c: #EAF8D2 },
                                    { t: "Writes it up nicely", x: "Punctuation, no “um”s, and lists when you ask.", d: "M4 7h16 M4 12h10 M4 17h13", c: #E2F0FF },
                                ]: HorizontalLayout {
                                    spacing: 14px;
                                    Tile { size: 40px; glyph: 18px; icon: p.d; background: p.c; y: (parent.height - self.height) / 2; }
                                    VerticalLayout {
                                        alignment: center;
                                        spacing: 2px;
                                        Text { text: p.t; font-size: 15px; font-weight: 700; color: Theme.ink; }
                                        Text { text: p.x; font-size: 14px; color: Theme.muted; }
                                    }
                                }
                                Rectangle { vertical-stretch: 1; }
                                HorizontalLayout {
                                    spacing: 14px;
                                    alignment: start;
                                    Btn { text: "Let's go"; primary: true; h: 52px; r: 18px; pad: 26px; fs: 16px; icon: "M5 12h14 M13 6l6 6-6 6"; clicked => { root.go(1); } }
                                    Text { text: "Free and open source."; font-size: 13px; color: Theme.muted; vertical-alignment: center; }
                                    Rectangle { horizontal-stretch: 1; preferred-width: 100px; min-width: 0px; }
                                    Link { text: "Skip setup"; font-size: 13px; vertical-alignment: center; clicked => { root.onb-finish(); } }
                                }
                            }
                            Panel {
                                horizontal-stretch: 1; preferred-width: 100px; min-width: 0px;
                                from: #FFF1E8; to: #FFDCC8; glow: #FFC6A2; gx: 0.7; gy: 0.3;
                                // a mock window with the capsule underneath
                                Rectangle {
                                    x: (parent.width - self.width) / 2;
                                    y: parent.height / 2 - 110px;
                                    width: min(420px, parent.width - 60px);
                                    height: 150px;
                                    border-radius: 20px;
                                    background: #FFFFFF;
                                    clip: true;
                                    drop-shadow-blur: 50px;
                                    drop-shadow-offset-y: 24px;
                                    drop-shadow-color: rgba(120, 50, 10, 0.18);
                                    Rectangle {
                                        height: 34px; y: 0px; background: Theme.soft; border-top-left-radius: 20px; border-top-right-radius: 20px;
                                        HorizontalLayout { padding-left: 14px; spacing: 6px; alignment: start; for i in 3: Rectangle { width: 10px; height: 10px; y: 12px; border-radius: 5px; background: Theme.line2; } }
                                    }
                                    VerticalLayout {
                                        y: 34px;
                                        height: parent.height - 34px;
                                        padding-left: 22px;
                                        padding-right: 22px;
                                        padding-top: 20px;
                                        spacing: 10px;
                                        alignment: start;
                                        Text { text: "Note to self"; font-size: 12px; font-weight: 700; color: Theme.muted; }
                                        Text { text: "Pick up the parcel, then call the plumber about Thursday."; font-size: 16px; color: Theme.ink; wrap: word-wrap; }
                                    }
                                }
                                Rectangle {
                                    x: (parent.width - self.width) / 2;
                                    y: parent.height - 52px - 42px;
                                    width: cap-row.preferred-width;
                                    height: 42px;
                                    border-radius: 21px;
                                    background: Theme.ink;
                                    drop-shadow-blur: 30px;
                                    drop-shadow-offset-y: 14px;
                                    drop-shadow-color: rgba(40, 20, 0, 0.3);
                                    cap-row := HorizontalLayout {
                                        padding-left: 12px;
                                        padding-right: 16px;
                                        spacing: 10px;
                                        Comb { size: 30px; y: (parent.height - self.height) / 2; }
                                        Text { text: "then call the plumber"; font-size: 13px; font-weight: 500; color: #F7F3EA; vertical-alignment: center; }
                                    }
                                }
                            }
                        }

                        // ---- 2 Microphone
                        if root.onb-step == 1: HorizontalLayout {
                            padding: 18px;
                            spacing: 18px;
                            VerticalLayout {
                                horizontal-stretch: 1; preferred-width: 100px; min-width: 0px;
                                padding-top: 38px;
                                padding-left: 30px;
                                padding-right: 30px;
                                padding-bottom: 26px;
                                spacing: 22px;
                                VerticalLayout {
                                    vertical-stretch: 0;
                                    alignment: start;
                                    Eyebrow { text: "STEP 2 · MICROPHONE"; }
                                    Rectangle { height: 10px; }
                                    H1 { text: "Say hello."; }
                                    Rectangle { height: 12px; }
                                    Text { text: "Talk the way you normally would. We'll check that Voicio hears you clearly."; font-size: 16px; color: Theme.body; wrap: word-wrap; }
                                }
                                VerticalLayout {
                                    spacing: 8px;
                                    Text { text: "Microphone"; font-size: 14px; font-weight: 700; color: Theme.ink; }
                                    Rectangle {
                                        height: 48px;
                                        border-radius: 14px;
                                        border-width: 2px;
                                        border-color: Theme.line2;
                                        background: #FFFFFF;
                                        HorizontalLayout {
                                            padding-left: 14px;
                                            padding-right: 14px;
                                            Text { text: root.mic-names.length > root.mic-index ? root.mic-names[root.mic-index] : "System default"; font-size: 15px; color: Theme.ink; vertical-alignment: center; overflow: elide; horizontal-stretch: 1; preferred-width: 100px; min-width: 0px; }
                                            if root.mic-names.length > 1: Glyph { d: "M7 10l5 5 5-5"; size: 16px; weight: 2.4px; y: (parent.height - self.height) / 2; }
                                        }
                                        TouchArea {
                                            enabled: root.mic-names.length > 1;
                                            mouse-cursor: self.enabled ? MouseCursor.pointer : MouseCursor.default;
                                            clicked => { root.mic-index = Math.mod(root.mic-index + 1, root.mic-names.length); root.mic-picked(root.mic-index); }
                                        }
                                    }
                                }
                                Rectangle {
                                    border-radius: 18px;
                                    background: root.mic-state == "ok" ? Theme.green-soft : root.mic-state == "quiet" ? #FFF3D6 : root.mic-state == "none" ? Theme.danger-soft : Theme.soft;
                                    height: 78px;
                                    HorizontalLayout {
                                        padding-left: 18px;
                                        padding-right: 18px;
                                        spacing: 14px;
                                        Rectangle {
                                            width: 38px; height: 38px; y: (parent.height - self.height) / 2; border-radius: 19px;
                                            background: root.mic-state == "ok" ? Theme.green : root.mic-state == "quiet" ? #C98A00 : root.mic-state == "none" ? Theme.danger : Theme.muted;
                                            Glyph {
                                                d: root.mic-state == "ok" ? "M5 12l5 5 9-10" : root.mic-state == "waiting" ? "M12 3a3 3 0 0 1 3 3v5a3 3 0 0 1-6 0V6a3 3 0 0 1 3-3z M5 11a7 7 0 0 0 14 0" : "M12 7v6 M12 17v.5";
                                                tint: #FFFFFF; size: 18px; weight: 3px; x: 10px; y: 10px;
                                            }
                                        }
                                        VerticalLayout {
                                            alignment: center;
                                            spacing: 2px;
                                            horizontal-stretch: 1; preferred-width: 100px; min-width: 0px;
                                            Text {
                                                text: root.mic-state == "ok" ? "Loud and clear" : root.mic-state == "quiet" ? "Too quiet" : root.mic-state == "none" ? "No sound yet" : "Listening for you";
                                                font-size: 15px; font-weight: 700;
                                                color: root.mic-state == "ok" ? Theme.green-ink : root.mic-state == "quiet" ? #6B4E00 : root.mic-state == "none" ? Theme.danger : Theme.ink;
                                            }
                                            Text {
                                                text: root.mic-state == "ok" ? "Your voice is well above the room noise." : root.mic-state == "quiet" ? "Speak up or move closer, or raise the input volume." : root.mic-state == "none" ? "Check the microphone is plugged in and not muted." : "Say a few words. The bars should move.";
                                                font-size: 14px;
                                                color: root.mic-state == "ok" ? #3D5A1C : Theme.body;
                                                wrap: word-wrap;
                                            }
                                        }
                                    }
                                }
                                Text { text: "If it says “too quiet” or nothing happens: pick another microphone above, or raise the input volume in your system's sound settings. If you use a headset, check its mute switch."; font-size: 14px; color: Theme.body; wrap: word-wrap; }
                                Rectangle { vertical-stretch: 1; }
                                Footer { go-back => { root.go(0); } go-next => { root.go(2); } }
                            }
                            Panel {
                                horizontal-stretch: 1; preferred-width: 100px; min-width: 0px;
                                from: #F6FDEB; to: #E3F6C6; glow: #D7F5A8; gx: 0.5; gy: 0.45;
                                VerticalLayout {
                                    alignment: center;
                                    spacing: 26px;
                                    HorizontalLayout { alignment: center; Comb { look: 1; size: 230px; level: root.mic-level; } }
                                    HorizontalLayout {
                                        alignment: center;
                                        spacing: 5px;
                                        height: 40px;
                                        for v in root.mic-bars: Rectangle {
                                            width: 6px;
                                            Rectangle {
                                                height: 6px + v * 34px;
                                                y: parent.height - self.height;
                                                border-radius: 3px;
                                                background: v > 0.6 ? Theme.green : #A6D66B;
                                            }
                                        }
                                    }
                                    Text { text: root.mic-state == "ok" ? "Listening…" : root.mic-state == "none" ? "Waiting for sound" : "Listening…"; font-size: 15px; font-weight: 600; color: Theme.green-ink; horizontal-alignment: center; }
                                }
                            }
                        }

                        // ---- 3 Shortcut
                        if root.onb-step == 2: HorizontalLayout {
                            padding: 18px;
                            spacing: 18px;
                            VerticalLayout {
                                horizontal-stretch: 1; preferred-width: 100px; min-width: 0px;
                                padding-top: 38px;
                                padding-left: 30px;
                                padding-right: 30px;
                                padding-bottom: 26px;
                                spacing: 22px;
                                VerticalLayout {
                                    vertical-stretch: 0;
                                    alignment: start;
                                    Eyebrow { text: "STEP 3 · SHORTCUT"; }
                                    Rectangle { height: 10px; }
                                    H1 { text: "Your talk key."; }
                                    Rectangle { height: 12px; }
                                    Text { text: root.key-note != "" ? root.key-note : root.key-label + " is your talk key. Click it on the right and press a different key to change it."; font-size: 16px; color: Theme.body; wrap: word-wrap; }
                                }
                                for m[i] in [
                                    { t: "Tap or hold", x: "Tap to start and stop, or hold while you speak. Recommended." },
                                    { t: "Tap only", x: "Tap once to start, tap again to stop." },
                                    { t: "Hold only", x: "Voicio listens only while the key is down." },
                                ]: Rectangle {
                                    min-height: 70px;
                                    border-radius: 18px;
                                    border-width: 2.5px;
                                    border-color: root.key-mode == i ? Theme.ink : Theme.line;
                                    background: root.key-mode == i ? Theme.soft : #FFFFFF;
                                    HorizontalLayout {
                                        padding-left: 16px;
                                        padding-right: 16px;
                                        padding-top: 12px;
                                        padding-bottom: 12px;
                                        spacing: 14px;
                                        Rectangle {
                                            width: 22px; height: 22px; y: (parent.height - self.height) / 2; border-radius: 11px;
                                            border-width: 2px; border-color: root.key-mode == i ? Theme.ink : #C9C1B2;
                                            Rectangle { x: 6px; y: 6px; width: 10px; height: 10px; border-radius: 5px; background: root.key-mode == i ? Theme.accent : transparent; }
                                        }
                                        VerticalLayout {
                                            alignment: center;
                                            spacing: 2px;
                                            Text { text: m.t; font-size: 15px; font-weight: 700; color: Theme.ink; }
                                            Text { text: m.x; font-size: 14px; color: Theme.muted; wrap: word-wrap; }
                                        }
                                    }
                                    TouchArea { mouse-cursor: MouseCursor.pointer; clicked => { root.key-mode = i; root.apply(); } }
                                }
                                Rectangle { vertical-stretch: 1; }
                                Footer { go-back => { root.go(1); } go-next => { root.go(3); } }
                            }
                            Panel {
                                horizontal-stretch: 1; preferred-width: 100px; min-width: 0px;
                                from: #F2F8FF; to: #DCECFF; glow: #CFE5FF; gx: 0.5; gy: 0.4;
                                VerticalLayout {
                                    alignment: center;
                                    spacing: 22px;
                                    padding: 30px;
                                    HorizontalLayout {
                                        alignment: center;
                                        KeyCap { label: root.key-label; fs: 72px; ring: 3px; drop: 12px; r: 34px; padx: 30px; min-width: 168px; height: 150px; capturing: root.capturing; clicked => { root.capture(); } }
                                    }
                                    Text { text: "Click the key, then press another to change it"; font-size: 14px; font-weight: 600; color: Theme.blue-ink; horizontal-alignment: center; }
                                    HorizontalLayout {
                                        alignment: center;
                                        spacing: 8px;
                                        for a in ["F8", "F10", "Super+Space"]: Rectangle {
                                            height: 40px;
                                            width: at.preferred-width + 28px;
                                            border-radius: 12px;
                                            background: #FFFFFF;
                                            border-width: 1.5px;
                                            border-color: #9CCBFF;
                                            at := Text { text: a; font-size: 13px; font-weight: 700; color: Theme.ink; horizontal-alignment: center; vertical-alignment: center; width: parent.width; height: parent.height; }
                                            TouchArea { mouse-cursor: MouseCursor.pointer; clicked => { root.key-set(a); } }
                                        }
                                    }
                                }
                            }
                        }

                        // ---- 4 Pack
                        if root.onb-step == 3: VerticalLayout {
                            padding-top: 36px;
                            padding-left: 40px;
                            padding-right: 40px;
                            padding-bottom: 28px;
                            spacing: 18px;
                            HorizontalLayout {
                                spacing: 16px;
                                VerticalLayout {
                                    horizontal-stretch: 1; preferred-width: 100px; min-width: 0px;
                                    alignment: end;
                                    Eyebrow { text: "STEP 4 · SPEECH PACK"; }
                                    Rectangle { height: 10px; }
                                    H1 { text: "Pick how smart it is."; }
                                }
                                VerticalLayout {
                                    alignment: end;
                                    Text { text: root.mem-note; width: 360px; font-size: 15px; color: Theme.body; wrap: word-wrap; }
                                }
                            }
                            HorizontalLayout {
                                spacing: 16px;
                                for p[i] in root.packs: GradCard {
                                    horizontal-stretch: 1; preferred-width: 100px; min-width: 0px;
                                    y: i == root.onb-pack ? -4px : 0px;
                                    animate y { duration: 180ms; easing: ease; }
                                    r: 26px;
                                    from: i == 0 ? #F3FCE4 : i == 1 ? #FFF1E8 : #EEF6FF;
                                    to: i == 0 ? #DDF3B8 : i == 1 ? #FFD7C0 : #CFE5FF;
                                    ring: i == root.onb-pack ? 3px : 0px;
                                    ring-color: Theme.ink;
                                    drop-shadow-blur: i == root.onb-pack ? 30px : 0px;
                                    drop-shadow-offset-y: 16px;
                                    drop-shadow-color: rgba(60, 40, 10, 0.12);
                                    min-height: pk.preferred-height;
                                    pk := VerticalLayout {
                                        padding: 22px;
                                        spacing: 14px;
                                        HorizontalLayout {
                                            height: 38px;
                                            Text { text: p.name; font-family: Theme.head; font-size: 30px; font-weight: 800; letter-spacing: -0.8px; color: Theme.ink; horizontal-stretch: 1; preferred-width: 100px; min-width: 0px; }
                                            if i == root.onb-pack || i == root.recommended-pack: Chip {
                                                y: (parent.height - self.height) / 2;
                                                h: 26px;
                                                text: i == root.onb-pack ? "Selected" : "Recommended";
                                                bg: i == root.onb-pack ? Theme.ink : #FFFFFF;
                                                fg: i == root.onb-pack ? #FFFFFF : Theme.ink;
                                            }
                                        }
                                        Text { text: p.line; font-size: 15px; color: Theme.body; wrap: word-wrap; min-height: 44px; }
                                        VerticalLayout {
                                            spacing: 8px;
                                            HorizontalLayout { spacing: 10px; Text { text: "Accuracy"; width: 70px; font-size: 12px; font-weight: 600; color: Theme.body; vertical-alignment: center; } Segs { n: p.acc; fill: i == 0 ? rgb(78, 154, 19) : i == 1 ? #F2541B : #2F6FC0; } }
                                            HorizontalLayout { spacing: 10px; Text { text: "Speed"; width: 70px; font-size: 12px; font-weight: 600; color: Theme.body; vertical-alignment: center; } Segs { n: p.spd; fill: i == 0 ? rgb(78, 154, 19) : i == 1 ? #F2541B : #2F6FC0; } }
                                        }
                                        Rectangle { height: 1.5px; background: rgba(22, 19, 15, 0.1); }
                                        HorizontalLayout {
                                            Text { text: p.download + " download"; font-size: 13px; font-weight: 700; color: Theme.ink; horizontal-stretch: 1; preferred-width: 100px; min-width: 0px; }
                                            Text { text: p.memory + " memory"; font-size: 13px; font-weight: 700; color: Theme.ink; }
                                        }
                                    }
                                    TouchArea { mouse-cursor: MouseCursor.pointer; clicked => { root.onb-pack = i; root.onb-enter(3); } }
                                }
                            }
                            VerticalLayout {
                                spacing: 10px;
                                HorizontalLayout {
                                    spacing: 16px;
                                    VerticalLayout {
                                        spacing: 2px;
                                        horizontal-stretch: 1; preferred-width: 100px; min-width: 0px;
                                        Text { text: "Add a language model"; font-size: 15px; font-weight: 700; color: Theme.ink; }
                                        Text { text: "Optional. More languages are in Models any time."; font-size: 14px; color: Theme.muted; wrap: word-wrap; }
                                    }
                                    VerticalLayout { alignment: end; Link { text: "Or use a cloud provider"; clicked => { root.onb-finish(); root.page = 3; } } }
                                }
                                for row[k] in Math.ceil(Math.min(root.onb-langs.length, 3) / 3): HorizontalLayout {
                                    spacing: 10px;
                                    for j in 3: Rectangle {
                                        property <int> n: k * 3 + j;
                                        horizontal-stretch: 1; preferred-width: 100px; min-width: 0px;
                                        height: 80px;
                                        border-radius: 18px;
                                        border-width: n < root.onb-langs.length ? 2px : 0px;
                                        border-color: n < root.onb-langs.length && root.onb-langs[n].added ? Theme.ink : Theme.line;
                                        background: n < root.onb-langs.length && root.onb-langs[n].added ? Theme.soft : #FFFFFF;
                                        if n < root.onb-langs.length: HorizontalLayout {
                                            padding: 12px;
                                            spacing: 12px;
                                            Badge { mark: root.onb-langs[n].mark; background: root.onb-langs[n].tint; size: 44px; y: (parent.height - self.height) / 2; }
                                            VerticalLayout {
                                                alignment: center;
                                                spacing: 2px;
                                                horizontal-stretch: 1; preferred-width: 100px; min-width: 0px;
                                                Text { text: root.onb-langs[n].name; font-size: 14px; font-weight: 700; color: Theme.ink; wrap: word-wrap; }
                                                Text { text: root.onb-langs[n].sub; font-size: 12px; color: Theme.body; overflow: elide; }
                                                Text { text: root.onb-langs[n].size; font-size: 12px; font-weight: 600; color: Theme.muted; overflow: elide; }
                                            }
                                            Rectangle {
                                                width: ob.preferred-width + 20px;
                                                height: 36px;
                                                y: (parent.height - self.height) / 2;
                                                border-radius: 12px;
                                                background: root.onb-langs[n].added ? Theme.ink : Theme.accent;
                                                ob := Text { text: root.onb-langs[n].installed ? "Have it" : root.onb-langs[n].added ? "Added" : "Download"; font-size: 13px; font-weight: 700; color: root.onb-langs[n].added ? #FFFFFF : Theme.ink; horizontal-alignment: center; vertical-alignment: center; width: parent.width; height: parent.height; }
                                                TouchArea { enabled: !root.onb-langs[n].installed; mouse-cursor: MouseCursor.pointer; clicked => { root.onb-lang-toggle(root.onb-langs[n].id); } }
                                            }
                                        }
                                    }
                                }
                            }
                            Rectangle { vertical-stretch: 1; }
                            Footer { next: root.onb-total == "" ? "Continue" : "Download " + root.onb-total; go-back => { root.go(2); } go-next => { root.go(4); } }
                        }

                        // ---- 5 Download
                        if root.onb-step == 4: HorizontalLayout {
                            padding: 18px;
                            spacing: 18px;
                            VerticalLayout {
                                horizontal-stretch: 1; preferred-width: 100px; min-width: 0px;
                                padding-top: 30px;
                                padding-left: 30px;
                                padding-right: 30px;
                                padding-bottom: 26px;
                                spacing: 16px;
                                VerticalLayout {
                                    vertical-stretch: 0;
                                    alignment: start;
                                    Eyebrow { text: "STEP 5 · DOWNLOAD"; }
                                    Rectangle { height: 10px; }
                                    H1 { font-size: 40px; letter-spacing: -1.2px; text: root.dl-done ? "Your " + (root.packs.length > root.onb-pack ? root.packs[root.onb-pack].name : "") + " pack is ready." : "Getting your " + (root.packs.length > root.onb-pack ? root.packs[root.onb-pack].name : "") + " pack."; }
                                    Rectangle { height: 12px; }
                                    Text { text: "This happens once. After that, Voicio works with Wi-Fi off. Each file is checked before it's used."; font-size: 16px; color: Theme.body; wrap: word-wrap; }
                                }
                                VerticalLayout {
                                spacing: 8px;
                                for f in root.dl-rows: Rectangle {
                                    height: 64px;
                                    border-radius: 16px;
                                    background: Theme.soft;
                                    VerticalLayout {
                                        padding-top: 9px;
                                        padding-bottom: 10px;
                                        padding-left: 16px;
                                        padding-right: 16px;
                                        spacing: 6px;
                                        HorizontalLayout {
                                            spacing: 12px;
                                            Rectangle {
                                                width: 32px; height: 32px; border-radius: 11px; background: f.tint; y: (parent.height - self.height) / 2;
                                                Text { text: f.progress >= 1 ? "✓" : f.glyph; font-size: 14px; font-weight: 800; color: Theme.ink; horizontal-alignment: center; vertical-alignment: center; width: parent.width; height: parent.height; }
                                            }
                                            VerticalLayout {
                                                alignment: center;
                                                spacing: 1px;
                                                horizontal-stretch: 1; preferred-width: 100px; min-width: 0px;
                                                Text { text: f.name; font-size: 14px; font-weight: 700; color: Theme.ink; }
                                                Text { text: f.detail; font-size: 12px; color: Theme.muted; }
                                            }
                                            Text { text: f.state; font-size: 13px; font-weight: 700; color: Theme.ink; vertical-alignment: center; }
                                        }
                                        Bar { value: f.progress; fill: f.progress >= 1 ? Theme.green : Theme.accent; }
                                    }
                                }
                                }
                                if root.dl-error != "": Text { text: root.dl-error; font-size: 14px; color: Theme.danger; wrap: word-wrap; }
                                Rectangle { vertical-stretch: 1; }
                                HorizontalLayout {
                                    spacing: 12px;
                                    if root.dl-done: Btn { text: "Back"; clicked => { root.go(3); } }
                                    if !root.dl-done: Btn { text: root.dl-paused || root.dl-error != "" ? "Resume" : "Pause"; clicked => { if root.dl-paused || root.dl-error != "" { root.go(4); } else { root.dl-pause(); } } }
                                    Rectangle { horizontal-stretch: 1; preferred-width: 100px; min-width: 0px; }
                                    Btn { text: root.dl-done ? "Continue" : "Keep going while it downloads"; primary: true; h: 52px; r: 18px; pad: 26px; fs: 16px; clicked => { root.go(5); } }
                                }
                            }
                            Panel {
                                horizontal-stretch: 1; preferred-width: 100px; min-width: 0px;
                                from: #FFF1E8; to: #FFDCC8; glow: #FFC6A2; gx: 0.5; gy: 0.4;
                                VerticalLayout {
                                    alignment: center;
                                    spacing: 22px;
                                    HorizontalLayout { alignment: center; Comb { look: 3; size: 230px; fill: root.dl-done ? 1 : root.dl-fraction; } }
                                    Text { text: Math.round((root.dl-done ? 1 : root.dl-fraction) * 100) + "%"; font-family: Theme.head; font-size: 64px; font-weight: 800; letter-spacing: -2px; color: Theme.ink; horizontal-alignment: center; }
                                    Text { text: root.dl-done ? "All files checked" : root.dl-paused ? "Paused" : root.dl-left; font-size: 15px; font-weight: 600; color: #8A2A08; horizontal-alignment: center; }
                                }
                            }
                        }

                        // ---- 6 Ready
                        if root.onb-step == 5: HorizontalLayout {
                            padding: 18px;
                            spacing: 18px;
                            VerticalLayout {
                                horizontal-stretch: 1; preferred-width: 100px; min-width: 0px;
                                padding-top: 38px;
                                padding-left: 30px;
                                padding-right: 30px;
                                padding-bottom: 26px;
                                spacing: 20px;
                                VerticalLayout {
                                    vertical-stretch: 0;
                                    alignment: start;
                                    Eyebrow { text: "ALL SET"; color: rgb(78, 122, 18); }
                                    Rectangle { height: 10px; }
                                    H1 { text: "Now say something."; font-size: 48px; letter-spacing: -1.6px; }
                                    Rectangle { height: 12px; }
                                    HorizontalLayout {
                                        spacing: 6px;
                                        alignment: start;
                                        Text { text: "Click the box, press"; font-size: 16px; color: Theme.body; vertical-alignment: center; }
                                        KeyCap { label: root.key-label; fs: 14px; ring: 1.5px; drop: 2px; r: 7px; padx: 8px; height: 24px; }
                                        Text { text: root.key-mode == 2 ? "and talk while you hold it." : "and talk. Press it again when you're done."; font-size: 16px; color: Theme.body; vertical-alignment: center; }
                                    }
                                }
                                Field { multiline: true; height: 120px; border-radius: 18px; border-width: 2.5px; border-color: Theme.ink; placeholder: "Try: “Note to self, as a list: book flights, renew passport, call mum.”"; }
                                HorizontalLayout {
                                    spacing: 8px;
                                    for s[i] in [
                                        { k: "MIC", v: root.summary-mic, c: #EAF8D2 },
                                        { k: "KEY", v: root.key-label + " · " + root.mode-text, c: #E2F0FF },
                                        { k: "PACK", v: root.summary-pack, c: #FFE3D3 },
                                    ]: Rectangle {
                                        horizontal-stretch: 1; preferred-width: 100px; min-width: 0px;
                                        height: 64px;
                                        border-radius: 16px;
                                        background: s.c;
                                        VerticalLayout {
                                            padding-left: 14px;
                                            padding-right: 14px;
                                            alignment: center;
                                            spacing: 3px;
                                            Text { text: s.k; font-size: 11px; font-weight: 700; letter-spacing: 0.8px; color: Theme.body; }
                                            Text { text: s.v; font-size: 14px; font-weight: 700; color: Theme.ink; overflow: elide; }
                                        }
                                    }
                                }
                                if !root.dl-done: Text { text: "Still downloading (" + Math.round(root.dl-fraction * 100) + "%). It keeps going in the background."; font-size: 13px; color: #8A2A08; }
                                Rectangle { vertical-stretch: 1; }
                                HorizontalLayout {
                                    spacing: 12px;
                                    HorizontalLayout {
                                        spacing: 10px;
                                        Rectangle {
                                            width: 20px; height: 20px; y: (parent.height - self.height) / 2; border-radius: 5px;
                                            background: root.onb-login ? Theme.ink : #FFFFFF;
                                            border-width: 2px; border-color: Theme.ink;
                                            if root.onb-login: Glyph { d: "M5 12l5 5 9-10"; tint: #FFFFFF; size: 14px; weight: 3px; x: 1px; y: 1px; }
                                        }
                                        Text { text: "Start Voicio when I log in"; font-size: 14px; font-weight: 600; color: Theme.ink; vertical-alignment: center; }
                                        TouchArea { mouse-cursor: MouseCursor.pointer; clicked => { root.onb-login = !root.onb-login; } }
                                    }
                                    Rectangle { horizontal-stretch: 1; preferred-width: 100px; min-width: 0px; }
                                    Btn { text: "Open Voicio"; primary: true; h: 52px; r: 18px; pad: 26px; fs: 16px; clicked => { root.onb-finish(); } }
                                }
                            }
                            Panel {
                                horizontal-stretch: 1; preferred-width: 100px; min-width: 0px;
                                from: #F6FDEB; to: #E3F6C6; glow: #D7F5A8; gx: 0.5; gy: 0.42;
                                for c in [
                                    { x: 0.12, y: 0.14, c: #FF8A5B, s: 14px }, { x: 0.80, y: 0.10, c: rgb(142, 197, 255), s: 10px }, { x: 0.70, y: 0.78, c: #FFD35B, s: 16px },
                                    { x: 0.18, y: 0.72, c: #16130F, s: 8px }, { x: 0.88, y: 0.46, c: #FF8A5B, s: 9px }, { x: 0.08, y: 0.42, c: rgb(142, 197, 255), s: 12px },
                                    { x: 0.42, y: 0.08, c: #B8E986, s: 12px }, { x: 0.56, y: 0.88, c: #16130F, s: 7px }, { x: 0.30, y: 0.86, c: #FFD35B, s: 10px },
                                    { x: 0.90, y: 0.86, c: #B8E986, s: 12px }, { x: 0.62, y: 0.18, c: #16130F, s: 6px }, { x: 0.24, y: 0.30, c: #FFD35B, s: 7px },
                                    { x: 0.76, y: 0.62, c: #B8E986, s: 8px }, { x: 0.36, y: 0.60, c: rgb(142, 197, 255), s: 7px },
                                ]: Rectangle { x: parent.width * c.x; y: parent.height * c.y; width: c.s; height: c.s; border-radius: c.s / 2; background: c.c; }
                                Rectangle {
                                    x: (parent.width - self.width) / 2;
                                    y: (parent.height - self.height) / 2 - 3px + 3px * Math.sin(animation-tick() / 3.2s * 360deg);
                                    width: rd.preferred-width;
                                    height: 56px;
                                    border-radius: 28px;
                                    background: Theme.ink;
                                    drop-shadow-blur: 40px;
                                    drop-shadow-offset-y: 20px;
                                    drop-shadow-color: rgba(40, 60, 0, 0.25);
                                    rd := HorizontalLayout {
                                        padding-left: 16px;
                                        padding-right: 22px;
                                        spacing: 12px;
                                        Comb { look: 4; size: 40px; y: (parent.height - self.height) / 2; }
                                        Text { text: "You're ready"; font-size: 16px; font-weight: 600; color: #F7F3EA; vertical-alignment: center; }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            if root.chip-open: PackMenu {
                x: parent.width - 28px - self.width;
                y: 72px;
                packs: root.packs;
                pack: root.onb-pack;
                recommended: root.recommended-pack;
                picked(i) => {
                    root.chip-open = false;
                    root.onb-pack = i;
                    if root.onb-step >= 4 { root.go(4); } else { root.onb-enter(root.onb-step); }
                }
            }
        }
    }
}

#[derive(Default)]
struct UiState {
    tx: Option<Sender<Msg>>,
    desktop: Option<Desktop>,
    tray: Option<Tray>,
    settings: Option<SettingsWindow>,
    overlay: Option<Overlay>,
    history: crate::history::History,
    history_rows: Rc<VecModel<HistoryRow>>,
    recent_rows: Rc<VecModel<HistoryRow>>,
    history_query: String,
    history_filter: i32,
    dict_rows: Rc<VecModel<DictRow>>,
    packs: Rc<VecModel<PackRow>>,
    lang_rows: Rc<VecModel<LangRow>>,
    onb_langs: Rc<VecModel<LangRow>>,
    /// Language models chosen in setup's Pack step, fetched with the pack.
    onb_added: Vec<String>,
    dl_rows: Rc<VecModel<DlRow>>,
    mic_bars: Rc<VecModel<f32>>,
    cloud_rows: Rc<VecModel<CloudRow>>,
    /// Setup's microphone check: the open stream, its judge and the timer that polls it.
    mic: Option<(crate::miclevel::Meter, crate::miclevel::Guidance, slint::Timer)>,
    install_cancel: Option<voicio_models::Cancel>,
    /// The pack (tier index) being fetched, if the download is for a whole pack.
    install_tier: Option<usize>,
    /// The download was paused on purpose (its "cancelled" is not an error).
    install_paused: bool,
    state: String,
    models: String,
    health: Option<(String, String, String)>,
    /// The About page asked for a health check: show its answer there.
    health_asked: bool,
    overlay_visible: bool,
    pill: Option<Pill>,
    /// Runs once the pill's native window has been placed (the startup prewarm hides it then).
    after_placed: Option<Box<dyn FnOnce()>>,
    injector: Injector,
    hotkey: Option<crate::hotkey::Registration>,
}

thread_local! {
    static UI: RefCell<UiState> = RefCell::new(UiState::default());
    /// Set while the overlay's native window is being created, so the attribute hook can make
    /// exactly that window non-activating.
    static CREATING_OVERLAY: Cell<bool> = const { Cell::new(false) };
}

fn command(cmd: Cmd) {
    UI.with(|ui| {
        if let Some(tx) = &ui.borrow().tx {
            let _ = tx.send(Msg::Command(cmd));
        }
    });
}

/// Runs the UI event loop on the calling (main) thread until `quit()`. Errors without a display.
pub fn run(tx: Sender<Msg>, desktop: Desktop, shortcut: &str) -> Result<(), slint::PlatformError> {
    select_backend()?;
    // Lets the desktop match this window to io.github.voicio.Voicio.desktop (dock icon, grouping).
    let _ = slint::set_xdg_app_id("io.github.voicio.Voicio");
    // History must exist before the first dictation finishes: load it now that the UI thread is ours.
    UI.with(|ui| ui.borrow_mut().history = crate::history::History::load(crate::history::History::default_path(), Config::load().history_days));
    let hotkey = crate::hotkey::register(desktop, shortcut, tx.clone());
    UI.with(|ui| {
        let mut ui = ui.borrow_mut();
        ui.tx = Some(tx);
        ui.hotkey = Some(hotkey);
        ui.desktop = Some(desktop);
    });
    if desktop.has_tray() {
        let tray = Tray::new()?;
        tray.on_toggle(|| command(Cmd::Toggle));
        tray.on_cancel(|| command(Cmd::Cancel));
        tray.on_settings(|| command(Cmd::OpenSettings));
        tray.on_quit(|| command(Cmd::Quit));
        UI.with(|ui| ui.borrow_mut().tray = Some(tray));
    }
    // GNOME's extension draws the pill itself, but the Slint one is its fallback: ready it too.
    if desktop.has_overlay() || (desktop == Desktop::Gnome && crate::desktop::wants_xwayland()) {
        prewarm_overlay();
    }
    slint::run_event_loop_until_quit()
}

/// X11 clients (here: XWayland) do not see GNOME's cursor setting, so libXcursor falls back to a
/// small default and the pointer shrinks over our windows. Hand it the desktop's own size and theme.
#[cfg(all(unix, not(target_os = "macos")))]
fn match_cursor_to_desktop() {
    let get = |key: &str| {
        std::process::Command::new("gsettings").args(["get", "org.gnome.desktop.interface", key]).output().ok()
            .filter(|o| o.status.success()).map(|o| String::from_utf8_lossy(&o.stdout).trim().trim_matches('\'').to_string())
    };
    if std::env::var_os("XCURSOR_SIZE").is_none() {
        if let Some(size) = get("cursor-size").filter(|s| s.parse::<u32>().is_ok()) {
            std::env::set_var("XCURSOR_SIZE", size); // before any other thread exists: still single threaded here
        }
    }
    if std::env::var_os("XCURSOR_THEME").is_none() {
        if let Some(theme) = get("cursor-theme").filter(|s| !s.is_empty()) {
            std::env::set_var("XCURSOR_THEME", theme);
        }
    }
}

/// The winit backend with a hook that makes the overlay a non-activating, always-on-top,
/// taskbar-less window, and (macOS) an accessory app with no Dock icon.
fn select_backend() -> Result<(), slint::PlatformError> {
    use slint::winit_030::winit;
    let selector = slint::BackendSelector::new().backend_name("winit".into()).with_winit_window_attributes_hook(
        |attributes: winit::window::WindowAttributes| {
            if !CREATING_OVERLAY.with(Cell::get) {
                return attributes;
            }
            let attributes = attributes
                .with_active(false)
                .with_window_level(winit::window::WindowLevel::AlwaysOnTop)
                .with_decorations(false)
                .with_resizable(false);
            #[cfg(all(unix, not(target_os = "macos")))]
            let attributes = {
                use winit::platform::x11::{WindowAttributesExtX11, WindowType};
                attributes.with_x11_window_type(vec![WindowType::Notification]).with_override_redirect(true)
            };
            #[cfg(windows)]
            let attributes = {
                use winit::platform::windows::WindowAttributesExtWindows;
                attributes.with_skip_taskbar(true)
            };
            attributes
        },
    );
    #[cfg(target_os = "macos")]
    let selector = {
        use winit::platform::macos::{ActivationPolicy, EventLoopBuilderExtMacOS};
        let mut builder = winit::event_loop::EventLoop::with_user_event();
        builder.with_activation_policy(ActivationPolicy::Accessory);
        selector.with_winit_event_loop_builder(builder)
    };
    // Native Wayland ignores always-on-top, no-focus and explicit position, so the pill would be an
    // ordinary window that opens behind others or steals focus. XWayland honours all three.
    // winit 0.30 no longer reads WINIT_UNIX_BACKEND; the backend must be chosen on the builder.
    #[cfg(all(unix, not(target_os = "macos")))]
    let selector = if crate::desktop::wants_xwayland() {
        use winit::platform::x11::EventLoopBuilderExtX11;
        match_cursor_to_desktop();
        let mut builder = winit::event_loop::EventLoop::with_user_event();
        builder.with_x11();
        selector.with_winit_event_loop_builder(builder)
    } else {
        selector
    };
    selector.select()
}

pub fn quit() {
    let _ = slint::quit_event_loop();
}

/// The pill's animation: Hex's lifecycle model, stepped by a timer only while something is visible.
struct Pill {
    model: PillModel,
    timer: slint::Timer,
    last_step: Instant,
    buffer: Vec<u8>,
    /// What the dictation is doing as the pill last heard: "" | listening | finishing | error.
    state: &'static str,
    /// The recognised words so far, and when they last changed (the new ones fade in).
    words: String,
    words_at: Instant,
    /// A message shown instead of the words (what went wrong, what to do).
    notice: Option<(NoticeKind, String)>,
    /// Words in the text that was just typed (for "Typed · 14 words"); None until delivered.
    typed: Option<usize>,
    /// Frame pacing numbers, only when `VOICIO_PILL_STATS` is set.
    stats: Option<PacingStats>,
}

/// Frame interval and paint cost, to prove the animation is steady.
#[derive(Default)]
struct PacingStats {
    intervals: Vec<f32>,
    paints: Vec<f32>,
    /// (frame number, interval in ms) of every gap over 40 ms.
    gaps: Vec<(u32, f32)>,
}

impl PacingStats {
    fn add(&mut self, dt: f64, paint_ms: f64) {
        if dt > 0.04 {
            self.gaps.push((self.intervals.len() as u32, dt as f32 * 1e3));
        }
        self.intervals.push(dt as f32 * 1e3);
        self.paints.push(paint_ms as f32);
    }

    fn report(&self) -> String {
        fn pct(v: &[f32], p: f32) -> f32 {
            let mut v = v.to_vec();
            v.sort_by(|a, b| a.total_cmp(b));
            v.get(((v.len() as f32 - 1.0) * p) as usize).copied().unwrap_or(0.0)
        }
        format!(
            "pill pacing: {} frames; interval median {:.2} ms, p95 {:.2} ms, p99 {:.2} ms; {} gaps over 40 ms {:?}; paint median {:.2} ms, p99 {:.2} ms",
            self.intervals.len(), pct(&self.intervals, 0.5), pct(&self.intervals, 0.95), pct(&self.intervals, 0.99), self.gaps.len(), self.gaps, pct(&self.paints, 0.5), pct(&self.paints, 0.99)
        )
    }
}

const CREAM: slint::Color = slint::Color::from_rgb_u8(0xEF, 0xE7, 0xD7);
const AMBER: slint::Color = slint::Color::from_rgb_u8(0xFF, 0xC2, 0x4B);
const LIME: slint::Color = slint::Color::from_rgb_u8(0xB8, 0xF0, 0x6A);

/// The image the field is painted into, in points (the `Image` in `Overlay`).
const PILL_W: f32 = 400.0;
const PILL_H: f32 = 64.0;

/// Shows (creating if needed) or updates the overlay pill.
pub fn overlay(state: &'static str, text: Option<String>) {
    let _ = slint::invoke_from_event_loop(move || {
        if let Err(e) = update_overlay(state, text) {
            eprintln!("voicio: overlay: {e}");
        }
    });
}

/// Sets the live words shown inside the pill without touching its state (partials can arrive late).
pub fn overlay_text(text: String) {
    let _ = slint::invoke_from_event_loop(move || {
        if with_pill(|p| p.state) == "listening" {
            set_words(text);
        }
    });
}

fn set_words(text: String) {
    let words = tail_words(text.trim(), 160);
    with_pill(|p| {
        if p.words != words {
            // a revised word keeps its look; only a word added at the end fades in
            if words.len() > p.words.len() && words.starts_with(p.words.as_str()) {
                p.words_at = Instant::now();
            }
            p.words = words;
        }
    });
}

/// Raw microphone loudness, overall and in three bands, for the pill (it levels itself).
pub fn meter(bands: Bands) {
    let _ = slint::invoke_from_event_loop(move || {
        pill_handle(PillEvent::Meter(bands));
    });
}

/// The speech models are loading (true) or ready (false).
pub fn warming(on: bool) {
    let _ = slint::invoke_from_event_loop(move || pill_handle(PillEvent::Warming(on)));
}

/// Shows a message in the pill with its own look: amber (adjust something), coral (went wrong) or
/// teal (good news). It leaves by itself; a second message replaces the first and restarts its time.
pub fn notice(kind: NoticeKind, text: String) {
    let _ = slint::invoke_from_event_loop(move || {
        if let Err(e) = ensure_overlay() {
            eprintln!("voicio: overlay: {e}");
            return;
        }
        with_pill(|p| {
            p.notice = Some((kind, text));
            p.state = "notice";
        });
        pill_handle(PillEvent::Notice(kind));
    });
}

fn ensure_overlay() -> Result<Overlay, slint::PlatformError> {
    use slint::ComponentHandle;
    let existing = UI.with(|ui| ui.borrow().overlay.as_ref().map(|o| o.as_weak()));
    let overlay = match existing.and_then(|w| w.upgrade()) {
        Some(overlay) => overlay,
        None => {
            // Slint creates the native window here, not at show(): the attribute hook that makes it
            // non-activating, always-on-top and override-redirect must run for exactly this one.
            CREATING_OVERLAY.with(|c| c.set(true));
            let created = Overlay::new();
            CREATING_OVERLAY.with(|c| c.set(false));
            let overlay = created?;
            UI.with(|ui| ui.borrow_mut().overlay = Some(overlay.clone_strong()));
            overlay
        }
    };
    if !UI.with(|ui| ui.borrow().overlay_visible) {
        overlay.show()?;
        place_when_ready(0);
        UI.with(|ui| ui.borrow_mut().overlay_visible = true);
    }
    Ok(overlay)
}

fn update_overlay(state: &'static str, text: Option<String>) -> Result<(), slint::PlatformError> {
    ensure_overlay()?;
    match text {
        Some(text) => set_words(text),
        None if state == "listening" => set_words(String::new()), // a fresh dictation drops old words
        None => {}
    }
    let previous = with_pill(|p| p.state);
    if previous != state {
        let event = match state {
            "listening" => Some(PillEvent::Started),
            "finishing" => Some(PillEvent::Submitted),
            _ => None,
        };
        pill_set_state(state);
        if state == "listening" {
            with_pill(|p| {
                p.notice = None;
                p.typed = None;
            });
        }
        if let Some(event) = event {
            pill_handle(event);
        }
    }
    Ok(())
}

/// Runs `f` on the pill, creating it on first use.
fn with_pill<R>(f: impl FnOnce(&mut Pill) -> R) -> R {
    UI.with(|ui| {
        let mut ui = ui.borrow_mut();
        let pill = ui.pill.get_or_insert_with(|| Pill {
            model: PillModel::new(Instant::now()),
            timer: slint::Timer::default(),
            last_step: Instant::now(),
            buffer: Vec::new(),
            state: "",
            words: String::new(),
            words_at: Instant::now(),
            notice: None,
            typed: None,
            stats: std::env::var_os("VOICIO_PILL_STATS").map(|_| PacingStats::default()),
        });
        f(pill)
    })
}

fn pill_set_state(state: &'static str) {
    with_pill(|p| p.state = state);
}

/// Feeds one event to the model and makes sure the timer is drawing it.
fn pill_handle(event: PillEvent) {
    with_pill(|pill| {
        pill.model.handle(event, Instant::now());
        if !pill.timer.running() {
            pill.last_step = Instant::now();
            pill.timer.start(slint::TimerMode::Repeated, Duration::from_millis(16), pill_tick);
        }
    });
}

/// One animation frame: step Hex's model, paint it, and stop drawing once it settles out of sight.
fn pill_tick() {
    use slint::ComponentHandle;
    let drawn = UI.with(|ui| {
        let mut ui = ui.borrow_mut();
        let overlay = ui.overlay.as_ref()?.clone_strong();
        let pill = ui.pill.as_mut()?;
        let now = Instant::now();
        let dt = now.duration_since(pill.last_step).as_secs_f32();
        pill.last_step = now;
        let frame = pill.model.step(now, dt);
        let painted_at = Instant::now();
        let target = Target { width: PILL_W, height: PILL_H, scale: overlay.window().scale_factor().min(3.0), dark: overlay.get_dark() };
        paint::paint(&frame, &target, &mut pill.buffer);
        let (w, h) = target.pixels();
        let image = slint::Image::from_rgba8_premultiplied(slint::SharedPixelBuffer::clone_from_slice(&pill.buffer, w as u32, h as u32));
        overlay.set_frame(image);
        let notice = pill.notice.clone().filter(|_| pill.state == "notice");
        // Capsule.dc.html: no text in a quiet room; "Tidying…" in amber while working; "Typed" in lime
        // with a quiet undo hint once the text is in. The pill never takes focus, so actions are hints.
        let (caption, colour, hint) = if notice.is_some() {
            (String::new(), CREAM, if matches!(notice, Some((NoticeKind::Error, _))) { "Open Voicio to fix" } else { "" })
        } else if frame.done > 0.3 {
            match pill.typed {
                Some(n) if n > 0 => (format!("Typed · {n} {}", if n == 1 { "word" } else { "words" }), LIME, "Ctrl+Z to undo"),
                _ => ("Typed".to_string(), LIME, ""),
            }
        } else if frame.working > 0.5 {
            ("Tidying…".to_string(), AMBER, "")
        } else if frame.warming > 0.2 {
            ("Warming up".to_string(), CREAM, "")
        } else {
            (String::new(), CREAM, "")
        };
        overlay.set_caption(caption.as_str().into());
        overlay.set_caption_color(colour);
        overlay.set_hint(hint.into());
        if let Some(stats) = pill.stats.as_mut() {
            if pill.model.active() {
                stats.add(dt as f64, painted_at.elapsed().as_secs_f64() * 1e3);
            }
        }
        // Words inside the capsule: settled ones bright, the last few dim and fading in, a caret
        // while listening; the capsule widens to fit what was measured last frame.
        let (settled, fresh) = split_words(&pill.words);
        let showing = notice.is_none() && !pill.words.is_empty() && frame.working < 0.5 && frame.done < 0.1 && frame.warming < 0.2;
        overlay.set_settled(settled.into());
        overlay.set_fresh(fresh.into());
        overlay.set_fresh_opacity((now.duration_since(pill.words_at).as_secs_f32() / 0.28).min(1.0));
        overlay.set_capsule_width(frame.width);
        overlay.set_show_words(showing);
        overlay.set_show_caret(showing && pill.state == "listening");
        overlay.set_caret_phase(0.35 + 0.65 * (0.5 + 0.5 * (frame.time * 3.5).sin()));
        match notice {
            Some((kind, text)) => {
                overlay.set_notice_text(text.as_str().into());
                overlay.set_notice_color(match kind {
                    NoticeKind::Warn => AMBER,
                    NoticeKind::Error => slint::Color::from_rgb_u8(0xFF, 0xB3, 0xA8),
                    NoticeKind::Info => LIME,
                });
                pill.model.set_text_width(Some(overlay.get_notice_width()));
            }
            None => {
                overlay.set_notice_text("".into());
                let width = if showing { Some(overlay.get_words_width()) } else if !caption.is_empty() { Some(overlay.get_caption_width()) } else { None };
                pill.model.set_text_width(width);
            }
        }
        Some((pill.model.active(), pill.state))
    });
    match drawn {
        Some((true, _)) => {}
        Some((false, _state)) => {
            UI.with(|ui| {
                if let Some(p) = ui.borrow().pill.as_ref() {
                    p.timer.stop();
                }
            });
            if let Some(stats) = with_pill(|p| p.stats.take()) {
                if !stats.intervals.is_empty() {
                    eprintln!("voicio: {}", stats.report());
                }
            }
            pill_set_state("");
            hide_now(); // the model has settled out of sight
        }
        None => {}
    }
}

/// Shows an error in the pill, so failures are never silent on any desktop.
pub fn overlay_message(message: String) {
    notice(NoticeKind::Error, message);
}

/// Bottom-centre of the current monitor; clicks pass through so it can never take focus.
/// Returns false while Slint has not created the native window yet (it does so on a later turn of
/// the event loop, after `show()`).
fn place_overlay(overlay: &Overlay) -> bool {
    use slint::winit_030::{winit::dpi::PhysicalPosition, WinitWindowAccessor};
    use slint::ComponentHandle;
    overlay
        .window()
        .with_winit_window(|window| {
            let _ = window.set_cursor_hittest(false);
            if let Some(monitor) = window.current_monitor().or_else(|| window.primary_monitor()) {
                let (screen, origin, size) = (monitor.size(), monitor.position(), window.outer_size());
                let x = origin.x + (screen.width as i32 - size.width as i32) / 2;
                let y = origin.y + screen.height as i32 - size.height as i32 - (screen.height as f64 * 0.08) as i32;
                window.set_outer_position(PhysicalPosition::new(x, y));
            }
        })
        .is_some()
}

/// Places the pill as soon as its native window exists (a few milliseconds after `show()`).
fn place_when_ready(attempt: u32) {
    let Some(overlay) = UI.with(|ui| ui.borrow().overlay.as_ref().map(|o| o.clone_strong())) else { return };
    if place_overlay(&overlay) || attempt >= 50 {
        if let Some(done) = UI.with(|ui| ui.borrow_mut().after_placed.take()) {
            done();
        }
        return;
    }
    slint::Timer::single_shot(Duration::from_millis(10), move || place_when_ready(attempt + 1));
}

/// Creates, places and hides the pill once at startup, so dictations reuse an already placed
/// window and never flash at the screen corner. The pill is empty (transparent) meanwhile.
pub fn prewarm_overlay() {
    let _ = slint::invoke_from_event_loop(|| {
        if ensure_overlay().is_ok() {
            UI.with(|ui| ui.borrow_mut().after_placed = Some(Box::new(hide_now)));
        }
    });
}

/// Hides the pill unless it is showing an error (that one hides itself after a few seconds).
pub fn hide_overlay() {
    let _ = slint::invoke_from_event_loop(|| {
        // The model plays the ending (done wipes out, a cancel just fades) and the timer hides the
        // window when it has settled. An error card keeps its own few seconds.
        let event = match with_pill(|p| p.state) {
            "listening" => PillEvent::Cancelled,
            "finishing" => PillEvent::Completed,
            _ => return, // a notice leaves by itself
        };
        pill_set_state("");
        pill_handle(event);
    });
}

fn hide_now() {
    UI.with(|ui| {
        let mut ui = ui.borrow_mut();
        if let Some(overlay) = &ui.overlay {
            let _ = overlay.hide();
        }
        ui.overlay_visible = false;
    });
}

/// Types or copies the finished dictation into the focused app.
pub fn deliver(text: String, type_text: bool) {
    let words = text.split_whitespace().count();
    let _ = slint::invoke_from_event_loop(move || with_pill(|p| p.typed = Some(words)));
    // Typing costs a couple of milliseconds per character: on the UI thread it froze the pill in
    // the middle of its "done" animation. macOS needs input synthesis on the main thread; every
    // other desktop gets a worker thread that owns its own injector (and clipboard).
    #[cfg(not(target_os = "macos"))]
    {
        use std::sync::{mpsc, OnceLock};
        static WORKER: OnceLock<mpsc::Sender<(String, bool)>> = OnceLock::new();
        let tx = WORKER.get_or_init(|| {
            let (tx, rx) = mpsc::channel::<(String, bool)>();
            let _ = std::thread::Builder::new().name("deliver".into()).spawn(move || {
                let mut injector = Injector::default();
                for (text, type_text) in rx {
                    report_delivery(injector.deliver(&text, type_text));
                }
            });
            tx
        });
        let _ = tx.send((text, type_text));
    }
    #[cfg(target_os = "macos")]
    {
        let _ = slint::invoke_from_event_loop(move || {
            let result = UI.with(|ui| ui.borrow_mut().injector.deliver(&text, type_text));
            report_delivery(result);
        });
    }
}

fn report_delivery(result: Result<Delivered, String>) {
    match result {
        Ok(Delivered::Typed) => {}
        // No pop-ups: the text is in the History page, and the log has the reason.
        Ok(Delivered::Clipboard) => eprintln!("voicio: could not type; the text is on the clipboard"),
        Err(e) => eprintln!("voicio: could not insert the text: {e}"),
    }
}

fn notify(summary: &str, body: &str) {
    #[cfg(target_os = "linux")]
    crate::dbus::notify(summary, body);
    #[cfg(not(target_os = "linux"))]
    eprintln!("voicio: {summary}: {body}");
}

/// The last `max` characters, cut at a word boundary, so the newest words stay in the pill.
fn tail_words(text: &str, max: usize) -> String {
    let count = text.chars().count();
    if count <= max {
        return text.to_string();
    }
    let rest: String = text.chars().skip(count - max).collect();
    match rest.find(' ') {
        Some(i) => rest[i + 1..].to_string(),
        None => rest,
    }
}

/// Splits live words into those that are settled and the last three, which may still change.
fn split_words(text: &str) -> (String, String) {
    let words: Vec<&str> = text.split_whitespace().collect();
    let keep = words.len().saturating_sub(3);
    let settled = words[..keep].join(" ");
    let fresh = words[keep..].join(" ");
    if settled.is_empty() {
        (String::new(), fresh)
    } else {
        (format!("{settled} "), fresh)
    }
}

#[cfg(test)]
mod pill_text_tests {
    use super::{split_words, tail_words};

    #[test]
    fn the_last_three_words_are_unsettled() {
        assert_eq!(split_words("hello"), (String::new(), "hello".to_string()));
        assert_eq!(split_words("one two three"), (String::new(), "one two three".to_string()));
        assert_eq!(split_words("one two three four five"), ("one two ".to_string(), "three four five".to_string()));
        assert_eq!(split_words(""), (String::new(), String::new()));
    }

    #[test]
    fn long_text_keeps_the_newest_words_from_a_word_boundary() {
        let text = "alpha beta gamma delta epsilon";
        assert_eq!(tail_words(text, 100), text);
        assert_eq!(tail_words(text, 14), "delta epsilon");
    }
}


pub fn set_state(state: &'static str) {
    let _ = slint::invoke_from_event_loop(move || {
        UI.with(|ui| {
            let mut ui = ui.borrow_mut();
            ui.state = state.to_string();
            if let Some(tray) = &ui.tray {
                tray.set_state(state.into());
            }
            if let Some(window) = &ui.settings {
                window.set_state(state.into());
            }
        })
    });
}

/// Remembers a finished dictation for the History page.
pub fn add_history(text: String) {
    let _ = slint::invoke_from_event_loop(move || {
        UI.with(|ui| ui.borrow_mut().history.add(&text));
        refresh_history();
    });
}

// ---- History ------------------------------------------------------------------------------------

/// What a dictation looks like: a bulleted list, an email (greeting and sign-off), or a note.
fn kind_of(text: &str) -> &'static str {
    let lines: Vec<&str> = text.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    let bullets = lines.iter().filter(|l| l.starts_with(['•', '-', '*']) || l.split_once(". ").is_some_and(|(n, _)| n.parse::<u32>().is_ok())).count();
    let first = lines.first().map(|l| l.to_lowercase()).unwrap_or_default();
    let greeting = ["hi ", "hi,", "hello", "dear ", "hey ", "good morning", "good afternoon"].iter().any(|g| first.starts_with(g));
    if bullets >= 2 {
        "List"
    } else if greeting && lines.len() >= 2 {
        "Email"
    } else {
        "Note"
    }
}

/// Days since the Unix epoch in local time (good enough for grouping: uses the TZ offset of now).
fn local_day(at: u64, offset: i64) -> i64 {
    (at as i64 + offset).div_euclid(86_400)
}

fn local_offset() -> i64 {
    // ponytail: the offset is read once from `date +%z` on Unix; UTC elsewhere. Add a time-zone crate if
    // grouping across a DST change ever matters.
    #[cfg(unix)]
    {
        if let Some(out) = std::process::Command::new("date").arg("+%z").output().ok().filter(|o| o.status.success()) {
            let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if s.len() == 5 {
                let sign = if s.starts_with('-') { -1 } else { 1 };
                let (h, m) = (s[1..3].parse::<i64>().unwrap_or(0), s[3..5].parse::<i64>().unwrap_or(0));
                return sign * (h * 3600 + m * 60);
            }
        }
    }
    0
}

fn clock(at: u64, offset: i64) -> String {
    let secs = (at as i64 + offset).rem_euclid(86_400);
    format!("{:02}:{:02}", secs / 3600, secs / 60 % 60)
}

fn day_label(day: i64, today: i64) -> String {
    match today - day {
        0 => "TODAY".into(),
        1 => "YESTERDAY".into(),
        n if n < 7 => format!("{n} DAYS AGO"),
        _ => {
            // civil date from days since epoch (Howard Hinnant's algorithm)
            let z = day + 719_468;
            let era = z.div_euclid(146_097);
            let doe = z - era * 146_097;
            let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
            let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
            let mp = (5 * doy + 2) / 153;
            let d = doy - (153 * mp + 2) / 5 + 1;
            let m = if mp < 10 { mp + 3 } else { mp - 9 };
            const MONTHS: [&str; 12] = ["JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV", "DEC"];
            format!("{d} {}", MONTHS[(m - 1) as usize])
        }
    }
}

/// History rows (newest first) grouped by day, filtered (0 all, 1 today, 2 lists, 3 emails).
fn history_rows(entries: &[(usize, &crate::history::Entry)], filter: i32, now: u64, offset: i64) -> Vec<HistoryRow> {
    let today = local_day(now, offset);
    let mut rows: Vec<HistoryRow> = Vec::new();
    let mut last_day = None;
    for (index, e) in entries {
        let kind = kind_of(&e.text);
        let day = local_day(e.at, offset);
        let keep = match filter {
            1 => day == today,
            2 => kind == "List",
            3 => kind == "Email",
            _ => true,
        };
        if !keep {
            continue;
        }
        let words = e.text.split_whitespace().count();
        rows.push(HistoryRow {
            text: e.text.as_str().into(),
            line: e.text.split_whitespace().collect::<Vec<_>>().join(" ").into(),
            when: crate::history::ago(e.at, now).into(),
            day: day_label(day, today).into(),
            first: last_day != Some(day),
            kind: kind.into(),
            meta: format!("{} · {words} {}", clock(e.at, offset), if words == 1 { "word" } else { "words" }).into(),
            index: *index as i32,
        });
        last_day = Some(day);
    }
    rows
}

/// Rebuilds the rows the History page and Home page show, from the data, the search and the filter.
fn refresh_history() {
    UI.with(|ui| {
        let ui = ui.borrow();
        let now = crate::history::now();
        let offset = local_offset();
        let found = ui.history.search(&ui.history_query);
        ui.history_rows.set_vec(history_rows(&found, ui.history_filter, now, offset));
        let all = ui.history.search("");
        ui.recent_rows.set_vec(history_rows(&all[..all.len().min(3)], 0, now, offset));
        if let Some(w) = &ui.settings {
            w.set_history_total(ui.history.len() as i32);
            w.set_history_subtitle(history_subtitle(w.get_history_days()).into());
        }
    });
}

fn history_subtitle(days: i32) -> String {
    if days > 0 {
        format!("Kept on this computer for {days} days. Only you can see it.")
    } else {
        "Kept in memory only while Voicio runs. Nothing is written to disk.".into()
    }
}

fn refresh_dictionary(config: &Config) {
    UI.with(|ui| {
        ui.borrow().dict_rows.set_vec(
            config.replacements.iter().map(|r| DictRow { phrase: r.matched_phrase.as_str().into(), output: r.output.as_str().into() }).collect::<Vec<_>>(),
        );
    });
}

// ---- Health -------------------------------------------------------------------------------------

/// The banner's button: ask the desktop again for whichever permission is missing.
fn health_act() {
    use crate::health::Action;
    let label = UI.with(|ui| ui.borrow().health.as_ref().map(|h| h.2.clone())).unwrap_or_default();
    if label == Action::AskShortcut.label() {
        if let Some(tx) = UI.with(|ui| ui.borrow().tx.clone()) {
            crate::hotkey::retry(&Config::load().shortcut, tx);
        }
    } else if label == Action::AskTyping.label() {
        #[cfg(target_os = "linux")]
        crate::typing::type_async(String::new(), |result| {
            if let Err(e) = result {
                eprintln!("voicio: typing permission: {e}");
            }
            command(Cmd::Recheck);
        });
    }
}

pub fn set_health(issue: Option<(String, String, String)>) {
    let _ = slint::invoke_from_event_loop(move || {
        UI.with(|ui| {
            let mut ui = ui.borrow_mut();
            if let Some(window) = &ui.settings {
                window.set_health_problem(issue.as_ref().map(|i| i.0.as_str()).unwrap_or("").into());
                window.set_health_fix(issue.as_ref().map(|i| i.1.as_str()).unwrap_or("").into());
                window.set_health_action(issue.as_ref().map(|i| i.2.as_str()).unwrap_or("").into());
                if ui.health_asked {
                    window.set_health_note(match &issue {
                        Some(i) => format!("{} {}", i.0, i.1),
                        None => "All good: the microphone, shortcut, typing and models check out.".into(),
                    }.into());
                }
            }
            ui.health_asked = false;
            ui.health = issue;
        })
    });
}

pub fn set_models(line: String) {
    let _ = slint::invoke_from_event_loop(move || {
        UI.with(|ui| {
            let mut ui = ui.borrow_mut();
            if let Some(tray) = &ui.tray {
                tray.set_models(line.clone().into());
            }
            if let Some(window) = &ui.settings {
                window.set_models(line.clone().into());
            }
            ui.models = line;
        })
    });
}

/// Which page of the app window to show.
#[derive(Clone, Copy, PartialEq)]
pub enum Page {
    Home,
    Models,
    /// First-run setup, from its first step.
    Onboarding,
}

impl Page {
    fn index(self) -> i32 {
        match self {
            Page::Home | Page::Onboarding => 0,
            Page::Models => 2,
        }
    }
}

pub fn open_window(config: Config, shortcut_hint: String, page: Page) {
    let _ = shortcut_hint; // the window shows the key itself
    let _ = slint::invoke_from_event_loop(move || {
        if let Err(e) = show_settings(config, page) {
            eprintln!("voicio: app window: {e}");
        }
    });
}

fn show_settings(config: Config, page: Page) -> Result<(), slint::PlatformError> {
    use slint::ComponentHandle;
    let existing = UI.with(|ui| ui.borrow().settings.as_ref().map(|w| w.as_weak()));
    if let Some(window) = existing.and_then(|w| w.upgrade()) {
        if page != Page::Home {
            window.set_page(page.index());
        }
        if page == Page::Onboarding {
            window.set_onb_step(0);
            onboarding_enter(&window, 0);
        }
        return window.show(); // already open: bring it back
    }
    let window = SettingsWindow::new()?;
    window.set_page(page.index());
    bind_models(&window);
    load_settings(&window, &config);
    wire(&window);
    refresh_history();
    refresh_dictionary(&config);
    refresh_models(&window);
    refresh_cloud(&window);
    UI.with(|ui| {
        let ui = ui.borrow();
        window.set_state(ui.state.as_str().into());
        window.set_models(ui.models.as_str().into());
        window.set_health_problem(ui.health.as_ref().map(|i| i.0.as_str()).unwrap_or("").into());
        window.set_health_fix(ui.health.as_ref().map(|i| i.1.as_str()).unwrap_or("").into());
        window.set_health_action(ui.health.as_ref().map(|i| i.2.as_str()).unwrap_or("").into());
    });
    if page == Page::Onboarding {
        window.set_onb_step(0);
        onboarding_enter(&window, 0);
    }
    window.window().on_close_requested(|| {
        close_settings();
        slint::CloseRequestResponse::HideWindow
    });
    window.show()?;
    UI.with(|ui| ui.borrow_mut().settings = Some(window));
    Ok(())
}

/// Hands the window the shared row models (they are updated in place from then on).
fn bind_models(window: &SettingsWindow) {
    UI.with(|ui| {
        let ui = ui.borrow();
        window.set_history(ModelRc::from(ui.history_rows.clone()));
        window.set_recent(ModelRc::from(ui.recent_rows.clone()));
        window.set_dict_rows(ModelRc::from(ui.dict_rows.clone()));
        window.set_packs(ModelRc::from(ui.packs.clone()));
        window.set_lang_rows(ModelRc::from(ui.lang_rows.clone()));
        window.set_onb_langs(ModelRc::from(ui.onb_langs.clone()));
        window.set_dl_rows(ModelRc::from(ui.dl_rows.clone()));
        window.set_mic_bars(ModelRc::from(ui.mic_bars.clone()));
        window.set_cloud_rows(ModelRc::from(ui.cloud_rows.clone()));
    });
    window.set_version(env!("CARGO_PKG_VERSION").into());
    window.set_credits(ModelRc::new(VecModel::from(credits())));
    window.set_mem_note(memory_note().into());
    let mics = microphones();
    window.set_mic_names(ModelRc::new(VecModel::from(mics.iter().map(|m| SharedString::from(m.as_str())).collect::<Vec<_>>())));
}

/// Settings from `config.toml` into the window.
fn load_settings(window: &SettingsWindow, config: &Config) {
    window.set_key_label(key_label(&config.shortcut).into());
    window.set_key_mode(match config.shortcut_mode.as_str() { "toggle" => 1, "hold" => 2, _ => 0 });
    // The GNOME keybinding fallback runs `voicio toggle` (no key release): tap only, on the key it got.
    if let Some(crate::hotkey::Fallback::Active { key, .. }) = crate::hotkey::fallback() {
        window.set_bound_label(key_label(&key).into());
        window.set_bound_mode(1);
    }
    window.set_live_text(config.live_text);
    window.set_cleanup(config.cleanup);
    window.set_voice_commands(config.voice_commands);
    window.set_noise_reduction(config.noise_reduction);
    window.set_lists_on(config.cleanup_structure != "prose");
    window.set_email_on(config.cleanup_context == "email");
    window.set_styling_index(match config.cleanup_styling.as_str() { "casual" => 0, "semi-casual" => 1, "formal" => 3, _ => 2 });
    window.set_cpu_index(match config.cpu_mode.as_str() { "low" => 0, "fast" => 2, _ => 1 });
    window.set_history_days(config.history_days as i32);
    window.set_history_subtitle(history_subtitle(config.history_days as i32).into());
    window.set_launch_at_login(autostart::enabled());
    window.set_pause_media(config.pause_media);
    let mics = microphones();
    window.set_mic_index(mics.iter().position(|m| *m == config.input_device).unwrap_or(0) as i32);
    window.set_cloud_mode(match config.cloud_mode.as_str() { "always" => 1, "fallback" => 2, _ => 0 });
    window.set_cloud_region(config.cloud_region.as_str().into());
}

/// Every callback of the app window.
fn wire(window: &SettingsWindow) {
    let weak = window.as_weak();
    window.on_apply(move || {
        if let Some(w) = weak.upgrade() {
            apply(&w);
        }
    });
    window.on_toggle(|| command(Cmd::Toggle));
    window.on_health_act(health_act);
    window.on_health_check(|| {
        UI.with(|ui| ui.borrow_mut().health_asked = true);
        command(Cmd::Recheck);
    });
    window.on_copy(|text| {
        let result = UI.with(|ui| ui.borrow_mut().injector.copy(&text));
        if let Err(e) = result {
            notify("Could not copy", &e);
        }
    });
    window.on_open_url(|url| open_url(&url));
    // history
    window.on_clear_history(|| {
        UI.with(|ui| ui.borrow_mut().history.clear());
        refresh_history();
    });
    window.on_history_search(|query| {
        UI.with(|ui| ui.borrow_mut().history_query = query.to_string());
        refresh_history();
    });
    window.on_history_filter_changed(|filter| {
        UI.with(|ui| ui.borrow_mut().history_filter = filter);
        refresh_history();
    });
    window.on_history_delete(|index| {
        UI.with(|ui| ui.borrow_mut().history.remove(index.max(0) as usize));
        refresh_history();
    });
    // dictionary
    window.on_dict_add(|phrase, output| {
        let mut config = Config::load();
        let phrase = phrase.trim().to_string();
        if phrase.is_empty() {
            return;
        }
        // adding an existing phrase replaces its output
        config.replacements.retain(|r| !r.matched_phrase.eq_ignore_ascii_case(&phrase));
        config.replacements.insert(0, crate::replacements::TextReplacement { matched_phrase: phrase, output: output.trim().to_string() });
        save_and_reload(config.clone(), "the dictionary");
        refresh_dictionary(&config);
    });
    window.on_dict_remove(|index| {
        let mut config = Config::load();
        if (index as usize) < config.replacements.len() {
            config.replacements.remove(index as usize);
        }
        save_and_reload(config.clone(), "the dictionary");
        refresh_dictionary(&config);
    });
    // packs and models
    window.on_pick_pack(|index| pick_pack(index.clamp(0, 2) as usize));
    window.on_install(|id| {
        if let Some(model) = voicio_models::model(&id) {
            start_install(vec![model], None);
        }
    });
    window.on_cancel_install(cancel_install);
    window.on_remove(|id| remove_model(&id));
    window.on_lang_use(|id| use_language_model(&id));
    // the talk key
    let weak = window.as_weak();
    window.on_key_captured(move |text, ctrl, alt, shift, meta| {
        let Some(w) = weak.upgrade() else { return };
        match shortcut_from_key(&text, ctrl, alt, shift, meta) {
            KeyCapture::Ignore => {}
            KeyCapture::Cancel => w.set_capturing(false),
            KeyCapture::Shortcut(s) => {
                w.set_capturing(false);
                rebind(&w, &s);
            }
        }
    });
    let weak = window.as_weak();
    window.on_key_set(move |shortcut| {
        if let Some(w) = weak.upgrade() {
            rebind(&w, &shortcut);
        }
    });
    window.on_mic_picked(|index| {
        let mut config = Config::load();
        config.input_device = microphones().get(index.max(0) as usize).filter(|_| index > 0).cloned().unwrap_or_default();
        save_and_reload(config, "the microphone");
        UI.with(|ui| {
            let ui = ui.borrow();
            if ui.mic.is_some() {
                if let Some(w) = &ui.settings {
                    let w = w.clone_strong();
                    drop(ui);
                    start_mic_check(&w); // the setup's meter follows the new microphone
                }
            }
        });
    });
    setup_cloud(window);
    setup_onboarding(window);
}

fn open_url(url: &str) {
    #[cfg(target_os = "windows")]
    let opened = std::process::Command::new("explorer").arg(url).spawn();
    #[cfg(target_os = "macos")]
    let opened = std::process::Command::new("open").arg(url).spawn();
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let opened = std::process::Command::new("xdg-open").arg(url).spawn();
    match opened {
        Ok(mut child) => drop(std::thread::spawn(move || child.wait())), // reap it: the agent runs for days
        Err(e) => notify("Could not open the browser", &format!("{url} ({e})")),
    }
}

fn save_and_reload(config: Config, what: &str) {
    match config.save() {
        Ok(()) => command(Cmd::ReloadConfig(config)),
        Err(e) => notify(&format!("Could not save {what}"), &e.to_string()),
    }
}

/// Settings apply as soon as they change: read the window, save, tell the agent.
fn apply(w: &SettingsWindow) {
    let mut config = Config::load();
    config.cleanup = w.get_cleanup();
    config.live_text = w.get_live_text();
    config.voice_commands = w.get_voice_commands();
    config.noise_reduction = w.get_noise_reduction();
    config.pause_media = w.get_pause_media();
    config.cleanup_styling = ["casual", "semi-casual", "semi-formal", "formal"][w.get_styling_index().clamp(0, 3) as usize].into();
    config.cleanup_structure = if !w.get_lists_on() { "prose" } else if config.cleanup_structure == "lists" { "lists" } else { "auto" }.into();
    config.cleanup_context = if w.get_email_on() { "email" } else { "general" }.into();
    config.shortcut_mode = ["hold-or-tap", "toggle", "hold"][w.get_key_mode().clamp(0, 2) as usize].into();
    config.cpu_mode = ["low", "balanced", "fast"][w.get_cpu_index().clamp(0, 2) as usize].into();
    config.history_days = w.get_history_days().max(0) as u32;
    let login = w.get_launch_at_login();
    if login != autostart::enabled() {
        if let Err(e) = autostart::set(login) {
            notify("Could not change start at login", &e);
        }
    }
    UI.with(|ui| ui.borrow_mut().history.set_keep_days(config.history_days));
    w.set_history_subtitle(history_subtitle(config.history_days as i32).into());
    save_and_reload(config, "settings");
}

// ---- The talk key ---------------------------------------------------------------------------------

/// "Super+Space" for people: "Super + Space".
fn key_label(shortcut: &str) -> String {
    shortcut.split('+').map(str::trim).collect::<Vec<_>>().join(" + ")
}

#[derive(Debug, PartialEq)]
enum KeyCapture {
    /// A modifier on its own: keep listening.
    Ignore,
    /// Escape: stop listening, change nothing.
    Cancel,
    Shortcut(String),
}

/// Turns one key press in the window into a shortcut string `global-hotkey` and the portal accept.
fn shortcut_from_key(text: &str, ctrl: bool, alt: bool, shift: bool, meta: bool) -> KeyCapture {
    use slint::platform::Key;
    let named: &[(Key, &str)] = &[
        (Key::F1, "F1"), (Key::F2, "F2"), (Key::F3, "F3"), (Key::F4, "F4"), (Key::F5, "F5"), (Key::F6, "F6"),
        (Key::F7, "F7"), (Key::F8, "F8"), (Key::F9, "F9"), (Key::F10, "F10"), (Key::F11, "F11"), (Key::F12, "F12"),
        (Key::F13, "F13"), (Key::F14, "F14"), (Key::F15, "F15"), (Key::F16, "F16"), (Key::F17, "F17"), (Key::F18, "F18"),
        (Key::F19, "F19"), (Key::F20, "F20"), (Key::F21, "F21"), (Key::F22, "F22"), (Key::F23, "F23"), (Key::F24, "F24"),
        (Key::Insert, "Insert"), (Key::Delete, "Delete"), (Key::Home, "Home"), (Key::End, "End"),
        (Key::PageUp, "PageUp"), (Key::PageDown, "PageDown"), (Key::Pause, "Pause"), (Key::ScrollLock, "ScrollLock"),
        (Key::Return, "Enter"), (Key::Tab, "Tab"), (Key::Backspace, "Backspace"),
        (Key::UpArrow, "ArrowUp"), (Key::DownArrow, "ArrowDown"), (Key::LeftArrow, "ArrowLeft"), (Key::RightArrow, "ArrowRight"),
    ];
    let modifiers = [Key::Control, Key::ControlR, Key::Shift, Key::ShiftR, Key::Alt, Key::AltGr, Key::Meta, Key::MetaR, Key::CapsLock];
    let is = |k: Key| text == SharedString::from(k).as_str();
    if text.is_empty() || modifiers.iter().any(|k| is(*k)) {
        return KeyCapture::Ignore;
    }
    if is(Key::Escape) {
        return KeyCapture::Cancel;
    }
    let key = if let Some((_, name)) = named.iter().find(|(k, _)| is(*k)) {
        name.to_string()
    } else if text == " " {
        "Space".into()
    } else {
        let c = text.chars().next().unwrap_or(' ');
        if !c.is_ascii_graphic() {
            return KeyCapture::Ignore;
        }
        c.to_ascii_uppercase().to_string()
    };
    // A bare letter or digit would type into every app: require a modifier for those.
    let plain = key.len() == 1 || key == "Space" || key == "Enter" || key == "Tab" || key == "Backspace";
    if plain && !(ctrl || alt || meta) {
        return KeyCapture::Ignore;
    }
    let mut parts = Vec::new();
    if ctrl {
        parts.push("Ctrl");
    }
    if alt {
        parts.push("Alt");
    }
    if shift {
        parts.push("Shift");
    }
    if meta {
        parts.push("Super");
    }
    parts.push(&key);
    KeyCapture::Shortcut(parts.join("+"))
}

/// Saves the new talk key and binds it: at once where Voicio registers keys itself, through the
/// desktop's shortcut service elsewhere (which may need Voicio to start again).
fn rebind(w: &SettingsWindow, shortcut: &str) {
    let mut config = Config::load();
    if config.shortcut == shortcut {
        return;
    }
    config.shortcut = shortcut.to_string();
    w.set_key_label(key_label(shortcut).into());
    let note = UI.with(|ui| {
        let mut ui = ui.borrow_mut();
        let (Some(desktop), Some(tx)) = (ui.desktop, ui.tx.clone()) else { return String::new() };
        match desktop {
            Desktop::X11 | Desktop::Windows | Desktop::MacOs => {
                ui.hotkey = None; // drop the old registration first, or the new one is refused
                let registration = crate::hotkey::register(desktop, shortcut, tx);
                let ok = !matches!(registration, crate::hotkey::Registration::None);
                ui.hotkey = Some(registration);
                if ok { String::new() } else { format!("{} could not be used. Try another key.", key_label(shortcut)) }
            }
            _ => {
                #[cfg(target_os = "linux")]
                if desktop == Desktop::Gnome {
                    let _ = crate::gnome_keys::ensure(shortcut);
                }
                if crate::hotkey::portal_bound() {
                    "The new key works after Voicio restarts.".to_string()
                } else {
                    crate::hotkey::retry(shortcut, tx);
                    String::new()
                }
            }
        }
    });
    w.set_key_note(note.into());
    save_and_reload(config, "the talk key");
}

// ---- Packs and models -----------------------------------------------------------------------------

fn tier_of(config: &Config) -> usize {
    let tier = config.tier();
    voicio_models::Tier::ALL.iter().position(|t| *t == tier).unwrap_or(1)
}

fn bytes(n: u64) -> String {
    voicio_models::format_bytes(n)
}


/// The three packs as the window shows them (real sizes, memory and what is installed).
fn pack_rows(dir: &std::path::Path, recommended: usize) -> Vec<PackRow> {
    use voicio_models::{Kind, Tier};
    const MENU: [&str; 3] = ["Fastest", "Smart clean-up", "Most accurate"];
    Tier::ALL
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let missing: u64 = voicio_models::missing(dir, *t).iter().map(|m| m.download_bytes()).sum();
            let ram = bytes(t.ram_mb() as u64 * 1_048_576);
            let download = bytes(t.download_bytes());
            let mut parts: Vec<PartRow> = t
                .models()
                .filter(|m| m.kind != Kind::VoiceDetection)
                .map(|m| PartRow {
                    name: if m.kind == Kind::Cleanup { format!("{} clean-up", m.name) } else { m.name.to_string() }.into(),
                    size: bytes(m.download_bytes()).into(),
                })
                .collect();
            if !t.cleanup() {
                parts.push(PartRow { name: "Rule-based tidy-up".into(), size: "built in".into() });
            }
            let (acc, spd) = t.scores();
            let needs_16 = *t == Tier::Quality;
            PackRow {
                name: t.label().into(),
                line: t.line().into(),
                menu_sub: format!("{} · {ram} memory", MENU[i]).into(),
                chip_sub: format!(
                    "{} · {download} download{}",
                    if i == recommended { "Recommended" } else { MENU[i] },
                    if needs_16 { ", 16 GB RAM" } else { "" }
                )
                .into(),
                download: download.as_str().into(),
                memory: ram.as_str().into(),
                facts: if needs_16 { format!("{download} · needs 16 GB memory") } else { format!("{download} · {ram} memory") }.into(),
                parts: ModelRc::new(VecModel::from(parts)),
                acc,
                spd,
                have: missing == 0,
                get: bytes(missing).into(),
                busy: false,
                progress: 0.0,
            }
        })
        .collect()
}

/// Two letters and a tint for a speech model's badge.
fn mark_of(id: &str) -> (&'static str, slint::Color) {
    let c = |v: u32| slint::Color::from_rgb_u8((v >> 16) as u8, (v >> 8) as u8, v as u8);
    match id {
        "parakeet-110m" => ("P1", c(0xFFE3D3)),
        "parakeet-0.6b-v2" => ("P6", c(0xE2F0FF)),
        "parakeet-v3" => ("P3", c(0xE2F0FF)),
        "indic-hi" => ("IC", c(0xFFF0B8)),
        "indic-ta" => ("IC", c(0xFFE3D3)),
        "whisper-base" => ("Wb", c(0xEAF8D2)),
        "whisper-small" => ("Ws", c(0xEAF8D2)),
        "whisper-tiny-en" => ("Wt", c(0xEAF8D2)),
        "nemo-de" => ("FC", c(0xEBE4FF)),
        _ => ("··", c(0xEFE9DD)),
    }
}

/// The speech model the agent will load: an installed optional pack, else the pack's own model.
fn speech_in_use(config: &Config) -> &'static voicio_models::Model {
    let dir = config.models_dir();
    let id = match (config.speech_pack.as_str(), config.language.as_str()) {
        ("", "hi") => "indic-hi",
        (id, _) => id,
    };
    voicio_models::model(id)
        .filter(|m| m.optional && voicio_models::status(&dir, m) == voicio_models::Status::Installed)
        .unwrap_or_else(|| config.tier().models().find(|m| m.kind == voicio_models::Kind::Speech).expect("every pack has a speech model"))
}

fn lang_row(dir: &std::path::Path, m: &voicio_models::Model, in_use: &str, builtin: bool, added: bool) -> LangRow {
    let (mark, tint) = mark_of(m.id);
    let installed = voicio_models::status(dir, m) == voicio_models::Status::Installed;
    LangRow {
        id: m.id.into(),
        mark: mark.into(),
        tint,
        name: m.name.into(),
        sub: if builtin { format!("{} · by {} · included in your pack", m.languages, m.maker) } else { format!("{} · by {}", m.languages, m.maker) }.into(),
        size: format!("{} · {} memory", bytes(m.download_bytes()), bytes(m.ram_mb as u64 * 1_048_576)).into(),
        experimental: m.experimental,
        installed,
        in_use: m.id == in_use,
        builtin,
        busy: false,
        progress: 0.0,
        status: "".into(),
        added: added || installed,
    }
}

/// Rebuilds the packs, the language models and the sidebar switcher from the files on disk.
fn refresh_models(window: &SettingsWindow) {
    let config = Config::load();
    let dir = config.models_dir();
    let tier = config.tier();
    let recommended = voicio_models::Tier::ALL.iter().position(|t| *t == crate::models_cmd::recommended()).unwrap_or(1);
    let in_use = speech_in_use(&config);
    let pack_speech = tier.models().find(|m| m.kind == voicio_models::Kind::Speech).expect("every pack has a speech model");
    let mut langs = vec![lang_row(&dir, pack_speech, in_use.id, true, false)];
    langs.extend(voicio_models::library().map(|m| lang_row(&dir, m, in_use.id, false, false)));
    let added = UI.with(|ui| ui.borrow().onb_added.clone());
    let onb: Vec<LangRow> = voicio_models::library().map(|m| lang_row(&dir, m, "", false, added.iter().any(|a| a == m.id))).collect();
    UI.with(|ui| {
        let ui = ui.borrow();
        ui.packs.set_vec(pack_rows(&dir, recommended));
        ui.lang_rows.set_vec(langs);
        ui.onb_langs.set_vec(onb);
    });
    window.set_pack_index(tier_of(&config) as i32);
    window.set_recommended_pack(recommended as i32);
    window.set_pack_sub(
        match config.cloud_active() {
            Some(id) if config.cloud_mode == "always" => format!("Cloud: {}", crate::cloud::provider(id).map_or(id.to_string(), provider_title)),
            _ => format!("{} · works offline", in_use.languages.split([':', ',']).next().unwrap_or("English")),
        }
        .into(),
    );
    window.set_models_busy(UI.with(|ui| ui.borrow().install_cancel.is_some()));
    window.set_onb_total(onb_total(window).into());
}

fn refresh_open_window() {
    if let Some(w) = UI.with(|ui| ui.borrow().settings.as_ref().map(|w| w.clone_strong())) {
        refresh_models(&w);
    }
}

/// Switches to pack `index` (the same `config.tier` + reload the Models page always used), and
/// downloads what it is missing.
fn pick_pack(index: usize) {
    let tier = voicio_models::Tier::ALL[index];
    let mut config = Config::load();
    let missing = voicio_models::missing(&config.models_dir(), tier);
    config.tier = tier.name().to_lowercase();
    save_and_reload(config, "the pack");
    if !missing.is_empty() && UI.with(|ui| ui.borrow().install_cancel.is_none()) {
        start_install(missing, Some(index));
    }
    refresh_open_window();
}

/// "Use" on a language model: the agent loads it for the next dictation.
fn use_language_model(id: &str) {
    let Some(model) = voicio_models::model(id) else { return };
    let mut config = Config::load();
    config.speech_pack = if model.optional { id.to_string() } else { String::new() };
    config.language = "auto".into(); // the model decides; "hi" would override the choice
    save_and_reload(config, "the language model");
    refresh_open_window();
}

fn cancel_install() {
    UI.with(|ui| {
        if let Some(cancel) = &ui.borrow().install_cancel {
            cancel.cancel();
        }
    })
}

/// Downloads `targets` one after another on a worker thread, reporting each file's progress to the
/// Models page, the pack cards and setup's Download step.
fn start_install(targets: Vec<&'static voicio_models::Model>, tier: Option<usize>) {
    if targets.is_empty() || UI.with(|ui| ui.borrow().install_cancel.is_some()) {
        return;
    }
    let dir = Config::load().models_dir();
    let cancel = voicio_models::Cancel::default();
    UI.with(|ui| {
        let mut ui = ui.borrow_mut();
        ui.install_cancel = Some(cancel.clone());
        ui.install_tier = tier;
        ui.install_paused = false;
        ui.dl_rows.set_vec(targets.iter().map(|m| dl_row(m)).collect::<Vec<_>>());
        if let Some(window) = &ui.settings {
            window.set_models_busy(true);
            window.set_models_message("".into());
            window.set_dl_paused(false);
            window.set_dl_error("".into());
            window.set_dl_done(false);
        }
    });
    let all_bytes: u64 = targets.iter().map(|m| m.download_bytes()).sum::<u64>().max(1);
    let started = std::time::Instant::now();
    std::thread::Builder::new()
        .name("model-download".into())
        .spawn(move || {
            let mut error = String::new();
            let mut finished: u64 = 0;
            for model in targets {
                let before = finished;
                let mut report = |p: voicio_models::Progress| {
                    let fraction = if p.total > 0 { p.done as f32 / p.total as f32 } else { 0.0 };
                    let done = before + (fraction * model.download_bytes() as f32) as u64;
                    let overall = (done as f32 / all_bytes as f32).min(1.0);
                    let elapsed = started.elapsed().as_secs_f32();
                    let left = if overall > 0.02 && elapsed > 2.0 { remaining(elapsed / overall - elapsed) } else { format!("{} to go", bytes(all_bytes - done.min(all_bytes))) };
                    let text = format!("{} {:.0}%", p.phase, fraction * 100.0);
                    let _ = slint::invoke_from_event_loop(move || show_progress(model.id, fraction, &text, overall, &left));
                };
                if let Err(e) = voicio_models::install(&dir, model, &cancel, &mut report) {
                    error = e.to_string();
                    break;
                }
                finished += model.download_bytes();
                let id = model.id;
                let _ = slint::invoke_from_event_loop(move || show_progress(id, 1.0, "Done", (finished as f32 / all_bytes as f32).min(1.0), ""));
            }
            let _ = slint::invoke_from_event_loop(move || {
                let paused = UI.with(|ui| {
                    let mut ui = ui.borrow_mut();
                    ui.install_cancel = None;
                    ui.install_tier = None;
                    ui.install_paused
                });
                let error = if paused { String::new() } else { error };
                UI.with(|ui| {
                    if let Some(window) = &ui.borrow().settings {
                        window.set_models_message(error.as_str().into());
                        window.set_dl_error(error.as_str().into());
                        window.set_dl_paused(paused);
                        window.set_dl_done(error.is_empty() && !paused);
                        if error.is_empty() && !paused {
                            window.set_dl_fraction(1.0);
                        }
                    }
                });
                refresh_open_window();
            });
        })
        .ok();
}

fn remaining(seconds: f32) -> String {
    let s = seconds.max(1.0).round() as u32;
    if s < 60 {
        format!("About {s} seconds left")
    } else {
        format!("About {} minutes left", s.div_ceil(60))
    }
}

/// One file row of setup's Download step.
fn dl_row(m: &voicio_models::Model) -> DlRow {
    use voicio_models::Kind;
    let c = |v: u32| slint::Color::from_rgb_u8((v >> 16) as u8, (v >> 8) as u8, v as u8);
    let (name, glyph, tint) = match (m.kind, m.optional) {
        (Kind::VoiceDetection, _) => ("Voice detection".to_string(), "1", c(0xEAF8D2)),
        (Kind::Cleanup, _) => ("Smart clean-up".to_string(), "3", c(0xE2F0FF)),
        (Kind::Speech, false) => ("Speech recognition".to_string(), "2", c(0xFFE3D3)),
        (Kind::Speech, true) => (m.languages.split([':', ',']).next().unwrap_or(m.languages).to_string(), "+", c(0xFFF0B8)),
    };
    DlRow { id: m.id.into(), name: name.into(), detail: format!("{} · {}", m.name, bytes(m.download_bytes())).into(), glyph: glyph.into(), tint, progress: 0.0, state: "Waiting".into() }
}

fn show_progress(id: &str, fraction: f32, text: &str, overall: f32, left: &str) {
    UI.with(|ui| {
        let ui = ui.borrow();
        for rows in [ui.lang_rows.clone(), ui.onb_langs.clone()] {
            for i in 0..rows.row_count() {
                if let Some(mut row) = rows.row_data(i) {
                    if row.id == id {
                        row.busy = fraction < 1.0;
                        row.progress = fraction;
                        row.status = text.into();
                        rows.set_row_data(i, row);
                    }
                }
            }
        }
        for i in 0..ui.dl_rows.row_count() {
            if let Some(mut row) = ui.dl_rows.row_data(i) {
                if row.id == id {
                    row.progress = fraction;
                    row.state = if fraction >= 1.0 { "Done".into() } else { format!("{:.0}%", fraction * 100.0).into() };
                    ui.dl_rows.set_row_data(i, row);
                }
            }
        }
        if let Some(t) = ui.install_tier {
            if let Some(mut row) = ui.packs.row_data(t) {
                row.busy = true;
                row.progress = overall;
                ui.packs.set_row_data(t, row);
            }
        }
        if let Some(w) = &ui.settings {
            w.set_dl_fraction(overall);
            if !left.is_empty() {
                w.set_dl_left(left.into());
            }
        }
    });
}

fn remove_model(id: &str) {
    let Some(model) = voicio_models::model(id) else { return };
    let mut config = Config::load();
    let dir = config.models_dir();
    for file in model.files() {
        let path = dir.join(file.path);
        let _ = std::fs::remove_file(&path);
        if let Some(parent) = path.parent() {
            if parent != dir {
                let _ = std::fs::remove_dir(parent); // only succeeds when empty
            }
        }
    }
    if config.speech_pack == id {
        config.speech_pack.clear();
        save_and_reload(config, "the language model");
    }
    refresh_open_window();
}

/// Credits the licences ask for (About › Built on), from the catalogue.
fn credits() -> Vec<CreditRow> {
    let mut rows: Vec<CreditRow> = voicio_models::CATALOG
        .iter()
        .map(|m| CreditRow { name: m.name.into(), detail: format!("{} · {}", m.maker, m.license).into() })
        .collect();
    rows.push(CreditRow { name: "sherpa-onnx".into(), detail: "k2-fsa · Apache-2.0".into() });
    rows.push(CreditRow { name: "llama.cpp".into(), detail: "ggml-org · MIT".into() });
    rows.push(CreditRow { name: "Slint".into(), detail: "SixtyFPS GmbH · royalty-free licence".into() });
    rows.push(CreditRow { name: "Inter and Bricolage Grotesque".into(), detail: "Fonts · SIL Open Font License".into() });
    rows
}

/// "This computer has 16 GB of memory, so any pack fits." for the Pack step.
fn memory_note() -> String {
    let mut system = sysinfo::System::new();
    system.refresh_memory();
    let gb = (system.total_memory() as f64 / 1_073_741_824.0).round() as u64;
    let fits = voicio_models::Tier::ALL.iter().filter(|t| (t.ram_mb() as u64) * 4 <= gb * 1024).count();
    match fits {
        3 => format!("This computer has {gb} GB of memory, so any pack fits. Switch any time from the sidebar."),
        2 => format!("This computer has {gb} GB of memory: Lite and Balanced fit well. Switch any time from the sidebar."),
        _ => format!("This computer has {gb} GB of memory: Lite is the best fit. Switch any time from the sidebar."),
    }
}

/// Input devices for the microphone choice: "System default" first, then each device by name.
fn microphones() -> Vec<String> {
    let mut names = vec!["System default".to_string()];
    names.extend(crate::audio::input_names());
    names
}

// ---- Cloud ----------------------------------------------------------------------------------------

/// The ten providers shown first, in this order; the rest follow under "More providers".
/// ponytail: a fixed list until the registry's own `featured`/`order` fields land in cloud.rs; then sort by those.
const FEATURED: [&str; 10] = ["groq", "openai", "gemini", "deepgram", "elevenlabs", "mistral", "assemblyai", "azure", "sarvam", "soniox"];

/// The providers in the order the Cloud manager shows them.
fn cloud_order() -> Vec<&'static crate::cloud::Provider> {
    let mut all: Vec<&crate::cloud::Provider> = crate::cloud::providers().iter().collect();
    all.sort_by_key(|p| FEATURED.iter().position(|f| *f == p.id).unwrap_or(FEATURED.len()));
    all
}

/// The provider's own name for the Cloud manager (provider first, the model is a setting inside it):
/// the maker, "Google Gemini" for Google, and the name's qualifier when a maker has two entries.
fn provider_title(p: &crate::cloud::Provider) -> String {
    let maker = if p.maker == "Google" { "Google Gemini".to_string() } else { p.maker.clone() };
    let twins = crate::cloud::providers().iter().filter(|o| o.maker == p.maker).count() > 1;
    match p.name.rsplit_once('(').filter(|_| twins) {
        Some((_, tail)) => format!("{maker} ({tail}"),
        None => maker,
    }
}

/// The provider behind row `index` of the Cloud manager.
fn cloud_at(index: i32) -> Option<&'static crate::cloud::Provider> {
    cloud_order().get(index.max(0) as usize).copied()
}

fn cloud_rows(config: &Config) -> Vec<CloudRow> {
    let c = |v: u32| slint::Color::from_rgb_u8((v >> 16) as u8, (v >> 8) as u8, v as u8);
    const TINTS: [u32; 6] = [0xFFE3D3, 0xE2F0FF, 0xEAF8D2, 0xFFF0B8, 0xEBE4FF, 0xE8E2D6];
    cloud_order()
        .into_iter()
        .enumerate()
        .map(|(i, p)| {
            let chosen = Some(config.cloud_model_for(&p.id)).filter(|m| !m.is_empty()).unwrap_or(p.default_model.as_str());
            let title = provider_title(p);
            let mark: String = {
                let caps: String = title.split_whitespace().filter_map(|w| w.chars().next()).filter(|c| c.is_alphanumeric()).take(2).collect();
                if caps.chars().count() == 2 { caps } else { title.chars().take(2).collect() }
            };
            CloudRow {
                id: p.id.as_str().into(),
                title: provider_title(p).into(),
                name: p.name.as_str().into(),
                maker: p.maker.as_str().into(),
                mark: mark.into(),
                tint: c(TINTS[i % TINTS.len()]),
                price: p.price_note.as_str().into(),
                speed: p.speed_note.as_str().into(),
                host: p.host_for(&config.cloud_region).into(),
                models: ModelRc::new(VecModel::from(p.models.iter().map(|m| SharedString::from(m.as_str())).collect::<Vec<_>>())),
                model_index: p.models.iter().position(|m| m == chosen).unwrap_or(0) as i32,
                needs_region: p.needs_region,
                has_key: crate::cloud::load_key(&p.id).is_some(),
                slow: p.slow,
                live_supported: p.live_supported(),
                live_experimental: p.live.as_ref().is_some_and(|l| l.experimental),
                live_on: config.cloud_live.contains(&p.id),
            }
        })
        .collect()
}

fn refresh_cloud(window: &SettingsWindow) {
    let config = Config::load();
    let rows = cloud_rows(&config);
    let chosen = rows.iter().position(|r| r.id == config.cloud_provider.as_str());
    UI.with(|ui| ui.borrow().cloud_rows.set_vec(rows));
    if let Some(i) = chosen {
        window.set_cloud_index(i as i32);
    }
    window.set_cloud_default(config.cloud_active().unwrap_or("").into());
    window.set_cloud_featured(FEATURED.iter().filter(|f| crate::cloud::provider(f).is_some()).count() as i32);
    let excluded: Vec<String> = crate::cloud::excluded().iter().map(|e| e.name.to_string()).collect();
    window.set_cloud_excluded(if excluded.is_empty() { String::new() } else { format!("Not offered (they need more than an API key): {}.", excluded.join(", ")) }.into());
}

fn cloud_status(w: &SettingsWindow, text: &str, kind: &str) {
    w.set_cloud_status(text.into());
    w.set_cloud_status_kind(kind.into());
}

fn setup_cloud(window: &SettingsWindow) {
    let weak = window.as_weak();
    window.on_cloud_pick(move |_| {
        if let Some(w) = weak.upgrade() {
            cloud_status(&w, "", "");
        }
    });
    window.on_cloud_model(|index, model| {
        let Some(p) = cloud_at(index) else { return };
        let Some(m) = p.models.get(model.max(0) as usize) else { return };
        let mut config = Config::load();
        config.cloud_models.insert(p.id.clone(), m.clone());
        if config.cloud_provider == p.id {
            config.cloud_model = m.clone();
        }
        save_and_reload(config, "the cloud model");
        UI.with(|ui| {
            let ui = ui.borrow();
            if let Some(mut row) = ui.cloud_rows.row_data(index as usize) {
                row.model_index = model;
                ui.cloud_rows.set_row_data(index as usize, row);
            }
        });
    });
    let weak = window.as_weak();
    window.on_cloud_add_key(move |index, key, region| {
        let Some(p) = cloud_at(index) else { return };
        let Some(w) = weak.upgrade() else { return };
        let model = UI.with(|ui| ui.borrow().cloud_rows.row_data(index as usize).and_then(|r| p.models.get(r.model_index.max(0) as usize).cloned())).unwrap_or_else(|| p.default_model.clone());
        cloud_status(&w, "Testing the key…", "busy");
        let (id, key, region) = (p.id.clone(), key.to_string(), region.trim().to_string());
        let weak = w.as_weak();
        std::thread::spawn(move || {
            let outcome = crate::cloud::test_key(&id, &key, &model, &region)
                .map_err(|e| format!("Key rejected: {e}"))
                .and_then(|()| crate::cloud::save_key(&id, &key).map_err(|e| format!("Could not store the key: {e}")));
            let _ = weak.upgrade_in_event_loop(move |w| match outcome {
                Ok(()) => {
                    let mut config = Config::load();
                    // The first key you add becomes the default, so "All dictations" just works.
                    if config.cloud_active().is_none() || config.cloud_provider.is_empty() || config.cloud_provider == id {
                        config.cloud_provider = id.clone();
                        config.cloud_consented = id.clone();
                        config.cloud_model = model.clone();
                    }
                    config.cloud_models.insert(id.clone(), model);
                    if !region.is_empty() {
                        config.cloud_region = region;
                    }
                    save_and_reload(config, "the cloud provider");
                    refresh_cloud(&w);
                    cloud_status(&w, "Connected. The key works.", "ok");
                }
                Err(message) => cloud_status(&w, &message, "error"),
            });
        });
    });
    let weak = window.as_weak();
    window.on_cloud_set_default(move |index| {
        let Some(p) = cloud_at(index) else { return };
        let Some(w) = weak.upgrade() else { return };
        let model = UI.with(|ui| ui.borrow().cloud_rows.row_data(index as usize).and_then(|r| p.models.get(r.model_index.max(0) as usize).cloned())).unwrap_or_else(|| p.default_model.clone());
        let mut config = Config::load();
        config.cloud_provider = p.id.clone();
        config.cloud_consented = p.id.clone(); // consent was given when the key was added
        config.cloud_models.insert(p.id.clone(), model.clone());
        config.cloud_model = model;
        if p.needs_region {
            config.cloud_region = w.get_cloud_region().to_string();
        }
        if config.cloud_mode == "off" {
            config.cloud_mode = "always".into();
            w.set_cloud_mode(1);
        }
        save_and_reload(config, "the cloud provider");
        refresh_cloud(&w);
        refresh_models(&w);
        cloud_status(&w, &format!("{} is now the default.", provider_title(p)), "ok");
    });
    let weak = window.as_weak();
    window.on_cloud_remove_key(move |index| {
        let Some(p) = cloud_at(index) else { return };
        let Some(w) = weak.upgrade() else { return };
        crate::cloud::delete_key(&p.id);
        let mut config = Config::load();
        if config.cloud_provider == p.id {
            config.cloud_provider.clear();
            config.cloud_consented.clear();
            config.cloud_model.clear();
        }
        save_and_reload(config, "the cloud provider");
        refresh_cloud(&w);
        refresh_models(&w);
        cloud_status(&w, "Key removed.", "");
    });
    window.on_cloud_live(|index, on| {
        let Some(p) = cloud_at(index) else { return };
        let mut config = Config::load();
        config.cloud_live.retain(|l| *l != p.id);
        if on {
            config.cloud_live.push(p.id.clone());
        }
        save_and_reload(config, "the live setting");
        UI.with(|ui| {
            let ui = ui.borrow();
            if let Some(mut row) = ui.cloud_rows.row_data(index.max(0) as usize) {
                row.live_on = on;
                ui.cloud_rows.set_row_data(index as usize, row);
            }
        });
    });
    let weak = window.as_weak();
    window.on_cloud_mode_changed(move |mode| {
        let Some(w) = weak.upgrade() else { return };
        let mut config = Config::load();
        config.cloud_mode = ["off", "always", "fallback"][mode.clamp(0, 2) as usize].into();
        save_and_reload(config.clone(), "the cloud setting");
        refresh_cloud(&w);
        refresh_models(&w);
        if mode > 0 && config.cloud_provider.is_empty() {
            cloud_status(&w, "Add a key below and make that provider the default.", "");
        }
    });
}

// ---- First-run setup ------------------------------------------------------------------------------

fn set_download_state(text: &str, error: &str, done: bool) {
    UI.with(|ui| {
        if let Some(window) = &ui.borrow().settings {
            window.set_dl_left(text.into());
            window.set_dl_error(error.into());
            window.set_dl_done(done);
        }
    });
}

/// What setup's Pack step will fetch: the pack's missing files plus the added language models.
fn onb_targets(window: &SettingsWindow) -> Vec<&'static voicio_models::Model> {
    let dir = Config::load().models_dir();
    let tier = voicio_models::Tier::ALL[window.get_onb_pack().clamp(0, 2) as usize];
    let mut targets = voicio_models::missing(&dir, tier);
    for id in UI.with(|ui| ui.borrow().onb_added.clone()) {
        if let Some(m) = voicio_models::model(&id).filter(|m| voicio_models::status(&dir, m) != voicio_models::Status::Installed) {
            targets.push(m);
        }
    }
    targets
}

fn onb_total(window: &SettingsWindow) -> String {
    let total: u64 = onb_targets(window).iter().map(|m| m.download_bytes()).sum();
    if total == 0 { String::new() } else { bytes(total) }
}

fn summary_pack(window: &SettingsWindow) -> String {
    let tier = voicio_models::Tier::ALL[window.get_onb_pack().clamp(0, 2) as usize];
    let added: Vec<String> = UI.with(|ui| ui.borrow().onb_added.clone())
        .iter()
        .filter_map(|id| voicio_models::model(id))
        .map(|m| m.languages.split([':', ',']).next().unwrap_or(m.name).trim().to_string())
        .collect();
    if added.is_empty() { tier.label().to_string() } else { format!("{} + {}", tier.label(), added.join(", ")) }
}

/// Wires the setup's callbacks.
fn setup_onboarding(window: &SettingsWindow) {
    let weak = window.as_weak();
    window.on_onb_enter(move |step| {
        if let Some(w) = weak.upgrade() {
            onboarding_enter(&w, step);
        }
    });
    let weak = window.as_weak();
    window.on_onb_lang_toggle(move |id| {
        let Some(w) = weak.upgrade() else { return };
        UI.with(|ui| {
            let mut ui = ui.borrow_mut();
            match ui.onb_added.iter().position(|a| a == id.as_str()) {
                Some(i) => drop(ui.onb_added.remove(i)),
                None => ui.onb_added.push(id.to_string()),
            }
        });
        refresh_models(&w);
    });
    let weak = window.as_weak();
    window.on_dl_pause(move || {
        UI.with(|ui| ui.borrow_mut().install_paused = true);
        cancel_install();
        if let Some(w) = weak.upgrade() {
            w.set_dl_paused(true);
        }
    });
    let weak = window.as_weak();
    window.on_onb_finish(move || {
        let Some(w) = weak.upgrade() else { return };
        onboarding_leave(-1);
        let mut config = Config::load();
        config.onboarded = true;
        save_and_reload(config, "the setup");
        if w.get_onb_step() == 5 && w.get_onb_login() != autostart::enabled() {
            if let Err(e) = autostart::set(w.get_onb_login()) {
                notify("Could not change start at login", &e);
            }
            w.set_launch_at_login(autostart::enabled());
        }
        w.set_onb_step(-1);
        w.set_page(0);
    });
}

/// Stops the microphone check, and the download only when going back to before it.
fn onboarding_leave(next: i32) {
    UI.with(|ui| ui.borrow_mut().mic = None);
    if (0..4).contains(&next) {
        cancel_install();
    }
}

/// Runs when a setup step opens: starts the microphone meter or the download, or refreshes data.
fn onboarding_enter(window: &SettingsWindow, step: i32) {
    onboarding_leave(step);
    let config = Config::load();
    match step {
        0 => {
            window.set_onb_pack(tier_of(&config) as i32);
            window.set_onb_login(true);
            window.set_dl_error("".into());
            window.set_dl_done(false);
            window.set_dl_paused(false);
            UI.with(|ui| ui.borrow_mut().onb_added.clear());
            refresh_models(window);
        }
        1 => start_mic_check(window),
        2 => {
            window.set_key_note(match crate::hotkey::fallback() {
                Some(crate::hotkey::Fallback::Active { note: Some(note), .. }) => note,
                Some(crate::hotkey::Fallback::Failed(e)) => format!("The desktop refused this key ({e}). Pick another one."),
                _ => String::new(),
            }.into());
        }
        3 => refresh_models(window),
        4 => {
            let tier = voicio_models::Tier::ALL[window.get_onb_pack().clamp(0, 2) as usize];
            let mut config = config;
            if config.tier() != tier || config.tier == "auto" {
                config.tier = tier.name().to_lowercase();
                save_and_reload(config, "the pack");
            }
            let targets = onb_targets(window);
            window.set_dl_error("".into());
            window.set_dl_paused(false);
            if targets.is_empty() {
                UI.with(|ui| ui.borrow().dl_rows.set_vec(tier.models().map(|m| DlRow { progress: 1.0, state: "Done".into(), ..dl_row(m) }).collect::<Vec<_>>()));
                window.set_dl_fraction(1.0);
                set_download_state("", "", true);
            } else {
                window.set_dl_fraction(0.0);
                set_download_state("Starting", "", false);
                cancel_install(); // a pack picked while another download ran: start over with the new one
                let weak = window.as_weak();
                // the cancelled download (if any) clears itself on the event loop first
                slint::Timer::single_shot(Duration::from_millis(50), move || {
                    if let Some(w) = weak.upgrade() {
                        let index = w.get_onb_pack().clamp(0, 2) as usize;
                        start_install(onb_targets(&w), Some(index));
                    }
                });
            }
            refresh_models(window);
        }
        5 => {
            let mic = microphones().get(window.get_mic_index().max(0) as usize).cloned().unwrap_or_default();
            window.set_summary_mic(if window.get_mic_index() <= 0 { "System default".into() } else { mic.into() });
            window.set_summary_pack(summary_pack(window).into());
            window.set_onb_login(true);
        }
        _ => {}
    }
}

fn start_mic_check(window: &SettingsWindow) {
    use crate::miclevel::{display_level, Guidance, Meter};
    use slint::{Timer, TimerMode};
    UI.with(|ui| ui.borrow_mut().mic = None); // close the previous stream first
    window.set_mic_state("waiting".into());
    window.set_mic_level(0.0);
    UI.with(|ui| ui.borrow().mic_bars.set_vec(vec![0.0f32; 24]));
    let timer = Timer::default();
    let weak = window.as_weak();
    let mut shown = 0.0f32;
    let mut tick = 0u32;
    timer.start(TimerMode::Repeated, Duration::from_millis(33), move || {
        let Some(w) = weak.upgrade() else { return };
        tick += 1;
        UI.with(|ui| {
            let mut ui = ui.borrow_mut();
            let Some((meter, guidance, _)) = ui.mic.as_mut() else { return };
            let peak = meter.take_peak();
            let state = guidance.push(peak, meter.error().is_some());
            let target = display_level(peak);
            shown = if target > shown { target } else { shown * 0.85 }; // quick attack, smooth fall
            w.set_mic_level(shown);
            w.set_mic_state(state.key().into());
            if tick.is_multiple_of(3) {
                // the bars scroll left: newest on the right
                let bars = &ui.mic_bars;
                for i in 0..bars.row_count().saturating_sub(1) {
                    bars.set_row_data(i, bars.row_data(i + 1).unwrap_or(0.0));
                }
                if bars.row_count() > 0 {
                    bars.set_row_data(bars.row_count() - 1, shown);
                }
            }
        });
    });
    let device = Config::load().input_device;
    UI.with(|ui| ui.borrow_mut().mic = Some((Meter::start(&device), Guidance::new(), timer)));
}

/// Developer tool (`voicio ui-snapshot PAGE out.ppm`): draws the app window on the chosen page and
/// saves a picture, so the design can be checked without a human screenshot. Pages: 0 Home,
/// 1 History, 2 Models, 3 Settings, 4 About, 5 Dictionary, 10..15 the six setup steps.
/// Fixtures stand in for history and the dictionary; packs, models and cloud providers are the real
/// catalogue. Env: VOICIO_SNAP_SIZE=WxH, VOICIO_SNAP_MENU=sidebar|chip, VOICIO_SNAP_MIC=ok|quiet|none|waiting,
/// VOICIO_SNAP_DL=0.46, VOICIO_SNAP_EMPTY=1 (fresh install), VOICIO_SNAP_HEALTH=1 (a health banner),
/// VOICIO_SNAP_CLOUD=N (open provider N; VOICIO_SNAP_LIVE=1 turns its Live switch on), VOICIO_SNAP_CAPTURE=1 (the key waiting for a press).
pub fn snapshot(page: i32, path: std::path::PathBuf, dark: bool) -> Result<(), String> {
    let _ = dark; // the design is light only
    select_backend().map_err(|e| e.to_string())?;
    let window = SettingsWindow::new().map_err(|e| e.to_string())?;
    let config = Config::load();
    bind_models(&window);
    load_settings(&window, &config);
    refresh_models(&window);
    refresh_cloud(&window);
    window.set_page(if page >= 10 { 0 } else { page });
    window.set_models("Models loaded · 182 MB".into());
    let env = |k: &str| std::env::var(k).ok();
    let empty = env("VOICIO_SNAP_EMPTY").is_some();
    if env("VOICIO_SNAP_HEALTH").is_some() {
        window.set_health_problem("Voicio is not allowed to type for you yet.".into());
        window.set_health_fix("Press the button and choose Allow when the desktop asks.".into());
        window.set_health_action("Allow typing".into());
    }
    if !empty {
        let now = crate::history::now();
        let mut history = crate::history::History::in_memory();
        // fixtures: our own sample sentences, at fixed offsets from now
        for (ago, text) in [
            (26 * 3600, "Move the release review to Monday and ask Sam for the final screenshots."),
            (3 * 3600, "Hi Dana,\nThe draft looks good. I left two comments on the pricing section.\nThanks, Alex"),
            (40 * 60, "• Bread\n• Coffee beans\n• Batteries for the remote"),
            (120, "Please send the report to Priya before noon, and remind her about the Thursday review."),
        ] {
            history.add_at(text, now - ago);
        }
        let found = history.search("");
        let offset = local_offset();
        UI.with(|ui| {
            let ui = ui.borrow();
            ui.history_rows.set_vec(history_rows(&found, 0, now, offset));
            ui.recent_rows.set_vec(history_rows(&found[..3], 0, now, offset));
            ui.dict_rows.set_vec(vec![
                DictRow { phrase: "voysio".into(), output: "Voicio".into() },
                DictRow { phrase: "my email".into(), output: "alex@example.com".into() },
                DictRow { phrase: "cube cuttle".into(), output: "kubectl".into() },
                DictRow { phrase: "jay son".into(), output: "JSON".into() },
                DictRow { phrase: "post gress".into(), output: "Postgres".into() },
            ]);
        });
        window.set_history_total(found.len() as i32);
    } else {
        window.set_history_total(0);
    }
    match env("VOICIO_SNAP_MENU").as_deref() {
        Some("sidebar") => window.set_menu_open(true),
        Some("chip") => window.set_chip_open(true),
        _ => {}
    }
    if env("VOICIO_SNAP_CAPTURE").is_some() {
        window.set_capturing(true);
    }
    if let Some(i) = env("VOICIO_SNAP_CLOUD").and_then(|v| v.parse::<i32>().ok()) {
        window.set_cloud_index(i);
        window.set_cloud_mode(1);
        // VOICIO_SNAP_LIVE=1: that provider's Live switch on
        if env("VOICIO_SNAP_LIVE").is_some() {
            UI.with(|ui| {
                let ui = ui.borrow();
                if let Some(mut row) = ui.cloud_rows.row_data(i.max(0) as usize) {
                    row.live_on = true;
                    ui.cloud_rows.set_row_data(i as usize, row);
                }
            });
        }
    }
    if page >= 10 {
        let step = page - 10;
        window.set_onb_step(step);
        window.set_onb_pack(1);
        let mic = env("VOICIO_SNAP_MIC").unwrap_or_else(|| "ok".into());
        window.set_mic_state(mic.as_str().into());
        let level = if mic == "ok" { 0.75 } else if mic == "quiet" { 0.18 } else { 0.0 };
        window.set_mic_level(level);
        let bars: Vec<f32> = (0..24).map(|i| if mic == "none" { 0.0 } else { level * (0.35 + 0.65 * ((i as f32 * 0.7).sin() * (i as f32 * 0.23).cos()).abs()) }).collect();
        UI.with(|ui| ui.borrow().mic_bars.set_vec(bars));
        UI.with(|ui| ui.borrow_mut().onb_added = vec!["indic-hi".into()]);
        refresh_models(&window);
        let fraction: f32 = env("VOICIO_SNAP_DL").and_then(|v| v.parse().ok()).unwrap_or(0.46);
        let tier = voicio_models::Tier::ALL[1];
        let mut targets: Vec<&voicio_models::Model> = tier.models().collect();
        targets.extend(voicio_models::model("indic-hi"));
        let total: u64 = targets.iter().map(|m| m.download_bytes()).sum();
        let mut before = 0u64;
        let rows: Vec<DlRow> = targets
            .iter()
            .map(|m| {
                let start = before as f32 / total as f32;
                before += m.download_bytes();
                let end = before as f32 / total as f32;
                let p = ((fraction - start) / (end - start)).clamp(0.0, 1.0);
                DlRow { progress: p, state: if p >= 1.0 { "Done".into() } else if p > 0.0 { format!("{:.0}%", p * 100.0).into() } else { "Waiting".into() }, ..dl_row(m) }
            })
            .collect();
        UI.with(|ui| ui.borrow().dl_rows.set_vec(rows));
        window.set_dl_fraction(fraction);
        window.set_dl_done(fraction >= 1.0);
        window.set_dl_left("About 40 seconds left".into());
        window.set_summary_mic("System default".into());
        window.set_summary_pack(summary_pack(&window).into());
    }
    window.show().map_err(|e| e.to_string())?;
    if let Some((w, h)) = env("VOICIO_SNAP_SIZE").and_then(|v| v.split_once('x').and_then(|(w, h)| Some((w.parse().ok()?, h.parse().ok()?)))) {
        window.window().set_size(slint::LogicalSize::new(w, h));
    }
    let weak = window.as_weak();
    slint::Timer::single_shot(std::time::Duration::from_millis(900), move || {
        if let Some(window) = weak.upgrade() {
            if let Some(y) = std::env::var("VOICIO_SNAP_SCROLLY").ok().and_then(|v| v.parse::<f32>().ok()) {
                window.set_page_y(-y);
            }
            if env_scroll_report() {
                eprintln!("scroll-report page={} content={} view={}", window.get_page(), window.get_page_content_h(), window.get_page_view_h());
            }
            match window.window().take_snapshot() {
                Ok(buffer) => {
                    let mut out = format!("P6\n{} {}\n255\n", buffer.width(), buffer.height()).into_bytes();
                    for pixel in buffer.as_slice() {
                        out.extend_from_slice(&[pixel.r, pixel.g, pixel.b]);
                    }
                    if let Err(e) = std::fs::write(&path, out) {
                        eprintln!("voicio: snapshot write failed: {e}");
                    }
                }
                Err(e) => eprintln!("voicio: snapshot failed: {e}"),
            }
        }
        let _ = slint::quit_event_loop();
    });
    slint::run_event_loop().map_err(|e| e.to_string())
}

fn env_scroll_report() -> bool {
    std::env::var_os("VOICIO_SNAP_SCROLL").is_some()
}

/// Hides now, frees the window on the next loop turn (never drop a component inside its own callback).
fn close_settings() {
    UI.with(|ui| {
        if let Some(window) = &ui.borrow().settings {
            let _ = window.hide();
        }
    });
    UI.with(|ui| ui.borrow_mut().mic = None);
    slint::Timer::single_shot(std::time::Duration::ZERO, || UI.with(|ui| drop(ui.borrow_mut().settings.take())));
}

/// Developer tool (`voicio overlay-snapshot out.ppm "words" [MODE]`): runs the real pill path
/// (events, meter, words, animation) and saves the overlay over the app's paper colour.
/// MODE: "" listening, "quiet" (no voice), "noise" (a noisy room), "working", "typed",
/// "warn:TEXT" (low memory), "error:TEXT" (a problem), "info:TEXT".
pub fn overlay_snapshot(path: std::path::PathBuf, words: String, mode: String) -> Result<(), String> {
    select_backend().map_err(|e| e.to_string())?;
    let feed = slint::Timer::default();
    let mut tick = 0u32;
    let notice = mode.split_once(':').map(|(k, t)| (k.to_string(), t.to_string()));
    let (working, typed) = (mode == "working", mode == "typed");
    let (quiet, noise) = (mode == "quiet", mode == "noise");
    {
        let (words, notice) = (words.clone(), notice.clone());
        slint::Timer::single_shot(Duration::from_millis(20), move || {
            let _ = update_overlay("listening", None);
            set_words(words);
            if let Some((kind, text)) = notice {
                let kind = match kind.as_str() {
                    "error" => NoticeKind::Error,
                    "info" => NoticeKind::Info,
                    _ => NoticeKind::Warn,
                };
                with_pill(|p| {
                    p.notice = Some((kind, text));
                    p.state = "notice";
                });
                pill_handle(PillEvent::Notice(kind));
            }
        });
    }
    feed.start(slint::TimerMode::Repeated, Duration::from_millis(25), move || {
        tick += 1;
        let level = if quiet {
            0.0015
        } else if noise {
            0.03 * (1.0 + 0.05 * ((tick as f32) * 0.7).sin())
        } else {
            0.015 + 0.09 * (((tick as f32) * 0.37).sin() * ((tick as f32) * 0.11).cos()).abs()
        };
        pill_handle(PillEvent::Meter(Bands { level, low: level * 1.1, mid: level * 0.9, high: level * 0.5 }));
    });
    std::mem::forget(feed);
    let wait = if working { 1900 } else if typed { 2100 } else { 1500 };
    if working || typed {
        slint::Timer::single_shot(Duration::from_millis(1100), || {
            let _ = update_overlay("finishing", None);
        });
    }
    if typed {
        let words = words.clone();
        slint::Timer::single_shot(Duration::from_millis(1300), move || {
            with_pill(|p| p.typed = Some(words.split_whitespace().count()));
            pill_set_state("");
            pill_handle(PillEvent::Completed);
        });
    }
    slint::Timer::single_shot(Duration::from_millis(wait), move || {
        let overlay = UI.with(|ui| ui.borrow().overlay.as_ref().map(|o| o.clone_strong()));
        if let Some(overlay) = overlay {
            match overlay.window().take_snapshot() {
                Ok(buffer) => {
                    let mut out = format!("P6\n{} {}\n255\n", buffer.width(), buffer.height()).into_bytes();
                    for p in buffer.as_slice() {
                        // the snapshot is straight (not premultiplied) alpha
                        let a = p.a as f32 / 255.0;
                        let backdrop = [243.0, 239.0, 230.0]; // #F3EFE6
                        out.extend_from_slice(&[
                            (p.r as f32 * a + backdrop[0] * (1.0 - a)) as u8,
                            (p.g as f32 * a + backdrop[1] * (1.0 - a)) as u8,
                            (p.b as f32 * a + backdrop[2] * (1.0 - a)) as u8,
                        ]);
                    }
                    if let Err(e) = std::fs::write(&path, out) {
                        eprintln!("voicio: snapshot write failed: {e}");
                    }
                }
                Err(e) => eprintln!("voicio: snapshot failed: {e}"),
            }
        }
        let _ = slint::quit_event_loop();
    });
    slint::run_event_loop().map_err(|e| e.to_string())
}

#[cfg(test)]
mod window_tests {
    use super::*;

    #[test]
    fn dictations_are_sorted_into_notes_lists_and_emails() {
        assert_eq!(kind_of("Call the bank at four."), "Note");
        assert_eq!(kind_of("• Bread\n• Coffee\n• Batteries"), "List");
        assert_eq!(kind_of("1. Bread\n2. Coffee"), "List");
        assert_eq!(kind_of("Hi Dana,\nThe draft looks good.\nThanks"), "Email");
        assert_eq!(kind_of("Hi there"), "Note", "a greeting alone is not an email");
    }

    #[test]
    fn history_is_grouped_by_day_and_filtered() {
        let day = 86_400;
        let now = 100 * day + 12 * 3600;
        let entries = [
            crate::history::Entry { at: now - 60, text: "• a\n• b".into() },
            crate::history::Entry { at: now - 120, text: "today".into() },
            crate::history::Entry { at: now - day, text: "yesterday".into() },
        ];
        let list: Vec<(usize, &crate::history::Entry)> = entries.iter().enumerate().collect();
        let rows = history_rows(&list, 0, now, 0);
        assert_eq!(rows.iter().map(|r| r.first).collect::<Vec<_>>(), [true, false, true]);
        assert_eq!(rows[0].day, "TODAY");
        assert_eq!(rows[2].day, "YESTERDAY");
        assert_eq!(rows[1].meta, "11:58 · 1 word");
        assert_eq!(history_rows(&list, 1, now, 0).len(), 2, "today");
        assert_eq!(history_rows(&list, 2, now, 0).len(), 1, "lists");
        assert_eq!(day_label(31, 100), "1 FEB"); // day 31 after 1 Jan 1970
    }

    #[test]
    fn a_captured_key_becomes_a_shortcut_string() {
        use slint::platform::Key;
        let f9 = SharedString::from(Key::F9);
        assert_eq!(shortcut_from_key(&f9, false, false, false, false), KeyCapture::Shortcut("F9".into()));
        assert_eq!(shortcut_from_key(" ", false, false, false, true), KeyCapture::Shortcut("Super+Space".into()));
        assert_eq!(shortcut_from_key("d", true, false, true, false), KeyCapture::Shortcut("Ctrl+Shift+D".into()));
        assert_eq!(shortcut_from_key("d", false, false, false, false), KeyCapture::Ignore, "a bare letter would type everywhere");
        assert_eq!(shortcut_from_key(&SharedString::from(Key::Control), true, false, false, false), KeyCapture::Ignore);
        assert_eq!(shortcut_from_key(&SharedString::from(Key::Escape), false, false, false, false), KeyCapture::Cancel);
        assert_eq!(key_label("Super+Space"), "Super + Space");
    }

    #[test]
    fn packs_show_real_sizes_and_model_first_language_rows() {
        let dir = std::path::Path::new("/nonexistent");
        let packs = pack_rows(dir, 1);
        assert_eq!(packs.iter().map(|p| p.name.to_string()).collect::<Vec<_>>(), ["Lite", "Balanced", "Max"]);
        assert!(packs.iter().all(|p| !p.have && !p.get.is_empty()));
        assert!(packs[1].chip_sub.starts_with("Recommended"));
        let hindi = lang_row(dir, voicio_models::model("indic-hi").unwrap(), "", false, false);
        assert_eq!(hindi.name, "IndicConformer Hindi");
        assert_eq!(hindi.sub, "Hindi · by AI4Bharat");
    }
}
