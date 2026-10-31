# A2 independent source/durability review — 2026-10-31

Reviewed by the independent audit agent in the shared A2 worktree. This report
records source inspection, test construction, and the limits of that inspection.
It does not report a successful native integration run.

## Candidate and validation ownership

- HEAD when this report was written: `f8c79c92f3c0a7500ce0878e25059772c61b2ce9`.
- HEAD at the later session-binding addendum inspection:
  `3d764a0c1fd34300d6e68b9e385e4e50403399dd`; the session correction was then
  present in the working tree. The SHA list includes that correction.
- The reviewed working tree additionally contains the touched-coordinate/full
  old-row CAS correction, its independent regression test, and the node-private
  consumer correction described below. The hashes below
  identify the actual files inspected, rather than attributing these additions
  to that HEAD.
- Root owns Cargo compilation/execution and the candidate rerun. This agent has
  not executed the current Cargo tests and makes no test-pass claim. Earlier
  candidate results do not validate this candidate.
- This agent constructed 19 independent pure source/codec tests in
  `consensus/core/src/palw_positive_source_geometry_v2/independent_source_ledger.rs`
  and 13 RocksDB tests, including the subprocess entry point, in
  `misaka/palw/remote/src/positive_miner_v2_rocksdb_tests.rs`.
- Both files passed this agent's `rustfmt --edition 2024 --check` before the
  report. No production code was edited during this final read audit. The
  touched-coordinate test addition was explicitly approved by root.

## Findings and fixes inspected

### P1: two journals could merge physical I/O into one anchor sequence

Clonable case-anchor handles allowed two different journal paths to construct
identical headers/checkpoints. Both could succeed through idempotent CAS and
perform separate I/O while accounting for one logical sequence.

The inspected fix removes `Clone` from `RocksCaseAnchorV2`, acquires an exclusive
case lease at `bind`, and holds it until handle drop. Database clones share the
same owner/lease registry; the physical DB retains its exclusive OS lock.
Existing heads prohibit creating a fresh journal after a lease is released.
Independent tests cover simultaneous binds through DB clones, different journal
paths, release/rebind, old-journal replay, and process restart. Their current
execution remains root-owned.

### P2: subprocess EOF invalidated the crash/lock fixture

The original child fixture inherited/null-read stdin. `read_exact` after its
pause marker could immediately see EOF, so a marker alone did not establish a
live process retaining the DB lock. A reported lock conflict could therefore
test an already exited child instead of an actual holder.

The fixture now keeps `Stdio::piped()` stdin alive in `Child`, checks that paused
children are still running, and requires the killed process's Unix exit signal
to equal SIGKILL (9). Normal DONE/successful exits are handled separately. This
is a test-fixture fix; it is not evidence that the corrected tests have run.

### P1: Case usage refresh could spend pending source-fee reservations

With fee cap 3 and a pending unit reserving 3, accepting refreshed fee usage 1
would leave only 2 available and make the admitted unit's first response fail.
The old refresh checked actual usage alone. A fully overlapping request also
skipped the fresh-unit loop where the combined check previously occurred.

The inspected fix maintains the pending fee total, checks refreshed actual fee
plus pending reservations against the cap before mutation, and checks this
invariant even for a request with zero fresh units. The independent test treats
such a refreshed row as invalid; it does not claim that another Panel may create
arbitrary non-source fees. The shared-usage refresh test changes work and witness
attempt usage while preserving source-only fee usage.

### P1: first receipt duty had to match the literal op4 RequestId

For A `[0,3)` followed by B `[1,4)`, an op4 referencing B for unit 1 must retain
B's canonical request material as `ServiceReceipt.duty`. The earliest unit fee
reservation from A is not a substitute for the literal request referenced by
the response.

The inspected fix uses the retained request named by op4 for the receipt duty.
Canonical unit completion and the once-only fee remain shared across A and B.
The independent regression requires duty B/response B, A's unit status Complete,
and no new receipt or fee intent when a later op4 references A for the same unit.
Ordered full G14 replay is compared against the original cache result.

### P1: no-output updates skipped old Union/Receipt CAS

`AlreadyComplete` emits no new receipt; `AlreadyApplied` can emit neither union
nor receipt. Conditioning native old-row comparison on output rows therefore
missed modifications to existing rows, despite unchanged Case/summary data.

The inspected fix stores private touched-source coordinates on every prepared
event and touched-unit coordinates on every op4. The private writer compares
the actual old union and, for op4, the complete old receipt before its
`AlreadyApplied` return. These comparisons no longer depend on whether a new
row is emitted. The independent getter regression covers both no-output modes;
it is distinct from a real private-builder/native-row mutation test.

### Earlier durability boundary retained

An unkeyed journal checksum cannot authorize an unknown tail after a protected
anchor. The inspected recovery implementation preserves strict exact-prefix
resume; explicit repair discards the unanchored suffix rather than adopting its
unit identities or discounted meters. The protected local I/O anchor is
independent of canonical chain reorg undo.

### P1 addendum: one raw journal could merge equal status meters from fresh sessions

A single exclusive RocksDB case lease did not itself bind the raw public
`FilePreparationJournalV2` to one live `MinerResponderV2`. With a completed unit,
session A reaches `(status_checks=1, rounds=0, provider_reads=0)`. A separately
constructed session B can initially persist that identical meter into A's raw
journal, then perform another status read. Repeating fresh sessions defeats
`max_status_checks=1` while the recorded meter stays at 1. The recommended
durable wrapper hides its inner responder, but the public recorded API still
needed enforcement.

The initially proposed unavailable-provider variant is a negative control:
after A records one failed provider read, B's first status checkpoint has lower
round/read/work counters and the existing monotonicity check already refuses
it before a second provider read. Do not report a provider-read bypass for this
variant.

Two independent real-RocksDB tests now cover these distinct cases using one
protected scope. The completed-status regression also requires legitimate
drop/reopen through `DurableMinerResponderV2::resume` to restore the exhausted
status budget. Root's pre-fix execution of the completed-status regression is
recorded in `/tmp/misaka-a2-session-repro.log` as 0 passed / 1 failed at the
fresh-session-rejection assertion. This agent read that log; this is a root-run
reproduction, not an independent Cargo execution or a fixed-candidate pass.

The inspected fix creates a private `Arc<()>` identity for each authenticated
responder instance. Raw journal create/open retains the supplied instance's
identity; append requires `Arc::ptr_eq` before any associated status/provider
effect, and a mismatch poisons the journal. A proper resume binds the new
instance and restores its protected state. The identity is process-local and
changes neither the on-disk header/checkpoint format nor network codecs.
Fixed-candidate test execution remains root-owned and pending in this report.

### P2: a dynamic consensus proxy is not a sealed native capability

`ConsensusProxy` aliases a session type with public constructors accepting an
arbitrary `DynConsensus`. A public wrapper accepting that proxy could therefore
receive a mock `ConsensusApi` returning an arbitrary source read port. The
adapter's private duty constructor did not, by itself, establish the stronger
claim that its input was the actual node's consensus implementation. This was
a local preparation/API boundary issue, not a route to accepted native credit
or payment.

The inspected fix makes the kaspad module private, its consumer types/methods
crate-visible, and `NativeSourceDutyV2` plus its resolver entirely
module-private. The module header now states that the proxy alias is not an
authentication capability and requires trusted node callsites plus an actual
VP-private read implementation/current-view validation. The source API remains
unavailable by default. This removes the public consumer exposure without
claiming that arbitrary trusted Rust implementations have become authentic.

## Final private-writer and own-node path inspection

- `TransitionBuilder` remains private. There is no exported source-row setter
  or live call site for this helper in the inspected candidate.
- A missing kernel-route header is an error, rather than a successful no-op.
  The writer checks the ledger seal/revision, actual typed signed bundle,
  expected Case, result and service, followed by old union/receipt CAS.
- The source transition may change Case usage, miner-data status and last-event
  origin. It rejects changes to witness/coverage, admission, liability or other
  Case fields. Value shape is checked before writing.
- Case/Union/Receipt writes run inside a checkpoint. A later write/resource
  refusal restores state and the delta length. Already spent construction
  reservation is conservatively retained; it does not leave canonical row
  mutations behind.
- The concrete miner wrapper authenticates the signed result before binding an
  immutable protected case slot, uses one exclusive durable responder, and
  does not auto-create a replacement namespace after resume failure or reorg.
- Stored-unit aliases share the core DTO and existing literal G14 codecs. Error
  mappings remain rejecting mappings. Neither a preparation nor an encoding
  grants a native receipt, payment or claim credit.
- Empty and nonempty response paths both re-authorize before returning data.
  Clock rollback, changed budget/geometry/reservation bounds, and expired
  cutoffs reject release. Local I/O costs incurred before that rejection remain
  in the durable meter.

No additional blocking finding was identified for this currently dormant
process/private-helper scope after the inspected CAS correction. This statement
does not close the native integration boundaries below.

## Native integration boundaries still open

1. Actual accepted-prefix/current-view publication, generation/root freshness,
   role/catalog/funding/reservation resolution and complete A6 history replay
   are not implemented by the A4 owner in this candidate. The consensus API's
   default source-duty method returns `Unavailable`. A successful pure context
   or prepared engine is not a successful native source flow.
2. A5 money and the A6 complete-case journal must participate in the same outer
   native checkpoint/UTXO transaction. The source-row helper documents this
   prerequisite but cannot implement or establish the missing outer owner.
3. A paid receipt legitimately has `settlement = Some(AcceptedMaterial)` while
   pure source replay creates a receipt with `settlement = None`. Strict full
   receipt CAS deliberately rejects that mismatch. The actual A4/A6 owner must
   resolve the accepted A5 settlement literal from the same current view,
   preserve every source-owned field, and supply the exact full expected
   receipt to the private combined transaction. A public cache refresh or a
   paid boolean does not establish that provenance. Do not weaken full CAS or
   derive Complete from payment to work around this absent owner integration.
4. `ConsensusProxy` remains a dynamic interface rather than a sealed
   authenticity token. The inspected consumer is now node-private, but the
   actual node owner must still supply genuine session provenance and its
   private current-view read implementation. Before claiming a sealed native
   duty, validate that real owner path or provide a sealed read capability;
   merely passing the proxy type does not establish the stronger property.
5. A current request B must be allowed to observe Complete through a literally
   resolved earlier response/duty A for the same canonical unit. Conversely,
   the stored receipt's actual duty must match its own op4 request. Both links
   need real native resolver tests, not a current-RequestId equality shortcut.

## SHA-256 of inspected files

Paths are repository-relative. These identify source snapshots; they do not
replace same-candidate compilation, execution or native integration validation.

```text
d5219307961f1fe9e4bfa5973ea0748b7f6f1d9597935cc55111da7bc6608814  consensus/core/src/palw_positive_source_apply_v2.rs
f2762b167aef92ae87315fea2f66fffe35213ebfd8bcb67bcde7bf262c671fab  consensus/core/src/palw_positive_source_v2.rs
0eb97d67adfb6df5e14e5a5c9b333765ab07a06ef633e7075beebd85b4329f79  consensus/core/src/palw_positive_source_v2_history.rs
d995ecd6380c61976918388949ecbc0b8f5837fbccd5914dcc6d25acf4b574c7  consensus/core/src/palw_positive_source_geometry_v2.rs
02ae42db761e84311356e5af55c0a13fd2332f43c92bee757ed2acf4418bda98  consensus/core/src/palw_positive_source_geometry_v2/codec.rs
c9fa336f7041cb62265935533273fdba36a0052bcab21a64e6b0d47b96fdc4f3  consensus/core/src/palw_positive_source_read_v2.rs
cc58702e34c6eccd9bb04bb09c55fe5db8a7e03e2a720aae5c80f9dbed8e5cfe  consensus/core/src/palw_positive_rows_v2.rs
2a2dd3c1f58c487d38865d773fc760b60862debdc089aee777aa0bd7a72c9fb1  consensus/core/src/api/mod.rs
da500afb9f6426f1c1309456fb9bbe5aef813d0ec05e2313f760507afe43a0d2  components/consensusmanager/src/session.rs
94494aa4083d49e6e5c25ab15365e82f77dfffe07d858c95669e7c90374e353b  kaspad/src/lib.rs
51297618c665585450e0c85a7191127b8f0eb2952135aaec17269638c4e79f3d  kaspad/src/palw_positive_miner_source_v2.rs
f563d5b638643f3d6c8317f356b00cfe23d8ee124812c8c8a88944a3f8f4caa9  misaka/palw/remote/src/positive_miner_v2.rs
af012051e160be481c05642914b94c949507b42aea6c8352911f62343b98ee5b  misaka/palw/remote/src/positive_miner_v2_geometry.rs
79cfb0f234fcd8b4911c4091c08cae24f493d7766ffcc136a2779d18fbb71056  misaka/palw/remote/src/positive_miner_v2_g14.rs
0a454dda00d93fc7adc7df7f494f43853d7528fedf1dd658c3abdacf90251bab  misaka/palw/remote/src/positive_miner_v2_recovery.rs
cb53d36e492ca30d111aa8d59bdc8693f424ddc584c2419e1c4e931ea4639612  misaka/palw/remote/src/positive_miner_v2_rocksdb.rs
fb137300c4db51f20a588bf32f664147a923611759003dd751ade283b9093b93  consensus/core/src/palw_positive_source_geometry_v2/independent_source_ledger.rs
53d8b269bf3c55b746120f11a3edb9636b12617b98e9d039ec5efcca17428b16  misaka/palw/remote/src/positive_miner_v2_rocksdb_tests.rs
```

## Read-only logical NoOp addendum — candidate 4da761423

The source implementation agent performed this static follow-up inspection of
`4da761423bd3f317052e30f2b5dfde7cd38917f7`. This is a read-only review, not
an additional independent test-author review or runtime validation. The three
production files below were clean in the working tree and their SHA-256 values
matched the files read directly from that commit. No production changes or
Cargo commands were made during this inspection. Root owns runtime validation;
earlier candidate/runtime evidence remains historical and is not a pass claim
for this candidate.

The inspection checked the following NoOp boundaries:

- An existing canonical RequestId with an identical complete SourceRequest is
  classified only after concrete geometry, literal op3 framing, requester/key
  binding, request signature, actual-context history/authorization and cutoff
  checks. Changing the carrier, author or signature alone does not create a
  charged request or replace its first retained literal.
- An existing canonical unit receipt is classified only after op4 framing,
  concrete G14 opening/source/unit authentication, actual-context provider and
  history authorization, cutoff checks and immutable service-schedule quote
  comparison. A different authenticated opening, provider or overlapping
  request cannot create another unit receipt, fee intent or source charge.
- Both classifications invoke history identity/order checks before returning.
  TxId/material digest equality, event ordinal zero, exact retained-origin body
  identity and new-origin block/clock/ordinal ordering remain checked. A known
  exact historical delivery can replay idempotently. The actual context still
  owns canonical full-history completeness and accepted-origin authentication;
  these local ordering checks cannot establish those facts.
- NoOp classification precedes history capacity checks and body retention.
  It emits no meaningful retained event and `changes_case()` is false. Cache
  application preserves revision, history length/bytes, every usage dimension,
  pending fees, first receipt and Case row. Meaningful requests with different
  semantic RequestIds remain subject to the normal finite budgets.
- Touched source/unit coordinates survive no-output proposals. The private
  writer checks the full expected Case and signed Result/service bundle, then
  exact old union and (for every op4) full old receipt before its
  `!changes_case()` return. The NoOp exit therefore does not bypass old-row CAS
  and produces no native row delta or case-journal event.

No new concrete blocking finding was identified within this static scope.
Actual native source ingress/current-view authorization, sealed source-view
publication, complete A6 accepted-case history, A5 money/settlement and their
combined outer native transaction remain unconnected in this candidate. A
successful pure context, cache NoOp or dormant private helper does not establish
a successful native source flow or permit claiming Complete from payment.

SHA-256 values for the production files inspected in this addendum:

```text
74eb0e5999b6fa586cbbc2e62de86f011720f283bff9ef43a7fc95bf036d5201  consensus/core/src/palw_positive_source_v2.rs
c5599378993fc1824cd4b72bfac984b0b20bc5adb3d600ea10f2d3ee453de0ea  consensus/core/src/palw_positive_source_v2_history.rs
053c48438c917014c7c7af6865aa63cb64696bf6ddbee90d6df7f42a0e0b4811  consensus/core/src/palw_positive_source_apply_v2.rs
```

## A6 source-journal helper static addendum — 2026-11-01

The source implementation agent reviewed the working-tree helper
`consensus/core/src/palw_positive_source_v2_journal.rs` after its allocation
correction. HEAD was `6817f430788cb08220489cb722854e1b4d1a2df9`, which imports
the A6 shared journal codec from stable A6 commit `5c30f499c`. The helper and its
parent-module changes were not committed at this inspection; the SHA-256
values below identify the actual source snapshot. This addendum is static
inspection only. No Cargo command or runtime test was executed by this agent,
and no source or test file was changed during this follow-up. Root owns the
same-candidate compilation and execution evidence.

The inspected helper binds an existing source event to a borrowed shared
case-journal record. It requires an already bound Case, rejects sequence 1,
and compares scope, Case/Result identities, NativeBody kind, exact op3/op4
prefix, full literal body and complete accepted origin. Source material must
retain event ordinal zero and the actual carrier TxId identity. The shared
full-record journal history digest cannot replace that source material.
Preparation then runs the existing request signature or concrete G14
authentication, geometry, context, schedule and budget checks. A proposal with
`changes_case() == false` is refused as a new journal event, after those
checks; it does not replace the first receipt or create another fee due.

The initial framing check recomputed `MinerResultCommit::case_id()` and
`id()`. The shared identity helper serializes into a temporary Vec, contrary
to the framing method's allocation-free description. The inspected correction
obtains the mandatory `case_basis` and compares its already validated
`case`/`result` fields instead. The framing check now uses borrowed data and a
fixed five-byte prefix without those hash/Vec regenerations. Existing source
preparation still performs its own bounded allocations and hashing; this fix
does not make the complete preparation path allocation-free.

No additional concrete blocking issue was found in this helper's data-only
scope. It returns a pure prepared cache update; it does not authenticate a
native historical guard, publish a source duty, append the complete case
journal, install native rows, move funds or grant credit. The A6/native parent
must precharge all source/private-heap, geometry and resolver work before
calling it, authenticate the complete canonical prefix and material preimages,
merge interleaved global Case usage, and perform final current-view/row CAS
inside the combined native transaction. Those responsibilities remain outside
this helper and were not validated by this inspection.

SHA-256 values for the inspected helper and supporting source snapshot:

```text
a56f922f0cd6f3b8e7e9ce072b8f19ddcdec31cd79f66706f287ed0e36d77fa2  consensus/core/src/palw_positive_source_v2_journal.rs
20bfda01abf453b1991af04b843a6c0a5c72a953d83e3fe7c4a7e8f77e0661f0  consensus/core/src/palw_positive_source_v2.rs
c5599378993fc1824cd4b72bfac984b0b20bc5adb3d600ea10f2d3ee453de0ea  consensus/core/src/palw_positive_source_v2_history.rs
```

Root validation note, 2026-11-01: the source-journal helper reviewed above was committed byte-identically in `b4499397f`. The subsequent fixed-source runner reports record remote library 251 passed/one existing ignored, focused source/geometry 37 passed, shared case-journal 9 passed, default-off library 176 passed/one existing ignored and client metadata exit zero (seven dead-code warnings for the unconnected private callsite). These are parent-run command results, not agent-executed tests or actual native authority. Source/log digests were rechecked against the current files before this note.
