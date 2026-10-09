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
