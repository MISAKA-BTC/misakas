# 03 — Claims: lifecycle, typestate, authority, and issuance capacity

> Normative. RFC 2119 keywords. Rule IDs `CLAIM-R*`, invariant IDs `INV-CLAIM-*`. Citations of the
> form `path:line` refer to the t12 reference tree at `a0af3c92` (see `../../PROVENANCE.md`);
> `0152:line` refers to `docs/adr/0152-account-stake-staged-reserve-and-vested-rewards.md` on
> `docs/adr-0152-v31-postedits` @ `9ed1adce`. A statement marked **[unverified]** was not checked
> against code. Clocks (`LocalDaa`, `SafeDaa`, `ChainFinalizedDaa`) are defined in `01-time-and-daa.md`,
> whose §2.7 is the only statement of which clock a rule may read; this chapter cites its rows.

## 1. Purpose

A *claim* is the chain's record that a bonded producer says it ran a piece of LLM work. The claim is
the unit on which Proof-of-LLM hangs everything that matters: fork-choice weight, the producer's
reward, the panel that checks the work, and the court that punishes a lie. The danger is that a claim
exists on chain *before* anybody has checked it — the lottery that admits it can be won with roots that
were never computed (chapter 04, §2.4). So the whole safety argument reduces to one discipline:

> **What a claim may do is a function of how much of it has been proven, and nothing else.**

This chapter defines the claim's states as distinct Rust types, states in one table exactly which
authority each type carries (§2.5), and bounds how many claims the chain may admit per unit of *safe*
time (§2.6). Chapter 04 explains where claims come from; `06-fork-choice.md`, `05-panel-validation.md`,
`02-bonds.md` (collateral, slashing, vesting) and `08-economics.md` consume the authorities granted here.

## 2. Concepts and types

### 2.1 The five facts, and where a claim sits among them

PoL separates five facts that t12's vocabulary often blurred (chapter 04 §2.1 is the full treatment):

| fact | type (next) | observable by consensus? |
|---|---|---|
| (a) the model was executed | `Execution` (off-chain) | never directly |
| (b) a commitment/root is held | `ExecutionCommitment` | yes — bytes in a block |
| (c) a lottery was won | `LotteryWin` | yes — a hash comparison |
| (d) the claim was verified by the panel | `LicenceCertificate` with `BasisK ≥ 2` → `VerifiedClaim` | yes — signed receipts |
| (e) the claim has consensus effect | `FinalClaim` (+ its authority rows) | yes — derived state |

A claim is created from (b)+(c) plus a bond, a collateral reservation and a capacity token. Fact (a)
enters consensus **only** through (d). No rule may treat (b) or (c) as evidence of (a).

### 2.2 The claim core

Every claim carries one immutable core, fixed at admission and snapshotted (never re-derived from
later state — t12 learned this through release/reserve drift, `palw_state_v2.rs:3932-4000`):

```rust
pub struct ClaimCore {
    pub id: ClaimId,                    // H(canonical attempt body), identity only
    pub lane: Lane,                     // Attempt | FreePrompt { quanta: u32 }
    pub class: ClassId,
    pub bond: BondKey,
    pub commitment: ExecutionCommitment,// roots + job identity (04 §2.2)
    pub work: DerivedWork,              // pwu; constructible only by derive_work() (04 §4)
    pub admission: AdmissionIndex,      // chain-assigned, in acceptance order; the per-claim seed input (04 POL-R9)
    pub accepted_at: ChainPoint,        // (LocalDaa, block) — ordering, indexing, deadline MARK
    pub accepted_safe: SafeDaa,         // the block's SafeDaa (its ClockContext.safe) — target in force, audit
    pub reserved: Collateral,           // snapshotted at admission, released byte-for-byte
    pub escrow: Escrow,                 // snapshotted carve of the carrying block's subsidy
}
```

`accepted_at` is a `LocalDaa`-bearing point and exists so that deadlines index deterministically.
It is also every deadline's *mark*, per 01 §2.6: a window is `Deadline::<C>::after(mark, span)`, and
CLAIM-R9 fixes which clock `C` must pass it. A window that grants authority or charges collateral is a
`Deadline<Safe>`, and one that releases value a `Deadline<ChainFinal>`, so a fast `LocalDaa` cannot
close it (the mark only sets where it ends). The mark
is deliberately *not* `accepted_safe`: with `SafeDaa` lagging `LocalDaa` by more than a window's span,
a window measured from `accepted_safe` could close almost as soon as it opened (INV-TIME-08,
`safe_mark_window_collapse`). *[Synthesis edit: an earlier draft of this chapter measured windows
from `accepted_safe`; aligned with 01 §2.6.]*

### 2.3 The typestates

**These are the only definitions of the claim types in the book.** 05 §2.2 and 04 §2.5 refer to them.
Only `consensus/claims` constructs them. The `pol/*` crates sit upstream of it (00 §7), so they take
and return plain records and proof tokens (`ClaimRecord`, `BoundRecord`, `LicenceProof`, `SeedLeaf`),
never a typestate.

```text
                 admit()              verify(proof) [basis_k ≥ 2]           mature() [challenge_ends elapsed
LotteryWin ───────────────▶ UnverifiedClaim ─────────────────────▶ VerifiedClaim ───────────────▶ FinalClaim   on SafeDaa, no open court]
 (04, header)                │  AwaitingRing                        │                               │
                             │  PanelBound (05's `BoundClaim`)      │                               │ convict() before
                             │  Optimistic (1 replay, not enough)   │                               │ conviction_ends
                             │ expire()/void()          convict()   │ convict()                     ▼
                             ▼                                      ▼                          ConvictedClaim
                         VoidedClaim                          ConvictedClaim
```

```rust
pub struct UnverifiedClaim { core: ClaimCore, stage: UnverifiedStage }
pub enum UnverifiedStage {
    /// Waiting for its seed ring (04 POL-R9). Uncharged void `NoRing` when `ring_by` elapses (01 §2.7 row 1).
    AwaitingRing  { ring_by: Deadline<Local>, redraws_left: u8 },
    /// Bound by the chain itself in the block that fixed its ring (05 PANEL-R11). 05's `BoundClaim`.
    PanelBound    { panel: Panel, bound_at: LocalDaa, receipts_by: Deadline<Safe>, redraws_left: u8 },
    /// One full replay seat said Valid. A fast path to *reservation release*, never to Final (OQ-21).
    Optimistic    { panel: Panel, licence: OptimisticLicence, replay_by: Deadline<Safe>, redraws_left: u8 },
}
pub struct VerifiedClaim  { core: ClaimCore, cert: LicenceCertificate, basis_k: BasisK,
                            licensed_at: LocalDaa, challenge_ends: Deadline<Safe> }
pub struct FinalClaim     { core: ClaimCore, cert: LicenceCertificate, basis_k: BasisK,
                            final_at: LocalDaa, conviction_ends: Deadline<Safe>, vesting: VestingRow }
pub struct VoidedClaim    { core: ClaimCore, at: LocalDaa, reason: VoidReason }        // terminal
pub struct ConvictedClaim { core: ClaimCore, at: LocalDaa, from: ConvictedFrom, conviction: ConvictionRef } // terminal
```

Every mark is the creating block's `LocalDaa`, and every window is `Deadline::<C>::after(mark, span)`
(01 §2.6):

* `challenge_ends = after(licensed_at, W_challenge)`;
* `conviction_ends = after(final_at, W_conviction)`;
* the vesting row's `expiry` is a `Deadline<ChainFinal>` after `final_at` with a horizon of at least
  `W_conviction` (02 §4.7).

A window measured from a `SafeDaa` value could close almost at once after a licence burst
(`safe_mark_window_collapse`), and none can be written.

Fields are private. Each type is constructible only by the transition function that produces it
(§4.1). `LicenceCertificate`, `BasisK` and `Panel` are defined in `05-panel-validation.md` §2.2. The
licence check `check_licence` (05 §4) returns a `LicenceProof`, and only `verify` turns an
`UnverifiedClaim` plus that proof into a `VerifiedClaim`. A `VerifiedClaim` exists only when
`basis_k ≥ 2`: at least two distinct seat indices replayed every segment (05 PANEL-R14). `mature`
takes `now: SafeDaa` (CLAIM-R9). `VoidReason` splits into
`Uncharged { NoRing | NoCapablePanel | FirstPanelFailed }` and `Forfeit { SecondPanelFailed |
NotReplayBacked | Withholding }`. A proven fraud is not a void but a `ConvictedClaim`.

**Why a separate `ConvictedClaim`.** t12 records a post-Final conviction by rewriting `Final` to
`Voided` and subtracting its weight (`palw_state_v2.rs:16218-16242`), while the module header still
calls `Final` "terminal and permanent" (`palw_state_v2.rs:48-58`). next makes the conviction a typed
transition out of `FinalClaim`, so no reader can assume that a `FinalClaim` it saw stays one. Following
`05-panel-validation.md` COURT-R10, a conviction of a claim whose fork-choice entry is already **safe**
(06 §3.3) debits money (vesting rows, locks) and does **not** roll back weight. A conviction object
that one branch carries and another omits must not make the honest branch the lighter one (§8 Q5).
Weight is frozen at safety, not at `Final`. `Final` is a SafeDaa-timed fact, and fork choice reads no
clock.

### 2.4 Authorities

An *authority* is anything a claim can cause in consensus. The complete list:

| id | authority | consumed by |
|---|---|---|
| A1 | block-level work of its carrying block (DAG ordering weight, e.g. GHOSTDAG blue work). **None exists in next** (06 FORK-R9); the row is kept for the t12 column | 06, 11 |
| A2 | live weight (tip selection among candidates with equal safe history) | 06 |
| A3 | safe weight | 06 |
| A4 | contribution to the safe clock | 01 |
| A5 | holding / advancing the claim frontier (the pruning and finality boundary) | 07 |
| A6 | escrow: withheld, vested, minted, or burned | 02, 08 |
| A7 | randomness contribution (a seed for panels, receipt draws, schedules) | 04, 05 |
| A8 | input to the lottery-target controller and to capacity observation | 04, §2.6 |
| A9 | capacity consumption (in-flight room, per-bond exposure) | §2.6, 02 |
| A10 | downstream rights (free-prompt quanta spend, execution-lane credit, class probe/share credit) | 04, 08 |
| A11 | standing in court: may be accused, and what a conviction reaches | 05 |

### 2.5 The authority table

This table is **the one place** the authority of a claim type is stated. The code MUST encode it
once (`crates/consensus/claims/src/authority.rs`), and every function that exercises an authority
MUST take the claim type that carries it (§4.2), so that exercising an authority a type lacks is a
compile error, not a review finding.

**next** (`0 ≤ β_u < β_v ≤ 1`, `w` = the claim's `DerivedWork`; "safe" = 06 §3.3, buried by `d_bury`
and not held by an unresolved dispute):

| | `LotteryWin` (not yet admitted) | `UnverifiedClaim` | `VerifiedClaim` | `FinalClaim` | `VoidedClaim` | `ConvictedClaim` |
|---|---|---|---|---|---|---|
| A1 header work | none; a lost-lottery header is invalid (11 BLK-R2) | none | none | none | none | none |
| A2 live weight | 0 | `β_u·w` with `β_u = 0` by default (06 FORK-R7; §8 Q2) | `β_v·w` (06's `β`) until safe, then counted in safe (live = safe + bounded) | as `VerifiedClaim`: `Final` does not change weight | 0 | 0, unless already safe |
| A3 safe weight | 0 | 0 | `w` once safe (06 FORK-R4) | `w` once safe; `Final` status does not enter | 0 | 0 if convicted before it became safe; `w` stays if already safe (frozen history, COURT-R10) |
| A4 safe clock | none | none | its licence enters the licence ring that mints `SafeDaa` (01 DAA-R11) | none further | none | none |
| A5 claim frontier | — | holds it below `accepted_at` (07 §2.3) | holds it | holds it until `conviction_ends` elapses | resolved | resolved; the frontier never retreats |
| A6 escrow | none | withheld (reserved) | withheld; reservation may drop to `w+rr` | vesting row (not spendable), released when `ChainFinalizedDaa` passes its expiry (02 BOND-R12; 01 §2.7) | never minted | never minted; vesting row burned |
| A7 seed | **none** | **none** | **none** | its nonce-free execution commitment, as one leaf of a lagged ring (04 POL-R9) | none | none from now on; a seed already derived from it stays derived |
| A8 controller input | none | none (recommended; §8 Q3) | counted | counted | none | none |
| A9 capacity | consumes one bucket token at admission | holds in-flight room + reservation `w+esc+rr` | holds room until replay; reservation `w+rr` or `w+esc+rr` | releases room and reservation (vesting lock remains, 02) | releases (uncharged) or forfeits | forfeits per 05/02 |
| A10 downstream rights | none | none | none | quanta spendable within a use window, from their maturity on `ChainFinalizedDaa` (08 ECON-R7); execution-lane credit; probe/share credit | none | revoked from the conviction on; unspent quanta lapse |
| A11 court | — | accusable | accusable | accusable until `conviction_ends` (`Deadline<Safe>`) elapses | not accusable (resolved) | terminal |

**t12** (what the reference actually grants; `β = 100‰`, `palw_fp_devnet_v3.rs:34`):

| | attempt header, not admitted | `Provisional` / `PanelBound` | `ReceiptLicensed`, S2 awaiting replay | `ReceiptLicensed`, counted (`basis_k ≥ 2` or V1) | `Final` | `Voided` | `Final`→`Voided` (post-Final conviction) |
|---|---|---|---|---|---|---|---|
| A1 blue work | **2²⁰** (`protocol.rs:666-670`); lottery never checked at header stage (`consensus/pow/src/lib.rs:594-595`) | 2²⁰ | 2²⁰ | 2²⁰ | 2²⁰ | 2²⁰ | 2²⁰ |
| A2 live | 0 (merged loser skipped, `processor.rs:11630-11652`) | `⌊β·pwu/1000⌋` into `bounded_immature` (`palw_state_v2.rs:28436-28444`, `:2035-2037`) | same | same | via safe | 0 (`:19351-19400`) | 0 |
| A3 safe | 0 | 0 | 0 | 0 | `pwu` (`:19193-19197`) | 0 | subtracted (`:16218-16228`) |
| A4 second clock | — | — | no | ticks `settled_attempt_finals` (`:18926-18940`, `:15088-15111`) | — | — | — |
| A5 frontier | — | holds (`:20919-20944`) | holds | holds | may set | resolved | frontier not retreated (`:16203-16206` doc) |
| A6 escrow | — | withheld from coinbase (`coinbase.rs:256-268`) | withheld | reservation may drop (`0152:867-872`) | vesting row past `palw_rcore_plus` (`:19212`, `:19323`) | never minted | row burned (0152 V-rules) **[burn path not re-read]** |
| A7 seed | — | **its block hash seeds panels and receipt beacons if it is the slot's first attempt chain block** (`processor.rs:10021-10045`, `palw_panel_v2.rs:1502-1508`, `:1667`, `palw_fp_beacon_v3.rs:119-126`) | same | same | same | same (the block stays) | same |
| A8 controller | — | counted in `epoch_counters` at acceptance (`:28543-28558`), never uncounted on void | counted | counted | counted | still counted | still counted |
| A9 capacity | — | panel room, per-bond class share, exposure ceiling (`palw_admission_v2.rs:587-705`; `:28490-28535`) | room held until replay (0152 T-2(c)) | room freed outside C7 | released | released or forfeited | — |
| A10 rights | — | none | none | none | FP quanta spend (`:26855-26857`); execution-lane credit (`:19327`) | none | FP spent set cleared (`:16234-16236`) |
| A11 court | — | DA court and panel court | yes | yes | yes, while its row is unmatured (0152 §2 DA row) | — | — |

**Where next is stricter, and why.**

1. **A1: no header work at all.** t12 gives every shape-valid, signed attempt
   header 2²⁰ blue work at the header stage; the class lottery, the bond and the exposure checks run
   only in the virtual processor (`processor.rs:11505-11552`). A merged loser is skipped for claims but
   keeps its blue work. t12's own comment names the hazard ("a shape-valid header that never faces the
   lottery", `protocol.rs:633`). next: no block-level work exists; a lost-lottery header is invalid
   and a refused attempt confers nothing (CLAIM-R3; `failed_lottery_blue_weight`, §7).
2. **A7: unverified work never seeds randomness.** In t12 an admitted-but-unverified attempt block —
   possibly fake-rooted — seeds every panel anchored on it, and its hash is re-rollable at the price of
   a signature (chapter 04 §6.4). next: only `FinalClaim` commitments, nonce-free, with lag.
3. **A8: the controller counts verified work.** t12 counts claims at acceptance and never uncounts a
   voided one, so fabricated wins steer the work target (`palw_state_v2.rs:22736-22775`).
4. **A2 split by proof.** t12 prices `Provisional` and a counted licence identically. next sets
   `β_u = 0` by default (06 FORK-R7: unverified claims are absent from `ChainView`) and leaves
   `0 < β_u < β_v` as the open alternative (§8 Q2).
5. **A3/A11: conviction after Final is a typed transition** (`ConvictedClaim`), not a rewrite.
6. **Every authority-granting or charging deadline reads `SafeDaa`, every value release
   `ChainFinalizedDaa`** (CLAIM-R9). t12 reaches `Final` by a `LocalDaa` sweep
   (`palw_state_v2.rs:23662-23771`).
7. **A3: weight is frozen at safety, not at `Final`.** Fork choice reads no clock (06 FORK-R8), so the
   point after which a conviction no longer moves weight is burial by verified weight (06 §3.3).

### 2.6 Issuance capacity

*Issuance capacity* is how many claims (hence escrows, hence minted reward and weight) the chain may
admit per unit of time. t12 had four overlapping controls; next keeps two, each with one clock.

**(i) The lane bucket — a rate, on the safe clock.** One `ClaimBucket` per lane (attempt, free-prompt,
and the liveness floor's own lane), network-wide, not per class and never per bond:

```rust
pub struct ClaimBucket { milli_tokens: u64, capacity: u64 /* B, milli */, rate: u64 /* milli per SafeDaa */, last: SafeDaa }
```

Admission takes 1000 milli-tokens; refill is `min(B, tokens + rate·(now − last))`. A refused attempt
loses its ticket (the ticket is position-bound, 04 §2.3); nothing is banked.

**The rate bound in local ticks.** A refill credits at most `B`. `SafeDaa` steps only when a licence
is recorded, or when the chain anchor moves, which with `k_final ≥ D_SAFE` (07 FINAL-R2) never binds
after bootstrap. After a lag of any length, `D_SAFE` licences recorded later bring `SafeDaa` back to
at least the `LocalDaa` at which the lag was measured. So over any `Δ` ticks of a branch's own
`LocalDaa` the bucket admits at most `rate·Δ + (D_SAFE + 1)·B` claims: `B` of initial tokens, at most
`D_SAFE` catch-up refills of at most `B` each, and `rate` per tick after that, since
`SafeDaa ≤ LocalDaa`. Over a whole chain it admits at most `B + rate·LocalDaa(tip)`, because the total
`SafeDaa` advance is at most `LocalDaa(tip)`. This is 06's W1(a) and 08's supply tie (ECON-R16).

**(ii) The in-flight room — an occupancy cap.** Per class, the number of claims the panel can verify
concurrently (t12's panel room, `palw_work_target_v1.rs:149-224`), and per bond, the collateral-linear
exposure ceiling (CLAIM-R6). Occupancy caps do not accumulate: they free only when claims resolve.

**Why not per-class shares or epoch budgets.** t12's per-class epoch budget is inert on t12 for every
model class (read only when no work target is armed, `palw_admission_v2.rs:515`) and its ADR-0123
release grew with `LocalDaa` position (`palw_state_v2.rs:22497-22523`). ADR-0137 already made the
share a result, not an input. One network rate plus per-class occupancy is what t12 converged to; next
states it directly.

## 3. Normative rules

**CLAIM-R1 (typestate).** A claim's state MUST be represented as one of the six types of §2.3, each
constructible only by its transition function. *Because* INV-CLAIM-03: authority is a type.

**CLAIM-R2 (admission is atomic).** A `LotteryWin` becomes an `UnverifiedClaim` only if, at one
candidate-chain point: the ticket admits under the target in force at the block's `SafeDaa`
(04 POL-R6), the bond is active and its key signed, the class admits claims, the bucket yields a
token, the in-flight room has space, and the bond's exposure after reservation is within its ceiling.
A partial admission MUST NOT exist. An admitted claim receives the next chain-assigned
`AdmissionIndex`, in the block's acceptance order (11 §4). *Because* `private_fake_root_burst`, P0-10.

**CLAIM-R3 (no block-level work; a lost or refused attempt confers nothing).** No block carries
block-level work. All weight a claim confers flows through A2/A3, and fork choice's only weight is
verified claim weight (06 FORK-R9). An attempt header whose ticket exceeds its carried target is
invalid (11 BLK-R2). A won attempt whose admission is refused confers nothing: no claim, no slot tick,
no subsidy, no ordering weight. As the block's own attempt it makes the block ineligible for the
selected chain; as a merged block's it is skipped (11 BLK-R5, BLK-R6; 08 ECON-R16). *Because*
`failed_lottery_blue_weight`; INV-CLAIM-04. *[Synthesis edit, review: this rule gave every attempt a
lane-minimum header work `ε` that no rule consumed once blue work was dropped (OQ-18).]*

**CLAIM-R4 (live weight is bounded and proof-graded).** An `UnverifiedClaim` contributes at most
`β_u·w`, a verified claim (`VerifiedClaim` or `FinalClaim`) that is not yet safe at most `β_v·w`, with
`0 ≤ β_u < β_v ≤ 1`, and `β_u = 0` unless the owner decides §8 Q2 otherwise (06 FORK-R7, FORK-R16);
β is applied to the chain total, not per block; `live_total` MUST be constructed as `safe + bounded`
so that maturing never lowers it.
*Because* INV-POL-01, INV-CLAIM-07; t12 `palw_fork_choice.rs:56-64`, `palw_chain_weight.rs:138-143`.

**CLAIM-R5 (safe weight only from verified, buried, undisputed claims).** Only a verified claim
contributes to safe weight. It contributes once it becomes **safe**: buried by `d_bury` of later
verified weight while neither voided nor held by an unresolved dispute (06 §3.3, FORK-R4). Whether
it is `Final` does not enter. Once safe, its weight stays in safe; a later conviction MUST NOT remove
it and is answered in money (05 COURT-R10, 02). *Because* INV-POL-01, INV-FORK-01: `Final` elapses on
`SafeDaa`, which a racing branch's licences move (01 §2.3), so a key that counted `Final` would reward
a fast clock. A weight rollback carried by one branch only would penalise the branch that convicted
(§8 Q5).

**CLAIM-R6 (collateral-linear exposure).** For every bond, `Σ commitment(live claims) + other
commitments ≤ ρ·collateral` with `ρ ≤ 1`, checked at admission and re-checked at the fold on the
exact state the claim joins. Every per-bond limit on claims MUST be homogeneous of degree one in
collateral; a per-bond *count* limit MUST NOT exist unless it is dominated by a collateral-linear one.
*Because* INV-BOND-06, INV-BOND-03 (which absorb the former INV-CLAIM-06), INV-BOND-01,
`bond_split_amplification`; t12 finding 17 (`palw_state_v2.rs:28490-28535`).

**CLAIM-R7 (staged reservation).** `commitment(claim)` MUST be `w + esc + rr` while unverified,
MAY drop to `w + rr` after a replay licence that satisfies 02's release conditions, and MUST be 0 at
Final or at an uncharged void. *Because* INV-ECON-01; t12 SR-1 (`0152:849-872`).

**CLAIM-R8 (verification before finality).** `mature` MUST take a `VerifiedClaim`; an
`Optimistic` stage MUST NOT reach Final and MUST redraw once and then void `NotReplayBacked`.
*Because* INV-CLAIM-08; t12 Q-5 (`palw_state_v2.rs:23744-23763`).

**CLAIM-R9 (the clock each claim deadline reads is 01 §2.7's).** Every deadline whose expiry grants
authority on the branch (`Final`, the redraw, the close of `conviction_ends`) or charges collateral
(forfeiting voids) MUST be a `Deadline<Safe>`. Every deadline whose expiry releases value (vesting,
lock release, right maturity) MUST be a `Deadline<ChainFinal>`. Both are marked at the creating
block's `LocalDaa` (01 §2.6). Deadlines whose expiry only *removes* a pending claim without charge
(the seed-ring wait `NoRing`) are `Deadline<Local>` per 01 §2.7 row 1. Other uncharged removals MAY
additionally fire on a bounded `LocalDaa` escape (§8 Q1). *Because*
`private_daa_finality_acceleration`, `heartbeat_clock_acceleration`; INV-CLAIM-09, INV-TIME-08.

**CLAIM-R10 (bucket on the safe clock).** `ClaimBucket::refill` MUST take `SafeDaa`; tokens MUST
saturate at `B`; refill MUST be path-independent (`refill(a).refill(b) == refill(b)` for `a ≤ b`).
Over any `Δ` ticks of a branch's own `LocalDaa` it MUST admit at most `rate·Δ + (D_SAFE + 1)·B`
claims (§2.6). *Because* INV-CLAIM-01, INV-CLAIM-02; 06 W1(a).

**CLAIM-R11 (escrow).** The carrying block's escrow carve MUST be withheld from its coinbase and
recorded in the claim; it MUST be named for payment only by `FinalClaim` through a vesting row that
is released on `ChainFinalizedDaa` (02 BOND-R12, 01 §2.7); a voided or convicted claim's escrow MUST
never be minted. *Because* INV-CLAIM-05.

**CLAIM-R12 (seeds from Final only).** No randomness consumed by consensus MAY be derived from a
`LotteryWin`, an `UnverifiedClaim`, a `VerifiedClaim`, or any block hash; see 04 POL-R9. *Because*
`panel_draw_seed_grind`, INV-POL-03.

**CLAIM-R13 (bounded life).** The lattice windows MUST be chosen so that every claim is
**resolved** within `Deadline::<Safe>::after(accepted_at, D_max + W_conviction)`, or earlier by an
uncharged `NoRing` void on `LocalDaa`. Resolved means terminal (`VoidedClaim`, `ConvictedClaim`) or
`FinalClaim` with `conviction_ends` elapsed. `D_max` bounds acceptance to `Final` or terminal.
`W_conviction` is the post-`Final` conviction window. Both are `DaaSpan`, and `validate_params`
checks the lattice's windows against them in that one unit. No comparison with a pruning depth is
needed, because pruning never passes an unresolved claim (07 FINAL-R10).
*Because* INV-CLAIM-10: an unresolved claim holds the claim frontier (A5), a reservation and an escrow,
so `D_max + W_conviction` bounds how long one claim can hold back finality and pruning
(`open_claim_frontier_pin`). *[Synthesis edit, review: the rule compared `D_max` with a pruning
depth measured in another unit.]*

**CLAIM-R14 (downstream rights from Final only).** Free-prompt quanta, execution-lane credits and
class probe/share credits MUST be derived only from `FinalClaim`; a won quantum draw MUST be spent
within its use window or lapse. *Because* INV-CLAIM-02 (no stockpiling); t12 F14
(`palw_freeprompt_v3.rs:981-987`).

**CLAIM-R15 (one lattice, both lanes).** Free-prompt commitments MUST pass through the same types
and table; their `Final` licenses quanta and confers no weight by itself. *Because* INV-CLAIM-03;
t12 `palw_state_v2.rs:3893-3904`.

## 4. Pure functions

All functions below are pure: inputs → output, no storage, network, RPC or wall clock.

### 4.1 Transitions

```rust
pub fn admit(win: LotteryWin, view: &AdmissionView, bucket: ClaimBucket, ctx: &ClockContext)
    -> Result<(UnverifiedClaim, ClaimBucket), AdmitError>;          // ClockContext: 01 §2.6 (one block's clocks)
pub fn record(c: &UnverifiedClaim) -> ClaimRecord;                   // the plain data the pol crates read
pub fn bind(c: UnverifiedClaim, panel: Panel, ctx: &ClockContext) -> Result<UnverifiedClaim, BindError>;   // panel from 05 derive_panel
pub fn verify(c: UnverifiedClaim, proof: LicenceProof, ctx: &ClockContext) -> Result<VerifiedClaim, VerifyError>; // proof from 05 check_licence
pub fn mature(c: VerifiedClaim, now: SafeDaa, open: &OpenSessions, class: &ClassTerms) -> Result<FinalClaim, VerifiedClaim>;
pub fn expire(c: UnverifiedClaim, now: &ClockContext) -> Expiry;     // Pending | Redrawn | Voided
pub fn convict(c: Convictable, verdict: &ProvenVerdict, now: SafeDaa) -> Result<ConvictedClaim, NotConvictable>;
pub enum Convictable { Unverified(UnverifiedClaim), Verified(VerifiedClaim), Final(FinalClaim) }
```

Semantics:

* `admit` checks CLAIM-R2 in a fixed order: stateless shape and signature, pins (04 POL-R3), bond,
  class, lottery (04 `draw`), bucket, room, exposure. It returns the claim with
  `stage = AwaitingRing { ring_by: Deadline::<Local>::after(ctx.local, W_ring), redraws_left: 1 }`.
  Any failure returns the bucket unchanged.
* `bind` is called by the chain itself, in the first chain block in which the claim's seed ring is
  fixed (04 POL-R9, 05 PANEL-R11). It sets `bound_at = ctx.local` and
  `receipts_by = Deadline::<Safe>::after(ctx.local, W_receipt)`. If `derive_panel` refuses there,
  the claim voids `Uncharged(NoCapablePanel)`.
* `verify` accepts only a `LicenceProof` minted by `pol/verification::check_licence` for this claim's
  bound panel (a private constructor; INV-PANEL-01). It sets `licensed_at = ctx.local` and
  `challenge_ends = Deadline::<Safe>::after(ctx.local, W_challenge(class))`.
* `mature` succeeds iff `c.challenge_ends.elapsed(now)` and `open` holds no court or DA session on
  `c.core.id`. It sets `final_at` to the block's `LocalDaa` and
  `conviction_ends = Deadline::<Safe>::after(final_at, W_conviction)`.
* `expire`: a first failed panel redraws (uncharged; `redraws_left -= 1`) when `receipts_by` elapses
  on `now.safe`; a second voids with `Forfeit(SecondPanelFailed)`. An `AwaitingRing` claim whose
  `ring_by` elapses on `now.local` voids `Uncharged(NoRing)`.
* `convict` on a `FinalClaim` requires `!c.conviction_ends.elapsed(now)`.

### 4.2 Authorities (each takes exactly the type that carries it)

```rust
pub enum LiveRef<'a> { Unverified(&'a UnverifiedClaim), Verified(&'a VerifiedClaim) }
pub fn live_term(c: LiveRef<'_>, p: &WeightParams) -> ImmatureWork;       // β_u·w or β_v·w, un-floored
pub fn fork_entry(c: VerifiedRef<'_>) -> VerifiedEntry;                   // 06 §2.3; only Verified and Final have one
pub fn chain_weights(live: &[LiveRef<'_>], entries: &[VerifiedEntry], p: &WeightParams) -> ChainWeights; // safe per 06 §3.3
pub fn commitment(c: ClaimRef<'_>, now: SafeDaa, p: &CollateralParams) -> Collateral;   // CLAIM-R7
pub fn vesting_row(c: &FinalClaim) -> VestingRow;
pub fn seed_leaf(c: &FinalClaim) -> SeedLeaf;   // 04 §4.6; consensus/claims holds the only SeedLeaf mint capability (00 §7)
pub fn spend_right(c: &FinalClaim, q: QuantumIndex, draw: &ReceiptDraw, now: SafeDaa) -> Result<SpendRight, SpendError>;
```

`chain_weights` applies β to the *total* immature work and floors once (t12 did the same to stop
per-block rounding games, `palw_chain_weight.rs:138-143`). Properties: permutation-invariant over its
inputs; monotone under any single transition along the lattice; `live_total ≥ safe`.

### 4.3 Capacity

```rust
impl ClaimBucket {
    pub fn refill(self, now: SafeDaa) -> ClaimBucket;               // saturating at B; idempotent for now ≤ last
    pub fn try_take(self) -> Result<ClaimBucket, BucketEmpty>;      // 1000 milli-tokens
}
pub fn room(class: &ClassRow, inflight: &InflightIndex) -> u64;      // occupancy, no clock
pub fn exposure_headroom(bond: &BondView, now: SafeDaa, p: &CollateralParams) -> Collateral;
```

Properties (each becomes a property test): `tokens ≤ B` always; `refill` is monotone in `now` and
path-independent; `refill(LocalDaa)` does not compile; over any `Δ` ticks of `LocalDaa` at most
`rate·Δ + (D_SAFE + 1)·B` admissions, and over a whole chain at most `B + rate·LocalDaa(tip)` (§2.6);
for any two bond sets with equal total collateral, the maximum concurrent exposure is equal (split
neutrality).

## 5. Invariants upheld

The full list and cross-chapter ownership live in `09-invariants.md`.

| ID | statement | test | t12 |
|---|---|---|---|
| INV-CLAIM-01 | Private DAA advancement MUST NOT increase lottery eligibility (the ticket target or the number of admissible claims) beyond the bound 01 S5 gives: the slots the compared branch left unclaimed, plus one. | `inv_claim_01_private_daa_does_not_create_tickets` | **violated (bounded)**: the work target steps at `LocalDaa` epoch boundaries and eases ÷4 per empty epoch to its floor `W₀` (`palw_state_v2.rs:22736-22775`, `palw_work_target_v1.rs:98-110`) |
| INV-CLAIM-02 | Unused issuance capacity MUST NOT accumulate beyond `B`. | `inv_claim_02_unused_capacity_saturates_at_b` | **partial**: no cross-epoch banking (`palw_admission_v2.rs:524-575`); FP wins lapse (`palw_freeprompt_v3.rs:981-987`); but the controller banks silence as an easier target down to `W₀` and nothing caps the resulting burst except occupancy caps |
| INV-CLAIM-03 | A claim exercises exactly the authorities its type carries in §2.5; a `VoidedClaim` or `ConvictedClaim` acquires no new authority. | `inv_claim_03_authority_is_a_function_of_type` | **partial**: see §2.5 rows A1, A7, A8 |
| INV-CLAIM-04 | No block acquires any fork-choice or ordering weight from its header; a lost-lottery attempt header is invalid; an attempt that was not admitted in full confers no claim, tick or subsidy. | `inv_claim_04_unadmitted_attempt_has_no_weight` | **violated**: `protocol.rs:666-670`, `consensus/pow/src/lib.rs:594-595`, `processor.rs:11630-11652` |
| INV-CLAIM-05 | Escrow is minted only via a matured `FinalClaim` vesting row; a voided or convicted claim's escrow is never minted. | `inv_claim_05_escrow_mints_only_from_final` | **holds** (`coinbase.rs:256-268`; void path `palw_state_v2.rs:19351-19400`) |
| ~~INV-CLAIM-06~~ | *Retired in synthesis: merged into INV-BOND-06 (the ceiling) and INV-BOND-03 (every per-account limit superadditive). See 09 §4.* | — | ceiling holds (`palw_admission_v2.rs:676-700`, fold re-check `palw_state_v2.rs:28490-28535`); the per-bond class share `⌈c/2⌉` is a count (0152 T-2(a)) |
| INV-CLAIM-07 | An `UnverifiedClaim` adds strictly less live weight than a `VerifiedClaim` of equal work (0 by default); `live_total − safe ≤ β_v·(non-voided verified weight not counted in safe: unburied, or held by a dispute) + β_u·(unverified weight)`; burial never lowers `live_total`. | `inv_claim_07_unverified_live_weight_is_bounded` | **partial**: bounded and monotone (`palw_fork_choice.rs:56-64`; `β=100‰`), but t12 prices `Provisional` and a counted licence identically (§2.5 A2) |
| INV-CLAIM-08 | Only a replay-backed licence (`k ≥ 2`) yields `VerifiedClaim`; `FinalClaim` only from `VerifiedClaim`. | `inv_claim_08_optimistic_licence_never_finalizes` | **holds** past `palw_rcore_plus`: the recount (`palw_state_v2.rs:2920-2924`) and the Final gate, which redraws then voids an unreplayed licence and finalizes only the rest (`:23744-23771`) |
| INV-CLAIM-09 | Every transition that grants authority or charges collateral is timed on `SafeDaa`; every value release on `ChainFinalizedDaa`. | `inv_claim_09_authority_deadlines_read_the_safe_clock` | **partial**: locks use two clocks (`palw_panel_var_v1.rs:206-224`); Final, timeouts and target steps read `LocalDaa` |
| INV-CLAIM-10 | Every claim is resolved (terminal, or `Final` with `conviction_ends` elapsed) by `Deadline::<Safe>::after(accepted_at, D_max + W_conviction)`, or voided uncharged earlier; pruning never passes an unresolved claim. | `inv_claim_10_claim_life_is_bounded` | **partial**: a lattice-vs-horizon test exists (`palw_v2_lattice_fits_pruning_horizon`, named at `palw_state_v2.rs:23695`; body not re-read **[unverified]**); class-derived `D(c)` is pending (`feat/t12-class-verify-deadline`) |
| INV-BOND-01 | Splitting one bond into N MUST NOT increase aggregate steady-state issuance rate (nor, per 02, aggregate concurrent claim capacity). (owned by `02-bonds.md`) | `inv_bond_01_split_does_not_raise_issuance_or_capacity` | **partial**: see `bond_split_amplification` |

## 6. t12 reference

### 6.1 The lattice

t12's claim record is `PalwClaimStateV2` (`palw_state_v2.rs:3932`) with phase `PalwClaimPhaseV2`
(`:3822-3882`): `Provisional → PanelBound → ReceiptLicensed → Final`, `Voided` from any live phase,
and `DefaultDisputed` below `palw_rcore_plus` only. Void reasons are enumerated at `:3783-3820`.
The source is `Attempt` or `FreePrompt { quanta, spent }` (`:3905`); a free-prompt claim adds no weight
at Final, only spend rights (`:3893-3904`, spend at `:26845-26901`).

Transitions: creation in `apply_attempt` (`:28324`), with the one-inference/one-claim guards
(`:28338-28360`), the producer floor, the class gate and per-bond share, the exposure re-check
(`:28490-28535`), the bind deadline at `ctx.daa_score + window_bind` (`:28540`) and the epoch counter
(`:28543-28558`). Licence: `license_claim` (`:18926-18952`) ticks the second clock when the licence is
counted. Final: the deadline sweep (`:23662`), which voids unbound claims (`:23672-23675`), redraws or
voids an S2 licence awaiting replay (`:23755-23763`) and finalizes the rest at their deadline
(`:23764-23771`); `finalize_claim` adds safe weight (`:19193-19197`) and, past `palw_rcore_plus`,
writes a vesting row instead of a payout (`:19212`, `:19323`). Void: `void_claim` (`:19351`);
charging: `void_and_slash_at` (`:18828`; S0′/S1/S2 tiers in its doc at `:18810-18826`).

### 6.2 Weights

`β` is a network constant (`palw_fp_devnet_v3.rs:34` = 100‰); the immature term is priced once at
creation (`palw_state_v2.rs:28436-28444`) and only if the class bears weight (`:2066`). The comparator
is `(safe_frontier, safe_weight, live_total, hash)` (`palw_fork_choice.rs:72-78`), wired into the
deep-reorg gate (`processor.rs:13213-13250`) and the other V2 selection sites (`processor.rs:3680-3703`).
The frontier is the deepest `Final` claim below the oldest open claim (`palw_state_v2.rs:20919-20944`),
so any open claim — honest or fabricated — holds it.

### 6.3 Capacity and clocks

* **Epoch budget.** Per-class `budget_blocks` per 1,000-DAA epoch (`palw_fp_devnet_v3.rs:293`),
  counted fresh each epoch (`palw_admission_v2.rs:524-575`), the floor exempt. **Not read on t12 for
  model classes** because the work target is armed (`:515`; t12 arms every fence at DAA 0,
  `config/params.rs:15751`, walk at `:15929-15943`, RC heights at `:15522`, `:15538`).
* **ADR-0123 release** (`palw_state_v2.rs:22497-22523`): `R = ceil(pos·(L−B)/L) − others`, with `pos =
  daa_score mod L` — a `LocalDaa` quantity that heartbeat blocks advance "without spending an attempt
  slot, so they widen the release" (doc at `:22490-22492`). Armed on t12 (`params.rs:15864`) but
  unreachable there: the branch that reads it is skipped for the floor always and for model classes
  whenever the work target is armed (`palw_admission_v2.rs:515`).
* **Work target** (ADR-0137/0132 S): stepped once per `LocalDaa` epoch boundary on the closed epoch's
  model-block count against `epoch_length × 900‰` (`palw_state_v2.rs:22736-22775`), clamped ×4/÷4,
  floored at `W₀ = escrow·10⁹/rate` (`palw_work_target_v1.rs:87-110`). An epoch with zero model blocks
  eases `W` by the full clamp. The counts include claims later voided.
* **Occupancy caps.** Panel room by rate (`palw_work_target_v1.rs:149-224`); C7 held to Final and a
  static cap; per-bond share of unlicensed claims (0152 T-2); exposure ceiling at 500‰
  (`palw_fp_devnet_v3.rs:420`), checked at admission (`palw_admission_v2.rs:587-705`) and re-checked in
  the fold on the exact state (`palw_state_v2.rs:28490-28535`); unminted-reward ceiling (0152 T-3,
  `0152:1846`).
* **Execution-lane permits** (ADR-0125/0139). Rounds are whole seconds from the genesis timestamp,
  read off the round block's header timestamp (`palw_execution_lane_v1.rs:160-164`); a span's schedule
  is built from the attempt claims that reached `Final` two spans earlier and seeded by an attempt's
  nonce-free execution key plus the safe frontier (`:9-24`, `:500-507`). The lane mints nothing, its
  blocks are never blue or selected parents and never count in the DAA score (`:3-8`); each distinct
  permitted round merged adds 3 M gas to the merging block's cap under a 390 M ceiling
  (`docs/adr/0139-the-execution-lanes-gas-is-one-budget-a-round.md:21-27`). Permits are keyed
  `(span, round, index)` and used once (`:1206-1245`); whether an unused permit of an old span can
  still be used is **[unverified]**. The lane consumes only `FinalClaim` authority (A10).
* **Free-prompt quanta** (ADR-0148). One pooled receipt target, walked at each epoch boundary on the
  receipt lane's output (`docs/adr/0148-the-free-prompt-lane-prices-compute.md:52-57`) — a `LocalDaa`
  epoch; a quantum's draw exists only after its claim is `Final` (`palw_freeprompt_v3.rs:974-979`) and
  a win lapses outside `[beacon_daa, beacon_daa + use_window]` (`:981-987`), so wins do not accumulate.
* **Two clocks.** Past `palw_audit_2026_09_23`, a slashable lock stays live until both the DAA window
  and `depth` further counted licences have passed (`palw_panel_var_v1.rs:206-224`); the licence
  counter ticks only in a block carrying a quorum's signatures (`palw_state_v2.rs:18928-18940`). This
  is t12's closest thing to `SafeDaa`, and next generalises it. Its liveness escape waives it after
  `2 × window_court` without a licence (`palw_panel_var_v1.rs:227-245`).

### 6.4 Escrow

The carve is `worker_carve_permille` of the carrying block's subsidy, withheld from the selected
parent's and each merged blue's worker output (`coinbase.rs:256-273`). On t12 the overlay carve is
720‰ against a worker base lowered to 72% (`config/params.rs:15492-15493` over the 62/8/30/0 split at
`:9012-9015`), so an attempt block's producer receives no subsidy before Final **[fee share not
traced]**.

### 6.5 Divergences (next ≠ t12) and pending deltas

| # | t12 | next | reason |
|---|---|---|---|
| D1 | header blue work 2²⁰ for any attempt header | no block-level work; a lost-lottery header is invalid | `failed_lottery_blue_weight` |
| D2 | `Final` by `LocalDaa` sweep; safe weight at `Final` | `Final` on `SafeDaa`; safe weight at burial, clock-free (06 §3.3) | `private_daa_finality_acceleration` |
| D3 | work target stepped on `LocalDaa` epochs, counting unverified claims | `SafeDaa` epochs, counting verified claims | INV-CLAIM-01, `w_controller_counts_nonfinal_blocks` |
| D4 | seeds from block hashes of unverified attempts | seeds from Final commitments with lag | `panel_draw_seed_grind` |
| D5 | Final → Voided rewrite, safe weight subtracted | `ConvictedClaim`; weight frozen, money debited | typestate; a one-branch rollback penalises the convicting branch |
| D6 | per-class epoch budget + release, dormant | one network bucket per lane on `SafeDaa` | INV-CLAIM-02 |
| D7 | per-bond count cap on unlicensed claims | collateral-linear only | INV-BOND-01 |

*Pending* (not in the reference): `feat/t12-class-verify-deadline` adds class-derived verification
deadlines `D(c)` and one licensed Final floor `max(L + wc, H)`; `rcore/p2-file` makes nodes file
convictions on their own evidence, which is what turns an honest panel's refusal of a fake root into a
conviction rather than a timeout (see 04 INV-POL-07).

## 7. Attacks this chapter defends against

Detailed in `10-attack-model.md`. Verdicts are for t12 at `a0af3c92`, from code reading.

* `private_fake_root_burst` — **partial.** Primary verdict in 04 §7. Claims-side: a fabricated win
  reaches `Provisional` and holds β-bounded live weight, the frontier, a seed position and a controller
  count until its panel fails it; it never reaches safe weight or a mint without a colluding quorum;
  concurrency is bounded by the per-bond exposure ceiling. Defence: CLAIM-R2/R3/R4/R12, INV-POL-01.
* `failed_lottery_blue_weight` — **real** (co-owned with `06-fork-choice.md`, FORK-R9). A header that never faced the class
  lottery, signed by any key, passes the header stage (`pre_ghostdag_validation.rs:250-330`,
  `consensus/pow/src/lib.rs:594-595`) and is counted at 2²⁰ when merged blue
  (`protocol.rs:340-353`, `:666-670`); the admission failure only skips its claim
  (`processor.rs:11630-11652`) or disqualifies it from the chain (`palw_attempt_v2.rs:1129-1131`).
  Defence: CLAIM-R3, INV-CLAIM-04.
* `private_daa_finality_acceleration` — **partial.** `Final` is `licensed_daa + window_challenge_at`
  on the branch's own DAA (`palw_state_v2.rs:23764-23771`, `:1393-1398`); locks are protected by the
  second clock, Final is not. How fast a private branch's DAA can run is 01's question. Defence:
  CLAIM-R9.
* `heartbeat_clock_acceleration` — **partial.** Claims-side readers of the heartbeat-driven DAA:
  bind/receipt timeouts (charged past the audit fence, `palw_state_v2.rs:23735-23741`), Final, the
  work-target epoch, the ADR-0123 release position, the panel anchor slot. Defence: CLAIM-R9/R10.
* `bond_split_amplification` — **partial.** The lottery and the exposure ceiling are split-neutral
  (a ticket per inference; ceiling linear in collateral); the per-bond share of a class's unlicensed
  claims is a count, so N floor-sized bonds hold N times the in-flight share of one, up to the class
  room; the producer floor (`palw_admission_v2.rs:587-598`) prices each split. Defence: CLAIM-R6.
* `w_controller_counts_nonfinal_blocks` — **partial.** Fabricated wins raise the closed
  epoch's model count and hence `W` for honest producers; bounded by ×4 per epoch and by the cost of
  forfeits. Defence: authority A8.
* `open_claim_frontier_pin` (new) — **partial.** Any open claim holds the safe frontier below it
  (`palw_state_v2.rs:20919-20944`) until it resolves, i.e. up to the full lattice life. In next it
  pins only the claim frontier (07 §2.3), not a fork key, and at most for `D_max + W_conviction`.
  Defence: CLAIM-R13.

## 8. Open questions for the project owner

**Q1. How do uncharged voids stay live when `SafeDaa` stalls?**
If `FinalClaim` needed `SafeDaa` and `SafeDaa` advanced only on `FinalClaim`, the chain would
deadlock; `01-time-and-daa.md` §2.3 avoids this by minting `SafeDaa` from licences (the
`D_SAFE`-th most recent licence, never below `ChainFinalizedDaa`, DAA-R11) — option (a) below, which
this chapter relies on. What remains open is the licence halt: with no licences, `SafeDaa` stops, and
so does every `SafeDaa` deadline, including the receipt windows that would release capacity. The
seed-ring wait (`NoRing`) is already an uncharged `Deadline<Local>` (CLAIM-R9) and does fire.
Options: (a) a bounded `LocalDaa` escape that may only void *uncharged* and release reservations;
(b) no escape — capacity stays held until licences resume; (c) the escape also refunds bucket tokens.
**Recommendation: (a)**, with the escape `≥ 2 × W_court` (t12's second-clock escape,
`palw_panel_var_v1.rs:227-245`) and the rule that capacity it releases is not re-issuable (no bucket
refund) before `SafeDaa` advances, so a private branch cannot turn its own silence into eligibility.

**Q2. Should verified-but-not-final claims weigh more than unverified ones?**
Options: (a) one β for both (t12); (b) `β_u < β_v`; (c) `β_u = 0`. **Recommendation: (b)** with
`β_u = 50‰`, `β_v = 100‰` as starting values: a fabricated root then buys half of what a replayed
claim buys, and (c) would leave fresh honest tips ordered by the hash tiebreak alone.
*Synthesis note:* the book's normative default is (c), because 06 FORK-R7 keeps unverified claims out
of `ChainView` entirely and a fabricated root costs only hashes (04 §2.4); the consolidated question
is OQ-2 in `00-overview.md` §10.

**Q3. What does the target controller count?**
Options: (a) admitted claims (t12); (b) replay-licensed claims, lagging by the verification time;
(c) Final claims. **Recommendation: (b)**: it removes fabricated wins from the controller at a lag of
~one licence latency, which the ×4 clamp already tolerates.

**Q4. What are `B` and the refill rate of the lane buckets, and is the liveness floor bucketed?**
Options for `B`: one SafeDaa epoch of expected claims; the verification window's panel capacity; a
small constant. **Recommendation:** `rate` = the lane's target cadence per SafeDaa unit, `B` = one
verification window's worth of that rate; the floor lane gets its own bucket sized so that the floor
alone keeps `SafeDaa` advancing (never exempt: an exempt lane is an unbounded one).

**Q5. Is conviction after the claim became safe allowed to remove safe weight?** (Asked about
`Final` in the draft; weight now freezes at safety, 06 §3.3.)
Options: (a) yes, until the conviction window closes (t12, `palw_state_v2.rs:16218-16228`); (b) no —
weight is history, only money is recovered (05 COURT-R10); (c) remove it only when every candidate
under comparison carries the conviction (a common-context rule in 06). **Recommendation: (b)**. Under
(a) the honest branch that carries a conviction weighs less than an attacker's branch that omits it,
which rewards withholding convictions; (c) is correct but adds a cross-branch rule to fork choice for a
case (a colluding quorum's Final) that 02/08 already price in money.
