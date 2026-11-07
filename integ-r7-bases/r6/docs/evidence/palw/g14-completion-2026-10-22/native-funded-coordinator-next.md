# 次の実連結単位（設計メモ、未実装）

対象source: integration候補、core checkpoint 0e3a23bd8。RFC-0035 Aの実独立escrowを目指すが、以下の試験成功を全G14受付成功と呼ばない。新価格・preset・fenceは採択しない。

## 実際に到達させる入口

1. `consensus/src/pipeline/virtual_processor/processor/palw_accept.rs::palw_v2_objects_of_block` のTFO専用拒否分岐を、**拒否を維持したまま** dedicated candidate検査へ接続する。旧FP extractorへinner TFP1を流さない。ここではUTXO受付後でentriesを失っているため、この関数だけへfunding検査を足しても不十分。
2. `utxo_validation.rs::calculate_utxo_state` の `ValidatedTransaction` loopで、同じ実tx・actual entries・calculated_feeからTFO candidateのexacttransaction証拠を保持する。`validate_transaction_in_utxo_context` の通常入力・署名・maturityチェックを必ず先に通す。callerが作ったentriesを受理した扱いにしない。
3. FP側は `palw_state_v2/objects.rs::apply_object_with_pair_v1` のnative resource checkpointと同じ検査環境へ到達させる。現在は `prepare_paired_original_v1` がTFP1用extras mapを要求するため、TFOをそのmapへ流用しない。inner-envelope/objectの既存conversion・scope検査を専用経路で呼び、private `prepare_original_v1` / `original_plan_v1` とactual builderのparams/extras/ctxを使う。
4. 登録class/layout、original header、actual current producer bond/key、同ctxのnative allocation rulesを解決。frame/sourceからsource7family graph、numerical geometry、original parameter coverを実際に導出する。通常courtとheld等を適用した `palw_court_params_at(daa)`、class step uplift、custodian再導出をallocationと同じ値にする。`extras.native_court_rules_v1`のraw courtを無条件に同一視しない。
5. 下記Missingを検査。native fee/capital等が全て解決できたときだけplanner内部で `OriginalFundingScopeV1` / `FundingCarrierObligationsV1` / `FundingCheckpointV1` を構成し、`preflight_funding_transaction_v1`へ同tx/entriesを渡す。現段階は必要値が不足するのでfunded installerへ到達しない。caller supplied complete bool、任意graph、consent中のprofileIDを規則の正本にしない。

## imports / visibility の最小差分

- processor: `PalwTirFundedOriginalEnvelopeV1::{decode_v1,original_v1}` とfundingpreflight、actual accepted tx entriesの橋渡し。TFO magic判定は既存predicateを使用。
- core adapterを `palw_state_v2` の新private子moduleに置く場合、兄弟 `palw_tir_public_fold_v1::{original_plan_v1,prepare_original_v1}` は現在private/pub(super)。前者だけ `pub(super)`へ狭く公開するか同fold内の専用検査関数を使う。`TransitionBuilder`をcore外へ公開しない。
- `palw_state_v2` のpublic exportはnative検査entry一点のみ。報告はMissing/Invalidとして返し、funding/install/credit権限を返すopaque helperを先に増やさない。
- graph imports: `NativeSourceMaterialGraphV3`、`NativeNumericalGeometryV3`、`Source::parameter_cover_v1`。native constructor経由のみ。現在のcompositionはcourt/demandのpolicy認証を持たないためそのままinstallerへ渡さない。
- current source IDはheader hashのみ。court/fee/demandは含まない。allocation `rules_id`はcourt等を含むがdemandを含まない。demandは実class登録DAAとnative fence履歴から解決する。

## Missing分類（値を0/仮値にしない）

- `NativeProfile`: network/stateが選んだservice・fee・capital・retention・settlement各profile本体/ID。自己整合するhashだけでは価格の採択・選択を認証しない。
- `PolicyProvenance`: original受付point、class登録DAA由来demand、resolved court、native rule/profileの一致。
- `CanonicalBranches`: non-dissection cone、output/stop/selection、parameter/prompt取得、Constraint/Tokenizer/PrefixState、F02 duty/復旧、終了・保持・金融効果のcanonical全分岐。source+numericalのみではcompleteではない。
- `PacketCostsAndWindows`: 全packetのCPU/IO/byte/保持費・実fee・実署名義務、release/deadline、依存契約horizon、runtime ORとallocation precedence。
- `RequiredCapital`: 全義務の同時拘束・旧予約資本との重複なしの必要額、実資金との割当、実collection/refund経路。旧depositを現金escrowとして流用しない。
- `NativeEscrowCommit`: rooted escrow outpoint/金額/残高/受取先権限/唯一のsettlement消費と、selected-parent＋同mergeset prospective lock、UTXO/state双方のapply/undo/replay。

Missingは検査順で早期に一つだけ返してもよいが、未検査を成功と表示しない。Invalid（不正署名・scope違い等）とUnavailable（未採択profile等）を分ける。

## 最小native試験（次実装の到達条件）

- 実MLDSA input署名とreal UTXO entriesを使うTFO carrierを `validate_transaction_in_utxo_context(Full)` に通す。そのactual accepted txが専用candidate検査のsource/geometry経路へ到達し、未採択価格/未完成grammarを理由に**funded原本としては拒否**することを記録する。ordinary carrier UTXO/fee受理とfunded FP拒否を区別する。
- 元FP/claim/reservation/frame/escrow各rootが不変、TFOがlegacy FPとして生えない。funding fallback outputはordinary UTXOとして残り、後続のordered mergeset blockで本物のpayer署名により支出できる。未資金拒否がordinary outputを凍結しない。
- actual carrier署名不正・missing inputならnative UTXO拒否でcandidateへ到達しない。内側consent不正ならordinary UTXOとFP拒否を区別する。
- exacttx payload/input entries/point/parentroot変更、別source/profile、現在producer鍵違い、registered demand/court違い、subset graph、探索予算不足を拒否。checkpoint後の失敗注入でFP予約等がrollbackされる。
- feeFにbond mandatory burn＋旧rent＋新carriageを二重充当しない。funding output額をfeeへ加算しない。actual accepted-setのみをfee/plan mapへ対応させる。
- Positiveは「通常署名UTXO受付」「native source/geometry検査」「条件付きfundingpreflightの結合」までに限定。**現missingを残したままfunded-original install成功を試験用trueで作らない。** 本当のpositive escrow install/同mergeset保護/settlementは採択profileと完全grammar＋native UTXO commit境界が揃った次単位で実施する。

## escrow commit/undo上の不可避な境界

FP checkpointはcore stateだけを戻し、既に受理したordinary transactionを取消さない。従ってescrowの新規lockはnative complete admissionと原子的に成立する必要がある。無効TFO payloadからprospective lockだけを残すとfallbackを不当に凍結する。逆にobject fold後だけlockすると同mergeset中の先行支出を見逃す。native accepted transaction順でplan/validate/commitを結合し、失敗時にはordinary fallback処理を維持する設計が必要。現在のBondSpendFilter/selected-parent lockは再利用の足場だが、escrow rowを追加せず既存bondと同じだと仮定できない。将来の署名不要relay settlementは、既知locked outpointと検証済みnative効果だけに限定したUTXO apply/remove/pay/undoが必要で、一般payer P2PKH署名をgeneric bypassしてはならない。
