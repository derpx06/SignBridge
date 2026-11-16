plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
    id("org.jetbrains.kotlin.kapt")
    id("org.jetbrains.kotlin.plugin.compose")
}

android {
    namespace = "com.whispr.app"
    compileSdk = 36
    ndkVersion = "27.2.12479018"

    defaultConfig {
        applicationId = "io.github.derpx06.voicio"
        minSdk = 29
        targetSdk = 35
        versionCode = 9
        versionName = "0.9.0"

        ndk {
            abiFilters += "arm64-v8a"
        }
        externalNativeBuild {
            cmake {
                // Build one compile job at a time so the C++ compile does not exhaust a shared machine's RAM.
                arguments += listOf("-DCMAKE_BUILD_TYPE=Release", "-DCMAKE_JOB_POOLS=compile=1", "-DCMAKE_JOB_POOL_COMPILE=compile", "-DANDROID_STL=c++_shared")
                cppFlags += listOf("-O3", "-fvisibility=hidden")
            }
        }
    }

    // Release signing comes from ~/.gradle/gradle.properties or the environment; no key lives in the repo.
    // Without all four values the release build still assembles, unsigned.
    val signing = listOf("VOICIO_KEYSTORE", "VOICIO_KEYSTORE_PASSWORD", "VOICIO_KEY_ALIAS", "VOICIO_KEY_PASSWORD")
        .associateWith { (project.findProperty(it) as String?) ?: System.getenv(it) }
    if (signing.values.all { !it.isNullOrBlank() }) {
        signingConfigs.create("release") {
            storeFile = file(signing.getValue("VOICIO_KEYSTORE")!!)
            storePassword = signing["VOICIO_KEYSTORE_PASSWORD"]
            keyAlias = signing["VOICIO_KEY_ALIAS"]
            keyPassword = signing["VOICIO_KEY_PASSWORD"]
        }
    }

    buildTypes {
        debug {
            applicationIdSuffix = ".debug"
            versionNameSuffix = "-debug"
        }
        release {
            isMinifyEnabled = true
            isShrinkResources = true
            signingConfig = signingConfigs.findByName("release")
            proguardFiles(
                getDefaultProguardFile("proguard-android-optimize.txt"),
                "proguard-rules.pro",
            )
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    externalNativeBuild {
        cmake {
            path = file("src/main/cpp/CMakeLists.txt")
            version = "3.22.1"
        }
    }

    packaging {
        // The CPU backends are loaded by file path at run time, so the libraries must be extracted.
        jniLibs.useLegacyPackaging = true
    }

    // JVM screenshot tests (Robolectric + Roborazzi): PNGs land in app/build/snap/.
    testOptions.unitTests {
        isIncludeAndroidResources = true
        all { it.systemProperty("roborazzi.test.record", "true") }
    }

    buildFeatures {
        compose = true
        buildConfig = true
    }
}

kotlin {
    jvmToolchain(17)
}

dependencies {
    implementation(project(":core"))
    implementation("androidx.activity:activity-compose:1.9.0")
    implementation(platform("androidx.compose:compose-bom:2025.05.01"))
    implementation("androidx.compose.ui:ui")
    implementation("androidx.compose.material3:material3")
    implementation("androidx.navigation:navigation-compose:2.9.0")
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-android:1.10.2")
    implementation("androidx.room:room-runtime:2.8.4")
    implementation("androidx.room:room-ktx:2.8.4")
    kapt("androidx.room:room-compiler:2.8.4")
    implementation("androidx.security:security-crypto:1.1.0-alpha06")
    // bzip2 + tar, to unpack the speech model archive
    implementation("org.apache.commons:commons-compress:1.27.1")

    testImplementation("junit:junit:4.13.2")
    testImplementation("org.robolectric:robolectric:4.16.1")
    testImplementation("io.github.takahirom.roborazzi:roborazzi:1.60.0")
    testImplementation("io.github.takahirom.roborazzi:roborazzi-compose:1.60.0")
    testImplementation(platform("androidx.compose:compose-bom:2025.05.01"))
    testImplementation("androidx.compose.ui:ui-test-junit4")
    testImplementation("androidx.compose.ui:ui-test-manifest")
    androidTestImplementation("androidx.test.ext:junit:1.2.1")
    androidTestImplementation("androidx.test.espresso:espresso-core:3.6.1")
}
