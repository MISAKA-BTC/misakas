# Actual program metadata and StepSpace allocation inventory — 2026-10-22

Candidate based on `1876fd9c3`; release build and 19 unique selected tests passed.

The existing borrowed canonical program scanner now returns an opaque metadata value with actual collection and byte-element cardinalities and the existing codec/analysis allocation request. Caller-supplied sizes cannot construct this value. The existing standalone allocation adapter delegates to the same scanner and retains its request calculation. Byte elements include `schedule.layers`; they must not be omitted from occurrence bounds.

A private StepSpace inventory uses its actual private element types to bound block vectors, independently growing commit vectors, retained and temporary occurrence vectors, slot bases, fixed/history instances and layer scratch with checked arithmetic and existing pinned Vec capacity accounting. No arbitrary bytes-per-node coefficient is introduced.

This is an allocation inventory, not an acquired lease, physical RSS measurement, CPU price, semantic validation or complete Source owner. Source input/header/layout copies, hash serialization, decode-view and additional source/verifier scratch must be covered before any complete reservation claim. It grants no original admission, fee/escrow authority or public paid standing.

[Exact frozen sources, commands and executable hashes](source-step-inventory/verification.json), [source patch](source-step-inventory/source.patch), [verification runner](source-step-inventory/verification-runner.py). All native build/test execution uses the shared heavy lock. No pre push, arming or deployment is included.

## Verification result

StepSpace inventory 2, existing retained TIR decode/inventory 3, t12 identity 9 and activation 5 passed. [Exact test names](source-step-inventory/selected-test-summary.json). The nonempty native fixture measures actual owned Vec capacities against the quote; malformed/trailing/hostile input and arithmetic overflow are refused. This does not measure RSS or establish source-wide coverage. Frozen Rust/Cargo hashes remained unchanged through execution.
