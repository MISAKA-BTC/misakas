# RFC-0010 dormant implementation and permissionlessness audit

> **PALW共通前提 — 2026-10-10:** [ADR-0176](../adr/0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](../adr/0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


Implemented on `pre`, 2026-10-08. **No network activation, no production beacon, no claim that end-to-end permissionlessness is complete.** All presets leave `palw_permissionless_panel_v1 = None`. `validate_palw_v2` refuses every real height, including custom configurations. `Some(never())` is absence for fingerprints, schedules and the identity visitor. The complete policy is committed Some-only beside its activation in params/schedule identities; a future independently scheduled height changes the fork ID.

**Audit baseline — 2026-10-10:** integration `3730cc90f` (`claude/g14-public-prosecution-integration-9bee39`). The original reference-only description below is updated to match that tree. The production fold, receipt/court handoff, shard draw and observation RPC exist behind the dormant fence. They do not establish an armable release. ECON round 4 and BUDGET round 2 are present; their unapproved economics and open safety hooks do not authorize RFC10 activation.

## Implemented boundaries

`misaka-palw-panel` is a deterministic, branch-scoped transition engine, with no dependency on a producer, local artifact store, RPC, DNS validator set or operator allowlist. `consensus/core/src/palw_permissionless_panel_v1.rs` supplies accepted claims and public structural eligibility. `palw_panel_v3_fold_v1.rs` embeds the engine in the production V2 fold, writes ordinary V2 Panel duties on its one exposure ledger, and hands bindings to the existing receipt/court/Final machinery. It also draws per shard and defers non-fraud expiry while an accusation is pending.

The production beacon adapter supports the shared work-beacon contract, including sealed-source v3. The chain history reads kernel-route OPV Finals, attribution, seals and derived OPV eligibility. **The approved Panel policy registry is empty**, so shipped validation refuses activation and shipped adapters reject every beacon proof. The V2 lattice's own Finals remain Panel-licensed and cannot seed their own Panel. Test-supplied histories and policies exercise the adapter; they provide no production approval.

RPC `getPalwPanelV3Status` (op 220) and `misaka palw panel-v3` expose the engine and named claim statuses. The appended `release.activationSupported` and `release.approvedBeaconSchemes` report this binary's release support separately from `overview.active`. A dormant chain can still report a named legacy claim or unknown id. This JSON is read-only and changes no wire object, state root or consensus fingerprint.

The engine provides:

- `PendingSeal → Sealed → EntropyReady → Bound`, bounded retries and named non-fraud termination;
- immutable claim seals excluding signatures and lookup claim IDs, and a retained once-per-history work-identity set;
- immutable pre-entropy candidate snapshots with maturity, collateral, class/floor roles, capability/readiness commitments and executor bond/operator/key exclusions;
- automatic assignment in `(beacon epoch, acceptance order, occurrence index)` order, before this block's own admissions, with a consensus processing cap and atomic live reservations;
- binding-carrier-independent seeds, including an empty heartbeat carrier, with only an inclusion witness recorded in `PanelBoundV3`;
- deterministic, non-reused alternate operators derived from the original seed, role and retry index;
- `SealUnavailable`, `BeaconUnavailable`, `NoCapablePanel`, `PanelUnavailable`, with no slash/strike/reward/refund instruction;
- versioned Borsh carriage, a rooted state, checked import, transactional fold and replay from retained parents; typed JSON observations include seal/snapshot/beacon/assignment/binding/retry/reason facts. Reservation amounts are decimal strings.

Below the fence the historical V2 root and carriage bytes remain unchanged; the dormant V3 sub-state uses root block `panel_v3/v1`, deltas 170–174 and carriage tail `0xED`. `PanelBound` and receipt/court signature domains are retained. `panel_claim_rule_v1` chooses a rule by **accepted DAA**, not a later binder/retry height. Historical anchor gates still govern old claims. A V3-rule claim is excluded from their candidate lists and binds automatically through the V3 fold. BASE-0 remains bonded useful work and heartbeat remains the separate emergency clock lane.

## Exact reference encoding

This is an **experimental reference format**, not an assigned consensus wire version. Any acceptance format must undergo the RFC's release review.

`WIRE_VERSION_V1 = 1`. Hashes are 64 fixed bytes. Unsigned integers are little endian (`u16/32/64/128`); vector/map lengths and variant tags use Borsh. Maps/sets serialize in ascending key order. Every new digest is unkeyed BLAKE2b-512 over `u32_le(domain byte length) || UTF8(domain) || Borsh(payload)`. These domains do not reuse v1/v2 Panel hashes, generation challenges or algebraic-check domains.

| Domain | Payload |
| --- | --- |
| `misaka-palw/panel-v3/policy` | All fields of `PanelPolicyV1`, in declaration order |
| `misaka-palw/panel-v3/claim-fields` | Source tag/quanta, class, producer bond, PWU, trace/output/execution roots, chunk count/retention, exposure, immature contribution, escrow, work leaves, rights exposure, job identity, owner payout and model root; no phase or spent-quantum ledger |
| `misaka-palw/panel-v3/key` | Borsh public-key bytes |
| `misaka-palw/panel-v3/capability` | Public capable-class set |
| `misaka-palw/panel-v3/readiness` | Authenticated checkpoint state root and judged class |
| `misaka-palw/panel-v3/seal` | Network, ruleset, canonical work ID, `(class, bond, operator, key, immutable fields, seat exposure)`, accepting block/DAA/height/order/occurrence, checkpoint block/height/DAA, anchor slot, beacon epoch, snapshot root |
| `misaka-palw/panel-v3/snapshot` | Complete `PanelSnapshotV1` with its own root zeroed: checkpoint, policy, class, exclusions, canonical bond-sorted candidate rows |
| `misaka-palw/panel-v3/beacon` | Network, ruleset, beacon scheme, epoch, certified output; proof/signature variants are not entropy |
| `misaka-palw/panel-v3/seed` | Network, ruleset, seal ID, anchor slot, snapshot root, epoch, certified output |
| `misaka-palw/panel-v3/seat-ticket` | Original seed, retry index, role, operator identity and optional bond outpoint |
| `misaka-palw/panel-v3/state` | Complete `PermissionlessPanelStateV1` including version, identities, policy, branch tip/clock, acceptance counter, claims, retained work IDs, live epochs and reservations |

`PanelPolicyV1` names every timing/size parameter and the beacon scheme identity. None is a production default. The limits in `types.rs` bound candidates, pending/history records, retry rounds, beacon proof bytes and block inputs. Engine state never deletes spent work identities; the configured history cap stops new admission rather than allowing old work to re-enter. A production runbook needs authenticated compaction/accumulation, not silent deletion.

## Ordering and clock

Claims enter only after existing execution, fee, exposure and ownership checks. The fold assigns acceptance order; miners supply neither queue priority nor an anchor slot. Sealing uses the parent checkpoint after `seal_depth_blocks` selected-chain edges. Parent public eligibility is frozen before the carrier's own objects. If the carrier has already crossed the assigned future entropy epoch, the claim terminates `SealUnavailable`; it never searches another epoch. This checkpoint depth is a reference replay condition, **not a demonstrated irreversible L1 finality primitive**.

The anchor slot is the next strictly future multiple of `beacon_period_daa`. Certificates may be accepted only from that release through `release + beacon_wait_daa`, inclusively. Output verification must bind network/ruleset/scheme/epoch/interval, canonical chain inclusion, uniqueness and source-specific fork rules. The bounded certificate interface is not a proof of beacon security.

Seed readiness is the end of the complete contribution window, independent of certificate arrival. After that boundary, missing output terminates `BeaconUnavailable`; available output sets the fixed assignment point to readiness plus `assignment_delay_daa`. Every first eligible selected-chain transition at/past that point processes the first due claims under the cap. Reservations use one pre-object live headroom view plus this engine's accumulated reservations, never a producer-selected subset or candidate list.

The integer exponential race uses frozen exact sompi weights, with exact 192-bit cross-products and no floating point. Same-key bonds share one operator race entry with aggregate collateral; the selected operator contributes at most one seat. Equal total same-key collateral retains the same first-choice race weight under splitting. **Different keys are not independent owners**: capture probability, collateral fragmentation, repeat draws, outsider and court capture still require the RFC's economic analysis. This engine does not assert a full split-invariance or withholding proof.

Receipt deadlines begin at binding. Timeouts release the previous round's reservations and draw bounded, non-reused alternate operators from the same snapshot and original seed. A validated terminal receipt/court outcome releases reservations; engine events themselves cannot authorize `Final`, mint money, license execution or slash anybody. A continuing chain and the pending/queue/retry bounds are needed for progress; no wall-clock guarantee exists while the chain is stopped.

## Remaining release work (activation prohibited)

1. Approve a concrete production beacon policy only after its bias, withholding, participation, private-fork, claim-search/Sybil and P0-10 analysis. The sealed-source implementation exists, but the approval registry is empty and ECON proves no positive conviction floor. Raw future block hashes, signatures and a test certificate are insufficient.
2. Specify the objective L1 sealing/finality rule and numeric network economics. Seal depth in the reference engine is not DNS finality or a Panel verdict.
3. ~~Freeze the Panel beacon's source-profile eligibility at a committed pre-entropy point.~~ **Implemented and tested, dormant (R10F, 2026-10-10):** the engine freezes each epoch's source set once, in the first block whose DAA reaches the epoch's `release_daa`, from what the host derives at that block's parent (`ConsensusViewV1::epoch_sources`; the fold's `palw_panel_v3_epoch_sources_v1`), stores it in its state (`EpochSourceSetV1`, root, delta 174, tail `0xED`, bounded to 1,024 profiles and to live epochs), and `ChainPanelBeaconHistoryV1::eligible_profiles` reads only the frozen set — it never re-derives. Replay, reorg and carriage-import tests are in `misaka-palw-panel/tests/stages.rs` and `consensus/core/tests/rfc0010_production_fold.rs` (`docs/design/palw/rfc-0010-production-path-record.md` §2). Still open: the `Chain` derivation from a real OPV-eligible kernel route and a processor-level reorg/IBD across a freeze block are not exercised; authenticated real pruning-proof recovery remains a release drill. Not armable: the fence stays refused.
4. Complete the RFC9 independent evidence, remote verification and public redemption paths, with measured inclusion and prosecution bounds. New encoded verification profiles need audited capability/readiness rules. RPC/CLI presence alone proves observation, not completion.
5. Run a fresh non-genesis cohort through production admission, independent source generation, automatic binding, public prosecution, Final and miner-owned redemption with the original operator fleet removed. Existing processor tests use a reference source and bypass activation validation. Publish concentration/capture and full splitting/fallback analysis, plus the release fingerprint, old-claim liability drain and rollback runbook.

These are explicit release blockers, not switches that can be enabled by setting a height. RFC10's security argument and end-to-end claim completion are not advertised as finished.

## Audit fixes — 2026-10-10

1. **Uncommitted mirror:** validation checked the top-level fence but the processor executes the V2 state's mirror. A caller could supply an active mirror with no fence and pass that specific guard. Validation now requires exact agreement on absence, height, policy, network and ruleset; a synchronized real fence is still rejected for the pending release gates. Fixtures deliberately bypass validation as before.
2. **Candidate-cap priority:** the shard fold truncated an oversized population to its largest individual bonds (ties by outpoint). That gave an unsplit large bond entry preference over smaller public bonds and differed from the flat adapter's capacity handling. Both adapters now refuse an oversized snapshot; the production fold follows its deterministic `NoCapablePanel` non-fraud path with no selected subset, slash or new reservation. Capacity remains a bounded liveness limit: an oversized cohort cannot bind until a reviewed larger-capacity or population-selection rule exists.
3. **Observation:** the CLI returned immediately when the V3 engine was absent, hiding explicitly requested lane-A claim facts and unknown ids. It now renders those facts and the binary's unsupported activation status. A fixture's active engine cannot imply that this release supports activation.

These fixes close defects in the dormant implementation. They do not remove the live eight-genesis anchor rule or claim the unresolved release gates have passed.

## Validation

**Run on this audit branch, 2026-10-10: 84/84 selected tests passed** — Panel engine 32, the five RFC10 core suites 45, virtual-processor E2E 4, CLI 3. The changed Rust files pass `rustfmt --check` and the diff passes `git diff --check`. Existing dependency warnings remain; no full-integration or VPS release gate is claimed by this targeted run.

`cargo test --locked -p misaka-palw-panel` covers carrier/timeout invariance, signature-equivalent lookup IDs, immutable population, acceptance-order contention, backlog, missing/invalid/late/conflicting beacon, bounded retries, retained duplicate identities, executor exclusions/maturity, outsider roles, exact same-key split weight, terminal release, carriage/reorg/replay and resource limits.

The core suites `rfc0010_permissionless_panel`, `rfc0010_beacon_adapter`, `rfc0010_production_fold`, `rfc0010_shard_v3` and `rfc0010_g14_guard` cover guarded activation/fingerprints, the beacon verifier, production fold and handoff, public registration, shard prosecution, accusation holds, delta reversal and carriage reload. The new regression cases reject orphan/stale mirrors, show the empty approval registry in observation, and reject candidate-cap priority through the full shard fold.

`cargo test --locked -p kaspa-consensus --lib t12_permissionless_panel_e2e -- --test-threads=1` runs real virtual-processor tests for binding, second-node block replay, reorg/carrier invariance and pre-fence object handling. `cargo test --locked -p misaka-cli palw_panel_v3::tests` checks CLI observation. The node tests' reference entropy and post-build activation bypass **do not meet the operator-free production-beacon/Final/redemption gate**.

The initial CLI run overflowed the existing parsing test's default 2 MiB thread stack. Its assertions passed with `RUST_MIN_STACK=16777216`; the parsing test now uses an explicit 16 MiB thread, matching the CLI's existing test convention, so the ordinary command needs no environment override.
