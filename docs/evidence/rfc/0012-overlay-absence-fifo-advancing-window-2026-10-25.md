# RFC-0012/0008: bounded absence-cache FIFO eviction

Status: the coordinated consensus no-run build passed in **8m17s**. The actual cache tests passed **12/12 in 0.07s** (new advancing-window 2 plus existing 10), rewarded-epoch real-DB tests **5/5 in 0.07s**, and quality-pool/epoch real-DB tests **5/5 in 0.04s**. The correction changes cache eviction only; no consensus policy, history window, authority or cache limit changes.

The immutable Typed material pruning Run 4 reached a real native Final, then showed a substantial slowdown around the 16,384-entry overlay absence-cache boundary. A read-only macOS sample (`/tmp/misaka-rfc08-pruning-run4-sample-20261025.txt`) captured the virtual processor in `validator_reward_outputs_for_block → selected_chain_overlay_window → OverlayAbsentCache::read → RocksDB Get`. The pruner was waiting on its channel, and the DB log separately recorded completed compaction. The sample contains only two counted observations per thread; it is not a throughput profile or proof that this defect is the sole runtime cost. That pinned Run 4 did not establish physical pruning. After validating this FIFO fix and observing actual Run 5 startup, root intentionally stopped the verified old writer with SIGTERM; it was not changed in place. This controller interruption is not a natural test failure, PASS or OS-crash campaign.

The source defect is exact: on a full miss, `swap_remove_index(0)` moves the final/newest cache entry to index zero, and insertion appends the next key. The next miss removes the entry just moved there. For capacity four: `[0,1,2,3] → read4:[3,1,2,4] → read5:[4,1,2,5] → read6:[5,1,2,6]`. Stale middle entries remain indefinitely while recent window entries churn. Cache hits do not reorder entries. This defeats the documented moving-window cache accelerator after initial fill.

The bounded correction uses `shift_remove_index(0)` for FIFO eviction. It retains the same 16,384 maximum and storage representation, adds no key queue or new allocation, and preserves all KeyNotFound-only caching, mutation generation, pending-batch and read-fault behavior. It performs O(limit) metadata movement on eviction; no new constant-time or physical-cost claim is made.

The new isolated regression leaf drives actual cache reads against a counted missing-row backend. A newest-to-oldest recent-window walk advances past three small-cache capacity turnovers and must incur exactly one backend read per new head; a second case uses the actual production maximum. Both assert bounded stored rows and eventual removal of every obsolete prefix. Existing cache sibling, pending/batch and read-fault regressions passed with the correction. These are cache mechanism tests, not a node/campaign performance result, whole memory bound, physical-pruning result or pruned-IBD acceptance.


## Executed records

The same pinned consensus binary supplied all three runtime groups:

- Binary `/tmp/misaka-rfc-fifo-staged-pruning-consensus-run5-20261025`.
- SHA-256 `07f2643ec3f99cac9530c13223defcace1af79705776d66f8e225453392372e7`.
- Build `/tmp/misaka-rfc-fifo-staged-classifier-consensus-build-20261025.log`.
- Cache `/tmp/misaka-rfc-fifo-native-fifo-tests-20261025.log`.
- Rewarded epoch `/tmp/misaka-rfc-fifo-native-rewarded-tests-20261025.log`.
- Quality/epoch `/tmp/misaka-rfc-fifo-native-quality-tests-20261025.log`.

The unchanged-depth native Typed pruning Run 5 was launched from that binary. Its own live artifacts, limits and partial progress are in [the pruning record](0008-typed-native-pruning-2026-10-25.md). Neither the focused cache results nor the debug-build runtime establish a cost campaign or completion of physical pruning/IBD.

The tested FIFO source and regression were committed as `f73f1d549`.
