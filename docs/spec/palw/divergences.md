# PALW spec: divergences

Where an ADR (or another written decision) and the code disagree, the Spec states what the **code**
does, and the disagreement is listed here until an ADR or a code fix resolves it
([INDEX.md](../../INDEX.md) §3.1). Phase 2 fills this list as it writes each chapter.

| # | Rule / chapter | The written decision says | The code does | Resolution |
| --: | --- | --- | --- | --- |
| 1 | 03 §3.2 (registration is a listing) | The operator's decision of 2026-09-25 (asynchronous, long-lived, no deadline before a panel, spam priced in MSK) is not in any ADR | TODO: compare with `palw_model_registry_v1.rs` in Phase 2 | an ADR, or a Spec note |
| 2 | 16 §16.3–§16.4 (post-launch fences) | No ADR records the DAA-750 set (13) or the DAA-1,300 set (2) | `PALW_T12_POST_LAUNCH_FENCES_V1` / `_V2` in `params.rs` | one short ADR per flag day |
| 3 | 08 §8.1, 07 §7.3 (panel seed) | ADR-0152 SW-8 prices an anchor re-roll at one block's work, and later one inference | Corrected by `palw_panel_seed_execution` (see `docs/t12-panel-seed-2026-09-25.md`) | **Resolved 2026-09-27:** ADR-0152's short record and its archive README name the correction, and ADR-0154 carries the fence |
| 4 | 02 PALW-ST-7 (carriage) | ADR-0046 D1: one subnetwork id per kind in the band `0x50`–`0x55` | Lifecycle objects ride one id, `SUBNETWORK_ID_PALW_LIFECYCLE` (`0x4b`), as tagged `PalwConsensusObjectV2` values; `0x50`+ is unused | Spec follows the code; ADR-0046's short record notes it |
| 5 | 02 PALW-ST-8 (bond collateral) | ADR-0046 D3: collateral is output 0's value, "read, not declared" | The registration declares `collateral`; the carrier's output must hold at least that much (`palw_bond_registration_binds_its_carrier_v2`) | Spec follows the code; ADR-0046's short record notes it |
