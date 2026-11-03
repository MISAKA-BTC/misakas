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

## Recovery beyond the Explorer index — 02:02 UTC

The prior public relay had a 32 MiB log stop condition. Its observed log
growth at 01:45–01:47 UTC was about 1.6 kB/s, principally from gRPC connection
lifecycle messages. The replacement supervisor rotates owned log files and
monitors the serving B0 generation. B1 does not supply this relay's data.
Removing that runtime dependency allows future B1 maintenance without
stopping the Explorer relay. The package passed 22 local tests.

The old relay stopped after a single SIGINT and returned exit 0. The
closed APP inventory recorded 1,284 entries, no process owners and all four
private ports released. The closure receipt SHA256 is
`fafb130f936ecb4349f4034ca6049c975007e1bc32430edfb7f5ea759b58b308`.
The successor reopened the same APP and bound the same private ports under
the same 8 GiB limit. Its canary matched B0 at DAA 19,542, with SHA256
`09bf87f2dfef73ad1e9d8997b6697b11661fa087e09f67716f8eeb498ff92586`.
The filler restarted once during the planned disconnect. A brief 503
cleared as the new RPC connection became ready. Both public WebSocket
routes and HTTP 200 were verified, and both SQL cursors advanced to the
new virtual DAA 19,543 on the successor UUID. Independent verification
SHA256 is `4fe6579be4f9e6a37deb9fb2e644309a86e241c62f0d948ac99200a1b8ed339a`.

The old 113 primary rejected the actual level-0 header `6d70329a…f31c`.
That header and its sole parent `3ca853a7…443a1` are zero-chain-weight lane
headers at DAA 316 with blue score 1,242 and identical blue work. The
sealed `ad9016` candidate handles this case and still checks genesis-rooted
GHOSTDAG work and score. A separate unsigned process on host 5 started
from an empty APP and connected to B0 through an isolated, loopback
forward. It passed the 19,003-header level-0 proof and set the staging
pruning point. At 02:02 UTC it was still importing ordinary headers,
with 107,145 processed, at 59% by the node's progress display. Full
IBD and PALW snapshot import had not completed. The 113 primary was
stopped by a separate operation while this work ran. Its signal-9 exit
is not clean closure evidence.

On the B0 branch the only currently ready seat matched the producer's
bond, which the G14 draw excludes for its own claim. The old seat5 bond
is visible on the B0 branch and independent of the producer. It has 38
live capital locks and two nonterminal claims. Its old APP is closed and
its signed state and evidence remain preserved. Any signer handoff
requires proof of evidence access, sufficient storage, and a compatible
current chain. The fork comparison continued to report the same economic
keys on B0 and B1 below the DAA 20,000 acceptance fence.

## Fresh IBD outcome and historic Court material — 02:42 UTC

The separate unsigned trial on host 5 processed all 230,284 ordinary
headers after validating the 19,003-header level-0 proof. B0 refused
the requested chunked PALW snapshot for the unchanged pruning point
`4dc9cebd…1ab9e`. The trial remained unqualified. It stopped normally
after its own time limit, with one SIGINT and exit 0. The previously
reported proof validation defect and this serving failure therefore
have different observable boundaries. The public relay's safe-mode
RPC rejected an attempted administrative peer attachment for a
first-chunk diagnostic, so no diagnostic peer was attached to it.

Both missing historic seat5 BASE0 captures were regenerated offline.
The replay checked each exact accepted-block input, claim ID, registered
executor key, public signature, and all signed execution, output, trace
and manifest roots and the chunk count. Each material file is 1,599,245
bytes. Their SHA256 values are
`3bd529c12cac7b81e8fa234aca590c000bdaaa4f17937ef1816ca207fd774537`
and `14e38416832987b0999f719626ee4771a57541d7c498448f99f15e093b4deb36`.
The one focused signed fixture test passed, including changed input and
root rejection. The captures are locally staged and do not yet establish
provider delivery or any Gate C decision.

At 02:40 UTC public REST was HTTP 200 and the public WebSocket reported
the successor UUID at DAA 19,557. The 113 primary remained inactive while
its separate build proceeded. B1's original APP and service had not been
changed. The newer Linux build was still running under its isolated
8 GiB container limit.

## Snapshot transfer and release gates — 02:59 UTC

Two short-lived diagnostics negotiated protocol 108 with the B0 source
and requested the same pruning point, `4dc9cebd…1ab9e`. The uncompressed
request received the first 4,194,304-byte chunk of a 47,050,528-byte,
12-chunk stream. The compressed request received a chunked `found: false`
response. Both requests reached the application handler. The older
serving build returned this response for multiple failures, so the
result localizes the refusal to the compressed path without identifying
the internal error. Neither diagnostic installed state.

Both regenerated Court captures were copied to a separate private
directory on the original seat host. The destination files matched the
sizes and digests above. The old APP and signer remained closed.

The isolated Linux amd64 build of `707a95b88` exited successfully under
its 8 GiB limit with no OOM. The ELF SHA256 is
`bc2a2d521a6f434225b50dbf15f7f93a0a488a960644d09d7f39d3b68f9bf404`.
The binary was verified locally and has not been deployed.

Public REST returned HTTP 200 and the WebSocket reported a synced relay
at DAA 19,562. At 19,563, B0 had produced 18 REAL attempts since restart,
with no accepted executor claim, receipt block, or kernel Final in the
checked state. The ready bond on B0 matched the producer and was excluded
from its own draw. The 8K model used by B0 was also distinct from the
512-token model selected by the existing Qwen dialogue worker.

Publication to `main` is gated on T12 convergence, a fresh live G14
Final with the applicable receipt and Gate C evidence, sustained block
generation and public indexing, and a real dialogue through the normal
gateway. A local publication candidate excludes the private operational
history. It passed all six fast CI gates. Full CI and the live release
gates remain open.

## Public follow-through and isolated dialogue — 03:03 UTC

Two public samples reported DAA 19,563 and 19,564 on the same synced
relay, with HTTP 200 and the current index cursor. At 19,563, B0 and
the relay reported the same selected sink. B1 remained unsynced at
19,389. The 113 primary answered RPC at 19,224, still unsynced, while
the separate Explorer tunnel reported the relay's current sink.

The relay retained 1,314 three-second samples from its current
invocation. It had zero restarts, zero swap, zero OOM events, and
zero memory-pressure stall time. The cgroup peak was 2,719,289,344
bytes under its 8 GiB limit. Both log rings were below their first
rotation threshold. The pressure receipt SHA256 is
`d215b025294c614ee7b0574edb394e15bea3fe7c3dd62c5dab4a9167321298b0`.

The exported Linux ELF also passed a network-disabled amd64 loader
check. Its in-container digest matched the build, all shared libraries
resolved, and the version command exited 0. This was a loader check.

The existing Studio gateway handled two loopback HTTP requests with
the exact T12 8K artifact and its committed tokenizer. It answered
`4` and then `7` when the second request included the previous turn.
Both requests returned HTTP 200. The isolated process had no signer or
node RPC, reported `committed: false`, and wrote no commitment files.
The smoke receipt SHA256 is
`24e4199ca1ba96c7e2ef50f4cdb81dee442977ed9e0b633206503a7cb566290b`.
Live framed G14 dialogue admission and settlement remain unverified.

## G14 validation gates — 03:43 UTC

| Gate | Current evidence | Required next observation |
| --- | --- | --- |
| G0, deployment identity | Executable digests, process generations, T12 genesis, parameter ID, schedule ID, and the live 20,000 retention fence were checked on B0, B1 and the relay. The repaired worker also passed two keyless 8K dialogue turns. | Bind each participating miner, verifier, provider and model-free node to the exact release and limits. Record the actual workload and cold or warm conditions. |
| G1, claim admission | The first 21 B0 submissions each matched a claim ID decoded from their carrier on B0 and the relay. Every carrier matched a typed retention refusal. Both claim-detail RPCs returned `found: false` for each ID. Remaining retention was 20,605 DAA for 3, 20,606 for 14, and 20,607 for 4. All required 20,608. The general checked GC fix is absent from the running binaries. | Observe one new normal claim with its signed deadline, accepting block and DAA, stored effective deadline, and matching provider and GC behavior. |
| G2, independent panel | On the current branch the producer's own bond was the sole ready Qwen seat. No independent assignment or receipt was observed. | Bind a qualified independent seat selected by the actual draw, verify its reservation and full-scope receipt. |
| G3, independent normal verification | The ordinary ModelOne seat replays locally and signs a receipt. The native licence checks the assigned receipt, scope and backing. A separate Panel-generated normal evidence path to an independent Node arithmetic check has yet to be demonstrated for Qwen 8K. | Trace normal inference, evidence construction and bounded data responses to a Node decision for the same real Qwen claim. Verify the with-data, requested-data and withheld-data cases. |
| G4, adversarial cases | Local ModelOne tests cover an independent seat, scoped signatures and false-valid liability. These are narrower than the requested role and arithmetic matrix. | Run the complete honest and adversarial matrix on an isolated network, including authenticated incorrect arithmetic and accountability for unavailable data. |
| G5, credit and settlement | Gate C and Final are separate in the current path. No new Qwen receipt, Final, certification or payout was observed in the audited cohort. | Trace all credit and accounting paths and verify unpaid, valid, invalid and unavailable outcomes independently. |
| G6, concurrent miners | Nine local collector-queue tests passed. The real cohort did not reach the Panel. | First drain staged cohorts, then measure controlled open arrival rates with independent miners and fixed Panel resources, followed by Panel and Node scaling. |
| G7, operational qualification | The public relay advances without observed OOM. B1 remains on another branch and fresh unsigned IBD did not complete. | Repeat restart, reorg, IBD, pruning, overload and sustained retention tests on the same candidate under measured limits. |

The exact 21-claim receipt SHA256 is
`6120d2d9284a170a7379802bcb8724eb1ba794f1ea0e3da114dd1e6bdb4c5175`.
The rebuilt worker answered `4` and `7` using the committed 8K artifact
in the same keyless loopback test. Its receipt SHA256 is
`a5da97d888794eb79efefd0e42449e2ecdf63ed91d2e3f63c746b75cb63d9bad`.
The existing G14 rollout, independent arithmetic adjudication and mainnet
load qualification are tracked as separate outcomes.

The count-based foreign cache sweep now repeats the accepted-chain
retention read immediately before unlink. An unavailable or changed read
preserves the file. A healthy reclaimable file can still be collected.
The regression against the previous code failed on a changed obligation;
the updated function passed the three scoped retention tests. The fix is
in the integration branch and has yet to run on a live verifier.

The existing Positive Court A0 integration already carries the separate
Panel and model-free Node checker. Its isolated three-process Pythia
short job completed 213,298 relations with model access denied to the
Node. Its native dispatch and the exact T12 Qwen A16 path remain open.
That implementation is the reference for the remaining G3–G7 work.

The Qwen cohort's attempt 21 provides an exact workload for the A16
comparison. It drew at DAA 19,566 with 1,023 input tokens and one output
token in the 8,192-position profile. Both the producer and the relay
returned a header whose block hash, claim ID and execution anchor were
recomputed independently. The anchor agreed with the saved answer and
capture. This fixes the workload input without establishing admission or
an independent Node verdict.

For provider delivery, the integration branch now supports bounded
numeric HTTP reads alongside the configured directory sources. A
single-flight fetch runs outside the Panel duty loop, with a three-second
deadline and 16 MiB material cap. Admission checks the current claim roots
and effective retention before giving material to the verifier. Three
scoped transport and admission tests passed. Full Node build and a live
accepted claim are still required for deployment qualification.

The producer now syncs its capture before publication and passes the
original buffer through retention and announcement. The focused file
test exercised atomic visibility, recovery after a failed publish, and
cleanup of the failed temporary file. The Unix file mode matched an
ordinary daemon write under the same umask. A process or host crash
during the full accepted claim lifecycle remains a separate G7 test.

The engine-only A16 prefill regression ran the exact updated source on
a temporary RAM disk. The 360 combinations agreed on logits and KV
state between the ordinary and capture paths. The test used local model
fixtures and cached dependencies. Rebuilding the full worker and running
the actual T12 8K comparison are separate checks.

The diagnostic `palw-tir-equiv` now selects the exact Testnet 12 CoreV1
draw from its anchor and block DAA. The four focused tests passed on the
exact diagnostic sources, including the T12 selector, the one-token draw,
the prior comparison mode and a changed-weight case. The real 8K
artifact has not yet been executed in this comparison. A model-free
Node verdict and native accounting still require the separate Positive
Court route.

## Original Qwen material and provider replay — 04:27 UTC

The producer and relay returned byte-identical public headers for Qwen
attempt 32 at DAA 19,585. The recomputed block, claim and anchor matched
the retained 2,409,135-byte capture. The native ML-DSA signature checked,
and the signed key matched the same bond's public registration on both
nodes. The verifier recovered the exact canonical 1,023-input, one-output
job and checked its prompt, output, trace, step and checkpoint commitments.
Changed capture and claim identities were refused. The capture SHA256 is
`9eeab9f8cbe88706a8308faa2740f576243102ddc835720d07a923e2855c7f5c`.

The actual merge logged `COURT_RETENTION_BELOW_GUARANTEE` for this
attempt, with 20,606 DAA remaining and 20,608 DAA required. At DAA
19,589 B0 and the public relay were synced and reported zero Panel
assignments and valid receipts. B1 remained unsynced.

An isolated filesystem and loopback HTTP test re-served the exact
capture. A provider manifest promising the raw signed deadline of
40,193 was rejected against a test-only effective deadline of 40,195.
A provider manifest covering 40,195 returned byte-identical material.
Wrong roots and a corrupted chunk were rejected. The original signed
envelope and capture were retained byte for byte. The provider test
does not establish real chain acceptance or an arithmetic verdict.

## Bounded provider reads and live acceptance probe — 04:33 UTC

The directory client now opens regular evidence files and checks their
size before allocation. Manifest-declared chunk lengths bound reads, and
ordinary Panel fallback uses the existing 16 MiB material limit. The
reference provider applies its configured object limits when serving
existing files and validates the backing files before accepting a
manifest. HEAD checks the opened file without reading its body. These
changes are in `37b87340f` and `d3064de61`.

Seventeen scoped tests on the updated provider modules passed. They
covered byte-identical delivery of the real Qwen #32 capture, recovery
using another copy after corruption, and refusal of oversized existing
files before allocation. The test log SHA256 is
`7a219eb02ee9aa6c4291cfacd93ea6b2b390c17cd51827ba07fe1c23f20aa011`. The tests measure per-object bounds. Aggregate
server admission and live deployment are separate.

A bounded read-only probe brackets the selected frontier on B0 and the
relay and checks a single claim's signed expiry, accepted block,
effective expiry, assigned seats, valid receipts, Final and Gate C.
Thirteen local boundary and negative cases passed. In the live sample at
DAA 19,590 the Qwen #32 carrier was visible on both nodes. The claim and
assignment were absent. A separate limited query found 15 older voided
claims across ten class bonds. Five bond responses were truncated, so
that query is not a complete claim inventory.

At the later public sample, REST returned 200 with the selected-chain
cursor current. WebSocket matched the configured synced relay at DAA
19,592. The sampled eight selected blocks were legacy heartbeats and
the relay's receipt-block counter was zero. Panel throughput cannot be
measured from these samples.

At 04:33 UTC B0, the relay, and the Explorer tunnel on 113 returned
the same selected tip at DAA 19,592. B1 reported unsynced at 19,389
and the separate 113 primary reported unsynced at 19,224. The producer
reported 34 draws and 34 produced blocks in its process. The journal
matched attempt #34 to another refusal with 20,606 DAA remaining
against 20,608 required. The class view showed zero assignments and
zero valid receipt seats. Scoped Node test metadata with default and
EVM features compiled on the provider changes. Full linkage and live
qualification remain pending.

The reference provider now admits up to four simultaneous request
handlers. Excess connections receive a best-effort 503 without waiting
in a user-space queue. Socket reads and writes share a 20-second
deadline measured from acceptance. An already blocked filesystem
operation may hold a slot beyond that deadline. Nineteen scoped tests
passed, including real Qwen material, slow sender and reader cases, and
normal service after the slots were released. Log SHA256 is
`6c1872ff3746098ac4e00a0cb1bbac081fb2f19418970c2417c8e322f9212476`. The listener change has yet to run in production.

## Exact Qwen comparison setup correction — 04:37 UTC

The first attempt #21 offline comparison verified the registered 8K
artifact and exited before running the mirror. It passed the native
held history cap of 2^21 positions into the diagnostic TIR, where the
key history exceeded the existing per-tensor element limit. Peak RSS
was 1,845,297,152 bytes before the refusal. The failure did not compare
the native and mirror outputs.

Commit `ec171af98` bounds the diagnostic history to the authenticated
8,192-position class context. At every allowed position, both sides
still see the complete prefix. The existing selector, normal-form
limit, native class identity and native execution rules remain in use.
Four scoped tests passed, including reproduction of the old structural
refusal and validation of both 8K history windows. The rebuilt
diagnostic executable SHA256 is
`ac7bffa3aae8d830db7aa0534f3dc1d500ee399802ab3f97e8bd3a22b1fb4d6f`.
The exact real comparison is queued behind the active local compiler.

For Qwen #34, both B0 and the relay returned the original BLUE carrier
and the same recomputed claim ID at a stable selected tip. No claim
row or Panel assignment was present. The carrier's block DAA was
19,588, and the observed virtual DAA was 19,592. Public REST and
WebSocket still agreed on the ready relay.

## Bounded evidence publication and merge observations — 04:40 UTC

The evidence CLI now validates its staged manifest and checks every
source chunk before publication. Upload and read-back retain one chunk
at a time. A changed or corrupt source chunk prevents that destination's
manifest from being published. The previous command accumulated every
source chunk together even for a valid manifest up to 4 GiB.
Commit `481c1ec6e` passed 20 scoped tests, including loopback
HTTP publication of the byte-identical Qwen #32 capture and a damaged
last chunk that prevented all destination writes. The scoped harness
also typechecked the CLI against those same provider modules. Log
SHA256 is `78ebb93db32c759e2c9162d8a14259ca52ce2f4d272a1d7001fae81c70c806b2`.

The live producer reported 36 outputs. Its journal linked #35 and #36
to refusal at 20,606 and 20,607 remaining DAA respectively, each
against 20,608 required. Public REST, WebSocket and the B0 read agreed
on the selected tip at DAA 19,594. The class view reported zero
assignments and valid receipt seats.

The local producer GC still needs an authoritative end to retention
before allowing deletion of a publication that is absent from the
current claim view. A scoped in-memory DAG fixture ran the actual
GHOSTDAG mergeset function and verified four cases, including that a
carrier already in the selected parent's DAG past cannot be merged
again. That result does not cover carriers outside the pruning
point's past or prove storage liveness when pruning stalls. The
streaming CLI consumes an already staged manifest. Native accepted
Attempt publication and end-to-end G1 remain open.

## Provider persistence and branch recheck — 04:43 UTC

Commit `9c28a1b14` re-reads the current accepted roots and retention
deadline after a directory fetch, before ordinary material injection
or TIR original staging. The same post-fetch check covers the TIR
HTTP path. Commit `e7eb1140d` gives ordinary provider chunks and
manifests separate temporary files per writer and syncs the written
data and containing directory before success. Chunk publication still
precedes manifest publication.

Twenty-one scoped tests passed, including real Qwen #32 loopback
publication, four concurrent writers, and a write failure followed by
successful retry. Node default/EVM test metadata compiled against the
combined source. The evidence covers normal writes and injected
rename failure. Power-loss recovery, full linkage and deployment
remain separate.

The read-only observer now emits provider roots only when two synced
nodes agree on the selected tip, accepted claim, effective deadline,
registered key and output root. Its extracted #32 header fields
matched the independent signed-material check. Fifteen fixture
checks used actual #32 signed roots with synthetic acceptance state.
The live probe for #36 returned no exportable roots at DAA 19,595.
The latest B0 and relay views agreed at that DAA, with zero active
Panel assignments and zero valid receipts. The observer produces
publication inputs. It does not write a provider manifest or change
retention.

## Keyless Qwen material staging rehearsal — 04:46 UTC

A private operator-stage harness combines the actual T12 header and
signed material checks with the read-only paired acceptance receipt.
It verifies the original public executor key and the exact Qwen
capture, derives the provider manifest from the effective expiry,
writes a fresh local provider directory with the current publication
code, and reads all bytes back. The harness performs no network
writes and uses no signing secret.

The real #32 bytes survived staging and read-back byte for byte in
three chunks. The fixture supplied a synthetic acceptance at DAA
20,000. Its provider deadline was 40,608 while the signed envelope
still said 40,193. The real unaccepted #36 receipt, a manifest deadline
shorter than the accepted requirement, and a changed trace root each
failed before staging. Harness receipt SHA256 is `f229b2c67a3e48317e784ee95248e738aed4c36320799d6ea3134e0a6293323a`.
The fixture validates staging and refusal. The first live post-fence
acceptance, ordinary producer export, and automatic provider
retention across reorg still require verification.
