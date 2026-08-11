// Flat C seam between the Rust worker and the pinned llama.cpp build.
//
// Compiled by build.rs against the pinned tree's own llama.h, so every by-value parameter
// struct (llama_model_params, llama_context_params — both of which grow fields across llama.cpp
// versions) is laid out by the same compiler that reads the header. Rust sees only opaque
// pointers and scalars.
//
// Everything numeric-affecting is fixed HERE, not configurable: the shape of the execution is
// part of what two replicas must agree on, so a knob would be a determinism hazard. The single
// deliberate choice is flash_attn = DISABLED (never AUTO — AUTO may pick differently across
// devices or versions, and a different attention kernel is a different reduction order).

#include "llama.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

typedef struct shim_ctx {
    struct llama_model * model;
    struct llama_context * lctx;
    const struct llama_vocab * vocab;
    int32_t n_vocab;
} shim_ctx;

// Keep llama.cpp's own logging to warnings and errors. The full model-load narration alone is
// >64 KiB — more than an OS pipe buffer — and the worker's stderr is a pipe when kaspad drives
// it, so an unfiltered load could block the process on write() before it ever reached the job.
// (The bridge drains the pipe concurrently since the same incident; this keeps stderr *legible*,
// not merely unblocked.)
static void shim_log_cb(enum ggml_log_level level, const char * text, void * user_data) {
    static enum ggml_log_level last = GGML_LOG_LEVEL_NONE;
    (void)user_data;
    if (level != GGML_LOG_LEVEL_CONT) {
        last = level;
    }
    if (last >= GGML_LOG_LEVEL_WARN) {
        fputs(text, stderr);
    }
}

shim_ctx * shim_open(const char * model_path, int32_t n_ctx, int32_t n_batch, int32_t n_threads) {
    llama_log_set(shim_log_cb, NULL);
    llama_backend_init();

    struct llama_model_params mp = llama_model_default_params();
#ifdef MISAKA_PALW_CPU_ONLY
    // CPU profile: NOTHING on a GPU. Not "prefer CPU" — zero offloaded layers, so the split
    // point cannot become a hidden knob and the arithmetic is the portable ggml kernels', fixed
    // by the source and the thread count. This is the profile a heterogeneous public fleet can
    // audit within (see `qwen35_pins::CPU_RUNTIME_CLASS`).
    mp.n_gpu_layers = 0;
#else
    // All layers on Metal: the profile is a GPU profile ("apple-metal-arm64"); partial offload
    // would split the numerics between two backends and make the split point a hidden knob.
    mp.n_gpu_layers = 999;
#endif

    struct llama_model * model = llama_model_load_from_file(model_path, mp);
    if (model == NULL) {
        return NULL;
    }

    struct llama_context_params cp = llama_context_default_params();
    cp.n_ctx = (uint32_t)n_ctx;
    cp.n_batch = (uint32_t)n_batch;
    cp.n_ubatch = (uint32_t)n_batch;
    cp.n_seq_max = 1;
    cp.n_threads = n_threads;
    cp.n_threads_batch = n_threads;
    cp.flash_attn_type = LLAMA_FLASH_ATTN_TYPE_DISABLED;

    struct llama_context * lctx = llama_init_from_model(model, cp);
    if (lctx == NULL) {
        llama_model_free(model);
        return NULL;
    }

    shim_ctx * s = (shim_ctx *)calloc(1, sizeof(shim_ctx));
    s->model = model;
    s->lctx = lctx;
    s->vocab = llama_model_get_vocab(model);
    s->n_vocab = llama_vocab_n_tokens(s->vocab);
    return s;
}

int32_t shim_n_vocab(const shim_ctx * s) {
    return s->n_vocab;
}

// add_special = true (the model's own BOS convention applies — part of "same function from
// prompt bytes to tokens"), parse_special = false (the prompt is untrusted bytes; nothing in it
// may alias a control token).
int32_t shim_tokenize(const shim_ctx * s, const char * text, int32_t text_len, int32_t * out, int32_t max_out) {
    return llama_tokenize(s->vocab, text, text_len, out, max_out, true, false);
}

// Decode `n` tokens appended to sequence 0; positions are tracked by the context's memory.
// Logits are produced for the last token of the batch only (llama_batch_get_one's contract).
int32_t shim_decode(shim_ctx * s, const int32_t * tokens, int32_t n) {
    struct llama_batch b = llama_batch_get_one((llama_token *)tokens, n);
    return llama_decode(s->lctx, b);
}

// Copy the logits of the last decoded token. Returns n_vocab, or -1 if unavailable.
int32_t shim_logits_last(shim_ctx * s, float * out) {
    float * l = llama_get_logits_ith(s->lctx, -1);
    if (l == NULL) {
        return -1;
    }
    memcpy(out, l, sizeof(float) * (size_t)s->n_vocab);
    return s->n_vocab;
}

int32_t shim_is_eog(const shim_ctx * s, int32_t token) {
    return llama_vocab_is_eog(s->vocab, token) ? 1 : 0;
}

// Render one token; returns the byte length written (no NUL), or negative on a too-small buffer.
int32_t shim_token_to_piece(const shim_ctx * s, int32_t token, char * buf, int32_t buf_len) {
    return llama_token_to_piece(s->vocab, token, buf, buf_len, 0, false);
}

void shim_close(shim_ctx * s) {
    if (s == NULL) {
        return;
    }
    if (s->lctx != NULL) {
        llama_free(s->lctx);
    }
    if (s->model != NULL) {
        llama_model_free(s->model);
    }
    free(s);
}
