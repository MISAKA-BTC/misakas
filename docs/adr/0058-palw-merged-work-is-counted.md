# ADR-0058: Merged work is counted — the mergeset carries claims, not just the chain

- Status: Accepted
- Date: 2026-08-27
- Depends on: ADR-0038 (PALW is consensus work), ADR-0039 (per-class DAA), ADR-0045/0056
  (share economy), ADR-0054 (share follows production)
- Supersedes: nothing; amends the transition's step 4 and the meaning of "produced" everywhere
  a counter reads it

## The defect, measured

On testnet-11 (`bb0a3ad3…`, 2026-08-27), the Qwen3.6 class produced 15 blocks by real
inference. Zero of them are on the selected chain; 743 floor blocks are (542 of 546 chain
slots). The margin each Qwen block lost by — 2M to 14M blue work, i.e. 2 to 10 floor blocks —
is its own inference latency: at 5–19 minutes per block against the floor's ~1 per minute, a
Qwen tip is always stale by the time it exists. The probability of losing 12 of 12 by chance
is ~4×10⁻⁷. This is structure, not luck.

The structure: a block's PALW attempt is applied to chain state only when the block joins the
virtual selected chain, and ordinary tip selection on a V2 network is deliberately blue-work
ordered (the pre-validation heap cannot evaluate the PALW comparator — see the wedge history in
`palw_tip_weights_v1` — so the one comparator runs at the deep-reorg gate, which a tip that
*extends* the sink never reaches). Both halves are individually correct. Together they mean a
slow-cadence class **cannot create claims at all**:

* no claim → `epoch_counters` never move → the per-class retarget's `observed` is 0 forever
  (`observed == 0 → continue`), so difficulty never eases;
* no counter → ADR-0054's growth walk sees "produced nothing" forever, so the share step the
  class was promised for filling its budget can never fire;
* no claim → no weight, so the work secures nothing and pays nothing.

Difficulty and share were designed to follow *measured production*, and the measurement was
wired to a race a slow class structurally loses. Every entrant class heavier than the floor
starves on arrival, at any share, at any difficulty.

## The decision

**A chain block applies the PALW work of its whole mergeset — blues and reds alike — not just
its own.**

Reds are not an edge case; on this network they are the point. The frozen 120 s cadence fixes
`ghostdag_k = 1`, so any block whose anticone holds two or more blocks is a red *by
construction* — which is every block of every class slower than the floor. All twelve real
Qwen3.6 blocks measured above are reds. A blues-only rule would have measured nothing.

1. **Step 4 of the transition** takes, after the block's own work, the work of every merged
   block of its mergeset (selected parent excluded — it applied its own work when it was the
   chain tip; non-DAA blocks excluded, matching the coinbase's pay set), in consensus mergeset
   order, reds included.
2. **Acceptance-context admission.** Each merged work passes the full stateful admission
   (`check_palw_attempt_admission_v2`) against the accepting block's live fold state — the same
   state, re-checked sequentially, so two merged blues cannot both take the last budget slot or
   the last sompi of exposure headroom. The stateless half (shape, challenge binding, executor
   signature) is checked once in the processor, against the carrying header.
3. **Refusal skips; it never disqualifies.** The accepting block did not author its anticone. A
   merged work the state refuses (budget exhausted, bond retired meanwhile, duplicate, exposure
   ceiling) is skipped deterministically — every node, same state, same order, same verdict —
   and the block stands. The block's OWN work still disqualifies on refusal, unchanged.
4. **The claim records the carrying block.** `accepted_block` is the merged blue itself — the
   panel derives the job anchor from the carrying header's pre-PoW hash, and the producer bound
   its material to that block. `accepted_daa`/`accepted_blue_score` are the accepting chain
   block's — deadlines and the safe frontier are chain-order facts.
5. **A merged claim escrows nothing (`escrowed_reward = 0`), this revision — and the coinbase
   pays an entitled in-window RED to its own miner script.** Blues were already paid to their
   own miners; entitled reds' worker shares were lumped into the *merging* miner's red reward,
   which under this ADR would have put the slash exposure on one key and the pay on another.
   The red's share now goes to the red's own script, through the same carve arithmetic, gated
   to `ConsensusV2` networks so every other network's coinbase stays byte-identical.
   Making merged claims escrow instead would require the coinbase to know the transition's
   outcome before the transition runs (they validate in that order). Today an entitled merged
   block is paid with *no claim, no verification and no slash exposure*; under this decision it
   is paid the same but its claim now exists — panel-verified, court-triable, bond-slashable
   (`reserved = pwu × slash_value_per_pwu ≫ carve`). Strictly tighter than the status quo.
   Symmetric escrow (withhold every applied merged block's carve, release at `Final`) is left
   as a follow-up that restructures coinbase validation ordering.
6. **Zero-escrow claims enqueue no payout.** A `Final` with `escrowed_reward = 0` writes no
   payout row, so no zero-value coinbase output can exist.

## What this closes

* **The retarget loop closes.** `apply_attempt` bumps `epoch_counters`; merged production now
  counts, `observed > 0`, the per-class DAA eases toward the class's budget share.
* **ADR-0054 closes.** The growth walk reads the same counters; a class that fills its budget
  from the anticone steps its share up exactly as promised.
* **ADR-0038's premise is restored.** PALW weight was "the network's whole fork choice", but
  only chain blocks minted weight, so a class that lost parent selection contributed no
  security. Merged claims mature into `safe_weight` on every chain that merges them; a private
  fork that excludes them now weighs less than the public chain that counts them.
* **Pay-without-verification narrows.** The entitled merged block's instant payment existed
  with no claim behind it; now the claim, panel duty and slash path exist for every applied
  work — and for reds, the payment finally lands on the key that carries the slash.

## Consensus impact — read before deploying

* `PALW_STATE_V2_VERSION` 9 → 10. Every state root moves from genesis.
* The params fingerprint now hashes `PALW_STATE_V2_VERSION` explicitly. Every previous bump
  moved the handshake only because it happened to ride a bundle-shape change; a semantic-only
  bump like this one would have left old and new nodes peering and then silently disqualifying
  each other's chains. The version is block-validity-relevant, so it is hashed.
* **testnet-11 must re-mint.** Old-fingerprint nodes refuse new ones at handshake (that is the
  point). Artifacts, class ids, pins and bonds' keys are untouched; genesis and the network
  fingerprint change.

## Rejected alternatives

* **Weigh the candidate tip's own attempt in fork choice.** Inverts the maturity principle
  (fork choice would trust unverified claimed pwu), and mirrors the bug instead of fixing it:
  if a Qwen tip always out-weighs floor tips, floor blocks stop being chain blocks and *their*
  production stops being counted.
* **Throttle the floor operationally.** The floor is permissionless; a fairness property that
  depends on volunteers slowing down is not a property.
* **Count merged blocks into the counters without applying claims.** Difficulty would ease and
  share would grow for work nobody verified and nobody can slash — production without
  accountability, worse than the defect.
