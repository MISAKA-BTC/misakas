# ADR-0235 — モデル採用をkernel自動追加とBFT YES/NOへ分ける

- Date: 2026-11-23（Asia/Tokyo）
- Status: **ADOPTED_DESIGN_NOT_ACTIVATED**
- Authority: 今回のユーザー指示「モデル追加経路でkernelの自動追加経路とBFTによる承認の追加経路で2種類」「BFTは…YES NOだけで証明不要」。
- RFC: [RFC-0067](../rfc/0067-model-admission-kernel-auto-and-bft-decision.md)
- Transition: [MAD2移行契約](../governance/model-admission-dual-route-transition.md)

## 背景

MAS1は自動判定のUNKNOWNを保留し、BFTを停止・証拠付き解除に限定した。これでは、通常のLLMに対する非衝突定理や費用審査が揃わない場合、人の採否判断で実用モデルを採用する経路がない。ユーザーは二つ目の入口を明示的に追加した。

また、LLMの出力が計算前に予測困難であることと、モデル非保有Nodeが提出出力を安く検証できることは別である。モデル採用の簡素化を、個別claimの検証廃止へ拡大しない。

## 決定

1. 採用basisはKERNEL_AUTO又はBFT_DECISIONの二種類。同じ不変instance、admission reducer、停止集合を共有する。
2. 自動経路は既存kernelへのloweringと現行自動条件を使う。新しい未知primitiveの実行意味を自動ダウンロードしない。
3. BFT経路は所定rosterのq YESで採用、q NOで不採用。理由・数学的証明・費用/品質の証拠提出は票の成立条件ではない。
4. 共通技術条件は両経路で必要。未知演算・非決定的意味・codec/commitment不正をYESで免除しない。
5. G-COLL/G-COST/G-USEのUNKNOWN又は採用判断FAILを保持したままBFT_POLICY_APPROVALとして採用できる。全PASSを共通ANDへ戻さない。
6. n=3f+1、q=2f+1の権限・署名認証と、同一proposalへの非二重署名を要求する。実人数・署名codec・activationは別途固定する。
7. BFT承認は数学的安全性の証明ではない。未知の高速算法や品質低下は採用判断の残余リスクとして引き受ける。
8. claimのPanel、Court、Final、独立Gate C、Ticket、権利と会計は維持する。新しい乱数・台帳finality・別の受賞経路を追加しない。
9. 停止は承認より優先し、解除は現在の停止集合へ明示的に束縛する。正しい高速化や誤った採用判断だけで過去資産を没収しない。
10. 旧claim・旧評価結果・既存市場・過去ログは保持する。本決定からQF1合格、native実装又は公開完了を推定しない。

## 追加指示：genesisだけをBFT管理対象外とする

genesis manifestでchain開始時に固定されたGENESIS_ORIGINALは、BFTによる採用・変更・削除・停止・解除の対象外とする。genesis追加や分類の後付けを許さない。個別claimの正当性・当選・会計検証は維持する。

追加モデルは自動採用又はBFT YESで登録し、どちらの経路でもBFTが停止・再審・正規解除を管理する。BFTはFORK_DESIGNATEDもYES/NOで追加でき、そのモデルはOption対象外・対象worker報酬R100% Minerというgenesis同様の経済扱いを受ける。しかしBFT停止免除は持たない。**停止管理の対象外は元genesisだけ**である。

投票本文に経済分類と不変originを束縛する。fork候補の事前ADDITIONAL市場登録は不要とし、YESの正規包含でfork登録・catalog指定・採用を原子的に行う。既存追加モデルの変換では永久LP・旧市場・責任・停止を消去できない。旧「coreも全てBFT停止対象」はgenesisについて明示的に置換する。

## 明示的な置換

| 旧規則 | MAD2対象での後続規則 |
| RFC0065のgenesis/fork一律のBFT安全管理 | 元genesisだけ除外。追加モデルとforkモデルはBFT管理対象 |
| forkは既存catalog指定でのみ追加 | MAD2ではBFT YESによるfork指定を追加。停止権限を保持 |
| --- | --- |
| RFC0062の全候補G-COLL定理・G-COST/G-USE合格必須 | KERNEL_AUTOの要件。BFT_DECISIONでは採否投票へ置換する |
| RFC0065/ADR0233のBFT停止専用・UNKNOWN承認禁止 | 採用権限を用途別に追加。q YESによるBFT承認を許す |
| RFC0066/ADR0234の未完成G-COLLによる一律採掘保留 | 自動経路は保留。技術互換なQF1 instanceはBFT経路を選べる |
| BFT安全停止解除時の全自動gate PASS | 自動経路は維持。BFT経路は技術互換・現YES・正規解除へ分岐する |
| ADR0180の台帳BFT禁止 | 維持。採用と停止以外の権限を与えない |

本ADRは旧文書番号・wire ID・署名済みpayloadを再利用しない。更新前の規範本文は[改訂記録](../maintenance/revisions/2026-11-23-dual-route-admission/README.md)に保存する。

## 影響と実装方針

採用を人の判断で進められる一方、数学的なモデル安全性の保証は増えない。誤採用、審査の集中、検閲、quorum不成立、停止遅延を明記する。自動経路は投票なしの入口として残す。

実装は新しいadmission basis、票のcarrier、通常状態へのfold、共通gate、UIの保証表示に限定して接続し、既存の停止・署名・kernel・claim処理を再利用する。Gate C等の削除は別の設計変更として扱う。
