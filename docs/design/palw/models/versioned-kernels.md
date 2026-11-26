# PALW kernelの意味論・互換性・更新

本書は[現行RFC-0002](../../../rfc/0002-tensor-ir-kernels-and-runtime.md)の補助設計です。モデルの追加承認は[RFC-0007](../../../rfc/0007-bft-model-admission.md)のBFT YES/NOだけで行います。kernel解析、lowering、適合試験は実行の技術条件を確認する処理であり、モデルを自動採用する経路ではありません。

## 1. kernelが担うこと

kernelは、有限の計算グラフに対する演算、型、丸め、メモリアクセス、状態更新、trace位置と局所Courtの意味を定めます。モデル名ごとの特例ではなく、複数のモデルに再利用できる演算族として定義します。

モデルの重み、tokenizer、入力規則、実行profile、Readout、trace schemaを固定し、同じ入力から同じ規範出力を再現できることを要求します。GPU、CPU、低RAMのストリーミング実行もこの同じ意味論に従います。高速化やcacheは意味を変えない範囲で許し、演算順序・丸め・量子化・停止条件を黙って変更しません。

kernelの存在、合格したテスト、BFTのYESは、LLM内部状態の予測困難性や計算費用の数学的下界を意味しません。評価がUNKNOWNなら、そのまま記録します。

## 2. モデル追加とkernel拡張の境界

| 変更内容 | 技術的に必要な処理 | モデルの採用判断 |
| --- | --- | --- |
| 対応済み演算・上限内の新しい重みやshape | artifact、graph、型、実行profileの固定と整合検査 | BFT YES/NO |
| 新しい入力形式を同じ意味へ変換 | 固定adapterと入力・tokenizerの一致確認 | BFT YES/NO |
| 対応済み演算の新しい組合せ | 有限graph、依存関係、資源上限、Court coverageの確認 | BFT YES/NO |
| 未対応の演算・状態更新・Court | kernel仕様とノード実装の追加、明示的な互換移行 | 対応前のYESだけで実行可能にしない |
| 再現不能な外部API、欠落資料、上限不明の処理 | 拒否理由を返し、技術条件を満たすまで実行を開始しない | YESでも技術的拒否を無効化しない |

モデル登録が任意のnative/WASM/Pythonコードを合意ノードに実行させる機能にはしません。任意VMへのフォールバックは採用しません。BFTは、未対応opcode、無効なencoding、資源上限超過を正常な実行へ置き換える権限を持ちません。

## 3. identityとdescriptor

モデルの技術的束縛には少なくとも次を含めます。

- kernelの意味論と演算集合、graph grammar、算術・メモリ規則。
- program／plan、重みartifact、tokenizer／入力schemaのroot。
- shape・状態上限、生成・停止規則、Readoutとtraceの位置規則。
- commitmentの符号化、局所checker・Courtの種類と資料上限。
- 適用するchallenge policy、受付・報酬・仕事会計のprofile。

これらは意味上の必須項目であり、ここで新しいwire tagやRust型を割り当てません。既存の`V1`等の型名、codec ID、署名domainは、RFC番号の再編によって改名・再利用しません。

claim、キャッシュ、供給資料、Courtは適用された同じdescriptorと元claimへ束縛します。別モデルの重み、別位置の本物のleaf、別profileの結果を入れ替えられないよう、期待root・型・位置を照合します。

## 4. 演算族ごとの検証対象

| 演算族 | 固定する意味と局所争点 |
| --- | --- |
| 行列積・線形演算 | 次元、重みindex、整数encoding、規範加算・丸め、入力から出力への関係 |
| 量子化・非線形演算 | scale、範囲、飽和、overflow、lookupや近似関数の規範値 |
| Attention・MoE | mask、位置、score、routing、TopKのtie、expert／gather／scatterの対応 |
| 状態更新・KV cache | 初期状態、認証済みread/write、順序、境界、同一入力prefixへの束縛 |
| 複合処理 | stage間の入出力一致、入力資料snapshot、各stageの有界な反証 |
| Readout・終了 | 参照する実状態、順序、hash処理、生成tokenと停止理由、Endとの一致 |

Freivalds等の代数的スポットチェックは直接の確率的検査としてのみ扱い、対象範囲と見逃し確率を明記します。暗号学的な計算validity proof、GKR transcript、PCS、ZK方式を導入しません。ハッシュ・署名・Merkle openingは資料の同一性と位置を認証するもので、計算全体の正しさを証明しません。

## 5. 個別claimの検証

1. BFT採用済みモデルについて、適用profileと実行資格を確認する。
2. Minerが規範実行と元traceを固定してclaimを提出する。
3. [G14 Panel](../../../rfc/0008-g14-panel-and-verification.md)が対象claimを独立に完全再実行する。
4. 不一致は元rootとモデルrootへ認証された資料を用い、[Court](../../../rfc/0009-court-and-claim-final.md)で有界な局所関係として裁く。
5. 反証・資料case・通知期間を含む正規条件からclaim Finalを判定する。
6. 合意creditは別途[独立Gate C](../../../rfc/0011-gate-c-and-consensus-credit.md)の条件を満たして認証する。

資料不供給を算術的不正と同一視せず、対応する資料caseに分類します。肯定署名が有効な反証を上書きすること、BFT採用を個別claimの成功とすること、local replayだけでGate Cを代替することは認めません。

## 6. 更新と互換性

新しい意味論には別のdescriptorを割り当て、対応するノードと明示的な発効条件を用意します。モデル登録のBFT YESと、ノードが新演算を実行できる状態への移行は別です。

既存claimには受付時の規則を適用します。旧domain、型ID、rootや署名の意味は変えません。未対応kernelを未知のまま成功扱いにしません。再起動、reorg、pruning、IBDをまたいでも、当該claimのdescriptorと資料を解決できる必要があります。

## 7. 互換試験

規範CPU実行と最適化backendについて、整数境界、tie、丸め、状態更新、trace順序、Readoutを照合します。未知opcode、欠落tile、重複位置、別モデルのopening、誤ったroute、資料不供給を拒否する試験を用意します。

有限の試験成功は全モデルの計算困難性の証明ではありません。BFTによる採用の判断資料と、実行系の技術的互換性を区別します。公開有効化は[RFC-0020](../../../rfc/0020-devnet-release-and-migration.md)に従い、文書更新だけで変更しません。
