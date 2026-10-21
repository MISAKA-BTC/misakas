# semantic potential packet inventory 候補の実装前不足

本メモは未実装。新public quoteやslot上界helperを追加しない。現native grammarを変えずに全family positiveを主張できるpatchはまだ作成していない。

## 既存planから固定part列を作れない具体的反例

1. `palw_constraint_stream_carriage_v1.rs:706..725` はEOG長1..PART_IDS、rendering長1..PART_BYTES、Tokenizer bytes<=65536 / proofs<=64 / encoded<=90000を許可。固定フルchunk以外を拒否しない。`ceil(raw/PART)`固定offset列では合法な短part/異なる区切りを落とす。
2. `palw_tokenizer_table_stream_v1.rs:298..340` はactual parser.pending_keyを見てbyteを止めkey proofを要求。空bytes+proof-only packetが合法。合法shapeはoffsetだけでなくpending key/event数/parser継続状態とproof消費数に依存する。bytes/proofs上限の直積を全合法packet数と呼べない。
3. `selected_event_resource_plan_v1` はD*tile全座標ではなくMAX_REQUESTS*(custodians+2)で最大moveを課金する。全potential coordinateと同時予約SUMは別。frameの旧価格planと一致すると偽ってはならない。
4. Prefix familyはnative cursor/unit/bodyで次sourceを選ぶ。TIR prefix leaf境界の固定列とlegacy head/page/rowの分岐を混ぜると不適用native actionを含む。native actor slotにはowner/liable signer等の別grammarがあり、source7familyのmaxcustodian数だけから同一と推定できない。

## 安全に追加する必要のある狭いAPI（設計候補）

- 各既存runtimeの内部にprivate `potential_next_shapes(native_state)` を置く。戻り値は `action_kind, coordinate_domain, authenticated_material_dependency, shape_guard`。次actionが存在しない終端/無効/未導出を区別する。stateはnative original/既存validator生成物のみ、caller DTO不可。
- shape guardは次を既存validatorの分岐と共有する: offset==actual cursor、非空/最大長、pending keyでbytes停止、compact proof数とnative key-index、current family、prefix canonical next unit、current signer/owner/relay scope、short-body未使用tail。
- original-level閉包walkは、material値がなくても区別が必要なparser/cursor/event状態の有限抽象を持ち、その抽象状態からnext shapeを過小評価しないことを既存runtimeと照合する。`maximum_moves`はwalk停止予算であり、全node数そのものではない。
- actual native costと公開material認証が未導出なら `Missing::TokenizerShapeClosure` / `Missing::ConstraintRenderingClosure` / `Missing::PrefixCursorClosure` を返す。partial成功・complete bool・native価格の仮値を返さない。

## なぜ現在patchを作らないか

state抽象/shape guardがないままnative `maximum_moves`個のslotを作る実装は『上界を列挙と偽るhelper』になる。逆に全byte内容を具体列挙すれば有限でも現予算では常に拒否するだけの実装となり、実familyでの意味あるpositive fixtureを作れない。固定partだけに狭めるのは既存wire/合法遷移の変更になる。この3方式は依頼条件に反するため採用しない。

## apply/testに進むための最小具体段階

1. `/tmp`候補で既存validatorのshape条件を参照可能にするprivate adapterを各family runtimeに足す（新wire/public quoteなし）。既存runtime側を同じ条件式へ内部整理する場合も意味を変えず実branch検査を共有。
2. 原本prepareでactual登録semantics/tokenizer cert/history pledgeからrequired familiesだけを選択し、family別native状態/shape closureへ渡す。family選択はframe.planの任意値ではなくcheck_tir_public_allocation_native一致済みplanに限る。
3. 正fixtureはactual native originalprepareを通したPublicPrefix/Constraint/Tokenizerをそれぞれ非空で作る。単なるwith_service_semantic_guard_fixture等のstruct注入をpositiveに使わない。
4. 各正fixtureについて、既存runtimeが受理する短part/fullpart/proof-only/byte+proof/EOF/prefix cursorのnext actionがshape setに入ること、対応する認証material/source dependencyが同じscopeであることを確認する。未対応native actionはMissingとしてoriginal-level結果を失敗させる。
5. bounded closure walkとnative resource upperを独立に比較する。vertex/edge/path/retained-state不足時は全Err。shortbodytailはruntime guardで非義務、actor違い・family違い・context替え・重複slotを拒否する。
6. native admission testからactual prepare→shape walkを実際に呼ぶ。旧TFP1/タグ/価格/時計/fenceは保持し、funded install/fullG14成功とは呼ばない。

未作成物: production candidate source / 適用patch。作成しなかった理由は上記のcanonical shape closure不足であり、測定や仮値で埋めていない。
