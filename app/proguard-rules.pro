# sherpa-onnx: the prebuilt JNI library looks up these classes, fields and methods by name.
-keep class com.k2fsa.** { *; }

# Our own JNI (llama.cpp bridge) and any other class with native methods: the C symbol names
# (Java_com_whispr_app_edit_LlamaBridge_*) must keep matching the class and method names.
-keep class com.whispr.app.edit.LlamaBridge { *; }
-keepclasseswithmembernames,includedescriptorclasses class * { native <methods>; }

# Named in the manifest and in res/xml/accessibility_service_config.xml (settingsActivity).
-keep class com.whispr.app.flow.FlowAccessibilityService
-keep class com.whispr.app.MainActivity

# Tink (via androidx.security:security-crypto) references compile-time-only annotations.
-dontwarn com.google.errorprone.annotations.**
-dontwarn javax.annotation.**
