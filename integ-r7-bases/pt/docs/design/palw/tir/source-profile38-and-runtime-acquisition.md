# Explicit source compiler profiles (implementation, unverified)

These are producer implementations. They do not establish source fidelity,
maximum-context execution, a native Final or initial-model eligibility.
Existing failed measurements and the original A6/A8 denominator stay unchanged.

## Pixtral / Mistral3 profile 38

`pixtral-mistral3-vision38` is an explicit opt-in mathematical profile. It keeps
bias-free projections, RMS normalization, the distinct H/W axial rotary
frequencies, every vision layer, channel-major spatial unfolding, the learned
patch merger and both projector layers. `VisionSpec.math_profile = None` keeps
the existing lowering. The new profile's full graph has a distinct program and
parameter identity. Per-head attention retains every head and reduction order;
its intermediate score matrix has one head rather than all heads.

The source-backed complete frontend declaration accepts `stage.vision38`, with
an exact calibration file/hash, normalization and quantization policy. It reads
actual original tensors through the bound checkpoint, checks declared/read
inventory equality, materializes real tensor files and derives Source28 PC/R.
It cannot substitute this component for the complete Mistral3 decoder+vision
pipeline. It rejects ambiguous simultaneous frontend/canonical/vision modes.
The wrapper configuration is independently file-pinned.

The publisher's maximum image is 1540×1540 (110×110 patches). One attention
head has 146,410,000 score cells; the complete 16-head graph still exceeds the
frozen 512-node block grammar. The original image's expanded reference also
exceeds Source28's 32 MiB limit. These are retained named refusals in the frozen
path. An explicitly selected new source/execution/resource/court profile must
own the larger graph and reference before it becomes admissible. Merely
changing metadata or old validator constants is not an implementation of that
contract.

## PALWQ361 exact acquisition

The public frontend source accepts an explicitly selected `.palwq36` file with
magic `PALWQ361`. It never selects among several runtime artifacts by model name.
The complete frontend's original file SHA pins apply before opening and after
materialization; a different runtime variant must have a different declared pin.
The reader streams the header with checked counts, offsets and allocation bounds
and requires a contiguous complete weight region with no trailing payload.

The source view exposes:

- original I8 weight codes and `.exp` group exponents, by original directory name;
- `palwq361.rope.cos` and `.sin`, original I32 rotary tables;
- `palwq361.parameter.<original-name>.m`, `.s`, `.z`, exact I64/I8/I64
  views of the original seventeen-byte quantization rows.

All views are flat, so the public canonical program must explicitly reshape and
implement their operations. Implicit float decoding refuses. Parameter reads
retain all zero-point bits: some runtime operations use them as coefficients.
The native `palwq361.*` configuration fields cannot be overridden by a sidecar.
Bounded batched range reads avoid loading the roughly 0.7 GiB parameter header.

Acquisition is not a PALWQ361 forward-to-TIR compiler. The complete runtime
still requires an exact mirror of grouped quantization, GDN recurrence and
key/value-head mapping, routed+shared experts, gating, source rotary tables and
decoding semantics. The 248320×2048 embedding has 508,559,360 elements, above the
frozen TIR element cap. A new authenticated lazy parameter contract or a proved
banked lowering is required; it cannot be flattened into a falsely admitted old
parameter. This work grants no source-fidelity or Final credit.

Unit/integration test sources were added for the new graph layout and bounded
container reader. They have not been run; code-first ordering remains in effect.

## Actual decoder source compiler

`stage.decoder38` selects the generic HF→ModelSpec→HL→integer lowering and the
same bounded actual-tensor materializer. It pins calibration bytes and policy,
requires the original declared maximum context exactly, preserves linear weight
width and recurrent precision as explicit conversion choices, and emits Logits
in canonical V2. This is usable for the existing dense-family readers without
preparing a separate hand-written canonical graph.

For Mistral3, an explicit image-row declaration binds the decoder input to an
earlier `Final` vision/projector stage. The producer checks original placeholder
ID, row count, hidden width, I32 dtype and the upstream materializer's exact
power-of-two output unit before writing the decoder parameters. Complete source
inventory equality still includes both components. Prefix-LM and cross-attention
semantics refused by the existing generic lowerer remain refusals.

The full image also exceeds frozen Master32's 1 MiB job-image/position-input
limits. A new maximum contract must bind actual image, expanded reference,
upstream output transport, restoration and court charges together. Profile38
cannot enlarge those native limits or qualify a full-size launch by itself.

Master32 SDK payload builders now expose actions34/35/36 separately from old
Stage28. The typed Commit builder checks the exact actual invocation input root,
actual admission, generated IDs and the original maximum quota. The corresponding
`native-payload` kinds accept the bounded invocation envelope for signing; the
native fold independently authenticates every prior component Final.
