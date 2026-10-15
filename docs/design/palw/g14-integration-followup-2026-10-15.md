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

### Original Capture and Owned Acquisition Batch (Unverified)

- A new producer/custodian reader handles original dense and fold TIR captures
  without materializing the whole capture, all step hashes or all selecting rows
  in RAM. It checks the raw/FPC1 original range, spools bounded native leaves and
  tile/row trees to anonymous disk, and verifies original trace, step and
  execution roots before answering. Fold reconstruction advances one native
  position per poll, retaining only the current resume state. Original IDs are
  authenticated arithmetic inputs, not proof of correct token selection. The
  operational quote includes temporary indexes, replay, ID-copy work and response
  I/O; native paid maximum-profile/restart qualification remains open.
- The new dormant owned-acquisition kernel derives a conservative full-source
  envelope from actual original geometry, independent literal custodians and all
  multipart turns. It preserves original funding and clocks and compares all 13
  allocated resource dimensions. A native-config comparison entry point rejects
  substituted wire prices; its caller must still supply real chain parameters
  and authenticated grants. Overflow remains a missing coverage case, not an
  excuse to narrow active profiles or silently declare the whole goal covered.
- Each response has canonical 16KiB parts. Mutable rows/checkpoints keep only
  incremental hashing, offsets and immutable chunk heads, rather than copying
  the accumulated body on every part. The returned chunk records MUST be rooted
  atomically with metadata by the future native fold; local CAS is not authority.
  Finish checks the exact chain, signer, body hash and original source before
  recording an accepted reference. IDs finish only after all canonical chunks
  reproduce the outer trace root. Original deadlines, independent producer/Valid
  duties, once-only collection targets and history-membership/nonce-bound
  reacquisition have source regressions, not executed economic collection tests.
- The filer driver plans Demand, Reacquire, Begin, Part, Finish and Collect from
  one borrowed accepted-state snapshot. Recovery checks each immutable record,
  current accepted slot or current history membership and original demand scope
  before source authentication. A prepared response holds its operational memory
  ticket, and one ID chunk cannot acknowledge the complete owned ID obligation.
  This is not yet a native snapshot getter, signing/submission actor or ACK path.
- A shared native predicate bridge preserves the existing four live TIR court
  predicates and applies original class, artifact, execution, trace and job pins
  for public arithmetic findings. Output checking accepts only authenticated
  original IDs/Flat EOF pins and separates claim-only output mismatch from an
  impossible vocabulary ID. Unknown/unretained output commitments refuse an
  output comparison, rather than convict an honest claim. Findings are private
  non-deserializable values, not rooted offence certificates. A nominated leaf
  alone cannot establish partial-receipt coverage of canonical dependencies.

No build or test was run for this batch. Native versioned claim/owner/chunk rows,
atomic fold/apply/revert, original preclaim SUM and capital, authenticated custody
grants, indexed duties and once-only economics, RPC/runtime recovery and retained
terminal admission still need integration. Independent own replay, sparse proof
or dissection, whole-map population amplification, non-preemptible inclusion and
all-family maximum-size/network terminals are not bought or proven by the new
acquisition envelope. PredicateHolds, OutputHolds and acquisition EOF are never
global honesty, objective conviction or full G14 completion.

### Rooted Storage Continuation (Unverified)

The first original-capture/owned-acquisition batch was saved as WIP `196927d38`
on `codex/integration-followup`. The following continuation remains source only.

- Append-only staged key/row variants separate acquisition metadata, immutable
  16KiB raw chunks and accepted body references. They do not change existing
  OwnerBundle fields, delta 211 or old Borsh ordinals. The dormant adapter returns
  at most two generic delta-210 changes per move, with exact old-row checks before
  atomic map mutation and reverse replay. Finish reconstructs original records
  from rooted heads, not private producer storage or unrooted CAS.
- A typed snapshot getter reconstructs the source against the retained original
  ClaimBundle, registered class and published layout and uses the same accepted
  block/clock as its metadata. Source ID and expected turn must be signed and
  checked on each future carrier, so a previously signed Demand cannot create a
  later turn. Existing-row transitions never implicitly admit a new allocation.
- These sidecars have no native Open/carrier/fee door. Existing ClaimBundle
  payment is not payment for the additional acquisition plan. They are excluded
  from legacy Owner quota/exposure and retirement handling until dedicated
  accounting, original source holds, literal Valid grants and indexed TTL/duties
  are integrated. This is dormant storage plumbing, not an active paid service.
- The provisional ID assembler now appends in place instead of cloning the whole
  prefix per chunk. Exact EOF still authenticates the original trace root; bad
  EOF rolls back the provisional suffix. A source regression checks multi-chunk
  failure atomicity and retention of a pre-reserved buffer. No timing or
  maximum-size benchmark has been run.
- The acquisition driver consumes a cumulative process-local operational ledger
  before polls, chunk reads, hash/decode, ID copies and response preparation.
  Pending/error work is not refunded, and failed ID/body recovery cannot retry
  the same retained buffer. Its snapshot adapter accepts the typed rooted view.
  This ledger is NOT native spent: restarts can still lose local accounting, and
  charging the whole preparation Need per poll is conservative rather than a
  maximum-profile slice guarantee. Native accepted work allocations, predebits,
  slice permits and restart counters remain necessary before activation.
- Terminal progress now obtains clock, reservation, immutable declaration and
  accepted-part bitmap from one accepted state snapshot, rather than combining
  it with a separately read virtual clock. Regression source covers partial
  progress, revert, foreign owner and malformed bitmap/deadline refusal.

The terminal audit also corrected an overly broad suspected gap: existing small
StagedCourt references already use ordinary objective evidence up to 100,000
bytes and do not depend on the legacy-only multipart reader. A typed transport
regression verifies that distinction with an injected native-gate result; it
does not prove a fabricated reference or a real-network conviction. Large
multipart evidence still requires a live legacy reservation. No retained row is
given a blanket 2.88MB entitlement: future large TIR proofs need a specifically
funded original allocation, or an incremental native court producing an ordinary
small rooted certificate. The whole-family, maximum-profile, collusion and
real-network acceptance requirements remain outstanding.

### Native Lifecycle and Work Continuation (Unverified)

Starting from WIP `f85a3e68e`, the following source changes supersede the earlier
statement that acquisition rows have no lifecycle accounting:

- Dedicated owner/allocation, claim/retention and responder EDF indexes now
  participate in actual native delta-210 apply/revert, rebuild and carriage
  consistency checks. Old Owner rows and multiple new allocations for the same
  claim/owner are counted separately for quota and escrow exposure. Raw/ref rows
  carry no duplicate escrow. Original source metadata survives terminal claim
  retirement until the indexed original horizon; native sweep removes expired
  acquisition/chunk/reference rows in bounded batches, not by semantic court CP.
- A bounded metadata reader returns at most 64 current outstanding literal TIR
  duties plus one indexed lookahead, with the accepted snapshot clock. These
  rows are not proof of a new custody signature or a right to slash. Only owner
  holds/exposure are added here; producer/Valid raw-source authority still needs
  a separately admitted signed contract. Global admission/GC throughput and
  population-wide clone/root costs are not established by the bounded batch.
- The original capture reader exposes deterministic per-phase slice quotes and
  cumulative progress before execution. Quotes bind request and cursor and cover
  dense input, prompt/ID/trace EOF, tree/spool work and individual Fold positions.
  Canonical MAC-equivalent work and traffic are distinct dimensions, not measured
  CPU cycles. Pending/error slices keep their pre-debit. Reopening cannot turn a
  reset local meter into a new native allowance.
- An independent work component is derived from the registered program/layout,
  original job/source, native frozen work rules and finite restart allowance.
  Its envelope is ADDITIONAL to acquisition, not a reinterpretation of historical
  payment. Rooted spent, actor lanes, receipt heads/nonces and checkpoints are
  intended to authorize each future local slice only after accepted pre-debit.
  Host tickets and uncommitted transition effects remain non-authoritative.
- New TIR-source pledge/grant domains bind original claim/class/geometry, source,
  complete resource-plan identity and horizon. Literal grants require the exact
  already-native-credited signed Valid receipt AND a separate raw-source grant
  signature. Membership, Sampled, a history grant or an uncredited receipt cannot
  create raw-source liability. The native adapter reads frozen original bundle
  terms, not a later active owner's terms. This contract is not yet persisted or
  admitted and does not itself prove full-SUM payment.

All new regression bodies remain unexecuted. Versioned full-SUM preclaim/carrier
admission, custody storage and producer/Valid collection, native objective
certificates/terminal continuation, explicit inclusion capacity and full-family
maximum-profile/network qualification still need completion. The work component
does not prove native coverage of executor/artifact/index resident memory or the
complete sparse/dissection proof sum merely by counting canonical operations.
No new fence is armed and no existing claim gains new duties or clocks from this
batch.

The deadline audit found an additional mandatory coupling: the present owned
response span prices Begin/Part/Finish, not N cold-capture preparation slices or
their native pre-debit transactions. A global work SUM does not repair that
per-demand clock. Charging the entire remaining preparation allowance on every
turn would also let colluding responders repeatedly wait until the last legal
moment and exhaust the original horizon. The prospective acceptance protocol
therefore still needs an explicitly priced original preparation/readiness phase
and retention of the prepared responder indexes, followed by per-response
slice/pre-debit capacity in the fixed DA spans. This must precede activation; an
operationally cold but honest responder must not be falsely treated as a DA
defaulter by an underpriced clock. Canonical WorkSpent also reverts on reorg:
it is not proof of at-most-once orphaned physical computation across a lost local
journal. Durable verifier recovery and the finite reorg/restart allowance remain
separate obligations.
