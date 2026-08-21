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

> **Update 2026-08-21 (later the same day).** Gates 0, 1 and 2 are closed in code. Every item in
> the tables below that is not marked otherwise has landed on `palw-base0-depth`; the summary is at
> the end of this section, and each entry names the property that was measured rather than the
> work that was done.

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

> **Closed.** `a_capture_becomes_a_refutation_the_court_adjudicates_both_ways` runs the integer
> engine, tiles every one of the thirty-eight steps a layer performs, assembles the binding and the
> refutation with the producer-side functions that did not exist (`canonical_input_leaves_v1`,
> `base0_binding_from_capture_v1`, `base0_refutation_from_capture_v1`,
> `open_artifact_leaf_v1`), and hands it to the court against the PRODUCTION inventory proven
> against its own artifact root. An honest capture is `NoFaultFound`; one changed value is
> `ComputationMismatch`. No synthetic leaf anywhere.
>
> **Five graph/engine divergences fell out of building it**, none of which any gate could see: three
> phantom norm-gain tensor families the engine never reads; the post table's missing narrowing; the
> four attention nodes declared once per LAYER while the engine runs them once per HEAD (so the step
> space did not contain the second head's softmax at all); the attention output declared `KvDim`
> where the engine writes `d_model`; and the inventory naming layers by substitution where the court
> asks by template. `the_engine_performs_exactly_the_graph_the_profile_declares` is what fails
> loudly on the sixth.

## Gate 1 — the chain can actually run the class

Three verified wiring breaks. Small, independent of Gate 0, and each fatal on day one.

| # | item | evidence |
|---|---|---|
| 1.1 | **thread the block's own work into the transition** | `processor.rs:1210` passes `None` for `current_attempt`, and admission decodes the envelope then discards it with `.map(\|_\| ())`. `apply_attempt` is the only thing that creates a claim, so no claim is ever created — no panels, no receipts, no courts, no payouts, and PALW weight permanently zero. |
| 1.2 | **route subnetwork 0x4a and 0x4b at tx validation** | `palw_carriage_tx_kind` covers 0x40–0x49. `SUBNETWORK_ID_PALW_FP_COMMITMENT` (0x4a) and `SUBNETWORK_ID_PALW_LIFECYCLE` (0x4b) fall to `SubnetworksDisabled`. Nothing in `consensus/src/processes/transaction_validator/` mentions either, and no test does. `subnets.rs:318` says the 0x4b id was minted to stop exactly the failure that is still live — and cites a test, `palw_v2_without_a_lifecycle_carriage_no_claim_can_ever_finalize`, **which does not exist**. |
| 1.3 | **a NetworkId that maps to a ConsensusV2 network** | `palw_rc_params` / `palw_rc_params_from_artifacts` exist and have only `#[cfg(test)]` callers; `Params::from(NetworkId)` panics on any testnet suffix but 10 and 11, and kaspad's only params source is `network.into()`. |

1.1 and 1.2 each independently force PALW weight to zero. Fixing one without the other changes
nothing observable, which is why they belong in one gate.

> **Closed, all three.** 1.1's fix made the per-bond exposure ceiling fire for the first time — it
> could not fail before, because `reserved` never left zero however long a chain grew. 1.2 routed
> both ids with the walk's own table. 1.3 returns the RC identity without the bundle, and an
> artifact-less node is refused at the handshake rather than joining silently.
>
> **A fourth wiring break was found beside them** (audit lane #3): the epoch budget could stop the
> chain, and the epoch could not end. Three causes, all real — the census denominator ADR-0045
> specifies was hard-wired to 1000‰, the liveness floor was subject to the cap, and the floor could
> be FROZEN by one accepted object with no path back.

## Gate 2 — the bounds and the money are real

| # | item | why |
|---|---|---|
| 2.1 | **ADR-0049 C — admission derives four cost bounds** | 0049-B made the opening small; C makes it *guaranteed*. Max opening bytes, terminal MACs, operand count, Merkle paths — each from the class's own geometry, each checked against a ruleset ceiling that joins `palw_ruleset_id_v2`. **Expires at genesis**, like the ladder. |
| 2.2 | **the bond is a locked UTXO** | `slash_bond` (`palw_state_v2.rs:1908`) decrements a `u64` and touches no output. `BondRegistered` is genesis-only *because* nothing locks collateral — the carriage says so. Until this lands, slashing a liar costs nothing spendable and every exposure ceiling and Sybil bound is denominated in an unenforced number. |
| 2.3 | **settle the escrow/subsidy question** | **Unverified.** `coinbase.rs` contains zero PALW arithmetic and the V2 escrow releases are appended to `validator_reward_outputs` (`utxo_validation.rs:835`) alongside a carve. Whether that rides the existing carve budget or adds to it decides between "the payout is wired" and "every finalized claim mints above the emission schedule". Settle it before anything carries value. |
| 2.4 | **re-decide ADR-0042 Decision 3c** | Its deferral rested on "only the bond holder can mint valid-signature siblings" — a statement about a signature somebody checks. P0 removed the exploit; the design question is open. |

> **Closed.** 2.2 landed in three parts — the genesis bond must SHOW its collateral, nobody may
> MOVE it while the bond lives, and a released bond's spend must DESTROY what the bond lost (with
> the block's fee pool losing it too, or the burn would be a redirection to the miner).
>
> 2.3 was the unverified one and it was **real**: the escrow was an ADDITION to the emission
> schedule, which ADR-0042 Decision 10 forbids in as many words. Measured on a live chain — subsidy
> 370,468,345 → worker share 229,690,375 → escrow 229,690,373 — and the carve now comes out of the
> block that earned it.
>
> 2.4's premise was **wrong**, and that is the finding. §A2's load-bearing reason is the
> cache-poisoning censorship path, not "nobody checks the signature", and verification ARMS it
> rather than removing it. 3c stays deferred on an unchanged precondition; what P0 narrowed is the
> residual, which is now measured instead of asserted.

## Gate 3 — the second class, weight-bearing

Only after Gates 0–2. Qwen2.5-1.5B is the proving ground and the plan for it is already recorded
(`docs/palw-two-class-plan-2026-08-21.md`), with one correction from this session's measurement:

* its shipped geometries are **inadmissible at their declared `n_ctx`** — 132.4 M and 219.7 M leaves
  against a 4,194,304 cap. `tile_len` 16,384 for 1.5B and 65,536 for 3B, the latter being
  `PALW_STEP_MAX_TILE_LEN` exactly. Either the tile grows or the context shrinks — and that
  sentence is a function now: `qwen25_admissible_geometry_v1` derives the pair against a given
  court, measuring **1.5B at `(64, 125)` and 3B at `(64, 79)`** under the shipped ceilings;
* post-genesis registration was **deliberately refused** by `palw_lifecycle_objects_v2`, and the
  reason given ("moves the share table, brings its own pwu rule") is exactly what
  `verify_class_admission_v2` checks. ADR-0049 H replaced the refusal with the gate, and it has
  landed: the object carries its shape profile and canonical job, acceptance runs the four checks,
  and an entrant joins at `min_grantable_share_permille` and no more;
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

## What is safe to say today, revised

> **Update 2026-08-22, `9d8c7645` — code-complete.** All eight launch dimensions in
> [the blockers document](palw-rc-launch-blockers-2026-08-21.md) are closed, each by a test that is
> red on regression — and the subsystem the 08-21 update named as the last blocker is now built:
> the material broadcast (2.27 MB per claim, relay-once, self-authenticating against the claim's
> committed roots), the panel service (`--palw-panel`), and the quorum submitter, whose object
> comes from the acceptance validator itself so the submitter and the chain cannot disagree. The
> wallet question was answered without a wallet: one funded outpoint on the bond key's own
> address, rolled through change. **What separates this from a public weight-bearing testnet-12 is
> operational**: a multi-node drill, and the genesis re-mint the state change already forces.
>
> Two consequences worth stating separately. The **ruleset id moved** (terminal-claim retirement
> changed `state_root`), so a testnet-12 genesis must be re-minted whatever else happens — which is
> the moment to settle M-02 rather than after. And the growth measurement behind that change is on
> record: without retirement the PALW state cost 8.2 ms and a 5.4 MB tip row per block after two
> weeks, 49 ms and 54 MB after four months, and did not plateau.

Gates 0, 1 and 2 are closed in code and covered by tests that fail when the property is removed.
What remains is not implementation:

* **Gate 3** — the second class needs calibrated numbers, which need the real checkpoint. The site
  and the plumbing exist (ADR-0050: the residual may amplify, and the gain is a committed tensor);
  the gains that make Qwen worth carrying weight are a converter output, and the class stays
  weightless until they justify activation.
* **Gate 4** — **the code half closed after this was written; what is left is operational.** See
  [the launch runbook](palw-rc-testnet12-launch-runbook.md). Three things changed the shape of this
  gate:
    * *The genesis artifact is derived, not hosted.* "The one input code cannot mint" was half
      wrong — BASE-0 has no file, it is a specification, so its artifact is produced by a rule and
      `artifact_root` is a re-derivable constant. What genuinely cannot be minted is the bond: a
      premine index and two ML-DSA-87 verification keys, and those ship UNSET on purpose.
    * *Nobody could produce a block.* `misaminer` and `pq-miner` branch on algo 4 and 5 only, and
      the sole algo-6 carriage builder was a `pub(crate)` test helper — a `ConsensusV2` network had
      a genesis and no second block, and no test said so. The producer is now in the node, because
      `challenge_v2` binds the nonce and so the carriage build and the nonce search are one loop.
      Third-party mining needs those facts on the RPC wire and is its own work.
    * *The RC carries the EVM lane, and that is a fleet build requirement.* testnet-12 inherits
      `TESTNET_PARAMS`'s EVM activation at DAA 0 while `MAINNET_PARAMS` never activates it. Turning
      it off looked right for a mainnet-candidate ruleset and was overruled: testnet-11 carries the
      lane and the RC is the network t11's traffic moves onto. The cost is real —
      `build_block_template` cannot build a template without `--features evm` — so a non-evm binary
      is now refused at startup with the rebuild command instead of panicking at its first
      template. The first end-to-end production test is what surfaced the whole question.

  Still operational and unchanged: the bond keys, seeds and public entry, the calibration-gated
  binary, and a soak whose clock survives redeployment.

  **What actually stops the launch is now tracked in its own document:**
  [palw-rc-launch-blockers-2026-08-21.md](palw-rc-launch-blockers-2026-08-21.md) — a 36-agent
  verification of the external audit's 19 findings plus eight launch dimensions, with what has since
  been closed (and the mutation measurement that says so) and what has not. Read it before planning
  any t12 work; this road map records the SHAPE of the gates, that one records their state.
* **Gate 5** — Qwen3.6 is still its own ADR, and Gate 0 closing is what makes it worth writing.

Two things a reader should carry forward from how these closed, because both recurred:

1. **The gates that never fired were the dangerous ones.** The exposure ceiling, the genesis
   loader, the court's cost ceilings and `op 9` had all been written, reviewed and tested — and
   none of them could refuse anything, because the thing they guarded never reached them. A check
   that cannot fail is indistinguishable from a check that passes.
2. **Correspondence defects are found by round trips, not by reading.** Five divergences between
   the engine, the profile, the inventory and the court survived every review; each one surfaced
   the moment two sides were asked to agree on a real object. Building the producer added four
   more, all of the same kind: the post table — including the LOGITS HEAD — was never captured, so
   the node that decides what the model said was the one part of the graph no refutation could
   open; a capture had no way to span a job's calls; `rc_job_context` is a yardstick and not a job,
   so the first honest execution run against it produced evidence the court refused; and the two
   float-runtime legs cannot be produced by an integer class at all.

## What was safe to say before this session

> The PALW ConsensusV2 state machine, integer primitives, step dispute and class economy are
> implemented and under internal shadow testing.

What is not safe to say, and will not be until Gate 0 closes:

> A weight-bearing testnet where full nodes verify a 35B model by re-executing one tile.
