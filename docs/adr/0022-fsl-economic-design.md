# ADR-0022: FSL Economic Design (Correction-Warranty Economics, No New Token)

## Status

Proposed (2026-06-11). Design **v0.5 — Economic Design Revised** —
`docs/misaka-fsl-design-v0.5-economics.md` (§31–§43) is the source design; this ADR is its
freeze. It is the **economics companion** to [ADR-0021](0021-fact-settlement-layer.md) (the FSL
truth-determination layer, design v0.3 §0–§30, which this ADR changes **nothing** in). It fully
revises and **replaces** the v0.4 economic working draft (never committed; v0.5 is the first
committed FSL economic design), incorporating all 15 findings of an external review (overall
7.5/10) — the finding-by-finding mapping is recorded in design §42.

Design-only. **Zero consensus changes** beyond what ADR-0021 already froze (the single
`MLDSA87_VERIFY` `0xF003` precompile); everything in this ADR is EVM-contract-, API- and
operations-layer economics on the [ADR-0020](0020-selected-parent-evm-lane.md) lane.

---

## Context

ADR-0021 froze *how FSL decides facts*. It deliberately deferred *how the system pays for
itself*: who funds template engineering, parser operation, adjudication and archival; what an
integrator's "FSL-settled" badge is actually worth in money; and how any of that survives a
MISAKA price shock — all without minting a token, which ADR-0021's credibility argument rules
out (a truth layer whose referees are paid in a token they can influence is structurally
conflicted; cf. the UMA precedent in ADR-0021's context).

A v0.4 economic draft existed as a working document. External review scored it 7.5/10 and found
15 defects, the worst being **load-bearing**:

1. **Coverage/exposure conflation (most severe).** The v0.4 unit-economics example charged a
   premium on a $10M *declared* exposure while the protocol's own cap rules (K9a) only ever
   stood behind a fraction of it — the flagship number quietly promised coverage the capital
   model never accepted. A guarantee business whose headline example overstates its guarantee
   is a credibility time bomb for a credibility product.
2. **No actuarial treatment of correlated loss.** FSL's worst loss is not one wrong fact but a
   *bundle* — one parser bug correcting 300 claims of the same template at once. A per-fact cap
   plus a loss-ratio KPI is not solvency management.
3. **MISAKA price dependence.** USD-denominated adjudicator collateral in MISAKA means a price
   crash margin-calls the panel — a *truth-supply capacity shock*, not merely a treasury
   problem; and deterministic DEX conversion made the conversion price itself a manipulation
   target.
4. **Cold-start paradox.** `subsidy ≤ μ × fee_revenue` yields zero subsidy at zero revenue —
   the gate designed to prevent emission-farming also prevented existence.
5. Unbounded/perpetual template royalties (rent-seeking + unlimited author liability),
   overturn-only adjudicator scoring (statistically starved at K1 ≤ 0.1%/yr), exposure
   under-declaration incentives, bounty sponsors steering the coverage agenda invisibly, and
   "insurance" terminology triggering insurance/derivatives regulation in several
   jurisdictions.

---

## Decision — correction-warranty economics with a hard truth/capital wall

The nine frozen judgments (design §43), grouped:

### 1. No new token; MISAKA-dependence severed (P3, §36)

FSL issues **no token** (unchanged from v0.4's rejected-mechanisms list: no emission-centred
rewards, no stake-weighted truth, no vertical sub-tokens, no adjudication delegation). New in
v0.5: payments and collateral stop *depending* on MISAKA —

- fees payable in **stablecoins**; the protocol routes an initial 10% of stablecoin
  revenue into MISAKA **buy & burn** (value linkage = buy & burn + stake demand + gas, not
  forced holding);
- adjudicator collateral becomes a **haircut basket** — MISAKA at 50% haircut, approved
  stablecoins 5%, approved tokenized T-bills 10%, warranty LP shares 30%;
  `slashable_value_usd = Σ amount × (1 − haircut) × oracle_price`;
- a **panel continuity reserve**: when margin calls exceed an epoch threshold, the treasury
  temporarily tops up collateral (with recourse) so a price crash cannot stall adjudication —
  "truth-supply does not halt on a price shock" becomes a precondition of the K10/K11
  time-to-resolution KPIs;
- conversion pricing is **manipulation-resistant by specification** (§36.3): multi-venue TWAP
  (≥ 30 min) primary, oracle backup, prior-epoch-median fallback, stablecoin-only emergency
  mode, with slippage/liquidity/volume constraints, randomized conversion timing and a circuit
  breaker. The deterministic-DEX-conversion design is abolished; the spec states plainly that
  the conversion price *is itself an oracle* and must be defended as one.

### 2. Guarantee language discipline — exposure ≠ coverage (P6, §35.1, K9a–K9c)

The single most important correction. Four separated quantities:

```
declared_exposure      what the integrator attests it settles against FSL facts
covered_exposure       the warranty limit actually purchased
accepted_exposure_cap  min(covered_exposure, security_cap(risk_tier), aggregate-limit headroom)
uncovered_exposure     declared − covered (must be DISPLAYED downstream if non-zero)
```

- **K9a** — the "FSL-settled (insured)" label is usable only when
  `declared_exposure ≤ accepted_exposure_cap`;
- **K9b** — `uncovered_exposure` must be shown to downstream users;
- **K9c** — over-cap markets may integrate as "references FSL evidence" but may **not** call
  themselves insured settlement.

Full renumbering against v0.3 §12.2's old K9a–K9d (design §35.1): old K9a (exposure ≤
published compensation capacity) is absorbed by the four-way decomposition above; old K9b
(attack-cost bound) → **K9d**; old K9c (bond + slashable stake bound) → **K9e**; old K9d
(reject/force-multi-oracle on K9d/K9e failure) → **K9f** — all three retained unchanged in
substance.

FSL's monetary promise extends to `covered_exposure` and not a basis point further; the v0.4
"$10M exposure for a $4,000 premium" example is repudiated and replaced by the two honest cases
in design §35.3.

### 3. Warranty priced on the correction tail, capital-managed for correlated loss (§35.2–§35.4)

- **Term**: not market duration but `settlement_window + correction_tail` (template-class
  defaults: filings 180d / official records 90d / contested 365d).
  `premium = rate(risk_tier) × covered_exposure × tail_year_fraction × tail_multiplier`;
  initial annualized rates 10/40/120bp for tiers A/B/C.
- **Correlation**: every fact carries a `risk_correlation_group`
  (template/parser/source-type/source-operator/jurisdiction/event-type/author);
  underwriting acceptance requires both the group's `aggregate_exposure_limit` headroom *and*
  a solvency floor (`solvency_ratio ≥ 2.0` initially). Capital metrics — PML (worst single
  correlation group total loss), stress losses at 99/99.5, tail reserve, reinsurance capacity —
  are continuously published (KPIs E8–E13). First-loss/senior tranching is permitted; external
  reinsurance attaches only after the §40 legal structure lands.
- Per-fact caps alone are explicitly disclaimed as a guarantee claim.

### 4. Two-phase bootstrap — seed runway, then revenue gate (P2, §37)

Phase S (≤ 18 months): a **pre-fixed, USD-denominated treasury runway** (not emission), spent
exclusively through the coverage-bounty market with a public spend dashboard; unspent funds
lock back to the treasury at expiry. Phase R: `subsidy(e+1) ≤ min(SUBSIDY_CAP(e+1), μ ×
fee_revenue(e))` with μ = 3.0 halving every 12 months and a hard sunset 36 months in. If demand
never materializes, FSL **shrinks instead of printing** (FM1/FM4): the D3 free tier plus the
evidence graph are the designed minimum survivable core.

### 5. Supply-side incentives that resist their own failure modes (§34)

- **Template royalty decays** 10% → 5% → 1–2% → 0 (incubation/growth/standard/deprecated),
  with maintenance bounties funded separately; 40% of royalty sits in escrow for the
  correction tail, and defect clawback (K2b) is **bounded by the escrow balance** (extra
  slashing only on adjudicated gross negligence/fraud). Royalty is quality-multiplied from
  published `template_metrics`, so over-broad templates lose money through their own
  challenge/void rates. Forks are allowed (independent red-team review; semantic-core forks
  pay the original author a halved standard-rate tail).
- **Adjudicator track record is multi-metric** (overturn involvement remains the slow,
  weightiest signal; plus citation completeness, third-party recomputability, COI hygiene,
  red-team/calibration performance, latency, minus false escalations) — majority-agreement
  rate stays banned. **Calibration cases** (known-answer synthetic disputes) bootstrap new
  entrants, countering top-entity concentration (E5). Fees stay schedule-only (no individual
  bidding) with protocol-level scarcity multipliers revised quarterly.
- **Source onboarding is a four-tier evidence-strength ladder** (§34.3): native-signed →
  notarized → reproducible-parser → human-confirmed. MVP accepts L1-parser-source and up —
  official ML-DSA source signatures remain the long-term moat, **not** an adoption
  precondition.
- **Exposure under-declaration is self-correcting** (§33.3): coverage is capped at the attested
  declaration, undeclared exposure forfeits the label, and an `exposure_registry` + indexer
  audits catch material understatement (label revocation + integration-score demotion).

### 6. The truth/capital wall stays, and the capital agenda becomes transparent (P1, §32, §38)

Capital (bounties, underwriting, archival investment) never enters truth determination — the
wall is regression-tested (E-F1/E-F2). What capital *does* decide is the **coverage agenda**
(which parts of the world FSL learns to adjudicate), and v0.5 makes that explicit rather than
pretending otherwise: bounty sponsors must disclose identity (credential-linked), related
markets and warranty exposure, downstream intentions, and COI with template
authors/parser operators/source negotiators (§38.2); sponsor–contractor COI is a focal item of
bounty acceptance red-teaming.

### 7. Legal product structure (§40)

Externally, the product splits four ways — `protocol_backstop` (MVP; marketed strictly as a
**"limited correction warranty"**), `parametric_warranty` (attached to D3/D4 SLAs),
`commercial_coverage` (licensed-partner/captive/mutual wrapper, Phase 3) and
`third_party_underwriting` (incorporated external capital pools). "Insurance"/"premium"/
"tranching" remain internal §35 vocabulary until the jurisdictional assessment (an operations
document, out of design scope) lands.

---

## Revenue model (design §33)

D1 claim-creation fees (template-class/risk-tier priced); D2 settlement fees on **accepted**
exposure (2/5/12bp by tier); D3 Product-A API subscriptions — treated as a *hypothesis* to be
validated per segment in Phase 1, charging for latency/bulk/diff/alerting/SLA/warranty hooks
while the public free tier (scoreboard, single-fact lookups, recomputation procedure) stays
free as a credibility precondition; D4 priority/SLA; D5 warranty premiums (§35).

---

## KPIs and failure modes (design §39)

Thirteen economic KPIs E1–E13: subsidy/fee ratio (monotone down in Phase R, 0 at sunset), fee
per settled fact, **loss ratio ≤ 30%**, underwriting capital + external share, supply-side
entity concentration (top-10 < 40%), clawback rate ≤ 5%, burn/stake totals, **solvency ≥ 2.0**,
aggregate-exposure concentration, PML, coverage decline rate (decline rate is published as
quality information), reinsurance capacity, tail-reserve adequacy. Eight failure modes FM1–FM8
each map to a designed response — most notably FM6 (correlated event → freeze the correlation
group's new acceptance + mass re-verify + tail-reserve drawdown) and FM1/FM4 (demand failure →
shrink, never emission life-support).

The economic test plan E-F1–E-F15 (design §41) regression-fixes the wall, the label rules
(K9a–K9c), conversion manipulation resistance, basket/margin/continuity mechanics, aggregate
limits, solvency-floor acceptance stops, the correlated-incident drill, under-declaration
detection, royalty decay/clawback bounds, sponsor-disclosure enforcement, and calibration-case
onboarding.

---

## Consequences

**Positive.** The guarantee language is now honest by construction (a downstream user can see
exactly what is and is not covered); solvency is managed against the *actual* worst case
(correlated bundles, not single facts); adjudication capacity survives a MISAKA crash; the
bootstrap can fund a cold start without an emission valve; template/adjudicator incentives
punish their own degenerate strategies; and the legal wrapper de-risks the word "insurance"
before regulators do it for us.

**Negative / accepted costs.** Underwriting capacity (not demand) becomes the binding growth
constraint — high-value integrations will be *declined* when correlation budgets or solvency
would breach, and E11 makes that refusal public; stablecoin/RWA collateral imports issuer and
oracle dependencies (mitigated by haircuts and the §36.3 price-defense, accepted as cheaper
than MISAKA mono-collateral); the buy-&-burn percentage and basket haircuts are new governance
surfaces; Phase-S spending is a treasury cost with no revenue guarantee (bounded by the fixed
runway and expiry lock); and the four-way legal split delays commercial coverage to Phase 3.

**Explicitly unchanged.** Everything in ADR-0021: the truth-determination layer, the L0–L4
ladder, entity-bound credentials, the F003-only consensus footprint, the Product-A-first
roadmap, and the no-new-token stance.

---

## References

- `docs/misaka-fsl-design-v0.5-economics.md` — source design (§31–§43); external-review
  mapping in §42; final judgments in §43.
- [ADR-0021](0021-fact-settlement-layer.md) — the FSL truth-determination layer
  (`docs/misaka-fsl-design-v0.3.md` §0–§30) this ADR funds.
- [ADR-0020](0020-selected-parent-evm-lane.md) — the EVM lane all FSL contracts (warranty
  escrow, bounty market, exposure registry) deploy on.
- [ADR-0018](0018-quality-gated-stakescore-inclusion-economics.md) — precedent for
  quality-gated economics on this chain (L1 PoS-v2).
- External economic review (overall 7.5/10, 15 findings, remediations A–G) — findings
  reproduced and mapped in design §42.
