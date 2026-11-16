Voicio: private voice typing that runs on your computer.

Install:   ./install.sh            (downloads the speech models for your computer, about 0.6 GB)
Offline:   use the package built with --with-models; no download is needed.
Remove:    ./install.sh --uninstall

Then press your shortcut in any text field and speak (F9 by default; the installer prints the key and
how it works, and  voicio status  shows it). Your audio leaves this computer only if you turn on an
optional cloud provider in Settings; otherwise everything runs on this computer. Privacy: PRIVACY.md.
Settings (shortcut, writing style, models) are in the app window: run  voicio settings
Spoken style commands at the start of a dictation: "email mode", "make it formal", "as a list".

What is inside: the agent (voicio), the model worker with voice detection, speech recognition and text
cleanup (voicio-engine), the GNOME extension, the launcher entry.
Licence: GPL-3.0-or-later (LICENSE). Third-party licences: THIRD_PARTY_NOTICES.md.
