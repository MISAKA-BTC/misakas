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

### Allocation Repair (Unverified)

The prospective PublicPrefix stage currently has inconsistent budget identity:
`palw_staged_preclaim_v1` adds history recovery to the frozen stage allocation,
but `PalwOwnedPublicPrefixCourtV1::admit` replaces `terms.plan` with the smaller
actual public-acquisition envelope. `PalwStagedBundleV1::check` requires exact
equality between the opened child's terms and its frozen stage descriptor.
The continuation adds a dedicated Bundle constructor which preserves the exact
allocated terms and clocks while retaining the actual execution plan separately.
All 13 resource dimensions must fit, including rent, deposit, resident memory,
stored bytes and hashing work. The strict Bundle descriptor comparison remains
unchanged, and the fold supplies the full frozen stage funding. Regression
source follows a prepared prospective ClaimBundle through BundleOpen, child
opening, activation and a real source demand for 0/1/5 qualified Panel
custodians, with funding, descriptor-tamper, serialization and rollback cases.
This is unexecuted source, not a passed original-carrier admission or node test.

### TIR Continuation Boundaries

- The typed arithmetic scheduler descends authenticated original step frontiers
  without converting them to legacy bindings or using a Panel seat's reach cap.
  Agreement on the step tree is explicitly not global honesty when trace/output
  or public semantics remain unproven.
- Explicit tag-156 reacquisition now accepts native typed TIR step leaves,
  frontiers, runs and row nodes after their historical answered bit survives
  local material loss. It preserves the original reservation, next nonce,
  deadline, capital and TIR2/Shard fences; it does not reset the session budget.
  Two native fold regressions and seven scheduler tests are source only.
- An independent own-model source streams the original job into disposable
  anonymous disk Merkle trees instead of retaining every step hash, selecting
  row and executor checkpoint in RAM. Each later read authenticates under its
  computed root. It uses the original constraint/V4/sampled selector and quotes
  checked working RAM and exact disk space before opening. Four source tests
  cover native odd-promotion roots, corruption/cleanup, Flat/Tiled trace codecs
  and streamed/materialized execution agreement. The quote is operational, not
  yet the native paid acquisition plan; restart requires a priced full rerun.
- The sparse public-run evidence entry point rehydrates the original binding
  from registered code, authenticates bounded ranges and invokes the existing
  objective TIR cone kernel without a producer capture. This entry point is not
  yet connected to the runtime filer and has no executed regression. A typed
  first-missing probe now uses that same bounded evaluator with explicit work
  and copy allocations, including a genuinely empty original-root sparse source.
  Allocation refusals are distinct from missing DA, and each wave must debit its
  entire work allocation even on Need/Refused/error. Public-only proof/kernel,
  tampered-range, missing-decode, invalid-ID and work/copy refusal regressions
  remain source only. Missing operands or absent trace pins cannot be promoted
  to an Honest verdict.
- TIR prefix responders now select a raw retained TIR capture or the inner range
  of an exact-job FPC1 envelope. Metadata, snapshot, original-file decoding and
  queued carrier memory are reserved before use. This corrects a real codec and
  unaccounted-memory path; whole local capture decoding still needs its declared
  host capacity and is not a maximum-profile streaming proof.
- Existing reserved non-seat typed DA transport buys 34 sessions per reservation
  (`PALW_DISPUTE_SESSIONS_PER_RESERVATION_V1`), distinct from DA-8's unreserved
  shared 16-session budget. The old reserved-path error text incorrectly said 16.
  TIR units cannot use legacy 64-lane joins. The prospective paid TIR arithmetic
  acquisition envelope, bounded wide trace/ID sources, and objective terminal
  bridge must be connected before calling general TIR prosecution complete.
- Native `TirRowNode` level 0 carries every row tile hash and generated IDs, and
  `TirEvent` has an 8-bit tile field. That legacy transport cannot be treated as
  an all-vocabulary/all-context bound. New own-model readers or unit schedulers
  alone do not repair public acquisition or prove a real-network terminal.
- Flat original EOF streams, literal source sessions and their semantic court
  can now be constructed from the native decode view without synthesizing a
  legacy TIR binding. Their existing serialized fields, exact original Flat
  hash, provisional-until-EOF IDs, independent signer slots and two-pass clocks
  remain unchanged. Source tests compare legacy/view construction bytes and
  exercise a TIR-header decode fixture through EOF, restart and an invalid-ID
  court fault, plus corrupt-root and foreign-scheme rejection. This is a helper
  bridge only: the prospective Flat-TIR admission refusal remains until versioned
  Bundle/fold/node opening and acquisition accounting are actually connected.
- The independent native TIR public-source module derives its private geometry
  from the actual registered program/layout and original header. It authenticates
  raw step leaves/runs, interior row/tile frontiers and full-width tile indices
  under the original domains. Sequential ID chunks stay provisional until exact
  ordered EOF reproduces the original outer trace hash. Authenticated result
  markers cannot be deserialized. Both absolute-format and context-bound vector
  decoders reject oversized allocations before reading their bodies. Six source
  regressions include genuine native classes, wide/ragged tiles and multichunk
  IDs. These are logical acquisition units, not yet paid custody duties: a run
  may need tens of MiB before multipart, and the per-unit bound is not the full
  sum for slots, retries, own replay, journals, dependent waves or terminal work.
  No old wire structure, native admission rule or fence is changed by this module.
- The arithmetic continuation helper consumes authenticated units, asks the
  canonical evaluator for the next missing dependency and advances bounded
  missing ranges without replacing producer operands with its own hashes. Its
  work, copy, material-byte and planning counters are cumulative; a built proof
  retains its host allocation. Already-present credentials that a local reader
  cannot supply do not create another DA obligation. The helper is not activated
  in the runtime: native paid state must bind/restore those counters and clocks,
  account for polling/restart and carry the actual terminal before that is safe.
