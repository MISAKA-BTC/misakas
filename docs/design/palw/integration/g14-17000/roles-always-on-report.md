# lane ROLES — G14 の役割を duty として常時 on(2026-10-27)

branch `claude/g14-roles`(基点は `claude/g14-panel1-17000` cd54005fb)。規則(09-25):protocol duty は起動 option ではない。producing(新しい work の投稿)だけはノードの選択とする。

| 役割 | 変更前 | 変更後 | fence 条件 |
|---|---|---|---|
| kernel-route 公開検証者 (`palw_kernel_verifier`) | `--palw-kernel-model` が空なら何もしない。kit はこの flag を渡さない → fleet では検証が行われない | flag で指定したファイルに加え、`<appdir>/<net>/palw-kernel-models/*.palwkm` を自動で保持 model とする(`palw_kernel_models_with_default_dir_v1`)。kit は任意の `KERNEL_MODEL` を渡せる | `palw_probabilistic_constraints_v1` または Panel=0 の fence が成立した後にだけ model を開く(既存の armed 判定、変更なし)。fee outpoint と bond も必要 |
| Panel=1 kernel seat / slice roles (`palw_kernel_roles`、producer 以外) | flag による gate はない(armed + fee outpoint + key)。ただし model を検証者の book から取るため、model がないと空回りする | 上記の model 自動読込で動く | 既存の armed 判定 |
| kernel producer (`--palw-kernel-producer`) | opt-in | **変更なし**(producing はノードの選択) | — |
| GDN step filer | `--palw-gdn-step-filer` による opt-in、既定 off | 既定 on。緊急停止は `--palw-disable-gdn-step-filer`。旧 flag は受理するが何もしない(ALWAYS-ON DUTY) | `palw_memory_routing_court_v2`(tick 内の既存判定) |
| 有料 court ingress(held/outsider filer) | `--palw-court-paid-ingress` による opt-in、既定 off | 既定 on。緊急停止は `--palw-disable-court-paid-ingress` | fee schedule の採択(typed-missing の間は休眠)かつ `palw_court_outsider_lane_v1` |
| Gate C 抽選検証者 / PanelDa 開示運搬 / FpSelection TFP1 provider fetch / Panel seat | コードを読んだ限り flag による gate はない(`palw_fp_seat.rs`、`p2_7_disclosure_policy.rs`、`palw_selected_event_responder.rs`、`palw_public_prefix.rs`、`seat_progress.rs` が参照するのは `config.params`・`retention_dir`・`fee_outpoint` だけ) | 変更なし(すでに duty) | 各 fence(params) |

fence 未満の挙動:2 種の filer は tick 冒頭の fence 判定で idle になる(log のみ)。kernel 検証者は fence 前に model を開かない。起動時に増えるのは default directory の一覧取得 1 回だけ(ファイルは読まない)。drill の extra seat では、従来どおり両 filer を off にする(`daemon.rs`)。

## kit
- `lib.sh`:`KERNEL_MODEL`(カンマ区切り)があれば `--palw-kernel-model=` を追加する。`fleet.env.example` には空の `KERNEL_MODEL=` を置いた。
- PALWKM01 は .palwart や class artifact からは導出できない(別形式の `MapParams`)。推奨は、登録 class の PALWKM01 を default directory に置く方法(ARGS が変わらない)。手順は `contrib/t12-deploy-kit/PLAN.md` の §15 追記にある。

## 資源(要確認)
- GDN step filer は 1 tick あたり発見 16、claim 1、tile probe 2 の上限がある。material session は held GDN filer と共有する。
- **kernel 検証者の再実行と GDN step filer の recompute は、`--palw-host-memory-share` の memory ledger にも replay slot にも予約しない**(コードを読んだ結果、どちらにも ledger/slot の参照がない)。このため BASE-0 audit や replay slot と取り合うことはないが、ledger の外でメモリを使う。小型の kernel model なら影響は小さいと見込むが、未測定。

## 試験・waiver
- 実施したのは `cargo check -p kaspad` だけ。
- **WAIVER-R1**:arg parse の単体試験は追加も実行もしていない(既定 on / disable flag)。
- **WAIVER-R2**:fence 17,000 を跨ぐ drill は行っていない(ユーザー指示で drill を skip)。
- **WAIVER-R3**:kernel 検証者と GDN step filer が ledger の外で使うメモリ量は未測定。
- **WAIVER-R4**:PALWKM01 ファイルを fleet に配置していない(配備は別途承認が要る)。配置しない限り、kernel 検証 duty は空回りする。

## R4 — Panel=1 slice の kernel model(wide128_v1(7))(2026-10-27 追記)

- 生成器:`misaka/palw/kernel/examples/gen-wide128-palwkm.rs`(`cargo run --release -p misaka-palw-kernel --example gen-wide128-palwkm -- <dir>`、kaspad の build は不要、決定論的)。出力は 2 つ:PALWKM01 と、E2E が card 1 で登録するのと同じ `RegisterClass` の encode 済み bytes(k2_tir_v2 descriptor、plan は 64 positions)。
- この Mac での実行結果(scratch。commit していない):
  - `wide128-v1-7.palwkm`:3 tensors、3170 B、sha256 `d071d3e20c906f0a4d1706b0005764642cb8c3bbdaecfec46e0b75ded79c3173`
  - `wide128-v1-7.register-class.bin`:sha256 `bdec4c6fdb2cf824cf13e9f6f7980310b094d58cb41b8c314f1a8473f31c3ccc`
  - param_root(`ParamCommitmentsV1::of`、RegisterClass の `param_commitments.root()`):`44c62870dea5d191ff2bc2eb8a0a1142ce4022212e6be7f19ad5893f052f22ea7f9033daba21885d655e619af557ab8e02091e139b46bb3169535313edcb960e`
  - param_root_v3:`62532c3fa452ad2912a9629981a2bf80c1d9024ae77ca238faaa6159890c125ddd407cd4a0f84352e6f3ba9d21d0605f3c1dec255a18a18e23da61496266d04b`
  - class_id(PanelLicensed なので binding id そのもの):`6a85c5bded8e3ba3fd777d5f40b145397395bb07030cce4f90b8e502a0af8c22d7ac4f5383dbf529ef128b427d70e17feb1862f31818f14a1d2b59d4de34ec55`
  - 由来:`misaka_palw_tir_sketch::fixture::wide128_v1(7)`(合成 fixture。実 checkpoint からの導出ではない)。
- kit:`distribute-from-mac.sh kernel-model <file>` が Mac 上で sha を照合し、3 host に `$KERNEL_MODEL_FILE.incoming` として送る。`install-*.sh stage` の `stage_kernel_model` が再照合して昇格させ、各 node の `<appdir>/misaka-testnet-12/palw-kernel-models/` に置き、置いた後にも照合する。fleet.env は `KERNEL_MODEL_FILE`、`KERNEL_MODEL_SHA256`。

### BLOCKER — 公開網の `RegisterClass` は consensus が拒否する
- `KernelLedgerV1::register_class`(`misaka/palw/kernel/src/ledger.rs` ≈1838)は `attested_artifacts` に artifact root がない限り「the artifact is not attested public by the consumer's registry」で拒否する。
- 本番 build で attest される経路は 2 つだけ(`consensus/core/src/palw_kernel_route_fold_v1.rs` ≈2130–2151):(a) 成熟した model artifact binding 行(`RegisterModelConformanceClassV2` / `RegisterFlatModelCandidateV7/V9` → conformance class になり、PanelLicensed ではない)、(b) OPV の conformance eligibility(optimistic mode のみ)。E2E が通るのは、`cfg(test)` 限定の `kernel_route_test_attest_artifact_v1` で attest しているから。
- このため、card 1 から `RegisterClass`(tag 1、Panel=1)を投げても live t12 では拒否される。進め方の選択肢:(i) fence 付きの consensus 変更で wide128 の param_root を attest する(指紋が変わる)、(ii) model artifact statement → conformance class の経路を使う(Panel=1 ではなくなる)。ユーザーの判断待ち。
- 補助ツールも欠けている:署名済み `KernelRouteV1` を任意に投稿する kaspad flag はない(あるのは poster の PostJob だけ)。最小の実装案は `--palw-kernel-register-class=<bin>` で、PostJob と同じ `kernel_roles_signed_v1` で bond 署名し、court outbox に載せる。BLOCKER の方針が決まるまで実装していない。
- 登録手順(BLOCKER 解消後):card 1(b1)の bond と fee outpoint を使い、上記 bin を投稿する → `palw_kernel_route_v1` RPC で class `6a85c5…` が出ることを確認 → 各 node の log に `kernel verifier: holds the model … commitment root 44c62870…` が出ることを確認。live への投稿は行っていない。

## R4 解消 — 17,000 fence で slice root を公開済みとして指定(2026-10-27、ユーザー決定)

- **ACT2 への連絡:** 定数は `kaspa_consensus_core::palw_kernel_route_v1::PALW_T12_KERNEL_SLICE_ATTESTED_ROOT_V1`(64 B、`44c62870…960e`)。fence は既存の `palw_kernel_route_panel_one_v1`(`PALW_T12_FULL_KERNEL_ROUTE_PANEL_ONE_ENTRY`。`PALW_T12_KERNEL_SLICE_FENCES_V1` に含まれる)。新しい Params 欄や fence は足していないので、ACT2 が 17,000 でこの fence を武装すれば同時に有効になる。
- consensus(`palw_kernel_route_fold_v1.rs`、attested set の組み立て):`kernel_route_panel_one_from_daa` 以上の block でだけ、この 1 行を attested set に加える。fence 未満では attested set は従来と同一。model を審査・判定する仕組みは作っていない(固定 1 行)。合成 fixture なので、公開性は seed から自明。
- 階層:kernel route の fold と ledger には c_m や tier の参照がない(grep で 0 件)。kernel claim は B=F=0 で poster 払いなので、係数 0.3 は配分に効く経路がない。登録 model として扱う上での 0.3 は §9 のとおりで、kernel route 側は c_m と結びついていない。
- 投稿ツール(kaspad):`--palw-kernel-register-class=<wide128-v1-7.register-class.bin>` は、その node の `--palw-producer-bond` で署名し、`--palw-fee-outpoint` で運ぶ。class が route に載るまで再投稿し、載ったら止まる。`--palw-kernel-register-class-dry-run` は class id・root・bond を log に出すだけで、何も投稿しない。kernel route の fence 前は不活性。
- 手順(live へは自分で投稿していない):(1) 17,000 を過ぎたら card 1 の node(b1)で、まず `--palw-kernel-register-class=… --palw-kernel-register-class-dry-run` を付けて起動し、log の class id が `6a85c5…ec55`、root が `44c62870…` であることを確認する。(2) dry-run を外して再起動(ARGS が変わるので `UPGRADE_ARGS_CHANGE_OK=1`)。(3) `palw_kernel_route_v1` で class が載ったことを確認し、各 node の log に `kernel verifier: holds the model … 44c62870…` が出ることを確認する。(4) 載ったら flag を外す。
- 試験:`g14_kernel_route_e2e::roles_slice_attest::roles_slice_root_is_attested_only_past_the_panel_one_fence` は PASS(fence 未満は拒否、fence 以後は登録、別 root の wide128_v1(8) は拒否)。test hook は process 全体で共有されるため、この試験は `--exact` で単独実行すること。`cargo check -p kaspad` も PASS。kaspad flag の parse 試験はしていない(WAIVER-R1 に追加)。
