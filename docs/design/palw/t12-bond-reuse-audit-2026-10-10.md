# testnet-12 bond再利用時刻の確認 — 2026-10-10

結論: **同額bondの発行tokenに同じ上限を課す仕組みはあるが、計算省略・Panel共謀でも再利用時刻が正直な計算と同じになる、という厳密な保証は現行コードにはない。** ライセンス成立がescrowとissuance slotの早期解放に結び付いている。

対象は`pre`の`palw_t12_shipped_params()`が組み立てる設定。前の49%改定を含む未commit状態のソースであり、稼働fleetのbinary・tip・7日間の実測を確認した結果ではない。数値、params ID、HEAD、再現条件は[JSON記録](t12-bond-reuse-audit-2026-10-10.json)に保存した。この確認でproductionコード・fork設定は変更していない。

## Attemptに同じ上限として実装されているもの

F-Sのslot保持・tokenのadmission経路は**Attempt claim**を対象とする。`palw_issuance_holds_slot_v1`はfree-promptを対象外として返す。free-promptの担保、receipt rights、class/panel予算等をこのtoken制限と同一視しない。全報酬対象claimについて目標を保証するには、その経路も検査・統一する必要がある。

最小bondは13,000 BILI、work exposureは500‰で6,500 BILI。Attemptのissuance側は`u = floor(collateral / 6,500 BILI)`、`N_out = u × rho`、token補充は`u × rho / 20` claims/DAA。これらの入力に実計算の有無やGPU速度はない。class room、network room、担保headroom等も満たす必要があり、この値だけで発行・報酬が保証されるわけではない。

| compiled t12の期間 | rho | 13k bondのslot上限 | token補充上限 |
| --- | ---: | ---: | ---: |
| DAA 1,700–5,299 | 10 | 20 | 1 claim/DAA |
| DAA 5,585以降 | 1,000 | 2,000 | 100 claims/DAA |

間のstepはDAA 5,300で25、5,395で100、5,490で250。rateはtokenの補充量であり、実際のFinal件数・計算能力・7日回転率を表さない。DAA 1,700より前はF-Sが未有効で、同じ計算式をその期間の有効な発行制限と扱ってはならない。

`N_out`の対象はslotを保持するclaimであり、**全ての未Final claimではない**。counted `ReceiptLicensed`はFinal前でもslotを離れる。

根拠: [issuance式とadmission](../../../consensus/core/src/palw_issuance_slots_v1.rs)、[t12のstep組立て](../../../consensus/core/src/config/params.rs)。

## 再利用時刻が固定されていない経路

| 資源 | 現行の解放条件 | 実際の計算時間と独立した固定時刻か |
| --- | --- | --- |
| 発行token | chainのDAAに従って補充。ライセンスによるtoken返却ではない | 共通の補充規則はある |
| producerのescrow予約 | Quorum/Coverage、basis_k >= 2、全seat Valid、unservedなし、redrawなし、非C7で早期解放 | ライセンスの到着・成立に依存 |
| issuance slot | countedライセンスで解放 | ライセンスの到着・成立に依存 |
| 残るproducer予約 | 原則Final/conviction等。理由によってvoid後もhold | phase・裁定・期限に依存 |
| seatのslashable lock・bond退出 | それぞれ別の責任期間・退出規則 | producerの新claim枠と同一ではない |

全員Validな署名が揃う時刻を早めれば、早期解放条件も早く成立する。通常の署名の認証は「その計算が実際に正しかったこと」そのものではない。共謀したPanelが不正claimへ早く署名できるという脅威条件では、同じtoken上限の内側でも、正直な検証が終わっていないclaimより早く枠を戻せる余地がある。

[SR-1の解放判定](../../../consensus/core/src/palw_state_v2.rs)と[slot保持判定](../../../consensus/core/src/palw_issuance_slots_v1.rs)を現在のshipped paramsで実行した。A16 8k（`ebf44d0a…`）、同じaccepted DAA 10,000、同じ観測DAA 10,021の合成recordを比較すると、countedライセンス済み側は**3,200.84650080 BILI相当のescrow termを先に解放し、slotを保持しない**。まだPanelBoundの側は予約とslotを保持する。

これは現在の純粋なdecision predicatesでの差の再現である。完全な署名・実行artifact・header admissionを通した偽造攻撃の実証ではない。将来の不正検出・Slash・audit、class予算等を無視して無制限に報酬を獲得できる、とも結論しない。

Finalの最低時刻も通常は`licensed_daa + window_challenge_at(licensed_daa)`を基準とし、class検証horizonやaudit等で遅くなる。accepted DAAだけを基準とする共通のbond再利用時刻ではない。現在の共通window値はreceipt 600 DAA、challenge 1,200 DAAだが、class/fenceによる派生値と別に扱う。

## 13k bondと予約額の読み方

空の合成13k bondへ、現在のproducer価格照会関数`palw_producer_facts_v4`を適用した**own-work Attempt**の値:

| class | accepted DAA 2 | accepted DAA 1,700 | accepted DAA 10,000 |
| --- | ---: | ---: | ---: |
| 汎用floor `f1c5635c…` | 3,200.95402740 | 320.09540274 | 3.20095404 |
| Qwen A16 8k `ebf44d0a…` | 3,225.56250310 | 3,203.31810103 | 3,200.87121681 |
| Qwen A16 2M `74c67e63…` | 62,943.78856900 | 9,175.14070762 | 3,260.58944287 |

単位はBILI。これらはempty-bondの価格照会であり、bondが埋まっている状態の新claim予約額、実際のadmission成功件数、全量実計算結果ではない。A16 8kの上記Attemptは金額だけなら6,500内に2件収まる。約6,451 BILIを現在の全claim種別の共通値としない。free-promptは実行量・入力と`rights_reserved`等を別途指定して評価する必要がある。

既存のfloor testでも、初期設定の予約約3,200.954 BILI・同時2件の金額条件が確認された。claim数だけで7日間のbond回転率を決めることはできない。

## 目標に必要な追加の不変条件

「計算を省略しても同額bondの再利用機会は早まらない」を固定するには、発行枠の消費・再利用を早いValid署名から独立させる必要がある。例えば登録済みclass・正規化したwork量から共通のHを決め、`reuse_not_before = accepted_daa + H`を合意状態に固定し、ライセンス・audit・claim破棄でこの発行creditの解禁を早めない。free-prompt・rider/batchを含む全報酬対象経路へ正規化したwork量に対応する同じ制限を適用し、receipt権利だけ追加してこの時計を短縮する経路も塞ぐ。

これは提案であり未実装。Hを何日にするかは、この確認では決定していない。加えて、責任期間内の未回収損失を担保するrisk reservationは別ledgerとして保持する。Hだけで未解決責任を解放して二重に担保を使わない。遅い正直な計算・network障害では解禁が遅れることがあるため、保証する共通の最早時刻と、実際の到着時間まで全員等しいことを区別する。

受入検証では同じbond・class・claimed work・accepted DAAを固定し、正直な計算と省略＋早期共謀署名を対にする。escrow/slotのrelease、token、Final前weight、未解決claimの責任、void/restart/reorg/退出、bond分割を含めて、後者に早い再利用・多い報酬機会が生じないことを確認する。

なお前の改定でreporter shareは49%である。攻撃者が別identityのreporter報酬を回収できる評価では、期待損失にgross Slashをそのまま使わず、reporterへの還流等を差し引いたcoalitionのnet lossを用いる。これは再利用時計の不変条件とは別の経済評価である。

## 検証・再現

既存test `t12_mainnet_assumed_bonds`の2件と、`palw_capacity_stage2_q_s::s_the_outstanding_cap_binds_and_a_counted_licence_frees_a_slot`の1件が通過。後者は歴史的t12 fixture上でslot解放の実際のfoldを検査している。現在設定・現在のdecision predicates・class別Attempt価格照会は追加probeの3件が通過。**合計6件成功**。本番の7日連続run、実モデル計算、実効検出率、C・R・L・最大不正利得の測定は行っていない。

再現用sourceは[evidence/probe](evidence/t12-bond-reuse-probe-2026-10-10.rs)。これを一時的に`consensus/core/tests/codex_t12_bond_reuse_probe.rs`へ置き、`cargo test -p kaspa-consensus-core --test codex_t12_bond_reuse_probe -- --nocapture`で実行する。共通fixtureを参照するため、その配置が必要。今回の一時testは実行後に除去した。
