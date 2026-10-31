# A3 Panel implementation — 2026-10-31

## Scope and baseline

A3 owns independent Panel replay, authenticated source acquisition, witness construction, bounded submission, restart and takeover preparation. The working branch is `codex/a3-panel-positive-court`; `c8670dbac` records the source checkout’s pre-existing changes as an isolated dependency snapshot. Integrators should apply only the A3 commits after that snapshot. The source checkout was not changed by A3.

The attachment’s RFC-0042 R2 reference resolves to the repository’s current RFC-0044 / ADR-0212 and RFC-v2-0014 Positive Court material. Existing `palw_positive_court_v2` contracts are reused without allocating wire IDs or signature domains. This is an implementation of the private EXACT_STREAM role APIs; it does not adopt or activate a new public-network ruleset.

## Implemented role APIs

- `PanelCaptureV2::replay` performs independent replay and checks program, class, layout, prompt and execution identity. Miner inference remains the ordinary no-capture path.
- `encode_recorded` / `restore_recorded` retain the Panel’s existing capture format. Restoring reauthenticates the expected candidate, prompt/trace commitments, dense leaf shape/order and roots with bounded decoding. It does not rerun inference or restore a trusted pass flag.
- `prepare_parts` builds a reusable dense evidence store. Its `stream_range` constructs only the canonical requested range; callers follow the receiving Node’s cursor. `stream_parts` and `stream_parts_range` remain compatible convenience wrappers.
- `PanelCandidateV2` delegates signing to the existing `DetachedSigner`, checks the common result/witness/policy bindings, and preserves the original author signature during relay takeover. Both positive evidence and a correct execution contradicting a signed Miner output use the shared exact checker; only the Node decides Passed versus Refuted.
- `PanelBatchBuilderV2` checks ordering, completeness and byte/count limits before encoding. `PanelBatchV2::submit` invokes the shared coordinator’s exact checker; an accepted prefix is authoritative even when a later part fails.
- `PanelSourceCacheV2::write_complete_source` assembles complete authenticated source chunks in canonical order. `open_panel_artifact_v2` bounds the existing PALWTIR1 file/header, checks the rooted class and actual parameter inventory root, and supplies model material for a cold Panel replay.
- `PanelSourceCacheV2` authenticates fixed-size source chunks, plans missing canonical ranges and forwards signed requests to the shared coordinator. Acquisition tries bounded providers then the Miner responder and refreshes DAA after I/O. Importing a cache never restores request/response/fee counters or marks Node delivery complete. Invalid provider data consumes the canonical work/bytes allowance where applicable; malformed overlong paths are skipped locally.

## Connection pattern

1. Resolve admitted result, class, policy and original role keys from the shared admission layer.
2. Obtain required source bytes through the shared coordinator. For a PALWTIR1 source, assemble all authenticated chunks with `write_complete_source` into a caller-owned temporary file, then use `open_panel_artifact_v2` to check the rooted class and actual inventory root before replay. This existing format accepts final chunk padding. Keep the mapped artifact path immutable throughout loading/replay. A cached source identity is not model qualification or computation approval.
3. Replay once, create the Panel witness and sign via `PanelCandidateV2`. Persist the signature and bounded capture using the caller’s atomic file replacement/journal policy.
4. After restart, reconstruct the Node case from authenticated records, reauthenticate the original candidate signature, then restore the capture against that candidate.
5. Prepare evidence once; use `candidate.next_step` and the authenticated Node cursor to build bounded batches and submit them. A takeover keeps the original witness author. A new candidate after rejection uses the next attempt, subject to shared coordinator admission.
6. Invoke the shared Node finish operation only after full coverage. No Panel API creates Gate C credit, Miner guilt or payment authority.

## Boundaries and remaining integration

A0 owns native result/witness wire assignments, rooted role resolution and activation dispatch. A4 owns native Court/carrier admission and canonical state transitions. A5 owns Gate C and actual coin accounting. A6 owns atomic durable coordinator/source-budget journal, reorg/IBD and pruning. Those systems must supply the authenticated current branch/ruleset, current DAA and replayed canonical state; cache import cannot substitute for them. No native request is inferred from an old trace demand.

The provider trait supplies committed source bytes, not a Miner trace-generation API. The implemented cold adapter uses the existing PALWTIR1 container and checks registered parameter commitments. Other source formats are not silently reinterpreted. Public-network delivery and accounting require the common native contracts above; unconnected native paths remain closed.

Capture material and dense step trees still scale with execution size. Preparing evidence amortizes the step tree across batches; logits trace-event construction and cone material have their existing costs. No full-scale LLM throughput/RSS bound is claimed. Measured public profiles, all-credit-consumer acceptance and multi-node recovery are separate integration gates.

## Verification

Executor validation passed: `panel_a3` 7/7 and existing `positive_court_roles` 7/7. This includes independently hand-derived greedy/tie output, i32 and i64 Q24, wrong signed output, corrupt/foreign capture, wrong parameters and layout, interrupted transport, and restored Node-prefix/takeover.

The repeatable role suite is `scripts/ci/a3-panel-ci.sh`. The local resource-constrained build used `CARGO_TARGET_DIR=/Users/wata/.cache/misaka-positive-a3-target`, `CARGO_BUILD_JOBS=2`, `CARGO_PROFILE_DEV_DEBUG=0`, `CARGO_INCREMENTAL=0`, and Cargo configuration `profile.dev.package.kaspa-consensus-core.opt-level=0`; these affect build resources only.

Documentation checks were run as required. `check_docs.py` reports three missing references to the already-absent `docs/design/palw/positive-court/IMPLEMENTATION-STATUS.md`. RFC-v2 integrity reports that same missing link. ADR-v2 integrity reports the inherited inventory/fingerprint/count mismatches. Repeating the checks against an exported `c8670dbac` documentation snapshot reproduces these errors; A3 does not rewrite their baseline or unrelated source documents. The new A3 record adds no missing-link occurrence. `git diff --check`, changed-file `rustfmt --check` and CI shell syntax validation pass.

Remote validation passed: `panel_submission` 12/12 and `--lib pursuit::` 16/16 (173 unrelated library tests filtered). Total: 42 selected tests passed, zero failures. The process parent invokes the child cold-Node checker twice, for normal output and a wrong signed Miner output; both child executions pass their expected verdict checks.

The cold Panel test obtains an existing container through signed canonical source requests, reconstructs it from authenticated chunks, removes the provider’s original file, validates the acquired file against the rooted class/inventory, then generates and submits all witness material to the same coordinator. Negative tests cover forged weights, file/header caps, foreign class/artifact, hidden Vec capacity, source budget exhaustion across takeover, cache versus canonical receipt, post-I/O deadline crossing and invalid provider fallback. No Miner witness-generation API is supplied.

Exact commands (with the local build environment/config above):

```sh
cargo test --config 'profile.dev.package.kaspa-consensus-core.opt-level=0' -p misaka-palw-tir-exec --features node --test positive_court_roles --test panel_a3
cargo test --config 'profile.dev.package.kaspa-consensus-core.opt-level=0' -p misaka-palw-remote --test panel_submission --locked
cargo test --config 'profile.dev.package.kaspa-consensus-core.opt-level=0' -p misaka-palw-remote --lib pursuit:: --locked
```

A3 private role implementation and selected validation are complete. No temporary-success verdicts, native wire allocations or activation changes were added. The A0/A4/A5/A6 native connection boundaries above remain required before network use. Public activation settings are unchanged and disabled.
