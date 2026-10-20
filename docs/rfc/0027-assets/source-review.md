# RFC-0027 / ADR-0193 — 出典照合と未確認事項

**日付:** 2026-10-20。対象は添付された `misakas-pre(4).zip` の内容と、この会話のユーザー指示。外部repository・稼働node・最新remote branchを確認した記録ではない。

- 入力ZIP SHA-256: `8f22b553a054a13ec432f96df4e4aed6928800d449f14fd657bec0f2f5c8b4bc`
- ZIPには `.git` の履歴がないため、このZIP全体のcommit IDを推測しない。
- 本体: [RFC-0027](../0027-miner-account-bond-and-panel-incentives.md) / [ADR-0193](../../adr/0193-miner-account-bond-for-panel-incentives.md)。
- 再現用の出典digestと行範囲: [source-manifest.json](source-manifest.json)。以下の行番号はこのZIPから読み取ったファイルに対応する。

## 1. ユーザー要件

最低100,000 MSKのMiner Account Bondを新設する方向でRFCとADRを作成する。claimの残存責任が増えたら追加bondが必要になる構造とし、Panelの追及に十分なインセンティブを与える。担保増によりclaimが減ってheartbeatに依存する問題も考慮する。

既存claim予約bondの主目的はclaimにコストを作ること、新Account Bondの主目的はPanelのインセンティブである。この目的の指定は、既存予約が持つ損失担保・返還・資源予約の意味を撤回する指示とは扱わない。

## 2. 会話のF02/F03案の扱い

ユーザーが先に貼付した詳細案はProposed / Review Requiredであり、ADR-0192 A/Aの詳細化と説明されている。ZIP内に対応するADR-0192本体と詳細案ファイルは確認できないため、リンク先本文を読んだと表示せず、実装・採択・有効化を仮定しない。

RFC-0027に引き継ぐ要件は、original acceptanceの原子性、閉じた要求集合、全分岐の最悪計上、ownerに依存しない義務identity、認証済み開示の共有、実供給とinclusion前提、同一DAA/skip/同一block境界、期限超過と供給欠損の区別、hard後の未解決責任保持である。数値、境界等号、配送前提、node達成は未決のままとする。

ADR番号は会話の0192を再利用せず0193を仮割当し、RFC番号はZIP内0026の次の0027とした。他branchの予約・競合は統合前レビュー事項である。

## 3. 読み取った根拠

| ファイル | 関連行（このZIP） | 照合する論点 |
| --- | --- | --- |
| [docs/PRINCIPLES.md](../../PRINCIPLES.md) | L38–L44, L81–L87, L224–L230, L243–L249 | 最終Goal、検出確率は評価指標、客観裁定、モデル検証の禁止範囲 |
| [docs/governance/design-freeze.md](../../governance/design-freeze.md) | L20–L26, L26–L32 | 重大変更は新RFC・ADRと明示採択、実装と有効化の分離 |
| [docs/adr/0174-token-name-misaka-ticker-bili-address-prefix-unchanged.md](../../adr/0174-token-name-misaka-ticker-bili-address-prefix-unchanged.md) | L21–L27, L33–L39 | MSKは旧ラベル、BILIの同一coin、1e8 sompi |
| [docs/adr/0176-bond-bounds-palw-production-rewards-and-final-weight.md](../../adr/0176-bond-bounds-palw-production-rewards-and-final-weight.md) | L77–L83, L88–L94, L176–L182, L184–L190, L189–L195 | 共通回復と担保解放、実用性、heartbeat、予約例と利用率 |
| [docs/spec/palw/bond-budgets.md](../../spec/palw/bond-budgets.md) | L5–L11, L7–L13 | Q/B/R/Fと実際の責任保持、Panel=0/1の区別 |
| [docs/adr/0124-the-panel-is-paid-out-of-the-claims-reward-a-seat-holds-exposure-and-a-claim-is-paid-for-the-compute-it-certifies.md](../../adr/0124-the-panel-is-paid-out-of-the-claims-reward-a-seat-holds-exposure-and-a-claim-is-paid-for-the-compute-it-certifies.md) | L190–L196, L245–L251 | 既存Panel報酬とPanel側の下限・責任 |
| [consensus/core/src/palw_panel_economy_v1.rs](../../../consensus/core/src/palw_panel_economy_v1.rs) | L56–L62, L67–L73, L70–L76, L125–L131 | 20%の既存定数と10倍floor、露出によるheadroom |
| [consensus/core/src/palw_state_v2.rs](../../../consensus/core/src/palw_state_v2.rs) | L1168–L1174, L1174–L1180, L2319–L2325 | 旧10%/後続49%の区別、generic bond gate |
| [docs/rfc/0018-dag-parallel-multi-model-consensus.md](../0018-dag-parallel-multi-model-consensus.md) | L191–L197, L202–L208, L353–L359 | FinalとSecurityCertified、未認証credit、Panel leg接続の未決 |
| [docs/rfc/0026-court-policy-master-contract.md](../0026-court-policy-master-contract.md) | L28–L34, L58–L64, L66–L72, L102–L108 | 3120/10000、horizon、F02無応答、F03外部追及のP0 |
| [docs/adr/0140-the-heartbeat-is-the-emergency-generator.md](../../adr/0140-the-heartbeat-is-the-emergency-generator.md) | L48–L54, L67–L73 | heartbeatはbondlessなliveness経路、妥当性とlocal policyの区別 |
| [docs/design/palw/economics/adr-0176-cap-derivation-and-measurement-plan-2026-10-15.md](../../design/palw/economics/adr-0176-cap-derivation-and-measurement-plan-2026-10-15.md) | L1–L7 | 既存の資本・能力・REAL継続の計測計画 |

## 4. 推測しないこと

約3,000 MSKを全claim種別の定数にしない。13,000/130,000や20%、49%という記録を全networkの現在有効値としない。原文のStatusと後続改定を保持し、新文書の支払規則の採用理由に「既に稼働済みだから」を使わない。

基礎10万と新責任が経済的に適切か、Panelが何人参加するか、token建てtariffが実費を十分に補うか、heartbeat率がどう変わるかは未測定。出典中の合成quote・旧テスト・休眠実装を本案の経済/実装合格に流用しない。

## 5. 検証記録

[validation](validation.md) はこの作成時の文書・数式確認を区別して記録する。入力ZIPに存在した参照欠損を隠すbaseline更新、元資料の数値・ログ・digestの改変は行わない。
