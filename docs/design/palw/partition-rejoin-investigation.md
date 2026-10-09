# Partition rejoin — investigation record (LIVE-R1, 2026-10-08)

Branch `fix/partition-rejoin` (on integration HEAD `42452229e`). Hotfix branch `hotfix/int-12-ibd-candidates-lock` (`57235a5b3`, on
the live release `rcore/int-12` @ `0b1c11b87`; the deadlock fix and its test only). Evidence: H1's devnet r1, a salted testnet-12
drill (kaspad `89ffb1fb7`, sha256 `6e9b1936…`), `wh-h1-run/runs/20261008-smollm2-1.7b-r1/reorg/` and `wh-h1-run/devnet-r1/{A,B,D6}`.

A 41-minute partition (18:47–19:28): minority {B, D6}, majority {A, C, D1–D5}. After the rejoin (19:31) B refused every heavier
majority candidate on every resolve, A never fetched B's branch, B's class row went inconsistent, and D6 froze at 0 % CPU.

| # | Finding | Kind | In int-12 / live t12 | Fix |
|---|---|---|---|---|
| 1 | D6: every tokio worker parked on `ibd_candidates` (recursive read of a parking_lot `RwLock`) | node-local bug | **yes** (`flow.rs:886`, since `53a723311`) | one read — node-only; hotfix branch |
| 2 | B: wedged by the PALW deep-reorg rule, which disagrees with the blue-work sink heap | consensus design gap (liveness) | **yes**, same code; effectively permanent on t12 | N1 diagnosis (done), N2 watchdog, verified resync (node-only); C1–C3 to the ADR study (FINX) |
| 3 | A has no header for B's branch | designed relay heuristic | same | none needed |
| 4 | B's class row "reverted" while B kept its branch | node-local reader bug | yes | N3: readers answer at the sink |

## 1. D6 — the deadlock

**Symbols.** Rebuilt `89ffb1fb7` in a worktree with H1's exact environment (`CARGO_PROFILE_RELEASE_LTO=off`, `--bin kaspad --bin misaka
--bin palw-class`, rustc 1.93.0) plus `RUSTFLAGS=-Cstrip=none`. `__text` is 53,784,592 bytes in both binaries, so the sample offsets
map one to one (`atos -l 0x100000000`). Both samples (20:07 and 20:20) are identical. The unstripped binary and the symbol map are
kept in `~/Downloads/MISAKA-wt-b/live-r1-artifacts/` for any later sample of a `89ffb1fb7` node.

**What is blocked.** Not the consensus: the header, body, virtual and pruning processors all sit in `crossbeam_channel::Receiver::recv`
(idle). The P2P flows on all ten tokio workers are parked on ONE lock, `FlowContext::ibd_candidates` (`parking_lot::RwLock`):

```text
1 worker   IbdFlow::start -> consider_post_ibd_switch -> RwLock::read -> lock_shared_slow      (holds a read, asks for a second)
7 workers  IbdFlow::start -> serve_pending_nomination -> RwLock::read -> lock_shared_slow
2 workers  HandleRelayInvsFlow::start_impl -> FlowContext::expire_stale_verifications -> write
           (one in lock_exclusive_slow -> wait_for_readers: holds WRITER_BIT, waits for the first read; one queued)
```

**Root cause** — not a lock-order inversion between two locks, not a channel waiting on itself: a reentrant read of one non-reentrant
lock behind a queued writer.

```rust
let claimed_tip_work = match self.ctx.ibd_candidates().read().get(&id).map(|c| c.validation) {
    Some(CandidateValidation::ProofValidated { .. }) => { self.ctx.ibd_candidates().read().get(&id)... }  // second read
```

A `match` scrutinee's temporaries live to the end of the match, so the first guard is alive in the arm. A writer arriving between the
two reads sets the writer bit and waits for the first guard; the second read parks behind the writer. Every tokio worker that touches
the registry afterwards parks too: RPC, P2P and the rule engine stop ("Chain participation held … 143s floor remaining" at 19:34:38 is
simply the rule engine's last tick). The trigger is the rejoin path: `consider_post_ibd_switch` runs only in candidate review
(participation closed, no IBD), after a peer's chain candidate validated with a pruning proof — D6 at 19:34:03.

**int-12.** The same lines at `0b1c11b87`, `89ffb1fb7` and `42452229e`; introduced by `53a723311` (2026-08-09). Live t12 can deadlock
this way on a restart or a partition rejoin.

**Fix (node-only, no params id moves).** `IbdCandidateRegistry::proof_validated_claimed_tip_work`, answered under one guard taken
and dropped in one statement. A scan of the whole tree for a lock guard held in a `match` / `if let` / `while let` / `for`
scrutinee and re-taken in the body finds no other instance (HEAD and int-12); the 12 other scrutinee-held guards in
flows/p2p/mining/rpc were reviewed by hand (none re-takes its lock).

**Tests** (`protocol/flows/src/flowcontext/ibd_candidates.rs`, `live_r1_reentrant_read_tests`): deterministic — the writer is queued
(its bit observed) between the two reads; the old shape's second read cannot be granted; the accessor answers under the guard a queued
writer waits on. Hotfix branch: the same three tests pass, `kaspad` builds, and `scripts/t12-repin.sh --drift-only` reports the ids.

## 2. B — the wedge

### 2.1 What refuses, and why it is not DNS

`dns_reorg_outcome` sends a NON-extension candidate on a V2 network to its V2 arm before any DNS rule (the DNS-retired arm the report
suspected handles extensions only). Past `palw_reorg_strict_economic_win` (t12: DAA 750; drill: 6) the arm refuses unless the challenger
STRICTLY wins on `(safe frontier, safe weight, live total)`; an all-economic tie is GHOSTDAG's only within
`PALW_REORG_SHALLOW_TIE_DAA_V1` = 2 ticks. The arm answers `DominanceViolation` — the DNS word — for a strict loss, a deep tie and an
unweighable candidate alike, which is why the log read like DNS. No "cannot be weighed" line appeared in B's log, so both sides were
weighed and the majority lost.

The numbers (each node's `capacity-shadow` line, its own tip's state; nothing `Final` on either side at DAA ≤ 101):

```text
                     DAA 70 (fork)   after
minority B / D6      immature 65     92 (DAA 78) .. 93 (DAA 89)     licence queue 28 -> 1, claims 126 -> 128
majority A / C       immature 65     67 (DAA 80) .. 69 (DAA 100)    licence queue 28 -> 45 -> 53, claims 126 -> 197
```

Past F-W (`palw_capacity_weight_cap`; t12: DAA 1,700; drill: 14) a claim's live weight is its STAGED weight — Created 0, Anchored 10‰,
Licensed 1000‰ — capped per bond. The minority licensed its share of the pre-fork backlog (receipts its own two seats signed); the
majority's panels lacked those seats and its queue grew. So the majority's many fresh attempts bought blue work (2²⁰ each) and no
economic weight: the minority wins strictly on `live`.

### 2.2 Why nobody converges: two orders

* The sink search pops candidates by GHOSTDAG's key (blue work). On B the majority's tip is on top; the V2 arm refuses it; B settles on
  its own tip — on every resolve.
* On A the heap pops A's own heavier tip, an EXTENSION, which the V2 arm is never asked about; A never weighs B's branch. On the wire A
  does not even fetch it (§3).

ADR-0042 Decision 9 requires one comparator at every chain-selection site, the virtual tip included. The heap orders by blue work and
the gate can only veto, so two honest nodes holding one DAG keep two sinks whenever the two orders disagree across a fork deeper than
the shallow window. Reproduced in-process:
`hb_fork_choice_probe::partition_rejoin::live_r1_a_minority_holding_a_licence_refuses_the_heavier_majority_after_the_heal`
(strict-win + lane A + F-W, as t12 runs past DAA 1,700: card 2's claim bound before the fork, the licence carried only by the minority,
the majority minting two attempts and racing two heartbeat producers every slot; after the heal and three more exchanged rounds,
two sinks over one DAG).

### 2.3 What the design says, and the thresholds

* No ADR states an operator-resync depth or rule for this case. Strict-win's documentation states the intent: a strict economic loss is
  always refused, a tie deeper than 2 DAA ticks keeps the incumbent — against a private branch's blue-work pile. On t12 that pile is
  cheap (an attempt header carries 2²⁰ at signature cost: hb_probe verdict 1, `palw_pruning_proof_strict_economic_win`'s doc), so no
  blue-work escape is safe.
* (a) **A majority `Final` heals it in principle** (the frontier is key 1). On t12 the windows are `PALW_RC_WINDOWS_V1` (bind 600,
  receipt 600, challenge 1,200, court 3,000, anchor_delay 20): the first `Final` is ≥ ~1,220 DAA after a claim's acceptance, ~41 h at
  120 s/DAA (the drill: ~30 h at ~88 s/DAA), and the frontier advances only inside the resolved prefix (below the oldest open claim).
  Live-weight overtaking needs the majority's licences to exceed the minority's; capped per bond it may only tie, and deep ties keep the
  incumbent.
* (b) **It can be — and on t12 typically is — permanent.** The minority keeps minting heartbeats (B: 273 blocks for DAA 0..90, ~2–3 a
  tick). Once its own chain is `finality_depth` = 360 blue score (~120–180 DAA, ~4–6 h) above the fork, every majority candidate
  fails `candidate_at_or_above_finality` ("Finality Violation Detected") and the PALW gate is never asked again. 4–6 h beats 41 h.
  After that only a resync from an EMPTY data directory recovers it: an IBD onto the existing one is refused by
  `palw_pruning_proof_strict_economic_win` (t12: DAA 750), because the incumbent is economically ahead.
* `merge_depth` = 30 blue score and `finality_depth` = 360 (t12's 120 s blockrate). The 28-DAA partition was inside both bounds that
  matter for kaspa's own finality — the PALW rule alone kept B off.

### 2.4 Options (C1–C3 are for the ADR study, agent FINX; none is implemented)

| Option | Kind | Outcome in r1-like cases | Cost / risk |
|---|---|---|---|
| **N1** diagnosis: the wedge warning names the PALW rule, the verdict (strict loss / deep tie / unweighable) and both sides' keys | node-local, **done** | operators see the real cause | none |
| **N2** partition watchdog + hold (§2.5) | node-local, **done** | the minority stops extending its branch, so its finality point freezes and the split stays healable; it does not heal by itself | a false hold takes one producer offline; Sybil-resistant trigger |
| **N3** `palw_*` readers answer at the sink (§4) | node-local, **done** | RPC shows the node's own chain | one walk per sink change |
| **verified resync** `kaspad --palw-verified-resync` (§5) | node-local, **done** | the operator's remedy; old datadir kept | none automatic |
| **C1** Decision-9 symmetry: the virtual tip is the V2-max over all valid tips, and relay/IBD fetch branches below the merge-depth root | consensus (fork-choice; fence + fingerprint) | heals without trusting blue work; the network adopts the economically heavier branch — in r1 the 2-node MINORITY's | a heavier node must fetch and weigh lighter branches (DoS surface); "more licences beats more nodes" is a policy decision |
| **C2** DNS-style work override (out-work by 4× since the fork) | consensus | heals r1 | **unsafe on t12**: blue work is forgeable; reopens what strict-win and `capacity_probe_w_t6*` pin |
| **C3** exclude "portable" stagings (licences of pre-fork-bound claims either branch could carry) from deep-reorg comparisons | consensus | removes r1's accidental lead | leaves deep ties, which keep the incumbent — does not heal alone |
| status quo + operational rule + N2 | — | wedge, sealed in ~4–6 h unless held/resynced | operator action per partition |

Alternatives FINX should also weigh: freezing the finality point while the PALW comparator refuses the chain the node's peers are on;
a bounded economic-majority rule; a resync procedure the protocol recognises (checkpointed).

### 2.5 N2 — the partition watchdog: threat model and trigger

Code: `protocol/flows/src/flowcontext/partition_watch.rs` (decision), `FlowContext::observe_relay_for_partition` (wiring, after every
relayed block is processed), `ChainParticipationGate::{set_partition_hold, partition_hold, chain_settled}` (the hold), consensus half
`VirtualStateProcessor::palw_partition_refusal_v1` / `ConsensusApi::palw_partition_refusal_v1`.

The hold stops mining, attesting and `is_synced` (the heartbeat lane, producers and the validator all ask `allows_participation`). It
never moves the sink, it is never persisted, and the IBD / candidate-recovery paths ask `chain_settled` (participation without the
hold), so a hold does not restart chain recovery.

Hold requires ALL of:

1. **A validated, weighed refusal run.** The last resolves settled after refusing a heavier candidate that the sink search
   UTXO-validated (bodies, PALW fold) and weighed on both sides, and that did not strictly out-weigh the sink — never a header-only or
   unweighable chain (`note_palw_refusal_streak`).
2. **Persistence:** the refused branch advanced ≥ `PARTITION_MIN_REFUSED_SPAN_DAA` = 10 ticks while refused (the network's clock moving
   on without this node, not one release).
3. **The node's own branch is the one losing connectivity:** at least `PARTITION_MIN_OUTBOUND_ON_OTHER` = 2 OUTBOUND peers connected ≥
   `PARTITION_MIN_PEER_AGE` = 10 min are "on the other chain", and they are a strict majority of ALL such peers (a silent peer counts
   against). "On the other chain" is measured, not claimed: within `PARTITION_OBSERVATION_WINDOW` = 30 min the peer relayed blocks
   outside the future of this node's sink and none inside it.

Release: the moment any of it stops being true (evaluated on every relayed block).

Threats, and why each does not hold an honest node:

* **Forged blue work.** A released heavy junk branch makes condition 1 (and 2, if the attacker keeps extending) true on EVERY honest
  node — the consensus half is deliberately not enough.
* **Sybil peers** relaying only the junk branch: inbound connections are in neither the numerator nor the denominator.
* **Honest outbound peers relay the junk too** (it is valid), but they also relay this chain's new blocks, so each counts as on OUR
  chain.
* **Fresh outbound connections** (an address the attacker planted and got dialled; a node just restarted): younger than 10 min, not
  counted.
* **Eclipse** (the attacker is most of the node's long-lived outbound peers): it already controls what the node hears; the hold is the
  safe failure (stop, never follow) and reverses when honest outbound peers are back in the majority.

Tests: `partition_watch::tests` — a genuine minority holds and releases on its own (refusal ends / observations go stale); a Sybil
flood of 200 inbound peers relaying a heavy junk branch does NOT hold; young outbound peers do not count; one peer, or a silent majority,
is not enough. Consensus half: the partition test asserts the minority records the run (the majority none) and that the refused branch
advanced while refused. Gate: `the_partition_hold_is_reversible_and_orthogonal_to_the_state`.

### 2.6 Runbook (until the ADR decides)

A partitioned t12 node that logs the PALW refusal (or `PARTITION HOLD`) against its peers: stop it, resync from an empty data directory
— `kaspad --palw-verified-resync` — before ~360 blue score of its own branch (~4 h of heartbeats); after that the split is sealed by
finality anyway. Do not IBD onto the existing data directory (refused by `palw_pruning_proof_strict_economic_win`).

## 3. A's "cannot find header"

Consequence of B's state, not a sync bug. A's relay flow skips a relayed block whose blue work is at or below the virtual's merge-depth
root (`protocol/flows/src/v7/blockrelay/flow.rs`, `blue_work_threshold`) — such a block can never be merged (bounded merge depth) or
selected (lighter). B's branch forked at DAA 69; by the rejoin A's merge-depth root (30 blue score below its sink) was above the fork,
so A never requested B's blocks — not even `9c2eb760…`. Under the current fork choice A has no use for them; under C1 it would.

## 4. Registry

* **The fold reverts exactly.** The class registry is part of `PalwChainStateV2`; every block's delta records `old`/`new` per key and
  `revert_delta_v2` verifies each value it replaces. When B converges, the registration folded at DAA 75 is reverted exactly.
* **It re-folds only if the carrier is mined again.** The mempool does not re-insert transactions of reorged-out chain blocks
  (`mining/src/manager.rs` has no reinsertion), and B's blocks are below the majority's merge-depth root, so they are never merged: the
  carrier must be resubmitted (H1's carrier status on A: `submitted`, never seen by A).
* **What H1 saw** (B's class row empty at 20:08 while B still held its branch) was a reader bug, not a revert: the `palw_*` readers read
  the PALW tip ROW, and the sink search rewrites that row wherever each walk ends — on a wedged node, on the refused branch most of the
  time (B's template warnings: "tip stood at X while the template selected parent is Y; re-derived … over 1–100 reverted and 16–73
  applied deltas"). **N3:** `VirtualStateProcessor::palw_v2_reader_state` answers at the committed sink (from `lkg_virtual_state`, no
  virtual lock — template building and the mempool call the readers under a held virtual read guard), walking from the tip row when it
  stands elsewhere, cached by sink. All 59 readers in `processor.rs` and `consensus/mod.rs` go through it; `palw_v2_state_at`
  (templates) keeps its own walk. Test: the partition test moves the tip row onto the refused branch (as mid-search) and shows the row
  says `PanelBound` while the readers answer `ReceiptLicensed` at the node's own sink.

## 5. Verified resync — `kaspad --palw-verified-resync`

`kaspad/src/palw_verified_resync.rs` + `FlowContext::begin_verified_resync`. Node-local; no consensus change.

1. The data directory is moved aside to `datadir.pre-resync-<unix ms>` (never deleted; a failure refuses to start) and
   `palw-resync-report.json` is written beside it (`status: syncing`, where the old one went).
2. The node syncs from empty by the ordinary IBD: the pruning proof validated, every body after it validated, the PALW fold computed
   locally.
3. It stays held until ≥ `RESYNC_MIN_CONFIRMING_PEERS` = 2 long-lived outbound peers relay blocks extending the synced chain, they are a
   strict majority of the long-lived outbound peers, and the synced chain refuses nothing heavier on the PALW rule. Then the report is
   rewritten (`status: confirmed`, sink, DAA, peer counts) and the node participates.

It is never started by the node, so nothing an attacker sends starts it. An eclipsed node handed the attacker's branch by its IBD peer
stays held (its long-lived outbound peers relay another chain; inbound peers confirm nothing) with its old data directory beside it.
Recommended: run it with `--connect` / `--addpeer` to operator-chosen hosts. Tests: `partition_watch::tests::
a_resync_is_confirmed_only_by_long_lived_outbound_peers_on_its_chain`, `palw_verified_resync::tests`.

## 6. Reproduce

```text
CARGO_INCREMENTAL=0 cargo test -p kaspa-p2p-flows -p kaspa-core -p kaspa-consensus -p kaspad --lib -- \
  live_r1 partition_watch partition_hold partition_rejoin
```
