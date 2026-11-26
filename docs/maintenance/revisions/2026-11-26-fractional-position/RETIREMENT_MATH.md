# モデル廃止とCourt個別slash — 抽象lot参照検査

2026-11-26（Asia/Tokyo）。[参照実装](retirement_reference.py)、[検査](test_retirement_reference.py)、[結果](retirement-check.json)は、用途別資産、廃止burn、別のCourt回収と49%発見報酬を扱う。BMR1の正本は [RFC0069](../../../rfc/0069-bft-model-retirement-and-role-bonds.md)／[ADR0237](../../../adr/0237-bft-model-retirement-and-role-bonds.md)。通常coinbase報酬の文脈は [RFC0068](../../../rfc/0068-fractional-position-miner-rewards.md)／[ADR0236](../../../adr/0236-fractional-position-miner-rewards.md)を参照する。署名、quorum、有罪立証、native権限の実装ではない。

## 1. アドレスではなく用途lot

lotはmodel ID、ownerの責任identity、用途、asset、原資source IDと重ならない原子数区間を持つ。同じ人の同じ10,000 MISAKAから、[0,6000)をmining、[6000,10000)をPanelに分ける。一つのsource区間を複数lot・別modelへ再使用する登録は拒否する。burn後も使用履歴を保持し、別名のlotで資産を復活させない。

廃止caseを固定すると、対象modelのlotの用途・asset・原資identity・原数量も固定する。miningからPanelへの付け替え、Panelからminingへの巻込み、対象lot追加を拒否する。正規Courtによる現在残高の減少は用途変更ではなく、別権限で処理できる。

## 2. BFT永久廃止

同じcase/modelに結合された正規BFT判断を明示入力とし、対象modelだけを処理する。

| 対象 | 処理 |
| --- | --- |
| current real pool MISAKA | 現在残高をburn |
| 登録者責任bond、mining bond | 対象用途lotの現残高をburn |
| 未発行・未確定mining報酬 | entitlement取消。mintしてburnしない |
| MISAKAの未確定mining escrow | 実在escrowをburn |
| POSITIONの未確定mining escrow | POSITION retire。通貨換算して重複burnしない |
| Panel / Gate C bond | BFT一括burnから保護 |
| 成立済み正当な検証・BFT報酬 | 支払権を保持 |
| 他modelのlot、既得の正当なsettled報酬 | 不変 |

virtual reserve、過去LP累計、receipt額をcurrent real cashへ足さない。試験のreal=200、過去LP=1,000、virtual=10,000 MISAKAでもpool burnは200だけ。同じ人のmining6,000 / Panel4,000なら6,000だけをburnし4,000を保護する。

検証者の疑い・善悪をBFT一括処理で判定しない。不正な検証者にも用途保護を適用し、本人の個別違反は別のCourtで裁く。廃止処分金からBFT、発見者、通報者へ賞金・払戻しを出さない。coinbaseのBFT5%は別枠である。

## 3. 原子性とnative資産保存

lot更新、pool減額、POSITION retire、未発行取消、廃止状態、case一回消費、receiptを同時commitする。lot準備後・receipt準備後の失敗は前状態を保つ。case再送・同じmodelの再廃止を拒否する。局所undoは現在状態と一致する最後の遷移のみで、replayは同じ結果となる。

実在source合計について、残資産、実burn、Court専用escrow・支払済み報酬を含めて保存を検査する。未発行取消量は実通貨の式へ入れない。

## 4. Court実回収の49%報酬・51%burn

Court有罪・実回収はBFT廃止とは別。offenceごとの責任identity、対象lot、回収上限、原期限を固定し、実際に回収したMISAKAだけをSへ足す。未回収請求額、未発行取消、virtual、BFT pool burnはSへ混ぜない。

```text
Q_e = floor(490*S_e/1000)       S_eは同じoffenceの累積実回収
delta_Q = Q_e(new)-Q_e(old)
delta_burn = delta_S-delta_Q
```

Qは正規の発見・立証Panel/Gate Cの**合計枠**。各roleが49%ずつ取らない。1 atom×100回でも100 atoms一括でも、総Q49・総基本burn51。各小額回収へ別々にfloorして報酬を0にしない。

回収は一意collection IDで記録し再送を拒否する。同じoffenceの対象lot、責任identity、上限、期限を途中で差し替えない。別の正規処理で既に減ったcollateralを二重回収しない。

## 5. 回収と受給の分離

`court_collect`は正規有罪と実回収に基づき、collateral減額、基本burn、delta_Q専用escrowを確定する。recipient欠落、不正recipient、未解決planを理由に有罪・回収を取り消さない。

`bind_court_plan`は期限内の正規資格解決という別の明示入力。許可roleはPanel/Gate Cだけで、被告と同じ責任identityは受給できない。解決済みplanの差替えを拒否する。未解決ならQは未受給専用escrowに残す。

`claim_court_reward`はcollectionごとのdelta_Qだけを一回配る。参照planは明示入力の受取順と整数weightを持ち、各人へ残額×weight/残weightのfloor、最後の人へ残額を配る。半々や受取先を自動生成しない。この配分器は参照用で、nativeの正規役務・発見資格・受取順を新たに採択したものではない。既払額を新しい累積Qから再Hamilton配分せず、過去の支払を逆行させない。

原期限を過ぎた未受給分は`expire_court_rewards`でburnする。期限resetはしない。期限後の追加実回収も免責せず、期限切れdelta_Qはburnへ回る。この場合、51%基本burnに失効分が加わる。

recipient資格、責任identityのalias照合、Court証拠・署名・quorum・異議期限はnative統合で検証する。参照の`verified=True`は検証済み結果の抽象入力であり、証明書ではない。

## 6. BFTとCourtの順序

Courtが100 atomsから40を先に回収すると基本burn21、報酬escrow19、collateral残60。続くBFT廃止は60だけをburnし、19を保護する。原期限内の正規受給は廃止後も可能。

BFTが100を先にburnした同じ原資の後続Court回収は0、報酬も0。追加1 atomの回収要求は拒否し、burn済み額をSへ再使用しない。これは有罪の免除ではなく、同じ資産の二重支出防止である。

Gate C検証者のbond2,000 MISAKAはBFT一括処理から保護する。本人の個別違反がCourtで確定し500を実回収した試験では、別の正規検証者に245、基本burn255、本人の残bond1,500となる。

## 7. 実行結果と限界

```sh
PYTHONDONTWRITEBYTECODE=1 python3 docs/maintenance/revisions/2026-11-26-fractional-position/check_retirement.py
```

20 unittestで、用途lot、原資重複・重なり、current poolのみ、未発行/escrow区別、既得報酬、Panel/Gate C保護、個別Court、case固定、原子性、再送、undo、累積49/51、受給遅延・失効、被告受給拒否、scope/plan固定、u128上限、BFT/Court順序を検査した。

対象は抽象lot ledgerと単一collateral lotのCourt offence。実際の複数lot回収、署名/BFT/Court、DAA時刻、異議申立て、永続化、DAG、EVM、pruning/IBD、支払・公開有効化は未検証。source IDと区間は認証済みnative assetの代用表現であり、その実在証明ではない。
