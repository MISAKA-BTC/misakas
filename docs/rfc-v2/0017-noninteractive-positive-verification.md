# RFC-v2-0017 — 非対話positiveと独立再実行の承認

<!-- QF1-ADOPTED-20261120-BEGIN -->
## 2026-11-20 後続採択 — QF1のQwen3具体profile

**ADOPTED_DESIGN_NOT_ACTIVATED**。[RFC0066](../rfc/0066-qwen3-fow1-execution-and-readout.md)／[ADR0234](../adr/0234-qwen3-fow1-execution-and-readout.md)／[適用・移行](../governance/qwen-fow1-profile-transition.md)。QF1のpositiveは固定整数実行と四Rootの完全比較を対象にする。異なるbackendの近似一致や他Panelの証拠コピーを完全検査の根拠にしない。

QF1は36層Qwen3-8Bの整数A16実行、各予測位置の37 vector、HEX64実入力、固定Readout/四Root/Ticketを定義する。G-COLL/G-COST/G-USE、MAS1のUNKNOWN・BFT安全停止、Panel＋Courtを維持する。Qwen実機の品質・費用定理・native有効化は未完であり、旧claimやMOE2経済を変更しない。
<!-- QF1-ADOPTED-20261120-END -->


<!-- MAS1-ADOPTED-20261120-BEGIN -->
## 2026-11-20 後続採択 — MAS1の構造・実用性検査とBFT安全停止

**ADOPTED_DESIGN_NOT_ACTIVATED**。[RFC0065](../rfc/0065-model-admission-screening-and-bft-stop.md)／[ADR0233](../adr/0233-model-admission-screening-and-bft-stop.md)／[MAS1適用・移行](../governance/model-admission-safety-transition.md)を、この新規採掘profileの追加正本とする。正しい肯定Courtは実行検証の材料であり、G-COST、G-USE又はBFT停止解除を代替しない。

構造検査はPASS/FAIL/UNKNOWNを区別し、UNKNOWNは採掘不可のままBFT reviewへ接続する。G-EXEC/G-COLL/G-COSTを維持し、公開・共通の実用性G-USEと停止状態をAND条件へ追加する。BFTはモデル/ruleの停止と条件付き解除に限り、未知の費用定理の承認・台帳finality・fork choice・reorgの権限を持たない。burnやowner変更で安全停止を解除しない。

旧「モデル品質・モデル単位の検査を一切しない」という文言は、MAS1の採掘資格に関する今回の限定検査へ置換する。一般登録とMOE2市場原子生成、通常取引、coreの経済配分、Panel＋Court、計算validity proof不使用、旧claimの元責任は維持する。旧採択ブロック・原文・実測はその版の履歴であり、今回の数学的証明・native実装・公開activationが完成したという意味ではない。
<!-- MAS1-ADOPTED-20261120-END -->

<!-- MOE2-ADOPTED-20261120-BEGIN -->
## 2026-11-20 後続採択 — MOE2の30%購入・20%永久流動性・50%残余

**ADOPTED_DESIGN_NOT_ACTIVATED**。新経済profileの正本は[RFC0064](../rfc/0064-reward-buy-and-permanent-liquidity.md)／[ADR0232](../adr/0232-reward-buy-and-permanent-liquidity.md)／[移行契約](../governance/model-option-economics-transition.md)。ADDITIONALの対象worker報酬Rを `B=floor(300R/1000)`、`A=floor(200R/1000)`、`D=R-B-A` に分け、Bで購入したPositionの必要分をAと同じ市場へ追加し、余剰だけをburnする。全工程とDの支払を同じ原子的決済に結合する。

永久LPはnativeの非償還記録であり、withdraw・transfer・owner/管理者引出し・LP手数料請求を持たない。実資産は通常swapに使い、Aを旧locked seed下限へ加算しない。登録時の一意市場生成、coreの市場なし・Miner=R、通常AMM5%/3%・直接移転、TB1/FOW1は維持する。永久LP記録済みmodel IDのcore指定は禁止する。

旧MOE1の「50%を全額購入し、購入Positionを全retireする」は旧cohortの規則であり、MOE2の新規経路へ適用しない。以下の旧採択・原条文・測定はその版の履歴として保存する。
<!-- MOE2-ADOPTED-20261120-END -->


<!-- MOE1-ADOPTED-20261120-BEGIN -->
## 2026-11-20 採択 — 追加モデル限定MisakaOption経済MOE1

**ADOPTED_DESIGN_NOT_ACTIVATED**。全モデルで完全再実行を維持する。core Rから検証者の取り分を差し引かず、別の事前役務契約へ接続する。報酬率が正当性・費用の証明になるとはしない。

追加モデル登録と一意な市場の作成を同じ原子遷移とし、通常の登録済み・市場未作成状態やseed待ちを設けない。genesis・有効softfork指定モデルは市場を作らず、価格付け・共通上限処理後の対象worker報酬RをMinerへ100%配分する。追加モデルのみ50%買上げ、通常AMMはburn5%・owner総額3%・net残差、直接送受信はAMM手数料なしとする。

[RFC0063](../rfc/0063-additional-model-option-economics.md) / [ADR0231](../adr/0231-additional-model-option-economics.md) / [適用と移行](../governance/model-option-economics-transition.md)。新経済profileではこの後続採択を優先する。旧率・旧署名・原文引用・過去の実装試験はその版の記録として保持し、新規経路へfallbackしない。TB1/FOW1の正当性・乱数・Court・適格性条件を免除しない。
<!-- MOE1-ADOPTED-20261120-END -->

<!-- TB1-FOW1-ADOPTED-20261120-BEGIN -->
## 2026-11-20 採択 — TB1乱数とFOW1コア採掘値

**ADOPTED_DESIGN_NOT_ACTIVATED**。今回のユーザー指示により、新規PALW採掘の方式をFOW1＋TB1へ統一する。コア推論の採掘値zと補助receiptを区別し、登録系列の非衝突定理・複数試行費用条件・完全検査対応を採掘適格性に要求する。未完成のQwen定理や参照fixtureを本番合格としない。

TB1-PANELの正規選出者が登録された同じコア実行のy/z/trace全体を再実行する。rho一致、局所Courtのコピー、TB1 share多数だけで完全承認を代替しない。本人の物理的GPU使用の証明とは扱わない。

[TB1の正本](../rfc/0061-palw-internal-threshold-beacon.md) / [FOW1の正本](../rfc/0062-palw-function-output-and-work-admissibility.md) / [用途・置換・移行](../governance/palw-tb1-fow1-transition.md) / [全旧記述の適用台帳](../maintenance/revisions/2026-11-20-tb1-fow1/BEACON_AND_POLICY_COVERAGE.md)。新profileでは本節とこれら後続規範を優先する。以下の旧方式・旧Status・原文収録は、その版の履歴又は本改定で明示的に継承する範囲であり、新規受付のfallbackではない。旧受理claimの署名・責任・原期限は保持する。設計採択、native実装、定理完成、公開activationを区別する。
<!-- TB1-FOW1-ADOPTED-20261120-END -->

<!-- AN1-DRAFT-20261118-BEGIN -->
## 2026-11-18 AN1追加草案の担当範囲

**DRAFT_NOT_ACTIVATED**。AN1 positiveは全scopeの完全再実行責務を持つ。Panel nonceは元実行の入力nonceと分離し、正しい局所Courtの共有やACKだけを全体承認へ昇格させない。

[RFC-0059](../rfc/0059-palw-an1-input-binding-and-panel-court.md) / [ADR-0227](../adr/0227-palw-an1-input-binding-and-panel-court.md) / [改訂記録](../maintenance/revisions/2026-11-18-an1/README.md)。以下の既存採択本文・署名・過去snapshotは、その版の意味を保持する。本参照で実装・公開有効化を変更しない。
<!-- AN1-DRAFT-20261118-END -->


<!-- SWC1-PROPOSAL-BEGIN -->
> **2026-11-17 監査対応の設計案:** [RFC-0058](../rfc/0058-staged-work-commitments-and-consensus-eligibility.md) / [ADR-0226](../adr/0226-staged-work-commitments-and-consensus-eligibility.md) とRFC-0056・0057の追補は、実行commit、仕事の一回性、JOB／TICKET／PANELの段階別抽選、用途別の成熟・取消接続を扱う。状態は **PROPOSED_DESIGN_NOT_ACTIVATED**。既存原則・採択済み本文・旧claim・runtimeをこの参照で変更しない。新BFT乱数候補は無効、PoW費用下界とDAG定理は未証明である。[変更と検証](../maintenance/revisions/2026-11-17-work-binding/README.md)を参照。
<!-- SWC1-PROPOSAL-END -->

<!-- CPN1-PROPOSAL-BEGIN -->
## 2026-11-17 Court nonce CPN1の設計案

全体肯定の個別challengeと、既存のclaim共通coverageを分ける案。本人の追加条件が未完ならその承認を数えないが、他Panel全員の提出をFinal条件へ追加しない。正しい局所資料の共有と、独立再実行を証明できない境界を維持する。

本追記は **PROPOSED_DESIGN_NOT_ACTIVATED** への参照であり、既存採択・署名・runtime・activationを変更しない。[RFC-0057](../rfc/0057-court-nonce-and-panel-challenges.md) / [ADR-0225](../adr/0225-court-nonce-and-panel-challenges.md)。
<!-- CPN1-PROPOSAL-END -->

2026-11-11の後続採択: [RFC-0053](../rfc/0053-additional-model-burn-pause.md)／[ADR-0221](../adr/0221-additional-model-burn-pause.md)がHF取得可否の認定をburn停止・解除へ置換する。追加登録モデルだけを対象に、誰でもburnでPAUSED、登録者だけがburnでACTIVEへ戻す。genesis・年次fork指定は対象外。第三者取得証明・定期可用性認定・取得不能の多数決を要求しない。HF固定版/hash照合はoff-chainの実行資料同一性確認として残す。claim固有資料、再実行承認、不正反証と責任・原期限は維持する。PAUSEDは新規受付・成功Final・新規報酬確定を止める。状態は **ADOPTED_DESIGN_NOT_ACTIVATED**。

| 欄 | 内容 |
| --- | --- |
| 状態 | **ADOPTED_DESIGN_NOT_ACTIVATED** |
| 採択日 | 2026-11-08 / 単独反証と保証範囲の改訂 2026-11-09 |
| 決定記録 | [ADR-0215](../adr/0215-model-free-node-noninteractive-positive-and-adversarial-availability.md) |
| 適用 | 16のMiner実行commitを持つ新claim／検証profile |
| 正常経路 | 固定済み資料の取得、Panelの独立再実行、root照合、positiveの一括提出 |
| Node | モデル本体・全traceを取得せず、承認情報と有界な局所証拠を検査する |
| 実装・有効化 | 本文は設計採択。既存fixture・builder・署名型の存在を接続完了とみなさない |

[20本の索引](README.md) / [16 Minerと資料](0016-miner-execution-commit-and-material-supply.md) / [18 negative](0018-interactive-negative-court-and-defaults.md) / [19 Node](0019-model-free-node-and-bounded-evidence.md) / [20 Final](0020-verification-gated-final-and-adversarial-models.md)

追加採択: [RFC-0049](../rfc/0049-palw-execution-integrity-and-unilateral-refutation.md)・[ADR-0217](../adr/0217-palw-execution-integrity-and-unilateral-refutation.md)。本書のpositiveは全体再実行の責任付き承認であり、MinerまたはPanel本人の物理的実行履歴の証明ではない。単独の適格な反証による無効化を多数のpositiveが妨げないことを明文化する。

同日の追加採択: [RFC-0050](../rfc/0050-public-materials-and-one-honest-panel.md)・[ADR-0218](../adr/0218-public-materials-and-one-honest-panel.md)。公開資料を取得できるモデル未保有の独立選出者を含め、各claimに能動的な正直者が少なくとも一人いることを条件にする。20の新profile `PublicOneHonest`では完全検証承認の最低件数を`minimum_full_replay_approvals = 1`に定め、十分な全体検査・異議期間と未解決caseの処理を必須のまま維持する。旧profileの75／41その他の承認規則・旧署名は変更しない。

## 1. 採用する正常経路

Minerが元の結果とcanonicalなtrace／execution rootを先に署名固定する。
独立に選出されたPanelは同じjob・モデル・入力・実行規則で再実行し、元のrootと出力を照合する。
一致したPanelは、自分の記録から元Minerのrootに対するopeningを作り、positive資料と承認を一括提出する。
Minerとの中間値の逐次照合、正常時の二分探索、Minerの再署名を、positive完成の条件にしない。

ここでいう非対話は、証拠内容を決めるための逐次質問・応答を正常経路に要求しないことをいう。
モデル・入力の配信、固定ファイルの取得、公開乱数の確定、証拠の搬送は残る。
一括提出は一つの論理objectとしての固定を意味し、サイズ上限内の複数part・複数blockで搬送することを妨げない。
partの受領確認は輸送の処理であり、計算上の一致点を探索する対話ではない。

この新profileでは、選出Panelによるclaim全体の完全再実行を肯定承認の基準にする。
これは2026-11-08の限定改定であり、Nodeに完全再実行を要求せず、旧profileの検査scopeも変更しない。
部分再実行・確率的検査の既存receiptを、この完全再実行承認へ読み替えてはならない。

## 2. 三つの異なる結果

| 結果 | 誰が何を確認するか | 保証の範囲 |
| --- | --- | --- |
| 独立再実行の肯定承認 | 選出Panelがclaim全体を再現したと署名する | 単独の署名だけでは全体の正しさを保証しない。新profileの安全性は一人の能動的な正直者と資料・検査・反証経路を合わせた20の前提に依存する |
| 局所positive court | Nodeが認証済み入力から一つの指定関係を計算する | その計算関係が元traceの値と一致する |
| 成功Final | Nodeが承認・局所検査・公開異議・依存・期限等の条件を適用する | 20のnative状態遷移と責任の条件 |

一つの正しい計算関係は、未検査部分を含むclaim全体の正しさを証明しない。
root、署名、opening、Panelの多数決を、全モデル計算の暗号学的証明と呼んではならない。
Nodeが直接検査する局所positiveと、Panelの完全再実行に対する承認を別の型・scopeで記録する。
全選出検証者が悪意の場合、小さな局所証拠と承認署名だけで全体の虚偽を必ず検出できるとはしない。

### 2.1 「計算証明」と呼ぶ対象を限定する

本profileが検証する命題は、16の固定された原実行に対して、規定の初期状態から規定の命令・読出し・状態更新・停止・出力・仕事量を満たすcanonical実行があること、およびその主張に破れがあれば19の有界な局所関係で反証できることである。全体の肯定は本書の承認と20の条件付き安全性に従う。

正しい結果やtraceを提出できることから、Miner本人が今回計算したこと、各Panelが独立にCPU／GPUを使用したこと、電力・時間・実機回数を費やしたことを導かない。コピー、過去の同一実行の再利用、外注、共同計算は、署名・openingだけでは排除できない。claimごとの仕事・creditの一回性は07のnative台帳で検査するが、それを物理的実行履歴の証明と呼ばない。

`FullCanonicalReplayExact`相当のscopeは、署名者が果たすべき検証責務を表す。Nodeがその履行を暗号学的に確認したというscopeへ拡大しない。commit/revealや追加の無作為検査は後出し変更・局所不整合の監査であり、独立実行や非共謀の証明へ変換しない。

### 2.2 肯定と単独反証を別の条件にする

所定の肯定承認数は成功の必要条件であり、反証を受理するための必要条件ではない。正直な選出者または18で正規に受理された適格な第三者一人が元claimへの有効な反証を提出した場合、native courtが確認した`ComputationRefuted`を、残り全員の肯定署名、veto閾値、局所positive、既存の`CertificationReady`で覆せない。

本書の承認閾値は正常時の受理と可用性を管理する。単独反証の安全性は、承認多数の正直さからではなく、16・19の局所完全性と18・20の資料・受付・裁定条件から導く。正直な人が存在するだけで十分とはせず、その人の期限内の取得・notice・証拠・包含を20の前提として明示する。

今回の新profileでは、完全検証承認の最低条件は、適格な選出bond一主体による有効な承認1件である。同一bondの重複抽選位置や署名再送を別人の検証と数えず、1件を得たことからPanelの担当人数mや全員の検証責務を減らさない。必要な局所positiveの被覆、資料供給、全体検査・異議期間、Qの解消、依存・費用・期限を省略してはならない。

正常claimについて、正直な一人が全体検証・承認を完了し、他の成功条件と全合法caseの客観的終結が元期限内に成立するなら、残りの不署名だけでは成功を妨げられない。誤claimについては、最初の承認が悪意者によるものでも、正直な一人の取得・全体検証・適格notice／反証の期間と保護経路が先に確保されているため、1件の承認だけでは成功できない。誰が正直かをNodeが特定できるという意味ではない。

この条件付き正常完了と、service障害・検閲・資料不足時の安全な未検証終了を分ける。正直者が承認できない場合、承認0件を時間経過で1件へ補完しない。正当なclaimでも資料・検査・包含・裁定の前提が破れればAbortとなり得る。旧75／41の多数承認を用いるclaimには、その旧版の不署名による失敗条件が残る。

## 3. 検証者の選出と独立性

抽選は、16の元claimが固定された後の乱数を用い、モデル保有者の名簿ではなくglobalな適格bond poolから行う。
producer自身のoperatorの除外、成熟したbond、元のpool snapshot、draw record、attempt上限を維持する。
元claimに対する抽選を、取得失敗、Panelの無回答、nonce・結果の差替えで都合よくやり直してはならない。
重み付き復元抽選の位置数を、互いに独立した運営者の人数と同一視しない。

モデルを事前に持つことや、Minerの非公開APIへアクセスできることを、選出資格にしてはならない。
Panelの過去実績・署名数・モデル所有者からの推薦だけで、独立性や正直さを認定しない。
global poolの範囲、承認閾値、抽選の相関・攻撃者比率、参加・取得可能性の前提は20に従う。
ここで別のBFT委員会や固定された特権名簿を導入しない。

各選出者は、供給された値を認証し自分で再実行する責任を負う。
他のPanelのrootや「一致した」という署名をコピーして、完全再実行したとの承認を出してはならない。
Nodeが各署名者の物理的なCPU／GPU利用を観測・証明できるという意味ではない。

モデル未保有は不適格の理由ではない。受理計画は、選出後に16の公開検証パッケージを取得する場合の計算・通信能力と時間を含む。特定モデルの所有者集合や私的API利用者へpoolを狭める、取得失敗者をその集合の味方へ置換する経路を認めない。能力profileを別途導入する場合にも、そのprofileを担当する最終的な各Panelに一人の正直者がいるという前提と独立選出を満たす必要があり、global poolに正直者がいるだけでは代用できない。

「Panel全体に一人は正直」から、「任意の一人だけが全体を検査すればよい」「各人が別layerだけを検査すればよい」とは導けない。担当者全員に元claim全体の完全検証責務を課し、少なくとも一人の正直者が全必須関係を検査できるcoverageを維持する。mを減らす場合は変更後の各Panelについて前提を再評価し、区間ごとの担当へ分割する場合は各区間の正直なcoverageを別途定義・採択する。本改訂はその省略や新mの具体値を承認しない。

20の任意の有限なatomic claim-batchを使う場合、全leafの対象・scope・入力・出力・traceをbatch rootへ先に固定してからPanelを抽選する。一人の正直者がbatch全体を検査できる計画を要求し、batchの承認を一部leafだけの検査で作らない。選出済みPanelへ後から新leafを足して同じ承認を流用せず、既に独立して受理された別claimのCaseId・時計・権利をbatchへ吸収しない。

## 4. Panelの処理

### 4.1 元の実行条件を認証する

Panelは、チェーンに受理された元claim・登録物・job・profile・drawから期待値を得る。
peerが送った資料の中だけで、期待するmodel rootやinput rootを決めてはならない。
元Minerの署名、original execution root、leaf数、output、規則版、依存範囲を確認する。
モデル・入力・program・必要な依存資料を16の経路から取得し、使用前に認証する。

### 4.2 同じ元contextで完全再実行する

Panelは元jobのcontextを使用する。Minerの識別子をPanel自身の識別子へ置換してtraceを計算しない。
同じrecord位置・順序・dtype・丸め・符号化・木の構成で、中間値とrootを生成する。
実行対象には、必要な初期状態、tensor接続、履歴・KV、非線形演算、生成・停止規則、出力と仕事量の関係を含める。
入力再利用がある場合は、依存rootと担当範囲を検証し、依存claimのcredit条件は20で別に確認する。
サンプリングした数箇所や一部のlayerだけの再実行を、完全再実行の完了にしてはならない。

### 4.3 rootと出力を照合する

Minerの元rootをR_M、Panelが同じ形式で作ったrootをR_Pとする。
PanelはR_P = R_Mに加え、構成root・leaf数・元context・宣言されたoutputとの結び付きを確認する。
比較対象はcanonicalな計算trace／execution rootであり、署名者を含む外側の封筒IDではない。
「値が近い」「最終出力だけ一致」「大部分のrootが一致」を、完全一致の代わりにしない。

### 4.4 自分の記録から局所資料を作る

同一の元context・canonical構成でrootが一致した場合、Panel自身の木から作ったopeningを元Minerのrootに対して検証できる。
通常のhashのbinding前提を使い、Minerへ個別中間値の開示や再承認を要求せずに組み立てる。
実際に提出する各openingも元Minerのrootに対して確認し、Panelの別rootを検証対象へ代入しない。
この方法では正常時にMinerの全中間値を転送する必要はないが、モデル・入力の取得とPanelの再実行費用は残る。

### 4.5 署名して一括提出する

Panelは、再実行した対象とscope、局所検査計画、資料digestを拘束した承認を署名する。
同じ元claimに対する必要な局所資料を、canonical順序で一つの論理bundleに固定する。
搬送中のpartの差替え、他Panelの異なるbundleとの混合、欠落したpartを成功flagで埋めることを拒否する。
通常経路はMinerのその後の応答を待たず、Nodeが完成したbundleを検査できる形にする。

## 5. 局所positiveの計画と証拠

Nodeに全traceの算術検査を要求しない。Nodeが確認する局所関係は、受理された有界の検査計画で決める。
計画は元program・job・profileから導出し、MinerやPanelが都合のよい演算だけを選べるものにしない。
局所箇所を無作為に選ぶ場合は、Minerの元commitより後の公開乱数とclaimに束縛する。
独立再実行の承認に使う抽選と、局所検査箇所の抽出は目的・domainを区別する。

計画は、対象空間、抽出規則、必要な局所証拠数、必須の境界検査、CPU・RAM・byte上限を明示する。
正数の局所証拠数とその上限はprofileに固定し、この文書で未測定の既定値を発明しない。
計画・乱数・必要資料・搬送資源を確定できないprofileを、成功対象として有効化しない。
commit前に予測可能な自己選択箇所や、Minerがnonceを探して選んだ箇所を無作為検査と称してはならない。

必須局所検査は、元claimと検査計画から導出されるcanonicalな対象集合の被覆として判定する。独立承認の抽選位置数m、閾値k、署名数を、局所検査の対象数へ換算しない。同じ対象の証拠を複数の選出者が提出しても、別の未検査対象を満たしたことにはならない。同一資料の共有・重複排除は許すが、各選出者の独立再実行責任は省略しない。

一つの局所資料には、次を含める。

- 対象claim、元execution root、検査計画、箇所・範囲・位置。
- 登録programに認証された演算種別、shape、dtype、丸め・接続。
- 元traceに属する入力と、元モデルに属する必要な重み断片。
- Minerが主張した対象出力と、各値のopening。
- 必要なprompt／生成ID／prefix等の局所認証情報。

Nodeは、資料の認証、必要範囲の過不足、canonicalな演算と資源上限を確認して局所再計算する。
入力の認証と局所的な一致は、未検査の前段全部が正しかったという保証ではない。
検査不足、資料不足、未対応演算、上限超過をLocalHoldsへ変換してはならない。
詳細なNodeの検査契約・保持・cold replayは19に従う。

無作為な局所検査は補助監査であり、任意の誤ったclaimを所定確率で必ず検出する保証を、この節の存在だけから得ない。N箇所のうち一箇所だけが不正なtraceを重複なしにk箇所抽出する例では、検出率はk/Nにとどまる。Freivalds等を使う別の検査scopeも、対象演算・認証済み入力へのアクセス・乱数の順序・数値規則・誤差を別途固定し、単なる一部のpassを本profileの完全再実行承認へ読み替えない。

## 6. 承認とbundleの論理schema

以下の名称は仕様上の区別であり、実装済みwire型や署名domainの名称ではない。

| Object | 必須のbinding／内容 |
| --- | --- |
| 独立再実行承認 | chain・claim・元commit・model／program／input／output／execution roots・profile／ruleset |
| 同承認の検証scope | FullCanonicalReplayExact相当の新しい明示scope、対象job、依存範囲、照合した元root |
| 同承認の選出情報 | pool／draw record、選出bond、operator、署名鍵、positionの数え方、期限 |
| 同承認の結果 | 完全再現一致、再現不一致、取得不能、未完了を区別したverdict |
| 局所証拠bundle | 計画・公開乱数・canonical target一覧・各局所資料・総数・総byteとwork・bundle digest |
| native受理record | 検査済みの局所scope、実際の署名・選出・期限の確認結果、元claimへの参照 |

承認verdictの自己申告だけでNodeの検査結果を作らない。
Nodeは自分の受理済み状態から期待するclaim、draw、profile、計画を解決して照合する。
重複署名、別claimへの移植、別scope、別profile、失効したbond、期限外の提出を規則どおり拒否する。
同一bondが複数の抽選位置を占めるときの票数は、実際のdraw policyから算定する。
署名を集約したというだけで、選出・scope・各承認の責任を省略してはならない。

既存のPositiveCheckedは完全な公開Court資料の検査を意味する。
これを「小さな局所positiveを一件確認した」「他者のrootと一致した」という意味へ再利用しない。
新しいscopeとverdictは、新しいcodec・署名domain・ルート別の検証規則で導入する。

## 7. 分岐と失敗の扱い

| 状況 | Panelの結果 | 次の処理 |
| --- | --- | --- |
| 完全再実行、元root・出力、必要な局所資料がすべて一致 | 肯定承認候補 | Nodeの検査後、20の成功条件へ |
| 元rootまたは出力が不一致 | 再現不一致 | 18のnegativeへ進む。FOW1・QF1は元claimに対する直接反証を提出でき、対話による局所化は任意 |
| 最終出力は一致するがtraceが不一致 | 再現不一致 | root一致の正常経路を通さない |
| 必要なモデル・入力・依存資料が不足 | 取得不能／未完了 | 肯定承認を出さず、18の適格MaterialRequestを開始。未解決なら16・20の有限終了へ |
| Panel自身が計算・搬送に失敗 | 未完了 | Minerの計算有罪とはせず、契約に従う |
| 提出した局所資料に不正なopeningや別scopeがある | 証拠拒否 | 成功に数えず、署名者の責任は18の要件で判断 |

root不一致だけでMinerをslashしてはならない。Panel側の誤りもあり得る。
negativeの局所化では元Minerのrootに対して開示を認証し、Panelの正しいtraceをMinerの誤ったtraceに見立てない。
既に元claimへの反証が完成している場合は、positive全部の完成や不要な対話を待たせない。
適格な第三者のnegative提出を、選出Panelの肯定署名によって閉じてはならない。

選出者が再現不一致を検出した場合は、期限内に18の署名付き不一致noticeまたは直接反証を提出する。署名した不一致verdictだけで計算有罪にはしないが、必要な資格・元root・比較root・座標または区間・費用・個別予約を伴うnoticeを、negative票の不足を理由に拒否しない。取得不能の場合も、適法なcanonical requestを18の同じ有限経路へ提出する。私的な「届かなかった」という申告だけをnativeの未履行記録にしない。

## 8. 正常系の非対話性と資料不足

Minerが最初に必要な局所資料をまとめて供給し、Panelがその資料を検査して提出する方法も許す。
ただし、その局所資料の確認だけでは、完全再実行の肯定承認を出せない。
完全再実行を終えたPanelが記録済みの値・Merkle tree・認証済み重みを再利用することは許す。

必要資料がそろわないときは、追加の供給・公開要求が発生し得る。
これは正常positiveの算術的な対話必須化ではなく、16の資料供給契約の履行・回復である。
未履行をpositiveと見なす例外、事前にモデルを持つ味方への置換、Nodeによるモデル取得を設けない。
適格要求の未解決状態、提出機会の欠如、期限後の処理は18・20に従う。

## 9. Finalへの接続

局所positiveを一件受理したこと、肯定署名を一件得たこと、対話がなかったことではFinalにしない。
所定の独立検証承認、必要な局所検査、公開異議、資料義務、依存関係、費用・責任・期限の条件を20で合わせる。
元claimの固定後に選出と認証を開始し、認証開始のために成功Finalを要求する循環を作らない。
旧版のFinal開始型Gate Cと新版の順序を、同じwire／状態値へ黙って混在させない。

本profileの保証には、当該claimを担当する独立選出Panelに能動的な正直者が少なくとも一人いて、その人がモデル未保有でも取得・全体検査・提出を完了できるという前提がある。多数の正直な肯定票を必要条件にはしない。モデル固有のMiner・Panelが寡占されても、その保有者名簿を選出母集団にしない。
悪意者の形式が整った承認1件を、Nodeが署名だけから虚偽と識別する保証はない。したがって、その提出で全体検査・異議期間を短縮したり、18の適格要求を締め切ったりしてはならない。全員共謀、正直者の全通信検閲、必要な検査・裁定機会の不成立を「1件の承認がある」「局所positiveがある」「異議がない」ことで解消したと記載しない。旧多数承認profileの確率評価を、この一人保証の評価へ流用しない。

一方、悪意側が肯定閾値を満たしても、18の未解決集合`Q(C)`に適格な一人の異議または資料要求がある間は成功不可である。肯定の追加・集約・差替えで`Q(C)`を消さない。`ComputationRefuted`が成立したclaimは20の`Refuted`／`Revoked`へ進み、肯定の再提出で復活しない。同じblock／canonical処理バッチ内の異議・裁定・期限・Finalの順序は20に従う。

承認不足、資料義務未成立、必要な局所検査未完了を、異議なし・対話終了・hard到達で補完しない。必要条件の欠落が期限までに解消しなければ`AbortedUnverified`であり、実行済みの役務費を除く成功仕事報酬・仕事creditは生まれない。

## 10. 費用・期限・実装接続

対話の削減は、Panelの再実行・資料取得・証拠作成の費用削減を自動的には意味しない。
cold取得、canonical再実行、全traceの記録・hash、局所opening生成、搬送、再起動時の回復を別々に測定する。
同じ内容のcacheや一回の再実行からの複数局所証拠生成は許すが、Nodeの上限を越える全資料転送へ置換しない。
positiveの予約がnegativeと第三者の異議枠を使い切らないよう、claim全期間の資源を先に予約する。
再実行の完了、公開乱数、追加要求、Panel交代から時計を再開しない。元の期限と最遅異議の余裕は20に従う。

既存のreplay_and_capture、prepare_parts、build_partはローカル記録からの資料構築の足場になる。
ただし旧R2のPanel自身のwitnessと、新版の元Minerに束縛した資料は同じ責任ではない。
実装時には元Miner rootとの明示照合、新しい署名scope、Nodeの局所検査、20のFinal条件への接続を追加する。
旧[RFC-0044](../rfc/0044-positive-court-full-coverage.md)のNode全被覆検査を本profileのNodeへ引き継がない。
旧[RFC-0045](../rfc/0045-positive-court-canonical-assignment.md)の仕事支払い条件や、[RFC-0046](../rfc/0046-positive-native-funded-relay.md)の資料搬送・custody同意を、合意creditの承認へ転用しない。
既存の[Positive Court実装状態](../design/palw/positive-court/IMPLEMENTATION-STATUS.md)は、その版の記録として読む。

## 11. 必須の受入確認

以下は新版の受入条件であり、実行済みのPASS報告ではない。

| ID | ケース | 必須結果 |
| --- | --- | --- |
| P17-01 | Minerのcommit後、追加応答なしで正常資料が得られる | Panelが再実行・root照合・局所証拠生成・一括提出を完了できる |
| P17-02 | 元Miner rootと同じ木をPanelが再構成する | Panelのopeningが元Miner rootに対して検証できる |
| P17-03 | Panelの署名者IDで元job contextを書き換える | 元claim不一致として拒否する |
| P17-04 | encoding／dtype／丸め／leaf順序が異なる | 近似一致を採用せず、不一致または不正形式として処理する |
| P17-05 | 出力だけ一致し、中間traceが異なる | 完全再現のpositiveを出さない |
| P17-06 | 別claim・別draw・別scopeの承認を移植する | 署名・binding・受理recordで拒否する |
| P17-07 | 一部のlayer／局所資料だけを検査する | 完全再実行scopeの承認に数えない |
| P17-08 | 都合のよい局所箇所やcommit前乱数を選ぶ | 受理された検査計画に一致せず拒否する |
| P17-09 | 必須part欠落、上限超過、private timeout | 成功に数えず、Nodeの全モデル取得へ移らない |
| P17-10 | モデルを隠したMinerと通常Panelが結託する | global抽選・公開要求を迂回できず、実際に検証できない正直な選出者は承認しない |
| P17-11 | root不一致だが誤っていたのはPanel | root不一致だけでMinerをslashしない |
| P17-12 | 全選出者が共謀し、検査箇所以外に誤りがある | 局所検査だけの保証限界を示す。認証前提破綻を無条件安全性のPASSにしない |
| P17-13 | Nodeにモデルcache・GPU・モデル配布先への接続がない | 正常bundleの検査と局所裁定を実施できる |
| P17-14 | restart／reorg／旧claimとの共存 | 元の版・scope・draw・未解決異議・期限を復元し、二重承認・時計再開を防ぐ |
| P17-15 | 同一局所証拠を必要承認数だけ重複提出し、計画の他対象が未検査 | 承認位置数を局所coverageへ換算せず、CertificationReadyにしない |
| P17-16 | 悪意者が肯定閾値を満たし、一人が適格な不一致noticeを提出 | negative票数を要求せず`Q(C)`で成功を保留する |
| P17-17 | 全員の肯定と有効な直接反証が同じcanonical処理バッチにある | 20の順序で反証を適用し、成功・creditを生成しない |
| P17-18 | 正しい資料をMinerから受け取り再計算せず署名する | Nodeが独立実行の有無を判別したという保証を付与しない。資料の整合性と署名者の責務を区別する |
| P17-19 | 反証で無効化されたclaimへ肯定bundleを再送する | 元subjectの無効状態を維持し、認証・報酬を復活させない |
| P17-20 | 本来の独立承認が不足したままnotice窓またはhardへ到達する | 異議なし・時間経過を承認へ変換せず、規定の未検証終了にする |
| P17-21 | 唯一の正直な選出者がcold取得から正常な全体検証を完了し、残りが不署名 | 本profileの1件承認を満たす。他条件・全case解消後に正常Finalへ進み、不署名だけで失敗させない |
| P17-22 | 悪意者の承認1件が検査期間の早期に届く | 検査・異議期間を短縮せず、正直な未保有者の取得・反証機会を維持する |
| P17-23 | 75人中一人の正直者がいるが、その人を含まない3人だけに全体検証を任せる | 元の一人前提を満たすcoverageの代用と認めない |
| P17-24 | 各Panelが別layerを検査し、正直者の担当外に不正がある | 全体完全検証scopeを与えず、区間ごとの保証なしに分割を採用しない |
| P17-25 | 他のPanelが偽の有限caseを最大数出し、正しいMinerが適格に応答する | 18の全case終結・包含・予約の前提の下で元期限内に解決し、1件の正直な承認から正常Finalへ進む |
| P17-26 | 旧75／41の署名または旧閾値policyを新profileの1件承認へ移植する | profile・scope・domainの相違として拒否し、旧claimの意味を変えない |
| P17-27 | batchの一部だけ一致、または抽選後にleafを追加する | batch全体承認にせず、固定scopeへの不一致として拒否する |
| P17-28 | 正直な一人を含むbatchだが、その人が全leafを検査する時間がない | 受理計画を拒否し、1件承認やbatch rootの小ささで計算容量不足を隠さない |
| P17-29 | 全体検証承認0件で、他の条件のみが成立する | 成功Finalにせず、期限内の回復または未検証終了 |

本profileの完全検証承認の最低件数`minimum_full_replay_approvals = 1`は今回の採択値である。担当者数m、局所検査数、batch上限、資源・期限への具体的割当は、20の版付きpolicyと受入計測で確定する。最低件数の変更だけでmや抽選枠・全体再実行数を減らしたと扱わない。
未承認の残りの値を本文だけで補ったり、旧claimのpolicyを書き換えたり、休眠のfenceを有効化したりしてはならない。
