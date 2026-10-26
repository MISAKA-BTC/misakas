# testnet-12: the 8k panel room and the bind wait feed each other — 2026-10-26

Status: analysis on the live fleet (read-only, release 247ade1f9, DAA ~15,835); node-only fixes on branch
`fix/t12-anchor-duty-short` (off `main` 321978444, the fleet's source); **nothing deployed**.

## 1. What was observed

The explorer's consensus composition showed 3 QWEN25-A16-8K blocks of 125 chain blocks (the rest heartbeats). The 8k
producer (ibm b0) made 51 claims in 12 h and held 78 times with

```
the panel has no room for a claim of class ebf44d0a…: 80557764400128 of replay in flight against a budget of 80640000000000 over 3 spans
```

## 2. The room (code: `fold_read.rs` `panel_rate_v1`, `palw_state_v2.rs` `palw_panel_owed_v1`)

The 8k room is a COUNT of claims whose replay the panel still owes — in flight and not licensed (a licensed claim with a
court open counts again; the 8k class is not C7, so a licence releases it). Past `palw_capacity_verify_room`:

```
capacity = min( ⌊ready × per_seat_per_span × window / (k · ccu)⌋ ,  ρ × shipped )
         = ⌊ 9 × 2.987e12 × 3 / (2 × 1.389e12) ⌋ = 29
```

The log's in-flight figure is exactly `29 × 2.778e12`: 29 of 29 owed. Time does not enter the count; the rate a slot frees
is the licence rate.

## 3. Where the time goes (130 claims, DAA 15,150–15,830, `[palw-panel] event` lines of ibm b0/b1/b3)

| span | median | p25 / p75 |
|---|---|---|
| on chain → bound | **78 DAA** | 30 / 82 |
| bound → quorum (licence) | 30 DAA | 21 / 38 |
| on chain → licence | 111 DAA | 60 / 121 |

DAA ran at ~165 s (967 DAA in 44.4 h). 93 of the 130 claims were bound, unbound and bound again (cause not found).

**The loop.** A claim binds only in a block that is or merges an OPERATOR's attempt at or past its slot (ADR-0165 §00.9).
With the room full the 8k producer holds, so no operator REAL attempt is mined; the claims wait for the floor producers'
anchor duty, which fires after 30 slots plus the bond's stagger (ibm b1: 36). Bind waits, licences wait, the room stays full.

A held 8k producer cannot mine the binder itself: its own attempt refused for `PanelRoomExhausted` is not in the fold's
own-attempt skip list (`AttemptExposureCeiling`, `FloorNotIdle`, … are), so the block is disqualified. That would take a
fence (`palw_anchor_at_ceiling` extended to the room) — not here.

**The seats are the next limit.** Every seat has 13–17 duties due. Replay (start → `licensed by replay`), 12 h:

| host | seats | replay median | p90 | note |
|---|---|---|---|---|
| .113 | b6, b7 | 250 s | 300 s | ~55 % of the time replaying |
| ibm | b0, b1, b3 | 490–530 s | 840–970 s | 3 nodes on 8 vCPU, b0 also produces |
| 5.104 | b2, b4, b5 | 710–750 s | **9,800–10,400 s** | load 8–11 on 8 vCPU, 4.6 GiB swapped, ~600 slot waits per seat |

About 40 % of the replays on ibm and .113 are pre-accusation judging (`judging the claim by replaying the anchor's job`).

## 4. Fix A1 — the short anchor-duty wait (node policy, no fork)

`PalwProducerFactsV2::real_attempts_refused` (filled in `palw_accept.rs` with `binder_due`, floor producer only): every model
class with a lifecycle row, the base excluded, refuses a new attempt at the fold's own class gate at the candidate's DAA. While
it holds, `palw_floor_anchor_duty_v1` waits `PALW_FLOOR_ANCHOR_DUTY_AFTER_SLOTS_NO_REAL_V1` = 4 slots (plus the stagger) instead
of 30. Otherwise nothing changes, so the control — operators' REAL attempts anchoring on their own — never sees a floor binder.

* consensus-core `palw_real_share_v1.rs`: the constant, the input field, `palw_floor_anchor_duty_after_slots_v1`;
  `palw_producer_v2.rs`: the fact.
* consensus `palw_accept.rs`: the fact, from `palw_class_admits_claim_v1` per rowed class.
* kaspad `palw_producer.rs`: the wrapper passes it; the anchor-duty line says when the short wait applied.

No consensus rule, no fingerprint: the facts are the producer's read; the binder is the floor attempt the fold already skips.

## 5. Fix A2 — 5.104 from three seats to two (deploy kit, not run)

`install-5104.sh` drops b5 from `NODES` (b2/b4 lose `127.0.0.1:26341` from their peers: their next `upgrade` is an ARGS change);
`move-nodes.sh` gains `RETIRES=( "5|5104" )` and b5's source spec so `move-host.sh retire 5` works with no target. .113 is not a
target (94 % disk, explorer, postgres, load spikes to 13); ibm already runs three nodes. The 8k class keeps 8 ready seats against 7
required (the retire check refuses below that); its room falls 29 → 25.

Procedure (on 5.104, operator, after the A1 release is rolled out):

```
cd /root/t12-rel/kit
./move-host.sh preflight retire 5
MOVE_TO=retired CONFIRM_MOVE_SEAT=yes ./move-host.sh retire 5
```

Undo: `CONFIRM_TARGET_STOPPED=yes ./move-host.sh unretire 5`.

## 6. Verification

**Replay of the rule against the live logs (72 h, read-only).** The two floor producers (ibm b1, .113 b6) fired 29 binders, every
one after exactly 36 slots. For each of the 130 claims: slot = on chain + 20; the new rule binds it no later than slot + 4 + 6
(both bonds' stagger is 6) when the 8k class was refusing there (b0's room-hold lines, mapped to DAA by the heartbeat clock lines —
true for all 130):

| | now | with A1 (receipt time unchanged) |
|---|---|---|
| on chain → bound, median (p25 / p75) | 78 (30 / 82) DAA | 30 (30 / 30) DAA |
| on chain → licence, median (mean) | 111 (105.3) DAA | 58.5 (66.6) DAA |
| 8k claims at the fixed room of 29 | 1× | **≈ 1.6×** |

The 1.6× is an upper bound: it holds the receipt time fixed, and the seats (§3) are already near their limit, so with A1 they
queue longer. A2 is what gives them room.

**Tests (this branch, Mac).**

* consensus-core `palw_real_share_v1` 15/15, `palw_producer_v2` 9/9: the rule's short wait, its stagger, a drill's shorter wait,
  and that nothing else gives way.
* kaspad `palw_producer` 29/29, including the wrapper reading `real_attempts_refused` from the facts.
* consensus `t12_real_share` 14/14 on the real pipeline. The new
  `real_share_anchor_duty_waits_short_when_every_real_class_refuses` fills the 8k class (5 of 5 in flight): the claim's slot is
  DAA 30 and one operator binder binds it at DAA 40 (4 + stagger 6). The release-wait test binds its claim at DAA 66. The control
  (operators' REAL attempts anchoring on their own) still mines no binder.
  `real_share_anchor_duty_fires_once_and_then_the_floor_stays_held` now also sees a second binder at DAA 85, for a later claim
  that fell due while the class was full. It now asserts that every binder after the first is a short-wait one, that binders are a
  wait apart, and that the first claim stays bound. `t12_bind_deadlock` 4/4.
* `main`'s consensus lib tests did not compile before this change: `tests.rs` read `calculate_utxo_state_relatively` as returning a
  hash (it returns `Result` now — fixed here), and `t12_a2u_new_kinds_uniform.rs` does not arm three newer fence kinds
  (`MaterialReleaseRecoveryV2`, `MemoryRoutingCourtV2`, `SelectedSourceBootstrapV1`). The runs above excluded that one module
  locally; it is unchanged on this branch.

**Not done: a node-process drill.** The state under test needs the 8k class Active, so 7 ready 8k seats. That means 8 artifact
nodes, which this 24 GiB Mac cannot run (it rebooted at 8 artifact nodes before). The drill kit refuses the three public hosts, and
95.111.236.186 has 11 GiB. A drill needs a host set the operator provides (`contrib/t12-drill-kit/README.md` §2).
