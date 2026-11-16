# Bank-safe accessibility: what payment apps see and what Voicio should declare

Researched 2026-11-14. Context: Google Pay (India) warned that "something suspicious is enabled" and named Voicio.
The constraint is **no package deny list**: Voicio must look and behave as harmlessly as possible everywhere.
"AOSP main" below means `android.googlesource.com/platform/frameworks/{base,native}/+/refs/heads/main`, read on 2026-11-14.

## Summary of conclusions

1. **Banks see the enabled service and its full declared config, no permission needed.** Any app can call
   `AccessibilityManager.getEnabledAccessibilityServiceList()` and get every bound service's
   `AccessibilityServiceInfo` (id, flags, capabilities, `isAccessibilityTool`, event types). The AOSP code does
   no per-caller filtering (`AccessibilityManagerService.getEnabledAccessibilityServiceList`, AOSP main).
   **Voicio cannot hide that it is enabled.** It can only make what it declares look harmless.
2. **Fraud SDKs use crude rules that Voicio cannot fully pass.** The published heuristics are: any non-allowlisted
   enabled service, any non-system service (`FLAG_SYSTEM`), or any service whose installer is not a known store
   (Guardsquare, Appdome; see Q1). A sideloaded, non-system, non-allowlisted dictation app trips all three.
   **Installing from Play reduces this; nothing in the manifest removes it.**
3. **Use `TYPE_ACCESSIBILITY_OVERLAY` for the bubble, always, and drop `SYSTEM_ALERT_WINDOW`.** Both points are
   confirmed in AOSP (see Q4):
   - `setHideOverlayWindows(true)` / `HIDE_OVERLAY_WINDOWS` hides **only** `TYPE_APPLICATION_OVERLAY` (and the
     other legacy system-alert types) and `TYPE_TOAST`. It **does not hide `TYPE_ACCESSIBILITY_OVERLAY`**.
   - `TYPE_ACCESSIBILITY_OVERLAY` is a trusted overlay. It never sets `FLAG_WINDOW_IS_(PARTIALLY_)OBSCURED` on the
     bank's touches, so `filterTouchesWhenObscured` does not drop them. A `TYPE_APPLICATION_OVERLAY` bubble does
     both: it obscures the bank's touches and it disappears on screens that hide overlays.

   Today `FlowAccessibilityService.kt:146` prefers `TYPE_APPLICATION_OVERLAY` whenever overlay permission is granted.
   That is the more suspicious and less reliable choice.
4. **Declare the minimum.** The current `accessibility_service_config.xml` is already close (see Q5):
   - Keep: `canRetrieveWindowContent="true"` (unavoidable), and only the focus and window event types.
   - Keep false: `canPerformGestures`, `canRequestFilterKeyEvents`, `canTakeScreenshot`, fingerprint gestures,
     magnification, the accessibility button, and touch exploration.
   - Keep `isAccessibilityTool="false"`. Play reserves `true` for disability tools, and a general dictation app does
     not qualify.
5. **`isAccessibilityTool=false` plus `accessibilityDataSensitive` is the platform's own "stay out of payment
   screens" mechanism (Android 14+).** Views that set `accessibilityDataSensitive=yes`, or that set
   `filterTouchesWhenObscured`, and all their descendants, are invisible to non-tool services. Events from them are
   not delivered either. Voicio gets this for free and should not fight it. For example, it must not fall back to
   OCR or gestures when such a node is missing.
6. **Text insertion only.** No clicks, no gestures, no global actions, no reading of other windows' text beyond the
   focused editable node. This matches the Play policy's ban on autonomous actions and keeps the service out of the
   "view and control" pattern that fraud malware uses.
7. **A one-tap Pause that calls `disableSelf()` is the only real fix for "suspicious app enabled" warnings.** After
   `disableSelf()` the service is no longer bound, so it drops out of `getEnabledAccessibilityServiceList`. The user
   must re-enable it in Settings; an app cannot re-enable itself. Point users to this Pause, and to the IME
   alternative, in the warning help text.
8. **The keyboard (IME) mode avoids the accessibility warning entirely.** Fraud checks and Play Protect's
   install-time block key on the Accessibility permission (Q1, Q2). An IME has no accessibility binding, and banks'
   overlay hiding does not apply to IME windows: the IME is a trusted overlay and is not a system-alert window.
   *Unconfirmed:* we found no source on whether banking SDKs flag third-party IMEs; some offer "secure keyboard"
   features.
9. **What Voicio cannot control:**
   - App-specific rules in GPay, PhonePe, Paytm and banks. They are not public; see Q1.
   - Play Protect's install-time block for sideloaded apps that declare accessibility (India pilot).
   - Android 13+ restricted settings for sideloaded apps.
   - Android 16 blocking new accessibility grants during a call.
   - Advanced Protection Mode, which restricts accessibility to verified tools.
   - The OS "can view and control your screen" reminder notification for non-tool services.

---

## Q1. What do GPay / PhonePe / Paytm / Indian bank apps check?

**Unconfirmed: none of these apps publish their detection logic.** We found no Google Pay, PhonePe, Paytm or bank
help page that names the APIs they call. What is documented:

- **Google Pay (India) public guidance** is about screen-sharing and remote-control apps (AnyDesk, TeamViewer,
  "Screen Share"), as reported in Nov 2023 news coverage of a Google advisory. We could not reach the primary
  support.google.com page. Examples: https://www.latestly.com/technology/gpay-users-beware-google-cautions-people-over-use-of-these-apps-while-using-google-pay-heres-why-5578980.html
  and https://newsable.asianetnews.com/technology/google-warning-users-should-remove-these-three-apps-if-they-re-using-google-pay-vkp-s4kgg3
  The exact in-app trigger for "something suspicious is enabled" is **unconfirmed**. Accessibility services are
  the most likely trigger, given that the warning named Voicio.
- **Google in-call scam warning in India (pilot, Nov 2025).** With Google Pay, Paytm and Navi, a warning fires when
  a financial app is opened during a call with an unknown number while the screen is being shared. It offers to end
  the call and the screen share. This is secondary coverage:
  https://techcrunch.com/2025/11/20/google-steps-up-ai-scam-protection-in-india-but-gaps-remain
  It is triggered by screen sharing plus a call, not by accessibility. The UK version is described in Google's own
  post (Q2).
- **NPCI / RBI** have published only user advisories against remote-screen-access apps, starting in 2019:
  https://www.npci.org.in/PDF/npci/press-releases/2019/NPCI%20Press%20release%20-%20Digital%20Payments%20Safety%20(2).pdf
  We found **no RBI or NPCI rule requiring apps to detect accessibility services** (unconfirmed absence).
- **Anti-fraud SDK techniques (vendor docs; these are what bank apps typically embed):**
  - Guardsquare (DexGuard/ThreatCast) documents these checks:
    - Allowlist by service id via `getEnabledAccessibilityServiceList(FEEDBACK_ALL_MASK)`:
      https://www.guardsquare.com/mobile-app-security-research-center/malware/accessibility-services-allowlisting
    - Flag enabled services whose app lacks `ApplicationInfo.FLAG_SYSTEM`:
      https://www.guardsquare.com/mobile-app-security-research-center/malware/non-system-accessibility-services
    - Flag enabled services whose installer (`getInstallSourceInfo().getInstallingPackageName()`) is not in a store
      allowlist (Play, Galaxy Store, AppGallery):
      https://www.guardsquare.com/mobile-app-security-research-center/malware/side-loaded-accessibility
    - The Mar 2023 blog adds server-side validation of the service list and `accessibilityDataSensitive`:
      https://guardsquare.com/blog/protecting-against-android-accessibility-services-threats
  - Appdome flags installed apps that hold the accessibility permission. By default it closes the protected app
    with a message. Developers can trust services by package or service id, or trust OS defaults such as Voice
    Access:
    https://www.appdome.com/how-to/account-takeover-prevention/android-and-ios-trojans/appdome-trusted-accessibility-services/
- **Takeaway:**
  - The documented heuristics are presence, non-system, sideloaded, and not on an allowlist.
  - **Unconfirmed:** whether any vendor scores on individual capabilities (gestures, key filtering, screenshots).
    They are all visible in `AccessibilityServiceInfo.getCapabilities()` and `getFlags()`, so declaring fewer can
    only help.
  - A deny list on Voicio's side cannot change what the bank sees. Voicio stays enabled and listed even if it does
    nothing in that app.
  - The `packageNames` attribute only limits which apps' events Voicio receives. It does not hide Voicio from the
    list.

## Q2. Play Protect and platform anti-fraud, 2024-2026

- **Enhanced fraud protection: install-time block (Google, Feb 6 2024; Singapore first).**
  - Play Protect "will inspect the permissions the app declared in real-time and specifically look for four
    permission requests: RECEIVE_SMS, READ_SMS, BIND_Notifications, and Accessibility."
  - It blocks the install when the APK comes from "web browsers, messaging apps or file managers".
  - Source: https://security.googleblog.com/2024/02/piloting-new-ways-to-protect-Android-users-from%20financial-fraud.html
- **India pilot (Google India blog, Oct 3 2024):** the same four permissions; installs from internet-sideloading
  sources are blocked "with an explanation to the user".
  https://blog.google/intl/en-in/products/launching-enhanced-fraud-protection-pilot-in-india/
  - Implication: a sideloaded Voicio APK may be **blocked at install** in India. Distribution through Play avoids
    this.
- **Restricted settings (Android 13+).** Sideloaded apps cannot have accessibility enabled until the user taps
  App info, then More, then "Allow restricted settings":
  https://support.google.com/android/answer/12623953
  - Android 15 reportedly extends this to more settings and roles (overlay, usage access, device admin). That is
    secondary coverage, **unconfirmed** against a primary source:
    https://9to5google.com/2024/09/12/android-15-sideloaded-apps-restrictions/
  - This is a further reason to drop `SYSTEM_ALERT_WINDOW`.
- **In-call protections (Google, May 13 2025).** During a call, Android 16 blocks "granting accessibility
  permissions" and first-time sideloading; Android 6+ blocks disabling Play Protect.
  - There is a UK pilot with Monzo, NatWest and Revolut that warns when a banking app is opened while sharing the
    screen with an unknown caller.
  - Source: https://blog.google/security/whats-new-in-android-security-privacy-2025/
  - Services already enabled before the call keep working.
- **Advanced Protection Mode:** "Accessibility tools: Restricts accessibility services to verified accessibility
  tools." https://support.google.com/android/answer/16339980
  - News reports date the accessibility restriction to Android 17.2 (Mar 2026):
    https://www.helpnetsecurity.com/2026/03/19/google-android-accessibility-api-restrictions/
  - With APM on, Voicio's accessibility mode will not work. The IME mode is the fallback.
- **OS reminder for non-tool services.** AOSP `PolicyWarningUIController` posts "Check access settings / *X* can view
  and control your screen. Tap to review." for bound services with `isAccessibilityTool=false`
  (`SEND_NOTIFICATION_DELAY_HOURS = 24`; `AccessibilitySecurityPolicy.onBoundServicesChangedLocked`, AOSP main).
  This is expected and cannot be avoided without the tool flag.

## Q3. `isAccessibilityTool` and `accessibilityDataSensitive` (Android 14 / API 34)

- **`android:isAccessibilityTool`** (API 31): "whether the accessibility service is used to assist users with
  disabilities. This criteria might be defined by the installer. The default is false."
  https://developer.android.com/reference/android/accessibilityservice/AccessibilityServiceInfo
- **Play policy:**
  - Only services "designed to help people with disabilities" may set the flag to true. A general voice assistant
    does not qualify.
  - Non-tools need an in-app prominent disclosure with affirmative consent, plus the Permission Declaration Form.
  - Any use that lets an app "autonomously initiate, plan, and execute actions" is prohibited.
  - Source: https://support.google.com/googleplay/android-developer/answer/10964491
- **`View.setAccessibilityDataSensitive(int)`** (API 34): restricts the view to services with
  `isAccessibilityTool=true`.
  https://developer.android.com/reference/android/view/View#setAccessibilityDataSensitive(int)
  - `AUTO` (the default) becomes YES if the view sets `filterTouchesWhenObscured` or any parent is data-sensitive.
    That is in the docs and in `View.calculateAccessibilityDataSensitive()`, AOSP main.
- **Enforcement:**
  - `AccessibilityInteractionController.isVisibleToAccessibilityService()` returns
    `isRequestFromAccessibilityTool() || !view.isAccessibilityDataSensitive()`. Sensitive nodes are simply absent
    from a non-tool service's node tree, so Voicio cannot find, read, `ACTION_SET_TEXT` or `ACTION_PASTE` them.
  - `AccessibilityEvent.setAccessibilityDataSensitive`: events from such sources are "delivered only to
    isAccessibilityTool services" (AOSP main).
- **What a non-tool service can still see:** every non-sensitive view, on every API level, and everything on
  Android 13 and below. Password fields are **not** automatically data-sensitive by this API; their text is masked
  by the long-standing password handling. **Unconfirmed** for non-framework (Compose/WebView) fields.
- For Voicio this is desirable. Payment-PIN screens that opt in become invisible to it, which proves at the platform
  level that it cannot read them.

## Q4. Overlay type: `TYPE_ACCESSIBILITY_OVERLAY` vs `TYPE_APPLICATION_OVERLAY`

- **`Window.setHideOverlayWindows(boolean)`** (API 31): "Prevent non-system overlay windows from being drawn on top
  of this window. Requires `HIDE_OVERLAY_WINDOWS`."
  https://developer.android.com/reference/android/view/Window#setHideOverlayWindows(boolean)
  - Guardsquare recommends that banks call it on every sensitive activity:
    https://www.guardsquare.com/mobile-app-security-research-center/malware/hide-non-system-overlays
- **What it actually hides (AOSP main, `WindowState.setForceHideNonSystemOverlayWindowIfNeeded`):**
  ```java
  if (mSession.mCanAddInternalSystemWindow
          || (!isSystemAlertWindowType(baseType) && baseType != TYPE_TOAST)) return;
  ```
  `WindowManager.LayoutParams.isSystemAlertWindowType()` is true only for `TYPE_PHONE`, `TYPE_PRIORITY_PHONE`,
  `TYPE_SYSTEM_ALERT`, `TYPE_SYSTEM_ERROR`, `TYPE_SYSTEM_OVERLAY` and `TYPE_APPLICATION_OVERLAY`.
  **`TYPE_ACCESSIBILITY_OVERLAY` is not hidden.** A `TYPE_APPLICATION_OVERLAY` bubble vanishes on every
  bank/UPI screen that uses this API.
- **Touch obscuring (`filterTouchesWhenObscured`, `FLAG_WINDOW_IS_OBSCURED` / `..._PARTIALLY_OBSCURED`):**
  - InputDispatcher `canBeObscuredBy()` (frameworks/native, AOSP main) returns false when the other window has
    `InputConfig::TRUSTED_OVERLAY`.
  - `InputMonitor.isTrustedOverlay(type)` includes `TYPE_ACCESSIBILITY_OVERLAY` (and the IME types).
  - So an accessibility-overlay bubble never makes the bank's touches "obscured". A `TYPE_APPLICATION_OVERLAY`
    bubble (a different uid, not trusted) does. Banks that log or reject partially-obscured touches will see Voicio
    as an overlay attack.
- **Android 12 untrusted-touch blocking:** "Accessibility windows" and IME windows are trusted. "Windows of type
  TYPE_APPLICATION_OVERLAY aren't trusted."
  https://developer.android.com/about/versions/12/behavior-changes-all
- **Permissions:**
  - `TYPE_APPLICATION_OVERLAY` "Requires SYSTEM_ALERT_WINDOW" (WindowManager.java javadoc).
  - `TYPE_ACCESSIBILITY_OVERLAY` is "overlaid only by a connected AccessibilityService" and needs no extra
    permission. It must be added through the service's own `WindowManager` (service context).
- **FLAG_SECURE** on the bank's window keeps it out of screenshots and screen capture. It does not affect either
  overlay type. **Unconfirmed:** whether FLAG_SECURE changes the node tree a service receives (AOSP reading
  suggests it does not; the data-sensitive mechanism is the content control).
- **Recommendation:**
  - Always use `TYPE_ACCESSIBILITY_OVERLAY`.
  - Remove `<uses-permission SYSTEM_ALERT_WINDOW>` and the `canDrawOverlays` branch. That is one less sensitive
    permission for Play Protect and restricted settings to weigh.
  - Keep the bubble small and touchable only on itself (`FLAG_NOT_FOCUSABLE`, `WRAP_CONTENT`). Never use a
    full-screen or pass-through overlay.

## Q5. Declarations that reduce suspicion, and what is unavoidable

Each capability is visible to banks through `getCapabilities()`. Reference:
https://developer.android.com/reference/android/accessibilityservice/AccessibilityServiceInfo

| Attribute / flag | Exposes | Voicio | Why |
|---|---|---|---|
| `canRetrieveWindowContent` (`CAPABILITY_CAN_RETRIEVE_WINDOW_CONTENT`, API 18) | "can retrieve the active window content" | **true (unavoidable)** | Needed to find the focused editable node and set or paste text |
| `canPerformGestures` (API 24) | "can perform gestures" (tap/swipe injection) | false | Core malware primitive (auto-approve payments) |
| `canRequestFilterKeyEvents` / `flagRequestFilterKeyEvents` (API 18) | receives key events "before applications" | false | Keylogging primitive |
| `canTakeScreenshot` (API 30) | "can take screenshot" | false | Screen-scraping primitive |
| `flagRequestFingerprintGestures` (API 26) | fingerprint-sensor gestures | false | Not needed |
| `canControlMagnification`, `canRequestTouchExplorationMode`, `flagRequestAccessibilityButton`, multi-finger / 2-finger passthrough | various | false / omit | Not needed. The a11y button also makes the service more prominent |
| `flagRetrieveInteractiveWindows` (API 21) | access to "content of all interactive windows" | true (kept) | Used only to find the IME window bounds. **Consider dropping** if keyboard position can be inferred otherwise (e.g. the focused node's bounds): it widens what the service *could* read |
| `accessibilityEventTypes` | which events arrive | focus + window changes only | Fewer events means less data. Avoid `typeViewTextChanged` and `typeAllMask` |
| `isAccessibilityTool` | tool status | false | Play policy, and keeps data-sensitive views invisible |
| `packageNames` | event source filter | omit | An allow list would break general dictation, and it does not hide the service from banks |

**Behaviour rules (not declarable, but they are what policy and fraud analysts judge):**
- Only `ACTION_SET_TEXT` / `ACTION_PASTE` on the focused editable node that the user chose.
- No `performGlobalAction`, no `ACTION_CLICK`, and no traversal or storage of other nodes' text.
- Never act on a node that is password-typed, or that appears only after a data-sensitive subtree is hidden.
- One-tap Pause calls `disableSelf()`. Explain in the in-app disclosure that payment apps may warn while Voicio is
  enabled, and that Pause or keyboard mode avoids the warning.

**Unavoidable:**
- Being listed as an enabled, non-system service, and its installer (sideloaded vs Play).
- `CAPABILITY_CAN_RETRIEVE_WINDOW_CONTENT`.
- The OS 24 h "view and control" reminder.
- Play Protect's sideload install block in pilot countries.
- Restricted settings.
- Android 16's block on new accessibility grants during calls.
- APM blocking non-tool services.
- Any bank SDK that warns on *any* non-allowlisted service.
