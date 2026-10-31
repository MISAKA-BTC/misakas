# A2 independent source/durability review — 2026-10-31

Reviewed by the independent audit agent in the shared A2 worktree. This report
records source inspection, test construction, and the limits of that inspection.
It does not report a successful native integration run.

## Candidate and validation ownership

- HEAD when this report was written: `f8c79c92f3c0a7500ce0878e25059772c61b2ce9`.
- The reviewed working tree additionally contains the touched-coordinate/full
  old-row CAS correction and its independent regression test. The hashes below
  identify the actual files inspected, rather than attributing these additions
  to that HEAD.
- Root owns Cargo compilation/execution and the candidate rerun. This agent has
  not executed the current Cargo tests and makes no test-pass claim. Earlier
  candidate results do not validate this candidate.
- This agent constructed 19 independent pure source/codec tests in
  `consensus/core/src/palw_positive_source_geometry_v2/independent_source_ledger.rs`
  and 11 RocksDB tests, including the subprocess entry point, in
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
4. `ConsensusProxy` is an alias for `ConsensusSessionOwned`, whose public
   constructors accept `DynConsensus`, including arbitrary `ConsensusApi`
   implementations. It is therefore not itself a sealed authenticity token.
   A caller can wrap a mock API/read port and pass it to the public miner
   wrapper. This can affect local preparation but cannot create a native
   receipt or credit through the inspected path. Before claiming a sealed
   actual-native duty, the node must own the entry/factory and actual session
   provenance, or provide a genuinely sealed read capability. Merely passing
   the proxy type does not establish that stronger property.
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
b81802dd01f29fddaf7c2d7770e6e70dcd1845ff132cb11e23c664c3a497a041  kaspad/src/palw_positive_miner_source_v2.rs
ceb745308dbc5b1df1427f6f21c33dae1cd297eed126e4548b408c986f8b4f49  misaka/palw/remote/src/positive_miner_v2.rs
af012051e160be481c05642914b94c949507b42aea6c8352911f62343b98ee5b  misaka/palw/remote/src/positive_miner_v2_geometry.rs
79cfb0f234fcd8b4911c4091c08cae24f493d7766ffcc136a2779d18fbb71056  misaka/palw/remote/src/positive_miner_v2_g14.rs
6d009a851bcc01ad48e0633b3a6511ebcd6b7b82ac5c9e9cb31c419604006d44  misaka/palw/remote/src/positive_miner_v2_recovery.rs
cb53d36e492ca30d111aa8d59bdc8693f424ddc584c2419e1c4e931ea4639612  misaka/palw/remote/src/positive_miner_v2_rocksdb.rs
fb137300c4db51f20a588bf32f664147a923611759003dd751ade283b9093b93  consensus/core/src/palw_positive_source_geometry_v2/independent_source_ledger.rs
e21056fa2b987386138b85a33e92b1e86f15c6a2c72040d83c3760d1060c3e50  misaka/palw/remote/src/positive_miner_v2_rocksdb_tests.rs
```
