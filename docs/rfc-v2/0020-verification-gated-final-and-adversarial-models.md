# RFC-v2-0020 — 公開資料・モデル未保有Panel・一人の反証と検証先行Final

2026-11-11の後続採択: [RFC-0053](../rfc/0053-additional-model-burn-pause.md)／[ADR-0221](../adr/0221-additional-model-burn-pause.md)がHF取得可否の認定をburn停止・解除へ置換する。追加登録モデルだけを対象に、誰でもburnでPAUSED、登録者だけがburnでACTIVEへ戻す。genesis・年次fork指定は対象外。第三者取得証明・定期可用性認定・取得不能の多数決を要求しない。HF固定版/hash照合はoff-chainの実行資料同一性確認として残す。claim固有資料、再実行承認、不正反証と責任・原期限は維持する。PAUSEDは新規受付・成功Final・新規報酬確定を止める。状態は **ADOPTED_DESIGN_NOT_ACTIVATED**。

採択日: 2026-11-08 / 状態: **ADOPTED_DESIGN_NOT_ACTIVATED**。

追加採択: 2026-11-09。[RFC-0049](../rfc/0049-palw-execution-integrity-and-unilateral-refutation.md)／[ADR-0217](../adr/0217-palw-execution-integrity-and-unilateral-refutation.md)。本改訂は、固定されたLLM実行の局所反証可能性と、一人の適格申立人による反証の優先を新profileの必須契約にする。本人の実行行為の証明、全員共謀時の無条件保証、実装・実測・activation完了を意味しない。

同日の追加採択: [RFC-0050](../rfc/0050-public-materials-and-one-honest-panel.md)／[ADR-0218](../adr/0218-public-materials-and-one-honest-panel.md)。以降の新profileを論理名 **PublicOneHonest** と呼ぶ。各claimの独立選出Panelに少なくとも一人の能動的な正直者が参加し、その人がモデル未保有から取得・全体検証を始められることを保証の前提とする。全必要資料の公開供給、最低1件の完全検証承認、有限な全異議の終結を採択する。論理名はwire IDではなく、旧75/41のpolicyと旧署名は変更しない。

[20本の索引](README.md) / [採択記録 ADR-0215](../adr/0215-model-free-node-noninteractive-positive-and-adversarial-availability.md) / [上位原則](../PRINCIPLES.md#model-free-adversarial-verification)

本書は、今回採択した**新しいclaim版**の脅威モデル、global検証者選出、Final前の肯定認証、公開異議の時限、成功・未検証終了・信用取消を所有する。旧claimの署名、Final、Gate C、wire ID、fence、権利は遡及変更しない。設計採択はruntime接続・実測・有効化ではない。

## 1. 目的と必須前提

Nodeはモデルの全重み・checkpointを取得または保持せず、モデル全体を推論しない。GPU、モデルrunner、外部モデルURLへの接続、Panel workerの同居をNodeの要件にしない。Nodeが実行するのは署名・選出・状態条件の認証と、有界な局所証拠による決定論的裁定である。[19](0019-model-free-node-and-bounded-evidence.md)が検査境界を定義する。

モデルが事前に共有済みであること、モデル保有者が多数いること、モデル内のMinerや通常Panelが正直であることを前提にしてはならない。PublicOneHonestは、選出されたPanelの少なくとも一人が、初めて扱うモデルでも全体検証と必要な要求・反証を能動的に行う条件を採る。選出時点のモデル保有は不要であり、モデル保有者にだけ正直者がいるという条件へ置換しない。

Minerはpromptだけでなく、16が定める重み・構造・program・tokenizer／入力token列・数値規則・RNG・KV等の依存・初回traceと認証を公開供給する。モデル所有者の許可なしに適格なPanel／第三者が取得・認証・再配布できる経路を必要とする。Minerと他のPanelが結託し、正直な一人だけに資料を渡さない場合は、算術反証を前提としない資料要求と成功停止で扱う。公開方針やURLの存在を、実際の配送の証明にしてはならない。

新profileは、必要な肯定検証が成立しないclaimに成功Final、仕事報酬、consensus creditを与えてはならない。これを止めるために計算不正の反証の完成を要求しない。

ここでPALWの「計算の証明／検証」という語が対象とするのは、Minerが署名したモデル・program・入力・数値規則・依存関係に従う**実行の正当性**である。Miner本人が今回そのGPUで実行した事実、各Panelの独立した物理的作業、実消費FLOPs／電力、出力内容の世界における真実は対象にしない。正しい結果の外注・複製・許可されたcacheと、独力での実行を提出資料だけから区別したと表示してはならない。仕事量・重複禁止・依存creditは[07](0007-claims-exec-and-answer-accounting.md)の定義を別に満たす。

## 2. 脅威モデルと保証の境界

| 攻撃・状況 | 規範的な処理 | 残る前提・限界 |
| --- | --- | --- |
| モデル保有者内のMiner／通常Panelが全員結託 | モデル保有者集合とは別のglobal bonded poolから検証者を抽選する。通常Panelの署名だけでは成功不可 | 実際に担当する選出集合に一人の能動的正直者がいる条件を別に必要とする |
| 正直な選出者だけに資料を渡さない | 取得不能者は肯定してはならない。必要承認不足または適格な未履行要求があれば成功不可 | receiptやURLだけでは実取得を証明できない |
| Panelには渡し、public outsiderには隠す | 同じclaim資料に対する有界な公開要求・代替供給・保持経路を残す。未解決の適格要求をFinal条件に反映する | 世界中の全相手への常時配送をrootから証明するものではない |
| 正常な小片だけを提出 | 検査計画を原program・profile・commit後乱数から決める。任意選択の局所passで全体承認しない | 局所passは当該関係だけの成立 |
| 同一運営者がkeyやbondを分ける | 成熟stake、operator除外、既存の重複・予算制限を適用する | operator IDの相違は実所有者の独立性の証明ではない |
| 有利な抽選が出るまで出し直す | 一claim一試行。execution／jobの重複、class／network累積試行、費用、bond拘束も制限する | 一claim一回だけでは多数claimによるgrindingを全て除けない |
| 取得不能の虚偽申告による停止 | 要求の資格、対象、件数、bond、費用、期限、提出機会を有限にする | 私的な「届かない」という申告へ無制限のvetoを与えない |
| 最低1件の承認を悪意者が先に提出する | 全検査・異議の窓を省略せず、§4.2の一人の反証を人数・stake・肯定票数に関係なく裁定する | 最初の署名者が正直であるとは判定しない |
| 正常claimに対し、正直な一人以外が肯定署名を出さない | 一人の完全承認で承認数条件を満たす。全員ACKや多数承認を隠れた成功条件にしない | 他の成功条件と§4.3の正常完了前提は必要 |
| 悪意多数が合法な最大数の虚偽caseを開く | 個別枠・客観的終結・原期限内の最大工程を事前予約する | 全合法caseを有界に終結するprofile別機構が未確立なら正常完了の保証は未確立 |
| 全global選出者が共謀し、適格な第三者も検査・異議を行わない | 承認前提が破れた残余リスクとして記録する | **署名と少数の局所positiveだけでは、虚偽の全体承認を無条件には排除できない** |

「モデル内のPanelは全員悪意でもよい」と「global検証者まで全員悪意でも安全」は別の命題である。本書は後者を主張しない。登録tier、bond額、root一致、局所証拠の存在、異議の不在を、欠けた独立検証の代用にしてはならない。

PublicOneHonestで成功を求めるclaimには公開供給を必須にする。モデル事前保有・所有者の個別許可を検証への参加障壁にせず、**必要資料が得られず所定の検証が成立しないclaimを成功させない**。これはclaimの供給契約であり、モデル登録の公開状況審査、保有者数審査、登録取消、tier昇格を新設するものではない。既存の1.0／0.3と配分規則は維持する。

## 3. 責務と用語

| 用語 | 意味 |
| --- | --- |
| 通常Panel／worker | 取得・再実行・資料生成・局所化を担う実行役。任意の自己申告groupの多数決は認証authorityにならない |
| 選出検証者 | 固定claimに対してglobal poolから選ばれ、所定の独立検証を行うbond／抽選枠。Panelを兼ねても認証はこの選出記録に基づく |
| LocalPositiveChecked | [17](0017-noninteractive-positive-verification.md)の有界な局所関係をNodeが検査した結果。全claimの正しさではない |
| IndependentReplayMatched | 選出検証者が元claimの全実行を再現し、出力とcanonical execution rootを照合したという署名付き承認 |
| Q(C) | claim Cのnativeな未解決CaseId集合。適格な計算noticeと資料要求を含み、承認票とは別に保存する |
| ComputationRefuted | 認証された元claimの必須関係が偽であるとnative courtが直接確認した評決。Panelの投票結果ではない |
| CertificationReady | 新版のFinal前認証条件がそろった状態。単独では報酬・fork-choice・settlement creditを生まない |
| Final | 新版では肯定検証・公開異議・依存・経済条件を満たした成功状態。Abortや未検証holdとは区別する |

論理名は新設計の意味を示す。既存codecのenum値、RPC tag、`PositiveChecked`、`SecurityCertified`を無断で同じ意味に変更しない。実wire値・署名domain・fence・activation heightは未割当である。

## 4. 結果固定後のglobal抽選

1. [16](0016-miner-execution-commit-and-material-supply.md)のMiner署名付きoriginal execution、資料manifest、提供義務を固定する。抽選後のresult／trace／入力／seed差替えを認めない。
2. 固定時点の成熟済みglobal bonded stakeを母集団にする。モデル保有者、Minerの推薦リスト、資料取得に成功した者だけに母集団を狭めてはならない。
3. producer自身のoperatorを既存のidentity規則で除外する。登録直後のbondを入れない。関連者が別IDを使う残余は悪意stakeに含める。
4. claim固定時には未知だった、固定点より後のcanonical seed anchorを用いる。
5. 版付きpolicyが定める`m`枠を選ぶ。既存のstake加重復元抽選を採る場合も、**m枠はm人の独立operatorを意味しない。** PublicOneHonestの最低完全承認数は`minimum_full_replay_approvals = 1`とする。正規に選出された一つのbondによる元subject全体への完全承認が必要で、非選出者の署名や局所positiveで代用しない。
6. 同じclaimを再抽選しない。取得失敗、Panel離脱、negative移行、relayer変更、再起動によって抽選やcase予算を作り直さない。
7. global pool不足、seed未確定、期限内の承認不足は成功不可とし、規定期限で未検証終了する。

`m >= 1`、成熟期間、抽選上限、全選出者の検証義務、資源・期限を版付きpolicyに明示する。本採択で確定する新profileの承認数条件は最低1件だけであり、`m`の新値、帯域、処理量、tile幅、liveなwire値は確定しない。旧Gate Cの`m=75,k=41`、多数誠実性の評価、旧署名には旧規則を適用する。新profileへ`2k > m`や全PanelのACKを暗黙に追加してはならない。

最低1件は全選出者の取得・全体検証義務を一人へ縮小する規則ではない。誰が正直か不明なので、任意の一人・小部分集合だけに計算させたり、layerを分担するだけで全体に一人の保証を保ったと扱わない。`m`の削減には削減後の各担当集合での一人前提の評価が必要である。**適格な一人のnotice、資料要求、完成反証に二票目を要求してはならない。** 未確定・未測定の残りpolicyがある間は当該profileを有効化しない。

### 4.1 誠実性前提をruntimeの観測値と混同しない

Nodeが検査できるのは、drawの正しさ、署名、同一subject、必要枠数、期限、局所証拠、状態条件である。署名から、署名者が実際に再実行したことや正直なstake比を測定できるわけではない。

PublicOneHonestの安全性は、各claimの担当集合に一人の能動的な正直者がいる条件と、その人の取得・検査・反証の実行可能性に基づく。旧Gate Cの正直な割合に関する評価とは別の保証である。この前提を全体の暗号学的計算証明へ読み替えず、`honest=true`等の自己申告を成功gateにしない。[PRINCIPLES §8](../PRINCIPLES.md)の禁止は維持する。

独立なstake抽選を仮定し、producer除外後の悪意割合を`f`、枠数を`m`とする理想化では、全枠悪意の確率は`f^m`である。これは一人前提を実際の選出方式でどう支えるかの評価用であり、一人がいるという今回の条件付き定理そのものではない。共通支配、共通障害、資料取得・検査能力、包含失敗、反復試行、batchごとの累積価値を別に評価する。旧bad-quorum確率をそのまま新profileの不正成功率へ流用せず、最低承認数1を理由に`m=1`が安全と推定しない。

### 4.2 一人の正しい反証が勝つ条件付き保証

PublicOneHonestでは、各claimの選出集合内に一人の能動的な正直者がいる前提を採る。その人以外のMiner・通常Panel・選出検証者が結託していても、有効な反証の評決を多数決で覆せない。正規に受理された適格第三者も単独で反証できるが、非選出者の存在だけで選出集合の前提や正常時の承認資格を置換しない。別key／重複抽選位置を人数として増幅しない。

保証の前提を次のように固定する。

| 前提 | 必須内容と所有RFC |
| --- | --- |
| 実行定義 | 固定されたjobを有界・決定論的な既存TIR／versioned kernelの関係として解釈できる。16のWellFormed／Init／全Step／Endを網羅し、19の局所検査に健全性と反証完全性がある |
| 認証 | hashのbinding、署名の認証、元claim・モデル・program・入力・位置・規則へのbindingが成立する |
| 検証者 | 選出集合内の正しい独立検証者が少なくとも一人、モデル未保有から取得し、受理profileの全体検証と証拠作成を期限内に実施する。Nodeがその人の正直さを測定したという意味ではない |
| 資料 | その人が認証付きの必要資料を取得できる。事前保有は不要。取得不能時は完成算術反証を要求せず、適格な資料要求・未解決caseと未検証終了の別経路を使う |
| 公開サービス | 18の保護された当人の要求・応答・提出枠、19の裁定上限、05の合算予約が実際に供給される。他人の合法要求・EXEC負荷がこの枠を消費しない |
| チェーン | 所定の配送・canonical包含・復旧・合意の前提が成立し、native verifierが規則どおり実行される。mempool送信をcanonical受理とみなさない |
| 時間と権利 | notice、証拠完成、裁定、責任窓が元の有限契約内に収まる。無期限の再審・時計resetを仮定しない |

この前提の下では、元claimが規定実行・出力・仕事会計に反する場合、16／19の局所完全性により偽の必須関係を少なくとも一つ特定できる。正直な一人がその反証を18の経路へ提出すれば、native裁定は`ComputationRefuted`となる。§5.2の優先規則により、肯定署名が`m`枠あっても成功を阻止し、既存の成功状態には元版の責任窓内で`Revoked`を適用する。

**これは反証可能性と反証優先の定理であり、全claimの小さな肯定計算証明ではない。** 正直な一人が単に不承認のまま沈黙するだけでは、悪意者の1件の肯定を止めるとは限らない。その人は期限内に適格notice／要求／反証をcanonicalに受理させる必要がある。正常claimの完了については、最低1件の承認に加えて次節の別条件を必要とする。

資料・サービスが不足し、それが適格要求、未解決case、未履行契約またはnativeに検出できる受入条件違反として記録される場合、成功を保留して期限内回復または`AbortedUnverified`にする。全員の秘密の共謀や、正直な人の全通信が永久に検閲されてNodeから観測不能な場合まで、Nodeが自動的に検出してAbortできるとは主張しない。この限界を「fail closed」の語で隠さない。

### 4.3 正常claimが悪意多数の不署名・虚偽caseで止まらない条件

次をすべて満たす場合に、原期限内で正常なclaimがFinalへ到達できることを有効化条件にする。

1. originalと全実行関係が正しく、Minerが署名した公開供給・応答義務を実際に履行する。
2. 正直な選出者がcold取得から全体検証を完了し、元subject全体への1件の承認を期限内に提出する。
3. 全ての合法な資料case・計算caseについて、[18](0018-interactive-negative-court-and-defaults.md)の客観的な供給・検査機会の成立、正規terminalまたはChallengerDefaultまでを有界に実行する具体的手順がある。
4. 他の選出者と適格第三者が合法な最大要求・最大遅延を使っても、その全工程、必要な防御応答、包含、裁定、回復の資源が原受理時に予約され、実際に供給される。
5. 依存・資金・責任・合意が成立し、原期限までにQが客観的に空になる。無関係な支払や新creditを先に必要とする循環を作らない。

この条件下では、悪意側の不署名は最低1件の承認を妨げず、偽の計算証拠は健全なnative checkerでMinerを有罪にできない。合法な虚偽caseも予約済みの有限工程で終了するため、公開窓後に§5.1が成立する。誤claimに対する反証と、正常claimに対する有限な防御は同じbinary・設定で試験する。

**一部chunkの提出、URL、provider署名、他のPanelのreceipt、要求quotaの消尽だけでは、申立人が全scopeを検査できたことにならない。** 18の`InspectionPrerequisitesEstablished`に相当する客観的前提が不足する場合、検査締切だけを根拠にChallengerDefaultとせず、NoSlashHold／元期限の未検証終了へ進める。その供給・検査機会を有界に確定する具体的機構が未確立のprofileは、正常完了の保証が未確立であり有効化不可である。全モデルをNodeへ逐次流してこの不足を埋める例外も置かない。これは要件の記載で解消したと扱ってはならないprofile別の未完gateである。

### 4.4 有限なatomic batchによる固定費共有

batchは任意の版付き最適化であり、単一claimの経路を必須のまま残す。今回の初期方式は、**同じMiner・model root・profileの有限なexecution leaf集合を受理前に封印し、単一の新originalとして受理するatomic batch**に限定する。独立した旧originalを後から吸収してはならない。

- 各leafのjob／execution identity、入力・出力・初回trace・依存・仕事量、canonical順序・leaf数をbatch rootへ束縛し、その後の未知seedでPanelを選ぶ。
- 選出者は全leafを検証する。最低1件の完全承認はbatch全体に対するもので、他のleafを未検証のまま成功させない。封印後の追加・削除・差替え、選出済みPanelへの仕事の継ぎ足しは禁止する。
- Qと個別CaseIdはbatch originalとleaf座標・申立人に結び付ける。共有bytesの再利用で別人の権利を消さない。
- 一つのleafの原claim反証でbatch全体を`Refuted`にする。資料・検証が未完ならbatch全体が成功不可。部分Final・部分credit・正常leafの自動切出しはこの方式では認めない。
- 全仕事量、価値、cold取得、trace、費用、最悪の全case・終結工程を合算予約する。抽選回数が減っても検証仕事量をbatch数だけへ縮約しない。
- execution IDごとの一回性を[07](0007-claims-exec-and-answer-accounting.md)で管理し、batchの再送・分割・別root化で既消費creditを二重発行しない。失敗後の再提出も既存の再試行・重複・grinding規則を満たす。
- batchの署名scope、資料形式、native admission／settlement、最大leaf数・価値・資源を実装・測定するまで有効化しない。既存codecを流用して有効化したと扱わない。

この方式は抽選・モデルロード・同一資料配信の固定費を共有できるが、1leafの違反で全体が失敗する範囲も拡大する。第三者の既受理claimを無断でこの失敗範囲へ入れない。

## 5. 新版の状態遷移

原受理DAAを`a`、Minerの結果・trace固定DAAを`c`とする。既存経路で両者が同じなら`c=a`でよいが、後の固定点やreceiptを理由に`a`を取り直してはならない。

| 状態／事象 | 次の状態・効果 |
| --- | --- |
| 有効なoriginalと必要な資源予約を受理 | `ResultCommitted`。同時に新版認証のsubjectと母集団cutを固定 |
| 後続seedと正規drawが確定 | `VerificationPending`。資料取得・独立再実行・公開検証を進める |
| 必須局所positiveと選出者の全subject完全承認が1件以上成立 | `CertificationReady`。検査・異議の窓を短縮せず、まだ成功creditは与えない |
| 一人から適格な計算noticeまたは資料要求を受理 | `Q(C)`へCaseIdを永続追加して成功を停止。同じcase・原時計・保護予約で解決する |
| 資料要求へ適格応答が到着 | 当該供給義務の履行と検証caseを分離。caseは`InspectionPending`としてQに残し、取得後の検査・反証へ継続 |
| 認証・形式・scopeが不正なpacketをnativeが拒否 | 当該packetのみ拒否。既に受理したCaseId・争点・手番・Qを解決済みにしない |
| 認証された正規terminalが固定争点の不成立を示す、または客観的なChallengerDefaultを確定 | 当該CaseIdだけを終了。他case、供給義務、依存、期限を残して再評価 |
| 公開窓、依存、責任、支払等の成功条件がすべて成立 | `Final`とcredit資格を原子的に反映 |
| 認証・供給・資源が期限までに成立しない | `AbortedUnverified`。成功Finalではない |
| native courtで計算不正が確定 | `Refuted`。元claimの無効化と帰責に沿った処理 |
| 既存Finalが正規の責任期間内に反証される | `Revoked`。対応するcreditと仕事報酬資格を取消 |

`Unavailable`、`NotReproducible`、root不一致は、それだけでMinerの計算有罪を確定しない。[18](0018-interactive-negative-court-and-defaults.md)に従う。

### 5.1 Finalと認証の循環を作らない

旧Gate Cは`Final`で認証を開始し、判定に`still_final`を読む。新版はこの前提を使わず、固定された**生存中の検証待ちsubject**を認証の入力にする。

```text
certification_ready =
    admitted_complete_bounded_profile
    && original_and_draw_valid
    && required_local_positive_set_checked
    && valid_selected_full_replay_approval_count >= 1
    && certificate_matches_original_and_version

can_success_final =
    certification_ready
    && now > notice_until
    && public_review_conditions_satisfied
    && Q(claim).is_empty()
    && no_delivery_default_or_recorded_service_failure
    && dependencies_credit_ready
    && economic_and_liability_conditions_satisfied
    && !refuted && !revoked && !aborted && !expired
```

`certification_ready`は`Final`を前提にしてはならない。`can_success_final`はNodeが認証できるnative状態から導出し、peerから渡されたboolを信用しない。認証待ち中のreorg・取消・失効もsubjectの生存判定に反映する。

上の承認数条件はPublicOneHonestだけに適用する。承認は元original・model・program・input・trace・規則版・全scopeに束縛された選出bondのものを数え、同じ署名の複製や部分leaf承認を増幅しない。全選出者のACK、他人の不署名、不成立の多数閾値を追加の隠れた成功条件にしない。同時に、承認数を満たした時点からnotice窓やcold検証機会を短縮してはならない。

新版の合意credit getterは、少なくとも`Final && valid_pre_final_certificate && Q(claim).is_empty() && source_rights_live && dependencies_credit_ready && !refuted && !revoked && !aborted`を必須にする。Final後の適格caseで未解決となった起源も、現在の権利使用をholdする。`CertificationReady`、名称だけのFinal、local positive、旧certificateを別経路でcreditに変換してはならない。

### 5.2 同じcanonical処理単位での反証優先と終端

`Q(C)`、裁定record、資料義務、保護予約、未払・留保権利はnative stateの一部として保存し、restart／reorg／IBDで復元する。肯定票追加、Panel交代、caseのalias変更、抽選位置の集計によって`Q(C)`を空にしてはならない。case解決は、そのcaseの認証された正規terminal、本人の許可された終了、客観的なChallengerDefault、または成功不可のままの有界終了だけによる。packet認証失敗はcase解決ではない。資料供給義務の履行だけでは、その資料を用いる検証caseを閉じない。

同じcanonical block／native処理batchに、notice・反証・期限処理・Final候補が含まれる場合、次の順序で結果を決める。Finalの先行実行をtransactionの並び替えで作れないよう、Final／成功creditのcommitは最後のphaseにする。

1. 元の版・権利・時刻・入力依存に従い、その単位内の合法なnotice、要求、応答、完成証拠を認証・受理する。完成した直接反証は、許された新規notice窓内ならnoticeとterminalを原子的に処理できる。
2. 受理済みの完成証拠を裁定し、`ComputationRefuted`を最優先で反映する。元claimの無効化と08の依存・credit失効を、循環を許さない依存順で確定する。無関係な`LocalPredicateHolds`や多数のpositiveは反証を相殺しない。
3. 当該単位までの合法な応答を反映した後で、義務default・case解決・期限終了・返還を判定する。期限で未検証終了になっても、それを有罪へ変換しない。
4. 残るclaimについて、更新後の全case・依存・承認・公開機会・資金・期限から`certification_ready`と`can_success_final`を再導出し、成功状態と成功資格を原子的にcommitする。

この優先順は新規Finalとmintだけでなく、**既発行creditの予約、schedule有効化、permitのcanonical受理、`reserved -> consumed`、関連fee・取引効果**にも適用する。すべてのconsumerは同batchの反証・Qによるhold・依存失効を反映したstateで現在の適格性を判定する。起源の反証とEXEC候補が同batchにある場合、EXECを先にconsumedへ動かして取消から逃がす順序を認めない。07／10が保持する「既受理のconsumed」は、このbatchより前のcanonical受理で既に消費した分を指す。同batchの候補署名・provisional実行・前方配置だけではその保護を得ない。

新規noticeは`now <= notice_until = a+3,120`で受理し、成功Finalは`now > notice_until`を必須とする。同じDAA値を持つ後続canonical単位があり得ても、`now == notice_until`で先にFinalにしない。`hard = a+10,000`当日も合法な既存case証拠はそのcaseの`collect_until`まで受理し、先にtimeoutで捨てない。`hard+1`は排他終端であり、成功していない未完了claimを遅くともそこで`AbortedUnverified`へ終える。case別のより早い終了義務はその値を守る。

notice窓終了後に初めて届く資料は、自動的に新規notice資格を得ない。既存caseの許された収集・裁定期限、または原受理時に明示したFinal後の訴追権・責任窓に従う。`material_until`だけから新しい訴追権を生成しない。元profileがFinal後の経路を認める場合、その新規申立期限・応答／裁定終端・保持・担保の各期限を受理前に一意に固定し、未割当ならそのprofileを有効化しない。

初回notice窓内に受理した`MaterialRequest`については、18 §2.3の取得後検査と、その同じscope内の局所化・反証を**既存caseの継続**として扱う。`notice_until`後の資料応答によって新規notice権を作るのではなく、既に保護したCaseIdの権利を完了まで維持する。応答到着や供給義務の履行を理由にQを消去し、検査者を期限外の新規申立てへ追い出してはならない。

`Refuted`／`Revoked`／`AbortedUnverified`からpositive追加・再送・再抽選で成功へ戻さない。同じ証拠・裁定のreplayは同じeffect identityで一回だけ適用する。`AbortedUnverified`後でも元の訴追権が残るなら計算責任を別途確定できるが、成功creditは復活しない。canonical reorgは、状態・draw・case・署名権利・決済の一体復元として扱い、別branchの評決だけを無条件に持ち越さない。

## 6. 一つの原時計と公開検証機会

既存の公開裁定契約の次の境界を維持する。

| 境界 | 新版での扱い |
| --- | --- |
| original admission | 原点`a`を保持 |
| notice受付 | `a+3,120`まで。既存の境界の包含規則を継承 |
| 係争hard deadline | `a+10,000`。後のreceipt・draw・取得・交代でresetしない |
| 最遅合法notice後の予算 | 6,880 DAA。実際のoff-chain取得・計算・提出・回復を含む |
| `collect_until` | hardを越えない。排他終端`hard+1`へ意味を置換しない |
| `material_until` | 収集とは別の保持義務。Final／Abort／collect終了だけで削除しない |

新版のprofileは、抽選確定、必要資料の取得、再実行、肯定提出の各期限を原時計内に定義する。旧Final後の認証窓をそのまま直列に足して、この契約に収まったと扱ってはならない。

とくに、資料をnotice終了直前に公開して「異議がなかった」を作る攻撃を拒否する。受理profileには、少なくとも次の二条件を満たす測定値・上限・予約が必要である。

```text
max(draw_ready_by, required_material_ready_by)
  + cold_acquisition_bound + independent_check_bound
  + notice_construction_and_inclusion_bound <= a + 3,120

latest_legal_notice
  + localization_and_response_bound + public_filing_bound
  + adjudication_and_recovery_bound <= a + 10,000
```

資料要求から始まるcaseについても、必要資料到着後の全体検査・局所化・証拠組立て・包含・裁定をこの予算に含める。`material_ready_at + remaining_worst_case_bound <= case_terminal_by <= hard`を満たさない実到着は、検査機会の成立ではない。検査scope、`material_ready_by`、`inspection_by`、局所化・包含・裁定終端を原受理計画へ固定し、遅い応答の時刻から再設定しない。この不成立をnativeで認定したcaseは、履行済み資料があっても成功不可のまま回復または未検証終了にする。

重なる工程を短縮として計上する場合は、その並行実行資源を予約する。新規の公開要求は資格・必要範囲・残り予算を満たすものだけを受理し、正規に受理した要求をPanelの肯定署名で消してはならない。

予定した提供時刻を越え、独立した公開検査機会が残らなくなったclaimは、時計を延ばさず成功不可にする。無係争claimを無条件に10,000まで待たせない。成功は必要な公開窓と他の条件が終了した時点、失敗は指定された終端で処理する。

### 6.1 一人の救済を他人の予約から隔離する

originalの受理計画には、全選出位置を占め得る主体の上限に対して、18が定める**各選出bondの救済予約**を確保する。draw確定前は上限を予約し、draw確定後にcanonicalなbondへ割り当てる。復元抽選で同じbondが複数回現れても、その主体の要求予算は増殖させない。独立したbondの予約を「全体枠は使用済み」として先着の共謀者へ譲渡しない。適格第三者の受理・予約は選出者の枠と分離し、無限の新規outsiderを収容できるとは仮定しない。

当人の局所化・資料要求・反証は一つの継続caseとして元の累積予算を使う。攻撃者が自分の枠で偽の異議を繰り返しても、正直な別人の救済を失わせない。共有blobの重複排除はbyteの再利用だけであり、他人の未解決caseや提出機会を消す根拠にしない。制御・提出・包含・裁定とEXECが共用する物理資源は05で合算し、相容れない予約の二重計上を禁止する。

受理前に有限な全合法分岐を予約できないclaimは拒否する。受理後の実サービス不足をnativeが確認した場合、回復費も元予約内に計上し、回復できなければ成功不可のまま期限終了する。料金やbondを払えば容量が自動的に存在するという前提は置かない。

### 6.2 cold検証と虚偽case終結を同じ時計へ収める

モデルを事前保有しない選出者について、公開manifestからの取得・認証・初期化、全関係検査、証拠作成と搬送を計上する。保証できないcache hitを冷取得量から控除しない。正直な者が誰か不明なので、能力・費用・期限の計画を一人分の先着枠だけにしてはならない。

最終合法noticeの時点でも、そのcaseに必要な供給応答、取得後検査、局所化、真偽の裁定、客観default、復旧がhardまでに収まる必要がある。単に『hardで全部Abort』とする方式は安全側の終了であり、§4.3の正常完了を満たした証拠ではない。虚偽caseを全て解決して成功する試験と、サービス不足で成功を停止する試験を分ける。

### 6.3 新profileの容量policy

旧台帳の4,096抽選position／日と75position／claimを用いれば上限は54claim／日である。最低承認数を1へ変えても、`m`と抽選上限を残せばこの計算は変わらない。新profileは旧枠を暗黙に流用せず、[05](0005-bounded-resources-and-long-context.md)の抽選・署名、再実行、cold配信、保持、native CPU／RAM／hash／parse／証拠bytes、canonical包含を資源ごと・期限区間ごとに確定する。新版の量的policyが未確定なら有効化不可であり、旧枠を削除して無制限受付にしてはならない。

5万モデルが各1claim／日なら平均約0.579claim／秒、各1claim／時なら約13.889claim／秒であり、これは活動量の算術例であって実測性能ではない。各modelの到着率・実検証主体数・再実行仕事量・cold率・資料bytesから需要を計算し、最悪の合法な係争を別に足す。Node数の追加は、全Nodeが同じcourtを実行する場合の一Node当たり上限を増やさない。batch・cacheの改善を独立検証計算の消滅として計上しない。

## 7. 何をもって資料条件成立とするか

[16](0016-miner-execution-commit-and-material-supply.md)の供給契約、認証された内容、完了記録、実際の検証者承認、未解決の適格な公開要求を読む。root、manifest、URL、providerの自己申告、Panelの単なる受領署名だけを完全な取得可能性の証明にしてはならない。

global選出者が必要資料を得られなければ肯定しない。public outsiderには、Minerの任意の許可や特定Panelのreceiptに依存しない、有界なcanonical requestと代替供給経路を残す。資料の共有は当事者固有の提出機会・義務の帰責を消さない。

資料要求の受付に、算術矛盾、モデルの事前保有、所有者の許可、二人目の署名を要求しない。対象は固定programとmanifestから導出する必要範囲とし、任意の無関係な巨大資料要求は認めない。応答の認証、供給義務の履行、全scopeの取得後検査の前提、caseの終了を別のnative事実として扱う。少数の有効応答やquota消尽で全scope取得を推定せず、未確認の機会不足を正直な申立人のdefaultへ転嫁しない。

Nodeが全モデルを取得して確認するfallbackは置かない。資料が足りないことは、成功条件が成立していないことであり、算術不正を推定する根拠ではない。

## 8. 成功・失敗の経済効果

| 終了理由 | 仕事報酬・合意credit | fee・bond・資料保持 |
| --- | --- | --- |
| 成功Final | その版の報酬資格とcredit条件を満たす場合のみ消費可 | 既存の成熟・liability・保持期間を継続 |
| AbortedUnverified | 0。成功Finalへ分類しない | 実行済みの有効な役務費用だけを署名済み契約内で精算し、未使用予約を返還。未確定の計算slashは行わない |
| 計算不正確定 | 無効化、未払い仕事報酬失効、適用範囲のcredit取消 | 元claimと署名・役割に束縛した責任だけを処理 |
| QualifiedDeliveryDefault | 成功不可。計算不正という名称にしない | [18](0018-interactive-negative-court-and-defaults.md)の客観的条件を満たす当事者のみ帰責 |
| Panel停止／service機会不足 | 成功不可、期限内の回復または未検証終了 | Minerの自動有罪にしない。未使用費用・実役務・既存責任を分ける |

safe frontier、safe/live weight、settlement depth、Rule E participation、settled anchor、issuance、関連するnative仕事receiptの全consumerが同じ認証失効を読む。fee-onlyの役務支払いを成功仕事報酬やconsensus creditとして数えない。利用者払いと発行由来の原資を合算して新しい支払権を作らない。

旧RFC-0045のWorkerPaymentとその `B=F=R=0` の原資区分、旧RFC-0046のfunded relayの同意・予約・返還権を、新しい肯定承認だけで拡張しない。追加の支払や発行を必要とする場合は、[11](0011-bond-budgets-fees-and-payments.md)で原資と署名済み権利を別に定義・採択する。

Final後の有効な反証・取消、依存claimへの伝播、徴収可能な担保、保持義務は[08](0008-claim-dependencies-and-liability.md)と[14](0014-court-master-contract.md)の版付き権利を維持する。新しい成功順序を理由に旧claimの責任期間を短くしない。

## 9. 旧規則との優先・移行

| 旧規則・文書 | 新profileでの変更 | 保持するもの |
| --- | --- | --- |
| RFC-0044 R2: Panel witnessをNodeが全被覆検査 | Nodeは有界な局所検査、全体は独立選出者の承認。保証が異なることを明示 | 元本文、旧署名、旧profileの全被覆の意味 |
| R2: Minerの結果commitにtrace rootを含めない | 新版originalにMiner署名付きtrace／execution rootを追加 | 旧型を別の意味でdecodeしない |
| 旧Gate C: Final後にcommit・draw | 新版はResultCommittedから認証し、成立後に成功Final | 旧claimのFinalとSecurityCertifiedの順序 |
| 旧75/41の肯定多数条件・前版の可変k | PublicOneHonestは最低1件の選出者による全scope承認＋全検査機会＋未解決caseなし。各担当集合の能動的正直者一人が前提 | 旧policy、旧署名、旧m/kの評価、旧経済値。新mは未確定 |
| PRINCIPLES §1–2の通常全量再実行不要 | 今回の新profileに限り、選出Panelの独立全再実行を正常基準とする | Nodeへの全再実行要求禁止、旧sampled profileの定義 |
| model-epoch配分の承認 | 本書のclaim認証とは別の制度 | 二階層、bond配分、既存経済パラメーター |
| 公開裁定の方針v0.1／RFC-0047 | Node非保有、資料供給、肯定認証先行を追加 | 原時計、一つの鎖、公開第三者経路、局所court、保持と収集の分離 |

新規claimの受理停止によるrollbackは可能にし、既に受理した新版claimの時計・権利はその版で完了させる。新形式を旧署名のoptional tailへ埋めて承認済みと扱わない。

## 10. 受入シナリオ

以下は要求される試験であり、この文書作成でPASSを宣言するものではない。

| ID | ケース | 必須結果 |
| --- | --- | --- |
| A20-01 | モデルキャッシュもGPUもないNodeで正常claimを処理 | 全モデルfetchなしで承認・局所証拠・状態遷移を検査 |
| A20-02 | モデル保有者内の全Miner／Panelが同一支配 | その集合の署名だけではFinal不可。global draw記録が必要 |
| A20-03 | 選出された正直な一人へ資料を渡さず、残りは肯定する | その人の適格な資料要求を保護枠で受理。承認数にかかわらず未解決なら成功不可。期限で未検証終了 |
| A20-04 | 正しい局所positiveと悪意quorumの肯定があり、正直な一人が元claimの反証を提出 | 反証をnativeが検査し、肯定数にかかわらず無効化。未解決中はFinal／成功credit不可 |
| A20-05 | Panelだけ取得可能、適格なoutsiderの要求は未履行 | 正規要求が未解決の間は成功不可。帰責と失敗終了を分離 |
| A20-06 | notice終了直前にだけ資料を公開 | 公開検査機会の残予算不足なら成功不可。時計resetなし |
| A20-07 | root不一致、最終出力だけ一致 | 正常承認しない。negativeによる確認へ。自動slashなし |
| A20-08 | peerがPassed／Availableのboolだけを提出 | native認証・証拠・履行記録を欠くため成功不可 |
| A20-09 | 認証Pending中にreorg、復元、seed anchor変更 | 元のcanonical subject／drawを再導出。二重認証・二重支払なし |
| A20-10 | 抽選不成立／資料不足／service停止 | 指定期限でAbort。Finalまたはcreditへ変換しない |
| A20-11 | 一claimの再抽選、別case／nonceでの予算更新 | 固定execution・重複・累積予算に基づき拒否 |
| A20-12 | 旧claimと新版claimが混在 | 各署名版の規則で処理。Final／認証の循環待ちなし |
| A20-13 | 認証済みclaimの後日conviction | 全credit consumerと依存先の取消・再計算が整合 |
| A20-14 | 上限を越えるcold取得・再実行・negative最悪ケース | 新profile受理不可。測定値を切り詰めて合格にしない |
| A20-15 | 全選出者と全外部検証者が共謀し、検査対象外に誤りがある | 無条件安全性のPASSにしない。小さなpositiveと署名の保証限界を記録 |
| A20-16 | 最終合法notice／反証／肯定／Final候補が同じcanonical単位にある | §5.2の順序で反証とQを先に反映し、tx順序で成功を作れない |
| A20-17 | notice_untilと等しいDAA値の複数block、hard当日の合法証拠 | 境界inclusiveの提出権を守り、Finalはnotice_untilより後、未完了はhard+1までにAbort |
| A20-18 | 悪意Panelが自分の合法要求枠をすべて使い、別の一人がnoticeする | 当人の保護予約で受付・包含・裁定。全体先着枠で排除しない |
| A20-19 | 一つの偽noticeが棄却され、別の正当caseが未解決 | 当該caseだけを閉じ、Qが残るためFinal不可 |
| A20-20 | 全Panelが不承認、またはPublicOneHonestの完全承認が0件で異議なし | 期限で未検証終了。異議なし／timeoutを承認へ変えない |
| A20-21 | private timeoutのみ、または一人の全通信がcanonicalへ届かない | 計算有罪・配送defaultを捏造せず、観測・包含前提外の保証を主張しない |
| A20-22 | Init、読み書き接続、停止、最終出力、仕事量のいずれかだけを偽る | 16／19の対応する関係で局所反証。内部算術だけ正しいtraceを全体合格にしない |
| A20-23 | 完成証拠の二重搬送、restart／reorg／責任窓内のFinal後証拠 | case・評決・支払・取消をscope付きで一度だけ反映し、元の訴追権を維持 |
| A20-24 | 元claim取消後に未使用ExecCredit／予約permitと既受理EXEC取引が共存 | 未消費権利を失効し、consumed provenanceを記録。取消だけで全既受理取引を巻き戻さない |
| A20-25 | 全claimまたは全profileを拒否するだけの実装 | 安全な拒否とは区別し、利用可能なPALWの完成判定をFAILのままにする |
| A20-26 | 正当caseを指す無効opening／誤署名／別scopeのpacketを悪意者が送る | packetだけを拒否し、既存Q・争点・手番を維持。悪意quorumでFinalへ進めない |
| A20-27 | notice窓内の資料要求へ窓後に応答し、直後に悪意quorumがFinalを要求 | 供給履行だけではQを閉じない。取得後検査・反証を元caseとして継続し、残時間不足なら成功不可 |
| A20-28 | 同じ資料への先着caseを悪意者がdefaultし、別の正直な請求人がいる | 共有MaterialIdとは別の申立人付きCaseIdを維持し、正直な人のQ・予約・終端権を消さない |
| A20-29 | 正規の上限内証拠が元Minerの不正形式recordを認証する | WellFormed関係の反証として無効化。packetの不正や未開示資料とは区別 |
| A20-30 | 既発行creditの起源への反証／適格noticeと、そのpermitの消費が同batchにある | 先に取消／holdを反映し新消費不可。batch以前のconsumedだけを既受理として保持 |
| A20-31 | モデル未保有の正直な選出者が一人、他全員が不署名 | cold取得・全体検証・1件承認を実行。§4.3の前提下で全窓・全case完了後Final。旧41票や全員ACKで停止しない |
| A20-32 | 悪意者の1件承認が早く到着し、正直なcold検証者は未完了 | CertificationReadyをFinalにせず、全体検査・異議窓と未解決権を維持 |
| A20-33 | 正直なcold検証者だけに全重みまたは一部chunkを秘匿 | 算術反証なしで適格要求を受理。選出のやり直しや取得成功者限定をせず、未解決なら成功不可 |
| A20-34 | 正常claimに悪意者が最大数の合法な虚偽資料／計算caseを開く | profileの客観的手順と予約が成立する条件下で各caseを有界終結しFinal。成立しないprofileを活性PASS／有効化可としない |
| A20-35 | 一部chunk応答・他人receipt・quota消尽だけで検査完了またはChallengerDefaultを要求 | 全scopeの客観的前提がなければ拒否。NoSlashHold／元期限の未検証終了、Node全量fetchなし |
| A20-36 | 75枠のうち任意3枠だけを再実行担当とし、正直者が担当外 | 一人保証の反例として検出。全体検証の義務・機会を維持、部分集合の誠実性を別途確立しなければ採用不可 |
| A20-37 | 同モデルの有限atomic batchで1leafだけ誤り、他leafは正常 | 当該leafの認証反証でbatch全体Refuted。部分credit不可、全execution IDの一回性を維持 |
| A20-38 | batch封印後のleaf追加、別Miner混入、既受理original吸収、旧leaf承認の流用 | scope／版／identityにより拒否。batchの承認を未検証仕事へ拡張しない |
| A20-39 | 5万登録・cold cache・最大係争・EXEC併走・restart／reorg／IBD | 活動量と各資源を計測。受理済み保護枠を維持し、新規受付を実容量内に制限。未測定値で性能PASSとしない |
| A20-40 | 旧75/41署名とPublicOneHonestの1件承認が同居 | 版別の意味と閾値を維持。旧claimを新閾値で成功させず、新claimを旧閾値で停止させない |

## 11. 実装・有効化に残る具体的gate

- 新署名形式、original trace帰責、各native入口への接続、receipt scope、pre-Final認証foldを実装する。
- RPC・wallet・indexerが成功Final、未検証Abort、旧手続きFinal、CertificationReadyを混同しない。
- 新profileごとの全体検証費、cold資料取得量、Node局所CPU／RAM／IO、最大証拠、public remedy、再起動・reorgを測る。
- 最低完全承認数1を新profileへ接続し、残る抽選m・全deadline・資源上限を既存の承認手続きで確定する。旧数値の流用を暗黙に行わない。
- Nodeの局所検査、global承認前提、資料義務の成立条件を別々にレビューする。
- 各有効化候補profileで16／19のWellFormed／Init／全Step／Endの網羅、偽関係のwitness構成、原programからの正規分解、native検査器の健全性を示す。Minerが宣言した`complete=true`や局所fixtureだけを根拠にしない。
- 少なくとも一つの対象実LLM・jobが実際に受理されること、正常Finalと一人反証・不供給Abortの全経路を同じbinary／設定で実行できることを確認する。全拒否で安全性だけを満たしても完了とはしない。
- 唯一の正直な選出者をcold状態から開始させ、正常Final、一人反証、選択的秘匿、遅延供給、多数不署名、最大合法虚偽caseを同じ実装で試験する。供給caseの全scope検査前提を客観化する具体機構が未定なら、profile procedure／正常完了は未確立と記録し有効化しない。
- model root単位のcache再利用とcold missを分離し、任意atomic batchのscope・全leaf coverage・合算予約・一回決済を試験する。batch最適化の未完成は単一claimの検証完成を偽装する理由にならない。
- registry／bond／claimの増分更新・期限索引・予約・抽選・復元を実測する。bootstrapでclaim固定後に未知のseedを供給し初回creditへ接続する経路、既存の報酬配分pinとADRの不整合の正本・端数・ゼロ分母を別途確定する。公開化と一人前提だけでこれらを解決済みにしない。
- 固定された実バイナリと一つの変更しないclaimで、受理から肯定・negative・未検証終了・決済までの対応経路を検証する。
- public activation／preset／fingerprint／releaseを別に確定する。本書はそれらを変更しない。

## 11A. EXEC生成権と成功Finalの追加境界

[07](0007-claims-exec-and-answer-accounting.md)と[10](0010-dag-consensus-and-native-settlement.md)が
認証仕事→期限付きExecCredit→future ExecSchedule→現在slotのEXEC_TXを担当する。
本書の成功Finalはcredit発行の必要条件であり、単独での十分条件ではない。
有効な認証、成熟、依存、資本・bond予算、仕事・権利の一回性、取消状態を07の単一gateへ渡す。
CertificationReady、旧版の手続きFinal、局所positive、未検証Abort、EXEC_SLICEから生成権を発行しない。

source claimの不正・認証取消は未使用creditと予約permitへ整合して伝播させる。
その取消だけで、すでに本体に受理された全取引を巻き戻す新規則を作らない。
未消費権利の失効、consumed provenance、canonical reorg、既存settlementの権利を区別する。

Final後の未解決caseは起源権利の一時holdとして07の適格性indexへ反映する。正規のcase解決でholdを解除する場合も、元expiry・既消費・二重発行防止・他の責任条件を再確認し、失効済み／取消済み権利を復活させない。同batchの取消と消費は§5.2を優先し、07／10のconsumed保全を先行消費の抜け道にしない。

認証用work anchor、claimの資料、公開異議、schedule commitは、既存EXEC creditや
producerサービスのon/offに循環依存させない。新creditが必要だから先にEXECが必要になる経路を禁止する。

## 11B. 受付の安全性と実モデルでの完了を分ける

[05](0005-bounded-resources-and-long-context.md)の全分岐資源計画を、原受理と原子的に確定する。
最長経路だけでなく、複数claim・全合法要求・共謀owner・cold outsider・再開の合算需要と
実供給に基づく予約が必要である。Nodeが全モデルを取得して予約不足を補うfallbackはない。

超過計画を拒否することと、目的のモデル・文脈長・jobを実際に受理して期限内に処理できることを、
別の完成判定として示す。EXECの目標1 BPSはcourtの実包含能力を証明しない。
同じbinary・設定・最低要件機材で混合負荷と供給失敗を検証し、
失敗時には成功Finalを止め、元の期限、未検証終了、未解決liability、保持・回復の条件を維持する。

## 12. 参照と判断根拠

[16](0016-miner-execution-commit-and-material-supply.md)はMinerのoriginalと供給義務、[17](0017-noninteractive-positive-verification.md)は非対話positive、[18](0018-interactive-negative-court-and-defaults.md)は対話negativeと帰責、[19](0019-model-free-node-and-bounded-evidence.md)はNodeの実行境界を所有する。規範を複数の文書で別々に定義しない。

比較資料: [Verde論文 §2](https://arxiv.org/html/2502.19405v1)は、重い独立実行と局所的なreferee裁定を分け、一人以上のhonest providerと必要情報へのアクセスを前提にする。[BoLD概要](https://docs.arbitrum.io/how-arbitrum-works/bold/gentle-introduction)は、単独の正直者による裁定と有界な係争時間を扱う。本書のモデル非保有Node、公開供給、資料case終結、選出の独立性、資源量をこれらの存在だけで証明したとは扱わず、外部VM・別鎖・新しい暗号計算proofを導入しない。
