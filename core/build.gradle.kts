plugins {
    id("org.jetbrains.kotlin.jvm")
}

kotlin {
    jvmToolchain(17)
}

// The cloud provider registry is shared with the desktop app and Chrome: ship it as a resource (no copy).
sourceSets["main"].resources { srcDir(rootProject.file("providers")); exclude("tests/**") }
// providers/tests: the docs' contract cases and the bench clip's reference, for tests only (not in the APK).
sourceSets["test"].resources.srcDir(rootProject.file("providers/tests"))

dependencies {
    // Live cloud transcription (CloudLive): the WebSocket client, on the phone and in the JVM tests.
    api("com.squareup.okhttp3:okhttp:4.12.0")
    testImplementation("junit:junit:4.13.2")
}

// CloudLiveTest / CloudProbeTest: with VOICIO_KEY_* or VOICIO_PROBE=1 set, always rerun and show the table.
if (System.getenv().keys.any { it.startsWith("VOICIO_KEY_") } || System.getenv("VOICIO_PROBE") == "1") {
    tasks.test { testLogging.showStandardStreams = true; outputs.upToDateWhen { false } }
}
