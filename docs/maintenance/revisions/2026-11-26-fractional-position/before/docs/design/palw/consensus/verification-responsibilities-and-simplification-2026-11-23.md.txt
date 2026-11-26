# PALWの検証責務、現在の不足、簡素化の境界

作成日: 2026-11-23。対象は添付misakas-pre (2).zip、ZIPの基準commitは7dddad7298d3a45e5ba07b6047cb32863c7540ce。
SHA-256: eccf17e60a485caed0c5936fd16a07b7ac3c3e7064f9c20d6259e76b1e77eb3e。
これは静的なコード・文書監査であり、現在のMac上のHEAD、実ネットワーク又は未添付commitを確認したものではない。

## 1. 判定

**Panel/Court等の役割には理由があるが、現在の細分化が唯一の最小構成とはいえない。通常受賞経路と適格性の不足が残り、実装全体を必要十分と認定できない。**

LLMの正しい出力を安く予測できないという仮定を置いても、提出された任意の文字列がその正しい出力であることは別途確かめる必要がある。モデル非保有Nodeと、計算validity proof不使用という選択の下では、モデルを持つ検証者と局所反証の役割が残る。

今回のユーザー変更は、モデルを採掘へ採用する入口を簡単にする。個別claimの正しさ、当選、権利消費をモデル承認票で置き換える変更ではない。

## 2. Bitcoin型PoWとの正確な差

Bitcoin型PoWでは多数のnonceを探索することが高価であり、提出された一候補のハッシュを再計算することは安い。原論文§4も、候補の検証に単一のhash評価を使う構成を説明している。[一次資料](https://bitcoin.org/bitcoin.pdf)

PALWでF_M(x)=(y,z)として、T=H(x,y,z,root)を採掘判定に使う場合、hashのtarget判定だけなら安い。しかしNodeがF_Mを評価せず、局所反証も承認も使わなければ、攻撃者が適当なy,z,rootでtargetを満たすまでhash探索した提出物を排除できない。F_Mの真の出力が予測困難でも、この経路は別問題である。

すべてのNodeがモデルを保持し、当選候補について一回の推論を完全再実行する方式なら、Panel委任に由来する機構の一部を減らせる。しかしその場合はNodeの費用、全モデルの保持・取得、数値再現性、処理量の条件が変わる。今回、そのアーキテクチャ変更は採用していない。

反証を使う方式の一般的な資料として、Al-Bassamほかの[Fraud and Data Availability Proofs](https://arxiv.org/abs/1809.09044)がある。正しい反証の存在と、その資料・搬送・検証機会の確保は異なる前提であり、同論文がそのままPALW全体の安全性を証明するものではない。

## 3. 各機能の必要性

| 機能 | 防ぐ問題・保証対象 | 今回の扱い |
| --- | --- | --- |
| 不変モデル・kernel契約 | 別重み、別数値意味、未知演算へのすり替え | 共通技術条件として必要 |
| 採掘採用 | 安価な正当計算・非実用モデルを採掘へ入れるか | 二経路に変更。BFT経路は判断を信頼し数学的保証を要求しない |
| Ticket | 一試行の当選判定・再抽選防止 | 必要。ただし正しい実行へのbindingが別に必要 |
| Panel完全再実行 | Nodeが持たないモデルで個別claimを検証 | 現アーキテクチャで維持 |
| Court・資料case | 嘘の承認、異議、資料未充足を決定論的に処理 | 現在の公開反証方式で維持。任意対話は必須にしない |
| Final | 個別claimの期限と未解決caseを閉じる | 状態遷移として必要。単独で全合意資格を意味しない |
| 独立Gate C | claimへの合意creditの別認証 | 現行規則では維持。数学的に唯一の構成ではない |
| 当選権利・仕事キー | 同じ仕事・同じ権利による再発行・再使用 | 非同期に仕事とブロックを分ける現方式では必要 |
| ブロック検証・UTXO・reorg | 選択鎖・支払い・消費の整合性 | 通常のチェーンにも必要 |
| MOE2市場・永久LP | 採用した経済配分 | LLM-PoW一般の必須要素ではないが、この製品仕様では必要 |
| 旧profile互換・観測drill | 過去claimの保全・試験 | 新しい採掘の安全条件とは区別して保守する |

「予測困難なLLM出力」という一つの仮定だけから、Gate Cを含め今の全モジュールが必然だとも、全部不要だとも結論しない。役割、故障前提、実装の分割数を別々に評価する。

## 4. コードで確認した不足と過剰分割の候補

### 4.1 通常受賞の未接続

- consensus/src/pipeline/virtual_processor/utxo_validation.rs:2304 は、予約・価格情報の解決後もPrivateMoe2AwardUnresolvedを返す。
- consensus/src/pipeline/virtual_processor/processor/palw_candidate_v3.rs の通常foldは、private_moe2_awardへNoneを渡す。
- consensus/core/src/palw_state_v2/private_moe2_settlement.rs:244 は、Qf1OriginalVerification::ReplayApprovedでもsuccessful Finalがないとして拒否する。
- misaka/palw/remote/src/tb1_exposure/qf1.rs:24 は、default Assessmentと固定falseでNewMiningを拒否する。
- consensus/core/src/palw_mas1.rs は六項目を全PASSへ畳む旧一経路であり、BFT_DECISIONの採用basisはない。

新しいorigin別管理、genesisへのBFT制御拒否、BFT forkの追加・停止も今回の文書設計であり、今回このnative経路を実装したとは扱わない。

これらの拒否は、未認証の支払いを防ぐ現在の安全な停止でもある。「拒否が残る」ことを既に不正支払いが可能な脆弱性と混同しない。正しい入力の解決と状態遷移を実装する必要がある。

### 4.2 Gate Cはモデル採用とは異なる

consensus/core/src/palw_security_certification_v1.rs は、FinalとSecurityCertifiedを別にし、全体poolからcommit後に抽選した検証者の承認を合意creditへ要求する。これはその正直比率・参加・資料取得を前提とする認証であり、モデルの数学的性質を証明しない。

Panelの全再実行と一部の責務は重なる。将来、抽選・再実行結果・期間を共通化する変更は検討可能である。しかし独立抽選、承認threshold、拒否条件、責任期間のどれを継承するかを決めず、同じ署名を両者へ転用することは条件の削除になる。今回、Gate Cを削除する変更は行わない。

### 4.3 実装を集約する箇所

| 集約先の責務 | 共有する処理 | 分離して保持する意味 |
| --- | --- | --- |
| ContextBinding / ExecutionChecker | model、program、四Root、位置、codec、opening、数値checker | 全再実行の承認と、局所反証の結論 |
| ModelAdmission | descriptor、policy、generation、停止集合 | 自動審査合格と、BFTの採用判断 |
| ClaimEligibility | Final、未解決case、停止、Gate C、期限の読取り | 各条件の故障前提と発効時点 |
| WorkUse / Settlement | attempt/work/right/予約の消費とjournal | 仕事の帰属・権利発行と、後続ブロックでの消費 |

これは新たなfour-stageプロトコルを追加する提案ではなく、既存呼出先を共有する実装整理である。単一のeffective predicateをProducer、Node、RPC、snapshot/import、reorgで用い、各所のboolや独自判定を増やさない。

## 5. 新しいモデル追加は何が簡単になるか

自動追加できるモデルは既存の自動検査で追加する。自動審査の対象外、定理がない、費用や品質の形式的根拠が足りないモデルも、技術互換があればBFTのYES/NOへ進める。費用や品質の測定は判断資料として使えるが、提出を承認の成立条件にしない。

この経路を導入すると、Qwenの未完成G-COLLだけを理由に全モデル採用を止め続ける必要はなくなる。ただしQF1が今すぐ承認済みになるわけではなく、runtime実装・policy有効化・実instanceへの正規投票が必要である。

BFTの結果は「採掘へ採用するという認可」であり、「通常推論と同じ費用が必ず必要」という定理ではない。安価に正しく計算できるモデルを誤って採用するリスクは残る。YES/NOは判断を完了させる機構であり、真実性を自動生成しない。

n=3f+1、q=2f+1の集合交差と署名者の非二重署名を使い、同じproposalへの矛盾した決定を制限する。[PBFT原論文の公開先](https://www.usenix.org/conference/osdi-99/practical-byzantine-fault-tolerance)。本設計はPBFTの実装一式やその活性定理を継承しない。

## 5.1 genesis・追加・forkの管理境界

今回の追加指示で、genesis manifestの元モデルだけをBFT管理対象外とした。追加モデルは自動又はBFT YESで採用し、いずれもBFT停止・正規解除の対象である。BFTはforkモデルもYES/NOで追加でき、Optionなし・Miner=Rを継承するが、forkは停止対象とする。

BFTがgenesisと同じ経済扱いのモデルを増やせることと、停止不能なgenesisモデルを増やせることは異なる。元genesisのentryをBFTで追加・差替えできず、rule停止でも間接停止しない。個別の誤計算、二重消費、無効ブロックはgenesisでも拒否する。物理的なネットワーク停止まで防ぐ保証ではない。

## 6. 公開への最小実装順

1. 既存MAS1へAdmissionBasisの二分岐と共通BaseCompatibleを加える。
2. 不変origin、BFT管理対象、MODEL_ADMISSION票の経済分類・quorum・期限・世代、genesis除外と非genesisの停止優先を通常native状態へ接続する。fork登録・catalog指定・採用は原子的にする。
3. QF1の固定拒否を、その正規admission recordを参照する判定へ置き換える。default trueにしない。
4. 既存計画のOriginal/Panel/Court → 成功Final → 独立Gate C → 権利消費 → 同一ブロックMOE2を完成させる。
5. 通常ブロックの短時間試験を通し、最後に実instanceと実票で3ピアの受賞・決済を検証する。

証明を必須にしないモデル採用と、既存claimの安全条件の完成は両立する。新たなモデル証明生成system、BFT finality、二重の報酬台帳を追加しない。

## 7. 本監査の限界

静的な参照と文書整合性の確認であり、全攻撃を発見する監査又は費用の最小性証明ではない。今回Cargo build、Qwen実推論、実投票、3ノード実行、公開設定変更は行っていない。文書検査には入力時点から歴史digest不一致・欠落リンクがあるため、今回の差分で増やさないことと入力原本の保全を別に確認する。

変更後の仕様と保存原本・検査結果は[改訂記録](../../../maintenance/revisions/2026-11-23-dual-route-admission/README.md)を参照する。
