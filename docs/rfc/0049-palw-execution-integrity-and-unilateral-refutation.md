# RFC-0049 — PALW実行整合性と一人による反証の完結契約

Status: **Accepted design / ADOPTED_DESIGN_NOT_ACTIVATED**
Date: 2026-11-09
Decision: [ADR-0217](../adr/0217-palw-execution-integrity-and-unilateral-refutation.md)
Current owner: [RFC-v2-0020](../rfc-v2/0020-verification-gated-final-and-adversarial-models.md)

本書は、ユーザーが指定した「一人の正直な検証者が有効な反証を提出すれば、多数のPanelの承認にかかわらず誤ったclaimを無効化する」「必要な検証承認が成立しなければ成功させない」という新claim版の追加採択記録である。現行の通常編集先はRFC-v2の20冊を維持し、本書を21冊目の独立した規範所有者にしない。番号0049は今回添付内で未使用だった範囲の割当であり、未取得branchの予約を保証しない。

## 1. 解決する重大な問題

従来の新profileには、個別の適格異議と多数の肯定承認が競合した場合の優先順序、同一block内の異議とFinal、全計算関係を局所反証できる条件、攻撃者の予約占有から一人の提出機会を守る条件の定義不足がある。署名数や検査箇所数だけの調整では、この空白を閉じられない。

これは現行runtimeに対する再現済み実装脆弱性の認定ではなく、添付設計の保証を成立させるための規範上の不足である。反証があるのにcreditを出す経路、資料不足を成功と扱う経路、局所passを全体証明に昇格する経路を除く。

## 2. 「計算証明」の採択した意味

保証対象は、登録済みモデル、program、入力、実行規則、seed、依存状態について、Minerが署名したclaimとcanonical executionの関係が整合することである。Miner本人または各Panel本人が特定GPUで新規に全演算を実行した物理的行為、費やした実時間、非外注・非共有は証明対象に含まない。

正常時の全体承認は所定の独立検証の署名付き承認であり、Nodeが全計算を数学的に直接検査した証拠ではない。一方、計算反証は元claimに束縛した小さな資料をNode自身が決定論的に再計算する客観的な裁定である。全検証者の共謀、永久検閲、資料の永久秘匿までを排除する無条件の全体正当性は主張しない。[PRINCIPLES §13](../PRINCIPLES.md#model-free-adversarial-verification)。

## 3. 採択する最小変更と規範所有者

| 変更 | 規範所有者 |
| --- | --- |
| Minerが初期状態・全状態遷移・終端と結果の対応を含む実行記録を先に固定する | [16](../rfc-v2/0016-miner-execution-commit-and-material-supply.md) |
| データ、演算規則、座標、依存、数値意味を認証し、有界なnative検査へ落とす | [19](../rfc-v2/0019-model-free-node-and-bounded-evidence.md) |
| 肯定承認の検証範囲と保証を限定する。署名や抜き取りを全体証明へ昇格しない | [17](../rfc-v2/0017-noninteractive-positive-verification.md) |
| 一人の適格noticeをnative未解決状態へ入れ、完成反証を他のPanelやMinerの許可なしに裁く | [18](../rfc-v2/0018-interactive-negative-court-and-defaults.md) |
| 必要承認・未解決異議・裁定結果・期限・依存・経済効果を一つの成功gateで判定する | [20](../rfc-v2/0020-verification-gated-final-and-adversarial-models.md) |
| 一人の検証者の合法経路を攻撃者の予約占有から保護し、総負荷とcold取得を予算化する | [05](../rfc-v2/0005-bounded-resources-and-long-context.md)・18・19・20 |
| 取消を仕事報酬・未消費credit・permit・依存へ伝播し、既存取引の一括rollbackとは分ける | [07](../rfc-v2/0007-claims-exec-and-answer-accounting.md)・[08](../rfc-v2/0008-claim-dependencies-and-liability.md)・[10](../rfc-v2/0010-dag-consensus-and-native-settlement.md) |

不正な実行に必ず裁ける矛盾が存在するという命題は、初期状態・遷移・終端の関係集合が完全で、各関係に認証付きの有界なwitnessがあり、その取得と提出が可能である受理profileについてのみ成立する。巨大な演算を名称だけで一primitiveと呼ばず、profileの全relation familyを具体化する。

## 4. 一人の反証が勝つ条件

他のPanel全員が肯定していても、一人の適格な未解決異議を肯定票で消してはならない。一人の提出した反証がnative検査で成立すれば、肯定署名数に関係なく元claimを無効化する。単なる私的な「届かなかった」、root不一致、未認証資料は算術有罪にしない。

同一blockの後方に有効な異議・反証があるのに、前方のFinalで仕事報酬・creditを外へ出す順序を認めない。block内の候補を認証・裁定し、claimごとの未解決状態・失効を確定してから成功と経済効果を反映する。詳細は18・20が所有する。

一人の経路に必要な資格、bond、費用、資料取得、予約された搬送・裁定容量、期限内包含、canonical stateへの到達を明示する。これらが破れた場合を「一人いれば無条件に必ず無効化」と呼ばない。所定の成功条件が確認できない場合は、時計をresetせず未検証終了へ進める。

## 5. 互換性・不変条件

新署名版・新profileだけに適用する。旧署名済みclaim、旧Positive Court R2、旧Gate CのFinal後認証、既存wire ID・fence・activation heightを読み替えない。

§8の暗号学的計算証明の禁止、§9のモデル二階層、モデル登録時審査の不採用、画像・動画・モデル改善の延期、一つの鎖を維持する。Nodeへ全モデルfetch・GPU・全推論のfallbackを追加しない。EXECの1 BPS目標、1,000 ms slot、global slot当たり最大1正規block、既存経済パラメーターを変更しない。

## 6. 検査・失敗時対応

反証可能性の関係網羅、元claimとの束縛、偽の反証、少数異議、多数の偽承認、同block順序、資料不足、予約枯渇、deadline、reorg、依存取消、credit一回性を別々に検査する。対応する規範、反例、修正、再レビュー結果を[今回の改訂](../maintenance/revisions/2026-11-09-palw-execution-integrity/README.md)に記録する。

文書検査や抽象モデルの探索を、runtimeの実装完成・実モデル測定・全ネットワーク安全性の証明に転用しない。未成立profileは受理・activationしない。既受理claimの失敗は、その版の原時計、未検証終了、残存責任、保持と返還の規則で処理する。

## 7. 適用と旧採択の置換範囲

[ADR-0215](../adr/0215-model-free-node-noninteractive-positive-and-adversarial-availability.md)のNode非保有・資料供給・検証先行Finalを維持し、今回新profileについてのみ、反証完全性・一人の異議の優先・同block順序・容量隔離・成功gateを16〜20の改訂で補う。[ADR-0216](../adr/0216-exec-credit-schedule-one-bps-and-court-capacity.md)のEXEC設計と容量契約は維持する。

新方針は[公開裁定v0.3](../design/palw/consensus/public-adjudication-policy-v0.3.md)。旧方針v0.1/v0.2、旧採択記録、過去検査ログ・hashをそのまま保ち、番号や日付だけで優先を決めない。
