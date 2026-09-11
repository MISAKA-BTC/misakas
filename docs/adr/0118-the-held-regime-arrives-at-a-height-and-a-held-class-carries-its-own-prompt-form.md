# ADR-0118 — The held regime arrives at a height, and a held class carries its own prompt form

* Status: PROPOSED and IMPLEMENTED 2026-09-12 on `feat/adr-0103-held-context`, at the operator's
  instruction ("held方式の実装完了をgoalとする"). The operator decided the same day that the
  regime takes a flag day of its own on testnet-11, at a height they will choose, and not the
  audit's 4,000 — and, asked again on 2026-09-12, chose **DAA 7,000**, the deep-audit fence's
  height, so one release carries both. testnet-11 therefore arms the regime at 7,000
  (`PALW_RC_HELD_FENCE_DAA = Some(7_000)`; its fingerprint on this branch alone is `80524c3b…`, and
  the release that also carries `palw_prefill_draw` and the deep-audit fence is re-pinned when it
  is assembled). Every other preset leaves the regime dormant.
* Builds on: [0081](0081-long-context-the-input-is-a-state-chain.md) (Decision 3: the prompt-ids
  form), [0103](0103-the-context-is-held-off-the-chain-and-the-chain-carries-a-root-an-opening-and-a-logarithm.md)
  (the held regime), [0111](0111-a-seat-may-demand-the-committed-leaf-it-needs-to-judge.md) (the
  held DA court's leaf demand), [0116](0116-an-attention-history-is-the-classs-and-the-held-regime-reduces-over-its-own-width.md)
  (the history bound is the class's — the same move this ADR makes for the prompt form).
* Amends: ADR-0103's assembly rules ("V4 and the tiled prompt ids at the fence") to "from genesis";
  ADR-0081 Decision 3's "the form is the network's" to "the form is the class's, and the network's
  for every class but a held one".

## 0. The sentence this ADR is

**A network minted before the held regime takes it at a height: the fence commits the V4 signing
contexts into the identity itself, a held class commits its prompt ids as the tiled Merkle root
while every other class keeps the network's genesis form, a prompt tile is demanded only of a
claim whose ids are a tile tree, and the carriers stay the chain's — so on a network minted flat
a held class commits under `PanelDa`.**

## 1. What was found

ADR-0103's regime was written as a mint. `validate_palw_v2` refused `palw_held_context` unless the
bundle stated the COMPLETE_V4 context set and the network's prompt ids were Merkle (trace format
4), and both of those are genesis-only by construction: the context root is inside the ruleset id,
and `palw_prompt_ids_merkle` is refused at any height but 0 because "no reader of a
`prompt_token_ids_hash` holds the job's anchor height" (audit M-8). testnet-11 was minted flat,
with the V1 set, and it takes consensus changes by activation and never by a re-genesis. So on the
network the regime was meant for next, it could not be armed at all. Probed on this branch's base:
`validate_palw_v2` for testnet-11 with the regime, the one-move court and the pins at one height
fails three ways — the shard court's V3 contexts, the regime's V4 contexts, and the flat ids.

Mapping every reader of the three facts (the held-regime survey of 2026-09-12) found:

* **No runtime verification reads the bundle's context root.** The acceptance layer verifies each
  object under a hardcoded context and the fold binds `signature: _`; the root is read only by
  `validate_palw_v2`, the bundle's own validation and the identity. testnet-11 already verifies,
  at runtime, ten COMPLETE_V2 contexts its frozen nine-context root does not name.
* **The prompt form's readers either hold the class's profile or hold no class at all.** The
  admission gate, the backends, the fp worker, the court and the held DA court hold the profile
  (the court's through a binding authenticated against the claim's execution root). The
  transaction's stateless check and the extraction walk hold neither a class nor, for the first,
  a height.
* **A held class's ids never ride** (ADR-0103 Decision 4), and a held class exists on a network
  only past the fence, where no bisection is opened — so the one reader that grades a bisection's
  close against the network's form never meets a held class.
* **The held DA court's `PromptIdsTile` demand had no form check.** On a network minted flat, a
  demand of a tile of a flat digest can never be answered, and the sweep slashes the producer for
  withholding at the disclosure deadline — an honest producer, by its silence, at the hands of any
  bond holding the claim's binding.
* **The validator sign gate spelled the flat digest alone**, so the rail refused to sign every job
  committed under the Merkle form, on any network.

## 2. The requirement

testnet-11 can arm the held regime at a height by a one-line change the operator makes. At that
height every existing class computes, commits and is judged exactly as before; a held class
registered past it commits ids its seats, courts and tools all read in one form; nobody can be
slashed for failing to open a unit their commitment does not contain; and no node can read a
different identity, form or context from another for the same class at the same point.

## 3. Decisions

**Decision 1 — past genesis, the fence commits the V4 contexts itself.** `validate_palw_v2` asks
for the COMPLETE_V4 bundle root only where the regime is armed from genesis — where the bundle is
being minted, and `palw_held_context_mint_v1` states it. Armed at a height over a bundle minted
before the regime, the objects the regime verifies under V4's contexts (the checkpoint court and
the held DA court's accusations and disclosures) did not exist before the fence, so no signature's
meaning changes at it; and where the bundle names an older set, `consensus_params_id` writes the
V4 root beside the fence's height, so two builds that spell a V4 context differently cannot share
an identity. M-8's rule, kept by the fence rather than by the bundle. A bundle that states V4 —
every mint that takes the regime from genesis — commits the set through the ruleset id already and
is not written twice, so no held network minted before this ADR changes its identity (the ADR-0110
vectors' documents, which name their ruleset's id, are unmoved).

**Decision 2 — the one-move court takes the fence's contexts at the same height.** The shard
court's "the bundle covers V3" check is waived where the regime is armed past genesis at or below
the court's own height: V4 covers V3, and Decision 1 committed V4 there. The regime requires the
one-move court at or below its height, so on a network that armed neither, the two land on one
height.

**Decision 3 — the prompt-ids form is the class's.** `palw_prompt_ids_form_of_class_v1(network,
profile)` is the tiled Merkle root for a profile under the held map and the network's genesis form
for every other. It is never a function of a height — the reason the network's form is
genesis-only still holds — but of the class, whose map is inside its id. Every reader that decides
the form reads it through this function: the admission gate (the held arm, the fit, and the ladder
rules' cost shape), the three backends and the fp worker (which store their class's form when
handed the network's), the panel (`class_prompt_ids_form`, off the chain's registration row or
this build's tables, remembered per class because the answer is a function of the id), the held DA
court, and the node's per-class producer facts (`class_prompt_ids_merkle`, wire version 8 and gRPC
field 30) that a gateway re-binds a worker's result under. On a network minted Merkle every class's
form is Merkle, so nothing there moves.

**Decision 4 — a prompt tile is demanded only of a claim whose ids are a tile tree.** The held DA
court refuses a `PromptIdsTile` accusation unless the claim's class commits Merkle ids, by name
("this claim's prompt ids are committed flat, and a flat digest has no tiles"), at the point the
binding says which class it is. The other units — a state chunk, a run of step leaves, one leaf —
read no prompt id and are unchanged.

**Decision 5 — the carriers are the chain's.** The transaction's stateless check and the
extraction walk hold no class, so they read the network's genesis form as before, and the one
builder that mirrors them for a class it chose — the panel's canonical claim — passes the network's
form (a canonical payload carries no ids, so the form reads nothing there either way). On a network
minted flat a PublicDa carrier therefore cannot hold a held class's Merkle-committed ids: it is
refused at the door, identically on every node. A held class commits under `PanelDa`, which the
regime already requires for its widest job and whose carrier holds no ids and meets no form. The
gateway refuses a PublicDa request for such a class before the inference, by name, and says to
serve it with `--privacy panel-da`. The rail derives the form from the worker's result, as it did;
the sign gate now binds a result's ids under either form (the two digests are domain-separated, so
no list binds one job under both), and which form the class commits in is the chain's and the
seats' to hold the claim to.

**Decision 6 — testnet-11's flag day is one line, and it is the operator's: DAA 7,000.**
`PALW_RC_HELD_FENCE_DAA: Option<u64> = Some(7_000)`, and `palw_arm_held_regime_at_v1` arms the three
fences the regime takes together on testnet-11: `palw_held_context`, the one-move court and the
retention pins (the k-ary court, `PanelDa` and the DA court are already armed there, from genesis
and at 1,900). Every fence at one height ships in one build — `fork_id_v1` digests the fired
heights, not the fence set — so the height is one no other schedule entry uses, or one whose other
fences ship in the same release: not 3,500, and not 4,000, which releases the audit and
`palw_prefill_draw` without the regime. 7,000 is shared with the deep-audit fence
(`palw_audit_2026_09_11_deep`), so no build may carry one of the two without the other.

**Decision 7 — a chain-registered class is served at the ruleset's ladder.** Every tabled lineage
builds its backend with `with_step_ladder_cap(court.max_step_leaf_count())`; the SDK's chain arm
(`resolve_chain_registered`) did not, and left both families at the executor's `2^22` default. A
held class reaches a network like testnet-11 only by registration, and a dense row's job passes
`2^22` leaves after a few dozen positions, so its own producer and seats refused it, and its
retained blocks were addressed at a level the seats did not derive. The chain arm now takes the
ruleset's ladder, as the tables do. Node policy; no rule moves.

## 4. What arming buys on testnet-11, measured

**Less than its name.** The held map lifts the context ceiling; it does not lift the step ladder
or the advertised prompt cap, and both are testnet-11's genesis's: the ladder is `2^26` because a
bisection opened before any fence must still be playable inside the court window (ADR-0103,
`a_ladder_past_the_clock_needs_the_regime_from_genesis`), and the free-prompt cap is 512. With the
regime armed at 7,000, the admission gate reads the dense lineage's held row (graph-v7) as:

| context | at testnet-11's ladder (`2^26`) | at the regime's ladder (`2^48`, not mintable on a live chain) |
|---|---|---|
| 512 | admitted | admitted |
| 1,024 | refused: the whole context as prefill is past `2^26`, and the close's order cannot be read | admitted |
| 32,768 and 2^21 | refused before the gate: the canonical job alone is 67,161,216 leaves | admitted |

So on testnet-11 the regime delivers its courts and its objects, and a held class up to about 512
positions that produces attempts and canonical claims — not the 2^21 ADR-0116 widened the history
for. A held class's USER prompts have no executor path there yet either: the chain refuses a
PublicDa carrier of its Merkle-committed ids (Decision 5), and the fp worker executes PublicDa jobs
only (`precheck_request_v1`), so the gateway has nothing to hand a PanelDa job to. This corrects the report of
2026-09-11, which said the 2^21 bound "changes nothing until the regime is armed" as if arming
were the only step. What would make 2^21 real there is the same move this ADR makes for the prompt
form, made for the ladder and the prompt cap: a held class exists only past the fence, where no
bisection is opened, so the bisection's clock — the only reason the ladder is frozen — never
binds a held class. That is §7's first question, not a decision taken here.

## 5. Invariants the tests hold

1. **I-1, testnet-11 takes the regime at a height of its own**: the preset is the helper at
   7,000; it validates with the network's ids still flat, gains its schedule entry and moves the
   identity from the build before it; armed from genesis over the same bundle it is refused
   (`testnet_11_takes_the_held_regime_at_a_height_of_its_own`), and armed at a scheduled height it
   is invisible to the fork-id gate (`a_held_fence_at_a_scheduled_height_is_invisible_to_the_fork_id_gate`).
   The fork-id gate, the schedule and the fingerprint pins name 7,000 (`fork_id_v1`'s measured
   schedules, `shipped_presets_have_pinned_fingerprints`).
2. **I-2, the form is the class's**: the held row is priced Merkle and admitted under a court
   reading the flat form, and the shipped row keeps the flat form byte for byte
   (`on_a_network_minted_flat_a_held_class_is_priced_at_its_own_merkle_form_and_admitted`); a
   backend handed the flat form keeps its class's and derives its jobs under it
   (`a_backend_keeps_its_classs_prompt_ids_form_whatever_the_networks`).
3. **I-3, a tile is demanded only of a tile tree**
   (`a_prompt_tile_is_accusable_only_where_the_claims_class_commits_a_tile_tree`).
4. **I-4, the fence writes the V4 root beside its height** in `consensus_params_id`
   (`the_held_fence_writes_the_v4_contexts_beside_its_height`), and the genesis rules are
   unchanged from genesis (`the_held_context_fence_is_dormant_and_arms_only_over_v4_with_its_courts_and_tiled_ids`).
5. **I-5, the carriers**: a held class on a flat network is refused PublicDa by name and served
   under PanelDa (`a_held_class_on_a_flat_network_commits_under_panel_da_and_is_refused_public_da_by_name`);
   the class bit crosses both wires and an older node reads as its network's form
   (`the_producer_facts_survive_the_borsh_round_trip`, the gRPC round trip); the sign gate signs a
   Merkle job and binds only its own ids (`a_merkle_committed_job_is_signable_and_binds_only_its_own_ids`).
6. **I-6, the limitation, pinned** (§4's table,
   `what_the_regime_admits_on_testnet_11_is_bounded_by_its_frozen_ladder_and_prompt_cap`): the day
   a held class's ladder moves, this test says so.

## 6. Supersession

| what | by |
|---|---|
| `validate_palw_v2`: the regime requires the COMPLETE_V4 root and Merkle ids at its fence | from genesis only (Decisions 1, 3) |
| the shard court's V3 check under a late held fence | waived at or below the fence (Decision 2) |
| ADR-0081 D3 / the payload decoders' doc: "the commitment's form is the network's" | the class's, the network's for every class but a held one (Decision 3) |
| ADR-0103's suite: a held class under a flat court is refused `LinearInTheContext` on the close | admitted, priced at its own Merkle ids (Decision 3) |
| `palw_fp_sign_gate`: the flat digest alone | either form (Decision 5) |
| `GetPalwProducerFactsResponse` version 7 | version 8, `class_prompt_ids_merkle` (Decision 3) |

## 7. What is deliberately not decided

* **A held class's ladder and prompt cap on a live network** (§4). The per-class argument holds
  for both, but the ladder is read by some ninety sites — the court's refutation walkers, the held
  DA court, the stateless work-leaves cap the extraction walk applies with no class in hand, the
  producer's and the seats' caps — and the cap by the stateless ruleset check. The extraction walk
  would need either a height-wide widening past the fence (every class's work is still bounded by
  its own registration, but that is a claim to prove, not assume) or a held bit in the class state.
  It is the next ADR if the operator wants 2^21 on testnet-11 rather than on a network minted with
  the regime.
* **The other walls between a held class and 2^21**, found by the same survey (2026-09-12). Each is
  a bound a network minted with the regime meets as well, because ADR-0110's vectors run the
  stages in process at a tiny geometry and never reached them:
  - the transaction door's structural work-leaves cap, `PALW_FP_STRUCTURAL_WORK_LEAVES_CAP = 2^32`
    — with no class, no state and no height, and failing it invalidates the block — against about
    `2^38` leaves for the dense 1.5B row at 2^21 positions;
  - the worker's result frame (256 KiB), which carries the prompt ids whole: about 60,000 ids;
  - the fp worker's PublicDa-only precheck, above;
  - the held dissection opens its bisection ladder at the ladder's size, and `PalwBisectLadderV1`
    refuses a space past `2^40`, so at the held mint's `2^48` a fused held leaf is not prosecutable;
  - the fused dissection's bottom and root claim open rows under the structural
    `PALW_STEP_MAX_LEAVES` (`2^22`), not the ruleset's ladder — a bound testnet-11's own fused rows
    meet today, since the court's ladder there is `2^26` (reported to the deep-audit session, whose
    court findings ride 7,000; not decided here).
* **The dense-court fences on testnet-11.** `palw_fused_dissectable` and `palw_attn_anchored_root`
  (the audit's AC-D8) stay dormant there, so a held class with fused attention is dissected under
  the same rules testnet-11's shipped fused row is. The regime does not require them, and arming
  them is the mainnet card's correction, not this ADR's.
* **A node without the registration row.** The panel and the node's facts read the class's form
  off the chain's registration row (ADR-0067's index), with this build's tables as the panel's
  second source; a pruned-synced node adopts the row it lacks (ADR-0067 Decision 6) before it can
  serve the class at all.

## 8. Number hygiene

Written as 0118 on `feat/adr-0103-held-context` on 2026-09-12, after `origin/main` (`d080154e`)
and every local branch were listed: none holds 0118 or later. **The next free number is 0119.**

## 9. Implementation record (2026-09-12)

| Decision | where | what pins it |
|---|---|---|
| **1** the contexts | `Params::validate_palw_v2` (the held block), `Params::consensus_params_id` | I-1, I-4 |
| **2** the one-move court | `Params::validate_palw_v2` (the shard court's contexts) | I-1 |
| **3** the form | `palw_prompt_ids_v1::palw_prompt_ids_form_of_class_v1`; `palw_class_admission_v2` (held arm), `palw_model_fit_v1::palw_model_fit_v2`, `palw_context_ladder::palw_class_ladder_rules_for_court_v1`; `with_prompt_ids_form` in `backend.rs`, `qwen25_a16_backend.rs`, `qwen36_backend.rs`; `FpWorkerRuntime::new`; `PalwPanelService::{class_prompt_ids_form, payload_prompt_ids_form}` and `palw_fp_class_id_peek_v1`; `rpc/service` `get_palw_producer_facts_call`; `ChainFacts::prompt_ids_form` | I-2, I-5 |
| **4** the tile | `palw_held_da_v1::palw_held_da_check_accusation_v1`; `PalwTransitionExtrasV1::prompt_ids_merkle` for the fold | I-3 |
| **5** the carriers | `privacy_mode_for_request`, `ChainFacts::public_ids_cannot_ride`; `palw_fp_sign_gate::signable_claim_id` | I-5 |
| **6** the flag day | `PALW_RC_HELD_FENCE_DAA`, `palw_arm_held_regime_at_v1`, `palw_rc_base_params` | I-1, I-6 |
| **7** the chain arm's ladder | `PalwClassSdk::resolve_chain_registered` | `the_chain_arm_applies_the_rulesets_ladder_like_every_lineage` |

The suites, on the branch's head: the whole workspace under nextest, 4,665 tests, all passing —
with the AC-SLOT fixture generated first (`AC_SLOT_FIXTURE`, which CI does not set yet; two tests of
the base read it) and with the base's own repairs in the two commits before this one (a lost
`#[allow]`, two literals and a one-element loop clippy refuses, fmt drift in five files, and the extension's
missing arm for `palw_share_growth_final`). clippy (`--tests --benches --examples -D warnings`),
fmt, the PQ guard, doctests and the docs are green. The ADR-0103 held suite is 11 tests and this
ADR's own 5.
