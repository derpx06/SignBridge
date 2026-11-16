# Chrome Web Store listing

**Name:** Voicio for Chrome
**Summary (132 max):** Private voice dictation. Speak and it types into any text field. Recognises speech on your computer, works offline.

**Description**
Voicio turns your voice into text in any web page. Speech is recognised on your own computer by default.

- Press Alt+Shift+V in a text field, speak, and the words appear as you talk
- Speech recognition runs inside Chrome on your own computer: no account, no analytics, no tracking
- Optional: use your own cloud speech provider (OpenAI, Groq, Mistral, ElevenLabs, Deepgram or Sarvam). Audio goes to that provider only if you add your own API key and turn it on
- Works with Wi-Fi off after a one-time model download (about 67 MB, starts by itself after install)
- Voice commands: "new line", "new paragraph", "scratch that", "press enter"
- Your own word list fixes names and jargon ("kube control" becomes "kubectl")
- Never touches password or card fields
- Choose where it works: allow or block individual sites

**Single purpose:** dictate speech into text fields on web pages.

**Permission justifications**
- `offscreen`: record the microphone and run the speech model in a hidden page while dictating.
- `activeTab`, `scripting`: show the recording pill and insert text in the tab where the user pressed the shortcut or used the menu.
- `storage`: save settings, site rules and the word list.
- `contextMenus`: the right-click "Dictate here" item.
- `clipboardWrite`: the Copy button in the popup.
- optional host access (`<all_urls>` in `optional_host_permissions`): requested per site only when the user chooses "Allow" for a site or "Works on every website"; also requested for a single cloud provider origin (below) when the user turns that provider on.

**Remote hosts (CSP `connect-src` and optional host permissions)**
- `huggingface.co`, `*.huggingface.co`, `*.hf.co`: one-time download of the speech model files (Hugging Face serves them through its CDN); each file is checked against a fixed SHA-256. No user data is sent.
- `api.openai.com`, `api.groq.com`, `api.mistral.ai`, `api.elevenlabs.io`, `api.deepgram.com`, `api.sarvam.ai`: optional bring-your-own-key cloud speech-to-text. Off by default. Contacted only after the user picks that provider, accepts a notice that their audio will be sent to it, grants Chrome's permission for that one origin, and saves their own API key. Then each dictation's audio (16 kHz WAV) is sent to that provider's transcription endpoint with the user's key, and the provider returns text. Saving a key sends one second of silence to check it. No other host is contacted.

**Data use:** the developer collects no personal data and runs no server. Audio is processed on the device; it is transferred only to the cloud speech provider the user turns on with their own key, solely to transcribe it. Not sold, not used for advertising or creditworthiness. Privacy policy: https://github.com/derpx06/voicio/blob/main/PRIVACY.md
