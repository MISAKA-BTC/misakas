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

## Stale provider deadline protection — 04:49 UTC

Commit `a4acffaab` serializes ordinary manifest updates in the provider
directory. A retry with the same manifest fields and chunk layout
cannot replace a longer published deadline with an earlier one.
A different content layout can still be published to repair an
incorrect manifest. The guard applies to directory publication and
the reference HTTP provider. A macOS directory-lock check confirmed
exclusion between separately opened handles and release on close.
The full loopback regression, including concurrent stale publication
and provider restart, is queued behind local memory pressure.

The original Qwen #37 carrier was visible on B0 and the relay. The
producer's journal matched it to a refusal at 20,606 remaining DAA
against 20,608 required. At the paired read's DAA 19,597 the claim
row, assignment and exportable provider roots were absent. The
public Explorer REST and WebSocket were available at DAA 19,597.

## Independent seat availability and relay memory — 04:50 UTC

A bounded read on B0 and the Explorer relay compared ten Qwen-class
bond rows at the same selected tip at DAA 19,597. Nine rows were
eligible and independent of the producer by bond, registered key and
operator identity. The only ready row was the producer's own. Its
readiness was recorded at DAA 19,588 and expired at 19,612. No seat
was assigned. The class inventory does not determine the later G14
draw, which uses the eligible global bond state for an accepted claim.

The same public relay invocation had zero restarts and zero cgroup
OOM or OOM-kill events at 04:50 UTC. Current memory was 1,671,737,344
bytes and the peak stayed 2,719,289,344 bytes under an 8 GiB limit.
B0's producer invocation also showed zero restarts and zero OOM or
OOM-kill events, with 7,289,430,016 bytes current and 9,459,302,400
bytes peak under its 20 GiB limit. These are continuity samples under
the currently observed traffic. They do not establish Panel capacity.

## Retention replay qualification and seat5 IBD storage — 04:53 UTC

The completed scoped suite ran 22 tests on `a4acffaab`, including
concurrent stale FS and HTTP publication, provider restart, and
loopback re-fetch of the original Qwen #32 capture under a test
effective deadline. Compiler output was on a guarded RAM volume and
sparse-file fixtures were on APFS. Peak allocated scratch was
14,876,672 bytes with zero measured swap growth. Test log SHA256 is
`9c679dfa00eb425e4ce19f6c52ea08cf1605b9f1144db9702a14b4343539d475`. Provider changes have yet to run on T12.

The stopped seat5 unsigned IBD invocation exited normally on SIGINT.
Its remaining APP occupied 376,832 bytes when rechecked. The old
trial's uncommitted staging was not present to resume, consistent
with the existing IBD cancellation and startup cleanup paths. No
independent archive or spare mounted data volume was found for this
trial. A fresh unsigned trial still needs an available compatible
pruning snapshot source and the existing 21 GiB launch floor. The
observed seat5 free space was about 0.85 GiB short.

For the closed-donor alternative, the earlier IBM copy had 1,269
files and 9,517,373,049 bytes, with disjoint inode pairs. Its saved
estimate plus reserve required more than 22,678,691,840 bytes of
free space at launch. The new IBM measurement was about 6.67 GB
short of that earlier figure. This leaves the live Explorer relay in
service and the old signer data preserved.

## Retained evidence reclaim accounting — 04:54 UTC

The janitor previously credited the scanned size of a whole capture
group even if some paths were missing or unlink failed. Commit
`a09effabc` credits only regular files removed by that pass and
counts a group only when at least one file was removed. The previous
routine reported 400 bytes against 14 bytes actually removed in the
new regression. The updated routine passed eight scoped tests,
including a repeated prune that credited zero bytes on the second
pass. The harness used the exact production prune function and
stubbed the consensus session. Log SHA256 is `cba98399dfe60473c5a4ec1bf8b6c3768f4aa50f3c93806439d02b48de0ec134`.
A full Node build and live deployment remain separate.

The paired B0 and relay reads at DAA 19,597 still showed zero Panel
assignments and receipts. The public REST and WebSocket matched the
configured ready relay. The arithmetic comparison queue remained
live below its local 12 GiB start floor.

## Capture protection during lead submission — 04:56 UTC

Commit `4de7e61a1` registers the lead claim before persisting its
capture and keeps that registration through local submission and
announcement. The retention janitor checks registration immediately
before unlink, in the same critical section as deletion. Each
publication guard releases its own registration on exit. A pressure
prune with a plan made before publication deleted the test capture
under the previous function. The updated scoped harness passed nine
tests and kept the material until both guards were released.
Log SHA256 is `5b1326f7cb8172b82244b0dcec79b1bd6cf047ad72e9ec6aee70c5e123f2b6e5`. Node default/EVM test
metadata compiled on the updated source. Live deployment and
the durable post-submit and rider retention lifecycles remain open.

B0 and its relay matched at DAA 19,598, as did the separate RPC
tunnel on 113 at the same sampling point. B1 remained unsynced at
19,389 and the 113 primary remained unsynced at 19,224. The later
public REST/WebSocket sample was available at DAA 19,598 with
zero relay receipt blocks.

The independent code review found that an active capture could still
satisfy the planner's estimated reclamation and prevent other files
from being selected. Commit `bce5d71b2` excludes active leads from
planning and keeps the final locked check. The previous actual
janitor pass returned a shortfall while an unpinned file was
available. The updated scoped harness passed eleven tests and
reclaimed that file while preserving the active lead. Log SHA256 is
`761b45d14ef2488dab59d08de84cde5ff284c2a8073ff5eca860e8c84f89aba7`. The full test-metadata
build above covers the earlier guard commit. The planning follow-up
was verified in the scoped harness.

## Rider capture ownership and Qwen carrier #39 — 05:00 UTC

Commit `5ea7b81b9` pins a rider's capture before persisting it
and moves the guard for that exact attempt into the existing
rider batch. The same owner covers producer handoff, Court queue
admission, and an in-process carrier retry. The guard occupies the
prior attempt-ID slot in the batch. Five scoped rider outbox tests
passed against the exact guard and outbox source, including wrong
attempt ID, retry after queue refusal, and release when the batch
is dropped. Log SHA256 is
`d377d3d9dc53730b906c2ffe43ecd7cfb765330dc5197c9023ef291152c7c2c5`.
Node default/EVM test metadata compiled. The extended Court flight
regression was type-checked. This commit still needs a Linux
executable and live qualification. Its scope ends at local queue
retirement and does not cover post-submit admission or restart.

B0, the relay, the RPC tunnel on 113, and public Explorer agreed on
the selected tip at DAA 19,599. Explorer REST returned 200 with a
current index. The public WebSocket also returned the original
Qwen #39 carrier as a real BLUE block outside the selected chain.
B0 logged a retention refusal for the same block at 20,607 remaining
DAA against 20,608 required. Its exact claim, Court, and lifecycle
rows were absent on B0, and B0's receipt and kernel Final counters
were zero. B1 and the primary on 113 remained unsynced. The live
Panel load measurement and promotion criteria remain open.

The independent review found that a stalled Court sender could hold
an unlimited number of those batches if native accounting were
disabled. Commit `afef37a9e` caps resident batches at eight per
process. The producer acquires the slot before inference. A refused
import or retry retains the slot until the original batch retires.
The existing per-batch limit is 64 riders. This bounds the number
of in-process pinned rider captures and leaves retained-file byte
accounting to the existing storage budget when configured. Six
scoped tests passed, including the default-accounting limit and
admission after retirement. Log SHA256 is
`80a1fa7b73c050da5253c2e34e689f09e69dedf55148cdf4c48d88603cb290fe`.
Node default/EVM test metadata also compiled with the resident cap.
At 05:01 UTC the unsigned seat5 trial was still stopped. Its volume
had 21,634,891,776 bytes free, below the 21 GiB launch floor.

## Retention planning cost and Qwen carrier #40 — 05:04 UTC

Commit `2b5047c22` builds reclaim candidates in one pass and sorts
eligible rows only when pressure remains after age-based pruning.
The scoped regression measured 11 chain reads and 9 obligation reads
for five files before the change, and five of each after it. Existing
selection order and the independent unlink guard were preserved.
Twelve tests passed. Log SHA256 is
`441fdc7990933934925fc050c32de387376a92dfecff6bc0e77c62add92e602c`.

Commit `6e1285b78` uses the resident material-obligation read for own
attempts and foreign copies, removing the separate full derived
artifact read for both. Free-prompt age pruning still uses the claim
phase. A scoped test of the actual janitor pass with a stub consensus
session reduced two derived artifact reads to zero for two
captures, while keeping four resident obligation reads across
planning and unlink. Thirteen tests passed. Log SHA256 is
`18cf9569123a98b8d9e2f16198c17fabde68ab5a667ca0b702f99f7c203d9aa6`.
The independent review matched the DA gate and pressure ordering.
Post-submit capture ownership across an unresolved merge or restart
still needs qualification.

The B0 producer logged Qwen carrier #40, and B0 and the relay returned
the same exact attempt ID. The public WebSocket served the original
REAL carrier. The journal recorded 20,606 remaining DAA against
20,608 required for the same block, and B0 returned no exact claim.
Its receipt and kernel Final counters were zero. At DAA 19,601, the
public REST index and WebSocket agreed with the B0 and relay tips.
No additional load cohort was started.
The matched B0 and relay class inventory at the same tip showed nine
eligible independent Qwen bonds and zero ready independent bonds.
The producer's own row was the only ready one. No seats were assigned.
This inventory precedes an accepted claim and a claim-specific draw.
Node default/EVM test metadata compiled on `6e1285b78`.

## Retention scan versus producer replacement — 05:06 UTC

Commit `ab609f520` snapshots retained files' identities when planning
GC. Under the publication lock, pruning defers the entire capture
when an inventoried file was replaced. A file written after the scan
clock has age zero. Two regressions failed on the previous code:
it deleted a same-sized replacement and its companion answer after
the writer released its pin, and it treated a later mtime as missing.
Fifteen scoped tests passed on the fix. Log SHA256 is
`6b8bf265ad369d48966a6d8631173d355a9311c42552c1a49adade09b9407520`.
Node default/EVM test metadata compiled. The independent review
confirmed the lock covers cooperating producer writes. Panel writers
that bypass that lock, the separate Panel sweep and abandoned
partials still require verification.

At DAA 19,602, the exact selected tip matched on B0, the relay, the
113 RPC tunnel, and public REST and WebSocket. B1 and the 113
primary remained unsynced at 19,389 and 19,224 respectively. B0
still reported 40 real producer blocks and zero receipts or kernel
Finals. The local fixes have yet to run on T12.

The queued exact Qwen #21 arithmetic comparison exited with status
124 after its 30-minute launch wait. Its last sample showed
2,029,334,528 free bytes versus the 12 GiB launch floor, with another
compiler still active. The corrected executable did not run and no
arithmetic result was produced. The existing Linux ELF still
matches the recorded digest for `707a95b88`, which predates the
current fixes. The stopped unsigned seat5 trial had 21,206,155,264
bytes free at 05:07 UTC, below its separate 21 GiB launch floor.

## Panel file publication and abandoned stages — 05:10 UTC

Commit `bd5ea1179` registers the Panel's normal retained material,
answer and provider-copy writers with the in-process publication lock.
The Panel's age and count sweeps compare file identity before unlink.
The count sweep credits successful unlinks only. The old count sweep
deleted a same-sized replacement during a reproducible callback race.
Three scoped tests passed on the new source, including replacement,
active publication and release. Log SHA256 is
`998005653a9a1baa372babe670c9e8cc83c4a7099a046e02288c308fc87999b9`.
Node default/EVM test metadata compiled. An independent review found
no lock-order cycle.

Commit `aa339851f` applies the same identity check to abandoned
stages, including the active claim check for a named stage. It also
looks up the current consensus instance at each Panel GC obligation
read, including the count sweep's final read. Sixteen scoped retention
tests passed. Log SHA256 is
`13522c35534653dc2773e08b7bc15a1e25c5b01d2614c37d2600e92c8387bc1b`.
Concurrent writers sharing a fixed staging name, cross-process
writers and durable post-submit ownership remain outside these tests.

The public WebSocket returned Qwen #41 as an original REAL carrier.
B0 and the relay had the same attempt ID and matched the public
selected tip at virtual DAA 19,603. The first exact read had no
claim and no rejection entry for this carrier. The later exact read
matched the block to a refusal at 20,605 remaining DAA against 20,608
required, still with no claim. B0 reported zero receipts. There was no
additional load cohort.
The later paired class inventory still showed zero ready independent
seats and zero assignments. Node default/EVM test metadata compiled on
`aa339851f`. Neither patch has been deployed.

## Unique staging for retained copies — 05:11 UTC

Commit `895b93785` shares the producer's atomic publication path
with Panel material and answer writers. Every attempt to write uses a
unique same-directory stage. Write-once foreign copies retain their
first complete publication, while replacement paths atomically install
the complete new file. The producer's existing file and directory
sync steps remain in the durable mode. The stage name preserves the
claim prefix for the abandoned-stage guard.

Eighteen scoped tests passed, using the actual tempfile dependency,
including eight simultaneous writers targeting one write-once path
and the producer's existing publication failure/retry and Unix mode
checks. Log SHA256 is
`880d1a8b00063d52a9bd0d67c936d4378fd2b967a3039c0ef702fccdc0511c2a`.
Node default/EVM test metadata compiled. This is a source change;
older cache contents, durable post-submit ownership and live
qualification are separate.

At virtual DAA 19,604, B0, the Explorer relay, public REST/WebSocket
and the 113 RPC tunnel agreed on the selected tip. B1 and the 113
primary were still unsynced at 19,389 and 19,224. B0 reported 41 real
producer blocks, zero receipts and zero kernel Finals.

## Verified replay inputs before a Panel licence — 05:15 UTC

Commit `134ae8fcd` requires a durably saved verified replay input
before the active R-core replay branch can issue Valid. The input is
kept under its own claim-scoped foreign path, so old captures and
answer envelopes remain available. The existing Court candidate list
includes the new copy. The configured storage budget covers its staging,
and the claim retention oracle governs both GC paths. Nineteen scoped
tests passed, including a legacy truncated capture, a separate replay
copy, and release after discharge. Log SHA256 is
`86462a1bc705ca561de3798f14696c9b6b85b2fd490e6cd08daf8a21be77de20`.

The independent review identified a privacy gap in the draft whole
material resolver. The final response gate in `e27e8ec15` requires an
explicit PublicDa declaration on the actual bounded bytes. Private
inputs and unknown encodings remain available to the typed local
Court readers. The focused serve test covered PublicDa, PanelDa,
unknown tensor encoding, malformed input and an over-limit file.
Log SHA256 is
`4a517601c53784efbf284ad9cbbbfeb5cece9b1491e078c3d6c38158cfc9c1d1`.
Node default/EVM metadata compiled on the final source. These changes
remain undeployed. Retrying an already persisted replay can still
reserve space for a second stage.

At virtual DAA 19,605, B0, the relay and public REST/WebSocket
agreed on the selected tip. The same Qwen #42 attempt was present on
B0 and the relay and was publicly returned as a REAL carrier. Its
claim was absent on B0, whose journal matched that carrier to a
retention refusal at 20,606 remaining DAA against 20,608 required.
B0 still had zero receipt blocks and zero kernel Finals.

## Replay reuse under disk pressure — 05:16 UTC

Commit `ec155af3f` reuses an exact verified replay without reserving
a second copy. It compares the file in fixed-size chunks, rechecks
file identity, and syncs the file and directory before reporting
success. Missing or changed material requires space admission and
atomic durable replacement. The old file remains when a different
candidate cannot be staged. The scoped suite passed 21 tests,
including reservation refusal, an equal-sized last-byte mismatch
and a symlink. Log SHA256 is
`3a942f28167e4bf4368e0dd4ee6192ffc1b75ccddfd832c6d3a03c9820da82eb`.
Node default/EVM metadata compiled. The independent review found no
new blocker. The publication pin remains in-process and ends before
receipt handoff. The code has yet to run on T12.

The B0, relay, 113 RPC tunnel and public REST/WebSocket tips still
matched at virtual DAA 19,605. B1 and the 113 primary remained unsynced.
B0 reported 42 producer blocks, zero receipts and zero kernel Finals.
The earlier exact Qwen comparison remained terminal at preflight;
the local filesystem was still below its guarded launch floor.

## Guarded Linux venue for the original Qwen #21 comparison — 05:21 UTC

IBM's read-only Qwen artifact has 1,799,359,436 bytes and the same
SHA-256 as the local artifact previously pinned to the signed job,
`b73600cfeef3f54fd6e9f6a831c504588aa3b20ea2506ae824799206201e8ac8`.
Both hashes were bracketed by stable file identities. The host had
the exact Rust 1.93.0 compiler and the offline crate cache. The
seat5 cached CLI lacks the exact anchor/DAA arguments and there was
another build on seat5.

The source from `ec171af9863dd8187259df3a81c473b63ad5e3eb` was streamed
to a separate IBM directory. The Cargo lock, diagnostic sources and
A16 inputs were pinned. The offline single-job build was observed
running in its transient unit with a 4 GiB memory cap, no swap, one
CPU and a 12.5 GiB filesystem stop floor. The first service launch
exited before Python loaded because its path was relative; the
absolute-path retry is the live invocation.

A separate transient unit waits for the build's successful receipt
and a matching executable. The arithmetic run also checks the
original signed job's class, anchor, 1,023 input tokens, one output
token and registered 8,192 context. It is limited to 7 GiB memory,
one CPU and a 12 GiB filesystem stop floor. Both unit IDs were
confirmed active. No arithmetic result had been produced at the
observation. This is the offline comparison of the exact job, with
live Panel-generated material and an independent Node verdict still
unverified.

The public REST and WebSocket remained current at virtual DAA
19,606 with zero receipt blocks on the relay.

## Live activation state and exact Qwen #43 — 05:24 UTC

B0, B1, the relay, the 113 primary and the 113 tunnel reported the
same schedule identity. The deployed retention activation was 20,000
and was still unarmed. ModelOne and the Gate C attestor-liability
fence were armed. At DAA 19,607, B0, relay, tunnel and the public
index agreed on the selected tip. B1 and the 113 primary remained
unsynced. The paired Qwen class inventory counted nine eligible
independent bonds, zero ready independent bonds and zero assignments.

The exact Qwen #43 carrier was present on B0, relay and the public
WebSocket. Its claim was absent on B0. The matching journal refusal
had 20,607 remaining DAA against 20,608 required. B0 reported zero
receipt blocks and zero kernel Finals.

The refreshed observer separates the stored carrier from the
accepting fold clock, requires post-fence acceptance for the
effective-retention gate, and reports whether the deadline was
actually lifted. It checks the credited full-seat mask and reports
Final and Gate C separately. Four local boundary fixtures passed.
The real paired #43 check kept all gates false. The accepting fold
hash, the first qualifying carrier across the boundary, and actual
producer/provider custody still need independent evidence.

The guarded IBM build and dependent arithmetic unit retained their
original invocation IDs. The build was compiling workspace
dependencies after 238 seconds and had about 13.4 GiB free. The
arithmetic unit was waiting for the build receipt.

## Selected-chain cursor for the acceptance boundary — 05:26 UTC

The paired read-only cursor scanner starts from a stored selected
block, checks both nodes' sink, DAA and virtual parents before and
after, and examines a page of selected additions. It checks both
selected blocks and merged carriers, re-derives real attempt IDs and
reads their exact claim rows. Reorg or conflicting views prevent
cursor advance. An accepting fold is reported as a candidate only
until it can be independently identified. The local fixture covered
merged blue work, semantic red filtering and cursor rewind.

The live six-block exercise from DAA 19,605 to 19,608 inspected eight
unique carrier references and found the exact Qwen #43 and #44
attempts even though neither was itself a selected-chain block.
Neither had a claim on B0 or the relay. B0's #44 refusal recorded
20,606 remaining DAA against 20,608 required, and the public
WebSocket returned the same REAL carrier. The guarded build was
still compiling workspace dependencies with about 13.3 GiB free,
and the arithmetic unit retained its original waiting invocation.

## Producer custody and diagnostic disk pressure — 05:29 UTC

A read-only B0 custody probe bracketed the selected tip and service
invocation, then looked up the exact signed Qwen #44 by claim and class.
Neither its material nor its answer was present. B0 also lacked the
accepted claim row. The aggregate retention logs show GC reclamation
before and during the build, with one 18-group pass during source
staging. They do not identify the deletion of individual #44 files.

B0 uses the default 60-minute wall-clock retention setting. Its root
volume reserve was 15,545,570,700 bytes, derived from its measured
310,911,414,272-byte capacity. The 12.5 GiB build and 12 GiB run stop
floors omitted that higher reserve. The exact Qwen diagnostic staging
and compilation added disk pressure on the shared root volume.

After matching the invocation IDs, the operator stopped the
diagnostic waiter and then the build. Both units were inactive with
zero main PID, and the arithmetic process had not started. Logs and
the source pin were saved. Removing the diagnostic's reproducible
source and target scratch raised the reported free space from
14,229,671,936 to 15,666,597,888 bytes. The original model, signed
history, Court materials and production services were preserved.
A private bounded tmpfs is a candidate for the exact Linux
diagnostic. It had not been mounted or used.

After cleanup, B0 and the relay agreed on their selected tip and were
synced at DAA 19,609. B0 reported 44 Qwen model blocks, zero receipt
blocks and zero kernel Finals. B1 remained unsynced at DAA 19,389.

## Private tmpfs Qwen comparison and public cursor recovery — 05:32 UTC

The exact Qwen #21 Linux diagnostic restarted as a separate transient
unit with a private 4 GiB tmpfs, a read-only host filesystem outside its
small diagnostic directory, and private networking. It received the
pinned source directly into tmpfs. The offline single-job build was
observed compiling after checking the source pins. The service caps
memory at 5.5 GiB with no swap and one CPU, and guards host memory
and the measured B0 filesystem reserve during transfer and build.
At the build-entry check, the shared root had 15.70 GB available
against the 15.55 GB reserve. No exact arithmetic result was available.

A paired selected-chain scan advanced from DAA 19,608 to virtual DAA
19,610 with no additional Qwen candidate. Public REST returned 503
with checkpoint virtual-context lag while WebSocket remained synced,
then returned HTTP 200 with the relay cursor current on recheck.

## Next Qwen carrier and independent IBD check — 05:34 UTC

The next paired scan found the Qwen REAL carrier with header DAA
19,608 in the selected fold at DAA 19,610. B0 and the relay had no
accepted claim row for the exact derived attempt ID. The B0 journal
confirmed submission. Both node headers independently re-derived
the carrier hash, attempt ID and input anchor. The public WebSocket
returned the same carrier as a merged blue and REST was current.

The read-only B0 custody check found 2,409,135 bytes of material and
4,172 bytes of answer for the exact claim. A local material copy
matched the digest from the stable custody read. Provider delivery
and the effective retention deadline remain unverified for this
claim.

B1 was still unsynced at DAA 19,389 with one outbound peer, no IBD
and no connection to B0. The unsigned trial on host 5 had already
stopped and had no synced database or active trial ports. The
available filesystem capacity on 5 is below the existing floor for
a fresh trial and below the space needed for an independent B0 seed.

## SmolLM2 Tokenizer Court quote and rendering check — 05:38 UTC

The existing Studio Rust diagnostic from `f4a60bcda` reproduced
8,782,776 DAA against the 3,120 DAA limit using the retained
SmolLM2-135M worker frame, public binding and class declaration.
The quote uses DAA 19,597, six custodian slots and a fixture EOF
certificate. The worker frame has three input tokens, one decode
token and 15,252 step leaves.

The door is 347,858 DAA for the non-Tokenizer path, 8,418,376 for
Tokenizer raw and key work, 16,392 for the original table rendering
ceiling, and 150 for inventory and semantic opening. The separate
non-Tokenizer comparison still exceeds the same limit.

The read-only V2 rendering derivation checks the exact raw hash,
unique JSON and IDs, all token leaves, and the registered EOG leaf.
It reuses the original promote-odd tree and holds only the Merkle
frontier while building it. Local input limits cover raw length,
vocab, EOG count, key events and total rendering bytes. A native
caller still needs to reserve transient JSON parsing memory and
schedule the full artifact check.

On the retained SmolLM2 tokenizer and registered table root, the
focused Rust harness derived 98,471 key events, 81 maximum
rendering bytes and 332,367 total rendering bytes over 49,152 IDs.
The four source fixtures and that retained-artifact case passed.
They included a shorter, recomputed table root with a valid EOG
opening, changed EOG, duplicate keys and IDs, and premature EOF.
The harness linked the existing Merkle implementation and compiled
the changed renderer sources. The V2 result has no state or clock
effect. Native branch persistence, actual batch scheduling, and the
remaining TIR path still need verification before acceptance.

Separately, the focused A1 fixture accepted signed I16 boundary
values widened to I32 and preserved first-max selection. It refused
a re-committed and re-signed bad Panel lane. This fixture used a
small trace. The exact Linux Qwen #21 job was still compiling in
its private tmpfs, with no OOM events at the last sample.

## Exact Qwen #45 custody and #46 refusal — 05:42 UTC

An offline independent check authenticated #45's saved PAV2
signature against the registered bond and key. It matched the
2,409,135-byte material to the signed prompt, output, step,
checkpoint, trace and execution commitments. Four altered inputs
were rejected by their corresponding checks. The test did not
re-execute the model or establish provider delivery.

B0 and the Explorer relay reached virtual DAA 19,613 on the same
selected tip. The Qwen #46 REAL carrier was merged blue, visible
on the public WebSocket and absent as a claim on both nodes. B0
recorded its explicit rejection at 20,606 remaining DAA against
20,608 required. Its material and answer were present in the
bracketed B0 custody read. The paired class inventory showed nine
eligible independent bonds, zero independent ready bonds and
zero assignments. B0 and the relay reported zero receipt blocks
and zero kernel Finals.

The source now samples up to four exact Attempt creations from
the per-block delta once the existing ModelOne retention rule is
active. It emits the committed fold, carrier, claim, class,
accepting DAA and effective retention deadline only after the
RocksDB write succeeds. A separate selected-chain observation
must establish canonicality. The sample has a truncation flag
and does not clone the delta or expose the flat claims collection.
A consensus crate metadata check with default, EVM and PQ-only
features passed against cached dependencies. The diagnostic is
source-only.

The isolated Qwen #21 Linux job was still compiling. Its reported
memory peak was 2.58 GB with zero cgroup OOM events and about
15.68 GB of free production root capacity.

## Retention recheck and exact codec 1 refusal — 05:48 UTC

Qwen #47 was visible in the selected merge scan and on the public
WebSocket. Both B0 and the relay lacked the exact claim row. B0's
journal recorded the retention refusal at 20,606 DAA remaining
against 20,608 required. Public REST and the relay agreed on the
selected tip at virtual DAA 19,614.

The retention janitor now uses the current resident obligation
oracle during planning and again at deletion for both income and
DA holds. Previously the deletion recheck could bypass a newly
observed income hold below R-core. Two extracted production-path
fixtures reproduced deletion on the old source and passed on the
new source, including an unavailable resident state. Kaspad
metadata also passed against cached dependencies. The change is
source-only.

B0 served the same historical codec 0 stream for pruning point
`4dc9…1ab9e`. All 12 chunks, 47,050,528 bytes, matched the saved
stream root, first-chunk SHA and independent keyed hash replay.
The temporary non-permanent peer and tunnel were closed. B0
retained the same process and public relay peer.

The old Rust decoder and declaration validators were rerun against
the captured bytes under the pinned T12 genesis, params and
schedule. Legacy validation passed. Strict validation failed for
Qwen class `7d7e…922fc`, registered at DAA 508. Its valid
9,362-byte declaration is present, but the checkpoint has neither
an authenticated canonical semantics row nor a registration-origin
row. The strict sender checks that identity before zlib, so the
captured checkpoint fails prior to compression. The capture is
evidence for the refusal and has not been installed as a sync seed.

The Linux Qwen #21 job stopped before arithmetic when host
available memory fell below the 10 GiB guard. The isolated
Darwin run then started with the existing corrected, hash-checked
binary and registered artifact, network disabled and a 5 GiB
RSS guard. The original signed output remains the expected result.

At DAA 19,615, B0 and the relay both reported the class semantics
and registration-origin fences as unscheduled. The retained
checkpoint contains an active registrant bond for the DAA 508
class. A prospective signed declaration would require an active
rule and a later checkpoint that actually carries the declaration.
Current signer authority was not inferred from the old snapshot.

## Producer disk admission and historical registration — 05:54 UTC

With no process storage budget, an ordinary producer now checks
that the retention volume can hold its complete staged capture
above the janitor's existing reserve before publishing an Attempt.
The measured floor is the larger of 8 GiB and 5% of the volume.
The focused test covered the boundary on both sides of that
choice and checked overflow. The Kaspad test crate metadata passed
with default and EVM features against cached dependencies.
The change is source-only.

The archived emission scan links a registration-size lifecycle
transaction with ID prefix `dcfcb2536802` to DAA 507 carrier
`e4da…f12f` and DAA 508 accepting candidate `998d…7fd0`.
The B0, B1 and relay RPCs agreed on the headers and their Merkle
roots. All three hold headers only for both blocks. The original
transaction body and exact acceptance membership still need an
archive to authenticate the historical class declaration.

Seat5's existing immutable Qwen 8K artifact matched the
1,799,359,436-byte B0/Mac SHA-256. The seat remained stopped
through the read. Public95 had 18.72 GB free and 11.46 million
KiB of available memory, below the existing independent trial
guards. Seat5 also had a separate Cargo build running and its
available root space was below its retention reserve at the
latest preflight.

B0 and the relay matched at virtual DAA 19,616. The exact Qwen
#48 remained absent as a claim on both. B0 recorded refusal at
20,607 remaining DAA against 20,608 required. The public index
was current on recheck.

The isolated Darwin Qwen #21 process entered the arithmetic
comparison and ran for 474 seconds. The APFS free-space guard
stopped it after a sample reported 939,098,112 bytes free.
The measured peak RSS was 4.42 GB, scratch stayed empty, swap
did not grow, and there was no arithmetic verdict.

## Studio TFP1 preflight and owner scheduling — 06:01 UTC

Studio `ca3769c` reads the selected JSON peer's exact class registration
before the rail reads its key. With a configured public bond, the same
peer's producer facts supply an explicit unbound-container refusal.
The startup error includes the independent inactive fence and local
semantics-file checks. Automatic bond selection still leaves the
binding check to the job's native unsigned quote. The Studio runtime
suite passed 259 tests with one ignored. The startup regression uses
JSON wRPC fixtures and checks that neither identity nor outbox exists
after each refusal.

Integration rail `b925495f8` carries `--tfp1 require` into each watched job
and refuses missing binding or RPC inputs. The extracted Rust mode
regression passed. The owner now ranks Demands by the last DAA at
which the accepted request can leave its full response span before the
stream deadline. It skips impossible new Demands and drops expired
unsent Demands. The exact Rust clock predicate passed 4,096 small
tuples and checked integer boundaries.
Kaspad's test crate, including the actual outbox ordering fixture,
passed metadata compilation with default and EVM features. The
outbox fixture was not linked or run.

The public T12 acceptance blockers remain. The fixture lacks an
applied registration, the two existing TIR classes lack an accepted
PALWART3 binding, and the class-semantics fence is unscheduled. The
recorded same-job quote requires 8,782,776 DAA against 3,120 allowed.
Removing the legacy tokenizer charges alone would still leave 347,858
DAA of TIR costs. The read-only rendering derivation has no native
record or claim-admission effect. Public TFP1 carrier, same-claim
Panel and Gate C, PFS4, reward and G14 dialogue are unverified.

B0 and the relay matched at virtual DAA 19,617. Exact Qwen #49
was absent as a claim and B0 again recorded 20,607 DAA of
remaining retention against 20,608 required. At 06:01:16 UTC,
public REST and WebSocket both reported virtual DAA 19,619, with
a current selected-chain index and zero receipt blocks reported
by that relay process. The three sampled selected blocks were
legacy heartbeats.

The closed Linux scratch checkout now retains the Git object tree,
build binary and logs, with only Cargo manifests checked out. This
recovered about 760 MiB. The exact Darwin Qwen #21 replay was
relaunched in private scratch under the existing APFS, RSS and
swap guards. No verdict was available during this observation.

## Original Qwen tile and historical class declaration — 06:13 UTC

B0, the relay and the public Explorer index matched at DAA 19,620.
Explorer served Qwen #50 as a REAL merged-blue block. B0 and the
relay both lacked its exact claim row. B0's native journal recorded
20,606 remaining DAA against the required 20,608. The merged block
remained valid. The public retention rule at DAA 20,000 was still
unarmed, with nine independently eligible Qwen bonds, zero
independently ready and zero assigned. That inventory is separate
from a claim-specific draw.

The original B1 and 113 primary remained unsynced at DAA 19,389 and
19,224 respectively. The separate Explorer relay and the 113 tunnel
matched B0. The unsigned seat5 IBD trial was closed, with 20.03 GB
of root space available against its 22.55 GB launch floor. A separate
build was already active on that host.

In the integration source, `7ec1d67eb` pins each rebuilt Panel
capture during storage admission and durable publication. Exact
durable copies are reused without reserving space for a duplicate.
A local storage refusal leaves the already built Court response
available. Kaspad test-crate metadata passed with default and EVM
features. The patch is source-only.

For exact Qwen #45, the isolated SDK builder used the ordinary 8K
artifact and original signed 1,023-input, one-output capture. It
replayed a folded A16 embedding tile and constructed its registered
artifact row opening. The distinct verifier process had OS-enforced
read denial for both model files and original capture. Native
adjudication matched the original tile, deferred a withheld operand
and refused an altered operand. The evidence was 10,827 bytes.
The final source rerun produced the same evidence SHA-256,
`cb590e8ef93865ccbd8efc31d2487441aeb4fbd5ba5a4bd207815fcba92cec3c`,
in 74.0 seconds including compilation, with a sampled builder peak
RSS of 1.91 GB and zero swap growth.
The separate single-row parity test also passed with odd-tail
Merkle leaves.
This is one original rooted arithmetic relation. Independent live
assignment, full-job normal evidence, carrier fold, credit and payment
remain open.

The closed B1 partial archive held the 111,994-byte transaction
body and 624-byte acceptance row for DAA 507/508. An exact-key
RocksDB 10.4.2 reader was tested on a synthetic fixture and ran
against the archive in a private read-only mount. Native Rust
rehashed both paired RPC headers, the transaction Merkle root and
the accepting ID root, then matched the full registration tx ID
`dcfcb2536802…1c33b` at its accepted carrier index. Its artifact
root and admission digest match the original, authenticated codec 0
snapshot's declaration for class `7d7e…922fc`. The archive and live
services were preserved.

The paired current-chain ancestry RPC refused from DAA 508 because
the query was below the retained root. The authenticated registration
bytes establish the declaration's historical origin at DAA 508.
They do not install a native origin row or qualify the codec 0
capture for strict fresh IBD. The live import remains blocked.

B1's archive also supplied the DAA 2,844 checkpoint header, the
level-zero GHOSTDAG row and its selected-parent header. Native
rehashing confirmed both block identities and their direct-parent
link, including a same-DAA edge. B0 and the relay then returned the
same immediate selected child of the checkpoint at DAA 2,845.
The offline snapshot reconstructed the exact PALW root committed
by that child. The checkpoint's own header commits its parent's
root, so comparing against that header gives a different value.
The strict historical declaration refusal still reproduces.

The complete archive path remains unverified. The first bounded
walk reached its 90-second deadline after repeated RocksDB table
reopens. With a bounded read-local cache it reached the 8,192-edge
limit before the exact DAA 508 endpoint. Partial output was removed
by the archive reader. The successful first hop and verified
registration remain separate pieces of evidence.

## Claim fold coordinates and original Qwen projection — 06:28 UTC

The paired selected-chain scan identified Qwen #54 in the fold at
DAA 19,625. Its carrier header was at DAA 19,623 and signed
retention at 40,231. B0 recorded the exact refusal at 20,606 DAA
remaining against the required 20,608, and both nodes lacked its
claim row. The extended scanner reached matching virtual DAA
19,627 without another Qwen candidate in that page.

Claim detail now reads the fold's accepted blue score and the
state's exact tip hash alongside the existing carrier hash and DAA.
This lets a reader match a fold when several selected blocks share
one DAA. The updated view was compiled against cached native types
and reproduced all three identities and effective retention for 32
rows from the authenticated cold T12 snapshot. Observer fixtures
covered same-DAA ambiguity, a different state tip and older replies.
The change is source-only.

The separate offline Qwen #45 diagnostic also verified original
query projection leaf 48. A verifier denied access to both model
and original capture authenticated 12 input tiles and two artifact
openings, then recomputed eight output lanes with 12,288 integer
products. Its 30,713-byte packet yielded the same result on recheck.
Withheld weights or coefficients deferred the verdict, and
altered weights, coefficients or input were refused. The existing
embedding diagnostic produced its original byte-identical result.
The scope is one projection tile from the original signed carrier.
Live assignment, whole-job adjudication, credit and payment remain
unverified.

Local submission can return before a carrier has folded. The
in-memory publication pin then drops and current-state absence can
allow pressure reclamation before a delayed fold or after reorg.
An exact retirement delta supplies positive evidence on one branch.
Pruning catch-up can still replace that branch above the previous
pruning point. A destructive release therefore needs an irreversible
branch-relative condition, or a verified serving archive while the
hot copy is absent. The scoped investigation produced no GC rule
change.

The archive walk reached the exact DAA 508 acceptance block in
10,609 edges from DAA 2,844. In the private read-only archive
namespace, the native verifier rehashed all 10,610 headers and
checked every level-zero direct-parent link. It consumed
64,965,699 bytes from a bounded pipe and retained a 1,442,960-byte
compact link manifest. The raw stream SHA-256 was
`6114b509d185d25bd9e309d83a0d4b9689c84f21ade2d4e102c935cfff294158`.
The archive and protected node generations were preserved.
The chosen path comes from the archived GHOSTDAG index. Independent
selected-parent recomputation, strict snapshot integration, and
live unsigned IBD remain open.

## Historical registration and Qwen execution checks — 06:36 UTC

The native archive reader independently selected all 10,609 parents
on the DAA 2,844 to DAA 508 route. It rehashed 15,426 distinct headers,
checked their declared score and work against archived compact records,
and resolved 626 work ties with the native order. The native round
rules excluded 3,176 round-parent references. The transcript SHA-256 is
`1920e458bc534754d7e17ddf32b0921352479ba102dc0ae3081d9c7e40ab3408`.
This check uses authenticated declared work. It does not independently
recompute full GHOSTDAG mergeset work.

The DAA 508 fold accepted six transactions from two DAA 507 blocks.
The two block bodies matched their rehashed headers. All accepted IDs
and indices matched the bodies and reconstructed the fold's accepted-ID
root. The exact Qwen class registration was the only one for its class
among the six transactions. Its ML-DSA signature verified against the
registrant bond public key in the authenticated DAA 2,844 checkpoint.
Flipping one signature bit caused verification to fail. The archived
declaration matched the checkpoint declaration digest. This did not
revalidate the historical UTXO state. Strict snapshot validation still
refuses the pre-fence legacy class under the current release rule.
No snapshot was installed or signing service operated.

The original Qwen #45 query projection passed the native arithmetic
check. A local counterfactual changed one output lane and rebuilt the
commitments for that change. Its structure and original activation
inputs authenticated under the synthetic root. Native arithmetic
rejected the changed lane, and the original signed claim refused the
synthetic root. This was a single unsigned, isolated fault case.
Two focused tests passed and the ordinary projection output was
byte-identical to the previous run. No synthetic claim was submitted.

The full Qwen #21 Darwin arithmetic comparison reached its 2,400-second
duration limit without a verdict. Peak RSS was 4,632,215,552 bytes
against a 5 GiB cap. All sampled APFS and swap bounds held. The original
artifact and executable fingerprints were unchanged. This establishes
a timeout and does not establish an arithmetic mismatch.

B0 and the relay agreed at DAA 19,629. The selected fold at DAA 19,628
referenced a new Qwen carrier with header DAA 19,627 and signed retention
DAA 40,235. The exact claim row was absent on both nodes. Public REST
and WebSocket matched at DAA 19,629, with twelve sampled selected
blocks all classified as legacy heartbeats.

The exact B0 journal confirmed the new carrier was Qwen #56 and
refused it with 20,607 of the required 20,608 DAA remaining. B0,
the public relay and the 113 tunnel agreed at DAA 19,630. B1
was still unsynced at DAA 19,389. The original 113 primary
service was inactive, and the unsigned host-5 IBD trial was
inactive with 19,258,056,704 root bytes free. The paired Qwen
inventory at DAA 19,630 found nine independently eligible bonds,
zero independently ready, and zero assigned. This is a class
inventory without a claim-specific draw.

Public REST returned a checkpoint virtual-context lag response
while its WebSocket was synced. The recheck returned HTTP 200
and its selected tip matched WebSocket at DAA 19,630.

The exact offline comparison now supports opt-in position and
phase timing on standard error. It retains the result schema and
reports native and mirror times separately. Five focused tests
passed, including final counter parity with the progress observer
off and on. The timed-out Darwin executable had linked unoptimized
native A16 and TIR arithmetic dependencies. The guarded diagnostic
build compiles those same sources with optimization and preserves
overflow checks and debug assertions.

## Exact original Qwen #21 full arithmetic comparison — 06:45 UTC

The guarded optimized diagnostic completed in 230.0 seconds with
`EQUAL`. It used the registered Qwen2.5 A16 8K artifact, the
original T12 anchor, and the DAA 19,566 attempt. The workload
was 1,023 input positions and one selected output token. Native
A16 and the typed mirror matched 689,505 of 689,505 compared
node rows and the final logits row. Both selected token ID 220
and reproduced the original signed output root. The input root
matched the original commitment. Peak RSS was 4,528,504,832
bytes. Across 114 samples, free disk stayed above 1,359,351,808
bytes, swap growth was zero, and no scratch files were written.
The run used two Rayon threads and a network-denied sandbox.
Its receipt SHA-256 is
`ab311b11749ecce2d3e11d655ecddbd2343b67070991e47de358cfaacea99e6a`.
The optimized build and its five focused tests used the pinned
original Base0 and TIR Exec sources with overflow checks and
debug assertions retained.

This is arithmetic compatibility for a specific original claim
in an isolated process that had the full model. It establishes
the exact baseline for a Panel-built packet and an independent
model-free Node check. Those checks and native adjudication
remain separate from this result.

The later public and paired-node read matched at DAA 19,631.
Qwen #57 had signed retention DAA 40,236 and was referenced
by the selected fold at DAA 19,630. B0 rejected the claim
with 20,606 DAA remaining against 20,608 required. Both nodes
returned no claim row for the exact ID.

## Exact Qwen material custody and Positive Court entry — 06:51 UTC

B0, the relay, and the public index agreed at DAA 19,632.
The new Qwen #58 carrier was merged blue. Both node reads
returned no claim row, and B0 logged 20,606 DAA remaining
against the required 20,608. Its producer still held
2,409,135 bytes of material and 4,172 bytes of answer.
A bounded read recovered both files and matched their hashes
to the earlier custody observation. The paired raw header
and registered-key reads agreed. Both nodes reported the
DAA 20,000 retention activation as scheduled and unarmed,
with zero receipt blocks in those process views.

The older #21 raw signed header, key, anchor, and roots
were rechecked on B0 and the relay. The exact material leaf
was absent at its recorded B0 path and the relay did not
have a retention directory. No cause or deletion time
was inferred, and no later capture was used as #21 material.

The existing Qwen A16 packet uses the legacy shard Court
format. The current Positive Court Panel admission expects
a rooted TIR class, PositiveResultV2, and a matching program
and job selector. The legacy CoreV1 draw is a prefill with
one selected output and has no such rooted result. The typed
mirror reproduces the computation without issuing that
result. The native assigned Case handler on the separate
A0 branch also still returns history unavailable at its
WitnessCommit, ExactFrame and Finish entry. The raw ingress
and pure frame checker are present. The private preflight
compared source and saved artifacts and did not exercise
a live Positive Court or change the A0 worktree.

For #58, an independent check reconstructed the 1,023
anchor-derived input IDs and matched their commitment. It
reconstructed the signed output root from the recovered
answer's token 220. A separate process denied access to
the model, original material, answer and network then
verified an exact legacy query projection packet. The
30,713-byte packet contained 12 authenticated activation
tiles and two registered artifact openings. All eight output
lanes matched. Missing operands deferred the verdict, and
altered input or source data was refused. This covers one
relation from the original signed claim.

## Same-claim Qwen #58 replay and ordinary Court recovery — 06:58 UTC

The separate complete A16-to-typed comparison for #58
replayed the recovered claim's exact anchor. All 689,505
compared rows across 1,023 input positions and the final
logits row matched. Both engines selected token 220 and
reproduced the original signed output root. Wall time was
222.089 seconds and peak RSS 4,682,760,192 bytes. Across
110 samples there was no swap growth, and the run was
network-isolated. The result SHA-256 is
`7f5b2921426814a3f3321fceb799f10cc9e714f063e21bb7e1c3c9572a249305`.
The full arithmetic comparison used the model. The
model-free verification used only the single previously
recorded projection from the same #58 claim.

Ordinary CoreV1 Court responses can now recover from an
existing external HTTP provider copy after local material
failure. The background fetch shares the existing single
slot and grants Seat requests first access in the tick.
Use requires the exact accepted carrier, matching roots
and effective retention, and successful backend material
validation. Provider-backed signing rechecks the current
claim, due unit, deadline and stable view before using the
same session for assembly. The patch adds no archive writes
or authority to delete a sole copy.

The first executable helper test exposed that the legacy
fixture did not check the optional output root. The new
provider branch is limited to CoreV1 and the corrected
fixture verifies both the expected result and refusal of
a different output root, invalid bytes, missing anchor,
missing draw and a legacy backend. The extracted helper
also opened a real event from the exact fixture material.
The final Kaspad test-crate, wRPC, remote and Base0 metadata
checks passed, as did the extracted native helper test.
The network fetch and signing path has not been deployed
or exercised on an accepted public claim.

B0 and the relay reached DAA 19,635. Qwen #59 had signed
retention DAA 40,240 and was referenced by the selected
fold at DAA 19,634. B0 logged 20,606 remaining against
20,608 required, and both nodes lacked the claim row.
At DAA 19,636 the public REST index briefly reported
checkpoint context lag and then matched its synced
WebSocket tip with HTTP 200.

## Same-DAA Court recovery and second model-free Qwen relation — 07:02 UTC

The first ordinary Court material miss previously stamped
the duty as submitted. The result could arrive during the
same DAA, but the next Court pass skipped that duty for ten
DAA and discarded the ready result as unused. The outbox now
distinguishes a miss that reserved a provider request from
a submission stamp. Only a ready result for the exact
claim and carrier can bypass the miss cooldown. A real
submission keeps the original cooldown. The Kaspad test
crate and dependency metadata compiled. The extracted
production debounce methods passed the same-DAA test.
This is a local scheduling check. A full provider and
carrier process test, competing fetch fairness and reorg
between queueing and final submission remain open.

A new independent run on the original Qwen #58 checked
layer-0 attention RMS normalization. A verifier denied
access to the model, original capture, answer and network
authenticated all 12 input tiles of the activation row
and recomputed all 128 output lanes. The 16,165-byte
packet uses no artifact opening because the selected
kernel has no such operand. Omitted and altered
activations were rejected. Both cross-substitutions
with the existing query projection packet were refused
and the existing query result was byte-identical.
The new packet SHA-256 is
`20fdead9b9005e62c7f44122bda05dc6b72d7ac07e24d79477936762b28f99ad`.
The scope is now two distinct legacy Court relations of
the same real signed claim, plus its separate full
arithmetic comparison using the model.

B0, the relay and the public index matched at virtual DAA
19,637. Qwen #60 was referenced by the selected fold at
DAA 19,636. B0 rejected it with 20,606 of the required
20,608 DAA remaining. Both nodes lacked the exact claim.

For a strictly higher retention deadline with identical chunk
layout, the FS and reference HTTP provider can reuse verified
chunks by linking them at the new manifest ID's paths. The
old ID remains readable. The publisher checks read-back, and
the manifest writer rechecks the predecessor before replacement.
The existing logical byte quota still charges the new paths.
A late race or interrupted extension can leave staged aliases;
no archive deletion or quota relaxation was added.

The guarded transport harness used the actual 2,409,135-byte
Qwen #32 capture and a fixture effective deadline. Both FS
and loopback HTTP served identical bytes and used the same
device/inode for every old/new chunk pair. The HTTP server
was restarted against the same store. All 24 tests in the
bounded run passed. Two unchanged tests that create multi-GiB
sparse files returned ENOSPC in the initial HFS+ RAM-volume
run and were filtered in this run.

The two sparse-file tests also passed with the same pinned
source in an APFS scratch directory. The 5 GiB sparse
preflight added no allocated blocks to an empty file.
Compiler output stayed in the owned RAM stage, the fixture
files were removed, and the RAM volume was detached. The
exact-source harness therefore has 26 passing cases across
the two environments. This is transport-level coverage.

The isolated provider outbox provenance candidate
`a202ebb57` retained the exact duty, claim roots and packet
identity through carrier preparation. It compiled with
Kaspad test-crate metadata after correcting the fixture
types. It remains outside the integration branch. Its
rechecks currently rebuild all obligations for a seat at
each carrier checkpoint. A bounded one-claim, one-unit
resident read with the same deadline, signer and multipart
rules is required before adoption. No full carrier process
test or live signature was produced.

## Retention boundary, pre-fold pressure protection and fused attention — 07:17 UTC

B0 and relay reported the same future retention fence at DAA 20,000,
still unarmed at DAA 19,640. Public REST and WebSocket agreed at that
DAA. B0 and relay subsequently agreed at virtual DAA 19,641. Qwen
#61 and #62 folded with only 20,607 and 20,606 of the 20,608 required
DAA remaining. B0 logged both refusals and the exact claim rows were
absent from both nodes. The read-only check still found #61 material
and answer on B0.

The native transition regression now covers acceptance at 19,999,
20,000 and 20,001 with merge delays of 0, 1, 2 and 10 DAA, plus a
21-DAA delay at 20,011. The focused harness linked the cached native
Core library and ran the expanded signed matrix. Before the fence,
the no-delay fixture accepted and delayed fixtures were refused.
At and beyond the fence, accepted rows used the effective deadline,
retained the signed identity and passed cold carriage and rollback.
Seven prior native retention tests also passed when rerun with
their original compiled executable. The focused matrix is an
isolated fixture, without a public post-fence acceptance.

Before publishing an ordinary local model lead, the producer now
reserves storage and writes a 172-byte durable marker bound to the
exact finalized carrier and capture. This protects a young marked
capture from pressure-only pruning after restart and before first
fold. The marker is checked again under the final unlink guard.
The age ceiling is the existing capture horizon, and the original
native DA and income checks remain in force. A failed marker
publication withholds the lead. A claim that is still unaccepted
beyond the existing horizon has no protection from this marker.
Two focused production-extract tests and Kaspad test-crate metadata
passed. The changes are source-only.

A third independent offline relation was checked on signed Qwen
#58. At position zero, fused attention authenticated Q, K and V
and four registered parameter openings, then recomputed eight
output lanes. The model-denied verifier rejected each removed or
altered input row, each removed or altered parameter, and a changed
output. The existing query and RMS diagnostics remained valid.
The bound on-chain one-move verifier still requires dissection.
The 27,100-byte packet SHA-256 is
`3023def6983a7bd83cd73ecd106aacff384a404151600bbc56fdc36f4194b2e2`.

The queued provenance candidate now has a one-duty resident
reader on the isolated branch at `dee83ebc4`. It borrows a stable
selected snapshot and checks the exact claim, carrier, roots,
deadline, covering rank and multipart progress. Full Node and
native fixture validation remain open. The first exact Core
metadata attempt was stopped at its 3 GiB child RSS guard while
the separate A0 build was active. The owned RAM volume detached.

## Exact claim observer and queued provider handoff — 07:24 UTC

B0 and the public relay agreed on Qwen #63 and #64. The respective
carrier/selected-fold DAA pairs were 19,639/19,641 and 19,641/19,643.
For each, B0 logged the 20,606/20,608 retention refusal, and both
claim-detail RPCs returned absent. The independent class inventory at
DAA 19,643 had nine eligible bonds, zero independently ready and zero
assigned. At DAA 19,644 both nodes still reported the retention rule
scheduled at 20,000 and unarmed. The public REST index and WebSocket
agreed at 19,644.

The read-only claim observer in
`/private/tmp/misaka-t12-oom-20261102/g14-live-goal.W3V5gB`
now separates the carrier from the exact selected fold. It pins both
nodes' frontier, requires the claim state's exact tip and matches the
fold's hash, DAA, blue score and carrier relationship. Ten local cases
passed, including same-DAA disagreement and missing coordinates.
The live negative path reported no assignment for #63 and #64.
The same observer read an historical voided Qwen row on both nodes.
That row's current retention is later than its initial signed deadline
and the running RPC omits fold coordinates. The observer reports both
facts separately and withholds an exact fold or provider export for it.

The isolated provider provenance candidate `a926c1a49` adds a
fixed-size local authorization check inside the blocking transaction
worker and repeats it under the mempool write lock before insertion or
replacement. It preserves ordinary/RBF policy and compares the pinned
worker instance to the current instance. A queued-worker race fixture
is present. Compilation and runtime validation remain open, and a
change after the last check is outside the guard's scope.

The private held-dissection harness for the original Qwen #58 prepares
the position-zero terminal checker with a checkpoint anchor and
original signed openings, plus altered and missing operands. The
guarded launch stopped before compilation at the 1 GiB APFS floor
and detached its owned RAM volume. There is no held verdict.

## Original Qwen held terminal and provider lock scope — 07:28 UTC

At DAA 19,645, B0, the relay and the public REST index and WebSocket
agreed. Qwen #65's carrier at 19,643 appeared in the selected fold at
19,644. Both claim-detail RPCs returned absent and B0 recorded 20,607
remaining against 20,608 required. The exact observer saw no assignment.
Seat5's unsigned IBD unit is stopped. Root space measured 18,758,246,400
bytes against a 22,548,578,304-byte launch floor, and the earlier strict
sidecar refusal remains a separate source qualification issue.

The guarded private run on Qwen #58 succeeded after APFS headroom
recovered. The original signed leaf 318, first checkpoint and original
input/output openings entered the native held checker. A verifier denied
the model, capture and answer returned `ChallengerDefeated`. Missing
anchor and altered Q, K, V and output were rejected. An altered local
responder maximum with a finalizing accumulator unchanged returned
`ExecutorGuilty` using the same original bottom. The packet was 11,667
bytes, SHA-256
`6b1a554ed1980c67468befa81ce6ba075afd8f3ec08ac59be7eefd2c5d080553`.
The build and run took 25.637 seconds with a 1,952,776,192-byte peak
child RSS, zero swap growth and verified RAM-volume detach. Both older
temporary binaries were retained. One history position requires zero
interactive rounds, and the test created no chain session or credit.

Isolated candidate `f8b39718a` now checks the exact provider duty at
worker entry and again after contextual validation, outside the mempool
write lock. The lock-held check uses a fixed committed virtual snapshot
identity, import epoch and active consensus instance. It rejects lock
contention and a staged, uncommitted virtual view. The candidate adds a
deterministic change-between-check-and-insertion regression alongside the
existing worker handoff test. Cargo execution remains outstanding.
A read-only fingerprint closure resolved 38 metadata units, including
both Core feature sets and Kaspad default+EVM, with every unchanged
direct artifact present. The bounded runner is prepared. A separate A0
compiler was active, so this compile slot remained unused.

## Isolated Linux type check and strict IBD recheck — 07:32 UTC

B0, relay and public Explorer agreed at virtual DAA 19,647. Qwen #66's
carrier at 19,644 appeared in the selected fold at 19,646. Both nodes
lacked the exact claim and B0 reported 20,606 DAA of remaining retention
against 20,608 required. The DAA 20,000 rule remained unarmed on both.
The Qwen readiness inventory had nine independently eligible bonds,
zero independently ready and zero assigned.

The original #58 now has a prepared held-dissection fixture at
zero-based prefill position 16. Its 17 history positions make two
tiles, one full and one ragged, and require a real disclosure and
choice. The negative case compensates the two child accumulators
while preserving the root aggregate and signed output. This fixture
has not been compiled or executed. The separate A0 compiler remained
live on Darwin.

The isolated Linux mining-worker type check for `f8b39718a` is active
on Seat5 under `misaka-t12-g1-mining-check-20261103t0731z.service`,
invocation `5afe92616cd94d49875bf55d6a51f34c`. The verified Git
archive and source pins are in a private RAM stage. Cargo is offline
in a separate network namespace with a private target tmpfs, one CPU,
a 6 GiB memory cap, no swap and monitored host disk and RAM reserves.
The observed process was compiling dependencies after 72 seconds.
Its cgroup reported zero OOM and zero kills. The scope is mining
worker test metadata, and no test verdict has been recorded yet.

A byte-identical native replay of the archived registration and
checkpoint confirmed why the strict IBD source still fails. Both
current B0 and relay advertised the same affected pruning point
at 19,647. The archived DAA 508 declaration and signature verify,
while the committed checkpoint lacks both the canonical semantics
and native registration-origin rows. The release's current strict
rules refuse that checkpoint. The old checkpoint and archives were
preserved.

## Qwen #67 custody and measured held replay — 07:42 UTC

B0, relay and the public index agreed at DAA 19,648. Qwen #67's
carrier at 19,646 was referenced at 19,648 and its claim was absent
on both nodes. The exact B0 journal again recorded 20,606 DAA
against 20,608. A stable read pinned 2,409,135 bytes of capture and
4,172 bytes of answer on B0. This confirms custody at the observation
point. The next paired scan reached 19,649 with no new Qwen fold.

The original #58 position-16 diagnostic first failed compilation
with a local closure lifetime error. The corrected compile passed.
Its Panel builder ran into the 175-second bound. A second source
revision removed the repeated query replay and timed the remaining
operations. It reproduced the original 17-position checkpoint root
at 93.815 seconds, then stopped at the wall limit in fused-output
replay, before any model-free verdict. Each failed attempt released
its owned RAM volume. The next private revision uses small planned
prefill batches without unused output projection, while preserving
the native original-checkpoint root check. Its compiled process was
observed running under the same runtime limits.

The same Seat5 worker check, invocation
`5afe92616cd94d49875bf55d6a51f34c`, was still checking Consensus
Core at 661 seconds. Peak cgroup memory was 5,385,728,000 bytes,
with 141,576 memory.high events and zero OOM or OOM kills at the
sample. This is a build-resource observation, without a test verdict.

### Qwen #58 one-round verdict — 07:46 UTC

The bounded no-post revision passed on the original signed claim.
At prefill position 16 and leaf 1,648,446, the native arity-four
checker processed one disclosure and selected each of two children,
with widths 16 and 1. The verifier was denied the model, original
capture and answer. Both honest child proofs returned
`ChallengerDefeated`. Compensating errors in the child accumulators
preserved the aggregate and original signed output, so the false
disclosure passed the native aggregation check. Native choice
selected child 0 and its terminal checker returned `ExecutorGuilty`.
A sibling's terminal input and a missing checkpoint anchor were
refused. These were private unsigned transitions.

The packet was 50,796 bytes, SHA-256
`437d26bb3a239301b8b542c68e3ddd3d25ca46f33c9bf52d9b2f3a89de49a51e`.
The checkpoint matched in 80.425 seconds, 13.390 seconds earlier
than the preceding source revision. The full diagnostic took
172.305 seconds, with peak child RSS 2,311,864,320 bytes, zero
swap growth and verified RAM-volume detach. This check covers
one held dissection and created no chain session or credit.

B0, relay and the public index subsequently agreed at DAA 19,651.
Qwen #68's carrier at 19,648 was referenced at 19,649. Both nodes
lacked the claim and B0 recorded 20,607 DAA against 20,608.

Seat5's exact worker check reached its 960-second time guard while
still checking Consensus Core under `memory.high` throttling. At
the last live cgroup sample no OOM or OOM kills were counted. The
terminal unit had PID 0, failed with exit 1 and had the original
invocation identity. No compiler verdict on the worker was reached.
The terminal result, complete log, resource samples, source pins
and runner were copied and matched the remote SHA-256 manifest.
The owned RAM source stage was then released, restoring
1,029,152,768 bytes of tmpfs space. This did not qualify Seat5
for unsigned IBD or change the persistent storage requirement.

## PALW clock, builder and TFP1 parallel follow-up — 08:06 UTC

The exact B0 and relay status read at DAA 19,656 reported REAL
attempt ticks active from 5,300, PFS4 spends active from 15,500,
and the 20,000 acceptance retention fence still unarmed. Both
reported class semantics, the bounded door and designated answers
unscheduled. A paired header-only replay of eight selected blocks
found four one-DAA increases whose only known source lane was
heartbeat, and three sets containing a REAL carrier with zero
increase. Mixed candidate sets are left unattributed. The running
RPC does not supply the actual cursor for each historical fold.
Explorer REST and WebSocket were current at the subsequent DAA
19,657 check and the relay process still reported zero receipt blocks.
B0 refused Qwen #69 and #70 at 20,606/20,608 and #71 at
20,607/20,608. Both nodes lacked each exact claim.

The builder now evaluates mature own and delegated receipts before
the local attempt registry hold. The spendable quote returns the
exact parent it used. A worker compares it and the candidate DAA
against the template and active consensus before signing. RDA4
regular-file reads use the existing 16 KiB receipt cap. The clock
trace records the native grant, next slot, source counts and
timestamps, and heartbeat logging records the actual local hint
and yield decision. Consensus tick rules and PFS4 pricing are
unchanged. Two extracted source checks for receipt ordering and
bounded reads passed. Thirty retained Core clock/heartbeat tests
and two retained native Consensus fixtures passed.

The provider authority candidate `f8b39718a` passed a coherent
38-unit metadata run including Kaspad default+EVM tests. It remains
isolated. Its full native Core test link crossed the 7 GiB RSS
guard before running any cases. Three retained baseline held-state
tests separately passed for input binding, checkpoint anchor and
penalty attribution. The original public #58 cannot enter that
transition because it never acquired an accepted claim row.
Positive Court's rooted TIR entry is also separate from this legacy
held challenge. The isolated native transition recipe preserves
these scopes.

The reproduced SmolLM2 fixture still requires 8,782,776 DAA
against 3,120. Its non-tokenizer diagnostic has 287,806 DAA
acquisition, 60,000 owner queue, 16 preparation and 36 other
Open/collection overhead, totaling 347,858. These values use the
original fixture EOF assumption and are independent of its live
registration and binding refusal.

Parallel prospective commits are `2086e5689` for tokenizer reuse
with a branch-checked verified EOF and a cold restart (five focused
tests passed), `f0ec44d50` for the bounded Studio all-root maximum
parameter/opening diagnostic (typecheck and real artifact run
pending), and `3687027c5` for bounded owner slots and explicit
parking without retiring response duties (four focused tests
passed). None is a live quote reduction. The all-root diagnostic
does not cover every response family, and the owner scheduler
still needs rooted lifecycle and filing integration.

The final builder and clock source `0fec7174b` passed the exact
38-unit metadata closure, including the Kaspad default+EVM test
crate and both Core feature sets. The first attempt found missing
session-proxy calls and a test import. The final source uses the
existing consensus worker and the bounded fixture import. The
retry took 210.04 seconds, used zero additional swap and released
the owned RAM volume. Six retained native PFS4 tests also passed,
including delegated payout, reorg and cross-version double spend.

The isolated quote diagnostics `db7cf3b3f` derives the optional
read-only JSON breakdown during the same native over-horizon
calculation. It preserves the refusal and existing binary
representations. The added tests have not run. Reorg work is
reported as quantities and the acquisition DAA still includes
its existing replay cost.

## Studio TFP1 registration and native quote follow-up

At DAA 19,660 two synced public T12 RPCs returned ADMISSION_OK for
the same unsigned SmolLM2-135M class registration under v10. Both
still reported that the class had no accepted registration, so the
unsigned claim quote stopped at the registry check. Both RPCs
reported the Studio bond Active at 19,661. The class registrar
signs the registration and, after it folds, the PALWART3 binding.
The unsigned registration, exact container and binding, class
semantics declaration, and native checks are pinned in
`/private/tmp/misaka-t12-oom-20261102/g14-smol-registration-preflight.GJLVP4/receipt.json`.
The class semantics fence remains unscheduled. An accepted signed
inventory with EOF is also required for the tokenizer route.

The historical Studio worker fixture names a synthetic executor bond.
A public attempt needs a new worker result for the real bond and
current anchor. The native fixture-EOF quote reproduced 8,782,776
DAA against 3,120. Its non-tokenizer subtotal was 347,858 DAA,
including 60,000 for owner queuing and 287,806 for TIR acquisition.
Four exact numerical leaves had parameter-opening sizes of 7,511,
7,511, 7,511 and 7,575 bytes. They are samples, and the all-root
run refused the native logical memory reservation before completing
the walk. See `/private/tmp/misaka-t12-oom-20261102/tfp1-exact-cover.Ov7I5J`.

The integration branch reports the over-horizon terms from the
actual native JSON quote and checks witness continuation identity
before copying the committed prefix once. Its prospective
tokenizer intake checks the accepted parent for an exact PALWART3
binding, class semantics and a signed inventory's accepted EOF.
Six focused tokenizer checks passed. The local preparation and
cache have no consensus pricing authority. A versioned native
record of validated tokenizer facts and independent retrieval of
rendering and EOG openings remain required. Claim-specific prompt,
output, constraint and stop checks remain part of the route.

The isolated owner bridge `80e3b1f5d` parks a runnable owner on
the checked acquisition demand and releases the processing slot
while its response clock remains in force. It requeues after
literal completion from all assigned custodians or native default.
Four standalone scheduler checks passed. The bridge still needs
rooted atomic state transitions and reorg/IBD reconstruction.

No carrier, signature or public deployment was produced by this
follow-up. The same-claim Panel response, Gate C, PFS4 block and
payout remain unobserved.

The 38-unit default and EVM metadata closure passed at `08a67143a`
in 207.12 seconds with no swap growth. This included the Core and
Kaspad test crates. It does not execute the native tests. The
initial closure caught five fixture calls missing the optional quote
diagnostics argument. The retry included those fixes.

The default runtime-pack check reached the 3,000,000 KiB RSS guard.
Streamed verification reached the first of five positions with peak
RSS 818,448 KiB before the 150-second guard. Registration now exposes
`--pack-stream` while preserving the required artifact and conformance
checks. The argument-parser test passed. A single read-only
streamed retry with a 1,000-second guard is recorded in
`/private/tmp/misaka-t12-oom-20261102/g14-smol-registration-preflight.GJLVP4/runtime-pack-long.log`.

The isolated all-root reservation fixes `b411f521e` and `bfb455ac7`
separately cover the maximum actual cloned node and the retained
range prelude, and report the bytes when native reservation refuses.
Native parity and the full SmolLM2 measurement are pending.

At 08:16 UTC Explorer REST and WebSocket agreed on the selected
tip at DAA 19,661. REST returned HTTP 200 with a current index.
This was a single synchronized sample. The last three selected
blocks were heartbeats and the serving relay counted no PFS4 spends.

## Batch Court clock and exact T12 recheck

The candidate opens the detection and actionable challenge window at
claim acceptance. It uses the RFC-0031 / ADR-0198 A predicate and
includes accepted DAA + 3,120.
An eligible, admitted case may collect through a separate deadline
bounded by the inclusive hard deadline at accepted DAA + 10,000.
The exclusive collection boundary can be one DAA later, with material
retention covering the collection window. A later batch cannot restart the case clock. This is a
prospective interpretation. The running TFP1 route still includes
acquisition and final filing in its existing 3,120 DAA admission ceiling.
An opening must have independently authenticated standing and actionable
grounds. The transport cannot establish a verdict or settlement credit.

The candidate gate adds the opening delay to measured service, with
independent wire, Node service and recovery terms. It conservatively
assumes no overlap between these terms. The example 9,000 DAA service
budget fits only with an opening by elapsed DAA 999 after the
T06 exclusive-collection margin. It would need 12,121 DAA at the
latest permissible opening. The 6,880 DAA residual must include the
1-DAA margin, leaving at most 6,879 DAA for the service terms. The new code refuses over-target and overflow.

The dormant batch progress contract caps ranges, part count and exact
payload. It accepts independently authenticated parts in canonical
slots, does not debit a duplicate again, and binds the checkpoint to the
case request and the accepted branch's expected checkpoint root. Missing
parts at timeout stay unverified. Four focused tests exercised the two
clock boundaries, conflicting or malformed data, the incomplete deadline,
reordered delivery, and resumption from a branch-authenticated checkpoint.
Transport queues, native opening verification, atomic archive/fold
persistence and claim-local Gate C and settlement remain separate.

Two paired selected-chain pages at DAA 19,662 show Qwen #72 to #75
referenced by selected mergesets and absent as claims on both nodes.
The four exact B0 journal lines show 20,606 DAA remaining against
20,608 required. The 20,000 retention fence remained unarmed on B0
and the serving relay. Both also agreed on 9 independently eligible,
0 independently ready, and 0 assigned seats in the class readiness
view. This was not a claim-specific draw. Raw evidence is under
`/private/tmp/misaka-t12-oom-20261102/g14-paired-tfp-followup.iuV43m`.

The integration branch includes the narrower native all-root workspace
estimate, typed refusal diagnostics and one-root-at-a-time parameter
sizing. The exact native SmolLM2 fixture was exported separately from
the historical worker result. The first guarded metadata run on the
narrow reservation commits stopped at the aggregate compiler RSS limit
while another compiler was active. The focused batch tests do not
validate the integrated native streaming run. No public deployment,
schedule change, claim submission or push to main was made.

## Native SmolLM2 range, arithmetic fault and batch recovery

The exact historical worker capture supplied a 2,019-byte StepRun
response for leaves 14,510–14,512. The native Source accepted it at
the original execution root and rejected a modified sibling. The honest
leaf 14,511 terminated with NoFaultFound. Its program-stripped verdict
payload was 18,064 bytes before the outer signature and carrier framing.
The exact class, artifact, worker and library hashes are recorded in
`/private/tmp/misaka-t12-oom-20261102/tfp1-c-native-capture.zeKljT/receipt.json`.

The isolated arithmetic test changed one in-range value at leaf 14,511
and regenerated its step root, execution root and opening. The data
authenticated at the rebuilt source and failed against the original
source. Native independent evaluation returned ComputationMismatch at
value index zero. The unmodified control still returned NoFaultFound.
This is one exact local relation and does not include a signed live
carrier, an independent drawn Panel, or the full branch budget.
See `/private/tmp/misaka-t12-oom-20261102/tfp1-c-native-mismatch.7dkhHJ/receipt.json`.

The Batch Court candidate advanced the exact honest 2,019-byte response
as one authenticated range and recorded two 1,024-byte slots, the second
one ragged. A native authentication rejection left the prior checkpoint
unchanged. The encoded 134-byte checkpoint restored against its expected
root and failed against a different root. The harness linked the cached
Studio verifier and the exact new Batch source. It did not test storage
atomicity in a running Node. Six focused tests also cover the classified
resource refusal, conflicting data, and unchanged state.
See `/private/tmp/misaka-t12-oom-20261102/tfp1-real-batch-checkpoint.5c7AEK/run.stdout`.

At DAA 19,667 B0 and relay agreed on the chain and both lacked Qwen #78
as a claim. B0 identified the exact retention refusal, 20,606 against
20,608. The subsequent class readiness view at 19,668 had nine
independently eligible seats and zero independently ready or assigned.
No public claim or deployment was made. The second guarded Core build
queue timed out waiting for existing rustc processes. It did not start
a compiler. Full all-root measurement and native owner replay remain open.

## Batch range authentication and public index advance

The Batch Court candidate now commits each range's native authentication
mode in the demand root. A whole-body source format rejects independent
part verification. Its checkpoint can hold either all verified slots or
none, and restore rejects a partial whole-body range even if the supplied
checkpoint digest matches. The native caller remains responsible for
deriving the mode from the accepted source and binding the archive to the
branch. Six focused tests passed, including codec and restart checks.

The exact 2,019-byte SmolLM2 StepRun was refused through the independent
part entry point. The unmodified body passed native whole-range
verification and restored both slots from its 134-byte checkpoint. A
modified sibling and a different branch checkpoint root were refused.
The harness linked the cached Studio Core for native source verification
and the current Batch source directly. It exercised local authentication
and checkpoint restoration. See
`/private/tmp/misaka-t12-oom-20261102/tfp1-real-batch-auth.fTSgtO/run.stdout`.

Two public Explorer reads showed the selected REST index and the serving
WebSocket advance together from DAA 19,670 to 19,671 on the pinned relay.
B0 and the relay agreed at the second tip. Qwen #81 was in the selected
mergeset, while the B0 fold refused its claim at 20,606 of 20,608 DAA
required retention. Both nodes lacked that claim. The class view had nine
independently eligible, zero independently ready and zero assigned seats.
The observations are under
`/private/tmp/misaka-t12-oom-20261102/g14-serial-core-live.NOaqnT`.

The serial LLVM retry reached 69% reported host memory availability with
zero swap growth, below its 70% guard. It stopped and detached its owned
RAM disk before linking the all-root fixture. The build receipt is
`/private/tmp/misaka-t12-oom-20261102/tfp1-stream-core-runtime-r4.vmql3jjq/result.json`.

## Batch delivery slots and native source measurement

The Batch candidate can project an exact demand into the existing causal
service scheduler. Each part becomes an independently eligible packet.
Whole-body verification depends on all parts of that range. The projection
preserves a hard response boundary and refuses a declared part cost below
its encoded body and inner header size. A full deadline, capacity, and
storage admission still depends on the native caller's profile and funded
service book. Subsequent adaptive turns and terminal filing require their
own reservations. The existing public material intake still uses the
serial accepted-offset cursor.

Using the exact 2,019-byte SmolLM2 StepRun and two raw Batch carriers
totalling 2,419 bytes, the illustrative service book placed both parts at
3,222 and the verification control at 3,223. Adding the old serial
dependency placed the second part at 3,223 and verification at 3,224
under the same illustrative capacity. This is a single local topology
comparison. Outer carrier bytes, actual per-DAA supply and the full Court
path remain unmeasured. The exact native opening and checkpoint checks
also passed. Thirteen focused Batch and causal service tests passed.
See `/private/tmp/misaka-t12-oom-20261102/tfp1-real-batch-reservation.CWbqvK/run.stdout`
and `/private/tmp/misaka-t12-oom-20261102/tfp1-batch-reservation.DLPc3z/focused-final.log`.

The exact Core metadata queue for the dynamic-input streaming change
expired before invoking the compiler. The last admission sample was
77% available memory with 8,281,407,488 bytes of other compiler RSS.
The all-root runtime and new native fixture test remain unverified.
At DAA 19,673 B0 and relay agreed on the selected chain. Qwen #82 was
referenced by the selected mergeset and rejected at 20,607 of 20,608 DAA
required retention. Both claim reads were absent. Raw evidence is in
`/private/tmp/misaka-t12-oom-20261102/g14-batch-service-live.zVWFZS`.

## Pipelined native material staging and tokenizer registration

Integration commit `7f104d648` adds a private version-2 material intake
adapter using the existing native draft, part and canonical material rows.
The signed version and draft namespace distinguish the new adapter. Each
draft has at most 256 canonical part offsets. The native adapter journals
valid parts in arbitrary arrival order, rejects conflicting duplicates,
checks branch-retained offsets and the received byte total on load, and
assembles only after all required parts arrive. Finish retains its assembly
memory reservation through the existing native source authentication and
atomic material write. The existing public dispatcher accepts version 1.
The version-2 adapter still needs a funded Batch coordinator, admitted
service slots and measured costs before use by the public dispatcher.

An isolated one-test harness exercised the exact draft and part geometry,
codec, orphan rejection, missing-part rejection and canonical reassembly
over a stand-in map with a bounded scratch stub. It did not exercise the
signed native fold or real branch carriage. The signed wide-leaf native
fixture for those paths is in the Core test source. The focused log is
`/private/tmp/misaka-t12-oom-20261102/tfp1-material-offset-focused.o2yTgA/result.log`.

Integration commit `62f634cad` brings in the tokenizer candidate from
`92ead1945`. The private writer derives the whole-table result from raw
bytes and the authenticated EOG opening, checking PALWART3, semantics and
accepted EOF on the exact parent. A versioned immutable record is installed
by the native writer and tied to the accepting point. Trusted journal
replay and carriage load check its branch context and registration digest.
The seven focused tests used a lightweight wrapper, with the native
replay and carriage fixture remaining a separate verification item. The
full generator run stopped at the existing `NumericalManifestV1` review
pin mismatch. Only the relevant bounded visitor changes were integrated.
The rooted registration is ready for a future authorized writer and quote
integration. The public quote still includes the full tokenizer path.
See `/private/tmp/misaka-t12-oom-20261102/tokenizer-rooted.8KHieL/test-final.log`.

Core test metadata compiled successfully at exact integration
`62f634cad985f922bb3679b41afe84214d92f59b` with default and pq-only
features. It covered the new native fixture source, the dynamic input
streaming change and both integrations. The run took 73.691 seconds,
peaked at 5,920,145,408 bytes of direct compiler RSS and detached its
owned RAM disk. This run emitted metadata and did not execute the native
fixtures. The guarded build receipt is
`/private/tmp/misaka-t12-oom-20261102/tfp1-rooted-batch-meta.siplgade/result.json`.

## Root-local TIR demand coverage and the control budget

The retained SmolLM2 worker capture has 15,252 leaves. The legacy query
universe contains 3,871,872 overlapping StepRun requests alone. Using the
new native bounded partitioner to cover the whole execution with disjoint
runs yields 119 demands and 15,430 raw parts at the native response
ceilings. The provisional service profile allocates one control per DAA,
so even this disjoint whole-source reservation exceeds the 6,879 DAA
remaining after the latest notice and the exclusive-boundary allowance.
This comparison covers raw parts for one copy. Authentication, Begin and
Finish, role multiplicity, unrelated obligations and recovery require
their own costs. The capacity is provisional and the source cover is a
specific disjoint partition, rather than a proof of a minimum across
possible redesigned codecs. See
`/private/tmp/misaka-t12-oom-20261102/tfp1-step-cover-capacity.CFjcnW/run.stdout`.

The integrated candidate `3ef2a2177` selects StepRun intervals from one
root's input cover. It merges repeated step indices, preserves gaps and
refuses to exceed the demand budget. The proof-of-geometry harness used a
257-leaf selection with a duplicate and a gap on the exact source. It
returned widths 1, 254 and 2. For the original honest cone at leaf 14,511,
the retained execution yields eight distinct step leaves, with eight
authenticated requests, eight actual raw parts and 16 upper-bound raw
parts. The native cone verdict is NoFaultFound. This sample leaves the
parameter, prompt and terminal costs outside its measured scope. The
harness links the cached Studio verifier and uses the extracted new
planner, with mirrored typed scratch arithmetic at the private API seam.
See `/private/tmp/misaka-t12-oom-20261102/tfp1-native-cone-batch.vesgS5/run.stdout`.

The worktree also integrates `dbb715eeb` for latest-notice reservation,
`bce856c03` for native run partitioning and `07cf09dd1` for rooted tokenizer
use in claim-specific stop replay. The raw tokenizer quote still needs a
versioned replacement with the accepted record and funded acquisition
and replay obligations. The all-root runtime queue for `f5f39b2dc`
expired before starting a compiler. Its last sample had 66% reported
memory availability and three other compiler processes. B0 and relay
agreed at DAA 19,677, with no new Qwen carrier since the prior cursor.

## Native StepRun projection and T12 selected-chain follow-up

The Batch planner projects root-local step intervals into canonical shared
material targets with the same identity as native intake. The Court
builder can now read a leaf and its opening from an authenticated
StepRun. The local candidate retains support for individual StepLeaf
units. RFC-0026 §11.3 specifies the projection, authentication, and
reservation boundaries.

The standalone SmolLM2 harness authenticated a 256-leaf interval as two
runs of 254 and 2 under the 256-part ceiling. It compared all 256
extracted leaf preimages and openings with the individual leaf material.
It reproduced the canonical range siblings across the split and refused
an absent leaf. The two runs used 16 and 1 actual parts, with native
reserved upper bounds of 256 and 3 parts. This harness links the cached
Studio verifier and uses the candidate projection and builder methods,
with mirrored scratch accounting at the private API seam. It does not
execute signed material intake or the full live Court.
See `/private/tmp/misaka-t12-oom-20261102/tfp1-step-run-builder.WATEkR/run.stdout`.

B0 and the relay agreed on the selected tip at DAA 19,678. The paired
read observed the Qwen carrier at DAA 19,676 referenced by the selected
fold at DAA 19,677. Its signed retention is 40,284 DAA and both
readers returned an absent live claim row. The exact producer journal
shows submission. No rejecting fold reason or accepting claim row was
observed. See
`/private/tmp/misaka-t12-oom-20261102/g14-run-builder-live.rLm3LS/page.json`
and its sibling `journal.txt`.

## Bounded native intake ledger computation

The first material Begin for a submitter computed its existing ledger
limit by visiting every source ordinal, even when submitting one
root-local StepRun. The candidate groups requests with identical native
byte ceilings. It accounts for the ragged last frontier separately at
each tree level and adds the same complete sequence of ID chunk bytes.
The existing parameter and prompt allowances are unchanged. This is a
CPU-path improvement, independent of the funded Batch service budget.

The retained SmolLM2 execution had 15,252 step leaves and 3,902,405
source ordinals. The exact grouped result from 294 representative bounds
and the legacy per-ordinal result were both 8,180,678,236,677 bytes.
The timings in one local sample were 12.2 microseconds and 1.91 seconds
for those two loops. The harness uses the candidate aggregation and
cached Studio Core native bounds. It does not time parameter cover,
ledger persistence, or the deployed fold.
See `/private/tmp/misaka-t12-oom-20261102/tfp1-exact-intake-budget.zzSidY/run.stdout`.

B0 and relay agreed at DAA 19,679. Both returned no claim row for the
Qwen carrier at DAA 19,677, referenced by the selected fold at DAA 19,678.
The producer journal confirms the submission. The exact fold refusal
was not observed. See
`/private/tmp/misaka-t12-oom-20261102/g14-intake-budget-live.pHse47/page.json`
and the sibling `journal.txt`.

## Signed Batch delivery and the original service book

The candidate projects one declared material body into Begin, parallel
part slots and a Finish that depends on every part. The exact Borsh
sizes include the ML-DSA signature for every control. The proposed
native adapter checks the exact action, reserved DAA, signed byte
budget and ownership of the packet by the original claim's rooted
service binding. The existing native fold still checks the signature,
persisted draft and material authentication. The branch projection does
not itself reserve the maximum native branch at original admission,
authenticate the demand root in the wire, or install an active
dispatcher.

The extracted Core scheduling and Batch delivery tests ran seven
checks. Under a two-control fixture profile the two parts use the same
DAA in either arrival order, with Begin and Finish on separate DAA.
The test rejects an underpriced envelope and wrong demand, signer,
draft and slot. The isolated harness does not run the full fold.
See `/private/tmp/misaka-t12-oom-20261102/tfp1-signed-batch-delivery.66FDlS/tests.log`.

B0 and relay agreed at DAA 19,680, with no new Qwen carrier since the
previous cursor. The StepRun integration metadata queue expired before
compilation at 68% reported memory availability, with other rustc
processes using about 8.6 GB RSS in total.

## Demand-bound signatures and resumable V3 drafts

The V3 candidate uses the accepted Batch request root in the ML-DSA
message for Begin, Part and Finish and in the draft identity. It
retains that root in the V3 draft while preserving the original shared
scope and the V1 and V2 identity formulas. A part re-signed under
another request still cannot attach to the prior draft. A completed
material row keeps the existing original-source authentication rules.

The focused harness ran eight checks, including real signatures over all
three actions, rejection of the same signatures under a second root
and a Borsh round trip of the retained V3 draft. Replacing the request
root under the restored draft key was rejected. The harness runs the
candidate code with cached Studio Core cryptographic and key types.
It does not run the native claim lifecycle or the public dispatcher.
See `/private/tmp/misaka-t12-oom-20261102/tfp1-demand-bound-v3.y9hVEj/tests.log`.

At DAA 19,680 B0 and relay agreed. The selected fold at DAA 19,680
references the new Qwen carrier at DAA 19,678. Signed retention is
40,286. The producer journal confirms submission and the paired RPC
read found no claim row. The exact fold refusal was not observed.
See `/private/tmp/misaka-t12-oom-20261102/g14-batch-bound-signature-live.ONTcBa/page.json`
and its sibling `journal.txt`.

## Batch V3 carrier storage fee allocation

The Batch candidate now accepts a positive storage charge only with a
debit matching the exact V3 Begin declaration, accepted demand root and
accepting point. Its price is the existing per-byte schedule. The
allocation uses the same checked transaction fee budget as other
obligations, has a fixed per-carrier purpose and cannot draw on escrow
principal. Legacy intake still has no debit source. The public route must
pass the native budget after the other mandatory allocations and exclude
the allocated amount from collectible fees. The accepted-demand
dispatcher and coinbase integration remain open.

Three focused checks ran against the candidate allocation and cached
native transaction/carrier code. They cover the exact paid carrier,
an insufficient remainder after another allocation, duplicate allocation
of the same carrier, substitution of the request, accepting point and
declaration bytes, and native fee conservation. The payload uses a
signature-width fixture and does not exercise the native fold's signature
verification. See
`/private/tmp/misaka-t12-oom-20261102/tfp1-batch-carrier-rent.BbvNSz/tests.log`.

B0 and relay agreed at DAA 19,681 with no new Qwen carrier since the
previous cursor. The preceding integration metadata queue expired before
starting a compiler at 63% reported memory availability.

## Accepted Batch range and collectible fee projection

The native delivery constructor now checks the canonical demand root,
the original's acceptance and material horizon, exact target and byte
length, whole-range authentication, native maximum, and part width. The
first delivery slot follows the demand's opening DAA. A service graph
or actual action outside the exclusive response window is refused.

The carrier fee projection now gives the same storage requirement,
withheld portion and collectible remainder to storage admission and the
future coinbase caller. The split preserves the fee left after senior
obligations. A short payment can yield no collectible remainder and
still refuses a storage debit. The live dispatcher and the coinbase
caller still require wiring under the Batch ruleset.

The extracted current demand, signature, reservation and storage code
passed 17 demand/scheduler checks and three carrier fee checks. See
`/private/tmp/misaka-t12-oom-20261102/tfp1-fee-split-demand.Dhi1th/tests-demand.log`
and `tests-fee.log` in the same directory. Full native fold validation
is separate.

B0 and relay agreed at DAA 19,682. The Qwen carrier at DAA 19,680 is
referenced by the selected fold at DAA 19,681 and has signed retention
40,288. The producer journal records submission, and paired reads
found no live claim row or exact fold refusal. See
`/private/tmp/misaka-t12-oom-20261102/g14-batch-fee-split-live.pdmCMN/page.json`
and its sibling `journal.txt`.
