# Remaining RFCs (Workstream C) — integration matrix

Owner: Lead (single Lead for workstreams A, B and C). Baseline `pre` @ `082636b64`; integration branch
`claude/g14-public-prosecution-integration-9bee39`. Inventory of 2026-10-08 against the code at `fcde1e3dc`.

Statuses (only these): **IMPLEMENTED_AND_TESTED** (real node path + tests) · **IMPLEMENTED_REFERENCE_ONLY** · **DORMANT_NOT_INTEGRATED**
(code behind an unarmed fence / no node caller) · **CODE_GAP** · **DESIGN_GAP** · **EXTERNAL_GATE_PENDING** · **DEFERRED_BY_USER**.
A fixture PASS, a reference engine or an RFC text is never production completeness.

Out of scope here: RFC-0004 (self-improvement), RFC-0012 (DNS/BFT retirement), RFC-0015 activation — **DEFERRED_BY_USER**.
Deferring RFC-0012 does not make DNS/BFT a dependency of any new path. Workstream A owns public prosecution, objective DA
demand/default and EXEC slices; workstream B owns model registration/admission and the conformance beacon. One evidence manifest,
one challenge contract (`misaka-palw-challenge`), one lifecycle state set across A/B/C.

## 1. Matrix

Columns: requirement · current implementation · missing production path · workstream/agent · target · dependency · G14 impact ·
wire/fence impact · test · real-node result · status.

### RFC-0009 remote miner (C1)

| Requirement | Current | Missing production path | Owner | Target | Depends on | G14 impact | Wire/fence | Test | Real node | Status |
|---|---|---|---|---|---|---|---|---|---|---|
| Node-less registration: quote, pre-sign gate, relay fan-out, tamper refusal, tracker | `misaka-palw-remote/src/{register,relay}.rs`, CLI `model add --quote --relay` | detached/offline signing (unsigned object export + signature import; seed never in builder); fee payer ≠ bond key; multi-RPC quote; ruleset id + expiry inside the signed object; registrant bond in class-row RPC; lifecycle states (Dormant/WaitingRandomness/…) from the shared contract | C1 (client), B-D (consensus registration) | `misaka-cli/src/operator/model_{add,remote}.rs`, `misaka-palw-remote` | B lane D registration E2E | none (registration ≠ Active) | class-row RPC field | 6+4 unit | not run | CODE_GAP (core flow IMPLEMENTED_REFERENCE_ONLY) |
| Remote ordinary attempt | `misaka-palw-remote/src/{attempt,template}.rs` (mount, stale/dissent checks, idempotent submit) | a remote-miner binary; executor extraction from `kaspad/src/palw_backends`; reorg-aware attempt tracking over RPC | C1 | new bin in `misaka-palw-remote` | — | claims still pass G14 | none | 11 unit | none | IMPLEMENTED_REFERENCE_ONLY |
| Remote free-prompt claim | `misaka-palw-gateway/src/bin/rail.rs` `--relay`/`--track` | sidecar signing of the funding input; material path independent of the miner; network bundle (not devnet) pricing | C1 | `rail.rs`, `misaka-palw-fp-submit` | DA row | material must be public for outsiders | none | watch-loop only | none | CODE_GAP |
| Independent DA transport | `palw_evidence_v1.rs` manifest; `misaka-palw-remote/src/evidence.rs` (fetch any, fs provider); seat-only `--palw-evidence-provider-dir` | network transport, multi-provider upload/fetch, availability status, retention monitoring after miner offline; same bytes/root for node, Panel and public verifier; `misaka-model-transport` absent | C1 (adapter) / A (objective demand/default) | `misaka-palw-remote`, `kaspad/src/palw_root_fetch.rs` | A: public material RPC + objective default | **direct**: outsiders need it | RFC-0009 reserved tags 150–153, deltas 140–141, tail 0xE9 | 6+4 unit | seat-only | CODE_GAP; provider court DORMANT_NOT_INTEGRATED; discovery DESIGN_GAP |
| Public receipt redemption V4 (builder-independent) | `palw_receipt_v4.rs` (auth, split, fee cap 1000 bps), header/processor/coinbase wired, node builder mode, rail `--redeem-auth-out` | arming plan + T12 drill; discovery/relay of authorizations (gossip/RPC); fee-cap market test; retiring-bond executors | C1 | `palw_receipt_v4.rs`, `kaspad/src/palw_producer.rs` | — | none | fence `palw_receipt_spend_v4` (None everywhere) | 8 unit + processor + fence | not armed | DORMANT_NOT_INTEGRATED (V3 live, unchanged) |
| Remote client verification | rail quorum (`view.rs` agree), unsigned checkpoint pin | `UNVERIFIED_REMOTE_STATE` labelling everywhere a proof is absent; expose `palw_state_proof_v1` over RPC and verify it client-side; header-chain verification | C1 | `misaka-palw-remote/src/{view,checkpoint,proof}.rs`, rpc | — | none | RPC op | 6+2+1+3 unit | none | IMPLEMENTED_REFERENCE_ONLY (proofs) / CODE_GAP (light client) |

### RFC-0010 permissionless Panel (C2)

| Requirement | Current | Missing production path | Owner | Target | Depends on | G14 impact | Wire/fence | Test | Real node | Status |
|---|---|---|---|---|---|---|---|---|---|---|
| Fence refusal | `palw_permissionless_panel_v1` refused on every preset and custom | keep refusing until beacon + fold gates pass | C2 | params | — | — | Some-only identity | 5 core tests | n/a | IMPLEMENTED_AND_TESTED (as refusal) |
| Seal / snapshot / assignment / retry engine | `misaka-palw-panel` (reference transition engine) | — (do not re-implement) | C2 | — | — | — | — | panel tests | reference | IMPLEMENTED_REFERENCE_ONLY |
| Production fold (state root, delta, carriage tail, pruning/IBD, ordering from V2 acceptance) | none in `PalwChainStateV2` | V3 sub-state in `PalwChainStateV2` with Some-only root block, delta entries, carriage tail; "drop and the block stands" for bad carried beacons; acceptance order from the V2 mergeset | C2 (Lead allocates tags) | `palw_state_v2.rs`, `processor.rs`, `palw_permissionless_panel_v1.rs` | Lead tag registry | V3 claims must stay accusable (below) | tags 120–129, deltas 170–179, tail 0xED (allocated §2) | — | — | CODE_GAP |
| Bond reservation | private `reservations` map | one exposure ledger with V2 `reserved_exposure` / seat duties (no double use during the lane-A drain), slashable locks | C2 | `palw_state_v2.rs` | A kernel-route bond mapping uses the same ledger | collateral family 20 | delta | — | — | CODE_GAP |
| Receipt / court handoff | `terminal_claim` callback only | V3 binding produces V2 `PalwPanelStateV2` (+ duty rows, RFC-0006 shard record) or a versioned equivalent, so V2 receipts, DA accusations (`DaClaimNotAccusable` today) and courts work | C2 | `palw_state_v2.rs` PanelBound arm | — | **direct**: without it a V3 claim cannot be accused | — | — | — | CODE_GAP |
| Production beacon | adapter always rejects; test certificates only | a unique, bias-bounded, costed, independently verifiable source. PALW Work Beacon with `SubjectKindV1::PanelAssignment` accepts only `FinalPathV1::PanelIndependent` sources (`fcde1e3dc`) — none exist until a Panel-independent Final path exists, so the beacon is BEACON_UNAVAILABLE and the fence stays dormant | C2 + Lead (contract) | `misaka-palw-challenge`, `palw_permissionless_panel_v1.rs` adapter | A (Panel-independent Final) | circularity work→Panel→Final→beacon→Panel excluded by construction | contract | 11 contract tests | — | DESIGN_GAP + EXTERNAL_GATE_PENDING (bias/withholding/P0-10 review) |
| Objective L1 seal/finality rule | seal depth only (doc: not a finality primitive) | — | C2 | RFC-0010 §3.1 | — | — | — | — | — | DESIGN_GAP |
| Void/payout mapping of `SealUnavailable`/`BeaconUnavailable` | none | `PalwVoidReasonV2` versioned counterparts, refunds | C2 | `palw_state_v2.rs` | — | — | delta | — | — | CODE_GAP |
| Legacy lane-A drain; operator-anchor privilege after fence | `panel_claim_rule_v1` uncalled; `operator_of_v1` unversioned | version the anchor functions in `processor.rs` so post-fence claims never inherit operator privilege; drain bound | C2 | `processor.rs` (~13400–13661) | — | — | fence | — | — | DORMANT_NOT_INTEGRATED / DESIGN_GAP (drain time) |
| State growth | `work_ids` never compacted | authenticated compaction | C2 | `misaka-palw-panel` | — | — | — | — | — | DESIGN_GAP |
| RPC/CLI observations | serde JSON of engine state | versioned RPC ops | C2 | rpc | — | — | RPC | — | — | CODE_GAP |

### RFC-0006 layer-sharded Panel (C2)

`palw_tir_shard_v1` is **armed on testnet-12 at DAA 5,300** (`params.rs` int-11/12 list); several docs that call it dormant are stale.

| Requirement | Status | Missing |
|---|---|---|
| canonical cell identity, carry-in, history/checkpoints, shard weights, cell assignment, coverage, receipts by parts, exact court (`TirShardCourtAccused`, any Active bond) | IMPLEMENTED_AND_TESTED (fold + kaspad e2e) | hashed cell id only if RFC-0007 scope receipts need it |
| per-cell readiness / resource pricing per segment | IMPLEMENTED_AND_TESTED at shard granularity | per-segment pricing (history grows with position): CODE_GAP |
| boundary/state fraud localization by a NON-seat public watcher | CODE_GAP (seat duty / shadow only) | public watcher path (shares A's fresh-verifier engine) |
| processor/T12Chain-level reorg + duplicate receipt test | CODE_GAP | virtual-processor test; D-S1…D-S6 drill evidence: EXTERNAL_GATE_PENDING |
| V3 per-shard draw (stratified seats) | DESIGN_GAP | depends on RFC-0010 beacon |
| stale "dormant" docs | doc fix | — |

### RFC-0001 inference surface / RFC-0003 generative classes (C3)

| Requirement | Current | Missing | Status |
|---|---|---|---|
| FP Job V4 (Implementation Frozen §A) | `palw_decode_pipeline_v4.rs`, vectors `consensus-vectors/fp-v4`, seat + gateway G6, t12 e2e with fence armed | gates G8–G10 (salted flag-day drill, release, activation) | DORMANT_NOT_INTEGRATED (frozen semantics must not change) |
| Serving surface (chat/embeddings, bounded queue, SSE equals commitment, outbox, rail submit) | gateway + rail | idempotency key / dedup; cancellation on disconnect; Retry-After header bug (`main.rs` 503 without header) | IMPLEMENTED_AND_TESTED; idempotency/cancellation DESIGN_GAP; header CODE_GAP |
| Generative jobs / multimodal pipelines (R, V5 job, gen courts, workers, gateway VLM in-process) | consensus + node + gateway libraries | fences `palw_gen_v1`, `palw_fp_job_v5` unarmed; preprocessing (resize/letterbox) in gateway; image-generation API; audio/video spec-only; image prices open | DORMANT_NOT_INTEGRATED; preprocessing/API CODE_GAP; audio/video DESIGN_GAP |

### Cross-workstream G14 boundary

| Boundary | Status |
|---|---|
| Kernel route on the real node (A) | CODE_GAP (lane B per-object API → Lead tags → lane D fold) |
| Public material read for outsiders (A row 16; C1 DA adapter consumes it) | CODE_GAP |
| V3/Panel=0 claims accusable without `state.panels` | CODE_GAP (C2 handoff) |
| One exposure ledger: V2 seats, V3 reservations, kernel-route claims/demands | CODE_GAP |
| Redemption V4 vs work-slice credit: one claim credited once | DESIGN_GAP (EXEC v2 not built) |

## 2. Consensus allocation registry (Lead-owned; explicit numbers survive merge order)

| Owner | Object tags | Delta numbers | Carriage tail | Fence |
|---|---|---|---|---|
| existing (pre) | positional ≤ 82, declared 83–95, 100–103 | … ≤ 150 (150 = lane MU seat root readiness) | 0x87–0xDA (see `PALW_CARRIAGE_*`), 0xE0, 0xE1, 0xE4, 0xE6, 0xEA, 0xEB | — |
| RFC-0009 B/D (reserved, spec) | 150–153 | 140–141 | 0xE9 | provider court (unallocated) |
| Kernel route (A, G14) | 110–119 | 160–169 | 0xEC | `palw_probabilistic_constraints_v1` (refused) |
| RFC-0010 V3 production fold (C2) | 120–129 (120 = `PanelBeaconProofV3`) | 170–179 (170–173 used) | 0xED | `palw_permissionless_panel_v1` (refused); `PalwVoidReasonV2` 120–129 (120–122 used; 0–10 implicit unchanged) |
| EXEC payload v2 (A, later) | 130–139 | 180–189 | 0xEE | `palw_exec_payload_v2` (not yet declared) |

No lane edits `PalwConsensusObjectV2`, `PalwDeltaEntryV2`, the root preimage or carriage tails without a Lead commit that adds the
skeleton first; lanes build on that commit.

## 3. Waves

0. Scope/contracts — this matrix; `fcde1e3dc` (Panel-assignment subject + circularity rule).
1. Remote entry — C1 detached signing / node-less registration / remote claim; C3 gateway → canonical job → signed claim; C4 malicious relay fixtures.
2. Evidence independence — C1 adapter on A's public material read + objective default.
3. Permissionless claim completion — C2 production fold; beacon stays dormant (BEACON_UNAVAILABLE) until a Panel-independent Final exists.
4. Redemption — C1 V4 discovery/relay, owner payout, fees, V3 compatibility.
5. Inference and sharding — C3 idempotency/cancellation/preprocessing; C2 non-seat cell watcher, per-segment pricing.
6. Real-node adversarial — C4; Lead runs the single final full regression.

## 4. Change log

* 2026-10-08 — **C1 final integrated** (11 commits, `rfc-0009-remote-record.md`). Status by row (tests: remote 71 lib + 2 CLI e2e,
  CLI 8, pq-validator-core 48, pq-signer 17, rpc op-202, real processor `t12_state_proof` 1 + `rfc9_redemption_v4` 5; nothing run
  against a live node/network):
  * Node-less registration (export/sign/submit on separate machines, bond key ≠ payer key, tamper refusal, ≥2-node quotes,
    resend vs duplicate, shared lifecycle codes) — IMPLEMENTED_AND_TESTED (client) / consensus binding of expiry + ruleset
    **DESIGN_GAP (G-EXPIRY, G-RULESET: signed `valid_until_daa` + ruleset in the registration object + fence; Lead)**.
  * Remote verification: `getPalwStateProof` (op 202) proves one collection against the committing header; client verifies against a
    pinned block hash; `UNVERIFIED_REMOTE_STATE` labelling; registrant bond read from the proven class record —
    IMPLEMENTED_AND_TESTED; header-chain PoW verification CODE_GAP; O(rows) proofs (flat commitment) DESIGN_GAP.
  * Remote attempt: `palw-remote-miner` (external `--executor-cmd`, signs once on a win, quorum recheck, reorg tracking) —
    IMPLEMENTED_REFERENCE_ONLY (no live run); backend extraction from kaspad CODE_GAP.
  * Remote FP claim: `rail --signer-socket` (seed never in the rail; `MessageSigner` refactor), `rail --relay-signed` —
    IMPLEMENTED_AND_TESTED (unit/local); V4 authorization from a sidecar needs a new `SigningPurpose` variant in
    `consensus-core::dns_finality` (Lead decision; recorded GAP).
  * DA transport: HTTP/dir providers, read-back upload, availability, repair, retention monitor; one fetch shared by node, Panel and
    public verifier (`evidence::fs::fetch_claim_material` delegates) — IMPLEMENTED_AND_TESTED (local); https untested; on-chain
    provider discovery DESIGN_GAP; `misaka-model-transport` absent CODE_GAP; kaspad provider flag dirs-only.
  * V4 redemption: authorizations on providers mirrored to the builder dir; fence test-armed: different bonds, miner offline, quantum
    once, payout to the registered address, fee carved from the worker reward, revert undoes use, round permits untouched, V3
    unchanged — DORMANT_NOT_INTEGRATED (fence None) with processor-level tests; full chain-block acceptance test CODE_GAP; arming
    plan / T12 drill / fee-cap market EXTERNAL_GATE_PENDING.

* 2026-10-08 — C2 milestone 1 (`rfc10/c2-panel` 5e294cdcc, d6f93e68b, 8f8410b0f; not yet integrated): V3 engine staged
  (advance / accept_beacon / admit); beacon adapter = borsh `WorkBeaconV1` verified by `verify_work_beacon_v1`; production fold —
  `PalwChainStateV2.panel_v3` Some-only root block `panel_v3/v1`, deltas 170–173, tail 0xED, tag 120 (bad proof dropped, block
  stands), void reasons 120–122; one exposure ledger via `reserve_seat_duties_with` (V3 binding writes the V2 Panel record anchored
  at the V3 seed, duty rows and seat `reserved_exposure`); a non-seat DA accusation + default convicts a V3-bound claim
  (ProducerWithholding, producer slashed). Open: V3 receipt/retry expiry vs non-seat DA default (G14 pre-emption — Lead asked for a
  structural guard + test); sharded classes end PermissionlessNoCapablePanel (per-shard V3 draw = RFC-0006 DESIGN_GAP).

* 2026-10-08 — C1 milestone 1 (`rfc9/c1-remote` d7f206a58, a8e073c5f; not yet integrated): detached registration —
  `model add --export-bundle` (no key read, ≥2 agreeing quote nodes unless `--allow-single-rpc`), offline `model sign` (bond key ≠
  payer key allowed; tampered change recipient/fee/class root/owner refused; `--yes` needs expect flags), `model submit` (verifies
  bytes alone; same-bytes resend idempotent; another carrier for the same class refused); registry rows show REGISTERED_DORMANT.
  Fixed: terms digest included `tip_daa`, tripping the pre-sign gate every block. 51 remote + 8 CLI tests; no real-node run yet.
  **Consensus gaps recorded (Lead, need a fence):** G-EXPIRY — the owner signature covers no expiry and lock time is not evaluated
  for the lifecycle input, so a leaked signed bundle stays valid until its funding input is spent (needs signed
  `valid_until_daa` + fence); G-RULESET — `consensus_params_id` is not signed (client-side check only). Approved: additive
  `registrant_bond` in the class-row RPC.

* 2026-10-08 — created from two code inventories; contract `fcde1e3dc`.
