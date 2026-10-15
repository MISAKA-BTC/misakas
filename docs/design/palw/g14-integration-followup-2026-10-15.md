# G14 Integration Follow-up, 2026-10-15

Status: WIP, source-first continuation. This record does not qualify G14, arm a
fence, or approve a new economic/security parameter.

## Workspace and Scope

- Workspace: `/Users/wata/.codex/worktrees/integration-followup/MISAKA-testnet`.
- Branch: `codex/integration-followup`.
- Starting integration: `5aa52b886130c2a440f477f4969abf15cc9956eb`.
- The remote `pre` and `integ/rfc02-rfc06-12-g14` both named that commit when
  inspected. Local tracking information was stale and was refreshed. No `pre`
  push is part of this continuation.
- Fast-forwarded the new branch to validation snapshot
  `78333e98e81af8fb3fd4dc482d8e720c552de2fc`. Its extra source changes are
  `dc5bc6bc72f7606be7089c52dc54913fe34343e4`: test fixtures, source-location
  assertions, a larger E2E worker stack, and the existing Panel workspace member
  in `default-members`. These repairs are included, not silently discarded.
- The user explicitly chose integrated scope and G14. Do not merge
  `integ/adr-0162-0178`, `integ/rfc0016-impl`, or
  `integ/rfc0017-model-eligibility`. RFC-0016 documentation already on the
  integration line remains included. The excluded branch tips were not ancestors
  of the starting validation snapshot.

The requested property remains all active-plan computation, job, input, output,
state and DA violations, across allowed families and maximum profiles, with a
producer and every Panel signer colluding. A single ordinary public bonded
outside verifier must use only public authenticated claim material and its own
registered model. A missing capability is not closed by dropping the family,
weakening admission, changing the sanction policy, or calling a Pending case
Honest.

## Existing Validation Evidence

These are inspected results from the validation owner's local
`target/consolidated-validation/` logs, not new runs in this workspace. Large logs
are not copied into this source checkpoint. Counts cannot be promoted to a newer
source snapshot.

| Evidence | Source | Recorded result | Limit |
| --- | --- | --- | --- |
| `L3-r1.log` through `L3-r18.log` | `78333e98e` | 18 `cargo check` commands, each exit 0 | Compilation, not runtime qualification; later source edits require a new run |
| `t2d-core-focused-int.log` | `62d637a3c` | Library: 4089 passed, 13 ignored; integration targets: 3, 26 and 6 passed; exit 0 | Earlier source; not all G14 families or node/network terminals |
| `t3-cons-lib.log` | `fd9d6a8e4` | 1009 passed, 16 failed, 29 ignored; exit 101 | Full consensus runtime suite was not green |
| `t3-kaspad-lib-rest.log` | `2940a55bc` | 719 passed, 1 failed, 50 filtered; exit 101 | Skipped `palw_service_`; fixture repair source is included but no fresh full-runtime PASS is established here |

The older consolidated validation document is a historical paused report. The
later raw logs above supersede it only for their exact commands and source
snapshots. Neither compilation nor a repaired test body proves an objective
service terminal, all-family completion, or maximum-size deadline/resource
guarantees.

## Source Continuation

This phase inspects the current integrated code before treating older audit gaps
as still present. Work is divided between core DA/terminal continuation, public
acquisition/recovery, and filer routing. The parent owns shared-cache/resource
and producer-publication changes. No builds or tests are started while this
source phase remains incomplete; regression bodies may be added for the later
consolidated run.

Current source repairs and their evidence are recorded in the checkpoint
commits. They remain unverified until that later run. The full acceptance ledger
and family matrix remain requirements and evidence history, not a percentage
score. In particular, all-family maximum-profile completion, collusion-resistant
inclusion/deadline/resource guarantees, DA collection and real-network recovery
terminals must still be demonstrated before calling G14 complete.

### Current Repair Batch (Unverified)

- Public record recovery excludes exact abandoned `record-<hash>.tmp` writes
  from paid canonical CAS occupancy. Cleanup shares the existing 64-entry scan
  bound and never evicts an authenticated original to make room. Regression
  source covers full count/byte budgets, cache loss and cold reopen.
- Original Flat/Float32 projection cleanup starts inside the blocking writer
  before either projection or binding-header creation can fail. The guard moves
  back to the carrier builder only on success; failures do not leave sidecars
  with no accepted native retention lease.
- TIR replay reports its own execution and output roots, in addition to the
  logits trace. A matching logits trace alone cannot settle a claim or skip its
  required public families. Constraint-stage nomination must progress through a
  neutral stage after a completed own replay when no violation is found, rather
  than wait forever before the next required family.
- PublicPrefix DA collection must distinguish an independently closed claim
  from a collected paid demand. The existing retained-source economic helper
  can skip an already charged producer while collecting still-unconsumed literal
  Valid signer obligations exactly once. No new sanction amount or policy is
  introduced by this repair.

These are targeted source repairs, not a claim that the general TIR arithmetic
localization path, all-family maximum-size execution, every recovery mode or
population-wide deadline/resource guarantees are complete. The new regression
tests have not been executed.

### Next Source Blocker

The prospective PublicPrefix stage currently has inconsistent budget identity:
`palw_staged_preclaim_v1` adds history recovery to the frozen stage allocation,
but `PalwOwnedPublicPrefixCourtV1::admit` replaces `terms.plan` with the smaller
actual public-acquisition envelope. `PalwStagedBundleV1::check` requires exact
equality between the opened child's terms and its frozen stage descriptor.
Thus the full prospective bundle-opening path is not established by the
accepted-state DA fixtures above. The next repair must preserve the immutable
paid allocation while keeping actual execution counters bounded, without
discarding recovery costs, loosening the descriptor check or reducing scope.
