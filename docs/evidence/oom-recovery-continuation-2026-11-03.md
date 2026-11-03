# OOM recovery continuation — 2026-11-03 JST

Evidence cutoff: 2026-11-02 19:43 UTC. This is an intermediate record, not a declaration of network recovery. At this cutoff the public95 node remains stopped and the Explorer filler remains paused. The first Linux candidate is still building. No new candidate has been installed into the production service in this continuation.

## Synchronization failure found by the unsigned test

The unsigned candidate at revision `69ca1cace62f5eafba46788984330dab2f76db5b` received 18,274 pruning-proof headers and stopped on missing reachability for a parent. Its memory use was approximately 600 MB; this was a synchronization panic, not another OOM. The failed private datadir and original failure result are retained and are ineligible for handoff.

Legitimate zero-weight lane blocks can give a parent and child equal blue work. Sorting by work and hash does not necessarily put the parent first. Import now orders included dependencies structurally and retains the actual maximal parent antichain. Reconstruction also orders forward dependencies and reverse future counts structurally, while keeping the existing sortable priority among ready entries. A typed local-context failure propagates through the runtime proof getter and prevents pruning from starting with an unavailable proof. Neither economic comparison nor claim acceptance changes.

Integrated runtime commits are `4d031b1bc` and `12ae97094`. A test-only missing import was corrected in `b6f58827b`. Nine actual DB regressions pass, including equal-work diamonds, parent antichains, reverse counts, invalid dependency graphs, cross-level boundaries, and a positive-work proof that is built, validated and rebuilt through its cached descriptor. The unified build also compiles the full Node library.

## Additional cache retention corrections

`d4b8339cd` updates reachability sets inside the cache write lock. Cache-only sets reuse their allocation; existing readers keep immutable snapshots through copy-on-write. Two actual DB regressions verify these properties, persisted membership and eviction.

`bf294254d` replaces the full EVM-state cache's nominal 64-item limit with a byte policy of `scaled(40_000_000)`, minimum retained items zero, without random budget padding. The existing estimator counts account, code and storage capacities. Oversized entries still persist and reload from the DB. Two actual DB regressions verify bounded retention and byte-identical answers after eviction and reopen. Two declared-budget checks pass as well. Combined with the pruning tests, these are 15 passing regressions on the integrated source. Documentation checks report no new issues; the existing 43 known reference occurrences remain.

These cache changes do not reserve cold decoding, caller-owned deep clones or DB write batches. The actual EVM-state size on the affected production node has not been measured. The earlier DHAT run did not reproduce the production workload or the approximately 10 GiB anonymous-memory growth. Attribution of that entire growth remains unresolved.

## Branch observations and recovery reference

Read-only observations around 19:33–19:36 UTC found B0 reporting synced at DAA 19,446–19,447 and producing legacy heartbeat blocks. B1 remained on sink `6671f066…191f3` with virtual DAA 19,389 and reported unsynced. Its template response reported synced because mining may use the existing sync-rate fallback; this does not replace the stricter synchronization check.

Both one-head fork-choice responses reported the same economic tuple: frontier blue score 60,588, safe weight 5,698,420,463,928,576 and bounded immature weight zero. These responses are diagnostic sender claims, not independent validation of the branch. The observed deep-fork rejection is therefore not corrected by allowing a blue-work tie-break. B0's advancing branch is the explicit operational recovery reference, subject to independent normal synchronization in a separate unsigned datadir. This does not assert a globally superior economic branch or alter the fork-choice rule.

Separate strict B0 validation and historical B1 validation helpers preserve their distinct meanings. Strict validation still requires synchronized and indexed observations on both nodes, three matching samples, a stable template frontier, signing roles off, resource guards and normal closed shutdown. Historical validation never sets current-network-health or strict-validation eligibility true. Neither script automatically promotes a datadir or starts an actor.

## Operational preservation

The original public95 live baseline and normal shutdown evidence remain intact. A separately reviewed original-app read-only restoration helper is prepared but has not been applied. A fresh B0 live baseline was captured without hashing a live DB or recording raw command lines, environment or private-key contents. Its receipt SHA is `afbb37ff1a7de829aacd3718f136c6f17ba4cd06ba4d32f1b1a83b775c861a23`.

Production activation, closed-datadir handoff, filler resumption, PALW execution, Court progress and public Explorer freshness remain unverified at this cutoff. Existing databases, keys, journals and evidence must remain preserved during the next steps.

## Later read-only observations — 20:04 UTC

The bounded 19:53 B0/B1 capture preserves both process and invocation identities. B0 had progressed from 119 to 122 produced blocks, then reported `holding` with `peers=false` and `participation_allowed=true`; both nodes reported unsynced. The primary113 service is now stopped with a different invocation. Its shutdown ownership is being coordinated with the separately authorized 113 build task; this continuation has not changed that service.

B0's journal provides a direct reason for absent economic progress: an actual QWEN attempt completed and was submitted after approximately 422 seconds, but merged admission reported `COURT_RETENTION_BELOW_GUARANTEE`, with 20,606 DAA promised against 20,608 required. The accepting block remains valid; the claim receives no reward/work/consensus credit. This confirms a live delayed-merge retention failure, separate from OOM and peer loss.

Producer-only voluntary padding is not a compatible repair: admission requires exactly the canonical header-derived retention value, and the field participates in the draw inputs. No existing attempt inclusion-expiry field provides an alternative bound. The 20,000 DAA activation remains unchanged. Existing claims are not rewritten or re-signed, and the credit rejection is not waived. Model credit recovery is still unverified.

## Follow-up evidence — 2026-11-03 00:21 UTC

The original November 2 19:41 JST interruption was a memory-cgroup kill on public95: the kernel killed `kaspad` at a 10 GiB limit with 10,376,820 KiB of anonymous RSS. The kernel record does not identify a single Rust owner of those bytes. The source now limits several previously unbounded owners, including retained bodies, EVM state caching, output queues, and snapshot serialization, and returns oversized receive scratch at message boundaries. Inbound allocation admission and complete coverage of RPC construction remain open.

Public95's later nonsigning working copy exited normally at 23:03:31 UTC, 56 minutes after one SIGINT. The original 300-second stop deadline failed and remains recorded separately. The closed working APP inventory contains 1,306 files and is inode-disjoint from the preserved original. The original executable's only protected metadata change was a same-inode perf build-ID cache link. The newer binary has not been installed on public95.

An unsigned fresh trial using `16ab41f30` passed checkpoint and trusted GHOSTDAG reconstruction, then twice reached the end of header sync and was refused a PALW pruning snapshot. In the first attempt B0 moved its single stored snapshot from `10cf…` to `4dc9…` during header sync. The second attempt requested the current `4dc9…` after B0 had logged durable capture; this rules out another point movement as its explanation. The old serving path returned the same not-found answer for missing data and several local errors. The trial stopped normally at its deadline and is ineligible for activation. Neither retry bypassed snapshot checks.

The sealed source for the next Linux candidate is `ad9016eec5a6bc76b5c08492281459404cc0802a`. Its 43 targeted tests passed, including actual database restart, safe shutdown and queue drainage, pinned transaction-ID projection against the persisted codec, corrupt snapshot recovery, and snapshot spill classification. The test log SHA256 is `27be956f110d98eca8580bd848ab5eea5f16945b21bb96b2dd0977e84b9d97d6`. The separate Linux build started at 00:14 UTC with an 8 GiB memory limit. The snapshot diagnostics distinguish local causes without changing the wire profile, acceptance or capacity ceilings. They have not yet identified the second live refusal branch.

The next operational recovery path is an independent unsigned copy of a normally closed B0 database, followed by resumption of the original B0 and a separate current-reference canary. It is a trusted donor replica and does not qualify as fresh IBD or demonstrate a globally preferred economic fork. No producer, signing key, or journal is copied into that relay. The copy has not been made. A separate verified archive of the failed short trial allowed retirement of only its closed scratch APP, freeing approximately 3.2 GB on IBM without changing either original node or the trial's logs.

Read-only Explorer analysis found 329 old selected-parent links before the nearest observed common ancestor `e0eb35a8…c490` at DAA 19,245. The corresponding 373 accepted transaction rows remain in PostgreSQL. The database is about 5.66 GB; the backup filesystem had about 28.29 GB free. The actual repair still requires the independent current relay, the dedicated connection, a verified database backup, and a fresh acceptance path and cursor check. Public health remains HTTP 503. At 00:19 UTC B0 reported a generated `REAL` PALW block at DAA 19,511 (BLUE in its DAG, outside the selected chain) and zero receipt blocks. This is evidence of block production, not recovery of model execution acceptance or Explorer updates.

## Follow-up checks — 2026-11-03 00:40 UTC

Thirteen journal samples from the killed public95 process show 9.58–9.88 GiB of anonymous RSS and about 0.08 GiB of file-backed RSS in the last 13 minutes. This shows sustained anonymous pressure ahead of the cgroup kill; the journal still does not identify the allocating subsystem. B0's direct `getPalwKernelFinals` response at DAA 19,519 reported zero Finals. The V2 execution observation was unavailable, consistent with `palw_exec_payload_v2: None` in the T12 preset. That setting and the separate 20,000 DAA retention activation have not changed.

Separate from the `ad9016` Linux build, `b646e3370` extends allocation-free protobuf preflight to duplicate singular and same-variant oneof fragments. `7b98a0c70` reserves EVM and overlay reply serialization buffers before allocation and transfers the reservation through the existing output owner. The combined lightweight harness passed 39 tests, including admission before either reply buffer and cleanup under cancellation; three production flow tests passed. Separate real plain/gzip frame tests at 256 MiB and near 320 MiB passed on the updated schema quote. These tests leave live inbound admission and source snapshot materialization as separate work.

## Trusted B0 relay and dedicated Explorer connection — 01:20 UTC

The isolated `ad9016` Linux build completed normally and produced executable SHA256
`227360a9bca06ee999e674174949b7a222092034a428117499019caee6361b7b`.
Original IBM B0 exited normally after one SIGINT. A verified independent 9.5 GB
native consensus copy was created before the original was restarted. B0 resumed
with the same executable, APP and actor configuration, and returned to synced
status after a single ephemeral reconnection to its previous 113 peer. The
copy and resumed-original receipt SHAs are `800ed0c271ed16a9a138b48e0377fceb18df1d74997c895292b7500a56c1e1aa`
and `f87a336c999946e63dcb33ef623f331d218d7a9e03084513135a63bbcce8898e`.

A separate unsigned process opened the copy under an 8 GiB cgroup memory
limit. Its only P2P connection was the resumed B0. Three live comparisons and
a final unsigned PALW template matched B0 at DAA 19,528. The current canary
SHA is `e1328dd4e08ecdc13ed0bfd1a6a8c8977d5f700e0f85f553fbed1fed0c774320`.
The relay then accepted newer blocks, with about 1.1 GB cgroup usage and no
swap at 01:20 UTC. The observed canary had zero OOM events. This is a trusted
donor replica, with no model signing or submissions. Fresh IBD and convergence
of both original branches are still separate checks.

The dedicated 113 tunnel confirmed the same synced and indexed relay through
both gRPC and JSON at DAA 19,529. Its receipt SHA is
`55195a105da29a431279eb890d074354132f091a05fd6aec30916074674babff`.
The old shared tunnel and other site were unchanged. The two existing
Explorer writer owners were identified and the no-force stop policy was
prepared while they were still running. Two read-only reorg plans stopped
when the relay advanced during per-candidate checks; their receipts record
zero SQL writes. The guarded backup, repair and public cutover had not
occurred at this cutoff.

## Additional source corrections and test coverage

`91c23b943` connects the global material janitor to the checked resident
retention reader under the existing R-core fence. A failed read now protects
material during reclamation planning and a fresh read immediately before
deletion. It does not change the DAA 20,000 acceptance rule. Six actual
`kaspad` library retention tests passed; the test log SHA256 is
`d024e370cfa278550c014a09ab6b2199342fa72ea3596e0b4f08d7d8205fab12`.

`46162a0e4` removes tonic's speculative reservation of a full receive message
after only a gRPC length header. The existing receive cap still applies.
Wire scratch grows only with arriving bytes, and an already owned coalesced
suffix is kept separately when appending it would copy a full-size frame.
The pinned tonic suite passed 104 tests and the isolated fragmented 320 MiB
test passed. A header-only declaration retained at most 8 KiB of wire
scratch. The large fragmented case can transiently have old and new scratch
owners during growth; this change does not impose a shared limit on all
inbound decoding or decompression. These two source corrections are newer
than the sealed relay build.

## Public Explorer recovery and remaining chain state — 01:35 UTC

The two original Explorer services stopped normally and no other canonical
writer was observed. PostgreSQL produced a full consistent 1,693,997,177-byte
custom archive with a verified restore inventory. Backup receipt SHA256:
`6285c05ef53a3960ba38b297d9df565cfc0f1d573660a3c9029edb5a2172329d`.

The planning read path now checks at most eight candidates concurrently
while consuming them in selected-parent order and requiring the same live
frontier. A live plan refusal due to a moving frontier left the cursor
untouched. A second apply reached the exclusive intent but stopped before
the write phase: the actual SQLAlchemy 1.4.54 default connection has no
explicit `commit` or `rollback` methods. The writer-only future-mode option
passed tests in the installed version with an isolated database and the
same-connection ORM event and rollback behavior.

A fresh plan and the guarded SQL repair succeeded at DAA 19,534. An
independent PostgreSQL read verified 578 added selected-prefix blocks and
628 accepted transaction references. All 329 old branch rows remained in
history, now unselected, with zero remaining transaction acceptance
references to that branch. The verification receipt SHA256 is
`de8f6663f55024dbfa9f2b8ef5b7f49ea9a6c3f1f5954d94a4fc459fa8c5c0b9`.
The revised filler resumed that exact cursor before REST was started.

Both scoped public WebSocket routes reach the same qualified relay. Public
REST returned HTTP 200 with its selected cursor current, then indexed the
next natural selected block: DAA 19,534 to 19,535. The rendered site showed
`node connected` and the new DAA 19,535 row. The advancing public observation
SHA256 is `d8d85e68c564d63b1c996aa0d5603f17c6eac828ce9c59d80bf9ba10ccb6b26d`.
The temporary no-force stop configuration was retired without restarting
the two new Explorer generations. The dedicated 113 tunnel is enabled at
boot; the IBM relay remains a supervised incident instance bound to the
current original B0 and B1 generations. The old shared tunnel generation
and other site configuration retained their exact hashes. At the last
relay check the cgroup had used at most 2,316,210,176 bytes under the 8 GiB
limit, with no swap use or OOM events.

Three REAL Qwen2.5 blocks were observed on B0 and the relay after the
original B0 resumed. Public RPC independently returned the third, DAA
19,531, as BLUE and off the selected chain. Its acceptance fold had
20,606 DAA left against the 20,608 DAA required by the Court. Claim and
Court records were absent for the generated attempts, and there were zero
producer receipt blocks and zero kernel Finals. The future acceptance-time
retention rule was still below its DAA 20,000 fence.

B1 remained on its old DAA 19,389 branch. Fresh one-head economic
observations again matched on safe frontier blue score 60,588, safe weight
5,698,420,463,928,576 and bounded immature weight zero. These responses
are diagnostics from the two nodes, rather than independent chain
validation. The incumbent-preserving rule for a deep tie is unchanged.
The separate primary113 incremental IBD with seat7 had no observed
completion; its full identity differs only by the unfired retention fence,
so the recovery summary refusal does not establish that ordinary
pre-fence sync must reject this connection. No fork-choice override or
model certification waiver was applied.
