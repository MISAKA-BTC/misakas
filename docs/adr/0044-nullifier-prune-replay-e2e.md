# ADR-0044 — Nullifier prune-then-replay を閉じる: full-lifecycle long-chain harness(keystone)

- **Status:** Accepted(方針とテスト設計の確定)
- **Date:** 2026-07-26
- **Supersedes / amends:** `docs/palw-nullifier-lifecycle-audit.md` の「missing」項(:58)への回答
- **Consumes:** ADR-0042(1c fixture が本 harness を共有)

## Context

nullifier 台帳(2026-07-25、node 1075900)は不変量
「**単一 selected chain 上で nullifier は高々 1 回 credit される**」を 5 本の e2e で固定済みだが、
**pruning point を実際に動かして**から窓内 nullifier を replay する統合テストが欠けている(:58)。
保護の建付けは二層:

- **ticket-nullifier(§15.2 窓):** per-block `PalwActiveNullifierSet`、`prune_below` +
  `palw_nullifier_retention_daa`(1,200)。pruning 境界は `PalwPrunedFrontierV1.active_nullifiers` が運ぶ。
- **job-nullifier(G16 paid-work):** 受理時の bounded walk
  (`paid_work_walk_bound_daa` ≪ pruning depth、`palw_paid_work_walk_stays_above_the_pruning_point` が pin)。

未証明なのは「**frontier 持ち越しが実 pruning pass を跨いで正しい**」ことの統合レベルの実証のみ。
2026-07-26 の pruning 実装拡張(per-block DA/search store の削除 + boundary-anchor スキップ、ffc9fb8)で
pruning pass 自体の対象が増えており、この統合実証の価値は上がっている。

## Decision

**full-lifecycle long-chain harness を consensus 統合テストとして新設し、それを keystone として
ADR-0042 の 1c fixture と本 ADR の prune-then-replay E2E の両方を載せる。**

1. **harness 仕様:** TestConsensus + 縮小 params(`finality_depth`/`pruning_depth` をテスト用に
   数百 DAA へ縮小 — 2026-07-26 の devnet 縮小(7,200/21,600)と同じ手法のさらに小さい版)で、
   **実 batch lifecycle**(manifest 受理 → leaf-chunk 受理 → cert 受理 → algo-4 mint → 支払い)を
   ブロック受理経路で構築し(hand-seed しない)、その後 plain block を pruning depth 超まで積んで
   **実 pruning pass を発火**させる。
2. **E2E アサーション(prune-then-replay):**
   - pass 後、pruned 領域の per-block nullifier 行が消えていること(store 監査)。
   - **replay(ticket):** pruning 前に使われた ticket nullifier を再利用する algo-4 ヘッダは、
     窓が frontier 経由で持ち越されている限り red 化(credit 拒否)されること。
     retention(1,200)より深い replay は設計上「窓外 = 新規扱い」であり、これは**仕様**として
     assert する(nullifier は epoch/batch 束縛で grind 済みのため窓外再利用は新しい ticket に等しい)。
   - **replay(job/G16):** bounded walk 内の重複支払いが依然 0 であること(既存 e2e の pruning 跨ぎ版)。
   - ADR-0042 1c: 同 harness の任意 chain block を pp として snapshot builder を通し、
     payload 導出 == store 導出を assert。
3. **配置:** `consensus/src/pipeline/virtual_processor/tests.rs` ではなく専用
   `consensus/src/pipeline/pruning_processor/palw_lifecycle_e2e.rs`(tests mod)に置く
   (pruning processor を実際に回すため)。

## Consequences

- mint-env(`palw_algo4_env`)の hand-seed は本 harness では使わない(coherence が pruning builder の
  要求に満たないことを 2026-07-26 に実証済み — manifest/lifecycle binding で builder が正しく拒否した)。
  受理経路で作る = builder の検証がそのまま fixture の検証になる。
- 実装は数時間規模の専用作業。in-session の即closeではなく、次の実装スロットの最優先項目とする
  (ADR-0042 の 1c がこれに依存するため)。
- job-nullifier の「bound 外 replay」は本 harness の範囲外の**パラメータ論証**
  (bound ≥ batch lifecycle 全長の保証)として ADR-0045 の経済/security model に引き渡す。

## Definition of done

- [ ] long-chain harness(縮小 params、full-lifecycle、実 pruning pass)
- [ ] prune-then-replay E2E(ticket 窓内 red 化 / 窓外仕様 / G16 不変)green
- [ ] ADR-0042 1c fixture green(同 harness)
- [ ] `docs/palw-nullifier-lifecycle-audit.md` の missing 項を Closed に更新
