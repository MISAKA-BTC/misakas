# PALW spec: divergences

Where an ADR (or another written decision) and the code disagree, the Spec states what the **code**
does, and the disagreement is listed here until an ADR or a code fix resolves it
([INDEX.md](../../INDEX.md) §3.1). Phase 2 fills this list as it writes each chapter.

| # | Rule / chapter | The written decision says | The code does | Resolution |
| --: | --- | --- | --- | --- |
| 1 | 03 §3.2 (registration is a listing) | The operator's decision of 2026-09-25 (asynchronous, long-lived, no deadline before a panel, spam priced in MSK) is not in any ADR | TODO: compare with `palw_model_registry_v1.rs` in Phase 2 | an ADR, or a Spec note |
| 2 | 16 §16.3–§16.4 (post-launch fences) | No ADR records the DAA-750 set (13) or the DAA-1,300 set (2) | `PALW_T12_POST_LAUNCH_FENCES_V1` / `_V2` in `params.rs` | one short ADR per flag day |
| 3 | 08 §8.1, 07 §7.3 (panel seed) | ADR-0152 SW-8 prices an anchor re-roll at one block's work, and later one inference | Corrected by `palw_panel_seed_execution` (see `docs/t12-panel-seed-2026-09-25.md`). The correction was never copied into ADR-0152 | copy it into ADR-0152 when it lands |
