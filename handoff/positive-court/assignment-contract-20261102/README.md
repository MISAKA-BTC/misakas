# Adopted Assignment: result-free contracts and native metadata — 2026-11-02

The user explicitly adopted both prepared proposals with
“両案を採択し、実装を進める”. The formal adopted documents are
[RFC-0045](../../../docs/rfc/0045-positive-court-canonical-assignment.md) and
[ADR-0213](../../../docs/adr/0213-positive-court-canonical-assignment.md).
Legacy native verification and that adoption are committed at `09ee83c8e`.
Public activation remains a separate unapproved step.

## Implemented boundary

- Fixed result-free AssignmentTermsV2 and a separate unsigned admission bundle
  with A funding authorization, future C caps and source/prover/cancellation
  preimage references. Terms ID has no output, signature, derived catalog,
  quote, carrier, accepted origin or lease input. The bundle ID binds its
  literal terms and all referenced declarations.
- Role-separated ML-DSA contexts for poster, worker and prover on the same
  bundle. Pure authentication grants no registered role or spend authority.
- Checked A required fee + capital + worker reward versus signed bounds and
  actual-output amount data. The native output qualifier and lock remain
  necessary. A authorization never replaces literal r1 C consent.
- A permanent nonce transition including u64 exhaustion, separate from payload
  retention. There is no reset API and no native-consumed claim based on the
  DTO itself.
- Native registry resolution of the declared TIR class and all three keys.
  Independent canonical job derivation from AssignmentId, using the original
  registered program/layout and the existing narrow or explicit wide profile.
  The full pair is preserved, without primary prefill-draw shortening.
- Assigned Result checking additionally binds seed, temperature, counts,
  context-token budget and an explicit assignment-scoped empty-dependency
  preimage. Generic r1 AdmissionContext did not contain these fields.
  Input, model, program, worker, rule and ancestor expectations come from the
  registry/terms; no expected field is copied from a submitted Result.
- The old header-anchored derivation and the new terms-anchored derivation
  share the same bounded profile/job helper. Original preflight ordering is
  retained by computing the anchor only after metadata/profile validation.

These are derivation and signature preflights. They are not accepted native
Assignment/Case rows, active bond eligibility, verified ancestry/policy scope,
physical service/capacity leases, UTXO locks, a native writer transaction,
P certification, arithmetic completion, credit or payment. The initial registry
fixtures directly install DATA rows for resolver tests; they are not actual
registered/admitted carrier histories. No public preset or wire decoder is
changed by this checkpoint. All seven stages remain in scope.

## Namespace review for this increment

Only new semantic ID/signature domains are introduced:

| Domain | Payload or use |
| --- | --- |
| misaka/positive/assignment-terms/v2/r1 | Full canonical terms |
| misaka/positive/assignment-bundle/v2/r1 | Full unsigned bundle |
| misaka/positive/assignment-dependencies/v2/r1 | Fixed 72-byte version/revision/AssignmentId/zero-list-count preimage |
| misaka/positive/assignment-poster/v2/r1 | Poster signature on bundle ID |
| misaka/positive/assignment-worker/v2/r1 | Worker signature on bundle ID |
| misaka/positive/assignment-prover/v2/r1 | Prover signature on bundle ID |

A repository-wide search before introduction found the proposed terms domain
only in the two source proposals and none of the other introduced domains in
code. Old r1 Result/Case/Witness/P identities and signatures are unchanged.
Local body operations 8–13, aux151 kinds 10–17, Assignment journals, policies,
funding locks and actual state writers still require their complete bounded
schema/owner/restore integration before acceptance. Old 0–7/0–9 remain exact.

## Verification

`assignment-contract-first.json` / `.log`: four tests pass on the recorded
source map. One unused test import warning remains for cleanup before strict
Clippy. The first Core build/checks are pending in `assignment-core-first`;
no Core pass is inferred from the implementation above. Commands preserve the
existing normal compiler dependencies, offline/locked inputs, finite process
limits, debug assertions/overflow checks and default test stack. The operator
script retains source maps and actual results for every attempt.


## Final source checkpoint

The unused test import is removed. On the same final source map, the four
contract tests and eleven Core metadata/assigned-binding tests pass, followed
by strict Core/Contract library Clippy and strict selected Contract test Clippy.
`final-source-validation.json` pins every record and actual test executable.
The first complete opt2 Core unit build takes 19m30s; its retained one-second
compiler sample shows LLVM instruction selection/code generation, not a
Court runtime measurement. The final selected runtime is 0.08s for eleven
Core cases and 0.13s for four contract cases. No native admission, money
writer, certification or activation result is inferred from these figures.

`pre-assignment-fence-fingerprints.json` captures params/schedule/identity for
six shipped presets (including the actual testnet-12 constructor), using the
real pre-Assignment-fence rlib pinned from the eight-case native build. The
helper source, command, artifact SHA and exit codes are retained. This is the
baseline for the upcoming None-preserving fence change; it is not a new-fence
pass or an inferred legacy equivalence result.
