# A1: explicit wide step/binding V2 API

2026-11-01。Base: Root `e5e7ec22e`。Branch: `codex/a1-wide-step-v2`。
既存 `codex/positive-court-a1` を保持し、同checkoutで別branchを使用した。
RFC-0043／ADR-0211の採択済み8-byte最終logitsへ接続するsource/APIの実装である。
native受付、wire dispatch、FullRelationのI64終端受理、公開有効化は追加しない。

## 明示APIと内部geometry共有

実装は `consensus/core/src/palw_tir_step_v1/wide_v2.rs`。親moduleから次を再公開する。

```rust,ignore
PalwTirStepSpaceV2::new(&class)
PalwTirStepSpaceV2::from_program(program, info, layout)
PalwTirStepLegBuilderV2::new(&space, &original_job, class_id, leaf_cap)
palw_tir_leaf_preimage_v2(&space, &original_job, &canonical_leaf, &values)
palw_tir_leaf_values_v2(&space, &original_job, &canonical_leaf, &preimage)
verify_tir_binding_v2(&existing_binding_dto, leaf_cap)
```

V1のfloor-sum、ordinal、slot、checkpoint／history、tile geometryはprivate constructorで
内部共有する。V1 `new/from_program` はwide拒否を維持する。V2は実programを再検証し、
caller構築可能な `ProgramInfo` も再導出結果との完全一致を求める。V2のinnerはprivateで、
V1 space参照／所有値への変換やDerefを提供しない。program／info／layoutのgetterは読取専用。

exact wide scheme、最終post logits、I64 dtype、commit属性、末尾nodeを実programから照合する。
未commit I64/I128を新たな旧commit pointにしたり、state／carry／任意nodeのI64 commitを
一般許可したりしない。公開の `wide: bool` やpeerのdtypeだけで8-byte権限を得るAPIはない。

leaf encoder／readerは、indexから再導出したcanonical leafの全fieldとcallerのleafを照合する。
実際の最終post I64 logitsだけversion 2・signed i64 little-endian 8 bytes/laneを使う。
他のnode／Fixed／Histはversion 1・従来の4-byte preimageを使う。値のclip、Q24単位変更、
i32への切捨て、元program／class／layout／最大contextの書換えは行わない。

producer encoderは元dtypeと値数を厳密に確認する。disclosed readerはversion／座標／値数／
byte長を確認するが、narrow laneの区間外値を丸めたり隠したりしない。G21型の不正値を
元bytesのまま認証し、その後のCourtで検査する用途を保つ。ここでの読取はmembershipや
算術検査の合格を意味しない。

## 既存opaque metadataを再利用する理由と境界

`PalwStepTileLeafV1` はversion、座標、count、`Vec<u8>`の既存field layoutを維持する。
既存hashはversion／context／class／座標／count／length-prefixed bytesを既に固定する。
8-byte leafを既存raw hashへ渡しても、旧version-1/four-byte consumerが対応済みにはならない。
`palw_step_leg.rs` ではこのbyte-carrierの説明だけを更新し、hash本体と全domainは変更しない。
RFC-0043のwide tiled-logits `/v2` domainはRoot既存の `palw_logits_q24_v2` が担当する。
新しいstep hash domainは独断で割り当てない。

`PalwTirStepBindingV1` も既存field layoutとformat version 1を維持する。別のbinding versionや
native tagは追加しない。明示 `verify_tir_binding_v2` だけがwide program／元wide contextの
exact scheme、class ID、canonical count、元context hashとexecution rootを照合し、private-field
`PalwTirVerifiedBindingV2` を返す。旧 `verify_tir_binding_v1` のwide拒否は維持する。
DTOをdeserializeできることと、profileを受理することを混同しない。

V2 contextはordinary version-2 jobだけを対象にする。元contextをnarrow schemeへ一時的に
置き換える方法は使わない。元のnetwork、prefill／decode／最大context、trace schemeを検査し、
元contextのhashをそのまま使う。inherited-prefix v3へのwide接続は今回のscope外である。
登録tokenizerとjob tokenizerの一致を追加条件にせず、canonical jobのゼロを登録値へ書換えない。

このvalidatorはself-consistentなdata identity／geometryを確認するAPIで、署名済みMinerの
期待metadata、rooted登録権限、actual admission、計算の正しさ、native creditを認証しない。
callerはRootのprogram由来profile resolverと独立の期待class／artifact／job照合を維持する。
旧serialized factsのtiled boolだけでwide profileを推測しない。

## V1互換性と最小hardening

有効なnarrow leafのbytes／hash／geometryは変えない。V1 spaceのprogram fieldは既存APIでは
publicに変更できるため、旧builder開始時にもwide拒否を再検査する。旧lane readerでは
もともとcommitted dtypeになれないI64/I128を拒否し、4-byte値への黙示読替えを防ぐ。
i8/i16等の不正な4-byte disclosed値は従来どおり読める。これらは有効な旧narrow値の
受理変更ではなく、未対応wideを旧入口へ流さない境界の補強である。

## 資源と残る接続

leaf capは個数の上限であり、heap/work leaseではない。builderのhash VecとMerkle構築の
一時領域、class/program/ProgramInfo/layout、context、preimage/result Vecが共存する費用を
callerが呼出し前に予約する必要がある。新builderのhash Vecとwide preimageは
`try_reserve` により確保失敗を拒否する。他の共有geometry／decoder／Merkle実装まで
一律にallocation failureを捕捉するという保証ではなく、capや事前予約を免除しない。
`maximum_leaf_payload_bytes_v2` は全leafを8 bytes/laneとして保守的に見積もり、DTO metadata、hash、
Merkle scratch、allocator overhead、total Node RSSを含めない。Borsh leaf headerは26 bytes
（u16 version＋4つのu32座標＋u32 count＋Vec長u32）で、wide 4 lanesのpayload32 bytesに
加えるとDTOのserialized総長は58 bytes。native pricingはpayloadだけをleaf総byte数とせず、
header／外側DTOと署名／opening bytes、decoded DTOとhash／Merkle scratchのheapを別予約する。
この段階ではnative pricingへ自動接続せず、旧4-byte価格は変更しない。

Court/source-onlyのwide operand読取、実行／capture／cell、native row/受付とfee、FullRelation
I64終端、実checkpoint／元最大context／性能の検証は別工程。今回の型を追加したことから
これらの完了や公開activationを導出しない。

## 検証状態

focused fixtureは新規のrange-valid synthetic programで、4-byte pre／中間leafと、i32を
超えるI64最終leafを混在させる。native chainへ登録済みという意味ではない。
manual little-endian bytesと既存hash／Merkle preimageを独立に組み立て、canonical geometry、
旧narrow互換、偽造leaf／ProgramInfo／context／binding、混在幅を検査する。
A0から専用移譲された既存targetで通常Cargo compileを実行し、新step test 5件と既存wide
primitive test 5件の**計10件が成功・失敗0件**。Core libraryと両test targetのstrict Clippyも
`--no-deps -- -D warnings` で成功した。Rootの過去件数や他scopeへ加算しない。

初回は、新testの「別program由来ProgramInfo」が実際には同内容だったため4/5成功・1失敗。
内容が等しいmetadataを受け入れるCore実装は変更せず、windowを実際に改ざんして差異を
確認するtestへ修正した。初回compile／失敗logと最終成功logを分けて保存する。

source commitは `41a00f590`、test前提修正は `b048ce4e1`。
`validation/wide-step-v2/` に対象command、原log、source／2つの実test binary／使用archiveの
SHA-256を保存する。指定済みRocksDB archiveを使用し、C++再build、Rust rlibの手動流用、
新large target作成は行わなかった。archive hash前後一致と実行中source不変を確認した。
A7の共有A1cacheは使用せず、A0の専用targetは検証終了後に解放した。

## Payload名の正確化とFixed／Hist混在回帰

後続source `9a3e952e9` で上限名を `maximum_leaf_payload_bytes_v2` とし、lane payloadだけの
保守的上界であることを明示した。serialized header 26 bytes、decoded DTO、opening、署名、
hash／Merkle scratchをnative callerが別予約する契約を維持する。

第6testはC=3、h_tile=2、W=3、P=2／D=6の新規structural fixture。位置別leaf数を独立に
`[2,6,7,7,6,8,6]` と列挙し、計42葉のcanonical index／count／bindingを照合した。
6つのwide最終leafはversion 2・8-byte、Fixed checkpoint 2葉とHist tile 3葉を含む
全非logits leafは旧version 1・4-byte preimageと一致する。全leafでBorsh長が26＋payload、
4つのI64 laneで32＋26＝58 bytesであることも確認した。演算成立、実LLM実行、native受付の
検証ではなく、data geometry／encoding／identityの回帰である。

この新候補はstep 6件＋既存wide primitive 5件の**計11件成功・失敗0件**。Core libraryと
両targetのstrict Clippyも成功した。前回10件や初回失敗rawはそのまま保存し、合算しない。
今回は専用移譲された既存 `/Users/wata/.cache/misaka-positive-a1-target` を使い、Root own
cache／A0 targetは使わなかった。原log／command／source・2 binary・archive SHAは
`validation/wide-step-v2-payload-state/` に別記し、cacheは終了後に解放した。
