# 2026-11-03 integration progress — RFC-0045 remains incomplete

The active checkout is `codex/positive-court-a0`. The190-test phase1 checkpoint
is committed as `556ff5c29` (based on `c9c93e18b`).
The user requested multiple agents to finish RFC-0045. Three agents handed off
candidates for the native writer, funds/Case and original materials/P/recovery.
All three subsequently stopped with a usage-limit error. Root continues their
work. An uncompiled candidate is not native authority or a completed feature.

The user separately adopted BOTH relay drafts on 2026-11-03:
「両案を採択し、限定実装を進める」. They are now RFC-0046 / ADR-0214.
This authorizes the limited relay implementation, with conditional partial
settlement, and excludes public activation and migration of old funds.

## Integrated behavior and verified boundaries

- Original fixed Assignment/Positive/native preimages, canonical job and source
  quote can be retained and independently compared after a later policy change.
  The Open preimage has its own `PC2AOPEN` format. Actual native control records
  additionally require `PC2ACTRL`, exact pre-event origin/root/delta watermark,
  ordered populated-input digest/count and actual fee. Those hashes authenticate
  data, not model computation or native acceptance by themselves.
- Raw actions are rehearsed and rerun at their actual accepted-object position;
  unpublished A/Case material has rollback watermarks and is staged in the same
  existing RocksDB batch as state/delta/UTXO changes. Old v7 refuses nonempty new
  output rather than dropping it. Actual journal fixtures remain DATA tests.
- Reserved Assignment derivation owns actual typed allocations. Source/Case
  prices and the deterministic work-unit mapping have finite, explicit
  preimages; quotes do not grant future service capacity. The A retention quote
  is stable across eligible inclusion clocks and is not an earned-service fee.
- Native operator identity and work-key Principal remain distinct. The fixture
  uses two actual registration keys; it does not establish work eligibility.
- New P data, draw/vote binding and collateral holds are separate from old
  S/K/V/PublicCheck certification. A purpose-specific WorkerPayment approval
  still needs a fresh genuine private Case Session. Original m/k and clocks are
  not shortened. Refutation/revocation charging is not replaced by a release.
- RFC-0046's exact77-byte op12 /0x82 signing template is implemented. Actual
  transaction crypto tests pass; the strengthened ordinary Full-UTXO regression
  passed on the prior frozen source. This does not create R, a future UTXO,
  custody, an earned invoice, a P decision or an accepted Assignment.

## Completed executions in this snapshot series

| record | result | scope |
| --- | --- | --- |
| `integrated-native-typecheck-tenth.json` | exit0 | contract/Core/Consensus lib and ALL test targets typecheck |
| `integrated-native-testcheck-ninth.json` | exit0 | Consensus lib/all tests, including independent fee-input Full script fixture |
| `integrated-shared-clippy-fourth.json` | exit0 | strict library Clippy for contract/Core/Consensus |
| `integrated-contract-all-second.json` |73 passed | all contract lib/test targets |
| `integrated-core-focused-fourth.json` |80 passed |10 external Core targets: body, fence, funds, clocks, original material, principals, finite resources, P codec/crypto, relay crypto and42 source regressions |

Each controlled run preserves its source-before/source-after maps, raw log
hash, exact command/environment and resource-monitor observations. In each
completed run the source maps are equal. The latest seven passing runs share exactly the same source map. Earlier
failures include a test-only zero attempt-share parameter and a fixed-boundary
width error; both are corrected without relaxing their validation rules.
The failed command target and failed fixture are retained as failures, not
counted as passing suites. Previous baseline141 is not added to these counts.

Native DB/G14/Full execution `integrated-native-db-legacy-relay-first.json`
passed14 tests on its retained source. The first private Core owner execution
passed14 of15 and exposed a fixed-boundary width error: Borsh is496 bytes, not596.
Both A and Case boundary constants/encoders are corrected; the rerun
`integrated-core-owners-second.json` passed16 of16, including the new missing-history
regression. A new external extent/roundtrip regression guards the A envelope.
The corrected-source contract73 + Core external80 + internal16 + Exec read-only replay21 =190 passing
executions. The prior Native14 and old baseline141 are not added. The artifact
audit is `corrected-integration-artifact-audit-20261103.json`. The redundant
Native second run was deliberately SIGTERM-canceled during rebuild, before any
test executed, to begin the next source stage; its execution JSON/log and separate
operator cancellation record are retained and counted as zero passes. Runs use
the unchanged default optimized test profile and normal stack. A process-group RSS
monitor is not an OS hard quota or a measurement of native request soundness.

## Corrected safety finding

Root found that the new raw-action interleave and its rehearsal propagated
`NativeResource` but swallowed `NativeContextUnavailable` as a semantic refusal.
Missing local history must instead abort/defer the transition, or differing
retention could yield different native states. An exact two-callsite fix and a
raw-dispatch regression were prepared under `candidates/root/local-context-fault`.
They are now integrated and the regression passed in the16-test owner run. The completed runs above retain their original frozen source. Public presets
remain None; the actual A/C path still has missing owners and accepts no job.

## Remaining owners; no completion claim

Complete prospective all-branch reservation/service/retention ownership,
genuine archived-history/Remaining/Case factory and op1–7 writer, full P native
flow and objective charge, A/C/all-prover money/due/refund owners, actual R
registration/fee-burn/custody/F/R2 binding and terminal payment, old-profile
behavior across forks, consumer/B=F=R=0 audit, cold restart/reorg/pruned IBD and
separate real-model/process cost measurements remain. The partial A1-memory
and P-index candidates are NOT applied or runtime-verified. In particular the
future global verifier-pool work bound is not solved by an at-A pool count.

`candidate-handoff-20261103.tar.gz` preserves all currently handed-off candidate
files, bases, patches and partial root follow-ups; its JSON manifest pins every
file and the archive hash. It is recovery material, not an implementation or
success artifact. The live candidate directory remains available for the next
integration phase. No RFC-0045, RFC-0046, overall goal or public activation is
marked complete.


## Next source stage (not applied at this checkpoint)

Root candidates add fixed R rows19/20, structural pair/material consistency,
exhaustive NativeHeap/Wire inventories, a separate Some-only relay fence and
fixed-recipe identity, a body14 real allocation owner, explicit signed fee-only
source mode/release rule, and a pure fixed-anchor registration cutoff. None of
these candidates is covered by the190 passing executions yet. Their actual
native fee/custody/F/R2/payment owners remain missing. The namespace/domain
collision audits cover1,450 visible refs; no conflicting implementation match
was found in the recorded source scopes. See `native-owner-order-audit-20261103.md`
for the pre-coinbase fee ordering, original contract routing and accounting-session
requirements that must be connected before native admission can succeed.

`integrated-exec-replay-clippy-first.json` passes strict Clippy for the actual
node-feature integration target. `integrated-exec-replay-test-first.json` passes
21 A6 read-only replay cases, including real op9 literal decoding, absent assigned
environment refusal and actual independent A1 execution. No native A/Case/funds/P
acceptance is asserted by those local fixtures. The source map matches the other
six passing runs exactly.

## Latest relay/source/cutoff stage

The next root candidates are now applied and checked: see
[relay-stage-progress-20261103.md](relay-stage-progress-20261103.md) for the actual
source scope,76 contract +90 Core unique cases, typecheck/strict Clippy, exact
source differences and remaining owners. The historical190 above belongs to
phase1 and is not added to166 from the later stage. Native R/A/C acceptance and
actual payment remain closed.


## Original profile and control-carriage followups

Original relay selection is now fixed in retained Assignment material, with no
later Params reselection. This is committed as `3a2bf5bed`; see
[the scoped record](original-profile-progress-20261103.md) and source/binary
audit. All 93 selected Core cases passed on that frozen source. This count is
not added to earlier stage totals.

The next change adds a concrete Assignment control-history entrypoint to the
private native archive loader, sharing actual original Full validation and
executed-prefix reconstruction with Case history. Its borrowed framing/input
checks have explicit DATA fixtures and do not grant native sessions. See
[the current carriage record](control-carriage-progress-20261103.md). Native all-test-target typechecking and strict Clippy passed. All six cases
in the focused Core material target passed. RFC-0045 remains incomplete.


## Original rules at the continuation entrypoint

The block walk now passes its selected-parent state to the op9–13 transport.
A fixed prefix scan supplies the Assignment lookup key. For a rooted
Assignment, the loader reads its ordinal-zero frame from the actual journal,
checks the original material, and scans the candidate against those retained
limits. It then checks the original accepted carriage and reconstructs the
pre-event state. The raw fold compares the same material reference against the
current Assignment row. Missing history for a rooted Assignment defers the
local transition. Malformed continuation bodies fail the original codec check.

This change uses the existing Assignment transport and ordering fence. It
covers original rule selection when the relay configuration changes while that
lane is active. Continuing a contract after removal of the enclosing lane still
requires transport and ordering integration. Actual funding, Case admission,
retention and P decisions still require their native owners.

Core and Consensus library Clippy and focused test Clippy passed. The first
library check found an incorrect relative module import in the UTXO child. The
second check passed with the corrected module path. Both focused Core targets
passed all ten cases on the same source as the passing Clippy checks. The
artifact audit records both executable hashes and the original failed check.
The native reader still requires an actual accepted Assignment for end-to-end
coverage.


## Native preflight for Assignment and Case funding

Op8 now checks the actual carrier output against the composed UTXO and
poster key. It checks the signed worker amount and total caps before deriving
resources and rejects an outpoint that already backs a native lock. The full
Assignment funding calculator uses the same output check.

For op9 the accepted original Open carriage retains its reconstructed pre-event
state through rehearsal and the final fold. The fold checks the current Open
row and original material reference, authenticates the Result and C payer,
checks the C output and original signed caps, and independently derives the
original job, source catalog, and required relations from the registered
program and original layout. Missing original carriage defers the transition.
A same-block Open would require a separate in-memory history path.

Op8 also runs the existing horizon calculation using the actual selected
Gate C policy. Op9 derives the same horizon at the original admission DAA and
checks its Case funding and source budget minima. Actual global service
reservations, native capacity custody, complete price and liability ownership,
and the atomic A/C acceptance writes remain necessary.

The first library Clippy run refused an unnecessary lazy error construction.
The corrected run and focused test Clippy passed. The funds, original material,
and resource quote targets passed all 16 Core cases on that source. The scoped
artifact audit records the three executable hashes. The native A/C path still
needs end-to-end execution with actual accepted contracts.
