package com.whispr.app.edit

/** Thin JNI wrapper over llama.cpp (see src/main/cpp/llm_jni.cpp). Used only inside the :llm process. */
object LlamaBridge {
    init { System.loadLibrary("voicio_llm") }

    /** Loads the CPU backends (picks the fastest for this phone) from [libDir]. Call once. */
    external fun nativeInit(libDir: String)
    /** Returns a handle, or 0 if the model could not be loaded. */
    external fun nativeLoad(path: String, threads: Int): Long
    /** Returns the model's answer, or null if decoding failed. */
    external fun nativeGenerate(handle: Long, prompt: String, maxNew: Int): String?
    external fun nativeFree(handle: Long)
}
