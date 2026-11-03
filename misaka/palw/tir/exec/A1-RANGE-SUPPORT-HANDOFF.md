# A1 range support: read-only findings

2026-11-01。参照snapshotはA1 `ea5bd852f`。調査対象のsourceは変更せず、buildも実行していない。
本書は既存実装の保証と対応範囲を記録する。guard変更、新actual-value経路の仕様化・採択、
新profile／wire／native authority／公開有効化は行わない。

## 何が拒否されているか

`tir/core/src/interval.rs::analyze_ranges` は、各tensorの全lane・全positionを覆う
保守的な区間を求める。`Ref::CarryIn` はproducerの実到達値域を引き継がず、宣言dtype
全域から始まる（:208）。Paramも宣言dtype全域（:211）、Constはliteralの最小・最大、
Fixed stateは宣言lo/hiを使う。Addの区間両端の和がoutput dtype内に収まらなければ拒否
する（:109–112）。

したがって、実際のI32 carryが0なら `+1` は正しく計算できても、解析上のI32最大値に
1を足すと範囲外になり得る。この拒否は、当該claimでoverflowしたことや、モデル／Miner／
Panelの不正を示さない。成功時は、解析の前提下で許されるparams・tokens・positionsに
対する型・演算条件の十分条件となる。失敗は実行不正の必要十分判定ではない。

## 現source-onlyと旧V1の境界

| 入口 | 現在行うこと | 読み替えてはいけないこと |
| --- | --- | --- |
| `source_only.rs::authenticate_source_only_v1` :228 | 全programのrange解析を必須とし、その後に資料認証とoperand区間検査 | 実値だけ安全なprogramも受理する入口ではない |
| `source_only.rs::build_material` :570以降 | 原storeから実値で資料集合を探索。validatorと同じrange解析はしない | builder成功をNode受理やadmission成功としない |
| `full_relation.rs::source_error` :623以降 | range解析のBinding拒否をDeliveryへ写し、coverageを進めない。累積予約は消費 | signed claimの算術不一致、Miner／Panel不正としない |
| `tir_admit_with_rules_v1` :697／native `palw_tir_admission_v1.rs` :590 | 旧V1の実admissionもrange解析を要求 | NF検証・binding構築・実行成功だけをadmission receiptとしない |
| `verify_tir_binding_v1` :1106以降 | class/job identity、geometry、count、rootの整合を検査 | range admissionを済ませた証拠ではない |

現FullRelationの対応範囲は、**既存range契約を満たす元programに対し、原資料を認証し、
claimの全relationを有限予算で直接検査すること**まで。実値が安全でもuniversal解析だけで
拒否される元programは現profileでは未対応である。モデルの信頼審査・tier変更へ転じない。

旧Courtではrange解析と `first_outside` が、nodeだけでなくcheckpoint writer／historyの
区間も制約する（`palw_tir_court_v1.rs` :640、:1450–1475）。この前提で後続の算術エラーを
`Unadjudicable` と扱い、規範的な裁定順序を維持する（:960、:1361以降）。guardの一括削除や
旧interval／fault意味の無言の変更は、この調査では正当化されない。

## Lowererとの適合とfixtureの呼び方

通常lowererの `Op::Add/Sub` はI64で演算してからtarget dtypeへClampする
（`tir/lower/src/lower/mod.rs` :1586–1588）。`library::narrow` はI128 Mul/DivからClampする
（`tir/core/src/library/mod.rs` :160以降）。手製fixtureの「I32 CarryInをそのままI32 Add」
を、この標準経路の出力と同一視しない。一方、通常 `lower()` の末尾はNF validateで、
range admissionは別入口である（`lower/mod.rs` :568、`lower/admission.rs` :23）。
lowering／capture／実行成功だけでは、すべての実学習済みTIRのrange適合を主張できない。

A3が報告したNEW I8 carry／Clamp fixtureの361／370 relation成功は、元の拒否された
programとは別identityの局所実証として扱う。この件数を別scopeや過去の成功件数へ加算しない。
「登録fixture」は、テスト内でclass metadataを構成したという意味と、actual chainでnative
admission receiptを得たという意味を分ける。今回actual chain receiptは供給されていない。

既存登録programへClampを挿入する、dtype／commit flags／layout／classを差し替える、
max contextを短縮する、といった回避で元classの対応完了としない。実学習済みprogramの
適合には、その元bytes／digest／profileと最初の失敗block・node・primitive・区間を特定する
必要があり、今回のfixtureの成功だけからは決められない。

## Claim-level actual-value経路に必要な前提（未仕様化・未採択）

FullRelationには、Nodeが次relation／frontierを再導出し、原opening・直接計算値・Panel固定値・
元tree値を照合してからprefixを進める構造がある。将来actual-value経路を扱う場合も、
単なる数値frontierや所属認証を、計算済みprefixの権限としてはならない。

必要条件は、非公開の実検査済みprefixへのfactory結合、原model／params／input認証、
全positions／nodes／lanes／state／historyの被覆、canonical read-set closure、元dtype・
state宣言範囲・Div／Gather条件・実overflow・正負別partial sumsの検査、有限なwork／read／
heapと再試行予算の維持である。有限資源拒否と実演算エラーの分類も固定する必要がある。
現在のDemand evaluatorにはchecked演算・dtype fit・state制約があるが、HistTileは
`evaluate_leaf_with_work` :945でsource値を直接連結するため、独立した実値／dtype検査を
省略できない。既存factoryからrange guardだけを取り除く案ではない。

この前提整理はclaimの実計算・資源・未検証分類のためであり、新しいモデル単位の審査を
追加するものではない。actual-value経路そのものは今回実装・採択していない。
