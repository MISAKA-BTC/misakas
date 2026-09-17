# ADR-0128 — DNS validators vote BFT by bonded stake, and that vote decides the stake reorg gate

* Status: **ACCEPTED and IMPLEMENTED 2026-09-17** on `feat/palw-exec-lane-and-validator-retirement`
  (§8). testnet-11 arms it at DAA 6,001 with ADR-0124, ADR-0125, ADR-0126 (revised) and ADR-0127 (the
  operator's choice, §5).
* Operator's direction, in the operator's words: "DNS validator の投票力を単純な bond 額に戻して
  inactivity leak は再実装して — 必要な過去 attestation 履歴を正しく取得できず『実装されたままでは有効化
  できない』というテストコメントがあるため、正しく取得して正しく有効化できるようにして — また DNS
  validator の投票を BFT にして DNS stake reorg gate を決める設計にして — また VLT 重み付けはもう使用
  しなく DNS stake reorg gate のみ BFT を使う"; "DNS stake reorg gate は設計上残す"; the leak's
  silence is "DAA の 7 日", counted with the 1-BPS lane in mind.
* Builds on: [0009](0009-dns-probabilistic-finality.md) (the overlay, its epochs and its gate),
  [0013](0013-validator-reward-distribution.md) (evidence and slashing), [0018](0018-quality-gated-stakescore-inclusion-economics.md)
  (the QualityFloor credit, which keeps paying), [0065](0065-a-bond-must-be-earned-and-a-seat-must-be-someone-else.md) D2 and
  [0042](0042-palw-mainnet-candidate-ruleset.md) Decision 9 (the PALW authority, which keeps deciding
  among the candidates this gate lets through), [0066](0066-the-heartbeat-lane-out-of-header-bits-and-a-committed-liveness-table.md) Decision 4
  and SA-2 (the leak's two numbers, whose earlier implementation this replaces), [0126](0126-the-validator-carve-drops-to-a-fifth-and-the-stake-reorg-gate-stays.md)
  (the overlay stays and is paid 20 %).
* Supersedes: the VLT voting weight (deleted 2026-09-17, `4568b082`) as the BFT round's denominator,
  and the window-bound inactivity leak it carried.

## 0. The sentence this ADR is

**Past its height, an epoch's canonical anchor is DNS-final when validators holding more than two
thirds of the epoch's bonded stake — less the stake of validators silent for seven days of DAA —
have both attested to it and precommitted to it under a lock the chain can check; and the stake
reorg gate refuses every sink that abandons the last DNS-final anchor, until that anchor goes stale.**
Voting power is the bond, nothing else. The gate is a veto: it never selects a tip, never makes a
PALW claim `Final`, never moves a safe frontier, and never decides whether a block is valid.

## 1. What exists

* **The gate is weaker on a PALW chain than its name.** On a `ConsensusV2` network
  `dns_reorg_outcome` answers a REORG candidate with the PALW comparator and returns before the DNS
  half runs; only a candidate that extends the sink meets the confirmed-anchor check. The stake
  preference (`dns_stake_preferred_tip`) still proposes a live-overlay tip when this chain's overlay
  is dead, and whatever it proposes must still win the PALW comparator.
* **Confirmation is a depth, not a vote.** `last_dns_confirmed_anchor` advances when the
  QualityFloor stake score and the work depth both clear their thresholds; no quorum of signers is
  named, so two branches can each confirm an anchor without anyone having signed both.
* **The two-round vote existed and was weighted by compute.** Attestations were round one,
  `StakePrecommitPayload` round two, with a declared lock (`locked_epoch`, `locked_hash`) the chain
  checks link by link and two-payload evidence (`precommit_fault`: `Equivocation`,
  `ContradictoryLock`). The round's denominator was the VLT voting weight; that weight never activated
  on any network and was deleted. Precommit admission and precommit-evidence slashing stayed in
  consensus.
* **The leak could not be armed**, and its own test said why
  (`the_leak_cannot_be_fed_the_evidence_its_meaning_needs`, deleted with it): its evidence — each
  validator's last attestation — was read from the StakeScore window (30 blue score on
  testnet-11, fifteen of its two-block epochs), so a validator seen once inside the window looked at most a window stale however long
  it had really been silent, and a validator absent from the window looked infinitely stale however
  recently it had attested. The provenance probe that followed made a node that could not cover the
  window leak nobody, which kept the rule inert and still left two nodes able to disagree.

## 2. Decisions

**Decision 1 — voting power is bonded stake.** A vote's weight is the `amount` of the bond it is cast
under, counted once per `(validator_id, bond_outpoint)`, for a bond that is `Active` at the epoch's
canonical anchor DAA (`is_bond_active_at`) — the set `total_active_stake_by_epoch` already sums. No
compute, no decay, no frozen VLT snapshot.

**Decision 2 — two rounds, one denominator.** For epoch `E` with canonical lagged anchor `A_E` at DAA
`a_E`, let `S(E)` be the counted set (Decision 3) and `W(E)` its total stake.
* Round one: `P(E)` = stake in `S(E)` with an attestation for `(E, A_E)` accepted on this chain and
  admissible under today's rules (zero validator-set commitment, signature, bond binding).
* Round two: `C(E)` = stake in `S(E)` with a precommit for `(E, A_E)` accepted on this chain whose
  declared lock is the one this chain shows for that bond (`lock_consistent_precommits`: the first
  counted precommit declares no lock, each later one declares its predecessor's `(epoch, target)`,
  and a misdeclaration stops that bond's count at that point) and whose `snapshot_commitment` equals
  the commitment of `(E, A_E, S(E))` (Decision 4). A precommit counts only for an epoch whose round
  one reached quorum on this chain.
* Quorum is strict: `3·P(E) > 2·W(E)` and `3·C(E) > 2·W(E)`, with `W(E) > 0`.
* `A_E` is **DNS-final** on a chain when both hold. Round one alone is never final.

**Decision 3 — the counted set leaks the silent, and the evidence covers the silence.** `S(E)` is the
bonds `Active` at `a_E`, less those leaked at `E`:
* A bond is **leaked** at `E` when `a_E − last_E(bond) ≥ t_leak_daa`. `last_E(bond)` is the anchor DAA
  of the youngest epoch the bond attested in an attestation **accepted by a chain block at or below
  `a_E`** whose anchor is at least `reentry_final_depth_daa` below `a_E`; where the bond has no such
  attestation in the evidence window, it is the later of the bond's activation DAA and the window's
  lower edge. Every input is in the chain prefix that ends at `a_E`, so an epoch's counted set is one
  answer on every sink that shares that prefix — a restart, a reorg that keeps the anchor, and a
  node that evaluated the epoch when it was new all agree. Re-entry waits for an attestation that is
  itself buried (ADR-0066 SA-2), and a bond younger than the leak period is never leaked for a silence
  it could not have broken.
* **The evidence window is the leak's own**: for epoch `E`, the selected chain from `a_E` back over
  `t_leak_daa + reentry_final_depth_daa + attestation_epoch_length_blue_score +
  attestation_lag_blue_score` of blue score, walked for accepted attestations exactly as the credit
  walk reads them — not the StakeScore window. Absence inside a window at least `t_leak_daa` long is
  silence of at least `t_leak_daa`, so the lower-edge fallback is exact rather than a guess. A sink
  evaluates the epochs of its StakeScore window, so its walk spans that window plus the leak window.
* **Every synced node holds that walk.** `validate_palw_v2` refuses a network whose pruning depth is
  below `stake_score_window_blue_score` plus the leak window, so the pruning point never passes the
  evidence a sink's leak reads. On testnet-11 (two-block epochs, lag 2, StakeScore window 30) the walk
  is 30 + 5,040 + 200 + 2 + 2 = 5,274 blue score and the derived pruning depth is 12,002 (the claim
  lattice with the DA court), both measured in the fence's refusal test. A node whose walk nonetheless cannot
  cover it (a store that will not read, a sync still in progress) does not compute a different
  quorum: the gate abstains for that evaluation (`GateInactive`) and says so in the log.
* **The floor is a halt, not a hole.** If leaking would leave fewer than `min_retained_validators`
  distinct validators in `S(E)`, nothing is leaked at `E`: finality waits, and the gate's TTL
  (Decision 5) releases a stale veto, rather than two validators finalizing for a network.

**Decision 4 — the denominator is signed.** `snapshot_commitment` =
`BLAKE2b-512("misaka/dns-bft-snapshot/v1", E ‖ A_E ‖ a_E ‖ W(E) ‖ root(S(E)))`, `root` over the
counted bonds sorted by outpoint as `(outpoint, validator_id, amount)`. A precommit binds the set it
was counted against, so a lock cannot be restated under a different denominator, and two branches
that count different sets for one epoch cannot share a precommit.

**Decision 5 — the gate follows the vote.** Past the fence:
* `DnsState.last_dns_confirmed_anchor` (and its DAA) advances to the newest DNS-final anchor on the
  sink's chain and is otherwise carried forward while it remains a chain ancestor of the sink — the
  StakeScore depth rule no longer confirms. `DnsState`'s layout, the QualityFloor epoch credit, the
  participation rewards, health and the rollout stage are unchanged.
* `dns_reorg_outcome` checks the confirmed anchor **before** the PALW comparator, for a reorg
  candidate and an extending one alike: a candidate that does not contain it is refused
  (`HardCheckpoint`) unless the anchor is stale under `dns_veto_ttl_daa_score` measured on this node's
  own chain (`ConfirmedAnchorStale`, released). A candidate that passes is weighed by the PALW
  comparator and ADR-0065 D2 exactly as today.
* The stake preference keeps its role (a dead-overlay escape) and its guard: whatever it proposes
  passes this gate and the comparator.

**Decision 6 — the precommit duty is read from the chain.** `ConsensusApi::get_precommit_duty` and
`getPrecommitDuty` answer, for a `(validator_id, bond)`: `round_active` (the fence at the sink), the
lock this chain shows the bond holding, and every epoch in the StakeScore window whose round one
reached quorum on this chain and which the bond has not precommitted, ascending, each with its anchor,
anchor DAA and snapshot commitment. The in-node validator service and the `kaspa-pq-validator` sidecar
precommit from it.

**Decision 7 — evidence is unchanged.** Two attestations for one epoch under one bond remain
`SlashingEvidence`; two precommits that disagree at one epoch or declare two locks at one
`locked_epoch` remain `PrecommitEvidence` (`precommit_fault`), each burning the bond and paying the
reporter as today.

**Decision 8 — the fence.** `Params::dns_bft_gate: Option<DnsBftGateV1 { activation, t_leak_daa,
reentry_final_depth_daa, min_retained_validators }>`, a top-level fence hashed Some-only into the
params id and the schedule id (its three numbers beside the height), visited as a fence, named to the
fork-id gate, answered only where the network runs an overlay. Refused at start: `reentry ≥ t_leak`,
`t_leak = 0`, `min_retained_validators < 4`, a pruning depth below the evidence window, and a network
without `dns_params`. `Params::palw_inactivity_leak` and `VltParams::vlt_activation_daa_score` stay
refused: the leak is this fence's, and there is no VLT weight.

## 3. What does not change

PALW block production, claims, panels, courts, `Final`, safe weight and the safe frontier; fork choice
among candidates the gate lets through; block validity; the overlay commitment root and the pruned
snapshot; attestation and precommit wire formats; rewards, the carve (ADR-0126) and slashing. Every
block and every gate decision below the fence.

**PALW does not depend on DNS finality or on BFT validators.** A PALW chain with no validators at all
produces, licenses, finalizes and orders exactly as it does today; the DNS-final anchor only adds a
reorg veto on a network that runs the overlay, and the veto lapses when the validators stop reaching
quorum. ADR-0127 states the PALW side and pins it.

## 4. Security amendments

* **SA-1 — accountable safety within the counted set.** Two conflicting DNS-final anchors for one
  epoch under one snapshot commitment need precommits from more than a third of `W(E)` for both —
  `Equivocation` evidence against each such bond. Across epochs, a bond that precommitted a final
  anchor and later precommits a conflicting one must either declare its old lock (contradicting the
  chain it signs on, so it does not count) or declare a different lock at the same `locked_epoch` on
  the other branch (`ContradictoryLock`).
* **SA-2 — the leak trades safety for liveness, after seven days, deliberately.** A partition that
  lasts longer than `t_leak_daa` lets each side leak the other's validators and finalize on its own;
  the veto then protects two different anchors. That is the price of a gate that heals after
  validator loss, and the TTL and the PALW comparator are what reconnect the network. `t_leak_daa` is
  the operator's number for how long that must take.
* **SA-3 — the evidence is the node's own, complete, and a function of the epoch.** The leak reads
  only attestations accepted on this chain at or below the epoch's anchor, inside a window every
  synced node stores (the pruning-depth refusal), so two synced nodes on one chain compute one counted
  set for an epoch whatever their sinks; a node that cannot cover the walk abstains rather than
  guessing.
* **SA-4 — a young bond is not silence.** Measuring an absent bond from its activation keeps a new
  validator counted for its first `t_leak_daa`, which is the time it has to attest.
* **SA-5 — the veto cannot decide.** It only refuses. A network whose validators never reach quorum
  runs exactly as it does without the fence.

## 5. testnet-11's numbers (§8 arms them)

* `activation` = DAA 6,001 — its own height beside 6,000, so the fork-id gate separates builds.
  Scheduled in the preset on 2026-09-17 (`d6c46f25`, at DAA ≈5,773) with ADR-0124, ADR-0125 and
  ADR-0126; fingerprint `4787b92a…`, re-pinned to `ab4e7b9c…` when ADR-0130 joined the height the same day.
* `t_leak_daa` = 5,040 — seven days at the chain's 120-second cadence. The 1-BPS execution lane does
  not change it: round blocks are outside the DAA set and advance no DAA score (ADR-0125 Decision 1).
* `reentry_final_depth_daa` = 200 — about 6.7 hours at the 120-second cadence (a hundred of testnet-11's
  two-block attestation epochs): deeper than the gate's 120-DAA veto TTL and the 120-DAA PALW challenge
  window, so re-entry rests on an attestation no reorg the gate or PALW would still weigh can remove.
* `min_retained_validators` = 4 — the smallest set in which one fault is tolerated.

## 6. Tests

The quorum is strict and one denominator serves both rounds; a precommit bound to another snapshot
does not count; lock consistency truncates at a misdeclaration; the leak measures an attester from its
youngest final attestation, an absent old bond from the window edge and an absent young bond from its
activation, and re-entry waits for burial — each against a window longer than `t_leak_daa`; the floor
halts rather than leaks; the pruning-depth refusal; the fence's refusals and Some-only hashing; the
duty answers what the chain shows; in the pipeline, a reorg abandoning a DNS-final anchor is refused
past the fence and allowed below it, a stale anchor releases, and a chain whose validators never reach
quorum behaves as without the fence.

## 7. Number hygiene

0128 was free when written; the next free number is 0129.

## 8. Implementation record (2026-09-17)

**Where it is.** The pure rules are `consensus/core/src/dns_bft_v1.rs`: the strict quorum (u128), the
window epochs and the evidence window's lower edge, the counted set with the leak and the floor, the
snapshot root and commitment, attested and precommitted stake, the lock chain
(`lock_consistent_precommits`, `held_precommit_lock`, restored from `f0cade50` with a horizon), epoch
verdicts, DNS-final, the confirmed anchor and the duty. The chain side is
`consensus/src/pipeline/virtual_processor/dns_bft.rs`: the walk, the confirmed state, the gate's
refusal and the duty, with a node-local `DnsBftRuntime` (the abstain flag, cached signature verdicts,
the duty cached per sink — none of it consensus state), hooked into `update_dns_state` and
`dns_reorg_outcome`. `Consensus::get_precommit_duty` answers Decision 6; ADR-0127's branch carries it
over RPC as op 183, and the in-node service and the sidecar precommit from it with one picker
(`pick_precommit_due`) and one precommit log path (`precommit_log_path`), the sidecar only where
`dns_bft_gate` is scheduled.

**The walk.** Once from the sink down the selected chain: the StakeScore window decides the evaluated
epochs, as `canonical_anchors_in_window` does; the walk continues to `min(anchor blue) − L` with
`L = t_leak + reentry + epoch + lag`, plus one block so the oldest anchors can be decided. It is
covered when it reaches that block or ends at genesis, and a gap when a header, acceptance-data or
block-transactions read fails first or the chain ends anywhere else. Evidence for epoch `E` counts only
when it is decidable inside `E`'s own window, so a counted set does not depend on how far a sink's walk
reached.

**The state and the gate.** Past the fence at the sink, `DnsState` keeps every field the depth rule
computes except the confirmed anchor and its DAA: the previous anchor is carried while the previous
state was written past the fence and the anchor is still a chain ancestor, and the newest DNS-final
anchor replaces it only when newer. A walk with a gap carries the anchor, logs, and the gate abstains
until a covered evaluation. The gate, in order: past the fence at the incumbent sink and the last
evaluation covered; a state written past the fence, the Active stage, a confirmed anchor; a candidate
that contains the anchor (or whose reachability is unreadable) continues; one that abandons a stale
anchor logs the release and continues; anything else is `HardCheckpointReject`. "Continues" is the V2
PALW comparator and ADR-0065 D2, or the old DNS half off V2; `dns_stake_preferred_tip` is untouched.

**What the decisions left open, decided here.**
* **The lock chain has a horizon.** Each epoch judges lock consistency from the floor of its own
  evidence window: the first precommit in view may declare no lock or a lock older than that floor, a
  rebroadcast of the same vote does not truncate, and every signed precommit is a link whether it
  counted or not. Read literally ("the first counted precommit declares no lock"), Decision 2 drops
  every long-running validator's count to zero once its first precommit leaves the walk.
* **A stale release on a V2 reorg candidate goes on to the PALW comparator** rather than returning
  `ConfirmedAnchorStale`, which would let a stale DNS anchor bypass it; off V2 and for extensions the old
  DNS half still returns it.
* An anchor from a state written below the fence never becomes a veto at the crossing; the gate
  requires the Active stage; the abstain flag is in memory and resets on restart; a due epoch also
  requires the bond in `S(E)` and no precommit of any kind for that epoch.

**Named, not closed.**
* **Pipeline coverage.** The V2 pipeline test writes `DnsState` by hand rather than running both rounds
  on a ConsensusV2 chain; the leak, the floor and lock truncation are pinned at the pure level (a
  single-validator pipeline always holds the floor); a pruned node's coverage gap is simulated by
  deleting one block's acceptance data.
* **Bond-set timing.** Bonds are read from the store as of the previous commit, as the credit walk
  reads them; a reorg that removes a bond can shift `S(E)` for one evaluation, which only delays
  finality (the commitments stop matching).
* **Cost.** An evaluation runs once per attestation epoch and walks about a week of chain blocks. The
  walk first re-read every block's acceptance data and every merged block's transactions each time —
  some 5,300 chain blocks per evaluation on testnet-11, and past ADR-0125's height each of them merges
  about 120 execution blocks, so some 630,000 block reads every four minutes on the virtual processor.
  Found at integration and fixed before any network armed the fence: a node-local memo keeps, per chain
  block, the votes it accepted — a block's acceptance data is a function of the block — with each
  signature judged when its bond is known and its bytes dropped, bounded to the blocks the latest
  covered walk visited. A later evaluation reads only the chain blocks new since the last one; the
  pipeline tests pin that a second walk reads nothing and that a walk from the memo decides exactly
  what a walk from the stores decides. The first evaluation after a restart still reads and verifies
  the week.
* **SA-1's cross-branch edge.** `precommit_fault` cannot convict a precommit on another branch that
  declares a lock OLDER than one the bond already precommitted: such a precommit does not count on the
  chain that knows the newer lock, but it is not evidence either.
