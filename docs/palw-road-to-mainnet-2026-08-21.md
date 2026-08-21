# PALW: the ordered path to a mainnet-grade network

Written 2026-08-21, after the external NO-GO audit, an eight-dimension adversarial sweep, a wiring
census, and three landed fixes. Every claim below is either cited to code this session verified, or
marked **unverified**.

**The premises are fixed and nothing here relaxes them.** Block production is a PALW lottery,
hash-function-independent. The free-prompt lane stays: a miner earns by running a useful inference
for a real prompt. Several items exist *because* of those two.

---

## 0. What "mainnet" can mean, and which one this is

The shipped mainnet identity **cannot carry PALW**, and this is enforced rather than merely stated:
`Params::validate_palw_v2` demands the frozen 120,000 ms cadence and mainnet is
`BlockrateParams::new::<10>()`; separately, at 10 BPS `finality_depth` exceeds any shipped
`w_challenge`, so `finality_depth < w_challenge` fails on the depth alone. Fixing either leaves the
other standing.

So "to mainnet" means **a new PALW mainnet-grade network**, not a flag day on the existing one.
Everything below is written for that reading. It is not a gap to close; it is a decision the code
already makes.

---

## Where the work stands

**Landed this session**, each mutation-checked:

| | what | evidence it was real |
|---|---|---|
| P0 | the attempt's ML-DSA-87 signature is verified on the live path | reverting it makes a forged block become the sink |
| 0049-A | the operand oracle speaks bytes | op 9 (`Rescale`) adjudicated through a production oracle for the first time |
| 0049-B | the court opens the challenged tile | Qwen2.5 unembed: ~223 MiB → 192 KiB |

**Three stale tests fell out of those**, each passing for a reason other than the one it stated. That
is the shape of the remaining risk: the code is further along than the docs, and the tests are
further behind than their names.

---

## Gate 0 — BASE-0 closes, end to end, from a real execution

**Nothing above this gate means anything.** The floor class is where a real worker execution, a real
block, a real challenge, a real artifact opening and a real court close must meet for the first
time. Adding classes before it closes makes unconnected parts more impressive, not more connected.

| # | item | why it blocks | size |
|---|---|---|---|
| 0.1 | **ADR-0050 A — the residual site gains its declared narrowing** | `palw_base0_profile.rs:246` is a bare `AddElem`; node 11 consumes it via `as_i8`, and an `AddElem` of two int8 codes ranges over `[-256, 254]`. The declared graph does not adjudicate at its residual sites the moment the residual carries signal. The FFN residual is worse — both operands are raw i32. | small |
| 0.2 | **ADR-0049 F — one canonical execution IR** | 0.1 exists because engine, profile, court and inventory are four hand-written descriptions of one computation. Fixing the residual by hand leaves the next divergence to be found the same way. Interim rule until the generator exists: **no worker may commit a step leg for a profile that omits a narrowing the engine performs.** | medium |
| 0.3 | **ADR-0049 E — adjudicable decode** | `palw_step_refute.rs:394` refuses embedding at `call_index != 0`, and BASE-0's own canonical job is prefill 8 / **decode 4**. Commit the logits root per decode position; a challenger opens ONE index to refute the argmax. O(1) in vocabulary. Required, not optional: on the free-prompt lane the generated text is the product. | medium |
| 0.4 | **ADR-0049 D — coverage over coordinates** | 0.3 is invisible to the current gate, which compares kernel ids. Every kernel BASE-0 reaches is catalogued and a whole call class still refuses. | small |
| 0.5 | **ADR-0049 G — the canonical artifact inventory** | A real opening needs a real inventory: one leaf per operand row, every byte covered exactly once, duplicates/overlaps/gaps refused. Also settles the two things called "class id" — `execution_class_id` is the shape profile id, `artifact_root` is the Merkle root over the manifest. | medium |
| 0.6 | **C-01 — the worker emits step legs** | The audit's largest finding, and the worker's own comments say it: no per-kernel tile capture, no path from an execution to `execution_root`. Depends on 0.2 (the IR defines what a tile is) and 0.5 (what an operand opening addresses). | large |

**Gate 0 passes when**: a worker runs one BASE-0 inference, commits a step leg, a block carries the
attempt, a challenger disputes one tile, and a node holding no model closes the court — with no
synthetic leaf anywhere in the chain of custody.

## Gate 1 — the chain can actually run the class

Three verified wiring breaks. Small, independent of Gate 0, and each fatal on day one.

| # | item | evidence |
|---|---|---|
| 1.1 | **thread the block's own work into the transition** | `processor.rs:1210` passes `None` for `current_attempt`, and admission decodes the envelope then discards it with `.map(\|_\| ())`. `apply_attempt` is the only thing that creates a claim, so no claim is ever created — no panels, no receipts, no courts, no payouts, and PALW weight permanently zero. |
| 1.2 | **route subnetwork 0x4a and 0x4b at tx validation** | `palw_carriage_tx_kind` covers 0x40–0x49. `SUBNETWORK_ID_PALW_FP_COMMITMENT` (0x4a) and `SUBNETWORK_ID_PALW_LIFECYCLE` (0x4b) fall to `SubnetworksDisabled`. Nothing in `consensus/src/processes/transaction_validator/` mentions either, and no test does. `subnets.rs:318` says the 0x4b id was minted to stop exactly the failure that is still live — and cites a test, `palw_v2_without_a_lifecycle_carriage_no_claim_can_ever_finalize`, **which does not exist**. |
| 1.3 | **a NetworkId that maps to a ConsensusV2 network** | `palw_rc_params` / `palw_rc_params_from_artifacts` exist and have only `#[cfg(test)]` callers; `Params::from(NetworkId)` panics on any testnet suffix but 10 and 11, and kaspad's only params source is `network.into()`. |

1.1 and 1.2 each independently force PALW weight to zero. Fixing one without the other changes
nothing observable, which is why they belong in one gate.

## Gate 2 — the bounds and the money are real

| # | item | why |
|---|---|---|
| 2.1 | **ADR-0049 C — admission derives four cost bounds** | 0049-B made the opening small; C makes it *guaranteed*. Max opening bytes, terminal MACs, operand count, Merkle paths — each from the class's own geometry, each checked against a ruleset ceiling that joins `palw_ruleset_id_v2`. **Expires at genesis**, like the ladder. |
| 2.2 | **the bond is a locked UTXO** | `slash_bond` (`palw_state_v2.rs:1908`) decrements a `u64` and touches no output. `BondRegistered` is genesis-only *because* nothing locks collateral — the carriage says so. Until this lands, slashing a liar costs nothing spendable and every exposure ceiling and Sybil bound is denominated in an unenforced number. |
| 2.3 | **settle the escrow/subsidy question** | **Unverified.** `coinbase.rs` contains zero PALW arithmetic and the V2 escrow releases are appended to `validator_reward_outputs` (`utxo_validation.rs:835`) alongside a carve. Whether that rides the existing carve budget or adds to it decides between "the payout is wired" and "every finalized claim mints above the emission schedule". Settle it before anything carries value. |
| 2.4 | **re-decide ADR-0042 Decision 3c** | Its deferral rested on "only the bond holder can mint valid-signature siblings" — a statement about a signature somebody checks. P0 removed the exploit; the design question is open. |

## Gate 3 — the second class, weight-bearing

Only after Gates 0–2. Qwen2.5-1.5B is the proving ground and the plan for it is already recorded
(`docs/palw-two-class-plan-2026-08-21.md`), with one correction from this session's measurement:

* its shipped geometries are **inadmissible at their declared `n_ctx`** — 132.4 M and 219.7 M leaves
  against a 4,194,304 cap. `tile_len` 16,384 for 1.5B and 65,536 for 3B, the latter being
  `PALW_STEP_MAX_TILE_LEN` exactly. Either the tile grows or the context shrinks;
* post-genesis registration is **deliberately refused** by `palw_lifecycle_objects_v2`, and the
  reason given ("moves the share table, brings its own pwu rule") is exactly what
  `verify_class_admission_v2` checks. ADR-0049 H replaces the refusal with the gate;
* 0‰ is impossible by construction (`min_grantable_share_permille` ≥ 1). "Weightless" is the
  **minimum grantable share**, and a class holding 1‰ produces — which is better, because a class
  that produces can be watched before it is grown.

## Gate 4 — the network exists

Operational, not consensus. Genesis artifact (the one input code cannot mint is `artifact_root`),
seeds, public entry, the calibration-gated binary fleet-wide, and a soak whose clock survives
redeployment. ADR-0035 §6 owns this list; the t11 experience says budget 1.5–2 days for a first sync
on a mature chain and expect the switch-counter and remote-panic fixes to be the thing nobody
deployed.

## Gate 5 — Qwen3.6-35B-A3B

MoE, expert routing, GatedDeltaNet, SSM. A new op set with accumulator and **state** bounds proven
over a recurrence rather than over one dot product — a genuine ADR, and one not worth writing
against an adjudication contract that does not close. Gate 0 is its precondition, not its rival.

---

## The two decisions that expire

Everything else can be fixed later. These cannot, because they are inside `palw_ruleset_id_v2` and
therefore inside the network's identity:

1. **the court ladder** — already gated: `assemble_palw_rc_identity_v2` refuses any
   `max_step_leaf_count` but `PALW_STEP_MAX_LEAVES`. Four extra bisection rounds buys every class
   that could ever be adjudicable.
2. **the four cost ceilings** (2.1) — not yet gated. Too small forecloses classes; too large admits
   a proof nobody can verify in time.

## What is safe to say today

> The PALW ConsensusV2 state machine, integer primitives, step dispute and class economy are
> implemented and under internal shadow testing.

What is not safe to say, and will not be until Gate 0 closes:

> A weight-bearing testnet where full nodes verify a 35B model by re-executing one tile.
