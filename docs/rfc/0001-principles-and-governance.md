# RFC-0001 — 設計原則・責務・適用範囲

- 文書日付：2026-11-26（Asia/Tokyo）
- 文書区分：**DESIGN_SPECIFICATION**。採択済みの規則を統合する。提案に限る部分は該当節で明示する。
- 判断理由：[ADR-0001](../adr/0001-principles-and-authority.md)
- 規範入口：[設計原則](../PRINCIPLES.md)／[RFC一覧](README.md)
- 本文は実装の設計を定める。実装・実測・公開有効化の完了を示すものではない。

## 1. 目的と適用単位

MISAKAは、指定モデルの有用な推論をPALWの仕事として扱い、実行検証、合意資格、ブロック生成、決済を接続する。モデル名や回答の一致だけで仕事量と費用を保証しない。

MUSTは当該profileを採用する実装の必須条件、MUST NOTは禁止条件である。採択、実装、検証、公開有効化は別の判断とする。文書番号、文書日付、BFTモデル採用票からネットワークのactivationを推定しない。

新規採掘はFOW1、用途別乱数はTB1を設計の中心とする。既に正規受理されたclaimは受付時のfamily・policy・署名・元期限で処理し、新番号への整理を理由に意味を変更しない。

## 2. 計算検証と暗号の境界

LLM・モデル計算の正しさを示す暗号学的validity proofは使用しない。ZK、SNARK/STARK、zkVM/zkML、PCSベースのsuccinct argument、GKRを使う計算validity argument、再帰証明・proof aggregationを採用経路に置かない。

ハッシュ、デジタル署名、Merkle commitment/opening、認証された乱数生成は、同一性・作成者・乱数のために使用する。commitmentが一致することを、全計算の正しさや必要計算費用の証明として扱わない。

検証は完全再実行、明示した範囲の確率的検査、公開証拠による局所化、決定論的Court、bondによる損失制限と個別slashで行う。部分検査の検出率を全体の無条件保証にしない。

## 3. 役割と権限

| 役割 | 責務 | その役割だけでは決めないもの |
| --- | --- | --- |
| Miner / Studio | 固定入力の実行、claim署名、資料・責任の提供 | 自分のclaimの合意資格 |
| G14 Panel | 選出義務に対する独立完全再実行、承認又は反証 | 全Nodeのfork choice、他人の資産没収 |
| 公開検証者 | 元claimへの適格notice、資料要求、局所反証 | 無根拠な有罪判定 |
| Node / Court | 同一性・署名・有界局所関係・状態遷移の検査 | 未取得モデルの全体再実行をしたという主張 |
| Gate C検証者 | 独立抽選による仕事認証 | BFTモデル採用、無条件の計算費用保証 |
| TB1 holder | 秘密準備・保持・規定時点での公開 | 台帳finality・任意の再抽選 |
| BFTモデル管理者 | モデル採用YES/NO、規定の停止・解除・廃止 | 個別claim検証の免除、fork choice、reorg veto |

同じ人が複数の役割を担っても、署名domain、権限、選出条件、用途別bondを分離する。Node数、Panel人数、Gate C抽選位置数、独立operator数、BFT定足数を混同しない。

## 4. 成功条件の分離

`ModelRegistered`、`BftApproved`、`ClaimAccepted`、`ClaimFinal`、`SecurityCertified`、`TicketWon`、`AwardAccepted`、`Settled`を別に保持する。claim Finalは台帳の決済確定ではない。Finalだけからcredit、受賞、支払いを作らない。

適格反証・失効・未解決資料case・依存取消は同一batchの成功効果に優先する。資料を取れないことを計算不正と断定せず、検証が成立しなければ成功を認めない。未解決責任を保ったまま、依存しない他claimは進められる設計にする。

一人の正直Panelという仮定は、対象claimを実際に検証し、資料を取得し、元期限内に反証を包含・処理できる能動的参加者を意味する。ネットワークのどこかに一人いるというだけでは足りない。Gate C、TB1、BFTの故障前提は別に記述する。

## 5. 全モデルに共通する方針

モデル追加の認可はBFTのYES定足数のみとする。kernel対応や自動解析のPASSは技術互換情報であり、自動採用、timeoutによる採用、投票を省略する採用経路にしない。

全モデルは係数1.0、一意なPOSITION市場、同じモデル管理・停止規則を使う。genesisの初期manifestは起動状態であり、稼働後に追加する第三の経路ではない。genesisもBFT管理の対象とし、差は正規creatorが存在しない点だけとする。forkを名乗るモデルにも特別な経済分類や管理免除を与えない。

BFT承認は採用判断である。G-COLL/G-COST/G-USEのUNKNOWNを証明済みPASSへ変更しない。未知kernel、壊れたcodec、未定義の数値意味、個別claimの不正をYESで免除しない。

## 6. 金銭と仕事の保存

同一仕事・用途・権利を一度だけ計上する。市場原資、役務原資、bond、未発行予約、実際に回収したslash、既発行通貨を分ける。全体発行枠、model枠、個別bond予算を同時に満たす。

MOE3の共通coinbase配分は50/20/20/5/5。Courtの個別slashは実回収額から49%を有資格の発見・立証者へ、残差をburnする。BFT恒久廃止の100%burnとは混ぜない。正しい高速計算そのものを算術詐欺としてslashしない。

## 7. 不採用・延期の範囲

| 対象 | 扱い |
| --- | --- |
| PALW汎用VM、PALW-BVM/GVM | 不採用。既知のTIR primitiveと版付きkernelを使う |
| kernel判定だけでのモデル自動追加 | 廃止。BFT採用だけを新規入口にする |
| Panel=0による成功確定 | 新規の標準経路にしない。G14 Panel承認と独立Gate Cを維持 |
| 画像・動画の採掘、モデル改善protocol | mainnet後へ延期・当面無効。共通のtext実行部品は利用できる |
| 年次forkだけの1.0昇格、追加モデル0.3、core市場免除 | 新経済には適用しない |
| 後付けID、receiptの非衝突性によるLLM費用保証 | 採掘費用の根拠にしない |
| HFの定期取得認定を採用の自動条件とする制度 | 使用しない。固定artifact同一性とclaim資料契約は必要 |

延期対象はmainnet開始だけでは有効にならない。新しい採択と実装・検証・有効化判断を要する。既存モデルを改善重みで上書きせず、変更した重みは新しい不変instanceとしてBFT提案する。

## 8. 文書の管理

現行RFCはこの20本だけとする。RFC-v1/RFC-v2の並行体系、旧RFC本文のarchive、同じ本文の複製は作らない。旧71ファイルとの対応は小さな対応表で示す。

この再編はwire tag、domain bytes、旧enum、既存claim ID、DB schema番号の改番ではない。コード中の歴史的識別子を文書番号と一括置換しない。ADRも0001〜0020の20本へ統合し、対応するRFCと同じ番号にする。RFCは設計、ADRは判断理由を正本とする。旧235本文のarchiveは作らず、旧参照は完全pathの対応表で解決する。

新しい意味変更は本書の原則に照らして対象RFCとADRへ明記する。今回ユーザーが指示した20本化とBFT一本化について追加の再承認は要求しない。詳細な実装記録は設計本文へ混ぜない。

## 9. ネットワークと基盤identity

MISAKAはupstream Kaspaと別networkとして、genesis、network ID、handshake、address prefix、既定port、seedを識別する。公開・testnet・devnetも別identity・データ領域・署名scopeを持つ。tokenの名称変更をaddress prefixやwire domainの変更へ読み替えない。

native UTXO署名の基盤profileはML-DSA-87 P2PKHを用いる。public keyは2592 bytes、signatureは4627 bytes。署名contextは`kaspa-pq-v2/tx/mldsa87`、sighash domainは`kaspa-pq-v2/sighash/mldsa87`。address payloadは`kaspa-pq-v2/address/mldsa87`を鍵domainとするBLAKE2b-512の64 bytesで、支出側も同じ導出を使う。長さ・型・canonical push・script上界は重い署名検査の前に確認する。既存tagの値はこの文書改番で変更しない。

consensusのblock/transaction/Merkle identityは対象networkが固定したHash64型で扱い、32-byteのEVM値、Ticket又は別suiteの値へ暗黙truncateしない。UTXO commitmentは専用の64-byte型で、RPC/SDKでも幅を保持する。基盤native署名profileを、TB1などが明示している別用途suiteへ自動置換しない。EVMの署名domainはRFC-0015で分離する。

genesisのnative premine総額は各networkで10,000,000,000 BILIに固定し、複数のwallet・bootstrap用途・運用者への割当はその内訳とする。bond登録や市場virtual reserveを追加の通貨mintにしない。通貨表示の旧MSKラベルは別assetを意味せず、base unitと既存署名額を改名で変更しない。

詳細な基盤encodingは[暗号仕様](../spec/pq/kaspa-pq-spec.md)と[PQ設計](../design/pq/kaspa-pq-design-mldsa87.md)へ型単位で結び付ける。これらの旧記述と競合する場合は本節の現行profileと明示的なnetwork migrationを優先し、同一台帳の意味を黙って変更しない。
