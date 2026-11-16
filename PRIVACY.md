# Voicio privacy policy

Last updated: 13 November 2026. Applies to Voicio for Android, Voicio for Linux (desktop and GNOME extension) and Voicio for Chrome.

## The short version

Voicio turns your speech into text on your own device. By default, nothing you say leaves your device. There are no accounts, no analytics, no ads and no telemetry. The only exception is optional: if you add your own API key for a cloud speech provider and turn it on, your audio for each dictation is sent to that provider.

## What is processed and where

- **Audio.** The microphone is used only while you are dictating. Audio is processed in memory on your device and is not saved to disk.
- **Text.** Recognised text is typed into the app or field you chose. Voicio does not read, store or send the contents of other apps or web pages, apart from the focused text field it is typing into. It never types into password fields.
- **History, notes and your word list** stay on your device:
  - Android: in app-private storage, encrypted (AES-256-GCM, key held in Android Keystore). They are excluded from cloud backup and device transfer.
  - Linux: in your home folder (`~/.local/share/voicio/history.jsonl`, kept for the number of days set in Settings, 30 by default; settings in `~/.config/voicio/`).
  - Chrome: settings in Chrome sync storage; the last transcript in session storage, cleared when the browser closes.
- **Speech models** are downloaded once from Hugging Face, GitHub or the sherpa-onnx project site, and checked against a fixed SHA-256. These requests contain no personal data beyond what any download reveals (your IP address, to those hosts).

## Optional cloud speech providers (off by default)

You can choose to send audio to one of these providers, using your own API key: Groq, OpenAI, Google (Gemini API), Deepgram, ElevenLabs, Mistral, AssemblyAI, Microsoft Azure (Speech, in the region you enter), Speechmatics, Sarvam AI, Together AI, Lemonfox (standard or EU endpoint), Cartesia, Soniox or OpenRouter (which forwards the audio to the model you pick). The full list with each provider's address is in `providers/cloud-stt.json`. You also choose when it is used: for every dictation, or only when no on-device speech pack is installed. When it is on:

- the audio of each dictation is sent to the provider you picked, and is handled under that provider's own privacy policy and terms;
- AssemblyAI and Soniox work by uploading the recording and then fetching the result; Voicio asks Soniox to delete the upload and the transcript straight after (AssemblyAI keeps them under its own retention policy);
- you agree per provider before it is used; on Chrome, the browser also asks for access to that one site;
- your key is stored only on your device (Android: encrypted storage; Linux: a file readable only by you in `~/.config/voicio/`; Chrome: local extension storage, not synced) and is never logged or sent anywhere except to that provider;
- if the provider fails, the on-device result is used (when no on-device pack is installed, the dictation fails with a message instead).

Turn it off or remove the key at any time in Settings.

## Permissions and why

- **Microphone**: to hear you while you dictate.
- **Android accessibility service**: to show the dictation bubble when a text field is focused and to type the text into it. It does not collect, store or transmit screen content.
- **Android display over other apps / Linux GNOME extension / Chrome site access**: to show the dictation capsule and insert text where you are typing.
- **Internet**: to download speech models, and to reach a cloud provider only if you turned one on.

## Children

Voicio is a general-purpose tool and does not knowingly collect data from anyone, including children.

## Deleting your data

Android: Settings > Reset app, or uninstall. Linux: `voicio` Settings > clear history, or run `install.sh --uninstall` and delete `~/.config/voicio`, `~/.local/share/voicio` and `~/.cache/voicio`. Chrome: remove the extension.

## Contact

Questions or concerns: open an issue at https://github.com/derpx06/voicio/issues.
