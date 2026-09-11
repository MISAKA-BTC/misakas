# ADR-0103 — The context is held off the chain, and the chain carries a root, an opening and a logarithm

* Status: PROPOSED 2026-09-11 on `docs/adr-0103-the-context-is-held-off-the-chain` (from
  `feat/adr-0099-sharded-seat` at `3e9c3b59`); **IMPLEMENTED 2026-09-11 on
  `feat/adr-0103-held-context`** (§10: what each Decision became, the generator's numbers, ten
  corrections to the text below, and what is not done). Nothing is armed: the fence
  (`Params::palw_held_context`) is `None` on every shipped preset and Some-only in the fingerprint,
  and a network that wants what this ADR describes MINTS with it (ADR-0092 Decision 4;
  `palw_held_context_mint_v1`). Testnet-11 does not move. Every number in §1 is one the tree's generators already printed or an ADR already
  recorded, cited by section; every number in §3 that is not is labelled *arithmetic* and is the
  generator's to print before it is normative (ADR-0092 §5).
* Builds on: [0097](0097-a-models-fit-is-a-lookup-and-the-entrance-says-its-limits-before-the-first-token.md)
  (the nine walls and their homes; §1.4: 2M is refused past eight layers; Decision 5's table, whose
  rows this ADR answers one by one), [0099](0099-the-adder-measures-the-chain-recomputes-and-a-seat-holds-a-shard.md)
  (a seat holds a shard; §1.2's two transfer forms, of which the *resume* form is Decision 2 here),
  [0100](0100-a-model-is-data-and-the-court-the-measure-and-the-licence-are-built-for-a-shard.md)
  (the one-move court is a consensus object and convicts on a live devnet; licensing per shard),
  [0082](0082-the-close-is-flat-in-the-context.md) (R4; Decision 2's dissection; Decision 3's
  derived arity; Decision 4 as amended — a checkpoint at every position; Decision 7's fold;
  Decision 9's seat; §10.3's quadratic term; §8's two refusals this ADR reverses by name),
  [0092](0092-the-ladder-is-minted-once-and-the-clock-is-what-binds.md) (the ladder is in the
  identity; the clock binds; a wider model is a new mint), [0086](0086-the-opening-carries-the-fold-not-the-leaves.md)
  (an opening is the fold's digests and a frontier; the court's address is a block, then a leaf),
  [0098](0098-the-panels-coverage-is-a-number-and-a-seat-that-found-a-lie-files-nothing-else.md)
  (the coverage is a generated number; a seat that found a lie files the one thing),
  [0062](0062-data-availability-court.md) (an accusation names what is missing; a disclosure is
  hash-checked and bounded), [0081](0081-long-context-the-input-is-a-state-chain.md) Decision 3 /
  [0077](0077-a-prompt-a-person-would-type-is-a-claim-the-court-can-try.md) Decisions 8 and 16
  (the tiled ids; the sampled interval; `PanelDa`), [0093](0093-the-court-can-try-a-fused-row-and-the-responder-is-what-is-missing.md)
  (the responder a fused site still needs), [0102](0102-the-embedding-lift-is-read-per-token-and-an-unarmed-kernel-is-not-in-the-identity.md)
  (a fenced kernel stays out of the identity — the shape every new thing here takes), and the
  close-cut ADR authored as 0102 on `feat/adr-0096-partb-drill` (§9: it renumbers) — a close too
  wide for one carrier is cut once, which is what carries a one-move court's object when it is wide.
* Amends: ADR-0082 R4, Decisions 3 and 9 and §8 (for a class under this fence only — §7);
  ADR-0092 Decision 1 (what a ladder is minted against, once it costs no rounds); ADR-0097
  Decision 3 (the geometry ceiling's form); ADR-0062 Decision 1 (what an accusation may name);
  ADR-0099 §1.2 (the resume form is chosen, for wide classes, by derivation). Supersedes nothing.

## 0. The sentence this ADR is

**A 2M context is not refused by one number. It is refused by six terms that grow with the
context, and every one of them is a term the chain carries, walks or waits on for something the
executor or a seat could hold instead.** ADR-0097 named the walls and ADR-0099 built the seat that
holds a shard of the model; what neither moved is that the *chain's own work* on a claim — the
ladder a court walks, the window a court waits, the chunk count a leg enumerates, the ids a
commitment carries, the product a registration validates, and the prefix a seat recomputes — is
linear in the context. This ADR moves each of them off the chain: a court that opens at a named
leaf and walks no ladder (D1), a seat that resumes from committed state it fetches rather than
recomputing the prompt (D2), a state map whose checkpoint is an append (D3), ids that never ride
(D4), a dissection that starts where the accusation points (D5), and a registration gate that costs
one position (D6). What the chain carries per claim becomes **a root, an opening and a logarithm** —
constant or `O(log C)` in the context `C` — and what grows with `C` is **held**: by the executor for
the claim's life, and by a seat as its shard's state, each bounded by a budget the plan prices and a
window the certification drill enforces (D7). The fit stays a lookup: the generator prints every
wall's *order* beside its number, and a class whose chain wall is linear is refused by name (D8).

The operator's framing (2026-09-11), adopted whole:

> 2M contextそのものが無理というより、「2M contextを今のPALWの証明・裁判・DA・搬送ルールのまま
> チェーンに載せると、帯域と時間の壁にぶつかる」… 巨大な状態・prompt・KV → off-chain / seat / shard
> 側に保持 → chainには commitment / root / 必要なopeningだけ … 2Mでもchain上の負荷はほぼ一定〜
> 対数的に近づける方向が正しいです。

That is the requirement (§2), and the six terms are §1.

## 1. What was measured

Read on `feat/adr-0099-sharded-seat` at `3e9c3b59` on 2026-09-11. Nothing was re-run for this
ADR: every figure is what `palw-model-fit` printed on 2026-09-10
(`docs/palw-model-fit-testnet11-2026-09-10.md`, ADR-0097 §1), what ADR-0082 §10 and ADR-0086 §1/§7
measured, or what ADR-0098 §1.2 generated — each cited where it is used. The RC's own constants:
ladder `2^26`, close `2,250,000` bytes / `27` carriers, turn deadline `42` DAA, terminal rounds
`2`, `window_court` `3,000`, `window_receipt` `600`, standard transaction `120,000` bytes
(`PALW_RC_WINDOWS_V1`, `palw_fp_devnet_v3.rs`; ADR-0097 §1.1).

### 1.1 Six terms, and the order each one grows with the context

`C` is the class's `n_ctx`; `L` its layer count; the *order* column is the order of the chain's own
term in `C`, read off the predicate that computes it.

| term | the chain carries / walks / waits on | order in `C` | where it lives | why it is linear |
|---|---|---|---|---|
| the ladder | `max_step_leaf_count ≥` the whole context as prefill, in leaves — `C × leaves_per_position` (103,008 a prefill position on dense graph-v5, ADR-0086 §1) | **linear** in leaves, so `⌈log₂⌉` rounds, each two clocked moves | `PalwCourtParamsV2`, inside `palw_ruleset_id_v2` | the bisection court (`PalwBisectLadderV1`) walks the JOB's leaf space to find one leaf; `2^26` is the widest the RC's window prosecutes at arity 2 with no headroom (ADR-0092 §8) |
| the court window | `(2 × (⌈log₂ ladder⌉ + ⌈log_k(C / 16)⌉) + terminal + 1) × 42 + 216 < 3,000` (`worst_case_duration_with_history_daa`, `palw_court_arity_v1`) | **logarithmic** in `C` on the history term — but the LADDER term is the leaf count's logarithm and the leaf count is linear in `C`, and every round is 84 DAA of wall clock | the lattice windows, the same id | the ladder's rounds are counted before the dissection's (ADR-0082 §10.4: the RC row is 65 moves, 26 of them the ladder's) |
| the state chunks | `L_attn × 2 × ⌈C / 16⌉ ≤ 65,536` (`tiled_kv_state_geometry_v3`) | **linear** — a count bound | `PALW_STEP_LEG_MAX_STATE_CHUNKS`, a code constant the checkpoint leg enforces | the v3 map indexes `(kind·L + layer)·chunks_per_slice + block`, so a slice that grows a block MOVES every later index and a per-position checkpoint re-hashes what did not change: 696,516,608 bytes serialised a job at 512 (ADR-0082 §10.3), a term **quadratic** in `C` |
| the public-DA payload | `C × 4 ≤ 120,000` under `PublicDa` | **linear** | `PALW_STANDARD_TX_BYTES` | the ids ride the commitment transaction (ADR-0077 D16: under `PanelDa` they do not) |
| the geometry ceiling | `C × L ≤ 2^24` (`validate_geometry`, `PALW_STEP_MAX_ENUMERATION`) | **linear** — a product | `palw_step.rs`, a code constant that gates every `ClassRegistered` | it bounds two walks that still visit every position — `step_leaf_count_capped_v1` and `canonical_step_coordinates`, "their driver is the CONTEXT's `declared_prefill_tokens`" — after `worst_case_step_leaf_count_capped_v1` was made a closed form (`palw_step.rs`'s own doc at the constant) |
| the seat (not a wall) | the prefix a seat recomputes before its first drawn interval, `C` positions (ADR-0082 D9); the draw's `N` counts DECODE calls, `max(1, ⌈(D − 1) / interval⌉)` (ADR-0098 §1.1), and interval 0 is the prefill whole (ADR-0086 §1) | **linear** compute per seat per claim | ADR-0082 D9; `base0_fp_interval_count_for_v1`; `PALW_FP_SEAT_INTERVAL_SAMPLES_V1 = 4` | "a seat recomputes the cache from the prompt it holds; it never fetches the history" |

Three terms are already what this ADR wants, and are listed so that nobody re-solves them: the
**close** is flat in the context after ADR-0082 (192 bytes for eight times the context, §10.1; the
Merkle prompt-id term is 64 bytes a doubling, ADR-0082 §1.1) and stays so here; the **licence** is
`3 × shards` receipts, constant in `C` (ADR-0100 D4); a **DA disclosure** is one event and its path
(ADR-0062 D3, SA-2). And one term is not the chain's at all and is left where it is: the executor
**retains** the cache for the claim's life — 276 GiB for the K3 stand-in at 2M (ADR-0097 §1.5) —
which is disk, priced by `claim_retirement`, and is what "held" means.

### 1.2 The same six at 2M, in the tree's own predicates

*Arithmetic from §1.1's formulas over the generator's recorded rows; the generator prints the
normative table when D8 exists.* Dense graph-v5 (28 attention layers), and the K3 stand-in of
ADR-0097 §1.3 (`stand_ins::KIMI_K3_AS_HYBRID_V1`, 24 attention + 68 recurrent layers; 3,323,778,104
leaves at 512 → 6,491,754 a position):

| term | dense graph-v5 at `2^21` | K3 stand-in at `2^21` | against |
|---|---|---|---|
| leaves, the ladder's `need` | `2^21 × 103,008 ≈ 2.16 × 10^11`, 38 binary rounds | `≈ 1.36 × 10^13`, 44 rounds | `2^26`, 26 rounds |
| court window at arity 2 | `(38 + 17) × 2 + 3 = 113` moves → `4,746 + 216 = 4,962` DAA | `125` moves → `5,466` DAA | `2,999` |
| court window at arity 64 (the history in 3 rounds) | `(38 + 3) × 2 + 3 = 85` → `3,786` DAA | `(44 + 3) × 2 + 3 = 97` → `4,290` | `2,999` — **the ladder alone overruns; no arity fits** |
| state chunks, v3 map | `28 × 2 × 2^17 = 7,340,032` | `24 × 2 × 2^17 = 6,291,456` | `65,536` |
| public-DA payload | `8,388,608` B | the same | `120,000` |
| geometry | `58,720,256` (the generator's row) | `192,937,984` (the generator's row) | `16,777,216` |
| the seat's recompute | `2^21 × 0.0938 s` (ADR-0082 D7's un-captured forward, the real dense artifact) `≈ 54.6 h`; at the a16 host's ≈ 1 s a token (ADR-0097 §1.5) `≈ 24 days` | the adder's rate (ADR-0099 D6; a K3 has none measured) | `window_receipt = 600` DAA ≈ 20 h at testnet-11's 120-second cadence (ADR-0082 §1.6) |

Two readings of that table, both true: the ladder and the window are one term (ADR-0092 §3: leaves
are cheap, rounds are not, and the leaf count is what makes the rounds), and the seat is the only
term with no ruleset number in front of it — which is why ADR-0097 Decision 5 put the seat first.
The ceiling's refusal of 2M "past eight layers" (ADR-0097 §1.4) is the first wall met, and the one
that means the least: raise it alone and the next five refuse in the order above.

### 1.3 What the tree already holds for each term

Checked against the tree rather than against the ADRs, in ADR-0092 §2's spirit — an ADR that asks
for what the code already does is the most expensive kind.

| this ADR needs | where it already is |
|---|---|
| a court that convicts at a named leaf with no search | `ShardCourtAccused` (ADR-0100 D1): acceptance arm, fold arm, signing context in the V3 set; **convicts on a live devnet** (ADR-0100 §9, run 2: leaf 0, `void_and_slash`, −38,520 sompi, no human step). Refuses a fused leaf `NeedsDissection` (`palw_shard_court_v1.rs`) |
| a checkpoint at every position, prefill included | ADR-0082 Decision 4 as amended: `PalwCheckpointCadenceV1::PerPosition` on every tiled-map class, read off `state_chunk_map_id` and therefore inside the class id |
| an opening that is the fold's digests, never the leaves | ADR-0086 D1–D3: `Base0FpIntervalOpeningV4`; the seat's leaves are its own; a fault has a block address; the block-leaves lane names the leaf (D6) |
| a resume opening a shard seat can start from, priced | ADR-0099 §1.2 / D2: the two transfer forms, "both committed material, so both verifiable"; the plan prints both and chooses neither |
| the seat's draw as a pure function of chain facts | `palw_fp_interval_draw_v1` (ADR-0077 D8), `k = 4`, over `N` |
| the prompt ids as a tiled root | `palw_prompt_ids_v1`, trace format 4 (`PALW_V2_TRACE_FORMAT_VERSION_MERKLE_IDS`), `Params::palw_prompt_ids_form_v1()` genesis-only; armed on the carded mainnet, testnet-11 keeps `Flat` (ADR-0081's 2026-09-06 note) |
| a commitment that carries no ids | `PanelDa` (ADR-0077 D16), `Params::palw_panel_da`, armed on the card only |
| a DA court whose disclosure is one named unit | ADR-0062 D1–D5 behind `Params::palw_da_court`, armed on testnet-11 (its 2026-09-06 rollout record) |
| a registration gate that costs no `n_ctx` factor | `worst_case_step_leaf_count_capped_v1` — closed form, `O(nodes)`, pinned by `a_sparse_leaf_profile_costs_the_same_at_every_context`; its two siblings are named there as still walking |
| an arity derived from the window and the carrier | `palw_court_arity_v1` (ADR-0092 D3) — with the ladder's rounds inside its divisor |
| a close wider than one carrier, filed by the node | the close-cut ADR on `feat/adr-0096-partb-drill` (authored as 0102, §9 here): the cut moves to consensus-core and the node's court files parts |
| the walls as a lookup, per wall, with `need` / `have` | `palw_model_fit_v1` (ADR-0097 D1) — without an order column |

So most of this ADR is re-plumbing shipped parts so that the chain's term is the logarithm and the
linear term is held. The parts that are new are: a state map whose index does not move (D3), a
seat whose sampling unit covers the prefill and whose starting state is fetched (D2), an output-id
form (D4), a DA accusation that names a tile or a chunk (D4), the fit's order column and its gate
(D8), and one fence.

## 2. The requirement

> **R-held — for a class under this ADR's fence, everything the chain carries, checks or waits on
> per claim is bounded by a constant or by `log C`: the commitment, the licence, each court move
> and the number of moves, a DA disclosure, and the work a validating node does on a registration.
> What grows with `C` is held off the chain — by the executor for the claim's life, and by a seat as
> its shard's state — and each held term is bounded by a budget the plan prices and a window the
> certification drill enforces. The fit stays a lookup: the generator prints every wall's order,
> and a class whose chain wall is linear is refused by name.**

ADR-0082 R4 stands inside it for the court and the executor's commitments; where R4 said a SEAT
fetches nothing that grows with the context, R-held says a seat fetches its shard's state and the
CHAIN carries nothing that grows with the context — the seat's fetch is the one linear term this
ADR keeps, and §3 Decision 2 says why it is the honest one.

```text
   a 2,097,152-token prompt, one claim              what the chain holds for it
   ──────────────────────────────────               ────────────────────────────
   the executor: the cache (276 GiB on a K3),        roots: step, checkpoint, prompt ids,
     retained for the claim's life; a checkpoint       output ids, output — CONSTANT
     root that is an APPEND each position (D3)      licence: 3 × shards receipts — CONSTANT
   a seat: its shard's state at the interval's      a court: one named leaf, its openings,
     start, FETCHED and verified (D2, D7);            ⌈log₂ leaves⌉ × 64 B of path — LOG
     P positions replayed; a receipt                a fused leaf: 1 + 2⌈log_k(C/16)⌉ moves,
   any holder: the ids, served under the root         each one carrier — LOG
     the chain names (D4)                           DA: one tile / chunk / block + path — LOG
                                                    registration: O(nodes) — CONSTANT
```

## 3. Decisions

**Decision 1 — the court opens at a named leaf, and the leaf ladder is walked by nobody.** Under
the fence, a claim's arithmetic is disputed only by an accusation that NAMES what is wrong and
carries what refutes it, adjudicated by the chain in one move. Two kinds: `ShardCourtAccused`
(ADR-0100 D1, unchanged) for a step leaf — the committed inputs opened against the step root, the
artifact openings against the class root, the terminal check (`check_execution_step_refutation_capped_v1`)
run by the chain; and its sibling `CheckpointAccused` for a checkpoint chunk that is not the
composition the class's map defines over the committed rows it covers — the chunk opened against
the checkpoint root, the rows against the step root (the `KCacheWrite` / `VCacheWrite` roles
ADR-0082 §10.2 bound), the composition recomputed by the chain and compared byte for byte. The
bisection court over the whole step space (`CourtOpened` on `PalwBisectLadderV1`) is refused by
name for a class under the fence, so the ladder's ROUNDS leave the clock. `max_step_leaf_count`
stays inside the ruleset id as what it also always was — the depth that prices a Merkle path, 64
bytes a level — and **ADR-0092 Decision 1 is amended for such a network: the ladder is minted at
the top of the CARRIER's budget, not the clock's**, because it costs no rounds; a `2^48` ladder is
3,072 bytes of path in a close and no wall clock anywhere. Who names the leaf is unchanged: a seat
that replayed it (ADR-0098 D2/D3, and the block-leaves lane of ADR-0086 D6 turns a block address
into a leaf off the chain), or any Active bond above the floor that is not the claim's own
(ADR-0062 SA-1); a false accusation costs `min(claim.reserved, floor)` (ADR-0100 D1). A fused leaf
is Decision 5's. What the accused is asked: nothing — the roots are theirs, the class's artifact is
the class's, the arithmetic is the court's — which is why there is no responder and no clock at any
leaf but the fused one.

**Decision 2 — the seat's unit is an interval of positions over the whole job, prefill included,
and a seat resumes from committed state it fetches.** Stated as the derivations they are:

* **The interval.** `N = ⌈(prefill + decode) / P⌉`, where `P` is the class's
  `seat_interval_positions`, DERIVED at certification: the largest power of two for which, on the
  slowest certified seat, `fetch(C) + replay(P positions at the last interval)` fits
  `window_receipt` with the drill's stated margin — ADR-0082 D9's derivation with the fetch term
  added, enforced where seats are measured (ADR-0075 D7's drill; `palw-certify`'s route), recorded
  on the certification beside the replay floor. The draw is `palw_fp_interval_draw_v1` over `N`,
  unchanged; interval 0 is `P` positions and never the whole prefill.
* **The route.** Derived from the class, never chosen by a seat: `Recompute` (today's route, ADR-0082
  D9) where `C × rate_slowest ≤ window_receipt`; `Resume` otherwise. Under `Resume` a seat asks the
  executor — or any holder, ADR-0101's serving rule — for the resume opening of ADR-0099 D2: the
  checkpoint chunks of ITS SHARD's layers at the interval's start under the checkpoint root (a
  recurrent layer's state; an attention layer's K and V tiles over `0..p`), verifies every chunk
  against the root (D3 makes that a shard-local check), replays its `P` positions with its shard's
  kernels from the previous shard's committed rows (ADR-0099 §1.1: a shard's input is another
  shard's committed output, opened against the step root; shard 0 from the ids it holds), and
  compares every committed row of its shard, the checkpoint root at the interval's end and — for
  the interval that selects tokens — the ids. `Valid`, `Incapable`, a fault at an address (ADR-0098
  D3), or nothing, as today. A seat's fetched bytes are `O(C)` — the shard's state at the
  interval's start — and that is the one linear term this ADR keeps, off the chain, bounded by
  the shard budget the plan prices (D7) and by `window_receipt × bandwidth`.
* **What this reverses, said plainly.** ADR-0082 R4 ("nothing a seat fetches grows with the
  context except through a logarithm"), D9 ("it never fetches the history") and §8 ("a fleet mode in
  which seats FETCH the cache — refused as a protocol default") were written for a seat that could
  recompute 131,072 positions in hours. At 2M there is no window that holds a recompute (§1.2:
  54.6 h on the fastest measured dense path against a 20-hour window), so the bounded seat is the
  one that fetches its shard's state; keeping R4's sentence would keep the class off every network
  that can hold it. R-held keeps R4's *substance* — the chain carries nothing linear — and moves the
  linear term to where a budget can bound it.
* **What this is not.** ADR-0081 §3's prefill-segment chain, still withdrawn (ADR-0082 §7). The
  executor's job is one context and one claim; nothing about the model's view, the leaf enumeration
  or the checkpoint leg changes — a tiled-map class already checkpoints every position. What
  changes is the SEAT's sampling unit and where its starting state comes from.
* **Coverage** (ADR-0098 D1) is computed over the new `N` and printed; ADR-0098 D4 stands — the
  deterrent is coverage × `claim.reserved`, and `k` is a mint-time number. Decision 9 prices it.

**Decision 3 — state chunk map v4: the index is append-only, the checkpoint root is a frontier, and
the chunk cap is a depth.** `state_chunk_leaf_hash_v1(map_id, index, bytes)` binds the index, and
the v3 tiled map's index moves when a slice grows (§1.1) — that is the quadratic term, and no
constant raises it away. v4 (`PALW_TILED_KV_STATE_CHUNK_MAP_NAME_V4`, `tiled_kv_state_geometry_v4`)
is two levels: the checkpoint root is a Merkle root over the SLICES' sub-roots (one slice per
`(kind, layer)`, `L_attn × 2` of them, plus a recurrent layer's state slices), and each sub-root is
the frontier of that slice's blocks in position order — the leaf binds `(map_id, slice, block,
bytes)`. A block appended to one slice moves no other index; the open tile (the last, partly filled
block of every slice) is one leaf replaced per position until it closes. Hashing per position is
`O(slices × depth)`, the bytes hashed per job are `O(C × row × slices)` — linear, once — and the
executor retains a frontier per slice beside the cache it already retains (ADR-0082 D4: the cache
is prefix-stable). Three consequences: a shard seat verifies its fetch against its own slices'
sub-roots and `log(slices)` siblings (D2's shard-local check); the court's bottom opening (ADR-0082
D2) is one tile with `log(blocks) + log(slices)` of path, the same order as v3's;
`PALW_STEP_LEG_MAX_STATE_CHUNKS` (a count, `65,536`) is replaced for v4 classes by
`PALW_STEP_LEG_MAX_STATE_DEPTH` (a path bound), the count check in `tiled_kv_state_geometry_v3`
becomes a depth check, and every reader that materialises a chunk list becomes closed-form
addressing over `(slice, block)` — as `worst_case_step_leaf_count_capped_v1` was made for leaves.
A class is its map (`state_chunk_map_id` is in the class id), so v4 is a new row per family —
graph-v7, from graph-v5 for the dense lineage and from ADR-0102's graph-v6 for the hybrid — and no
shipped row moves.

**Decision 4 — the ids never ride above one standard transaction, and a DA accusation names a
tile.** A class under the fence is admitted only in the forms whose chain term is logarithmic:
the prompt ids as the tiled Merkle root (trace format 4; ADR-0081 D3 / ADR-0082 D5 — the fence's
"until its inputs exist" condition is met by the class itself), the OUTPUT ids as the same tiled
root — ADR-0082 U-07b, taken here: `PalwFpOutputIdsFormV1::Merkle`, so the decode pin's flat id
list becomes a root and the refutation of a gather at a generated position opens one tile — and
`PanelDa` (ADR-0077 D16): the commitment carries no ids, `PublicDa` is refused by name where
`C × 4 > PALW_STANDARD_TX_BYTES`. **Public is a serving fact, not a carriage fact:** where a
network wants its prompts public, the ids are SERVED under the root the chain names, by the
executor and by any holder (ADR-0101's rule), and misakascan reads them from a holder as it reads
material today; where it does not, they are the panel's. The DA court generalises ADR-0062 D1's
"name what is missing": `ProducerDefaulted.missing` becomes `PalwMissingV1::{ TraceEvent(index) |
PromptIdsTile(index) | OutputIdsTile(index) | StateChunk { checkpoint, slice, block } |
StepBlock(block) }`; the disclosure (D3 there) is the named unit with its path — a tile of ids is
`tile × 4` bytes, a state chunk at most `PALW_STEP_LEG_MAX_STATE_CHUNK_BYTES`, a block of leaves at
most `4,096 × 64` — each inside one carrier by construction; D2, D4 and D5 of ADR-0062 apply
unchanged, and SA-2's "checked by hash arithmetic, never by execution" is kept exactly.

**Decision 5 — a fused leaf's dissection opens at the named leaf, and the arity is derived with
the ladder term at zero.** `ShardCourtAccused` at an `AttnFused` leaf no longer answers
`NeedsDissection`: under the fence it OPENS the dissection session of ADR-0082 D2 at that leaf — the
accusation is the challenge, the responder's root claim is the first clocked move — so a session's
moves are `1 + 2 × ⌈log_k(p / 16)⌉ + terminal`, and `palw_court_arity_v1` and
`palw_attn_court_admits_row_v1` take `ladder = 0` for a class under the fence (the leaf ladder that
term counted is Decision 1's, and it is not played). The responder is ADR-0093's, unchanged and
still unbuilt (*2026-09-11: built before this branch, `c9be7676`, and drilled on the held rows —
§10.3 item 9*): a family with no `attn_tile_claim` is refused under the fence exactly as
`FusedAttentionNeedsTheKaryCourt` refuses today, and ADR-0093 D2's narrowing of the mercy arm rides
this activation. The carrier half of the pair rule is unchanged (ADR-0082 D3: `k × (4 + 8 + 8 ×
lanes)` inside one carrier at the widest registered tile; at a 128-lane head every legal arity
fits, ADR-0092 §8). *Arithmetic*, at `p = 2^21`, tile 16 (`2^17` tiles), the RC's 42-DAA clock and
216-DAA reserve, terminal 2:

| arity `k` | history rounds `⌈log_k 2^17⌉` | moves | DAA | fits `2,999`? |
|---|---|---|---|---|
| 2 | 17 | 37 | `1,554 + 216 = 1,770` | yes — and it is what the derivation selects (the smallest that fits) |
| 4 | 9 | 21 | `1,098` | yes |
| 16 | 5 | 13 | `762` | yes |
| 64 | 3 | 9 | `594` | yes |

With the ladder's 38–44 rounds gone, the RC's own window holds a dissection at 2M at arity 2; the
generator's table (D8) is the normative one.

**Decision 6 — the registration gate costs one position, and the geometry ceiling is a
per-position budget.** `step_leaf_count_capped_v1` and `canonical_step_coordinates` become closed
forms over `PalwLeafShapeV1`, as `worst_case_step_leaf_count_capped_v1` was made: the enumeration
is position-major with at most three position shapes (a prefill position, the position that selects
a token, a decode call — ADR-0086 §1's 103,008 / 122,024), so a leaf index resolves to
`(position, node_slot)` by one division and a table of at most `PALW_STEP_MAX_NODES_PER_TABLE ×
layer_count` prefix sums, and a job's count is three multiplications. `PALW_STEP_MAX_ENUMERATION`
— a product with `n_ctx` whose whole stated reason was those walks — is replaced for a class under
the fence by two bounds with no `C` in them: `layer_count ≤ PALW_STEP_MAX_LAYERS` and
`nodes_per_position ≤ PALW_STEP_MAX_NODES_PER_POSITION`; `n_ctx` is bounded by the ladder's depth
(leaves `≤ 2^depth`) and by its own `u32`. It gates `ClassRegistered`, so it is a ruleset move in
ADR-0097 D3's shape and lives under the fence. `a_sparse_leaf_profile_costs_the_same_at_every_context`
is extended to both siblings and to the gate as a whole (§5, invariant 5).

**Decision 7 — a seat holds a shard of the model and of its state, and the plan prices the
fetch.** ADR-0099 D2's plan gains the per-interval fetch as a column: for shard `i` at position `p`,
`Σ_{attention layers in i} 2 × p × kv_row + Σ_{recurrent layers in i} state` — worst at the last
interval, `O(C)` — and `palw_shard_plan_for_seat_v1` takes a bandwidth and the window beside the
seat budget, so "the fewest shards a seat can hold" becomes "the fewest shards a seat can hold AND
resume inside `window_receipt`". The certification drill certifies a width only when the slowest
certified seat resumes its widest interval inside the window (ADR-0075 D7; ADR-0099 U-02/U-03 are
its units), and the number is on the certification. Shard 0 resumes from the ids and fetches
nothing (ADR-0099 §1.2), which is the argument for placing the embedding and the first layers on the
widest seat. `Incapable` stays the honest verdict for a seat that cannot (ADR-0082 D9).

**Decision 8 — every wall prints its order, and R-held is a predicate.** `palw_model_fit_v1`
gains `order: Constant | Logarithmic | Linear | Unpriced` on every row, classified from `need` at
`C`, `2C` and `4C` by the second difference — an unchanged `need` is constant, a constant first
difference is logarithmic, a doubling one is linear — and prints beside the chain walls the HELD
terms (the executor's retention, the seat's fetch, the seat's replay) with their orders and the
budget each is checked against, so "constant on the chain, linear where it is held" is a printed
table and not a sentence. `verify_class_admission_v8` (v7 with this fence) refuses a class under
the fence whose chain wall is `Linear` — `LinearInTheContext { wall }` — before any number is
compared. §1.1's table is pinned as the limitations it lists by
`consensus/core/tests/palw_adr0103_held_context.rs`, ADR-0097's discipline: the day one moves, a
test says so.

**Decision 9 — what earns and what is at stake do not change, and the deterrent at 2M is priced
before it is armed.** ADR-0074 D5 (a quantum is leaves), ADR-0082 D10 (prefill priced at zero on
the free-prompt lane; decode leaves earn) and ADR-0080 D3's invariance are untouched: a claim's
reservation is its decode leaves' whatever its prompt, so nothing here raises a bond with the
context. ADR-0098 D4's deterrent — coverage × `claim.reserved` — is therefore the number a network
under this fence reads at mint: with `N ≈ C / P`, the row-replay coverage `1 − ((N − k) / N)^s`
falls as `s·k / N`, so a fixed `k` buys a vanishing catch on a 2M prompt; either `k` or `s` scales
with `N` (`s × k × P` is the panel's whole replay, linear in `C` and spread over seats, each bounded
by its window), or the bonded watchdog (`--palw-challenge`, certain at one inference a claim) is the
deterrent and its cost at 2M is that inference's. The generator prints ADR-0098 D1's inverse table
at 2M; this ADR chooses no number.

## 4. What this costs, stated before it is measured

* **Chain, per claim under the fence:** the commitment — roots, constant; the licence — `3 ×
  shards` receipts, constant in `C`; a court — one carrier-bounded object (cut once when wider,
  the close-cut ADR) with `⌈log₂ leaves⌉ × 64` bytes of path, or a dissection of
  `⌈log_k(C / 16)⌉` rounds each inside one carrier; DA — one tile, chunk or block with its path.
  Per registration, `O(nodes)`. Nothing per claim grows faster than a logarithm.
* **Executor:** the cache for the claim's life (`claim_retirement`, 3,000 DAA on the RC), linear
  and held — 276 GiB for the K3 stand-in at 2M, 112 GiB for the dense 1.5B (ADR-0097 §1.5);
  hashing `O(slices × depth)` a position; serving resume openings whose size is the shard's state,
  bounded by the shard and not by the job — the interval lane's caps
  (`PALW_INTERVAL_OPENING_MAX_BYTES = 4 MiB`, `PALW_MATERIAL_MAX_BYTES = 16 MiB`) do not hold a
  resume opening and a class under the fence declares its own, read off the plan.
* **Seat:** the fetch plus `P` positions, inside `window_receipt` by construction (D2, D7);
  `Incapable` where it cannot.
* **Identity:** one fence; per family a new row (graph-v7: the v4 map, the Merkle output ids,
  `PanelDa`); the ladder minted as a depth; trace format 4; the DA accusation's new variant and
  `CheckpointAccused` in the committed context set the shard court's V3 precedent already opened.
  A new mint carries all of it (ADR-0092 D4); testnet-11's and devnet's fingerprints are
  byte-identical with the fence declared.

## 5. Invariants the tests must hold

```
1  Under the fence no CourtOpened bisection is accepted for the class; every conviction on it is
   a one-move object or a dissection opened at a named leaf; ADR-0100's devnet drill re-run under
   the fence convicts the same tampered leaf with the same slash.
2  A close's bytes and a DA disclosure's bytes at C = 512, 32,768 and 2^21 differ by at most
   64 bytes per doubling of C (the path term) — swept by the generator under the fence.
3  The court window's need under the fence is (1 + 2·⌈log_k(C/16)⌉ + terminal) × deadline +
   reserve with the arity derived at ladder = 0; at zero history it is terminal × deadline +
   reserve; the RC's window admits the dense row's dissection at 2^21 at the derived arity.
4  A v4 map's checkpoint at p + 1 is the checkpoint at p with one leaf replaced per slice or one
   block appended per slice: hashes per checkpoint ≤ 2 × slices × depth, no index moves, and the
   bytes serialised per job at 512 are ≤ positions × slices × row × 2 (the §10.3 quadratic term
   is gone, asserted as an order against the derived bound).
5  The registration gate visits the same number of nodes at C = 2 and at C = 2^21, on
   step_leaf_count_capped_v1, canonical_step_coordinates and the gate as a whole; a
   ClassRegistered at 2^21 × 93 layers validates in the time one at 512 does.
6  The seat's draw covers the prefill: on a job of (prefill, decode) the interval count is
   ⌈(prefill + decode)/P⌉ and interval 0 is P positions; a seat that resumes from fetched chunks
   and a seat that recomputes reach the same receipt on honest material and the same fault
   address on tampered material; a fetched chunk that does not verify against its slice's
   sub-root is refused by name and never replayed from.
7  The ids never ride: a commitment of a class under the fence carries no ids; a DA accusation
   naming a tile, a chunk or a block is answered by that unit and its path, inside one carrier;
   a PublicDa registration whose flat ids exceed the standard transaction is refused by name.
8  Every wall's order is printed and pinned: §1.1's orders as limitations on the shipped
   presets, and under the fence no chain wall of an admitted class reads Linear.
9  Economics unchanged: quanta, pwu and ticket inputs are identical for the same decode leaves
   at every prompt width (ADR-0082 Z7, restated under the fence); no bond grows with C.
10 The fence is None on every shipped preset, Some-only in the fingerprint, the schedule id and
   the fork id; it refuses arming without palw_kary_court, palw_shard_court and trace format 4
   at or below its height; both shipped fingerprints are byte-identical with it declared.
```

## 6. Order of work

Nothing below is built before the measurement above it exists; steps 1 and 2 move no fingerprint.

1. **Decision 8 first, because it measures everything after it.** The order column, the held
   terms' rows, and §1.1 pinned as limitations. Consensus-inert.
2. **Decision 6's closed forms as a refactor** — identical results, pinned at every context by the
   cost test — with the ceiling's replacement behind the fence. The refactor is inert; the
   ceiling's move is not.
3. **Decision 3's map v4 and Decision 2's seat**, node-side and drillable on the loopback devnet
   with the fold classes: the interval count over positions, the resume opening served and
   verified, the receipt; then the tampered-chunk drill (invariant 6).
4. **Decisions 1 and 5 behind the fence** — `CheckpointAccused`, the bisection refused, the
   accusation at a fused leaf opening the session — on the floor class first (ADR-0093 §6's order:
   its arithmetic is integer end to end), because the fused half waits on the responder.
5. **Decision 4** — the output-id form, `PanelDa` required above the standard transaction, the DA
   accusation's `missing` kinds and their disclosures.
6. **Decision 7's drill:** a devnet class registered under the fence at the widest width this
   Mac can seat (the geometry ceiling is gone; the seat's window is what binds — the drill says
   which width), a claim, a stratified panel resuming from fetched state, a conviction at a named
   prefill leaf.
7. **The mint** (ADR-0092 D4): a K3-class network prices its card with ADR-0097's generator under
   the fence, ADR-0099's plan with D7's column, and ADR-0098's inverse table for Decision 9.

**Done when** a class registered under the fence at a context the shipped geometry ceiling refuses
is admitted with every chain wall printed `Constant` or `Logarithmic`; a claim on it is licensed by
seats that fetched their shard's state and replayed `P` positions each; a deliberately corrupted
prefill leaf on it is convicted by one object; and the same job's quanta equal what the same decode
leaves earn at 512.

## 7. Supersession

| what | this ADR |
|---|---|
| ADR-0082 R4 ("nothing a court carries, a seat fetches, or an executor commits grows with the context except through a logarithm") | kept for the court and the executor's commitments; **amended for the seat** under this fence: a seat fetches its shard's state (linear, held, budgeted) and the CHAIN carries nothing linear (R-held) |
| ADR-0082 Decision 9 (the seat recomputes the cache from the prompt; never fetches the history) | kept as the `Recompute` route where the window holds it; the `Resume` route is derived beside it (D2) |
| ADR-0082 §8 ("a fleet mode in which seats FETCH the cache — refused as a protocol default") | **reversed for a class under the fence**, with §1.2's reason: no window holds a 2M recompute |
| ADR-0082 Decision 3 (the arity derived with the ladder's rounds in the divisor) | the derivation is kept; the ladder term is zero under the fence because the ladder is not played (D5) |
| ADR-0082 Decision 4 as amended (a checkpoint at every position; the v3 tiled map) | the cadence is kept; the map's index scheme is replaced by v4's append-only one (D3), which §10.3 named as the only thing that could remove the quadratic term |
| ADR-0082 U-07b (the output ids as a tiled Merkle root, not decided) | taken (D4) |
| ADR-0092 Decision 1 (the ladder is minted at the top of the wall-clock budget) | amended: a ladder that costs no rounds is minted at the top of the carrier's budget (D1); Decisions 2–4 kept whole — this ADR is the new mint D4 describes |
| ADR-0097 Decision 3 (the geometry ceiling is a wall with a name) | kept on the shipped rulesets; replaced under the fence by a per-position budget (D6) |
| ADR-0097 Decision 5 (what a K3-class network must mint with, wall by wall) | each row answered: the seat (D2, D7), the geometry (D6), the ladder (D1), the window (D5), the close (unchanged; ADR-0093 still owed), the chunks (D3), the public-DA payload (D4), the answer (ADR-0096 D5, unchanged) |
| ADR-0097 §1.4 ("2M is refused past eight layers") | stands on every shipped ruleset; a class under the fence meets no such wall |
| ADR-0099 §1.2 / Decision 2 (two transfer forms, priced, neither chosen) | the resume form is chosen by derivation for a class whose recompute exceeds the window (D2); the plan gains the fetch column (D7) |
| ADR-0100 Decision 1 (the one-move court, for a seat holding a shard) | becomes THE court for a class under the fence (D1), and opens a dissection at a fused leaf (D5) |
| ADR-0098 Decisions 1 and 4 (the coverage is generated; the deterrent is coverage × reserved) | kept; computed over an `N` that now counts prefill intervals (D2), and priced at 2M before a mint (D9) |
| ADR-0062 Decision 1 (an accusation names a trace event) | generalised to a tile, a chunk or a block (D4); D2–D5 and SA-2 unchanged |
| ADR-0077 Decision 8 (the seat verifies one interval), Decision 16 (`PanelDa`) | the interval now counts positions, prefill included; `PanelDa` is required above the standard transaction |
| ADR-0081 §3 (the prompt as a chain of prefill segments) | **still withdrawn** — D2 changes the seat's unit, not the executor's job; nothing here is a segment |
| ADR-0093 (the responder) | unchanged and still the prerequisite of D5; a family without it is refused under the fence as today |

## 8. What is deliberately not decided

* **Any number.** `P`, `k`, `s`, the ladder's depth, the fence's height — each is the generator's
  or the drill's (ADR-0092 §5), and the card's author reads the table.
* **KV continuation across jobs** (ADR-0077 §8, ADR-0082 §8, ADR-0096 D5, ADR-0097 D6). Still
  refused. Named here because D2 moves it: once a seat resumes from a committed checkpoint it
  fetched, a *licensed* claim's final checkpoint is the same kind of material as any interval's
  start, and what remains against a continuation job is ADR-0072's unit (one inference, one ticket)
  and the licence dependency, not the seat. A 2M conversation that re-prefills 2M tokens a turn is
  the executor's cost, not the chain's; the successor that prices a continuation is its own ADR.
* **Who pays for a seat's fetch bandwidth**, and the interval lane's cap for a resume opening: a
  plan quantity, declared by the class, not a protocol number.
* **The responder** (ADR-0093): owed as before; D5 makes it the only thing between a fused class
  and a 2M dissection inside the RC's own window.
* **ADR-0100 D5's order** (paid seats and the fuzz gate before a sharded class bears weight):
  inherited unchanged by a class under this fence.
* **The streaming inventory** (ADR-0102 §8): the prerequisite for measuring a K3-class artifact at
  all, and not this ADR's.
* **Whether the per-slice sub-roots should be per shard** rather than per `(kind, layer)`: a
  shard is a plan quantity and the map is a class quantity, and binding the map to a plan would
  make a re-plan a new class. Per slice, with the plan's shards as contiguous slice ranges, is the
  form D3 takes; a measurement of the seat's verification cost at 2M could move it.

## 9. Number hygiene

0103 is the next free number after ADR-0102 (whose §9 says so; the README row agrees). Claimed on
`docs/adr-0103-the-context-is-held-off-the-chain`, branched from `feat/adr-0099-sharded-seat` at
`3e9c3b59`, on 2026-09-11. **The next free number is 0104.**

**0102 was claimed twice, and the rule decides it.** `feat/adr-0099-sharded-seat` carries
[0102 — the embedding lift is read per token](0102-the-embedding-lift-is-read-per-token-and-an-unarmed-kernel-is-not-in-the-identity.md)
(authored 2026-09-10 per its own §9, committed `3e9c3b59` at 04:41 JST on 2026-09-11 after the
scratchpad wipe it records); `feat/adr-0096-partb-drill` carries a second 0102, *a close too wide
for one carrier is cut once, and every filer shares the cut* (authored and committed `90ec2317` at
05:02 JST on 2026-09-11), whose own §8 says "a concurrent claimant of 0102 renumbers the later
writer". The later writer is the close-cut ADR, by both dates; it takes 0104 when it lands beside
this file, and every citation of it here says "the close-cut ADR" rather than a number, so nothing
dangles when it does. Neither branch is edited by this one.

## 10. Implementation record (2026-09-11)

Built on `feat/adr-0103-held-context`, branched from `252b7b3b` (ADR-0093's responder, `c9be7676`,
already in it), commits `c204bf0f` … the branch head. **The fence is `None` on every shipped
preset, and both shipped fingerprints are byte-identical** (`shipped_presets_have_pinned_fingerprints`
green); testnet-11 does not move. A network that wants this ADR mints with
`palw_held_context_mint_v1` — the one spelling the generator, the tests and `kaspad
--palw-held-context-devnet` share.

### 10.1 What each Decision became

| Decision | where it lives | what pins it |
|---|---|---|
| **1** the one-move court is the court | `palw_checkpoint_court_v1.rs` (`CheckpointAccused`, tag 42, a COMPLETE_V4 context: the chunk opened against the checkpoint root, the cache-write rows against the step root, the composition recomputed); `CourtOpened` refused by name once the fence is armed (acceptance and fold), and the panel's challenger half opens none; a ladder past the bisection's clock admitted only over COMPLETE_V4 (`PalwConsensusParamsV2::validate`) and only with the fence armed from genesis (`Params::validate_palw_v2`); the accusation's prompt carriage (`PalwShardCourtAccusationV1::prompt_ids_opening`, built by `palw_refutation_prompt_carriage_v1` and read by the opened adjudicator — §10.3 item 11) | the checkpoint court's tests on v3 and v4; the regime's fold tests; `a_ladder_past_the_clock_needs_the_regime_from_genesis`; `a_refutation_rides_the_list_on_a_flat_network_and_one_tile_on_a_merkle_one`; `under_the_merkle_prompt_form_the_drill_convicts_and_the_seat_recomputes_the_jobs_ids` |
| **2** the seat's unit is positions, and a seat resumes | `palw_held_context_v1.rs` (intervals, route, `P`, fetch); base0's interval geometry counts in STEPS (`Base0FpIntervalUnitV1::Positions`, one replay loop over windows); the Resume route (`Base0FpResumeOpeningV1` under a bit-30 request on the interval lane, `base0_fp_verify_resume_v1`'s slice-local check, `base0_fp_accept_resume_v1`); seam verbs `fp_held_route_v1` / `open_fp_resume_v1` / `fp_accept_resume_v1`; the panel derives the route from the class and `window_receipt` and asks for the state beside the interval | `every_held_graph_v7_interval_opens_and_a_recomputing_seat_licenses_it`, `a_held_interval_that_resumes_inside_the_prompt_opens_and_is_licensed`, `a_seat_that_resumes_reaches_the_verdict_a_seat_that_recomputes_does`, `a_held_class_seats_its_prompt_in_intervals_and_a_lie_in_it_is_a_fault` |
| **3** map v4 | `palw_step_leg.rs` (leaf `(map, slice, block)`, promote-odd slice trees, the frontier, the depth cap 48); `palw_state_chunk_map.rs` (layouts, one dispatch for leaves, roots, paths, top leaves); graph-v7 rows (`qwen25_a16_profile_v7`, `qwen36_profile_v7`); the producer's per-position capture folds a held checkpoint as an APPEND (per-slice frontiers — one tile a slice and `depth` nodes a position) | `the_held_fold_is_an_append_and_its_root_is_the_courts`, `the_held_composition_folds_and_its_root_is_the_courts`; both fused dissection drills on graph-v7 (dense and hybrid) |
| **4** the ids never ride | Merkle prompt ids and `PanelDa` required at or below the fence (assembly); the payload wall under the fence; a PublicDa carrier past one standard transaction skipped by name (`palw_fp_objects_from_accepted_txs_under_held_v3`); the held DA court (`palw_held_da_v1.rs`, tags 43/44, the `held_da_missing` collection behind tail 0xA3) | `under_the_held_regime_public_ids_past_one_transaction_are_skipped_by_name`, the held DA court's fold tests |
| **5** a fused leaf's dissection opens at the leaf | `open_at_named_leaf`, `worst_case_duration_held_daa`, `palw_court_arity_held_v1`, `palw_attn_court_admits_row_held_v1`, `palw_court_params_held_at_v2` | the fold tests; invariant 3 in `palw_adr0103_held_context.rs` |
| **6** one position | closed-form coordinates and index; the per-position budget in `validate_geometry`/`validate_shape` for a held map | `the_registration_gate_visits_the_same_nodes_at_every_context` and D6's own tests |
| **7** the plan prices the fetch | `PalwShardV1::fetch_bytes_at_v1`, `palw_shard_resume_ms_v1`, `palw_shard_plan_for_seat_within_window_v1`; `palw-shard-plan` prints the column | `the_shard_plan_prices_the_fetch_and_the_window_binds_the_shard_count` |
| **8** every wall's order | `palw_model_fit_v1.rs` (`PalwFitOrderV1`, the held and held-network regimes, the held terms); `verify_class_admission_v8` (`HeldMapNeedsItsFence`, `LinearInTheContext`, `ChainWallOrderUnknown`); `palw-model-fit --preset held` (`docs/palw-model-fit-held-2026-09-11.md`) | `consensus/core/tests/palw_adr0103_held_context.rs` |
| **9** priced before armed | `palw-seat-coverage` §7 | `the_held_mint_moves_no_economics` |

### 10.2 The normative numbers (the generator's, replacing §1.2's and §3's arithmetic)

`palw-model-fit --preset held` — testnet-11's lattice minted with the fence at a `2^48` ladder:

| | dense graph-v7 at 512 | at 32,768 | at 2^21 |
|---|---|---|---|
| per-position budget (nodes) | 677, constant | 677 | 677 |
| ladder as a depth | 26 levels, logarithmic | 32 | 38 |
| close | 87,743 B, logarithmic | 88,127 | 88,511 — 64 bytes a doubling |
| window | 13 moves → 762 DAA | 25 → 1,266 | **37 moves × 42 + 216 = 1,770 DAA at arity 2** |
| state proof | 12 levels | 18 | 24 |
| ids on the commitment | 0 (PanelDa) | 0 | 0 |
| held: retention / fetch / replay | 28 MiB / — / 512 | 1.8 GiB / — / 2,048 | 112 GiB / 112 GiB (Resume) / 2,048 positions |

Every chain wall of the dense, the hybrid and the K3 stand-in's graph-v7 rows reads constant or
logarithmic from 512 to 2M, and every one admits; the K3 stand-in's ladder binds first, at
43,821,980 positions. Decision 5's table is confirmed at arity 2. D7: at a 512 GiB seat the dense
row resumes its last 2M interval on one shard at 100 Mbit/s and above (2.7 h of the window's 10);
the K3 stand-in needs six. D9: the shipped `k = 4` catches a one-token lie in a 2M prompt with
1.94 % at five seats on the dense row (`N` = 1,025) and 0.03 % on the K3 stand-in (`N` = 65,568,
`P` = 32); 90 % needs `k` = 379 or `s` = 589 on the dense row.

### 10.3 Corrections to this ADR, found while building it

1. **§1.1 called the close already flat. It is not, on either shipped preset**: the order column
   reads it `Linear` — the prompt ids are flat there, and the generated-token pin is priced at the
   whole context (`decode_pin_price_v1(profile, n_ctx)`). Under the fence the pin is priced at the
   trace cap (`PalwCourtCostShapeV1::decode_bound`), because one job decodes at most
   `PALW_V2_MAX_TRACE_EVENTS` calls; which is also why Decision 4's Merkle OUTPUT-id form
   (ADR-0082 U-07b) is not needed for R-held — the output-id term is bounded by the cap — and it is
   not built.
2. **Decision 8's classifier read `C`, `2C` and `4C` by the second difference.** Three points
   cannot tell a logarithm with a ceiling (a dissection at arity 64 gains one round in six
   doublings: first differences `0, 1`) from a line. It reads seven (`C … 2^6·C`) and calls a need
   linear when its last growth is at least eight times its first nonzero growth.
3. **Decision 1's ladder is a NETWORK number and the regime was written per class.** A ladder past
   the bisection's clock makes the bisection unplayable for every class, so `CourtOpened` is refused
   for every claim once the fence is armed, every class's window is read on the held clock, such a
   ladder is admitted only with the fence from genesis, and a class that registers no held map on a
   held network keeps its shipped walls (`PalwFitRegimeV1::HeldNetwork`) — never refused for an
   order.
4. **Decision 4's "PublicDa is refused by name where `C × 4 > PALW_STANDARD_TX_BYTES`"** became: the
   fence requires `PanelDa` at or below its height; a held class judged without it reads its
   payload wall `Linear` and is refused `LinearInTheContext` at every width (invariant 7's first
   clause and invariant 8 need the stricter reading); and at the commitment, a PublicDa carrier
   past one standard transaction is skipped by name.
5. **Decision 2's `P` "derived at certification"** would have had to be a consensus object before
   the executor could open by it and a seat draw over it. `P` is a class function
   (`palw_held_interval_positions_v1`: the widest power of two whose opening fits the interval
   lane's cap and which cuts the context into at least the draw's `k` intervals), and the CLOCK is
   the certification drill's check (`palw_held_seat_interval_positions_v1`,
   `palw_shard_plan_for_seat_within_window_v1`).
6. **The seventh wall.** The advertised free-prompt cap was bounded by the IPC frame's 4,096 ids
   (`PALW_V2_MAX_PROMPT_TOKENS`), which made a 2M prompt inexpressible whatever the six terms said.
   A held mint advertises the regime's own (`PALW_FP_HELD_MAX_PROMPT_TOKENS_V1` = `2^26`), admitted
   only over COMPLETE_V4 with the fence from genesis. The worker's request frame
   (`PALW_V2_MAX_FRAME_BYTES`, 256 KiB — about 60,000 ids) is a node-local transport bound and is
   not raised here.
7. **A soundness gap this regime exposed:** nothing tied a checkpoint chunk to the committed
   cache-write rows it summarises. Under the fence a dissection's bottom and a seat's resume both
   stand on checkpoints, so `CheckpointAccused` is the object that makes a false checkpoint
   convictable, not a convenience beside the shard court.
8. **The data-availability court is not required at the fence's height.** The held DA court arms
   with `palw_da_court` (it reuses ADR-0062's `DefaultDisputed` phase with a sentinel index); a
   held network without it withholds into ADR-0077 SA-5's void.
9. **Decision 5 said the responder was "unchanged and still unbuilt".** ADR-0093's responder was
   built before this branch (`c9be7676`); both fused dissection drills run on the held rows here —
   exact on real committed rows, and a forged row convicted at its bottom.
10. **Decision 9's premise** — "a claim's reservation is its decode leaves' whatever its prompt" — is
    ADR-0082 Decision 10's numerator, which `validate_palw_v2` refuses to arm on any network today
    (audit D M-1). The mint changes no economics (`the_held_mint_moves_no_economics`); a network
    that wants Decision 9's sentence at 2M arms Decision 10 with it, which is not this ADR's move.
11. **Decision 1 said the one-move court runs `check_execution_step_refutation_capped_v1`
    "unchanged". Under trace format 4 it cannot convict a free-prompt claim at all** — found by the
    live drill, not by a test. That entry point compares a refutation's carried id list against a
    FLAT `prompt_token_ids_hash`; a held network commits the ids as the tiled root (Decision 4), so
    every refutation that carried the list read `InputSetNotCanonical` — no verdict — and a
    refutation that carried none could not adjudicate a gather. ADR-0081 had built the opened
    adjudicator and deferred the carriage field "to the genesis cut that arms
    `palw_prompt_ids_merkle`"; the held mint is that cut. Built: the accusation carries the gather's
    one tile (`prompt_ids_opening`, outside the session id — it is bound by the job's root; a new
    field on tag 38, whose fence is `None` on every preset and which no chain carries), the verdict
    reads it through `check_execution_step_refutation_opened_capped_v1`, the ceiling prices it, and
    one helper decides the carriage for the one-move accusation and the bisection close's
    `ArithmeticOpened` arm alike. So the close term stays the path Decision 1 promised — one tile
    and `⌈log₂ tiles⌉` siblings — rather than the whole prompt.
12. **Three readers still hashed the prompt flat on a Merkle network**, each silently: every
    family's `operand_openings_for` ran the flat check to learn which artifact rows the court
    resolves, so it recorded none for a gather (now `check_execution_step_refutation_carried_capped_v1`,
    the check the chain will run); the seat's prefix recompute (`base0_fp_recompute_state_*`,
    `base0_fp_seat_state_memoized_v1`) refused an honest job's own ids, so every interval past the
    first filed `Incapable` (it now takes the network's form); and the seat's capture sampler graded
    the flat carriage, so each of its 32 draws was "not a sample" at trace level and the seat filed
    nothing. The live drill showed all three at once; `under_the_merkle_prompt_form_…` pins them.

### 10.4 What was run

The consensus-core suite, the base0 library suite, kaspad's PALW tests and the SDK's — green; the
ADR-0103 integration tests (invariants 2, 3, 5, 8, 9 and 10 at 2M on the held mint, §1.1 pinned
as the shipped presets' limitations); invariant 4 on the producer's capture; invariant 6 at the
seat (resume against recompute, a tampered fetched chunk refused by its slice, a shard's slices
verified alone); invariant 7 at the carrier and the held DA court; invariant 1 in the fold and the
acceptance arm, and on a live devnet by `HELD=1 scripts/misaka-palw-shard-court-devnet-drill.sh`:

* **Run 1 (2026-09-11 11:58 JST) found §10.3 items 11 and 12.** Three devnet nodes, the floor class
  only, `--palw-held-context-devnet` on every node; node-0 committed canonical claim `aeff845e…`
  (tx `ed3f166a…`) with its capture corrupted at step leaf 0 at 12:05:06, and block `8bdbb377…`
  carried its `FreePromptCommitted`. At 12:13:26 both seats drew intervals `[0, 1, 2]`: one found
  interval 0 does not replay in leaves `[0, +4096)` and fetched the block, the other filed
  `Incapable` — "the served ids do not hash to the job's prompt_token_ids_hash" — and neither
  accused: every sample of the tampered capture read no verdict under the Merkle root. Stopped at
  12:26 with the drill still at step 2 of 5.
* **Run 2 (12:57 JST), PASS — invariant 1 on a live devnet.** The same drill on the fixed build
  (`c593e78a`). Node-0 committed canonical claim `20995717…` (tx `84b525d8…`) at 13:04:58; at
  13:08:57 both seats' first duty round found leaf 0 of the served capture does not recompute and
  filed `ShardCourtAccused` carrying the prompt's tile (sessions `fa17158f…`, `d0c24a1e…`);
  13:09:37 block `50b97e87…` carried one accusation and every node folded it; the claim reads
  `voided court_fraud` on node-1 and node-2 over RPC; bond 0's collateral fell from 1,110,106,160
  to 1,110,067,640 sompi, −38,520 — to the sompi the slash of ADR-0100's run 2 without the fence,
  which is invariant 1's "the same tampered leaf, the same one-move conviction, the same slash".
  No node built a `CourtOpened`, no seat filed `Incapable`, and no sampler refusal was logged.

### 10.5 What is not done

* **No 2M claim ran.** The dense row's cache at 2M is 112 GiB and its recompute 54.6 hours
  (§1.2); this Mac seats neither. The "done when" is met at devnet widths — the held graph-v7
  rows' intervals, resumes and convictions in the tests, the one-move drill under the fence — and
  at 2M in the generators' tables.
* **A seat that names a leaf without holding the capture cannot yet accuse it.** Decision 1 took
  ADR-0100's premise that "its interval opening IS the refutation's inputs". For BASE-0's openings
  it is not: an opening carries the interval's committed leaves as hashes (block roots where the
  capture is retained sparse) and the block-leaves lane carries leaf hashes, so a seat can name the
  leaf off the chain (ADR-0086 D6) but holds none of the committed tiles a refutation reveals. The executor serves those tiles only as a close annex, and only for a leaf an open
  session names (ADR-0085 §6 item 4) — and under the fence no session opens. The held DA court
  cannot compel them either: its units are ids, chunks and leaf-hash blocks, not a step tile. At
  devnet widths every seat holds the capture and accuses directly (§10.4, run 2); at the widths this
  ADR is for, a liar that commits a wrong leaf is named but not convicted. Two moves close it, and
  choosing is a protocol decision: (a) the executor serves a named leaf's annex on a panel seat's
  signed request — the one-move court's stand-in for the session that used to authorise it (the
  interval lane's `u32` index cannot address a leaf, so it is a new request) — which an honest
  executor answers and a liar simply does not; so (b) the held DA court gains a unit for a leaf's
  committed evidence (its output tile and input rows, each with its opening against the step root,
  bounded by the court's close ceiling), whose withholding defaults the executor exactly as a
  missing tile of ids does. (b) is the one that makes silence convictable, and (a) is its
  off-chain fast path; recommended together. **Closed by [ADR-0111](0111-a-seat-may-demand-the-committed-leaf-it-needs-to-judge.md)**,
  which built both — the leaf request rides the interval lane after all, under bit 29 with the leaf
  in a signed field — and drilled each to a slash with no seat holding the capture (ADR-0111 §8.3).
* **A resume opening past the interval lane's 4 MiB** (a 2M shard's state is gigabytes): §4 and §8
  already say the class declares its own lane cap off the plan; no such lane is built.
* **A shard seat's replay from its own slices.** The fetch is verified slice-locally; replaying one
  shard from that state is ADR-0099's shard replay and is not rebuilt here.
* **The worker's request frame** for prompts past ~60,000 ids (§10.3 item 6).

### 10.6 Decision 2's price, amended (2026-09-11): a replay pays for its history

Decision 2 priced a seat's replay as a line: `n_ctx × replay_ms_per_position` decided the route,
and `budget / replay_ms_per_position` bounded the width's clock. But a position's attention reads
every row before it. ADR-0110 §9.3 measured the consequence on the context vectors' thin row: from
4,096 to 32,768 positions (8×) the seat's recompute grew 41×. The route was therefore optimistic
exactly where it matters. It sent seats to recompute contexts they could not recompute inside the
window, and a seat that overruns files nothing.

**The amendment** (`consensus/core/src/palw_held_context_v1.rs`, consensus-inert like the rest of
the regime's pure half):

* **The knee.** `palw_held_attention_knee_v1(profile)` is the class's own multiply-accumulates for one
  position over its attention's for one row of history, read off the registered geometry. The
  numerator counts the attention layers' `q`, `k`, `v`, `o` projections and every layer's MLP. The
  denominator is `2 × heads × head_dim` per attention layer. The unembedding is left out of the
  numerator because a recompute drops the logits, and leaving work out can only make the knee
  smaller, the side a bound must err on. The dense tier's real row (Qwen2.5-1.5B) has a knee of
  15,232 rows. The context vectors' thin row has 28. A class with no attention layer has
  `u64::MAX`, and its replay is the old line to the millisecond.
* **The price.** `PalwHeldReplayCostV1 { ms_per_position, knee_positions }`. The rate is still the
  family's measured row (SA-4's source), taken as the cost with an empty history, which is how it was
  measured. `replay_ms_v1(first, count)` prices positions `first .. first + count`, each against every
  row before it: `count + (count·first + count(count − 1)/2) / knee` positions' worth, rounded up
  once and saturating.
* **The route** is `Recompute` exactly when `replay_ms_v1(0, n_ctx)` fits the seat's budget.
* **The width's clock** is the LAST interval's replay, `replay_ms_v1(n_ctx − P, P)`, plus the fetch.
  `P` is halved from the wire's and the context's bound until that fits. The class's chain-facing `P`
  (`palw_held_interval_positions_v1`, §10.3 item 5) is unchanged; this is the certification's check.
* **The shard plan** (`palw_shard_resume_ms_v1`) prices the same last interval and gives each shard
  its layers' share of it.

**What it moves**, at testnet-11's windows (`window_receipt` 600, a 10-hour seat budget):

| | the line | with the history |
|---|---|---|
| widest context the dense row RECOMPUTES | 1,058,823 positions | 165,012 positions |
| the dense row's last 2M interval (`P` = 2,048, the wire's) | 70 s | 2.68 h |
| one whole-model shard resumes it (112 GiB fetch) down to | ≈ 27 Mbit/s | ≈ 37 Mbit/s |
| the thin vector row's route on the held devnet (`window_receipt` 40) | recompute to 70,588 positions | recompute below 1,961; resume above |

D7's sentence in §10.2 reads, amended: at a 512 GiB seat the dense row resumes its last 2M interval
on one shard at 100 Mbit/s in 2.7 h of fetch and 2.7 h of replay, 5.4 h of the budget's 10.

**The model is a bound, and the vectors check it.** On the thin row the model grows 63× from 4,096 to
32,768 positions. The engine measured 41× for the seat and 22.6× for produce. The model is
steeper, which is the direction a price that sends seats to recompute must err in. The 4,096 and
32,768-position vectors now take the Resume route, and their `document_id`s were re-pinned for that
alone. Only `seat.route` and the intervals' `resume_bytes` moved; every root, count, size and verdict
still reads as ADR-0110 §9.2's table. So the pinned suite now exercises both routes: 512 positions
recompute, and 4,096 and 32,768 resume.

**What it does not move.** SA-4's turn deadlines (`palw_context_ladder`, `palw_court_deadline`) still
price a court move's replay linearly. Those are consensus numbers on every shipped preset, and this
section does not touch them. At the widths a shipped network executes (a free prompt is capped at
4,096 ids), the history adds 1.7% at 512 positions and 13.4% at 4,096, which the deadlines' margins
absorb. ADR-0082 Decision 9's width bound for a non-held seat (`palw-certify`) is likewise
unchanged. A held network that prices court moves at a wide context takes this price with it, and
that is a ruleset move, not this amendment.

### 10.7 The eighth wall, found planning the 2M vector (2026-09-11)

§10.2 says every chain wall of the dense row admits from 512 to 2M. It does, but a wall that is
not the chain's refuses first. The A16 tier's catalog attention ops bound a history at
`A16_MAX_DOT_LEN` = 2^18:

* `a16_attn_values` refuses `kv_len > 2^18`;
* the fused site's tile arithmetic refuses more than 2^18 positions;
* the engine's fused arm composes those ops over the whole history;
* the fast kernels mirror the same refusals.

So no A16-family class executes past position 262,143. At the 262,145th row the forward fails with
`DotTooLong`, and the producer, the seat and the court all stop there. The widest A16 context
anything in this tree can produce, replay or adjudicate is 262,144 positions.

The bound was set for projections ("one power of two above the largest real reduction in the family
— Qwen2.5's `d_ff` and vocabulary"), when no attention history was longer than a few thousand rows.
Its exactness argument has room for a longer history. A value product is below `2^30`, so `2^26`
rows sum inside `i64`, and a row of `2^21` Q24 exponents sums below `2^45`. But a separate history
bound is a change to catalog ops the court runs, so it is a ruleset decision. It is inert on every
shipped network, because a free prompt there is at most 4,096 ids. It also carries a precision
question the arithmetic does not answer: at `2^21` positions a near-uniform row's Q24 probabilities
are about `8/2^24`, three bits each. It is recorded here, and ADR-0110 §9.5 carries what it means
for the 2M vector. Nothing is changed.

**Decided 2026-09-11: the bound stays** — **reversed the same day by the operator, and implemented as [ADR-0116](0116-an-attention-history-is-the-classs-and-the-held-regime-reduces-over-its-own-width.md):** a held class's history bound is the regime's 2^21, read off its registered map; every other class keeps 2^18. The reasoning below is kept as the record of what the reversal accepted — above all that the bound moving makes 2M executable and not cheap. The operator asked that the open design calls be made
where the gain is large. This one's gain is nil today and its cost is not small:

* **What raising it would buy now: nothing a network runs.** Every shipped network admits a free
  prompt of at most 4,096 ids, 64 times under the bound; no class, lane or vector beyond 262,144
  positions is proposed except the 2M vector, which is a measurement.
* **What would still stop 2M after it.** The seat's replay grows with the square of the width
  past the knee (§10.6). The 131,072-position vector's seat took 819 s of its 44.7 minutes on a
  12-core host (ADR-0110 §9.5), and sixteen times that width is, by the same rule, up to 256 times
  that seat — days, which no court clock in this tree is sized for (ADR-0092). A bound raised alone
  would trade a refusal by name for a replay nobody finishes.
* **What it would cost.** A catalog op the court runs changes: a ruleset move on every network,
  and on testnet-11 another flag day beside DAA 3,500's. A constant alone is also not enough:
  three-bit probabilities at 2^21 need a wider probability format, which is new kernel semantics,
  not a bigger number.

So the refusal by name stays the answer at 2M (`the_widest_runnable_vector_is_2_18_and_the_2m_one_is_refused_by_name`),
and the question reopens with any one of: a lane that admits prompts past 2^18; a seat that replays
2^19 positions inside the court's wall clock; a probability format with at least eight bits at
2^21.
