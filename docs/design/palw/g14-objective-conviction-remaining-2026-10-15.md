# G14: 客観的裁定・slash・ブロック生成／報酬拒否までの残作業

更新日: 2026-10-15。対象workspace: `codex/integration-followup`。
確定した保存点は WIP `0909ad5add2c2d696d9a131db297f67f350d85a0`。
その後の新claim frameは未commitの作業差分であり、完成分には数えない。
本書は新経路の本番有効化や`pre`へのpushを許可するものではない。
正式なnative設定の下で経路が実行可能になることと、shipping presetのfenceを有効化する判断は区別する。

## 1. 到達すべき状態

> producerとPanel全員が共謀しても、active planがcoverする計算不正・job/input/output/state/DA違反について、
> Panel外の1人の適格なpublic bonded verifierが、producerの秘密状態やPanelの協力を使わず、
> 公開・認証済みmaterialから局所化・客観的裁定・責任追及を完遂できる。

ユーザーが示した具体例も、次のように成立させる。

1. 計算と一致しないclaimをminerが提出する。
2. 1人でも正しい検証者が、自分の登録モデルのコピーで不一致を確認する。
3. その1人が、他のPanelの承認・投票・証拠提供を必要とせず、共通の告発経路で公開証拠を提出する。
4. native courtが決定論的に違反を認定する。非応答なら、計算不正を推定せずDA/defaultを認定する。
5. 認定を実際のclaim無効化、責任のあるbondのslash、当該不正claim／失効したbondに基づくブロック生成資格・work weight・報酬の拒否へ反映する。

「正しいPanelが1つ」は、他の全員が反対しても**1人の正しい検証者から追及できる**という要件として扱う。
全Panelが不正なら、Panel外のpublic bonded verifierが同じ終局へ到達しなければならない。

### 判定してよいこと／いけないこと

- 判定対象は、実行した時間やCPU使用履歴ではなく、登録された検証関係に対する具体的な不一致・違反である。
- 計算を省略したminerでも、観測できる違反がなく結果が正しい場合、「計算しなかった」という行為だけをこの経路で立証・slashできるとはしない。今回の例は、計算と一致しないことを検証者が確認できた場合である。
- サンプリングで全不正が必ず発見されるとはしない。ただし、active planの対象である具体的な不一致を発見した後の裁定を、Panel投票へ戻してはならない。
- 正しい登録モデルのコピーを検証者が入手していることは前提とする。producerの実行時の秘密状態は不要にする。モデルの配布可否を合意で監査・処罰する機構は追加しない。
- ここでいう「立証」は、公開materialの同一性認証と局所的な決定論的裁定である。LLM計算の暗号学的な正当性証明は使用しない。[PRINCIPLES §8](../../PRINCIPLES.md)と[AGENTS.md](../../../AGENTS.md)に従う。

## 2. 現在地と、完成に数えていないもの

既存コードには、公開証拠の分割搬送、元sourceの認証、小分け取得、own-model再計算、局所predicate、native stateへの差分適用／巻き戻し、準備／Ready、再開用索引、資源quoteの部品がある。
しかし、新経路はまだnative受理・支払い・資格・徴収・終局へ一貫して接続されていない。**G14全体は未達成であり、生成・報酬拒否まで動作する状態とは扱わない。**

| 現在あるもの | 残っている境界 |
| --- | --- |
| original TIR source認証・取得・再計算・局所predicate | findingをnative courtの確定した違反／棄却へ変換し、実際の制裁と結ぶ経路が未完 |
| FullSUM allocation planner | `Complete`は部品の資源導出だけ。実際の料金徴収・担保確保・native admissionの成立ではない |
| claim単位の準備期限、Ready、literal defaultのkernel | 署名付きcarrier、native資格、通常nodeキュー、実際の徴収が未接続 |
| 再開用のprepared capture／索引 | native lease・Ready・GCとの運用接続、最大サイズと再開期限の実測は未完 |
| 新しい元claim frameの候補実装 | `0909ad5ad`後の作業途中。構造検査や署名検査だけで支払い済みstandingにはしない |
| 新control／grant carrierの設計案 | 調査時点でDTO実装なし。wire tag・独立fence・受理処理も未実装 |
| 追加した回帰テストのソース | 未実行。syntax／差分検査を、コンパイル・試験・実ネットワークの証拠に数えない |

過去の別commit・別branchの試験結果は、この保存点と作業中差分の通し検証ではない。
登録モデルからの告発試験も、起動しただけ／実行中／局所不一致の発見だけでは完成に数えない。

## 3. 実装の残作業

以下は達成条件であり、単にAPIやテスト名を追加しただけでは完了しない。

### R1. 元claimのnative受理・資金・保持を一本化する

- [ ] generic arithmetic-only TIRも含め、元のpublic FP commitment、登録class/layout、binding、source、FullSUM、絶対期限を同じ受理文脈で固定する。
- [ ] resource identityと、受理DAAを含むallocation identityを区別し、元署名・元job・元input/output/state・artifactの差し替えを拒否する。
- [ ] 実際の料金とdeposit、producerのexposure、必要なbond exit hold、source保持を確保してから、frameと準備状態をatomicにrootへ反映する。
- [ ] 構造的に正しいquote、署名、deserialize済みrow、旧ClaimBundleの支払いを、新FullSUMの支払い済みstandingに読み替えない。
- [ ] 個々のownerの追加acquisition／workと、元claimのprepaid poolを分離する。複数ownerが同じ支払いを無制限に複製できず、攻撃者の先取りで正直な1人を排除しない料金・容量規則を完成する。

### R2. 署名付き搬送と全native入口を接続する

- [ ] 元FP envelope、Open、Preparation、Acquisition、RawValidGrantを、独立したprospective fenceの下で実装する。既存claimの料金・期限は変更しない。
- [ ] claim、source、元allocation、owner固有row、reserved DAA、turn、actorを署名へ結び付ける。元allocation IDと`descriptor.allocation_id_v1(owner_contract)`を混同しない。
- [ ] bounded codec、正しいbond鍵、domain、transaction pairing、重複・再送・期限・reorgの検査を実装する。
- [ ] isolation／header検査、mempool、blockからの抽出、native fold、fee検査、import／recoveryを同じ規則で接続する。下位fenceでも旧履歴の判定を変えない。
- [ ] senderの自己申告上限ではなく、nativeに固定されたbyte／count上限で、割当て前のdecodeを制限する。

### R3. Validの資格と担保を、元署名に結び付ける

- [ ] 旧ClaimBundleに依存せず、実際にnativeで認められる**同一の署名済みValid receipt**と、新しいraw-source custody grantを照合する。
- [ ] Panel所属、Sampled、旧history grant、未credit receiptだけでは、新しいraw custody義務・slash権限を発生させない。
- [ ] grantを認める前に、実際に徴収できる担保と元horizonまでのholdを確保する。既存担保金額の規則を勝手に変えない。
- [ ] 元claim受理時はproducerの署名しかない点を扱う。後続grantの義務追加を、元期限・元planを変えずに配線する。
- [ ] late grantに、完了不可能なcold preparation期限を課さない。十分な残り準備期間、または準備済みactorのatomic grant＋Readyなど、採用する規則と資源の出所を確定して実装する。現時点では未決・未接続。
- [ ] owner Open時のliteral責任集合を確定し、後からのgrantで過去の継続処理を壊したり、過去の責任を増やしたりしない。再編時は現在のnative根拠と照合する。
- [ ] 既存grantの署名、exact receipt、credit DAAを保持し、単にsigner集合が増えたことだけで不正なcontract置換を認めない。

### R4. 1人の検証者による検出・局所化を全対象へ通す

- [ ] 正しいPanelの1人とPanel外の1人が、同じ検証関係・同じ客観的裁定へ到達する共通filerを使えるようにする。
- [ ] own-modelで得た不一致と、元claimの認証済みmaterialを結び付け、producerの秘密capture・秘密checkpoint・秘密状態の注入を排除する。
- [ ] 計算、job、input、output、stateの各違反を既存の決定論的courtへ接続する。局所findingを、そのままportableな有罪certificateと扱わない。
- [ ] claim、class、artifact、position、selector、sample／top-k等の意味、依存cone、元inputの完全なbindingを確認する。
- [ ] 部分receiptや単一leafだけで、本来必要な依存閉包の認証・再計算を済ませたことにしない。
- [ ] `Need`、一時的取得失敗、予算不足、EOF、`NoFault`、`PredicateHolds`を、計算有罪またはclaim全体の正しさへ読み替えない。

### R5. DA非応答を実際の徴収まで到達させる

- [ ] 元pledge／qualified grantから、回答義務のあるliteral signerと期限を確定する。
- [ ] PreparationDefaultと、その後のReady済みactorに対するDA defaultを分離する。known non-Readyへ不可能なDA義務を繰り返し発行しない。
- [ ] kernelのdefault effectを、nativeの既存経済規則・実bond lock・残高変更・会計へ接続する。効果オブジェクトを返すだけでは徴収完了ではない。
- [ ] 担保がなく既存helperが徴収をskipした場合を、徴収成功と数えない。必要なlockの不在を受理時に防ぎ、終局時は実際の徴収額を確認する。
- [ ] 同じ義務の徴収は1回のみとし、再送・複数告発者・再編・再開で二重徴収しない。無関係なPanel席や外部verifierを処罰しない。
- [ ] sourceが出ない場合でも、他のPanelの署名や多数決なしで、1人のpublic bonded verifierが客観的DA終局へ到達できるようにする。

### R6. 裁定結果をclaim・ブロック生成・報酬へ反映する

- [ ] 計算違反の確定から、native claim無効化と責任のあるproducer／Valid bondの実slashまでを接続する。
- [ ] 不正claimや失効したbondを使った生成資格・claim-backed work・block opportunityが、テンプレート生成だけでなく、受信blockの検証・合意受理でも拒否されることを保証する。
- [ ] work weight、Final weight、coinbase、model配分の対象額、escrow、FinalReward等、当該不正を根拠に支払う全経路を照合し、既存方針どおり拒否／失効／保留を反映する。
- [ ] 報酬・担保が経済的Finalへ逃げる前に裁定できるhold／windowを維持する。支払済み・使用済み資産を、後から存在しないclawbackで回収できる前提にしない。
- [ ] EXEC root／slice／riderでは、元claimの違反を依存するsuffix・root・支払いへ伝播させる。無関係な仕事まで無条件に無効化しない。
- [ ] 保留中の有効な告発、期限、棄却、返金、再編と再評価を同じnative状態で整合させる。正しいclaimは正常終了・支払いできるようにする。

**「slashできる」だけでは、この節は完了しない。** 生成資格／block検証と各報酬consumerの実際の拒否結果が必要。
既に受理したblockやFinal後の処理は、既存のfork-choice／liability方針へ正確に従う。文書だけで過去blockの扱いを決めたことにはしない。

### R7. 全family・最大サイズで資源と期限を閉じる

- [ ] preparationとowner workの別予算、RAM、artifact inventoryのresident tree、disk、sealed＋partial索引、IO、hash、parser、copy、演算、署名検査、carrier、court、terminal、再開をFullSUMへ反映する。
- [ ] 合計金額を払っただけでなく、各actorの実work上限へ予算を配線する。Panelが外部verifierのprotected laneを消費できないようにする。
- [ ] 固定された元preparation期限と、warm response／WorkDebit／part搬送の局所期限を両方満たす。colluderの合法な最大遅延を積み重ねても全体horizonを超えないようにする。
- [ ] 実nodeのキュー、backpressure、inclusion容量、bonded verifierの参加枠を配線し、他owner・他claim・悪性送信者の負荷で完走が壊れない条件を定める。
- [ ] 全体mapのclone／state root／GCの増幅、複数active claimの同時RAM・disk、native sweepの処理量を扱う。1rowや64件pageの上限だけで全体有界とはしない。
- [ ] 新frame、元public commitment、後続grant、delta／journalの実際のfootprintを13資源次元と照合する。新しいrowを追加した後も、以前のquoteで全量を払えたと仮定しない。
- [ ] 大きなwitnessを既存の小さいterminal上限へ黙って切り詰めない。明示的に資金化した分割搬送、またはnative incremental courtから小さいrooted certificateを作る経路を完成する。
- [ ] `Incomplete`、overflow、サイズ拒否を「そのfamilyは完成」に数えず、元の対象範囲を満たす経路を実装する。

### R8. 再開・pruning・reorgと通常node運用を閉じる

- [ ] prepared descriptorと実際のnative Ready、元material TTL、保持ticket、通常worker、native権限付きGCを結ぶ。
- [ ] Tiled reopenの全D IDs再認証が必要な場合、その時間・IO・carrier・RAMを最大条件で処理する。N再走査を避けただけで完走としない。
- [ ] process再開・複数processのcache共有・破損・partial generation・期限前後のcleanupを扱い、正しい公開materialを早期削除しない。
- [ ] canonical work counterの巻き戻しと、既に行った物理的な孤立fork作業を区別する。durable journalと有限recovery規則、または明示されたreorg前提なしに、restartで予算が復活しないと保証しない。
- [ ] fresh node、IBD、pruned importでも、元claim・資格・raw bytes・証拠・期限・責任を現在のnative状態から復元できるようにする。

## 4. 全familyの完了表を作る

以下は対象の棚卸しであり、存在する部品を再実装する指示ではない。各active planについて、実際の登録値・最大profile・predicate・公開source・court・slash・生成／報酬consumerを対応付ける。

| 対象 | 必要な確認 |
| --- | --- |
| legacy V2 Panel route／legacy計算 | 1 honest seatと全Panel共謀の両方。旧filer・held DA・Final前後からnative終局まで |
| native TIR arithmetic-only | semantic bundleがなくても、元claim受理から局所化・court・徴収・consumer拒否まで |
| TIR Flat／Tiled、dense／folded source | 元domain・全ID EOF・tile／step境界・最大サイズ。quote対応とlive route対応を混同しない |
| Constraint／job／input／output | active planが認めるselector、greedy／top-k／sampling、prompt／tokenizer／grammar等の意味を保持 |
| Prefix／state | legacy・TIRの元state、境界、carry／history、依存閉包。producer秘密checkpointを必要としない |
| Float32 route | 実際のFloat32意味とsourceに対応するcourt。整数TIRの部品があるだけで対応済みとしない |
| Tokenizer／artifact binding | claim固有の必要証拠を裁けること。モデル配布義務への置き換えはしない |
| kernel／OPV、EXEC、generative／pipeline等 | その経路で実際に有効なplanを棚卸しし、root／sliceと全経済consumerを照合する。未対応を隠して除外しない |

未統合のADR・RFC-0016実装・RFC-0017 branchは、このworkspaceへ取り込まない指定を維持する。
ただし、生成資格／配分等がそれらの正式interfaceに依存する場合は、依存と未完了consumerを記録し、G14の完成証拠として代用しない。

## 5. 実装後にまとめて行う検証

ユーザー指定の順序は、**残りのコード実装を先に揃え、その後に一括検証**。この文書作成ではbuild／testを開始していない。

| 検証 | 必須の終局／観測結果 |
| --- | --- |
| V1 コンパイル・既存回帰・境界decode | 変更一式のbuild、関連crate・既存挙動・新carrier／import／fenceの回帰。syntax検査では代替不可 |
| V2 正しい1 seat、他全員不正 | 正しい1人のfilerが不一致からnative conviction、実slash、生成／報酬拒否まで到達 |
| V3 producer＋全Panel共謀 | genesis特権・管理overrideなしの外部bond 1つ、自分のモデル、公開RPC／認証済みmaterialだけで同じ終局へ到達 |
| V4 各違反familyとhonest twin | 計算・job・input・output・state・DAそれぞれの攻撃が正しい責任へ到達し、正当なclaimは棄却・返金・正常支払い |
| V5 最大profile・資源・最大遅延 | 各familyの実最大値、全搬送、準備、再取得、最大合法遅延、容量競合で期限内終局。縮小profileや合成inputだけでは不可 |
| V6 実ネットワークと経済会計 | 通常node／mempool／block受理で実施。block／claim／receipt／court／slash／coinbase等のIDと残高、weight、生成資格の変化を記録 |
| V7 restart／IBD／pruned／reorg | 中断地点を変えて再開し、同じ裁定・1回だけの徴収・同じ最終会計へ復元。Final前後・TTL境界・他source混入も検査 |

登録モデルの告発試験は、V2／V3のnative終局と経済結果まで揃って初めて、その対応範囲の完成証拠にする。
検証ログはcommit・profile・native設定・実行command・node構成・期待値・実測値と結び付け、未完了／UNKNOWN／失敗を残す。
検出確率や経済評価は、根拠のある値だけを報告する。計測していない成功率を置かない。

## 6. 完成判定

次の全てに証拠が揃うまで、G14全体を完成としない。

- [ ] R1〜R8を対象の全active planへ接続し、未実装・休眠hook・未決の必須規則が残っていない。
- [ ] 各対象の不一致／DA違反について、1人の検証者からnativeの確定裁定と徴収まで到達している。
- [ ] 実際のblock生成資格／block検証／weight／全報酬consumerが裁定を消費している。
- [ ] 正当なclaimの正常系、最大サイズ、共謀、再開・再編を含むV1〜V7が必要範囲で通っている。
- [ ] fixtureだけのbond、テスト専用class許可、producerのprivate capture、手動state注入、fenceの不正overrideを完成証拠に使っていない。

優先順は **R1〜R3のnative受理・資格 → R4〜R6の裁定と経済consumer → R7〜R8の全範囲閉包 → V1〜V7の一括検証**。
R7の予算・期限条件はR1の元契約に必要なので、後付けで元horizonを伸ばすのではなく、受理実装と同時に固定する。

## 7. コードと関連記録

- [これまでの実装記録と未検証境界](g14-integration-followup-2026-10-15.md)
- [activation readiness matrix](activation-readiness-matrix.md): 過去の結果・他laneの状況を含む。新経路の完了証拠として無条件に転用しない。
- Source／work／Ready: `consensus/core/src/palw_tir_public_{source,owned,state,allocation}_v1.rs`
- 元claim frame候補: `consensus/core/src/palw_tir_public_claim_v1.rs`（この文書作成時は作業中）
- Custody: `consensus/core/src/palw_tir_public_custody_v1.rs`
- 局所裁定: `consensus/core/src/palw_tir_objective_v1.rs`
- Native状態・terminal: `consensus/core/src/palw_state_v2.rs`、`palw_staged_court_fold_v1.rs`、`palw_terminal_carriage_fold_v1.rs`
- Node接続: `kaspad/src/palw_tir_acquisition_driver.rs`、`misaka/palw/tir/exec/src/node/{public_source,prepared_source}.rs`
- block／経済consumer照合先: `consensus/src/pipeline/virtual_processor/processor/palw_accept.rs`、`consensus/src/processes/coinbase.rs`、`consensus/core/src/palw_weight{,_block,_cap_v1}.rs`、`palw_bond_budget_fold_v1.rs`
