# ADR-0031: Canonical transcendentals — exp and log are algorithms, not functions

Status: **Accepted (programs land consensus-inert; every binding is registration-measured).**
IEEE-754 does not require correctly-rounded libm, so `expf` is not one function — it is
whichever algorithm the class actually runs. This ADR pins how transcendental sites are
identified, which programs exist today, and what can never be pinned.
Date: 2026-08-16
Relates to: ADR-0030 (Fact 15, the site taxonomy and `transcendental_algorithm_id`),
ADR-0027 §2 (reference semantics), `consensus/core/src/palw_reference.rs` (ruleset v2 — the
arithmetic these programs are written in), `consensus/core/src/palw_transcendental.rs`.

## Facts (read from the pinned tree and the fleet's libc lineage, 2026-08-16)

1. **The vector exp is per-lane pure, and NEON ≡ AVX2 in values.** `ggml_v_expf`'s two
   implementations differ in lane width (4 vs 8), argument order, and blend structure — but
   term-by-term the expressions are the same single-rounding operations (fma argument order
   commutes; the "fast path" `k + j·k` and the slow-path blend's `k + k·j` are the same
   value; the group-wide fast/slow branch selects between per-lane-equal expressions). One
   per-element transcription therefore serves both CPU classes, and the vector exp is NOT
   what separates them (the separators remain fp16 accumulation and repack coverage,
   ADR-0030 Facts 13–14).
2. **The fleet's libm `expf`/`logf` are glibc 2.39's, in the FMA multiarch build.**
   `sysdeps/x86_64/fpu/multiarch/e_expf-fma.c` recompiles the generic C with FMA enabled and
   IFUNC-dispatches on FMA-capable CPUs — both fleet microarchitectures qualify. Which of the
   three (expf) / five (logf) candidate `a·b±c` sites the compiler fused is a property of the
   distro's build, resolvable only by disassembling the fleet's `libm.so.6` — a per-class
   measured contraction fact, exactly ADR-0030 Fact 9's pattern. Both contraction variants
   are implemented; registration binds the disassembly-confirmed one.
3. **The glibc algorithms are fully transcribable.** `e_expf.c` (EXP2F_TABLE_BITS=5: a
   32-entry u64 table, degree-3 scaled polynomial, double internals, the `+0x1.8p52` round
   trick) and `e_logf.c` (LOGF_TABLE_BITS=4: 16 `{invc, logc}` pairs, degree-3 poly, double
   internals) — sources archived alongside the transcription; all `/N` constant scalings are
   by powers of two and therefore exact.
4. **Apple libm is closed.** The scalar tails, `sigmoid`, `softplus` and the GDN decay on the
   Metal/arm classes call an algorithm whose source cannot be transcribed. Consequence: the
   arm-side classes **cannot bind libm sites** and remain structural-fault-only for steps
   that traverse them. That is an honest admission boundary (ADR-0027: hardware that cannot
   match the reference is not admitted to arithmetic adjudication), not a gap to paper over.
5. **`sqrt` and division need no ADR**: IEEE-754 basic operations, correctly rounded by
   ruleset v2 (`rms_norm`'s `1/sqrtf(mean+eps)` and `l2_norm`'s `1/max(sqrtf(sum), eps)` are
   adjudicable with ruleset ops alone). RoPE's `sinf`/`cosf` (glibc, with their own -fma
   twins and large reduction tables) are **named but not yet transcribed**: RoPE steps stay
   catalog-pending until their programs land; nothing else blocks on them.

## Decision

* Transcendental identity = `transcendental_algorithm_id_v1(descriptor)` (ADR-0030's
  domain). Descriptors in the catalog today:
  - `source-poly/ggml-v-expf/llama-030ebb558/per-lane/v1` — the vector exp, per-element.
  - `source-poly/ggml-v-silu/llama-030ebb558/per-lane/v1` — `x / (1 + v_expf(0 − x))`, true
    divide, `0 − x` transcribed literally (not negation).
  - `libm/glibc-2.39/expf/{fma,nofma}/v1`, `libm/glibc-2.39/logf/{fma,nofma}/v1` — the
    scalar sites (sigmoid `1/(1+expf(−x))`, softplus `x>20 ? x : logf(1+expf(x))`, the GDN
    decay `expf(g)`, vector-op tails). One of `{fma,nofma}` per class, by disassembly.
  - `libm/glibc-2.39/sinf/…`, `…/cosf/…` — **reserved, unimplemented** (RoPE gate).
* Programs are written in ruleset-v2 arithmetic only (soft f32/f64; integer bit steps are
  native integers), live in `consensus/core/src/palw_transcendental.rs`, and are frozen by
  golden vectors. New algorithm = new descriptor = new id; never an edit.
* NaN policy: committed bytes are finite (fail-closed), so a transcendental's NaN handling
  is transiently observable at most; programs canonicalize NaN outputs like the ruleset does
  and the divergence from glibc's payload-preserving `x + x` is recorded as unobservable in
  adjudication.
* Validation: local twins (hardware-fma expression mirror for v_expf; loose ≤1-ulp envelope
  against the host libm for the glibc programs — the host is Apple, glibc's 0.502-ulp budget
  makes exact agreement wrong to demand) now; **exact-bits differential against the fleet's
  actual `libm.so.6`/compiled `vec.h` on the class hosts is the ADR-0030 §5.1 registration
  gate** — a program that has not run against its kernel is not a candidate id.

## Consequences

* The GDN layers (18 of 24) become arithmetically adjudicable end-to-end once the glibc expf
  program is fleet-validated: the recurrence itself needs only ruleset ops + one `expf` per
  (token, head).
* Softmax and SwiGLU steps bind `v-expf/per-lane` + ruleset ops (the double-precision sum
  and reciprocal are ruleset binary64); their tails bind the libm ids — for the pinned
  geometry no tail ever executes, and the profile records the ids anyway so an off-geometry
  job cannot dodge adjudication.
* RoPE steps and (on arm classes) every libm site remain catalog-pending — named, honest,
  and visible in the profile rather than silently unadjudicable.
