# G14: 客観的裁定・slash・ブロック生成／報酬拒否までの残作業

更新日: 2026-10-15（夜に Claude が引継ぎ・追記、branch `integ/g14-remaining`）。元の対象workspace: `codex/integration-followup`。
部品実装の基準保存点は WIP `0909ad5add2c2d696d9a131db297f67f350d85a0`。
その後の新claim frame・native接続も実装途中のWIPであり、保存済みでも完成分には数えない。
本書と追加ソースの保存点は、このファイルのgit履歴で確認する。
本書は新経路の本番有効化や`pre`へのpushを許可するものではない。
正式なnative設定の下で経路が実行可能になることと、shipping presetのfenceを有効化する判断は区別する。

## 停止時点の引継ぎ

2026-10-15、ユーザーの「ここまでで作業を完了して残作業をmdに残してpush」の指示により、ここで実装作業を停止する。
**完了したのは今回の保存・引継ぎであり、G14全体ではない。** 全担当の編集を凍結し、未検証のWIPとして作業ブランチへ保存する。
`pre`への反映・本番fence有効化・未統合branchの取込みは行わない。

### 今回追加したソース（未検証）

- `49794e9a7`では、元claim frame、署名付きcontrol／grant、atomic Valid＋S4＋raw grant＋Ready、producer exposure／保持／復元索引、native Rules／network domainの接続候補を保存した。
- その後の保存対象は、元FPを包む`TFP1` bounded envelope、isolation／header-contextの元FP view、実carrierとの対応付け、元FPの通常予約後に同じatomic objectでframeを保存する経路、旧bundleの二重要求回避である。
- 新しいoriginal／owner Openのrentをnative planから照合し、外側transactionの実feeで不足・未価格sentinelを拒否するソースを追加した。rent宣言をUTXOのcollectible feeから控除し、self-miningでその宣言額がそのまま戻らないよう接続する候補も追加した。
- canonical foldには、受理に成功した元FPのcompanionだけを渡す。別の同claim transaction、拒否されたcarrier、後続の重複carrierのpledgeを貸し借りしないための処理である。
- 上記はコンパイルも実行もしていない。構文／書式と差分検査だけであり、実料金の徴収・native受理・経済結果・期限内完走の証拠ではない。

## 2026-10-15 夜の引継ぎ（Claude、`integ/g14-remaining`）

`origin/integ/rfc02-rfc06-12-g14`（9fad35323、のち 10cbaeda6 も merge）へ `codex/integration-followup`（65c799ee3）を merge し、
以下をソースで追加した。**build・test・node 実行はしていない。PASS と書いた項目は無い。** 各 checkbox の状態は下の注記に書く。

**DAA 14,000 での扱い（推奨）: `palw_tir_public_conviction_v1` は有効化対象から外し、休眠のまま載せる。** 理由（いずれも未完）:
Valid seat の準備 worker（prepared descriptor の作成）と GrantReady 投稿の scheduler、node filer の scheduler／queue／restart journal、
V3/V4 typed receipt、owner 数の容量規則と custodian 負荷上限（R1 最終項・R7）、最大 profile の期限・資源実測（R7/R8）、wire pin、V1〜V7。
（producer 側の TFP1 作成は今回ソースで接続した: quote API `palw_tir_public_original_quote_v1`、`PalwTirPublicCustodyPledgeV1::for_plan_v1`、
validator-core `build_fp_commitment_tx_with_tir_public_original_v1`、kaspad の canonical producer。GrantReady の署名 builder は
`kaspad/src/palw_tir_public_filer.rs` の `palw_tir_public_grant_ready_object_v1`。いずれも未検証。）
休眠中は A-2 で旧ルールと同一（下の「休眠時の同一性」）。

### 今回の merge 解消
- `consensus/core/src/lib.rs`: module 宣言を両方残した。
- `palw_lifecycle_objects_v2.rs`: fence enum の index 14 が衝突（RFC08 `MaterialReleaseRecoveryV2 = 14` と G14 `TirPublicConvictionV1 = 14`）。
  index は wire ではなく局所 activation table の slot なので、`TirPublicConvictionV1 = 15`、`ALL: [Self; 16]` に両方を並べた。A-2 行は fence を
  Params field 名で引くので変更不要（170–171 の行はそのまま）。
- `processor.rs` / `palw_accept.rs`: `PalwCarriedObjectV1.tir_public_original` を integ 側の field と併置。受理 filter は integ の
  `Result<_, NativeCoverageV1>` / `NativeChainStateV1` 契約を保ち、Codex の 4 番目の戻り値（受理された TFP1 original）を
  `palw_v2_accepted_objects_and_originals` 経由で返す（Result 化）。
- merge 後の API 差: integ では `TransitionBuilder::checkpoint()`/`new()` が test 専用。`objects.rs` と `palw_tir_public_fold_v1.rs` の本番経路を
  `try_checkpoint()?`/`try_new(..)?` に直した。
- 割当て確認: object tag 170/171 は integ・rfc02-remaining・rfc06-12-remaining（243 を使用）・decisions-1015 のどこでも未使用。FP envelope magic
  `TFP1` も未使用。新しい kernel inner / aux table / RPC / proto / p2p / funded action / v39 action の割当ては無い。
- 生成物: integ の native resource inventory / wire scanner は Tir* staged row・staged_tir_* index・extras の新 field 等で古くなっていた。
  `gen_rfc06_native_inventory.py` と `gen_rfc06_native_wire.py` を再実行し rustfmt（`wire_kernel_aux.rs`・`ledger_rows_generated.rs`・
  `kernel_ledger.rs`・`wire_budget.rs`・`wire_jobs.rs` は他 lane の手書き arm があるため再生成結果を採らない）。TIR 経路の手書き codec 15 件を宣言と
  照合し `rfc06_native_manual_wire_review_v1.py` に `reviewed-field-layout` で pin。新しい owner-local visitor（`palw_mode_v2`、
  `palw_tir_public_{claim,custody,owned}_v1`）を `#[path] mod` で接続。
- A-2 wire pin: tag 170/171 は `PALW_A2_NEW_KIND_WIRE_V1` に **0x0 の仮 pin** を置いた。検証段で `[a2u-pins]` 出力から pin し直す。

### 休眠時の同一性（A-2）
- 新しい行（TirClaim / TirPreparation / TirAcquisition）は fence 後にしか書かれない。Final floor（`palw_claim_final_floor_v1`）、
  `palw_accusation_pending_v1` の追加項、Gate C の待機、RPC の `revoked`/`withdrawn` は行が無ければ従来と同一。
- `fee_of` は TIR の carrier（TFP1 / TIR control）だけ実 fee を読む。それ以外の carrier の読みは従来どおり（Codex 版は fence 後に全 carrier の
  読みを変えていたのを狭めた）。finality fee からの TIR rent 控除は fence 前 0。
- `palw_tir_public_conviction_v1_fence()` は staged TIR route・staged public court・objective offence に加え、`palw_rcore_plus` と
  `palw_capacity_aggregate_liability` も要求する（全 preset で `None`）。

### 再開時に先に確認する具体的な穴

1. 追加一式の型・全struct literal・enum／wire manifest pin・通常fee consumerをソース段階で揃える。G14の残りのコード実装を終えるまでは、ユーザー指定どおりbuild／testを始めない。
   — **2026-10-15 夜: ソース上で整合（未コンパイル）。** merge 後の API 差（try_checkpoint/try_new）を修正、生成 inventory/wire scanner を再生成、
   wire pin は 0x0 の仮 pin（検証段で再 pin）。
2. `TFP1`のrent宣言、実UTXO fee、burn、producer／owner deposit、同一transactionのnative受理を通しで監査する。現時点では実徴収済みとは扱わない。
   — **監査済み（ソース）**: 宣言 = native plan rent（`palw_tir_public_fold_v1.rs:85`）、実 fee ≥ 宣言（`palw_accept.rs:1712`、
   `palw_tir_public_minimum_rent_v1` `:2190`）、宣言額を collectible fee から控除＝burn（`utxo_validation.rs:356`、`:383`）、
   producer deposit は Work gate room 検査（`palw_tir_public_fold_v1.rs:236`）と exposure index、FP 受理と frame 設置は 1 object で原子的
   （`objects.rs:9`）。conviction filing も同じ方式で terminal rent を宣言・burn する。実徴収の実行確認は未。
3. `utxo_validation.rs`の`finality_fee += validated_tx.calculated_fee`は控除前額の分類である。新rent控除後のcollectible totalとの整合、EVM depositを併せ持つcarrier、他transactionの通常feeを巻き込む配分がないことを、template／Round／coinbaseと同じ規則で解決する。総feeから控除しただけで全配分の整合まで証明したことにはしない。
   — **修正（ソース）**: finality 分類から TIR 宣言 rent を控除（`utxo_validation.rs:628`、fence 前 0）。控除は tx 単位の `saturating_sub` で
   他 tx の fee を巻き込まない。template・Round preflight・coinbase は同じ `palw_collectible_transaction_fee_v1` を通る。bond burn と finality
   分類の既存の不一致（TIR 以外）は範囲外として残す。
4. 新envelopeは通常FP payloadのstrict decoderであり、Evalの追加tail等の別codecをまだ扱わない。全nested job-tail Vecの割当前上限も未監査である。外側48 KiB上限と一部preflightだけで全nested allocationが安全とはしない。
   — **一部**: eval/gen 形式は TFP1 の対象外のまま（`paired_claim` が `eval: None` を要求）。さらに frame は job tail と非 greedy decode 設定を
   持つ original を拒否する（`prepare_original_v1`）。nested job-tail の割当前上限の全数監査は未。producer は TFP1 を quote → pledge 署名 →
   envelope の順で作る（`palw_tir_public_original_quote_v1` は admission と同じ `original_plan_v1` を使うので quote と受理が食い違わない）。
5. context再導出helperのD=0拒否と、既存native routeが許す元FP形式・job種別・decode countの範囲を照合する。新carrierが合法な元形式を黙って落とす場合、対象を狭めて完成扱いせず対応を足す。
   — **一部（ソースのみ・未検証、2026-10-16）**: frame は sampling（seeded Decision 11）・V4 pipeline（penalty/bias/stop）・semantic stage
   （constraint / tokenizer / prefix state）を持つ original も受理する（下の R4 注記）。まだ拒否するのは job tail（eval/gen）と flat logits scheme の
   class（selection pin が行全体を運ぶため bounded control に入らない、R7 の分割搬送待ち）で、それらは旧 ClaimBundle 経路のまま。
6. V3/V4のtyped grant、共通filerからnative terminal、小さいrooted裁定参照／大witness搬送、実slash、生成資格と受信block拒否、全報酬consumerを完成させる。
   — **共通 filer → native terminal → 実 slash → 生成／報酬の拒否はソースで接続**（R4〜R6 の注記）。V3/V4 typed grant と大 witness 分割搬送は未。
7. native worker／queue／inclusion、実CPU予約・反復上限、最大profile、GC、IBD／pruned import、再編で孤立した物理workのjournalを閉じる。
   — **未**（node worker・queue・journal・実測は今回の範囲外。frame の index は rooted row から再構築できる: `chain_state.rs` の
   `assert_tir_acquisition_indices_v1`、`palw_staged_court_fold_v1::rebuild_indices`）。
8. R1〜R8の実装を全対象で揃えた後にV1〜V7をまとめて実行する。追加した未実行テストソースや別branchの結果を、今回の完成証拠に置き換えない。
   — **未実行**。fold 水準の E2E ソースを追加（`palw_tir_public_fold_v1.rs:861` `conviction_e2e_tests`）。V1〜V7 は検証 agent の担当。

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
| 新しい元claim frame、保持・producer exposure・復元索引、元FPとのatomic受理の候補実装 | `0909ad5ad`後のWIP。transaction pairingと実料金検査をソースで接続した段階で、支払い済みstandingの実動作は未検証 |
| bounded署名付きcontrol／grant carrier、tag 170／171、TFP1、独立fence・native foldの候補実装 | nativeネットワーク設定・通常入口・料金・burn・enum全体の整合と回帰は未検証。稼働する受理経路の完成ではない |
| 追加した回帰テストのソース | 未実行。syntax／差分検査を、コンパイル・試験・実ネットワークの証拠に数えない |

過去の別commit・別branchの試験結果は、この保存点と作業中差分の通し検証ではない。
登録モデルからの告発試験も、起動しただけ／実行中／局所不一致の発見だけでは完成に数えない。

### 今回のソース接続で採用した境界

- 元FP受理時のmandatory preparerはproducerのみ。実Panelは通常のnative PanelBound時に一度固定し、drawn seatであるだけではcustody義務・slash権限を発生させない。
- drawn seatは既存の元予算で準備作業を先行できるが、資格は**同じnative objectでのexact Valid receipt、実S4 lock、distinct raw grant、準備済みReadyのatomic受理**で初めて追加する。途中失敗でcreditだけを残してはならない。
- grantやowner Openで元の準備期限・horizonを延ばさない。ownerの取得・work・元frame再検査は別途料金・depositを必要とし、元producer予算をownerごとに複製しない。
- 現在のcarrier候補はwhole-job V2 receipt用。V3/V4のsegment／shard署名をV2署名へ読み替えず、新frameから未対応入口を拒否する。これは迂回防止であり、V3/V4対応完了ではない。
- FullSUMにはframeの成長、old/new rowとjournal、再検査、署名検査回数・state参照数の上限を載せる。ただし、実native処理の反復回数制限・crypto予約・MLDSAのCPU料金／最悪時間は未接続・未実測である。回数を載せただけではCPU期限保証やG14 qualificationにならない。

## 3. 実装の残作業

以下は達成条件であり、単にAPIやテスト名を追加しただけでは完了しない。

### R1. 元claimのnative受理・資金・保持を一本化する

- [ ] generic arithmetic-only TIRも含め、元のpublic FP commitment、登録class/layout、binding、source、FullSUM、絶対期限を同じ受理文脈で固定する。
  — **実装（ソースのみ・未検証）**: `palw_tir_public_fold_v1.rs:71`（`prepare_paired_original_v1`）、`:124`（`prepare_original_v1`）、`:217`
  （`install_original_v1`）、`objects.rs:3496`/`:3582`。X3 の hard deadline が frame の collect door より前なら拒否。job tail と flat logits scheme の
  original は拒否（範囲の限定）。semantic stage は ClaimBundle 契約を frame と同じ d0 に設置して受理（R4 の注記）。
- [ ] resource identityと、受理DAAを含むallocation identityを区別し、元署名・元job・元input/output/state・artifactの差し替えを拒否する。
  — **実装（Codex、ソースのみ・未検証）**: `resource_plan_id`（pledge が署名）と `allocation_id`（受理 DAA を含む）を分離
  （`palw_tir_public_allocation_v1.rs:140`/`:142`）、差し替え拒否は `PalwTirPublicClaimV1::check_structure_v1` と
  `PalwTirPublicCustodyPledgeV1::authenticate_original_object_v1`、TFP1 の `original_fp_v1`/`matches_object_v1`。
- [ ] 実際の料金とdeposit、producerのexposure、必要なbond exit hold、source保持を確保してから、frameと準備状態をatomicにrootへ反映する。
  — **実装（ソースのみ・未検証）**: rent（上の穴 2）、deposit ≤ Work gate room（`palw_tir_public_fold_v1.rs:236`）、producer exposure
  （`staged_tir_claim_exposure_v1` → `palw_accuser_exposure_v1`）、liable signer の exit hold と source 保持（`palw_staged_court_fold_v1.rs:3012`、
  retirement floor）、FP 受理と frame/準備行の原子性（`objects.rs:9`、`try_checkpoint`）。
- [ ] 構造的に正しいquote、署名、deserialize済みrow、旧ClaimBundleの支払いを、新FullSUMの支払い済みstandingに読み替えない。
  — **実装（Codex、ソースのみ・未検証）**: frame は native 受理でのみ設置（`install_original_v1`）、旧 bundle 経路と排他（`objects.rs:3496` 以下）、
  旧 licence 経路は exact credit を要求（`require_qualified_receipts_v1` `:786`）。
- [ ] 個々のownerの追加acquisition／workと、元claimのprepaid poolを分離する。複数ownerが同じ支払いを無制限に複製できず、攻撃者の先取りで正直な1人を排除しない料金・容量規則を完成する。
  — **一部**: owner は自分の rent + deposit を別に払い（`open_v1` `:320`、`owner_terms_v1`）、元 pool を使わない。owner ごとの row 上限
  （`maximum_owned_rows_per_reserver`）はある。
  **実装（ソースのみ・未検証、Lead 既定 (a)・ユーザー確認待ち）**: claim あたりの outside owner 数を `PALW_TIR_PUBLIC_MAX_OWNERS_PER_CLAIM_V1`
  （K_owner = 8、`palw_tir_public_claim_v1.rs:277`）に制限し、超過 owner は duty・row を作る前に Open で拒否（`open_v1`
  `palw_tir_public_fold_v1.rs:379`、数は `staged_tir_owner_count_v1` `palw_staged_court_fold_v1.rs:208`）。custodian の応答負荷上限は plan の
  acquisition 容量から導出（`custodian_load_bound_v1` = K_owner × `maximum_turns` 件、× `maximum_unit_bytes` B、frame view に公開）。
  **残る穴（ユーザー判断待ち）**: colluder が K 個の owner 枠を自費で先取りすると、正直な owner が Open できない（枠の再利用＝idle owner の
  枠回収規則が未決）。この穴があるため「攻撃者の先取りで正直な1人を排除しない」は未達で、checkbox は閉じない。

### R2. 署名付き搬送と全native入口を接続する

- [ ] 元FP envelope、Open、Preparation、Acquisition、RawValidGrantを、独立したprospective fenceの下で実装する。既存claimの料金・期限は変更しない。
  — **実装（ソースのみ・未検証）**: 全て `palw_tir_public_conviction_v1`（`params.rs:4970`）の下。common conviction filing（tag 170 の
  `Conviction`/`Convict`）も同じ fence。旧 claim に frame は無く、料金・期限は不変。
- [ ] claim、source、元allocation、owner固有row、reserved DAA、turn、actorを署名へ結び付ける。元allocation IDと`descriptor.allocation_id_v1(owner_contract)`を混同しない。
  — **実装（Codex、ソースのみ・未検証）**: `PalwTirPublicControlV1::signing_message_v1`（claim/allocation/source/target(owner, reserved_daa)/turn/
  rent/signer/action）、owner row key は `owner_key`（`descriptor.allocation_id_v1(owner_contract)`）で別。
- [ ] bounded codec、正しいbond鍵、domain、transaction pairing、重複・再送・期限・reorgの検査を実装する。
  — **実装（ソースのみ・未検証）**: bounded decode（carriage・TFP1・conviction evidence の ID 数 preflight）、literal bond 鍵と native network
  domain（`validate_native_signature_v1`）、tx 単位の TFP1 pairing（`palw_accept.rs:2285`/`:2405`）、turn と現行 row 比較で再送拒否、
  conviction は 1 回のみ（consumed offence）、reorg は delta journal（E2E ソースで root 復元を検査）。
- [ ] isolation／header検査、mempool、blockからの抽出、native fold、fee検査、import／recoveryを同じ規則で接続する。下位fenceでも旧履歴の判定を変えない。
  — **実装（ソースのみ・未検証）**: isolation `tx_validation_in_isolation.rs:42`、header `tx_validation_in_header_context.rs:60`（mempool は
  isolation を通る）、抽出 `palw_accept.rs:2285`、fold `objects.rs`、fee `palw_accept.rs:1712`/`utxo_validation.rs:356`、import は rooted row
  からの index 再構築。fence 前は A-2 で旧判定（上の「休眠時の同一性」）。
- [ ] senderの自己申告上限ではなく、nativeに固定されたbyte／count上限で、割当て前のdecodeを制限する。
  — **実装（ソースのみ・未検証）**: control 90,000 B、TFP1 の各 field 上限、custody の count を pledge の native upper で制限、conviction の
  ID 数 20,000（`PALW_TIR_PUBLIC_CONVICTION_MAX_IDS_V1`）、proof は prepaid terminal witness 上限で adjudication 前に拒否。
- [ ] 新kindのbounded fixture、wire manifest pin、通常carrier分類、署名検査、fee／massの全consumerを揃える。tagを確保しただけでwire統合完了にしない。
  — **一部**: fixture（`zero_filled_kind` 170/171）、carrier 分類（`palw_heartbeat_carriers_v1`）、署名検査（`palw_validate.rs:3772`/`:3780`）、
  fee consumer（受理 walk・UTXO burn・finality 分類・node の carrier rent `palw_carrier_fees.rs:14`）。**A-2 wire pin は 0x0 の仮 pin**
  （検証段で再 pin）。

### R3. Validの資格と担保を、元署名に結び付ける

- [ ] 旧ClaimBundleに依存せず、実際にnativeで認められる**同一の署名済みValid receipt**と、新しいraw-source custody grantを照合する。
  — **実装（Codex、ソースのみ・未検証）**: `apply_grant_ready_v1`（`palw_tir_public_fold_v1.rs:713`）、
  `qualify_original_native_v1`（`palw_tir_public_custody_v1.rs:592`）。
- [ ] Panel所属、Sampled、旧history grant、未credit receiptだけでは、新しいraw custody義務・slash権限を発生させない。
  — **実装（Codex＋今回、ソースのみ・未検証）**: liable = producer + exact grant のみ（`liable_signers_v1`）。conviction も DA default も
  liable 以外を課金しない（E2E ソースで drawn だが Valid 無しの seat が無傷であることを検査）。
- [ ] grantを認める前に、実際に徴収できる担保と元horizonまでのholdを確保する。既存担保金額の規則を勝手に変えない。
  — **実装（Codex、ソースのみ・未検証）**: 既存 credit funnel が作る S4 lock（金額は再計算しない）を `bond_exit_after_daa` まで延長し、frame の
  exit hold を張る（`palw_tir_public_fold_v1.rs` `apply_grant_ready_inner_v1`）。
- [ ] 元claim受理時はproducerの署名しかない点を扱う。後続grantの義務追加を、元期限・元planを変えずに配線する。
  — **実装（Codex、ソースのみ・未検証）**: `PalwTirPublicOriginalCustodyV1::new_v1` は producer のみ、grant は後から追加、terms/plan は不変
  （`preserves_credited_v1`）。
- [ ] late grantに、完了不可能なcold preparation期限を課さない。採用した先行準備＋atomic grant／Readyを、実nodeの準備worker・carrier・元予算・実担保へ通し、固定期限内に成立させる。候補helperの存在だけでは完了しない。
  — **一部**: consensus 側の atomic grant/Ready は Codex 実装済み。node 側は 3 署名（V2 Valid・raw grant・outer）を元 domain で作る builder を追加
  （`kaspad/src/palw_tir_public_filer.rs` `palw_tir_public_grant_ready_object_v1`、frame view に pledge id）。
  **node 準備 worker を追加（ソースのみ・未検証）**: `kaspad/src/palw_tir_public_custodian.rs`（`palw_fraud_filer` の子 module、panel loop から
  毎 tick `tir_public_custodian_tick_v1` を呼ぶ、`palw_panel.rs` の history grant の直後）。producer（d0 の custodian、accepted index
  `staged_tir_preparation_duties_after_v1` から duty を読む）と、Valid に署名した drawn seat（`own_receipts`）が、producer が前払いした claim 全体の
  準備 lane で slice ごとに「backend の quote → work book が accepted state で exact debit を作る → 署名 `WorkDebit` を court queue
  （round `u32::MAX − 24`）→ accept 後に `confirm_v1` が permit を mint → その permit で slice 実行」を繰り返し、prepared capture を seal する。
  seal 後に producer は自分の `PreparationReady`、seat は Valid credit＋raw grant＋Ready の atomic object を投稿（round `u32::MAX − 25`）。
  seat は seal するまで grant しないので、grant 後に cold preparation 期限を負わない（late grant）。sealed descriptor は
  `state_dir/palw-tir-public-custodian.v1` に journal（tmp→rename）し、再起動後は再準備なしに Ready を再投稿できる。accepted state を読む
  consensus API `palw_tir_public_read_v1`（fence 未満は `Ok(false)`、`api/mod.rs`・`consensus/mod.rs`・`processor/palw.rs`・`session.rs`）を追加。
  **未**: 1 slice ごとに 1 carrier の往復が要り、固定 preparation 期限内に最大 profile で完走することは未実測。seat が元 capture を保持して
  いない場合は grant しない（責任を負わない側に倒れる）。owner の demand に prepared capture から応答する responder（Begin/Part/Finish）は未接続。
- [ ] whole-job V2に加え、実際に有効なV3/V4等のtyped receipt／grantを、元のsegment／shard署名domainのまま接続する。未対応入口の拒否を「全receipt対応」と数えない。
  — **blocked**: V3 の supplementary credit（`credit_supplementary_receipts_v3`）は `ReceiptLicensed` を要求する一方、frame の grant は licence
  前に要る。licence 前の typed credit door は新しい金銭規則で判断が要る。V4 shard receipt は RFC-0006 の shard record 経由。現状は frame の claim に
  typed licence を名指しで拒否（`require_qualified_typed_receipts_v1` `:799`）。
  **Lead 既定 (b)（2026-10-15、ユーザー確認待ち）: この release では V3/V4 typed receipt を frame で名指し拒否のまま残し、licence 前の credit door は
  導入しない。** よって本 checkbox は未完のまま（frame の対象は whole-job V2 Valid のみ）。
- [ ] owner Open時のliteral責任集合を確定し、後からのgrantで過去の継続処理を壊したり、過去の責任を増やしたりしない。再編時は現在のnative根拠と照合する。
  — **実装（Codex、ソースのみ・未検証）**: owner contract の `custodians` は Open 時の liable 集合、`bind_preparation_v1` で Ready 済み応答者を固定、
  変更は現行 row 比較つきで適用。
- [ ] 既存grantの署名、exact receipt、credit DAAを保持し、単にsigner集合が増えたことだけで不正なcontract置換を認めない。
  — **実装（Codex、ソースのみ・未検証）**: `preserves_credited_v1`（custody `:251`/`:500`）。

### R4. 1人の検証者による検出・局所化を全対象へ通す

- [ ] 正しいPanelの1人とPanel外の1人が、同じ検証関係・同じ客観的裁定へ到達する共通filerを使えるようにする。
  — **実装（ソースのみ・未検証）**: tag 170 に `Conviction`/`Convict` を追加（`palw_tir_public_carriage_v1.rs:65`/`:73`）。filer は liable 以外の
  Active・floor 以上の任意 bond（`conviction_filer_v1` `palw_tir_public_fold_v1.rs:526`）で、Panel 席でも外部でも同じ object・同じ裁定
  （E2E ソース `:1096` で両者の結果一致を検査）。node 側は reader API `palw_tir_public_frame_v1` と署名済み object の builder
  （`kaspad/src/palw_tir_public_filer.rs`）。**filer scheduler を追加（ソースのみ・未検証）**: `kaspad/src/palw_tir_public_filer_scheduler.rs`
  （panel loop の fraud filer の直後で毎 tick）。finding は `queue_finding_v1` で受け、d0 の door 内で即 filing（door は延びず、claim は door
  閉鎖まで Final にならない）。署名前に fold と同じ経路の dry run（consensus-core `palw_tir_public_conviction_preflight_v1`、API
  `palw_tir_public_conviction_preflight_v1`、次 DAA で評価し builder は破棄）で受理を確認し、holds・責任者・既に有罪・exit hold 喪失など
  accepted state で分かる拒否には rent を払わない。queue は round `u32::MAX − 26`（claim ごと 1 件、due = `collect_until − 1`、
  `COURT_MOVE_REPLAN_DAA` 後に再送、最大 3 回）。restart journal は `state_dir/palw-tir-public-filer.v1`（最大 16 件、tmp→rename、読めない
  journal は退避）。**未**: finding の生産者（outside owner の acquisition 駆動 Open→Demand→Collect と、arithmetic scheduler の局所化）が
  panel loop から未接続。DA の Open は owner 規則上 drawn seat 不可のまま（外部 bond が DA 終局に到達できる）。
- [ ] own-modelで得た不一致と、元claimの認証済みmaterialを結び付け、producerの秘密capture・秘密checkpoint・秘密状態の注入を排除する。
  — **実装（ソースのみ・未検証）**: 証拠は元 root への opening と ID の EOF 認証のみで裁定し、program は state の登録値を native に埋める
  （carried program は拒否）。`adjudicate_original_v1`/`adjudicate_original_output_v1`（`palw_tir_objective_v1.rs:251`/`:103`）。
- [ ] 計算、job、input、output、stateの各違反を既存の決定論的courtへ接続する。局所findingを、そのままportableな有罪certificateと扱わない。
  — **一部→接続（ソースのみ・未検証、2026-10-16）**: 計算（cone/logits）、output identity、DA に加え、**selection と stop を original job 自身の規則で**
  裁定する: `PalwTirSelectionRuleV1::of_job_v1`（`palw_tir_court_v1.rs`、`decoder_v1` と同じ選択: V3 は Decision 11 key、V4 は penalty/bias/ban
  付き pipeline）と新 door `check_tir_decode_token_tiled_rule_v1`（認証は live greedy door と同一、比較は認証済み prefix 上の job の key。banned
  lane の commit はそれ自体が fault。live の greedy door は不変）、`adjudicate_original_decode_v1`（`palw_tir_objective_v1.rs`）。stop 規則は Output
  door で完全認証済み ID に `original_stop_rule_fault_v1`（V4 は `decode_answer_stop_v4`、V3 は budget 長）を適用し、output が一致しても stop
  違反なら有罪（execution-proving）。**semantic stage**: frame は自身の allocation が既に価格付け・rebase した semantic component から
  ClaimBundle 契約（staged semantic court が依拠するもの）を同じ d0 に設置する（`frame_semantic_bundle_v1`、prepare で検査し install で書く）。
  producer の history pledge は TFP1 の新しい optional field `history_pledge` で運び（carrier walk が HFP1 と同じく元 object に対して認証し
  claim で重複排除）、admission が署名と全 field（claim/producer/execution/job/cu_ruleset/`semantic_resource_plan_id`/history 容量/
  acquisition custodian 上限/受理期限/material horizon）を再計算値と照合する。rent/deposit は frame の FullSUM が既に semantic を含むので二重に
  払わない。semantic court が有罪にした claim の Valid custodian は ClaimBundle 経路と同じく `CourtFraud` void に基づく kind 3 で課金され、
  frame の exit hold が collateral を残す。producer 側（kaspad の TFP1 経路）は semantic plan のとき history pledge を frame pledge と同じ受理期限・
  horizon で署名して載せる。**未**: job tail と flat scheme（上記）。semantic court 側の G14 完全性（Panel 外 1 人で終局できるか）は ClaimBundle
  経路の既存実装に依拠し、frame 経由での E2E は未。
- [ ] claim、class、artifact、position、selector、sample／top-k等の意味、依存cone、元inputの完全なbindingを確認する。
  — **一部（ソースのみ・未検証）**: claim/class/artifact/execution/trace/job は target で固定、依存 cone は cone 裁定が要求、selection は job 自身の
  規則（seed・temperature・V4 penalty/bias/ban は original job から取り、filer は選べない）を認証済み prefix 上で、stop は完全認証済み ID 上で裁定。
  V4 の選択は lane ごとの key の argmax で top-k/top-p は無いので、1 本の beating lane で反証できる。constraint mask は staged Constraint court。
- [ ] 部分receiptや単一leafだけで、本来必要な依存閉包の認証・再計算を済ませたことにしない。
  — **実装（ソースのみ・未検証）**: cone 裁定は operand の opening を全て要求し、output 裁定は全 ID の EOF 認証が無ければ拒否
  （`authenticated_original_ids_v1` `:541`）。
- [ ] `Need`、一時的取得失敗、予算不足、EOF、`NoFault`、`PredicateHolds`を、計算有罪またはclaim全体の正しさへ読み替えない。
  — **実装（ソースのみ・未検証）**: `PredicateHolds`/`OutputHolds` とあらゆる error は filing を拒否するだけで state を変えない（有罪でも無罪
  証明でもない）。E2E ソース（`:1162`）で honest twin の root 不変を検査。

### R5. DA非応答を実際の徴収まで到達させる

- [ ] 元pledge／qualified grantから、回答義務のあるliteral signerと期限を確定する。
  — **実装（Codex、ソースのみ・未検証）**: `liable_signers_v1` と準備・応答の期限（preparation_until / respond_until / collect_until）。
- [ ] PreparationDefaultと、その後のReady済みactorに対するDA defaultを分離する。known non-Readyへ不可能なDA義務を繰り返し発行しない。
  — **実装（Codex、ソースのみ・未検証）**: 効果を分離（`Effect::PreparationDefault`/`Effect::Default`）、応答義務は Ready 済みのみ
  （`response_custodians_v1`）。producer の徴収 key は両者共通の DA offence id で 1 回のみ。
- [ ] kernelのdefault effectを、nativeの既存経済規則・実bond lock・残高変更・会計へ接続する。効果オブジェクトを返すだけでは徴収完了ではない。
  — **実装（Codex、ソースのみ・未検証）**: `staged_selected_default_charge_v1`（`transition_builder.rs:2727`）→ void/S4/DA record/報奨。
- [ ] 担保がなく既存helperが徴収をskipした場合を、徴収成功と数えない。必要なlockの不在を受理時に防ぎ、終局時は実際の徴収額を確認する。
  — **実装（ソースのみ・未検証）**: 受理時は grant が S4 lock を要求（Codex）、終局時は全 liable bond が元の exit hold を保持していることを
  要求し（`palw_tir_public_fold_v1.rs:497`、conviction `:692`）、hold があれば conviction 開始時に collateral が locked と判定されて
  `conviction_collected_v1` が実際の debit を記録する。
- [ ] 同じ義務の徴収は1回のみとし、再送・複数告発者・再編・再開で二重徴収しない。無関係なPanel席や外部verifierを処罰しない。
  — **実装（ソースのみ・未検証）**: consumed offence（producer: DA / CourtConviction、seat: false-Valid key）で全経路共通に 1 回、row の
  `collected`/`defaulted` で同じ default の再 collect を拒否、reorg は journal。liable 以外は課金しない（E2E ソースで検査）。
- [ ] sourceが出ない場合でも、他のPanelの署名や多数決なしで、1人のpublic bonded verifierが客観的DA終局へ到達できるようにする。
  — **実装（Codex＋E2E ソース、未検証）**: 外部 bond 1 つが Open/Demand/Collect または PreparationCollect で終局（E2E ソース `:1220` で
  PreparationDefault を外部 1 bond が徴収）。

### R6. 裁定結果をclaim・ブロック生成・報酬へ反映する

- [ ] 計算違反の確定から、native claim無効化と責任のあるproducer／Valid bondの実slashまでを接続する。
  — **実装（ソースのみ・未検証）**: `apply_conviction_v1`（`palw_tir_public_fold_v1.rs:589`）→ credited Valid 全員に S4
  （`charge_covering_signers_v1` `transition_builder.rs:2841`）→ producer は `convict_by_court_verdict_v1`（`:3294`、void / Final 反転、
  CourtConviction、filer への報奨）。
- [ ] 不正claimや失効したbondを使った生成資格・claim-backed work・block opportunityが、テンプレート生成だけでなく、受信blockの検証・合意受理でも拒否されることを保証する。
  — **実装（既存規則の再利用、ソースのみ・未検証）**: void された claim は Final にならず、receipt spend は受信検証（`palw.rs:9724`、Final を要求）と
  template 側の spendable 一覧（`palw.rs:3433`、Final・未没収 root のみ）の両方で拒否。execution-proving の違反は root で没収。有罪の producer は
  ADR-0160 の intent-class funnel で forfeiture＋freeze になり、admission（`palw_admission_v2.rs:652` `ProducerFrozen`）と producer readiness
  （frozen）で拒否。このため fence は `palw_capacity_aggregate_liability` を要求する（`params.rs:4970`）。RFC-0016 の bond-weighted opportunity は
  未統合（担当外）。
- [ ] work weight、Final weight、coinbase、model配分の対象額、escrow、FinalReward等、当該不正を根拠に支払う全経路を照合し、既存方針どおり拒否／失効／保留を反映する。
  — **実装（ソースのみ・未検証）**: frame の claim は collect door が閉じるまで Final にならない（`palw_claim_final_floor_v1` `palw_state_v2.rs:3606`）
  ので、spend weight・Final weight・escrow・model 配分（`gate_final_reward_v1`）・Gate C の coinbase 保留は全て裁定可能期間の後。有罪なら void で
  escrow 没収・未消化 receipt 権利の没収（claim 単位、execution-proving は root 単位）。Gate C は frame の door が開いている間は認証を待ち
  （`palw_security_certification_fold_v1.rs:334`）、void は唯一の述語で credit を外す（RPC は `revoked`/`withdrawn`）。kernel route の
  FinalReward は kernel claim のみで frame とは別経路。
- [ ] 報酬・担保が経済的Finalへ逃げる前に裁定できるhold／windowを維持する。支払済み・使用済み資産を、後から存在しないclawbackで回収できる前提にしない。
  — **実装（ソースのみ・未検証）**: Final floor（上記）、liable bond の exit hold は `bond_exit_after_daa` まで、retirement floor は material horizon。
  clawback を前提にする箇所は無い。
- [ ] EXEC root／slice／riderでは、元claimの違反を依存するsuffix・root・支払いへ伝播させる。無関係な仕事まで無条件に無効化しない。
  — **実装（既存 funnel の再利用、ソースのみ・未検証）**: `void_claim` → `palw_exec_v2_fold::on_claim_voided_v2`（依存 suffix の void と報酬取消）、
  execution-proving は root 単位、claim-proving（output root 不一致）は claim 単位の没収。
- [ ] 保留中の有効な告発、期限、棄却、返金、再編と再評価を同じnative状態で整合させる。正しいclaimは正常終了・支払いできるようにする。
  — **一部**: owner の open demand は `palw_accusation_pending_v1` に入る（`chain_state.rs:1459`）。棄却は state を変えない。正しい claim は door
  の後に通常どおり Final（遅延のみ、課金なし）。reorg は journal。owner deposit の返却・正しい claim の支払いの実行確認は未。

**「slashできる」だけでは、この節は完了しない。** 生成資格／block検証と各報酬consumerの実際の拒否結果が必要。
既に受理したblockやFinal後の処理は、既存のfork-choice／liability方針へ正確に従う。文書だけで過去blockの扱いを決めたことにはしない。

### R7. 全family・最大サイズで資源と期限を閉じる

- [ ] preparationとowner workの別予算、RAM、artifact inventoryのresident tree、disk、sealed＋partial索引、IO、hash、parser、copy、演算、署名検査、carrier、court、terminal、再開をFullSUMへ反映する。
  — **未**（今回の範囲外。Codex の FullSUM 部品はある。実測・配線は未）。
- [ ] 合計金額を払っただけでなく、各actorの実work上限へ予算を配線する。Panelが外部verifierのprotected laneを消費できないようにする。
  — **未**（今回の範囲外。Codex の FullSUM 部品はある。実測・配線は未）。
- [ ] 固定された元preparation期限と、warm response／WorkDebit／part搬送の局所期限を両方満たす。colluderの合法な最大遅延を積み重ねても全体horizonを超えないようにする。
  — **未**（今回の範囲外。Codex の FullSUM 部品はある。実測・配線は未）。
- [ ] 実nodeのキュー、backpressure、inclusion容量、bonded verifierの参加枠を配線し、他owner・他claim・悪性送信者の負荷で完走が壊れない条件を定める。
  — **一部（ソースのみ・未検証）**: owner 容量は K_owner と custodian 負荷上限（R1、Lead 既定・ユーザー確認待ち、先取りの穴あり）。
  staged witness は (claim, filer) ごと 1 row、rent 前払いで door 閉鎖時に sweep。node 側 lane はすべて有界: custodian 準備は同時 4 frame・
  各 lane claim ごと 1 件（`u32::MAX − 24/−25`）、filer は最大 16 件・claim ごと 1 件・part は逐次（`u32::MAX − 26`）、court queue の EDF と
  carrier の in-flight 上限に従う。**未**: block あたりの TIR control の inclusion 上限と、Panel が外部 verifier の lane を消費しない保証の実測。
- [ ] 全体mapのclone／state root／GCの増幅、複数active claimの同時RAM・disk、native sweepの処理量を扱う。1rowや64件pageの上限だけで全体有界とはしない。
  — **未**（今回の範囲外。Codex の FullSUM 部品はある。実測・配線は未）。
- [ ] 新frame、元public commitment、後続grant、delta／journalの実際のfootprintを13資源次元と照合する。新しいrowを追加した後も、以前のquoteで全量を払えたと仮定しない。
  — **未**（今回の範囲外。Codex の FullSUM 部品はある。実測・配線は未）。
- [ ] admission／rehearsal／canonical foldで繰り返す署名検査・frame再検査を実際に予算化する。宣言した検査回数の上限をnative処理で守り、CPU料金／最悪処理時間が不明な部分を0扱いせず、期限条件の不足として残す。
  — **未**（今回の範囲外。Codex の FullSUM 部品はある。実測・配線は未）。
- [ ] 大きなwitnessを既存の小さいterminal上限へ黙って切り詰めない。明示的に資金化した分割搬送、またはnative incremental courtから小さいrooted certificateを作る経路を完成する。
  — **実装（ソースのみ・未検証、2026-10-16）: 明示的に資金化した分割搬送**。tag 170 に `ConvictionWitness` target と `WitnessPart`
  action を追加（`palw_tir_public_carriage_v1.rs`、part は 16 KiB 以下で decode 前に上限検査、`PalwTirPublicWitnessPartV1`）。最初の part
  （offset 0）が frame の conviction rent を宣言・burn し（terminal plan は witness 上限を 16 KiB part で価格付け済み）、以後の part は 0。part は
  (claim, filer) の rooted row `TirWitness`（`PalwTirPublicWitnessRowV1`、door 閉鎖で sweep）に順番どおり追記され、宣言 bytes を満たした
  part が digest を照合し `decode_staged_v1` で復号して `apply_conviction_v1` で直接 filing と同じ裁定を原子的に行う（holds/拒否なら part ごと
  巻き戻り、row は rent 消費済みのまま door 閉鎖まで残る）。全体は frame の `terminal.maximum_witness_bytes` 以下。切り詰めは無い。node の
  filer scheduler は control に入らない証拠を自動で part 分割し、accepted row の offset から再開する（`palw_tir_public_witness_part_object_v1`）。
  E2E ソース `g14_staged_witness_assembles_in_order_and_convicts_once`。**未**: incremental court（rooted 参照で小さい certificate にする方式）は
  採らず分割搬送で閉じた。flat scheme の selection pin（行全体）は witness 上限内なら同じ搬送で運べるが、flat の frame 受理は未解禁（R4 注記）。
- [ ] `Incomplete`、overflow、サイズ拒否を「そのfamilyは完成」に数えず、元の対象範囲を満たす経路を実装する。
  — **一部（範囲を明記）**: `Incomplete`、job tail、flat logits scheme は frame 受理を拒否する（sampling / V4 / semantic は受理に拡張済み、
  ソースのみ・未検証）。flat scheme は分割搬送（本節の上の項）待ち。

### R8. 再開・pruning・reorgと通常node運用を閉じる

- [ ] prepared descriptorと実際のnative Ready、元material TTL、保持ticket、通常worker、native権限付きGCを結ぶ。
  — **一部（ソースのみ・未検証）**: 準備 worker が descriptor を作り Ready まで投稿し、descriptor を journal（material horizon 到達で journal から
  除去）。prepared file の GC（`retire_tir_prepared_capture_v1`、native irreversible DAA が horizon を超えた後のみ）と保持 ticket の接続は未。
- [ ] Tiled reopenの全D IDs再認証が必要な場合、その時間・IO・carrier・RAMを最大条件で処理する。N再走査を避けただけで完走としない。
  — **未**（node 運用の範囲。今回は未着手）。
- [ ] process再開・複数processのcache共有・破損・partial generation・期限前後のcleanupを扱い、正しい公開materialを早期削除しない。
  — **未**（node 運用の範囲。今回は未着手）。
- [ ] canonical work counterの巻き戻しと、既に行った物理的な孤立fork作業を区別する。durable journalと有限recovery規則、または明示されたreorg前提なしに、restartで予算が復活しないと保証しない。
  — **未**（node 運用の範囲。今回は未着手）。
- [ ] fresh node、IBD、pruned importでも、元claim・資格・raw bytes・証拠・期限・責任を現在のnative状態から復元できるようにする。
  — **一部**: frame・準備・acquisition の row は rooted state にあり、index は row から再構築（`rebuild_indices`、`assert_tir_acquisition_indices_v1`）。raw bytes・証拠の node 側復元は未。

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

2026-10-15 夜の時点で上の 5 項目はいずれも未達（ソースの接続が進んだだけで、証拠は無い）。

優先順は **R1〜R3のnative受理・資格 → R4〜R6の裁定と経済consumer → R7〜R8の全範囲閉包 → V1〜V7の一括検証**。
R7の予算・期限条件はR1の元契約に必要なので、後付けで元horizonを伸ばすのではなく、受理実装と同時に固定する。

## 7. コードと関連記録

- [これまでの実装記録と未検証境界](g14-integration-followup-2026-10-15.md)
- [activation readiness matrix](activation-readiness-matrix.md): 過去の結果・他laneの状況を含む。新経路の完了証拠として無条件に転用しない。
- Source／work／Ready: `consensus/core/src/palw_tir_public_{source,owned,state,allocation}_v1.rs`
- 元claim frame／署名搬送／元FP envelope／native fold候補: `consensus/core/src/palw_tir_public_{claim,carriage,fp_envelope,fold}_v1.rs`（WIP）
- 共通 conviction filing と fold 水準 E2E ソース: `consensus/core/src/palw_tir_public_fold_v1.rs`（`apply_conviction_v1`、`conviction_e2e_tests`）
- node 側 filing builder: `kaspad/src/palw_tir_public_filer.rs`、reader API `palw_tir_public_frame_v1`
- Custody: `consensus/core/src/palw_tir_public_custody_v1.rs`
- 局所裁定: `consensus/core/src/palw_tir_objective_v1.rs`
- Native状態・terminal: `consensus/core/src/palw_state_v2.rs`、`palw_staged_court_fold_v1.rs`、`palw_terminal_carriage_fold_v1.rs`
- Node接続: `kaspad/src/palw_tir_acquisition_driver.rs`、`misaka/palw/tir/exec/src/node/{public_source,prepared_source}.rs`
- block／経済consumer照合先: `consensus/src/pipeline/virtual_processor/processor/palw_accept.rs`、`consensus/src/processes/coinbase.rs`、`consensus/core/src/palw_weight{,_block,_cap_v1}.rs`、`palw_bond_budget_fold_v1.rs`
