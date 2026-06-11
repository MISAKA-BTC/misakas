# ADR-0021: MISAKA Fact Settlement Layer (FSL)

## Status
Proposed (2026-06-10). Design **v0.3 (consolidated)** — `docs/misaka-fsl-design-v0.3.md` is the
source design; this ADR is its code-grounded freeze against the kaspa-pq tree. Nothing is implemented
yet (the only consensus change is the F003 precompile, scheduled F-M0).

The **economic design** (funding, correction-warranty economics, collateral policy, bootstrap) is
frozen separately in [ADR-0022](0022-fsl-economic-design.md) /
`docs/misaka-fsl-design-v0.5-economics.md` (§31–§43); it changes nothing in this ADR.

Builds on, and changes nothing in, the existing consensus except one EVM precompile:
- [ADR-0020](0020-selected-parent-evm-lane.md) — the Selected-Parent EVM lane (`docs/misaka-evm-design-v0.4.md`); FSL is an EVM-contract subsystem on this lane and adds one precompile to its set (`0xF001` WMISAKA, `0xF002` withdraw, **`0xF003` ML-DSA-87 verify**).
- [ADR-0009](0009-dns-probabilistic-finality.md)/[0012](0012-mainnet-validator-sortition-commit-reveal.md) — the DNS finality overlay (attestor set, stake, slashing, commit-reveal sortition) is **reused** for the adjudicator panel + beacon.
- [ADR-0019](0019-mldsa87-migration.md)/[0008](0008-hash64-consensus-identity.md) — ML-DSA-87 + Hash64 are the PQ-safe domain for evidence/verdict attribution.
- `docs/archival.md` — evidence/verdict bodies live in the archival network (pin obligation + storage challenge + slashing).

---

## Context

L1 oracle resolution in 2026 is in a structural crisis that FSL targets (design §1, with citations):
Polymarket/UMA's optimistic-oracle stack settled high-value **event facts** by a permanent token vote
that concentrated in a few wallets (WSJ 2026-05: a majority of disputed-market votes came from the
top-10 wallets; ~1/5 of disputes had financially-interested voters), allowed **post-hoc rule changes**
(the Strategy BTC-sale $60M market: "execution time vs public-confirmation time" was undefined in the
claim text and decided by a token vote after an `Additional Context` edit read as a retroactive rule
change), permitted **evidence-free whale votes** (the UFO market), and could be **de-facto overridden**
off-oracle (the Barron Trump refund). The bond ($750) was five orders of magnitude below the settled
value ($60M).

The thing to build is **not** a prediction market but a **Fact Settlement Layer**: define a proposition
(Claim) in a machine-readable Predicate DSL, preserve its evidence as a third-party-recomputable
forensic package, settle it through an entity-COI-controlled adjudication mechanism, and serve the
settled fact as risk-decomposed data. Prediction markets, insurance, DeFi liquidation, RWA, and AI
agents are all **downstream integrations**.

**Honest limit (top risk R1):** the oracle problem cannot be solved, only *structured*. FSL does not
settle metaphysical truth — it settles "what the protocol adjudicated, with an evidence trail."

---

## Decision — an evidence-anchored Fact Settlement Layer on MISAKA L1

Four core principles drive the design:
1. **Kill ambiguity at authoring time, not adjudication time.** A Predicate DSL (design §5) is a
   market-rule compiler; the Strategy-class "event time vs confirmation time" defect is excluded
   structurally at claim creation (`predicate_hash` is fixed at creation with no post-hoc edit path).
2. **Mechanize the adjudication hierarchy.** `evidence > reputation > expertise > stake > vote` is an
   escalation ladder L0–L4; the vote is a last resort (L3 final round only) with a permanent `void`
   escape hatch. **A vote that cites no evidence is invalid.**
3. **Verifiable accountability.** A verdict carries a recomputable evidence package; participation is
   **entity-bound** (one-entity-one-seat, third-party auditable); economic guarantees are published as
   *decomposed* risk, never a single "guarantee amount."
4. **Earn credibility by record, not claim.** Ship the **Evidence Graph API (Product A, no settlement
   power)** first, build a methodology-fixed public **Shadow Resolution** track record, then layer the
   **Settlement Adapter (Product B)** on top.

The 14 frozen design decisions (FD1–FD14, design §3) in brief: layer-not-chain (FD1); ladder L0–L4
(FD2); the **F003 ML-DSA-87 verify precompile is the only consensus change** (FD3); evidence commitment
on-chain / body in archival (FD4); attestor commit-reveal beacon for juror selection — **never
`prevrandao`** (FD5); claims only from reviewed Predicate DSL templates (FD6); finality-tagged reads,
high-value = `settled_final` only (FD7); ML-DSA-87 + DNS-domain source identity (FD8); **entity-bound
adjudicator credentials** (FD9); **decomposed economic guarantees** (FD10); **two-product split** (FD11);
**independent correction/backstop court** (FD12); **downstream product contract banning post-facto rule
change** (FD13); **evidence = recomputable forensic package, not a link list** (FD14).

---

## The one consensus change — `MLDSA87_VERIFY` precompile (FD3)

The sole consensus/protocol change. It extends the EVM lane's precompile set (ADR-0020) so FSL
contracts can verify the ML-DSA-87 signatures that bind attestors / sources / verdicts to entities in
the PQ-safe domain — using the *same* verifier already shipped for native UTXO ML-DSA-87 (ADR-0019,
`kaspa-txscript`).

| Item | Value | Notes |
|---|---|---|
| Address | `0x…F003` | Next after WMISAKA `0xF001`, withdraw `0xF002` (EVM v0.4 §). |
| Input | `pubkey(2592) ‖ message_hash(64, BLAKE2b-512) ‖ signature(4627)` | Fixed-width concat. |
| Output | `1` valid / `0` invalid | Pure; no state. |
| Gas | `MLDSA_VERIFY_GAS_BASE` (init 20 000) + calldata cost (~117k dominates) | Calibrated to the ML-DSA-87 verify cost (~64–77 µs portable, kaspa-pq ML-DSA-87 measurements). |
| Tests | FIPS-204 KAT + differential fuzz (mandatory) | Reuse the `pq-ci-guard.sh` ACVP/KAT gate. |
| Activation | behind the `evm` feature, gated like F001/F002 | Inert until the EVM lane activates; fork-bundling (v0.4 M10) vs standalone = open decision FO1. |

It mirrors the existing `0xF002` withdraw-precompile pattern (a custom revm precompile behind the `evm`
cargo feature, so the default secp-free node is unaffected). Everything else in FSL is ordinary EVM
contracts + reuse of existing infrastructure — **no new L1, no new oracle token, no consensus-rule
change beyond F003.**

---

## Frozen parameters (P0)

| Item | Value | Notes |
|---|---|---|
| Evidence package domain | keyed BLAKE2b-512 `"FSL_Evidence64"` | Recomputable forensic package hash (design §6). |
| Evidence-graph commitment | BLAKE2b-512 Merkle (`Hash64`) | ADR-0008 identity domain. |
| Beacon domain | keyed BLAKE2b-512 `"FSL_Beacon64"` | Juror-selection randomness (commit-reveal, §11); `beacon(e)` used only at epoch `e+1`. |
| Attestor / source / verdict signatures | ML-DSA-87 | PQ-safe attribution (§4.2). |
| Bond / fee / reward settlement | EVM native / ERC-20 (secp256k1 tx) | "secp for moving money now; ML-DSA-87 for facts verified later" (§4.2). |
| Escalation ladder | L0 auto-source · L1 optimistic · L2 attestor panel (7) · L3 appeal court (jurors ×2+1, commit-reveal Schelling) · L4 void | Vote exists at L3-final **only**; `void` always available. |
| `risk_tier` β (attack-cost multiplier) | A 1.5 / B 3.0 / C 5.0 | `attack_cost_lower_bound ≥ β × downstream_exposure` (K9b; renumbered **K9d** in ADR-0022/v0.5 §35.1). |
| `risk_tier` γ (bond+stake multiplier) | A 0.05 / B 0.15 / C 0.30 | `posted_bond + slashable_panel_stake ≥ γ × downstream_exposure` (K9c; renumbered **K9e** in ADR-0022/v0.5 §35.1). |
| `α` (min bond ratio) | 0.5% | **tier-A L1 only**; tier-B/C are governed by K9b/K9c (FD10) — K9d/K9e under the ADR-0022 numbering. |
| KPIs (public, recomputable) | K1 overturn ≤ 0.1%/yr · K2 void ≤ 2% · K3 L3-vote-reach ≤ 0.5% · K4 evidence-recompute = 100% · K6 single-entity panel power < 1/3 | `misaka_getFslMetrics` (§18). |

EVM-state stores (`FactStore`, registries, commitments) **persist** (no pruning); evidence/verdict
bodies live in the archival network; an epoch `FactStore` state-root + verdict-signature bundle is
co-recorded in `EvmPruningPointTrustedData` (EVM v0.4 §12.2) so facts survive archive loss (R7/§16).

---

## Architecture placement & PQ boundary

```
MISAKA L1 (UTXO/DAG, PQ-safe, ML-DSA-87)
 ├─ DNS finality overlay ──── attestor set / stake / slashing / commit-reveal (REUSED: ADR-0009/0012)
 ├─ EVM lane (ADR-0020, v0.4) ── FSL contracts + MLDSA87_VERIFY (0xF003)
 │     TemplateRegistry · ClaimRegistry · EvidenceAnchor · SourceRegistry ·
 │     EntityCredentialRegistry · ResolutionEngine(L0/L1) · DisputeCourt(L2/L3) ·
 │     CorrectionCourt(independent) · FactStore (NO admin key)
 ├─ Archival network ──────── evidence body / verdict docs (pin + storage challenge + slashing)
 └─ Consumer-chain adapters ── Product A API gateway · Product B OOV2/CTF-compatible surface
```

**PQ boundary (R8):** ML-DSA-87 + keyed BLAKE2b-512 for everything that is *verified later* (evidence
hashes, graph commitments, attestor/source/verdict signatures); secp256k1 only for *moving money now*
(bonds/fees/rewards on the EVM lane). FSL adjudication stake is **separate** from the DNS-overlay bond
so an adjudication dispute cannot bleed into L1 finality (§12.4).

---

## Scope

**In:** machine-checkable **event facts** with a one-source-of-truth primary record — corporate actions
(divestiture/M&A/earnings), regulatory filings, listing/delisting, official records
(sports/election-board/statistics), macro indicators.

**Out (non-goals):** building a new L1 (FD1); subjective/interpretive propositions (template gating
rejects them at creation, FD6); operating a gambling platform (markets are downstream); "deciding truth
by vote" (stake-weighted vote is L3-final only, with `void`); **price feeds** (Chainlink is structurally
superior — no head-on competition).

---

## Roadmap (design §26) — Product A first

| Milestone | Scope |
|---|---|
| F-M0 | `MLDSA87_VERIFY` precompile (fork-bundling vs standalone = FO1) |
| F-M1 | Predicate DSL / TemplateRegistry / ClaimRegistry / EvidenceAnchor (forensic) / FactStore (no admin path) |
| F-M2 | L0/L1 ResolutionEngine + SourceRegistry + open-source versioned parser base |
| **F-M7′** | **Product A public (top priority)** — Evidence Graph API + methodology-fixed shadow scoreboard; 100–300 Strategy/filing/official-record mismatch post-mortems |
| F-M3 | EntityCredentialRegistry + provider accreditation + attestor panel (L2) + beacon |
| F-M4 | DisputeCourt (L3/L4) + blackout enforcement + appeal economics |
| F-M5 | CorrectionCourt + backstop fund + parametric compensation |
| F-M6 | `misaka_getFactRiskInfo` / `misaka_getFslMetrics` + audit/red-team cycle 1 |
| F-M8 | Product B (OOV2/CTF adapter + N-of-M relayer) — after §19.3 shadow success |
| F-M9 | Phase B delivery (light-client proof, trustless) |

---

## Consequences
- **One consensus change** (the F003 precompile); everything else is EVM contracts + reuse of the DNS
  overlay + archival network. The default secp-free node is unaffected (F003 is `evm`-feature-gated).
- FSL inherits the EVM lane's properties: opt-in `--features evm`, inert until EVM activation, and the
  EVM legacy (non-PQ) compatibility zone for its *economic* surface — adjudication *attribution* stays
  PQ-safe (§4.2).
- The product is credibility-gated: Product A (no settlement power) precedes Product B (settlement), and
  every dispute / void / correction / compensation-denial is published as a post-mortem within 14 days,
  including FSL's own errors (K12).
- Residual, disclosed risks remain by design: undetectable COI (R4), a sufficiently large bribe can
  break the L3 final round (R2), and downstream override is contractual-not-cryptographic (FSL only
  guarantees the override *cannot be hidden*, R5/§14).

---

## References
- Source design: `docs/misaka-fsl-design-v0.3.md` (full v0.3 consolidated spec; review-traceability matrix §28).
- EVM lane: [ADR-0020](0020-selected-parent-evm-lane.md), `docs/misaka-evm-design-v0.4.md`.
- DNS finality / sortition: [ADR-0009](0009-dns-probabilistic-finality.md), [ADR-0012](0012-mainnet-validator-sortition-commit-reveal.md).
- PQ / identity: [ADR-0019](0019-mldsa87-migration.md), [ADR-0008](0008-hash64-consensus-identity.md), NIST FIPS 204 (ML-DSA).
- Archival: `docs/archival.md`. Schelling-game attack: Buterin, "The P + epsilon Attack".
