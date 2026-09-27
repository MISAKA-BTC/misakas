# PALW spec — 14. Node duties

> **Skeleton (Phase 1, 2026-09-27).** [00-index.md](00-index.md) gives the conventions. The rules here
> are *(node policy)* unless they say otherwise. They bind a conforming node, but no validator checks
> them.

**Purpose.** Some protocol duties can only be done by nodes: answering for a seat, filing what a seat
found, responding in a court, serving data for DA, producing round blocks and heartbeats. **A
protocol duty is always on.** It is decided by the params and the node's identity. It is never a
start-up option, and it cannot be switched off. A flag may choose identity, resources, what to
produce, or a devnet or drill mode, and nothing else. This chapter lists each duty, the policy a node
follows when building blocks (licence assembly, receipt pooling, readiness escalation), and the
host-side surfaces: artifacts and memory, the extension manifest, and the operator interface.

**Principles served:** P1 (the host surface is local), P5 (runtime backends are free below the
semantic boundary).

## 14.1 Protocol duties (always on)

- [ ] The duty list and who owes each one: seat answer and receipt, the DA answer (for seats, and the
  producer's DA responder, unconditionally), the court responder (fused rows), the false-valid and
  replay filers, round-block production, heartbeat mining where the identity calls for it. *Sources:*
  0152 Q-7 (node roles) and IA (the DA responder), 0093 (the responder), 0098, 0125, the operator's
  rule of 2026-09-25 (duties are not flags). *Code:* `kaspad/src/palw_duties.rs`,
  `kaspad/src/palw_panel.rs`, `kaspad/src/palw_operator_da.rs`, `kaspad/src/palw_reporter_filer.rs`,
  `kaspad/src/palw_filer_false_valid.rs`, `kaspad/src/palw_filer_replay.rs`,
  `kaspad/src/palw_filer_held.rs`, `kaspad/src/palw_round_producer.rs`,
  `kaspad/src/palw_heartbeat_miner.rs`.
- [ ] Serving: a seat may demand the committed leaf it needs, so a node serves the leaves it
  committed. A held capture is served from its fold as the replay streams. *Sources:* 0111, 0121.

## 14.2 Block-building policy

- [ ] Licence assembly: which receipts and licences a producer includes, and in what order. This must
  not be greedy first-two-votes (the floor-licence stall of 2026-09-24). *Code:*
  `kaspad/src/palw_licence_order.rs`, `kaspad/src/palw_receipt_pool.rs`.
- [ ] Readiness escalation and the memory ledger. *Code:* `kaspad/src/palw_readiness_escalation.rs`,
  `kaspad/src/palw_memory_ledger.rs`.
- [ ] The producer: attempts, anchors, and when an operator's own attempt anchors (chapter 07 §7.3).
  *Code:* `kaspad/src/palw_producer.rs`, `core/palw_producer_v2.rs`.
- [ ] A pending transaction is announced until it lands. *Sources:* 0115 (EVM; specified in `spec/evm`
  and cited here).

## 14.3 Sync

- [ ] IBD hands blocks over parents first. Tied round-lane blue work must not deliver a child before
  its parent. This is a node fix of 2026-09-27 with no fence. IBD checkpoints: chain blocks at
  DAA 100, 200 and 300 on testnet-12. *Sources:* the launch note §00. *Code:* `protocol/flows/src/ibd/`.

## 14.4 Artifacts, memory and backends

- [ ] An artifact is mapped, not read, and a host holds one copy. *Sources:* 0136.
- [ ] An inventory is built as a stream of leaves, not a copy of the model. *Sources:* 0106.
- [ ] A class's weights are read within a budget the operator states, and the budget is a fifth of the
  artifact. *Sources:* 0112.
- [ ] A model's fit is a lookup, and the entrance says its limits before the first token. *Sources:*
  0097. *Code:* `core/palw_model_fit_v1.rs`.
- [ ] Runtime backends sit below the semantic boundary, with no kernel certificates and a differential
  gate per backend. *Sources:* 0057. *Code:* `kaspad/src/palw_backends.rs`.

## 14.5 Host surfaces

- [ ] The sandbox is for the host. A pure function needs no permissions. *Sources:* 0079 (as amended by
  0144).
- [ ] An extension is a manifest the verifier recomputes. *Sources:* 0108. See also
  [palw-extension-envelope.md](../../palw-extension-envelope.md).
- [ ] The operator interface: one command, one work id, one error catalogue. *Sources:* 0122 (partly
  built). *Code:* `misaka-cli`, `kaspad/src/palw_agent.rs`.
- [ ] Operator tooling and keys: the gap that locks money in. *Sources:* 0063 (proposed), and the CLI
  fixes in the launch note §00 (`misaka model add`, `bond status`/`retire`, `verifier setup`).

**Activation.** Node policy ships with the binary. The rules in 14.1 bind from the fence height of
the rule each duty serves.

**Design:** `design/palw/node.md`.
