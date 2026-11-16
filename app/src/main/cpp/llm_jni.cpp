// Minimal JNI bridge to llama.cpp for Voicio's optional "Smart Edit". Memory is the constraint, so:
//  - weights stay memory-mapped and are not repacked (use_extra_bufts=false): about half the RAM;
//  - one fresh context per request, sized to the prompt, with a q8 KV cache and flash attention,
//    freed as soon as the answer is produced;
//  - greedy decoding, no allocation of the whole vocabulary logits beyond the last token.
#include <jni.h>
#include <android/log.h>
#include <algorithm>
#include <string>
#include <vector>
#include "llama.h"
#include "ggml-backend.h"

#define LOG(...) __android_log_print(ANDROID_LOG_INFO, "VoicioLlm", __VA_ARGS__)

namespace {
struct Llm { llama_model* model; int threads; };

std::string to_std(JNIEnv* env, jstring s) {
    const char* c = env->GetStringUTFChars(s, nullptr);
    std::string r(c ? c : "");
    if (c) env->ReleaseStringUTFChars(s, c);
    return r;
}
}  // namespace

extern "C" {

JNIEXPORT void JNICALL Java_com_whispr_app_edit_LlamaBridge_nativeInit(JNIEnv* env, jclass, jstring libDir) {
    llama_log_set([](ggml_log_level, const char*, void*) {}, nullptr);  // silence: logging costs time
    ggml_backend_load_all_from_path(to_std(env, libDir).c_str());
    llama_backend_init();
}

JNIEXPORT jlong JNICALL Java_com_whispr_app_edit_LlamaBridge_nativeLoad(JNIEnv* env, jclass, jstring path, jint threads) {
    llama_model_params mp = llama_model_default_params();
    mp.use_extra_bufts = false;  // keep the weights mmap'd instead of copying them into repacked buffers
    llama_model* model = llama_model_load_from_file(to_std(env, path).c_str(), mp);
    if (!model) return 0;
    return reinterpret_cast<jlong>(new Llm{model, threads});
}

JNIEXPORT jstring JNICALL Java_com_whispr_app_edit_LlamaBridge_nativeGenerate(JNIEnv* env, jclass, jlong handle, jstring prompt, jint maxNew) {
    auto* llm = reinterpret_cast<Llm*>(handle);
    if (!llm) return nullptr;
    const std::string text = to_std(env, prompt);
    const llama_vocab* vocab = llama_model_get_vocab(llm->model);
    std::vector<llama_token> toks(text.size() + 16);
    int n = llama_tokenize(vocab, text.c_str(), (int32_t)text.size(), toks.data(), (int32_t)toks.size(), /*add_special=*/false, /*parse_special=*/true);
    if (n <= 0) return nullptr;
    toks.resize(n);

    const int ub = 256;
    llama_context_params cp = llama_context_default_params();
    cp.n_ctx = (uint32_t)(((n + maxNew + 63) / 64) * 64);
    cp.n_batch = ub; cp.n_ubatch = ub;
    cp.n_threads = llm->threads; cp.n_threads_batch = llm->threads;
    cp.flash_attn_type = LLAMA_FLASH_ATTN_TYPE_ENABLED;
    cp.type_k = GGML_TYPE_Q8_0; cp.type_v = GGML_TYPE_Q8_0;
    llama_context* ctx = llama_init_from_model(llm->model, cp);
    if (!ctx) return nullptr;

    std::string out;
    bool ok = true;
    for (int i = 0; i < n && ok; i += ub) {
        llama_batch b = llama_batch_get_one(toks.data() + i, std::min(ub, n - i));
        ok = llama_decode(ctx, b) == 0;
    }
    if (ok) {
        llama_sampler* smpl = llama_sampler_chain_init(llama_sampler_chain_default_params());
        llama_sampler_chain_add(smpl, llama_sampler_init_greedy());
        for (int g = 0; g < maxNew; g++) {
            llama_token t = llama_sampler_sample(smpl, ctx, -1);
            if (llama_vocab_is_eog(vocab, t)) break;
            char buf[256];
            int l = llama_token_to_piece(vocab, t, buf, sizeof buf, 0, true);
            if (l > 0) out.append(buf, l);
            llama_batch nb = llama_batch_get_one(&t, 1);
            if (llama_decode(ctx, nb) != 0) break;
        }
        llama_sampler_free(smpl);
    }
    llama_free(ctx);
    return ok ? env->NewStringUTF(out.c_str()) : nullptr;
}

JNIEXPORT void JNICALL Java_com_whispr_app_edit_LlamaBridge_nativeFree(JNIEnv*, jclass, jlong handle) {
    auto* llm = reinterpret_cast<Llm*>(handle);
    if (!llm) return;
    llama_model_free(llm->model);
    delete llm;
}

}  // extern "C"
