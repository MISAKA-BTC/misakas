# Ambient PoL binary audit — `llama.cpp-ambient-bin`, 2026-08-15

Status: **measurement record** (evidence for ADR-0026). Reproducible; no reverse engineering of
proprietary logic beyond reading exported symbols and static strings.
Method: the four `ambient-xyz/llama.cpp-ambient-bin` tarballs are Git-LFS objects (the repo blobs
are 132–134-byte pointers). The `macos-arm64` object was fetched through the LFS batch API and
its strings/exports read; the strings were diffed against a local upstream `llama.cpp` checkout to
separate Ambient additions from upstream. No control flow was reconstructed and no binary was
decompiled.

```
artifact  llama-2026.07.21.17-ambient-bin-macos-arm64.tar.gz
lfs oid   sha256:df28d69e20940f5f9eef3c414db7e38c5eb271b1231ffd79e62b50ca1852e9cf
size      9 358 784 bytes (LFS), 31 files unpacked
contents  llama-server (arm64 Mach-O) + libllama*/libggml* dylibs; verification lives in
          libllama-server-impl.dylib (47 "ambient" strings) and libllama-common.dylib (12)
upstream diff  rbo_score / mlp_output_p95 / ambient_capture / ambient_forced_tokens / min_rbo
               = 0 files in local upstream llama.cpp → all Ambient-specific additions
```

The three x64 tarballs (`ubuntu-cuda-x64` 283 MB, `ubuntu-rocm-7.2-x64`, `ubuntu-vulkan-x64`) were
not downloaded; the macOS object is sufficient to establish the scheme, and the backend matrix
(CUDA/ROCm/Vulkan/Metal) is already visible from the repo file list.

## What the strings establish (fact)

**1. The verification computation is inside the llama.cpp fork, not a separate daemon.**
Hypothesis C from the earlier survey is confirmed and B is refuted for the compute path. The
Ambient-forked `llama-server` exposes an HTTP verification surface and does the recompute,
comparison and Merkle-proof check itself:

```
/ambient/v1/inference/verify
/ambient/v1/inference/:request_id/token-data
/ambient/v1/inference/:request_id/:token/data
ambient_verify_remote_capture_request
handler-remote-verify-start / handler-remote-verify-finished
"Ambient verification remote phase finished: verified=%d verified_requests=%zu failures=%zu warnings=%zu"
```

The verification logic is source-unreleased (binary-only) but **not** architecturally separated
from the runtime — a verifier must run Ambient's llama.cpp build. This is exactly the
runtime/consensus coupling ADR-0026 §1 refuses.

**2. The comparison rule is statistical/tolerant, NOT bit-exact.** Four tunable knobs, exposed as
CLI flags and a settings JSON, govern acceptance:

```
--ambient-verification-logit-min-prob        "minimum softmax probability for logits included
                                              in Ambient verification (default %.3f)"
--ambient-verification-logit-min-rbo-score    "minimum logit PRBO score ... (default %.2f)"
--ambient-verification-mlp-output-p95-abs-diff "maximum MLP output p95 absolute difference
                                              ... (default %.1f)"
--ambient-verification-settings   JSON: logit_min_prob, logit_min_rbo_score, mlp_threshold,
                                        percentages, num_mlp_failure_threshold
```

Decoded:

- **RBO / PRBO** = (Partial) Rank-Biased Overlap — a *ranking-similarity* score between the
  miner's and verifier's top logits. It accepts numeric differences as long as the **ranking** of
  high-probability tokens is stable. This is a deliberate hardware-FP-divergence absorber.
- **`logit_min_prob`** restricts the comparison to logits above a softmax-probability floor — the
  low-probability tail (the noisiest part across hardware) is excluded from scoring.
- **`mlp_output_p95_abs_diff`** compares *intermediate MLP-layer outputs* with a **p95 absolute
  difference tolerance band** — not equality.
- **`num_mlp_failure_threshold`** permits a bounded number of per-layer MLP failures before the
  token is rejected.

Ambient absorbs hardware divergence in the **comparison** (fuzzy match). This is the exact design
the Litepaper gestures at ("abstracted away … floating point and random numbers"), now pinned to
concrete metrics.

**3. Ambient captures intermediate activations, not just final logits.** The capture is
configurable per layer and includes MLP outputs:

```
ambient_capture / ambient_captures / ambient_capture_layers ("must be an array")
"maximum MLP output p95 absolute difference"
ambient_forced_tokens ("must be an array")
```

So the commitment material already spans **logits + selected MLP-layer activations** — the "go
deeper than logits" direction, already implemented on their side (though compared with tolerance).

**4. Teacher-forcing is the challenge mechanism.** `ambient_forced_tokens` forces the exact token
sequence at the verifier so it recomputes logits at the challenged positions deterministically
(no sampling drift). This confirms the KV-cache caveat: forcing tokens removes *sampling* cost,
not *prefill* cost — the verifier still prefills the forced prefix to reach a mid-sequence token.

**5. Merkle commitment + transport, with an optional HDF5 tensor payload.** The proof path is
real and versioned:

```
merkle_root / merkle_root_hash / Merkle-Proof / merkle.json
"Merkle proof verification failed" / "invalid Merkle proof path" / "invalid Merkle proof rule bit"
"unsupported Merkle proof algorithm" / "unsupported insecure Merkle proof"
"Ambient verification request item: source=%s request_id=%s start_token=%d end_token=%d merkle_root_hash=%s"
"Ambient verification token window normalized: ... prompt_tokens=%d total_tokens=%d start_token=%d end_token=%d"
"llama.cpp was built without HDF5; only Merkle transport verification was performed"
"llama.cpp was built without HDF5; cannot export SGLang-compatible Ambient token files"
```

The captured tensors travel as **HDF5**, and there is **SGLang-compatible** export — i.e. the
*capture format* is meant to be portable across runtimes (vLLM/SGLang/llama.cpp all produce the
same token files), even though the *comparison code* is the llama.cpp fork. The on-chain
`VerificationState` fields the public docs list (`merkle_root`, `assigned_verifiers`,
`assigned_verifiers_token_ranges`, `verified_tokens`, `output_hash`) line up with the server-side
`start_token`/`end_token` window and `verified_tokens` counters seen here.

**6. The chain assigns token ranges; the server verifies a window.** `start_token`/`end_token`
per request item, "token window normalized", and per-token `token_idx` accounting match the public
"validators are assigned token ranges" description. A `verified=… failures=… warnings=…` summary
is the verdict the chain layer consumes.

## What remains unknown (even after the audit)

- The **exact preimage** the Merkle leaves commit (which tensors, in what canonical byte layout),
  the RBO depth/weighting, and the default threshold values (format strings hide the numeric
  defaults). "Insecure Merkle proof" implies ≥1 hardened algorithm but does not name it.
- Whether cross-**backend** (CUDA vs ROCm vs Metal) verification uses the *same* thresholds or
  per-backend settings — the JSON is per-deploy, so this is an operational choice, not visible.
- Challenge *selection* (how ranges/positions are drawn, and against what randomness), committee
  sampling, bond/slash amounts, and LStake accounting — none are in the runtime; they live in the
  unreleased Solana programs.

## Consequences for MISAKA / PALW (feeds ADR-0026)

- The earlier survey's open question — "do they compare with a tolerance?" — is **answered: yes**
  (RBO + p95 band + failure threshold). ADR-0026 must state this as fact and take a position on
  it, not leave it as an inferred maybe.
- Ambient and MISAKA give **opposite answers to the same hardware-divergence problem**: Ambient
  absorbs it in a *fuzzy comparison* (viable because their verification is asynchronous/optimistic
  and off the chain-validity path — a fuzzy verdict triggers a probabilistic slash, it never forks
  the chain); MISAKA absorbs it in an *exact-within-pinned-class* rule. Both are defensible; the
  choice is coupled to *where* the verdict is consumed.
- Three mechanisms are worth **borrowing** on evidence, independent of the comparison choice:
  intermediate-activation capture (already in our trace ambition), post-commit teacher-forced
  challenge (with fresh-validator prefill cost measured, per the KV-cache caveat), and a
  runtime-portable capture format so a vLLM/SGLang miner and a llama.cpp verifier interoperate.
- One anti-pattern is **confirmed by their build**: the verification computation is compiled into
  the inference runtime. That is the coupling ADR-0026 §1 keeps out of the runtime by single-
  sourcing the scheme kernel in consensus-core.
