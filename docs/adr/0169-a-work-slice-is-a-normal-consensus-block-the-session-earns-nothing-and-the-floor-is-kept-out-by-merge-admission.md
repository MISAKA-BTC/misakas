# ADR-0169 — A work slice is a normal consensus block; the session that defines the job earns nothing, and the floor is kept out of the chain by merge admission, not by colouring

> **Mission alignment, 2026-10-07:** [ADR-0173](0173-public-verifier-dispute-completeness-is-misaka-purpose.md) governs future PALW design. Where the earlier body conflicts with ordinary non-Panel public-bond prosecution from authenticated public material, without producer-private state, the dated amendment at the end supersedes that direction. Earlier Status, measurements and activation records are preserved; this is not a claim of implementation or activation.


* Status: **IMPLEMENTED 2026-10-03/04 on `rfc8/claim-backed-blocks` (lane RF8), DORMANT.** The three fences — `palw_work_slice_v1`,
  `palw_ws_clock_v1`, `palw_merge_admission_v1` — are `None` on every shipped preset, in no flag-day list, with no activation height
  chosen; `Params::validate_palw_work_slice_v1` refuses to arm them while a part of the lane is unbuilt (`PALW_WS_UNBUILT_V1`) and,
  outside a salted drill genesis, while RFC-0008 §7's consensus-safety prerequisites are open (`PALW_WS_OPEN_ITEMS_V1`, seven of them,
  named in the error). **The lane is not safe to arm and this ADR says why in §3.**
* Builds on: [RFC-0008](../rfc/0008-palw-claim-backed-consensus-blocks.md) and its [implementation spec](../design/palw/rfc-0008-implementation-spec.md)
  (the as-built record, stage by stage); [0165](0165-the-floor-is-a-reserve-and-the-work-carries-the-clock.md) (the floor machine, the
  REAL clock tick), [0142](0142-the-consensus-clock-is-a-cursor-a-heartbeat-consumes-a-slot.md) and [0140](0140-the-heartbeat-is-the-emergency-generator.md)
  (the clock), [0058](0058-palw-merged-work-is-counted.md) (merged work), [0105](0105-a-heartbeat-never-turns-a-bonded-block-red.md)
  (colouring), [0132](0132-what-a-model-is-actually-paid-per-forward-it-ran-and-why-the-gap-is-liveness-before-it-is-price.md) (S: the single lottery). **Not touched:** algo 10 and ADR-0168.
* Amends: ADR-0058 under `palw_merge_admission_v1` only; ADR-0142 §10 and ADR-0140 §6 carry the clock amendment.

## 0. The sentence this ADR is

**One bounded LLM session is a root that defines the job and the deterministic slice boundaries and earns nothing; each slice of it is a
normal algo-11 consensus block whose work the chain credits once per history, at `Final`, through the claim lifecycle attempts already
use — and the floor blocks that would colour such work RED are kept out of the canonical chain by a rule on what a chain block may merge.**

## 1. Decisions

**D1. A slice is a block, not an object.** Algo id 11 (`POW_ALGO_ID_PALW_WORK_SLICE_V1`), a normal selected-parent candidate: coloured
like an attempt (`LaneColoring::Weighted`), blue work the attempt constant (`2^20`, never the slice's CCU), counted in the DAA set,
exempt from DAA pricing, its PoW digest compared to nothing (the single lottery; there is no per-slice lottery — open item 4). Its
envelope is signed under the bond's key and bound to the block's own position (pre-PoW hash, stamp, nonce), so a slice cannot be
re-mounted at another position and a third party cannot re-sign it.

**D2. The root earns nothing.** A session root (object tag 96, tx-carried) opens a session and writes a job ledger row; it burns its fee
and creates no claim, CCU, PWU, reward or weight. One root per job per history (the job ledger is never pruned), open roots per bond and
the root's window are bounded. The plan — slice ranges cut at `lcm(checkpoint_interval, h_tile)` by the class's per-position cost — is
derived, never carried, and a slice's CCU is never carried either (a declared pwu once bought fork-choice weight).

**D3. A slice claim is a claim.** Its id is its work id; it rides the core ledgers (reservation, immature weight, safe weight, escrow,
deadlines, panel, court) and `Final` adds its weight once through the attempts' helper and pays its escrow priced by its own work.
Finality is in order (a slice waits, deferred, for its predecessor); a fraud verdict voids the slice and every later one and slashes
once; a deadline lapse voids without a slash (silence is not slashable); dependents never pay. Conservation (Σ final ≤ Σ plan = the job's
CCU; one credit per `(root, index)`; the root earns 0; a slice grants no round permit) is property-tested over random event sequences, a
second node and a restart — and run through testnet-12's real pipeline: a slice claim is bound by the chain's own panel derivation, licensed
(the harness signs `Valid` for the seats: no seat replays a slice, D7), finalized after its challenge window, and credits its session and its
weight once; a slice nobody licenses lapses, voids its session and slashes nothing.

**D4. Merged slices are entitled by one function.** A merged slice block is paid (its carve withheld and escrowed) iff the PARENT's ledger
accepts it in acceptance order; the coinbase and the fold ask the same question of the same view, and the fold may only refuse more, so a
claim is never created for a block the coinbase did not pay. State additions are Some-only: deltas 107–109, carriage tail `0xE8`. The
processor-level tests read the agreement off the coinbase itself: the coinbase that follows a slice block pays its miner the worker base
less exactly the carve the claim escrowed, for an own slice and for a merged BLUE one.

**D5. The clock.** Past `palw_ws_clock_v1` an algo-11 block is a header-derived tick source beside the heartbeat and the attempt: the
cursor's one tick a slot is the bound (a forged slice consumes a slot as a heartbeat does, never two), the lead cap reaches it where it
is a source, heartbeats stay unconditional, and the court is not on the tick's path.

**D6. Merge admission, stage A (floors).** Colouring stays header-only; instead a chain block may not merge a `PALW-BASE-0` floor the
chain's own floor state refused at the floor's slot, unless the anchor duty required it. The chain keeps a ring of the last
`merge_depth + 2` slots in rooted state (the floor state at the slot's end; the wait of a claim for an anchor as of its start); the
verdict is the floor machine's Idle, or RS's `palw_floor_anchor_duty_v1` over the ring's facts, so the one binder an operator mines for a
non-operator's waiting claim stays mergeable. A refused merge disqualifies the merger (ADR-0058 amended under the fence only), a template
never builds one, and the decision reads only the merging chain's state: arrival order, IBD, a pruned join and a reorg give one answer
(tested). **Stage B (static eligibility of REAL attempts and slices before the chain colours them) is not built**; spec §7 says what it
needs.

**D7. Verification is replay, and it exists only as a library.** A position range is replayable from a committed boundary state: the
slice's roots commit the predecessor boundary, the leaves, the selected ids, the boundary it leaves (in a canonical form, so two honest
executors commit one root) and the data-availability root of that state. `TirClassRunnerV1::{produce_slice_v1, verify_slice_v1}` produce
and replay a slice and name each lie. **No seat service replays a slice, nothing carries the boundary state and the prompt to a seat, and
no court path adjudicates a slice**; so a slice is claimed and never licensed. This is the one unbuilt part (`PALW_WS_UNBUILT_V1`).

**D8. The producer opens nothing, and signs an index again only on evidence.** A drill-only node service reads the chain's session (not
its own memory) to know which slice is next, holds back what the ledger would refuse, replays and signs the slice, and re-declares the
node's own template as algo 11. It writes the signed root for `misaka palw submit-object` and holds no fee float. A duplicate
`(root, index)` block earns nothing — one credit per history — but it is an algo-11 block and **colours**, so the producer signs an index
again only when the consensus's own reachability says its previous block will not count (absent from the node, beyond the merge bound, folded
and not taken, absent from the virtual's past two chain blocks on, or its parents left the selected chain) and **never while that block is a
tip**; a timer is no evidence. Duplicates are counted and logged.

## 2. What it costs and what it does not touch

Nothing, while dormant: fingerprints of every shipped preset are byte-identical (pinned), state roots and carriages are Some-only, no
header, block or RPC shape moves, and the producer and every flag are refused off a salted drill genesis.

## 3. Why the fences cannot arm

`Params::validate_palw_work_slice_v1` refuses to arm any of the three fences on every ruleset, a salted drill included, while
`PALW_WS_UNBUILT_V1` is non-empty (today: the slice verifier's seat side), and on every ruleset that is not a salted drill's while any of the
seven `PALW_WS_OPEN_ITEMS_V1` rows is open (tested: `palw_work_slice_fences`). What it protects against, in order of weight (spec §11/§11.1 has
the table and the detail):

1. **Nothing can license a slice.** No seat replays a slice, nothing carries the boundary state and the prompt to a seat, no court path
   adjudicates one (D7). A slice claim times out and voids its session; and because the lifecycle only checks the quorum's signatures, the first
   seat that signs `Valid` without replaying credits a slice nobody verified (the pipeline test that walks a slice to `Final` signs for the seats by
   hand, and the chain accepts it). Arming first would make a slice's credit depend on seats' honesty with nothing the chain can check.
2. **Header-time eligibility is half built.** Stage A keeps refused floors out of the chain (D6); stage B (REAL attempts and slices) is not built, so
   header-claimed, never-accepted attempts and slices still colour, and a bonded producer can still make statically eligible fake ones.
3. **Holding floors out removes the anchors lane A and the seed stand on.** Stage A answers the binding half, not the seed half;
   `palw_anchor_window_v1` (ADR-0170) is merged here and is not shown to cover it.
4. **Economics are undecided and unmeasured**: p = 1 at slice size W; a slice is metered by neither the class budget nor the work target; the
   capacity lanes keyed on an attempt do not see a slice claim; the job ledger grows by a row per root for ever, priced by the fee.
5. **Equivocation versus an honest re-mine is not separable in block data**, so the evidence is validated and not carried and nothing is slashed
   (D8 changes when this producer signs, not what is punishable); and nothing has been drilled or simulated: producer and panel stopped together,
   challenge/court throughput for slices, cheap private forks, header flooding, long-range sync.

**Not run — plainly:** stage B (not built); a pipeline run that crosses an activation height (every pipeline test arms at DAA 0 or 1; the `Legacy`
verdict is unit-tested only); a live drill; the item-1 simulations; a session root accepted through the real pipeline (the session is planted: opening
one needs a court-admitted IR class); a mutation check of S1.

## 4. Rejected

* **Colour-aware merging in GHOSTDAG (R3):** header-claimed, unverified REAL/slice blocks would be granted floor-transparent BLUE, turning
  "a floor made my REAL RED" into "a fake REAL made my REAL RED"; spec §7.1 compares it with merge admission and leaves the combination
  undesigned.
* **A slice that carries its own CCU:** the declared-weight attack of ADR-0137.
* **A per-slice lottery:** the single lottery is the network's; a decision, not a default (open item 4).
* **Evidence that slashes on a heuristic:** with no rule that tells equivocation from an honest re-mine, the evidence type is validated and
  nothing acts on it.
* **A re-mine on a timer ("one block per index per 30 s"):** shorter than a slot, it minted a duplicate while the first block was merely
  unmerged (S7 amendment, D8).

## Mission alignment amendment — 2026-10-07

work-sliceの既存core ledger再利用は維持するが、各sliceとそのboundaryの公開prosecutionを受入に含める。session無報酬やslice clockでclaimのmaturity・liability・証拠保持を短絡しない。

* 将来のlicense/Final、early weight、slice/claimの報酬解放は、証拠保持・proof期間・clock・collectible collateralと整合させる。多数派の署名で有効なfraud proofを無効にしない。DA default、算術conviction、false Validのscope別責任は区別し、verifier不在やローカルtimeoutをproducer fraudにしない。

本amendmentは衝突する将来の設計・受入条件を改定する。既存の実装・測定・fenceの記録はそのまま保持する。変更する合意規則は別のversioned移行を必要とする。Panel=0は[RFC14](../rfc/0014-panel-independent-fraud-prosecution.md)の全completion gatesと[RFC15](../rfc/0015-panel-free-permissionless-verification.md)固有gateが成立するまで有効化しない。
