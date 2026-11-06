# Wide Q24 native登録接続 — read-only限定調査

対象固定source: `cc22b0da13784eb03a023cbd1781969577883d9e`。レビューのみ。Root/index変更・Cargo実行なし。2026-11-01。

採択済み [RFC-0043](/Users/wata/.codex/worktrees/positive-court-a0/MISAKA-testnet/docs/rfc/0043-wide-q24-llm-output.md) / [ADR-0211](/Users/wata/.codex/worktrees/positive-court-a0/MISAKA-testnet/docs/adr/0211-wide-q24-llm-output.md) は新profileの実装を許可する。公開有効化は許可しない。現Rootのpure admissionやCPU capture成功はnative登録受理、全claim consumer、実HF最大文脈長の完了を意味しない。

## 最小変更plan

1. 固定 baseline と完了語彙を pin: RFC43/ADR211の許可は explicit i64 Q24 profileと既存 consumer adaptersの私設実装。構造/metadata、private native registration、既存claim対応、RFC44 native Result、公開activationを別状態として記録。

2. 共通の deterministic profile dispatch を作る: 厳格program scheme/dtype識別と trusted court_version/activation の両条件。narrowは旧v10/旧bytes/旧関数のまま。wideだけ既存 verify_tir_class_admission_v2 (full range + M=max(coarse O,exact8B S), stop_above=None)を選ぶ。selectorを Params→preflight→native validator→rehearsal→fold まで同じruntime scopeで伝える。既存 hash/fingerprint にcourt2が含まれることと mirror一致を検査。

3. 既存タグ61の登録を native 経路へ接続: SDK既存message、Active bond署名、target/share、1/block IR cap、通常bond burn/exposure、carrier transaction通常script/UTXO検証、single writerのjournal/rollbackを保持。foldは元class/artifactからprofiled schema1 rowを導出し、既存write_tir_class/common class registrationを通す。新wire/domain/row semantics/tierは作らない。

4. wide rowを読む全利用側を audit: 上の表を最低集合とし、raw facts.tiled/logits_tiles/raw facts job/old StepSpace へwideが到達しないことを保証。既存width adapterを接続できないconsumerは明示拒否する。単なるrow insertをモデル利用/登録作業全体の完了としない。

5. private typed Paramsのみで実native検証: adopted private-wide-q24/court2の既存識別を使用し、公開preset/flag-day/version/IDを変更しない。環境変数はprivate configを明示構築する入口までで、各Nodeのobject裁定が環境によって分かれてはならない。Params.validateとmirrorが通ること、court1/unknown/newflagsNoneではwide拒否を試験。

6. 実際の署名・原資・受理evidence: 既存T12 harnessから実Active bondとMLDSA署名のacceptance/fold検証を流用できるが、それだけではcarrier UTXO inclusionを検証しない。通常lifecycle txを実script認証/UTXOへ載せ、block acceptance→folded classes/tir_classes/root+origin+journal→restart/reorg→registration statusを確認する別e2eを必須にする。fake pubkey/sig vecやaccepted=true/stored flagは完了証拠にしない。

7. 同一Root source/binary/Paramsで学習済み複数HF artifactを再現: source revision/weights/tokenizer/元最大文脈長/profile/IDをpin。clip/scale/中心化/短縮をしない。既存ceilingに失敗した場合は失敗原因を記録して実装修正範囲を評価し、上限免除で成功にしない。native登録と全最大位置実行/Panel・Node費用は別測定。newAssignment/r1Resultへの結線は採択まで行わない。

## 残consumer表（native登録に必要な最低集合。全repositoryの網羅証明ではない）

| consumer | exact path / API | 現状・必要な接続 | 許可境界 |
|---|---|---|---|
| SDK登録候補・署名object | `misaka/palw/sdk/src/tir_registration.rs:119,135`<br>`tir_registration_preflight_v1 / build_tir_registration_v1` | 現在 v1 constructor/preflight 固定。entry の explicit wide canonical context だけ作れても登録は拒否。strict profile + trusted Params court2 の両方で既存 v2 builder/preflight を選ぶ。署名message/tag61は維持。 | 43通常実装 |
| native acceptance | `consensus/src/pipeline/virtual_processor/processor/palw_validate/class_registration.rs:399,454`<br>`VirtualStateProcessor::palw_validate_class_registration_object_v1 → ClassRegisteredTirV1 → verify_class_admission_v10` | 既存 chain target→Active bond 実MLDSA署名→admissibility→share規則の順を維持し、正確wide/court2のみ verify_tir_class_admission_v2。public court1/unknownは拒否。 | 43通常実装 |
| Params / fold selector | `consensus/core/src/palw_tir_v1.rs:231,254,275; palw_state_v2/state_params.rs:374`<br>`sync_palw_tir_v1 / validate_palw_tir_v1 / with_tir_from_daa` | validator court1のみ。mirrorは高さだけで version が欠ける。adopted court2を private typed ruleset から同じ identity/activation で runtime mirror/extras まで渡す。object処理中の環境変数や peer bool を許可に使わない。 | 43通常実装、公開arm不可 |
| rehearsal / actual fold | `consensus/src/pipeline/virtual_processor/processor/palw_accept.rs:1633,1664; consensus/core/src/palw_state_v2/objects.rs:1915,1936`<br>`palw_v2_apply_one_object_retained_v1 → tag61 → palw_tir_class_record_v1` | 本foldと事前rehearsalの両方がlegacy factory固定。court2を同じnative runtime scopeで知り、profile-aware schema1 derivationへ分岐。full range/admissionを無制限にfoldで再実行せず既存一件上限を維持。row/class work/dissected/window/witness副作用を一致させる。 | 43通常実装 |
| 永続row / identity / restore | `consensus/core/src/palw_tir_class_record_v2.rs:68,113,168; palw_state_v2/transition_builder.rs:2083; palw_state_v2.rs:25418`<br>`ProfiledTirClassRecordV2::{build,resolve_record,into_storage_row}; write_tir_class` | schema1/rooted bytes/Arc program/graph+layout+artifact+tokenizer identityを維持。restore check_program はhash/長さだけなのでconsumerは明示resolver/full-row一致で読む。layout preimageは別既存公開経路から認証する。 | 43通常実装 |
| layout不要のmetadata view | `consensus/core/src/palw_tir_row_profile_v2.rs:74,190`<br>`ResolvedTirRowProfileV2::resolve / row_max_context / logits_tiles` | 既存keyとnative class artifactを入力してprogram/schema/profileを再導出できる。layout geometry/canonical job/native admissionは与えない。row_max_contextのrangeだけを元layout最大長一致と誤認しない。 | 43通常実装、metadataのみ |
| 既存 job 導出 / REAL work plan | `consensus/core/src/palw_tir_attempt_v1.rs:102,171,183; palw_exec_v2_verify.rs:186; palw_exec_v2_fold.rs:54,64,888`<br>`palw_tir_job_context_v1 / palw_tir_attempt_context_v1 / palw_exec_v2_real_job_v1` | raw facts.tiled=false はwideを旧flat traceへ誤導出する。原job identityを変えず explicit resolver を使うか wide拒否。canonicalpair/seed/prompt/tokenizer/CU/held規則維持。既存primary prefill-draw契約を新assignmentへ読み替えない。 | 43既存claim adapterは可能、新Assignmentは未許可 |
| Positive metadata | `consensus/core/src/palw_positive_metadata_v2.rs:58,101,121`<br>`PositiveNativeTirMetadataV2::resolve / derive_attempt` | full-layout + schema1 record照合を用いる明示wide metadata接続は既存。actual current-prefix owner/Header/bondを借りる読取DTOであり r1 Result/worker/assignment をnative受理した証拠ではない。 | 既存読取基盤、native Result別 |
| DA / tiled分類 | `consensus/core/src/palw_state_v2/da_sessions.rs:226; palw_staged_court_state_v1.rs:740; palw_tir_public_fold_v1.rs:476,644`<br>`record.logits_tiles / facts.tiled` | wide rowの旧logits_tilesは1。vocab4097なら本当は2 tileであり見落としとなる。authenticated row-profileの明示wide tile数/8B materialまで接続するか早期拒否。旧 bool をfamily bitに変更しない。 | 43通常実装、非対応拒否必須 |
| decode / sampling / output readers | `consensus/core/src/palw_decode_view_v1.rs:54; palw_logits_q24_v2.rs; palw_tir_objective_v1.rs`<br>`PalwDecodeViewV1::from_tir_header_v1 / V2 logits primitives` | V1 known-wide deny guardあり。V2 integer selection/data rootsは存在するがnative decode view/各output attribution/sampling readerへ接続未完。旧Int32 fallback解除だけは不可。 | 43通常実装 |
| Fraud court / original binding / identity | `consensus/core/src/palw_court_v2.rs:2084,2102; palw_offence_attribution_v1.rs:1258,1289; palw_tir_fp_lane_v1.rs:56`<br>`tir_binding_filled_v1 / adjudicate_filled_v1 / verify_tir_binding_v1 / PalwTirStepSpaceV1` | V1 court/identity/FP geometryがwideを拒否。explicit V2 full source relation checkerがlocalにはあっても既存native court/carriage責任接続は別。all court caps/8B上界・claimへのroot joinを保持し、全検査前のMiner責任判定をしない。 | 43のwidth adapter可能、新Result Court authorityは未許可 |
| registration副作用 witness/work | `consensus/core/src/palw_state_v2/objects.rs:1941,1968; palw_mesh_fold_v1.rs:126; palw_mesh_v1.rs:155`<br>`palw_tir_model_work_v2 / record_witness_profile_v1` | 登録foldのwork/window/dissected/mesh副作用も対象。既存mesh8BはMatMul witness単位でfinal-logits8B対応の代用ではない。署名や旧manifestの意味を変更せず、非対応のmesh executionを拒否。 | 43通常実装の影響監査 |
| 新r1 Result / role / funding / P / credit | `docs/design/palw/integration/positive-native-assignment-rfc-draft-2026-10-31.md:3,13; consensus/core/src/palw_positive_court_v2/gate.rs:109`<br>`new AssignmentTerms/AcceptAssignment/body0 result binding; positive_native_credit_v2` | 未採択draft。新Job正本、poster/worker/prover同意、actual UTXO hold、nonce/一消費/取消/reorg、P、worker対価は登録実装へ混ぜない。既存fail-closed 7credit consumersを維持し、署名/role/fundingをDTO/hash/self consistencyで代替しない。 | 採択待ち、実装範囲外 |

## 現在の明確な停止点

native validatorはv10固定、fold/rehearsalはlegacy record factory固定でwideを拒否する。Params court2を認めるvalidatorとfold selectorのmirrorが無い。raw facts.tiled=falseをflatと解釈するjob/DA読取箇所が残る。これらを未対応のままrow保存だけ通して登録完了とは呼ばない。

full schema1 rowはlayout digestだけを保存する。layout不要row-profile viewはprogram/profile/幅の読取用であり、元layoutの最大context一致・geometry・canonical jobを認証しない。jobやcourtは既存認証済みlayout preimageとのfull joinが必要。

## 実受理fixtureの境界

`consensus/src/pipeline/virtual_processor/tests/t12_tir_registration_cap.rs` は本物のMLDSA署名・harness Active bond・native acceptance walkとfoldを使い、無署名拒否と1/blockを検査する。ただしobjectsをhelperへ直接渡すため実carrier UTXO inclusion試験ではない。`consensus/core/tests/palw_tir_class_registration.rs` はwire/stateless fixtureに擬似signature bytesを使うため、署名/原資実受理の証拠に使わない。`palw_model_registration_v1::palw_registration_row_written_by_v1` と origin aux141 / actual accepted transactionを照合し、carrierがminedだっただけとfold済み登録を区別する。

新NativeAssignment draftは未採択。poster-funded paid経路でprimary L1置換ではなく、登録モデルの0.3/年次1.0/モデル単位審査不採用を変更しない。現gateの7 consumer (reward, work receipt, DAG, Rule E, settlement, prefix reuse, dependent claim) はMissingNativeResultBindingを含む拒否を保持する。
