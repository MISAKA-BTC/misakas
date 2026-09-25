# 04 — Proof of LLM: from an execution to a consensus effect

> Normative. RFC 2119 keywords. Rule IDs `POL-R*`, invariant IDs `INV-POL-*`. Citations `path:line`
> refer to the t12 reference tree at `a0af3c92`; `0152:line` to ADR-0152 on
> `docs/adr-0152-v31-postedits` @ `9ed1adce`; `docs/adr/NNNN:line` to the ADR file in the reference
> tree. **[unverified]** marks a statement not checked against code. Claim types and the authority
> table are in `03-claims.md`; clocks in `01-time-and-daa.md`.

## 1. Purpose

Proof-of-LLM pays for, and orders the chain by, language-model inference. Consensus nodes do not run
the model — t12 made that a hard rule (the consensus crates have no model-runtime edge,
`docs/palw-rc-threat-model.md:450-462`) — so no node can check at acceptance that an inference
happened. What a node *can* check is a hash comparison and a signature. PoL therefore works in two
halves that must never be confused:

* a **lottery** that decides *who may claim* and at what rate, priced so that an honest producer pays
  one inference per draw; and
* a **verification** by a panel of bonded seats that replays the work, after which — and only after
  which — the claim acquires the authority that verified computation deserves.

Everything a lottery win is allowed to do before verification must be affordable to lose to a
fabricator, because a fabricator can win the lottery without running the model (§2.4). This chapter
specifies the pipeline, the exact lottery input, what is committed, what is priced, what may seed
randomness, and how verification turns a claim into a fact.

## 2. Concepts and types

### 2.1 The pipeline and the five facts

```text
 (a) Execution ──commit──▶ (b) ExecutionCommitment ──draw──▶ (c) LotteryWin ──admit──▶ UnverifiedClaim
      off-chain               roots + job identity            ticket ≤ target            (03)
                                                                                           │ bind panel,
                                                                                           │ replay (k ≥ 2)
                                                                                           ▼
                             (e) consensus effect ◀──mature──── FinalClaim ◀──────── (d) VerifiedClaim
                              safe weight, vesting,              SafeDaa ≥ challenge end
                              seeds, rights
```

| fact | type | who can produce it cheaply | what it proves |
|---|---|---|---|
| (a) | `Execution { job, trace }` — never in consensus | the executor only (costs one forward per draw) | — |
| (b) | `ExecutionCommitment { anchor, roots, job_identity }` | **anyone** — roots are just 64-byte values | that bytes were committed |
| (c) | `LotteryWin { commitment, ticket, target }` | anyone, at `≈ 1/p` hashes; free when `p = 1` | that a hash fell under a target |
| (d) | `LicenceCertificate`, `basis_k ≥ 2` (05) | only a quorum of independent seats | that (b) is the output of (a) |
| (e) | authority rows of `FinalClaim` | the chain, deterministically | — |

**The one sentence of this chapter:** (c) implies nothing about (a). Every rule below exists either to
make (c) cost an honest producer exactly one execution, or to keep (c) from buying anything that only
(d) should buy.

### 2.2 The job and its anchor — what the execution answers

An attempt answers a job the chain names; the producer does not choose the question.

```rust
pub struct ExecutionAnchor(Hash64);
pub fn execution_anchor(net: NetworkDomain, template: TemplateId, class: ClassId, bond: BondKey, bucket: Bucket) -> ExecutionAnchor;
pub fn job_for_anchor(class: &ClassRow, anchor: ExecutionAnchor, draw: DrawShape) -> Job;   // prompt ids are a function of the anchor
```

* `TemplateId` is the block template's pre-PoW hash with nonce and timestamp zeroed. It binds the
  execution to one position in the DAG, so a won ticket cannot be re-mounted on another template.
* `Bucket` is the high bits of the nonce. One bucket is one execution; the low bits are a uniqueness
  field. The bucket index is capped per template.
* `DrawShape` is the class's canonical job, or its prefill-only form (one forward pass over the
  canonical prompt; ADR-0117). The job — not only its id — is what a seat checks (POL-R12).

A **free-prompt** claim answers a user's prompt instead: its commitment is placed on chain *before*
its draw, and its draw is taken from randomness that did not exist when it was committed (§2.6).

### 2.3 The commitment, and what is priced, pinned and replayed

The ticket is a hash of the execution commitment. Every byte inside that hash is **priced** (it
changes the ticket). A priced field that the producer can vary freely is a free lottery draw — a
nonce by another name (t12 found `trace_retention_daa` giving 4,096 tickets from one execution,
`docs/adr/0072-the-ticket-is-the-execution.md:113-128`). So every field of the committed attempt MUST
be classified, exhaustively and in code, as exactly one of:

| class | meaning | examples (t12 names) | checked where |
|---|---|---|---|
| **chain-equal** | equals a value in chain state | `class_id`, `executor_bond`, key, `operator_id`, `artifact_root`, `network_domain`, `version` | admission |
| **derived** | a function of other fields and the block | `pwu`, `trace_manifest_root`, `trace_chunk_count`, `trace_retention_daa` | admission |
| **replay-checked** | only a replay of the job can tell right from wrong | `trace_root`, `output_root`, `execution_root` | the panel (05) |
| **position** | binds the block's position; excluded from the ticket | `challenge` (nonce, timestamp) | admission |

The replay-checked row is the whole problem: **three 64-byte fields whose only pin is a panel that
convenes after the claim exists.** t12 pins this as an open gap in a test
(`palw_attempt_v2.rs:1119-1180`). next keeps the row (it is intrinsic — consensus cannot run the model)
and instead bounds what the resulting win can buy (03 §2.5).

### 2.4 The lottery

```rust
pub struct Ticket(u128);
pub fn execution_commitment(body: &AttemptBody, anchor: ExecutionAnchor) -> ExecutionCommitment; // position field zeroed
pub fn ticket(c: &ExecutionCommitment) -> Ticket;
pub fn ticket_target(class: &ClassWork, w: WorkTarget) -> Target;       // MAX · min(1, CCU_class / W)
pub fn draw(c: ExecutionCommitment, target: Target) -> Option<LotteryWin>;
```

The target prices one unit of network work `W`: a class whose one draw costs `CCU` of compute wins
with `p = min(1, CCU/W)`, so the expected compute behind a win is `W` for every class lighter than `W`
(ADR-0137). The **liveness floor** class keeps a target of its own. The derived work of a win is
`w = max(1, expected_draws(target)) × CCU_per_draw` (POL-R7) — it is what the claim will weigh if it
is verified. So `w ≈ W` for a class lighter than `W` and `w = CCU` for a heavier one. `W` is clamped to
`[W₀, W_max]` and a class's `CCU` to at most `CCU_max` at registration, both genesis-fixed. Every
claim's weight is then at most `w_max = max(W_max, CCU_max)`, the per-claim bound that 06's rate
constant `ρ` needs (06 W1).

**What a win costs.** An honest producer pays one execution per draw, because its roots are the
execution's output and a different ticket needs a different execution. A **fabricator** writes any
roots it likes: each new value of `trace_root` is a new ticket at the cost of about seven BLAKE2b calls
(`palw_attempt_v2.rs:242-247`). So a fabricated win costs `≈ 1/p` hashes — and **zero** for a class with
`CCU ≥ W`, whose target is `MAX` (`palw_work_target_v1.rs:116-124`). The lottery is rate control and a
price for the honest; it is not evidence.

### 2.5 Verification

Verification is specified in `05-panel-validation.md`; this chapter fixes only what PoL needs of it.
The licence check is 05's, and it returns a proof token, not a claim type. `pol/verification` sits
upstream of `consensus/claims` (00 §7), so it cannot construct a typestate:

```rust
pub fn check_licence(bound: &BoundRecord, receipts: &[SignedReceipt], ctx: &PanelContext)
    -> Result<LicenceProof, LicenceRefusal>;           // 05 §4; LicenceProof has a private constructor
// consensus/claims::verify(c: UnverifiedClaim, proof: LicenceProof, ..) is the only constructor
// of VerifiedClaim (03 §4.1).
```

A seat either replays the job from the anchor and compares roots, or checks served material against
the committed roots **and** the job the anchor names (POL-R12). A `VerifiedClaim` exists iff every
segment of the job is covered by `Valid` replays from at least two distinct seat indices (05's
`BasisK`, t12's `basis_k`). A single full-replay `Valid` (t12's optimistic single-replay door, ADR-0133's
"S2") is recorded but is not a licence (03 CLAIM-R8). An `Invalid` verdict names a fault that a court can adjudicate against the claim's own
recorded `job_identity` (POL-R13), so a fabricated root is convicted, not merely timed out.

### 2.6 Randomness

Consensus consumes randomness in three places: the panel draw, the free-prompt quantum draw, and
schedules (e.g. an execution lane). Each is a seed that some party would like to choose. The rule
(POL-R9) is that a seed MUST be a function of values that (i) were fixed after the seeded object and
its population were fixed and (ii) no single party can vary at a cost below one *licensed* claim per
try. In practice:

```rust
pub struct SeedLeaf(Hash64);        // minted only by consensus/claims::seed_leaf(&FinalClaim): the nonce-free commitment
/// The first K leaves of claims that reached Final strictly after the seeded claim's accepting
/// block, in chain order; fixed in the block where the K-th of them reaches Final.
pub struct SeedRing { leaves: [SeedLeaf; K], fixed_at: BlockId }
pub fn panel_seed(ring: &SeedRing, admission: AdmissionIndex, draw: DrawIndex) -> PanelSeed;
pub fn quantum_seed(ring: &SeedRing, admission: AdmissionIndex, q: QuantumIndex) -> QuantumSeed;
```

The per-claim input is the chain-assigned `AdmissionIndex`, never the claim id or any commitment byte.
The producer chooses a claim's id for the price of a fake root (§2.4), but it can move the claim's
index only by consuming admission capacity (a bucket token and a reservation).

A block hash is never a seed: it covers the nonce and timestamp, which the block's producer may vary
freely after the ticket is fixed (§6.4). An unverified commitment is never a seed: its roots are free
to a fabricator (§2.4).

An earlier draft of `05-panel-validation.md` PANEL-R7 seeded the panel from the anchor attempt's
nonce-free `execution_key`. That closes the nonce/timestamp axis but not the root axis. The anchor's
producer can fabricate roots, keep whichever winning variant yields the panels it wants, and pay one
forfeit for the one anchor it publishes: a price per *published* anchor, not per *try*. POL-R9 is the
stricter rule (Final leaves only). The gap between the two, and the bootstrap, are §8 Q2 and OQ-1.

**What POL-R9 does not close.** Which pending claims reach `Final` next is partly predictable at
admission: their challenge windows are on chain. A producer may therefore predict its ring and time
its submission to land on a favourable admission index. Each try then costs admission capacity, not a
hash. On a branch the adversary produces, every leaf is its own, so it can also shape the ring by
choosing when its licences land. The resulting capture probability `P_cap` is what bounds a private
branch (10 §2.1 C11). The simulator must measure it under this rule (OQ-26).

### 2.7 What is priced — the unit

Weight and pay are denominated in one network unit of compute (MAC-equivalents derived from the
class's registered graph — ADR-0145/0148), never in a registrant's declaration and never in wall-clock
cost. Per win: `w` (above). Per free-prompt claim: its derived compute `C`, split into quanta whose
*odds* scale with compute (ADR-0148 D3), each quantum's weight arriving only when it is spent after the
claim is Final (03 CLAIM-R14). Heavier classes are not paid more per unit; they win less often.

## 3. Normative rules

**POL-R1 (the ticket is the execution).** `ticket` MUST be a pure function of
`execution_commitment(body, anchor)`, and `execution_commitment` MUST hash the whole attempt body with
the position field zeroed, so a field added to the body is priced the moment it exists. *Because*
nonce sweeps (ADR-0071/0072); INV-POL-02.

**POL-R2 (the anchor is derived, never carried).** Every verifier MUST derive the execution anchor
from the header it holds (network, template, class, bond, bucket); the attempt MUST NOT carry it.
*Because* the accused must not set the question; INV-POL-02.

**POL-R3 (every priced field is pinned or is the position).** Each field of the attempt body MUST be
classified in code as chain-equal, derived, replay-checked or position, and a test MUST fail to
compile or pass when a field is added unclassified. Derived fields MUST be checked by equality, at
admission and on the relay path. *Because* `unpinned_priced_field_free_draw`; INV-POL-02.

**POL-R4 (bounded buckets).** One template MUST admit at most `B_max` buckets, `B_max` derived from the
fastest class's execution time and the template's useful life. *Because* the bucket index is free to a
fabricator (`palw_attempt_v2.rs:237-261`).

**POL-R5 (a win is only a win).** A `LotteryWin` MUST NOT confer any authority listed in 03 §2.5 except
the right to be admitted; in particular no block-level work (none exists, 03 CLAIM-R3). A lost ticket
is not a block at all: an attempt header whose ticket exceeds its carried target is invalid
(11 BLK-R2). *Because* `private_fake_root_burst`, `failed_lottery_blue_weight`; INV-POL-01.

**POL-R6 (the target is read at the block's SafeDaa).** The target a ticket is compared against MUST be
the controller's output as of the block's `SafeDaa`: `W` is stepped only at `SafeDaa` epoch
boundaries, and a block uses the `W` of the last boundary `≤` its `SafeDaa`. *Because* a private branch
that advances its own clock through empty epochs eases its own target (§6.3); INV-POL-08, INV-CLAIM-01.

**POL-R7 (work is derived and bounded).** A claim's `DerivedWork` MUST equal `max(1,
expected_draws(target)) × CCU_per_draw(class, draw shape)` computed by the chain; a carried value that
differs MUST be refused, never corrected. The work target MUST stay in `[W₀, W_max]` and a class MUST
NOT be registered with `CCU_per_draw > CCU_max`. `W_max` and `CCU_max` are genesis-fixed, so every
`DerivedWork ≤ w_max = max(W_max, CCU_max)`. *Because* `declared_canonical_job_weight_inflation`
(ADR-0149); INV-POL-04. Without a ceiling, 06's rate constant `ρ` is not a constant and
`d_bury ≥ ρ·L` cannot be checked (06 FORK-R16).

**POL-R8 (one execution, one claim).** An execution commitment (its work identity) MUST back at most
one non-retired claim on a chain, across blocks. *Because* `one_execution_many_claims`; INV-POL-05.

**POL-R9 (seeds).** Every seed consumed by consensus for a claim `c` MUST satisfy all of:

1. **Ring.** It is derived from a `SeedRing` of `K ≥ 2` `SeedLeaf`s, each minted from a claim that
   reached `Final` strictly after `c`'s accepting block. The ring is the first `K` such leaves in
   chain order, fixed in the block in which the `K`-th reaches `Final`.
2. **Per-claim input.** Its only per-claim input is `c`'s chain-assigned `AdmissionIndex`, plus a
   `DrawIndex` or `QuantumIndex`. Never `c`'s id or any byte of its commitment.
3. **Population.** The population it draws from is fixed strictly before `c`'s accepting block, in
   registration order (05 PANEL-R8, 02 BOND-R3), before any ring leaf exists.
4. **Binding.** The panel binds in the first chain block in which the ring is fixed (05 PANEL-R11).
   The claim's wait for its ring is an uncharged `Deadline<Local>` (`ring_by`, span `W_ring`), not a
   bind window counted from acceptance.
5. **Forbidden inputs.** It never reads a block hash, a header nonce or timestamp, a `LotteryWin`, or
   an unverified commitment.

No "distinct bonds" clause is made: accounts are not identities (02 §2.1). A ring's leaves are bounded
by the admission share of whoever produced them.
*Because* `panel_draw_seed_grind`; INV-POL-03. A ring known at admission would let a producer grind
its claim id with fake roots, for hashes, until the panel suits it. A population fixed after the seed
could be predicted would let it grind account keys into the draw's cumulative intervals. A bind
window counted from acceptance would void every claim whenever `SafeDaa` lags by more than the window.
*[Synthesis edit, review: this rule fixed the ring "at a `SafeDaa` after the seeded object", which does
not require post-acceptance leaves, and it required distinct bonds.]* The residual predictability and
the bootstrap are OQ-26 and OQ-1.

**POL-R10 (panels judge replays).** A licence MUST require `k_min ≥ 2` distinct `Valid` replays over
every segment; `Sampled`, `Incapable` and `Unavailable` MUST NOT count. *Because* F4 of the ADR-0152
review; INV-POL-06, 03 INV-CLAIM-08.

**POL-R11 (verification deadline fits the class).** A class MUST NOT admit claims unless a reference
replay of one job fits inside its verification deadline `D(c)` on `SafeDaa`. *Because* a claim whose
fraud cannot be proven in time is unpunishable (0152 §4.1 (iii)); INV-POL-07.

**POL-R12 (a Valid covers the named job).** A seat's `Valid` MUST attest that the material answers the
whole job the anchor names (prompt, prefill, decode shape), not only the job's id. *Because*
`short_job_same_job_id` (ADR-0117 D3); INV-POL-06.

**POL-R13 (attribution).** Admission MUST record the claim's `job_identity` (the anchor its commitment
was derived under), and a court MUST be able to convict a claim whose roots are not the named job's
execution — including borrowed roots and garbage pins — from the claim record and served material
alone. *Because* F1 of the ADR-0152 review; INV-POL-07.

**POL-R14 (free-prompt order).** A free-prompt claim MUST be committed before its draw; its quanta MUST
be drawn only after it is Final, from a seed per POL-R9, and a win MUST be spent within a use window or
lapse. *Because* commit-then-draw is what makes a user's prompt ungrindable; 03 INV-CLAIM-02.

**POL-R15 (one unit).** Work and pay MUST be denominated in the chain's derived compute unit; no class
coefficient, declaration or measurement a registrant controls MAY enter `DerivedWork`. *Because*
ADR-0146/0149 (arbitrage by declaration).

**POL-R16 (no runtime in consensus).** Consensus crates MUST NOT depend on a model runtime; a node
without a model validates every block. *Because* validation DoS and determinism (t12 W1).

## 4. Pure functions

```rust
// 4.1 Position and job
pub fn template_id(header: &HeaderPrePow) -> TemplateId;                         // nonce = 0, timestamp = 0
pub fn bucket_of(nonce: u64) -> Result<Bucket, BucketAboveCeiling>;
pub fn execution_anchor(net: NetworkDomain, t: TemplateId, class: ClassId, bond: BondKey, b: Bucket) -> ExecutionAnchor;
pub fn job_for_anchor(class: &ClassRow, anchor: ExecutionAnchor, shape: DrawShape) -> Job;

// 4.2 Commitment and pins
pub fn check_pins(body: &AttemptBody, block: &BlockFacts, params: &PinParams) -> Result<(), PinError>;
pub fn execution_commitment(body: &AttemptBody, anchor: ExecutionAnchor) -> ExecutionCommitment;

// 4.3 Lottery
pub fn ticket(c: &ExecutionCommitment) -> Ticket;
pub fn ticket_target(class: &ClassWork, w: WorkTarget) -> Target;
pub fn draw(c: ExecutionCommitment, target: Target) -> Option<LotteryWin>;
pub fn derive_work(target: Target, per_draw: DrawWork) -> DerivedWork;

// 4.4 Controller (reads the safe clock only)
pub fn step_work_target(current: WorkTarget, closed: &ClosedEpoch, floor: WorkFloor, ceiling: WorkCeiling, clamp: u32) -> WorkTarget;
pub struct ClosedEpoch { index: SafeEpoch, verified_model_claims: u64, expected: u64 }
pub fn target_in_force(history: &TargetHistory, at: SafeDaa) -> WorkTarget;

// 4.5 Verification — defined in 05-panel-validation.md §4, restated for reference
pub fn check_licence(bound: &BoundRecord, receipts: &[SignedReceipt], ctx: &PanelContext)
    -> Result<LicenceProof, LicenceRefusal>;

// 4.6 Seeds (leaves are minted by consensus/claims::seed_leaf(&FinalClaim); this crate only combines them)
pub fn seed_ring(post_acceptance_leaves: &[SeedLeaf], fixed_at: BlockId, k: usize) -> Result<SeedRing, NotEnoughLeaves>;
pub fn panel_seed(ring: &SeedRing, admission: AdmissionIndex, draw: DrawIndex) -> PanelSeed;
```

Semantics and properties:

* `ticket(execution_commitment(body, anchor))` is invariant under any change of the position field
  and changes under any change of any other field (property: `every_priced_field_moves_the_ticket`).
* `draw` admits iff `ticket ≤ target` (`≤`, matching `expected_draws = 2¹²⁸/(target+1)`;
  t12 `palw_pwu.rs:82-110`).
* `derive_work` is monotone non-increasing in `target` and never 0.
* `step_work_target(W, e, floor, ceiling, f)` ∈ `[max(floor, W/f), min(ceiling, max(floor, W·f))]`,
  and never above `W_max` (POL-R7); with `e.expected = 0` it returns `max(W, floor)`; it is defined
  over `SafeEpoch`, so `step_work_target` over a `LocalDaa` epoch does not type-check.
* `target_in_force` is a function of `SafeDaa` only: two branches at equal `SafeDaa` compare
  tickets against the same target (INV-POL-08).
* `check_licence` is deterministic in its inputs, independent of receipt order, and MUST refuse a
  receipt that attests a job other than the one the claim's recorded `job_identity` names (POL-R12).
* `seed_ring` refuses a leaf of a claim that reached `Final` at or before the seeded claim's accepting
  block; `panel_seed` changes if any ring leaf or the admission index changes, and is unchanged by
  the seeded claim's id and by any header field.

## 5. Invariants upheld

| ID | statement | test | t12 |
|---|---|---|---|
| INV-POL-01 | An unverified LLM commitment MUST NOT obtain the same consensus authority as verified computation. | `inv_pol_01_unverified_commitment_lacks_verified_authority` | **violated** for block-level work (2²⁰ to every attempt header, `protocol.rs:666-670`), for seeds (§6.4) and for controller input (`palw_state_v2.rs:28543-28558`); **holds** for safe weight and mint |
| INV-POL-02 | The ticket is a function of (commitment, derived anchor) only; every priced field is chain-equal, derived, replay-checked or the position field. | `inv_pol_02_every_priced_field_is_pinned` | **holds** (`palw_attempt_v2.rs:562-589`; pins `palw_admission_v2.rs:833-850`; classification test `palw_attempt_v2.rs:1103-1107`) |
| INV-POL-03 | No seed consumed by consensus is re-rollable by one party at a cost below one admitted, licensed claim per try; no seed reads a block hash, a claim id or an unverified commitment. | `inv_pol_03_seeds_are_not_rerollable` | **violated**: panel seeds and receipt beacons read block hashes (§6.4) |
| INV-POL-04 | A claim's work equals the chain derivation; no producer or registrant input. | `inv_pol_04_pwu_is_the_derivation` | **holds** past `palw_canonical_work` (`palw_admission_v2.rs:452-473`) |
| INV-POL-05 | One execution commitment backs at most one non-retired claim on a chain. | `inv_pol_05_one_execution_one_claim` | **holds** past the 2026-09-23 audit fence (`palw_state_v2.rs:28336-28360`) |
| INV-POL-06 | A licence needs `k_min ≥ 2` full-coverage `Valid` replays, each attesting the whole named job. | `inv_pol_06_a_valid_covers_the_named_job` | **holds** (recount `palw_state_v2.rs:2920-2924`, Final gate `:23744-23771`; ADR-0117 D3) **[seat-side material check not re-read]** |
| INV-POL-07 | A claim whose roots are not the named job's execution is convictable within the class's verification deadline. | `inv_pol_07_fake_roots_are_attributable` | **partial**: `job_identity` recorded past `palw_offence_attribution` (`palw_state_v2.rs:28478-28486`); automatic filing pending (`rcore/p2-file`); an `AttnFused` arithmetic lie on held-context classes not yet provable (`0152:3100-3130`) |
| INV-POL-08 | The target in force at a block is a function of its `SafeDaa`; a branch-local clock does not ease it. | `inv_pol_08_target_reads_the_safe_context` | **violated (bounded by `W₀`)**: `palw_state_v2.rs:22736-22775` |

## 6. t12 reference

### 6.1 The exact lottery input on t12

t12 arms every fence at DAA 0 (`config/params.rs:15751`, the walk at `:15929-15943`), including the
work target and the single lottery (RC heights `:15522`, `:15538`). For an attempt header (algo 6/9):

1. `pre_pow_hash = hash_override_nonce_time_64(header, 0, 0)` — nonce, timestamp and the PALW
   carriage are excluded (`hashing/header.rs:200-210`).
2. `anchor = palw_job_anchor_v1(network_domain, pre_pow_hash, class_id, executor_bond, nonce >> 22)`
   (`palw_attempt_v2.rs:182-199`, `:530-538`); the bucket is capped at 2²⁴ (`:261-266`).
3. `commitment = H_keyed("…/execution-commitment/v3" ‖ anchor ‖ len ‖ borsh(attempt with challenge = 0))`
   (`:562-572`); `ticket = u128_le(H_keyed("…/class-ticket/v3" ‖ commitment)[..16])` (`:581-589`).
4. Target: for a model class `MAX·min(1, CCU/max(W₀, W))` (`palw_admission_v2.rs:944-1004`,
   `palw_work_target_v1.rs:116-124`); for the floor its own class target (ADR-0076 seed, per-epoch
   retarget) (`palw_admission_v2.rs:993-996`). Admit iff `ticket ≤ target` (`:811`, `:1003`).
5. The Layer-0 digest is admitted unconditionally for an attempt past the single lottery
   (`consensus/pow/src/lib.rs:594-595`); the lane's block-level work is the constant 2²⁰
   (`protocol.rs:666-670`).
6. **Where:** only in the virtual processor's composed admission (`processor.rs:11505-11552`,
   `palw_admission_v2.rs:916-936`), for the chain block's own attempt and for each merged blue
   (`processor.rs:11630`). The header stage checks only shape, the `challenge` equation, the DA pins and
   the signature under the *carried* key (`pre_ghostdag_validation.rs:250-330`).

**The ADR map.** ADR-0072 (the ticket is the execution; D8 pins) is §6.1 steps 2-3. **ADR-0074**'s
beacon no longer enters the attempt lottery; it survives only as the free-prompt quantum draw
(`palw_freeprompt_v3.rs:962-973`). **ADR-0076**'s share-seeded class target governs only the floor
past the work target. **ADR-0117** makes the draw one forward pass (prefill-only job past its fence,
`palw_attempt_v2.rs:227`) and moves the job check to seats. **ADR-0137/0132 S** replace per-class
targets and `bits` with one work target. **ADR-0141** proposes nothing and records that the lottery is
rate control, with weight a lane constant (`docs/adr/0141-can-an-inference-be-the-ticket-without-a-hash-lottery.md:25-33`).
**ADR-0149** makes `pwu` the derivation (`palw_admission_v2.rs:452-473`).

### 6.2 Can a producer compute the draw without running the model? — Yes.

The three replay-checked roots are inside the priced bytes and are checked by nobody at acceptance:
"Nothing in block acceptance checks that the execution was the job the anchor names. Seats check it"
(`docs/adr/0117-a-draw-is-one-forward.md:50-58`). t12 pins the free space in a test that is expected to
pass — "3 execution roots x 512 bits … the panel that would pin them never convenes" for an unadmitted
attempt (`palw_attempt_v2.rs:1119-1180`) — and notes that for a party that runs no inference each
candidate costs "one borsh and ~7 BLAKE2b" (`:242-247`). t12 accepted this knowingly: a free tag is
tolerable only beside the per-bond exposure cap (`:591-596`). The 2026-08-19 audit's P0-10 and the
threat model's Attack B (`docs/palw-critical-audit-2026-08-19-ja.md:357-398`, Attack B at `:427-433`;
`docs/palw-rc-threat-model.md:432-452`) describe exactly this, and it still holds for the *grind*; what
changed is the payoff (§6.3).

### 6.3 What a winning-but-unverified claim gets on t12

| authority | granted? | evidence |
|---|---|---|
| block-level work | 2²⁰, before admission and regardless of the lottery | `protocol.rs:633`, `:666-670`; `consensus/pow/src/lib.rs:594-595` |
| live weight | `⌊β·pwu/1000⌋`, β = 100‰, until voided | `palw_state_v2.rs:28436-28444`, `palw_fp_devnet_v3.rs:34` |
| safe weight / frontier advance | no; it *holds* the frontier while open | `palw_state_v2.rs:20919-20944` |
| subsidy | none: the escrow is withheld and never minted if voided | `coinbase.rs:256-268`; `config/params.rs:15492-15493` |
| licence | only from a colluding quorum; honest seats refuse a job mismatch | ADR-0117 D3; 0152 J-rules |
| seed | its block hash anchors panels and receipt beacons (§6.4) | `processor.rs:10021-10045` |
| controller | counted toward `W` at acceptance, not uncounted on void | `palw_state_v2.rs:28543-28558`, `:22736-22775` |
| concurrency | bounded: exposure ≤ 500‰ of collateral, producer floor, per-bond class share, panel room | `palw_admission_v2.rs:587-705`; `palw_state_v2.rs:28488-28532` |
| cost when it fails | first panel failure redraws uncharged; the second forfeits `w + esc + rr` (S0′); a proven fraud adds the action tier | `palw_state_v2.rs:23733-23763`, `:18810-18826` |

The P0-10 *stacking* condition (one bond, unbounded immature work) is closed; the grind is not.

### 6.4 Seeds read block hashes, and a block hash is free to re-roll

* The panel seat/operator/stake tickets are `H(domain ‖ anchor_block ‖ claim ‖ operator)`
  (`palw_panel_v2.rs:1495-1508`, `:1667-1673`), where `anchor_block` is the **block hash** of the first
  attempt chain block at or past `accepted_daa + anchor_delay` (`processor.rs:10021-10045`).
* The free-prompt quantum ticket is `H(network ‖ beacon_block ‖ claim ‖ q)` (`palw_freeprompt_v3.rs:962-973`)
  with `beacon_block` the hash of the first attempt chain block at or past the slot
  (`palw_fp_beacon_v3.rs:119-126`); the ADR-0073 SA-1 fold is dormant on t12 (`config/params.rs:15740`).
* A block hash covers `nonce` and `timestamp` (`hashing/header.rs:169-173`); the ticket does not
  (§6.1). Within one bucket the low 22 nonce bits are free (`palw_attempt_v2.rs:233-235`); the producer
  re-derives the `challenge` and re-signs with its own key. So **one won execution yields ~2²² distinct
  anchor hashes at one signature each**, and a fabricator gets more by varying roots.
* ADR-0152 prices this lever at "one inference per try" (`0152:2958-2962`) and ADR-0074 calls the beacon
  ungrindable (`docs/adr/0074-the-attempt-is-a-claim-drawn-by-the-chain.md:75-78`); the code does not
  support either statement. The execution-lane seed is better — it reads the nonce-free
  `execution_key` of an attempt plus the safe frontier (`palw_execution_lane_v1.rs:403-410`, `:500-507`)
  — but that attempt is unverified, so a fabricator can still grind it at hash cost.

This is the `panel_draw_seed_grind` finding (§7). It is a verdict from reading code; the simulator
must reproduce it before it is treated as exploitable at a given stake.

### 6.5 Verification on t12

Doors: V1 quorum, V2 coverage, the optimistic single-replay door (ADR-0133's "S2"), shard part
(`palw_economic_safety_v1.rs:220-230`). The optimistic door licenses on the full-replay seat alone
(`palw_optimistic_licence_v2.rs:23-50`) but past `palw_rcore_plus` never reaches Final without
`basis_k ≥ 2`: the recount is `palw_state_v2.rs:2920-2924` and the Final gate redraws, then voids, an
unreplayed licence (`:23744-23771`). The seat's material check and SEAT-0 (the seat-only check that "closes the free
execution-root grind on live t12", `0152:3560`) are seat policy, not acceptance **[seat code not
re-read]**.

### 6.6 Divergences and pending deltas

| # | t12 | next | reason |
|---|---|---|---|
| P1 | lottery checked only in the virtual processor; header work 2²⁰ | lost ticket invalid at the header stage (carried target); no block-level work; weight only through claims | POL-R5 |
| P2 | seeds from block hashes | `SeedRing` of post-acceptance `FinalClaim` leaves, per-claim input the admission index | POL-R9 |
| P3 | `W` stepped on `LocalDaa`, counting admitted claims | stepped on `SafeDaa`, counting verified claims | POL-R6 |
| P4 | optimistic single-replay door exists as a licence type | recorded, never a licence (`basis_k < 2`) | POL-R10 |
| P5 | attribution arrived as fences (`job_identity`, J-rules) | required from genesis | POL-R13 |

*Pending:* `rcore/p2-file` (nodes file convictions on their own evidence — what turns an honest panel's
`Invalid` on a fake root into a conviction); `feat/t12-class-verify-deadline` (`D(c)` per class, POL-R11);
`feat/t12-aheld-node` (held-attention attribution, relevant to INV-POL-07 on held-context classes).

## 7. Attacks this chapter defends against

Verdicts are for t12 at `a0af3c92`, from code reading; detail in `10-attack-model.md`.

* **`private_fake_root_burst` — partial.** *Mechanism:* grind `trace_root`/`output_root`/`execution_root`
  until `ticket ≤ target` (free for `CCU ≥ W`), mine the block, publish (or keep private). *t12:* the grind
  is real (§6.2); the win gets block-level work, β-bounded live weight, a seed position and a
  controller count, and holds the frontier until two panels fail it (§6.3); it cannot mature, mint or
  licence without a colluding quorum, and one bond can hold only as many as its exposure ceiling
  allows. *next:* POL-R5, POL-R9, 03 CLAIM-R2/R3/R4/R6, INV-POL-01.
* **`failed_lottery_blue_weight` — real** (co-owned with `06-fork-choice.md`). A header that never faced the lottery
  carries 2²⁰ when merged blue (§6.1 step 5-6; `processor.rs:11630-11652` skips only its claim).
  *next:* POL-R5, 03 INV-CLAIM-04.
* **`panel_draw_seed_grind` — real (code-read); here extended to the receipt beacon.** The producer of a panel's anchor block (or a
  receipt beacon) re-rolls the seed by nonce/timestamp at one signature per try (§6.4), choosing panels
  for every claim anchored there — including its own fabricated claims — or quantum wins. *next:*
  POL-R9, INV-POL-03.
* **`private_work_target_easing` (this chapter's earlier name: `private_silence_eases_target`; cf. `idle_class_target_relaxation`) — real, bounded.** A private branch advances its clock through
  epochs with no model claims; each boundary eases `W` ÷4 to `W₀` (`palw_work_target_v1.rs:98-110`,
  `palw_state_v2.rs:22736-22775`), so the branch wins more tickets per inference; `pwu` falls with the
  easier target but the 2²⁰ block-level work and the per-block escrow do not. *next:* POL-R6, INV-POL-08.
* **`w_controller_counts_nonfinal_blocks` — partial.** Fabricated wins raise the model count
  and harden `W` for honest producers; bounded ×4 per epoch and by forfeits. *next:* POL-R6 counts verified
  claims.
* **`unpinned_priced_field_free_draw` — closed.** DA fields pinned by equality (`palw_admission_v2.rs:833-850`),
  also on the relay path (`pre_ghostdag_validation.rs:266-274`).
* **`nonce_free_lottery_draws` — closed for tickets** (the nonce is outside the commitment, `palw_attempt_v2.rs:562-572`;
  bucket cap 2²⁴, `:261-266`); **open for block identity** (see `panel_draw_seed_grind`).
* **`short_job_same_job_id` — closed** by the seat's whole-job check (ADR-0117 D3) **[seat code not re-read]**.
* **`declared_canonical_job_weight_inflation` (this chapter's earlier name: `pwu_declaration_inflation`) — closed** past `palw_canonical_work` (`palw_admission_v2.rs:452-473`).
* **`one_execution_many_claims` — closed** past the audit fence (`palw_state_v2.rs:28336-28360`).
* **`one_bond_backs_unbounded_immature_work` (P0-10; earlier name `one_bond_unbounded_immature`) — closed** (`palw_admission_v2.rs:676-700`; fold re-check).

## 8. Open questions for the project owner

**Q1. Should the ticket target be header-verifiable, so a lost lottery is invalid at the header stage?**
Options: (a) keep the lottery in the state-aware stage and give attempt headers `ε` work (POL-R5);
(b) carry the target in the header like `bits` and verify the ticket at the header stage, checking the
carried target against the derivation later; (c) both. **Recommendation: (c)** — (b) stops lost-lottery
headers from entering the DAG at all (a relay-DoS benefit), and (a) keeps a mis-declared target from ever
buying weight.
*Synthesis note (review):* the book now takes (b) normatively (11 BLK-R2, BLK-R5) and drops `ε`. Once
blue work is gone (OQ-18) no rule consumes header work, and a mis-declared target is caught when the
carried target is compared with `target_in_force(SafeDaa)` at the state stage. OQ-13 records it.

**Q2. What seeds panels: the anchor's execution key (the draft's 05 PANEL-R7) or a ring of Final leaves
(POL-R9)?** Options: (a) the execution key: simple and immediate, but re-rollable by root fabrication
at hash cost per try and one forfeit per published anchor; (b) POL-R9 with `K = 2, 4, 8`
post-acceptance Final leaves and the admission index as the per-claim input: not re-rollable below
one licensed claim per try, but a panel waits for `K` Finals past its claim; (c) (a) for the first
panel and (b) for the redraw. **Recommendation: (b)** with `K = 4`, falling back to (c) only if the
simulator shows the bind latency of (b) breaks the lattice windows; either way the block hash goes.
*Synthesis note:* (b) has no bootstrap or halt rule. At genesis, and after any stretch in which no
claim reached `Final`, no ring of leaves fixed *after* a pending claim exists, while those claims
cannot reach `Final` without a panel: a circular wait (`seed_ring_bootstrap_deadlock`,
10-attack-model.md §6; INV-PANEL-12). t12 met the same circularity in its seat-maturity floor with an
explicit bootstrap waiver: with fewer than `depth` anchors before the draw, the floor is waived,
"because the first anchors of a chain are produced by panels drawn before any anchor could have
settled" (`palw_panel_v2.rs:660-667`). Whichever option is chosen must ship with such a rule.
Consolidated as OQ-1 in `00-overview.md` §10; the predictability residual is OQ-26.

**Q3. Should a fabricated root be convictable at the first panel instead of timing out twice?**
Options: (a) t12 launch rule — two failed panels forfeit (S0′); (b) an honest seat's `Invalid` with a
job-identity mismatch opens a court immediately (P2-8-style automatic filing). **Recommendation: (b)**,
with (a) as the backstop for silent panels; it shortens the window in which a fake claim holds live
weight and the frontier from two panel windows to one court.

**Q4. Should classes with `CCU ≥ W` exist?** Their target is `MAX`, so their lottery prices nothing and
admission is limited only by capacity. Options: (a) allow (t12); (b) require `CCU < W` at registration;
(c) split such a draw into sub-draws. **Recommendation: (a)** only with the lane bucket of 03 §2.6 in
force — the bucket, not the lottery, is then the rate limiter, and the chapter says so instead of
implying the lottery bounds them.
