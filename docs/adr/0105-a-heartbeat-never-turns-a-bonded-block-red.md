# ADR-0105 — A heartbeat never turns a bonded block red, and the clock steps aside for a draw that has landed

> **Body moved (2026-09-27).** Normative rules → [spec/palw/13](../spec/palw/13-fork-choice-and-heartbeat.md); the full text as written → [design/palw/archive/0105-a-heartbeat-never-turns-a-bonded-block-red.md](../design/palw/archive/0105-a-heartbeat-never-turns-a-bonded-block-red.md); the reasoning is summarised in [design/palw/liveness.md](../design/palw/liveness.md).

* Status: Proposed 2026-09-11. **D1 is implemented behind `palw_heartbeat_transparent`**: `None` on
  testnet-11's shipped preset, and armed on testnet-12 from genesis. D2 (node policy) ships in the
  binary. **Tightened by ADR-0154:** from DAA 750 the transparency stops at the merging block's own
  chain (`palw_heartbeat_transparent_same_chain`).
* Date: 2026-09-11

## Context

From 11:41Z to about 13:20Z on 2026-09-10, testnet-11 ran on heartbeats alone. A bonded block that
took seventeen minutes to draw landed eight heartbeats behind the tip. At `ghostdag_k = 1` it was
coloured red, so nothing inside the chain could end the stall. DNS finality stopped, and the EVM
bridge stayed paused.

## Decision

- **D1 — A heartbeat never turns a bonded block red** (consensus, fenced). GHOSTDAG colours by lane:
  Classic, Weighted (heartbeats invisible) or Heartbeat. The exemption stops at the merge-depth
  window. → spec 13 PALW-FC-15.
- **D2 — The heartbeat miner steps aside for a draw that has landed** (node policy). → spec 13
  PALW-FC-14.
- **D3 — Operators:** what to run while the fence is dormant.

## Consequences

- Slow producers are no longer trapped by fast heartbeats.
- The double spend this transparency allowed was found after the testnet-12 launch and closed at
  DAA 750.

## Links

- Spec: [13 Fork choice and heartbeat](../spec/palw/13-fork-choice-and-heartbeat.md)
- Design: [design/palw/liveness.md](../design/palw/liveness.md)
- Full text as written: [design/palw/archive/0105-a-heartbeat-never-turns-a-bonded-block-red.md](../design/palw/archive/0105-a-heartbeat-never-turns-a-bonded-block-red.md)
