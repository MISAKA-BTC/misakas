# RFC-0050 — 公開資料とモデル未保有の一人検証

後続採択: [RFC-0053](../rfc/0053-additional-model-burn-pause.md)／[ADR-0221](../adr/0221-additional-model-burn-pause.md)によりHF取得可否認定を廃止し、追加モデルだけをburn停止・解除の対象とする。genesis・年次fork指定は対象外。claim固有資料・TKR1・不正反証・原期限は維持する。

Status: **Accepted design / ADOPTED_DESIGN_NOT_ACTIVATED**
Date: 2026-11-09
Decision: [ADR-0218](../adr/0218-public-materials-and-one-honest-panel.md)
Current owner: [RFC-v2-0020](../rfc-v2/0020-verification-gated-final-and-adversarial-models.md)

本書は、ユーザーが提示しdocsへの反映を指示した「独立選出されたPanelに少なくとも一人の能動的な正直者がいる。その人がモデルを事前保有しなくても資料を取得し全体を検証できる」という追加採択の記録である。通常の規範所有者は現行RFC20冊を維持する。0050は今回利用できた資料内で未使用の番号であり、未取得branchの予約を保証しない。

## 1. 解決する問題

モデル保有者だけを選出する、資料を取得できた者だけをPanelとして数え直す、取得不能の異議に最初から算術反証を要求する構成では、資料を隠す者が検証参加と報酬を独占できる。また、唯一正直な選出者以外が全員不署名の場合、旧75／41の肯定閾値では正常claimの完了を保証できない。

今回の変更は、モデル非保有Nodeと一人の局所反証を維持しながら、資料取得の権利、有限の供給義務、正常claimの条件付き完了を接続する。これはruntimeの再現済み脆弱性や、数万モデルの実測成立を認定する記録ではない。

## 2. 採択する前提と保証

対象は、固定されたモデル・入力・実行規則に従う実行整合性である。各claim、または固定後に抽選する有限batch全体を、少なくとも一人の正直な選出者が能動的に完全検証することを前提にする。その人はモデルを事前保有する必要がないが、受理profileの取得・計算・認証・証拠構成を遂行できる必要がある。

Nodeは誰が正直かを署名から識別できない。Panelの一部だけへ任意に仕事を渡したり、各人へ別layerだけを割り当てたりして、一人の全体coverageを失ってはならない。Panel人数の削減は今回採択しない。縮小後や分割後にも一人の正直者がいるという保証なしに、既存前提を移し替えない。

誤claimは一人の有効なnative反証で無効化する。必要資料を得られない適格caseは成功を止め、算術反証の有無にかかわらず、規定期限までに検証条件が成立しなければ未検証終了とする。正常claimは、Minerの供給・応答、唯一正直な選出者の完全検証、最大合法な虚偽caseの有限終結、予約された実サービスと包含が成立する条件の下で、他者の不署名によって停止しない。

## 3. 公開検証パッケージと供給義務

2026-11-11限定置換: 新HF profileのモデル静的資料は[RFC-0052](./0052-hugging-face-model-acquisition-and-claim-materials.md)／[ADR-0220](../adr/0220-hugging-face-model-acquisition-and-claim-materials.md)の必須HF公開契約・固定版匿名取得・全file/native root照合を用いる。本節をモデル重みのMISAKA独自全量再配布の必須要件として解釈しない。claim固有資料、取得case、有限保持と元時計は維持する。旧profileは遡及変更しない。

公開対象はpromptだけでなく、重み・構造・program、tokenizer／前処理／canonical token列、数値型・量子化・丸め・overflow・演算順序、seed／乱数規則、必要な初期KV・依存状態、Miner初回traceとopening、出力・停止・長さ・仕事会計である。[16](../rfc-v2/0016-miner-execution-commit-and-material-supply.md)がcanonicalな内容と範囲を所有する。

公開は全Nodeへの全量配布ではない。内容rootで認証できるchunkをMiner、独立provider、cache、再配布者から取得でき、モデル所有者の個別許可や特定keeperの席を利用条件にしない。cacheがない正直な選出者のcold取得を資源・期限へ含める。root・URL・保存署名だけを実取得完了にしない。

Minerは適格な第三者にも同じ資料への有限な取得経路を負う。資料取得要求には算術反証を前提とせず、対象claim・manifest範囲・義務者・要求者・期限・個別予算をnative状態へ結ぶ。供給履行後もInspectionPendingを保ち、同じCaseId・scope・原時計で検査と反証へ継続する。供給拒否で成功や報酬を得る経路を認めない。モデル登録時の審査・tier変更を追加する規則ではない。

## 4. 最低一件の完全検証承認

今回採択する新profileのlogicalな最低肯定条件を **k_min = 1** とする。これは選出された有資格な検証者による、元claimの全必須関係とcanonical traceを確認した承認である。Miner自身の申告、未選出者の署名、局所positive、資料受領だけを数えない。

一件の承認直後に成功へ進めない。[17](../rfc-v2/0017-noninteractive-positive-verification.md)の承認、必須局所検査、全体検証と異議提出の機会、Qの解消、依存・資金・原期限を、[20](../rfc-v2/0020-verification-gated-final-and-adversarial-models.md)の単一gateで確認する。最初の承認者の正直さを推定せず、前提上存在する正直な一人の経路を最後まで保護する。全員の受領・完了承認を別名で必要条件へ戻してはならない。

旧claimと旧Gate Cの75／41、4,096抽選位置／日、旧署名・既存runtime設定は維持する。新profileのk_minだけを今回の意味変更として採択し、m、帯域、局所区間B、予約容量等の未測定値を作らない。

## 5. 虚偽の異議による停止と容量

一人の適格caseは未解決の間成功を止めるが、無条件の永久vetoではない。[18](../rfc-v2/0018-interactive-negative-court-and-defaults.md)の有界な争点、手番、元scope、累積予算、認証されたterminal、客観的defaultで終了する。無効packetの拒否で既存caseを消さず、他の申立人の終了で正直な人のcaseを消さない。

正常claimの完了を主張するprofileは、最遅の適格noticeから最大の合法な虚偽caseを終結する時間・供給・検査・包含・裁定を、元hardまでに予約する。他人の悪意を単に無視する、全caseを期限で無条件に消す、Nodeが全モデルを再計算する、といった救済を使わない。実サービス不足では成功不可を維持し、条件付き完了の前提破れとして記録する。

新しい容量policyは抽選・署名、Panel全再実行、cold配信、trace保持、NodeのCPU／hash／parse／証拠bytes、包含を分離し、全共有資源と期限区間で検査する。旧54claim／日の設定を流用して数万モデルの継続稼働が成立したと扱わない。[05](../rfc-v2/0005-bounded-resources-and-long-context.md)が資源契約を所有する。

大きな公開資料の虚偽未着申告と、正直なcold検証者への選択的秘匿を、小さなnative資料で客観的に区別・終結できる具体的なprofile手順が未確立の場合は、**PROFILE PROCEDURE NOT ESTABLISHED**とする。URL、他人のreceipt、少数chunkの応答、quota消尽から全scopeの検査前提を推定せず、その人をdefaultにして成功へ進めない。18 §2.4と20 §4.3に従い、対応が確立するまで当該profileを有効化しない。これは実装・性能測定だけとは別の未完gateであり、本採択で解決済みとはしない。

## 6. 固定費の共有と小さなcourt

同じ内容のcache・chunk再配布、固定された有限batchの抽選・署名搬送・モデルロード共有を認める。batchは全leafとmembershipを先に固定し、その後の乱数でPanelを選ぶ。後から仕事を追加せず、同じ一人が全leafを検証できる予算・期限を必要とする。batch rootや一件の承認によって未検証leafを成功にしない。

初期batchは20 §4.4のatomic方式に限定する。同Miner・同model・同profileのleaf群を単一の新originalとして受理し、既受理originalを吸収しない。一leafの反証で全体Refuted、部分Final・部分creditなしとし、全仕事量・価値・最悪caseを合算する。

Nodeの上限内で裁ける区間の境界・部分和をMinerの初回commitへ含め、整数規則・参照・状態・停止まで全関係を接続する。Bを小さくすればtrace費用が増えるため、最低要件Nodeで測定する。後付けtrace、未知kernel、未分解の巨大演算を例外受理しない。[19](../rfc-v2/0019-model-free-node-and-bounded-evidence.md)。

## 7. 有効化と不変条件

最初に、モデルを一度も保有したことがない正直なPanelを一人だけ置く同一binary・同一設定の試験で、正常Final、不正反証、選択的不供給、遅延供給、悪意多数の不署名・虚偽case、cold取得と混合最大負荷を確認する。抽象検査を実LLM・runtime・性能・activationへ昇格しない。

全員共謀、永久検閲、合意Nodeが規則を実施しない場合を無条件に保証しない。正直者の存在だけで計算能力・費用・通信を得たとしない。Miner本人の物理的実行・非外注・実FLOPsも保証対象に含めない。

PRINCIPLES §8の計算暗号proof禁止、§9の二階層、§14のEXEC 1 BPS、旧署名と既存経済値、延期機能を維持する。報酬配分の既存不整合、bootstrap乱数接続、全kernel実装は別の有効化gateとして残す。今回状態は **ADOPTED_DESIGN_NOT_ACTIVATED**。方針は[公開裁定v0.4](../design/palw/consensus/public-adjudication-policy-v0.4.md)、レビューと検査は[今回の改訂](../maintenance/revisions/2026-11-09-public-panel/README.md)へ記録する。
