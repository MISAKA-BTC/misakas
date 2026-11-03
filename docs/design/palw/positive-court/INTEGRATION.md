# Positive Court R2 — 文書と実装の統合先

## 番号と現行文書数

**A0適用注記（2026-10-31）:** 下段には元ZIPの整理計画を保持する。現在のローカル採択IDはユーザー指定の**RFC-0044 / ADR-0212**。既存RFC-0042 / ADR-0210は別内容のため保持した。添付の旧番号・本数を現checkoutの番号割当として使わない。[実装状態](IMPLEMENTATION-STATUS.md)と[native契約](NATIVE-CONTRACT-R1.md)が現時点の接続状況を示す。

添付の最新ZIPは旧番号のRFC41本・ADR204ファイルを含み、`docs/rfc-v2/`と`docs/adr-v2/`は含まない。本パッケージでは履歴用にRFC0042 / ADR0210を追加する。別branch予約は未確認。

整理後は**RFC-v2-0014へ本RFCの正常Court章を統合**し、新しい独立RFC-v2番号を作らない。ADR側はADR-v2-0038の後続判断として統合する。旧番号を履歴として残し、RFC15本/ADR50本の現行入口を維持する。既存のv2全文・原文hash・行対応表を、この提案だけで再生成済みとはしない。

| 主題 | RFC-v2 | ADR-v2 | 旧RFC |
|---|---|---|---|
| 正常/不正のCourt主契約 | 14（主担当） | 38、42〜47 | 14、25、26、28、30〜35 |
| TIR全関係被覆・開示形式 | 02 | 13、14、44 | 02、29 |
| Panel資料生成・新しい所見 | 06 | 31〜34 | 06、07、10 |
| Claim・仕事量・回答会計 | 07 | 26、27 | 08、38 |
| 依存・prefix起源 | 08 | 48 | 23、40 |
| SDK / relay | 09 | 29 | 09 |
| Gate C・DAG・EVM | 10 | 05〜07 | 12、18 |
| 料金・資金・予約 | 11 | 35〜37 | 16、35、39 |
| state・Pruning・IBD | 12 | 49 | 30、36、37 |
| bootstrap独立搬送 | 13 | 09〜11 | 41 |
| 凍結例外の状態 | 15（本文を重複しない） | 50 | 21、22 |

model二階層の新04、メモリの新05は参照するが、係数・性能目標を改定しない。

## Source上の接続候補

以下は存在を確認したファイル/モジュールであり、新機能が実装済みという意味ではない。

| 既存ファイル | 必要な作業 |
|---|---|
| `consensus/core/src/palw_security_certification_v1.rs` | 新claimへのPositive必要条件、typed result、旧public-check shortcutの新ruleset迂回防止 |
| `consensus/core/src/palw_security_certification_fold_v1.rs` | 新所見のversioned codec/受付、result・branch・scopeへのbinding |
| `kaspad/src/palw_kernel_roles.rs` | 旧producerのbuild_evidenceを新経路で必須にしない。結果commit＋有限Respond APIを別versionで追加 |
| `kaspad/src/palw_kernel_verifier.rs` | Clean時にもPositive witnessの再計算・構築・提出へ進むPanel prover。既存不正経路は残す |
| `misaka/palw/remote/src/pursuit.rs` | Clean/Prosecute/Demandの既存計画と分離したPositivePlan。要求集約、resume、Panel交代 |
| `misaka/palw/kernel/src/` | Panel witness生成とNode checkerを別API。署名root所有者・source/proverの型を分離 |
| `consensus/core/src/palw_court_shared_original_aggregate_v1.rs` | 全coverage・positive/negative・carrier・保持の原子的予約 |
| `consensus/core/src/palw_court_shared_material_intake_fold_v1.rs` | 全range、bounded chunks、part dedupe、署名帰属と計算矛盾の分離 |
| `consensus/core/src/palw_claim_local_v1.rs` | 依存hold、子孫取消、独立claim進行 |
| `consensus/core/src/palw_native_settlement_v1.rs` | native→settlementの共通追加gate |
| `consensus/core/src/config/params/full_activation_v1.rs` | 承認済みprofileと複数gateのnetwork別同期。未決ならNone |
| `consensus/core/src/palw_row_stream_v1.rs` | positive cursor/resultと資料の復旧。rootだけを計算検証と混同しない |
| `misaka/palw/sdk/src/lib.rs` | 正常Courtの公開API、モジュール宣言・実呼出しまでの接続 |

上表以外にも実consumerを全検索する必要がある。表のファイルだけ変更して完了扱いにしない。最低限、`certifies`、`security_certified`、`Final`のreward reader、receipt、coinbase、rule-E、safe/live weight、EVM、prefixを列挙してコードカバレッジ表を作る。

## 実装順序

1. Minerのwitness生成を持たない新ResultCommit経路で、Panelの再計算・証拠生成→model未保有NodeのEXACT検査を通す。
2. 正直Miner＋偽witness Panel、応答後のPanel放棄、要求重複/越権、provider/fallback、全員共謀を試す。
3. 直接matrix検査をexact基準実装と比較する。全relation被覆は減らさない。
4. 実LLMのcold/warmでMiner追加負担・Panel生成・Node検査を別計測。合わなければ縮小・provider委任・未対応とする。
5. 全consumer、複数Node、reorg/IBD、bootstrap搬送、凍結解除・有効化を同一candidateで閉じる。

本パッケージは文書と小さな参照モデルのみ。Rustの変更、build、CI、node drillは行っていない。

## R2の署名・帰責移行

`Miner result` / `Panel witness` / `source delivery` / `relay transport`を別型・別署名scopeにする。旧trace root必須のclaimを空fieldで流用しない。新source responseを受理した後のPanel計算失敗をMinerのDA defaultへ入れない。SDK/UIにもWitnessRejectedとClaimRefutedを分ける。

新しい型名は設計上の仮称。今回の添付にはRust実装を含めない。
