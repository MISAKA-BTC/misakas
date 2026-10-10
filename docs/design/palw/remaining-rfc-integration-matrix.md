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
| Production fold (state root, delta, carriage tail, pruning/IBD, ordering from V2 acceptance) | none in `PalwChainStateV2` | V3 sub-state in `PalwChainStateV2` with Some-only root block, delta entries, carriage tail; "drop and the block stands" for bad carried beacons; acceptance order from the V2 mergeset | C2 (Lead allocates tags) | `palw_state_v2.rs`, `processor.rs`, `palw_permissionless_panel_v1.rs` | Lead tag registry | V3 claims must stay accusable (below) | tags 120–129, deltas 170–179, tail 0xED (allocated §2) | `rfc0010_production_fold.rs` 18 (fold, whole life, G14 handoff), `stages.rs` 5 (engine), processor e2e 4 | T12Chain harness: fence armed by bypass + `#[cfg(test)]` beacon hook (heartbeat bind, IBD, reorg, below-fence drop); no real network | IMPLEMENTED_AND_TESTED (fold + processor harness); DORMANT_NOT_INTEGRATED (fence refused at every real height) |
| Bond reservation | private `reservations` map | one exposure ledger with V2 `reserved_exposure` / seat duties (no double use during the lane-A drain), slashable locks | C2 | `palw_state_v2.rs` | A kernel-route bond mapping uses the same ledger | collateral family 20 | delta | `rfc0010_production_fold.rs`: both directions (V3 vs lane-A duty cannot spend one sompi), exposure released on every end | — | IMPLEMENTED_AND_TESTED (V2 seats + V3 reservations share `reserved_exposure`; kernel-route bonds: CODE_GAP, A) |
| Receipt / court handoff | `terminal_claim` callback only | V3 binding produces V2 `PalwPanelStateV2` (+ duty rows, RFC-0006 shard record) or a versioned equivalent, so V2 receipts, DA accusations (`DaClaimNotAccusable` today) and courts work | C2 | `palw_state_v2.rs` PanelBound arm | — | **direct**: without it a V3 claim cannot be accused | — | `rfc0010_production_fold.rs`: non-seat public bond accuses and convicts a V3-bound claim (`ProducerWithholding`); receipt clock paused while any session is open | — | IMPLEMENTED_AND_TESTED (fold); RFC-0006 shard record for a V3 claim: DESIGN_GAP |
| Production beacon | adapter always rejects; test certificates only | a unique, bias-bounded, costed, independently verifiable source. PALW Work Beacon with `SubjectKindV1::PanelAssignment` accepts only `FinalPathV1::PanelIndependent` sources (`fcde1e3dc`) — none exist until a Panel-independent Final path exists, so the beacon is BEACON_UNAVAILABLE and the fence stays dormant | C2 + Lead (contract) | `misaka-palw-challenge`, `palw_permissionless_panel_v1.rs` adapter | A (Panel-independent Final) | circularity work→Panel→Final→beacon→Panel excluded by construction | contract | `rfc0010_beacon_adapter.rs` 7 + 11 contract tests | — | adapter IMPLEMENTED_AND_TESTED (today's chain: BEACON_UNAVAILABLE; refuses Panel-licensed, heartbeat, BASE-0, EXEC, self-candidate, duplicate, reordered); source DESIGN_GAP + EXTERNAL_GATE_PENDING (bias/withholding/P0-10 review: `rfc-0010-production-path-record.md` §3) |
| Objective L1 seal/finality rule | seal depth only (doc: not a finality primitive) | — | C2 | RFC-0010 §3.1 | — | — | — | — | — | DESIGN_GAP |
| Void/payout mapping of `SealUnavailable`/`BeaconUnavailable` | none | `PalwVoidReasonV2` versioned counterparts, refunds | C2 | `palw_state_v2.rs` | — | — | delta | `rfc0010_production_fold.rs`: SealUnavailable / BeaconUnavailable / NoCapablePanel / PanelUnavailable, no slash, no strike, no E-4 hold | — | IMPLEMENTED_AND_TESTED (void reasons 120–122 appended, 0–10 unchanged) |
| Legacy lane-A drain; operator-anchor privilege after fence | `panel_claim_rule_v1` uncalled; `operator_of_v1` unversioned | version the anchor functions in `processor.rs` so post-fence claims never inherit operator privilege; drain bound | C2 | `processor.rs` (~13400–13661) | — | — | fence | processor: `t12_permissionless_panel_e2e` (legacy claim bound by lane A across the fence), regression `t12_operator_anchor_fence`, `t12_bind_deadlock`, `t12_stake_draw_integration`, `t12_panel_seed_fence` | — | IMPLEMENTED_AND_TESTED (versioned door `palw_v2_anchor_fact_for_claim_v1`: post-fence claims never inherit operator anchoring); DORMANT; drain bound (time): DESIGN_GAP |
| State growth | `work_ids` never compacted | authenticated compaction | C2 | `misaka-palw-panel` | — | — | — | — | — | DESIGN_GAP |
| RPC/CLI observations | serde JSON of engine state | versioned RPC ops | C2 | rpc | — | — | RPC | `rfc0010_production_fold.rs` 3 (read model), rpc-core wRPC round-trip 2, gRPC round-trip 1, processor read equals pure function; CLI parse + render 2 | — | IMPLEMENTED_AND_TESTED: wRPC op 220 `getPalwPanelV3Status`, gRPC 1254/1255, `misaka palw panel-v3` (versioned JSON observation; typed protobuf: not built) |

### RFC-0006 layer-sharded Panel (C2)

`palw_tir_shard_v1` is **armed on testnet-12 at DAA 5,300** (`params.rs` int-11/12 list); several docs that call it dormant are stale.

| Requirement | Status | Missing |
|---|---|---|
| canonical cell identity, carry-in, history/checkpoints, shard weights, cell assignment, coverage, receipts by parts, exact court (`TirShardCourtAccused`, any Active bond) | IMPLEMENTED_AND_TESTED (fold + kaspad e2e) | hashed cell id only if RFC-0007 scope receipts need it |
| per-cell readiness / resource pricing per segment | IMPLEMENTED_AND_TESTED at shard granularity (work share per cell already prices position growth) | per-segment *residency* pricing: IMPLEMENTED, dormant behind `palw_tir_shard_segment_v2` (lock/pay = max(work, resident), keyed on `accepted_daa`) — was DESIGN_GAP — proposal for a new versioned fence in `rfc-0010-production-path-record.md` §6; the armed fence (DAA 5,300) is unchanged |
| boundary/state fraud localization by a NON-seat public watcher | IMPLEMENTED_AND_TESTED, dormant (`--palw-tir-shard-watch`, `palw_tir_shard_watch_duties_v1`; SHARD 7b3c0b306) — was CODE_GAP (seat duty / shadow only; `TirShardCourtAccused` is already open to any Active bond) | public watcher path (shares A's fresh-verifier engine and the public material read) |
| processor/T12Chain-level reorg + duplicate receipt test | fold level IMPLEMENTED_AND_TESTED (`palw_tir_shard_fold.rs`: parts are branch-local, reverted exactly, the same receipts fold again on the other branch, duplicate part refused by name); processor level CODE_GAP | needs an IR class with a shard plan and a signed `ReceiptV4` chain in the T12 harness; D-S1…D-S6 drill evidence: EXTERNAL_GATE_PENDING |
| V3 per-shard draw (stratified seats) | IMPLEMENTED_AND_TESTED, dormant (strata in misaka-palw-panel; per-shard records at bind; G14 pre-emption guard; SHARD ae92729bf/d37419308) | depends on RFC-0010 beacon |
| stale "dormant" docs | DONE (RFC-0006 status note, `palw_tir_shard_v1.rs`, `palw_tir_shard_fold_v1.rs`, `params.rs` comments) | — |

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
| V3/Panel=0 claims accusable without `state.panels` | IMPLEMENTED_AND_TESTED at the fold (C2: a V3 binding writes the V2 panel, a non-seat public bond accuses and convicts; before the bind the claim is `DaClaimNotAccusable`, as V2); dormant with the fence |
| One exposure ledger: V2 seats, V3 reservations, kernel-route claims/demands | V2 seats + V3 reservations IMPLEMENTED_AND_TESTED (C2); kernel-route claims/demands CODE_GAP (A) |
| Redemption V4 vs work-slice credit: one claim credited once | DESIGN_GAP (EXEC v2 not built) |

## 2. Consensus allocation registry (Lead-owned; explicit numbers survive merge order)

| Owner | Object tags | Delta numbers | Carriage tail | Fence |
|---|---|---|---|---|
| existing (pre) | positional ≤ 82, declared 83–95, 100–103 | … ≤ 150 (150 = lane MU seat root readiness) | 0x87–0xDA (see `PALW_CARRIAGE_*`), 0xE0, 0xE1, 0xE4, 0xE6, 0xEA, 0xEB | — |
| RFC-0009 provider court (DA16) | **150–153 used** (lease, challenge, answer, transfer) | 140–141 **released** | 0xE9 **released** (rows ride kernel route aux 43–45, delta 160, tail 0xEC) | `palw_provider_court_v1` (dormant) |
| Kernel route (A, G14) | 110–119 (110–112 used; **113 reserved for G14-R4** accusation seal, 2026-10-08) | 160–169 | 0xEC (aux tables: 36–38 onboarding, 39–40 OB-P0, **41–42 G14-R4** G14 chunk table) | `palw_probabilistic_constraints_v1` (refused) |
| RFC-0010 V3 production fold (C2) | 120–129 (120 = `PanelBeaconProofV3`) | 170–179 (170–173 used) | 0xED | `palw_permissionless_panel_v1` (refused); `PalwVoidReasonV2` 120–129 (120–122 used; 0–10 implicit unchanged) |
| EXEC payload v2 (X8/X8R) | 130–139 (130 used: `ExecWorkRootOpenedV2`) | 180–189 (180 `ExecV2Row`, tables 1–4; 181 `ExecV2Verdict`, the refusal-record note) | 0xEE | `palw_exec_payload_v2` (declared; refused: `PALW_EXEC_PAYLOAD_V2_ARMABLE = false`, armable only on a salted t12 drill); `PalwVoidReasonV2` 130–139 (130 `WorkRootExpired`, 131 `WorkSliceProvenFalse`, 132 `WorkSliceDefaulted`) |
| Onboarding objects (D phase 3) | 104–108 used (104 ArtifactBound, 105 refutation, 106 KernelBound, 107 ConformanceCommitted, 108 SignedRegistrationV1), 109 **used by OB-P0** `ConformanceEvidenceV1` (2026-10-08 18:10; aux tables 39 attempts / 40 evidence material used with it) | none (rows in the kernel route's aux tables 36–38, journalled by deltas 160/161) | none (tail 0xEC) | `palw_signed_registration_v1` (108; refused) |
| RPC ops | C1 202–209 (202 used; 203 getPalwForkChoiceOpening (L2FC), 204–209 free), D 210–219 (210, 211, 212 used) and 230–239 (230 used; 231 used by OB-P0 `getPalwConformanceEvidence`, 232 unused), C2 220–229 (220 used), X8R 240–249 (RFC-0008 v2; 240 used: `getPalwExecV2Status`, gRPC 1294/1295) | — | — | — |
| Other fences (dormant) | `palw_provider_court_v1` (DA16), `palw_fork_choice_commitment_v1` (L2FC), `palw_tir_shard_segment_v2` (SHARD), `palw_typed_roots_v1` (R4X), `palw_task_heads_v1` (HFX) | — | — | — |
| Kernel route inner kinds / ledger tables (inside tag 110) | inner discriminants: 12 SealClaim, 13/14 OPV mode, **15 G14-R4 accuser seal**, **16 CommitSegmentedClaim, 17 PostTiledJob, 18 PostPromptTile (K2S)**, **19 Spec (R4X, RFC-0004 Part II)**, **20 CommitClaimSalted (G14R, `claim_seal_v2`, OPV beacon v3 GAP-B1a; fence `palw_panel_free_v1`; inner `SaltedCommitV1` 5 Claim / 6 Pipeline / 16 Segmented (K2S, at merge) / 19 Spec (also needs `palw_typed_roots_v1`); table 26 prune horizon pending the v3 policy constants)** — 21 and up free; `ProsecutionV1` 3 Segmented (K2S), 4 Spec (R4X); `ClaimBodyV1` 2 Segmented (K2S), 3 Spec (R4X); kernel ledger tables 1–14 used, **15–19 G14-R4** (18 `job_posters`, F-C4R4-08), **20–21 K2S**, **22–24 R4X**, **25 `claim_beacon_salts`, 26 `forfeited_claim_seals` (G14R, seal v2; one root extension)** (hashed only when non-empty); kernel route aux tables 36–38 onboarding, 39–40 OB-P0, 41–42 G14-R4, **43–45 DA16**; LedgerEventV1 21–24 G14-R4 (24 ServedDemandBondsBurned), 30 ProviderLiableDefault + 31 ProviderLapsed (DA16); SettlementKindV1 14–21 G14-R4 (21 ForfeitDemandBond); fence `palw_typed_roots_v1` (R4X, dormant, refused) | — | — | — |
| Bond budget + model allocation (BUDGET, ADR-0176/0177, 2026-10-10) | **140–149** (verify unused before first use) | **190–199** | **0xEF** | `palw_bond_budget_v1`, `palw_model_bond_allocation_v1` (dormant, refused when armed); RPC **250–259**; V2 root block `bond_budget/v1` |
| Reporter share migration (INTF, ADR-0032 2026-10-10) | — | — | — | `palw_reporter_share_v2` (dormant; Some-only hashed) |
| Immutable registrations (pre, ADR-0175) | — (refuses existing lifecycle tags past it) | — | — | `palw_model_immutable_v1` (dormant) |
| `PalwVoidReasonV2` | C2 120–122; X8R 130–132 (explicit; 130 was X8's implicit 11) | — | — | — |
| Legacy V2 Panel route G14, filer/reservation (LG14-A, RFC-0014 §6–§7, 2026-10-10) | **154–156** | **200–204** | **0xE2** | `palw_legacy_public_filer_v1` (dormant, refused); RPC **204–206** |
| Legacy V2 Panel route G14, held/fused DA + localization (LG14-B, RFC-0014 §4–§5, 2026-10-10) | **157–159** | **205–209** | **0xE3** | `palw_legacy_held_da_v2` (dormant, refused); RPC **207–209** |
| Kernel `LedgerEventV1` (2026-10-10) | 21–23 G14R, **24 G14R** (approved 10-10), 30–31 DA16 | — | — | — |

No lane edits `PalwConsensusObjectV2`, `PalwDeltaEntryV2`, the root preimage or carriage tails without a Lead commit that adds the
skeleton first; lanes build on that commit.

**A-2 uniformity rule (A2U, 2026-10-08/09; `a2-uniformity-new-kinds.md`).** Every allocation in this table that the live testnet-12
build (int-12) cannot read as a newer build does — an object tag, a kernel inner kind or nested variant, a header form, a coinbase
trailer, a form appended or re-read inside an int-12 kind, an FP job form, a header formula, a state encoding — has a row in the
central kind→fence table `PALW_A2_KIND_FENCE_TABLE_V1` naming its fence; A2U keeps it, and a lane's merge flips its row to landed (the
table test holds the code to the row). A new object kind also needs a row in `PALW_LIFECYCLE_NEW_KINDS_V1` and an arm in
`palw_lifecycle_kind_owner_v1`; a new header form or trailer a `PalwHeaderFormFenceV1` variant; a change inside a type int-12 decodes a
classification in `PALW_INT12_WIRE_CHANGES_V1`. **Below its owning fence a kind or form rides unjudged**: it is read exactly as int-12
reads its bytes — the same verdict (0x4b: tolerated and skipped; 0x4a and header forms: refused), the same skip, nothing charged,
counted or written. Never refuse it at isolation on 0x4b. Its own rule (may-ride, shape, size) is asked only past the fence, in the
header context, the acceptance walk and the fold. Merge gate: the A2U tests, the mixed-verdict pin test on both rulesets, and the replay
of its chains through int-12 itself (`scripts/a2u-int12-replay.sh`). The live build's lists are frozen.

**A-2 additions (A2U, 2026-10-10).** (a) The object-tag column above is mirrored in `PALW_A2_TAG_ALLOCATIONS_V1` with each allocation's
fences; a tag row outside its allocation or naming another lane's fence fails (`every_tag_row_is_inside_its_allocation`) — a new
allocation is a Lead commit to both. (b) Every landed post-int-12 kind's wire form is pinned (`PALW_A2_NEW_KIND_WIRE_V1`, in the A2U
test module) and so are int-12's 100 variants (`PALW_A2_INT12_VARIANTS_WIRE_V1`): a lane that creates or changes a kind re-pins it in
the same commit, keeping the change under its row's fence. **BUDGET (140–149):** in the commit that creates each kind — the variant,
its owner arm (a `PalwLifecycleKindFenceV1` variant per fence), its `PALW_LIFECYCLE_NEW_KINDS_V1` entry, an `ObjectTags` row naming
`palw_bond_budget_v1` or `palw_model_bond_allocation_v1`, its pin, and its `StateEncoding` rows flipped to landed. **DA16 (150–153
re-scope):** the commit removing the `Artifact` lease subject re-pins 150–153 and keeps `palw_provider_court_v1` (or adds a row for
another fence). (c) Kinds int-12 decodes that a fence judges anew (ADR-0175 `palw_model_immutable_v1`: 27, 28, 29, 81, 37 with
`EARLY_VERSION` refused by name; 3, 26, 39, 61, 68, 70, 91 folded by state) have `Int12RefusedByName` / `Int12FoldPastFence` rows: past
the fence the object is not applied and its block stands — never an isolation or header-context refusal. (d) `palw_reporter_share_v2`
(INTF) has a pending `StateEncoding` row: on `b8ae9412b` the 49% share is unfenced while testnet-12 arms R-core+ at genesis.

## 3. Waves

0. Scope/contracts — this matrix; `fcde1e3dc` (Panel-assignment subject + circularity rule).
1. Remote entry — C1 detached signing / node-less registration / remote claim; C3 gateway → canonical job → signed claim; C4 malicious relay fixtures.
2. Evidence independence — C1 adapter on A's public material read + objective default.
3. Permissionless claim completion — C2 production fold; beacon stays dormant (BEACON_UNAVAILABLE) until a Panel-independent Final exists.
4. Redemption — C1 V4 discovery/relay, owner payout, fees, V3 compatibility.
5. Inference and sharding — C3 idempotency/cancellation/preprocessing; C2 non-seat cell watcher, per-segment pricing.
6. Real-node adversarial — C4; Lead runs the single final full regression.

## 4. Change log

* 2026-10-08 night — **X8 (RFC-0008 v2) final, HELD out of the integration line until after the DAA-9,000 cut.** Branch
  `rfc8/x8-exec-v2` (7 commits on `c931df046`; consensus lib 614/0/22, `t12_exec_v2_carriage` 10 green ×6). Fence `palw_exec_payload_v2`
  None everywhere and unarmable (`PALW_EXEC_PAYLOAD_V2_ARMABLE = false`). Status: weightless carriage, gates, slice admission/expiry,
  EXEC_TX permits, restart/replay/IBD/reorg — IMPLEMENTED_AND_TESTED behind the fence; RPC/producer DORMANT_NOT_INTEGRATED; verification
  route, prosecution/DA, capacity/liveness drills EXTERNAL_GATE_PENDING; suffix void, relay backpressure CODE_GAP; flood residual,
  schedule credit, post-Final liability, permit equivocation, anchoring-window strand DESIGN_GAP. **Why held:** ~8k lines including
  un-gated pipeline paths (header pre/post-PoW validation, `deps_manager`, sync, orphan pool, coinbase) — an opus review of every path
  the fence does not guard comes before it enters a release candidate; RFC-0008 is outside 9,000 by the user's §12 anyway.
* 2026-10-08 night — **X12 (RFC-0012) integrated** (`9050b06bc` + test fixes `cd1abccb1`, `1c9532536`): zero-DNS matrix x0–x15 (evm
  feature), evidence from deltas; fence `palw_dns_retirement_v1` dormant. D1 (first `safe` ≥ 5,400 DAA) analysed on
  `rfc12/x12-safe-maturity` — user: no change now, not final; attack tests and RPC readiness reasons pending disk.
* 2026-10-08 night — **C4 round 3 integrated** (`b2d43068f`): mandatory test 3 passes on the real node (Panel-licensed and OPV, node-less
  relay path); F-C4R3-01(a) fixed; 01(b) → OB-P0; 02/03/05 + GAP-R7 + GAP-5 + GAP-11 → G14-R4; 04 → int-13 list frozen at the cut.
  **C1r2 P1 integrated** (`7ddf22251`): `rfc9_v4_chain_e2e` (fence 4 end to end, miner offline, another builder redeems) and the drill's V4
  leg (`audit-combined/rfc9-v4-leg.sh`, ≈19.5 h).

* 2026-10-08 evening — integrated: lane D phases 2/2b/3 (kernel route + RFC-0015 OPV on the node, onboarding objects 104–108,
  `palw_panel_free_v1` as a struct fence with the OPV admission list/terms, `palw_signed_registration_v1` for G-EXPIRY/G-RULESET,
  RPC 210/211/212/230), C4 round-2 fixes, X15, lane A, R9's DAA-9,000 flag day (params `2e567642…`, schedule `5f5df817…`).

* 2026-10-08 13:00 — **Correction (fence inventory of params.rs):** on testnet-12 the int-11 list ARMS at DAA 5,300 `palw_fp_decode_rules`
  (FP Job V4 / RFC-0001 §A), `palw_gen_v1` + `palw_fp_job_v5` (RFC-0003), `palw_improvement_v1` (RFC-0004), `palw_tir_shard_v1`
  (RFC-0006) and the RFC-0007 vertex/witness/mesh/capped fences. Rows above that call FP V4 / RFC-0003 "DORMANT (fence unarmed)"
  described the mainnet/testnet presets, not testnet-12. Still unarmed on t12: `palw_receipt_spend_v4`, `palw_evidence_court_v1`,
  `palw_audit_1004_v1`, `palw_gen_range_twin_v1`, `palw_dns_retirement_v1`; refused: `palw_probabilistic_constraints_v1`,
  `palw_permissionless_panel_v1`; undeclared: `palw_exec_payload_v2` (X8), `palw_panel_free_v1` (X15). **User goal (10-08):** implement
  RFC-0001…0015 and fence at DAA 9,000 — RFC-0004/0012/0015 are no longer deferred.

* 2026-10-08 — **C3 final integrated** (10 commits, `rfc-0001-0003-delivery-record.md`; gateway 158 + rail 7 + drill 8 + DSL 3 +
  fp-submit 12 tests; no real node / real weights):
  * Binding audit generated from `binding::AUDIT`, one mutation test per field, destructuring without `..` (a new field fails to
    compile until audited): **no unbound claim field**; frozen FP V4 untouched. Fixed in the gateway: F1 tokenizer/class/context
    held to the worker's manifest; F2 user stop strings must be spelled. By design: original request + chat template id are not in
    the claim (ids are; receipt carries request digest + template id) — consensus binding = DESIGN_GAP (versioned FP amendment).
  * Serving: Idempotency-Key + RFC 8785 digest (retry = same claim, no second inference/charge; conflicting request → 409); cancel
    on disconnect (queued never runs; mid-run drains and is discarded — worker `Cancel` frame CODE_GAP, proposal P1); CAS bounded
    queue; Retry-After bug fixed; status route with `final`/`voided` only from chain via ClaimTracker labelled
    UNVERIFIED_REMOTE_STATE; SSE chunks `streaming`, `final:false` — IMPLEMENTED_AND_TESTED.
  * Evidence: `--evidence-provider` (dir/HTTP) + `--evidence-min-copies`, placed and read back before commit; fetched after all
    producer state is deleted — IMPLEMENTED_AND_TESTED (local providers).
  * Receipt `GET /v1/receipts/<id>` authenticated by the chain (re-derives identities, recomputes `output_root`) —
    IMPLEMENTED_AND_TESTED (in-process chain).
  * VLM: integer stretch/letterbox preprocessing with golden vectors from an independent implementation, V5 job over canonical pixels,
    seat judges Valid — DORMANT_NOT_INTEGRATED (`palw_fp_job_v5` unarmed; fold does not open version-8 claims). Image generation:
    request → `PalwGenJobV1` → tiny SD3 run → payload, seat replay — library only; `/v1/images/generations` answers 501. Audio/video
    DESIGN_GAP. FP V4 G8–G10 EXTERNAL_GATE_PENDING.
  * Real path: gateway request on t12 shipped params → admission → worker → fp-submit → extraction walk → fold → claim under the
    gateway's bond → seat replay → receipt matches the node's row (in-process; also with decode rules test-armed) —
    IMPLEMENTED_AND_TESTED in-process; wRPC `observe_claim` against a node not run.
  * **F8 (privacy, rail):** `misaka-palw-fp-rail --evidence-out` has no `PanelDa` guard and writes private prompt ids to the evidence
    dir (a publicly served dir would disclose them); the gateway refuses `PanelDa` with public providers — rail guard CODE_GAP.
  * Proposals: P1 worker Cancel frame; P2 hoist F1/F2 into `validate_against_request`; P3 DA trio/output_root for version-8 claims
    before a V5 fold; P4 `Text` arm in the generative worker frame.

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
* 2026-10-08 — C2 (`rfc10/c2-panel`): RFC-0010 production fold, one ledger, handoff, beacon adapter, void mapping, anchor versioning, observation (wRPC op 220 / gRPC 1254-1255 / `misaka palw panel-v3`); RFC-0006 fold-level reorg test and stale-doc fixes. The fence stays refused at every real height; the beacon is BEACON_UNAVAILABLE. See `rfc-0010-production-path-record.md`.
