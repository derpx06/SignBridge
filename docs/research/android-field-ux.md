# Android dictation field and app UI: research, audit, redesign spec

2026-11-13. Read-only audit of the current `app/` tree. V = verified in a primary source; I = inferred or secondary only.

## 1. How others do it

- **Wispr Flow Android** ([size], [hands-free], [nav] docs)
  - Floating bubble, keyboard stays up. Default spot is the right edge (V).
  - Shown only while a field is focused and the keyboard is up (V).
  - Size slider 0.7–1.15x, default 0.85x. Opt-in "shrink when idle" (V).
  - **Tap = hands-free, hold = push-to-talk, release submits** (V).
  - **Done (check) inserts. Cancel discards, and is the only control while processing.** Tapping the bubble body does nothing mid-session (V).
  - Copy fallback for 5 s if the insert can't be confirmed. Vibrates before the 5 min cap (V).
- **Gboard classic:** voice panel replaces the keyboard, "Speak now", tap mic to pause (V).
- **Gboard voice toolbar:** a pill bar takes the keyboard's place to show more screen. Draggable, or a vertical side pill (V help + 9to5Google).
- **Gboard Rambler:** keyboard stays, a glowing animation shows listening. **No live text**; cleaned text lands at the cursor. Done button, or hold and release (V).
- **SwiftKey AI voice (beta):** mic, then a check ends it, **no live preview** (secondary).
- **Otter / Superwhisper / Aqua / VoiceInk:** no Android field-dictation pattern (I).

**Takeaways**
- Every product has a 48dp+ target and an explicit **Done**. Android guidance is 48dp targets with 8dp gaps (V); the M3 FAB is 56dp (I).
- Nobody shows live words in a small chip. Either none, or a keyboard-width bar.
- Haptics: use `View.performHapticFeedback` (no VIBRATE permission, follows the system setting), with CONFIRM for success and REJECT for failure (V). Those are API 30; minSdk is 29, so fall back to `VIRTUAL_KEY`.

## 2. Audit

### Floating field
1. **The default trigger is Hold.** `FlowSettings.kt:14` defaults to `"hold"`, and a tap gives `Discard` plus "Hold the button while you speak" (`DictationController.kt:117-120`). Wispr's tap gesture fails here. This is probably most of the "not usable".
2. **The capsule is 32x48dp** (`BubbleUi.kt:51-52`), drawn at 0.88 scale when idle (`:104`), so about 28x42dp. The press area is only the left 48dp square (`OverlayHost.kt:122`).
3. **Live text is 13sp, one line, clipped at 248dp** (`BubbleUi.kt:61,138`), so about 25 characters show.
4. **No cancel and no confirm.** The only way out is a drag (`FlowAccessibilityService.kt:87`). Undo is bare 13sp text with no 48dp target (`BubbleUi.kt:140`).
5. **No haptics** anywhere in `app/`.
6. **Private colour copies** (`BubbleUi.kt:43-48`); Graphite is duplicated again at `NoteEditor.kt:70`.
7. **Field avoidance runs only at idle** (`FlowAccessibilityService.kt:137`). Default Y is 60% of the screen (`:129`), mid-content, not docked to the keyboard.
8. Settings says "Live text above the bubble" (`Screens.kt:471`), but the text is inside it.

### App screens
9. **The type scale is bypassed 58 times** with `.copy(fontSize=…)`: 11, 12, 13, 14, 15 and 17 sp all appear.
10. **35 hardcoded colours** outside Theme/Kit, e.g. the amber notice set (`Screens.kt:231-236`, `Onboarding.kt:637-643`) and `0xFF4ADE80` (`Screens.kt:220`, `Onboarding.kt:617`).
11. **Six dark headers of different heights:** 320 / 176 / 150 / 160 / 124 / 300dp (`Screens.kt:213,299,360,403,466,558`). Three re-implement `Hero`, and every tab carries a full ripple.
12. **The Home hero is half the screen.** The headline says "Hold the bubble" whatever the mode (`Screens.kt:225`).
13. **Home shows one setup problem at a time** (`Screens.kt:229`).
14. **The custom NavBar** has a 3dp line indicator, 11sp labels, and a 4-sided border (`Screens.kt:124-139`). M3 `NavigationBar` is already available.
15. **Walls of text in Settings:** SmartEditCard (`SmartEditCard.kt:42-75`) and CloudCard (`CloudCard.kt:50-63`) sit in the middle of Settings.
16. **Dead rows that look tappable:** "Speech model" has an empty trailing slot (`Screens.kt:481`), and "Free memory 5 min" can't be changed (`:482`).
17. **Destructive actions use "tap again in 3 s"** with three separate timers (`Screens.kt:454-509`), while CloudCard already uses `AlertDialog`.
18. **No dividers between group rows** (`Screens.kt:523-547`).
19. **Notes:** tints cycle by index with an O(n²) `indexOf` (`Screens.kt:357,368`), and the grid is built from `chunked(2)`. Two copy-pasted FABs use `bottom = 88.dp` (`:385,425`).
20. **NoteEditor:** Back and the Save pill both save, which makes five header controls (`NoteEditor.kt:147-151`). Success messages show in amber (`:166`).
21. **Onboarding has 9 steps.** The 3 intro slides use three palettes, violet, teal and brown (`Onboarding.kt:123-125`), breaking the one-accent rule (`Theme.kt:5`).
22. **PhoneArt** shows a lattice mid-phone, not the real control (`Onboarding.kt:213`).
23. **On the Mic test step,** the mic disc is Deep on Deep, so invisible (`:439`), and the disabled button is a grey slab on dark (`:432`).
24. **The Models step** has one choice plus an empty "packs" card (`:485-489`).
25. **Ready never lets you try the bubble** (`:605-633`).

## 3. Redesign spec

### Overlay tokens (add to `C`, delete the private copies)

| Token | Value |
|---|---|
| `OverlayBg` | `C.Deep` |
| `OverlayRim` | `0x33C4B5FD` |
| `OverlayInk` | White |
| `OverlayMuted` | `C.Lavender` |
| `OverlayGo` | `C.Accent` |
| `OverlayWarn` | `0xFFF5B83D` |
| `OverlayError` | `0xFFF87171` |

The lattice keeps its teal and mint.

### (a) Floating field

**Behaviour:** default `shortcutMode = HoldOrTap`. Never answer a tap with an error.

**Idle: Orb**
- 56dp circle, `OverlayBg`, 1dp rim, 12dp shadow at 30%. 40dp lattice inside. Window 64dp.
- Docked to the right edge with an 8dp margin. Y = 16dp above the keyboard top, or 96dp above the nav bar.
- Visible only when a field is focused and the keyboard is up, unless `alwaysShowBubble` is on.
- Opacity 0.92. No idle shrink.

**Listening: Bar**
- The window becomes `MATCH_PARENT` wide and stays `FLAG_NOT_FOCUSABLE`.
- Size: screen − 2×12dp (max 560dp, centred), 64dp tall (84dp for 2 lines), radius 32dp.
- The bottom sits 8dp above the keyboard. If the focused field intersects the bar (a chat input glued to the keyboard), move the bar above the field top − 8dp. Extend `target()` for this.
- Row, 8dp padding and 8dp gaps:
  1. **Cancel:** 48dp circle, white 10%, 20dp X in `OverlayMuted`.
  2. **Lattice:** 40dp.
  3. **Text:** weight 1. Live tail, max 2 lines, `T.Body` (15/22) white, 12dp fade on the top line. "Listening…" in `OverlayMuted` before the first word. The Cloud chip sits at top-right.
  4. **Done:** 48dp circle, `OverlayGo`, 22dp white check.
- Tapping the bar body does nothing. Drag still moves it.

**States**
- **Warming:** "Starting…".
- **Working:** Done collapses to 0 width, amber lattice sweep, text at 60%. Cancel stays.
- **Done:** lattice check for 450ms, then collapse to the orb, plus an **Undo** pill (40dp high, 48dp target) beside it for 4s.
- **Error / warn:** 2dp border in `OverlayError`/`OverlayWarn`, message in that colour. The Done slot becomes "Retry" or "Copy" (48dp). Auto-dismiss after 4s.

**Haptics**
- Start: `GESTURE_START` (API 30+, else `VIRTUAL_KEY`).
- Inserted: `CONFIRM`.
- Error or discard: `REJECT`.
- 10s before `MAX_HOLD_MS`: `CLOCK_TICK`.

**Motion:** the orb grows from its docked corner using `Motion.snappy()` (about 280ms, no bounce). Content fades in after 60ms. Collapse is the reverse.

**Accessibility:** live region on the text only (it is on the whole capsule at `BubbleUi.kt:127` today). Label each button separately.

Skipped: a size slider and shrink-to-dot. Add them on request.

### (b) App screens

**System**
- **Spacing** 4 / 8 / 12 / 16 / 24 / 32. Gutter 16, section gap 24, gap inside a card 12.
- **Cards:** one style, radius 16, 1dp `C.Border`, white, padding 16. Tinted fills only for notices.
- **Rows:** 56dp min (72dp with a subtitle), 1dp dividers inset 16.
- **Type:** add `T.Body2` (14/20) and `T.Caption` (12/16), and remove all 58 `.copy(fontSize)` calls.
- **Colour:** add `C.AmberInk`, `C.AmberBorder`, `C.OnDeepMuted`, `C.Success`. Leave no `Color(0x` outside Theme/Kit.
- **Notices:** one `Notice(kind, title, body, action)` replaces four hand-built amber cards.

**Chrome**
- Only Home keeps the dark ripple hero, cut to 220dp.
- The other tabs get a light 64dp top bar with `T.H1` in Ink.
- Use M3 `NavigationBar` (80dp, `AccentSoft` pill indicator) and `ExtendedFloatingActionButton` in a `Scaffold`, which also gets rid of the 88dp offsets.

**Per screen**
- **Home:**
  - The headline follows the mode ("Tap the bubble in any app").
  - A setup checklist shows every item, each with a tick or a 48dp "Fix".
  - The practice field goes right below it.
  - The model card becomes tappable.
- **History:** tap a card to copy, with a snackbar. Swipe to delete, with Undo. Two fewer icons per row.
- **Notes:** `LazyVerticalStaggeredGrid(2)` with white cards.
- **NoteEditor:**
  - Back saves, so drop the Save pill.
  - Copy and Delete move into an overflow menu.
  - Messages use `C.Muted`, and `C.Coral` only for errors.
- **Words:** key rows by content. The info card becomes the subtitle.
- **Settings:**
  - Smart Edit and Cloud become one row each (e.g. "Cloud · Off ›") that opens a sub-screen.
  - Remove the dead rows, or make them work.
  - Destructive actions use `AlertDialog`.
- **Onboarding:**
  - One violet palette.
  - Two intro slides, showing the real orb → bar over a keyboard.
  - Fold Models into Download.
  - Mic disc uses `C.Tile`, and the footer button is hidden until there is a result.
  - One shared `StepNumber` (32dp circle).
  - Ready gets a live practice field.

## Sources
[size]: https://docs.wisprflow.ai/articles/2807859589-customize-flow-bubble-size-and-shrink-behavior-on-android
[hands-free]: https://docs.wisprflow.ai/articles/6391241694-use-flow-hands-free
[nav]: https://docs.wisprflow.ai/articles/5096240724-navigating-the-wispr-flow-app-desktop-ios-and-android

- [Wispr bubble size](https://docs.wisprflow.ai/articles/2807859589-customize-flow-bubble-size-and-shrink-behavior-on-android), [Wispr hands-free](https://docs.wisprflow.ai/articles/6391241694-use-flow-hands-free), [Wispr navigating](https://docs.wisprflow.ai/articles/5096240724-navigating-the-wispr-flow-app-desktop-ios-and-android)
- [Gboard voice typing](https://support.google.com/gboard/answer/2781851), [Gboard Rambler](https://support.google.com/gboard/answer/17468539?hl=en), [9to5Google voice toolbar](https://9to5google.com/2025/02/10/pixel-gboard-voice-typing-toolbar-phones/)
- [9to5Google SwiftKey AI voice](https://9to5google.com/2026/09/15/swiftkey-ai-voice/), [SwiftKey voice typing](https://support.microsoft.com/en-us/topic/how-do-i-use-voice-to-text-with-microsoft-swiftkey-keyboard-b56f06e5-dda3-4edd-9b2f-0e6404806b2f), [SamMobile Samsung leak](https://sammobile.com/news/samsung-keyboard-gboard-rambler-feature-one-ui-9-5)
- [Android haptic feedback](https://developer.android.com/develop/ui/views/haptics/haptic-feedback), [haptics principles](https://developer.android.com/develop/ui/views/haptics/haptics-principles), [touch target size](https://support.google.com/accessibility/android/answer/7101858)
