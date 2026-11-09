# 公開裁定の方針 v0.4 — 公開資料とモデル未保有の一人検証

採択日: 2026-11-09 / 状態: **ADOPTED_DESIGN_NOT_ACTIVATED**。

[RFC-0050](../../../rfc/0050-public-materials-and-one-honest-panel.md)と[ADR-0218](../../../adr/0218-public-materials-and-one-honest-panel.md)の新claim/profileに適用する。通常の規範所有者は[RFC-v2-0016〜0020](../../../rfc-v2/README.md)。[v0.3](public-adjudication-policy-v0.3.md)までの採択記録・旧署名・過去証拠を保存し、今回の資料公開・一人前提・正常完了条件についてだけ補完する。

## 1. モデル未保有の正直者を排除しない

各claim、または固定後抽選の有限batch全体を能動的に完全検証する正直な選出者が少なくとも一人いることを前提とする。モデルの事前保有を選出資格にせず、Minerの推薦や資料取得に成功した者だけへの母集団変更を認めない。能力・通信・費用・期限内の実行可能性は別に必要であり、Nodeが誰を正直と識別できるという前提は置かない。

## 2. 公開する資料と有限な供給

Minerは元claimへ認証できる重み・program・promptとtoken化・数値規則・seed・依存状態・初回trace・出力等を供給する。provider・cache・第三者が同じ認証済みchunkを再配布でき、所有者の個別許可や独占席へ取得を依存させない。全Nodeへ全量配布しない。

適格な資料要求には算術反証を前提としない。caseをnativeに保持し、他者の肯定票では消さない。資料が届いてもInspectionPendingを維持し、同じscope・CaseId・原時計で検査と反証へ進む。URL・保存署名だけで供給や検査完了を認定しない。客観的な供給義務違反、算術不正、サービス障害、未検証終了を区別する。[16](../../../rfc-v2/0016-miner-execution-commit-and-material-supply.md)・[18](../../../rfc-v2/0018-interactive-negative-court-and-defaults.md)。

## 3. 一人の反証と一件の完全承認

全必須関係を検査した選出者の承認を、logicalに **k_min = 1** 件以上必要とする。署名は主張と責任であり、局所positive・資料受領を全体証明へ昇格しない。一件の署名が来ても、正直な選出者のcold取得・全再実行・異議提出の機会を飛ばさない。全員の受領・署名を別名の完了条件にしない。

一人の有効な局所反証は、他の全員の肯定にかかわらず元claimを無効化する。WellFormed・Init・全Step・Endと元Miner初回commitへのbindingを維持し、Nodeは上限付き断片だけを認証・再計算する。[17](../../../rfc-v2/0017-noninteractive-positive-verification.md)・[19](../../../rfc-v2/0019-model-free-node-and-bounded-evidence.md)。

## 4. 成功・保留・有限終了

必須の承認と局所検査、全体検証・異議の機会、Qの解消、依存・資金・期限を[20](../../../rfc-v2/0020-verification-gated-final-and-adversarial-models.md)の単一gateで確認する。反証・Q hold・依存失効を同batchのFinalと全credit consumerより先に反映する。必要資料や検証条件が未成立なら成功させず、原期限で未検証終了にする。

多数の不署名は、それ自体を正常claimの失敗理由にしない。虚偽caseは有界な争点・個別予算・手番・認証terminal・客観defaultで終結させる。無効packetだけで既存caseを閉じず、他人の終了で当人の救済を消さない。

## 5. Cold取得・全合法係争・容量

cold取得、全再実行、局所化、証拠構成・包含・裁定・復元と、正常claimへの最大合法な虚偽caseを元期限内に収容する予約を受理条件にする。受付締切、hard、保持の分離を維持し、資料到着やPanel交代で時計をresetしない。予定された実サービスが供給されなければ、正常完了の前提破れとして成功不可・有限終了へ接続する。

cache、固定後抽選の有限batch、Nodeが上限内で再計算できる区間を使って固定費を共有する。全体coverage、元trace、leaf別の結果・責任・一回会計を失わず、後から仕事を追加しない。初期batchは20 §4.4のatomic方式であり、batch全体の単一Final後に全leafの非重複仕事を計上する。一leafの反証で全体Refuted、部分Final・部分creditなしとし、既受理originalを吸収しない。抽選件数、再実行、配信、保存、Node court、包含を別に測定する。旧54claim／日やEXEC 1 BPSを新profileの実測容量にしない。[05](../../../rfc-v2/0005-bounded-resources-and-long-context.md)。

## 6. 条件付き保証と有効化

少なくとも一人の能動的な正直者、必要な資料または適格要求、完全な局所関係、資格・費用・保護予約・期限内包含・native規則の実施を前提とする。正常完了には正しいMinerの供給・応答と、最大合法な虚偽caseの有限終結も必要である。全員共謀・永久検閲・無条件配送・本人の物理的実行を保証しない。

モデル未保有の唯一正直なPanelで、正常Final、不正反証、不供給、遅延、悪意多数の不署名・虚偽case、cold最大負荷を同じ実装で検査する。設計・抽象モデルを実LLM性能やactivationへ昇格しない。旧75／41・旧署名、二階層、既存経済値、EXEC 1 BPS、一つの鎖、計算暗号proof禁止、延期機能を保持する。

大きな資料の供給・取得後検査を有界なnative根拠で客観終結するprofile別手順は、具体化できていない場合 **PROFILE PROCEDURE NOT ESTABLISHED** として有効化不可である。最低承認数1や公開方針だけでは、この手順の成立を証明しない。実装・実測の未完と区別し、18 §2.4／20 §4.3のgateを維持する。
