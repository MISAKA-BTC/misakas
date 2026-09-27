# ADR-0035: The public PALW testnet is testnet-11, continued — and it pins its determinism class at the door

> **Body moved (2026-09-27).** Normative rules → [spec/palw/16](../spec/palw/16-network-parameters-and-fences.md); the full text as written → [design/palw/archive/0035-palw-public-testnet-strategy.md](../design/palw/archive/0035-palw-public-testnet-strategy.md); the reasoning is summarised in [design/palw/lineage.md](../design/palw/lineage.md).

* Status: Accepted (implemented 2026-08-17). **Superseded in part:** Decision 1 ("the soaking chain is
  the public chain; no re-genesis at announce") held for Relaunch 1 only. ADR-0042's two-network split
  re-genesises a public RC on any rule change, and testnet-11 was re-minted as Relaunches 2–5e. The
  algo-4 `LegacyTn11` lane it launched runs nowhere. Decision 2 stands as the template for pinning a
  network's classes.
* Date: 2026-08-17

## Context

Track A's gate 5: announce the public PALW testnet. The evidence base was the algo-4 forgery audit,
the cross-host determinism measurement, the difficulty economics, and the soak (gates 1–4).
`algo_id = 4` had no boot-time calibration, so a drifted worker runtime would fork itself off
silently.

## Decision

- **D1 — The public PALW testnet is testnet-11, the current chain** (no re-genesis at announce).
  *Superseded by ADR-0042.*
- **D2 — Class admission is pinned in code.** A per-network pin table, `palw_worker_calibration_v1`,
  holds the probe seed `POW_L1_PALW_PROBE_SEED_V1` and the calibration tag
  `POW_L1_PALW_WORKER_CALIBRATION_TN11_V1`. testnet-11 pins the x86-64 CPU class. devnet pins nothing.
  Every future public PALW network adds its row before launch.
- **D3 — The participation model is stated honestly:** who can join, on which hardware class.
- Launch economics and the operator items were recorded, not decided in code (§5–§6 of the full text).

## Consequences

- Track A's remaining work became the operator items and the announce. Track B (credits, the VLT
  overlay) stayed decoupled.
- The class-pin table became the template for any public PALW network. Registry-era networks
  (testnet-12) admit classes by registration instead (spec 03).

## Links

- Spec: [16 Network parameters and fences](../spec/palw/16-network-parameters-and-fences.md)
- Design: [design/palw/lineage.md](../design/palw/lineage.md)
- Full text as written: [design/palw/archive/0035-palw-public-testnet-strategy.md](../design/palw/archive/0035-palw-public-testnet-strategy.md)
