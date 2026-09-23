# Whispr Android project

## Verified commands

The project uses Java 17. These commands were verified on the host without an
emulator:

```bash
JAVA_HOME=/usr/lib/jvm/java-17-openjdk GRADLE_USER_HOME=/home/manas/.gradle \
  ./gradlew --no-daemon --offline :core:test

JAVA_HOME=/usr/lib/jvm/java-17-openjdk GRADLE_USER_HOME=/home/manas/.gradle \
  ./gradlew --no-daemon --offline :app:assembleDebug

JAVA_HOME=/usr/lib/jvm/java-17-openjdk GRADLE_USER_HOME=/home/manas/.gradle \
  ./gradlew --no-daemon --offline --max-workers=1 \
  -Dorg.gradle.jvmargs='-Xmx768m -Dfile.encoding=UTF-8' :app:assembleRelease
```

APK outputs are `app/build/outputs/apk/debug/app-debug.apk` and
`app/build/outputs/apk/release/app-release-unsigned.apk`.

## Architecture boundaries

- `core` is a pure Kotlin/JVM domain module. It must not depend on Android,
  Compose, Room, microphone APIs, native inference libraries, or networking.
- `core` owns session lifecycle, transcript segment types, saved-session types,
  and the reducer. Invalid events must be explicit failures, never silent state
  mutation.
- `app` is the Android boundary: Compose UI, navigation, persistence, and future
  platform integrations live here. It may depend on `core`; `core` must never
  depend on `app`.
- Do not add model assets, microphone capture, native models, or network calls
  to this foundation.
