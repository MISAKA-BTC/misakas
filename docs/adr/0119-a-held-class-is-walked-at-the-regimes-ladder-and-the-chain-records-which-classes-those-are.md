# ADR-0119 — A held class is walked at the regime's ladder, and the chain records which classes those are

* Status: PROPOSED and, for its consensus half, IMPLEMENTED 2026-09-12 on
  `feat/adr-0103-held-context`, at the operator's instruction ("ADR-0119 に進める"), after ADR-0118
  §4 measured that testnet-11's frozen ladder admits a held dense class to about 512 positions.
  Rides testnet-11's held flag day, DAA 7,000 (ADR-0118 Decision 6). **Inert wherever the held
  regime is not in force:** the new state collection is empty until a held class registers, which
  nothing below the fence can do; the transaction door keeps its validity below the fence; every
  class that is not held is walked, priced and bounded exactly as before.
* Builds on: [0103](0103-the-context-is-held-off-the-chain-and-the-chain-carries-a-root-an-opening-and-a-logarithm.md)
  (the regime, and Decision 1's "a ladder that costs no rounds is minted at the top of the carrier's
  budget"), [0118](0118-the-held-regime-arrives-at-a-height-and-a-held-class-carries-its-own-prompt-form.md)
  (the regime at a height; the class owns its prompt form — the move this ADR makes for the ladder),
  [0116](0116-an-attention-history-is-the-classs-and-the-held-regime-reduces-over-its-own-width.md)
  (the history bound is the class's).
* Amends: ADR-0103 Decision 1's `2^48` example ladder (a held network's ladder is at most `2^40`);
  ADR-0082's "the opening cap is the structural one" for a fused site's rows (under the held
  regime it is the claim's ladder).

## 0. The sentence this ADR is

**A class under the held regime is walked, priced, bounded and prosecuted at the regime's ladder,
`2^40` leaves, on every network, and the chain records which classes those are when they register;
every other class keeps the network's ladder; and under the regime a fused site's rows are opened
at the claim's ladder, not at the structural `2^22`.**

## 1. What was found

ADR-0118 made the regime armable at a height and measured what that buys on testnet-11: the held
dense row is admitted at 512 positions and refused one doubling on, because the network's step
ladder — `2^26`, frozen in the genesis bundle — bounds the whole context as prefill. The ladder is
frozen at what a BISECTION of the whole step space can play inside the court window, since a
bisection opened before any fence must stay playable; a bundle that took a deeper one is refused
(`window_court does not fit the worst-case honest prosecution`). But a held class exists only past
the held fence, and past it no bisection opens. The clock that froze the ladder never binds a held
class.

Mapping every reader of the ladder (the survey of 2026-09-12, some ninety sites) found the ones
that decide:

* **Two consensus readers hold no class.** The free-prompt extraction walk bounds a commitment's
  `work_leaves` at the network's ladder, from the accepted transactions and the parent state; the
  class state kept no record of which classes are held, only `fused_attention`. The transaction
  door — isolation, which invalidates a block on failure — caps `work_leaves` at the structural
  `2^32` with no class, no state and no height; a dense job at `2^21` positions is `≈2^37.6`.
* **The held dissection could not open past `2^40`.** It opens its session on a bisection ladder
  whose space is the ladder, and a bisection ladder refuses a space past `PALW_BISECT_MAX_SPACE`
  (`2^40`). ADR-0103's reference mint at `2^48` had no prosecutable fused leaf.
* **A fused site's rows opened under the structural `2^22`**, not the ruleset's ladder
  (`palw_attn_dispute_site_v2`, deliberately: the fold holds no bundle). Past the court-ladder
  fence a class's claims are admitted to `2^26`; testnet-11's own graph-v5 512 row has a canonical
  job of 6,630,544 leaves, and every job of it past about 36 prompt tokens is above `2^22`
  (measured: 32 tokens and 4 decode 3,681,344 leaves; 40 and 4, 4,505,408). Its fused leaves could
  not be dissected at all — on testnet-11 today, not only under the regime. (Reported to the
  deep-audit session, whose C-01 binds the canonical count at 7,000 and so exposes exactly this.)
* **testnet-11's 512-token prompt cap is not enforced at runtime** (`palw_fp_ruleset_caps` is dormant
  there), so it was never the binding wall there; the ladder was.
* **On the node side**, the free-prompt worker refused every PanelDa job ("a mode the panel cannot
  replay must not execute", written four days before private prompts landed) — so a gateway started
  `--privacy panel-da` could file nothing, and a held class on a network minted flat, which commits
  under PanelDa only (ADR-0118 Decision 5), had no executor path.

## 2. The requirement

On testnet-11 past its held fence, a held class is admitted, produced, seated and prosecuted at
the contexts ADR-0116 widened the history for, and no other class's bounds move; below the fence
every node — upgraded or not — accepts exactly the transactions and blocks it accepted before;
and no two nodes can bound one claim at two ladders. This ADR meets the chain's half — what is
admitted, bounded and tried — and the node's half up to the network's ladder; the node's held route
past it is §7's.

## 3. Decisions

**Decision 1 — the held ladder is `2^40`.** `PALW_HELD_STEP_LADDER_V1 = PALW_BISECT_MAX_SPACE`: the
deepest ladder whose dissection session opens, 40 levels and 2,560 bytes of path in a close. The
dense 1.5B row at `2^21` positions is `≈2^37.6` leaves and Qwen3.6-35B's density `≈2^39.3`; a class
past `2^40` is refused by the gate's own ladder wall. `palw_class_step_ladder_v1(network, profile)`
is at least that for a held profile and the network's for every other. A held network's court ladder
is at most `2^40` (`validate_palw_v2`, and `palw_held_context_mint_v1` refuses a deeper one), so for
any real network a held class's ladder is exactly the regime's.

**Decision 2 — the chain records which classes are held.** `PalwChainStateV2::class_step_ladders`,
a guarded collection (carriage tail `0xA5`, delta variant 41, rooted only when non-empty): written
when a class registers with a held profile — the fold holds the profile then and never again — with
the regime's ladder. A class id is a hash of its profile, so a row is a function of the id and is
never rewritten; it reverts with the registration and travels with a pruned sync's carriage. Every
consensus reader of a claim's ladder asks `class_step_ladder_v1(class_id, network_ladder)`.

**Decision 3 — admission prices and counts a held class at its ladder.** The gate's ladder wall and
recount, the ladder rules' cost shape (the one door every gate, fit and deadline reads its rules
through), the fit, and the registration builder's canonical count all read the class's ladder.

**Decision 4 — the courts read the claim's ladder.** The one-move court (acceptance now finds the
claim before bounding the accusation's shape), the checkpoint court, the held DA court's answer,
and the held dissection's session space all read `class_step_ladder_v1` of the claim's class, in the
acceptance layer and in the fold alike. A fused site's rows open under `palw_attn_opening_cap_v1`:
the structural `2^22` before the held regime, as always; under it, the claim's ladder — a held
class's `2^40`, every other class's the network's step ladder. The acceptance layer asks it at the
block's DAA (`adjudicate_court_close_v3`), the fold through its extras. On testnet-11 this makes the
shipped dense row's fused leaves dissectable from 7,000 — a consensus change for that row's claims
past the fence, and the one the deep-audit session's C-01 needs beside it. On the node, a party
derives the site at its backend's ladder (`base0_attn_site_evidence_v1`; it derived at `2^22` and
refused its own first opening past it), and the panel files a root claim — the one move filed
without asking the chain first — only when the court will open it
(`attn_root_claim_is_openable_v1`: always under the regime, to `2^22` before it). The court's cap is
an upper bound (`step_opening_root_capped_v1` bounds the leaf count and the path), so evidence built
for one tree verifies under any cap that holds it. And an executor's range opening is bounded by its
tree's own depth, not the default leg's 44 siblings, which refused an honest opening of a claim past
`2^22` leaves that the chain walks under the ladder that admitted it.

**Decision 5 — the extraction walk bounds a commitment by its class.**
`palw_fp_objects_from_accepted_txs_by_class_v1` reads the parent state by the commitment's own class
id: a held class's commitment is bounded by its recorded ladder (and, where the ruleset caps are
armed, by the regime's prompt cap `PALW_FP_HELD_MAX_PROMPT_TOKENS_V1`), every other class's exactly
as before. A commitment in the block its class registers in meets the network's bounds — its class
is not yet in the parent state — and a producer resubmits it one block later.

**Decision 6 — the transaction door is the ADR-0087 D6 pair.** Isolation holds no height, so it asks
the height-free question: its work-leaves cap is the regime's `2^40` on a ruleset that declares the
regime at all, the structural `2^32` otherwise (`palw_fp_isolation_work_leaves_cap_v1`). The
header-context door (`check_palw_fp_work_leaves_in_context`, blocks at their own DAA and the mempool
at the virtual's) refuses a commitment past `2^32` below the fence, by name
(`PalwFpWorkLeavesBeforeHeldActivation`). So a build that schedules the regime and one that does not
carry it agree on every transaction before the activation — the refusal a stale node makes at the
door, an upgraded node makes one door later — and nothing is committed on the permissive answer.

**Decision 7 — the private free-prompt path runs.** The worker's precheck admits both modes the chain
carries, PublicDa and PanelDa. The rest of the path already handled PanelDa: the commitment builder
carries no ids under it, and the drawn seats read them from the staged material the submitter took
from the worker's result.

## 4. What it costs

* **A consensus change at 7,000 for the shipped dense row:** its fused leaves' rows open at `2^26`
  instead of `2^22`. Before the fence, byte-identical.
* **A state collection** that roots and encodes nothing until the first held registration.
* **A header-context check** on every free-prompt commitment on a ruleset that declares the regime:
  one decode of a payload isolation already decoded.
* **What it does not buy: a node that produces or tries a held job past the network's ladder.**
  Every backend is still built at the network's ladder, so a held class's producer refuses a job
  past `2^26` leaves on testnet-11 (about 650 positions of the 1.5B dense row) and a seat signs a
  claim past it `Unavailable`. The chain admits to `2^40`; the gap between the two is claims no
  honest producer makes, and until the node side lands (§7) no honest seat can verify such a claim
  (each signs `Unavailable`, and a quorum of those voids it as `ProducerDefaulted`) and no honest
  challenger can prosecute one — only a quorum of dishonest drawn seats could certify it.

## 5. Invariants the tests hold

1. **I-1, the ladder is the class's**: a held class records `2^40` at registration and a floor class
   nothing; the row is rooted, carried, imported and reverted
   (`a_held_class_records_its_ladder_at_registration_and_nothing_else_does`).
2. **I-2, admission**: on testnet-11 past 7,000 the held dense row is admitted at 512, 1,024, 32,768
   and `2^21`, not before the fence, and a graph-v5 row at 1,024 keeps the network's `2^26`
   (`a_held_class_on_testnet_11_is_admitted_to_2_21_at_its_own_ladder`); a held mint or network past
   `2^40` is refused; ADR-0103's 2M suite runs at `2^40`.
3. **I-3, the courts**: every held court arm of the fold and of the acceptance layer reads the
   claim's class ladder, and every close is judged knowing the regime
   (`every_held_court_arm_of_the_fold_reads_the_claims_class_ladder`,
   `every_held_court_acceptance_arm_reads_the_claims_class_ladder`); the opening cap is `2^22` before
   the regime and the claim's ladder under it (I-1's test); a party derives the site at its backend's
   ladder and files a root claim exactly when the court opens it
   (`a_party_derives_the_fused_site_at_its_backends_ladder`,
   `a_root_claim_is_filed_exactly_when_the_court_opens_its_rows`); a `2^26`-leaf tree serves a
   49-sibling opening the chain accepts (`a_tree_deeper_than_the_default_leg_serves_its_honest_openings`).
4. **I-4, the walk**: a held class's commitment past the network's ladder is extracted and every other
   class's is skipped as before (`a_held_class_commitment_is_bounded_by_its_own_ladder_and_every_other_by_the_networks`).
5. **I-5, the door**: isolation admits `2^40` only where the regime is declared
   (`the_isolation_door_admits_the_regimes_ladder_only_where_the_regime_is_declared`); a scheduled
   regime refuses a held job's leaves below its fence and admits them from it, and a commitment inside
   `2^32` is untouched (`a_scheduled_held_regime_admits_a_held_jobs_leaves_only_from_its_own_fence`).
6. **I-6, the private path**: a PanelDa job runs and returns its ids for the staged material
   (`a_panel_da_job_runs_and_returns_its_ids_for_the_staged_material`).

## 6. Supersession

| what | by |
|---|---|
| ADR-0103 Decision 1: "a `2^48` ladder is 3,072 bytes of path" | Decision 1: at most `2^40`, 2,560 bytes |
| ADR-0118 §4's pinned limitation (a held class to about 512 on testnet-11) | Decisions 2–6, at the chain; §7 for the node |
| `palw_attn_dispute_site_v2`'s structural opening cap | Decision 4 under the regime; unchanged before it |
| `fp_worker::precheck_request_v1`: PublicDa only | Decision 7 |

## 7. What is deliberately not decided yet

* **The node's held route past the network's ladder** — mapped on 2026-09-12 (every read of a
  backend's ladder, every job-sized allocation, the producer's and the seat's retention), and a design
  of its own (the next ADR), because what it found is not a number to raise:
  * **The backend's ladder is two things.** It prices and refuses a job, and it is the only guard in
    front of the whole-capture paths that allocate a vector of `step_leaf_count` hashes from a decoded
    or gossiped count (`leaves_by_position`, the dense re-executions, `verify_material`'s dense arm).
    At `2^40` a hostile blob could ask for `2^46` bytes. The node needs the class's ladder for pricing,
    refusal and Merkle walks, and a separate materialization cap — the network's — at every job-sized
    site, with held claims kept off the whole-capture arms.
  * **The held route does not stream yet.** The producer's fold does (it holds one open block and
    the retained nodes), but every interval replay — the executor's openings, the seat's V4 verify,
    the held DA court's answers, name-the-leaf — allocates a leaf vector the size of the whole job and
    keeps every tile of the replayed window: tens of gigabytes at 4,096 positions of the 1.5B dense
    row, by the code's own sizes. The replays must fold digests on the fly and serve openings from the
    retained digests and the two boundary blocks, which the capture's own documentation already says
    suffice.
  * **The retain level must be pinned, not derived.** Producer and seat each derive it from their
    ladder (`r = max(⌈log2 cap⌉ − 20, 12)`: 12 at `2^26`, 20 at `2^40`); a seat that derives 12 against
    a producer at 20 addresses the wrong block and names no leaf, and an `r = 20` block is 64 MiB, past
    the 4 MiB interval lane. The held route keeps `r = 12`, the seat reads it off the opening, and the
    block-leaves request indexes within the interval (a global 16-bit block index reaches leaf `2^28`).
  * **Two walls are not the ladder's.** A resume opening carries the whole state over the same 4 MiB
    lane, and a class of `n_ctx` `2^21` always takes the Resume route; and a fused site's dissection
    evidence is built from dense rows and the whole K/V history. Each needs its own transport or a
    windowed builder. (A `2^21`-position job of the 1.5B row also needs a K/V cache of about 120 GB —
    a host's question, not the protocol's.)
  * **The worker frame and the gateway's prompt limit** (256 KiB, about 60,000 ids; 64 KiB of text)
    bind only past the network's ladder, so they move with the rest of this.
* **The same bottom cap on a network without the regime.** The mainnet card arms the court ladder
  from genesis and not the regime, so its fused rows' bottoms stay at `2^22`; that is the card's
  decision, and the deep audit's.

## 8. Number hygiene

Written as 0119 on `feat/adr-0103-held-context` on 2026-09-12; `origin/main` and every local branch
hold nothing past 0118. The state tail `0xA5` and delta variant 41 were agreed with the deep-audit
session, whose fixes add none. **The next free number is 0120.**
