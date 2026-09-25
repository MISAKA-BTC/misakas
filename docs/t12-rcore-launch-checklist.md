# testnet-12 R-core+ 公開チェックリスト（ADR-0152 §8.3 の launch gate と IA-14 の ship 条件）

作成: 2026-09-25。基準は統合線 `rcore/int-3` @ `a0af3c92`（このファイルは branch `rcore/release-prep`）。
ADR 本体は `docs/adr/0152-account-stake-staged-reserve-and-vested-rewards.md` v3.1（§8.3 が gate、§8.4 が公開後の観測）。
配備の手順と kit は `contrib/t12-deploy-kit/PLAN.md`、explorer は `contrib/misakascan-t12/DEPLOY.md`、公開後の drill は
`contrib/t12-drill-kit/README.md`。

**この文書は準備であり、何も実行しない。** 各項目の「証拠」は **出荷する commit の上で** 取り直して添付する（a0af3c92 で
GREEN でも、後の merge で動く）。**【確認】** の付いた step は、実行の前にユーザーの明示の確認を取る。

---

## 1. launch gate（ADR §8.3 item 1〜9）— 出荷 commit に添付する証拠

a0af3c92 の状態は 2026-09-25 に `git merge-base --is-ancestor` と `git grep` で確かめたもの。test の所在は test 名の接頭辞
（`fn tNN_…`）と ADR の番号の言及で探した。「所在不明」は a0af3c92 でその番号の test が見つからなかったもので、監査が
所在を示すか書く。

### item 1 — M1〜M5 GREEN（§7.1）。F1-M・F1c・stake 加重の抽選・addendum の T18q〜T18y・T18p-M・SR-10・U2/U3 を含む

添付: 下の各 file の test が出荷 commit で全部 PASS した battery のログ（§1 item 2 の 2 本）と、所在不明の番号についての監査の回答。

| 範囲 | ADR の test | 所在（a0af3c92） |
|---|---|---|
| M1（F2） | T46a〜T46n、T26、T31 | `consensus/src/pipeline/virtual_processor/tests/t46_false_valid_real_claim.rs`、`consensus/core/src/palw_state_v2.rs`（t26）。T31 は所在不明 |
| M2（F1、F1-M、F1c） | T18、T18b〜T18h、T18k、T18m、T-THREAD、T62 | `…/tests/t46_false_valid_real_claim.rs`（t18b〜t18k・t18m・t18y・T-THREAD）、`consensus/core/src/palw_offence_attribution_v1.rs`（T18h）、`…/tests/t12_round_lane_e2e.rs`（T-THREAD）。T62 は所在不明 |
| addendum | T18p、T18p-M、T18q〜T18y | `misaka-palw-base0/tests/seat_material_duty.rs`（t18p、T18p-M）、`misaka-palw-base0/tests/f1c_logits_not_step_output.rs`（t18q、t18r）、`…/tests/t47_model_class_attribution.rs`（T18s、T18t、t18u）、`consensus/core/tests/palw_court_decode_close_door.rs`（t18v）、`consensus/core/src/palw_attempt_rules_v1.rs`（t18w）、`consensus/core/tests/palw_offence_attribution_t11_verdicts.rs`（T18x）、`…/tests/t46_false_valid_real_claim.rs`（T18y） |
| S（担保・vesting・slash） | T01〜T05、T08、T09、T11〜T17、T20〜T24、T27、T28、T30、T33、T35〜T37、T39、T43、T44、T57、T76〜T78、T81、T82、T84 | `consensus/core/tests/rcore_s2_staged_reserve.rs`、`rcore_s3_one_ledger.rs`、`rcore_s4_conviction_funnel.rs`、`rcore_s6_per_bond_share.rs`、`consensus/core/src/palw_state_v2/tests/vesting_fold_v1.rs`、`consensus/core/tests/dos_repro_2_free_prompt_flood_linear_per_block_cost.rs`（T02、t02c、T17）、`dos_l5_4_reorg_fuzz.rs`（T09）、`kaspad/src/palw_producer_t12_tests.rs`（T08 の node 側）。T02b・T05・T28・T57 は所在不明（T05 は P2 側 `p2_b3_vesting_payee_gate.rs` にも言及あり） |
| M3（DA court） | T32、T34、T42、T64〜T69、T27 | `consensus/core/tests/rcore_m3_da_court.rs`、`…/tests/t46_false_valid_real_claim.rs` |
| M4（F4、SEAT-R、stake 加重の抽選、SR-10） | T06、T15、T45、T70〜T74、T85〜T94、T18p-M | `consensus/core/src/palw_panel_v2.rs`（t85〜t94）、`consensus/core/tests/adr0152_sw9_ready_eff_room.rs`（t91）、`consensus/core/tests/palw_rcore_q5_gate.rs`（T72、t72b）、`…/tests/t12_rcore_sr10_door_gate.rs`（T33、T45、T72）、`consensus/core/src/palw_state_v2.rs`（t70、t71、t74）、`consensus/core/src/palw_da_rcore_v1.rs`（T73）、`consensus/src/pipeline/virtual_processor/tests/t12_stake_draw_integration.rs`（T89 / IA-1）。**T06（EV grid）と T90 は所在不明** — T06 の閾値は `docs/handoff/t12-rcore-20260924/v3calc/v31_stake_draw.py` 側にある可能性 |
| M5（reorg・restart・golden） | T07、T40、T41、T75、T83、T01 の revert/IBD twin | `consensus/core/tests/rcore_m5_reorg.rs`、`rcore_m5_restart.rs`、`rcore_m5_v22_golden.rs`。**`rcore/m5b-tests` @ 8f3adebb（T40 の Q-5 DL-1 行、T01 の twin）は a0af3c92 に未 merge** |

### item 2 — kaspa-consensus の battery を 2 回（既定 features と `--features evm`）

添付: 2 本のログの `test result:` 行の合計（passed / failed / ignored）と、失敗があればその名前と既知扱いの根拠（例:
監査が既知とした `dos_repro_3d`）。1 つの cargo を 1 本ずつ（`CARGO_BUILD_JOBS=4 CARGO_INCREMENTAL=0`）。

```bash
export CARGO_TARGET_DIR=<出荷 commit 用の target> CARGO_BUILD_JOBS=4 CARGO_INCREMENTAL=0
cargo test --locked -p kaspa-consensus --lib --tests --no-fail-fast                  > battery-consensus-default.log 2>&1
cargo test --locked -p kaspa-consensus --lib --tests --no-fail-fast --features evm   > battery-consensus-evm.log 2>&1
# 同じ commit で合わせて取る（gate の他の項目の証拠になる）
cargo test --locked -p kaspa-consensus-core --lib --tests --no-fail-fast             > battery-core.log 2>&1
cargo test --locked -p kaspad --lib --no-fail-fast                                   > battery-kaspad.log 2>&1
cargo test --locked -p misaka-palw-base0 --tests --no-fail-fast                      > battery-base0.log 2>&1
cargo test --locked -p misaka-cli --no-fail-fast                                     > battery-cli.log 2>&1
awk '/test result/ {for(i=1;i<=NF;i++){if($i=="passed;")p+=$(i-1); if($i=="failed;")f+=$(i-1)}} END{print "passed",p,"failed",f}' battery-*.log
```

### item 3 — short drill D-1〜D-10 → **公開後に移動**（運用者決定 2026-09-24、§9.3 Q12 (b)）

公開の gate ではない。公開後に `contrib/t12-drill-kit`（P2-12 の salt、出荷した binary、公開 node の無い 4 台以上のホスト）で行う。
D-1〜D-10 の中身は `scripts/misaka-palw-t12-rcore-drill.sh steps`。drill 用ホストの用意は運用者（PLAN R4）。

### item 4 — T41 の golden は出荷 commit のもの

添付: §5 の再 pin 後に `rcore_m5_v22_golden.rs` と `the_version_22_state_root_golden_vectors` が PASS したログ、再 pin の diff。

### item 5 — Phase 2 の要の部分が出荷 commit で GREEN（C12）

添付: T03、T05、T23（両半分: `palw_v2_locked_bond_outpoints` / `palw_v2_bond_burn_obligations` で UTXO 層に B-3）、T25、T47〜T53、T58 の PASS。
所在（a0af3c92）: `consensus/src/pipeline/virtual_processor/tests/p2_mint_path.rs`（T03、T05、T25、T29、T48、T49、T50、T56、T58）、
`p2_b3_vesting_payee_gate.rs`（T05、T23）、`p2_reorg_and_ibd.rs`（T48、T49）、`p2_evm_twin.rs`（T50、`--features evm`）、
`p2_t29_conviction_and_maturity.rs`、`consensus/core/src/palw_vesting_read_v1.rs` / `misaka-cli/src/palw_vesting.rs`（T51）、
`misaka-cli/src/operator/*` / `kaspad/src/palw_panel.rs`（T52）、`consensus/src/pipeline/virtual_processor/tests/t53_drill_isolation.rs` と
`kaspad/src/palw_drill.rs`（T53）、`consensus/core/src/palw_state_v2/tests/vesting_fold_v1.rs`（T47、T58）。
T54a と T54b は processor 層の半分が `consensus/src/pipeline/virtual_processor/tests/t46_false_valid_real_claim.rs` にある:
`t34_t54a_an_unserved_seats_automatic_accusation_defaults_a_silent_producer_and_pays_it_through_3d`（T54a、:4771）と
`t32_t54b_a_silent_producers_covering_signer_answers_and_the_accusers_pay`（T54b、:4546）。どちらも doc が「devnet preset 上の node e2e は
公開後」と書いている。T54c・T54d〜T54g・T55 は所在不明または P2-8（`rcore/p2-file`、未 merge）に依存。D-4 と D-5 は P2-7 と P2-8 が要る。

### item 6 — SEAT-R は F2 の fence と同じ binary（T18p-M GREEN）、SEAT-S1〜S4 と `PalwDrillFaultV1` も

a0af3c92: `palw_seat_r_in_force_v1`（`kaspad/src/palw_panel.rs`、test `seat_r_is_in_force_on_testnet_12_from_genesis_and_nowhere_else`）、
`PalwDrillFaultV1`（`consensus/core/src/palw_backend.rs`、`misaka-palw-base0/src/{backend,produce}.rs`）、T18p-M（`seat_material_duty.rs`）、
SEAT-S1/S2/S4（`fix/t12-seat-s1s2s4` は merge 済み、`misaka-palw-base0/tests/seat_s1_whole_job.rs`・`seat_s2_output_root.rs`・`seat_s4_*.rs`）、
SEAT-0（`fix/t12-seat0-launch` は merge 済み。`fix/t12-live-seat0` cec601e7 は live 用で未 merge — 公開 line には不要か監査が確認）。
SEAT-S3（`AnyValid` の site が立つ条件の一つ）: `palw_seat_s3_sample_v1`（`kaspad/src/palw_panel.rs`、doc は「capture は
`verify_material(capture, roots) == Matches` の後にだけ使う」）、test `seat_s_tests::c1_s3_never_attests_past_the_fence_without_verify_material`
と `q7_sampled_and_collector_tests::s3_samples_only_a_capture_seat_s3_verified`（どちらも `kaspad/src/palw_panel.rs`）。
添付: 上の test（SEAT-S1・S2・S3・S4・T18p-M・SEAT-R）の PASS と、`AnyValid` の site（contradiction 9 と 13）が SEAT-S1・S3・S4 と
T18p-M の GREEN の上で立っていること（Q-6。さもなければ build の前に `Whole` へ）。

```bash
cargo test --locked -p kaspad --lib -- seat_s_tests::c1_s3_never_attests_past_the_fence_without_verify_material \
    q7_sampled_and_collector_tests::s3_samples_only_a_capture_seat_s3_verified seat_r_is_in_force_on_testnet_12_from_genesis_and_nowhere_else
```

### item 7 — `AttnFused` の held-class の穴（8k は A-held、2M は閉じる）

a0af3c92 には **無い**: `feat/t12-aheld` @ 19736312、`feat/t12-aheld-node` @ c68479db（object 57 `CourtAttnRootClaimedHeld` の自動応答、
N4）、その下の `fix/t12-shard-court-openings-first` @ 8be0f661。2M を閉じる 4-quater（`feat/t12-class-verify-deadline` @ a4323997、
`ClassDeadlineUnmeasured`）も無い。8k の実 weight timing drill は公開後（IA-12）。
添付: A-held の C1〜C5 と object 57 の test（8k fixture で attention の嘘が有罪になる、kaspad producer が期限内に object 57 で答える）、
2M の attempt と FP claim が `ClassDeadlineUnmeasured` で拒否される test（O-11 の T-D2）。

### item 8 — launch line の修正が出荷 commit にある

a0af3c92: licence-stall（a4dfe903、d94d3a1b）あり、panel-room の C7 re-key（e93be0f2、f8c91f19、
`consensus/core/tests/panel_room_short_class_is_released_at_licence.rs`、T20、T21）あり、**shard court の one-move rule（8be0f661、J-8）は無い**（A-held と一緒に入る）。

### item 9 — ship 条件（IA-14）: §2

---

## 2. IA-14 の ship 条件（各項目を出荷 commit で）

| # | 条件 | a0af3c92 | 添付する証拠 |
|---|---|---|---|
| 9-1 | F2 と F1 の fence（`palw_offence_attribution`）は SEAT-S1・SEAT-S2・SEAT-R と一緒にだけ出す。SEAT-S2 の kaspad 半分は `palw_seat_replay_step_v1` で `CoreV1` の `output_root` を比べる | あり（`449fd892` の実 model-class replay test、`palw_seat_replay_step_v1`） | `449fd892` の test（held A16 v7 行: 正直な `CoreV1` claim が licence、Legacy の `output_root` は refute）の PASS |
| 9-2 | `AnyValid` fence の H-1（S3 の layer-sample は `verify_material` の前に Valid を署名しない）と H-2（FP S1 resume は `palw_fp_job_pin_of_context_v1(&ctx) == duty.fp_job_pin_v1()`） | H-1 あり（`1e20edd50`、test `c1_s3_never_attests_past_the_fence_without_verify_material`）。H-2 あり（`kaspad/src/palw_panel.rs` の `palw_fp_job_pin_of_context_v1(&ctx) != pin` の拒否、`d675423d` 取り込み済み） | 両 test の PASS、T18p-M の PASS。H-2 が入っていなければ identity site は `Whole` のまま |
| 9-3 | producer の V2 DA responder が `palw_rcore_plus` を武装する全 build にある（honest producer が告発に答え、課金されない processor e2e） | P2-7 merge 済み（7c2850b3、`palw_disclosure_duties_v1` が自分の claim と lock が覆う claim の session を列挙）。processor e2e は `t46_false_valid_real_claim.rs` の `p2_7_an_honest_producer_answers_two_accusers_with_the_nodes_builder_and_is_not_charged`（:4428）と `p2_7_a_held_session_on_an_attempt_claim_is_answered_from_its_capture`（:4655） | 両 test の PASS。監査の M3 review F2（CRITICAL）が閉じたことの確認。**運用上の飢え**（seat replay 2 本が DA 応答の余地を取る、PLAN §2.5 R-2b）は §7 の open item |
| 9-4 | `PALW_RCORE_SEAT_DA_ANSWER_LANDED_V1` は seat の DA 自動応答（P2-7）と同じ commit でだけ `true` | `true`（`consensus/core/src/palw_da_rcore_v1.rs:67`）、P2-7 と同じ line | flag の値と、covering signer の `MaterialDisclosedV2` の test |
| 9-5 | `misaka-palw-derive` と stranger script は network の attempt rule（t12 は `CoreV1`）で `output_root` を再計算。FP worker も | あり（`a4682a8d`、`d675423d`）。stranger script は `scripts/misaka-palw-derive-stranger.py` | `cargo test --locked -p misaka-palw-derive --test output_root_rules`（`the_network_names_the_rule_testnet_12_core_v1_and_testnet_11_legacy`、`derive_recomputes_the_chains_root_on_every_testnet_12_model_class`、`the_output_commitment_is_the_strangers_pinned_literal`）、`python3 scripts/misaka-palw-derive-stranger.py selftest`（build 不要）、FP worker の rule の test の PASS |
| 9-6 | `PALW_RCORE_VESTING_ROWS_LANDED_V1 = true` と S-4 の funnel（kaspad は false なら起動しない） | `true`（`consensus/core/src/palw_state_v2.rs:2846`）、S-4 merge 済み | flag の値と `rcore_s4_conviction_funnel.rs` の PASS |
| 9-7 | 4-quater の consensus 半分と P-1（`palw_class_verify_deadline`、pruning depth ≈ 74,920 DAA、D_cap 16,000）が genesis params にある — 2M は規則で閉じる | **無い**（`feat/t12-class-verify-deadline` 未 merge）。pruning depth は regenesis でしか決められない | 出荷 commit の t12 params の値（`probe-identity-local.sh` の fingerprint が動くこと）と O-11 の拒否 test |
| 9-8 | A-held と shard court（`feat/t12-aheld`、8be0f661 の上）が監査の review 後に merge、B の routing・object 57 の自動応答・N4 | **無い** | 監査 review の結論、merge commit、item 7 の test |
| — | 公開条件ではない（IA-13）: SEAT-S4 の forged-sibling residual（2M の flag day 項目） | — | — |

---

## 3. testnet-12 の identity（§5 の tool が書く）

**8270cf03 での暫定値（出荷値ではない）** — `scripts/t12-repin.sh` が build から計算した値（`consensus/core/tests/t12_repin_values.rs` の
`REPIN` 行。node が起動ログに出すのと同じ関数: fingerprint は `Params::from(testnet-12).consensus_params_id()`、genesis は premine から
再計算した utxo commitment・merkle root・header hash）。下の block は §5 の `--apply` が書き換え（`# re-pin` 行が理由）、太字の表示も
tool が書く（`--apply` は計算した commit に、出荷 commit での `--apply --shipping` は「出荷 commit（<commit> ＋ 再 pin）の値」に）。
release build の probe（§4 step 10〜11 の `IDENTITY`）がこれと一致しなければ止める。

```
# re-pin 2026-09-25 @8270cf032d24: provisional ids at rcore/int-3 8270cf03 (the class-verify-deadline merge a66509f9 moved t12's params and schedule ids after a0af3c92) (was 8a481023…, 8b0ee13c…)
EXPECT_FP=99eae89db05887c0ee21e451d5a78bd296533db59eb818edb3a4a320565c0ba3
EXPECT_GENESIS=a27f8f44fe4d91a5bed940be9dbd6d260ccb95cc00d948b1c08ddb6bd1a5f02542a6cf35c7a4d959ba4863ac1557861671763e5cc22937c697870283a8ca1f23
PREMINE_TXID=5e0d5f1b37a71288cc0eb24acc10d2f4973dd3475569f274f03cc64a2233d035099d386e24c91d48427c30a895664dea979abedc90a7788fad170379e55e2669
# schedule id 5f53b691c98dbd4352835d18699c3051af209064c8ff79e2a920bbcfae421c1b（fence schedule "1000"）
# rule manifest digest 9def81a1c56c02d5d1f9d24c5ddbe78d6f7598b31928ab6f388a2a074f14b4d4d4cf8c2245666bc13b9c91688c106efe428e8643075b68b40c4820adb4201d8d
```

経緯: a0af3c92 で `probe-identity-local.sh` が読んだ値は fingerprint `8a481023…`・schedule id `8b0ee13c…`（genesis・premine txid・rule
manifest は同じ）。deadline＋P-1 の merge（a66509f9、t12 の pruning depth 12,002 → 74,920）が params id と schedule id を動かした。
8270cf03 の後の A-held・Activation Pool（readiness horizon を含む）・P2-8e・本番値の merge もこれを動かす（§5 の再 pin が吸収する）。

drill salt `5353…53`（例示用。実際の drill の salt ではない）の drill genesis は `1678f353…70349`（`probe-identity-local.sh --drill-salt` が
出す値。tool が t12 の genesis と一緒に計算して書き換える）。

---

## 4. 順序（出荷 commit から公開後まで）

| step | 何を | 誰 | 確認 |
|---|---|---|---|
| 1 | **deadline ＋ P-1**（監査: `feat/t12-class-verify-deadline`、4-quater の consensus 半分、pruning depth ≈ 74,920）を統合線に merge。監査の review 後 | 監査 → 実装 | |
| 2 | **A-held の node 半分**（`feat/t12-aheld` ＋ `feat/t12-aheld-node`、8be0f661 を含む）を merge。review 後 | 監査 → 実装 | |
| 3 | **Activation Pool**（`feat/t12-activation-pool`）を merge（genesis の行を動かし得る） | 実装 | |
| 4 | **B の lane**（`rcore/p2-file` の P2-8 filer 3 本、`rcore/m5b-tests`、必要なら `rcore/readiness-escalation`）を merge。merge は手で解いたら push 前に build（メモリ則） | 実装 | |
| 5 | §1・§2 の各 test を出荷候補で確認し、所在不明の番号を監査と詰める | 実装・監査 | |
| 6 | **再 pin**（§5）: `scripts/t12-repin.sh --shipping`（dry run）→ `--apply --shipping --reason "…"`（t11 の layout golden が動くなら監査の確認の後に `--allow-t11-layout`）→ もう一度 `--shipping` の dry run で `no drift`（exit 0）。t12 の identity と kit の写しを probe でも読み直し（`probe-identity-local.sh --build --layout`、exit 0 かつ §3 の block と一致）、resource profile を再評価して PLAN §2 の表と install-*.sh の share / memmax を直す: `cargo test --locked -p kaspad --test t12_role_memory_figures -- --ignored --nocapture`（全行が `OK`） | 実装 | |
| 7 | genesis が a27f8f44 から動いたら: 旧 genesis を `fleet.env.example` の `FORBIDDEN_GENESIS` と `t12_regenesis.rs` の superseded 接頭辞リストに足すのは step 6 の `--apply` が行う（`genesis.rs`・§3・regenesis 記録の表・join 文書・explorer の写しも）。tool が「Other mentions of the old values」に出した散文（PLAN・DEPLOY・docs の説明文）を手で直す | 実装 | |
| 8 | **最終 battery を 2 回**（§1 item 2 のコマンド、既定と `--features evm`）。同じ commit で core・kaspad・base0・cli も | 実装 | |
| 9 | 出荷 commit を origin に push | ユーザー | **【確認】** |
| 10 | **fleet の build host（5.104）で release build**: `build-release-5104.sh <commit>`。IDENTITY が step 6 の値と一致 | ユーザー | **【確認】**（公開予定ホストでの 20 分超のビルド） |
| 11 | `fleet.env` を埋める（`REV`・sha256 3 本・`EXPECT_FP`・`EXPECT_GENESIS`・`PREMINE_TXID`・`HB_ADDR`）→ `distribute-from-mac.sh kit / binaries / artifact` → 各 host で `preflight` → `stage` | ユーザー | **【確認】**（リモートへの書き込み） |
| 12 | **ユーザーの最終確認**: 公開 node の停止・置換の時間帯、告知の文面、DNS validator と faucet の資金 | ユーザー | **【確認】** |
| 13 | **配備**: ibm → .113 → 5.104 の順に `install-<host>.sh switch`（公開 node を止めて置き換える）。host ごとの前提（PLAN §5）: **ibm** — §10 Q12 の旧モデル削除で disk を空ける、`DISABLE_T11_NODE0=1`（Q5）。**.113** — `systemctl stop misaka-validator misaka-validator-2`（Q7。switch は動いていれば拒否）。**5.104** — B0: 別 session の t12f/t12p/scan/daa-obs node の停止（switch は拒否）、t11 fixture node の RSS が RESERVE_MIB（4,096）に収まること（preflight が表示）。各 host で preflight が OK | ユーザー | **【確認】**（host ごと） |
| 14 | **explorer**: `contrib/misakascan-t12/DEPLOY.md` §9 の 1 系統（推奨 `deploy.sh`）で nginx・filler・DB・app.js | ユーザー | **【確認】** |
| 15 | **seeder**: 触らない（KEEP）。`seeders/40-verify.sh` と `60-join-check.sh` | ユーザー | **【確認】**（60 は 5.104 で使い捨て node） |
| 16 | `check-fleet.sh`、`CHECK_REGISTRY=1 check-fleet.sh`（PLAN §5 の合格条件） | ユーザー | |
| 17 | **告知**（O-5 が合格するまで t12 は価値を持たない、と書く） | ユーザー | **【確認】** |
| 18 | 公開後: DNS validator 6 × 20M MSK と faucet の資金を premine の主ウォレットから（運用者の鍵）。validator 2 本の t12 化（PLAN §10 Q7） | 運用者 | **【確認】**（資金移動はユーザーが行う） |
| 19 | 公開後の観測 O-1〜O-13（§6）と後回しの drill（`contrib/t12-drill-kit`） | B（A が review） | drill ホストは運用者が用意（**【確認】**） |

---

## 5. 再 pin の手順（出荷 commit で一度だけ。step 6）

pin を動かす前に、何が動いたのかを 1 つずつ言えること（「両側を残す機械的 merge」や「default hash は全てと一致する」の事故則）。
値は **計算して貼る**（手で打たない）。その作業は `scripts/t12-repin.sh`（本体と pin の registry は `scripts/t12_repin.py`）が行う。
計算側はこの tree の build だけ: `consensus/core/tests/t12_repin_values.rs` の `REPIN` 行（全 preset の params / identity / schedule id、
全 network の genesis を premine から再計算した hash・merkle root・utxo commitment、t12 の premine / community txid、class id と C7 の導出、
kit の値、例示 drill salt の drill genesis）、pin test が assert の前に print する値（「その fence だけが t12 を動かした」twin、readiness
horizon の twin と t12 の全 id、T41、t11 parity の dump、dormant ring root、t11 verdict の root、corpus の verdict digest）、committed class
manifest を JSON として読んだ値（`class_manifest_const_v1` の転記 test の第二経路）。

**手順**（出荷候補の commit で。`CARGO_TARGET_DIR` はその build のもの、`CARGO_BUILD_JOBS=4`）:

1. `scripts/t12-repin.sh --shipping` — dry run。consensus-core の pin test 群を build して走らせ、全 pin を `pinned vs computed` の表にする
   （初回は consensus-core の test build、Mac Studio で約 12 分）。`--shipping` は「これが出荷 tree」の宣言で、merge が持ち込む pin file
   （`palw_activation_pool_is_t12_only.rs`・`palw_readiness_horizon_is_t12_only.rs`）と A-held の gate test が無ければ `NOT FOUND` / `CHECK`
   にする（`--shipping` 無しでは merge 前の `absent` として数えない。ただし HEAD の履歴に一度あった pin file が無いのは常に `NOT FOUND`）。
   exit: **0** = drift 無し・gate 全 pass・未登録 literal 無し、**1** = 書き換えで直る drift だけ（行ごとに `-> MOVE`）、**3** = `REFUSE`・
   `NOT COMPUTED`・`NOT FOUND`・`CHECK`・`UNREGISTERED` のどれか（drift の有無によらない）、**4** = build 失敗。
   - `DRIFT -> MOVE` の各行について、**どの merge がそれを動かしたかを言えること**。言えなければ止める。
   - `REFUSE` があれば止める。testnet-11・testnet-10・devnet・simnet の pin が動いた build は出荷しない（`would fail:` がその test）。
   - `NOT COMPUTED` は、値を出す test が print より前で落ちた（例: twin の前提 `pruning_depth == 12_002` が本番値の変更で動いた）。
     前提を直して再実行する。`NOT FOUND` は pin の書式か anchor が変わった、または pin file が消えた: registry を直す。
   - `UNREGISTERED` は consensus-core の integration test（`consensus/core/tests/*.rs` 全部を glob で読む）に registry が知らない 64/128 桁の
     hex literal がある（merge で足された pin）。`scripts/t12_repin.py` の `registry()` に 1 行足す。値を test の中で作る pin なら、その test
     に assert より前の print も足す（twin の print と同じ形）。
   - `CHECK` は書き換えでは直らないもの（explorer の `LLM_CLASSES` に genesis class の行が足りない・余る、genesis class の並びが [8k, 2M] で
     ない、C7 の導出が 1 本でない、card の premine index が 0..7 でない、T41 の record と `want` の名前・数が合わない、gate test の失敗、
     `--shipping` で表示が暫定のまま）。書き換えが直す写し（`app.js` の class id など）は、書き換えた後の姿で判定するので `CHECK` にならない。
   - `scripts/t12-repin.sh --selftest`（build 不要、直前の harvest log を 1 値ずつ書き換えて判定規則を確かめる。まだ無い pin file と gate の
     場合は SKIP）。
2. `scripts/t12-repin.sh --apply --shipping --reason "<何が動かしたか、1 行>"` — `MOVE` の pin を書き換え、各書き換え（表の行・tuple const・
   genesis field・`want` 配列ごと）の上に `re-pin <日付> @<commit>: <reason> (was <旧値>…)` の 1 行 comment を残す（Markdown の表と散文には
   残さない。commit message が理由を持つ）。**round で進む**: genesis 定数と C7 の literal（`PALW_T12_2M_CLASS_ID_BYTES`）は pin であると
   同時に入力（params id がそれを hash する）なので、それが動いたら 1 round 目はそれだけを書き換えて build し直し、読み直した値で 2 round 目
   に残りを書き換え、最後の round は drift 0 を確かめる build（確認の dry run を兼ねる）。dry run がそれと一緒に出す fingerprint・twin・写し
   の値は古い定数の上で計算したもので、`--apply` の 2 round 目で出る値が正しい（salt を変えた試験では fingerprint が 1 round 目
   `fe4e0c92…`、2 round 目 `5633ba0d…` だった）。t12 の genesis が動いたら旧 genesis を `fleet.env.example` の `FORBIDDEN_GENESIS`（全桁と
   comment 行）と `t12_regenesis.rs` の superseded 接頭辞リストに足す。§3・PLAN §4・misakascan `DEPLOY.md` の写しの block の太字の表示
   （「<commit> での暫定値（出荷値ではない）」）は、`--shipping` なら「出荷 commit（<commit> ＋ 再 pin）の値」に書き換える（無しなら
   書き換えた file の表示の commit だけ）。その後、pin を持つ test を全部走らせて結果を出す（lib の genesis / params / C7 / class manifest /
   v22 golden と A-held の `held_forfeits_v1`、pin のある integration target 全部、`t12_deploy_kit_constants`・`t12_regenesis`・
   `t12_repin_values`。exit 5 = どれかが落ちた）。どの round でも `REFUSE`・`NOT COMPUTED`・`NOT FOUND`・`CHECK`・`UNREGISTERED` が
   1 つでもあればそこで止まる（exit 3。1 round 目なら何も書いていない）。
3. `--apply` の最後の round が drift 0 でなければ exit 3/4/5 で止まる。commit の前にもう一度 `scripts/t12-repin.sh --shipping` →
   `DRY RUN: no drift`（exit 0）。`--apply` が「Other mentions of the old values」に出した行（PLAN・misakascan DEPLOY・docs の散文、
   fleet.env.example の説明 comment など）を手で直す。`git diff` を読み、`re-pin` comment の reason を確かめ、record の encoding が動いた
   なら T41 の test の doc にどの field がなぜ動いたかを書いて commit。
4. identity の照合: dry run の末尾の `computed from.testnet-12.params_id / genesis.testnet-12.hash / testnet-12.premine_txid /
   from.testnet-12.schedule_id / rule_manifest.digest` が §3 の block（`--apply` が書いた）と同じで、
   `contrib/t12-deploy-kit/probe-identity-local.sh --build --layout`（exit 0）と release build の `IDENTITY`（§4 step 10）がそれと同じこと。
   fleet.env は release build の probe から埋める（§3 から貼らない）。
5. その後に step 8 の battery を 2 回（`-p kaspa-consensus-core --tests` と `-p kaspad` が上の pin test を全部含む）。

**判定の規則**（scope は registry の各行にある）:

| scope | 主な pin | drift したとき |
|---|---|---|
| `t12` | `genesis.rs` の `PALW_T12_GENESIS`（hash・merkle root・utxo commitment）、`params.rs` の C7 literal `PALW_T12_2M_CLASS_ID_BYTES`、twin（`T12_BEFORE_THE_ATTRIBUTION`・`…_AND_THE_DEADLINE`・`T12_AT_THE_PARENT_WITHOUT_THE_ATTRIBUTION`・`T12_BEFORE_THE_HORIZON`）、`T12_WITH_THE_HORIZON`（t12 の全 id）、`the_held_rows_are_the_fleets_classes` の class id、`class_manifest_const_v1` の転記 6 値、T41 の inhabited root / carriage | MOVE |
| `mainnet` | `shipped_presets_have_pinned_fingerprints` の mainnet、`AT_V22`・`BEFORE_THE_FLOOR`・`BEFORE_THE_ATTRIBUTION`・`BEFORE_THE_DEADLINE`・`BEFORE_THE_POOL`・`BEFORE_THE_HORIZON` の mainnet 行と `AT_V21` の mainnet 行（同じ test が等しいと assert）、`MAINNET_CONSENSUS_PARAMS_ID`、`GENESIS` | MOVE（本番値の変更。mainnet は未公開） |
| `layout` | T41 の empty root / carriage と record encoding（`want` の各 literal の `// <Record>` comment で名指し）、`palw_state_v2` の `want_empty` / `want_full` | MOVE（v22 の encoding が動いた。T41 の doc に理由） |
| `copy` | `fleet.env.example`（`CLASS_8K`・`FEE_FLOAT_BASE`・`ART_8K_BYTES`・`MANIFEST_8K_SHA256`・`HB_ADDR`）、`lib.sh` の `CLASS_2M_PREFIX`、`app.js` の `PANEL_BOND_TX`・`LLM_CLASSES`、`params.rs` の C7 の doc の略記、§3 の block と例示 drill genesis、PLAN §4 の暫定値、regenesis 記録の表、`testnet12-join-mining.md`、misakascan `DEPLOY.md` | MOVE |
| `label` | §3・PLAN §4・misakascan `DEPLOY.md` の写しの block の太字の表示 | 比べない。`--apply` が commit を、`--apply --shipping` が出荷の表示を書く。`--shipping` の dry run で暫定のままなら `CHECK` |
| `t11-layout` | t11 parity の `PARENT_DUMP_BLAKE2B_256`、`t12_two_clock_ring` の `DORMANT_GOLDEN_ROOT`、t11 verdict の `ROOT_*` 3 本 | 同じ run で `layout` の pin が動き、隣の非 root 値（parity の roots-masked digest・長さ・行数 / verdict の lock・collateral）が動いておらず、**かつ `--allow-t11-layout` を付けたとき**だけ MOVE。flag 無しでは REFUSE にして「AUDIT SIGN-OFF NEEDED」に監査が確かめること（動いた layout pin、動く t11 golden、t11 の fold が encoding 以外で動いていないこと）を出す。ring root と verdict root の guard は parity の masked digest ほど強くないので、flag は監査の確認の後だけ付ける（下の 5） |
| `verdict` | `CORPUS_VERDICT_DIGEST`（ADR-0150、stateless で全 network） | `--allow-verdict` かつ rule manifest digest が動いたときだけ MOVE（`PALW_CONSENSUS_RULE_MANIFEST_V1` の revision を上げた commit） |
| `testnet-11`・`devnet`・`testnet-10`・`simnet` | 上の表の各 network の行、`T11_CONSENSUS_*`、各 genesis、parity の roots-masked digest・長さ・行数、t11 verdict の lock・collateral | REFUSE |

registry が持つ pin の所在（表の読み方。書き換えは tool が行う）:

1. **t12 の identity**: `consensus/core/src/config/genesis.rs` の `PALW_T12_GENESIS`（`test_genesis_hashes`、
   `every_genesis_commits_to_the_premine_this_build_mints` — `genesis.rs` の doc が旧名 `every_shipped_genesis_commits_to_its_own_premine` で
   呼んでいる test）。premine txid はコードに pin されていない（`premine_txid_for(testnet-12)` が salt から導く）: 動くのは
   `PALW_T12_PREMINE_SALT` か sentinel が動いたときだけで、そのとき写し（`app.js` `PANEL_BOND_TX`、§3、PLAN、regenesis 記録、join 文書）が
   drift する。`TESTNET12_COMMUNITY_TXID` は sentinel（入力）で pin ではない。**t12 の fingerprint そのもの**は readiness horizon の merge
   からコードに pin される: `palw_readiness_horizon_is_t12_only.rs` の `T12_WITH_THE_HORIZON`（shipped preset の params / identity /
   schedule id。tool は `palw_t12_shipped_params()` の値と比べる）。それまでは §3 と kit の写しだけ。
2. **twin**（その fence だけが t12 を動かした）: `palw_offence_attribution_is_t12_only.rs`、`evm_bridge_ledger_is_t12_only.rs`
   （Activation Pool と readiness horizon の merge 後は、その 2 つも外した値。horizon の branch が両 test で `palw_activation_pool = None` と
   `palw_readiness_v2_max_age_spans = None` にする）、`palw_readiness_horizon_is_t12_only.rs` の `T12_BEFORE_THE_HORIZON`（test が
   `testnet-12 with the horizon (…) / without (…)` を assert の前に print する）。
3. **t11 / devnet / mainnet が動いていないことの pin**: `params.rs` の `shipped_presets_have_pinned_fingerprints`、
   `palw_rcore_plus_is_t12_only.rs`（`AT_V22`。`AT_V21` は f1192685 の記録で、mainnet 行以外は比べない）、`palw_clock_floor_is_t12_only.rs`、
   `palw_offence_attribution_is_t12_only.rs`、`palw_class_verify_deadline_is_t12_only.rs`、`palw_activation_pool_is_t12_only.rs`・
   `palw_readiness_horizon_is_t12_only.rs`（merge 後。`BEFORE_THE_POOL`・`BEFORE_THE_HORIZON`）、`palw_the_release_did_not_move.rs`。
4. **T41 の v22 golden**: `rcore_m5_v22_golden.rs` の 4 値と record（`want` の各 literal を行末の `// <Record>` comment で名指し。test は
   全 record を print してから数を assert する）、`palw_state_v2.rs` の `the_version_22_state_root_golden_vectors`（lib test。値は失敗 message
   にだけ出るので単独で走らせ、pass なら pin が現在値）。v22 は一度だけ切る（v23 は作らない、handoff §3）。T41 は R-core+ の item しか
   埋めないので、**root block の並びは固定していない**。並びの pin は literal の無い gate test で、A-held の merge で入る（tool が harvest で
   走らせ、落ちれば `CHECK`、`--shipping` で無ければ `CHECK`。両方の merge の後に pass すること）:

   | gate（`palw_state_v2::tests::held_forfeits_v1::`） | 固定するもの |
   |---|---|
   | `the_held_forfeits_block_and_tail_come_after_r_core_plus_and_the_activation_pool` | root block `rcore_plus/v1` < `activation_pool/v1` < `held_forfeits/v1`、carriage tail 0xB4 < 0xB5 < 0xB6（source を読む） |
   | `the_held_forfeit_entry_applies_reverts_and_the_placeholders_are_refused` | delta entry 80（held forfeit）と、Activation Pool 用に予約した 76〜79 の placeholder の拒否。**pool の merge が 76〜79 を実 entry にするなら、この test をその merge で直す**（A-held と pool の `palw_state_v2.rs` は merge で衝突する） |

5. **t11 の休眠 parity**: `panel_room_t11_dormant_parity.rs` の `PARENT_DUMP_BLAKE2B_256` は v22 layout が動いたときだけ動く。それを
   区別するために同じ dump の state root を伏せた digest（`PARENT_DUMP_ROOTS_MASKED_BLAKE2B_256`、8270cf03 で固定、書き換えない）と長さ・
   行数を pin した。`t12_two_clock_ring.rs` の `DORMANT_GOLDEN_ROOT` と `palw_offence_attribution_t11_verdicts.rs` の root 3 本にはこれほど
   強い区別が無い（ring は前提の assert、verdict は lock と collateral だけ）。だから t11-layout の pin は `--allow-t11-layout` 無しでは
   動かさない。監査が「AUDIT SIGN-OFF NEEDED」の 3 点（動いた layout pin とその理由、動く t11 golden、t11 の fold が encoding 以外で
   動いていないこと — fold code の diff を読む）を確かめてから flag を付けて再実行し、commit する。
6. **その他の digest**: `palw_same_fingerprint_same_verdict.rs` の `CORPUS_VERDICT_DIGEST`（verdict）。`kaspad/src/palw_drill.rs` の T53 の
   salt は入力で pin ではない。
7. **genesis の class id・artifact root・C7**: `class_manifest_const_v1.rs` の 2 test（committed sidecar を JSON として読んだ値と比べる）、
   `t12_regenesis.rs` の `the_held_rows_are_the_fleets_classes`、`params.rs` の `PALW_T12_RCORE_CONSERVATIVE_CLASSES` の literal
   `PALW_T12_2M_CLASS_ID_BYTES`（ADR-0152 C7。tool は `palw_t12_rcore_conservative_classes_v1()` の導出と比べ、lib test
   `consensus_params_id_tests::the_t12_c7_list_is_the_2m_row` が同じことを assert する。t12 の params id の入力なので stage 0）とその doc の
   略記。genesis class の並びが [8k, 2M] でなくなったら `CHECK`（node の表と `t12_regenesis` は手で）。committed sidecar を差し替えたら
   kit の `MANIFEST_8K_SHA256`・`ART_8K_BYTES` も drift する（MOVE）。`ART_8K_SHA256` は host 上の artifact file の sha で、この tree からは
   計算できない: sidecar が動いたときは artifact を作った host で取り直す。
8. **premine の index 配置**: `MAIN_PREMINE_INDEX + 1 = FEE_FLOAT_BASE`（copy）、card の宣言 index が 0..7 の順（`CHECK`）。
   `t12_regenesis.rs` の `t12_bond_collateral_matches_the_card`・`the_fleets_premine_indices_are_unchanged_on_t12s_own_txid`・
   `t12_genesis_mints_exactly_the_cap` と `t12_deploy_kit_constants`（`--apply` の後に走る）。
9. **kit と explorer の写し**: 上の `copy`。node は起動時にこれらを確かめない。`t12_deploy_kit_constants` がこの build の genesis と突き合わせ
   （`cargo test --locked -p kaspa-consensus-core --test t12_deploy_kit_constants -- --nocapture` が `LAYOUT card N …` を印字）、
   `probe-identity-local.sh --build --layout` が binary の genesis の class と bond、committed sidecar の sha も比べる（exit 0）。
   公開時の gate: `install-<host>.sh switch` は node ごとに bond 8 本が `PREMINE_TXID:0..7`、`CLASS_8K` と `CLASS_2M_PREFIX` の class が
   登録済みであることを RPC で確かめ、違えばその node を止める（`lib.sh` `wait_genesis`、`t12check.py --expect-*`）。

**tool の外に残るもの**（手で。tool は数えない）:

- 手で導出した数値の pin: t12 の pruning depth（`T12_PRUNING_DEPTH = 74_920`、`palw_class_verify_deadline_is_t12_only` の lattice）、
  withdrawal delay `12_900`、twin の前提 `12_002` など。本番値の変更が窓を動かせばこれらも動く（その merge が導出と一緒に直す）。twin の
  前提が落ちると tool は twin を `NOT COMPUTED` と出す。
- `misaka-palw-derive` の transformer id と free-prompt の golden（`scripts/check-repin-enumeration.py` の MOVES）: derive の tree が
  動いたときだけ動く。battery（§1 item 2）が拾う。`misaka-palw-base0` の context vector が pin する devnet の params id は devnet で、動かない。
- resource profile（§4 step 6 の `t12_role_memory_figures`）、release binary の sha256 と `fleet.env`（§4 step 10〜11）。

## 6. 公開後（ADR §8.4）

観測（B が analyzer と報告、A が review）。drill の snapshot/analyzer は公開 t12 にもそのまま当たる（`misaka palw vesting --json`）。

| # | 観測 | 合格条件 |
|---|---|---|
| O-1 | licence の頻度 | 最初の Final から 3,000 DAA 以内に licence ≥ 30（`scripts/misaka-palw-t12-rcore-analyze.py` の report 3） |
| O-2 | f・p・door histogram、genesis seat の coverage、8k の throughput、S2 の redraw 率、operator ごとの panel 参加 vs stake、dead seat の率、SW-A5 の閾値、`InsufficientEligibleStake` の数、`ready_eff` | 週次に報告。§3.7 のモデル行を置き換える |
| O-3 | 敵対 producer に対する `q_f`（naive / garbage / borrowed） | 全 attempt が窓内で課金。M6 の条件は全 attempt が **帰責** される |
| O-4 | Final 後の有罪が maturity 前に行を burn | 行が burn、最初の committer に支払い |
| O-5 | 最初の自然 maturity | latch → move → mint。mempool は mint+599 で拒否、mint+600 で使える。第 2 時計の深さを実測から調整。**これが通るまで t12 は価値を持たない** |
| O-6 | FP だけが忙しい区間 | FP だけで止まった区間の数 |
| O-7 | 試験 producer の withholding による DA default。あわせて **正直な producer の DA 応答が ledger で飢えていないか**（PLAN §2.5 R-2b） | 期限（`W_disclose` = `window_challenge` = 1,200 DAA。名目 120 s/DAA で 40 時間、実測 ~200 s/DAA で約 2.8 日）で S1 ＋ strike、最初の告発者に報酬。fleet の producer（ibm b0・b1、.113 b6）の自分の claim への `da-answer` が ledger に拒否され続けた時間が期限の半分（600 DAA）を超えない |
| O-8 | heartbeat だけの区間で有罪が運ばれる | heartbeat block で fold |
| O-9 | 行の maturity 中の market 負荷 | queue ≤ 1,032、maturity 中は 1 block ≥ 1 行、market に 2 枠 |
| O-10 | retirement の引き出し完了 | B-3 の上限内 |
| O-11 | 2M が公開時点で閉じている | 2M の attempt と FP claim は flag day まで全部 `ClassDeadlineUnmeasured` で拒否（4-quater の前は T20 のとおり） |
| O-12 | vesting backlog の中の新 node の IBD | root と次の coinbase が archival node と一致 |
| O-13 | SW-8 の anchor lane | anchor block ごとの `BindTimeout` の数、窓の backstop での void、attempt block の比率と heartbeat だけの最長区間 |

**後回しにした drill**（公開後、`contrib/t12-drill-kit`。公開 node の無いホストでだけ）: SEAT-0 の混在版 live drill、deadline 設計の
M5 / M5b / M9 / M12、8k の実 weight timing drill（§3.9、§8.3 item 7）、short drill D-1〜D-10、V1 fallback での 8k の licence 率と replay 時間、
O-13 の計数。**X10（M6）**: `palw_rcore_attributed_charging` は M1〜M5 と O-3 の帰責条件の後に、flag day を跨ぐ drill（出荷 binary）を経て公開の flag day で武装。

失敗した O 項目は公開 t12 上の fence で直す（行・session・報酬は state として持ち越される）。regenesis は構造的な欠陥（root や encoding の誤り）に限る。

---

## 7. 運用者・監査に残っている判断

- **運用者**: `HB_ADDR` の確認（card 0 の payout address を全桁で入れた。旧 kit の `qf6hf5v0…` と同じ。PLAN R3）／公開後の drill ホスト 4 台以上（PLAN R4）／
  5.104 b2 を seat のみにしたこと（kit の既定。8k producer は ibm b0 の 1 本。PLAN §2.3）の了承／MemoryMax を crash guard として上げた値
  （b0 20G・b1 16G・b6 17G・5.104 の seat 9G）で残すか、cache の蓄積で cgroup 項が share を下回ったら `-`（MemoryMax=infinity）にするか
  （PLAN §2.3・§10 Q4 の再確認）／ibm・.113 の share 引き上げ（PLAN §2）の承認／explorer の配線を `deploy.sh` と `explorer-apply` のどちらにするか
  （DEPLOY.md §9。両者は互いを拒否する）／公開 node の停止時間帯・告知・DNS validator と faucet の資金。
- **実装（公開前に推奨）**: 自分の claim の DA duty が保留中は seat の 2 本目の replay を始めない、または DA 応答に ledger の予約枠を先取りさせる
  （PLAN §2.5 R-2b。`kaspad/src/palw_panel.rs` の `PalwSeatReplaysV1::IN_FLIGHT = 2` と `reserve_replay_v1("da-answer")`）。入らなければ既知リスク
  として公開し O-7 で監視する。
- **監査**: 所在不明の test 番号（T02b、T06、T28、T31、T57、T62、T90、T54c〜T54g、T55）の所在か追加／4-quater と A-held の review と merge／
  `fix/t12-live-seat0` が公開 line に要るか／t11 の休眠 parity の digest が動いた場合の確認。
