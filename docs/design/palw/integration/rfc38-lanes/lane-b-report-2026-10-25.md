# Lane B 報告 — 因果graph・全branch・期限calendar（2026-10-25）

開始 SHA `14fc6e997`、branch `claude/rfc38-lane-b`。code commit: `bab113757`、`e7fa48871`。push・deploy・activation 変更なし。公開 activation は None のまま。計算の暗号学的証明は不使用。モデル二階層は変更なし。中央ファイル（`palw_state_v2.rs` 本体、Params、mod 登録、Cargo.toml、wire/DB registry）は無変更のため `central:` commit はない。

## 1. 要件 → 現実装 → 実 caller の対応表

| 要件 | 現実装 | 実 caller | 本差分 |
|---|---|---|---|
| RFC23 §4.1/§4.5 未解決集合と material の checkpoint 拘束 | claim row と court session row だけを manifest へ | `write_claim_local_checkpoint_v1`（fold 末尾）、`assert_claim_local_checkpoint_v1`（load/pruned IBD） | close group を追加（閉じた） |
| RFC23 §4.2/§4.4 inputs / reservations の実依存 | `inputs` 空、reservations 空 | 同上 | 未着手。tx 出力→claim 消費 map が state に無い（CODE） |
| RFC25 §5/§7 calendar、§4,6 bounded extension | 既存 court budget/EDF/hard deadline | `palw_court_capacity_fold_v1` | 未着手（POLICY） |
| RFC26 §3 全branch原子予約 / RFC29・30 §5–7 funding | `reservation_readiness_v1` 常に Unresolved | native aggregate test | 入力境界を型で定義（閉じない） |
| RFC28 marker / RFC31 §3–4 | 既存 | — | 未着手 |
| RFC30 §1–4 未決 side の資料保存 | close group 資料は state のみ | 上記 fold | checkpoint で保存（部分） |

選定理由: 既存 native consumer（fold・carriage load）に直接接続でき、K3・native39 写像・料金/供給 policy に依存しない。

## 2. 実装

### B-1 RFC23 §4.1 / RFC30 §1–4: open court close group を liability material に拘束（`bab113757`）
- 変更前: `expected_claim_local_checkpoint_v1` は ADR-0080 design A の半組立 close（宣言 digest、verdict、assembly 期限、到着 chunk、deposit outpoint）を material に含めず、restart/pruned IBD 後の再開資料が checkpoint 外だった。
- 実装: open claim の各 session について `(session, Challenger)`→`(session, Executor)` の順で `borsh(&(sid, side, group))` を manifest/material に追加。`resources::requests` は group 数分の entry と encode buffer を構築前に予約（借用のみ、clone なし）。
- ファイル: `consensus/core/src/palw_claim_local_checkpoint_fold_v1.rs`、`palw_claim_local_checkpoint_resources_v1.rs`、試験 `palw_state_v2/tests/claim_local_rewards_v1.rs`。
- 不変条件: material は state の純関数・canonical 順。stored row は常に再計算と比較され、group 欠落 row は拒否。row は既存 `KernelRouteRow` delta なので reorg で厳密に戻る。独立 claim の record は不変。fence 未満の bytes は不変（休眠 fence）。

### B-2 RFC26 §3 / RFC30 §5–7: all-branch terms 入力境界（`e7fa48871`）
- 変更前: `reservation_readiness_v1` に C/F 契約の接続点が無かった。
- 実装: `trait AllBranchTermsSourceV1 { fn adopted_terms_v1(&self, original_claim) -> Result<Infallible, UnresolvedSupplyContractV1> }`、唯一の実装 `NoAdoptedAllBranchTermsV1`。`reservation_readiness_with_v1` は Prefix 非対応 → family-local 未供給（numeric→constraint→tokenizer）→ terms source の順で判定し、実 original claim を渡す。成功型は uninhabited。架空 supplier・無条件 Ready なし。
- ファイル: `consensus/core/src/palw_court_shared_original_aggregate_v1.rs`、試験 `palw_court_original_aggregate_native_tests.rs`。

## 3. 境界 API（A・C・D・F 向け）
- duty/branch ID: `PalwClaimLocalWorkRefV1`。
- 依存辺: `PalwClaimLocalRecordV1.inputs`（実消費出力のみ）、`execution_root`、`reservations`（reservation ID、bond ID ではない）。closure は `validate(..).dependencies(work)`、cycle・欠落は validate が拒否。
- terminal: `final_claim`/`certified`/`voided`/`liability_closed`、`credit_eligible`/`irreversible_eligible`、`irreversible_frontier()`。
- material: `palw_liability_material_id_v1(bytes)`。court close は `(session_id, side, group)` の borsh bytes。順序は BTreeMap 順、side は Challenger→Executor。
- deadline: 現状は session `deadline_daa`、group `assembly_deadline_daa`。一般 resource-vector calendar 型は未定義（work ref をキーにする前提で A・統括と合意して定義）。
- supply 入力: `AllBranchTermsSourceV1`（C/F の実契約型が出た時点で `Ok` 型を統括が中央で差し替え）。

## 4. 試験証拠
- `CARGO_INCREMENTAL=0 ~/Downloads/MISAKA-wt-b/buildslot.sh cargo test --offline -p kaspa-consensus-core --lib -- rfc23s2_checkpoint original_aggregate_draft`（既定 feature、上記 2 commit と同内容の source）
- exit 0、`14 passed; 0 failed; 0 ignored; 5202 filtered out`。新規 `rfc23s2_checkpoint_binds_open_court_close_groups_as_liability_material`（両 side が material に入る・exact bytes・独立 B 不変・決定論的・欠落 row 拒否）。aggregate 試験は probe source が実 original claim を受け取ることを検査。
- fixture 上の PASS であり実 node campaign ではない。15 gate は更新しない。

## 5. 残件
- CODE: (a) `inputs` の実依存 — admission で消費関係を rooted に記録する経路が必要（依存: E）。(b) `derived_artifacts`・pending chunks の material 化要否。(c) 一般 resource-vector calendar と依存 terms の原子更新（依存: A）。
- TEST: 実 court ladder で close group を宣言する fold 経路での checkpoint 試験、restart/pruned IBD を跨ぐ multi-node 試験。
- EXTERNAL: C（funding/supply）と F（material/provider）の採択契約型。
- POLICY: RFC25 bounded 混雑 extension の上限・id、K3、native39→RFC12 写像。

## 6. 閉じた要件 / 閉じない RFC
- 閉じた: RFC23 §4.1 の open court state のうち close group 部分。RFC26 readiness の入力境界定義（Ready 経路は意図的に無し）。
- 閉じない: RFC23・25・26・28・29・30・31 はいずれも全体未完了。
