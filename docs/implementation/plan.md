# Whispr implementation plan

## Global constraints

- Native Kotlin/Jetpack Compose Android app, min SDK 29, arm64-v8a delivery.
- All transcription, diarization, transcript storage, and export remain on device after a one-time model download.
- Support English, Hindi, and natural code switching; words appear live while speaker labels may settle later.
- Store finalized transcript text with timestamps and speaker turns only. Delete raw audio after finalization.
- V1 has no accounts, cloud sync, retained audio, transcript editing, summaries, or Play Store publication.
- Use test-first development for behavioral code; leave each focused test suite green.

## Task 1: Android foundation and testable session domain

Create a new Android Gradle project using Kotlin, Jetpack Compose Material 3, Navigation Compose, coroutines, Room, and AndroidX test dependencies. Add a root `AGENTS.md` with the verified project commands and architectural boundaries. Configure SDK 36/target 35/min 29, Java 17, arm64-v8a ABI, release and debug APK output, and a test command that works without an emulator.

Implement a pure Kotlin `core` domain module/package before UI. Its public types must represent session lifecycle (`Idle`, `Recording`, `Finalizing`, `Completed`, `Failure`), provisional/final transcript segments (stable id, text, start/end milliseconds, optional speaker id), and a complete saved session. Add a reducer/state machine that accepts start, live text, delayed speaker update, stop, finalize-success, and failure events. It must reject invalid transitions without silently corrupting state, replace speaker labels by segment id, and make finalization transcript-only.

Write focused unit tests first for valid state progression, invalid transitions, updating a delayed speaker label, and completed session audio-retention behavior. Run the unit test suite and report RED/GREEN evidence. Do not use real microphone, native models, or network calls in this task.

## Task 2: Local storage, export, and model-pack management

Build repository implementations around the Task 1 domain: Room entities/DAO and encrypted application storage for transcript sessions; `TranscriptRepository` for save/list/get/delete; and a plain-text exporter/share payload that includes title, timestamps, speaker labels, and text. Add a `ModelPackManager` that models absent/downloading/ready/error states, validates a downloaded ZIP against a SHA-256 manifest, installs atomically in app-private storage, and removes temporary files after failure.

Provide dependency-injected interfaces/fakes so domain and repository tests do not need Android inference libraries. The model manifest must contain distinct ASR and diarization pack descriptors; URLs are app configuration, never transcript/audio destinations. Raw audio must never be persisted through this layer.

Write tests first for session round-trip ordering, export content, integrity mismatch/atomic install cleanup, and model-ready state. Verify the focused and full JVM tests.

## Task 3: Recording UI and offline runtime adapters

Build the Compose user experience: first-run model setup screen, transcript library, focused live-recording screen, and session detail screen. Use a single clear start/stop action with 48dp targets, visible recording/finalizing/error feedback, accessible labels, dynamic text layout, and a predictable back stack. Home shows an empty state and saved sessions; detail supports copy and Android share sheet plain-text export.

Implement `AudioCapture`, `StreamingTranscriber`, and `SpeakerDiarizer` adapters. AudioCapture uses `AudioRecord` at 16 kHz mono and releases resources on stop/interruption. Runtime adapters run inference off the main thread and emit live segments plus delayed diarization assignments. Add a `SherpaRuntime` boundary that loads prebuilt JNI/model files when installed and reports a recoverable `RuntimeUnavailable` status when they are absent; no fake transcription results in production. Keep model-engine-specific calls isolated behind this boundary so prebuilt `sherpa-onnx` Android JNI libraries can be supplied without rewriting UI/domain code.

Connect ViewModels to the reducer, repositories, and runtime adapters. Add unit tests for view-model state/error handling and instrumentation-ready tests for permission/recording/export navigation. Build a debug APK and verify it contains no model assets or persisted raw-audio path.
