# PALW spec — 14. Node duties

> **Normative.** This chapter states what a conforming node MUST do. Unless a rule says otherwise, the
> rules here are *(node policy)*: they bind a conforming implementation, but no validator checks them.
> Reasoning: [design/palw/node.md](../../design/palw/node.md). Where the code and this text disagree,
> the code is the truth, and the difference goes into [divergences.md](divergences.md).

**Applies to:** every node on a network where PALW is on (testnet-12)
**Reconciled with code at:** `55a7be02f` (2026-09-27)
**Principles served:** P1 (the host surface is local); P5 (runtime backends are free below the semantic
boundary).

## 14.1 Protocol duties (always on)

- **PALW-ND-1 (duties are not options).** A protocol duty MUST be decided by the params and the node's
  identity, and MUST NOT be a start-up flag that can turn it off. A flag MAY choose identity,
  resources, what to produce, or a devnet or drill mode, and nothing else. *(Operator rule of
  2026-09-25.)*
- **PALW-ND-2 (the duties).** The duties are:
  - **Seats:** replay per role and sign receipts (08 PALW-VF-12). File `Unavailable` when a fetch
    fails, and open a DA session before the receipt window closes, or at once when a licence lands on a
    claim that did not serve the seat (08 §8.6).
  - **Producers:** answer every DA session on their claims with `MaterialDisclosedV2`. This V2 DA
    responder is unconditional, and it is a ship condition of every build that arms `palw_rcore_plus`.
    Answer a dissection's root claim and every rung owed (09 PALW-CT-24). Answer leaf requests on the
    interval lane.
  - **Locked `Valid` signers:** retain, and disclose automatically, what a DA session on their claim
    demands.
  - **Filers:** file false-valid, replay and held refutations as they are found. A reporter commits
    before it broadcasts evidence (10 PALW-CO-28).
  - **Round producers and heartbeat miners:** produce round blocks for held permits, and heartbeats
    where the identity calls for them, carrying conviction objects (13 PALW-FC-1).
- **PALW-ND-3 (seat role discipline).** Under `palw_offence_attribution` and `palw_verification_v2`
  (SEAT-R):
  - a full-mask `Valid` comes only from the attempt replay or the free-prompt replay;
  - a replay mismatch is terminal;
  - the material, interval and capture-sample arms are evidence only, never a `Valid` exit;
  - partial seats sign only from the S1 resume;
  - a pooled capture is used only after it verifies against the claim's full roots.

  A seat that sees an S2 licence left un-upgraded near its deadline full-replays and signs a V2 `Valid`
  (not on C7).
- **PALW-ND-4 (the fault ledger).** A seat that proves a fault files nothing else about that claim
  (08 PALW-VF-34).

**Sources:** ADR-0152 Q-7 and DA-7 (the producer's responder), P2-6…P2-8; ADR-0093; ADR-0098 D2; ADR-0111
D6; ADR-0125 D8. **Code:** `kaspad/src/palw_duties.rs`, `palw_panel.rs`, `palw_operator_da.rs`,
`palw_reporter_filer.rs`, `palw_filer_false_valid.rs`, `palw_filer_replay.rs`, `palw_filer_held.rs`,
`palw_round_producer.rs`, `palw_heartbeat_miner.rs`, `palw_fp_seat.rs`.

## 14.2 Block-building policy

- **PALW-ND-5 (licence assembly).** A producer MUST offer a licence set with `basis_k ≥ 2` before an S2
  set, and S2 only when no such set is at hand. It must not stop at the first two votes (the
  floor-licence stall of 2026-09-24). *Code:* `kaspad/src/palw_licence_order.rs`,
  `kaspad/src/palw_receipt_pool.rs`.
- **PALW-ND-6 (producing).** A producer MUST NOT mine an attempt the fold would refuse. It reads the same
  readiness verdict as the fold (`ready_to_produce_v3`) and holds below the producer floor. When it
  holds, it logs the shortfall and the way out: a new bond. *Code:* `kaspad/src/palw_producer.rs`,
  `core/palw_producer_v2.rs`.
- **PALW-ND-7 (readiness).** A seat MUST keep its readiness proofs current, submitting a new
  `SeatReadinessProvedV2` within the horizon. Readiness escalation and the memory ledger govern what it
  holds. *Code:* `kaspad/src/palw_readiness_escalation.rs`, `kaspad/src/palw_memory_ledger.rs`.
- **PALW-ND-8 (pending transactions).** A node announces a pending transaction until it lands (ADR-0115;
  the EVM half is in `spec/evm`).

## 14.3 Sync

- **PALW-ND-9.** IBD MUST hand blocks over parents first, so a tied round-lane child is never delivered
  before its parent. This is a node fix of 2026-09-27, with no fence. testnet-12 nodes carry IBD
  checkpoints at the chain blocks of DAA 100, 200 and 300. First sync SHOULD use trusted public nodes
  (13 PALW-FC-7). *Code:* `protocol/flows/src/ibd/`.

## 14.4 Artifacts, memory and backends

- **PALW-ND-10 (one mapped copy).** A host MUST map an artifact, not read it into anonymous memory, and
  hold one copy of it for all the classes and seats that use it.
- **PALW-ND-11 (the read budget).** A class's weights are read within a budget the operator states. The
  default is a fifth of the artifact. There are two tiers (routed experts, and everything else, with a
  floor), and a layer's experts are read together once its router commits. The class's identity and
  arithmetic are untouched. The node prints what each draw read from storage.
- **PALW-ND-12 (streamed inventories).** A node MUST build an inventory root as a stream of leaves, never
  as a copy of the model. It keeps a Merkle frontier (`PalwArtifactMerkleFrontierV1`) and holds
  row-sized scratch. Registration and measurement stream the same way.
- **PALW-ND-13 (held captures).** A node serves, checks and prosecutes a held capture from its fold as
  the replay streams. It holds two ladders: the class's for walking, and the network's for
  materializing.
- **PALW-ND-14 (backends).** A backend MAY accelerate any catalogued kernel below the semantic boundary.
  There are no kernel certificates. Each backend has a differential gate that must fire on a wrong
  result, and fusion stops at the committed row.
- **PALW-ND-15 (fit).** A node MUST answer a model's fit as a lookup over the chain's own predicates
  (`palw_model_fit_v1`). The entrance states its limits before the first token (`GET /v1/models`). A
  stand-in is a verdict, never a row.

**Sources:** ADR-0136, 0112, 0106, 0121, 0057, 0097. **Code:** `core/palw_model_fit_v1.rs`,
`core/palw_artifact.rs`, `kaspad/src/palw_backends.rs`, `kaspad/src/palw_class_context.rs`.

## 14.5 Host surfaces

- **PALW-ND-16 (the sandbox is for the host).** A worker that runs a stranger's model:
  - starts with no capabilities and is confined by the platform;
  - holds no key;
  - has a memory ceiling and a wall-clock deadline per job.

  Untrusted text cannot become a control token, and a model's output is data on every path. No security
  field enters the priced bytes, and the posture is a local report that nobody signs. The public
  entrance is not a PALW product (ADR-0144).
- **PALW-ND-17 (extensions).** An extension is a manifest that the verifier recomputes
  (`PalwExtensionManifestV1`, RFC 8785 canonical). Every answer names its tier and its verification
  depth. A reproduction receipt is evidence, not a vote. A manifest never activates a ruleset.
  Publication is off chain. See [palw-extension-envelope.md](../../palw-extension-envelope.md).
- **PALW-ND-18 (the operator interface).** `misaka` exposes purposes (mine, verify, validate, list a
  model, hold a position), not components:
  - one work id follows a job from request to reward;
  - one line says why nothing is happening;
  - human errors come from one catalogue;
  - setup is resumable;
  - events are structured.
- **PALW-ND-19 (money must not lock in).** Operator tooling MUST let an operator see and retire what it
  bonded: `bond status` and `bond retire` show the locks and the release DAA, and `misaka model add`
  never charges for a registration the chain will drop.

**Sources:** ADR-0079 D1–D13 (as aligned by 0144), ADR-0108 D1–D9, ADR-0122 D1–D10, ADR-0063 (wallet;
Phase 3), the launch note §00. **Code:** `misaka-cli/`, `kaspad/src/palw_agent.rs`,
`misaka-palw-extension/`.

**Activation.** Node policy ships with the binary. The duties in 14.1 bind from the fence height of the
rule each duty serves.
