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

- [x] long-chain harness(縮小 params、full-lifecycle、実 pruning pass)—
  `consensus/src/pipeline/pruning_processor/palw_lifecycle_e2e.rs`、
  `palw_full_lifecycle_prune_then_replay_e2e`。実 batch lifecycle を bond→manifest→beacon commit/reveal→
  DNS 確認→leaf-chunk→DA challenge/response(Satisfied)→attested cert→algo-4 mint→支払い を
  **全てブロック受理経路**で構築し、plain block を pruning depth 超まで積んで**実 pruning pass**を発火。
- [x] prune-then-replay E2E(ticket 窓内 red 化 / 窓外仕様 / G16 不変)green
  - **重要な構造的事実(実測):** pruning point は `sink − pruning_depth` を追い、初回前進は
    ~`0.9·pruning_depth` へ一気に飛ぶ。`walk_bound < pruning_depth`(preset pin)ゆえ **batch 窓全体が
    必ず初回 pp より下**に来る = pp 上位で mint することは原理的に不可能。よって全 mint は pruned。
  - (1) pruned 領域の per-block nullifier 行消滅 + pp 直上行の窓が pruned 領域を跨いで両 ticket を保持。
  - (2a) PRE-PASS baseline: reuse-merger が同高さ再利用を **red 化**(実 mint ticket 上での機構実証)。
    (2b) POST-PASS survivor: 全 mint が pruned のため red-catch を担うのは **frontier**(= pp の
    persisted window)。両 mint nullifier を retention 内で保持し、join ノードの再 import が再利用を
    red 化できる。recolor コードパスは pre-pruning nullifier e2e が既に pin 済み。
  - (3) clause-5 coupling: W の target interval が pp 以下に埋没し、SP の窓も消滅 = 窓内 replay は
    局所的に再構成不能(再入は frontier import のみ)。
  - (4) retention 超で nullifier は全 live 窓から退出(spec、新規 ticket 扱い)。
  - (5) G16: bounded walk の below-boundary paid-work 行が pruning snapshot に持ち越し。
  - **G13 withhold(ADR-0045):** quorum 不足 cert(1/3 票 < 2/3 stake)が acceptance の attestation
    gate で store に到達せず拒否(live-devnet CertAbsent 根因の再現)。
- [x] ADR-0042 1c fixture green(同 harness)— `palw_pruning_payload_paid_work_nullifiers ==
  palw_paid_work_window(pp)` かつ `palw_pruning_payload_da_state_root ==
  palw_da_parent_state(pp).state_root()` を lifecycle-coherent chain の実 pruning point 上で assert。
- [x] `docs/palw-nullifier-lifecycle-audit.md` の missing 項を Closed に更新

## harness が発見した production 欠陥(hand-seed 禁止でのみ露見)

受理経路構築を強制した結果、pruning snapshot writer の coherence 契約に対する **3 件の潜在バグ**を
検出・修正した(いずれも activated chain で pruning point を恒久停止させうる):

1. body-fold の overlay view が `Default`(version 0)で seed され、snapshot writer の
   "overlay-view version" 検査を落としていた → `PalwBatchViewV1::new()`(version 1)に修正。
2. paid-work builder が `once(pruning_point).chain(backward_iter)` で pruning point 行を二重計上し、
   dup-block 検査を落としていた → backward iterator は始点 inclusive なので `once` を除去。
3. beacon accumulator 行が `Default`(version 0)で生成され、"beacon accumulator rows" 検査を
   落としていた → `PalwBeaconEpochAccumV1::new()`(version 1)に修正。
