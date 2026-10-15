# ADR-0152 (R-core+ v3.1): the text as written

This folder holds the whole of ADR-0152 v3.1 as it stood on branch `docs/adr-0152-v31-postedits` at
`9ed1adced` (2026-09-24 23:10 JST): v3.1, with its 13 post-edits and the integration amendments
IA-1…IA-15. It is split into eleven files at its section boundaries, with no other changes. Each file
starts with a two-line header naming the lines it holds. Joined in order, the files reproduce the
original byte for byte. The split was checked by script when the files were written.

**Not normative.** The rules testnet-12 runs are in [spec/palw](../../../../spec/palw/00-index.md).
Where this text and the code disagree, the code wins, and the disagreement is listed in
[spec/palw/divergences.md](../../../../spec/palw/divergences.md). The decision record is
[ADR-0152](../../../../adr/0152-account-stake-staged-reserve-and-vested-rewards.md). The reasoning is
summarised in [design/palw/collateral.md](../../collateral.md) and
[design/palw/verification.md](../../verification.md).

| File | Holds | Spec it feeds |
| --- | --- | --- |
| [00-header-and-changelogs.md](00-header-and-changelogs.md) | Title and status block; the v3.1 changes (N1–N13), the post-edits, the integration amendments IA-1…IA-15, the v3 changes | — (history) |
| [01-operator-summary-ja.md](01-operator-summary-ja.md) | 運用者向け要約（日本語） | — |
| [02-motivation-and-overview.md](02-motivation-and-overview.md) | v2 changes; §1 motivation and **§1.5 the binding operator decisions**; §2 decision overview (with the genesis values table) | 10 |
| [03-rules-b-sr-v-l-a.md](03-rules-b-sr-v-l-a.md) | §3.1 B, §3.2 SR, §3.3 V and H-1, §3.4 L, §3.5 A | 07, 10, 13 |
| [04-rules-s-t-w-p0-10.md](04-rules-s-t-w-p0-10.md) | §3.6 S and R, §3.7 T, §3.8 W, §3.9 P0-10 (per attacker strategy; also 4-ter held attention and 4-quater deadlines) | 03, 05, 10 |
| [05-rules-j-da.md](05-rules-j-da.md) | §3.10 J (attribution F1/F2), §3.11 DA | 08, 09 |
| [06-rules-q-dl-sw.md](06-rules-q-dl-sw.md) | §3.12 Q, §3.13 DL, §3.14 SW | 07, 08 |
| [07-invariant-and-comparison.md](07-invariant-and-comparison.md) | §4 the invariant per attacker strategy (success probability per door, per-class numbers), §5 comparison | — (design) |
| [08-fences-schema-phasing.md](08-fences-schema-phasing.md) | §6 fences, prerequisites, schema v22; §7 ownership and phasing | 02, 16 |
| [09-tests-and-launch-gate.md](09-tests-and-launch-gate.md) | §8 tests, the drill replay rule, the launch gate, the post-launch observation program | — (process) |
| [10-questions-traceability-reviews.md](10-questions-traceability-reviews.md) | §9 operator questions; traceability; the v3 and v3.1 review dispositions; v3.1 notes | — (history) |

## What changed after this text

These amendments are not in the text above. The Spec carries them.

1. **SW-8's seed, corrected (2026-09-25).** SW-8 priced the anchor re-roll at one block's work and
   later at one inference per try. Neither held: the seed was the anchor block's identity, which the
   producer could re-sign for free. From DAA 750 the seed is the anchor attempt's execution
   commitment (`palw_panel_seed_execution`). The record is
   [t12-panel-seed-2026-09-25.md](../../../../t12-panel-seed-2026-09-25.md), which also specifies
   lane A, the operator-anchored panel (`palw_operator_anchor`, DAA 750). Spec: 07 §7.3, 08 §8.1.
2. **The DAA-750 flag day** (ADR-0154). It adds registry resilience (a no-capable-panel claim retries
   until its bind window closes), bond maturity from 750 for non-genesis bonds, the model sink bound,
   the anchor at the exposure ceiling, genuine slashing evidence on the UTXO side, the strict economic
   win (reorg and pruning proof), parents-first acceptance, and two changes to L-3 and A-6: a
   resolved lock is carried by the whole collateral with a four-floor accuser reserve, and a resolved
   `Valid` lock lives F + 1,000.
3. **The DAA-1,300 flag day** (ADR-0155). SW-10 and eligibility refusals re-anchor instead of voiding
   (`palw_floor_refusal_retry`). Locks left at F + 3,000 are re-dated to
   `min(expiry, max(F + 1,000, H))` (`palw_final_lock_life_retro`).
4. **State of two flags at `55a7be02f`.** `PALW_RCORE_VESTING_ROWS_LANDED_V1 = true`, and
   `PALW_RCORE_SEAT_DA_ANSWER_LANDED_V1 = true`, which arms DA-7's signer S4 on full-mask signers.
   X10 (`palw_rcore_attributed_charging`) is still not declared.

## Companion files not imported

The files ADR-0152 cites by name are on branch `docs/adr-0152-v31-postedits` under
`docs/handoff/t12-rcore-20260924/`:

- `adr-snapshots/0152-v2.md`, `0152-v3.md`, `0152-v3.1-draft.md`
- `audit-specs/f2f1_spec.md`, which is authoritative for the F1/F2 names, `f1c_f1m_spec.md` (the
  addendum), `s_spec.md` (S-SPEC), and the review syntheses
- `phase2-plan.md` and `v3.1-postedits.md`
- `v3calc/*.py`, the scripts behind every figure

They are evidence for this text, not part of it. They land with that branch, or under `evidence/`,
if the operator asks.
