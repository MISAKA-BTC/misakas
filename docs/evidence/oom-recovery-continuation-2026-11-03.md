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
