# A1 → A0: source-only認証factoryの最小抽出差分案

2026-10-31。**レビュー用の接続案。未採択・未実装・無効。**
本書だけを追加し、consensus source・登録program・旧Court・wire・有効化を変更しない。
参照位置はA1 `d5bedb350` 時点。関数名を併記し、行番号が動いた場合は名前で追跡する。
以下の型名・signatureは責務を示す仮名で、共有型・wire・IDの割当ではない。

目的は、元programの未commit nodeを `eval_demanded` でexact再計算する際に、
既存G14の原資料認証を使えるようにすること。未commit i64/i128値をPanelから
受け取る入口や、架空のoutput leafは作らない。公開の `ExpandedRelation` は
計画の説明用DTOであり、sourceの読取権限・計算済みprefix・成功の証拠ではない。

## 1. 依存方向と二つの信頼境界

依存は **Node側tir-exec → consensus-core → tir-core** のまま保つ。
consensus-coreがNode側の `ExpandedPlan` をimportする案にはしない。

| 境界 | 所有者 | 認証するもの／認証しないもの |
| --- | --- | --- |
| 原資料の所属・座標・範囲 | coreのopaque read context/units | 登録bindingへのopening、dtype、元の座標、canonical source集合。計算検証済みとはしない |
| 必須target・検査済みprefix | Nodeのopaque checker state | 登録programから再導出した次relation、完了したexpanded cursorとlegacy frontier。peer DTOや保存flagから生成しない |
| author・case・branch・受付 | A0/A4の既存入口 | 元の署名済みResult/Witness、同一scope、現caseの義務と予算。上の二つをnative creditへ直接読み替えない |

coreのunitsは **membership-authenticated data only** とする。core単体に
constructibleなfrontier数値を渡しても「そのprefixの計算が検証済み」という権限は
生まれない。計算の帰納境界を保証するのはNode wrapperの非公開状態である。
新しいunits型自身は `DemandSource` を公開実装せず、外部の任意callbackやmutable mapを
受け取らない。`DemandSource` はNode所有のsealed wrapperでだけ実装する。

## 2. 現sourceの抽出位置

すべて `consensus/core/src/palw_tir_court_v1.rs`（repo相対）。

| 現位置 | 保持・抽出する責務 |
| --- | --- |
| `check_binding` :550、`check_output_leaf` :592 | binding検証とoutput leaf検証を別に保つ。新source-only入口から後者を呼ばない |
| `Units` :675、`Requests` :736 | 元資料の保持と使用unit記録。builder用fallback storeとvalidator用認証済みunitsを混在させない |
| `TirSource` :743、`DemandSource`実装 :821 | 元座標への写像、lane読取、token split、state replayとread meter。新wrapperへ必要なgeometry/accessorを抽出 |
| `authenticate_operands` :972 | 昇順・重複なし、source境界、4-byte legacy shape、元step rootへのrun opening |
| `authenticate_params` :1029、`check_inventory_operand_v1` :1105 | 元inventoryのparam/layer/offset/byte数とroot。別のsource木を作らない |
| `authenticate_prompt` :1256 | Flat/Merkleの排他、元input root、正規prompt tile |
| `authenticate_decode_pin` :1308 | 元trace scheme/root、生成ID列。新caseの固定生成IDとの一致もNodeが確認 |
| `check_carriage` :1427 | 既存leaf用の1–6段階。outputを要求するため丸ごと任意nodeへ公開しない |
| `CheckedCarriage::source/check_canonical` :1490付近/:1519 | 認証済みunitsの組立と「carried＝used」のclosure。source-onlyでも同じ集合条件を再利用 |
| `builder_prelude/store_source/assemble_carriage` :1610/:1628/:1661 | Panel側で必要集合を発見・組立。builderのstore値を検証済み値へ昇格させない |

`source_geometry.rs` の `reserved` :11、`commit` :21、`fixed` :35、`history` :60
も共有する。公開する場合はopaque contextのread-only methodに包み、一般の座標DTOを
「認証済みの依存先」というcapabilityにしない。

## 3. 提案するcore側の不透明API

新しい専用submoduleをA0が登録する。概念signatureは次の形とする。

```rust,ignore
// 仮名。field非公開、Deserializeなし。いずれも計算合格の型ではない。
pub struct AuthenticatedReadContext<'binding> { /* verified binding, ranges, index */ }
pub struct AuthenticatedUnits<'context, 'material> { /* exact units + used set + lease */ }
pub struct CanonicalReadReceipt { /* material/use identity + measured work */ }

pub fn verify_read_context<'b>(
    binding: &'b PalwTirStepBindingV1,
    expected: &RootedReadInputs, // class/artifact/job、A4が独立解決した期待値
    rules: &ReadRules,
    reservation: ReadContextReservation,
) -> Result<AuthenticatedReadContext<'b>, ReadError>;

pub fn authenticate_read_units<'c, 'm>(
    context: &'c AuthenticatedReadContext<'_>,
    material: &'m BoundedSourceCarriage, // decode済みでも値・所属は未検証
    geometry_limit: LegacyReadBoundary, // 範囲制約だけ。計算済みの証明ではない
    lease: SourceReadLease,             // 必須・有限、None/UNLIMITEDの既定なし
) -> Result<AuthenticatedUnits<'c, 'm>, ReadError>;

impl AuthenticatedUnits<'_, '_> {
    // 各accessorは呼出し前にlogical readを課金し、実際に読んだunitを記録する。
    // 原programのgeometryを再計算し、任意のbytes/index解釈を受け取らない。
    pub fn read_step(&mut self, request: StepReadRequest) -> Result<i128, ReadError>;
    pub fn read_param(&mut self, request: ParamReadRequest) -> Result<i128, ReadError>;
    pub fn read_token(&mut self, position: u32) -> Result<u32, ReadError>;
    pub fn finish(self) -> Result<CanonicalReadReceipt, ReadError>;
}
```

`RootedReadInputs/ReadRules/...` も正式型を割り当てる前の責務名である。bindingの
期待class/artifact/job・元program/layout/rulesを独立解決した値に照合し、既存binding
検証、範囲解析、inventory indexの構築を経てopaque contextを生成する。
公開fieldを持つ `PalwTirVerifiedBindingV1` の任意インスタンスやpeerの
`ExpandedRelation` を、そのまま検証済みcapabilityとして受け取らない。
carried stepはopening認証後、元 `palw_tir_leaf_interval_v1/first_outside` による
PALW-TIR-33を保つ。返るunitsの値は所属・形状・許容intervalまで確認したデータであり、
その値を実際に算術検査したという主張は、なおNodeのchecked prefixにだけ属する。

新materialは論理的に、元のstep operand row、既存param opening、元prompt carriage、
元decode pinを運ぶ。**output_opening/output_preimageは存在しない**。新しいsource-only
区別とbounded decoder・author署名対象はA0が確定する。本案はそのbyte layoutやtag、
signature domain、proof IDを定めない。既存 `ExactPart` の旧coneへ仮outputを挿入しない。

coreのgeometry limitは `[0, frontier)` の元leaf indexに対する上限として扱えるが、
frontierがNodeの現在の検査済み境界かどうかは、このAPIの返値だけでは保証しない。
Node wrapperが下記のprivate prefixから取り出した値に固定し、coreとwrapperの両方で
同じ上限を強制する。core返値を単独でcreditやprefix advanceへ使わせない。

## 4. Node側でのみ生成するtarget/prefix capability

Node側の追加moduleで、次の非公開stateと処理を持つ。

```rust,ignore
struct CheckedPrefix { /* case/branch/witness/plan, next_relation, legacy_frontier */ }
struct DerivedTarget { /* private next relation + exact target/range + prefix identity */ }
struct ExpandedSource<'c, 'm> { /* opaque units + borrowed private target/prefix */ }

// Nodeが独立導出したplanと現在のprivate stateからのみmintする。
fn derive_next_target(plan: &ExpandedPlan, state: &CheckedPrefix) -> Result<DerivedTarget, Error>;

// arbitrary callback/公開mapを受け取らない。関数内だけでDemandSourceを構築する。
fn evaluate_next_uncommitted(
    checker: &mut ExpandedChecker,
    target: DerivedTarget,
    material: &BoundedSourceCarriage,
    lease: SourceReadLease,
) -> Result<ComputedRelation, Error>;
```

signatureは概念上のもの。coreはこれらのNode型をimportしない。Node wrapperは
`ExpandedPlan` を登録class/job/有限policyから構築し直し、現在cursorのrelationを
再導出する。callerのDTOをtargetに変換して済ませず、position・occurrence・block/node・
dtype・resolved shape・lane範囲・expected legacy frontierの全一致を確認する。

`CheckedPrefix` は署名済みcontextの初期受付から空prefixとして作り、Nodeの実検査だけで
更新する。未commit項目は、固定targetの `eval_demanded` と `units.finish()` の両方が
成功した場合だけexpanded cursorを進める。legacy項目は元のleaf indexがfrontierと
一致し、既存Courtの実検査が `NoFaultFound`、必要なlogits bridge/選択の検査も済んだ
場合だけlegacy frontierを1進める。local `NoFaultFound` を全case合格とはしない。

sourceを作る時点のprefix/targetをborrowして変更できなくし、そのsourceの寿命中は
checkerを進めない。再送・古いtarget・別case/branch/witness/plan、変更されたpolicyや
lane範囲を拒否する。DTO deserialize、保存Passed、任意frontier setter、PanelのACKから
prefixやComputedRelationを作るconstructorは公開しない。復旧は認証済みpacketと実検査の
replayで同じstateを作る。未接続なら受付拒否／未検証とし、仮の成功型を返さない。

## 5. geometry・source supplyで保持する検査

現geometryは検証済みprogramとevaluatorからの正規requestを前提とする。新しいpublic
accessorでは、計算前にpositionが実job内であること、当該positionでoccurrenceが実行
されること（prefill途中のpostを含めないこと）、node/state種別、layer、element/tile範囲を
検証する。任意の外部DTOをprivate関数へ直接流して、既存の `unreachable!` やcheckedな
呼出し元に依存した減算・index操作を公開しない。

- **commit:** programのoccurrenceからblock/layer、元node slot、元tile幅とindexを導出。
  legacy indexがchecked prefix未満で、current target自身やfuture leafでないことを確認。
  同じleaf byteを別dtype・別node・別positionへ読み替えない。元の4-byte lane規則を保持。
- **Fixed:** position 0のzeroは元state規則からだけ導出。checkpoint境界の開始stateは
  `pos-1` の元reserved coordinateを開く。非checkpointでは `StateSupply::Replay` と
  元writer再計算を使い、任意の値を「現在state」として供給しない。instance/layer/indexを
  programで検証し、checkpoint値も元interval内で既に検査したleafに限る。
- **Hist:** 元window、`h_tile`、instance/layerと行範囲を検証。完了した過去tileは元
  reserved slotのsub-row/laneへ写像し、未完のtileは `hist_row_node_v1` の元committed
  row/carryへ戻る。row位置・tile端・lane幅をcallerに選ばせない。W-1/W/W+1、ragged末尾、
  same blockの異layerを区別し、全source stepをprefix境界へ照合する。
- **carry:** `carry_in_node_v1` が示す直前occurrenceの元carry-outだけ。未検証prefixの
  checkpointや別layerのcarryで穴埋めしない。
- **param:** 元inventory indexのparam/layer、dtype幅、tensor名、byte offsetとexact piece長。
  原byteと元artifact openingを検査し、dtype fitを保つ。Compositeの既存読取互換性を
  抽出時に変えず、新profileでの可否は延期scopeを含めA0が別途固定する。新しいmodel審査・
  保持者/可用性審査を追加しない。
- **prompt/decode:** 固定jobのtoken-source splitを使う。FlatとMerkleを混在させず、
  prompt/decode token数・token_bound・元root/pinを照合。生成IDは元witnessの固定列と
  一致させる。token pinは生成IDの所属認証であり、argmax計算を検査済みとはしない。

## 6. canonical使用集合と資源・lifetime

`finish()` はsourceをconsumeし、元 `CheckedCarriage::check_canonical` と同じ条件を
閉じる。stepはcarried index集合とused集合が等しいこと、paramは認証済みcarried集合を
sourceだけが読めることを保った上で使用集合と等しいこと、Flat promptは何か読む場合に
だけwhole promptを運ぶこと、Merkle promptは使用positionのtile集合とcarried tile集合が
等しいこと、decode pinは生成tokenを読む場合にだけ存在することを確認する。
余分・不足・重複資料や別sourceをrefillした場合は、coverageを進めず拒否する。

現 `TirSource.read_budget` は `Option` で、旧Courtは `None` を使う。新factoryではこれを
既定として流用しない。新wrapperは必須の有限read leaseを保持し、`source_read()` と同じく
cache hit/miss・store fetch・history経由readを含め、値検索前にlogical readを課金する。
累積claim予算を各tile/呼出し/再開/Panel交代で復活させない。失敗した認証・評価・canonical
closureも費用を記録する。`DemandWork` のelement/term、target数、source reads、hash/署名、
raw/encoded/signed bytesを別計上し、WorkLimitや資源不足を算術不正へ読み替えない。

認証decodeの前にbyte/shape/native-request preflightと予約を行う。retained所有物には
対応するleaseを保持し、借用参照がpacket/認証context/leaseより長生きしない型にする。
`Units` のstep/param clone、BTreeMap/BTreeSet、used set、leaf cache、range解析、inventory
index、target index list、i128結果、interpreter memo/pending stackの同時所有を見積もる。
inventory indexは既存 `TirInventoryOpeningVerifierV1::index_requests` 付近の導出も再利用
候補とし、encoded長だけから全memoryを推定しない。総Node RSS/実LLM費用は未測定。

validatorのunitsは `store: None` に固定する。Panel builderだけが別入口でboundedな
store-backed discoveryを使う。外部callbackから後付けの値を読み、最初のcanonical集合を
迂回できるvalidatorを作らない。Nodeのwide計算結果を将来cacheする場合も、同一context/
rangeの計算済み値を保持する別のopaque cacheとして設計し、今回のfactoryへmapを追加しない。

## 7. A0所有の最小file差分案と回帰条件

1. `palw_tir_court_v1` 配下へ専用認証units submoduleを追加し、A0が登録する。
   まず既存のoperand/param/prompt/decode認証とcanonical-set比較の内部helperを抽出する。
   `check_carriage` は既存のbinding→output→ranges→output interval→carriage→operand
   intervalの順に同じhelperを呼び、旧verdict/evidence ID/旧bytes/例外分類を保持する。
2. 新しいpublic opaque read context/units/accessorだけを追加する。source-only入口は
   output leafを要求せず、targetを認証したという結果も返さない。既存 `TirSource` の旧入口、
   `evaluate_leaf`、dissectionの `supplied` とbuilder fallbackの意味を変更しない。
3. 元geometry選択をopaque contextのread-only accessorへ薄く接続する。`source_geometry`
   のFixed/Hist/replay/carry分岐を新実装で複製せず、元関数を呼ぶ。prefixの計算済み判定は
   Node側sealed wrapperに残す。
4. source-only bounded decoderとouter Result/Witness/author binding、derivation識別、
   予約quoteをA0が別途確定する。Nodeのpure expanded planとauth factoryが未接続の間は
   新derivationを一切受理しない。既存all-node-commit IDを流用しない。
5. A1はNode側のsealed wrapper/checked prefix、A3はsource-only builder、A4/A6は受付/
   cumulative budget/journalを接続する。同じcandidateで欠落/余分/重複source、fake frontier、
   未来leaf、別position/layer/branch/target、未認証wide map、token差替え、source quota、
   closure失敗、再起動/reorgの予算復活を拒否する。

旧Courtは既存golden bytes・verdict/evidence ID・fault順序を差分試験する。原資料の元root/
座標、state replayとpartial-history geometryは既存fixtureに対して比較し、新factoryの
carried/used集合と実評価を独立に確認する。pure plan試験や既存算術diffだけを、公開認証
入口・実process・実LLM・native creditの接続完了とは報告しない。有効化は変更しない。
