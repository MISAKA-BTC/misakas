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
T54a/b/c・T54d〜T54g・T55 は所在不明または P2-8（`rcore/p2-file`、未 merge）に依存。D-4 と D-5 は P2-7 と P2-8 が要る。

### item 6 — SEAT-R は F2 の fence と同じ binary（T18p-M GREEN）、SEAT-S1〜S4 と `PalwDrillFaultV1` も

a0af3c92: `palw_seat_r_in_force_v1`（`kaspad/src/palw_panel.rs`、test `seat_r_is_in_force_on_testnet_12_from_genesis_and_nowhere_else`）、
`PalwDrillFaultV1`（`consensus/core/src/palw_backend.rs`、`misaka-palw-base0/src/{backend,produce}.rs`）、T18p-M（`seat_material_duty.rs`）、
SEAT-S1/S2/S4（`fix/t12-seat-s1s2s4` は merge 済み、`misaka-palw-base0/tests/seat_s1_whole_job.rs`・`seat_s2_output_root.rs`・`seat_s4_*.rs`）、
SEAT-0（`fix/t12-seat0-launch` は merge 済み。`fix/t12-live-seat0` cec601e7 は live 用で未 merge — 公開 line には不要か監査が確認）。
添付: 上の test の PASS と、`AnyValid` の site（contradiction 9 と 13）が SEAT-S1・S3・S4 と T18p-M の GREEN の上で立っていること（Q-6。
さもなければ build の前に `Whole` へ）。

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
| 9-3 | producer の V2 DA responder が `palw_rcore_plus` を武装する全 build にある（honest producer が告発に答え、課金されない processor e2e） | P2-7 merge 済み（7c2850b3、`palw_disclosure_duties_v1` が自分の claim と lock が覆う claim の session を列挙） | processor の e2e test（正直な producer が答えて課金されない）の名前と PASS。監査の M3 review F2（CRITICAL）が閉じたことの確認 |
| 9-4 | `PALW_RCORE_SEAT_DA_ANSWER_LANDED_V1` は seat の DA 自動応答（P2-7）と同じ commit でだけ `true` | `true`（`consensus/core/src/palw_da_rcore_v1.rs:67`）、P2-7 と同じ line | flag の値と、covering signer の `MaterialDisclosedV2` の test |
| 9-5 | `misaka-palw-derive` と stranger script は network の attempt rule（t12 は `CoreV1`）で `output_root` を再計算。FP worker も | あり（`a4682a8d`、`d675423d`） | `misaka-palw-derive` の test と FP worker の rule の test の PASS |
| 9-6 | `PALW_RCORE_VESTING_ROWS_LANDED_V1 = true` と S-4 の funnel（kaspad は false なら起動しない） | `true`（`consensus/core/src/palw_state_v2.rs:2846`）、S-4 merge 済み | flag の値と `rcore_s4_conviction_funnel.rs` の PASS |
| 9-7 | 4-quater の consensus 半分と P-1（`palw_class_verify_deadline`、pruning depth ≈ 74,920 DAA、D_cap 16,000）が genesis params にある — 2M は規則で閉じる | **無い**（`feat/t12-class-verify-deadline` 未 merge）。pruning depth は regenesis でしか決められない | 出荷 commit の t12 params の値（`probe-identity-local.sh` の fingerprint が動くこと）と O-11 の拒否 test |
| 9-8 | A-held と shard court（`feat/t12-aheld`、8be0f661 の上）が監査の review 後に merge、B の routing・object 57 の自動応答・N4 | **無い** | 監査 review の結論、merge commit、item 7 の test |
| — | 公開条件ではない（IA-13）: SEAT-S4 の forged-sibling residual（2M の flag day 項目） | — | — |

---

## 3. a0af3c92 での identity（**暫定。出荷値ではない**）

2026-09-25、Mac、`contrib/t12-deploy-kit/probe-identity-local.sh`（dev build の kaspad、sha256 `2a7dd6e4…`、隔離 node）で読んだ値。
§1 item 7・§2 9-7/9-8 の merge（deadline＋P-1、A-held、Activation Pool）と P2-8 がこれを動かす。

```
EXPECT_FP=8a4810231e4f54e7d62b57ef0c4bc7973b23ebb87b1943b6541c05190b2d5ee6
EXPECT_GENESIS=a27f8f44fe4d91a5bed940be9dbd6d260ccb95cc00d948b1c08ddb6bd1a5f02542a6cf35c7a4d959ba4863ac1557861671763e5cc22937c697870283a8ca1f23
PREMINE_TXID=5e0d5f1b37a71288cc0eb24acc10d2f4973dd3475569f274f03cc64a2233d035099d386e24c91d48427c30a895664dea979abedc90a7788fad170379e55e2669
# schedule id 8b0ee13cd93ed0ca532466ec907fe58c655e9be32e8d532f3cc46c5077e284c8（fence schedule "1000"）
# rule manifest digest 9def81a1c56c02d5d1f9d24c5ddbe78d6f7598b31928ab6f388a2a074f14b4d4d4cf8c2245666bc13b9c91688c106efe428e8643075b68b40c4820adb4201d8d
```

fingerprint `8a481023…` は EVM bridge ledger（fad522c6、int-3 への merge dee8b595）の時点でメモリに記録された t12 の値と同じ（その後の merge は t12 の params を動かしていない）。genesis は c8652a97 の候補
`a27f8f44…` のまま（R-core+ の genesis 行はまだ動いていない）。drill salt `5353…53`（例示用）の drill genesis は
`1678f353…70349`（`--drill-salt` の出力。実際の drill の salt ではない）。

---

## 4. 順序（出荷 commit から公開後まで）

| step | 何を | 誰 | 確認 |
|---|---|---|---|
| 1 | **deadline ＋ P-1**（監査: `feat/t12-class-verify-deadline`、4-quater の consensus 半分、pruning depth ≈ 74,920）を統合線に merge。監査の review 後 | 監査 → 実装 | |
| 2 | **A-held の node 半分**（`feat/t12-aheld` ＋ `feat/t12-aheld-node`、8be0f661 を含む）を merge。review 後 | 監査 → 実装 | |
| 3 | **Activation Pool**（`feat/t12-activation-pool`）を merge（genesis の行を動かし得る） | 実装 | |
| 4 | **B の lane**（`rcore/p2-file` の P2-8 filer 3 本、`rcore/m5b-tests`、必要なら `rcore/readiness-escalation`）を merge。merge は手で解いたら push 前に build（メモリ則） | 実装 | |
| 5 | §1・§2 の各 test を出荷候補で確認し、所在不明の番号を監査と詰める | 実装・監査 | |
| 6 | **再 pin**（§5）。t12 の identity を読み直し（`probe-identity-local.sh --build`）、resource profile を再評価して PLAN §2 の表を直す（`misaka-palw-base0` の profile の scratch test、PLAN §2） | 実装 | |
| 7 | genesis が a27f8f44 から動いたら、a27f8f44 を `fleet.env.example` の `FORBIDDEN_GENESIS` に足し、docs（`docs/testnet12-join-mining.md`、`docs/testnet-12-regenesis-2026-09-23.md`）と explorer の `PANEL_BOND_TX` を直す | 実装 | |
| 8 | **最終 battery を 2 回**（§1 item 2 のコマンド、既定と `--features evm`）。同じ commit で core・kaspad・base0・cli も | 実装 | |
| 9 | 出荷 commit を origin に push | ユーザー | **【確認】** |
| 10 | **fleet の build host（5.104）で release build**: `build-release-5104.sh <commit>`。IDENTITY が step 6 の値と一致 | ユーザー | **【確認】**（公開予定ホストでの 20 分超のビルド） |
| 11 | `fleet.env` を埋める（`REV`・sha256 3 本・`EXPECT_FP`・`EXPECT_GENESIS`・`PREMINE_TXID`・`HB_ADDR`）→ `distribute-from-mac.sh kit / binaries / artifact` → 各 host で `preflight` → `stage` | ユーザー | **【確認】**（リモートへの書き込み） |
| 12 | **ユーザーの最終確認**: 公開 node の停止・置換の時間帯、告知の文面、DNS validator と faucet の資金 | ユーザー | **【確認】** |
| 13 | **配備**: ibm → .113 → 5.104 の順に `install-<host>.sh switch`（公開 node を止めて置き換える） | ユーザー | **【確認】**（host ごと） |
| 14 | **explorer**: `contrib/misakascan-t12/DEPLOY.md` §9 の 1 系統（推奨 `deploy.sh`）で nginx・filler・DB・app.js | ユーザー | **【確認】** |
| 15 | **seeder**: 触らない（KEEP）。`seeders/40-verify.sh` と `60-join-check.sh` | ユーザー | **【確認】**（60 は 5.104 で使い捨て node） |
| 16 | `check-fleet.sh`、`CHECK_REGISTRY=1 check-fleet.sh`（PLAN §5 の合格条件） | ユーザー | |
| 17 | **告知**（O-5 が合格するまで t12 は価値を持たない、と書く） | ユーザー | **【確認】** |
| 18 | 公開後: DNS validator 6 × 20M MSK と faucet の資金を premine の主ウォレットから（運用者の鍵）。validator 2 本の t12 化（PLAN §10 Q7） | 運用者 | **【確認】**（資金移動はユーザーが行う） |
| 19 | 公開後の観測 O-1〜O-13（§6）と後回しの drill（`contrib/t12-drill-kit`） | B（A が review） | drill ホストは運用者が用意（**【確認】**） |

---

## 5. 再 pin の手順（出荷 commit で一度だけ。step 6）

pin を動かす前に、何が動いたのかを 1 つずつ言えること（「両側を残す機械的 merge」や「default hash は全てと一致する」の事故則）。
値は **計算して貼る**（手で打たない）。

1. **t12 の identity**
   - `contrib/t12-deploy-kit/probe-identity-local.sh --build` → `EXPECT_FP`・`EXPECT_GENESIS`・`PREMINE_TXID`・schedule id・rule manifest digest。
   - genesis が動いた場合: `cargo test -p kaspa-consensus-core --lib config::genesis::tests::gen_kaspa_pq_genesis_hashes -- --nocapture` と
     `config::premine::tests::print_premine_commitment` の出力を `consensus/core/src/config/genesis.rs` の `PALW_T12_GENESIS`（`hash`・
     `hash_merkle_root`・`utxo_commitment`）へ。`test_genesis_hashes` と `every_genesis_commits_to_the_premine_this_build_mints`（`genesis.rs` の doc が旧名 `every_shipped_genesis_commits_to_its_own_premine` で呼んでいる test）が確認する。
     `consensus/src/consensus/utxo_set_override.rs` の `print_repinned_t12_genesis` も同じ値を出す。
   - premine の salt が動いた場合のみ: `consensus/core/src/config/premine.rs` の `TESTNET12_COMMUNITY_TXID`。
2. **t12 の fingerprint を名指しで pin している test**（`git grep -n -E '"[0-9a-f]{64}"' -- 'consensus/core/tests/*.rs'` で全数を確認）
   - `consensus/core/tests/palw_offence_attribution_is_t12_only.rs` — `T12_BEFORE_THE_ATTRIBUTION`（fence を外した t12 の params / identity /
     schedule id）。t12 の他の params が動くと動く。
   - `consensus/core/tests/evm_bridge_ledger_is_t12_only.rs` — `T12_AT_THE_PARENT_WITHOUT_THE_ATTRIBUTION`（ledger と attribution を外した t12）。
   - これらは「その fence だけが t12 を動かした」ことの pin。新しい merge が t12 を動かしたら、何が動かしたかを書いて値を更新する。
3. **t11 / devnet / mainnet が動いていないことの pin**（動いたら出荷しない。動かすべき理由があるときだけ更新）
   - `consensus/core/src/config/params.rs` の `shipped_presets_have_pinned_fingerprints`（mainnet `badaa8e9…`、testnet、testnet-11 `bd633ce9…`、
     simnet、devnet `7a27f341…`）と `the_pinned_testnet_fingerprint_is_the_one_a_node_announces`。
   - `consensus/core/tests/palw_rcore_plus_is_t12_only.rs`（`AT_V21`・`AT_V22`）、`palw_clock_floor_is_t12_only.rs`、
     `palw_offence_attribution_is_t12_only.rs`（t11 / devnet / mainnet の 3 つ組）、`palw_the_release_did_not_move.rs`（`T11_CONSENSUS_*`、
     `MAINNET_CONSENSUS_PARAMS_ID`）。
4. **T41 の v22 golden**
   - `consensus/core/tests/rcore_m5_v22_golden.rs`: `t41_the_v22_golden_vectors_empty_and_inhabited`（空と全 map 充填の 2 値）と
     `t41_the_v22_record_encodings_are_pinned`（`PalwClaimRcoreV1`〜`PalwConsumedOffenceV1` の 11 個の encoding digest）。
   - `consensus/core/src/palw_state_v2.rs` の `the_version_22_state_root_golden_vectors`（`want_empty` / `want_full`）。
   - v22 は一度だけ切る（v23 は作らない、handoff §3）。record の encoding が動いたなら、どの field がなぜ動いたかを test の doc に書く。
5. **t11 の休眠 parity**: `consensus/core/tests/panel_room_t11_dormant_parity.rs` の `PARENT_DUMP_BLAKE2B_256`。t11 の fold の dump が
   動いたら、その理由（R-core+ は t11 で休眠のはず）を監査が確認してから更新する。
6. **その他の digest**: `consensus/core/tests/palw_same_fingerprint_same_verdict.rs` の `CORPUS_VERDICT_DIGEST`（verdict が動いたときだけ）、
   `consensus/core/src/config/params.rs` の `PALW_T12_RCORE_CONSERVATIVE_CLASSES`（2M の class id。graph-v7 profile が動いたときだけ、
   `the_t12_c7_list_is_the_2m_row` が確認）、`consensus/core/tests/t12_regenesis.rs` の genesis 系 test、`kaspad/src/palw_drill.rs` の T53。
7. **文書と kit**: `docs/testnet12-join-mining.md`・`docs/testnet-12-regenesis-2026-09-23.md`（genesis `a27f8f44…`、premine `5e0d5f1b…`）、
   `contrib/t12-deploy-kit/PLAN.md` §4 の暫定値、`contrib/misakascan-t12/app.js` の `PANEL_BOND_TX`、この文書の §3。
8. 再 pin の commit の後に step 8 の battery を 2 回。

---

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
| O-7 | 試験 producer の withholding による DA default | 期限で S1 ＋ strike、最初の告発者に報酬（PLAN §2 R-2 の確認も兼ねる） |
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

- **運用者**: `HB_ADDR` の全桁（PLAN R3）／公開後の drill ホスト 4 台以上（PLAN R4）／5.104 b2 の 8k producer を残すか（PLAN §2 R-2、推奨は ibm b0 の 1 本に）／
  ibm・.113 の share 引き上げ（PLAN §2）の承認／explorer の配線を `deploy.sh` と `explorer-apply` のどちらにするか（DEPLOY.md §9）／
  公開 node の停止時間帯・告知・DNS validator と faucet の資金。
- **監査**: 所在不明の test 番号（T02b、T06、T28、T31、T57、T62、T90、T54d〜T54g、T55）の所在か追加／4-quater と A-held の review と merge／
  `fix/t12-live-seat0` が公開 line に要るか／t11 の休眠 parity の digest が動いた場合の確認／IA-14 9-3 の processor e2e の名前。
