# ADR-0228 — モデル登録で採掘試行に束縛した認証出力形式を要求する

<!-- FOW1-TB1-20261120-BEGIN -->
## 後続採択と要求対象の訂正 — 2026-11-20

今回改めて明示された要求は、識別用envelopeにとどまらず、**LLM関数自体の採掘出力**を別試行で非衝突にし、新しい機会に必要な計算を保証する設計である。以下のMOR1初版が要求を`O=(y,s,rho)`へ限定した解釈を、その要求全体の充足としては採用しない。

新規採掘は[ADR-0230](0230-palw-function-output-and-work-admissibility.md)／[RFC-0062 PALW-FOW1](../rfc/0062-palw-function-output-and-work-admissibility.md)へ、乱数生成は[ADR-0229](0229-palw-internal-threshold-beacon.md)／[RFC-0061 PALW-TB1](../rfc/0061-palw-internal-threshold-beacon.md)へ統一する。新方針は**ADOPTED_DESIGN_NOT_ACTIVATED**であり、FOW1のz・四Root Ticket・三つの必須gateを適用する。MOR1のreceipt定理は補助保証として保持する。

以下の旧判断で自然回答yの一致を許すことは、FOW1のzの衝突を無条件に許すことを意味しない。「失効させる節はない」という初版記録は、今回の明示的置換より優先しない。0062のactivation A以後は旧MOR1の新規採掘受付を閉じ、旧署名済みclaimの履歴・固定drainは保持する。対象LLMでの反復費用検査・本番実装・公開activationは未完成である。
<!-- FOW1-TB1-20261120-END -->

- 日付: 2026-11-20（Asia/Tokyo）
- 状態: **DRAFT_NOT_ACTIVATED**
- 対応RFC: [RFC-0060](../rfc/0060-model-registration-and-attempt-output-binding.md)
- 提案family: **PALW-MOR1**
- 依存する草案: [RFC-0059 / PALW-AN1](../rfc/0059-palw-an1-input-binding-and-panel-court.md)、[ADR-0227](0227-palw-an1-input-binding-and-panel-court.md)
- 記録: [モデル出力束縛改訂README](../maintenance/revisions/2026-11-20-model-output-binding/README.md)

## 背景とMOR1初版への指示（旧記録）

以下はMOR1初版の背景解釈である。今回の要求全体へは不足していたため、冒頭とADR0230で保証対象・登録条件・移行を訂正する。

ユーザーは、モデル登録時に「別の採掘試行の出力が天文学的な確率でしか重複しない構成」を要求する設計をRFCへ追加するよう指示した。対象はQwen等の自然言語出力を無理に毎回変えることではなく、実際の推論入力・出力・元実行traceに結び付いた、試行ごとに区別できる正式なプロトコル出力である。

既存AN1草案は、owner/nonceを含む事前登録文脈を実token入力へ結び付け、同じ試行の決定論的な再実行、三Rootに依存するTicket、Panelによる完全再実行とNodeによる局所Courtを定義している。MOR1はこの関係を引き継ぎ、登録仕様に認証出力の形式と共通の構造適合条件を追加する。

MOR1初版への指示は設計文書の追加を認めるものであり、その時点でAN1やMOR1の本番採択・native実装・公開activationを済ませたという指示ではなかった。今回の追加指示による新規採掘の限定的置換は冒頭とADR0230に従い、既存claimや既存署名の意味は保持する。

## 現行方針との整合

1. [RFC-v2-0003](https://github.com/MISAKA-BTC/misakas/blob/7ad72a9e0795586d50e0d1b9cbb99a222c500a33/docs/rfc-v2/0003-model-onboarding-and-artifacts.md)と[ADR-v2-0015](https://github.com/MISAKA-BTC/misakas/blob/7ad72a9e0795586d50e0d1b9cbb99a222c500a33/docs/adr-v2/0015-permissionless-model-registration.md)は、permissionlessな登録と、形式・構造・資源等の技術的検査を区別している。廃止済みのモデル資格審査を、非重複性の名目で復活させない。
2. [RFC-0053](../rfc/0053-additional-model-burn-pause.md)は、RFC-0052の独立した取得検査、定期可用性認定、HF可用性裁定を置き換えている。MOR1は取得検査員の署名、公開・gated判定、HFへのNodeからのアクセスを登録や復帰の条件にしない。固定revision/hashによる実bytesの同一性確認と、claim単位の資料義務は維持する。
3. 登録済みModel ID、artifact root、tokenizer、実行仕様等のimmutableな同一性を維持する。新しい入出力規則を既存の識別子へ上書きしない。
4. モデル非保有Node、Panelの完全再実行、単独の有効反証、未充足の資料・適格異議による成功停止を維持する。計算validity proofをFinal必須条件にする方式へ変更しない。

## 判断

### 1. 正式な出力を固定する

以下の正式出力は旧MOR1のenvelopeである。新FOW1のコア実行は`(y,z,tau)`、補助envelopeは`(y,z,s,rho)`とし、zの性質は0062の別gateで検査する。

MOR1に適合する登録profileでは、自然言語等のコア出力を `y`、試行IDを `s`、実行識別値を `rho` として、正式な出力を次に固定する。

\[
\mathcal O=(y,s,\rho).
\]

\[
\rho=H_{\rm receipt}
\left(\operatorname{Enc}(C,\sigma,s,R_x,R_y,R_\tau)\right).
\]

`C` はAN1で事前固定する完全な正規文脈、`sigma` はその登録から一意に解決される認証済み実行beacon値である。`C` の一部を抜き出した自己申告のcontextや、自由なsaltで代替しない。符号化、hash、domain、長さ、順序、型を登録profileで固定する。

自然言語の `y` が異なる試行で一致することは、正当な結果として許す。同じ試行についてMinerとPanelが再実行したときは、同じ `y`、Root、`s`、`rho` に到達する必要がある。「再実行すると別の出力になる」という意味の一回性は採らない。

### 2. 出力の非循環性を固定する

`R_y` はコア出力 `y` に対するRootとし、`rho` を含む正式出力全体のRootに読み替えない。`R_tau` は登録されたコア実行のtraceを対象とし、`rho` の生成をその同じtraceへ再帰的に含めない。登録文脈・profile IDの導出も、未来の `rho`、出力Root、trace Rootや自身の導出結果への循環参照を持たせない。

receiptはコア実行の後に計算する決定論的なenvelopeである。既存の同名Rootへ異なる意味を与える変更は、新versionの明示なしには認めない。

### 3. 登録検査を共通の構造適合条件とする

新規FOW1では以下に加え、0062のTheoremRuleと全適合predicate、関数の非衝突性、複数試行費用、checker完全性を必須とする。MOR1の構造適合だけで新しい採掘資格を得る経路は閉じる。

MOR1を選択した登録profileでは、認証出力の規則、実入力adapter、決定論的な数値意味論、生成・停止規則、trace/checker、全認証対象を必須の技術項目とする。適合性は、固定された規則と具体的なartifact bytesに基づいて検査する。

「大量の回答を試したところ重複しなかった」「著名モデルだから安全」「検査員が高品質と認定した」は適合根拠としない。モデルの評判、品質、保有者数、公開可用性、通常推論費用の審査委員会を追加しない。これらの審査を通らないモデルのpermissionless登録を一般的に禁止する変更でもない。

MOR1の採用profileについて、構造条件を満たさない出力をMOR1形式として受理しないことと、全モデルに新しい資格審査を課すことを区別する。対応しない旧profileがどのclaimを受け付けるかは、別途固定する移行規則に従う。

### 4. 試行情報を実際の推論入力へ接続する

AN1の固定されたbyteからtokenへの対応、challenge配置、attention mask、position、長さ、実入力読出しを維持する。challengeの切詰め、UNKへの集約、chat templateによる除去、無視した旧入力への置換を認めない。

回答を一度計算し、nonceだけを変更したreceiptを何個も付ける方式を、AN1/MOR1の正しい実行としない。ただし、異なる実入力が異なる自然言語出力や異なる全中間値を必ず生むとは主張しない。正しい最適化、cache、batch、外部計算の利用を、この規則だけを理由に不正やslash対象へ変更しない。

### 5. Ticketと一回会計を引き継ぐ

以下は旧MOR1の三Root式である。新規FOW1には0062の四Root Ticket・新domainを使い、この式を同じwireの意味変更で流用しない。EXEC/PANEL乱数もTB1に限定する。

MOR1はAN1のTicket式

\[
T=H_{\rm ticket}\left(\operatorname{Enc}(s,R_x,R_y,R_\tau)\right)
\]

を維持する。receiptを追加の抽選、追加の難易度判定、自由なnonce、再署名による再抽選に使わない。正しい同一実行から得られるTicketは一つである。

署名対象やclaim wireへ `rho` を追加する必要がある場合は、MOR1に対応する新しいcodec/profileで対象fieldと署名domainを明示する。旧署名から新fieldを暗黙に認証した扱いにしない。登録slot、試行、実仕事、用途別creditの既存の消費規則は引き継ぎ、別carrierや別envelopeで二重発行しない。

### 6. receiptの検査と計算の検査を接続する

Nodeは元claimの認証済み文脈から期待する `C`、`sigma`、`s`、Rootを決め、receiptを再計算する。提出者が自分で選んだ期待値との一致で受理しない。

正しいreceiptのhashを作れたことは、Rootが正しいモデル実行を表す証明ではない。偽Rootについてもhash自体は計算できるため、Panelによる元claimの完全再実行、期待するmodel rootと位置に対するopening、Nodeの局所Court、資料義務、Final gateを継続する。

モデルを持たないNodeに全体推論を要求せず、receiptをGroth16等の計算validity proofとして扱わない。少なくとも一人の能動的な正直Panel、必要資料、適格反証の包含・処理等の既存条件をreceiptによって解消したことにしない。

### 7. 非重複性と計算費用を別の主張として記録する

完全な異なる正規文脈を異なるreceipt入力へ符号化し、256bitのreceipt hashを古典ランダムオラクルとして扱うモデルでは、相異なる入力への総照会数を `Q` として、receipt衝突確率は

\[
\Pr[\mathrm{collision}]
\leq \min\left(1,\frac{Q(Q-1)}{2^{257}}\right)
\]

で評価する。`Q` には、正規参加者の照会だけでなく、攻撃者が提出せず捨てた候補への照会も含める。具体的なhash関数への適用は、その暗号学的安全性を別に前提とする。無限回の照会や無条件の数学的一意性を保証しない。

この結論は認証出力の非重複性に関するものであり、各試行に通常LLM推論と同額のGPU時間、FLOPs、電力、新しい有用仕事が必要という下界ではない。登録合格やreceipt一致を、その費用定理の代替としない。過去のFULL1の部分和生成量に関する下界を、AN1/MOR1のLLM費用へそのまま流用しない。

## 保持する制度と変更範囲

- 全モデル共通の容量・difficulty、120秒の制御epoch、外側全carrier合計1 BPS、追加モデル0.3／指定モデル1.0、年次forkの昇格経路、既存経済上限を変更しない。
- RFC-0053のadditional model burn-pause/resume、除外区分、登録owner、現在state、資料義務、原期限を維持する。MOR1対応を理由に既存pauseを初期化したり、認定証によるresumeへ置き換えたりしない。
- 重みbytesが同じであれば、そのcontent rootの再利用自体は妨げない。一方、adapter、出力規則、実行意味を変える登録bundle/profileには、その変更を表す新しいimmutableな識別を与える。既存Model IDが同じ内容・規則を指し続ける条件を壊さない。
- 既存claim、過去の署名、Root、credit、監査出力を新しい形式へ書き換えない。今回の草案に合わないことを遡及的な不正やslash理由にしない。
- mainnet後まで延期されている画像・動画生成やモデル改善経路を、本profile登録の入口から有効化しない。
- RFC-v2の20冊、ADR-v2の50冊という統合構造を維持する。0060/0228は統合後に追加した草案であり、既存採択仕様を暗黙に上書きしない。

## 代案と採らない推論

| 案・推論 | 判断 |
| --- | --- |
| 自然言語回答を毎回別にする | 正解が同じ仕事の有用性と衝突する。正式な認証出力を区別する |
| `y` の後ろへ自由なnonceを付ける | 実入力への束縛を失う。AN1のchallenge入力と元trace認証を維持する |
| 出力の重複が見つからないモデルだけを審査で採用 | 有限の観測は暗号学的非重複性の証明にならず、廃止した資格審査にもつながる |
| `R_y` を `rho` 込みの出力Rootとする | receipt生成との循環参照になる。コア出力とenvelopeを分ける |
| receiptから第二のTicketを作る | 一実行一抽選と共通difficultyを変えてしまう。AN1のTicketを保持する |
| receipt一致だけでFinalにする | 偽Rootでもreceiptは計算できる。Panel・Court・資料・Final gateを維持する |
| 非重複性からLLM費用下界を結論する | 別の性質である。費用保証は未解決の独立課題として残す |
| 既存登録IDの意味を新出力仕様へ変更する | immutable identityと旧claim検査を壊す。新profileと明示した移行を使う |
| Groth16等へ検証方式を変更する | 今回の出力形式追加の範囲外。AN1のPanel＋Courtを維持する |

## 検証・移行・失敗時の扱い

登録profileの実装では、同じ試行の再現、異なる文脈の正規入力、receiptの改竄検出、非正規符号化の拒否、三Rootの束縛、旧profileとの識別、二重会計防止を検査する。代表例が通ったことを衝突確率やLLM費用下界の実証としない。

有効化前に、具体的なwire codec、hash/domain、登録解決、署名対象、全claim consumer、legacy受付境界、最悪資源、材料取得、Court処理、reorg時の会計を固定して検証する。MOR1対応を宣言したprofileの形式不正を、旧形式への暗黙のfallbackで成功扱いにしない。既存stateの失効・移行・受付停止は別途採択された規則に従い、本草案から自動実行しない。

native接続、実LLM上の再現性と品質、実資料取得、ネットワークの包含・Court活性、DAG/settlement、複数試行の費用下界は、本文の非重複性定理だけでは完了しない。未完事項を新しい小さな誤差項として理由なく消去しない。

## 状態・根拠・既存文書への影響

以下の「失効させる節はない」はMOR1初版の状態記録である。今回ユーザーが明示した修正については、0061/0062とADR0229/0230の限定的置換・新規受付終了・互換drainを適用する。

本ADRはユーザーの設計追加指示に基づく**草案**であり、設計freezeを解除する本番採択決定ではない。承認済みの追加作業は0060/0228の設計記述と関連参照の追加である。依存する0059/0227も `DRAFT_NOT_ACTIVATED` のままとする。

本ADRが現時点で失効させる採択済みRFC/ADRの節はない。将来MOR1を採択する際は、適用profile、対象登録、開始点、旧claimの扱い、採択者・review記録、実装と公開activationの個別条件を記録する。単に文書番号が新しいことや文書へMUST相当の条件を書いたことを、旧仕様の変更・有効化の根拠にしない。
