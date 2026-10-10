# ADR-0175 implementation validation — 2026-10-10

Validated the uncommitted immutable-registration implementation on `pre`, based on
`c20fca1d8e3611e201e54d6b662d3d7874ff6328`. This record concerns
[ADR-0175](../0175-registered-models-are-permanently-immutable.md), not the completion of
transport availability or a new verification protocol.

## Implemented behavior

- An independent `palw_model_immutable_v1` fence is resolved at the accepting block's DAA.
  All shipped presets leave it `None`; no existing dormant path is activated.
- After that fence, version publish/promote/withdraw and lineage rollback are refused
  before state writes. The production acceptance collector applies the same refusal;
  recognized but refused objects do not invalidate their carrying block.
- New `ModelLineFounded` IDs bind the canonical artifact root as well as class, founder
  and name. Different weights on the same legacy graph class can register independently.
  Existing class hashes, founding aliases, wire tags and historical IDs remain unchanged.
- Dormant class recovery must retain the definition and original registration attribution.
  Verification plans can be declared only in a new class's registration block. An equal
  DAA score in a later block does not confer that authority. The acceptance rehearsal and
  full fold both track actual creation in the block.
- Improvement evaluation records a winning independent candidate as `CandidateSelected`
  (appended outcome tag 2), preserves settlement and money conservation, and does not move
  the parent's head, versions, Position or AMM. Opt-out/re-entry retains the existing head
  and head history, including a head selected before activation.
- New `EARLY_VERSION` declarations are refused. Fixed-model `PRIVATE_BETA` and service
  benefits remain allowed. CLI version writers reject before signing or submitting.

## Validation

Commands were run from the `pre` checkout with Rust 1.93 on macOS arm64. A local cargo
wrapper used two build jobs, an isolated target directory, existing RocksDB/Snappy static
libraries, and the macOS C++ link flag. It also disabled incremental compilation and debug
info for repository crates. Tests themselves were not disabled or filtered by the wrapper.

```sh
cargo check -p kaspa-consensus-core -p kaspa-consensus -p misaka-cli --tests
cargo test -p kaspa-consensus-core --lib immutable_models -- --nocapture
```

The compile check passed. The dedicated `immutable_models` filter ran **9 tests, all passed**.
It exercises historical versions and delta undo, distinct root-bound registrations with a
nonempty seeded market and Position, definition-preserving dormancy recovery, exact-block
plan binding and rehearsal parity, fixed-model benefits, independent improvement selection
and settlement, opt-out/re-entry, outcome wire tags, and fence/fingerprint behavior.

The following regression commands also passed:

| `cargo test -p kaspa-consensus-core` arguments | Passed |
| --- | ---: |
| `--lib palw_state_v2::tests::model_lines::` | 20 |
| `--lib palw_state_v2::tests::model_benefits::` | 8 |
| `--lib palw_model_market_v1::tests::` | 13 |
| `--lib palw_state_v2::palw_improve_fold_v1::tests::` | 18 |
| `--lib config::params::consensus_params_id_tests::` | 129 |
| `--lib fork_id_v1::tests::` | 20 |
| `--lib consensus_object_discriminants_are_the_ones_the_chain_carries` | 1 |
| `--test rcore_m5_v22_golden` | 2 |
| `--test palw_improvement_v1_fence` | 13 |
| `--test palw_permissionless_model_addition` | 4 |

These regression runs contain 228 passed test executions; three also appear in the dedicated
filter. No failures occurred. `git diff --check` passed. The full workspace test suite and
live network activation were not performed.

## Remaining decisions

Activation height is deliberately unset. Activation requires ConsensusV2, model lines and
artifact-root ownership at or before the new fence. Historical replay below it continues
to apply the old rules.

RFC0004's memory-material collateral decision and kind-specific checker measurements
(MEAS) remain unresolved/unmeasured. The stale status row has been corrected; this policy
does not invent collateral amounts or checker latency, or declare those gates complete.

## Conformance audit of RFC-0004 Part II and the improvement paths (lane INTF, 2026-10-10)

Scope: every path on the integration tree (`b8ae9412b`, which carries R4X's typed roots) that could change a registration past
`palw_model_immutable_v1` — the improvement fold, model lines, typed roots and memory lines. Read from the code; the integration build
of this branch runs the tests named here.

| Path | What it can write | Past `palw_model_immutable_v1` | Verdict |
| --- | --- | --- | --- |
| improvement fold, epoch decision (`palw_improve_fold_v1.rs`, `decide_epoch_v1`) | the line head (`move_head_v1`), head history | a winning `Promoted` is recorded as `CandidateSelected`; `move_head_v1` is called only under `!model_immutable_active` | conforms |
| improvement fold, rollback (tag 81) | the head back to its parent | refused by name (`ImmutableModelUpdate("LineageHeadRolledBack")`) before any write | conforms |
| improvement fold, opt-in / dissolve | the head on re-entry, the head history on dissolve | the existing head and its history are kept | conforms |
| model lines (`ModelVersionPublished` / `Promoted` / `Withdrawn`, `EARLY_VERSION`) | a line's version, root, preview | refused before state writes (`palw_model_definition_update_v1`); a plan binds only in its class's registration block | conforms |
| class re-registration | a class definition | refused when the definition changes | conforms |
| kernel route classes (`register_class`, `register_pipeline_class`, `register_spec_class`) | class rows (kernel table 22 for typed classes) | a class id binds program, plan, commitments and mode; a second registration of an id is refused | conforms (fence-independent) |
| `KernelBound` (tag 106) | a V2 class's kernel binding | refused once bound; it resolves only plain kernel classes (table `classes`), never a typed `Memory` class | conforms |
| typed-roots memory line (kernel table 24, `MemoryLineV1::on_final` / `on_post_final_conviction`) | the line's head (slot commitments, head root, head source) | moves at a memory claim's Final and rolls back on a post-Final conviction; touches no class row, spec, `M0`, artifact root, model line, improvement line, Position or AMM | conforms: the head is the registered computation's declared state, not its definition (ADR-0175: independent material) |
| kernel `improve.rs` (`promotion_decision_v1`, `admit_candidate_v1`) | nothing (pure functions, used by kernel tests only) | a candidate is a NEW class by construction | conforms |

**No code conflict was found; nothing was changed.** Two design gaps and one adjacent finding are recorded for the Lead:

* **DESIGN_GAP (RFC-0004 §II.3 wording).** "A promoted memory state is … evaluated and promoted under §§4–8, with its root recorded on
  the line." Past `palw_model_immutable_v1` a promotion can only be `CandidateSelected`: a memory state worth promoting becomes a NEW
  independent registration (a `Memory` class whose `M0` is that state, its own id, Position and AMM), and the parent's line may record
  its root only as provenance, never as a head or version. No code implements a memory promotion (R4X's design defers it to "a later
  lane"), so nothing conflicts today; the RFC text needs the ADR-0175 reading before any lane builds it.
* **DESIGN_GAP (what a `Memory` registration fixes).** The registration fixes the rule program, slots, base weights and `M0`; the line
  head evolves per verified job by that fixed rule, and RFC-0004 §II.3's "test-time parameter updates" are exactly such state. ADR-0175
  should say so explicitly (the head is per-class computation state, not a version), and a market ever attached to a `Memory` class
  prices the line's semantics, not a frozen weight file. The collateral question (line value at risk) stays the open POLICY item.
* **Adjacent finding (DA16, behind `palw_provider_court_v1`).** `apply_artifact_bound_v1` lets a matured, unrefuted artifact binding whose
  provider pair lapsed be replaced by a binding to a DIFFERENT kernel root. That contradicts ADR-0175 (an accepted root binding is
  immutable) and ADR-0177 (availability is not a consensus matter). It belongs to DA16b's re-scope: a lapsed binding should never lapse
  (or rebind only to the same root).

**Proposal (not implemented): fence ordering.** `palw_typed_roots_v1` shares no state with model lines or the improvement fold, so it
has no technical dependency on `palw_model_immutable_v1`. Two orderings would still close the gap a partial arming could open:
(1) STRONG: `palw_improvement_v1` requires `palw_model_immutable_v1` armed at or below it — the improvement fold is the one path that can
move a head; (2) recommended: `palw_typed_roots_v1` requires `palw_model_immutable_v1` at or below it, so a `Memory` line is never armed
under the old promotion semantics. Both cost nothing at the single release, which arms all three together. Also note that
`validate_palw_model_immutable_v1` checks its prerequisites but does not refuse arming, unlike every other unarmed fence.
