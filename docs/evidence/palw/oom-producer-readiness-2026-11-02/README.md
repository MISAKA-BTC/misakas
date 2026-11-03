# Producer local context readiness — 2026-11-02

Based on the isolated retention branch after `7cf9ffe66`. This patch changes only local preparation/publication of **new Attempts**. It does not move a sink, change fork choice, restore withdrawn activation 18,850, alter the 20,000 retention fence or waive Gate C.

## Finding

The worker already checks IBD and `FlowContext::should_mine`, including peers and the shared chain-participation gate. `PartitionWatch` requires an advancing, fully validated refusal run and a strict majority of at least two long-lived outbound peers relaying only the other branch. A single refused heavy/junk candidate is deliberately insufficient.

The consensus template builder already prepares a checked selected-parent state, verifies import metadata and runs the required selected-source bootstrap preflight. However, the mining manager can return a cached template (`mining/src/manager.rs::get_block_template`). `produce_one` did not compare that template with its earlier `PalwProducerFactsV2.chain_point/daa_score`, or run a general context check again after backend/resource preparation. Native routing had its own optional freshness checks; the non-routing path did not have a general import-generation publication guard.

## Change

`ConsensusApi::palw_producer_context_v1(parent, candidate_daa, committed_root)` is a local read-only API, with no wire/RPC representation. It:

1. Captures the actual virtual snapshot and requires its selected parent and candidate clock to match.
2. Uses the existing strict checked parent-state reader and the original import key. It does not invoke the legacy template fallback when history is absent.
3. Requires the template's committed root to match the checked native state and runs the existing bootstrap preflight, including the exact required own selected-chain history.
4. Rechecks virtual Arc identity and import/state metadata. Cold loading and bootstrap preparation run outside the virtual guard; only the final fixed-size metadata checks hold that guard.
5. Returns the observed import epoch.

The producer compares facts/template view and checks this context before backend preparation, then again immediately before inference with the original epoch required. The read runs in a blocking worker so it does not occupy the async runtime that supplies Court/material duties. It also verifies that the observed consensus instance remains active and uses the existing participation gate; no new single-refusal stopping condition is introduced.

After inference and immediately before final block submission (including after custody/witness/drill delays), publication requires the same active consensus instance, an unchanged stable import epoch, and open participation. Publication deliberately permits ordinary selected-parent advancement during a long model draw. Requiring the original parent to remain the current sink at publication would discard valid delayed work repeatedly. Existing routing/custody checks and full block validation remain in place.

The new holds return from `produce_one`; they do not shut down services, clear retained material, stop Court responders or change existing claims. The receipt/redemption lane still runs earlier in the worker. No empty-history substitution, resync/datadir operation or fork-choice change occurs here.

## Validation

Executed: [readiness-harness.py](readiness-harness.py), **2/2 passed**; [output with actual production-function hashes](readiness-harness.log). It extracts the actual readiness/publication decision functions and their actual test source. Only Hash64 is replaced by a small local value type. It verifies parent/clock mismatch, missing bootstrap/state/import, changed epoch, replaced consensus, participation hold, no authorized execution on refusal, and recovery without a sticky hold. Publication checks retain the distinction between parent progress and import replacement.

Added, **not executed here** because the parent reserves central Cargo resources:

- `producer_context_requires_exact_parent_state_clock_and_stable_import`: actual Consensus API over TestConsensus/native storage. Covers correct context and repeat read, wrong selected parent, wrong clock, wrong committed root, deleted own delta/state, restoration/recovery and active import.
- Normal Node tests `producer_context_hold_does_not_authorize_new_execution` and `producer_publication_allows_parent_progress_but_rejects_replaced_import` with the real Hash64 type.

Suggested central filters: `cargo test --locked -p kaspa-consensus --lib producer_context_requires_exact_parent_state_clock_and_stable_import` and `cargo test --locked -p kaspad --lib producer_context_tests`. Existing template-parent/bootstrap and `partition_watch_front_runner` tests should accompany those checks. No new full Node build, remote command or deployment was run by this subtask.

## Operational limit

A coherent local view is a necessary preparation condition, not evidence that the node follows the public branch. The existing stable-outbound-peer watchdog still decides that question under its stated connectivity assumptions. A coherent stranded branch without sufficient watchdog observations can still satisfy the local context check; this patch does not claim to resynchronize B0 or turn a single `DominanceViolation` into authority to stop or switch branches. Parent-owned network observation/resync remains separate. Live Court progress and end-to-end model acceptance/finality remain deployment validation.
