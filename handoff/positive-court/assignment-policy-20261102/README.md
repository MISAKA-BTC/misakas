# Assignment finite policy, consent preimages and dormant fence — 2026-11-02

Implements the next bounded slice of adopted RFC-0045 / ADR-0213, starting at
`2e6a00a7f`. Full native admission and the full seven-stage goal remain open.

## Implemented

- Fixed canonical AssignmentPolicyV2 commits the Positive policy, independent
  canonical job derivation, fee/source/reserve/physical-service/cancellation
  identities, global/per-role/per-class live caps, candidate/join caps,
  frame/program/layout/catalog/history/retention/replay/heap/work caps and
  all local windows. Zero/unbounded/missing/overflowing/inconsistent policy
  shapes reject. This is not a physical capacity lease or progress guarantee.
- Assignment-only ruleset recipe binds the actual fired fork, Positive policy
  and full Assignment policy. The existing r1 ruleset recipe is unchanged.
- Source/prover declaration preimages bind the Assignment and actual declared
  role, source/catalog/policy, finite global budgets, post-Case cutoffs and
  cancellation policy. Their hashes are authenticated by the existing common
  bundle signatures. They do not start pre-Case data/prover service or create
  receipts, earned fees, reservations or funding rights.
- Params.palw_positive_assignment_v2 is None in every shipped preset. Complete
  policy identity is Some-only in both fingerprints; clone, fired identity,
  future schedule, never normalization and fence visitation include the field.
  Current intake selection requires the exact active Positive policy. Existing
  accepted assignments must retain their own original policy, not consult a
  later intake policy as a replacement.
- Public validation still refuses activation while actual roles/funding/
  capacity/history/Case/P/consumer readiness is incomplete. No public height,
  private profile measurement or successful native Assignment is supplied.

The six existing preset fingerprints are compared against literal outputs
from the actual old core rlib and its captured helper, including testnet-12's
real shipped constructor. The baseline source/artifact/command pins are in
`../assignment-contract-20261102/pre-assignment-fence-fingerprints.json`.
This is stronger than comparing two copies of the same new fingerprint code.

## Actual verification

`final-source-validation.json` pins one unchanged source map, actual test
executables/profiles and five successful process records. Six contract cases
and three Core fence cases pass. Strict Core/Contract library Clippy, selected
Contract test Clippy and selected Core fence test Clippy all pass. All process
groups are empty. The Core library/fence build takes 10m17s; the runtime is
0.02s for the three fence cases. These are synthetic finite policy/consent
fixtures, not measured native or LLM service profiles.

## Still required

Local body 8–13 and aux151 10–17 bounded codec/scanner/heap/restore integration;
real role eligibility and lineage; source/catalog/resource/fee/P-envelope
preimages; actual validated-carrier UTXO locks; one native Assignment/nonce/
role/fund/reservation/history writer; assigned Result/Case admission and
single consumption; P and all use-specific credit/fund consumers; real role
services; restart/reorg/IBD/pruning; complete canonical LLM/resource/retention/
concurrency measurements and separately approved activation. None of these is
inferred from a finite policy shape or an authenticated declaration.


## Superseding rule-scope correction

The separate Assignment ruleset recipe described above was incompatible with
the existing r1 Result-row predicate. Its earlier nine passing cases did not
check that boundary. The recipe is withdrawn and the complete fired-policy
resolver now feeds the unchanged r1 recipe; the actual row-compatibility and
retired-candidate regressions are in
[the correction record](../assignment-ruleset-correction-20261102/README.md).
No native Assignment was accepted under the withdrawn candidate. Earlier
command/result artifacts remain unchanged.
