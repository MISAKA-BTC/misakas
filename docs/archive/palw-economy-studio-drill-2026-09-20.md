# The ADR-0144 economy on a live chain, with MISAKA Studio as the client

ADR-0144 §8 does not accept a green test suite as the proof that the economy works. It asks for a
person's real inference, done in MISAKA Studio for the person's own reasons, to earn. The part of
that a drill can show is this chain of facts, each one observed on a running chain rather than in a
fold under test:

1. the three economic fences of the ADR-0145 bundle (canonical work, independent admission, the
   free-prompt lane's derived work) arm at one height, together with the registry, the work target
   and its payout, and artifact-root ownership, and the chain keeps producing across them with no
   heartbeat miner running;
2. a model the build never shipped is registered by a node, and **registration is not
   eligibility**: the class is admitted by possession its seats prove (and, when it was bought past
   the independence fence, by a jury the network drew), and reaches PROBATION before a claim of it
   can earn;
3. a chat typed into **MISAKA Studio** (`misaka-studiod`, Gateway backend) becomes a free-prompt
   commitment priced by the chain's own expression, a Final claim on every node, and a receipt block
   that spends one of its quanta;
4. a node that was never up syncs the whole chain from genesis and holds the same registry rows and
   the same claim;
5. the chain survives a reorg across the bundle: two partitions extend their own selected chains,
   join, and every node converges on one chain with the same economic state.

Scripts: `scripts/misaka-studio-economy-e2e.sh` (1–4) and `scripts/misaka-studio-economy-reorg.sh`
(5), on branch `feat/economy-closure`. MISAKA Studio from `MISAKA-Studio` branch
`feat/gateway-commit-verdict`.

## The setup

* Devnet, eight floor-only validators on one 24 GB Mac — every devnet genesis bond live, because a
  panel seats five, the registry wants `seat_count + 2 = 7` ready seats, and a bond nobody runs can
  be drawn as a bought class's outsider seat, whose silence voids the claim (ADR-0147 §2.1).
* Every validator holds the tokenizer-bound Qwen2.5-1.5B A16 artifact (`graph-v5@512`, class
  `4277d84f7d91528c…`), mapped once per host (ADR-0136), `--ram-scale 0.3`.
* No heartbeat miner. The anchor clock is dormant, so the DAA advances per block — about 17–25 s per
  DAA on this host.
* Devnet windows: bind 40, receipt 40, challenge 100 DAA; epoch 1,000 DAA; span 5 DAA.
* The genesis hash is read from node-0's own log ("Importing the UTXO set of the pruning point"),
  never typed in: a guessed value produces claims whose context hash no seat can reproduce.

## Runs

| run | fences | what it showed | outcome |
|---|---|---|---|
| run1 | registry = work target = payout = ownership = bundle = 30 | the bundle armed at the registry's own height, no heartbeat miner: fence crossed at 31, DAA 40, all eight nodes past it — ADR-0149 §5 on a chain | stage 1 PASS; then the registration straddle and the FP entrance pricing gap (below) → `ac4e5821` |
| run2 | as run1, fixed binary `6bc7eccb` | the registration lands at 0‰ and opens CANDIDATE — a bought class past independence | stage 2 PASS; stopped: a bought class's first jury audit is at the first epoch boundary (DAA 1,000 on devnet), by ADR-0147's rationing |
| run3 | registry 20, bundle 60 | — | stopped at start: `misaka-studiod --backend gateway` is not a valid value → `8d2d0b15` |
| run4 | registry 20, bundle 60 | stages 1–5 PASS, Studio answered the chat | stage 6: the gateway answered and did not commit; Studio said it did → `2d7c314e`, Studio `d1e70ec` |
| run5 | registry 20, bundle 60 | — | stopped at stage 1 by hand: stage 8 read a key its command never prints → `5f51b365` |
| run6 | registry 20, bundle 60 | stages 1–6 PASS: the chat committed at the chain's price (17,883 quanta, 86,152,240 sompi reserved) and Studio logged it as committed | stage 7: the rail refused to build the carrier — "earns no quanta at 3806528 leaves against a 83102171136-leaf canonical job" → `751263db` |
| run7 | registry 20, bundle 60, binaries `a709e60c` | stages 1–6 PASS; stage 2 judged the door by the fence ("registered before the independence fence at 60: the pre-independence path (grandfathered), no jury"); the rail carried the chat at the chain's price (quanta 17,883, pwu 48,413,644,452); accepted at DAA 112, panel bound at 116 | stage 7: one seat filed Valid, four filed Unavailable at the half-window, the producer was defaulted and the claim voided → `59c5d85a` |
| run8 | registry 20, bundle 60, binaries `59c5d85a` | RUN8_SUMMARY | RUN8_OUTCOME |
| jury | registry = bundle = 30, to DAA > 1,000 | JURY_SUMMARY | JURY_OUTCOME |
| reorg | run8's datadirs, 4 + 4 | REORG_SUMMARY | REORG_OUTCOME |

## The defects the drills found

Every one of these passed the suite. Each is named with the run that found it and the commit that
closed it.

| found by | defect | why the tests did not see it | fix |
|---|---|---|---|
| (reasoning, proved by run1) | **ADR-0149 §5** — with the bundle armed at the registry's own height, the parent of the first blocks past it has no rows, so every attempt including the floor's was `PwuUnderivable` and producers held: the chain stopped until a heartbeat restarted it | every fixture folded rows before the first attempt past the fence | the floor is priced on `genesis_works` while row-less (`base_known_draw`) — `f0896a34` |
| (reasoning) | **ADR-0149 §6** — the fold's merged re-run used default fences, so every merged attempt past the bundle was skipped | the merged path was tested below the bundle | the re-run carries the block's own fences — `f0896a34` / `6dae3235` |
| run1 | **registration straddle** — the panel computed registration terms at the sink DAA; at R = H it registered at 1‰ and the carrier landed past independence, where the share is 0‰, and was dropped (retry only after 200 DAA) | terms and landing were tested apart | terms at the virtual DAA, and no registration within 10 DAA below the independence fence (`palw_registration_waits_for_fences_v2`) — `ac4e5821` |
| run1 | **FP entrance priced in leaves** — past the bundle the fold reserves the compute era's exposure; the gateway and the rail still priced the leaves era's, so the entrance admitted jobs the transition would refuse | the entrance and the fold each had tests; nothing compared them | op 187 `GetPalwFreePromptPrice` over the pure `palw_fp_commitment_price_v1` that the fold itself now calls — `ac4e5821` |
| run4 | **one claim larger than the public-job budget** — past the bundle the entrance priced one claim at 147,880,590 sompi (the chain's per-claim figure; that chat's exact reservation, run6, was 86,152,240); the gateway's default 200‰ of a devnet genesis bond's room is 110,000,868, so the job could not commit, and the gateway called it "spent (0 of …)" | a drill configuration: the Studio pool already runs 1000‰ for the same reason | the drill's gateway spends like the pool's, `GATEWAY_PUBLIC_BUDGET_PERMILLE` [1000] — `2d7c314e`; the gateway names a claim larger than the whole window — `751263db` |
| run4 | **Studio said "committed" for a chat the gateway did not commit** — the chat backend logged a commitment off the claim id alone, and the mining queue marked the job `Committed` ("mined · claim …" in the chat) | Studio's tests had a job block, never one that said `committed: false` | Studio reads the gateway's verdict; `committed: false` is a refusal with the gateway's reason — MISAKA-Studio `d1e70ec` |
| run6 | **the carrier builder asked the leaves** — `build_fp_commitment_tx` refused, as earning no quanta, a chat the chain had priced at 17,883: it asked the leaves era's question (`derive_quanta_and_pwu(work_leaves, class_canonical_leaves)`) of a claim priced in compute. ADR-0148 §6 had moved the entrance's exposure to the chain's expression; this guard, one step later, was left behind. The rail's summary re-derived quanta the same way behind an `.expect` | the builder's tests and the fold's tests each passed; nothing ran a compute-era claim through the builder | `FpCommitmentPriceV1 { Chain, Leaves }`: the chain's quote wherever a node was asked (rail one-shot via op 187, kaspad's canonical claim via the session), the leaves rule only offline — `751263db` |
| run6 (reading the fix) | **a quote asked with the wrong prompt** — the gateway and the rail asked op 187 with the job's full prompt even under PanelDa, whose carrier holds none; the fold's prefix accounting reads the carrier's ids, so the quote priced a claim the fold never sees | every quote test used PublicDA | `palw_fp_carried_prompt_ids_v1`, used by every quote — `751263db` |
| run7 | **four of five seats were refused the material they were bound to** — a user's free-prompt material is not broadcast (the pull is the obligation), a pull is answered to the asker alone, and the whole-material serve throttle was keyed by CLAIM: the panel's five seats pulled the 9.8 MB material in the same second, the first was served and four were refused for 10 s, silently. Their re-ask was a flat 25 DAA and the devnet's half-window is 20, so they signed `Unavailable` without asking twice; the quorum defaulted an honest producer | the throttle's tests asserted exactly the claim-keyed behaviour (a second peer refused) as a DoS property; nothing ran a panel's simultaneous pulls | `ServeThrottle` keyed by (peer, claim) — as the interval-opening throttle already was — and a seat re-asks every quarter-window (cap 25), so it asks twice before it may accuse — `59c5d85a` |
| run3, run5 | two drill defects: an invalid `--backend` value, and stage 8 waiting on `quanta_spent` from `palw derived`, which prints no quanta | — | `8d2d0b15`, `5f51b365` |

## run8 — stage lines

```
1/9 OK — fence crossed at 22, the chain is at 72 with every node past 60 (72 floor blocks produced so far, no heartbeat miner running)
2/9 OK — 4277d84f7d91528c… is on the chain with a row, opened as Prefetching (registered before the independence fence at 60: the pre-independence path (grandfathered), no jury)
3/9 OK — ('Probation { probes_passed: 0 }', 8, 3) (state, ready seats, admission milli)
4/9 OK — the free-prompt lane is certified for 4277d84f7d91528c…
    genesis d239d8bf48909155… (from node-0's own first pruning point)
    gateway: {'registered': True, 'fp_certified': True, 'bond_active': True, 'exposure_room': 549811640}
5/9 OK — gateway on :18895 (bond 0), Studio on :18896 with the Gateway backend
6/9 one chat through Studio: "In one sentence: what does a hash function do?"
    Studio lists the artifact as model "bound-candidate"
  answer: 'A hash function takes an input (or "message") and returns a fixed-size'
    studio: free-prompt claim committed claim=fc60963d41dabdb2…
6/9 OK — Studio's chat produced a commitment: fp-job-c8b99765cbcdb940
    gateway: committed claim fc60963d41dabdb2… (17883 quanta, 86152240 sompi of exposure)
  commitment: prompt_tokens=19 decode=16 work_leaves=3806528 (the leaves are a comparand, not the price)
7/9 submitted; following claim fc60963d41dabdb2…
    provisional on every node (DAA 118)
    panel_bound on every node (DAA 122)
    receipt_licensed on every node (DAA 129)
```

The rail's own line for the carrier, which is where the chain's price and the leaves part company:

```
fp-job-c8b99765cbcdb940: SUBMITTED carrier … — pwu 48413644452 quanta 17883 fee 329987 sompi
```

and the seats' verdicts for that claim, which is what `59c5d85a` changed:

```
4 × filed a "Valid" receipt for claim fc60963d41dabdb2…     (run8, after the fix)
1 × Valid + 4 × Unavailable → ProducerDefaulted → voided    (run7, before it)
```


## The jury, on a chain

JURY_LINES

## The reorg

REORG_LINES

## What this does not show

* A devnet's windows, not testnet-11's, and one host: every "every node" above is eight processes
  on one machine.
* A court case: nothing here prosecutes a fault (the court round trip is its own drill,
  `docs/palw-court-round-trip-drill.md`).
* A day of real use. ADR-0144 §8's first half is a product measurement; this is one chat.
* Whether the answer was any good: a PASS says the pipeline reaches a receipt block.
