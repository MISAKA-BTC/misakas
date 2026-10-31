# A1 → A0: supplementary Panel recordを使う全relation exact prototype案

2026-10-31。**LOCAL typed prototypeを実装・静的レビュー済み。正式契約・native schemaは未採択・
未割当・未有効化。** 現在の実装はrepo相対
`consensus/core/src/palw_positive_check_v2/full_relation.rs` と
`consensus/core/src/palw_tir_court_v1/source_only.rs`。本書は初期案から更新した接続記録で、
試験結果・受入状態は親candidateの検証記録で別に報告する。
profile値、ID、hash/signature domain、magic、wire tag、正式byte field順序、native受付、
有効化は割り当てない。既存all-node-commit checkerとprimitive execution-record codecの
受理意味も変更しない。typed prototypeの存在を、正式wire・実process・実LLM・nativeの
全経路が接続済みという主張にしない。

**最初のlocal prototypeは、有限なfull manifestを一括固定する。**
元native binding、generated IDs、署名済みResult ID、Panel author、planの有限policyと
全relationの `Vec<Vec<i128>>` claimを、canonical structural preimageへ
固定してPanelが署名する。新しいMerkle wireやopening codecは初回prototypeへ導入しない。
relation数・lane数は元native入力とpolicyから再導出し、正規vector長へ一致させる。
現structural tupleはsummaryの各countを別fieldで保存せず、このgeometryとvector長にbindする。
現local policyは24個のu64へ固定して署名入力に含め、追加されたsource認証storage/
evaluator workspace上限も取り込む。この配列を正式wireのfield番号として扱わない。
以下のsupplementary tree/codecは、将来A0が分割搬送を定める場合の契約候補として読む。
full manifestの保持費が大きいことを隠さず、実LLM対応・効率化とは別に検証する。

## 1. 目的と不変条件

元programの未commit nodeにあるi64/i128を含む全laneを、**Panel自身の追加record**へ
固定する。Nodeはその全laneを元資料からexact再計算し、固定値と比較する。追加rootや
署名は値の同一性・作者を認証するためであり、算術の正しさを確立するものではない。

- Minerは従来どおり結果commitと有限の原資料応答だけ。追加recordの生成・署名・
  全trace再構築をMinerやproviderの義務にしない。
- 登録class/program/dtype/layout、実行semantics、最大context、元model/input rootを
  変更しない。全nodeのcommit属性を書き換えない。モデル単位の審査は追加しない。
- 元のstep tree/root/leaf番号/`step_leaf_count` と4-byte legacy leaf規則を保存する。
  i64/i128をそのleafへ切り詰めたり、未commit nodeへ旧leaf indexを付けたりしない。
- expanded relation cursorと元legacy frontierを分ける。追加recordの数を元の
  `step_leaf_count` へ代入しない。旧claim・旧record・旧derivationは再解釈しない。
- 初期対象の終端は元々のi32 tiled logits・fixed-length greedy。wide中間値を扱えても
  i64 logitsの入口が接続済みとはしない。RFC-0043 / ADR-0211は2026-10-31に
  実装対象として採択済みだが、当該APIは本branchへ未接続・公開網未有効。
  旧i32経路へcast/再解釈せず、明示的に未対応として拒否する。

先行する `A1-FULL-NODE-EXACT-DRAFT.md` は「Nodeが未commit値を計算し、Panel値を
受け取らない」案である。本案は**比較対象となるPanel追加recordも固定する**別の接続案。
本案のrecordが欠けた場合に、同じcandidateを自動的に先行案へ落として合格させない。
A0がprofile/derivationを明示的に分け、どの条件で受付したかを固定する。

## 2. 既存recordと追加recordの役割

| 対象 | 固定する資料 | Nodeが行う検査 |
| --- | --- | --- |
| 元のcommit node | 元step treeの既存leaf/opening | 元の局所Courtによるexact再計算と全target lane比較 |
| 元の未commit node | localでは署名済みfull manifestの全claim。将来はsupplementary tile/opening候補 | source-only exact評価との全lane比較。wide値も元dtypeのまま |
| Fixed checkpoint / Hist tile | 元のreserved leaf/opening | 元state/history geometryと先行した検査済みprefixから再計算 |
| carry | 元の直前occurrenceのcommitted carry-out | 元instance/layer/contextとprefixを確認。追加recordを任意carryとして使わない |
| logits / generated IDs | 元のlogits trace/pin・候補出力root | 全語彙laneとのbridge、greedy/最小index tie、元jobの出力root |

追加recordはwide nodeだけでなく、**元programのすべての未commit node**を対象にする。
未commitのi8/i16/i32/idxも省略しない。元commit nodeとstate/historyは旧treeで認証する
ため、同じ値を新treeへ重複格納する必要はない。これらを合わせて全必須relationを覆う。
追加評価や二重保持を別の推論仕事としてcreditへ上乗せしない。

local full manifestはlegacy relationのclaim値も保持し、canonical expanded scheduleの
1項目に1つの値vectorを対応させる。Nodeの直接評価、元legacy openingの値、必要な
state/history/logits bridgeをすべて整合させる。将来の分割codecではlegacy値の重複格納を
避け、未commit値だけの追加treeと元legacy treeを併用する余地がある。この二つの保持
形式を同じbyte形式や同じrecord IDとして扱わない。

## 3. A0が確定する追加契約

以下は論理field候補で、正式型やIDの定義ではない。

| 接続箇所 | 固定する内容 |
| --- | --- |
| profile / derivation | 元programの全node、position/occurrence順、未commit tile幅、legacy項目の位置、dtype/shape、全終端条件。現all-node-commit IDは保存 |
| supplementary commitment | scope、Result、Panel author、attempt、profile/derivation/plan、元native execution root、candidate output、元provenance、追加record root、各count、codec revision |
| supplementary record metadata | 元class/artifact/program/job/inputのidentityを既存Result/planへbindし、legacy count・expanded count・追加tile countを明確に区別 |
| supplementary tile | expanded relation ordinal、追加record内ordinal、position、occurrence、node、元dtype、resolved shape、first lane、lane count、exact lane bytes |
| source-only material | 元step operand row、原param opening、prompt、decode pin。output leafを含まない別payloadとして既存coneと識別 |
| carrier | 固定したPanel record authorが署名したexact packetと追加commitmentの対応。同一candidateの値/root差替えを拒否 |
| 資源と復旧 | 全lane値の保持/構築/opening/送信、署名、Node評価/認証、metadata、replay、再送・試行上限を同一caseで累積 |

これは全範囲を**exact**評価するprototypeである。既存enumの `FullRelationDirectV1` は
予約された識別子で、現在のshape validatorは `UnsupportedProfile` として拒否する。
型が存在するだけで本案に対応しているとは扱わず、近道として再利用しない。既存
`coverage_plan_id` や `PanelWitnessCommit` にどこまで新derivationを追加するか、
追加の署名済みobjectが必要かはA0が明示決定する。未割当のまま現入口で受理しない。

### localな署名入力とhash依存

local prototypeは呼出し側が固定する `PrototypeDomains`（仮名）を検査設定として受け取る
案とする。hash keyは非空・64 bytes以下、ML-DSA contextは非空・255 bytes以下とする。
固定値・正式domainを本書では割り当てない。peerが任意domainを選んでnativeの署名認証を
すり抜ける入口にはしない。設定をprototype context/progress identityにも固定し、異なる
設定で同じ進捗を使い回さない。native dispatchにこの任意設定のAPIを接続しない。

元Result → 検証済み元native binding → 有限full manifest → local digest → Panel署名の順。
署名対象preimageに自分自身のdigest/署名を含めない。将来のMerkle形式でも同様に、
tileは元Result/author/plan/native rootとcanonical座標にbindするが、**自分自身のrootを
含む追加commitment IDを、そのrootのleaf preimageへ入れない**。

既存 `PanelWitnessCommit.execution_record` は元native execution rootを表している。
このfieldへ新record digestを黙って代入しない。旧 `PanelWitnessCommit::profile` は
expanded full relation用としては未対応のまま保ち、local prototypeでは新Panel recordの
署名と元Miner Resultの署名を独立に認証する。A0が新しい正式record/witness形式を定める。
追加recordは最初のrelation検査より前に
固定し、進行途中で変更しない。rootやprofileの変更で新しい無料case/試行枠を作らない。
以下のpairは、署名済みMiner Resultと署名済み追加Panel recordの組を指す。
旧Panel Witnessを新profileに対応済みと見なす意味ではない。

### full manifestの有限性を署名検査前に確認する

入力が既に型付き `Vec<Vec<i128>>` であっても、無制限のrecordをhash/cloneしない。
まず認証済み元native入力を照合し、登録programからplanを再導出する。outer claim数が
planの全relation数と一致し、各vector長がそのrelationの正規lane数と一致することを確認。
各値を元dtypeの範囲へ照合し、wide値をnarrowとして丸めたりcastしたりしない。

aggregate lane数、16 bytes/lane、outer/nested Vec metadata、実capacity、frozen record/
native入力/planのclone、canonical hashing bufferの同時所有をcheckedに導出し、有限上限へ
収まる場合だけ固定する。過大な余剰capacityも無視しない。原byte幅のwire見積りと、
i128格納のheap見積りを混同しない。local structural encodingは固定field/順序で一意に
定める必要があるが、その選択を正式wire番号やnative codecの割当として扱わない。

## 4. Typed pure APIの構成案

依存方向はpure core → TIR leaf/shared contractのまま。model runtimeをconsensusへ
引き込まない。現在のLOCAL APIは次のとおり。型のfieldは非公開で、shared wire DTOではない。

| 現API | 責務 |
| --- | --- |
| `PrototypeDomains::local_only` | 呼出し側が固定する有限local設定。正式domain定数やpeer選択値を持たない |
| `FullRelationPreparation::authenticate` | 元Miner署名・rooted native入力・expanded plan・policyの照合 |
| `FullRelationPreparation::freeze_record` | 全正規claim、元binding/生成ID/author/policyを有限manifestとして固定 |
| `PrototypePanelRecord::signing_digest/signature_context` | Panelが独立に署名するlocal構造の入力。旧Witness/nativeのIDではない |
| `FullRelationPreparation::begin` | 不変recordとPanel署名を認証してcheckerを構築。引数をconsumeするlocal constructor |
| `FullRelationChecker::submit/progress/finish` | 元sourceからexact評価・全claim比較・private進捗・終端比較。native receipt/creditを生成しない |

以下は将来のbounded delivery/分割codecに必要な責務を示す仮APIであり、現APIと同名の
実装が存在するという意味ではない。

```rust,ignore
struct FullCheckPreparation { /* independently rooted Result + original native inputs + expanded plan */ }
struct FrozenPanelRecord { /* authenticated local full manifest + domains + exact geometry/counts */ }
struct FullCheckProgress { /* private expanded cursor, checked legacy frontier, budgets, argmax state */ }
struct CheckedFullTerminal { /* immutable scoped comparison fact, not credit authority */ }

// local full manifestのclaimはFrozenPanelRecordからprivate cursorで選ぶ。
// 下記のsupplementary opening付き形状は将来の分割codecの責務例。
enum FullRelationMaterial<'a> {
    Legacy { /* existing cone, optional original logits bridge */ },
    Uncommitted {
        // Declared coordinates are rederived, not authority.
        tile: BorrowedSupplementaryTile<'a>,
        opening: BorrowedSupplementaryOpening<'a>,
        source: TirSourceOnlyMaterialV1<'a>,
    },
}

fn prepare_full_check(rooted_inputs, result, limits) -> Result<FullCheckPreparation, Error>;
fn authenticate_panel_record(&FullCheckPreparation, local_domains, signed_full_manifest)
    -> Result<FrozenPanelRecord, RecordDeliveryError>;
fn start_full_check(&FullCheckPreparation, &FrozenPanelRecord) -> Result<FullCheckProgress, Error>;
fn check_next_full_relation(context, &mut FullCheckProgress, author_authenticated_material)
    -> Result<(), Error>;
fn finish_full_check(context, FullCheckProgress) -> Result<CheckedFullTerminal, Error>;
```

peer提供の登録program/jobを正本として採用せず、元の認証済みnative値へ照合する。
checkerの進捗はfield非公開の内部stateで、公開snapshotを復元用constructorへ渡せない。
将来のrecord deliveryでdecode/署名/bindingが失敗しても、単独では進行中candidateを
拒否しない。現 `begin` はpreparationとrecordをconsumeするため、このconstructorに
借用recordのretry性までは主張しない。`submit` の配送失敗では、残り予算の範囲で同cursorの
source資料を再試行できる。
A4が受理・固定したpairと、正規authorに帰属するrelation packetを検査する入口を分ける。

`ExpandedRelation` はconstructible DTOなので、この値を渡すだけで検査対象を選ばせない。
checkerは固定planとprivate cursorから次relationを再導出し、tileの全field、追加ordinal、
legacy frontierが一致することを確認する。PlanIdもPanelのrelation listから作らない。

## 5. source-only APIとの接続

現在のcore APIは、borrowed `TirSourceOnlyMaterialV1`、`authenticate_source_only_v1`、
one-use `AuthenticatedTirSourceOnlyV1::evaluate_node/evaluate_legacy`。
sourceは元bindingのstep/param/prompt/decode資料だけを認証し、元programのtargetを
`eval_demanded` で評価する。任意store/callback、仮output leaf、Panelのwide `supplied`
mapを持たず、canonical read-set closureを必須とする。返るdtype/shape/values/work/
source readsは局所評価の事実であり、全claim合格や検査済みprefixの証明ではない。

full manifestの署名はclaim値と元bindingを固定するが、**後から届くsource-only proof
packetのbyteまで署名したことにはならない**。source packetを正規authorへ別途認証して
いない場合、壊れたopening・資料欠落・canonical集合の不成立だけでPanel recordを拒否
しない。配送拒否／未検証として同cursorで正しい資料を再試行できるようにし、使用した
work/bytesは累積する。認証済み原資料からexact評価が成功し、その値と署名済みclaimが
異なる場合に初めてPanel record自体の不一致を確定できる。raw legacy cone/traceの
配送破損にも同じ区別を適用する。localな材料署名を追加する場合もA0正式wireとは分ける。

full checkerの未commit項目の順序は次のとおり。

1. context/署名済みpair/author packet、現在のNode導出relationと有限予算を照合する。
2. localでは、署名で固定したfull manifestの正規claimをprivate cursorで選ぶ。
   将来の分割形式ではtileが追加rootのcanonical座標に属することを確認する。
   いずれもrecord署名・所属認証だけでは通さない。
3. 元bindingからsourceを認証し、**現在のprivate checked legacy prefix未満**のstep値だけを
   供給させる。公開の数値frontierやrelation DTOを計算済みという権限にしない。
4. 元node/dtype/shape/rangeの全target laneをexact評価する。target自身・未来値・別layer/
   position/branch、追加rootのwide値を入力として供給しない。
5. 使用資料集合を閉じ、すべての計算値をPanelが固定したexact値と比較する。余分/不足/
   重複資料、dtype幅切詰め、どこかのlane不一致はcoverageを進めない。
6. 全チェック成功時だけexpanded cursorを進める。legacy項目の実検査が完了した時だけ
   legacy frontierを進める。追加node項目で元leaf数を増やさない。

補助rootへのmembershipが有効でも、偽wide値をsourceとして使うと自己整合する偽計算を
通す危険がある。初期prototypeでは追加値を**比較対象としてのみ**扱う。将来Node計算済み
cacheを入れる場合も同一target/rangeをexact検査したprivate cacheに限り、別の最適化とする。

## 6. codecに必要な正規性と境界

実際のbyte順・tag・hash domainはA0の割当を待つ。必要なcodecの性質は以下。

- 旧primitive record/ExactPartと明確に区別し、profile/derivationでdispatchする。
  expanded ordinalを旧leaf indexとして旧readerへ渡さない。未対応形式は拒否する。
- tileのdtypeは元nodeから導出し、wireでは**元の整数幅**を保持する案とする。
  i64は8 bytes、i128は16 bytesのlittle-endian two's complement、idxはu32として扱う。
  その他の整数幅も元宣言に従う。比較時は正しくi128へ拡張する。旧4-byte leaf規則とは別。
- shape/rank/position/node/lane範囲をNode導出値へ一致させ、`lane_count × dtype.width()`、
  総bytes、count、proof depth/兄弟数、展開heapをcheckedに導出してからallocする。
  同じbyte列の別dtype/別shape/別offsetへの差替え、余剰末尾、短い値、過長表現を拒否。
- supplementary treeのleaf数・順序はNodeが導出する全未commit tile列に一致させる。
  range openingの重複/欠番や別treeサイズを許さない。空recordを認めるか旧profileだけへ
  routeするかはA0が固定し、未定義のzero rootを「全被覆済み」と見なさない。
- source-only物理資料は元のG14 root/座標を使う。新しい原weight/inputのsource木を作らない。
  supplementary treeはPanel中間値のrecordだけで、登録modelや供給契約を置換しない。
- signed packetはそのexact bodyを固定Panel record authorへ帰属させる。未認証relay改変は配送拒否。
  relay交代では同じ署名済みpacketを運ぶ。新規署名・委任の意味を勝手に足さない。

## 7. Panel構築、費用、復旧、帰責

Panelは独立実行から元legacy recordと追加recordを作り、全未commit出力tileを固定する。
現在のcommit-only captureだけでwide未commit tensorを採取できるとは主張しない。
全node値を観測するcapture hook、streaming root構築、opening生成/保持の実装が必要。
再実行が追加で必要ならPanel費用へ計上し、「一回の推論で無料」と報告しない。値をdiskへ
streamしても総byte・IO・保持・root生成の負担は残る。Minerへその構築を要求しない。

Nodeはwire幅と別にi128結果の16 bytes/laneを予約する。source preflight/保持物、program/
range/index、metadata、hash/opening、署名、evaluator memo/stack、target vector、同時cloneを
別に計上する。各laneのreduction全体が有限な呼出し枠に入るjobだけを初期対象とし、
WorkLimitは未検証。失敗時も使用量または事前に予約した保守的上限を累積し、試行・
tile・process再起動で予算を復活させない。総RSS/CPU/実LLMの実測認定は未完了。

現LOCAL候補のsource予約は `max_auth_storage_bytes` と `max_workspace_bytes` を分ける。
前者はsource map/byte/cache/opening-copyのlogical quoteを認証helperの割当前に制限する。
後者はevaluatorのcontext/node metadata、memo/state map、pending/stack、reduction配列、
target index/resultを含む保守的typed quoteを評価前に制限し、結果から
`workspace_bytes()` を読めるようにする。これらはallocator leaseや物理RSSの測定ではない。
program decode、range/inventory解析、Merkle検証scratchなどの除外部分は別予約が必要。
`max_record_heap_bytes` もrecord所有のnative binding/claims容量/policy/domainsに限る。
Preparation所有物・plan解析を含むNode全体の同時heap上限ではない。A0のnative接続では
これらの重なった所有・scratchを実reservationへまとめるまで、公開profileを受理しない。

geometryも累積対象とする。record固定/context用の保守的な基礎予約に加え、各
`submit` のrelation再導出を実行前に課金する。配送失敗のretryで同じlookupを無料にせず、
geometry/material/elements/terms/source-readの全予約が通ってからcounterを更新する。

A6は固定pair、author packet、expanded cursor、legacy frontier、費用・試行・責任を同じ
branchでreplay/undoする。保存された成功flagだけでは復元しない。hash検証済みの追加
recordを計算検証済みcacheへ読み替えない。scope/root変更で新しい無料予算を作らない。

| 所見 | 意味 |
| --- | --- |
| 未認証record/packet破損 | 配送拒否。同じ元candidateのretryを妨げず、Miner/Panelの算術有罪にしない |
| 受理済みpairにbindするauthor-authenticated偽tile/不正証拠 | そのPanel candidateの拒否。Minerへ責任転嫁しない |
| 全relationを完了できない／資源・期限不足 | 未検証。正常・不正を推定しない |
| 全exact検査・終端が完了してMiner署名出力と一致 | scopedな計算一致事実。Gate C/信用/支払は別条件 |
| 全exact検査・正しい終端がMiner署名出力と不一致 | A4が扱うchecked mismatch。Panelの偽中間値だけではこの結果を作らない |

帰責・罰・支払の実行はA4/A5の既存境界を使い、source factoryやmetadata decoderへ
複製しない。追加recordを登録modelの審査や新しいtier認定として使わない。

## 8. 最小fixtureと受入条件

元programの例は `[2,3] → i64 × 2^32 → i128二乗 = [2^66, 9×2^64]` の後、lane0だけを
使ってi32終端へ狭めるもの。未消費lane1を `9×2^64+1` としてPanelがcommitしても、
元の最終結果は変わらない。新recordの署名が正しく、元legacy出力も一致する
このcandidateを、Nodeのlane1 exact評価が拒否しなければならない。
現在のtyped fixtureは `tests/positive_full_relation.rs` の `wide_fixture` に追加されている。
未消費i128 laneの署名済み偽値を対象とする試験も同fileにある。まだ登録modelや
公開claimへ接続したことにはならず、compile/実行結果は親candidateのraw logで別に記録する。

受入試験は全relation数/最後のlane、未消費wide/narrow lane、dtype/shape/位置、別layer/
input/model、root/pair差替え、same-output偽中間値、canonical source余剰/不足、future
leaf、state/historyのW-1/W/W+1・ragged末尾、argmax負けlane、署名/relay破損、試行/予算/
復旧を含める。whole-node oracleとの算術差分と、小さい手計算期待値は独立に用意する。

LOCAL typed入力に対するexact検査と、正式profileとしての統合を区別する。A0が新契約を
固定し、A1 source-only/比較器、A3全node capture/record builder、A4受付/auth carrier、
A6復旧、A7独立試験を同一candidateで接続するまで、正式profileでの全relation検査・
実LLM対応・公開網有効化の完了とは報告しない。

## 9. LOCAL prototypeの実行検証

最終の対象runは **48件成功・失敗0件**。新しいfull-relation試験9件と、既存Node／
expanded plan／primitive record／同一型portの39件を、同じ最終ソースで実行した。
新試験は独立したwhole-node Interpreterと手計算期待値を使い、未消費I128 laneの署名済み
偽値、正しいrootに属する過去・未来の不要資料、配送拒否後の再試行、Fixed／Hist／
動的H／反復layer、正負HAFZ丸めとgreedy tie、record署名・actor・policy・固定domain、
有限な認証storage／workspace／source reads／geometry、全範囲後だけのMiner出力比較を確認した。

旧Courtの7件も別scopeで成功（194.06秒）。旧entrypoint／evaluator／planner本体は
byte-identicalで、差分は専用module登録とprivate getterだけであることを確認した。
この7件や過去phaseの件数は、最終48件に加算しない。

初回の組合せrunは、新しい丸めfixture1件がrange前提に違反して失敗した。
I32 CarryInのfull-rangeに加算するfixtureを、登録前に明示Clamp付きの有効なprogramへ
修正した。既存登録programを変更したものではない。初回の失敗raw logを保存し、
成功runへ書き換えていない。

対象packageのlibrary／新試験Clippyは `--no-deps -- -D warnings` で成功。
core全体のstrict Clippyは既知の `palw_model_panel_one_v1.rs:92` の `clone_on_copy` と
`palw_state_v2/chain_state.rs:3728` の `nonminimal_bool` で失敗し、新prototypeへの
指摘は残っていない。文書検査はno-new-regressions、既知43件・新規0件。
原log、対象command、最終source hash、scopeの機械可読記録は
`validation/full-relation/` に保存する。実LLM性能・total Node RSS・native受付・正式profile・
wire割当・公開有効化は、この検証から導出しない。

## 10. canonical job tokenizerと登録tokenizerの分離

canonical TIR jobのconstructor `palw_tir_job_context_v1` はjobのtokenizerを0にする。
登録classのtokenizerはclass IDへ固定された別fieldであり、両者の数値一致を求めない。
ExpandedPlanの余分なequalityだけを除き、登録値へjobを書き換える処理は追加しない。

FullRelationの `Result.tokenizer == actual job.tokenizer`、実jobのcontext hash、登録class ID／
artifact／program、record bindingの元class／元jobとの完全一致は維持する。source-onlyの
所属認証と元rootへのopeningも変えない。classとjobを一貫して別値へ差し替えても、
元のrooted／署名済みResultや準備済みrecordを流用できることにはならない。

登録tokenizerが非0・job tokenizerが0のままcaptureと資料構築を行う追加fixtureを用意し、
既存の非0一致fixtureも残す。この修正の検証は `validation/canonical-tokenizer/` に別記し、
前節48件の履歴を更新・加算しない。初期CoveragePlan親の同種hunkはA3担当で、本A1差分に
含めない。`palw_public_binding_v1` の照合はA0が確認した既存free-promptの意味を維持し、
変更しない。canonical新modeの別metadata／assignment routeやwide profile解決のv2 APIは
今回のnarrow tokenizer修正へ混ぜない。新native job authorityや新profileの採択・有効化を
意味しない。

追加検証はexpanded plan 5件、full relation 11件の**計16件成功・失敗0件**。
両test targetのpackage-only Clippyも `--no-deps -- -D warnings` で成功した。
新規caseはcanonical-tokenizer0のgeometryと実capture／source-only全検査、および
旧expected execution／署名済みResultを固定したclass・job・record binding差替えの拒否。
この件数を過去の48件やA3の別scopeへ加算しない。
