# Positive Court R2 — 既存方針に対する対象限定の改定案

状態: Proposed。以下は採択時に反映する差分であり、現在のPRINCIPLES・RFC・ADR本文を上書きしていない。利用者による設計作成の指示と、全条項の本番採択は区別する。

## 変更が必要な箇所

| 現行の入口 | 現行の境界 | 新規claimに限る追加案 |
|---|---|---|
| PRINCIPLES §0.3、RFC14、ADR177 | モデル入手へ不介入。G14は正しいmodel保有に条件づける | 登録だけでは配布義務なし。一方Positive-required claimを提出する者は、当該全計算検査に必要な認証資料を開示することへ同意する。必要なら全weight相当となることを明記 |
| PRINCIPLES §0.4 | 検出確率下限をactivation gateとしない | 新Positive profileでは全関係被覆・条件付き見逃し上界・grinding評価をactivation必須にする。旧profileへ遡及しない |
| PRINCIPLES §8.3、RFC18 | local replayだけで抽選承認を代替しない | 抽選承認は残す。新型署名はNodeが同じ公開Courtを検査したscopeへbindできる。PositivePassedを全claim信用の追加の必要条件とする |
| RFC14 / RFC26 | 不一致発見からの紛争を中心に設計 | 正常計算Courtは不一致の発見なしで開始し、対象profile全件の検査を必須にする。既存紛争・direct refutationを弱めない |
| RFC26 / RFC28 / RFC31 | clock、応答機会と帰責の条件 | 正常Courtのcoverage不足を客観的未完了として扱い、計算有罪・qualified default・service障害を別結果にする |
| RFC08 / RFC12 / RFC18 | 既存認証・報酬・weightのread経路 | Positive-required claimは全consumerで追加predicateを読む。Final・cached receipt・native分岐での迂回禁止 |
| RFC23 / RFC30 / RFC36 / RFC37 / RFC40 | 未解決責任・保持・依存・IBD | Positive state・coverage・seed・資料保持と、その信用依存を同じrooted stateへ含める |

## そのまま維持する方針

- 暗号学的な計算validity proofを使わない。
- 個別モデルの公開性・保持者数を審査しない。正常Courtはclaim単位の契約。
- モデルID/rootの不変性、1.0/0.3、年次カタログ、資本配分の上限を維持する。
- Panel=0を有効化しない。画像・動画・モデル改善の延期を解除しない。
- 全block/全networkの共謀・永久検閲への耐性を追加したと主張しない。
- Minerが実GPU時間や電力を払ったことを検査結果に含めない。

## 注意：開示方針の本当の変更

「modelを公開せよ」という登録資格審査を復活させるのではない。しかし、通常のdense LLM claimを全範囲検査するには全weight相当が必要になり得る。したがって旧『モデル取得のための反復range要求を禁止』をそのまま残して、何も変更していないと主張することもできない。

採択時は、報酬を求める当該claimの必要範囲・総資源・署名された供給義務に限る例外を明記する。無制限にモデル全体を請求する攻撃用入口は設けない。受付前に必要範囲と費用をNodeが導出する。

## 新Gate C所見の追加案

旧`Reproduced`は意味・encodingを保存する。新rulesetでは別versioned所見`PositiveCourtReplayed`（仮称）を割り当てる。対象はcase ID、固定root群、plan/profile、branch、判定scope。署名者は同じ公開資料から同じ検査を実行する。

Nodeはその署名を『自分がまだ行っていない計算の代行証明』として信用しない。NodeのPositivePassedが無ければ、署名数にかかわらずcreditを拒否する。署名は既存Gate C参加・責任policyのために残す。

## 採択手続き

RFC0042・ADR0210の独立レビュー、明示採択、必要な原則節への限定追記、versioned wire/activation、全実装と測定、network別承認の順に進める。本成果物は最初の仕様案である。

## R2: 結果commit / 証拠commit / 対話契約

- Minerの新型claimは結果と固定計算条件へ署名し、全trace生成を必須にしない。旧trace必須claimは明示version/fenceで分離する。
- Panelが証拠用traceを再計算してrootへ署名する。Nodeは全関係を検査する。Panelの証拠責任をMinerへ包括転嫁しない。
- 取得できないmodelを登録しただけでは罰しない。新claimで引き受けたdata供給の有限な範囲だけを義務にする。実際に全weightが必要なら公開bytesを隠さず受付予算へ計上する。
- 対話要求はcanonical key/rangeで集約し、全Panel合計のMiner予算内。証拠生成用の計算要求は禁止。
- data応答を完了していれば、Panelのwitness未完成はMiner delivery defaultではない。private ACK欠如を違反証拠にしない。
- soundnessに必要な事前固定は廃止せず、Miner全traceの代わりにPanel witnessへ移す。

これはPRINCIPLESの承認済み改定本文ではない。採択前に原則文書・RFC14/26/29/35/39・SDKの適用対象を併せて更新する。
