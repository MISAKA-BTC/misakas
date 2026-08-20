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

// Activation capture (PALW execution-commitment legs v1).
//
// A tap reads the graph node `l_out-<il>` — the post-block residual stream, the tensor every
// later layer consumes, so a wrong one cannot be hidden downstream. Capture is armed at
// shim_open_capture() time and NOT afterwards, because llama.cpp only accepts an eval callback
// through llama_context_params: a context opened without taps never installs one and therefore
// runs the byte-identical scheduler path the frozen v2 goldens were measured on.
//
// That asymmetry is deliberate and is the thing to remember about this file: with a callback
// installed, ggml_backend_sched computes a split in sub-ranges cut at every tensor the callback
// asks for, instead of one whole-split compute. Whether that changes the arithmetic is a
// MEASURED question per backend (`--mode v2-legs-selftest` answers it), never an assumption.
#define SHIM_MAX_TAPS 16

// Capture fault codes. Any non-zero value means the Rust side must abort the job with no
// receipt: a partially or wrongly captured leg is exactly what a challenger convicts on.
#define SHIM_CAPTURE_OK            0
#define SHIM_CAPTURE_ERR_DTYPE     1  // the tapped node is not F32
#define SHIM_CAPTURE_ERR_NEMBD     2  // ne[0] is not the hidden dim the buffer was sized for
#define SHIM_CAPTURE_ERR_POSITIONS 3  // more positions in one call than the ubatch cap
#define SHIM_CAPTURE_ERR_RANK      4  // the node is not a plain [n_embd, n_tokens] matrix
#define SHIM_CAPTURE_ERR_DUPLICATE 5  // the same tap fired twice in one call (split ubatch)

typedef struct shim_capture {
    int32_t   armed;
    int32_t   n_taps;
    int32_t   tap_layer[SHIM_MAX_TAPS];
    int32_t   n_embd;
    int32_t   max_positions;
    float   * rows;                       // n_taps × max_positions × n_embd
    int32_t   positions[SHIM_MAX_TAPS];   // positions captured per tap in the current call
    int32_t   status;
} shim_capture;

typedef struct shim_ctx {
    struct llama_model * model;
    struct llama_context * lctx;
    const struct llama_vocab * vocab;
    int32_t n_vocab;
    shim_capture cap;
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

// Which tap slot a graph node belongs to, or -1. Matches `l_out-<il>` exactly: a prefix test
// alone would also match a hypothetical `l_out_something-3`, and tapping the wrong tensor while
// claiming `tap_semantics_id` is the one failure mode this comparison exists to prevent.
static int shim_tap_slot(const shim_ctx * s, const char * name) {
    static const char prefix[] = "l_out-";
    const size_t plen = sizeof(prefix) - 1;
    if (strncmp(name, prefix, plen) != 0) {
        return -1;
    }
    const char * digits = name + plen;
    if (*digits == '\0') {
        return -1;
    }
    int32_t il = 0;
    for (const char * p = digits; *p != '\0'; ++p) {
        if (*p < '0' || *p > '9') {
            return -1;
        }
        il = il * 10 + (*p - '0');
    }
    for (int32_t slot = 0; slot < s->cap.n_taps; ++slot) {
        if (s->cap.tap_layer[slot] == il) {
            return slot;
        }
    }
    return -1;
}

// ggml calls this twice per node it offers: `ask` to learn whether we want the data, then again
// with the data computed. Every fault sets a sticky status and lets the graph finish — refusing
// mid-graph would leave the context in a state the next call would inherit, and the Rust side
// aborts the whole job on any non-zero status anyway.
static bool shim_eval_cb(struct ggml_tensor * t, bool ask, void * user_data) {
    shim_ctx * s = (shim_ctx *)user_data;
    const int slot = shim_tap_slot(s, t->name);
    if (ask) {
        return slot >= 0;
    }
    if (slot < 0) {
        return true;
    }
    if (t->type != GGML_TYPE_F32) {
        s->cap.status = SHIM_CAPTURE_ERR_DTYPE;
        return true;
    }
    if (t->ne[2] != 1 || t->ne[3] != 1) {
        s->cap.status = SHIM_CAPTURE_ERR_RANK;
        return true;
    }
    if (t->ne[0] != (int64_t)s->cap.n_embd) {
        s->cap.status = SHIM_CAPTURE_ERR_NEMBD;
        return true;
    }
    if (t->ne[1] > (int64_t)s->cap.max_positions) {
        s->cap.status = SHIM_CAPTURE_ERR_POSITIONS;
        return true;
    }
    // A second firing means the call was split into more than one ubatch, so the rows already
    // captured are a different token range: silently keeping the last one would commit a leg
    // that covers a fraction of the call.
    if (s->cap.positions[slot] != 0) {
        s->cap.status = SHIM_CAPTURE_ERR_DUPLICATE;
        return true;
    }
    const size_t row_bytes = (size_t)s->cap.n_embd * sizeof(float);
    for (int64_t pos = 0; pos < t->ne[1]; ++pos) {
        float * dst = s->cap.rows + ((size_t)slot * (size_t)s->cap.max_positions + (size_t)pos) * (size_t)s->cap.n_embd;
        // Per row, at the tensor's own row stride: correct whether or not the node is contiguous.
        ggml_backend_tensor_get(t, dst, (size_t)pos * t->nb[1], row_bytes);
    }
    s->cap.positions[slot] = (int32_t)t->ne[1];
    return true;
}

// Opens with activation capture armed for `tap_layers`. `n_taps == 0` installs no callback and
// is byte-for-byte the pre-capture path (`shim_open` below is exactly that call).
shim_ctx * shim_open_capture(
        const char * model_path,
        int32_t n_ctx,
        int32_t n_batch,
        int32_t n_threads,
        const int32_t * tap_layers,
        int32_t n_taps) {
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

    // The context has to know the callback at creation time, and the callback has to know the
    // shim context, so the shim context is allocated first.
    shim_ctx * s = (shim_ctx *)calloc(1, sizeof(shim_ctx));
    if (s == NULL) {
        llama_model_free(model);
        return NULL;
    }
    if (n_taps > 0) {
        if (n_taps > SHIM_MAX_TAPS) {
            free(s);
            llama_model_free(model);
            return NULL;
        }
        const int32_t n_layer = llama_model_n_layer(model);
        for (int32_t i = 0; i < n_taps; ++i) {
            // Ascending, in range, no repeats: the same rules the committed tap profile is held
            // to, checked here so a bad arm cannot become a bad commitment.
            if (tap_layers[i] < 0 || tap_layers[i] >= n_layer || (i > 0 && tap_layers[i] <= tap_layers[i - 1])) {
                free(s);
                llama_model_free(model);
                return NULL;
            }
            s->cap.tap_layer[i] = tap_layers[i];
        }
        s->cap.n_taps        = n_taps;
        s->cap.n_embd        = llama_model_n_embd(model);
        s->cap.max_positions = n_batch;
        s->cap.rows = (float *)calloc((size_t)n_taps * (size_t)n_batch * (size_t)s->cap.n_embd, sizeof(float));
        if (s->cap.rows == NULL) {
            free(s);
            llama_model_free(model);
            return NULL;
        }
        s->cap.armed = 1;
        cp.cb_eval = shim_eval_cb;
        cp.cb_eval_user_data = s;
    }

    struct llama_context * lctx = llama_init_from_model(model, cp);
    if (lctx == NULL) {
        free(s->cap.rows);
        free(s);
        llama_model_free(model);
        return NULL;
    }

    s->model = model;
    s->lctx = lctx;
    s->vocab = llama_model_get_vocab(model);
    s->n_vocab = llama_vocab_n_tokens(s->vocab);
    return s;
}

shim_ctx * shim_open(const char * model_path, int32_t n_ctx, int32_t n_batch, int32_t n_threads) {
    return shim_open_capture(model_path, n_ctx, n_batch, n_threads, NULL, 0);
}

// Return the context to a pristine decode state without reloading the model (ADR-0041 Decision 1').
// Synchronize the backend first, then clear the memory: the model, vocab and context object all
// survive, only the decode state goes. Refuses a capture context — this is the tag path.
// Returns 0 on success, -1 if unusable, -2 on a capture context.
int32_t shim_reset_context(shim_ctx * s) {
    if (s == NULL || s->lctx == NULL) {
        return -1;
    }
    if (s->cap.armed) {
        return -2;
    }
    llama_synchronize(s->lctx);
    llama_memory_clear(llama_get_memory(s->lctx), true);
    return 0;
}

int32_t shim_n_embd(const shim_ctx * s) {
    return llama_model_n_embd(s->model);
}

int32_t shim_n_layer(const shim_ctx * s) {
    return llama_model_n_layer(s->model);
}

// The rest of the geometry a step-space shape profile is built from (P0-8b).
//
// `PalwShapeProfileV3` restates the model's shape so the profile is self-contained, and every
// one of those numbers has to come from the loaded model rather than from a constant a human
// typed: a profile that disagrees with the GGUF describes a different execution, and the court
// would then adjudicate steps that never ran. `n_embd`/`n_layer` were already exported for the
// tap profile; these are the remainder.
//
// Each returns -1 when llama.cpp cannot answer, and the Rust side treats that as fail-closed —
// a geometry it could not measure is one it must not claim.
int32_t shim_n_head(const shim_ctx * s) {
    return llama_model_n_head(s->model);
}

int32_t shim_n_head_kv(const shim_ctx * s) {
    return llama_model_n_head_kv(s->model);
}

int32_t shim_n_embd_head(const shim_ctx * s) {
    const int32_t n_head = llama_model_n_head(s->model);
    if (n_head <= 0) {
        return -1;
    }
    return llama_model_n_embd(s->model) / n_head;
}

int32_t shim_rope_type(const shim_ctx * s) {
    return (int32_t) llama_model_rope_type(s->model);
}

float shim_rope_freq_base(const shim_ctx * s) {
    return llama_model_rope_freq_scale_train(s->model);
}

// Clears the per-call bookkeeping. Called before every decode: `positions[]` is what tells the
// Rust side how many rows this call produced, and the duplicate check depends on it starting at
// zero. The sticky `status` is deliberately NOT cleared — a fault anywhere in the job must
// still be visible at the end of it.
void shim_capture_begin(shim_ctx * s) {
    memset(s->cap.positions, 0, sizeof(s->cap.positions));
}

int32_t shim_capture_status(const shim_ctx * s) {
    return s->cap.status;
}

// Positions captured for `slot` in the last call: the prefill's token count, or 1 per decode.
// A tap that never fired reports 0, which the Rust side treats as a fault — a missing tap means
// the graph did not contain the node the tap profile claims to read.
int32_t shim_capture_positions(const shim_ctx * s, int32_t slot) {
    if (!s->cap.armed || slot < 0 || slot >= s->cap.n_taps) {
        return -1;
    }
    return s->cap.positions[slot];
}

// Copies one captured row out. Returns the value count written, or negative on a bad request —
// never a short row, because a truncated activation would hash to a leaf nothing can reproduce.
int32_t shim_capture_row(const shim_ctx * s, int32_t slot, int32_t position, float * out, int32_t max_out) {
    if (!s->cap.armed || slot < 0 || slot >= s->cap.n_taps) {
        return -1;
    }
    if (position < 0 || position >= s->cap.positions[slot]) {
        return -2;
    }
    if (max_out < s->cap.n_embd) {
        return -3;
    }
    const float * src = s->cap.rows + ((size_t)slot * (size_t)s->cap.max_positions + (size_t)position) * (size_t)s->cap.n_embd;
    memcpy(out, src, sizeof(float) * (size_t)s->cap.n_embd);
    return s->cap.n_embd;
}

// Serialized replay state of sequence 0 — the bytes a checkpoint commits to. The layout is
// llama.cpp's own and is opaque here on purpose: what the commitment says is "this runtime, at
// this version, produced these bytes", and `state_layout_id` carries that claim.
int32_t shim_state_seq_size(shim_ctx * s) {
    const size_t size = llama_state_seq_get_size(s->lctx, 0);
    if (size > (size_t)INT32_MAX) {
        return -1;
    }
    return (int32_t)size;
}

int32_t shim_state_seq_read(shim_ctx * s, uint8_t * out, int32_t max_out) {
    if (max_out < 0) {
        return -1;
    }
    const size_t written = llama_state_seq_get_data(s->lctx, out, (size_t)max_out, 0);
    if (written == 0 || written > (size_t)max_out) {
        return -2;
    }
    return (int32_t)written;
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
    free(s->cap.rows);
    free(s);
}
