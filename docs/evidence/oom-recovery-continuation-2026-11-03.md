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
