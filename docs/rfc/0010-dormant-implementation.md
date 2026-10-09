# RFC-0010 dormant implementation (reference format v1)

> **PALW共通前提 — 2026-10-10:** [ADR-0176](../adr/0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](../adr/0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


Implemented on `pre`, 2026-10-08. **No network activation, no production beacon, no claim that end-to-end permissionlessness is complete.** All presets leave `palw_permissionless_panel_v1 = None`. `validate_palw_v2` refuses every real height, including custom configurations. `Some(never())` is absence for fingerprints, schedules and the identity visitor. The complete policy is committed Some-only beside its activation in params/schedule identities; a future independently scheduled height changes the fork ID.

## Implemented boundaries

`misaka-palw-panel` is a deterministic, branch-scoped reference transition engine, with no dependency on a producer, local artifact store, RPC, DNS validator set or operator allowlist. `consensus/core/src/palw_permissionless_panel_v1.rs` connects accepted claim records, public structural seat eligibility and the existing one-ledger headroom calculation to it. The public chain adapter **always rejects beacon certificates** until an audited primitive is implemented. Test certificates exist only in integration-test fixtures.

The engine provides:

- `PendingSeal → Sealed → EntropyReady → Bound`, bounded retries and named non-fraud termination;
- immutable claim seals excluding signatures and lookup claim IDs, and a retained once-per-history work-identity set;
- immutable pre-entropy candidate snapshots with maturity, collateral, class/floor roles, capability/readiness commitments and executor bond/operator/key exclusions;
- automatic assignment in `(beacon epoch, acceptance order, occurrence index)` order, before this block's own admissions, with a consensus processing cap and atomic live reservations;
- binding-carrier-independent seeds, including an empty heartbeat carrier, with only an inclusion witness recorded in `PanelBoundV3`;
- deterministic, non-reused alternate operators derived from the original seed, role and retry index;
- `SealUnavailable`, `BeaconUnavailable`, `NoCapablePanel`, `PanelUnavailable`, with no slash/strike/reward/refund instruction;
- versioned Borsh carriage, a rooted state, checked import, transactional fold and replay from retained parents; typed JSON observations include seal/snapshot/beacon/assignment/binding/retry/reason facts. Reservation amounts are decimal strings.

Historical V2 state, `PanelBound`, receipt/court signatures, roots and codecs are untouched. `panel_claim_rule_v1` chooses a rule by **accepted DAA**, not a later binder/retry height. `palw_anchor_at_ceiling`'s lane-A prerequisite remains necessary for the historical drain; it is not a qualification rule in the reference automatic binder. BASE-0 remains bonded useful work and heartbeat remains the separate emergency clock lane.

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

1. Specify and implement the independent, unique, bias-bounded beacon, including contributor admission, certified tickets if used, proof verification cost, withholding/private-fork/claim-search/Sybil analysis and the P0-10 fix. Raw future block hashes, signatures and a test certificate are explicitly insufficient.
2. Specify the objective L1 sealing/finality rule and numeric network economics. Seal depth in the reference engine is not DNS finality or a Panel verdict.
3. Wire the V3 sidecar/root/delta into the production virtual state fold, stores, pruning/IBD, versioned claim admission and automatic receipt/court/payout handoff. Replace historical anchor gates only for new admitted claims; preserve old lane A until its full configured timeout/court/DA liability drain completes. No old claim migration is performed here.
4. Add versioned node/worker/RPC/CLI views and independent evidence/public redemption paths. The typed reference JSON is not an enabled network RPC. New encoded verification profiles require their own audited capability/readiness rules; the adapter currently preserves conservative existing eligibility.
5. Complete independent-node real chain replay, fresh non-genesis cohort registration/onboarding, an operator-free end-to-end Final/redemption drill, RFC14 non-Panel prosecution, published concentration/capture measurements and release rollback/fingerprint review. Binding success alone is not these gates.

These are explicit release blockers, not switches that can be enabled by setting a height. RFC10's security argument and end-to-end claim completion are not advertised as finished.

## Validation

`cargo test --locked -p misaka-palw-panel` covers carrier/timeout invariance, signature-equivalent lookup IDs, immutable population, acceptance-order contention, backlog, missing/invalid/late/conflicting beacon, bounded retries, retained duplicate identities, executor exclusions/maturity, outsider roles, exact same-key split weight, terminal release, carriage/reorg/replay and resource limits. `cargo test --locked -p kaspa-consensus-core --test rfc0010_permissionless_panel` covers dormant/refused activation, identities/schedules/fork boundary, historical admission and complete explicit binding facts. These fixture replays **do not meet the independent real-node replay gate**.
