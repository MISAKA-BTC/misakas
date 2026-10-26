# ADR-0206: PanelDa の G14 経路 — 期限付きの必須公開

- Status: **accepted**(2026-10-26 ユーザー決定: 期限付き必須公開と「期限まで非公開」への縮小を採用。値は目安のまま。fence は全 preset で `None`)
- Date: 2026-10-26
- Lane: g14-fam-panelda(branch `claude/g14-fam-panelda`)
- 関連: ADR-0077 D16 / SA-5、ADR-0119 D7、ADR-0192(claim 共通 frontier)、ADR-0198(notice a+3,120 / hard a+10,000)、2026-10-17 決定(court 保持 ≥ 4W+開示窓)、PRINCIPLES §8・§9
- ユーザー決定 2026-10-26:「PanelDa の実装は行なって 画像は実装しないけど」— FP V5(画像入力)には経路を作らない。

## 問題

PanelDa(privacy mode 2)はプロンプトの ids を commitment tx に載せず、抽選 seat だけが material で受け取り、紛争時にだけ公開される。G14 の不変条件(producer と Panel 全員が共謀しても、Panel 外の public bonded verifier 1 人が公開認証 material だけで客観的に conviction し、期限内に slash まで届く)に対し、全 owner が共謀すると誰も紛争を起こさず、外部者は ids を得られない。よって現行表では V5 と同じ `NonPublicEvidence`(経路なし)で、G-01 (b) により報酬・weight・credit なし。

## 比較した候補

| 案 | 内容 | 判定 |
|---|---|---|
| A. 期限付き必須公開 | commit 時に `prompt_token_ids_hash` で束縛、service 中は非公開、受理 a から a+1,040 までに ids を公開 material として開示。未開示は default(deposit 没収・claim void、計算の有罪にしない)。開示後は PublicDa と同じ ③〜⑥ | **採用** |
| B. 外部者による開示請求(請求があれば開示) | 外部者は請求すべき claim を知るのに material が要らないが、請求の bond・期限・応答遅延の窓が増え、全 claim に請求を出す Sybil 費用を外部者が負う | 不採用(外部者 1 人で足りる保証が費用依存になる) |
| C. 抽選された外部 seat への秘匿共有 | 「外部」seat も抽選された保持者で、共謀集合に入りうる。結局公開しない限り外部者 1 人の保証にならない | 不採用 |
| D. 暗号化して公開し後で鍵公開 | A と同じ期限問題に帰着し、鍵の不開示=A の default。構造が増えるだけ | 不採用(A に含まれる) |
| E. 計算の暗号学的証明で非公開のまま検証 | PRINCIPLES §8 で恒久禁止 | 対象外 |

A を選ぶ理由: 外部者が必要とするのは「hash に束縛された ids」だけで、期限で公開を強制すれば共謀の有無に関係なく material が公開側に出る。取得不能は default として経済的に処理し(計算誤りの有罪にしない)、開示後の告発経路は既存の公開 FP family をそのまま使うので新しい裁定規則が要らない。

## 決定(案)

1. **束縛**: job の `prompt_token_ids_hash`(class の prompt-ids form、Flat / MerkleV1)が ids を固定する。同一性のための hash commitment であり、計算の正しさの証明ではない。
2. **非公開の期間**: 受理 `a` から最大 `a + 1,040` DAA(目安 `PALW_G14_PANEL_DA_DISCLOSE_BY_DAA_V1`)。その間 ids を見るのは抽選 seat だけ。**confidentiality は約束しない**(seat は見る、期限で必ず公開される)。
3. **開示義務**: 期限(含む)までに ids を公開 material として載せる。誰が載せてもよい(hash が中身を固定するので、filer の身元は関係ない)。責任者は producer。上限 32,768 ids(目安)。
4. **外部者の窓**: 期限後も notice 窓 `3,120 − 1,040 = 2,080` DAA が残る。武装条件として `DISCLOSE_BY + OUTSIDER_MIN(2,000) ≤ 3,120` を validation で検査。hard deadline・延長は `palw_court_clock_v2`(a+3,120 / a+10,000)に従う。
5. **保留**: 開示が fold されるまで報酬・weight・settlement credit を出さない(CERT の保留と同じ扱い:未開示の claim は Final 決済を進めない)。
6. **default**: 期限までに開示がなければ claim void、報酬なし、開示 deposit(目安 13,107,200 sompi = 最大開示の運搬費の 2 倍)を没収(burn)。**計算の有罪・計算 bond の slash にはしない**(取得不能を計算誤りの有罪にしない)。期限後の開示で復活しない。
7. **開示後**: claim は PublicDa と同形の公開 FP family(FpSelection・FpConstraint・FpPrefix・FpTokenizer、③〜⑥)で外部者が告発でき、有罪なら既存経路の slash。
8. **G14 表**: 新 family `FpPanelDaDisclosure`(enum に追加、既存番号不変、serialize/hash されない)。fence `palw_panel_da_disclosure_v1` 以降に受理された PanelDa の**テキスト** job は `[FpPanelDaDisclosure, FpSelection, (FpConstraint), (FpPrefix), FpTokenizer]`。fence 未満と V5 は従来どおり `NonPublicEvidence`(byte 同一)。
9. **武装に要る fence**: `palw_panel_da`、`palw_court_clock_v2`、`palw_panel_da_disclosure_v1`、`palw_g14_route_required_v1`(+開示後の公開 FP family 自身の fence)。record は空(pin は統括)。
10. モデル二階層・claim 単位の抽選承認(Gate C)は変えない。

## 利用者への開示の文面(fence が governs する network で、初回利用前に gateway が表示)

> Private only for a while: the verifying seat sees this prompt during the work, and the prompt is published on chain within about 1,040 DAA of acceptance whether or not anyone disputes it. This is not confidentiality. If it is not published in time the claim is void and its disclosure deposit is forfeited.

(`PALW_FP_PANEL_DA_MANDATORY_DISCLOSURE_V1`。fence 未満では従来の `PALW_FP_PANEL_DA_DISCLOSURE_V1`。)

## 費用・保持

- 開示の運搬: 最大 32,768 × 4 B = 131,072 B、t12 料率 50 sompi/B で 6,553,600 sompi。deposit はその 2 倍。
- 保持: 開示 material は公開 material として court class の保持(≥ 4W+開示窓、2026-10-17 決定)と clock v2 の `a + 10,000 + W_disclose` に従う。新しい保持規則は足さない。

## 結果

- 正: PanelDa テキストが G14 経路を持ち、全員共謀でも期限で material が公開される。
- 負: プロンプトは「期限まで非公開」に縮む(紛争がなくても公開)。利用者に明示する。
- 未了(本 ADR の武装前提): 開示 carrier(tx object・version 付き)、fold row、deposit lock、Final 保留の fold 配線、実ノード e2e。これらが入るまで validation は武装を拒否する。

## 値(目安、`palw_g14_values_v1`、台帳「2026-10-26 lane PANELDA」)

| 値 | 目安 | 根拠 | 見直し条件 |
|---|---|---|---|
| DISCLOSE_BY | 1,040 DAA | notice 窓の 1/3 | notice 窓・turn 期限・外部者 replay 実測 |
| OUTSIDER_MIN | 2,000 DAA | 包含遅延+最大 class replay+turn の 2 倍超 | 外部者 replay 実測 |
| MAX_DISCLOSED_IDS | 32,768 | t12 最大 8K class に余裕 | 長 context class |
| DEPOSIT | 13,107,200 sompi | 最大開示の運搬費の 2 倍 | 料率・最大 ids |

## 実装記録(2026-10-27、lane g14-fam-panelda)

- carrier tag 233(割当は統括の確認要)、row delta 236 / tail 0xE8 / root `panel_da_disclosures/v1`、void reason 140。
- 価格: derived-work 下で governed PanelDa は prompt 全体を既払い扱い(prefill 0)。過少払いのみで二重払いはない。answer accounting 武装時は従来どおり拒否。**ユーザー確認待ち**。
- validation は実ノード E2E が済むまで武装を拒否する。
