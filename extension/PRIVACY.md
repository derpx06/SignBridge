# Voicio for Chrome: privacy policy

The full policy for all Voicio apps is at https://github.com/derpx06/voicio/blob/main/PRIVACY.md. This page covers the Chrome extension.

- Speech is recognised on your computer, inside Chrome, by default. Audio is recorded only while you are dictating, kept in memory and never stored.
- Audio goes to a cloud provider only if you add your own API key and turn that provider on (see below).
- Recognised text is inserted into the page you are on. Voicio does not read, store or send page content.
- With cloud off, the only network request is the one-time download of the speech model from huggingface.co (checked against a fixed SHA-256). No analytics, no accounts, no telemetry.
- Settings (shortcut mode, site rules, word list, capsule position) are stored with Chrome sync storage. The last transcript is kept in session storage and is cleared when the browser closes.
- Password and credit-card fields are never dictated into.

Permissions: `storage` (settings), `contextMenus` ("Dictate here"), `offscreen` (microphone and model run in a hidden page), `activeTab` and `scripting` (show the capsule and insert text in the tab you act on), `clipboardWrite` (Copy button). Access to specific sites is requested per site, only when you choose "Allow" for a site or "Works on every website".

## Optional cloud speech (off by default)

If you turn on "Cloud (optional)" in Settings, each dictation's audio is sent to the provider you chose (OpenAI `api.openai.com`, Groq `api.groq.com`, Mistral `api.mistral.ai`, ElevenLabs `api.elevenlabs.io`, Deepgram `api.deepgram.com` or Sarvam `api.sarvam.ai`), using your own API key, under that provider's terms. You agree per provider, and Chrome asks for access to that one site. Saving a key sends one second of silence to the provider to check that it works. The key is kept in local extension storage only (not synced) and is never logged. If the provider fails, the on-device result is used. With cloud off, no audio leaves your computer.
