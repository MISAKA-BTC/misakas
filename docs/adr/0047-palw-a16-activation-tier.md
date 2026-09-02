# ADR-0047: The A16 activation tier — sixteen-bit activations for the classes int8 cannot carry

> **Residency note (2026-09-02).** This ADR was written on `palw-mainnet-rc-integration`
> (`d3880149`) and never reached `main`'s `docs/adr/`, although the tier it specifies did:
> `palw_base0_a16`, the `qwen25_a16_*` profiles, ADR-0052 ("Activations are A16 codes (ADR-0047)"),
> and code comments in `misaka-palw-base0/src/{artifact,engine_a16}.rs` and
> `consensus/core/src/palw_base0_a16.rs` all cite it by number. It is restored here verbatim so the
> number resolves. Later ADRs moved the A16 class, not this arithmetic: the registered class is
> the court-capable graph-v2 row (ADR-0067 — its `artifact_root` is the openable inventory root,
> not the flat digest), its step space is adjudicable end to end (ADR-0070), and its weight is the
> price of that certification (ADR-0069, on chain by ADR-0075).

**Status:** Accepted (2026-08-21). Amends ADR-0040's catalog by addition; BASE-0 itself is
untouched — its profile references none of the new ops, its class id and behavior are unchanged,
and it remains the liveness floor exactly as ADR-0039 ordered.

## Context: the measured ceiling

The Qwen2.5-1.5B class was built first on int8 activations, with every known remedy applied and
measured one at a time — static per-site calibration, per-channel norm scales carrying the LN
gain, SmoothQuant-style folds, split-scale attention logits, per-channel FFN rows, stream-bias
separation, per-row weight scales, saturation headroom, a per-position-zero scale column. Each
moved the fidelity score by one or two tokens. None was the budget, and the fake-quantization
ladder inside the f32 reference finally said why (top-1 agreement against the exact model over a
57-position prompt):

| activations (float-simulated, weights int8) | top-1 |
| --- | --- |
| dynamic 8-bit | 44/57 |
| dynamic 7-bit | 31/57 |
| dynamic 6-bit | **4/57 — the static-int8 engine's exact score** |
| 15-bit | 57/57 |

Static scales, power-of-two rounding and multi-stage requantization cost ~2 bits, so a static
int8-activation pipeline lands at effective A6 regardless of calibration quality. Separately,
one experiment identified the sink: W8A8 with ONLY position 0 clamped to the generic range
collapses 44/57 → 3/57 — the attention-sink token's giant channels are load-bearing through the
k/v cache, so any tier needs a per-position-zero parameter column.

Two candidate extensions were measured and **rejected**: a fine-row-normalized-by-full-rms norm
op (negligible gain) and dynamic activation scaling (lands at the razor-thin 44/57 and needs
runtime-scale plumbing through every op signature).

## Decision

1. **The A16 op set** (`palw_base0_a16`): `MatMulRequant`, `MatMulRequantRow`, `MatMulRescale`,
   `RmsNorm`, `Requant`, `AddElem`, `MulElem`, `Rope` — i8 weights per row, i16 activation
   codes, `i64` accumulation. Decision E carries to this width: `|w·x| ≤ 127·32767 < 2^22`,
   `A16_MAX_DOT_LEN` keeps every accumulation exact in `i64`, and the differential tests run
   interleaved/blocked/reversed reductions bit-identical.
2. **The boundary rule: `i64` never crosses a step boundary.** Step tiles ride 4-byte lanes, so
   every op whose intermediate exceeds 32 bits is FUSED with its narrowing — committed rows are
   A16 codes or Q24 `i32`, and the wide accumulator lives and dies inside one adjudicable op.
   This is why the tier's matmuls are fused rather than a bare dot, and why the committed output
   row (and therefore the class argmax, lowest index on ties) is defined over i16 logit CODES.
3. **Reuse over redefinition:** `rope_table`'s pinned table (head-tiled at the oracle; the tier's
   own `Rope` adds only the i16 saturation), `silu` (defined on the Q24 values `MatMulRescale`
   commits), the embedding gather, and op 5W (`softmax_shifted`) — which this ADR also
   registers in the catalog (`base0/softmax-shifted/...`, `up_bits` as a one-byte oracle row).
4. **One parameter store.** Every runtime parameter is an integer `(m: i64, shift ≤ 62,
   zero: i64)` triple (17 bytes on the wire), derived at conversion, held in the artifact's
   `a16_params` keyed exactly as the shape profile names them, digest-covered. The engine
   pre-resolves its tables FROM those bytes; the dispute oracle serves THOSE bytes. What the
   engine ran and what the court recomputes with cannot come apart.
5. **The sink lane is court-visible.** At position zero, a parameter name resolves with the
   `.sink0` suffix first (generic row on absence) — in `a16_row` and in the engine alike.
6. **The registry keeps its single-source invariant.** `KERNEL_CATALOG` holds the descriptors;
   `catalogued_kernel_ids_v1` (the coverage gate) and `a16_row` (the court) read the same table;
   `kernel_can_serve_node_v1` gained the tier's shape arms. `recompute_step_row_v1` exposes the
   dispatch for external verifiers.

## Measured consequences

* Formalized-engine fidelity (integer params, committed i16 logit codes): calibration prompt
  44/57 top-1, 56/57 top-5, ρ 0.863; held-out 39/48, 46/48, ρ 0.849 — the fidelity gate
  (top-1 ≥ 3/4, ρ ≥ 0.5) is green on both.
* `qwen25_a16_profile_v1` — 2/27/3 nodes, 100% servable, inventory closed over the graph
  including the implied `.a16` params names. The RC's two-class genesis registers THIS profile;
  the adjudicability boundary re-measured at 3,875,229 leaves for the whole-context job
  (tile 2,048 / ctx 2,048), inside the 2²² ladder.
* The artifact digest gained the `a16_params` presence tag, so derived ids and the RC floor
  root re-froze (`3713ae8e…6d9af6`) — the floor's LOGITS are unchanged; the rename is the
  review gate doing its job.

## What remains open (recorded, not hidden)

* ADR-0040's second-party caveat extends to the new primitives: the A16 ops are this tree's own
  definitions with differential self-tests; no third-party normative reference exists yet.
* The engine's attention arms call the ops per head; the court's canonical input set builds the
  same rows head-major. The loop-closing test pins the risky seams (sink resolution, params
  tiling, head-tiled rope) bit-for-bit; a full per-node replay of an entire job through
  `a16_row` remains future hardening.
* Decode-position embedding (G5d's second half) is inherited unchanged from the int8 record.
