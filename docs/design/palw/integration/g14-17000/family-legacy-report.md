# Family lane LEGACY 報告(`LegacyPanelAttempt` / `LegacyFusedLeaf`)— 2026-10-26

Branch `claude/g14-fam-legacy2`(base `d8aded6de`)。push・deploy・activation 変更なし。record は空のまま。

## 結論

この 2 family は **武装できる状態ではない**。武装を止めている主因は family 固有の実装ではなく、slice lane の共有部品:

- `validate_palw_court_policy_completeness_v1` が `LegacyPublicFilerV1` / `LegacyHeldDaV2` を実高さで武装すると、record が通った後に
  **F02**(`PALW_COURT_SERVICE_PROFILE_V1` が `None`)と **F03**(`palw_court_ingress_schedule_v1()` が `Err(Schedule)`)で拒否する。
  両方とも slice lane(F02 / F03b)の値・採用待ち。LEGACY lane は独自に埋めない(並行 runtime を作らない指示どおり)。
- これが lane ACT の報告した `g14_close_legacy` / `g14_court_capacity` 失敗の原因(base `d8aded6de` で実測: close_legacy 3 failed、court_capacity は先の target 失敗で未実行;修正後は close_legacy 8 passed / 1 failed、court_capacity 14 passed / 1 failed / 1 ignored、残る 1 件はどちらも下記の pin)。
  試験は「武装がテストで全体 validation を通る」前提のまま残っていた。

## 試験の修正(試験のみ、consensus コード変更なし)

`consensus/core/tests/g14_close_legacy.rs`、`consensus/core/tests/g14_court_capacity.rs`:

- helper `validates_with_records_but_f02_f03[_cost]`: 全体 validation が通る、または **唯一の拒否が F02/F03 の typed refusal** で、
  かつ武装中の各 gated fence の record が単独で admit する場合のみ `Ok`。それ以外の拒否は返す。F02/F03 拒否を握りつぶさず、名前で判定。
- 全体 validation が F02/F03 で先に止まるため見えなくなった否定試験(record 欠落、別高さ、frame cost 欠落・horizon 超過、
  court capacity の drift / digest 不一致 / 未測定 / placeholder / record 欠落)は、各 fence 自身の validator
  (`palw_g14_arming_record_admits_v1`、`validate_palw_tir_frame_cost_in_v1`、`validate_palw_court_capacity_in_v1`)で読む。
  全体 validation が `Err` であることも併せて assert。
- 残る失敗 `testnet12_identity_did_not_move`(両 file): **identity は不変**(`5de80e64…`)、params / schedule id が
  base の統合 commit(`c01d885e2` 等)で変わった pin の追従漏れ。LEGACY の変更ではない。fleet / release 組み立て lane で pin 更新を。

## 段ごとの状態表

| 段 | LegacyPanelAttempt | LegacyFusedLeaf(+ court capacity) |
|---|---|---|
| 受付(option (b) gate、`route_armed`) | 実装済み・V-unit(未武装なら `G14_ROUTE_ABSENT`) | 実装済み(`palw_g14_route_attempt_refusal_for_class_v1`) |
| duty / 予約(tag 154、34 session 予算) | V-fold、8k は並列 lane で fit、2M は名前で拒否 | 同左 + court slot 予約(outsider lane は F03 待ち) |
| 供給(held DA、tag 157–159、再取得 tag 156) | V-fold / 一部 V-node(svc) | CKW fused opening V-fold |
| 正直 Final・報酬 | Final hold は V-fold、報酬復帰は fence 武装待ち | 同左 |
| 嘘 → 外部者の有罪・slash | floor / LG14-B で V-node(svc) pre-Final;全 family 網羅なし | fused-tile 嘘 V-fold;court 混雑下は未 drill |
| 沈黙 → DA default | V-fold + fresh service 1 件 | 同左 |
| reorg / restart / pruned IBD | 部分(pruning 6 件)、full campaign 未 | 予約・期限の同一性 V7 未 |
| 武装要件 V5 測定・V6 実網・drill | **未**(hard-deadline envelope、Panel 容量、descent 予算) | **未**(court op 実測、ρ_court) |

## 武装の機構

- fence 一覧 `palw_g14_route_family_fences_v1`: LegacyPanelAttempt = filer + held DA + route required;LegacyFusedLeaf = + court capacity。
  既存のまま正しい。新 fence 追加なし(identity 影響なし)。
- record 要件 `PALW_LEGACY_FILER_/HELD_DA_/COURT_CAPACITY_ARMING_REQUIREMENTS_V1` 既存。record は空(pin は統括)。
- 統括決定(10-26): LG14-A record は汎用 public filer を、LegacyPanelAttempt/LegacyFusedLeaf の個別ケースは本 lane が担う。

## 値

新しい値は足していない。family 固有で要る値はすべて測定(V5・court op・ρ_court)で、目安で置けるものは無い:
court capacity の試験用測定は `test_measurement()`(試験専用)のみ。

## 次に必要なもの(順)

1. slice lane: F02 service profile と F03 paid ingress の採用 → legacy fence の completeness が解ける。
2. node E2E: `legacy_v2_attempt_full_collusion_outside_bond_convicts`、`legacy_v2_attempt_held_da_withholding_ends_in_da_default`。
3. V5 測定(class 最大 profile の envelope、court op の最遅ノード実測)→ `palw_g14_values_v1` へ目安でなく測定値。
4. restart / reorg / pruned IBD の full campaign、出荷 binary で fence を跨ぐ drill。
