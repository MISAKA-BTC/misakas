# ADR-0044: Free-prompt PALW — the user's own inference becomes the consensus work, certified before it mines

Status: **Proposed.** Activates nothing, moves no shipped preset, changes no live network. It is the
engineering spec the `palw-freeprompt-v3` branch implements on top of the `palw-v2` RC substrate
(ADR-0042, PR-00…PR-10).

Date: 2026-08-20
Branch: `palw-freeprompt-v3` (cut from `palw-v2` @ `a460cdd7`).

Relates to / builds on:

- **ADR-0042** (the atomic RC ruleset; `PalwConsensusMode::ConsensusV2`) — this ADR **extends the
  V2 bundle** with a second work source. It does not add a fence: the free-prompt ruleset is a
  required part of the bundle, validated at boot, hashed into `palw_ruleset_id_v2`. A bundle
  without it is a different ruleset, checkable by fingerprint, exactly as Decision 11 demands.
- **ADR-0038 / ADR-0039** (PALW is the consensus work; ticket-not-hash; per-class DAA; bonded
  permissionless) — carried. This ADR keeps the attempt lottery *and* adds the receipt lane.
- **ADR-0037 Decisions 2–9** (state machine, panels, court seating) — the claim lattice this ADR
  certifies receipts through is the one `palw_state_v2.rs` already implements.
- The **submitted free-prompt draft** (2026-08-20, "Audit-before-Mine"): user-selected prompts,
  commit-then-future-randomness audit, one-shot certified receipts, PWU from measured work, no
  retroactive weight. This ADR adopts that structure and **repairs two of its mechanisms** (§Context)
  under the standing instruction to prefer the safer construction where one exists.

Supersedes: nothing's decisions. It **amends** ADR-0042 Decision 3's "the only algorithm a V2
network demands or accepts" from one id to two (attempt + receipt), and it **fills** ADR-0039's
open PWU-derivation item for the receipt lane only.

---

## Context — what was asked, and the two flaws in the straightforward answer

The product ask: a person uses their own LLM for their own work — code review, drafting,
summarizing — through a normal chat interface, and **that same single inference** is what mines
MISAKA blocks. The chain must not assign the prompt; the user must not run a second
mining-only inference; a receipt must be usable once; and block weight must never be revised
after acceptance.

The submitted draft gets the *shape* right: run the user's inference first, commit its trace,
select an audit panel from randomness that becomes known only after the commitment, certify,
then let the certified receipt win a block ticket drawn from later randomness. Two of its
mechanisms, however, are unsound on this codebase, and both failures are structural, not
parametric:

### Flaw 1 — "future block hash" randomness is free to grind once blocks need no inference

Today's attempt lottery (algo 4, and V2's algo 6) has one property everything else leans on:
**re-rolling any chain hash costs one inference.** `challenge = H(net ‖ pre_pow ‖ timestamp ‖
nonce ‖ class ‖ bond)` is consumed *by* the model run, so a producer who wants a different block
hash — to bias a panel draw, a ticket seed, anything derived from "the first chain block after
X" — pays a full inference per sample. `PalwAnchorFactV2`'s anchor rule is sound *because* of
this.

The draft removes the inference from block production ("nonce探索なし") and keeps deriving both
the audit-panel seed and the block-ticket seed from future block hashes. But a receipt-licensed
block with no work function over its header bytes is **costlessly malleable by its own
producer**: nonce and timestamp alone give ~2^64 free re-rolls, parent selection gives more. The
producer of the anchor block can therefore grind the panel that will audit *their own* pending
commitment, and the ticket seed that assigns the next block rights — at zero cost. That is not a
parameter to tune; it is the loss of the property the whole anchor construction stands on.

### Flaw 2 — any executor-chosen field the inference does not consume is a free grinding surface

In the attempt lottery, grinding is priced because the lottery inputs pass *through* the model.
A free-prompt job is the opposite: the model consumes **only the user's tokens** (that is the
point — F1, the prompt is not mutated), so every other identity field (`job_nonce`, bond choice,
padding) can be re-chosen *after* the inference at zero cost. Any randomness derived from the
receipt's own identity — `ticket = H(receipt_id)`, or a per-bond precommitted secret evaluated
against `receipt_id` — is therefore grindable: one inference, millions of candidate ids, submit
the winning one. The draft's own rule "the ticket must not derive from prompt or output" is
necessary but not sufficient; it must not derive from *anything the executor can re-choose
post-hoc*, which is every field except the chain's own later randomness.

### The repair, in one sentence

**Keep the attempt lane as a minority floor, and let its inference-hardened block hashes be the
only randomness source ("beacons") for both panel selection and receipt tickets; receipts are
certified through the existing claim lattice before any block may spend them.** Grinding a beacon
costs one inference per sample again; receipts are fixed on chain before their beacon exists; the
floor also solves bootstrap (a fresh chain runs on attempts until the first receipts certify) and
anti-stall (a broken receipt pipeline degrades to the floor instead of halting the chain), which
retires the draft's genesis-receipt ceremony entirely.

### What certification reuses

The draft's pipeline — commitment → future-randomness panel → replay attestations → quorum
certificate → challenge window → certified receipt — is **structurally the claim lattice that
already exists**:

```
Provisional ──beacon anchor──▶ PanelBound ──quorum receipt──▶ ReceiptLicensed ──window──▶ Final
                                                    │                                        │
                                                    ▼                                        ▼
                                                 Voided ◀──── court fraud / withholding    (certified;
                                                                                            spendable)
```

`palw_panel_v2.rs` (sortition, exclusion trio, two-sided quorum), `palw_court_v2.rs`
(proof-carrying arithmetic adjudication), `palw_state_v2.rs` (snapshot accounting, exposure,
deadline sweeps, delta apply/revert) and `palw_reward_v2.rs` apply verbatim. What ADR-0042 audits
*after* a block (its own attempt), this ADR audits *before* one (a standalone job). A
**certified receipt is a free-prompt claim in `Final`**, nothing more.

---

## Decision 1 — Two work sources, one atomic bundle

A `ConsensusV2` network with this ADR's bundle runs two block kinds:

| Lane | Algo id | Work | Share of cadence | Roles |
|---|---|---|---|---|
| **Attempt** | 6 (`POW_ALGO_ID_PALW_COMMITTED_V2`) | one fresh chain-challenge inference per ticket, exactly ADR-0042 | minority floor (`attempt_share_permille`, e.g. 150‰) | randomness beacons · bootstrap · anti-stall floor |
| **Receipt** | 7 (`POW_ALGO_ID_PALW_RECEIPT_V3`) | spending one quantum of a certified free-prompt receipt | the remainder (e.g. 850‰) | the user's own inference, mined |

Rejected alternative — a pure-receipt chain (the draft's final form): it needs a genesis
bootstrap-receipt ceremony, halts outright if the certification pipeline stalls (this network has
already lived through an 81-hour wedge; a work source with no floor is a wedge invitation), and —
Flaw 1 — has no grind-priced hashes left to derive randomness from. The floor's "waste" is the
explicit price of unbiasable randomness, and it is the same inference-work the chain's identity
is built on, not a hash fallback (ADR-0039's hash floor stays superseded).

Rejected alternative — RANDAO-style epoch seeds over certified-receipt id sets: last-actor
withholding bias is bounded but real, it adds epoch machinery, and it still needs the floor for
bootstrap and stall anyway. Beacons dominate it on every axis here.

The two lanes are **one bundle**. There is no receipt lane without the attempt lane, no fence
that enables one of them, and `palw_ruleset_id_v2` covers both (Decision 9).

## Decision 2 — The free-prompt job: user tokens in, nothing appended, total binding

```rust
pub struct PalwFreePromptJobV3 {
    pub version: u16,               // PALW_FP_V3_VERSION
    pub network_domain: Hash64,     // same value the attempt lane binds (Decision 3a of ADR-0042)
    pub class_id: PalwClassId,      // registry resolves model/runtime/tokenizer/manifest — ids are lookup keys, not passengers
    pub executor_bond: TransactionOutpoint,
    pub executor_pubkey: …,         // MUST equal the bond record's key (admission, not trust)
    pub operator_id: Hash64,
    pub anchor_block: Hash64,       // chain freshness binding: a recent chain block
    pub anchor_daa: u64,
    pub job_nonce: [u8; 32],        // uniqueness only — carries no lottery meaning (Flaw 2)
    pub tokenizer_id: Hash64,       // MUST equal the class row's tokenizer_id (cross-check, admission-refused on mismatch)
    pub prompt_token_ids_hash: Hash64,
    pub prompt_tokens: u32,
    pub decode_token_limit: u32,    // ceiling; actual decode may stop at EOG (Decision 7)
    pub max_context_tokens: u32,
    pub privacy_mode: u8,           // 1 = PublicDa; the only weight-bearing mode at v1 (Decision 8)
}
```

`fp_job_id_v3 = H(domain ‖ canonical(job))` — **every field is in the preimage.** The draft's
job-id list omitted four of its own struct fields; on this codebase "bound indirectly through
another field" is the defect class the audits keep finding, so the rule is total binding, no
exceptions. Model/runtime/manifest identities are deliberately *not* repeated in the job: the
class registration (`PalwClassRegistrationV1`) already binds them, admission checks the job
against the registered row, and carrying the same fact in two places is how the two drift apart.

**F1 — the prompt is not modified.** The model consumes `prompt_token_ids` and nothing else. The
legacy VLT executor's `new_job_input` DAA-suffix (`kaspad/src/compute.rs:428`) does not exist on
this path and must never be ported to it: freshness is bound by `(anchor_block, anchor_daa,
job_nonce)` in the job identity, outside the model's input. PALW on or off, the user's answer is
byte-identical.

## Decision 3 — The execution commitment, and certification through the existing lattice

After the inference (one inference — the same one that answered the user):

```rust
pub struct PalwFreePromptCommitmentV3 {
    pub job: PalwFreePromptJobV3,
    pub trace_root: Hash64,             // full_logits_trace_root_v2 — openable, court-grade
    pub output_root: Hash64,            // output_commitment_v2
    pub schedule_root: Hash64,          // operation_schedule_commitment
    pub decode_tokens_executed: u32,    // ≤ decode_token_limit; EOG is a real stop (Decision 7)
    pub stop_reason: …,                 // ExactBudgetReached | EndOfGeneration
    pub cu: u128,                       // recomputed by every validator from the CU rule — a claim, checked, never trusted
    pub trace_manifest_root: Hash64,    // DA obligation trio, as PR-06 defined it
    pub trace_chunk_count: u32,
    pub trace_retention_daa: u64,
}
// + envelope { commitment, signature }  — identity is fp_claim_id_v3 = H(canonical(commitment)),
//   NOT the signature bytes (ML-DSA-87 signatures are not unique; ADR-0042 Decision 3c).
```

The commitment (with the **prompt token ids carried whole** — PublicDA, Decision 8) rides an
overlay transaction. When its block is accepted, the state transition folds a
`FreePromptCommitted` object: a claim is created at `Provisional`, exposure is reserved against
the executor's bond exactly as an attempt claim reserves (one shared per-bond ceiling across both
lanes — two lanes with separate ceilings is doubled leverage), and from there the existing
machinery runs unmodified:

- **PanelBound** — panel derived by `derive_panel_v2` from the claim's **beacon** (Decision 4),
  exclusion trio and operator dedup as PR-06 built them;
- **ReceiptLicensed** — `validate_receipt_quorum_v2`, two-sided (Valid / producer-Unavailable);
  a panel seat replays the job from the on-chain prompt tokens and the class's pinned runtime —
  the same `--mode v2-job`-style replay the VLT lane already performs;
- **court** — `palw_court_v2` verbatim: proof-carrying arithmetic refutation against the
  committed trace root; `DoesNotAdjudicate` refuses the close; window_court backstops;
- **Final** — the challenge-window sweep. **Final IS the certified receipt.** `Voided` forfeits
  and slashes through the existing paths.

No new certification machinery is invented by this ADR. That is its main safety argument: the
panel, quorum, court, exposure and sweep code paths are the ones already carrying adversarial
tests.

## Decision 4 — The beacon rule: only attempt blocks carry randomness

```
beacon(slot) = the FIRST chain block B with B.daa ≥ slot whose algo id is the ATTEMPT id (6)
```

- **Panels**: a free-prompt claim's anchor slot is `accepted_daa + anchor_delay` (as V2), and its
  anchor is `beacon(slot)` — carried as `PalwBeaconFactV3 { beacon_block, beacon_daa,
  prev_attempt_daa }`, the `PalwAnchorFactV2` shape with one added obligation: `prev_attempt_daa`
  is the DAA of the last attempt-class chain block *before* the slot, so "first attempt block at
  or after the slot" is checkable, and every receipt-class block in between is structurally
  irrelevant. Under this ADR's bundle the **attempt lane's own claims anchor on beacons too** —
  one anchor rule per network, not one per lane.
- **Tickets** (Decision 5): drawn from `beacon(final_daa + receipt_maturity_daa)`.

Why this closes Flaw 1: an attempt block's hash is downstream of its commitment root, which is
downstream of `challenge = H(… ‖ timestamp ‖ nonce ‖ …)` — the finalizer consumes
`Expand(commitment_root)`, so **every alternative beacon sample costs one full inference**, and
withholding a winning attempt to re-roll additionally forfeits its block reward. Receipt blocks,
whose headers are costlessly malleable, can never be anchors. This is also why the floor share
has a lower bound (Decision 9): beacons must keep arriving.

**Nothing may derive randomness from receipt-block hashes.** That sentence is an invariant
(F15), not a guideline.

## Decision 5 — Quantized one-shot tickets

Certification prices the work (`cu`, Decision 7); the lottery consumes it in **uniform quanta**:

```
quanta(claim)          = min( ⌊cu / quantum_cu⌋ , max_quanta_per_receipt )
ticket(claim, q)       = leading 128 bits of H(FP_TICKET_DOMAIN ‖ network_domain ‖ beacon_block ‖ claim_id ‖ q_le)
eligible(claim, q)     ⟺ palw_ticket_admits_v1(ticket, receipt_target[class])
```

- One quantum = **one draw**, against the single beacon `beacon(final_daa +
  receipt_maturity_daa)` — fixed by the chain, after the claim is irrevocably `Final`. The
  executor cannot re-choose anything the draw depends on (Flaw 2 closed: claim id is fixed on
  chain before the beacon exists; grinding `job_nonce` at job time cannot aim at a beacon that
  does not exist yet, and grinding the beacon costs inferences).
- One winning quantum = **one block**, spendable once per candidate chain (the spent set lives in
  the claim's state; a fork may spend it too — that is the UTXO double-spend analogy, resolved by
  fork choice, never by a node-global cache).
- A win must be used promptly: the spending block's DAA must lie in
  `[beacon_daa, beacon_daa + receipt_use_window_daa]`. An unused win expires — a weight claim
  from the deep past is a reorg lever, not a savings account.
- Uniform quanta make the lottery math trivial (flat per-class target, the existing 128-bit
  ticket space and `palw_ticket_admits_v1` reused as-is), give a big job and ten small jobs the
  same expected block count per CU, and bound the per-receipt jackpot (`max_quanta_per_receipt`)
  — the draft's `MAX_PWU_PER_RECEIPT` split, made the primitive instead of the patch. A
  sub-quantum job (`cu < quantum_cu`) is not carried on chain at all: the state refuses a
  zero-quanta commitment, because a claim that can never act is exposure reserved for nothing
  and dead weight in every root.
- Expected value is linear in certified CU; variance favors nobody. Pooling needs no protocol:
  a gateway pointing at a shared bond *is* a pool (the bond owner is the accountable party).

Public predictability: once the beacon lands, everyone can enumerate the winning quanta of the
next window. That is an accepted v1 trade (producers are bonded operators; the window is short;
a DoS'd producer costs the chain one slot, not liveness). A private-eligibility layer
(per-bond precommitted secret Merkle trees) is sketched under *Not decided*.

## Decision 6 — The receipt block (algo 7): admission a full node runs with no model

A receipt block's post-PoW header extension carries the spend envelope (the
`palw_block_commitment.rs` carriage shape):

```rust
pub struct PalwReceiptSpendUnsignedV3 {
    pub version: u16,
    pub network_domain: Hash64,
    pub claim_id: Hash64,          // the certified commitment being spent
    pub quantum_index: u32,
    pub beacon_block: Hash64,      // the draw this spend claims
    pub producer_bond: TransactionOutpoint,   // MUST be the claim's executor bond
    pub producer_pubkey: …,
}
// spend_id_v3 = H(canonical(unsigned));  L1 tag = Expand(spend_id)  — the finalizer consumes it,
// so a receipt block's identity is total over its spend, and the signature is a witness.
```

Admission, stateless: version · canonical encoding · size bounds · `spend_id` recompute ·
signature under the carried key · L1-tag/finalizer recompute · network domain. Stateful, against
the candidate chain's `PalwChainStateV2`:

```
1. claim exists and is a free-prompt claim in Final
2. quantum_index < quanta(claim), and not in the claim's spent set
3. beacon_block == beacon(final_daa + receipt_maturity_daa) on this chain (beacon fact checked)
4. ticket(claim, q) admits under receipt_target[class] at the CANDIDATE point
5. block DAA within [beacon_daa, beacon_daa + receipt_use_window_daa]
6. producer_bond == the claim's executor bond, Active, not in withdrawal
7. the bond record's pubkey == the carried producer_pubkey
8. class Active (not frozen) at the candidate point
```

Item 4's target point, precisely: the BEACON fixes the draw (historical, grind-priced); the
TARGET is the spending block's own difficulty context, read at the candidate point like every
difficulty check on this chain — past targets are not state. A marginal ticket can flip
eligibility across a retarget boundary inside its use window, deterministically and identically
on every node, with no grinding surface. (An earlier draft of this ADR said "as of the beacon's
chain point"; the implementation is where that was discovered to demand a target history nobody
keeps, and this text now matches the code.)

Weight: a receipt block contributes `pwu_per_quantum` at **`Final` stage immediately** — safe
weight at acceptance. Everything the block's validity rests on is prior chain fact; there is
nothing left to dispute, so there is no ramp, no `Provisional` stage, and **no post-acceptance
weight revision** (the draft's F10, achieved by construction). For the same reason the receipt
block's coinbase needs **no escrow ladder**: `palw_reward_v2`'s escrow exists because an attempt
block can still be voided; a receipt block cannot. Ordinary coinbase maturity applies.

The header `nonce` remains a uniqueness field with **no work meaning and no randomness meaning**
(Decision 4). Full nodes run: hashes, signatures, one beacon-fact check, one set lookup, one
ticket comparison. No model, no exception — algo 7 is deliberately *not* in `is_palw_algo`'s
inference-priced set, and a test pins that a node with no runtime installed validates receipt
blocks completely.

## Decision 7 — Pricing: CU from the executed shape, conservative by construction

The VLT lane's `cu = prefill + 8·decode` (v2, frozen) is a fairness heuristic, not a security
bound. The receipt lane gets its own rule:

```
fp_cu_v3 = prompt_tokens · cu_prefill_weight  +  decode_tokens_executed · cu_decode_weight
```

with weights in the bundle (hence in the ruleset id), chosen from the class calibration harness
under one invariant:

> **No workload shape may yield more CU per real second than the pure-decode reference shape on
> the registered hardware class.** Mispricing must only ever under-pay.

Prefill is batched and an order of magnitude cheaper per token than decode, so its weight starts
heavily discounted (initial: `cu_prefill_weight = 1`, `cu_decode_weight = 64`). The honest
consequence, stated rather than hidden: a prompt-heavy job earns somewhat less CU per second of
real compute than a decode-heavy one, and a dedicated miner running decode-heavy garbage prompts
earns CU at the reference rate. **Usefulness is not adjudicable and this ADR does not pretend to
adjudicate it** — the guarantee is the draft's own honest one: real usage mines at (nearly) the
dedicated-mining rate, so the useful and the mercenary pay the same protocol costs, instead of
useful work being worthless. Chat-shaped usage is decode-dominant, so the discount is small in
practice.

Variable length is real: `decode_token_limit` is a ceiling, `EndOfGeneration` is a legitimate
stop (a chat answer that ends, ends), `decode_tokens_executed` is what the trace commits and what
CU counts, and a replay must reproduce the same EOG step or the trace is refuted. This is a new
wire version and a new shape/class identity (`…/early-eog-allowed/…`) — the V2 exact-decode
profile is not edited in place (a second meaning under one id is the fork-bug shape).

Spam floor: tiny jobs are bounded by the commitment transaction's fee and by quantization itself
(`cu < quantum_cu` certifies but never draws). No minimum-prompt rule pretends to filter "real"
usage.

## Decision 8 — Data availability and privacy, v1

| Mode | Prompt tokens | Weight-bearing |
|---|---|---|
| `PublicDa` (v1) | carried whole in the commitment transaction (≤ `max_prompt_tokens`, ≤ the existing 4096-token frame budget) | **yes** |
| encrypted / panel-keyed | future ADR | no — zero weight until specified |

PublicDA-on-chain does to the prompt-withholding failure mode what carrying the output token list
whole did to output disputes: it deletes the failure mode instead of adjudicating it. The panel
replays from chain data alone; there is no "producer hid the prompt" arm. The **trace** stays
off-chain under the PR-06 DA obligation trio (manifest root, chunk count, retention deadline) —
withholding a requested opening defaults the producer, as already specified.

The gateway MUST show the user, before first use, that PublicDA prompts are permanently public.
A daily-use tool that silently publishes prompts is a betrayal dressed as a feature.

## Decision 9 — Bundle extension, invariants, and what moves

`PalwConsensusParamsV2` gains one required field:

```rust
pub freeprompt: PalwFreePromptParamsV3 {
    receipt_algorithm_id: u8,          // must equal POW_ALGO_ID_PALW_RECEIPT_V3
    attempt_share_permille: u16,       // the floor; the receipt lane gets the remainder
    quantum_cu: u128,
    pwu_per_quantum: u64,
    cu_prefill_weight: u32,
    cu_decode_weight: u32,
    max_quanta_per_receipt: u32,
    max_prompt_tokens: u32,
    max_decode_tokens: u32,
    receipt_maturity_daa: u64,
    receipt_use_window_daa: u64,
    max_beacon_gap_daa: u64,           // a measured/declared bound, enforced against windows below
}
```

New startup invariants (a node that fails any does not boot, as Decision 1 of ADR-0042):

```
receipt_algorithm_id == POW_ALGO_ID_PALW_RECEIPT_V3
0 < attempt_share_permille < 1000            (a zero floor has no beacons; 1000 has no receipts)
quantum_cu, pwu_per_quantum, cu_decode_weight, max_quanta_per_receipt > 0
max_prompt_tokens ≤ PALW_V2_MAX_PROMPT_TOKENS, and prompt+decode fit the trace-event cap
receipt_use_window_daa > 0
anchor_delay + max_beacon_gap_daa < window_bind     (a late beacon must still bind inside the window)
receipt_maturity_daa ≥ reorg_margin_daa             (a draw beacon deeper than the reorg margin)
```

Per-source retarget: the state's target table becomes keyed by `(class, lane)`. Attempt targets
retarget exactly as PR-09 landed (share of realized production); receipt targets retarget with
expectation `share[class] · (1000 − attempt_share) / 1000 · span blocks`, through the same
`retarget_over_span_v1` fold. Expectation is over **realized production**, so one lane starving
never silently re-prices the other; the floor share is what guarantees the attempt lane's
expectation is nonzero.

The mode's algo demand becomes a set: `{6, 7}` under this bundle (template side chooses per lane;
header side accepts either and then applies that lane's admission). The four wired seam sites
keep their `Option<u8>` shape until the wiring PR swaps them to the set form — every shipped
preset answers `None` before and after, pinned by the existing seam test.

`palw_ruleset_id_v2` moves for any bundle (the struct grew). No shipped preset carries a bundle,
so every pinned preset fingerprint is unchanged — the existing golden-vector tests prove it.

## Decision 10 — The user pipeline: one inference, an answer and a commitment

Off-consensus, but specified here because F1 lives or dies in it:

```
user app ──POST /v1/chat/completions──▶ misaka-palw-gateway
    │  canonical template render (frozen text transform, gateway-side, template id pinned)
    │  canonical tokenize (worker mode, pinned GGUF tokenizer — the class's tokenizer_id)
    ▼
palw-worker v3-job  ──▶  answer (output ids + rendered bytes, returned to the CALLER)
                    └──▶  projection (trace/output/schedule roots, counts)  ──▶ commitment tx
```

- The worker's v2 rule "no rendered output leaves the process" is **amended for the v3 job
  response only**: the response envelope carries the answer to its caller. The *projection* still
  never carries raw text — it is the consensus-compared object and stays hashes-and-counts.
- Tokenization and template rendering are the canonical front end the tree currently lacks. The
  template is a frozen string transform applied gateway-side (its sha pinned in the class
  profile); tokenization is a new worker mode against the pinned GGUF. Both are new identities —
  the v2 class pins `"none/token-ids-input/v2"` and is not edited.
- v1 is non-streaming (one frame in, one frame out, the agent's Phase-A single slot respected;
  the gateway queues). A streaming side channel is *Not decided*.
- The gateway never re-runs the inference for mining. There is no second lane. That property is
  testable: the worker executes once per job id, and the commitment's roots are byte-derived from
  the same execution that produced the returned answer.
- Two properties that must not be conflated (the gateway smoke's first draft did, instructively):
  re-asking the same conversation yields the SAME answer — F1, the fresh `job_nonce` never
  touches the model's input — and a DIFFERENT trace root, because every trace event binds the
  job id (nonce included). The second is the anti-replay binding: one job's trace can never be
  presented as another's. "Same input, same trace" holds per job, never across jobs.
- The v1 template is plain-marker text, deliberately: the pinned tokenizer runs with
  `parse_special = false` (untrusted text must never smuggle control tokens), so a ChatML
  template would tokenize its own markers as prose. Consequence, measured: the model rarely
  emits EOG under it, answers end at the decode ceiling, and the gateway's display-layer stop
  guard trims presentation while the commitment covers every executed token. A ChatML profile
  with segment-wise special tokenization is a future class identity.

## Invariants

```
F1   The user's prompt token sequence is never modified by PALW metadata (no suffix, no wrap).
F2   Canonical token ids, not text, are the execution identity; the tokenizer is the class's.
F3   Model/runtime/tokenizer/template/shape are bound via the registered class row; the job
     carries the class id and the admission cross-checks, never a second copy of the facts.
F4   Panel randomness becomes known only after the commitment is irrevocable on chain.
F5   Ticket randomness becomes known only after certification (Final) is irrevocable on chain.
F6   No randomness derives from prompt, output, trace, receipt id alone, or any executor-chosen field.
F7   CU/PWU is recomputed by every validator from the committed shape under the bundle's rule;
     a mismatched claim is invalid, never clamped.
F8   One quantum is spendable at most once per candidate chain; spends are branch-scoped state.
F9   A claim below Final licenses no block.
F10  A receipt block's weight is safe at acceptance and never revised.
F11  Same genesis · DAG · bodies · evidence ⟹ same receipt state, same weights, same tip.
F12  A full node validates receipt blocks with no model runtime, structurally (algo 7 is not an
     inference-priced algo; the CI dependency edge test already forbids the import).
F13  Per-bond exposure is one ceiling across attempt and free-prompt claims.
F14  An expired draw (outside the use window) licenses nothing, forever.
F15  Only attempt-class (algo 6) chain blocks are randomness anchors — for panels AND tickets.
F16  The attempt floor share is bounded away from zero by a startup invariant.
```

## Threat disposition

| Attack | Closed by |
|---|---|
| Prompt grinding for favorable tickets | F5/F6 — the draw beacon postdates Final; D5 |
| `job_nonce`/identity grinding (Flaw 2) | same — no randomness derives from the id; D5 |
| Anchor/beacon grinding by block producers (Flaw 1) | D4 — beacons are inference-priced; receipt hashes carry no randomness (F15) |
| Panel selection bias toward colluders | D4 + `derive_panel_v2`'s registry-resolved exclusion trio |
| Receipt replay / double-spend | F8 branch-scoped spent set; cross-network by `network_domain`; cross-class/bond by total binding |
| Fake commitment, never served | exposure reserve at commit + panel two-sided quorum (ProducerDefaulted) + DA default |
| Fabricated trace | court: proof-carrying arithmetic refutation against the committed root |
| PWU inflation via workload shape | D7 conservative pricing invariant + quantization + per-class epoch budgets (existing) |
| Tiny-job spam | commitment tx fees + sub-quantum certification draws nothing |
| Stockpiled wins injected later | F14 use window |
| Receipt pipeline outage → chain halt | D1 attempt floor |
| Weight revision as reorg lever | F10 — nothing to revise |
| Bond leverage across lanes | F13 shared exposure ceiling |

## Implementation ladder

Substrate first, consensus-inert, exactly as PR-01…PR-10 were staged; no shipped preset moves at
any point before a dedicated RC/devnet preset PR.

| PR | Content | Done when |
|---|---|---|
| **FP-00** | this ADR | — |
| **FP-01** | `palw_freeprompt_v3.rs`: job/commitment/spend objects, total-binding ids, CU rule, quanta, tickets, beacon fact — golden vectors + per-field mutation tests | mutating any field moves the id; domains separate from every V2 tag |
| **FP-02** | algo id 7 registered, demanded by nothing; mode-level `allowed_algo_ids` (unwired) | every preset's accepted set unchanged, pinned |
| **FP-03** | state machine: free-prompt claims through the lattice, shared exposure, spent sets, per-lane targets + retarget, sweeps | delta replay/revert differentials green; equal-DAG differential green |
| **FP-04** | receipt-block admission (stateless + stateful) + spend header carriage | every admission item has a named red test |
| **FP-05** | bundle extension + startup invariants + ruleset id | invariant-refusal table extended; preset fingerprints byte-stable |
| **FP-06** | worker v3: tokenize mode, v3-job (EOG stop, answer in response), new shape/profile ids | one execution yields answer + projection; v2 golden gate untouched |
| **FP-07** | `misaka-palw-gateway`: /v1/chat/completions → template → tokenize → job → answer + commitment emission | F1 test: bytes to the model == canonical tokens of the user request, nothing else |
| **FP-08** | pipeline wiring: objects from carriage, beacon facts from stores, seam swap to the two-id set, trace DA retention + the signer/executor rail, store layer (shared with V2's own pending wiring) | devnet blocks of both kinds validate cross-node |
| **FP-09** | fleet drill + measured params + RC/devnet preset | soak outputs fill the bundle |

This branch lands **FP-00 through FP-09**, with FP-08's pipeline wiring staged as five atomic
units (A–E) recorded in `docs/palw-fp-wiring-atomicity.md`:

| Unit | Content |
|---|---|
| **A** | the attempt lane's PoW arm (algo 6) |
| **B** | the receipt lane's arm (algo 7) — no PoW, level 0, ticket-admitted |
| **C** | the store layer, the candidate-scoped walk, the block's own work, objects from accepted txs, beacon facts from the chain |
| **D** | one fork-choice authority at all four sites — virtual sink, deep reorg, pruning ceiling, IBD commit |
| **E** | the pruning point's PALW carriage: capture, the wire pair, and an import gate that refuses what no header commits to |

FP-06/07 are measured on the real pinned model (Qwen3.5-2B): one inference produced the user's
answer AND the commitment roots; the Text arm and the TokenIds replay arm reached byte-identical
roots; repeated runs were byte-identical on every consensus-visible field; the OpenAI-compatible
surface answered with the roots and the CU in-band and the artifact in the outbox.

FP-09's drill (`docs/palw-fp-fleet-drill.md`) carries a transaction the rail really built across
into the consensus extractor and state machine — the one boundary nothing else in the tree
crosses — and measures the CU weights on both backends (CPU 8.0 : 1, Metal 11.9 : 1), which is
what lets the shipped 1 : 64 be stated as a bound rather than a guess. It reaches a **Provisional**
claim and says so: certification needs the panel's overlay rounds on more than one node, and only
a `Final` claim can be spent by a receipt block. `WORST_CASE_COURT` remains declared, not
measured, and is labelled as such in the source.

## What this ADR does not decide

- **Measured parameters** — shares, quantum size, CU weights, windows are soak/calibration
  outputs, like every other number in the bundle.
- **Streaming** — the side-channel token stream for the gateway (the commitment still only exists
  at completion; the stream is UX, not consensus).
- **Private eligibility** — hiding the winner until use (per-bond precommitted secret Merkle
  trees over round indices: secrets fixed at bond registration, revealed in the spending block,
  verified against the bond's committed root; grinding at registration is aimless because neither
  beacons nor claim ids exist yet). v1 accepts public draws.
- **Encrypted prompts** — `PublicDa` is the only weight-bearing mode; ML-KEM panel-keyed DA and
  ZK forms are their own ADR, zero-weight until then.
- **KV-cache / multi-turn continuation** (`ContinueFrom{parent_receipt}`) — priced continuation
  without double-counting cached prefill; needs its own trace semantics.
- **Receipt delegation/transfer** — receipts stay bound to the executing bond; pooling happens at
  the gateway/bond layer.

## Number hygiene

This is ADR-0044; 0043 (`palw-v2` state-root ordering) is the last committed on the base branch.
If a concurrent branch also claims 0044, the tie breaks by content-keep-and-renumber-the-later
writer, per ADR-0036 Decision 5.
