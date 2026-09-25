# 01 — Time and the DAA score

*Normative chapter of the MISAKA Consensus Book. Reference implementation: t12 at `a0af3c92`
(see [PROVENANCE.md](../../PROVENANCE.md)); every `path:line` below is at that commit unless it says
otherwise. RFC 2119 keywords are normative.*

## 1. Purpose

Almost every rule in a Proof-of-LLM chain is a rule about time. A claim may be challenged for a
while and then becomes final; a panel seat must answer before a deadline; a retiring bond waits
before its collateral is spendable; an eligibility budget refills per epoch. In a permissionless DAG
nobody's wall clock is trusted, so the chain keeps its own clock — the *DAA score* — and every one of
those rules reads it. That makes the clock the most attacked number in the protocol: anything that
can move it cheaply can mature, expire, refill or convict whatever reads it.

The lesson t12 teaches (section 6) is that "time passed on this branch" and "the honest network had
a chance to act while it passed" are two different facts. A private branch can make the first true
at almost no cost; only the second protects a rule whose purpose is to give honest parties an
opportunity. This chapter therefore defines **three chain clocks, as three types**, plus one node
fact that is deliberately not a chain clock:

* **LocalDaa** — how many clock slots this branch has claimed. Cheap, permissionless, bounded by
  wall-clock time, and with *no authority* over anything a private branch could profit from.
* **SafeDaa** — the point up to which the branch's history has been vouched for by collateral-backed
  verification (panel licences). Appending heartbeat-only history does not advance it.
* **ChainFinalizedDaa** — the `LocalDaa` of the finalized anchor *that the block's own chain
  computes* (07 FINAL-R3), read at the block's selected parent. A pure function of the chain, so
  every node computes the same value for the same block; it reverts with its branch.
* **FinalizedDaa** (the node's) — the `LocalDaa` of the node's own finalized anchor (07 FINAL-R4). It
  never reverts on that node, and two honest nodes may hold different values. It is a node fact, so
  no consensus state transition may read it.

It also fixes which pure functions may mint each clock, and records, site by site, which clock every
t12 rule read and which one it must read in misaka-next (the *clock census*, §6.2).

## 2. Concepts and types

### 2.1 The quantities that look like time

| Quantity | Unit | Who chooses it | Is it a clock? |
|---|---|---|---|
| `Timestamp` | ms since the Unix epoch | the block producer, within bounds (DAA-R6, DAA-R7) | no — an input to the slot clock |
| `SlotIndex` | whole `SLOT_MS` intervals since the genesis timestamp | derived from a timestamp | no — the slot clock's grain |
| `LocalDaa` | clock slots claimed along the branch's selected chain | derived (DAA-R1) | yes, branch-local |
| `SafeDaa` | a `LocalDaa` value vouched for by `D_SAFE` later licences | derived (DAA-R11) | yes, safe |
| `ChainFinalizedDaa` | the `LocalDaa` of the chain's own finalized anchor, read at the selected parent | derived by the finality chapter (07 FINAL-R3) | yes, chain-final |
| `FinalizedDaa` | the `LocalDaa` of the node's finalized anchor | the node (07 FINAL-R4) | **no** — a node fact; no consensus rule reads it |
| `DaaSpan` | a number of slot ticks (a duration) | a parameter | a duration, not a point |
| `BlueScore` | blue blocks in a block's past | derived by GHOSTDAG | **no** — structural depth |

The name "DAA" (difficulty adjustment algorithm) is kept for continuity with Kaspa and t12. In
misaka-next it no longer names a difficulty adjustment: nothing is retargeted by it except the class
work target, which reads `SafeDaa` epochs (DAA-R17), and no clock is derived from a difficulty
(DAA-R8).

### 2.2 LocalDaa: a slot clock

Time is divided into slots of `SLOT_MS` (120,000 ms) counted from the genesis timestamp. A block
**claims** a slot when its own timestamp falls in a slot later than the last slot claimed on its
selected chain; claiming a slot is one *tick*. `LocalDaa(B)` is the number of ticks on `B`'s
selected chain. The header commits both `daa_score` and `clock_slot` (the last claimed slot), so the
clock is a two-field function of a header and its selected parent's header, and nothing else.

Why slots and not blocks: t12 began with Kaspa's rule (the score counts merged blocks) and every
lane that produced faster than intended ran the clock fast — at the attempt lane's projected rate one
"120-second" DAA would have been 12 s, with every window ten times short (ADR-0138 §1). A slot clock
cannot run faster than the slots it claims, and a timestamp can claim a slot at most `DRIFT_MS`
ahead of an honest node's wall clock, so `LocalDaa` is bounded above by elapsed real time
(INV-DAA-02) whatever the block rate.

Why a slot needs a block: a slot nobody claimed is lost, not banked (DAA-R3). The clock therefore
counts slots *in which the branch was demonstrably live*. After an outage the clock resumes where it
stopped instead of jumping, so a deadline cannot expire on a stretch during which nobody could have
posted anything.

**The heartbeat lane** exists to keep claiming slots when nothing else is produced: a bondless,
claimless, fee-only block at a fixed network-constant hash price (`2^24`), with no fork-choice
weight, at most four per mergeset. It is the chain's emergency generator (ADR-0060, ADR-0140).
Because the heartbeat is permissionless and cheap, **anything a heartbeat can advance is something a
private branch can advance for free**. That is the reason `LocalDaa` carries no authority over
rules an attacker could profit from (§2.7).

### 2.3 SafeDaa: time the honest network could have seen

A **licence** is the transition `UnverifiedClaim → VerifiedClaim` of an attempt claim (the claims
chapter): a panel drawn by stake signs the claim's verification, with collateral bound to the
signature. A licence is recorded at the `LocalDaa` of the **selected-chain block whose transition
performs it** (11 §4 step 5). A licence carried by a merged block is recorded at the accepting chain
block's `LocalDaa`, never at the merged block's own, which may be lower (DAA-R12). The chain state
keeps a **licence ring**: the recorded `LocalDaa` values of recent licences, in order.

> `SafeDaa(B) = max(ChainFinalizedDaa(B), LocalDaa of the D_SAFE-th most recent licence recorded on
> B's selected chain strictly before B)`, or `ChainFinalizedDaa(B)` when fewer than `D_SAFE` exist.

`SafeDaa(B)` is therefore a function of `B`'s selected parent's state. `B`'s own licences first move
its children's `SafeDaa`, so every rule in `B`'s transition reads one clock value fixed before the
transition starts (DAA-R18; 11 §4).

Read it as: "at least `D_SAFE` panels, each with collateral at stake, have signed verifications at or
after this point on this branch". To move `SafeDaa` past a value `x`, a branch needs `D_SAFE`
licences recorded at `LocalDaa ≥ x`. On the honest chain a licence needs a licensing set of seats,
drawn by stake, that replayed the job. On a branch the adversary produces, honest seats sign nothing,
so every licence comes from the adversary's own drawn seats: a licensing set forms with probability
`P_cap(s)` per admitted claim, including any seed selection the seed rule leaves it (10 §2.1 C11,
04 POL-R9). A fake-root licence's `Valid` signatures are convictable once the branch is published,
but a licence of honestly executed work by captured seats is not. So a private branch's safe clock
is bounded by `P_cap(s)` times its bucket rate, and never by its compute. Heartbeat blocks carry no
licence authority of their own: a heartbeat may *carry* a receipt object, but the licence's
authority is its signatures, not the lane (INV-TIME-06).

**`SafeDaa` copies the spacing of `LocalDaa`.** It is the `LocalDaa` of a licence-recording block.
So ticks inserted *before* a licence raise the value recorded for it, and for every later one, by
as many ticks (S5, §4). Appending blocks that record no licence moves nothing (S4). What the
insertion buys is bounded by INV-DAA-02: no branch's `LocalDaa` exceeds the wall-clock slot count by
more than one. Let branch `A` and a racing branch `A'` carry the same verified content.

* If `A` claims every slot, `A'`'s `SafeDaa` leads by at most the one-slot drift lead.
* If `A` left `m` slots unclaimed, the lead is at most `m + 1`.
* If `A'` re-times its licences, bunching its last `D_SAFE` just before a point, it reaches
  `SafeDaa ≈ LocalDaa` there. That is a lead of at most `A`'s own licence lag plus the two terms above.

Honest heartbeat producers claiming every slot on the honest chain (10 §2.2 H6) keep the first two
terms at one. The invariants that read `SafeDaa` are stated as these bounds, not as equalities
(INV-CLAIM-01, INV-TIME-06, INV-FORK-01).

Licences, not `Final`, and the reason is circularity: `Final` is reached when a challenge window has
elapsed, so if the safe clock were minted from `Final` claims (t12's `safe_frontier`, §6.3 F2) the
window would be measured by a clock that the window itself advances. A licence depends only on
bind/receipt windows, which are liveness deadlines; `Final` then *consumes* `SafeDaa`.

`SafeDaa` lags `LocalDaa` by the time `D_SAFE` licences take. In a licence halt it stops, except
that it never falls below `ChainFinalizedDaa`; nothing economic concludes on history nobody vouched
for (the stance t12 already took for its second clock, `config/params.rs:14700-14711`).

### 2.4 ChainFinalizedDaa and the node's FinalizedDaa

"Finalized" names two different facts, and they get two types.

* **`ChainFinalizedDaa(B)`** is the `LocalDaa` of `block_finalized_anchor` (07 FINAL-R3) evaluated on
  `B`'s selected parent's chain. It is a pure function of that chain's state, so every node computes
  the same value for `B`, archival or pruned. It is non-decreasing along a chain (07 FINAL-R6) and
  reverts with its branch. It reads the *parent's* anchor, not `B`'s, because `B`'s own safe anchors
  move `B`'s anchor, and a rule in `B` that read them would be circular. Every state rule that needs
  "final" time reads it: the floor of `SafeDaa`, vesting release, exit, seat-lock expiry, registration
  maturity, coinbase maturity and DA defaults (§2.7).
* **`FinalizedDaa`** is the `LocalDaa` of the **node's** finalized anchor (07 FINAL-R4): the deeper of
  the node's previous anchor and the chain anchor of the tip it selected. It never decreases on that
  node across any reorg. It depends on the node's history, so two honest nodes can hold different
  values at the same moment. If a state rule read it, those two nodes would compute different states
  for one block. It is therefore a separate type: not a `Daa<C>`, no `Deadline`, no conversion. Only
  node-side decisions read it: confirmations, admissibility of candidates (06 FORK-R1), the node's own
  retention, and 07 FINAL-R4/R5.

### 2.5 SafeContext: comparing two branches

When fork choice compares candidate chains, each candidate has its own `LocalDaa` and `SafeDaa`.
Evaluating a time-dependent predicate (is this claim final? has this deadline passed?) at either
candidate's own clock lets the branch that ran its clock hardest win on that predicate — the seed
invariant INV-FORK-01. An earlier draft of this chapter proposed a common clock reading (the minimum
`SafeDaa` over the candidate set). `06-fork-choice.md` §3.2 shows that every such reading fails —
pairwise it is intransitive, set-wide one junk candidate drags it, over "contenders" a stalled leader
drags it — so the book adopts 06's resolution: the one `SafeContext` per selection is the
**finalized anchor** (06 FORK-R3), it carries no clock reading, and fork choice reads no clock at all
(06 FORK-R8); maturity is measured as burial by verified weight (06 §3.3). This chapter's
per-block clock readings are a different type, `ClockContext` (§2.6), used only by single-branch
rules.

### 2.6 Types

```rust
pub struct Timestamp(pub u64);          // ms since the Unix epoch, producer-chosen
pub struct SlotIndex(u64);              // floor((ts - genesis_ts) / SLOT_MS)
pub struct DaaSpan(pub u64);            // a duration, in ticks
pub struct BlueScore(pub u64);          // structural depth — never compared with any Daa

mod sealed { pub trait Sealed {} }
pub trait ClockKind: sealed::Sealed {}
pub enum Local {}  pub enum Safe {}  pub enum ChainFinal {}   // each impls Sealed + ClockKind

/// A point on one chain clock. The field is private: the only constructors are the mint
/// functions of §4 (Local, Safe) and 07's `chain_finalized_daa` (ChainFinal).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Daa<C: ClockKind> { ticks: u64, _kind: PhantomData<C> }
pub type LocalDaa = Daa<Local>;
pub type SafeDaa = Daa<Safe>;
pub type ChainFinalizedDaa = Daa<ChainFinal>;

/// A deadline carries the clock that judges it, chosen once where it is created.
pub struct Deadline<C: ClockKind> { at: u64, _kind: PhantomData<C> }
impl<C: ClockKind> Deadline<C> {
    /// THE ONLY constructor. Marks are always the creating block's LocalDaa (the only value a
    /// block knows for certain); the kind says which clock must pass mark + span.
    pub fn after(mark: LocalDaa, span: DaaSpan) -> Self;
    pub fn elapsed(self, now: Daa<C>) -> bool;      // now.ticks > self.at
}

pub struct ClockState { pub daa: LocalDaa, pub last_slot: SlotIndex }   // header-committed
/// The clock readings of ONE block on ONE branch, the input of single-branch rules (deadlines,
/// refills, epochs). `safe` and `chain_final` are functions of the selected parent (DAA-R18).
/// Not a fork-choice input: fork choice's `SafeContext` is 06's (the node's anchor).
pub struct ClockContext { pub local: LocalDaa, pub safe: SafeDaa, pub chain_final: ChainFinalizedDaa }

// In consensus/finality (07), not here: the node's anchor DAA. Not a Daa<C>: no Deadline, no
// conversion, no arithmetic. Minted only by FinalizedAnchor::daa; no state transition takes it.
pub struct FinalizedDaa(u64);
```

There is no `From`/`Into` between clock kinds and no public arithmetic on a `Daa<C>`: a `Daa<C>` can
be compared with a `Daa<C>` of the same kind and nothing else. The only way to make a window is
`Deadline::<C>::after(mark: LocalDaa, span)`. There is **no** `Daa<C> + DaaSpan` for any `C`. A window
marked on `SafeDaa` or `ChainFinalizedDaa` would collapse whenever that clock jumps
(`safe_mark_window_collapse`), so it cannot be written. So:

```rust
bucket.refill(ctx.safe);              // compiles: ClaimBucket::refill(now: SafeDaa)
bucket.refill(header_clock.daa);      // does not compile: expected SafeDaa, found LocalDaa
challenge_end.elapsed(block.local);   // does not compile when challenge_end: Deadline<Safe>
let end = ctx.safe + span;            // does not compile: no Add<DaaSpan> for Daa<C> (compile-fail test)
exit_permitted(.., node.anchor().daa, ..); // does not compile: FinalizedDaa is not ChainFinalizedDaa
```

`BlueScore` has no conversion to or from any `Daa`, which makes t12's
`finality_depth = window_challenge / 2` (§6.3 F6) unwritable.

### 2.7 Clock authority

This table is the one place that states what each clock may decide. Every consensus rule that reads
time names its row; the clock census (§6.2) is the t12 audit against it.

| Kind of rule | Clock | Why |
|---|---|---|
| Liveness timeouts whose only effect is an **uncharged** void or release of a pending object (a claim's wait for its seed ring, `NoRing`; activation lookahead) | `LocalDaa` | nobody is charged and nothing is released to anyone if a private branch expires it |
| Header-level shape rules (heartbeat must tick, slot claims, round index) | `LocalDaa` / `Timestamp` | must be decidable from headers alone |
| Rules that **punish absence** of another party (receipt timeout charged to anyone, court no-show, inactivity leak, reclamation of an idle class, readiness expiry that drops a seat from draws) | `SafeDaa`; DA defaults `ChainFinalizedDaa` (05 COURT-R11) | absence on a private branch is not evidence of absence |
| Rules that **grant authority on the branch** (`Final`, the redraw, the close of a `FinalClaim`'s conviction window) | `SafeDaa` | a private clock must not buy maturity or end an honest party's chance to act |
| Rules that **release value out of consensus control or admit a party** (vesting release, exit, seat-lock expiry, execution-quantum and other right maturity, coinbase maturity, registration maturity for draws) | `ChainFinalizedDaa` | release is what a private branch wants to buy; it waits until the history it rests on is final on that chain |
| **Eligibility** refills and epoch boundaries (claim buckets, work-target and receipt-target retargets, audits) | `SafeDaa` | INV-CLAIM-01 |
| Pruning | none: the committed pruning point is structural (07 FINAL-R10); a node's own retention reads its `FinalizedDaa` (node policy) | data must outlive every rule that can still read it |
| Anything evaluated while comparing chains | none: fork choice reads no clock; its only context is 06's `SafeContext` (the node's anchor) | INV-FORK-01 |
| Fork-choice weight, finality depth | none of these clocks: verified weight and safe anchors (06, 07) | `BlueScore` is structure, not time |

No consensus state transition may take the node's `FinalizedDaa`. This table is the only statement of
clock authority. 03 §2.5 (claim authority) and 02 (stake) cite its rows and do not restate them.

## 3. Normative rules

**DAA-R1 (the slot clock).** For every block `B` with selected parent `P`:
`clock(B) = clock_step(clock(P), B.timestamp)` where a tick happens iff
`slot_of(B.timestamp) > clock(P).last_slot` and `B`'s lane is clock-eligible (DAA-R4). Genesis has
`daa = 0` and `last_slot = slot_of(genesis_ts) = 0`. *Because:* a block-count clock runs at the
producers' rate (ADR-0138 §1).

**DAA-R2 (one tick per block).** A tick increments `daa` by exactly one and sets `last_slot` to the
slot of `B.timestamp`, however many slots were skipped. *Because:* counting skipped slots would let
one late block expire every deadline of an outage at once (ADR-0138 §3b, ADR-0142 §4).

**DAA-R3 (missed slots are lost).** No rule may credit a slot that no selected-chain block claimed.
*Because:* a banked backlog lets a returning producer run the clock arbitrarily fast just when the
windows it feeds are most stretched (ADR-0142 §4 "why the skip term").

**DAA-R4 (clock-eligible lanes).** Heartbeat and attempt blocks are clock-eligible. Lanes that carry
no chain position (round blocks, receipt blocks) MUST NOT tick and MUST carry their selected
parent's `ClockState` unchanged. A heartbeat block MUST tick: a heartbeat whose timestamp does not
claim a new slot is invalid. An attempt block claims a slot on a selected chain only if it is on that
chain. A lost-lottery attempt header is invalid (11 BLK-R2). An attempt whose admission is refused
makes its block ineligible for any selected chain (11 BLK-R5). So neither ever claims a slot, although
the header's clock fields are still computed by `clock_step` alone (INV-DAA-06). *Because:* a beat
that cannot tick is pure blue-score and relay load, which was 89% of t12's beats before the floor
(ADR-0142 §9.1, §9.3 rule 1). A refused attempt costs a fabricator a few hashes, so it must not tick.
The choice to let attempt blocks tick is Open Question Q1.

**DAA-R5 (the clock is committed and has one implementation).** A header carries `daa_score` and
`clock_slot`; validation MUST refuse a header whose fields differ from `clock_step` over its own
timestamp and its selected parent's committed fields. Template construction MUST call the same
`clock_step`. *Because:* t12's construction and validation drifted apart twice (ADR-0142 §5), and a
clock derived from node-local stores split archival and pruned nodes twice (ADR-0066 finding 4,
ADR-0142 §6a check 3).

**DAA-R6 (future drift is an admission deferral).** A node MUST NOT admit a header whose timestamp
exceeds its local wall clock by more than `DRIFT_MS`; it MUST defer such a header, not mark it
invalid. `DRIFT_MS` MUST be strictly less than `SLOT_MS`. This check is node policy outside the pure
consensus functions (INV-TIME-01). *Because:* drift is the only thing that lets a timestamp claim a
slot ahead of real time; with `DRIFT_MS < SLOT_MS` the lead is at most one slot, once, never a rate.
t12's 132 s drift against a 120 s slot allowed two (ADR-0142 §6a check 4, §9.3 rule 3).

**DAA-R7 (median time).** A header's timestamp MUST exceed the past median time of its selected
chain window. *Because:* it keeps producer-chosen timestamps from running backwards and bounds the
round lane's index (DAA-R16).

**DAA-R8 (no clock from difficulty, no difficulty from the local clock).** No consensus value is
derived from a PoW target, and there is no `header.bits`. Lane prices are network constants
(heartbeat `2^24`). The attempt lottery's work target is retargeted per `SafeDaa` epoch (DAA-R17).
*Because:* in t12 heartbeat rows in the global difficulty window priced the bonded lane off its own
chain (ADR-0066 F1, measured 33,554,432 vs 2), and a retarget that reads a clock the lane itself
advances is a feedback loop.

**DAA-R9 (the heartbeat's authority is exactly LocalDaa).** A heartbeat block MUST be fee-only, MUST
carry no fork-choice weight (there is no block-level work in misaka-next; 06 FORK-R9), MUST NOT count
toward any depth used as a safety bound, and at most `HEARTBEAT_MAX_PER_MERGESET = 4` heartbeats may
appear in one mergeset. Its only consensus effect beyond carrying transactions and objects is
claiming slots. *Because:* ADR-0140's claims C1–C4 (no difficulty, no weight, no earnings, no
production capture), made structural.

**DAA-R10 (the authority table binds).** Every consensus rule that reads time MUST take the clock
type §2.7 assigns it. Every rule that punishes absence, grants authority on the branch or refills
eligibility MUST take `SafeDaa` (or a `Deadline<Safe>`). Every rule that releases value out of
consensus control or admits a party MUST take `ChainFinalizedDaa` (or a `Deadline<ChainFinal>`). No
consensus state transition may take the node's `FinalizedDaa`. *Because:* INV-TIME-07, INV-CLAIM-01;
a rule that read the node's anchor would split two honest nodes on one block.

**DAA-R11 (the only SafeDaa mint).** `SafeDaa` MUST be produced only by `safe_daa` (§4) from the
licence ring as of the selected parent, the depth `D_SAFE` and the `ChainFinalizedDaa` floor.
*Because:* a second constructor is a second definition of "safe", and t12 grew three (safe frontier,
settled-anchor floor, second-clock depth with an escape).

**DAA-R12 (the licence ring).** The chain state MUST carry, per selected chain, one entry per licence
of an attempt claim: the `LocalDaa` of the selected-chain block whose transition performed the licence
(for a licence carried by a merged block, the accepting chain block's, never the merged block's own).
Entries MUST be pruned only when no rule can still ask about them (t12's rule: keep every entry inside
the bind-plus-receipt horizon and the `D_SAFE` newest entries older than it,
`palw_state_v2.rs:2233-2246`).
*Because:* `safe_daa` must be answerable in `O(log n)` from rooted state on every node, including one
that joined by snapshot. A merged block's own `LocalDaa` can be lower than its merging block's, so
recording it would move the ring backwards and break S1.

**DAA-R13 (no escape into the local clock).** No rule may fall back from `SafeDaa` to `LocalDaa`
because the safe clock stalled. *Because:* t12's liveness escape (`palw_state_v2.rs:2224-2229`)
reverts to the DAA clock after `2 × window_court` without a licence, a stretch any heartbeat-only
branch produces for 2 × 2^24 hashes a tick (§6.3 F3). What replaces it is Open Question Q2.

**DAA-R14 (comparisons read no clock).** No time-dependent predicate may be evaluated inside chain
comparison at a candidate's own `LocalDaa` or `SafeDaa`, nor at any clock reading derived from the
candidates. The comparison's only context is 06's `SafeContext` (the finalized anchor), and its keys
take no clock (06 FORK-R3, FORK-R8). *Because:* INV-FORK-01; 06 §3.2.

**DAA-R15 (units do not mix).** No rule may compare, add or assign values of different time units
(`Timestamp`, `SlotIndex`, `Daa<_>` of different kinds, `DaaSpan`, `BlueScore`). A parameter derived
across units MUST go through a named function that states the rate it assumes, with a test that
fails when the measured rate differs. *Because:* t12 set a blue-score finality depth from a DAA
challenge window and a blue-score pruning depth from a DAA claim lattice (§6.3 F6); the rates differ
by 3–6× on the live chain (ADR-0142 §9.1).

**DAA-R16 (the round lane's clock).** The execution lane's round index MAY be derived from the
timestamp (`(ts - genesis_ts) / ROUND_MS`); it is bounded by DAA-R6 and DAA-R7 and MUST NOT feed any
`Daa` value. *Because:* rounds are a throughput grain, not a clock with authority.

**DAA-R17 (epochs are safe).** An eligibility epoch index is `SafeDaa / EPOCH_LEN`. Every epoch-keyed
rule (budgets, progressive release, work-target and receipt-target retargets, reclamation, admission
audits) MUST read it. A refill that sees `SafeDaa` jump MUST cap at the bucket's capacity `B`
(INV-CLAIM-02). *Because:* in t12 each of these read the local clock (§6.2 C35–C39), so a private
branch that ticks with heartbeats and produces little eased its own lottery (§6.3 F5).

**DAA-R18 (one clock reading per block).** Every rule applied in block `B`'s transition MUST read the
one `ClockContext(B) = (LocalDaa(B), SafeDaa(B), ChainFinalizedDaa(B))`. `LocalDaa(B)` comes from
`B`'s header. `SafeDaa(B)` and `ChainFinalizedDaa(B)` are functions of `B`'s selected parent's state.
Licences and safe anchors that `B` itself produces first move the clocks of `B`'s children (11 §4
step 0). *Because:* a transition that read a clock its own objects move would depend on the order of
its own steps, and `B`'s anchor would depend on `B`'s rules (a cycle).

## 4. Pure functions

All functions below are pure: inputs to output, no store, network, RPC or wall clock.

```rust
pub const SLOT_MS: u64 = 120_000;
pub const DRIFT_MS: u64 = 60_000;          // MUST be < SLOT_MS (DAA-R6); value is Q3
pub const D_SAFE: u32 = 30;                // >= 1; t12's PALW_T12_SETTLED_ANCHOR_DEPTH; value is Q2

pub fn slot_of(ts: Timestamp, genesis: Timestamp) -> SlotIndex;
pub fn clock_step(parent: ClockState, ts: Timestamp, lane: Lane, genesis: Timestamp) -> ClockStep;
pub fn check_header_clock(fields: &HeaderClockFields, parent: ClockState, ts: Timestamp,
                          lane: Lane, genesis: Timestamp) -> Result<ClockState, ClockError>;
pub fn safe_daa(parent_ring: &LicenceRing, depth: u32, floor: ChainFinalizedDaa) -> SafeDaa;
pub fn licence_ring_push(ring: &LicenceRing, at: LocalDaa, keep: DaaSpan, depth: u32) -> LicenceRing;
pub fn common_safe_daa(candidates: impl IntoIterator<Item = SafeDaa>) -> Option<SafeDaa>; // diagnostic only; no rule reads it (DAA-R14)
pub fn epoch_of(now: SafeDaa, len: DaaSpan) -> EpochIndex;
// Node admission, NOT a consensus function: it reads the node's wall clock.
pub fn admit_timestamp(ts: Timestamp, local_now: WallClockMs) -> Admission; // Now or Defer(until)
```

**`slot_of`** — `(ts.saturating_sub(genesis)) / SLOT_MS`. A timestamp before genesis is slot 0.

**`clock_step`**

```text
clock_step(parent, ts, lane, genesis):
    if not lane.clock_eligible():            # DAA-R4: rounds, receipts
        return ClockStep { state: parent, ticked: false }
    s = slot_of(ts, genesis)
    if s > parent.last_slot:
        return ClockStep { state: ClockState { daa: parent.daa + 1, last_slot: s }, ticked: true }
    return ClockStep { state: parent, ticked: false }
```

Properties (each is a test): (P1) `daa` is non-decreasing and increases by at most one per block;
(P2) `daa ≤ last_slot` for every header, so at admission time `t`, `LocalDaa ≤ (t + DRIFT_MS −
genesis_ts) / SLOT_MS`; (P3) a non-ticking block returns its parent's state unchanged, so it cannot
postpone the next tick (ADR-0142 §2); (P4) one tick per block whatever the gap; (P5) the result
depends only on the block's timestamp and lane and its selected parent's committed fields; (P6)
deterministic.

**`check_header_clock`** — `clock_step`, then: if `lane == Heartbeat && !ticked`, refuse
`HeartbeatDidNotTick`; if `fields != step.state`, refuse `ClockMismatch`. Returns the state the
children read.

**`safe_daa`** — over the ring as of `B`'s selected parent: let `n = ring.len()`; if `n < depth`
return `floor.as_safe()`; else return `max(floor.as_safe(), Safe(ring[n − depth]))`. `as_safe` is
private to this module (the only ChainFinal→Safe path). Properties, each a test:

* (S1) Non-decreasing along a chain. Licences are recorded at the performing chain block's
  `LocalDaa`, which is non-decreasing (DAA-R12), and the floor is non-decreasing (07 FINAL-R6).
* (S2) `ChainFinalizedDaa(B) ≤ SafeDaa(B) ≤ LocalDaa(parent(B)) ≤ LocalDaa(B)`.
* (S3) Moving it past `x` needs `depth` licences recorded at `LocalDaa ≥ x` on that chain, or chain
  finality past `x`.
* (S4) **Append invariance.** Appending blocks that perform no licence and move no chain anchor
  leaves `SafeDaa` unchanged for every block appended (heartbeat-only suffixes in particular).
* (S5) **Insertion bound.** Inserting `k` ticking blocks before a licence raises the recorded
  `LocalDaa` of that licence and of every later one by at most `k`, so `SafeDaa` of every later block
  by at most `k`. `SafeDaa` is **not** invariant under insertions: it copies `LocalDaa`'s spacing
  (§2.3). With INV-DAA-02 this bounds a racer's lead over a branch with the same verified content by
  the slots that branch left unclaimed plus one (§2.3). A branch that also re-times its licences can
  add at most the compared branch's licence lag.

**`licence_ring_push`** — insert `at` (≥ every entry by S1), then drop the oldest entries older than
`at − keep` beyond the newest `depth` of them. The result MUST give the same `safe_daa` as the
unpruned ring for every `now ≥ at − keep`.

**`common_safe_daa`** — the minimum over the set; `None` for an empty set. It is **not** a
fork-choice input: 06 §3.2 shows a set-wide minimum is dragged to the anchor by one junk candidate.
It survives only as a diagnostic (how far the slowest candidate's safe clock lags), and no consensus
rule may read it.

**`Deadline::elapsed`** — `now.ticks > at`, matching t12's sweep, which resolves a deadline strictly
below the block's score (`palw_state_v2.rs:23666`). The mark is always the creating block's
`LocalDaa`, and `ChainFinalizedDaa ≤ SafeDaa ≤ LocalDaa` (S2). So neither a `Deadline<Safe>` nor a
`Deadline<ChainFinal>` can elapse before the branch's own clock has run its span (INV-TIME-08). A
mark taken on `SafeDaa` or `ChainFinalizedDaa` would not have this property. With that clock lagging
by `L` ticks, a window of span `s < L` could elapse almost as soon as it was opened
(`safe_mark_window_collapse`, 10-attack-model.md §6). That is why no `Daa<C> + DaaSpan` exists.

## 5. Invariants upheld

Full statements and test mapping live in `09-invariants.md`.

| ID | Statement | Test |
|---|---|---|
| INV-DAA-01 | `LocalDaa` is non-decreasing along every selected chain and increases by at most one per block. | `inv_daa_01_local_daa_is_monotone_and_steps_by_at_most_one` |
| INV-DAA-02 | Every header satisfies `daa_score ≤ clock_slot`; hence an admitted chain's `LocalDaa` never exceeds `(now + DRIFT_MS − genesis_ts) / SLOT_MS`. | `inv_daa_02_local_daa_never_outruns_the_wall_clock` |
| INV-DAA-03 | A block that does not tick leaves the clock state unchanged: it cannot postpone the next tick. | `inv_daa_03_a_block_that_does_not_tick_does_not_postpone_the_next` |
| INV-DAA-04 | Missed slots are lost: one tick per block, however many slots it skipped. | `inv_daa_04_missed_slots_are_lost_not_banked` |
| INV-DAA-05 | Template construction and header validation compute the clock with one function and agree on every input. | `inv_daa_05_template_and_validation_compute_one_clock` |
| INV-DAA-06 | The clock of a block is a function of its header and its selected parent's header only; a pruned node, a snapshot node and an archival node compute the same value. | `inv_daa_06_clock_depends_only_on_header_and_selected_parent` |
| INV-TIME-01 | No consensus function reads the wall clock; the future-drift check defers admission and never marks a header invalid. | `inv_time_01_no_consensus_function_reads_the_wall_clock` |
| INV-TIME-02 | Values of different time units cannot be compared, added or assigned (compile-fail suite). | `inv_time_02_time_units_do_not_mix` |
| INV-TIME-03 | `SafeDaa` is minted only by `safe_daa`; advancing it past `x` requires `D_SAFE` licences recorded at `LocalDaa ≥ x` on that chain, or chain finality past `x`. | `inv_time_03_safe_daa_advances_only_with_licences` |
| INV-TIME-04 | `ChainFinalizedDaa ≤ SafeDaa ≤ LocalDaa` at every block. | `inv_time_04_finalized_le_safe_le_local` |
| INV-TIME-05 | A node's `FinalizedDaa` never decreases, across every reorg; `ChainFinalizedDaa` is non-decreasing along every chain. | `inv_time_05_finalized_daa_never_decreases` |
| INV-TIME-06 | Appending blocks that perform no licence and carry no verified weight advances neither `SafeDaa` nor `ChainFinalizedDaa` (S4); inserting `k` ticks before a licence raises later `SafeDaa` by at most `k` (S5). | `inv_time_06_heartbeat_only_history_advances_no_safe_clock` |
| INV-TIME-07 | Every rule that punishes absence, grants or releases, or refills eligibility reads `SafeDaa` or `ChainFinalizedDaa` per §2.7; no state transition reads the node's `FinalizedDaa`. | `inv_time_07_absence_release_and_refill_rules_read_the_safe_clock` |
| INV-CLAIM-01 | Private DAA advancement MUST NOT increase lottery eligibility beyond the S5 bound. Clock half: every refill and epoch rule takes `SafeDaa` (DAA-R17), and a racer's `SafeDaa` lead over a branch with the same verified content is at most the slots that branch left unclaimed plus one. | `inv_claim_01_private_daa_does_not_create_tickets` |
| INV-FORK-01 | A branch MUST NOT gain comparative maturity solely because its private DAA is ahead. Clock half: fork choice reads no clock (DAA-R14, 06 FORK-R8). | `inv_fork_01_private_daa_does_not_buy_comparative_maturity` |
| INV-TIME-08 | No deadline is shorter, on its own branch's `LocalDaa`, than its span: a `Deadline<C>` marked at block `B` with span `s` is not elapsed at any descendant whose `LocalDaa ≤ LocalDaa(B) + s`. | `inv_time_08_no_window_is_shorter_than_its_span` |

## 6. t12 reference

### 6.1 How t12 keeps time

**The score.** A block's DAA score is its selected parent's score plus the number of its mergeset
blocks inside the DAA window, minus an exemption count (`consensus/src/processes/difficulty.rs:44-56`).
The window is the 264 blue blocks below the block (`difficulty.rs:316-320`; `config/params.rs:9900`,
sample rate 1); blocks below it, and round blocks, are `mergeset_non_daa` and never count
(`difficulty.rs:333-349`). That is Kaspa's rule plus two t12 layers:

1. **ADR-0138, the anchor clock.** Past `palw_anchor_clock` a merged block counts only if `bits`
   priced its lane (`difficulty.rs:705-722`; predicate `pow_layer0.rs:428-430`). Attempt (6, 9),
   receipt (7), heartbeat (8) and round (10) lanes are all unpriced. The heartbeat stands in for a
   missing priced block: a mergeset with no priced block and at least one heartbeat gets one
   exemption back (`difficulty.rs:459-466`), so at most one tick per block.
2. **ADR-0142, the cursor, and its 2026-09-24 floor.** Past `palw_clock_cursor` the stand-in is
   granted only if the newest merged beat is stamped at or after the cursor
   (`difficulty.rs:502-505`). The cursor is derived, not stored: the reference is the block at the
   selected parent's score with the lowest blue score (past `palw_clock_floor`, ties broken by the
   earliest timestamp, then hash), and the next slot opens 120 s after its timestamp
   (`palw_clock_cursor_v1.rs:120-152`; `difficulty.rs:476-500`). Past the floor a heartbeat must be
   stamped at or after its slot (H3, `consensus/src/pipeline/header_processor/pre_pow_validation.rs:82-87`)
   and so must the block that steps the clock (H5, `pre_pow_validation.rs:94-97`).

**What t12 arms.** Testnet-12 arms every fence at DAA 0 (`config/params.rs:15940-15944`), except bond
maturity at 1,000 (`config/params.rs:15795-15796`). `set_palw_single_lottery` arms the lottery and the
anchor clock together (`config/params.rs:5229-5232`); the RC base arms the cursor beside them
(`config/params.rs:15536-15538`), and the floor is armed by name (`config/params.rs:15909`).

**So what advances the DAA score on t12? Only the heartbeat lane.** A ConsensusV2 bundle accepts its
attempt id and its receipt id (`consensus/core/src/palw_mode_v2.rs:1462-1464`); the heartbeat, exec
and round lanes are ORed in behind their fences (`pre_ghostdag_validation.rs:154-187`). The one
`bits`-priced lane, algo 3, is not admitted, so `palw_lane_advances_daa_v1` is false for every
admissible block and the score moves exactly when a mergeset carries a granted heartbeat
(`difficulty.rs:459-517`). ADR-0138 §3c measured the same composition on testnet-11 (60/60 selected
blocks algo 6, none algo 3). A heartbeat does not tick its own score; the block that merges it (the
*step*) does — in steady state another heartbeat, "two beats a slot" (ADR-0142 §9.5).

**Cost.** A heartbeat is `2^24` evaluations of the algo-3 tag at a network-constant target, never
`header.bits` (`pow_layer0.rs:562-575`); ADR-0142 §9.5 measured 5–15 s of one core. It is fee-only
(`body_validation_in_context.rs:78-86`), weighs `ε = 1` against an attempt block's `2^20`
(`ghostdag/protocol.rs:585-670`; `pow_layer0.rs:618`), and at most four may share a mergeset unless
they form one chain paced by its own timestamps (`post_pow_validation.rs:153-195`;
`palw_heartbeat_v1.rs:330-332`). An attempt block costs `W / CCU` expected full inferences
(`palw_work_target_v1.rs:98-130`) and **advances the score by zero**. One DAA tick on t12 therefore
costs about `2 × 2^24` hashes on any branch, honest or private.

**The other quantities.** `header.bits` is vestigial on t12: with no priced row in the window the
retarget answers `max_difficulty_target` (`difficulty.rs:583-585`). The class DAA module
(`palw_class_daa.rs`) is arithmetic over realized-production censuses in blocks and permille shares,
with no timestamp (`palw_class_daa.rs:1-30`); the fold applies it once per closed DAA epoch
(`palw_state_v2.rs:23012-23021`). The execution lane counts rounds in whole seconds since the
genesis timestamp (`palw_execution_lane_v1.rs:162-164`) and schedule spans in DAA
(`palw_execution_lane_v1.rs:167-169`; 1 DAA on t12, `config/params.rs:15988-15990`). The epoch budget
(ADR-0123) is attempt blocks per epoch of `epoch_length` DAA (1,000 on the RC,
`palw_fp_devnet_v3.rs:293`), released progressively at `p = DAA mod L`
(`palw_state_v2.rs:22497-22528`). ADR-0133's "own clock" for verification is spans of that same
DAA: a class's receipt window is `max(window_receipt, spans × span_daa)`
(`palw_state_v2.rs:1529-1539`). The *second clock* counts licences of attempt claims
(`palw_state_v2.rs:7875-7884`, ticked at `palw_state_v2.rs:18930-18940`), depth 30 on t12
(`config/params.rs:14711`).

### 6.2 Clock census

`Next` is the clock the rule MUST read in misaka-next (§2.7). `Exploitable` says whether a private
branch (P) or heartbeat blocks on the honest chain (H) can move the rule's outcome on t12.
Consensus-core paths are under `consensus/core/src/`; pipeline paths under `consensus/src/`.

| # | Site | Rule | Reads | Next | Exploitable on t12 |
|---|---|---|---|---|---|
| C01 | `processes/difficulty.rs:44-56` | DAA score = sp score + in-window mergeset − exempt | mergeset lanes | Local (`clock_step`) | P: yes, ~2×2^24 hash per tick; H: sole source, rate bounded |
| C02 | `processes/difficulty.rs:705-722` | does a lane tick on its own | algo id, fences | none | no (false for every V2 lane) |
| C03 | `processes/difficulty.rs:459-466,502-517` | heartbeat stand-in grant, ≤1 per block | newest beat ts, cursor | Local | P: yes; H: rate closed by floor |
| C04 | `processes/difficulty.rs:476-500`; `palw_clock_cursor_v1.rs:140-152` | reference = lowest-blue block at parent score; slot = ref + 120 s | window daa/blue/ts | Local (`clock_slot` in header) | closed (H5 tie rule) |
| C05 | `palw_clock_cursor_v1.rs:118-122,195-224`; `processes/difficulty.rs:502-505` | no reference in window ⇒ grant, H3/H5 void | window reach (264 blue) | none | yes: one free tick after 264 blue at one score |
| C06 | `processes/difficulty.rs:316-349` | DAA window bounded by blue score | blue score | none | no |
| C07 | `processes/difficulty.rs:351-384,549-596` | `bits` retarget; no priced row ⇒ max target | window timestamps | none (no bits) | closed (ADR-0066/0083) |
| C08 | `pipeline/header_processor/pre_pow_validation.rs:30-33` | header `daa_score` equals computed | computed score | Local | no |
| C09 | `pipeline/header_processor/pre_pow_validation.rs:82-87` | H3: heartbeat stamped ≥ slot | ts vs cursor | Local (DAA-R4) | closed |
| C10 | `pipeline/header_processor/pre_pow_validation.rs:94-97` | H5: step stamped ≥ slot | ts vs cursor | subsumed by DAA-R1 | closed (was §9.2 acceleration) |
| C11 | `pipeline/header_processor/pre_pow_validation.rs:98-130` | pre-cursor slot rule `sp.ts + interval` | sp ts, lane | none | inert on t12 |
| C12 | `pipeline/header_processor/pre_ghostdag_validation.rs:434-440` | ts ≤ `unix_now()` + 132 s | wall clock | admission deferral (DAA-R6) | yes: ≤2-slot one-time lead |
| C13 | `pipeline/header_processor/post_pow_validation.rs:24-30` | ts > past median time | window timestamps | Local | no |
| C14 | `pipeline/header_processor/post_pow_validation.rs:153-195` | ≤4 beats a mergeset unless one paced chain | beat timestamps | Local | closed (F3a, H3) |
| C15 | `processes/ghostdag/protocol.rs:338` | blue score += mergeset blues, beats included | blue set | none (structure) | H: beats add ~2 blue per tick |
| C16 | `processes/ghostdag/protocol.rs:585-670` | beat work ε = 1, attempt 2^20 | lane | none (fork choice) | no |
| C17 | `processes/block_depth.rs:59-80` | merge root / finality point at blue depth | blue score | none: safe anchors and verified weight (07 FINAL-R2) | H: depth reached sooner in wall time |
| C18 | `processes/pruning.rs:112-145` | pruning point at blue depth | blue score | none: structural (07 FINAL-R10) | H: prunes sooner than the DAA lattice [needs probe] |
| C19 | `config/params.rs:2781` | `finality_depth` (blue) = `window_challenge` (DAA) / 2 | two units | none (DAA-R15) | unit mix |
| C20 | `config/params.rs:2697-2718` | pruning depth (blue) ≥ claim lattice (DAA) | two units | none: structural, above every unresolved claim (07 FINAL-R10) | unit mix; pending P-1 keeps it |
| C21 | `config/params.rs:56-58` | `ForkActivation::is_active(daa)`, every fence | header DAA | Local (Q4) | P: reaches heights early; low on t12 (all at 0 but one) |
| C22 | `pipeline/body_processor/body_validation_in_context.rs:78-86` | subsidy = `calc_block_subsidy(daa)`; beats and rounds 0 | header DAA | Local | no [schedule assumed non-increasing, unverified] |
| C23 | `processes/transaction_validator/tx_validation_in_header_context.rs:25-38` | lock time by DAA or median time | DAA / PMT | Local | no protocol value |
| C24 | `dns_finality.rs:4123-4144` | coinbase spend: age ≥ maturity; DAA-only long fallback | DAA age | ChainFinal (11 BLK-R8) | P: yes |
| C25 | `dns_bft_v1.rs:130-132` | leak evidence window = blue span ∪ DAA span | blue + DAA | Safe | P: yes |
| C26 | `dns_bft_v1.rs:381-388` | leak if `anchor_daa − last ≥ t_leak_daa` | DAA | Safe | P: yes, honest validators leaked |
| C27 | `palw_state_v2.rs:23662-23668` | claim deadline sweep at the block's score | DAA | per phase (C28–C30) | — |
| C28 | `palw_state_v2.rs:23672-23675` | bind timeout voids a Provisional claim, uncharged | DAA | Local (next: the seed-ring wait `NoRing`, 04 POL-R9) | low |
| C29 | `palw_state_v2.rs:23676-23743` | receipt timeout: redraw, then `void_and_slash` the producer | DAA | Safe | P: yes, producer slashed for panels' absence |
| C30 | `palw_state_v2.rs:23766-23772`, `1393-1398` | `Final` at licensed + `window_challenge_at` (120) | DAA | Safe | P: yes, self-finalization |
| C31 | `palw_state_v2.rs:1529-1539` | class receipt window = max(W_r, spans × span_daa) | DAA spans | Safe | P: yes (as C29) |
| C32 | `palw_state_v2.rs:21933-21960` | court deadlines: the silent side loses | DAA | Safe | P: yes |
| C33 | `palw_state_v2.rs:23438-23444` | DA session default at its deadline | DAA | ChainFinal (05 COURT-R11) | P: yes |
| C34 | `palw_state_v2.rs:23388-23395` | claim unbound past its anchor slot is voided | DAA | Local | low |
| C35 | `palw_state_v2.rs:22497-22528` | epoch budget release at `p = DAA mod L` | DAA | Safe | P and H: yes (INV-CLAIM-01) |
| C36 | `palw_state_v2.rs:22736-22775` | work target `W` steps per closed DAA epoch; eases on silence | DAA epoch | Safe | P: yes, down to the floor |
| C37 | `palw_state_v2.rs:23012-23021,23117-23121` | class and pooled receipt retargets per DAA epoch; silence eases | DAA epoch | Safe | P: yes |
| C38 | `palw_state_v2.rs:22890-22905` | reclaim a class silent for `reclaim_epochs` epochs | DAA epoch | Safe | P: yes (pending R1 narrows) |
| C39 | `palw_state_v2.rs:16920-16922` | admission jury every 100 DAA | DAA spans | Safe | P: timing only |
| C40 | `palw_state_v2.rs:24633-24638` | class activation ≤ 4,000 DAA ahead | DAA | Local | no |
| C41 | `palw_state_v2.rs:2379-2388` | withdrawal after `since_daa + delay` | DAA | ChainFinal (02 BOND-R14) | mitigated by C42 |
| C42 | `palw_state_v2.rs:2196-2214` | and `depth` licences since retirement | licences | Safe (precursor) | escape C43 |
| C43 | `palw_state_v2.rs:2224-2229` | second clock off after `2 × window_court` without a licence | DAA | none (DAA-R13) | P and H: yes |
| C44 | `palw_panel_var_v1.rs:246-259` | second clock holds ≤ DAA release + `2 × window_court` | DAA, licences | ChainFinal plus licence count (02 BOND-R8, BOND-R12) | bounded by C43 |
| C45 | `palw_panel_v2.rs:656-658,678-700` | seat maturity: `anchor_daa − 1,000`, widened to the 30th licence | DAA, ring | ChainFinal (02 BOND-R4) | DAA part yes (`:663-665` says so); mitigated |
| C46 | `palw_vesting_v1.rs:484-530` | vesting row matures: expiry DAA ∧ second clock ∧ no halt | DAA, licences | ChainFinal plus licence count (02 BOND-R12) | via C43 once a licence resumes (`palw_vesting_v1.rs:494`, `:509`) |
| C47 | `palw_state_v2.rs:18930-18940` | a licence settles an anchor at its DAA | DAA | Local (ring input) | no |
| C48 | `palw_state_v2.rs:20919-20946` | safe frontier = blue score of deepest `Final` | blue of Finals | replaced by `safe_daa` | P: yes, via C30 |
| C49 | `palw_execution_lane_v1.rs:162-164` | round = whole seconds since genesis | timestamp | Timestamp (DAA-R16) | minor: pre-stamp ≤ drift |
| C50 | `palw_execution_lane_v1.rs:167-169` | schedule span = DAA / span_daa | DAA | Safe | P: permit timing |
| C51 | `palw_court_deadline.rs:191` | `moves × turn_deadline + reserve < window_court` | DAA spans | Safe, plus INV-DAA-02 | no (needs DAA ≤ wall clock) |
| C52 | `palw_model_registry_v1.rs:779-799`, `:859` | a readiness row stands 8 spans (`PALW_READINESS_V2_MAX_AGE_SPANS_V1`; 8 DAA on t12's one-DAA spans, `:787`, `:811`); past `palw_audit_2026_09_23` it gates the panel draw (`:781-782`) | DAA | Safe (expiry drops a party from draws: absence) | P: yes. On a private heartbeat branch honest rows lapse after 8 ticks and only rows the forker refreshes stay drawable (`private_readiness_lapse_panel_capture`) [whether SW-10's eligible base counts only fresh rows is unverified] |
| C53 | `palw_economic_safety_v1.rs:95-113` | execution quanta spendable from `final + window_challenge` | DAA | ChainFinal (08 ECON-R7) | P: yes, via C30 |
| C54 | `palw_freeprompt_v3.rs:974-987` | free-prompt draw slot `final + receipt_maturity`; win usable in `[beacon, beacon + use_window]` | DAA | ChainFinal for the draw slot; the use window is an uncharged lapse (Local) | P: slot timing |
| C55 | `palw_model_registry_v1.rs:750-768` | registry governs past `grace_until` (activation + readiness age); lifecycle ages | DAA | Safe (a lapse that removes a class or seat is absence) | P: timing |
| C56 | `palw_model_benefits_v1.rs:223-258` | model-benefit tier needs `tenure_daa ≥ min_hold_daa`; lapse at `expires_daa` or a missed cadence | DAA | ChainFinal (tenure grants a benefit); lapses Safe | P: tenure accrues on heartbeats [consensus effect of a benefit tier not traced] |
| C57 | `palw_model_market_v1.rs:592` | a model sell fills only while `point_daa ≤ not_after_daa` (window ≤ 4,000 DAA, `:574`) | DAA | Local (the holder's own signed bound; an uncharged lapse) | low |

### 6.3 Findings

**F1. The emergency generator is the mains.** ADR-0140 C2 says the heartbeat "must not pace the
clock while something else is pacing it". On t12 nothing else paces it (§6.1): every DAA-denominated
rule in the census is paced by an unbonded lane at `2^24` hashes a beat. The floor (§6.1 item 2)
makes that pace wall-clock-bounded on the honest chain; it does not make it expensive anywhere.

**F2. `Final` is local.** The deadline sweep finalizes a licensed claim once the block's own score
passes `licensed + window_challenge_at` (`palw_state_v2.rs:23766-23772`). The fold's own comment
says so: `Final` is "reached by the DAA sweep alone and must not settle anything on history nobody
signed" (`palw_state_v2.rs:19338-19342`). The safe frontier — fork choice's first key — is the blue
score of the deepest `Final` (`palw_state_v2.rs:20919-20946`), so it inherits the local clock.

**F3. The second clock escapes into the first.** The settled-anchor clock (licences) is the right
idea and the precursor of `SafeDaa`, but after `2 × window_court` DAA with no licence it switches off
(`palw_state_v2.rs:2224-2229`), and the fold says in as many words that "a stretch of `2 ×
window_court` DAA an attacker drives with heartbeats releases the escape"
(`palw_panel_var_v1.rs:241-243`). It also gates only locks, withdrawals, seat maturity and vesting,
not `Final` (F2).

**F4. Absence is punished on the local clock.** Receipt timeout (C29), court no-show (C32), DA
default (C33), the inactivity leak (C26) and class reclamation (C38) all convict or strip a party
for not acting before a local deadline. On a private branch the honest party could not act at all.

**F5. Eligibility refills on the local clock.** The progressive release grows with `DAA mod L`
(C35), and "receipt and heartbeat blocks advance `pos`" by design (`palw_state_v2.rs:22490-22491`);
`W` eases by the clamp each closed epoch with few model blocks, down to its floor (C36,
`palw_work_target_v1.rs:98-111`); the pooled receipt target eases on a silent epoch (C37). A private
branch that ticks with heartbeats and produces little makes its own lottery cheaper.

**F6. Units mixed.** `finality_depth` (blue) is set to `window_challenge / 2` (DAA)
(`config/params.rs:2781`), and the pruning depth (blue) is floored by a DAA claim lattice
(`config/params.rs:2697-2718`). Blue score counts heartbeats and attempts, DAA counts granted slots;
ADR-0142 §9.1 measured blue running 3–6× the design rate. ADR-0138 §3c accepted the blue axis
explicitly; the DNS leak window already needed a blue-∪-DAA patch (`dns_bft_v1.rs:113-132`).

**F7. The derived reference can be absent.** When the window holds no block at the parent's score,
the grant is unconditional and both floors are void (C05). Reaching it takes 264 blue blocks at one
score, i.e. a stretch in which producers merge no heartbeat.

**F8. One clock, five repairs.** t12's clock was broken and repaired five times in eight days: the
freeze and the double pace (ADR-0138 §3b), busy-producer starvation (ADR-0142 §1), store divergence
on pruned nodes (ADR-0142 §6a check 3), acceleration and sibling delay (ADR-0142 §9.2). Each repair
added a derivation. misaka-next replaces the derivations with a committed two-field state (DAA-R1,
DAA-R5), under which the invariants those repairs chased are properties of one function (P1–P6).

### 6.4 Where next differs, and why

| t12 | misaka-next | Reason |
|---|---|---|
| score counts in-window mergeset blocks minus exemptions | `clock_step` over own timestamp and parent's committed state | F8; INV-DAA-06 structurally |
| a beat ticks via the block that merges it (the step) | a clock-eligible block ticks by claiming a slot with its own timestamp | no step, no "two beats a slot" |
| reference derived from a 264-blue window | `clock_slot` committed in the header | F7; ADR-0142 §6a's concern met by the header |
| only heartbeats advance the score | heartbeats and attempt blocks claim slots (Q1) | the busy lane keeps the clock without a beat |
| drift 132 s > interval 120 s | `DRIFT_MS < SLOT_MS` | lead ≤ 1 slot |
| `header.bits` validated, constant max | no `bits` | DAA-R8 |
| one `u64` for every clock | `Daa<Local/Safe/ChainFinal>`, the node's `FinalizedDaa`, `BlueScore`, `Timestamp` | F6; DAA-R15; a rule that read a node's anchor would split honest nodes |
| second clock with a DAA escape | `SafeDaa` from licences; no escape | F3; DAA-R13 |
| `Final`, absence, refills on the local score | on `SafeDaa` | F2, F4, F5 |
| vesting, exit, coinbase and right maturity on the local score (plus a licence count) | on `ChainFinalizedDaa` (plus the licence count for vesting) | F3; release waits for chain finality |
| a licence recorded at its carrying block's score | recorded at the performing chain block's `LocalDaa` (DAA-R12) | S1 |

### 6.5 Pending t12 deltas

* **`feat/t12-class-verify-deadline` (pending).** Class-derived verification deadlines `D(c)` in
  reference spans, and P-1: the pruning depth at regenesis becomes the `D_cap` claim lattice, 74,920
  on testnet-12 (commit `2c4b6516`). The lattice is DAA and the depth is still counted in blue score,
  so F6 persists.
* **`feat/t12-activation-pool` (pending).** R1 exempts non-admitting registry rows and genesis rows
  from reclamation (commit `5a248235`), narrowing C38 without changing its clock; R2 audits a
  Candidate at its own staggered span (commit `87036309`), changing C39's schedule, not its clock.
* `feat/t12-aheld-node` and `rcore/p2-file` were not read for clock changes [unverified].

## 7. Attacks this chapter defends against

Details and reproductions are in `10-attack-model.md`.

* `heartbeat_clock_acceleration` — **partial on t12.** Rate closed on the honest chain by the floor
  (C09, C10, C04); the heartbeat is still the only source of the clock (F1), every census row reads
  it, and the absent-reference grant (C05) skips the floor. Next: DAA-R1–R6, DAA-R9, DAA-R10.
* `private_daa_finality_acceleration` — **partial on t12** (co-owned with fork choice). A private
  branch cannot outrun the wall clock (INV-DAA-02 analogue holds past the floor), but it reaches the
  whole wall-clock score for `2 × 2^24` hashes a tick while honest ticks lag by grind and propagation
  (ADR-0142 §9.5), and it finalizes claims no honest party could challenge (F2). Next: `Final` on
  `Deadline<Safe>`, DAA-R14.
* `clock_slot_rule_freeze` (this chapter's earlier name: `busy_producer_clock_starvation`) — closed
  on t12 by the cursor (ADR-0142 §1–§2). Next: P3.
* `heartbeat_future_stamp_step` — closed on t12 by H5 (C10). Next: slot index, `DRIFT_MS < SLOT_MS`.
* `heartbeat_sibling_step_delay` — closed on t12 by the earliest-timestamp tie (C04). Next: no
  reference to delay.
* `heartbeat_width_burst` — closed on t12 (C14). Next: width bound and DAA-R4.
* `heartbeat_rows_price_bonded_lane_out` (earlier name: `heartbeat_bits_lockout`) — closed on t12
  (C07). Next: not applicable (no `bits`).
* `heartbeat_only_trap` (earlier name: `heartbeat_red_trap`) — closed on t12: past
  `palw_heartbeat_transparent` (armed at 0,
  `config/params.rs:15855`) a heartbeat never turns a bonded block red (ADR-0105). Next: fork-choice
  chapter, under DAA-R9.
* `clock_reference_window_escape` — real on t12 (C05, F7), low severity. Next: committed state.
* `second_clock_heartbeat_escape` — real on t12 (C43, F3) for locks and exits. Vesting is blocked during the halt itself (`palw_vesting_v1.rs:494`, `:509`) and matures once one licence resumes. Next: DAA-R13.
* `private_readiness_lapse_panel_capture` — unverified on t12 (C52). Next: readiness expiry reads `SafeDaa` (§2.7).
* `private_absence_conviction` — real on t12 (F4), effective only if the branch wins fork choice.
  Next: DAA-R10.
* `private_work_target_easing` — real on t12 (C36, F5); INV-CLAIM-01 violated, bounded by `W`'s
  floor and the per-epoch clamp. Next: DAA-R17, plus difficulty-aware weight (fork choice).
* `blue_depth_unit_mismatch` — partial on t12 (F6), accepted in ADR-0138 §3c. Next: DAA-R15.

## 8. Open questions for the project owner

**Q1. Which lanes may claim a slot?** (a) Heartbeats only, as t12 — the clock then depends on the
bondless lane even while production runs (F1). (b) Heartbeat and attempt blocks — an attempt
envelope is validated against its header's own timestamp and nonce
(`pipeline/header_processor/pre_ghostdag_validation.rs:310-326`), so the producer fixes the
timestamp before its draw [that the ticket binds it is ADR-0072 Decision 8, not re-read here], the
busy lane keeps the clock and the heartbeat only fills gaps. (c) Any block. *Recommend (b):* it
satisfies ADR-0140 C2 for real, and a slot costs a private branch one heartbeat under every option,
so security is unchanged.

**Q2. What happens in a licence halt?** `SafeDaa` stops (DAA-R13), and so does verified weight, so
nothing matures, finalizes, vests or withdraws. (a) Accept the freeze; the `ChainFinalizedDaa` floor moves it if finality still advances.
(b) Let a validator-signed finality overlay advance `ChainFinalizedDaa` independently of licences, which
(a) already absorbs. (c) t12's timed escape — rejected: a private heartbeat branch drives it.
*Recommend (a), with `D_SAFE = 30` retuned from measured licence cadence, never from a calendar.*

**Q3. `SLOT_MS` and `DRIFT_MS`.** *Recommend* `SLOT_MS = 120,000` (t12's recovery interval, every
window in the RC lattice is sized in it) and `DRIFT_MS = 60,000`. Any `DRIFT_MS < SLOT_MS` bounds the
lead to one slot; smaller values defer more headers from hosts with bad clocks.

**Q4. What clock activates a fence?** (a) The block's `LocalDaa` (t12, header-decidable). (b) The
block's `ChainFinalizedDaa` (reorg-proof; a function of the parent's chain). *Recommend:*
launch with no scheduled fence; if one is ever needed, (b).

**Q5. What replaces blue-score depths?** Merge depth, finality depth and pruning depth are blue-score
counts that heartbeats inflate (C15–C20). *Recommend:* finality in safe anchors and verified weight
(07 FINAL-R2), pruning at the structural point above every unresolved claim (07 FINAL-R10), and
merge bounds in blocks (11 BLK-R3). No blue-score depth survives.

**Q6. Should heartbeats exist at all?** Without them a chain whose producers all stop has a frozen
`LocalDaa` and cannot carry the transactions that restart it (ADR-0060, ADR-0064 Fact A). *Recommend:*
keep them, with exactly the authority of DAA-R9: they tick the local clock and carry transactions,
and nothing a private branch could profit from reads what they tick.

**Q7. Is `SafeDaa`'s spacing bound acceptable?** `SafeDaa` copies `LocalDaa`'s spacing (S5, §2.3), so
a racer gains at most the slots the compared branch left unclaimed, plus one, plus the compared
branch's licence lag if it re-times licences. (a) Accept the bound and require honest heartbeat
coverage of every slot (10 §2.2 H6). (b) Cap `SafeDaa`'s advance per licence (`MAX_SAFE_STEP`). This
does not remove the lead when licences are dense, and when they are sparse the honest lag grows
without bound, which stretches every honest window. (c) A licence-count clock for eligibility and
absence. It is spacing-free, but a bucket refilled per licence refills itself, so it no longer
bounds the rate. *Recommend (a).* The bound is small and testable, and the other two options break
either honest windows or the rate control. *[Synthesis edit, review: property S4 claimed invariance
under insertions, which is false; this question records the choice made.]*
