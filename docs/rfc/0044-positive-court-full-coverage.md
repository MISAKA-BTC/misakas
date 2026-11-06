> Local adoption 2026-10-31: ユーザーの実装指示と番号確認により、Positive Court R2をRFC-0044 / ADR-0212として追加。既存0042 / 0210は保持。Implementation in this branch: A0 shared contract r1; the prior EXACT prototype remains in the source checkout and is not integrated. Independent review / native carrier / public activation: incomplete, inactive. 元ZIP内のDraft表示・未測定事項は履歴として下に保持。現行実装範囲は [status](../design/palw/positive-court/IMPLEMENTATION-STATUS.md) を参照。

# RFC-0044 — Positive Court：Panelによる正常計算証拠の生成・提出と、Minerの上限付き対話応答

- **状態:** Draft / Review Required / 未実装・未計測・未有効化。
- **起案日 / 改訂日:** 2026-10-31。
- **改訂:** R2 / Panel-as-Prover, Miner-as-Responder。前案の同一RFCを置換する。旧wireのversion変更を実施済みという意味ではない。
- **依頼:** 登録者・Miner・全Panelの共謀、独立Nodeのモデル事前未保有、資料不提出、Node資源上限、未検証claimの無信用という5条件を満たす正常計算Courtを設計する。
- **承認の境界:** 利用者が指定した役割は「Miner＝LLM推論・結果commit・必要データ応答、Panel＝検証計算・証拠構築・Positive Court提出、Node＝独立検査・裁定・信用制御」。その役割で仕様を改訂する。具体的な本番パラメーター、凍結原則の全面改定、mainnet有効化まで承認済みとはしない。
- **対になる判断案:** [ADR-0212](../adr/0212-positive-court-without-model-prepossession.md)。
- **基準:** 添付 `misakas-main (1)(1).zip`。旧RFC41本・旧ADR204ファイル。ZIPと参照元のdigestは[ソース照合](../design/palw/positive-court/SOURCE-AUDIT.md)。Git commitはZIPから確認できないので推定しない。
- **番号:** RFC-0044 / ADR-0212はこのZIPで未使用。別branchの予約は統合時に照合する。
- **15本への統合:** 本提案の主たる所属は **RFC-v2-0014「Court統合契約」のPositive Court章**。0044は変更履歴用IDであり、現行閲覧先を16本に増やす意図ではない。[統合表](../design/palw/positive-court/INTEGRATION.md)。
- **非変更:** runtime、既存wire tag、署名domain、fence、高さ、年次カタログ、1.0 / 0.3、既存claimの規則は変更しない。

MUST / MUST NOT / SHOULD / MAYは、大文字の場合のみBCP 14の規範語として用いる。[E3] 本文の新しい型名・関数名・列名は**仕様上の仮称**であり、現ソースに存在するとの主張ではない。

## 0. 要約と、保証しないこと

既存の **Fraud Court** は「不一致を発見した検証者が、その不正を客観裁定へ持ち込む」経路である。本提案ではこれを残し、報酬対象claimごとに必須の **Positive Court** を追加する。

```text
Miner: 通常の推論 → model/program/input/output等の結果を署名固定
  ↕ Panel: 認証済みdataを再利用し、不足dataだけを上限付きで要求
Miner/provider: 要求dataの読み出し・既存opening・限定checkpoint応答
  ↓ Panel: 自分で再計算・局所化・証拠用trace構築・認証資料の組立て
Panel: 証拠用witness rootを自己署名で固定（確率型は乱数確定前）
  ↓ chain: 必要な場合だけ、承認されたpost-commit検査乱数を確定
Panel: Positive Courtの証拠を構築・分割提出（Minerの代筆ではない）
  ↓ Node: 全必須関係を導出し、認証・算術・接続・最終出力を自分で検査
      ├─ 全被覆・output一致 → PositivePassed → 全既存gate → credit
      ├─ Panel witnessのみ不正 → WitnessRejected、Minerの計算有罪にしない
      ├─ Minerの署名主張の不正を独立に立証 → ClaimRefuted → 責任処理
      └─ 必要資料/証拠未完了 → EndedUnverified、推論報酬・信用なし
```

**正常時もPanelが証拠生成の計算を担う。MinerにPositive witness、全計算traceの新規構築、全relationのopening集、検査用projectionの作成を必須業務として課してはならない（MUST NOT）。** 既存claim形式で必須のcommitは旧規則として区別し、新規の軽量な結果commit形式は明示的なversion/fenceで導入する。

**Panelの署名、無異議、タイムアウト、資料rootだけではPositivePassedにしない。** 署名・Merkle openingは帰属・同一性のためであり、算術真実はNodeが直接検査する。

このRFCは、モデルを最初から丸ごと保有していないNodeを対象にする。**計算に必要なweightも最後まで秘匿する設計ではない。** 当該claimが全weightを読むなら、全weight相当を順次開示する場合がある。全量の事前配布・常駐保存は不要でも、総通信量が小さいとは限らない。ここを了承できないモデルは、この方式では報酬対象claimを完成できない。

「正しい計算結果と認証された計算関係」を検査する。**Miner本人がGPUを使った履歴、最低電力量、指定FLOPsの実消費、モデルの有用性・学習品質は証明しない。** 別人への計算委託、正しいキャッシュ、代数的な高速化を、偽の計算結果と混同しない。

全員共謀とは登録者・Miner・全Panelの共謀を指す。基盤DAGの全支配、全ネットワーク永久遮断、全てのNodeが検証規則を無視する状況は保証対象外。正直なフル検証Nodeは不正な結果を自ら拒否できるが、世界全体の進行を単独で保証しない。

### 0.1 R2で変えた点

前案は、提出者をPanelとしながらMinerに全trace rootの事前固定を求め、誰が中間証拠を計算し誰が責任を持つかが不明瞭だった。R2はMinerの**結果commit**とPanelの**証拠commit**を分離する。Panelが中間値を再構築してから固定し、Nodeがそれを入力から終端まで検査するため、Minerが全traceを生成することを正常認証の前提にしない。

対話は資料取得・再利用・負荷上限のために使う。全関係の検査を少数サンプルへ置換するものではなく、「質問がなかった」「Panelが終了した」だけでは成功にしない。**Minerの軽量化、証拠サイズの縮小、Nodeの軽量化は別々に評価する。**

## 1. 5要件の規範化

| ID | 必須条件 | 本RFCでの閉じ方 | 未完了なら |
|---|---|---|---|
| PC-1 | 全所有者・全Panel共謀でも偽計算の受理を抑える | 全関係被覆、exact再実行または全行列関係への直接検査、事前固定、反復・seed選別を含む上界 | profileを有効化しない |
| PC-2 | モデル事前未保有Nodeでも同じ検査 | 使うweight・input・中間値・programの認証付き開示、chain由来資料のみで再現 | PositivePassed禁止 |
| PC-3 | 必要資料の不提出を客観的に扱う | 予約済みの提出範囲とchain受理済みcoverageの差を判定。未完了と帰責defaultを分離 | creditなし。根拠なしslashは禁止 |
| PC-4 | NodeのCPU・RAM・通信・保存が許容範囲 | Node総量に加え、Minerの追加負担とPanelの証拠生成を別々に予約・実測。claim/block/epoch/保持/復旧を制限 | 受付拒否。未測定profileは禁止 |
| PC-5 | 未検証claimに報酬・DAG信用・決済信用を与えない | 共通read gate、全consumer接続、依存先・reorg・snapshot復元にも適用 | fail closed |

これは「5条件のmainnet達成済み」という報告ではない。PC-1/2の方式と条件付き論証を記述し、PC-3/5の状態遷移を定義し、PC-4を**測定がなければ開かない合意上のadmission条件**にする。実LLMでPC-4を満たすprofileは本成果物では1つも認定しない。

## 2. 既存方針との差分・適用範囲

現行RFC14はモデル取得可能な検証者を前提とし、モデル取得を目的とする反復range/全weightsの強制公開を撤回している。PRINCIPLES §0は検出率下限をactivation gateとしない。§8.3はlocal replayだけで抽選承認を代替しない。[S01–S04]

本提案は次を**新規claimに限る変更案**とする。

1. claimを報酬対象として提出する者は、その全範囲検査に必要なデータを開示する契約へ明示署名する。モデル登録だけでは配布義務・slashを発生させない。
2. 新profileについては、検査範囲・見逃し上界・資源上限を必須の有効化条件にする。旧claimや旧検査記録の意味を変えない。
3. Gate Cは維持するが、**PositivePassedを追加の必要条件**にする。新しいversioned attestationは公開CourtをNode自身が検査した事実を対象にできる。旧`Reproduced`を黙って再定義しない。
4. 正常Courtで全員が肯定しても、Nodeの全範囲検査に失敗すれば無信用。G14の既存不正告発入口は残す。

ZK、SNARK/STARK、zkVM、PCS、GKR、再帰証明、証明集約は導入しない。使うのは認証データ、bit-exact再実行、直接の代数的検査である。モデル単位の公開性審査、旧Seeder制度、Panel=0、年次カタログ廃止も導入しない。

正確な改定対象は[POLICY-DELTA](../design/palw/positive-court/POLICY-DELTA.md)。正式採択までは現行方針が優先し、本提案のfenceは存在しない。

## 3. 役割と脅威モデル

| 役割 | 義務 / 権限 | 信頼しない事項 |
|---|---|---|
| 登録者 | model/program/layoutの不変登録。自ら供給契約へ署名した場合だけ資料供給 | 正直さ、無署名の配布義務、モデル品質 |
| Miner / Producer | 通常推論、結果commit、署名済み範囲のdata応答。自ら証拠生成役を兼任することは任意 | 自己申告の計算履歴。Panel witness全体への包括責任 |
| Panel / Prover | 検証の再計算、証拠用trace・Merkle tree・局所/全体witness構築、Positive/Fraud Court提出、prover資源確保 | 正直さ・票数・自己申告の検査済み印 |
| data provider / relay | 契約範囲のdata供給・認証搬送。Minerの代替応答が可能 | 他者の算術への無限定責任 |
| 検証Node | 公開資料から独立検査・状態遷移・帰責・信用の制御。modelの事前コピー不要 | Panelの計算結果を未検査で信用すること |
| Gate C抽選者 | 公開Courtを自ら検査してversioned所見へ署名 | Positive検査を署名で代替すること |

安全性の条件は、commitment binding・署名の安全性、仕様通りの算術検査器、canonical chain view、対応profileの被覆完全性。確率型ではpost-commit乱数の条件を追加する。資源不足のNodeがheaderだけを読む運用は、この検証Node保証を得ない。

全Panelが提出を拒めば進行は止められる。これはfalse acceptanceではなく可用性喪失。非Panelも同じ認証資料を提出でき、独占owner席・指定Panelの署名待ちだけで代替提出を拒否してはならない。誰も必要資料を公開しないなら無信用のまま閉じる。

### 3.1 実務分担：証拠生成と資料応答は別作業

| 作業 | 通常の担当 | Minerの必須追加業務か |
|---|---|---|
| LLMの推論、結果の固定 | Miner | 本来の仕事 |
| programからcoverage planを導出 | Node（Panelも同じ導出器を利用） | いいえ |
| 登録済み重み/inputの取得・検査 | Panel、provider | sole holderなら有限な供給義務の範囲で応答 |
| 全中間値の再計算、証拠用traceの記録 | **Panel** | **いいえ** |
| 証拠用Merkle tree、relation bundle、局所化、証拠codec | **Panel** | **いいえ** |
| 検査用ベクトル・積などの証拠生成側計算 | **Panel** | **いいえ**。Node側検査を省略する意味ではない |
| 既存data片・保存checkpoint・登録物openingの応答 | 指定source。Minerも可 | 事前同意の上限内のみ |
| 公開Courtのcarrier組立て・署名・再送・Finish | **Panel / relay** | **いいえ** |
| 公開資料からの全範囲検査・裁定 | **Node** | **いいえ** |

Panelがmodelを既に持つ正常ケースでは、Minerへの中間値要求はゼロでもよい。Panelが自分で作れる証拠の生成を、`EvidenceRequest`という名称でMinerへ押し戻してはならない（MUST NOT）。

### 3.2 Minerの軽量化契約

Minerは`MinerResponsePolicyV1`に署名する。少なくとも最大round数、unique bytes、再送bytes、同時要求数、追加hash/IO/保存量、任意のcheckpoint再計算work、応答期間、料金上限を固定する。profileに対応した単位でNodeが保守的な最悪費用を計算できなければ、その要求種別は無効とする。

`REPLAY_BY_PANEL`を標準とする。Minerの全activation保存、毎claimの全trace構築、Positive witnessの生成、全推論の再実行は必須ではない。既存artifact indexの構築・維持にも費用があるため、一回限り/償却/毎claimのどれかを測定記録に分ける。既存indexがないのに巨大な全artifact再hashを低価格の一回応答として要求しない。

`CHECKPOINT_ASSISTED`は任意の最適化である。Minerが既に保存したcheckpointを提供でき、明示的に同意した有限量の再計算も可能だが、未検証checkpointを正しい初期状態としてNodeが採用してはならない。PanelまたはNodeがその起源と前区間を検査する。checkpoint提供を断っただけで、標準profileのMinerを失格・defaultにしない。

Minerが停止する場合は、必要なdataと有限の供給義務をproviderへ引き継げる。委任は署名済み範囲・費用・解除条件を明示する。provider鍵や資金鍵の無限定共有を要求しない。誰もdataを供給できなければ未検証であり、証拠生成義務を後からMinerへ自動移転しない。

## 4. 正しさの対象：登録計算への結び付き

claimの主張を次で固定する。

```text
y = Exec(program_root, model_root, job_input_root,
         tokenizer_root, execution_ruleset, generation_seed, dependency_roots)
```

`Exec`は当該登録物の決定論的意味である。Panelが用意した別の小さいprogramや、都合のよいinputを検査してはならない。

初期仕様では、有限展開でき、登録型・長さ・loop上限が確定するTIR profileだけを対象とする。未対応の動的制御、opaque外部呼出し、未規定のfloat/NaN/丸めは拒否する。一般のML VMは導入しない。

MUST検査する関係は次の全てを含む。

- 固定program、input/tokenizer、modelパラメーター、shape・dtype・layout・byte length、乱数seed。
- 全演算の入力・出力の関係。matrix以外のactivation・正規化・丸め・clamp・比較・選択も含む。
- tensorのproducer/use、alias、初期state、KV read/write、token位置、position encoding、prefix起源。
- logitsからtokenへの選択、全候補範囲、tie-break、temperature等の既存規則、EOS、出力長。
- terminal outputとclaim出力rootの一致。途中で止めた短い正しいprefixを全jobの完了と認めない。

「登録rootと一致する」ことは、その登録モデルが良いモデルであることを意味しない。登録者が最初からゼロ行列を登録した場合、正しいゼロ出力は計算不正ではない。資本配分・canonical work metering・重複報酬防止は別規則を維持する。

## 5. 被覆計画：Panelは検査対象を選べない

新しい`PositiveCoveragePlanV1`を、登録program・job shape・最大出力長・ruleset・検証profileからNodeが導出する。提出されたrelation listをそのまま正本としない。

```text
plan_root
  = H(version, program_root, model_root, job_shape, ruleset,
      relation_schedule, tensor_schema, boundary_rules, cost_envelope)
```

全relationに一意の`relation_id`、入力tensor/range、出力tensor/range、検査mode、canonical順序、総chunk数、資源上限を割り当てる。全outputから初期入力まで依存をたどる被覆だけでは足りず、報酬に算入する全仕事も覆う。不要な架空演算を足してcreditを増やせないようwork算定は登録programから導出する。

欠番・重複・別位置への差替え・最後のrelation省略・依存edge省略・偽のzero-length scheduleは拒否する。empty jobは対応profileが明示し、非ゼロ仕事として報酬化しない。

動的分岐を後に対応させる場合も、分岐条件をexactに検査し、選択された全分岐と非選択の正当性を有限な規則で導出する。Panel自己申告の「そこは実行しなかった」は不可。

## 6. 結果commitとPanel証拠commitを分離する

以下は新profile用の論理schemaであり、実wire番号・byte layout・signature domainは未割当。旧claimや既存署名の意味は変えない。ハッシュ入力にはnetwork、ruleset、種類、canonical length、型・位置を含める。

| オブジェクト | 署名者 / 主な内容 |
|---|---|
| `PositiveResultCommitV2` | **Miner**。claim/execution key、model/program/input/tokenizer/output/dependency roots、generation seed、plan/profile digest、供給上限・資金同意。**新規全trace rootは必須項目ではない** |
| `MinerResponsePolicyV1` | Minerまたは指定source。要求可能data範囲、受付数、累積bytes/work/期間、checkpoint opt-in、料金上限 |
| `PanelProverContractV1` | **Panel**。再計算・証拠構築・搬送を引き受けるscope、完了期限、有限な費用・delivery責任 |
| `PositiveAdmissionV2` | Node導出の全coverage、Miner/Panel/Node別予約、実原資、source割当、service slot、cutoff |
| `PositiveCaseOpenV2` | Panelまたは公開prover。同じclaim/caseへの冪等join。反証の発見は不要 |
| `EvidenceRequestV1` | Panel。case、正規data key、取得root/range、既存依存、要求kind、上限、署名。chainが許容性を再計算 |
| `EvidenceResponseV1` | Miner/provider。request key、data、既存rootへのopening、応答scope。**Panel witnessへの承認署名ではない** |
| `PanelWitnessCommitV1` | **Panel**。claim commit hash、全coverage、証拠用trace root、source/provenance map root、candidate出力root、prover identity、attempt。確率型では乱数より先に固定 |
| `PositiveMaterialPartV2` | Panel/relay。Panel witness/登録物/inputへのopening、relation、data range、出典、part順序、canonical codec |
| `PositiveCheckProgressV2` | Node導出cursor・前後state・被覆・消費資源。自己申告の進捗を採用しない |
| `PositiveCaseFinishV2` | Panel/代替prover。終了要求。検査を省くcertificateではない |
| `PositiveResultV2` | Node導出結果、claim/witness/plan/profile/ruleset/branch、依存、責任horizon |
| `PositiveCourtAttestationV2` | Gate Cの新型所見。検査済み結果とbranchを署名。旧形式は再解釈しない |

### 6.1 固定順序と署名の帰属

```text
Miner result commit accepted
 → Panelが必要資料を取得・再計算
 → Panel witness commit accepted（全必須relationの値を固定）
 → 承認されたpost-commit乱数を確定（確率型のみ）
 → 固定済みwitnessのopening / Node検査
```

**乱数前に固定する全witnessはPanelが作る。** Minerに全traceを生成・署名させる必要はない。PanelはMinerの固定結果を別の出力に修正できない。全量を一括RAMに置く必要はなく、Panelが証拠用traceをdiskへstreaming構築してrootを固定してよい。ただしそのIO・保持・root構築費をPanel予算へ計上する。

Panel witness内の中間値は**Panelが主張した値**であり、Merkleで認証されても自動的に正しくはない。Nodeが全relationと接続を検査する。Minerがそのrootを再署名して責任を肩代わりすることを必須にしてはならない。

Miner自身が任意のcheckpoint・既存traceを署名済みとして提出した場合は、そのscopeだけ別に帰属させる。他者が作ったwitness全体の責任へ拡張しない。`source_owner`、`value_author`、`prover`、`carrier_signer`を区別し、relayの署名を算術の自己申告に読み替えない。

### 6.2 再試行・交代・grinding

`case_id`はnetwork/ruleset/claim/execution identity/結果commitから導出する。1 claimに単一の正本caseを持つ。Panel鍵変更、分割Panel、provider交代、再送で新しい無償の乱数試行枠を作らない。

最初のadmissible witness candidateをcanonical受付で固定する。複数候補・交代候補を認める場合は、profileで件数・総費用・soundness試行数を制限する。新candidateは**固定後の新しい未知乱数**を必要とし、以前に見たseedへ適応させてはならない。同じ固定済みcandidateを別submitterが続きを運ぶことは新しい試行ではない。

Panelは任意のroundで値を後付けできない。初期V2は全witnessのrootをseed前に固定する。段階的commit方式は別途adaptive soundnessと予算を仕様化しない限り無効とする。EXACTでもscope/branch/値の差替えは不可。

### 6.3 旧runtimeとの境界

既存kernel routeにはproducer側で`build_evidence_v1`を呼ぶclaim生成がある。[S08] 本改訂は「今のMinerが既に結果commitだけで動く」とは主張しない。旧claimのtrace必須フィールドを空にして通す実装は禁止する。新形式を追加し、署名domain、受付、SDK、旧/新gateを明示的に分ける。旧形式を使う移行期間のMiner負担を新形式の性能として報告しない。

## 7. Model未保有Nodeへ渡すもの

Nodeが使う全値は、次のいずれかから来なければならない。

1. canonical chainに記録されたjob・input・固定ruleset。
2. 登録model/program/tokenizer rootへの認証opening。
3. 事前固定した**Panel witness root**への認証opening。これはPanelの値主張の認証であり、正しさは全relation検査で決める。
4. 同じbranch上で既にNodeが検査した前段状態・依存claimの確定状態。

Minerのcheckpointは、claim/sourceに結び付いていても前段検証の代わりではない。Panelは再計算のヒントとして使えるが、Nodeの検査開始点にするには認証済みの既存状態か、今回の全被覆による導出が必要。

単一のMerkle leafにtensor ID、種類、dtype、layout、shape、offset、byte length、dataをbindする。単に値のhashが一致するだけでは、別位置・別dtypeの取り違えを防げない。proofのdepth、兄弟数、展開bytes、重複ノードにも上限を課す。

必須データは**consensusで検証するcarrier本体**、または同等に全検証Nodeへの取得がblock validityの必要条件となる既存transportに載せる。外部URL、少数providerの「保持した」署名、資料rootだけでは十分でない。このRFCの初期実装はcarrier本体への分割格納を基準とし、外部DA委譲を成立済みとしない。

全資料をRAMへ一括保持しない。検査済みcursor、authenticated state、active relationのworking setだけを常駐させ、必要に応じて再openingする。**bounded RAMと総bytes削減を混同しない。** Nodeが後から全weightを再構成できる可能性も排除しない。

## 8. 検査profile A：EXACT_STREAM_V1

最初の安全性基準は、全必須関係をbit-exactに再実行する方式である。

1. 初期stateを認証済みinputとprogramから導出する。
2. 全relationをcanonical順に検査する。大きいrelationはmicro-stepへ分割し、各chunkの前stateを直前にNodeが導出したstateへbindする。
3. 中断したmatrix accumulator・loop index・読書位置をstateへ含める。chunk境界で積算器を勝手にresetできない。
4. 全てのmaterial membership、算術、control、producer/use整合、終端条件を検査する。
5. 最終cursorが計画の終端と一致し、出力がclaimのoutput rootと一致したときだけscopeを完了する。

このmodeには抜き取りがなく、正しい仕様・正しい実装・binding成立の下で、誤った関係の見逃し確率は0である。暗号破り・実装バグ・誤った仕様のリスクまで0とは言わない。

**代償:** 合計演算は推論の再実行相当以上になり得る。分割により1blockの負荷を制御できても、全claimの処理速度は増えない。profile admissionはこの実コストを支払える範囲に限定する。

## 9. 検査profile B：FULL_RELATION_DIRECT_V1

証拠用のA/B/C、中間値、値域、openingはPanelが構築する。Minerへこれらの計算や証拠生成用projectionを要求しない。Nodeは提出projectionを信用して検査を省略しない。

これは、少数stepを抜く方式ではない。**全てのmatrix relationを直接Freivalds型検査にかけ、それ以外をexactに検査する**profileである。NodeはA/B/Cの認証データを読む。Panelが計算したprojectionだけを信用しない。[E4]

### 9.1 対応する算術

初期対象は、登録semanticsが既にexact integer accumulationを定めるmatrix relationだけ。

```text
A ∈ Z^(m×k), B ∈ Z^(k×n), C ∈ Z^(m×n)
C = A B                      # 丸め前の整数accumulation
Y = ExactQuantize(C, params) # 丸め・飽和・scaleは別のexact relation
```

floatの演算順・丸めを整数matrixの式へ勝手に置換しない。既存f32/f16等の意味をこの形へ正しく帰着できなければ、その演算はEXACTへ戻すかprofileを拒否する。既存モデルのsemanticsを変更する場合は別モデル・別rulesetが必要。

A/B/Cの値域を全読み取り時に検査する。`|A_ij| ≤ a`, `|B_ij| ≤ b`, `|C_ij| ≤ c`を確認し、prime `p > k*a*b+c`を満たすとき、非ゼロの整数差`AB-C`が全てmod pで0になることはない。より保守的な`p > 2*max(k*a*b,c)`も可。積・上界・mod演算の実装自身にoverflowを許さない。

prime・反復数・値域・field実装はprofileに固定する。Panelがprimeを選ばない。浮動小数点の近似比較や「誤差の範囲だからpass」は使わない。

### 9.2 Nodeによる全関係検査

各relationと各反復に対して、独立一様を理想化した`r ∈ F_p^n`を用い、Node自身が次を計算する。

```text
v = B r
left = A v
right = C r
accept_relation iff left == right for all t repetitions
```

全要素を認証して読み、range/shape/edgeを検査する。streamingは可能だが、vなどのworking setやA/B/Cの再読込みも費用に数える。Panelが提出する`v`、`left`、`right`をそのまま採用してはならない。

概算算術量は1relation当たり`O(t*(k*n+m*k+m*n))`。全bytesは少なくとも読んだoperand総量相当。decodeのmatrix-vector（n=1）では大きな削減を期待できず、反復により遅くなる場合がある。実測でEXACTより良くなければ使わない。

### 9.3 見逃し上界の論証

固定された偽relationについて`D=AB-C ≠0 (mod p)`。Dの非ゼロ行を1つ選び、その行の非ゼロ係数に対応する`r_j`以外を固定すると、`D r=0`となる`r_j`は高々1つ。したがって一様なrでは`Pr[D r=0] ≤1/p`、独立t反復では`≤p^-t`。

全graphの出力が誤っていて、model・input・control・edge・終端をexactに検査し、被覆が完全なら、少なくとも1つのrelationが偽である。最初の偽relationを固定すれば、全graphが通る確率はそのrelationが通る確率以下。全matrixが同じ最低強度なら`≤p^-t`。profile間で強度が異なれば最大の見逃し上界を用いる。これは関係を全件検査する場合の論証であり、relation抽選へ転用しない。

この上界は、固定済みPanel witness・理想的乱数・exact field・binding・完全被覆に条件づけた**直接確率検査の見逃し上界**である。LLMの暗号学的validity proofや、実ネットワークの無条件な安全性ではない。

### 9.4 Beacon・seed選別・中断を隠さない

乱数はMinerの結果commitとPanel witnessの両方の受付固定より後の、networkが承認したsourceから導出する。Panel/Minerだけが選べるseed、Panel witness hashだけをseedにする方式、共謀者しか貢献しないbeaconは不可。現在のPanel beacon policyを、無審査で本profileに流用しない。

実装はdomain-separated XOFとbiasのないfieldへの変換を使い、`network/ruleset/claim/plan/witness_attempt/relation/repetition`へbindする。長いベクトルを短いseedから展開する場合、その疑似乱数性は追加仮定であり、情報理論的に独立と偽らない。

Q個の固定済み不正候補、各候補で高々G個のseed候補を選べる条件では、期間中の不正受理イベントを概略次で上界化する。

```text
Pr[期間内に1件以上の不正claimがcreditを得る]
 ≤ min(1, Q*G*epsilon_relation + delta_randomness + delta_binding)
```

deltaは期間全体の仮定・攻撃成功の上界として定義する。測定されていない項に都合のよい数値を入れない。reorgでの再試行・同じ仕事の複数commit・Panel交代・複数候補・abortをQ/Gから外さない。合格seedだけ提出する場合、**提出されたclaimに条件づけた不正率が小さいとは主張しない**。

Q/G・source・変換・bias boundが未決なら、このprofileは有効化禁止。EXACT modeへの変更は新claim受付時のみ許し、悪い乱数を見た後の同一claimの変更は不可。EXACTではこのrandomness依存を要しない。

### 9.5 禁止する近道

- 少数のlayer/token/leafを検査して、他のrelationを「covered」にする。
- 算術正当性を署名数、bond額、無異議期間で置き換える。
- 正直な検証者が秘密モデルを持つことをPC-2の証明に使う。
- Modular alias、float tolerance、unchecked lookup tableで偽値を通す。
- Panelが計算した積・projectionをNodeが検査せず使用する。

## 10. 正常計算Court：Panel主導の上限付き対話

### 10.1 正常時の手順

| 手順 | 主担当 | 内容 |
|---|---|---|
| 1. Admit | Miner + Panel + payer、Nodeが審査 | 三者別資源・有限な供給義務・証拠生成契約・料金を原子的に予約。Panel不在のままMinerへ無限定義務を与えない |
| 2. ResultCommit | Miner | 推論結果と計算条件を固定。既存の計算結果を後でPanelに変更させない |
| 3. Open / Resolve | Panel | 正常Courtを開く。chain・認証cache・登録者/provider・自身のモデルからdataを集める |
| 4. Ask | Panel | 不足する**原data**だけを正規keyと上限付きで要求。scope外・重複・予算超過は拒否 |
| 5. Reply | Minerまたはprovider | data片・既存opening・opt-in checkpointを返す。全証拠は生成しない |
| 6. Reexecute / Build | **Panel** | 全検査用traceを再計算し、証拠用index・provenance・全relation bundleを組み立てる |
| 7. WitnessCommit | **Panel** | 自分の証拠用rootを署名固定。Minerへ包括承認署名を要求しない |
| 8. Challenge | chain | FULL_RELATION_DIRECTのみ、固定後の承認済み乱数。EXACTは不要 |
| 9. Present / Check | **Panel → Node** | Panelが分割証拠・openingを提示。Nodeが独立検査し、検査cursorを更新 |
| 10. Finish / Certify | Panel → Node + Gate C | 全被覆・出力一致・依存・責任window・Gate Cを満たす場合だけ信用を許可 |

3〜5は上限内で複数回行える。6〜9の証拠生成・提出はPanelの仕事であり、Nodeの検査で不足が判明したらPanelが固定済みwitnessの資料を追加する。証拠不備を理由にMinerへ全traceの再計算を強制しない。

実装上の早期資源preflightと最終受付を分けてもよいが、失効時の資金・予約を全rollbackする。結果commit後の仕事の差替え、署名scopeの追加、料金の自動増額は不可。

### 10.2 要求の許容性

許可する要求kindは`ARTIFACT_RANGE`、`INPUT_RANGE`、`AUTHENTICATED_DEPENDENCY`、任意の`SAVED_CHECKPOINT`、明示opt-inの`BOUNDED_CHECKPOINT_REPLAY`に限定する。Panel側の`BUILD_WITNESS`、`REBUILD_FULL_TRACE`、`MAKE_FREIVALDS_PROJECTION`、全treeの毎claim再生成は要求kindに含めない。

`request_key = H(case, source_root, canonical_range, kind, response_policy_version)`を正本とする。Panel identityや任意nonceをkeyに入れて同じdataを重複要求できないようにする。異なるoverlap rangeも正規区間へ分解し、unionのunique bytesと実際の再送bytesを別に計上する。全Panelの合計でpolicy上限を適用する。

Nodeはprofileのdependency graph・kind・range・署名済みsource範囲・残予算からadmissibleかを決める。Panelの「必要だ」という発言だけで新たなMiner義務を作らない。要求提出の前払feeと有限の依頼責任を課し、無効要求の費用をMinerへ転嫁しない。

既にchainで受理された応答、完了済み要求、providerの受理済み代替応答は再要求・再徴収しない。単にPanelが「まだ持っていない」と主張して時計をrestartできない。private cacheを持っているかの主観的判定をconsensus条件にはしない。

Panelに取得不能dataがあり、全supplierが共謀して供給しない場合は未検証で終わる。**対話だけで情報が不要になるわけではない。** sole holderであるMinerのcold-case bytesが大きすぎる場合はprovider契約または事前共有が必要で、そのprofile/jobを軽量Miner対応として受付しない。

### 10.3 通常transportと客観的な応答証跡

正常時は認証済みP2P/RPC応答とPanelによる一括搬送を使ってMinerのchain送信を減らしてよい。ただしprivateな送受信記録だけでMinerをdefaultにしない。

法的帰責ではなく本protocol内の有限なdelivery義務を開始するには、admissible request/noticeと予約済み包含機会をcanonical stateへ確定する。MinerはPanel以外のprovider/relayを使うか、予約されたfallback carrierへ応答本体を直接提出できる。Panelがreceiptを出さなくても、Nodeが受理した適格応答は完了とする。Minerのfallback送信は小片のdata応答であってPositive proof生成ではない。

通常の私的対話に応答がない場合、Panelは有限期限内でcanonical noticeへ昇格できる。受理前の時間を遡ってMinerの違反期間へ足さない。昇格にもbyte/work/包含枠が必要で、枠がないならMiner有罪ではなくServiceInterrupted/Unverifiedとする。

### 10.4 Fraud Courtとの共存

Panelは再計算中に不正を見つけたら、従来どおりFraud Courtへ証拠を提出できる。Positive witnessの全構築が終わるまで待つ義務はない。ただし既存の局所不正証拠に必要なMiner署名済みtrace/checkpointがない新形式では、それがあると仮定して従来の反証を作らない。

新形式の出力不正は、Nodeが入力から検査した正しい計算の終端と、Minerの署名済みoutputが異なること、またはその署名scopeに結び付いた既存のexact反証で立証する。**Panelの再計算結果だけ**ではMinerを有罪にしない。既存Fraud入口は残し、positiveの失敗を自動的にMiner convictionへ読み替えない。

## 11. claimと証拠candidateを分けた状態機械・帰責

```text
Claim:   ResultCommitted → PositivePending → PositivePassed → CreditReady
                                │                    │              │
                                └─ cutoff → Unverified └─ valid fraud/reorg → Revoked
                                └─ exact Miner-attributed fraud → ClaimRefuted
Witness: Building → WitnessCommitted → Checking → Complete
                              │              └─ bad witness → WitnessRejected
                              └─ no data/deadline → WitnessIncomplete
```

Panelのcandidateが失敗しても、claimは原則PositivePending/Unverifiedであり、**Minerの計算有罪と同一状態にしない**。代替Panelは契約・総資源・試行上限内で再構築できる。同じwitnessのrelay交代では最初の署名帰属を維持する。

| 結果 | Nodeが確認した事実 | 信用 / 帰責 |
|---|---|---|
| `PositivePassed` | 全必須関係・接続・終端が検査を通り、固定claim出力と一致 | Gate C等の残条件前なので単独では無信用 |
| `CreditReady` | 全gateと責任windowも成立 | 既存上限内で一回のみ |
| `WitnessRejected` | Panelが固定した値・proof構成が検査に失敗 | 当該candidateは無効。Minerへの転嫁禁止。Panelの署名契約の範囲で処理 |
| `ClaimRefuted` | Minerの署名scopeの不正をNodeがexactに立証 | claim取消、既存責任規則の範囲でのみMinerへ帰責 |
| `EndedUnverified` | 期限までに有効な全witnessが揃わない | 推論報酬・creditなし。Miner計算有罪ではない |
| `QualifiedDeliveryDefault` | 指定source/Panelの有限delivery義務と機会が成立し、本人の提出scopeが未履行 | 当該roleの合意済みdelivery責任のみ |
| `ServiceInterrupted` | chain/transport等の機会前提が崩れた | 未検証。無根拠slashなし |

不正packet、偽opening、codec不正はまず有界に拒否する。無署名のpacketだけで他者を有罪にしない。Panel署名witnessの算術不一致もMinerの結果不正を意味しない。Panelの作業ミス・未提出・応答隠匿の責任はPanelの署名範囲へ留める。

**確率検査と罰則:** FULL_RELATION_DIRECTの不合格はcandidate拒否の根拠になるが、Minerの計算罰則にはMinerに帰属する決定論的反証を要する。Panelが出した「別の正しい出力」が確率検査を通っただけでMinerをslashしない。初期仕様で終端不一致によりMinerを裁く場合、独立したexact negative court（必要なら全exact再実行）を完了する。Panelにも算術罰則を適用するなら、署名された虚偽命題と決定論的な失敗証拠を確認する。曖昧なprofile/実装errorは未検証に留める。

従来traceもMinerが明示署名した場合の局所反証はそのscope内で維持する。**結果commitとPanel witnessを同じroot所有者にまとめない**。この帰属表はproof parser、会計fold、RPC/UI、監査ログでも共通化する。

## 12. 不提出：data応答完了とCourt完成を混同しない

Nodeが客観判定するのは、canonical ledgerでのadmissible obligationと受理済み応答の差であり、通信障害の背後の故意ではない。

```text
MinerMissingData = AdmittedMinerDataRequests - AcceptedAuthenticatedResponses
PanelMissingEvidence = RequiredPanelEvidence - ChainAcceptedEvidence
UncheckedRelations = RequiredRelations - NodeCheckedRelations
```

この3集合を分ける。**Minerが依頼dataを全て返した後に、Panelが証拠を作らない／不完全なものを作ったことはMinerの不提出ではない。** 逆にdataが届いても、Nodeの全関係検査が済むまでcreditは出ない。

delivery defaultには、本人の範囲・費用上限・期限への署名、正規notice、prerequisiteの取得可能性、十分な予約service/包含機会、exclusive cutoff時点での本人scopeの未履行が全て必要。適格応答は暗号認証と型・rangeで受理する。単に下流PanelのPositive witnessが失敗したから、受理済みMiner応答を遡及的に「未提供」にしない。data自体のMiner署名虚偽は別の反証として扱う。

次はdefault条件にしない: private RPC timeout、Panelの否定証言、重複/無効要求への無応答、上限外要求への拒否、未同意checkpoint再計算の拒否、Panelが受領署名を出さないこと、既に受理済みのprovider応答の不再送。

actor別のnotice/response/prover-build/material/check/negative/grace cutoffを受付時に既存主契約のhard deadline内へ配置する。既存3,120/10,000の意味を無断で変更せず、新しい数値は未割当とする。`accepted_daa < cutoff`を原則とし、同一blockで締切sweepと遅れた応答を並べ替えて成功にしない。reorgで予約・notice・応答・消費を同時に巻き戻す。

機会規則を立証できない場合は帰責defaultを有効化しない。**未検証claimへの信用拒否は維持する。** 対話が終了した、問い合わせが尽きた、Panelが肯定したという理由でcoverage不足を埋めない。

## 13. Miner・Panel・Node別の資源上限と受付

### 13.1 三者の資源を別々に測り、予約する

```text
R_miner_extra = commit/hash + admitted data read/upload + retention + opted-in replay
R_panel = acquisition + reexecution + witness/tree build + proof codec + carrier + retry
R_node = authentication + relation checks + state/persist + retention + recovery
```

通常推論の時間/VRAM/IOをbaselineとし、Minerの追加分と比率を測る。Minerに隠れたfull-trace作成を入れたベンチマーク、Panel費用をMinerへ移しただけの高速化、warm-cacheだけの改善値を認定しない。

上限はper-requestだけでなく**全Panel合計・claim全期間**へ適用する。Panel要求の増殖、case/鍵替え、再起動、timeout再送で上限を再生しない。認証済みdataのcontent-addressed再利用とcanonical coalescingで二重取得を抑えるが、cold pathのbytes/workも受付可能範囲内でなければならない。

追加推論work・p95/p99/max遅延、GPU占有、CPU hash、upload、disk spill、応答window、provider委任時のMiner費用、Panelの収支を記録する。数値は本RFCでは未指定。Minerが唯一のweight保持者で全weightのcold開示が必要なら、その最悪値を明記し、余裕がなければproviderへ委任するか新claimを受付拒否する。

### 13.2 Node側の総量

profileに次を必須とする。全て有限・非ゼロ、measurements/digestとnetwork承認が必要。値がnull/UNKNOWNならarm不可。

| 区分 | 必須の上限 |
|---|---|
| 単一message/block | codec展開bytes、署名数、Merkle nodes、hash work、exact/field work、IO、更新行数 |
| claim全体 | 全operand bytes、再送/再read上限、全検査work、総disk write/read、最大relations/parts、全期間 |
| 全network/epoch | 通常検証とnegative courtを引いたpositive work/bytes/IO/retention/controlの予算 |
| 同時進行 | case数、active working sets、途中cursor行数、供給fee予約、global pending liability |
| 保存・復旧 | 証拠保持bytes×期間、snapshot/IBD、pruning boundary、reorg replay burst、unverified archive |

Node側vector `R(c)=(cpu, hash, bytes, io, peak_ram, live_storage, retention, control, replay)`をNodeがplanから導出する。Miner/Panelの自己申告値を使わない。三者それぞれの保守的cost envelopeをNodeが導出し、実測済みprofileへbindする。全future service intervalで予約合計がcap以下であることを検査し、資金予約と一体で原子的に確定する。

block per-capだけでなく総claim予算も必須。上限回避のために分割・大量並列化することを禁止する。case入替え・retryで消費した予算を消してはならない。既存claimのcourt権利を後続positive workloadが奪わない専用quotaを残す。

概算の必要条件:

```text
Σ admitted Work(c) ≤ EpochWorkBudget - BaseConsensus - ReservedNegativeCourt
Σ admitted Bytes(c) ≤ EpochByteBudget - OtherRequiredTraffic
Σ active PeakState(c) + BaseNodeRAM + ReorgReserve ≤ NodeRAMBudget
RetainedEvidence + State + IBD/ReorgReserve ≤ NodeStorageBudget
```

これらは必要条件であって、実行時間測定の代用ではない。cold cache、low-end target hardware、同時ピーク、悪意codec、DAG sibling集中、reorgとIBDが重なる試験を要する。

**総量の例（実測ではない）:** 80億個のweightを非圧縮16-bitで1回読むだけで16,000,000,000 bytes。キャッシュなしではモデルを部分配信しても、この読取総量は消えない。activation・KV・Merkle・再読込みは別。これがネットワーク予算を超えるなら、そのprofileの新claimを受付拒否する。分割したという理由で「低負荷」と報告しない。

profile追加は演算semanticsと資源の認定であり、個別modelの公開性審査・年次tier審査ではない。全登録modelに同じprofile条件を適用する。

## 14. 料金・担保・報酬

正常Courtは必須作業なので、攻撃者からslashが取れる場合だけ費用が賄える設計にしない。受付時にjob payer、Miner、Panel等のうち明示署名した支払者から実原資付き検査feeをholdする。証拠生成の実行担当がPanelであることと、その経済費用の負担者は別である。Minerの署名済み料金上限を超えて課金しない。参加者の同意・fee-only範囲・返金条件はRFC35/39を継承し、無署名の第三者へ債務を作らない。

```text
Hold_initial = Paid_unique_service + Refunded + Remaining_hold + Contractual_burn
```

同じmaterial、同じ検査scope、同じ回答carrierについて重複支払しない。サービスfeeと「正しい計算の報酬」を明確に分ける。未検証claimに支払ってよいのは既に提供された適格サービスへの契約上の対価だけであり、推論成功報酬・coinbase・work creditではない。

未完了での残資金返還、消費済みサービスfee、返還可能bond、未終了の他claim責任を分離する。計算有罪の責任、delivery defaultの有限責任、第三者の悪いpacketは別勘定。provider fee-onlyに後から無制限slashを足さない。

Panelの再計算・witness構築は検証サービスであり、Minerの仕事量を二重発行しない。正常証拠の完成feeはNode確認済みscopeに一回だけ支払う。入力data提供料・carrier費・完成報酬を分け、失敗witnessを大量提出して稼げる無制限の出来高払いにしない。

Panelが有効dataを受け取った後に放棄した場合、未実施の完成feeは支払わない。別proverへの引継ぎ・返金・当初Panelの有限責任は契約で固定する。Minerへ自動的に証拠生成義務や追加債務を移さない。

## 15. 共通Gate Cと、DAGへの早期信用流入の禁止

new rulesetでの共通predicateを設ける。

```text
PositiveCreditEligible(c, view) :=
    positive_required(c)
    AND PositivePassed(c, view)
    AND coverage_complete(c)
    AND roots_and_ruleset_match(c)
    AND all_dependencies_credit_ready(c, view)
    AND no_accepted_pending_fraud(c)
    AND existing_gate_c_approval(c, view)
    AND existing_maturity_and_liability_rules(c, view)
```

`existing_gate_c_approval`をPositivePassedだけで消さない。新attestationは公開Courtを自分で検査した結果を対象にするため、抽選者に秘密モデルの事前コピーを要求しない。抽選者が全員共謀しても、誤ったPositiveCourtはNodeで拒否される。全員が署名を拒否する場合のlivenessまでは保証しない。

少なくとも次の全consumerが同じpredicateを読む: legacy/kernel/native mining、Final後のworker/user escrow成功報酬、coinbase/model leg、PWU、receipt mint/spend、rule E参加、safe/live/provisional weight、safe frontier、settlement depth、settled anchor、EVM settlement、prefix再利用credit、SDK/RPC表示。

**carrierとしてDAGへ格納されたことと、そのclaimがブロック生成・経済的信用を得ることは別。** case登録・資料搬送・検証制御のための有限なblock容量は、別途既に有効なブロック権/BASE/制御予約で確保する。未検証claimを自分自身の搬送blockの資格・anchor乱数の健全性の根拠にしてはならない。

自分のclaimが認証されるまでそのclaimのためのblockを作れない循環を避けるため、初期起動・PALW停止時でも利用できる独立した制御搬送経路の実装と実測をactivation gateにする。heartbeatがあるというだけでは資金・bytes・実行quotaの確保とみなさない。

BLUE/RED、selected parent、mergeset arrival順、別modelのclaimにより結果を変えない。物理DAG保存は維持し、非認証PALWの安全なweight・決済creditだけを0にする。model Aが未検証でも独立model Bの有効claimを止めないことは、RFC23の独立進行fenceと実装が揃う場合に限って保証する。

## 16. 依存計算・prefix・再利用

非genesisの再利用stateは、出自claim、export identity、state root、位置、ruleset、PositivePassed/credit状態へ結び付ける。未検証prefixをrootだけでtrusted inputへ昇格させない。

初期R2では依存DAGの全祖先がcredit-readyになるまで子claimのcreditをholdする。依存先のreorg/取消しは子孫へ伝播し、無関係なclaimに伝播しない。循環依存、自己証明、異なるbranchの結果再利用、異なるmodel/tokenizer/layout間の誤ったcache共有は拒否する。

同一modelの再読込み削減は、Nodeが既に検証したimmutable bytesのローカルcacheだけに依存できる。cacheがなくても上限内で検証できるcold pathが必須。未検査のprefetch成功やprovider receiptを計算済みcacheにしない。

## 17. Reorg・再起動・Pruning・IBD

case、Miner結果root、Panel witness/attempt、source provenance、request/responseのdedupeと累積予算、cursor、coverage、beacon、fee、署名責任、credit利用を同じjournalでrollbackする。reorgでcommit後のseedが変わる確率型のケースは、古いpassを再利用しない。必要な再検査資源とgrinding試行回数を予約・計上する。exact型でも変わったinput/依存/viewを検査し直す。

snapshotにある`PositivePassed=true`だけを信じない。rootのmembershipは、その履歴で正しい計算を検査したことを新Nodeへ自動的に証明しない。

- **fresh self-verifying node:** 当該信用へ遡るための認証済み履歴・materialを入手して規則通り再実行する。モデル事前保有は不要だが資料取得は必要。
- **pruned node:** 明示された既存のpruning-point trust前提を継承する範囲と、自分で再検証した範囲を分ける。前者を本RFCによる無信頼の全履歴証明と呼ばない。
- **資料が全て失われた場合:** snapshotだけから自力検証できるとは主張しない。新Nodeは該当信用を未検証として扱い、必要な履歴を取得するまで自己検証完了と表示しない。

pending case、告発window、料金/責任horizon、最大reorg、検証可能な新規参加手順を覆うmaterial retentionをprofileへ固定する。永久の全履歴自己検証を提供する運用なら、archive層の総量・費用も別途認定する。30 GB等のNode保存目標を本文だけで達成済みにしない。

## 18. Safety論証と残余リスク

### 18.1 EXACTの場合

登録programの有限relation列をR1…RNとする。初期状態の認証と意味を正しく検査し、各Riについて以前に検査した出力以外の任意値を入力へ使えず、Riをexact再計算するなら、帰納的に全状態は登録計算と一致する。終端rootまで一致すればclaim出力も一致する。偽の出力がPositivePassedとなるには、被覆/入力binding/算術実装/終端条件のいずれかが破れている必要がある。

### 18.2 FULL_RELATION_DIRECTの場合

exactな初期/edge/control/終端検査が成立し、全matrix relationを検査するなら、偽計算の受理には固定された少なくとも1つの偽relationで§9の見逃しが必要。公開資料をNodeが直接読むため、Panelの正直さや外部検証者のモデル事前取得を確率式の前提にしない。代わりに乱数・binding・被覆・実装の条件を明示する。

### 18.3 Panel生成へ移しても安全性が保たれる条件

Minerが全traceを作ることは、`output=Exec(...)`の検査に必要な論理前提ではない。Nodeが固定model/inputから全relationを確かめ、終端をMinerの固定outputと比較すれば、witnessの作成者がPanelでも同じ帰納条件が成立する。確率型ではPanel生成witnessを乱数前に固定し、Panel交代を含む全候補・seedを上界へ数えることが必要。

Panelの偽witnessを拒否することと、Minerの計算不正の立証は別である。正直なMinerに対しPanelが故意に不正な中間値を提出する反例を必須試験とし、Miner罰則が0であることを確認する。Panelが公開可能な正しいwitnessを隠してlivenessを妨害することは残る。第三者の代替提出を許すが、全員の供給拒否を解消したとは主張しない。

### 18.4 まだ証明されていない事項

仕様の被覆compilerの正しさ、Rust実装、実LLM profileの性能、beacon上界、carrier包含、全consumerの接続、実機での復旧は本成果物では未検証。数理的なcore relationの論証と、mainnet system全体の達成を同一視しない。

保証を妨げる未解決事項を「UNKNOWNなので暫定pass」としない。armを拒否する。これにより安全な拒否は定義できるが、使いたい全LLMの処理能力まで保証したことにはならない。

## 19. 実装の分割（提案）

| Workstream | 主な成果物 | 接続先 |
|---|---|---|
| W1 被覆compiler | program→全relation、control/edge/終端、値域・cost導出 | TIR/RFC02・29 |
| W2 Authenticated material | model/trace/inputの統一opening、bounded codec、第三者提出 | shared TIR・terminal intake |
| W3 Exact checker | streaming exact、chunk state、reference evaluator差分試験 | versioned kernel |
| W4 Direct checker | integer full-relation、乱数、overflow/range、全非線形exact | 現行算術検査に限定した新profile |
| W5 Positive state fold | admission、cursor、終了、default分離、料金 | court主契約・資金契約 |
| W6 Gate integration | 統一predicate、new attestation、全reward/weight/settlement consumer | RFC08/12/18/38/40 |
| W7 Capacity/DA | 全体予約、独立carrier、global fairness、DoS | RFC16/25/26/32 |
| W8 Persistence | reorg、restart、pruned import、IBD、retention | RFC23/30/36/37 |
| W9 Panel prover | 自前再計算・witness root・provenance・positive/fraud生成・提出。署名だけのpassを不可 | SDK、Panel roles、remote pursuit |
| W10 Qualification | 2実装差分、攻撃試験、target hardware、全branch統合 | network-specific activation |
| W11 Miner responder | 結果commit、有限data API、request coalescing、provider委任、追加負担計測 | producer、SDK、artifact store |
| W12 責任分離 | witness rejection / claim refutation / source defaultの型・署名・会計分離 | court fold・UI・監査ログ |

細かなRust module名は実装時に決める。現状の`palw_security_certification_*`、`palw_kernel_roles`、court material intake、shared original reservation、claim-local、native settlementを候補として[INTEGRATION](../design/palw/positive-court/INTEGRATION.md)に列挙する。既存部品が存在することを接続完了と読み替えない。

## 20. Activation gate

`positive_court_v1`は仮の名前で、heightは未設定。全てのgateを同一候補で満たすまでarm不可。

| Gate | 必須証拠 |
|---|---|
| A01 変更承認 | RFC+ADR、PRINCIPLESの対象限定改定、旧model/tier/claim非変更確認 |
| A02 被覆完全性 | 全対応TIR op・input/control/output、最大shape、未対応拒否の証拠 |
| A03 全員共謀安全性 | 正直なmodel holderなしの検査。EXACTの帰納条件、確率型のQ/G/source含む上界 |
| A04 bounded implementation | parser・opening・field・CPU/RAM/IO/controlの最大値 |
| A05 raw measurements | target minimum node、cold/warm、全LLM job/profile、敵対ピーク、再起動 |
| A06 原子的資金・予約 | 全必要期間・全分岐・独立搬送、rollbackと重複支払防止 |
| A07 不提出判定 | coverage不足と帰責の分離、機会台帳、境界・censorship前提 |
| A08 全信用経路 | legacy/kernel/native・全tier・全readerの未検証=0を実processで確認 |
| A09 復旧 | reorg、seed変更、snapshot、pruning、IBD、独立claim不巻込み |
| A10 release | versioned wire/domain、networkごとの設定、実署名・build/CI・独立review |
| A11 Miner軽量性 | 標準経路でMinerの全trace/witness生成呼出しが0。追加CPU/GPU/bytes/保持が実測上限内 |
| A12 役割と帰責 | 正直Miner＋悪意Panel、data応答完了後のPanel放棄、checkpoint偽造・Panel交代・無効要求拒否の試験 |

現時点の判定は**A01〜A12すべて未完了**。利用者の仕様作成指示はA01全体の完了ではない。EXACTの参考モデル試験や小行列の全数検査はA02〜A12の代用ではない。

## 21. 受入試験

全て要求仕様。添付の小さな参照モデルで試したものと、実Nodeで必要な未実行試験を区別する。

| ID | 攻撃・境界 | 期待結果 |
|---|---|---|
| T01 | 正しいモデル・入力・全資料、Node事前モデルなし | 正常完了。ただしGate C未完ならcreditなし |
| T02 | 登録者・Miner・全Panel共謀、署名だけ | pass不可 |
| T03 | 正しい局所計算だけ抜粋、最後の1relation欠落 | pass不可 |
| T04 | Panel中間stateを1か所偽造し、その後は整合的に計算 | witnessを拒否。直接検査は仕様上界で検出。Minerへ自動帰責しない |
| T05 | 正しい別model・別input・別tokenizerの資料 | bindingで拒否 |
| T06 | Panel witness rootを乱数後に変更 | 拒否 |
| T07 | seedをPanelが選択、abort/reorgで選別 | 確率型arm不可またはQ/Gへ計上 |
| T08 | AB-Cがprimeの倍数 | 値域条件違反を拒否。modだけの見逃しを実証 |
| T09 | float round・overflow・saturation・NaN・signed zero | 登録semantics通り。未対応拒否 |
| T10 | logits/選択/制約/tokenizer/KV/prefixだけ不正 | exact関係で拒否 |
| T11 | 空・重複・重なり・逆順・巨大length・圧縮爆弾 | 有界に拒否、他者責任は発生させない |
| T12 | 純粋な不提出、正規機会あり | Unverified、契約条件成立時だけdefault |
| T13 | 送信済みだが検閲/配送妨害、機会不成立 | 無信用、計算有罪・根拠なしslash禁止 |
| T14 | cutoff直前・一致・直後、同一blockのFinishと反証 | deterministic。遅れた成功・二重終了なし |
| T15 | positive大量受付でnegative Courtを締め出す | 予約段階で拒否。既存権利を保持 |
| T16 | Miner停止後、provider/第三者が正常資料提出 | 限定契約と認証データで進行、資金鍵不要 |
| T17 | Gate C 41/75肯定、Positive未完 | 全consumerでcredit=0 |
| T18 | Positive通過、Gate C不足 | credit=0。前提を取り違えない |
| T19 | 同じ回答・同じexecution・同じmaterialの二重支払 | 一回だけ |
| T20 | A未検証、CがA依存、Bは独立 | A/C hold、Bは対応fence下で進行 |
| T21 | 3model siblingの全到着順、BLUE/RED差 | 同じcase/credit/state root |
| T22 | reorgでseed・input・依存が変化 | 旧passを破棄、再検査・rollback |
| T23 | pruning/IBDでpass flagだけを搬入 | 履歴検証または明示trustなしでは自己検証済みにしない |
| T24 | cold-cache実LLM、最大context、全同時case | 全資源上限・deadline内。未達profileは無効 |
| T25 | zero/簡約可能model | 計算正当性と実FLOPsを混同しない |
| T26 | 署名scope外providerへ有罪責任転嫁 | 拒否 |
| T27 | bootstrapで未検証claimの搬送をそのcreditに依存 | 循環検出。独立carrier未完成ならarm拒否 |
| T28 | 旧/新fence境界・全tier・legacy/kernel/native迂回 | 新claimは全て同じ必須gate |
| T29 | Minerに全trace/witness生成APIを用意しない正常run | Panelが再構築・提出し、Nodeが完了できる |
| T30 | 正直Miner、悪意Panelが偽witnessを署名 | witness拒否、Miner計算罰則0 |
| T31 | Miner outputは誤り、Panelが正しい全traceを作る | exact終端比較でMiner claimを反証。Panelは誤りを肩代わりしない |
| T32 | 別Panel・nonce・overlapで同じdataを大量要求 | canonical union、累積予算、冪等応答。一回の費用 |
| T33 | full witness/projectionをMinerに要求 | 要求自体を拒否。無応答defaultなし |
| T34 | Minerは全data応答済み、Panelが放棄 | Panel scope未完。Miner delivery defaultなし。claimは未検証 |
| T35 | Panelがreceiptを握り潰す | provider/直接fallbackのcanonical応答でMiner義務完了 |
| T36 | checkpointがproducer署名済みだが値が偽 | 未検証初期状態として採用しない。署名scopeに従って裁定 |
| T37 | Panel交代が既知seedに適応した新witnessを提出 | 拒否。新candidateは未知seedと試行/費用予算を要する |
| T38 | Panel takeoverが同じwitnessを再送 | 最初の帰属を維持、二重課金・Miner再応答なし |
| T39 | cold sole-holder model、要求dataがMiner上限超過 | 受付/要求拒否または事前provider委任。隠れた全量義務なし |
| T40 | Panelが正しいinputを捨て、別inputから正しいtraceを作る | input bindingで拒否 |
| T41 | 証拠生成費をMinerへ後付け、再計算を新たな採掘と計上 | 同意/上限または二重報酬規則で拒否 |
| T42 | ResultCommitのみの新routeと旧trace必須routeの混在 | 明示version/fence。旧fieldを省略して互換扱いにしない |
| T43 | 確率型の別出力だけでMinerをslash | exact negative court未完なら計算罰則なし |
| T44 | restartでrequest予算・witness attempt数を消す | journal復元、予算増加なし |

## 22. 評価と進め方

最初にEXACT_STREAMで小さい既存TIR profileの全経路を作り、被覆・開示・責任・Gate C・資源予約を閉じる。次に整数matrixのFULL_RELATION_DIRECTを、同じ全coverageのまま比較する。高速化のために検査範囲を減らしてはならない。

年次カタログ/0.3を本RFCだけで廃止しない。5条件の実装・実測・運用安全性が成立した後も、配分政策、model有用性、実work難度は別判断である。

**役割分担の核心:** Minerは本来の推論と有限なdata応答、Panelは検証用再計算・全witness構築・正常/不正Court提出、Nodeは独立検査と帰責・信用制御を担う。Minerの軽量化を実測せず「実現済み」としない。

**設計の安全性の核心:** 全員共謀でもPanelが検査範囲を選べず、Nodeが全必須関係を資料付きで検査し、未検証のまま信用が流れないこと。**実用性の核心:** その全検査の総コストを、対象LLMと実Nodeで計り、入らない仕事を受付段階で拒否すること。

## 23. 根拠・参考資料

### 同梱ZIP

[S01] [PRINCIPLES](../PRINCIPLES.md) §0、§8、§9、§11。本文と衝突する点はPOLICY-DELTAで明示。

[S02] [RFC14](0014-panel-independent-fraud-prosecution.md) §0・§2・モデル入手への不介入。

[S03] `consensus/core/src/palw_security_certification_v1.rs`、`palw_security_certification_fold_v1.rs`、`kaspad/src/palw_kernel_roles.rs`。Gate Cの署名前提、public-check未pin、モデル未保有時の処理。

[S04] [RFC26](0026-court-policy-master-contract.md) §2〜4。全体予約と、無応答/妨害の識別限界。

[S05] [RFC29](0029-owner-independent-shared-tir-query-data.md)、[RFC32](0032-shared-court-terminal-material-and-nonblocking-ingress.md)、[RFC35](0035-court-original-funded-contract-and-role-consent.md)。

[S06] [RFC23](0023-unresolved-claim-liability-and-independent-progress.md)、[RFC36](0036-authenticated-palw-state-map-and-row-access.md)、[RFC37](0037-ibd-bounded-row-stream-and-storage-generations.md)、[RFC40](0040-authenticated-prefix-work-dependencies.md)。

[S07] [RFC18](0018-dag-parallel-multi-model-consensus.md)、[RFC17](0017-model-eligibility-examination-and-renewal.md)、[RFC19](0019-annual-model-upgrade.md)。

[S08] `kaspad/src/palw_kernel_verifier.rs`（不正発見→FileProof）、`misaka/palw/remote/src/pursuit.rs`（Clean/Prosecute/Demand）、`kaspad/src/palw_kernel_roles.rs`（producerのbuild_evidence/Respond）。[R2ソース確認](../design/palw/positive-court/IMPLEMENTATION-ALIGNMENT.md)に行・digest・確認範囲を記載。既存正常署名をPositive Court実装済みとはしない。

### 外部参考資料（R1から継承。今回の責務改訂は同梱sourceと利用者指示に基づく）

[E1] OP Stack Specification, Fault Proof. https://specs.optimism.io/fault-proof/ — 入力を認証して局所stepを再実行する構造の参考。MISAKAの全範囲安全性を保証する出典ではない。

[E2] OP Stack Specification, Honest Challenger. https://specs.optimism.io/fault-proof/stage-one/honest-challenger-fdg.html — 正直なchallengerが自分のtraceと比較する点の参考。本RFCでは正常Courtを全claimに必須化するため、その外部発見を唯一の入口としない。

[E3] RFC 8174 / BCP 14. https://www.rfc-editor.org/rfc/rfc8174.html — 規範語の扱い。

[E4] Matrix Multiplication Verification Using Coding Theory, APPROX/RANDOM 2024, LIPIcs 317, Article 42. https://doi.org/10.4230/LIPIcs.APPROX/RANDOM.2024.42 — matrix verificationの背景。公開要旨を参照。§9の有限体上の条件付き上界は本RFC内で別途導出したもので、LLM全体の性能をこの論文から推定しない。
