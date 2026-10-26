# Panel=1 の公開有効化までの残件（2026-10-26）

この記録は `codex/rfc06-12-continuation` の `f4054fe39` を基準にした点検であり、arming の承認や実 campaign の PASS ではない。2026-10-26 03:11 UTC 頃、`misaka-ibm` のローカル JSON wRPC（読み取りのみ）の `getBlockDagInfo` は `virtualDaaScore=15842`、`sink=b341b7e3c311b2ac…`、`getPalwNodeStatus` は genesis `a27f8f44fe4d91a5…`、params ID `ef379378f09bf33f…` と返した。これは一台の時点観測であり、fleet 同期・出荷 binary・fence drill の証拠ではない。

## 直接の依存関係

2026-10-22 の[採択記録](../../design/palw/integration/g14-t12-16000-decisions-2026-10-22.md)は kernel route の 1 seat / quorum 1 を採択し、DAA 16000 を意図した。`palw_kernel_route_panel_one_v1` の座席計算、draw/lock の consumer、V2 mirror と params identity は実装済み。現行 preset は `None` であり、`PALW_T12_FULL_ACTIVATION_DAA` も `None`。当該高さだけを書き換えても有効化できない。

`validate_palw_kernel_route_panel_one_v1` は `palw_probabilistic_constraints_v1` が同じ高さ以前に有効で、V2 mirror が一致することを要求する。実ネットワークの kernel route は `PALW_KERNEL_ROUTE_ARMING_RECORDS_V1 = &[]` のため、全ての armed height を拒否する。record は network genesis、高さ、試験に使った source commit、8 要件それぞれの異なる evidence digest、review digest を要する。形式を満たす digest だけでは実証にならない。

| kernel route の arming 要件 | 現在の残り |
| --- | --- |
| fold・boundary decode・全 consumer | in-process の実装と局所試験を出荷 source で再確認し、全 carrier/consumer の境界を閉じる |
| 実 node E2E | 正直な claim の quorum→Final、外部者による偽 claim の公開裁定、DA 不足・無 quorum・reorg の拒否を同一の凍結 binary で示す |
| 適応的・算術的な adversarial suite | 誤 scalar・modulus alias・rounding・TopK・memory・projection・重複 work・challenge 再利用・beacon withholding・相関 seat を埋める |
| 独立 review | claim 全体の検出前提と beacon/transcript を実装者以外が審査する。選出 verifier の正直な割合・参加・取得可能性を明示する |
| Panel route との shadow 比較 | 同じ claim について一致、偽受理・偽拒否、費用を計測する |
| 実規模の費用 | 実 9B-8k と妥当性確認済み 2M について producer、witness、前処理、verifier、転送、最悪 court 負荷を別々に測る。ADR-0171 §5 の K3 feasibility は正式入力が未作成。小型 fixture を代用しない |
| court 容量 | 最大許容 model/context の遅い単一故障と同時複数事件について総 work・bytes・期限の上限を測る |
| fence 跨ぎ | 凍結した出荷 binary で前後の受理、restart・IBD・reorg、旧 claim の旧規則精算を確認する |

`g14_kernel_route_e2e` など既存の in-process 試験は上記の個別入力になり得るが、実 node の campaign や実測を置き換えない。Panel=1 では claim ごとの抽選検証者の承認、Gate C / `SecurityCertified`、公開 court を全モデルで維持する。

さらに、現行 `draw_seats` は `palw_kernel_interim_seed_v1(claim_id)` で候補を並べる。コード自身がこの seed を **grindable / NOT a production beacon** と記している。1席の局所試験が PASS しても、その抽選を公開 Panel=1 の安全性の根拠にできない。RFC07 の独立した post-commit source の接続、bias・abort/retry・reorg の評価、正直な抽選検証者の割合と取得可能性の前提が必要である。

既存の pinned `kaspa-consensus-core` テスト executable（SHA-256 `99f72d2b04c8b65b0928004f14dfdb7ce75f423a753bec9369715a78d2f68c51`）で `palw_kernel_route_arming_v1::tests::` を実行し、4/4 PASS。確認範囲は dormant/refusal、record の network/height 一致、Panel=1 の座席境界、registration/provider court の順序だけである。実 node E2E、独立 review、測定、fence 跨ぎは未実施。

追加の native in-process E2E `g14_kernel_route_panel_one_assignment_and_outsider_conviction` は `cargo test --offline -p kaspa-consensus --lib --features evm,private-integration-config` で **1/1 PASS**。実際の native draw が座席 1・quorum 1 を書き、producer 自身を座席にせず、座席が偽 claim を覆った後も外部者の公開 proof が有罪に到達することを確認した。テスト world は arming record 不在を前提に validation を迂回するため、公開 t12 の武装・post-commit source・選出検証者の承認を証明しない。[実行記録](panel-one-native-test-2026-10-26/record.json)（source `8bbfaba0e` + test 差分、実行時ファイル SHA、exit 0）、[全ログ](panel-one-native-test-2026-10-26/build-and-test.log)（1 passed）。binary SHA-256 は `d4f0a521a4896ecae8f7afd0c432901380c2a767ca0c229734ce18276a77f91f`、record SHA-256 は `fc289796590057410b09564e06fa08294e87345c8f2e87942ab64132bbf5eeb3`、log SHA-256 は `a4c4fbc85985d0e9e02a9b80d5ca8bf3fb80171df0f7efc9b87b97be27cf8947`。

## 有効化の境界

- **Panel=1 の座席切替だけを考える場合:** kernel route の 8 件を実証し、独立 review と network/height に対応した record を pin し、mirror・identity・fork-id と凍結 binary の跨ぎ試験を通す。16000 に間に合わない場合、過去高を後から武装せず、改めて将来高を定める。
- **2026-10-26 の新目標:** G14 slice の候補は DAA 17000。高さは依然 `None` で、上記 8 要件と claim 単位の `SecurityCertified`、独立 beacon、費用・容量・実 node campaign が満たされるまで武装できない。
- **既存の t12 full-activation 一覧を使う場合:** 上記に加え、一覧の G14、RFC16、TIR、RFC06/08 などの各 record、測定値、policy、依存 fence をすべて充足する。これは Panel 座席切替より広い作業であり、現在の `None` を解除できない。
- **RFC06/07/08/12 の公開全機能:** 予約 consumer・実 provider/adapter・全 reward の実 coinbase/一回精算・restart/reorg/prune/OS crash・EVM/bridge/market/history の検証は別の残件。K3 入力は未作成、native39→RFC12 決済写像は未策定、実 campaign の 15 gate は未実行で UNKNOWN。いずれも Panel 座席ポリシーの局所試験から PASS へ昇格しない。

参照: `consensus/core/src/palw_kernel_route_arming_v1.rs`、`consensus/core/src/palw_kernel_route_v1.rs`、`consensus/core/src/config/params/full_activation_v1.rs`、`docs/PRINCIPLES.md` §§5, 8, 9。
