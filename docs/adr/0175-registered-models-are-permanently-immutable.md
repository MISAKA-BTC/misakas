# ADR-0175 — 新規モデル登録は自由、登録モデルは永久不変

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


**Status:** Accepted design, 2026-10-09. `pre`に独立fenceとfold/CLIを実装済み（休眠）。2026-10-10の[検証記録](evidence/0175-immutable-model-registration-2026-10-10.md)を参照。既存ネットワークの有効化高さは設定しない。

## 決定

MISAKAでは誰でも新しいモデルを継続して登録できる。登録が成立したモデルの重み、計算グラフ、tokenizer、実行仕様、canonical artifact root、kernelとverification planのbinding、登録IDは永久に不変である。登録者、開発者、Panel、miner、運営者、ガバナンス、自己改善プロトコルにも既存登録を更新・上書き・置換する権限を与えない。

fine-tuning、蒸留、LoRA、新しい重み、異なるtokenizer・実行仕様・kernel/planの成果物は、新しい独立モデルとして登録する。新規登録には自身の登録ID、固定root、検証責任、報酬資格、PositionとAMMがある。派生元への参照は来歴であり、親を書き換える権限ではない。登録の自由は既存の客観的admission、資源上限、手数料・担保条件を免除しない。配布や報酬資格の成立を登録成立と混同しない。

モデルAの改善からBが生まれても、AのPosition、AMMのreserve、価格、holder残高をBへ付け替えてはならない。Bに投資・利用する場合はB自身の市場と登録を明示的に選ぶ。Aは永久にAのままである。

| 状態・操作 | 新規則 |
| --- | --- |
| 重み、graph、tokenizer、artifact root、実行/kernel/verification binding、登録ID | 永久不変 |
| 同一登録へのV2/V3 publish、preview、promotion、withdraw、head rollback/自動切替 | 拒否 |
| 改善版の独立登録、来歴の記録、評価・学習・dataset処理 | 許可。親の内容・市場を変更しない |
| 担保、slash、報酬資格の停止・回復、利用量、運用担当者 | 別の合意規則で更新可能 |
| Seeder、配布先、同じ内容の追加包装 | off-chainの任意運用。合意上のavailability lease/取得資格を要求しない。任意の受理済みmanifest/infohash/root bindingは不変とし、別包装は別binding IDで固定rootとの対応を認証する。登録内容を上書きしない |
| EARLY_VERSION | 旧wire bitを保持し、新規則下の新しい宣言では拒否 |
| PRIVATE_BETA / サービス特典 | 当該固定モデルへのアクセス・運用サービス。改善版への権利を含めない |

## 計算classと登録ID

`class_id`は既存のexecution/admission classを識別する。legacy profileのclassはgraphに由来し、TIR/Genの既存class IDはprogram/pipeline、layout、tokenizer、artifact root等も含む。この既存hash式を変更して履歴のclassを再採番しない。graphのみの識別子はlegacy profile IDまたはTIR `graph_ir_root`として区別できる。

`ModelLineFounded`が新fence以降に作る登録ID（stateの`line_id`）は、次のdomain-separated hashで固定する。

```text
model_registration_id_v1 = H64(key "misaka-palw/model-registration/id/v1",
  class_id || canonical_artifact_root || borsh(founder) || le32(name.len) || name)
```

同じ計算graph、founder、nameでも異なるrootは別登録になる。実行・tokenizer・kernelの意味は変更不能なclass定義とcanonical artifactに拘束する。kernelを拡張する場合は新たなclass/登録を使い、稼働中のprimitive semanticsを置換しない。宣言だけの`runtime_hash`等を検証済み仕様とみなさない。

既存のfounding lineの`line_id == class_id`とV1のline IDは互換aliasとして保持する。既存class登録carriageも自身の固定rootを持つ独立のfounding登録として保持する。既存ID・class hash・Positionの再採番は行わない。同一class内の追加登録は既存のroot ownership indexで一意性を検査する。一つのartifactを別の計算classで使う既存規則は維持し、同一classの他登録からrootを奪えない。

## 有効化と履歴

新しい`Params::palw_model_immutable_v1: Option<ForkActivation>`を使う。ConsensusV2、model lines、artifact root ownershipがその高さまでに有効であることを必要とする。全shipped presetで`None`。既存のimprovement、typed artifact、kernel、transportの休眠fenceをこの変更で有効化しない。

新fenceより前のwire object、enum tag、署名message、state row、snapshot、履歴foldは保持する。`None`/`Some(never())`は既存fingerprintを変えない。将来の高さはparams/scheduleにcommitし、将来fenceのidentity正規化も他のfenceと同じ方式を使う。reorgのdelta undoで過去の状態を再現する。

有効化時に存在するモデルは、その時点で確定している登録・versionと履歴を保つ。過去のV2をV1へ戻したり、旧previewを別の登録へ勝手に移したりしない。その後のpublish/promote/withdraw、lineage rollbackは名前を指定して決定論的に拒否する。既存preview等の履歴上の資格・期限は旧定義のままで、変更権限を再開しない。認識できるが拒否されたlifecycle objectは通常のacceptance規則に従って適用せず、carrying blockは成立する。

休眠classの再登録による運用復帰は、既存root、PWU規則、slash単位、fused semantics、registrantが一致する場合だけ認め、元の登録DAAを維持する。内容が変わる再登録は拒否する。shard verification planは新classの登録block内に確定させ、古いclassへの後付けを拒否する。提供者の加入・脱退やleaseの更新とは分離する。

## 自己改善

学習・評価は継続できる。旧`Promoted`（outcome tag 0）、`NoChange`（tag 1）を維持し、`CandidateSelected`をtag 2として追加する。新fence下の評価勝者は既に独立admissionを受けたcandidate classを指し、親の`head`、version、Position、AMMを変更しない。候補が利用・報酬の対象になるには候補自身の登録と資格が必要である。評価勝利だけで資格を継承しない。

既存の評価料、bond返還、trainer/dataset grant、vestingを旧資金保存則に従って清算する。未決epochがfenceを跨いでも勝者選択と支払は完了し、headは動かさない。opt-out/re-opt-inでも既存headを別のclassへ切り替えない。既存のrollback用wire payloadと履歴は残すが、新規則下では処理しない。

## 固定同一性・配布不介入と報酬配分の境界 — 2026-10-10後続改定

[ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。chainはmodel取得可否・Seeder登録/独立性・配布量/速度を管理せず、
旧必須Torrent、独立Bonded Seeder、PoR/Full Fetch/lease/TRDC/FPRと取得不履行に基づく資格/weight停止を撤回する。
model配布は任意のTorrent/mirror/共有契約で行い、取得失敗・非公開だけでSlash/Final延長を起動しない。

固定Model ID/root、weights/tokenizer/config/specは不変とし、変更は新Model IDとして登録する。
取得bytesは認証と全量復元rootで照合し、claimも同じroot/specをbindする。
任意のinfohash対応を登録する場合はimmutable bindingとし、必須取得経路にしない。
claim固有state/trace/output/witnessの有限な証拠責任は維持するが、modelの全量/反復range公開へ転用しない。
必要operandの認証と許可unit/累積scopeをADR177 D2で確定し、modelを得たfresh non-seat verifierの
計算反証/localization/exact courtを試験する。全model保有者拒否時の取得保証は撤回する。

重複しないminer拘束元本`S_m=sum_b C_{b,m}`からmodelのcoinbase予算を`f(S_m)`で配分する。
同じ元本をclaim数/rho/複数modelで増幅せず、総発行予算と個別bondのQ/B/R/F cap・共通DAA拘束を維持する。
資本預託だけでは支払わない。model別配分はblock頻度・DAA・fork choiceを自動変更しない。
公開参加資本が閉鎖自己資本より圧倒的に有利になることは倍率式・経済評価の未立証目標であり、
同額資本の所有者独立性をchainが識別できるとはしない。既存minerの実収入増も無条件保証しない。
non-interference、distinct-capital、三層予算、公開/閉鎖比較、回復/移行のgateはADR177 §3を用いる。
既存実装/測定・旧claim/activation記録は保持し、新配分・Panel=0は未実装/未有効化である。

## 改定対象と検証

ADR0088の同一line更新、RFC0004/spec17の自動head置換を改定する。ADR0095の将来version特典を禁止し、ADR0087/0089/0090/0091/0162の市場を固定モデルに拘束する。ADR0143、RFC0011とRFC0014/Torrentは内容bindingと運用可用性を分離する。

検証対象は、fence前の旧更新とwire tag、fence後の拒否、同一graphの独立root登録、既存履歴・Position・reserveの保持、休眠復帰と不正な上書き、既存epochの候補選択・資金保存・rollback拒否、snapshotとdelta undo、shipped fingerprintとfence scheduleである。これらの試験はtransport availabilityや新verification方式の完成証明ではない。

## MISAKA Torrent・Seeder報酬の廃止 — 2026-10-10後続改定

[ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)に従い、MISAKA Torrentの採用/統合、専用Bonded Seeder、
Seeder報酬・固定15%配分の概念を廃止する。一般的な任意配布はoff-chain運用とし、
モデル入手の合意gateやSeeder向けcoinbase legへ復活させない。過去の設計/試験は撤回前の記録として保持する。
