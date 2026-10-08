# SHARD — RFC-0006 × RFC-0010: per-shard V3 draw, G14 pre-emption guard, non-seat cell watcher, per-segment pricing

Agent SHARD, branch `shard/rfc6-10`, 2026-10-09. Owner: Lead (single integration line). Companion to
`remaining-rfc-integration-matrix.md` (RFC-0006 and RFC-0010 rows) and `rfc-0010-production-path-record.md` (§5, §6).
Everything here is behind a dormant fence: `palw_permissionless_panel_v1` (refused at every real height),
`palw_tir_shard_v1` (armed on testnet-12 at DAA 5,300; **no armed rule changes**) and one new fence proposed here,
`palw_tir_shard_segment_v2` (None on every preset). No live testnet-12 id moves (params `5ee7fd8e…`, schedule
`1678e073…`; checked with `scripts/t12-repin.sh --shipping --drift-only`).

## 概要（日本語）

- **RFC-0006 の V3 per-shard draw。** RFC-0010 の permissionless Panel engine（`misaka-palw-panel`）に *strata*（層 shard ごとの
  層別抽選）を入れる。shard plan を宣言した IR class の V3 claim は、今の `PermissionlessNoCapablePanel` で終わらず、shard ごとに
  `[outsider?] ++ 3 class seats` を引く。class seat はその shard の readiness（`palw_tir_shard_ready_class_v1`）を証明した bond、
  outsider は base class の母集団から（1 operator は claim あたり outsider 1 席まで）。seed は V3 seed を shard ごとに分けたもの。
  bind は V2 の `PalwPanelStateV2`（shard-major）＋ `tir_shard_claims` の記録を書くので、既存の `TirShardReceiptLicensed`（parts）・
  recount over cells・lock・pay・court がそのまま動く。
- **G14 pre-emption guard（RFC-0010）。** 「告発が開いている間、V3 の non-fraud な終わり（期限切れ・再抽選・seal/beacon 不在）は
  claim を閉じない」を **一つの述語と一つの書き手** で構造的に保証する。C2 の receipt-clock pause は bound 後の engine 期限にしか
  効かず、次の二つが残っていた：(1) S2 licence の V3 期限切れ（`ReceiptLicensed`、engine は release 済み）が非 seat の DA session を
  `PanelUnavailable` で中立に閉じる、(2) bind 前の engine 終了（BeaconUnavailable 等）が開いている court / dissection session を中立に閉じる。
  修正：engine の判断は engine の時計のまま（grinding の自由度を増やさない）、V2 への適用（`end_claim`）を告発が終わるまで *保留* する。
  S2 の期限は告発中は張らない。
- **非 seat の cell watcher。** seat でない bond が、公開 material（producer が公開 provider に置いた capture、または on-chain の
  `TirStepRun` 開示）と shard の重み（registered artifact の行）だけで sharded claim の cell を検証し、嘘なら自分の bond で
  `TirShardCourtAccused` を、material が無ければ `TirStepRun` の DA 要求（非 seat 予算）を出す。node の opt-in pass
  `--palw-tir-shard-watch`。
- **per-segment pricing。** 新しい休眠 fence `palw_tir_shard_segment_v2`。その先では、cell の lock と pay の share を
  `max(work share, resident share)` にする（resident = shard の重み＋その segment の終わりまでの K/V 履歴＋Fixed state）。
  「後ろの segment ほど重い」を価格に入れる。tier（seat がどの segment まで載せられるか）は on-chain で証明できないので consensus に
  持たず、node の readiness gate を「shard の一番重い cell を載せられるときだけ possession proof を出す」にする（fence の先のみ）。

## 1. RFC-0006 — the per-shard V3 draw

### 1.1 Problem

`palw_panel_v3_fold_v1::admission_of` ends every V3-rule claim of a class with a shard plan non-fraud
(`PermissionlessNoCapablePanel`): the engine draws one flat panel of `policy.seat_count` seats, and a flat panel cannot
license by parts (`bind_record_v1` writes the per-shard record only for a panel of the plan's stratified length). So no
sharded class could ever reach a Panel under V3 — the DESIGN_GAP in the matrix.

### 1.2 Design: strata in the engine

The draw is the engine's (`stage_assign`): it alone sees the frozen snapshot, the seed, the retry index and the
one-ledger headroom. A per-shard draw computed outside the engine would need a second reservation ledger and a second
retry history, so the engine learns *strata* — generically, with no RFC-0006 import:

| Item | Value |
|---|---|
| `PanelStrataV1 { count, class_seats, outsider }` | `count` strata (2 ..= 64), each `[outsider?] ++ class_seats` seats; frozen at admission |
| `ClaimRecordV3.strata: Option<PanelStrataV1>` | `None` = the flat draw, byte-for-byte the old rules |
| `PanelSnapshotV1.strata_members: Vec<u64>` | empty for a flat claim; for a stratified one, one bitmap per candidate (aligned with `candidates`): bit `s` = may hold a class seat of stratum `s`. The snapshot root covers it, so the per-stratum populations are frozen at the pre-entropy checkpoint with everything else |
| `ConsensusViewV1::stratified_candidates(claim, strata)` | default: empty population (a host that cannot answer seals nothing a stratified claim can use → `NoCapablePanel`, never a failed block) |
| `PermissionlessPanelStateV1::admit_with_strata` | `admit` is `admit_with_strata` with `None` everywhere |
| Stratum seed | `H("misaka-palw/panel-v3/stratum-seed" ‖ seed ‖ stratum)`; tickets as the flat draw (`seat_order` with the stratum's eligibility) |
| Seal id | flat: unchanged tuple; stratified: the same tuple and the strata under `…/seal-strata` |

**Draw rules** (`stage_assign`, stratified record), mirroring `derive_tir_shard_panel_v1` (lane A) where V3 has no reason
to differ:

1. For each stratum `s` in order: if `outsider`, one OUTSIDER-role seat whose operator holds no other outsider seat of this
   round and none in an earlier round; then `class_seats` seats whose bitmap has bit `s`, one per operator inside the
   stratum, the stratum's outsider operator excluded. An operator may sit in several strata (distinct duties, distinct
   masks — RFC §4.1).
2. **Alternates are never reused per stratum:** a retry excludes every operator that sat in stratum `s` in an earlier
   round (class or outsider), and every earlier outsider from the outsider draw.
3. **One ledger, once per bond per claim:** a bond seated in two strata of one round reserves the per-seat exposure once
   (V2's duty row holds each bond once — `reserve_seat_duties_with` — and a second shard's price is posted at licence time by
   `apply_part_v1`'s increment rule). The engine's `rebuild_reservations` counts each bond once per binding (a no-op for a
   flat binding, whose bonds are distinct), and the fold's added-back duties do the same.
4. A stratum that cannot be filled ends the claim `NoCapablePanel` (non-fraud) — a sharded class never falls back to a
   flat panel (RFC §4.1).

**Validation** (`validate`, the carriage import's check) re-derives every rule above from the binding history: seat
count `count × stride`, roles and stratum bits per seat, operators distinct per stratum slice, outsiders distinct per
round, alternates per stratum, `used_operators` = the concatenation. A tampered stratified binding does not load.

### 1.3 Fold wiring (`palw_panel_v3_fold_v1`)

* `admission_of`: a class with a plan (past `palw_tir_shard_v1`) is admitted with
  `PanelStrataV1 { count: s_l, class_seats: PALW_TIR_SHARD_SEATS_PER_SHARD_V1 (3), outsider: palw_claim_is_outsider_judged_v1 }`;
  the flat outsider-policy check does not apply; the exposure is `rcore_bind_prices` over `s_l × stride` seats.
* `FoldView::stratified_candidates`: per shard, `palw_panel_stake_base_bonds_judging_v1` over the shard's readiness class
  (`palw_tir_shard_ready_class_v1`) — the same eligibility lane A's per-shard draw reads — plus, if `outsider`, the base-class
  population as OUTSIDER role; sanitised exactly as the flat candidates (maturity, exclusions, collateral, cap by collateral).
* `bind_v2`: after writing the V2 panel record (anchor = V3 seed, shard-major seats) calls
  `palw_tir_shard_fold_v1::bind_record_v1`, which writes the per-shard record (plan frozen, outsider flag, each seat's share
  by the S1 assignment of the V3 seed). From there the armed RFC-0006 machinery runs unchanged: cell-masked V4 receipts,
  `TirShardReceiptLicensed` parts, `basis_k` over cells, scaled locks, pay by share, `TirShardCourtAccused`, `TirStepRun`.
* `assert_panel_v3_consistency_v1`: a bound stratified claim holds its per-shard record.

**Wire/fence impact.** No new object tag, delta number or carriage tail: the engine's grown records ride delta 171
(`PanelV3Claim`) and tail `0xED`. The engine's encodings change (dormant: the fence is refused at every height, no network
holds an engine). The pinned seed vector (`seed_vector.rs`) hashes the snapshot root, not the snapshot, and is unchanged.

## 2. RFC-0010 — the G14 pre-emption guard

### 2.1 What C2 left open

C2's receipt clock is paused while any DA (seat or non-seat) or court session is open on a **bound** claim, and re-based
to the last close. Three paths were not covered:

1. **The V3 S2 expiry.** An S2 licence (`OptimisticLicensed`, `basis_k` 1) of a V3 claim that no supplementary set raised
   to 2 *expires* in `sweep_deadlines` (`PanelUnavailable`, uncharged). The claim is `ReceiptLicensed`, the engine has
   released it, and V2's deadline is not paused by a non-seat session (V3S-08). `sweep_deadlines` runs **before**
   `sweep_da_sessions` in step 2, so a non-seat DA session whose deadline is at or after the licence's gate is closed
   neutrally by the void (`void_claim` → `da_release_all_v1(convicted = false)`): the withholding is classified as panel
   unavailability. A G14 pre-emption.
2. **Pre-bind engine ends.** `SealUnavailable`, `BeaconUnavailable` and a first-draw `NoCapablePanel` do not read the
   receipt clock. A claim cannot be DA-accused before its bind (`DaClaimNotAccusable`), but a court can be open on it:
   `CourtOpened` and `TirShardCourtAccused` accept any non-terminal phase, and under the held regime a one-move accusation
   at a dissected leaf *opens a dissection session* on a `Provisional` claim. On today's chain every V3 claim ends
   `BeaconUnavailable`, so every such session would be closed neutrally by the void (C3 of ADR-0152 §4-ter).
3. **Defence in depth.** Any future path that ends a V3 claim non-fraud while an accusation is pending would silently
   pre-empt it; nothing failed loudly.

### 2.2 The guard

**One predicate** — `PalwChainStateV2::palw_accusation_pending_v1(claim)`: a DA session (seat or non-seat) is open on the
claim, a court session (bisection or dissection) is open on it, or the claim is `DefaultDisputed`. Read by:

* the engine's receipt clock (C2's pause, now through the predicate: no redraw, no expiry while pending);
* **the one writer of every V3 non-fraud end, `end_claim`**: while the predicate holds, the V2 void is **deferred** — the
  engine keeps its own decision (`Voided { reason }` on its own clock, so the outcome is not re-rolled by timing and no
  grinding freedom is added), the V2 claim stays where it is, holding its reservation and duties; the deferred void is
  applied at the first stage (2f or 4b″) at which nothing is pending — or never, because the accusation convicted first
  (`ProducerWithholding`, `CourtFraud`, `CourtDefault`), which is the point;
* `panel_v3_clocks_claim_v1`: a claim whose engine record is `Voided` while its V2 claim still awaits its Panel is still
  the engine's (no V2 bind or receipt deadline, never offered to the lane-A binder);
* `palw_rcore_deadline_v1` (DL-1): a V3 claim's S2 licence holds **no deadline** while the predicate holds; opening any DA
  session on it disarms the deadline, and the close of the last one re-derives it (`rearm_claim_deadline_dl1_v1`);
* the S2 sweep arm: a V3 claim whose deadline fires while pending is skipped (unreachable by the derivation; consistent
  with it — the close re-arms);
* consistency (`assert_panel_v3_consistency_v1`, the deadline checks): an engine-`Voided` claim still waiting in V2 is
  consistent iff an accusation is pending; a V3 S2 claim owes no deadline while one is.

**Semantics at the edge (documented, tested).** A non-fraud end is decided at the block's pre-object stage (2f) or in its
step-2 sweep; an accusation carried by that same block comes after the decision and lands on a claim that already ended
non-fraud — a late accusation, not a pre-empted one. Any accusation open at the decision holds it. A DA default is swept
in step 2 before 2f, so at the block a default matures it always wins.

**Bound.** A deferral lasts at most as long as the accusations can: three non-seat DA sessions open at once and sixteen
over a claim's life, four per seat, each at most `W_disclose`; court sessions by the court's own capacity and backstop.

### 2.3 Not changed (reported)

Lane A (armed or not) keeps V3S-08: a non-seat session does not pause a V2 claim's receipt timeout. Today the second
timeout charges the producer (`ReceiptTimeout`, S0′), which is a charge, not an acquittal, but the accuser's session is
closed unrewarded; and **lane PL part C (`palw_panel_unavailable_expiry`, dormant)** would make it an uncharged
`PanelUnavailable` with the same pre-emption shape as 2.1(1). If part C is armed in the full-activation release it needs
the same hold (a fence decision for the Lead; no code here).

## 3. RFC-0006 — the non-seat cell watcher

G14 for a sharded claim: one bonded verifier outside the seats, with public material only, reaches a conviction or a
correctly classified default. Consensus already allows it (`TirShardCourtAccused` takes any Active bond at the floor;
`TirStepRun` is a DA unit any Active bond may demand within the non-seat budget). What was missing is the verifier.

`kaspad/src/palw_panel/tir_shard_watch.rs`:

* **Targets** (`palw_tir_shard_watch_targets_v1`, pure over the tip state): every live claim drawn per shard
  (`tir_shard_claims`: `PanelBound`, `ReceiptLicensed`) whose producer is not the watcher, and every shard of it the
  watcher does not seat (a seat's own duty covers that shard). A watcher checks a **whole shard** (every segment), the
  outsider's span.
* **Verdict**: the seat's own cell verifier (`palw_tir_shard_outcome_v1`, or `_over_v1` for a watcher that holds only the
  shard's rows fetched and proven against the registered root) over the public capture, through a watch duty that carries
  no seat index and signs nothing.
* **Actions**: a finding → the IR one-move accusation (`palw_tir_shard_accusation_v1` / `_over_v1`) with **the watcher's
  bond as accuser** (`TirShardCourtAccused`); no material → a `TirStepRun` DA demand (`DefaultAccusedTirStep`) on the first
  run the shard's cells read, within the non-seat budget, whose default is `ProducerWithholding`.
* **Node**: opt-in `--palw-tir-shard-watch` pass after the seat pass; findings ride the seats' carrier path. Material is
  read only from the public pool (provider dirs, the chain's disclosures); the watcher never asks the producer with a
  seat's signature.

## 4. RFC-0006 — per-segment pricing (fence proposal + implementation)

### 4.1 Gap

The armed rules price a cell's *work* (`cell_permille`, attention work grows with the segment) but not its *residency*:
a late segment's cell needs the shard's whole K/V history up to the segment's end. Readiness is one possession proof per
shard, so a seat can hold the shard's rows, be assigned a late cell it cannot host, and be a non-responder (a liveness cost
borne by the claim; safety unchanged — `rfc-0010-production-path-record.md` §6).

### 4.2 Fence `palw_tir_shard_segment_v2` (proposed; declared dormant here)

A bare height, `None` on every preset; the four writes (field, `for_each_fence`, Some-only in both ids, `never()`
collapse) and the bundle mirror `PalwStateParamsV2::tir_shard_segment_from_daa`. Refused off ConsensusV2 and without
`palw_tir_shard_v1` in force at or below it. Its height must be one no other fence uses. **The Lead chooses the height**
(the full-activation release).

### 4.3 Rules past the fence (keyed on the claim's panel `bound_daa`, so bind and parts agree)

* **Resident table** (`palw_tir_shard_cell_resident_permille_v2`): per cell `(i, j)`, the shard's weight bytes
  (`pre`/`post`/globals as RFC §1.1) + its layers' `Fixed` state + its layers' `Hist` rows up to the segment's end
  (`min(window, end_j)` rows, the canonical job of `palw_tir_shard_canonical_facts_v1`), as permille of the sum over cells
  (largest remainder, sums to 1,000). A pure function of the registered program and the plan; computed on demand (no new
  rooted state, so the armed `PalwTirShardPlanV1` / `PalwTirShardClaimV1` encodings are untouched).
* **Price share** (`palw_tir_shard_price_share_v2`): a seat's share is `max(Σ work, Σ resident)` over its mask's cells
  (floored at 125 ‰, capped at 1,000 ‰, as today). Used for the lock a counted signer posts (`apply_part_v1`) **and** the
  pay weights the bind fixes (`bind_record_v1` writes `drawn_permille` from it — the field exists; only its values change past
  the fence). The slash term is unchanged, so the deterrent does not shrink.
* **Readiness = the heaviest cell** (node policy past the fence): a node files a shard's possession proof only when its host
  ledger can hold the shard's heaviest cell (`palw_tir_shard_seat_need_bytes_v2`: max over segments of the resident bytes).
  So every ready seat can host every cell of its shard, and the draw needs no tier.

### 4.4 Why no on-chain tier

A tier ("the highest segment this seat can host") is a capacity claim; nothing on chain can prove it, and silence is not
convictable (ADR-0064). Carrying it would need a new object (the armed `TirSeatReadinessProved` cannot change its
encoding) and a slash rule for over-declaring that the chain cannot adjudicate. With `S_P = 1` (decision 5's default) there
is one segment and the question does not arise; with `S_P = 2` the readiness gate above gives the same liveness property
without a consensus field. If a measured drill later shows the gate too strict for the network's hosts, a tiered
readiness class can be added under its own fence.

## 5. Allocations and coordination

* **No new object tag, delta number, carriage tail or void reason.** One new fence name (`palw_tir_shard_segment_v2`) for
  the Lead's registry.
* **Beacon untouched** (OPV-BOOT owns `opv/bootstrap-beacon`); the per-shard draw consumes whatever certified output the
  engine accepts.
* **A2U:** no new object kinds or variants.

## 6. Tests

| Deliverable | Test |
|---|---|
| Engine strata | `misaka-palw-panel/tests/strata.rs`: per-stratum seats and operators, outsiders distinct, alternates per stratum, thin stratum → `NoCapablePanel`, reservations once per bond, tampered stratified binding refused, flat claims unchanged |
| Sharded class bound by a per-shard V3 draw | `consensus/core/tests/rfc0010_shard_v3.rs`: an IR class with a 2-shard plan, readiness per shard, a V3 claim sealed, certified and bound per shard; the per-shard record written; two parts license it by cells; the delta reverts and the carriage reloads at every block |
| Non-seat watcher acting | `rfc0010_shard_v3.rs`: a bond outside every seat convicts the V3-bound sharded claim with `TirShardCourtAccused` from public material, and a non-seat `TirStepRun` demand defaults to `ProducerWithholding`; `kaspad/src/palw_panel/tir_shard_watch_e2e.rs`: targets, verdicts and the watcher-built accusation the one-move gate convicts |
| Per-segment pricing | `consensus/core/tests/palw_tir_shard_segment_v2.rs`: the fence's refusals and dormancy; resident table sums to 1,000 and grows with the segment; lock and pay by `max(work, resident)` past the fence, unchanged below it |
| G14 guard | `consensus/core/tests/rfc0010_g14_guard.rs`: the S2-expiry race (default wins, refuted twin expires after the close), pre-bind `BeaconUnavailable` deferred under an open court (conviction or deferred void, never a neutral close), the receipt-window boundary races |
