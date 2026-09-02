# ADR index — what governs, and what was reversed

One list, maintained. Every ADR that has been superseded, amended or withdrawn — in whole or in
part — is named here together with the ADR (or the measurement) that moved it and the clause that
actually moved. If a decision you are about to rely on is not in the "still governing" section,
follow the arrow before you build against it. `docs/kaspa-pq-spec.md` §12 is the PQ-phase snapshot
(ADR-0001–0015) and is not extended; this file is the index.

The hazard this file exists to stop has now happened six times on this repo: a number reused by a
non-ancestral lineage (0039–0048), two same-day ADRs claiming one number (0035, 0045, 0054/0055,
0059, 0069), and an ADR cited by code and by ADR-0052 that was never resident on `main` (0047).
Reading by number alone gives you the wrong decision. The "Number hygiene" table below is the
authoritative map.

Last reconciled: **2026-09-02**, against `main` at the Relaunch 5e stack (ADR-0075 + ADR-0076),
including ADR-0077–0079, whose texts landed here in the same pass. Every ADR whose Status line no longer describes the shipped
state carries a blockquote banner dated 2026-09-02 under its header, pointing back here, and every
unimplemented decision that was weak against an adversary carries a dated **Security amendment**
section at its end (the "Security amendments" section below is the list). ADR bodies are otherwise
never rewritten: a reversed decision stays in the file, labelled, so the reasoning that reached it
can be found again.

## The direction that governs the PALW lineage (2026-09-02)

**PALW is the consensus work, and PALW produces the blocks.** The load-bearing chain is:

ADR-0038 (layer inversion) → ADR-0039 (no hash production path; `PALW-BASE-0` is the floor;
two-weight fork choice) → ADR-0042 (one atomic ruleset, one fingerprint, RC == mainnet by tag) →
ADR-0043 / ADR-0045 / ADR-0046 (state-root ordering; class economy as chain state; consensus-object
carriage) → ADR-0053 (one execution family) → ADR-0054 / ADR-0056 / ADR-0058 (share follows
production; permissionless admission; merged work is counted) → ADR-0059 / ADR-0061 / ADR-0065
(10B cap; zero-seat genesis and 10,000 MSK collateral; a bond is earned, a failure is not a
verdict) → ADR-0060 / ADR-0066 (the liveness doctrine; the heartbeat clock out of `header.bits`) →
ADR-0067 (classes are chain data, kernels are the build) → ADR-0068 (the LLM-primary economy) →
ADR-0069 / ADR-0070 (weight is the price of end-to-end adjudicability; the model tiers are
adjudicable) → ADR-0072 (+ Decision 8) / ADR-0074 (the ticket is the execution; the attempt is a
claim drawn by the chain's beacon) → ADR-0073 (real-demand work bears the weight — Phases ① and ③
landed, ② and ④ open) → ADR-0075 (certification is a consensus object) → ADR-0076 (per-class
attempt-lane seed) → *on branches, PROPOSED:* ADR-0077 (a prompt a person would type is a claim the
court can try — one runtime, checkpoint-priced court, the 512-token rows), ADR-0078 (what was made
from it is committed; the thing itself never rides — derived artifacts), ADR-0079 (a pure function
needs no permissions; the sandbox is for the host — no security field ever enters the priced bytes).

What that chain means today, in the sentences people most often get wrong:

* **There is no hash block-production lane and no hash floor to degrade to.** ADR-0036 D4's
  permanent floor was reversed by ADR-0039; ADR-0038 W4/W6's anti-stall hash floor by ADR-0039
  W4′/W6′. The floor is a *class* (`PALW-BASE-0`), permanently Active, exempt from the epoch budget
  (W6′), and under ADR-0068 it retires to the doctrine's minimum share (20‰ reserve) — never
  withdrawn.
* **The one hash lane that does exist is a clock, not a producer.** ADR-0060 D1/D2 added a bondless
  heartbeat lane so a totally stopped chain can restart without a bonded actor; the 2026-08-30
  audit found its price in `header.bits` self-perpetuating and ADR-0066 re-seated it (`algo_id = 8`,
  fixed target, fee-only, no mint). It sits behind a top-level fence (`Params::palw_heartbeat`) that
  ADR-0068 Phase 2 arms **from genesis** on testnet-11 (Relaunch 5 onward) and devnet, and that a
  carded mainnet would arm the same way; the ADR-0060/0066/0068 Status lines that say it "ships OFF"
  describe the Phase 1 build. A heartbeat block carries ε = 1 blue work against 2²⁰ for an attempt
  block (`Params::palw_attempt_work`), so it can never out-weigh bonded production.
* **Certification, not the build, decides who bears weight.** ADR-0069 made end-to-end
  adjudicability the price of weight (an uncertified family registers at share 0 and cannot grow);
  ADR-0070 proved the model tiers' step spaces adjudicable; ADR-0075 moved the certified set from
  the binary into chain state (`FamilyCertified`, `ClassLaneCertified`, read as genesis ∪ chain), so
  a new model is admitted weightless and seated by objects, not by a fingerprint move.
* **The lottery is priced in inferences.** Under ADR-0072 both attempt-lane draws are functions of
  the execution commitment; the nonce is a uniqueness field; every field inside the priced bytes is
  pinned (Decision 8). ADR-0074 draws the attempt from the chain's own beacon — a walk over blocks
  the node already holds, never an attestation, quorum, validator set or finality overlay. ADR-0076
  seeds each class's target from its own share and its own work (`MAX · share · pwu_per_inference /
  2³¹`). Block interval is `header.bits`' job — ADR-0071 Decision 1's target freeze was withdrawn
  after Relaunch 5 measured it.
* **Genesis is fixed; rules ship by activation — on mainnet.** The standing doctrine (2026-08-27,
  recorded in ADR-0059, 0067, 0069, 0071, 0072) is that consensus changes ship by activation or
  version, never by re-genesis, because mainnet cannot re-mint. testnet-11 has nonetheless been
  re-minted as Relaunch 2, 3, 4, 5, 5c, 5d and 5e: on the RC lineage a ruleset change moves the
  fingerprint, a moved fingerprint refuses un-wiped peers at handshake, and ADR-0042 §"The
  two-network split" already says a public RC that changes any rule is RC(n+1), never continued.
  ADR-0072 §3 records the activation shape mainnet will use instead (new algo id + DAA fence, old
  arm kept for history; the fork-id handshake that would let a fingerprint stay put is a later ADR).
  ADR-0059's supply shape and ADR-0076's genesis seeds are the two things only a genesis can express.
* **Mainnet is born floor-only with PALW disabled, and every model arrives later.** The mainnet
  preset ships `PalwConsensusMode::Disabled` (its genesis card is unset); when carded it assembles
  the RC's ruleset (ADR-0042 D11) with the same fences armed from genesis (ADR-0068 Phase 2). A
  model reaches weight through registration → family drill → `ClassLaneCertified`, seated at the
  floor share and re-seeded (ADR-0075 Decision 8, ADR-0076 §4 and Decision 4).
* **Finality is an overlay.** VLT (ADR-0024, dormant) is a finality overlay on top of PALW block
  production, not a consensus; the liveness doctrine (ADR-0060 D4 → ADR-0066's fenced inactivity
  leak, still `None` everywhere) exists so it cannot hold the clock hostage. The beacon (ADR-0074)
  is forbidden from reading it.

## Superseded, in whole or in part

| ADR | What moved | Superseded / amended by |
| --- | --- | --- |
| 0002 — ML-DSA-65 P2PKH | the signature scheme (the P2PKH structure stands) | [0019](0019-mldsa87-migration.md) |
| 0009 — DNS probabilistic finality | the voting-weight half | [0024](0024-verified-llm-token-weighted-bft.md) |
| 0012 — validator sortition commit-reveal | **the whole ADR** | [0017](0017-all-active-staker-attestation.md) |
| 0017 / 0018 §B | the voting-weight half | [0024](0024-verified-llm-token-weighted-bft.md) |
| 0021 — PALW LLM PoW (`algo_id = 4`) | reward / PoW **activation**; the Ollama output-text commitment was shown forgeable. The *lottery shape* is kept by 0038; the *verification shape* is replaced | [0026](0026-palw-v2-runtime-separated-verification.md), [0038](0038-palw-is-the-consensus-work.md) |
| 0026 / 0027 / 0028 | not reversed — **promoted**: the court, fraud proofs and sampling stop being credit machinery and become L1 machinery. 0026's thesis was walked back for one family by 0051 and **restored in full** by 0053 | [0038](0038-palw-is-the-consensus-work.md), [0053](0053-palw-one-execution-family.md) |
| 0028 §4e — the credit-price remedy set (Remedy 1 rate cap, Remedy 2 subsidy fraction) | Remedy 1 was already recorded as non-existent at this panel; Remedy 2's variable — a subsidy fraction paid to an overlay job — has no referent once the block *is* the unit of credit. The admission quantity moves to a block-denominated per-class epoch budget; the `max_leverage` half moves to a per-**bond** exposure reserve | [0045](0045-palw-class-economy-on-chain.md) D2 (admission), [0042](0042-palw-mainnet-candidate-ruleset.md) D6 / P0-10 (leverage) |
| 0029 — V1 chain carriage | the Stage-1 shape is reused; the V2 object set replaces it | [0046](0046-palw-v2-consensus-object-carriage.md) |
| 0032 / 0033 — fee-bond escrow, the credit gate | not reversed, **dormant**: the credit-overlay lineage's value flows. On the V2 lineage the block is the unit of credit; escrow, void and slash are ADR-0042 D6/D10 and the carriage's fee-as-rent (0046, 0075 D1) | — |
| 0035 D1 — "testnet-11 is the *current chain*, continued; no re-genesis at announce" | held for Relaunch 1. The RC rule re-genesises a public RC on any rule change, and testnet-11 has been re-minted as Relaunch 2–5e; the algo-4 `LegacyTn11` lane is not running anywhere. D2 (class admission pinned in code) stands | [0042](0042-palw-mainnet-candidate-ruleset.md) §"The two-network split" |
| 0036 D2 — backup-lineage ADR-0041's mechanism (`palw_spam` / `palw_algo4_accept` / `palw_compute_work_scale` / qwen-8.0 `mint.rs`) | **not adopted**, not ported. Two of that ADR's *conclusions* are adopted (new network identity; land→accept→mint) | — (0036 is itself the superseding record) |
| 0036 D4 — "mainnet MUST ship the permanent hash floor" | reversed: a lane that can always produce blocks is a permanent incentive to mine the lane instead of the work. The testnet half of D4 (no floor on TN11/devnet; a loud halt beats a silent fork) survives verbatim — and is then *refined* by 0060: a bounded, near-weightless, fee-only clock lane is not a production floor | [0039](0039-palw-only-block-production.md) D1/D2 (W6′); [0060](0060-the-liveness-doctrine.md) / [0066](0066-the-heartbeat-lane-out-of-header-bits-and-a-committed-liveness-table.md) |
| 0037 D1 — PALW off the block-critical path (async model) | reversed the same day by the layer inversion. D2–D9 (the state machine and mint hygiene) are **carried**, re-seated under the new layer assignment | [0038](0038-palw-is-the-consensus-work.md) |
| 0038 W4, W6 | the last two hash paths to consensus participation: the anti-stall floor as a block-production path, and `spam_hash_work` as a fork-choice term | [0039](0039-palw-only-block-production.md) W4′, W6′ |
| 0038 "pure-PALW production" as *no hash lane at all* | amended: a clock lane re-enters, bounded and near-weightless, as the chain's clock and nothing else | [0060](0060-the-liveness-doctrine.md) (doctrine), [0066](0066-the-heartbeat-lane-out-of-header-bits-and-a-committed-liveness-table.md) (form), [0068](0068-the-llm-primary-economy-and-the-floors-minimum.md) (armed) |
| 0039 D5 + its 2026-08-17 amendment | the epoch cap's **currency**: the amendment elected `pwu` and then proved in defect (e) that the election starves every above-mean class. Denominating in **blocks** closes (a)–(e) and unblocks the enforcement point that defect (c) had left open | [0045](0045-palw-class-economy-on-chain.md) D2 |
| 0039 D5 — share as a params constant | shares become chain state, granted at registration, conserved to 1000‰; they then follow production and may be zero for an uncertified family | [0045](0045-palw-class-economy-on-chain.md) D3, [0054](0054-palw-share-follows-production.md), [0069](0069-e2e-adjudicability-is-the-price-of-weight.md) |
| 0041 (live) D1 — sampled pruning-proof verification | withdrawn as unsound; verification is exhaustive and amortised | [0041](0041-palw-pruning-proof-verification.md) D1′ itself |
| 0044 D4 — "the receipt lane is weightless" | amended: after 0073 Decisions 1 and 3 it is not (weight activation still gated — Phase ②) | [0073](0073-real-demand-work-bears-the-weight.md) |
| 0044 D4 — "only attempt blocks carry randomness" (the beacon rule) | kept as law, and the attempt lane itself now draws from the beacon | [0074](0074-the-attempt-is-a-claim-drawn-by-the-chain.md) D2 |
| 0044 D6 — the receipt block | extended: chain position is earned and the question is set by the block (`PALW_ATTEMPT_V2_VERSION` 4 → 5) | [0055](0055-palw-position-is-earned-not-declared.md) |
| 0044 D7 — CU pricing (`fp_cu_v3`, `QUANTUM_CU`, `PWU_PER_QUANTUM`) | **withdrawn**: the unit is step leaves; a quantum is an eighth of the class's canonical job | [0074](0074-the-attempt-is-a-claim-drawn-by-the-chain.md) D5 |
| 0044 — the certified free-prompt set lives in the build | it is chain state (genesis ∪ chain) | [0075](0075-certification-is-a-consensus-object.md) |
| 0045 D3 — "automatic share re-allocation from class health", deferred | written: share follows production | [0054](0054-palw-share-follows-production.md) D1 |
| 0045 D1 — `DerivedV1`'s expected-attempts term | drawn once per inference (the 2²² nonce sweep is gone); `pwu_per_inference` also prices the class's attempt-lane seed | [0072](0072-the-ticket-is-the-execution.md) D5, [0076](0076-the-attempt-lanes-seed-is-the-retargets-equilibrium.md) |
| 0049 D-C — court-cost ceilings "shipped as three fields, gated nowhere" | amended: `max_close_bytes = 80 KiB`, checked by the RC identity gate; the metric changed | [0049](0049-palw-adjudication-contract.md) §Amendment (2026-08-26) |
| 0049 D-H — a registrant takes the minimum grantable share | refined twice: an uncertified family takes **0** (weight is what certification buys); a certified entrant is seated by a chain object, not by a re-genesis | [0069](0069-e2e-adjudicability-is-the-price-of-weight.md) D5/D6, [0075](0075-certification-is-a-consensus-object.md) D5 |
| 0051 — the Metal/GGUF execution family (Family M) | **the whole ADR**: two families, a tolerant proof model, per-class panels, no court. Its three safety boundaries were never implemented and 0052 removed its motive | [0053](0053-palw-one-execution-family.md) |
| 0052 — "a Qwen3.6 class … must not carry weight" (pre-amendment) | the same-day amendment decided calibration and the court; the class is adjudicable end to end and weight-bearing when certified | [0052](0052-palw-qwen36-hybrid-class.md) §Amendment, [0070](0070-the-model-tiers-step-spaces-are-adjudicable.md), [0069](0069-e2e-adjudicability-is-the-price-of-weight.md) |
| 0053 — "a genesis registration carries `admission: None` because the ruleset id commits to the catalog" | not reversed, **re-scoped**: the registered profile becomes the authority and the interpreter runs from it (classes are chain data; only kernels are the build). The genesis path stands and 0067's chain-class arming is still PROPOSED, so today's testnet-11 classes are genesis rows | [0067](0067-classes-are-chain-data-kernels-are-the-build.md) |
| 0054 D2 — floor reserve `base_class_reserve_permille = 500‰`; 0056's `min_base_class_share_permille = 300‰` | the floor retires to 20‰ (Relaunch 5) | [0068](0068-the-llm-primary-economy-and-the-floors-minimum.md) Phase 2 |
| 0056 D4 — the streak-based share walk | **withdrawn** at the merge in favour of one share rule | [0054](0054-palw-share-follows-production.md) D1 |
| 0056 item 7 — "the mid-epoch budget defect is also closed" | contradicted by 0053 (the fix was reverted on `main`) and by the shipped transition (`ensure_epoch_budgets`): a mid-epoch entrant has no attempt-lane budget until the next boundary — a known gap kept for state-root compatibility | [0053](0053-palw-one-execution-family.md) §Context |
| 0059 item 4 — genesis bonds bond 0.1B each | 10,000 MSK a seat, sized by arithmetic | [0061](0061-zero-seat-genesis-and-right-sized-collateral.md) D2 |
| 0060 D1/D2 — heartbeat lane on `algo_id = 3` with its own windowed retarget; D2's timestamp ramp; D4's leak | the 2026-08-30 audit found the price in `header.bits` self-perpetuating and the evidence walk node-relative; re-implemented: `algo_id = 8`, fixed target, one-block-deep slot rule, top-level fences. The *doctrine* is unaffected | [0066](0066-the-heartbeat-lane-out-of-header-bits-and-a-committed-liveness-table.md) |
| 0060 / 0066 / 0068 Status — "ships OFF" | the heartbeat lane and the attempt-work constant are armed **from genesis** on testnet-11 (Relaunch 5 onward) and devnet; only the inactivity-leak fence stays `None` | [0068](0068-the-llm-primary-economy-and-the-floors-minimum.md) Phase 2 (`palw_rc_arm_phase1`) |
| 0061 audit amendment — "the bootstrap is not yet" | the Phase 1 drill bootstrapped a zero-bond devnet over heartbeats; testnet-11 ships eight seats since Relaunch 4 so 0065 D1 can be armed | [0068](0068-the-llm-primary-economy-and-the-floors-minimum.md), [0065](0065-a-bond-must-be-earned-and-a-seat-must-be-someone-else.md) |
| 0062 — the data-availability court | not reversed, **still Proposed**; its motivating harm (an `Unavailable` quorum taking a bond) is removed on armed presets by 0065 D4 | [0065](0065-a-bond-must-be-earned-and-a-seat-must-be-someone-else.md) D4 |
| 0064 — the earlier bond lookup as recovery from a total stop | self-corrected: it does not close the deadlock (a block's body is never in its own mergeset). Facts A (silence is not checkable) and B (a zero-weight lane hands fork choice to the hash) stand. The open item is answered on armed presets by the clock lane | [0064](0064-trustless-recovery-from-a-total-stop.md) §Correction, [0066](0066-the-heartbeat-lane-out-of-header-bits-and-a-committed-liveness-table.md), [0068](0068-the-llm-primary-economy-and-the-floors-minimum.md) |
| 0065 D2 (frontier provenance as written) | unimplementable — a fork-point-dependent value in `state_root` splits the chain; restated as D2a (a comparison-site rule at the deep-reorg gate); D2b withdrawn | [0065](0065-a-bond-must-be-earned-and-a-seat-must-be-someone-else.md) itself |
| 0065 D3 — "a seat must be someone else" | **withdrawn**: the dedup exists; "distinct operator" is not a checkable predicate (Fact A) | [0065](0065-a-bond-must-be-earned-and-a-seat-must-be-someone-else.md) itself |
| 0066 D3 — attempt blue work leaves `calc_work(bits)`; F3a — sibling heartbeat width | closed: a 2²⁰ constant behind `Params::palw_attempt_work`; at most four heartbeats per mergeset or one chain | [0068](0068-the-llm-primary-economy-and-the-floors-minimum.md) F2, F3a/F5 |
| 0068 Phase 0 — "walk the table via 0054 on the running chain" | corrected in-text: the live reserve was 500‰, so the walk could never pass the half-table; Phase 2's re-mint was the only route | [0068](0068-the-llm-primary-economy-and-the-floors-minimum.md) Status |
| 0069 D2 / D5 / D6 — the certified set is the build's (`court_e2e_root`, pinned) | the set the weight gate reads is genesis ∪ chain; a weightless entrant is seated by an object, not by a re-genesis; a certificate is scoped to the lane it was drilled on | [0075](0075-certification-is-a-consensus-object.md), [0073](0073-real-demand-work-bears-the-weight.md) |
| 0069 §1 — "no uncertified family carries fork-choice weight" | amended on review to **cadence**: a weightless class's one floor block per epoch still adds pwu; pricing it at zero is an undone fork-choice change | [0069](0069-e2e-adjudicability-is-the-price-of-weight.md) review notes |
| 0071 D1 — the attempt lane's price frozen off `header.bits` | **withdrawn after Relaunch 5 measured it**: freezing the target removed the only control on block interval (41–54 blocks/min against 0.5). `bits` keeps block interval; the per-class retarget only redistributes share | [0071](0071-the-attempt-lanes-price-and-the-tickets-bound.md) §3 |
| 0071 D1a (first draft) — an absolute expectation `share × DAA span` | rejected at implementation; the standing rule is a ceiling: an idle class converges toward the producing classes' price, never past it (`converge_idle_target_v1`) | [0071](0071-the-attempt-lanes-price-and-the-tickets-bound.md) D1a |
| 0071 D2 — `pwu = 2^k × per_inference` (the bucket's work) | withdrawn: `pwu = max(1, expected_draws) × per_inference`; the bucket `k = 22` stands only as the anchor's position field | [0072](0072-the-ticket-is-the-execution.md) D5 |
| 0071 §5 — "a declared seat that cannot serve is convicted" | false as written (0065 D4 makes `Unavailable` an abstention); a false capability declaration is not yet priced — recorded open | [0071](0071-the-attempt-lanes-price-and-the-tickets-bound.md) itself |
| 0072 header — "a version bump and a coordinated upgrade, not a re-mint" | D7 decided the same day: the rule went live inside Relaunch 5's re-genesis, no fence; the activation shape is recorded for mainnet | [0072](0072-the-ticket-is-the-execution.md) §3 |
| 0072 §3 — "the attempt lane is still the canonical-job lane" | the canonical job is a claim too, drawn by the beacon; real demand is the primary lane by design (weight activation gated) | [0074](0074-the-attempt-is-a-claim-drawn-by-the-chain.md), [0073](0073-real-demand-work-bears-the-weight.md) |
| 0073 D3 — one unit, quantum = leaf count | as landed: quantum = `max(1, canonical_leaves / 8)`; the lottery discipline is the beacon draw | [0074](0074-the-attempt-is-a-claim-drawn-by-the-chain.md) D5, D2 |
| 0073 D6 / 0074 D6 — the free-prompt-certified set is the build's | the set has a chain half; a weightless entrant is seated by `ClassLaneCertified` | [0075](0075-certification-is-a-consensus-object.md) |
| 0075 §7 — the mainnet card names model-root constants | the assembly can pin tiers, but the decided route is floor-only at genesis with every model arriving by registration → drill → binding | [0076](0076-the-attempt-lanes-seed-is-the-retargets-equilibrium.md) §4, [0075](0075-certification-is-a-consensus-object.md) D8 |

Two labels to read with care: ADR-0076 cites "ADR-0071 Decision 1 as amended" — the standing rule
it means is 0071 **Decision 1a** (`converge_idle_target_v1`); Decision 1 proper is withdrawn.
ADR-0071 §5 says 0065 D4 is "armed on every shipped preset" — it is armed on testnet-11 from
genesis and is `None` on devnet and mainnet.

## Number hygiene — collisions, renumberings and dangling references

| Number | What happened |
| --- | --- |
| 0003, 0008 | ADR-0019 links `0003-pq-address-format.md` and `0008-pq-genesis-premine.md`. Neither is resident; the numbers are held by [LtHash UTXO accumulator](0003-lthash-utxo-accumulator.md) and [Hash64 consensus identity](0008-hash64-consensus-identity.md) — different decisions. De-linked in place. |
| 0021, 0022 | ADR-0023 links `0021-fact-settlement-layer.md` and `0022-fsl-economic-design.md`. Neither is resident; the numbers are held by [PALW LLM PoW](0021-palw-llm-pow.md) and [pruned IBD / EVM overlay snapshot](0022-pruned-ibd-evm-overlay-snapshot.md). De-linked in place. |
| 0035 | Drafted twice the same day. The mainnet-activation draft was renumbered to [0036](0036-palw-mainnet-activation-model.md); [0035](0035-palw-public-testnet-strategy.md) is the public-testnet decision. |
| 0039–0048 | Held by the non-ancestral `main-backup-8107bfb-20260807` snapshot (`48e3e05f`: snapshot auth, sibling-flood bounding, nullifier prune, PCPB fraud audit, economic calibration, model-genesis dual reproduction, header v4 staging). Released to the live lineage by [ADR-0036](0036-palw-mainnet-activation-model.md) Decision 1; the live 0039–0047 are unrelated to the snapshot's. In particular the live [0041](0041-palw-pruning-proof-verification.md) (pruning-proof verification) is **not** the ADR-0041 that ADR-0036 supersedes. |
| 0045 | Drafted twice the same day on two branches — the class economy and the V2 consensus-object carriage. The class economy keeps [0045](0045-palw-class-economy-on-chain.md); the carriage ADR is [0046](0046-palw-v2-consensus-object-carriage.md). Code, runbooks and later ADRs (0049, 0060, 0062) already cite the carriage as **ADR-0046**; `main` carried the file under 0045 until 2026-09-02, when this rename reached it. `docs/evidence/palw-rc-launch-blockers-2026-08-21-findings.json` records the old name as a dated audit artifact and is left as is. |
| 0047 | [The A16 activation tier](0047-palw-a16-activation-tier.md) was written on `palw-mainnet-rc-integration` (`d3880149`, 2026-08-21) and never merged into `main`'s `docs/adr/`, while ADR-0052, `palw_base0_a16.rs`, `engine_a16.rs`, `artifact.rs` and two design docs cite it by number. Restored verbatim 2026-09-02 with a residency note; ADR-0040's amendment trailer that came with it is restored too. |
| 0048 | Unused on the live lineage (the snapshot's `0048-header-v4-staging-mainnet.md` was never released here). |
| 0077, 0078, 0079 | Written 2026-09-02 on three sessions' own branches and **landed here the same day**, texts and security amendments together: [0077](0077-a-prompt-a-person-would-type-is-a-claim-the-court-can-try.md) (`70084763`…`d3a6190b`), [0078](0078-what-was-made-from-it-is-committed-the-thing-never-rides.md) (`4047e86c`, `bb14b63a`), [0079](0079-a-pure-function-needs-no-permissions-the-sandbox-is-for-the-host.md) (`9f97db97`). Only the ADR text is here; the implementation branches (`palw-adr0078-impl` and the `palw-adr0077-*` set) carry code that is theirs to land. `palw-adr0078-impl` also edits ADR-0078's §6 as it goes, in a different region from the security amendment appended at the end of that file, and merging it into this `main` was measured clean on the day this row was written. Each ADR's own §"Number hygiene" says a concurrent claimant renumbers the later writer; the numbers are now resident, so **the next free number is 0080.** |
| 0054 / 0055 | Two same-day pairs collided at the `palw-share-follows-production` merge (`c6fba1f8`, 2026-08-27). "Share follows production" kept [0054](0054-palw-share-follows-production.md) and "chain position is earned" kept [0055](0055-palw-position-is-earned-not-declared.md); the permissionless-admission ADR (authored as 0054, `81a968bb`) became [0056](0056-palw-permissionless-class-admission-and-share-economy.md) and the runtime-acceleration ADR (authored as 0055, `7d907ebe`) became [0057](0057-palw-base0-runtime-acceleration.md). Commit messages `docs(adr-0054)` / `ADR-0055 — runtime acceleration` and the comment "until ADR-0055 the one still running the scalar" in `misaka-palw-base0/src/kernels.rs` refer to the pre-merge numbers. |
| 0059 / 0060 | The data-availability court was authored as 0059 (`554ca77c`) and became [0062](0062-data-availability-court.md); the operator-tooling ADR was authored as 0060 and became [0063](0063-operator-tooling-the-missing-half.md) — both at the bond-economics merge (`3ae74676`, 2026-08-30). [0059](0059-the-10b-premine-cap.md) is the premine cap and [0060](0060-the-liveness-doctrine.md) the liveness doctrine. Every `ADR-0059` citation in code and runbooks means the premine cap. |
| 0069 | The step-space ADR was authored as 0069 and renumbered to [0070](0070-the-model-tiers-step-spaces-are-adjudicable.md) at the audit-remediation merge (`a0b92bf1`, 2026-09-01); [0069](0069-e2e-adjudicability-is-the-price-of-weight.md) is end-to-end adjudicability. Code citations of `ADR-0069 Decision 5/6` mean the weight gate; `docs/palw-step-space-deployment-notes.md` correctly says ADR-0070. |
| 0065 | Filename keeps the drafting title ("…and a seat must be someone else"); D3, which that clause named, is withdrawn and the H1 reads "a failure is not a verdict". ADR-0071 cites it by the old title. |

Renumbering rule used every time: the ADR with fewer citations in code moves, the other keeps its
number, and the index — this file — is the only place the mapping is written down. Take the next
number from this file, not from `ls`.

## Activation axis — what is armed where (2026-09-02, `consensus/core/src/config/params.rs`)

Supersession and activation are different axes. Most PALW ADRs are consensus-inert until a preset
carries them; the table says what each shipped preset actually installs, read from the assembly
functions (`palw_rc_shipped_params`, `devnet_shipped_params`, `mainnet_shipped_params`) rather
than from any ADR's Status line.

| Preset | PALW mode | Fences armed from genesis | Not armed |
| --- | --- | --- | --- |
| **testnet-11** (`--netsuffix=11`; the PALW-RC) | `ConsensusV2` — the RC bundle: floor + QWEN36 graph-v3 + QWEN25-A16 graph-v2 classes, 8 genesis bonds, genesis free-prompt set = all three families (ADR-0075 D6), per-class seeds (ADR-0076) | heartbeat lane (ADR-0060/0066/0068), attempt-work constant (ADR-0068 F2), `palw_unavailable_abstains` (ADR-0065 D4) | `palw_bond_maturity` (ADR-0065 D1, armable), `palw_inactivity_leak` (ADR-0066 D4 fence), `palw_bootstrap_activation` (ADR-0064) |
| **devnet** | `ConsensusV2` — floor only, 6 public-seed genesis bonds (ADR-0075 §7's rehearsal chain) | heartbeat lane, attempt-work constant | maturity, abstains, leak, bootstrap |
| **mainnet** | `Disabled` — the genesis card (`PALW_MAINNET_GENESIS_ARTIFACT_ROOT`, `PALW_MAINNET_GENESIS_BONDS`) is unset; when set, the same arming as the RC applies from genesis | — | everything |
| testnet-10, simnet | `Disabled` | — | everything |

testnet-12 is retired into 11 (`--netsuffix=12` is refused by name). The `LegacyTn11` algo-4 lane
survives only as the unrouted `TESTNET11_PARAMS` constant.

## What the current direction still owes (open items recorded in the ADRs)

* **ADR-0073 Phases ② and ④** — the receipt lane's weight activation (gated on ① and ③, both
  landed) and the receipt block's chain position / share migration (`algo_id_carries_no_chain_position(7)`
  still true; 0055 D1 stands until then).
* **ADR-0075** — the SDK preflight and the gateway still read the build's certified set, not the
  chain's; the mainnet card is empty until real operator keys exist.
* **ADR-0076 §8** — restated 2026-09-02: the processor already pins a post-genesis entrant's
  `initial_target` to the base class's live target (M2-12), so the field is not free; what stays
  open is that the pinned price is the floor's until `ClassLaneCertified` re-seeds it (Decision 4),
  and the fork-choice consequence for an *uncertified* entrant is closed by ADR-0069 Decision 7.
* **ADR-0069 Decision 7 (2026-09-02)** — an uncertified family's blocks weigh nothing in either chain
  weight; a fork-choice rule, a ruleset move, and a mainnet precondition (see "Security amendments").
* **ADR-0072 §3** — mainnet's activation path (a new algo id behind a DAA fence, a lane-filtered
  difficulty window) and the fork-id handshake are recorded, not built; the doctrine's rolling
  upgrade has no shipped mechanism yet.
* **ADR-0071 §5** — a false capability declaration costs nothing, and pricing it collides with the
  silence doctrine.
* **ADR-0069** — a zero-share class's floor block still adds pwu to fork choice.
* **ADR-0067** — operational arming of chain-registered classes (`--palw-chain-classes`); nothing
  pays a panel seat (R-7).
* **ADR-0066 D4** — the committed liveness table (the pruned-IBD snapshot component) and the leak
  fence's arming; **ADR-0065 D1** — seat maturity is armable and unarmed; seat accountability past
  the D4 fence is zero until receipts ride the chain independently.
* **ADR-0064** — trustless recovery from a total stop is answered by the armed clock, not by this
  ADR's mechanism, which stays dormant; the four pipeline fixtures it names were never written.
* **ADR-0062, ADR-0063** — Proposed: the proof-carrying default that can take a bond; the BIP39
  derivation and the `miner` subcommand.
* **ADR-0070** — the hybrid's checkpoint-anchored recurrence (`n_ctx 8` until then), the per-token
  `embed_lift.a16` store, the `MulElem` cost arm's under-pricing; **ADR-0049 D-G** — the tied-head
  inventory; **ADR-0053 D1a** — the genesis path does not check court-cost ceilings.
* **ADR-0045 D2 / 0056** — the mid-epoch budget gap (an entrant has no attempt-lane budget until
  the next boundary), kept for state-root compatibility.

## Security amendments (2026-09-02) — what the unimplemented decisions gained before they are built

Read against the shipped tree (Relaunch 5e: ADR-0065 D4 armed, ADR-0072 D8 pins, `min_collateral`
400,000 sompi post-genesis, the beacon = one attempt block), each still-open decision below was
weak against an adversary in a way its text did not price. The amendment is appended to the ADR it
belongs to, dated, with the attack it closes and the invariant that proves it; nothing above the
amendment is rewritten. Four principles recur, and are the ones to check first in any new ADR:

1. **A free field is a free draw** (ADR-0072 D8) — every value an accused party chooses must be
   pinned by chain equality, replay, derivation or position, or it is a nonce.
2. **Silence is not a verdict** (ADR-0064 Fact A, ADR-0065 D4) — a rule may charge a positive,
   falsifiable claim; it may never charge an abstention; "no object on this chain within W" is the
   one checkable form of silence, and only with a window a majority would have to sustain.
3. **Weight is what certification buys** (ADR-0069 D5) — anything an unprosecutable party can
   produce for free must weigh nothing, however it is licensed.
4. **The chain never takes the host's word** (ADR-0079 D2/D3) — posture, confinement and
   attestation live off the consensus path; the host's protection is the operator's, bounded by the
   exposure ceiling.

| ADR | Security amendment (§ at the end of the file) |
| --- | --- |
| [0062](0062-data-availability-court.md) — DA court (Proposed) | SA-1…SA-6: the accusation is a bonded, singular `DefaultAccused` inside the retention window; the disclosure is hash-checked and bounded by the RULESET's `PalwCourtParamsV2::max_close_bytes` (there is no per-class ceiling on that path); "silence" is a fold fact with `W_disclose ≥ 2×` finality and permissionless carriage; abstaining seats pay nothing; poverty is not default; the lattice is re-derived. **SA-7 (2026-09-03)**: an accusation may not suspend the arithmetic court, the paused clock is given back, one bond's total DA liability is its exposure ceiling, and the reservation equals the charge — an accuser REWARD is still unfunded and needs its own ADR |
| [0063](0063-operator-tooling-the-missing-half.md) — operator tooling (D1/D4 open) | SA-1…SA-5: seeds never on argv/env; role-separated BIP39 derivation; retirement signed under the network domain; the `miner` subcommand is deleted; `ClassFrozen` is authored only by the transition |
| [0066](0066-the-heartbeat-lane-out-of-header-bits-and-a-committed-liveness-table.md) — D4 committed table (Proposed) | SA-1…SA-4: verified from the pruning-point snapshot or no leak; monotone with hysteresis; never below `min_active_validators`; `t_leak_daa` in the identity raw before arming |
| [0067](0067-classes-are-chain-data-kernels-are-the-build.md) — D5 arming, R-7 (open) | SA-1…SA-4: the interpreter fence is a security fence (ADR-0079 D11's two conditions); resolution off the consensus thread, fails closed to "cannot serve"; re-entry re-verifies from bytes; unpaid seats are a security item |
| [0069](0069-e2e-adjudicability-is-the-price-of-weight.md) — the open item | **Decision 7**: an uncertified family's blocks weigh nothing — one fabricated block per epoch at the floor's `expected_draws` × 2²² leaves would outweigh ≈ 8 epochs of the honest network; invariants E1–E3; a ruleset move and a mainnet precondition |
| [0071](0071-the-attempt-lanes-price-and-the-tickets-bound.md) — §5 open item, D3 | SA-1…SA-4: `capable_classes` bounded (16) and replacing; each declared class reserves exposure; a seat is drawn for a class only after production on it (a fold fact); a declaration is a lifecycle object with rent |
| [0072](0072-the-ticket-is-the-execution.md) — §3 mainnet activation path | SA-1…SA-4: the new lane is seeded by ADR-0076's rule at the fence, not restarted; the fork-id handshake precedes the fence; the version check is fenced; cross-lane replay refused by algo id |
| [0073](0073-real-demand-work-bears-the-weight.md) — Phase ④ | SA-1…SA-3: the beacon folds `k ≥ 3` attempt blocks before receipt blocks gain position (withholding bias `p → pᵏ`); 4b's supply metric counts executors, capped per bond; receipt weight ramps like attempts |
| [0075](0075-certification-is-a-consensus-object.md) — open items | SA-1…SA-4: chunk groups carry a deposit (eight junk groups could block drills for ~5.5 days at carriage price); grading is fee-priced before it is performed; the card's keys are the operators' own; tooling reads genesis ∪ chain |
| [0076](0076-the-attempt-lanes-seed-is-the-retargets-equilibrium.md) — §8 | restated: the processor pins the entrant's target (M2-12); the remaining gap is priced by ADR-0069 D7 |
| [0077](0077-a-prompt-a-person-would-type-is-a-claim-the-court-can-try.md) — Phases A–D (PROPOSED) | SA-1…SA-7: a public gateway's spend of the operator's exposure is quota'd and bounded by the exposure ceiling; openings served to bonded requesters only; F1 covers the prompt side of the stream; the turn deadline is derived from worst-case honest replay; `PanelDa` is a licence until ADR-0062; the persistent runtime re-verifies what it mapped |
| [0078](0078-what-was-made-from-it-is-committed-the-thing-never-rides.md) — kinds (PROPOSED) | SA-1…SA-6: model-written code runs on an ephemeral EVM state under a gas ceiling in a confined process; every transformer declares input/output bounds; uploaded inputs and the DSL DA election are bounded and authenticated; no manifest, no object; task graphs are never executed |
| [0079](0079-a-pure-function-needs-no-permissions-the-sandbox-is-for-the-host.md) — the security ADR (PROPOSED) | SA-1…SA-8: the memory ceiling is not `RLIMIT_AS` (the hybrid maps 33 GiB); the signer trusts the supervisor's channel, not the gateway's bytes; the DA server authenticates; `PATH` leaves the allowlist; Decision 7 = ADR-0077 D6; nothing logs a prompt; per-source rate is not the bound |

Not amended, by decision: [0023](0023-base-three-lane-execution.md) (forward-looking, nothing
started, outside the PALW lineage — a security pass belongs to the ADR that revives it); 0049 D-G,
0053 D1a and 0070's open items are correctness gaps in the court's coverage rather than adversarial
surfaces, and are listed under "What the current direction still owes".

## Still governing, unreversed

0001, 0003–0008, 0010, 0011, 0013–0016, 0019, 0020, 0022–0025, 0030, 0031, 0034, 0035 (D2), 0038
(as amended by 0039 and 0060), 0039 (D1–D4, D6), 0040 (+ 0047), 0041, 0042, 0043, 0044 (as
amended), 0045 (as amended), 0046, 0047, 0049 (as amended), 0050, 0052 (as amended), 0053, 0054,
0055, 0056 (D4 withdrawn), 0057, 0058, 0059, 0060 (the doctrine; D1/D2/D4 in 0066's form), 0061,
0064 (Facts A and B), 0065 (D1, D2a, D4–D6), 0066 (D1, D2, D4's fence), 0067, 0068, 0069 (as
amended by 0075), 0070, 0071 (D1a, D2's bucket, D3), 0072, 0073 (① and ③; ② decided, ④ open), 0074,
0075, 0076. Proposed and unlanded: 0062, 0063. Dormant, not on the V2 path: 0032, 0033. Forward-looking
and not started: 0023 (Base three-lane execution; the EVM lane it builds on is 0020, activated).

ADR-0007 (layered PoW) is unreversed and worth stating explicitly, because it is easy to read as
the hash-lane ADR: PALW is a Layer-1 `algo_id` variant, which is the extension point 0007 already
specifies — including the hard cut-off rule that two `algo_id` values never coexist on one network.
That rule is *why* ADR-0039 can say "no hash lane" without inventing a mixed-algo difficulty
relation, and why the heartbeat lane needed its own `algo_id = 8` (ADR-0066) rather than a re-armed
`algo_id = 3`.
