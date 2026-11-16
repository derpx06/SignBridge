# Releasing Voicio

Versions: desktop, GNOME extension and Android 0.9.0 (beta); Chrome stays on its own 1.x line (store versions cannot go backwards). Licence: GPL-3.0-or-later (`LICENSE`). Privacy policy: `PRIVACY.md` (link it in every store listing).

## Support status (be honest in release notes)
| Surface | Status |
|---|---|
| Linux desktop (GNOME Wayland/X11 tested) | Supported. KDE/Sway: not tested here. |
| Android | Supported after the on-device checklist below has been run. |
| Chrome extension | Supported (Chromium 116+; Firefox/Safari unsupported). |
| Windows, macOS | Experimental: CI only proves they compile. No installer, no signing. |

## Build
- Desktop: `cd engine && bash scripts/package-release.sh` -> `dist/voicio-<v>-linux-x86_64.tar.gz` + `.sha256`.
- Android: `JAVA_HOME=/usr/lib/jvm/java-17-openjdk flock /tmp/claude-1000/build.lock ./gradlew --no-daemon --max-workers=2 :app:bundleRelease` -> `app/build/outputs/bundle/release/app-release.aab` (Play) and `:app:assembleRelease` for a sideload APK. Signing: set `VOICIO_KEYSTORE`, `VOICIO_KEYSTORE_PASSWORD`, `VOICIO_KEY_ALIAS`, `VOICIO_KEY_PASSWORD` (env or `~/.gradle/gradle.properties`); never commit the keystore. Without them the build is unsigned.
- Chrome: `cd extension && pnpm install && pnpm --filter @voicio/chrome zip`.

## Checks before tagging
1. `cd engine && cargo test --release --workspace`
2. `engine/scripts/extension-harness/run.sh` (GNOME extension in an invisible shell; look at the screenshots)
3. Package -> install into a throwaway HOME -> `voicio doctor` -> `install.sh --uninstall`.
4. `./gradlew :core:test`; `cd extension && pnpm test`.
5. Android on-device checklist: fresh install -> onboarding -> bubble in Notes, WhatsApp, Chrome -> voice note -> uninstall -> reinstall.
6. GNOME: log out/in once after installing, then Super+Alt+Space in a text field.

## Store paperwork
- **Play Data Safety** (answers match `PRIVACY.md`): Audio is collected only if a cloud provider is turned on -> "Voice or sound recordings: shared with the chosen provider, optional, not stored by us, used for app functionality, encrypted in transit". Otherwise no data collected or shared. No analytics, no ads, no account. Users can delete data in-app (Reset app).
- **Play accessibility declaration**: the service is used only to show the dictation bubble on focused text fields and insert the user's dictated text; no screen content is collected or sent. Record the prominent-disclosure screen from onboarding for the review video.
- **Chrome Web Store**: permission and host justifications are in `extension/store/LISTING.md`.
- **extensions.gnome.org**: GPL-compatible licence (done); the extension needs the `voicio` app installed, say so in the description.

## Accessibility, banking apps and the Voicio Keyboard (Android)
Background and sources: `docs/research/bank-safe-accessibility.md`.
- **Why banking apps warn:** any app can list enabled accessibility services and their declared capabilities; fraud SDKs flag services that are not on their allowlist, not system apps, or not installed from a store. Voicio cannot hide, it can only declare and do less. Sideloaded debug builds look worse than Play-installed release builds.
- **What the service declares:** only `canRetrieveWindowContent` (needed to find the focused field), focus and window-change events (no click/selection events, which carry text), `flagRetrieveInteractiveWindows` (only to find the keyboard's top edge), `isAccessibilityTool="false"`. No gestures, key filtering, screenshots, fingerprint gestures, magnification or accessibility button. It never taps or swipes: text goes in only by paste / set-text on the focused editable field. It never shows or types in password fields or views marked `accessibilityDataSensitive` (Android 14+). The bubble is a `TYPE_ACCESSIBILITY_OVERLAY` (bank screens' `HIDE_OVERLAY_WINDOWS` does not hide it and it never marks their touches as obscured), so `SYSTEM_ALERT_WINDOW` is no longer requested.
- **Pause:** Quick Settings tile "Voicio" calls `disableSelf()`, so the service drops out of the enabled list that banks read. Turning it back on opens the service's Settings page.
- **Voicio Keyboard** (`ime/VoicioIme`) needs no accessibility permission at all (only the microphone) and is the recommended mode for people who use banking apps. In password, number and phone fields it shows plain keys only: no microphone, suggestions or clipboard.
- **Play accessibility declaration text:** "Voicio uses the Accessibility API only to show its dictation bubble next to the text field the user is typing in, and to insert the text the user dictated into that field. It reads only the focused field, never password fields, does not perform gestures, filter keys or take screenshots, and collects, stores or sends no screen content. Voicio is not an accessibility tool (isAccessibilityTool=false). A keyboard mode that needs no accessibility permission is offered as an alternative." Record the onboarding disclosure screen and the Pause tile for the review video.
- **On-device check before release:** with the bubble on, open Google Pay and note any warning; pause with the tile and confirm it is gone; use the Voicio Keyboard only and confirm no warning.
