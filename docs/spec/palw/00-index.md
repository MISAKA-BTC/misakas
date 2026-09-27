# PALW specification: index

> **Normative (Phase 2, 2026-09-27).** The chapters were written from the ADRs and from the code at
> `55a7be02f`. Where the two disagreed, the chapters state what the code does, and the disagreement is
> listed in [divergences.md](divergences.md). [INDEX.md](../../INDEX.md) explains how Spec, Design,
> RFC and ADR fit together, and [adr/INVENTORY.md](../../adr/INVENTORY.md) records which ADRs each
> chapter slimmed.

## 1. What this specification is

PALW is how MISAKA produces blocks and pays for work. This specification states PALW's rules as they
are **today, on each network**: what a block, an object or a node MUST and MUST NOT do, from which
fence height, and which code decides it. It carries no history. The ADRs hold the decisions, and
`design/palw/` holds the reasoning.

Its backbone is [ADR-0144](../../adr/0144-palw-pays-for-the-inference-you-were-going-to-run-anyway.md),
the constitution:

> A person uses a local LLM on their own machine, with prompts they chose for their own reasons, and
> the same inference that answered them is the inference the chain rewards.

[01-principles.md](01-principles.md) restates ADR-0144's principles P1–P7, the scope of verification
(§3) and "execute now, settle later" (§4) as constraints. Every later chapter says which principles
it serves, and chapter 01 lists, for each principle, the rules that enforce it and the gaps that
remain.

## 2. Networks

| Network | PALW | Ruleset (code) |
| --- | --- | --- |
| **mainnet** | **Off.** `MAINNET_PARAMS.palw_consensus_mode = PalwConsensusMode::Disabled`. There are no genesis bonds or artifact roots (`PALW_MAINNET_GENESIS_BONDS` is empty and the roots are zero) | `consensus/core/src/config/params.rs` `MAINNET_PARAMS` |
| **testnet-12** | **On (R-core+).** Nearly every rule this binary knows is armed at DAA 0 (`palw_t12_arm_every_rule_from_genesis`). The exceptions are bond maturity at DAA 1,000 and six rules that stay dormant. Post-launch flag days: the **DAA-750 set of 13 fences** (`PALW_T12_POST_LAUNCH_FENCES_V1`) and the **DAA-1,300 set of 2** (`PALW_T12_POST_LAUNCH_FENCES_V2`: `palw_floor_refusal_retry`, `palw_final_lock_life_retro`) | `palw_t12_shipped_params` → `palw_t12_params_with_registry_v1` |
| testnet-11 | Retired lineage, still runnable at build `1f98d3bf4`. Its activation map (7,100 / 7,101 / 7,200 / 7,300 / 7,301) is recorded in chapter 16 and not maintained | `TESTNET11_PARAMS` |
| devnet / drill | Drill networks. They are described only where they differ, mainly in how drills move fences to low heights | `devnet_shipped_params`, `config/drill.rs` |

[16-network-parameters-and-fences.md](16-network-parameters-and-fences.md) has the values and the fence
tables. The chapters name only the fences that change their own rules.

## 3. Chapters

The order follows one claim through ADR-0144's sentence: which models may run (03), how a run is
executed and committed (04), how much work it was (05), whether it was eligible (06), and what happens
to the claim from then until it is paid (07–10). After those come the two lanes (11, 12), fork choice
(13), what nodes must do (14), the market beside the fold (15) and the parameters (16).

| # | Chapter | Governs | Main sources |
| --- | --- | --- | --- |
| 01 | [Principles and scope](01-principles.md) | P1–P7, what is and is not verified, execute now and settle later | 0144, 0127 |
| 02 | [State, objects and carriage](02-state-objects-and-carriage.md) | The PALW fold, object kinds and subnetworks, validation layers, acceptance order, the state root | 0042, 0043, 0046, 0058 |
| 03 | [Classes and registry](03-classes-and-registry.md) | Class identity, registration as a listing, lifecycle and earned admission, certification, artifact ownership, the Activation Pool | 0056, 0067, 0075, 0135, 0143, 0145, 0147, 0152 |
| 04 | [Execution semantics](04-execution-semantics.md) (+ [04a integer arithmetic](04a-integer-arithmetic.md)) | The one execution family, BASE-0 and its tiers, kernels, the step function, held context | 0030, 0031, 0040, 0047, 0052, 0053, 0082, 0103 |
| 05 | [Canonical work](05-canonical-work.md) | The CanonicalWorkVector, one derivation for both lanes, pwu, the work target W, the coefficient rule | 0137, 0145, 0146, 0148, 0149 |
| 06 | [Eligibility and block production](06-eligibility-and-block-production.md) | The beacon, the ticket, the single lottery, `bits`, the DAA and anchor clock, the clock cursor | 0072, 0074, 0083, 0137, 0138, 0142 |
| 07 | [Claim lifecycle](07-claim-lifecycle.md) | Attempt → claim → anchor/bind → panel → licence → Final. Voids and retries, escrow, settlement and payout | 0042, 0124, 0127, 0129, 0152 |
| 08 | [Verification](08-verification.md) | The panel draw (seed, the stake-weighted SW rules), seats and readiness, receipts, replay, quorum, DA | 0028, 0098, 0108, 0124, 0133, 0147, 0152 |
| 09 | [Court and offences](09-court-and-offences.md) | Adjudication, bisection, the two-tile refutation, held and fused courts, offence attribution | 0027, 0049, 0082, 0093, 0100, 0152 J |
| 10 | [Collateral and economics](10-collateral-and-economics.md) | Bonds, reservations, seat locks (F + 1,000 and the re-date), the committed ledger, slashing, vesting, emission, capacity | 0065, 0151, 0152, 0160 |
| 11 | [Free-prompt lane](11-free-prompt-lane.md) | Jobs, execution commitments, quantized tickets, receipts, served answers, prefix state, decode rules and constraints | 0044, 0077, 0084, 0096, 0145, 0148 |
| 12 | [Execution lane](12-execution-lane.md) | Round blocks, permits, gas per round, parents-first acceptance, and why execution blocks are not finality | 0125, 0129, 0130, 0139 |
| 13 | [Fork choice and heartbeat](13-fork-choice-and-heartbeat.md) | Weight, the strict economic win with its two-tick tie rule, the heartbeat lane and its transparency, pruning proofs | 0039, 0060, 0066, 0105, 0140, 0142 |
| 14 | [Node duties](14-node-duties.md) | Protocol duties, which are always on, and node policy: licence assembly, filers, DA answers, artifacts, the operator interface | 0093, 0106, 0122, 0136, 0152 Q-7 |
| 15 | [Model lines and market](15-model-lines-and-market.md) | Lines, versions, the store curve, seeds, memberships, the owner's leg, the model sink | 0087–0091, 0094, 0101, 0114, 0120 |
| 16 | [Network parameters and fences](16-network-parameters-and-fences.md) | Per-network values, every fence and its height, the fingerprint, fork-id and schedule id, and how a fence is added | 0042, 0150, the launch note |

## 4. One claim through the chapters

```
t0  the user types a prompt ─ the commitment is fixed ─ local inference runs ─ the answer is shown
     (11 free-prompt job · 04 execution and commitment)                          the product ends here
t1  a later beacon resolves ─ was this inference eligible?                           (06)
     no  → it was just a local inference
     yes → claim (07) → anchor and bind (07) → panel draw (08) → receipts and licence (08, 07)
         → challenge window, court and offences (09) → Final (07)
         → payout, vesting, locks, slashing (10) → weight in fork choice (13)
```

The attempt lane, which draws protocol-generated jobs, takes the same path from t1 on. ADR-0144 §6
item 5 is the rule for shrinking it. Chapter 06 states how far that has gone.

## 5. Conventions

- **Keywords.** MUST, MUST NOT, SHOULD and MAY are used as in RFC 2119. A rule that only a producer
  or a node follows, and that no validator checks, is marked *(node policy)*.
- **Rule IDs.** Each rule has an ID `PALW-<chapter code>-<n>`, stable and never reused. IDs are not in
  file order. The chapter codes are PR 01, ST 02, CL 03, EX 04 (and 04a), WK 05, EL 06, LC 07, VF 08,
  CT 09, CO 10, FP 11, XL 12, FC 13, ND 14, MK 15, NP 16.
- **Sections.** Each section closes with **Activation** (one row per network), **Sources** (ADRs and
  clauses) and **Code** (paths and functions), as in [templates/spec.md](../../templates/spec.md).
- **Code paths** are relative to the repository root. `consensus/core/src/` is abbreviated `core/`,
  and `consensus/src/` is abbreviated `cons/`.
- **Activation rows** follow `palw_t12_arm_every_rule_from_genesis`: every rule at DAA 0 except those
  it names, plus the two post-launch lists (16 §16.2–§16.4). A rule that the RC base arms at a height is
  moved to DAA 0 on testnet-12 by that function's second pass.
- **ADR-0152** (R-core+ v3.1) is cited by its own rule labels (for example "0152 SW-8"). Its full text is
  in [design/palw/archive/0152](../../design/palw/archive/0152/README.md). **ADR-0160** (claim capacity
  v3) is imported after the int-6 integration, and 10 §10.10 is its placeholder.

## 6. Terms carried by code types

| Term | Code |
| --- | --- |
| claim, and its phases Provisional → PanelBound → ReceiptLicensed → Final / Voided / DefaultDisputed | `core/palw_state_v2.rs` `PalwClaimStateV2`, `PalwClaimPhaseV2` |
| claim source: Attempt, or FreePrompt { quanta, spent } | `PalwClaimSourceV2` |
| void reasons | `PalwVoidReasonV2`: BindTimeout, ReceiptTimeout, CourtFraud, ProducerWithholding, NoCapablePanel, UnavailableQuorum, NotReplayBacked, CourtDefault, CourtHeldVerdict |
| bond, and its status Active / Retiring | `PalwBondStateV2`, `PalwBondStatusV2` |
| class row, and its status Registered / Active / Frozen | `PalwClassRowV2`, `PalwClassStatusV2` |
| model lifecycle | `core/palw_model_registry_v1.rs` `PalwModelLifecycleV1` (Candidate, Registered, Prefetching, Probation, ActiveLimited, Active, Held) |
| offence kinds | `core/palw_offence_v1.rs` `PalwOffenceKindV1` (0–6) |
| canonical work | `core/palw_canonical_work_v1.rs` `PalwCanonicalWorkVectorV1` |
| panel draw policy | `core/palw_panel_v2.rs` `PalwPanelDrawPolicyV1` |
| vesting row | `core/palw_vesting_v1.rs` `PalwVestingRowV1` |

The rules that use each term define it where it is first used.
