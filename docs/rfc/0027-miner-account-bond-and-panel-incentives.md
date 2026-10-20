# RFC-0027 — Miner Account Bond と Panel 追及報酬: 10万の基礎担保、責任連動の追加拘束、REAL継続性

- **Status:** Proposed / Review Required（未採択）。作成日: 2026-10-20。
- **依頼:** ユーザーの「100,000 MSKを最低額とし、抱える claim の責任が増えれば追加 bond」「Panel に十分なインセンティブ」「担保負担による claim 減少・heartbeat 依存を考慮」を詳細化する。
- **対応 ADR:** [ADR-0193](../adr/0193-miner-account-bond-for-panel-incentives.md)。番号は添付 ZIP と会話で確認した範囲のローカル仮割当。他 branch の重複確認は統合前に必要。
- **Owner / reviewers:** 設計責任者による採択・経済レビュー・合意実装レビュー・独立検証レビューを要する。担当者は未指定。
- **Implementation / Verification / Activation:** 合意実装なし。数式検算に加え、既存node/外部検証の実測ログ再集計、shadow再生、合成資本実験を実行（§18）。新Account Bondでのnode実測・敵対的統合試験・有効化なし。wire ID、Params、fence DAA、fingerprint は未割当。
- **通貨表記:** ユーザーの表記に合わせて本書では MSK と記す。添付正本の ticker は BILI（[ADR-0174](../adr/0174-token-name-misaka-ticker-bili-address-prefix-unchanged.md)）。同じ native coin の旧ラベルであり、再改名・別通貨化・単位変更ではない。`1 coin = 100,000,000 sompi`。
- **上位規範:** [PRINCIPLES](../PRINCIPLES.md)、[変更手続き](../governance/design-freeze.md)、[RFC-0026](0026-court-policy-master-contract.md)。採択済み条項・旧 claim・既存証拠は変更しない。

## 0. 一文でいう設計

**既存の claim 予約 bond は「claim を出すことにコストを持たせる」ために維持し、新しい Miner Account Bond は「不正が客観的に確定したとき、Panel / 外部検証者が追及費用と成果報酬を実際に回収できる」ための別勘定とする。Account Bond は最低100,000 MSKで、残存責任の総額がその内側に収まる間は追加不要とする。**

```text
既存の生産・claim 予約勘定 P ── 既存の claim 発行コスト・権利・責任・Q/B/R/F
新しい Account Bond A     ── 客観的な違反時の追及費用償還・固定成果報酬・純罰則
通常の Panel 報酬勘定     ── 既存の適格報酬配分 / 実在する承認済み原資
court の FeeEscrow        ── 開示・対話・裁定補助の作業料を段階払い
```

「100,000 MSKを入れるだけで Panel が毎回それを受け取る」設計ではない。正当に行動した Miner の新担保元本を通常の運営費として削らない。返還可能な担保は、未没収のまま支出できる収入ではない。

## 1. 問題、確認済みの事実、変更が必要な理由

### 1.1 解く問題

Panel が異常を見つけた後、開示要求・対話・局所化・最終証拠・回復・保持に費用を負う。通常の receipt 報酬だけでこの追加作業が採算に合うとは限らず、追及しない方が有利になる懸念がある。一方、一律に claim ごとの担保を大幅増額すると、正直な Miner の受理余力が減り、有用な REAL block が途切れ、heartbeat に依存する時間が増える可能性がある。

これは**設計上の懸念と検証課題**である。§18の追記測定で長期拘束と既存P減少による制約を数値化したが、「現行 Panel は必ず赤字」「10万で十分」「新担保で heartbeat が減る」と実証したものではない。重大変更の正式採択には §15 の費用・参加・継続性の測定証拠を要する。

### 1.2 既存設計との照合

| 観察 | 本提案への制約 | 出典 |
| --- | --- | --- |
| 既存の claim 予約は一律3,000という全種別共通定数ではない。約3,200の記録は特定 Attempt の見積りで、weight / right / penalty / other に分かれる後続案もある | 「約3,000」は説明用概数。現行の式・保持・損失担保を消さない | [ADR-0176](../adr/0176-bond-bounds-palw-production-rewards-and-final-weight.md) の2026-10-15改定 |
| 既存の Panel 報酬配分と、Panel 自身の exposure 拘束がある | 「Panel 報酬ゼロ」からの新設ではなく、追及報酬の不足を補う提案 | [ADR-0124](../adr/0124-the-panel-is-paid-out-of-the-claims-reward-a-seat-holds-exposure-and-a-claim-is-paid-for-the-compute-it-certifies.md)、[Panel economy 実装](../../consensus/core/src/palw_panel_economy_v1.rs) |
| Panel の最低 collateral を producer の共通下限の10倍から導く経路がある | 新しい10万を既存 `min_collateral_sompi` に直接代入しない | 同上 `palw_panel_collateral_floor_v1` |
| reporter share は旧10%と後続49%を version/fence で分ける | 新 account 没収の全額へ旧 reporter 割合を追加適用しない | [既存 settlement 定数](../../consensus/core/src/palw_state_v2.rs)、[ADR-0032](../adr/0032-palw-fee-bond-escrow.md) |
| 共通期間・Q/B/R/F・非早期回復と、REAL / heartbeat の区別が採用方針にある | 参加負担を減らすために既存の上限・責任時計を短絡しない | [bond 仕様](../spec/palw/bond-budgets.md)、ADR-0176 |
| Final と SecurityCertified は別 | 正常報酬の支払を口実に未認証の発行由来報酬を払わない | [RFC-0018 §7](0018-dag-parallel-multi-model-consensus.md) |

既存 fee / 報酬率だけの調整では、Miner 全体の新しい参加資格、複数 claim を跨ぐ新しい責任予約、退出制限、account 原資からの償還を定義できない。本案は単なる数値変更ではなく、新 RFC と新 ADR を要する責任会計の追加である。

### 1.3 入力の範囲

添付 ZIP には RFC-0026 はあるが、会話にある ADR-0192 本体と F02/F03 詳細案の対応ファイルは確認できなかった。会話の貼付案は **Proposed の追加要件**として扱い、A/A の採択・実装を仮定しない。確認したファイル、行範囲、digest、未確認事項は [source-review](0027-assets/source-review.md) に記録する。

## 2. 目的と非目的

**目的:** 不正追及に対する徴収可能な報酬原資を事前固定する。正常検証の報酬と不正立証の成果報酬を分離する。基礎担保を複数 claim に無二重計上で割り当て、必要な追加額を予測可能にする。小規模 Miner の実用性、Panel の採算、継続的 REAL 生産を同じ試験で評価する。

**非目的:** 既存予約 bond を廃止・一律値へ変更しない。既存の損失担保を「単なる fee」と再定義しない。全額没収を Panel の自由裁量にしない。モデル単位の公開性・取得可能性を審査しない。計算の暗号学的正当性証明を導入しない。DNS-BFT、Panel=0、独立 frontier、延期中の画像・動画・モデル改善の経路を本案だけで有効化しない。

## 3. 用語と勘定の境界

| 記号 | 意味 |
| --- | --- |
| `AccountID` | 署名鍵の変更でも残る責任 identity。新規 wire/hash domain は別レビューで割当 |
| `A` | 新 Account Bond に専用ロックした実在元本。既払 slash を控除した残額 |
| `A_min` | 100,000 MSK。本提案に対するユーザー指定の最低額。稼働パラメーターとして未有効 |
| `P` | 既存の生産資本・claim 予約・既存責任を支える勘定。適用 ruleset の制約をそのまま受ける |
| `E_A` | account 元本がまだ負っている、新しい責任予約の総額 |
| `L_c` | claim `c` の全許容訴追・違反 outcome に対して必要な account 責任 envelope |
| `FeeEscrow_j` | job `j` の作業料として前払いされた、消費可能な実在資金 |
| `ServicePool` | 既存報酬配分等から適法に資金化された通常検証費用用の勘定。未発行・未認証の予定額を含めない |
| `Q/B/R/F` | 既存の claim / 報酬 block / 報酬 / Final weight の上限 |

Account Bond は EVM 表示残高そのものではなく、既存 native bond と接続する拘束資本の**別用途 subledger**とする。同じ物理 UTXO / bond 内で表現しても、`lot_id / purpose / amount / encumbrance` による元本分離を要する。

原則 `A` は既存生産資本 `P`、Panel 自身の担保、申立人担保、利用者 escrow、第三者 custody の担保と兼用しない。将来の没収予定額、未払報酬、資金移動待ちを入金と見なさない。既存の元本を用途変更できるのは、それが無拘束であり、既存の期間予算・責任履歴が移動によって消えない場合だけである。

## 4. 最低額と追加 bond

### 4.1 推奨式: 基礎担保を先に使い、不足だけを追加する

```text
A_required = max(A_min, E_A)
TopUpNeeded = max(0, A_required - A)
HeadroomForNewLiability = max(0, A - E_A)
```

`A_min + E_A` としない。最低10万は、未使用のまま永久に上積みする手数料ではなく、責任を予約できる実在の基礎プールである。初期の複数 claim が10万の範囲に収まるなら、account の追加入金は不要である。ただし同じ1 MSKを同時に二つの損失へ保証しない。

複数 claim の同時裁定を全て扱うため、基準は `E_A = Σ_c L_c_remaining + L_account_specific` とする。account 固有の違反を設ける場合、その別予約も和に含める。何件も不正を起こせるのに最大1件だけを取る `max_c L_c` は禁止する。

### 4.2 例: 数式の説明であり採択値ではない

仮に同一 profile の `L_c = 12,000 MSK` とする。これは測定済み料金・本番設定ではない。**§18の追記実験では、この説明値は長期責任下の継続発行を大きく制限したため、実用候補額としては採用しない。**

| 残存責任を持つ claim 数 | account 責任総額 | 必要 Account Bond | 最低10万を超える追加額 |
| ---: | ---: | ---: | ---: |
| 1 | 12,000 | 100,000 | 0 |
| 5 | 60,000 | 100,000 | 0 |
| 8 | 96,000 | 100,000 | 0 |
| 9 | 108,000 | 108,000 | 8,000 |
| 20 | 240,000 | 240,000 | 140,000 |

既存予約を説明上3,000/claimと置くなら、5件の「新 account + 旧予約の額面」は115,000となる。しかし**必要 wallet 残高の保証値ではない**。例えば既存側で `ΣL ≤ uP`、`u=0.5` が適用されるなら、旧予約15,000に必要な `P` は少なくとも30,000となり、`A+P` は130,000以上になる。他の役割の拘束、既存下限、fee、保持・Q/B/R/Fも別に確認する。`A` に既存 `u` を再度掛けて「10万入金したのに5万しか裏付けにならない」と黙って変更しない。

### 4.3 基礎額の限界

10万は「全 profile の任意件数を保証する額」ではない。最小の実用 profile でも `L_c > 100,000` となるなら、その差額が必要であり、「10万で参加できる」と宣伝しない。この不一致は §15 の参加実用性 gate で未達と報告する。最低額を自動で上げたり、モデルを除外したり、未計上の分岐を消して達成と表示しない。

## 5. `L_c` の導出と最悪責任

### 5.1 original acceptance で固定する項目

claim、ruleset、source root、profile、許容要求集合と canonical な導出、責任 signer、義務 identity、各作業の tariff と件数上限、成果報酬上限、不可還流の罰則、通常期限、hard deadline、後続 horizon、資金 lot と帰属先を固定する。

F02/F03 貼付案の義務 identity は、`(claim, ruleset, source root, dispute family, range/coordinate, turn, response kind)` とする。owner、relay、受付競争順を含めない。account の支払い ticket はその義務 identity と `funding ruleset / job kind / entitlement slot` を参照する。支払先を変えることと仕事を増やすことを混同しない。

### 5.2 費用 envelope

各合法 scenario `s` に対し、同時発生し得る一意な費用・違反の集合を `J(c,s)` とし、次を基準に導出する。

```text
L_c = max over legal s of Σ over unique j in J(c,s)
        (CourtCostReimbursementCap_j + FixedBountyCap_j + NonreturnablePenaltyCap_j)
```

scenario 内で複数の合法 choice、内部 node、署名、回復、post-Final の別責任が同時に必要になり得るなら全部を含める。相互排他的であると合意規則から証明できる outcome だけ `max` で共有し、それ以外は和を取る。tree 全 node に要求が広がる profile を「1人1経路」で見積もらない。

同じ認証済み body を共有できる場合の重複統合は許すが、異なる claim / root / ruleset は流用しない。期待不正率、期待 owner 数、保険的な分散、確率的 overbooking で額を割らない。予約の厳密な最大値を求める計算自身も有界であり、導出できない項は admission 不可とする。

### 5.3 既存担保との二重徴収防止

責任台帳の費目に `charge_id / obligation_id / funding_source / cap / paid / reimbursed / extinguished` を持たせる。既存予約が保証する費用・penaltyを、新 account の同じ費目へ無条件に足さない。

既存予約による没収・損失は元 ruleset のまま保ち、新 account は**追加の追及費用償還・成果報酬・明示された追加罰則**のみを負う。旧規則により同じ実費がすでに賄われた場合、新 account の償還はその未償還差額までとする。旧 reporter 配分と新 bounty の併給を認める場合は、双方を最初から総報酬 cap と共謀還流分析へ入れる。旧 reporter 割合を新没収額へ機械的に重ねない。

## 6. Panel の報酬: 通常作業、追及作業、立証成功を分ける

### 6.1 三つの報酬

| 種類 | 支払対象 | 原資 | 不正がなくても成立するか |
| --- | --- | --- | --- |
| 通常検証報酬 | 適用 verifier 規則で認められた検証 job / receipt | 既存の Panel 報酬配分、または別途承認・資金化済み ServicePool | 成立する。未認証の新規発行は不可 |
| 追及作業料 | 規定の開示・局所化・対話・裁定補助を完了した一意な milestone | 申立て時に担保とは別に確保した FeeEscrow | 成立する。棄却でも完了分は支払う |
| 立証成果報酬 | 客観裁定で有効になった一意な違反の成果 entitlement | 当該 claim に予約された Account Bond の没収 | 計算不正 / 帰責成立の default 等、定義済み違反に限る |

ここで Panel は drawn seat に限定しない。既存の適格 public verifier も同じ仕事に同じ料金で参加できる。ただし、誰でも同じ仕事を無制限に重複請求できるという意味ではない。

### 6.2 正常時の財源を没収見込みに依存させない

不正ゼロなら没収収入もゼロになる。従って通常検証は既存の適格報酬経路を維持する。既存20%という値をこの文書で変更せず、「20%あれば十分」とも置かない。既存の Gate C と Panel leg の接続には未決記録があるため、実際の支払経路を確認する。

新しい通常作業料や最低報酬保証を既存経路だけで賄えない場合、承認済みの実在 ServicePool を予約してから仕事を依頼する。既存 `panel_reserve_sompi` が表示されているだけでは自由に送金可能とは仮定せず、資金化・支払権限・保全・払い出し規則を実装レビューする。未承認の reserve を予算へ算入しない。これが未完なら新しい通常報酬保証は未達であり、没収予定額で穴埋めしない。

claim に由来する issuance leg は `Final` だけで支払わず、適用 ruleset の `SecurityCertified` 条件を満たす。外部前払いの作業料と発行由来報酬は別勘定であり、前者の支払から claim の計算 credit は生まれない。

### 6.3 court の段階払いと費用償還

申立て時に、閉じた要求集合の該当 job と最大実行 tariff に対する `FeeEscrow` を確保する。既存の fee / challenger bond に支払用原資が含まれるかは科目ごとに照合し、**返還される予定の bond を同時に作業料として支出しない**。公開 frontier の法的な開示権と、費用を負担する job の支払権を分ける。

作業料は完了 milestone ごとに entitlement を確定し、適用する成熟 / reorg 条件を経て支払う。全 court 完了を待たせない段階支払を推奨するが、合意前の自由送金や未取得成果への前払いはしない。料金は実時間の自己申告ではなく、予約済みの標準作業単位と適格成果物に基づく。

Miner の違反が確定した場合、account から FeeEscrow の実際の費用負担者へ、支払済み・未償還の適格費用を cap 内で戻す。Panel が既に受け取った作業料をもう一度 Panel に払わない。第三者が費用を拠出する場合は、非保管型の資金帰属と受領先を固定し、Signer の資金鍵を委譲しない。

正しい claim に対する棄却は、申立人側が完了済み作業料を負担し、残額を返す。棄却だけで申立人を「悪意あり」と slash しない。偽署名・二重請求など別の客観的違反は、その責任者に帰属させる。

### 6.4 料金と「十分」の判定

profile / job ごとに以下を計測し、承認済み tariff を original acceptance で固定する。

```text
Cost_ref = 計算 + bytes/転送 + IO + 署名/検証 + 保持 + 運用
           + 費用立替の資本拘束 + 支払待ち + 適格な回復作業
TargetPay = Cost_ref + 明示された利益余裕
```

利益余裕を正に置く方針を推奨するが、その率・参照ハードウェア・token建て費用シナリオは測定・承認待ちである。単一マシンの費用だけで全 Panel の採算を保証しない。チェーンが電気代・人件費・token価格・実測 GPU 時間を oracle なしに知ると仮定しない。費用調査を人手でレビューし、versioned tariff に反映する。

通常検証、成功 court、棄却 court、供給欠損、交代/回復について、総受領・自己負担・資本費・待ち時間・純利益を別々に示す。Panel 報酬だけを上げて Miner が退出する点まで、同じ感度分析に含める。

## 7. 成果報酬と slash の客観性

### 7.1 裁定権限

Panel は証拠を提出・検証する。資金の没収は、合意が再計算できる有効な客観裁定と固定された科目・上限に従う。Panel vote、自己申告の労力、罰を重くしたいという要求で額を決めない。データの署名・commitment は帰属・同一性を認証するものであり、モデル計算の正しさそのものの証明ではない。

主観的な「悪意の強さ」ではなく、計算不正、署名済み矛盾、適格な開示/応答義務違反等の機械的な違反種別へ価格を対応させる。前の説明にあった5,000 / 25,000 / 100,000の段階額と40/40/20配分は採用しない。料金表は未確定である。

### 7.2 新 account charge の配分

```text
S_j = CostReimbursement_j + FixedBounty_j + NonreturnablePenalty_j
0 <= S_j <= 予約された当該違反の残存責任
```

費用償還は実際の適格費用と cap の小さい方。bounty は profile / 違反種別ごとの事前固定額を上限とし、Miner の account 総額、Panel が要求した slash 額、owner 数には比例させない。不可還流部分は v1 提案では burn とし、同じ攻撃から claim 可能な ServicePool への再循環を行わない。burn額・比率は未承認であり、没収対象ごとに全配分の合計が実没収額と一致しなければならない。

**1件の違反で account 全額を機械的に取らない。** 当該 claim に予約された上限を超えず、他 claim の保証元本を侵食しない。「最大100,000没収」という商品を提供するなら、その最大額を実際に当該責任へ予約する必要がある。複数件それぞれに10万を保証しながら総元本10万だけとすることは禁止する。

### 7.3 無応答は四条件を維持する

RFC-0026 の (i) accepted 義務と鍵・資源・予約、(ii) 十分な実供給、(iii) 明示的 inclusion 前提の保証範囲内、(iv) 適格応答不在と期限超過、を全て要する。未収録の送信や個別検閲はチェーンから観測できないという限界を変えない。F02 の供給・配送境界・包含前提が未完成の profile では、新しい default slash を有効化しない。

## 8. 自作自演、Sybil、重複報酬への防御

### 8.1 誰が仕事を取っても額と開示権を増減させない

同じ義務 identity は一つの job 群に統合する。作業料の受領 entitlement は既存の割当を尊重し、未割当の job には versioned な機械的割当を定義する。外部の人が開示を取得・対話を継続する権利は、その受領 entitlement の所有・先着とは独立させる。

drawn Panel が放棄した場合、未実施 milestone だけを引き継ぐ。完了済み milestone の再請求はしない。同じ違反の bounty は `offence identity` ごとに一回。既存の reporter commit/reveal 等で正当に確定した受領者を第三者 relay に置換しない。共有開示から複数者が同じ反証を作った場合も、報酬資格の決定を開示経路の owner 独占条件へ逆流させない。

### 8.2 共謀者の合計収支で評価する

Miner、requester、Panel、reporter、relayer、費用拠出者を同一の経済主体が運営する scenario を必須とする。異なる鍵であることを独立した主体の証明としない。

v1 の任意 court は requester の実資金で作業料を前払いし、無制限の一般財源補助を与えない。同一主体がその作業料を自分へ払っても新収入は生まれない。自己告発で実費償還と bounty を自分へ戻しても、`S - 全還流 > 0` の不可還流損失が残るように設定する。旧予約の reporter 報酬、既発生の補助、別leg・複数jobの支払も全て集計する。

この会計上の自己還流防止と、外部市場利得・計算省略を含む不正抑止は別である。実効検出率 `p_effective` は未測定なら UNKNOWN。`p_effective × (失効報酬 + 還流控除後損失)` は評価指標であり、PRINCIPLES §0 に反して p の下限証明や全攻撃の期待利益負を最終 gate にしない。

### 8.3 「作業した」の偽装

認証済みで形式に適合した成果物の存在だけでは、全計算を正しく検査した証明にならない。receipt の資格は既存の承認前提・検証規則・責任へ接続し、裁定 milestone は必要な数値検査を含む。署名だけの架空完了、no-op、同じ body の再包装を支払の根拠にしない。見えない労働を完全に観測できると主張しない。

## 9. 原子的 admission、状態、退出

### 9.1 新規 Miner の参加範囲

新 fence 対象の**報酬対象 producer / Miner claim の新規受理**には `A >= max(A_min,E_A_after)` を要求する。Attempt、Free Prompt、receipt-backed な報酬権、rider / batch 等の元 claim に漏れなく結びつける。同じ元 claim の運搬・再送で二重に10万を要求しない。

通常 tx、wallet の受取、単なる relay、外部告発者、既存の Panel-only 役割、heartbeat には、新しい Miner 参加条件を課さない。fee-only / Kernel 等の非採掘経路は既存責任のまま保ち、採掘 credit へ変換する経路があるならそこで本条件を検査する。非採掘 job からの不正補償の拡張は別 scope とする。

共通 `min_collateral_sompi` は変更しない。新しい account 最低額を Panel の10倍係数、告発予約倍率、モデル seed、既存生産枠へ自動伝播させない。既存の各役割の下限・露出検査は、分離後に実際に使える対応資本で維持する。

### 9.2 original acceptance の原子的処理

1. 旧新 ruleset と claim identity、AccountID、資金 lot、責任 signer を解決し、閉じた要求集合と全支払 envelope を導出する。
2. RFC-0026 の全資源予約・既存 `P` 側の claim 予約・Q/B/R/F・保持・依存 horizon を検査する。
3. 新 `E_A_after`、最低額、既存責任との非重複、通常検証の支払契約と必要な事前原資を検査する。court の実際の requester FeeEscrow は申立て時に資金化するが、その最大 tariff と被告の償還 cap はここで予約する。
4. すべて成功した場合だけ claim、予約、料金表、責任、期限、帰属を一つの状態遷移で記録する。失敗時は一部だけ残さない。

後着の高 fee court、別 root、owner / Sybil、途中の料金表変更で受理済み保証を減らさない。block 開始状態による適格区間、object 順、sweep 順は F02/F03 の新 profile の採択・境界試験に従い、旧 claim に流用しない。

### 9.3 状態の分離

```text
Unregistered / Funding → Active
Active → AdmissionPaused（不足・新規受付停止）
Active / AdmissionPaused → Exiting（新規受理禁止、既存義務を履行）
Exiting → Withdrawable（全必要責任の消滅を確認した範囲だけ）
```

account の不足は既存 claim の計算不正を意味しない。slash 後に `A < A_min` なら新規の報酬対象 claim を停止し、補充で再開できる。既存の応答・開示・court・回復・証拠保管は不足中でも継続できるようにする。新担保不足を理由に正しい応答の受付や relay を拒否しない。

slashing と `L_remaining` の減少は原子的に適用する。予め当該責任を十分に予約していれば、当該 debit 後も他 claim の予約を残せる。想定外にそれが破れる場合は会計破綻として新規受付・退出を停止し、被害や不足を隠す pro-rata な減額・架空 payout をしない。

### 9.4 退出と余剰資本

Active 中は `A - max(A_min,E_A)` の範囲のみを余剰候補とする。ただし退出成熟・transfer hold 等の適用条件を満たす必要がある。全退出では新規受理を先に停止し、claim の終了だけでなく、post-Final 告発、Gate C、bond の別 claim、保持、未払作業料、未償還費用、monetary material の horizon を完了してから必要分を解放する。

hard deadline 到達だけで返還しない。署名済み旧責任の horizon を後から縮めない。鍵変更、分割、退出再登録、複数modelで消費履歴・slash・残存責任を消さない。退出要求を撤回しても枠の回復時計を早めない。

## 10. 終了理由の効果

| 結果 | account 元本 / Panel 支払 | claim・責任・証拠 |
| --- | --- | --- |
| 正常に完了 | 新 account を slash しない。通常報酬は適格条件の後。残存予約は必要 horizon 後に解放 | Final と認証・支払・削除条件を分離 |
| 計算不正が客観的に確定 | 規定償還 + 固定 bounty + 不可還流罰則。予約 cap 内 | 適用規則の void / credit 取消 / 責任継続 |
| 十分な供給と inclusion 前提下で default | 定義された default 用 charge のみ。計算不正と同一視しない | 元 ruleset の義務違反の効果 |
| 正しい claim に対する棄却 | 新 account は無傷。requester が完了分の作業料を負担。未使用 fee は返還 | 既存責任終了条件を満たすまで保持 |
| Panel が放棄 / 最終証拠未提出 | 未実施分に報酬なし。適格完了分はその契約どおり。Minerへ転嫁しない | 未実施jobの引継ぎ。Minerの既履行を無視してslashしない |
| 合意で検出できる供給欠損 | Miner の default slash なし。未提供分は非支払。既実施分は元の FeeEscrow / 正規原資から | Final 保留、元 hard 内で再割当、残余不足は未解決・非 credit |
| hard 到達で未解決 | 到達のみでは没収・報酬成功・返還を発生させない | 担保・証拠・依存責任を保持。共有 frontier 段階1の blocking を残す |
| 偽署名・偽造完了・重複請求 | 適格作業 / supply 消費として数えない。署名者等の別責任を検査 | 無関係な Miner へ責任転嫁しない |

## 11. Miner の資本負担と heartbeat への配慮

### 11.1 守ること

10万の固定部分は一回だけ、追加は残存責任の不足分だけとする。claim 数だけの荒い tier ではなく必要 `L_c` で増やす。正当な重複開示を統合し、標準 tariff と費用上限を固定して予測可能にする。追加担保は任意のトップアップとし、wallet からの無断引落しをしない。

既存 `P` から `A` を取り分けると生産 headroom が減る。新資金として足す場合も参入資本が増える。**本案が Miner を必ず軽くするとは主張しない。** 100,000未満しか用意できない新規 Miner は、本提案の採掘経路へ参加できない。集中・退出への影響を受入判定に含める。

新 account 元本を積んだだけで Q/B/R/F、model の配分 score、rule E participation、検証抽選の stake を増やさない。既存の対応勘定 `P` / verifier 元本と役割を区別する。移行時の全 reader との整合を §14 で確認する。

### 11.2 必要資本と生産量の評価

```text
A_required ≈ max(100,000, λ × L_account_per_claim × H_actual + burst責任)
TotalCapital_required = A_required + P_required + 他の非重複拘束 + fee流動性
```

`λ` と `H_actual` は同じ時間単位で評価する。これは資本計画の近似であり、合意 admission は実際の予約集合の厳密な和で判定する。`H_actual` は Final までだけではなく、新 account が実際に負う post-Final・回復・保持まで含む。旧3,000の拘束と新 `L_c` の拘束を混同しない。

参照 Miner に対して、最低10万で何件同時に保持できるか、何 claim/DAAを維持できるか、いつ・いくら補充が要るか、Panel費を含む純収益が正かを表示する。特定 model を除外して実用性を達成したことにせず、profile 別に達成 / 未達 / UNKNOWN を残す。

### 11.3 heartbeat は非常用のまま残す

[ADR-0140](../adr/0140-the-heartbeat-is-the-emergency-generator.md) と ADR-0176 に従い、heartbeat の妥当性、fee-only の扱い、時計・回復経路を本案では変えない。bond が足りないノードも heartbeat を扱える。heartbeat の観測比率だけを入力として担保要件・責任予約・検証条件を自動で緩めない。攻撃者の claim withholding による担保値操作を作らない。

「heartbeat = 0」を目標にしない。REAL と数える block の既存定義を保ち、riderやただの claim 受付を採用 REAL block の代用にしない。計算完了、抽選当選、claim admission、採用 REAL、認証/Final、支払、担保再利用を別々に測る。

### 11.4 未達時の順序

まず重複計上・一律10万/claim・最低額の二重加算・Panel下限への伝播を直す。次に実仕事の再利用、対話・codec・実装の最適化、同じ保証内の料金・純粋な予算変数を評価する。受理済み義務を削ったり既存 reservation を安く見積もったりしない。

それでも追及原資と参入実用性の両方を満たす領域がなければ、**本案の有効化を保留**する。10万の再検討、通常報酬配分の変更、別の資金源は明示的な再承認事項とし、本 RFC を口実に自動適用しない。旧新 claim が混在する状況での safety-preserving な新規受付停止を、既存責任の免除にしない。

## 12. RFC-0026 F02/F03 との接続

本案の原資保証は、ブロックの bytes、work、IO、byte-DAA、control の実供給を代替しない。original acceptance で両方を AND 検査する。通常申立て `a+3,120` と hard `a+10,000` を維持し、責任 horizon はその先も必要に応じて保持する。各等号は未採択の境界表を推測で埋めない。

同じ DAA の最初の accepting chain block のみを supply とする案、DAA skip の供給ゼロ、専用 quota の再計算は、貼付 F02/F03 案と同じく Proposed。account 入金や支払済み fee を「供給済み」の第三者証明にしない。通常 tx が予約 quota を使った場合の処罰猶予と再割当を維持する。

owner 8枠の外の検証者にも、閉じた要求集合内で開示から裁定まで進める経路を残す。新 Miner Account Bond をその外部者の参加条件にしない。requester feeは実費・濫用コストであり開示権の買占め権ではない。共有 frontier 段階2を前提に段階1の停止リスクを消したと表示しない。

## 13. 合意状態・RPC・保存要件（設計契約、未実装）

最低限、account ごとの元本lot、用途、受理済み責任envelopeと消滅条件、役割別のfree/encumbered、ruleset、料金表version、出金待ち、将来の新規受理可否を復元可能にする。

支払側は `job_id / milestone_id / offence_id / fee funding lot / recipient / tariff cap / earned / matured / paid / reimbursed / burn / remaining` を持ち、複数経路の累積上限と再送の冪等性を確認する。実装は全額を整数 sompi で計算し、丸め規則、checked overflow、0件/0額、最大値を固定する。誤って不足状態を通す saturating 加算は採用しない。

RPC は次を別々に返す: `accountRequired`, `accountLocked`, `accountLiability`, `accountTopUpNeeded`, `productionHeadroom`, `legacyReservation`, `unresolvedLiability`, `nextLegalRelease`, `panelJobFunding`, `admissionReason`。仮の名称であり wire field / proto 番号は未割当。

少なくとも理由を `AccountMinimumMissing`、`AccountLiabilityInsufficient`、`LegacyProductionHeadroomInsufficient`、`CourtCapacityInsufficient`、`ServiceFundingInsufficient`、`QBRFExceeded`、`DependencyHorizonInsufficient` に分ける。これにより「bond不足」のどの要素がREAL停止を引き起こしたかを測定できる。

snapshot / IBD / reorg / undo は元本、charge、FeeEscrow、entitlement、既払・未払・償還、burn、旧新rulesetを一貫して復元する。会計根拠を pruning するのは monetary material の責任が終わってからとする。

## 14. 互換性・最小変更・移行

| 既存文書・経路 | 将来採択された場合に追加する範囲 | 変更しない範囲 |
| --- | --- | --- |
| ADR-0176 / bond formula | 新 account 専用責任、原子admission、役割別資本reader | 既存Q/B/R/F、P側予約式、回復時計、損失有界化 |
| ADR-0124 / Panel economy | 追及費用と成果報酬、jobの明示的料金・帰属 | 共通下限の自動引上げなし。旧receipt報酬・旧claim不変 |
| ADR-0032 / reporter | 新 account に限定した償還/bounty/burnの独立科目 | 旧chargeの10%/49% versionと既存offence identity |
| RFC-0018 / Gate C | 新支払が原資・認証条件を守るよう接続 | 未認証creditゼロ、Finalと認証の分離 |
| RFC-0026 F02/F03 | 原資を予約・供給・共有開示へ接続 | 時計・供給・inclusion・owner非独占・未解決保全 |
| heartbeat / recovery | 新account資格の対象外であることを確認 | validity、clock、REAL定義、fallback |
| ADR-0177 / 配分読取 | 新account専用資本を生産/配分資本から区分する接続 | モデル二階層・既存配分規則・検証者の独立要件 |

本作成では既存正文を置換しない。採択時にこの表の具体節・実装責任者を確定する。新しい ruleset / claim fence は後日の別承認とする。既存 `min_collateral_sompi` の数値変更で実装しない。

旧 claim に新 account への遡及的な連帯保証を追加しない。移行時に既存元本を新 account に移すなら、旧請求・未解決責任に充てた lot を使わない。対象となる新規claimを停止してdrainしても、旧新の応答や復旧は継続する。fork境界直前直後のclaim、同一block、reorg、再起動で責任が増減・消失しないことを検査する。

## 15. 受入試験と有効化 gate

### 15.1 合意・会計・攻撃試験（すべて node-level は未実施）

| ID | 試験 | 必須結果 |
| --- | --- | --- |
| AB01 | 10万未満/等値/超過、責任が10万未満/等値/超過 | `max`式、追加額、Active条件が一致 |
| AB02 | 全claim同時不正、同時default、account固有責任 | promised charge総額が実在元本以下。他claimの予約を奪わない |
| AB03 | original acceptanceの各途中で失敗 | claim・両担保・fee・予約・責任の部分残りなし |
| AB04 | 同一要求、全許容choice、tree全node、別root、owner全枠共謀 | 重複は一回、非重複は全計上。外部者が開示から裁定に到達 |
| AB05 | 自己告発、複数鍵Panel、relay/front-run、複数bounty | 一意なentitlement、不可還流損失、旧新charge非二重払い |
| AB06 | 正常/棄却/不正ゼロ、FeeEscrow不足、ServicePool空 | 正常元本を削らない。無原資支払なし。正当な完了作業を契約どおり支払 |
| AB07 | Panel放棄・永久沈黙・偽署名・架空完了・偽実費 | 別理由として処理。Minerへの無関係な転嫁なし |
| AB08 | 通常tx、有料court、不正codec flood、個別検閲仮定外 | 実供給と受理前DoSを分離。供給欠損でdefault slashしない |
| AB09 | 同一DAA、DAA skip、同一block、期限±1/等値、最遅申立て | 新旧境界と供給会計一致。遅延による期限逃れなし |
| AB10 | slash後のA_min割れ、追加資金、未解決court、hard到達 | 新規停止/再開を分離。応答継続、必要担保/証拠保持 |
| AB11 | 退出、鍵変更、分割、model変更、元本purpose変更 | 旧責任/消費履歴をresetしない。旧資本二重使用なし |
| AB12 | 10万を導入、Panel下限10倍経路、告発枠、heartbeat | 共通下限へ漏れず、非Miner役割を不当に締め出さない |
| AB13 | Final未認証、認証失敗、post-Final反証、複数報酬leg | 未認証issuanceなし、支払/償還/返還/creditの整合 |
| AB14 | MAX整数、丸め、0件、同時payout、再送 | 元本保存、overflow拒否、二重支払なし |
| AB15 | fence混在、snapshot、IBD、restart、deep reorg、pruning | 全writer/reader・undoが同じ責任/原資/支払へ復元 |
| AB16 | 担保不足でREALが止まりheartbeatのみ、その後補充 | heartbeatは常時既存規則どおり。REALへの復帰を実証 |

F02/F03 の T02/T03/T04/T05/T06/T07/T09/T13/T14/T15 と重複部分を対応付けるが、それらが本ZIPに実装済みとは主張しない。オフライン数式検算が AB01/AB02 の一部を確認しても、この表の node-level 合格とは表示しない。

### 15.2 経済・継続性の同時 gate

同一の入力job、model/profile集合、計算能力、ネットワーク条件、参加者構成を用い、(A)旧規則、(B)総資本固定で新accountを取り分ける、(C)新資金を追加して既存Pを維持、を比較する。

| Gate | 測定・承認すること | 本作成の状態 |
| --- | --- | --- |
| G-Collectible | 全同時責任、全分岐、post-Finalを含む原資・予約の不足ゼロ | 数式・shadow部分検証済み / 全branch・node未達 |
| G-Panel | 対象profileごとの純利益、支払待ち、資金立替、p50/p95費用、通常/棄却/成功/回復の報酬 | 旧実測CPU/bytesを再集計、価格感度のみ。金銭的十分性は未達（§18） |
| G-Entry | 参照小規模Minerの総必要資本、最低額での有用な同時件数・持続claim率、追加担保の頻度 | 長期責任・旧P切出しの制約を数値化。同profileのnode実用性は未達（§18） |
| G-REAL | 採用REAL率・最長bond起因停止・heartbeat依存時間・補充後回復が承認許容域内 | UNKNOWN / shadow拒否数や合成admissionをREAL実測に換算しない（§18） |
| G-Concentration | 参入/退出率、資本集中、PanelとMinerの両役割拘束、token価値シナリオ | UNKNOWN |
| G-Compatibility | 旧claim、共通下限、全reader、Gate C、reorg/IBD、返還・pruningに不整合なし | 未実施 |
| G-Court | RFC-0026の供給・inclusion・共有訴追・hard horizonをnode-levelで満たす | 未達 / 既存P0を解除しない |

「十分なPanel利益」「許容heartbeat依存」「参照Minerの実用件数」の数値閾値はレビュー前に固定する。測定結果を見て都合よく閾値を動かさない。閾値未定のままPASSとしない。全てが同時に成立するパラメーター領域が確認できないなら、新規則の有効化は不可とする。最低10万を保って両立できるという経済保証はまだない。

## 16. パラメーター候補と未決事項

| 項目 | 値 / 方針 | 状態 |
| --- | --- | --- |
| `A_min` | 100,000 MSK = 10,000,000,000,000 sompi | ユーザー指定の提案要件。設定変更なし |
| 追加額 | `max(0, max(A_min,E_A) - A)` | 本RFCの推奨構造、未採択 |
| 既存予約 | 適用rulesetの実額。約3,000は説明用 | 変更なし |
| `L_c` / tariff / bounty / burn | scope別に測定・導出。前の段階額・割合は不採用 | §18に局所費用・感度を追加。全branchの責任額と料金承認は未決 |
| 通常Panel報酬 | 現行versionの経路を維持。追加保証のServicePoolは実資金を要する | 配線・採算の検証待ち |
| 初期court fee立替 | requester / 明示的第三者の実資金。返還担保の二重支出なし | 既存feeとの接続待ち |
| 新accountの全stake readerからの区分 | Q/B/R/F・model配分・rule E・抽選への自動加算なし | 実装設計待ち |
| duty/entitlementの重複排除・bounty受領者 | 既存commit/revealを尊重し、新jobへ明示的に接続 | 詳細レビュー待ち |
| 全境界と支払成熟 | acceptance / deadline / payout / release別に定義 | 未決 |
| 経済gate閾値 | 同時測定で採択前に固定 | 未決 |
| 新ruleset / domain / fence / fingerprint | なし | 割当・有効化は別承認 |

## 17. 証拠・レビュー・採択記録

2026-10-20: ユーザーの文書作成依頼に基づき、この RFC と ADR を作成。既存ZIPの正文・実装を読取り、数式例・リンク・差分を検査するための補助資料を同梱した。**文書作成の承認と、設計採択・経済妥当性・実装完了・統合・有効化は別である。**

必要レビュー: 費用導出と外部Panelの採算、Miner資本とREAL継続、全責任の非二重計上、役割別資本reader、shared frontier権利、客観裁定、資金保存、移行・リプレイ。採択者・採択日・有効化日はいずれも未記入。

補助資料: [出典と未確認事項](0027-assets/source-review.md) / [数式・資金フロー検算](0027-assets/economic-examples.md) / [検証記録](0027-assets/validation.md)。


## 18. 2026-10-20 追記 — 実測ログ再集計とAccount Bond負荷実験

**証拠正本:** [追記測定レポート](0027-assets/measurement/measurement-report.md)。既存実測（M_ARCHIVE）、新shadow再生（R_TRACE）、合成実験（S_MODEL）、仮定付き価格導出（D_CONDITIONAL）を分ける。新Account Bondのnode実測や本番十分性を完了したものではない。

### 18.1 実施した範囲

添付されたnode時系列17 run・2,563 claim、外部検証者の21 sampleを再集計した。追加で1,320条件の受付列shadow、326条件の合成資本実験、150行の資本逆算、120行の費用感度を実行した。再集計・会計の1,365検査と解析コード32単体試験はPASS。元の実測は2026-09-26と2026-10-09/10、今回の処理は2026-10-20である。

この環境にRust toolchainと元の大型artifactがなく、新Account Bondの合意実装・支払配線も入力にない。したがってfresh node / 実モデルの追加再実行は未実施。既存生ログ再解析と新しい合成実験を「新規node実測」とは表示しない。

### 18.2 最低額と実用頻度に関する結論

`L_c=12,000`では10万が支えるのは8件。全責任が適法に終了するイベントをH=10,000 DAA後へ置いた楽観的な合成条件でも、新accountだけの継続上限は0.576件/720 DAAだった。目標30件/720 DAAの試験で、旧Pを維持して10万を別途加えても、account不足で提供機会5,000回中4,904回を止めた。これはadmissionモデルの値であって、heartbeat比率ではない。

同条件で目標5 / 30 / 90件を均等発行するなら、必要な同時account責任は70 / 417 / 1,250件。10万だけに収める1件責任上限はそれぞれ1,428.57142857 / 239.80815347 / 80 MSKとなる。1件12,000なら必要Accountは840,000 / 5,004,000 / 15,000,000 MSK。**これらの頻度は感度試験の入力で、実用性の採択済み閾値ではない。**

H=3,120/10,000/20,000は試験入力であり、notice/hard到達だけで解放する新規則ではない。未解決でreleaseなしの試験は最初の8件以降停止した。一般のaccount保有責任が後続horizonまで残る場合、その実際の保持を使って必要額を再計算する。同DAA release先行という楽観境界と、厳しい境界で重複数が1件増える条件も区別した。

### 18.3 Panel費用と財源

Qwen2.5-0.5B referenceの外部検証は、3 positions正常でprocess CPU p50 141.85秒（n=5）、32 positionsのlate違反のローカル裁定まで628.31秒（n=1）。小標本であり、全profileの最大費用ではない。21 sample中4件のgather証拠は272,271,336 bytesとなり、当該wire上限67,108,864 bytesで拒否された。報酬を増やしてもこの提出不能を解消したことにならない。

実費単価は未取得のため、仮定のCPU/転送/保持/運用単価から感度だけを導出した。一つの局所作業に利益を残す料金を置いた例では、32 positionsの部分責任は約43.21 / 396.63 / 3,966.31 MSKと単価仮定で変動した。全合法branchのL_cではない。運用費を含む中間例396.63でも、30件/720 DAA・H=10,000を最低10万で支える条件239.81を超える。逆に一部が収まっても、全分岐・同profile・実支払・参加者採算の合格とはしない。

未没収のAccountは通常Panel収入ではない。正常時のFeeEscrow/既存報酬資金化、実費、支払待ち、同一主体への還流を別に検証する。価格モデルで利益率を付けたことを、実地の利益・参加動機の実証としない。

### 18.4 RFCへの反映

100,000という最低要件と`max(A_min,ΣL_c)`を維持するが、**最低額を「実用頻度を保証する推奨資本額」と表示しない。** 前版の12,000/claimは説明に限り、実用料金候補へ昇格させない。

候補の評価順は、同profileで全合法job・費用・固定bounty・burnを導出し、実際の保持期間と目標頻度から必要Account、旧P、fee流動性を計上する。旧PにAを上乗せする場合と、同じ総資本からAを切り出す場合を別に提示する。同一profileでない旧A16予約例とQwen2.5検証費用を組み合わせて両立達成としない。

G-Panel/G-Entryは部分証拠が増えたが十分性は未達、G-REAL/G-Concentrationは未実測、G-Court/G-Compatibilityは未達。有効化保留と旧claim保護を維持する。heartbeat数値のための担保/検証の自動緩和はしない。
