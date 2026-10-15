# RFC02 残作業とモデル追加時のフォーク方針

更新日: 2026-10-15。作業ブランチ: `codex/workspace-rfc02-completion`。同日、残りの14項目のコードは `integ/rfc02-remaining`（`integ/rfc02-rfc06-12-g14` から分岐・定期merge）で実装を続けた。各行の「状態」列がその結果で、すべて**未検証**（build・test・変換・ノード起動は一括検証フェーズで行う）。PASSと書いた項目はない。

**RFC02はコード実装・検証ともに未完了。初期7モデルはいずれも `NOT_RUN`。**
`pre` の `5aa52b886` に対する監査で、過去の「コード実装は完了し、検証だけが残る」という説明は成立しないと確認した。その後の変更は実装途中の保存であり、過去のビルド・テスト結果で現在のソースを検証済みとは扱わない。

ユーザー指定どおり、全コードを完成させてから検証をまとめて行う。保存・pushのためにビルド、テスト、変換、ノード起動、測定を開始しない。通常のpush先は上記ブランチのみとし、`pre`・`main`へのpush、force push、reset、rebaseは行わない。

## 今回までに追加したコード（未検証）

- Owning39のprogram/source、可変履歴のselection、実parameterとinputの分離、元の長さ・資源上限。
- 実sourceを使うcomponentの実行、元のproducer consent/Fund、nonce、実escrow debit、DA・公開局所裁定、独立Finalと一回だけのescrow精算。
- Master39の登録・元request・順序付きcomponentの実Fundとの対応、上流Finalの参照、67–70の分割uploadと32-KiBの永続Masterページ。
- ディスク上のtensor/hash/trace、実再実行による独立検出・局所化、公開データ応答、上流出力のデータ同一性認証とmaterial生成。
- 同一の独立anchorに結び付いたaux streamとpayout collection、元Master/sourceの再導出、receipt39の元発行契約と共有queue検査。
- 参照数によるsource/class本体の保持・解放と、元の同一性に一致する再配置。実consent・Fund・Masterの保持を接続。元の固定headerは残す。
- 新39のFinal条件では、検出率・期待損益を報告指標へ分離し、旧27の数値gateを流用しない。元coins・資源・期限・公開裁定の条件は維持する。

これらの追加は、登録済みモデルの採掘資格、全モデルのsource fidelity、実ノードでのFinal・coinbase・償還の成功を意味しない。

## 残りのコード実装

| 項目 | 完了させる内容 | 状態（2026-10-15、`integ/rfc02-remaining`） |
| --- | --- | --- |
| Native39 transport・終了処理 | action53のGC、54/55の実producer commit transport、paid carrier候補・eligibility・index・scanner接続。GC時のjob保持参照解放と、元Final・生成tokenを後続処理が再利用できる記録を実装する。 | implemented, unverified: action53 `consensus/core/src/palw_native_collect_fold_v39.rs:300`（終端・credit・保持期間・live owner/pendingなしを要求、queue30/physical22を一回だけretire、`HOLD_JOB`解放、16/17/23/28/84は保持、kind 31 tombstoneにconsent digestを記録）、54/55 `:179`/`:227`（Fund済みproducerのみ・commit窓内・2 attempt、1ページ目でMetadata envelopeを前払い）、owner/eligibility `:151`、node側signer照合 `consensus/src/pipeline/virtual_processor/processor/palw.rs`（54/55）、paid carrier候補/index `consensus/core/src/palw_native_carrier_index_v22.rs:28`、route extent `misaka/palw/kernel/src/route.rs`。scanner: spec arm `scripts/drills/rfc06_native_aux_spec.py`（owning39・mining39・master32の全行、502 site・未claim 0）。**blocked（生成armのみ）:** 本branchの `gen_rfc06_native_wire.py` は `PalwTirPublicChunkRecordV1` 等6型のshape-review pin欠如で停止。pinは `e0c1f1263`（`integ/rfc06-12-remaining`）にあり、integへ入った後に再生成する（一時pinでのdry runでは全armが展開できた。生成物はcommitしていない）。 |
| 独立Job/Final監査 | `CheckedNativeJobAuditV39` は型とgetterまで。実aux streamから元book・maximum・consent・資源・Final・一回のcreditを再導出するfactoryを実装する。既知の記録欠損は成功扱いしない。 | implemented, unverified: `PalwChainStateV2::native_job_audit_from_stream_v39` `consensus/core/src/palw_native_job_audit_fold_v39.rs:83`。認証済みfull aux streamとarchiveから元header・book・class・consent（live、またはGC後はkind 31 tombstoneで認証したarchive）・maximum・quota・contract・nonce・debit receipt 25・Commit 28・一回のcredit 23・Master binding 84を再導出。欠損は失敗。 |
| 完全Master Final監査 | aggregate83の照合コードを追加したが、上のJob factory未実装のため接続未完。全stage・実Fund・元claim・精算・unique queue・credit集合を同じ独立anchorで照合する。 | implemented, unverified: `native_master_final_audit_from_stream_v39` `palw_native_job_audit_fold_v39.rs:356`（aggregate83と全componentのJob監査を同一anchorで照合）。CLI `misaka/palw/sdk/src/bin/palw-native-final-audit39.rs`（`--mode job|master|archive`）。 |
| GC後の証拠取得 | 元headerに結び付くsource/class archiveの具体的な読取・取得・保存とreadonly監査を接続する。任意の外部bytesから権限を作らない。モデル登録中はclassを保持し、退役時にだけ解放する。 | implemented, unverified: `NativeArchiveDirV39` `misaka/palw/sdk/src/native_archive_v39.rs:31`、`save_native_archive_from_stream_v39` `:108`（元identityで排他保存、bytesは権限を持たず監査時に元headerで認証）。登録model中のclassは `HOLD_MODEL` で保持し退役時のみ解放（`consensus/core/src/palw_native_mining_fold_v39.rs:204`/`:271`）。 |
| 実request composer | 完全なpublic requestから各stageの最大長、stop/input、token/image/random/upstream materialを同じpure規則で生成する。未知の上流出力や未来のtokenを仮定しない。 | implemented, unverified: `compose_component_request_v39` `misaka/palw/kernel/src/flat_invocation39.rs:489`、`compose_and_check_component_v39` `:716`（Random/Public/Image/Upstream、未観測の上流出力は要求しない）。SDK `misaka/palw/sdk/src/native_request_composer_v39.rs:41`。test `misaka/palw/kernel/tests/flat_compose39.rs`。 |
| Frontendとruntimeの全接続 | 実checkpointから完全source・全component・元context・tokenizer・task preprocessingを固定する。特にMistralの画像処理とtoken展開、PALWQ361の整数演算・routing・再帰stateの元runtimeとの対応を完了する。 | partially implemented, unverified: Mistral画像のraw source/tensorとfrontend→Master request `misaka/palw/sdk/src/full_component_v28/source39/master_request41.rs:24`/`:61`、text adapter `:151`、`prepare_text_request39.py`、PALWQ361差分driver `misaka/palw/sdk/src/native_q36_differential_v40.rs:53`（bin `palw-q36-differential40`、base0 Qwen36Engineと `InterpreterV39` のlogit比較、`EQUAL_OVER_MEASURED`/`MISMATCH` のみでPASSなし）。**blocked:** 実checkpoint・tokenizer・contextのpinと差分の実測は変換・実行を伴うため本フェーズでは行えない。35B以外の6モデルの完全pinも未了（launch README）。 |
| A6/A8 | 新39のreceipt point・累積accumulator・CLI・Python auditor・runnerを接続する。二つの元window、発行rate、全失敗を含む分母を維持する。元Masterとstage契約を事前固定し、未来のcomponent jobIDは真正なFund84を初めて観測した時点で固定して以後変更しない。 | implemented, unverified: `AnchoredReceiptPointV39::derive` `misaka/palw/sdk/src/native_receipt_windows_v39.rs:408`、`ReceiptAccumulatorV39` `:491`（二つの元window、全分母、Master別の名前付き状態、Fund84初見時に固定）、CLI `palw-native-receipt-audit39`、Python `native_receipt_window_driver_v39.py`・`native_receipt_window_audit_v39.py`・`test_native_receipt_windows_v39.py`（py_compileのみ）。測定はNOT_RUN。 |
| 実payout・償還 | 独立Finalの精算と、pending payout、実coinbase delivery、実UTXO spendを区別して接続する。初見Finalでpendingが既に消えていた場合は真正なdelivery witnessを必要とし、欠損から配達・償還を推定しない。 | implemented, unverified: `NativePayoutStatusV39`/`native_payout_status_v39`/`derive_native_redemption_v39` `consensus/core/src/palw_native_redemption_v33.rs:294`/`:319`/`:340`（精算・pending・coinbase delivery・UTXO spendを区別、pending消失時はdelivery witness必須）、SDK `misaka/palw/sdk/src/native_redemption_v39.rs:67`、bin `palw-native-redemption39`。 |
| 採掘用canonical work | `palw_native_mining_v39.rs` の数式・型は未接続。実positions・終了長・全component演算量から計算し、局所court費用をLLM全体の計算量に流用しない。再利用・分割・重複によるcredit増加を防ぐ。 | implemented, unverified: `derive_component_work_v39` `consensus/core/src/palw_native_mining_v39.rs:334`（実positions・終了長・全component演算量）。消費は action75 `palw_native_mining_fold_v39.rs:375`。各Master stageはkind 108で一度だけcredit、署名者が実際に生成したstageのみ。 |
| 普通のmodel/claim登録 | 新73–79 laneのfold、実economic class、stake・seating・possession・share、元の登録・成熟条件を接続する。Source39を偽のTIR1や旧FP claimに詰め替えない。 | implemented (dormant), unverified: 73–79 `consensus/core/src/palw_native_mining_fold_v39.rs:182`（register `:204`、seat/exit/retire `:271`、commit `:375`、issue `:496`、collect `:562`）。economic classは元Master/sourceから導出、seat stakeはADR-0176 liability、shareは政策上限内、成熟は元登録DAAから。Source39をTIR1/旧FP claimへ詰め替えない（PNR9専用carriage）。**ユーザー判断 2026-10-15 第4・5回:** 登録modelは登録時から0.3固定、model単位の審査・試験・probation・更新・昇格なし。tierはintegへmerge済みの `palw_model_tier_v1`（ADR-0179）を `ModelTierV1SourceV39` `palw_native_mining_fold_v39.rs:67` で読む（v39 model idはclassでもmodel lineでもないため常に `Registered`＝tier fence以降0.3。v39登録が1.0になる経路はない。係数は報告用で、適用は `A_m` の一回のみ）。 |
| 抽選検証者の承認 | 元model-epoch、beacon、抽選対象、検査対象、期限、署名、承認数を固定する。正直な割合の前提をpolicyに明記し、実承認からprivate eligibilityを発行する。自己再実行の一致やescrow Finalを承認の代用にしない。 | implemented at claim level only, unverified: RFC-0018 Gate C に `'V'` subject を追加（`consensus/core/src/palw_security_certification_v1.rs:69`/`:282`、fold `palw_security_certification_fold_v1.rs:92`/`:138`、同じpolicy・draw・attestation・verdict・sweep）。正直な割合の前提は `NativeMiningPolicyV39` `palw_native_mining_v39.rs:376`/`:465` に明記しk/mを満たさないpolicyは拒否。receipt発行は `palw_native_independently_work_certified_v39` `consensus/core/src/palw_rfc0018_v1.rs:181` を要求。自己再実行やescrow Finalは代用しない。**superseded:** model単位の抽選承認・private eligibility発行は第5回判断で廃止（RFC-0017 table 131は参照しない）。 |
| 経済上限と一回使用 | 共有bond budgetのQ/B/R/F、exposure、拘束期間、配分clippingと0.51 exportを新consumerへ接続する。元creditと採掘receiptを区別し、receiptの二重使用を防ぐ。 | implemented, unverified: `plan_native_mining_budget_v39` `consensus/core/src/palw_bond_budget_fold_v1.rs:2482`（Q/B/R/F・exposure・hold・配分clipping）、`bond_formula_native_mining_liability_v39` `:2512`（ADR-0176 bond formula v2 `NativeFunded`）、`bond_budget_native_receipt_fits_v39`/`bond_budget_native_receipt_spend_v39` `:2465`/`:2538`（0.51 export cap）、native job liabilityもformula v2へ。receipt quantumの一回使用はbitmap `native_mining_quantum_spent_v39` `consensus/core/src/palw_native_receipt_v39.rs:134`。元creditと採掘receiptは別row・別budget id。 |
| Block・coinbase・Final weight | 正当な登録・元計算量・実抽選承認・経済上限を満たした新receiptだけをnodeのblock gate、coinbase、Final weightへ接続する。現在の私費jobのFinalはこれらの権利を与えない。 | implemented (dormant), unverified: `PNR9` header form `consensus/core/src/pow_layer0.rs:686`、pre-ghostdag `consensus/src/pipeline/header_processor/pre_ghostdag_validation.rs:301`（fence・decode・challenge・ML-DSA-87）、virtual `processor/palw.rs:9777`/`:9796`、fold `apply_native_receipt_block_v39` `palw_native_mining_fold_v39.rs:610`（Gate C・budget・safe_weight）。coinbaseは既存receipt-spend entitlement経路（fold version 39）。私費jobのFinalは権利を与えない。 |
| 有効化 | 明示policy、fork fingerprint、資格審査、SDKの71/72保持解放・期限回収およびsigned 73–79、運用手順を完成させる。ローカルのqualified flagや固定PASSでshipping gateを迂回しない。Rule-E有効環境の正しいV2 anchor exportも接続する。 | implemented except activation (OFF), unverified: 明示policyと**測定値gate**: `validate_palw_native_mining_v39` `palw_native_mining_v39.rs:529`（前提fence native39・Final27・funded execution・OPV・Gate C・ADR-0176 budget・tierがすべて同じか低いDAAで有効、隔離calibration profile以外は `PALW_NATIVE_MINING_MEASURED_RECORDS_V39` に一致するreview済み記録が必要。記録は空）、native39側も同様に `PALW_NATIVE_EXECUTION_MEASURED_RECORDS_V39`（`palw_native_execution_v39.rs`、空）。**t12 DAA 14,000:** 記録がpinされない限り両fenceは休眠のまま出荷する。fork fingerprint（`consensus/core/src/config/params/identity.rs` Some-only hash、`consensus/core/src/fork_id_v1.rs:886` probe）、SDK 71/72 `misaka/palw/sdk/src/native_payload_v29.rs:1579`/`:1592`、期限回収候補 `native_expired_consent_holds_v39` `:1602`（core reader `palw_native_source_fold_v39.rs:460`）、signed 73–79 builderとframe test `misaka/palw/sdk/tests/native_lifecycle_payload_v39.rs`、運用手順 `docs/design/palw/tir/launch/native39-operations.md`、Rule-E V2 anchor export `consensus/src/pipeline/virtual_processor/processor/palw_accept.rs:2538`。資格審査は第5回判断でsuperseded。**blocked（一部）:** header側の `palw_committed_state_root_v1` はV1 leafのみをcommitする（L2FC/ADR-0178 lane）。exporterとauditorはV2で一致したが、rule E有効時のheader rootとはまだ一致しない。rule Eは全presetで休眠・armed拒否のためfail-closed。 |

## コード完成後の一括検証

1. 依存・全対象のコンパイル、必要な回帰・敵対的入力・決定論的courtのテスト。過去に失敗した全体テストも原因を切り分けて再確認する。
2. 同一の実artifact・元contextでのsource再現、実行、停止長、資源量、独立検査、故意の不正に対する公開局所化・裁定。
3. ローカルdevnetで、独立ノードの登録・Fund・全component・Final・実抽選承認・ブロック生成・coinbase・実UTXO償還・reorgを通す。
4. 元の二つのwindowと発行条件によるA6、および元のcensusの `D_all` / `D_complete` を維持したA8。欠測・失敗・未対応を分母から除外しない。
5. 初期7モデルをすべて通す。70Bは元のPanel=1・1日相当DAAの準備・検証・証拠提出も実測する。

専用の大型GPU測定環境は未確保。利用可能なbuild VPSは `5.104.81.23` だけで、指定script・MemoryMax=8G・nice19・`-j3`・作業後削除を守る。稼働中のt12ノードを測定に使わない。不足する元の重み、tokenizer、実行機器、独立検証者は明示的な残件として扱う。

## genesis 8モデル(2026-10-15 ユーザー決定、初期7モデルを置き換え)

初期7モデルは同日にgenesis 8モデルへ置き換えた(下の表は履歴として残す)。対象はhuihui abliterated版(1.5BはDolphin 3.0)で、番号は元の1〜10を維持する(7と9は削除)。全モデルを一つの版付き `PALWART3` `.palwart` 形式で出荷する。状態・アーキテクチャ×層種別の網羅表・未完了項目は [genesis8-status.md](../genesis8-status.md)、変換パイプラインは `scripts/genesis/genesis10_pipeline.py`、genesisリストは `consensus/core/src/palw_genesis8_catalog_v1.rs`(休眠)。

| # | 対象 | 経路 | 状態 |
| --- | --- | --- | --- |
| 1 | dphn/Dolphin3.0-Qwen2.5-1.5B | A16 | 旧形式 `.palwart` 変換済み(コーディネータ記録)、PALWART3 包装は未実行 |
| 2 | Huihui-Qwen3.5-2B-abliterated | TIR | 未記録(コーディネータが実行中) |
| 3 | Huihui-Qwen3.5-9B-abliterated(暫定) | TIR | NOT_RUN |
| 4 | Huihui-gemma-4-12B-it-abliterated | TIR | NOT_RUN |
| 5 | Huihui-gpt-oss-20b-BF16-abliterated | TIR | NOT_RUN |
| 6 | Huihui-Qwen3.8-27B-Coder390-EfficientThink-abliterated(bf16 GGUF) | TIR | NOT_RUN(HTTPS範囲読みの完全ストリーミングは実装済み・未検証) |
| 8 | Huihui-Qwen3.6-35B-A3B-abliterated | PALWQ361 | BLOCKED(safetensors→PALWQ361 変換器なし) |
| 10 | Llama-3.3-70B-Instruct-abliterated | TIR | NOT_RUN(e2e 裁定は不要) |

### 履歴: 初期7モデル(2026-10-15、同日に置き換え)

| 規模 | 対象 | 状態 |
| --- | --- | --- |
| 1.5B | Qwen2.5-1.5B PALW A16 | NOT_RUN |
| 8B | Llama 3.1 8B Instruct | NOT_RUN |
| 14B | Qwen3-14B | NOT_RUN |
| 24B | Mistral Small 3.2 24B | NOT_RUN |
| 32B | Qwen3-32B | NOT_RUN |
| 70B | Llama 3.3 70B Instruct | NOT_RUN |
| 35B-A3B | Qwen3.6-35B-A3B、元のPALWQ361整数runtime | NOT_RUN |

35Bは `palw-runtime/qwen36-35b-a3b.palwq36`、36,492,831,232 bytes、SHA-256 `5fe1d15caf0e544b718a4a2398db4f902b9a6722b266dd53824fbc9723578b29` を対象とする。他の整数variantやClaude系GGUFへ置き換えない。詳細pinは [initial-seven-models-v1.json](launch/initial-seven-models-v1.json)。

7モデルを同時に有効化する目標を維持する。均等な7分の1配分、架空の初期bond、未検証モデルの強制Active化はしない。`alpha = 1.5`、30日平均の適格bond配分と共通Q/B/R/F上限を守る。

## 未対応モデルが出た場合のハードフォーク・ソフトフォーク

**RFC02の現行規則で対応できないモデルが出た場合は、必要なハードフォークまたはソフトフォークを実施して対応する。** 未対応を恒久的な除外理由にせず、モデル・task・contextと不足機能を具体的に記録して対応する。

- 既存のprimitive・型・整数規則・資源上限で表現できる場合は、まずfrontend・lowering・runtime側の対応を行う。
- **ハードフォーク:** 新しいprimitive、型・shape・state規則、wire grammar、合意が許容する範囲の拡張など、従来ノードと有効性判断が一致しない変更が必要な場合に行う。新しいversion・明示したactivationを設け、ノード更新と移行を行う。
- **ソフトフォーク:** 従来有効だった集合を制限する追加条件など、旧ノードが新規則のブロックを有効と認識できる変更で対応できる場合に行う。追加条件を実際に強制するnode側実装と有効化手順を設ける。
- どちらにするかは、実際の旧ノードとの互換性とブロック有効性の差で決める。変更をソフトフォークと呼ぶだけで互換性が成立するとは扱わない。凍結済みの旧bytes・class ID・primitiveの意味を後から書き換えない。
- 新規則には、元モデルのsource・全context、独立再実行、公開局所化・決定論的裁定、資源・期限・DA、抽選承認、経済上限、Final・償還、reorgの検証を付ける。対応フォーク前の未対応モデルに、便宜的な報酬資格を与えない。
- フォークで対応する場合も、[PRINCIPLES §8](../../../PRINCIPLES.md#8-llm計算の暗号学的証明を使用しない--2026-10-15ユーザー決定)の恒久決定を守る。LLM計算の暗号学的正当性証明は採用しない。ハッシュ・署名・Merkle・VRFは同一性・認証・乱数の用途に限る。

対応仕様と証拠は [RFC02 completion contract](../../../rfc/0002-palw-tensor-ir.md#ii14-completion-contract--2026-10-10) と [実装記録](rfc0002-implementation.md) に反映する。
