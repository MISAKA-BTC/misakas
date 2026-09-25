# testnet-12 R-core+ 公開チェックリスト（ADR-0152 §8.3 の launch gate と IA-14 の ship 条件）

作成: 2026-09-25。基準は統合線 `rcore/int-3` @ `a0af3c92`（このファイルは branch `rcore/release-prep`）。
**更新（2026-09-25、lane 3「gate の証拠」、branch `rcore/gate-evidence`、base `rcore/int-3` @ `8270cf03`）**: §1 item 1 と item 5 の
test の所在を全番号の表（§1a）に置き換え、欠けていた test を足し（§1b）、§2 の IA-14 を 8270cf03 で読み直した。
**更新 2（同日、lane 3 の review 対応）**: gate の item ごとの判定（§1.0）を足した — **gate は未達**。T06 と T62 を書き（§1b）、
T37・T66・T18m を「一部」に直し、監査 branch の snapshot を local の sha で取り直し（§1a、§5 の identity を動かすもの）、
§3 の値を貼れない形にし、§4 の順序を 8270cf03 の状態に合わせた。
**更新 3（同日 15:10 頃）**: `rcore/int-3` が `f7350af91`（15:01、A-held node `2f92228f` の merge — `1ee0e08d4` と shard court の
`8be0f661` を含む）に進んだので、本 lane に merge し（`rcore/gate-evidence` の merge commit）、本 lane の test を merge 後の tree で回し直した
（§1b）。item 7・8・9-8 の「merge」の条件は満たされた — 判定は §1.0。
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

### 1.0 gate の判定（item ごと）— **gate は未達。公開できる状態ではない**

`rcore/gate-evidence`（base `rcore/int-3` @ `8270cf03`、本 lane の test、その後 `rcore/int-3` @ `f7350af91` を merge）で 2026-09-25 に判定。**満** = 規則と test が揃い、
残りは出荷 commit で証拠を取るだけ。**未** = 規則・test・merge のどれかが欠けている（欠けているものを右に書く）。

| item | 判定 | 何が欠けているか（blocking） |
|---|---|---|
| 1 — M1〜M5 GREEN | **未** | **規則欠 T57**（SR-9: S-5 の object 56 `PanelUnavailableQuorum` が無く、fold は名指しで拒否する）— **ユーザーの判断が要る**: S-5 を公開前に入れるか、RT#2（S0′）を唯一の課金として出し gate（§7.1 S の done-when と §8.3 item 1）の文言を直すか。**一部の cell**（§1a）: T16/T82 の V3S-02 条項、T42 の引き出し完了の端から端、T26 の fence 未満 F21 twin（公開 t12 では到達不能）、T18p の kaspad 層、T37 の「3 本の genesis retirement」「`NoCapablePanel`」の halt、T66 の「3 round で F + 4,000」の S4 経路、T54e の node e2e、T18m と T39 の 2M cell（2M の flag day の gate に移すなら、その waiver を ADR の amendment として記録する — §7）。T06 と T62 は本 lane が書いた（§1b） |
| 2 — battery を 2 回 | **未** | 出荷 commit で取る（step 1 の merge と §5 の再 pin の後） |
| 3 — short drill | 公開後へ移動 | 運用者決定 2026-09-24（公開の gate ではない） |
| 4 — T41 golden | **未** | §5 の再 pin を出荷 commit で一度（readiness-horizon と Pool の merge の後） |
| 5 — Phase 2 の要（C12） | 8270cf03 で test は全部 **有** | 出荷 commit の battery で GREEN を取る（T50 は evm 側） |
| 6 — SEAT-R 等 | code と test は 8270cf03 に **有** | 出荷 commit で PASS を取る |
| 7 — held-class の穴 | code は **有**（int-3 `f7350af91` で merge）— 証拠は未 | A-held（`2f92228f`、`1ee0e08d4`、object 57・N3/N4・F3 (B)・F5）と `8be0f661` は `f7350af91` の祖先。2M を閉じる 4-quater も満（`a66509f9`）。**出荷 commit で取る**: A-held の T-A1〜T-A11（`palw_state_v2.rs` の `t_a1_…`・`t_a2_…`・`t_a6_…`・`t_a7_…`・`t_a11_…`、kaspad `palw_panel/held_court_e2e.rs` の `t_a9_…`・`t_a10_…`、`held_court.rs` の `t_a10_…`）と T-D2 の PASS（kaspad の test は本 lane では回していない）、8k の実 weight timing drill は公開後（IA-12） |
| 8 — launch line の修正 | code は **有**（`8be0f661` は `f7350af91` の祖先） | 出荷 commit で J-8 の test の PASS を取る |
| 9 — IA-14 | 9-1〜9-7 は満、9-8 は merge 済み | 9-8 の「監査の review 後」— A-held line の review（4 回目 `1ee0e08d4` まで）の結論を添付する |

### item 1 — M1〜M5 GREEN（§7.1）。F1-M・F1c・stake 加重の抽選・addendum の T18q〜T18y・T18p-M・SR-10・U2/U3 を含む

添付: §1a の各 test が出荷 commit で全部 PASS した battery のログ（item 2 の 2 本と、同じ commit の core・kaspad・base0・cli）。
**番号ごとの所在は §1a**（8270cf03 で読み直した全番号の表。a0af3c92 時点のこの節の表を置き換えた）。8270cf03 ＋ 本 lane で:

* **有**: M1〜M5 の番号のほぼ全部。a0af3c92 で所在不明だった T02b・T28・T90 と T18m の kind 3／Final 後の cell、T37 の F18、T02 の
  `NoCapablePanel` 0、**T06（実 stake 抽選の EV grid）と T62（J-6 の一意の経路）** は本 lane で書いた（§1b）。T31・T57 の扱いは下。
* **規則欠（gap、production code は書いていない）**: **T57**（SR-9: S-5 の object 56 `PanelUnavailableQuorum` が int-3 に無く、fold は
  名指しで拒否する）。安全側（RT#2 と `NotReplayBacked` が同じ S0′ を課す）だが、D-8 の SR-9 と §3.7 の SR-9 付きの H_f は成り立たない。
  **ユーザーの判断**: S-5 を公開前に入れる（S の done-when が要求する形）か、RT#2 を唯一の課金として出し gate の文言を直すか。
* **一部（欠けた cell、owner 付きで監査へ）**: T16/T82 の V3S-02 条項（session が開いた head 行の後ろの行が動き続ける。A/B）、
  T42 の引き出し完了の端から端（A）、T26 の fence 未満の F21 twin（A。公開 t12 には届かない）、T18p の kaspad 層の partial seat（B）、
  T37 の「3 本の genesis retirement」と「`NoCapablePanel`」で halt に至る場面（A）、T66 の「3 round で F + 4,000」の S4 経路（A、M3）、
  T54e の devnet preset の node e2e（B、公開後の drill でも可）。
* **gate の変更が要るもの（N/A の cell）**: T31 の fence 未満の半分と T04 の ConflictingPermit 1 share（R-core+ は attribution を前提に
  要求し、S4′ の `ShareBurned` に writer が無い — ADR の文言を直す）、**T18m と T39 の 2M cell**（2M は公開時に閉じている、U-D1。
  T02b は `t12_2m_open` で 2M 行を動かせるので前提はあるが、2M 幅の producer が test に無い）— 2M の flag day の gate に移すことを
  ADR の amendment として記録し、ユーザーか監査が了承する（§7）。了承までは item 1 の「一部」。

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
**所在は §1a の「item 5」表**（8270cf03 で全部 **有**。T50 は `--features evm` の battery でだけ build される）。T54a〜T54f は processor の
半分が有（node e2e は公開後）、**T54g は監査 branch `rcore/p2-8e` にだけある**（P2-8e の merge で入る）、T55 は ledger の列が無く規則欠
（item 5 の範囲外）。D-4 と D-5 は P2-7 と P2-8 が要る（どちらも 8270cf03 に merge 済み）。

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
**8270cf03（lane 3 が確認）**: 4-quater は merge 済み（a66509f9 が a4323997 を入れる。`core/tests/t12_class_verify_deadline.rs::td2_the_2m_row_is_refused_at_launch_attempt_and_free_prompt`）。
A-held（local `feat/t12-aheld-node` @ `2f92228fc` = 修正 `1ee0e08d4` ＋ int-3 8270cf03 の merge、14:36）と 8be0f661 は **まだ祖先ではない**
（`git merge-base --is-ancestor` で確認）— 8k の半分は未達。
添付: A-held の C1〜C5 と object 57 の test（8k fixture で attention の嘘が有罪になる、kaspad producer が期限内に object 57 で答える）、
2M の attempt と FP claim が `ClassDeadlineUnmeasured` で拒否される test（O-11 の T-D2）。

### item 8 — launch line の修正が出荷 commit にある

a0af3c92: licence-stall（a4dfe903、d94d3a1b）あり、panel-room の C7 re-key（e93be0f2、f8c91f19、
`consensus/core/tests/panel_room_short_class_is_released_at_licence.rs`、T20、T21）あり、**shard court の one-move rule（8be0f661、J-8）は無い**（A-held と一緒に入る）。
8270cf03 でも同じ（a4dfe903・d94d3a1b・e93be0f2・f8c91f19 は祖先、8be0f661 は祖先ではない）。

### item 9 — ship 条件（IA-14）: §2

### 1a. test の所在表（§8.3 item 1 と item 5 の全番号）— `rcore/int-3` @ `8270cf03` ＋ 本 lane

作成: 2026-09-25、lane 3（branch `rcore/gate-evidence`、base `rcore/int-3` @ `8270cf032`）。ADR は v3.1（post-edits と
integration amendments 込み）の §7.1（M1〜M5 の "done when"）と §8.1（test 表）。各番号を、test 名の接頭辞（`fn tNN_…`）と
test の doc の番号の言及で機械的に拾い（`#[test]` / `#[tokio::test]` の直前の doc と fn 名）、各行の条項を本文で読み合わせた。
監査の未 merge branch も同じ方法で `git show` から読んだ。**local の branch の sha**（remotes/macbook ではない。2026-09-25 14:40 頃に
読み直した — これらは今も動いているので、step 1 の merge の後に出荷候補の上でもう一度読む）:

* `feat/t12-aheld-node` @ `2f92228fc`（14:36、int-3 8270cf03 の merge。修正の中身は `1ee0e08d4`、13:11）。
* **15:10 の読み直し**: `rcore/int-3` は `f7350af91`（15:01、A-held node `2f92228f` を merge — 下の A-held の行は int-3 に入った）、
  `feat/t12-activation-pool` は `f92f34a7f`（14:41、P4 の review 修正）、`feat/t12-readiness-horizon` は `a9d9f8320`（15:09: `3e9ae4ba1` で
  horizon を genesis-only の param に — t12 は 24 span、他は None（既定の 8）— し、pool `f92f34a7` と int-3 `f7350af9` を merge して t12 の
  pin を取り直した）。以下は 14:40 の記録。
* `feat/t12-activation-pool` @ `2932bd57d`（14:16）: 前回の `2e5f370e` の後に **P4** `b84afa1a5`（13:32、登録が listing を sponsor、RPC）、
  **P1** `8ccc41e0e`（14:00、(b) の operator cap `b_cap = E × 200‰ / 5`、payee cap 50、`PALW_T12_GENESIS_CLAIM_ESCROW_SOMPI`）、
  **P2** `2932bd57d`（14:16、pool の fence 以降は admission jury を panel の床の bond から引く — T90 の行を参照）。
* `feat/t12-readiness-horizon` @ `694012721`（14:34）: pool ＋ int-3 3692c7e9 の merge（`eb360804`）の上に `99247983a`（merge の
  意味的衝突 1 件の test）、**`f6e970e98`（WIP: readiness-V2 の horizon を新しい t12 param `palw_readiness_v2_max_age_spans`（t12 で 24
  span、`consensus_params_id` に hash）の後ろに置く）**、`694012721`（WIP: その pin を解決 — `palw_offence_attribution_is_t12_only.rs`・
  `evm_bridge_ledger_is_t12_only.rs`・`palw_readiness_horizon_is_t12_only.rs`）。**t12 の identity を動かす**（§5）。
* `rcore/p2-8e` @ `ade4b4581`（12:57、A-held node `a6ab8bf76` の上）。

番号を持つ test を足すのは p2-8e の T54g だけ（下の「参考」）。Pool の P1/P2/P4 と readiness-horizon は番号の外の回帰 test を足す。

**状態の読み方**: **有** = その番号を実装する test が 8270cf03 にある（GREEN かどうかは出荷 commit の battery で取る、item 2）。
**有（本 lane）** = 本 lane で書いた test（commit は §1b）。rule を一時的に壊すと red になることを確かめた。**一部** = 条項の一部
だけ（欠けた条項を書く）。**欠** = rule は int-3 にあるが test が無い（owner と、本 lane で書かなかった理由）。**規則欠** = rule が
int-3 に無い（gap。production code は書いていない）。**N/A** = 構成上起こらない（理由）。path は repo からの相対。
`t46::` = `consensus/src/pipeline/virtual_processor/tests/t46_false_valid_real_claim.rs`、`t47::` = 同 `t47_model_class_attribution.rs`、
`core/tests/` = `consensus/core/tests/`、`vf::` = `consensus/core/src/palw_state_v2/tests/vesting_fold_v1.rs`、
`sv2::` = `consensus/core/src/palw_state_v2.rs` の lib test、`pv2::` = `consensus/core/src/palw_panel_v2.rs` の lib test。

#### M1（F2 = SPEC §3）

| ID | ADR の要求（一行） | test | 状態 |
|---|---|---|---|
| T46a–T46n | 実 producer の claim で F2 の赤・注入 fault の有罪（full と leaf の partial）・forged output（full のみ）・Shape・Final 後・retirement 後・FP・kind 1 拒否・正直・domain 違い・拒否 kind・(seat, claim) 1 件・reorg/restart・open session | `t46::t46a_…`〜`t46n_session_open`、`t46n_r_an_rcore_da_session_does_not_defer_a_conviction`（ほか t46o〜t46u） | 有 |
| （M1 GREEN の付帯） | `palw_offence_attribution_is_t12_only`、`palw_offence_attribution_t11_verdicts` | `core/tests/palw_offence_attribution_is_t12_only.rs`、`core/tests/palw_offence_attribution_t11_verdicts.rs` | 有 |
| T26 | fence 以降、`CourtExecutorGuilty` と `ExecutorEquivocation` の kind 3 は `ContradictionNotAdmitted`。fence 未満では無関係 job の Eq ＋ `CourtExecutorGuilty` を拒否し、root 0 の consumed offence は何も bind しない（F21） | fence 以降: `t46::t46k_refused_kinds`、`palw_offence_attribution_v1.rs` lib（`palw_false_valid_admission_v1` の拒否）。fence 未満: fold の拒否（`sv2` 16868 付近 "CourtExecutorGuilty does not name this claim's executor"）はあるが名指しの test 無し。※ `sv2::t26_t81_a_proven_court_verdict_records_court_conviction…` は名前に T26 を含むが court verdict の test（T81 側） | 一部（F21 の fence 未満 twin が欠。t12 では fence が genesis から武装なので公開 t12 には届かない。owner A） |
| T31 | fence 以降 `ConflictingPermit` は名指しで拒否、kind 1 は `SupersededOnThisNetwork`（S4′ は t12 で発火しない）。fence 未満は v3 の場合（Final のまま、1 share burn、reload） | `t46::t46h_v1_kind_refused_past_fence`、`t46::t46k_refused_kinds`（`ConflictingPermit`）、`sv2::f2_the_fence_supersedes_the_v1_kind_and_arms_the_v2_kind` | 有（fence 以降）。fence 未満の半分は N/A: `palw_rcore_plus` は `palw_offence_attribution` を前提に要求し（`palw_rcore_plus_is_t12_only::validate_refuses_the_fence_without_each_prerequisite` の "attribution"）、S4′ の `PalwVestingNoteV1::ShareBurned` を書く writer は無い。ADR の「fence 未満の v3 の場合」は R-core+ と両立しない（ADR の文言を直す候補） |

#### M2（F1 = SPEC §4、F1-M、F1c、addendum）と Seat fixes

| ID | ADR の要求（一行） | test | 状態 |
|---|---|---|---|
| T18 | P0-10 naive: 素材なしの DA default は S1、告発なしなら RT#2 で没収 | `core/tests/rcore_m3_da_court.rs::t18_da7_a_live_default_is_s1_and_writes_one_da_default_record`、RT#2: `core/tests/rcore_s4_conviction_funnel.rs::t22_s0_prime_the_second_failed_panel_forfeits_the_commitment_and_nothing_else`、filer: `t46_p2_8_reporter_filer.rs::t18_t54c_…` | 有 |
| T18b | 借り root: R と C1 は admit、R の開示は identity で拒否、C1 は `ProducerWithholding`、C2 の kind 4 `IdentityMismatch` は `CourtFraud` | `t46::t18b_a_borrowed_root_answers_another_job`、`t18b_r_a_borrowed_root_answers_no_rcore_session` | 有 |
| T18c(i–iii) | licence 前の kind 4（算術・Shape）、preimage 無しの root は default | `t46::t18c_before_licence_the_executor_is_refuted`、`t18c_ii_a_root_with_no_preimage_defaults`、`t18c_ii_r_…` | 有 |
| T18c(iv) | 正直な step tree 上の garbage logits は `LogitsNotStepOutput` で有罪（F1c、gate 内） | `t46::t18c_before_licence_the_executor_is_refuted`（(iv) の段）、`t46::t46u_…`、`t54f_replay_filer.rs::t54f_f1c_garbage_logits_on_an_honest_step_tree_are_refuted_by_12`、base0 `f1c_logits_not_step_output.rs::t18q_…` | 有 |
| T18d | output grind は `OutputMismatch`、正直な `output_root` は拒否 | `t46::t18d_the_output_grind_is_refuted` | 有 |
| T18e（floor） | relabel は full-context の検査で有罪 | `t46::t18e_the_floor_relabel_is_refuted_by_j5` | 有 |
| T18e（8k、F1-M） | 同、model class で | `t47::t47a_the_model_relabel_is_refuted_by_j5b`（8k 行の family = held A16 graph-v7、n_ctx 128 の fixture と Qwen3.6 v7） | 有 |
| T18e（2M、F1-M） | 同、2M で | `t47::t47e_the_2m_relabel_is_refuted_by_prompt_not_anchored`、`t47f_a_prompt_fault_convicts_every_valid_signer`（13 は `AnyValid`） | 有 |
| T18f / T18g / T18h / T18k | J4 の trace root 不一致 / `job_identity == 0` は有罪にしない / FP pin 不一致と `fp_pin_spellings_agree` / claim 単位の没収は C0 の行を残す | `t46::t18f_…`、`t46::t18g_…`、`palw_offence_attribution_v1.rs::f1_fp_pin_spellings_agree_and_convict_on_the_free_prompt_lane`、`t46::t18k_…` | 有 |
| T18m（F1-M） | `ForgedOutputTiled` が 8k tiled/A16 の実 decode と 2M fixture の forged token を kind 3（full-mask signer）と kind 4 で Final の前後に有罪。正直な tiled decode は有罪にしない | 既存: `t47::t47d_a_models_logits_and_tokens_are_refuted_by_root`（kind 4・licence 前・A16/Qwen3.6）、base0 `f1c_logits_not_step_output.rs::t18r_eleven_convicts_a_forged_tiled_token`。**本 lane**: `t18m_forged_output_tiled.rs::t18m_forged_output_tiled_convicts_the_full_mask_signer_before_final_and_never_an_honest_decode`（kind 3、partial は `SiteNotAttested`、正直な decode は kind 3・4 とも `TokenHolds`）、`::t18m_forged_output_tiled_after_final_reverses_it_by_kind_3_and_by_kind_4`（Final の取り消し、root 没収、vesting 行の burn と S3、kind 3 は seat の S4 も） | **一部**（2M の cell → 2M の flag day の gate）。8k の kind 3・kind 4・Final 前後は本 lane で有。2M の cell は、2M 行が §4-quater（U-D1）で公開時に閉じている（`t12_class_verify_deadline::td2_…`）ため 2M を開ける flag day の gate に移す案 — `t12_2m_open` の前提はある（T02b が 2M 行を動かす）が 2M 幅の producer が test で動かない（`t47e` の注記）。adjudicator は幅を読まない。**移すことは gate の変更**なので ADR の amendment として記録し、ユーザーか監査が了承する（§7） |
| T18p | partial な kaspad seat が借り root の claim を拒否（`AnyValid` の根拠） | base0 `seat_s4_segment_opening.rs::the_floor_authenticates_every_segment_link_and_refuses_each_forgery`（「another claim's opening (a borrowed binding)」→ `NotTheClaimsExecution`）、`seat_s1_whole_job.rs` | 一部（family 層の拒否はある。kaspad の partial seat（`palw_v2_try_partial_resume_v1`）で借り root を拒否する名指しの test は無い。owner B（kaspad）。addendum は `AnyValid` の根拠を T18p-M ＋ SEAT-S1/S3/S4 に移している） |
| T18p-M | 全 drill fault で `Valid` が出ない、replay 不一致は終端、material/interval/capture の arm は `Valid` を出さない、S3 は `verify_material == Matches` の後だけ | base0 `seat_material_duty.rs::t18p_m_no_drill_fault_gets_a_full_valid`、`the_floors_/the_a16_/the_qwen36_seat_gives_no_valid_to_a_drill_fault…`、kaspad `seat_s_tests::c1_s3_never_attests_past_the_fence_without_verify_material`、`q7_…::s3_samples_only_a_capture_seat_s3_verified` | 有 |
| T18q / T18r | 12（行×argmax の曲げ×lane、dense と fold、ragged vocab 8,292）/ 11 `NotSelected` と `OutOfVocab` | base0 `f1c_logits_not_step_output.rs::t18q_twelve_convicts_exactly_the_bent_row_and_tile`、`::t18r_eleven_convicts_a_forged_tiled_token`、chain: `t47::t47d_…` | 有 |
| T18s / T18t | 10 を model attempt と FP model claim に、claim 単位の没収 / J6・J7・J5a | `t47::t47b_the_model_output_rule_convicts_by_claim` / `t47::t47c_the_moved_legs_and_the_non_formula_contexts_are_refuted` | 有 |
| T18u | heavy budget: 1 block に 2 本目の `Whole` 13 は計算前に落ち、block は残り root は同じ | `t47::t18u_a_block_recomputes_one_whole_2m_prompt`、`t18u_holding_the_heavy_slot_costs_what_it_consumes` | 有 |
| T18v / T18w / T18x | court の扉 / admission（非 formula canonical・head 述語・Float32・Kimi は拒否、genesis 行は通る）/ tag 9–13 の V1 parity | `core/tests/palw_court_decode_close_door.rs::t18v_…` / `palw_attempt_rules_v1.rs::t18w_a_registration_is_attributable_or_refused_by_name`、`t18w_testnet_12s_genesis_rows_are_attributable` / `core/tests/palw_offence_attribution_t11_verdicts.rs::t11_attribution_tags_and_kinds_are_refused_as_they_were` | 有 |
| T18y | DA session 下の claim を kind 4 が void、sweep・予約・行が整合、reload | `t46::t46n_session_open`、`t46n_r_an_rcore_da_session_does_not_defer_a_conviction` | 有 |
| T-THREAD | pipeline で mined した block が `job_identity ==` producer の anchor を記録（自分の work と merged blue）、reorg と v22 の reload | `t12_round_lane_e2e.rs::t12_a_claim_records_the_anchor_of_the_header_that_carried_it`、`t46::t18_thread_a_claim_records_the_anchor_of_its_own_header`、`t46::t18_job_identity_survives_reorg_across_admission` | 有 |
| T62 | 一意の経路（J-6）を性質として: 全有罪が claim → root → job → index → signer → fault → target を claim・liability 記録・（R-core+ 以降）vesting 行の写し（N8）で解決、retired claim を含む | **本 lane**: `t62_the_unique_path.rs`（t46 の子）— `::t62_before_final_every_conviction_kind_resolves_one_path`（kind 3 の full seat と leaf の holder、kind 4 の by root と by claim、DA default（DA-7 の S1 と covering signer の S4））、`::t62_after_final_and_retirement_every_source_resolves_the_same_path`（Final 後は claim・liability・vesting 行の 3 源が一致、retired は liability と行、carriage で liability を剪定した fixture では行の写しだけで同じ経路）、`::t62_a_court_default_lands_on_the_resolved_path`。各有罪で resolver の target = admission で commit された経路、kind 3/4 の adjudicator の target = resolver の target、record の claim・accused・kind・root、課金される bond = 告発された signer と resolver の executor だけ。部品は従来どおり `t46f`、`vf`（N8） | 有（本 lane） |
| Tier B golden | `job_for_anchor` under `CoreV1` = `palw_attempt_context_v1`（16 anchor × 3 family ＋ 実 8k/2M/Q36 の profile） | base0 `attempt_rules_core_v1_golden.rs`、`floor_attempt_context_golden.rs` | 有 |
| v22 golden（M2/M5） | v22 の golden vector | `core/tests/rcore_m5_v22_golden.rs::t41_…` ×2、`sv2::the_version_22_state_root_golden_vectors` | 有（再 pin は §5） |

#### S（Phase 1 skeleton。T01–T44 の P1 部分、T02c、T17、T22、T47、T57、T74 V2、T75–T78、T80–T82、T84）

| ID | ADR の要求（一行） | test | 状態 |
|---|---|---|---|
| T01 | staged lifecycle: 全 seat Valid の quorum/coverage licence で `E` 解放、Withheld/Incapable/Sampled/欠/redraw/S2 は保持、C7 は Final まで、8k は floor 同様、Final で `w`、revert/IBD、reload | `core/tests/rcore_s2_staged_reserve.rs::t01_…`、`rcore_m5_restart.rs::t01_revert_and_ibd_twins_of_the_staged_lifecycle`、`::t01_model_class_twins_8k_released_and_held_and_2m_held_to_final`、`rcore_m5_q5_gate.rs::t01_q5_twins_…`、`rcore_s2::u1_a_c7_claim_holds_the_escrow_to_final_on_a_full_service_licence` | 有 |
| T02 | 公開時の課金: 2 回目の `ReceiptTimeout` は S0′（strike・報酬なし）、`BindTimeout`・`NoCapablePanel`・1 回目の RT は 0 | `rcore_s4::t22_s0_prime_…`（RT#1 0、RT#2 S0′）、`pv2::t94_a_refused_draw_binds_nothing_and_the_claim_voids_bind_timeout_without_forfeit`（step 4c の BindTimeout 0）。**本 lane**: `core/tests/t02b_rt2_is_s0_prime_on_every_class.rs::t02_bind_timeout_and_no_capable_panel_charge_nothing`（bind window の backstop で floor は `BindTimeout`、8k 行は `NoCapablePanel`、どちらも bond 不変・strike/記録なし・予約は void で解放） | 有（`NoCapablePanel` 0 は本 lane） |
| T02b | regenesis 時点に X10 は無く、RT#2 は全 class で S0′。fence を跨ぐ twin は M6 | **本 lane**: `core/tests/t02b_rt2_is_s0_prime_on_every_class.rs::t02b_rt2_is_s0_prime_on_the_floor_the_8k_row_and_the_2m_row`（floor・8k・2M（`t12_2m_open`）で RT#1 は 0、RT#2 は commitment ちょうど、strike・記録・seat 課金なし）。FP は `rcore_m5_q5_gate::t40_dl1_q5_a_compute_priced_fp_licence…`（`rr` 込みで RT#2 と同額） | 有（本 lane、regenesis の半分）。twin は M6 |
| T02c | FP の abandon hold 600 DAA、load で再導出、hold 中の restart | `core/tests/dos_repro_2_…::t02c_the_free_prompt_abandon_hold_is_the_commitment_to_its_boundary_and_reloads_mid_hold`、`rcore_m5_restart::t40_dl1_restart_mid_abandon_hold` | 有 |
| T03 | 行の作成と正確な移動、counter、coinbase の恒等式 | `vf::t03_…` ×2、`p2_mint_path.rs::p2_t03_the_coinbase_identity_closes_over_real_finals_a_real_conviction_and_real_mints`、`::p2_t58_t03_…` | 有 |
| T04 | burn: 実行を証明する有罪で行を削除、ConflictingPermit は 1 share、行と lock の述語が F+2,999 / F+3,000 で一致 | `vf::t04_a_row_burns_until_it_moves_and_never_after`、述語の一致: `vf::t12_the_maturity_clocks_and_the_halt`（`palw_vesting_lock_is_live_v1 == is_live_v3` の grid） | 有（ConflictingPermit の 1 share は T31 と同じ理由で N/A） |
| T05 | state 層と UTXO 層で抜け道なし | `p2_b3_vesting_payee_gate.rs::p2_t23_every_utxo_site_holds_a_vesting_payee_exactly_while_v4a_does`、`p2_mint_path.rs::p2_t25_t05_a_leg_is_no_utxo_until_minted_and_then_obeys_decision_a_alone` | 有 |
| T07 | reorg fuzz: bond ごとに Σ回収可能 ≥ Σ抽出可能、1 有罪の 2 写しで reporter が違う twin（F17） | `core/tests/rcore_m5_reorg.rs::t07_…` ×4（`t07_f17_…` を含む） | 有 |
| T08 | capacity は p と f の関数、fold = processor = node が sompi まで一致 | `rcore_s3_one_ledger.rs::t08_fold_admission_producer_facts_and_draw_read_one_committed_number`、kaspad `palw_producer_t12_tests.rs`（2 本）、`kaspad/src/palw_producer.rs::the_nodes_decision_is_the_chains_refusal_on_the_same_state` | 有 |
| T09 | (seat, claim) ごとに `max(duty, lock)`、`dos_l5_4b` | `core/tests/dos_l5_4_reorg_fuzz.rs::dos_l5_4b_one_ledger_backs_every_lock_and_duty` | 有 |
| T11 / T12 | 行は claim の retirement より長生き / 同 block の有罪と maturity は burn が先 | `vf::t11_a_row_outlives_claim_retirement` / `vf::t12_a_same_block_conviction_burns_before_maturity_moves`、`p2_t29_conviction_and_maturity.rs::p2_t29_…` | 有 |
| T13 / T14 / T15 | buyback の除外と価格、FP の権利は `G_res` / staging は単調、`escrow_released` は戻らない / lock は再計数した k で価格、flat-/3 の反例 | `vf::t13_…` ×2、`vf::t03_t13_…`、`rcore_s3::t13_…` / `rcore_s2::t01_…`（"T14"）、`rcore_s2::t74_sr1b_…` / `rcore_s3::t15_every_door_locks_l1s_price_with_the_attested_mask` | 有 |
| T16 | V-7 の予算は新 key で数え market 予約付き、止まり飛ばさない、運ばれた行も burn 可、**head 行に 1,200 DAA 開いた session が後の latch 済み行を止めない（V3S-02）** | `vf::t16_the_budget_is_new_keys_with_the_market_reserve_and_stops_never_skips`、`vf::t04_…`（運ばれた行の burn） | 一部（V3S-02 の条項の test が欠。DA-5 の re-key（`da_rekey_v1`）は `rcore_m3::t66_…` にあるが「後の行が move し続ける」は未検証。owner A/B） |
| T17 | 床: 129,999.99 MSK は抽選されず、13,000 は登録可、U2 の producer 床（S0′ 後は ADM と `apply_attempt` と FP executor で拒否、再登録で通る）、`ProducerBelowFloor` は block を無効にしない、shortfall の読み、kaspad の P6 | `rcore_s3::t17_u2_a_producer_below_the_floor_after_s0_prime_is_refused_until_it_re_registers`、`dos_repro_2::t17_an_fp_executor_below_the_producer_floor_commits_nothing`、`palw_producer_v2.rs::ready_to_produce_v3_reads_the_floor_and_the_committed_ledger_past_the_fence`、kaspad `palw_producer_t12_tests::under_the_t12_floor_the_node_and_the_fold_both_name_the_floor` | 有 |
| T20 | C7 は t12 genesis でちょうど {2M}、2M は Final まで借り `c_2M = 1`、8k は保持しない、≥1,000 span の後発 class は規則で C7、held 行の外の C7 は拒否、2M の RT#2 は S0′、増幅 1.00 | `core/tests/panel_room_short_class_is_released_at_licence.rs::the_genesis_c7_set_is_the_2m_row_alone`・`beside_it_the_2m_row_is_still_held_to_final_at_its_cap_of_one`、`params.rs::the_t12_c7_list_is_the_2m_row`、`palw_work_target_v1.rs::adr0152_c7_is_a_window_of_at_least_1000_spans`、`palw_rcore_plus_is_t12_only`（C7 の拒否）、`rcore_s6_per_bond_share.rs::t20_…`、2M の RT#2: `dos_repro_3_…`、本 lane の T02b | 有 |
| T21 | rate room（C7 に re-key）: bond ごとの share、C7 外の class は licence で replay 解放（8k の (owed, room) = (0, 5)、6 本目が fold）、2M は解放しない、court は再課金、非 seat の DA は止めも課金もしない、t11 parity | `panel_room_short_class_is_released_at_licence.rs`（4 本）、`rcore_s6::t21_…` ×2、`panel_room_step3_…::an_rcore_da_session_on_a_licensed_claim_recharges_nothing`、`panel_room_t11_dormant_parity.rs` | 有 |
| T22 | m = 3 の各 tier と上限、strike の epoch、claim ごとの没収、U3（FP の Final 後有罪は `≤ 3·G_fp`）、IA-9（court default は forfeit ＋ S2 で記録なし、kind 3 の記録は producer の leg 込み、lock の無い seat は 0） | `sv2::t22_the_action_tiers_are_the_adrs_table`、`sv2::t22_u3_an_fp_claim_convicted_after_final_charges_the_capped_producer_tier_once`、`rcore_s4::t22_t35_t36_…`・`t22_s0_prime_…`・`s2_a_court_default_is_charged_as_a_fraud_and_writes_no_court_conviction`、`t46::t46g_fp_claim`（U3） | 有 |
| T23 | B-3: gate は `palw_bond_committed_v1 > 0`・`accuser_exposure > 0`・未成熟行、retire-while-bound は保持、exit の上限を関数として（12,900 / 18,900 / F + 9,000）、UTXO の半分（P2-1） | `sv2::v6_is_v5_below_the_fence_and_adds_the_accuser_clause_past_it`、`sv2`（`with_rcore_plus_mirrors(Some(0), 12_900, …)` の pin）、`p2_b3_vesting_payee_gate.rs::p2_t23_…` | 有 |
| T24 | fingerprint: t11/devnet/mainnet は pin、v22 の params id は一度だけ再 pin、前提ごとの負例（`palw_operator_id_unique` と `DuplicateOperator` を含む）、mirror 不一致、V5 以外、C7 と shard licensing、reporter bps = DnsParams | `core/tests/palw_rcore_plus_is_t12_only.rs::validate_refuses_the_fence_without_each_prerequisite`（ほか AT_V21/AT_V22 と :115 の bps）、`params.rs::shipped_presets_have_pinned_fingerprints`、`sv2::an_operator_identity_already_on_the_chain_cannot_be_registered_again` | 有 |
| T27 | X2 の release: 3 Valid ＋ 2 欠/Unavailable/Incapable/S3 だけの Sampled は保持、5 Valid で解放 | `rcore_s2::t27_t68_missing_and_unavailable_seats_hold_the_escrow_to_final`・`t27_u1_the_8k_row_holds_on_incapable_and_releases_on_full_service`、`rcore_m3::t27_t68_x2_…`、`sv2::t70_a_sampled_seat_is_credited_takes_no_lock_and_is_never_dissent_slashed` | 有 |
| T28 | F+3,001…F+9,000 の有罪が、第 2 時計が保持中・retirement 後に行を burn、`basis_k` は行から | **本 lane**: `t28_a_retired_claims_row_burns.rs::t28_a_conviction_after_retirement_under_a_held_second_clock_burns_the_row`（retirement 後 F+4,000 に kind 3: 行 burn・S3・S4・記録。報酬の X は **liability 記録の** `basis_k` — funnel（`palw_claim_g_v1`）は liability 記録、無ければ claim 記録を読み、vesting 行は読まない（X29 で行がある間は liability 記録もある）。行の写し（N8）は一致し、行の写しだけを carriage で `basis_k = 1` にした probe でも同じ有罪が同じ報酬を払う — どちらを読むかを区別する） | 有（本 lane）。ADR の「`basis_k` is read from the row」は、X29 で liability 記録が行より先に消えない以上、「liability 記録から（行の写しと一致）」と読む — 文言の訂正候補（§7） |
| T29 / T30 | 1,016 の queue と 3d の maturity で filter = fold / market 行が queue を埋めても成熟中は 1 block ≥ 1 行、queue ≤ 1,032 | `p2_mint_path.rs::p2_t29_…`、`p2_t29_conviction_and_maturity.rs` / `vf::t30_t58_…` | 有 |
| T32 / T33 | X7（licence 済みで producer 沈黙 → S1 と signer S4、signer が開示すれば告発者負け）/ backed subset（4 backed ＋ 1 unbacked で licence） | `rcore_m3::t32_c7_…`、`t46::t32_t64_…`・`t32_t54b_…` / `rcore_s3::t33_…`・`n1_…`、`t12_rcore_sr10_door_gate.rs::t12_t33_…` | 有 |
| T34 | DA を全 class で、自動 DA（node の半分）、`dos_repro_4` を自動告発で | `rcore_m3::t34_da_on_the_8k_row_defaults_like_the_floor`・`t34_p2_6_…`、`t46::t34_t54a_…`、kaspad `palw_panel.rs::t34_an_unavailable_accuses_inside_its_landing_margin_on_every_class`、`dos_repro_4_…` | 有 |
| T35 / T36 | `withholding_strikes` の root・運搬・revert・7,500 で剪定・最大 9 / 昇格なし・status 不変・tombstone なし | `sv2::t35_the_strike_rule`、`rcore_s4::t22_t35_t36_…` | 有 |
| T37 | 3 本の genesis retirement・`NoCapablePanel`・heartbeat だけの区間で licence halt、**FP だけの区間も halt（F18）**、upgrade しない S2 は `settled_attempt_finals` を進めない（C5） | `vf::t37_rows_never_mature_during_a_licence_halt`、`rcore_s2::t37_t74_coverage_ticks_s2_does_not_and_its_v2_door_upgrade_ticks_and_releases`。**本 lane（F18）**: `core/tests/t37_an_fp_only_stretch_is_a_halt.rs::t37_f18_a_stretch_in_which_only_free_prompts_license_is_a_halt` | **一部**（F18 は本 lane）。欠けた cell: 「3 本の genesis retirement」と「`NoCapablePanel`」で licence が止まり halt に至る場面の名指し test（halt 自体は述語 `palw_chain_vesting_halted_v1` で同じだが、そこへ至る経路は未検証）。owner A、監査へ |
| T38 | heartbeat の carrier: H-1 の全 object を heartbeat block で fold、miner が入れ、relay が保つ | `t12_h1_carrier_gate.rs::h1_the_gate_passes_what_the_fold_takes_and_refuses_what_it_drops`、`palw_heartbeat_carriers_v1.rs`、`mining/src/manager_tests.rs`・`transactions_pool.rs`（carrier）、`protocol/flows/src/palw_heartbeat_relay.rs`（2 本） | 有 |
| T39 | reporter 報酬（`collected` の基底、枯れた bond、gate の開いた bond は 0、`ΣR ≤ r·Σcollected`、自己有罪は負、commit–reveal、DA/court key への commit 拒否、DA default は最早の告発者、court default は報酬なし、基底の kind 違いは拒否、pending 中の commitment は剪定しない、2M j < k は ADR-0153 まで expected-FAIL） | `sv2::t39_…` ×6、`rcore_s4::r2_a_bond_whose_gate_is_open_collects_nothing`、`t46_p2_8_reporter_filer.rs::t39_…`、kaspad `palw_reporter_filer.rs::t39_…` | 有（**2M j < k の cell は T18m の 2M cell と同じ扱い**: 2M が公開時に閉じているため 2M の flag day の gate に移す案。ADR の amendment として了承が要る、§7） |
| T40 | revert 後の load で再導出（licensed の released/held、open DA）、DL-1: session 中・gate 中・各 phase・U ≥ floor の upgrade 後・abandon hold 中の restart | `rcore_m5_restart.rs::t40_dl1_restart_mid_session_and_mid_gate_on_every_phase_equals_the_uninterrupted_run`・`t40_dl1_restart_mid_abandon_hold`、`rcore_m5_q5_gate.rs::t40_…` ×5、`palw_rcore_q5_gate.rs::dl1_gates_an_s2_licence_past_both_doors_and_the_twin_does_not` | 有 |
| T41 | v22 golden（空と全 map 充填） | `rcore_m5_v22_golden.rs::t41_the_v22_golden_vectors_empty_and_inhabited`・`t41_the_v22_record_encodings_are_pinned`、`vf::t41_…` | 有（出荷 commit で再 pin、§5） |
| T42 | 引き出しは `max(since + 12,900, 最後の F + 9,000)` で完了（court・DA session の有無で）、session を開いたまま retire する告発者は session と `refuted_held` の解決まで保持 | `rcore_m3::t84_t42_an_accuser_uses_its_free_half_and_its_exposure_is_held_until_the_claim_resolves`、上限の関数: `sv2::v6_is_v5_below_the_fence_and_adds_the_accuser_clause_past_it`（12,900） | 一部（「court と DA session の有無で引き出しが上限ちょうどで完了する」端から端の test が欠。owner A） |
| T43 / T44 | trickle で F + 9,000 に成熟 / 行がある限り liability を剪定しない、latch は re-arm を越える | `vf::t43_the_trickle_regime_matures_at_the_bound` / `vf::t44_no_decision_a_term_and_the_latch_survives_a_re_arm` | 有 |
| T47 | `0xFF` の claim id は `0x00` key（A-KEY） | `vf::t47_an_0xff_claim_id_keys_0x00_and_mints_behind_a_full_market`、`p2_mint_path.rs::p2_t47_…` | 有 |
| T57 | SR-9: 1 枚目の panel で 3 Unavailable → 早期 redraw、2 枚目で `UnavailableQuorum`（S0′） | 無し | **規則欠**: S-5（object 56 `PanelUnavailableQuorum`）が int-3 に無い。fold は tag 56 を名指しで拒否し（`RcoreObjectNotLanded`、`palw_state_v2.rs` の "arms 55 … and not yet 56 (S-5)"）、void reason 5 を書く規則が無い（`PalwVoidReasonV2::UnavailableQuorum` の doc「written by S-5, by nobody yet」）。node 側（P2-6 の filer）も無い。production code は書いていない。影響: RT#2 と Q-5 の `NotReplayBacked` が同じ S0′ を課すので安全側（早期 redraw が無い分 H_f が長い、ADR §3.7 の "v3 without SR-9"）。drill D-8 の SR-9 部分は通らない |
| T74（V2 の扉） | SR-1b: L+60 の補完 receipt で flip、L+61 は flip しない、Sampled は flip しない、un-flip なし | `rcore_s2::t74_sr1b_flips_at_l_plus_60_and_not_at_l_plus_61`、`rcore_s2::t37_t74_…` | 有 |
| T75 | 報酬の時期: sweep → `reporter_rewards` → 3d、reorg twin、admission 時の DA key への投機的 commit は拒否 | `sv2::t75_reward_timing_and_its_reorg_twins`、`rcore_m5_reorg.rs::t75_reorg_twin_…` | 有 |
| T76 / T77 / T78 | Eq は `min(C, 3·G_eq)`、genesis 2 本の Eq の後も licence / `duty_bind`（8k 0.6212、2M 1.0000）/ 2M の licence 時 top-up・`lock_2` 適格・FP lane | `rcore_s4::t76_eq_takes_min_c_3_g_eq_and_licensing_continues` / `rcore_s3::t77_…` / `rcore_s3::t78_the_2m_top_up_and_the_lock_2_eligibility` | 有 |
| T80 | kaspad は t11 の params と datadir を起動時に拒否 | `kaspad/src/daemon.rs::t80_testnet_11_is_refused_at_startup_by_name`、`consensus/src/model/stores/palw_state_v2.rs::t80_a_carriage_of_another_state_version_is_refused_by_name` | 有 |
| T81 / T82 / T84 | 4/5/6 の consumed offence と root の扱い（V-2b）/ 6,000 DAA の halt 後の V-7 / A-6 の free half、`accuser_exposure` の再導出、court 挑戦者の予約 | `rcore_s4::t81_…`、`sv2::t26_t81_…` / `vf::t82_a_post_halt_backlog_drains_at_least_one_row_per_block` / `rcore_s3::t84_a_court_challenger_accuses_on_its_free_half`、`rcore_m3::t84_t42_…` | 有（T82 の「行に開いた session が drain を止めない」は T16 の V3S-02 と同じく欠） |

#### M3（F3: DA court）

| ID | ADR の要求（一行） | test | 状態 |
|---|---|---|---|
| T64 | 抽選 unit は受理 block が seed、run の内側、held unit、全 unit に答える、Flat の答えと `OutOfRange` | `rcore_m3::t64_da3_an_event_accusation_draws_inside_the_one_row_run`、`t46::t32_t64_…`、`palw_da_rcore_v1.rs` lib（`the_draw_is_seeded_by_the_block_…`、`one_flat_answers_…`） | 有 |
| T65 | 独占なし（Sybil の session が seat を塞がない、非 seat の上限 3 と生涯 16、seat の 5 本目は拒否、2 人目の告発者、非 seat の `StepLeaf`、`NeedsDissection` は拒否） | `rcore_m3::t65_da8_seats_and_non_seats_each_accuse_and_nobody_monopolizes` | 有 |
| T66 | licence 後と Final 後の session（行の re-key、default で行 burn ＋ S3 ＋ covering signer の S4、3 round で F + 4,000 でも S4、coverage で honest partial は課金されない、floor の X2 端から端） | `rcore_m3::t66_da5_da7_a_final_row_is_rekeyed_and_its_default_is_s3_and_s4`（re-key と「lock が session に付いて行く」V3S-04 の assert）、`rcore_m3::t32_c7_…`（coverage の honest partial は 0）、`t46::t66_x2_a_seat_names_the_divergent_leaf_and_the_executor_is_convicted` | **一部**。欠けた cell: 「3 round で F + 4,000 に届いても covering signer に S4」の経路そのもの（lock が行に付いて行く V3S-04 の assert が近いが代わりではない）。owner A（M3）、監査へ |
| T67 / T68 / T69 | pause credit / 「served」は Valid だけ / session の費用と refuted の保持・返金・burn | `rcore_m3::t67_…`、`t46::t67_r_…` / `rcore_s2::t27_t68_…`、`rcore_m3::t27_t68_…` / `rcore_m3::t69_…` ×2、`t46::t69_r_…`、`palw_da_rcore_v1.rs::a_session_costs_r_times_its_stage_base_and_never_more_than_the_floor` | 有 |

#### M4（F4、SEAT-R、stake 加重の抽選、SR-10）

| ID | ADR の要求（一行） | test | 状態 |
|---|---|---|---|
| T06 | 実 stake 加重の抽選で `(P_k, q, q_f)` の EV grid（coverage の嘘、V1 withholding、re-roll の各戦略）が §4.3 の閾値（floor 17.29M / 13.39M / 8.32M / 6.63M / 50.83M、SW-10 床の最悪 12.74M / 9.88M / 7.15M / 5.72M / 25.61M、executor 項で 6.63M、`stake: None` で 20/16/10/8/56）を再現 | **本 lane**: `core/tests/t06_stake_draw_ev_grid.rs` — `::t06_the_real_race_follows_the_successive_sampling_law`（`palw_panel_stake_race_of_v1` と `palw_segment_assignment_v2` で 6,000 panel を引き、攻撃者席の分布と P2 が厳密な successive sampling の法則の 4σ 内: design point 133、P3 閾値 51、最悪の許容状態 6/8 ＋ 98、`stake: None` は `palw_panel_operator_lottery_of_v1` を一様の法則と）、`::t06_the_ev_grid_reproduces_section_4_3`（§4.3 の stake 加重表 10 行と v3 の一様表 6 行の P2/P3/P5 と floor・8k・2M の EV 全 cell、閾値 floor 133/103/64/51/391、8k 132/102/63/51/387、2M 132/102/63/51/388、`stake: None` 20/16/10/8/56）、`::t06_the_worst_admitted_state_under_sw10`（床の判定は code の `palw_panel_stake_floor_v1`・`palw_panel_stake_weight_v1`: 床が要る Sybil 数 0/0/58/116/174/232/289/347/405、cliff 表、最悪 floor 98/76/55/44/197・8k と 2M 97/75/54/44/195、IA-1b の executor 項（上限）で free redraw 51（6.63M、k = 6）、P2 は 98 のまま、k = 5 は 108。§4.3 が再実行していなかった 8k/2M・30% offline・P3・P5 の executor 項の行も走らせ、動くのは free redraw だけ（8k と 2M も 51 = 6.63M）、どの行も上がらず X 以下しか下がらない）。既存の `adr0152_stake_draw_sw10_split.rs` も | 有（本 lane）。q は 0（§4.3 の慣例、攻撃者に有利な側）、q_f は filing の model（file / 30% offline / free redraw / no filing）で入る |
| T45 | collector は S2 より `basis_k ≥ 2` の組を選ぶ | `t12_rcore_sr10_door_gate.rs::t12_t45_…`、kaspad `palw_panel.rs::the_collector_assembles_coverage_then_v1_then_optimistic`・`the_collector_composes_a_backed_v1_before_s2_…` | 有 |
| T54e | seat の役割: Sampled と S1 Valid（node 端から端） | kaspad `q7_sampled_and_collector_tests::sampled_is_due_only_for_a_partial_seat_at_the_end_of_its_window_past_every_fence`・`sampled_is_filed_as_the_seats_v3_over_its_assigned_mask`・`s3_samples_only_a_capture_seat_s3_verified` | 一部（node の単体はある。devnet preset の node e2e は T54a/b と同じく公開後） |
| T70 / T71 | Sampled（S3 seat が署名、V2 の答えに写像、quorum/coverage に数えない、lock なし、served でない、dissent slash なし、outsider の Sampled は veto を満たさない、fence 未満は拒否）/ 再計数 | `sv2::t70_…` ×5 / `sv2::t71_the_recount_through_the_v3_door`・`t71_a_mixed_set_with_a_v2_full_replay_valid` | 有 |
| T72 / T72b | S2 の fast path（licence、replay は課金のまま、escrow 保持、anchor を刻まない、upgrade で `max(L+120, U)` に Final、1 枚目は redraw、2 枚目は `NotReplayBacked`）/ V3S-01 | `palw_rcore_q5_gate.rs::an_upgrade_lifts_the_gate_and_rearms_the_deadline`・`t72b_…`、`t12_rcore_sr10_door_gate.rs::t12_q5_an_s2_licence_redraws_once_then_voids_not_replay_backed`・`t12_t72b_…` | 有 |
| T73 | Q-6: Sampled の receipt は liable にならない、lock の mask = 割当 mask、DA-7 は lock mask で covering signer だけ（(seat, claim) key）、後の `ProducerWithholding` 提出は no-op | `palw_offence_attribution_v1.rs` lib（Sampled → `PanelFalseValidNotValidVerdict`、2441〜2455 行）、`rcore_s3::t15_…`（mask）、`rcore_m3::t32_c7_…`（covering のみ、(seat, claim) key）、`rcore_m3::m3r_f1_a_post_final_da_default_is_withholding_and_convicts_no_signer`（後の `ProducerWithholding` は no-op）、`palw_da_rcore_v1.rs::only_a_full_mask_covers_a_da_unit` | 有（部品に分かれている） |
| T74（V3 の扉、SR-10） | SR-1b の V3 半分 | `sv2::t74_v3_a_completing_set_at_l_plus_60_reaches_the_sr1b_seam_and_at_l_plus_61_does_not`、`sv2::sr1b_the_v3_flip_moves_the_escrow_term_off_the_producer_ledger` | 有 |
| T85 / T86 / T87 | 整数 log の golden・key 比較・重み上限・2 domain / 抽選の法則（successive sampling、4σ、P2、等重みで一様）/ 重みは operator の 1 bond の担保で上限付き、`DuplicateOperator`、anchor 後の登録は無、ledger は重みを動かさない | `pv2::t85_the_integer_log_the_key_order_and_the_cap` / `pv2::t86_the_draw_is_successive_sampling_and_s1_is_untouched` / `pv2::t87_the_weight_sums_eligible_bonds_and_keys_are_the_operators_own`、`pv2::t93_another_bonds_commitments_move_no_key` | 有 |
| T88 / T89 | `stake: None` は ADR-0130/0147 と byte 一致 / build = accept = fold を 1 state で、anchor 後の `PanelBound` は拒否され `BindTimeout`、IA-1a/1c | `pv2::t88_…` ×2（ほか） / `pv2::build_equals_accept_…`・`t89_…` ×4、`t12_stake_draw_integration.rs::t12_genesis_binds_under_the_stake_draw`・`sw8_the_anchor_block_voids_a_claim_it_does_not_bind` | 有 |
| T90 | admission jury は fence の前後で ADR-0147 のまま（無加重）、`stake` を読まない。残余（40 × 13,000 MSK の Sybil が 0.9728 で過半）は文書のみ | **本 lane**: `core/tests/t90_admission_jury_is_unweighted.rs::t90_the_admission_jury_is_the_operator_ticket_order_whatever_the_stake`（256 母集団 × 再配置・分割・逆順で陪審不変、対照に stake race は動く）、`::t90_the_folds_jury_reads_neither_the_stake_nor_the_fence`（fold の jury 関数の本文 — この line の `admission_jury_seated` か Pool の `admission_jury_v1` のどちらか一方 — と `palw_admission_jury_v1`、seed・quorum・audit 周期の helper。population の collateral 読みは床での閾値だけ） | 有（本 lane）。fold の半分は source の検査で、fence の on/off の fold twin ではない（`Candidate` の audit span に届く integration fixture が無い）。**Pool の merge で変わる**: `feat/t12-activation-pool` @ `2932bd57d` の P2 は関数を `admission_jury_v1` に改名し、`palw_activation_pool`（t12 は genesis）以降は population を panel の床（`palw_panel_collateral_floor_v1`、130,000 MSK）で切り、seed を `palw_admission_jury_seed_v2` にする。jury は無加重のままだが ADR-0147 のものではなくなる — T90 を「無加重、pool の fence 以降は panel の床から引く」に言い直し、twin に pool の fence を足し、ADR の「the jury stays ADR-0147's」を直す（merge の側の仕事。本 test の source 検査は両方の名前を読むので merge で壊れない — Pool の本文で同じ検査が通ることを確認した） |
| T91 / T92 / T93 / T94 | `ready_eff` / `InsufficientEligibleBonds` の条件、cap の operator は 1 席 / anchor 後に panel は動かない / SW-10 床（875‰ で bind、874‰ で拒否、…、executor 項） | `pv2::t91_…`・`sw9_…`、`core/tests/adr0152_sw9_ready_eff_room.rs` ×3 / `pv2::t92_…` / `pv2::t93_…` ×2 / `pv2::t94_…` ×5 | 有。T94 の「still to add: その void で producer の予約が解放される」（IA-1c）は `pv2::t94_a_refused_draw_…` が既に assert している（"the executor's reservation is released"）— ADR の pending 表記は古い |
| T18p-M | （上の M2 表） | | 有 |

#### M5（統合後の fold / reorg / restart）

| ID | ADR の要求（一行） | test | 状態 |
|---|---|---|---|
| T07 / T40 / T41 / T75 の reorg twin / T01 の revert・IBD twin | （上の S 表） | `rcore_m5_reorg.rs`、`rcore_m5_restart.rs`、`rcore_m5_q5_gate.rs`、`rcore_m5_v22_golden.rs` | 有 |
| T48 | mint の reorg | `p2_reorg_and_ibd.rs::p2_t48_a_reorg_across_latch_move_mint_and_burn_lands_on_a_fresh_replay_s_root` | 有 |
| T49 | backlog の中の IBD | `p2_reorg_and_ibd.rs::p2_t49_…` ×2 | 有 |
| T83 | session 中・reveal window 中・backlog 中の restart で root が一致 | `rcore_m5_restart.rs::t83_restart_mid_session_mid_reveal_window_and_mid_backlog` | 有 |

#### item 5（Phase 2 の要の部分、C12）

| ID | ADR の要求（一行） | test | 状態 |
|---|---|---|---|
| T03 / T05 / T23 / T25 | （上の S 表）/ Decision A の分離 | T25: `p2_mint_path.rs::p2_t25_t05_…`・`p2_t25_a_licence_halt_latches_and_moves_nothing_at_the_processor_s_fold`、`vf::t44_…` | 有 |
| T23 の UTXO 半分 | B-3 を UTXO 層で（`palw_v2_locked_bond_outpoints`、`palw_v2_bond_burn_obligations`） | `p2_b3_vesting_payee_gate.rs::p2_t23_every_utxo_site_holds_a_vesting_payee_exactly_while_v4a_does` | 有 |
| T47 | `0xFF` key | （上の S 表） | 有 |
| T48 / T49 | （上の M5 表） | | 有 |
| T50 | EVM twin | `p2_evm_twin.rs::p2_t50_…` ×2（`--features evm` でのみ build） | 有（evm の battery で走る） |
| T51 | RPC | `palw_vesting_read_v1.rs::t51_…` ×7、`misaka-cli/src/palw_vesting.rs::t51_…`、`rpc/core/src/model/message.rs` | 有 |
| T52 | CLI | `misaka-cli`（`operator/roles.rs`・`status.rs`・`work.rs`・`wallet.rs`・`main.rs`）、kaspad `palw_panel.rs::t52_the_fee_funder_…` | 有 |
| T53 | drill の隔離（drill chain の登録・attempt・有罪は公開 t12 で拒否、drill の miner script で coinbase txid が別） | `t53_drill_isolation.rs` ×4、`consensus/core/src/config/drill.rs` ×6、kaspad `palw_drill.rs` ×5、`protocol/flows/src/flow_context.rs`、`utxo_set_override.rs`、`misaka-cli/src/bond.rs` | 有 |
| T58 | queue の補題 | `p2_mint_path.rs::p2_t58_t03_…`、`vf::t30_t58_…` | 有 |

**参考（release-prep lane が a0af3c92 で所在不明とした番号のうち上に無いもの、と IA-14 9-3）**:
T54a（`t46::t34_t54a_…`、processor 半分）・T54b（`t46::t32_t54b_…`）・T54c（kaspad `palw_reporter_filer.rs::t54c_…`、`t46_p2_8_reporter_filer.rs::t18_t54c_…`）・
T54d（`t54d_false_valid_filer.rs` 12 本、int-3 で merge 済み）・T54f（`t54f_replay_filer.rs` 7 本）は **有**（devnet preset の node e2e は公開後）。
**T54g は監査 branch `rcore/p2-8e` @ `ade4b458` にだけある**（`consensus/src/pipeline/virtual_processor/tests/t54g_object_rehearsal.rs` 2 本、
`kaspad/src/palw_filer_held_e2e.rs` 13 本）— P2-8e の merge で入る。**T55**（経済 ledger の `named`・`minted`・`burned_by_conviction` が
root の counter と一致）は **規則欠**: `palw_economics_ledger_v1.rs` にその列が無い（item 5 の範囲外）。T56 は `p2_mint_path.rs::p2_t03_…`（供給の恒等式）で有。
**IA-14 9-3 の processor e2e**（正直な producer が DA 告発に答え、課金されない）は **有**:
`t46::p2_7_an_honest_producer_answers_two_accusers_with_the_nodes_builder_and_is_not_charged`・`p2_7_a_held_session_on_an_attempt_claim_is_answered_from_its_capture`。

**監査の未 merge branch に属するもの（本 lane では書かない。監査へ）**: A-held（`feat/t12-aheld-node`）の T-A 系（object 57 の自動応答、C1〜C5、
`palw_filer_held_e2e`）と shard court の one-move（`8be0f661`、J-8。int-3 の祖先ではない）の test は item 7・8・9-8 の証拠で、
番号 T01〜T94 の外。Activation Pool（`feat/t12-activation-pool`）の P1〜P5 の回帰と R1/R2 の test も同じ。merge 後に出荷 commit で上の表と一緒に取る。

### 1b. 本 lane で足した test と、red になることの確認（mutation）

commit `3568510f`（test だけ。production code は変えていない）＋ rustfmt と本文書の commit（§1b の記録）。いずれも base `8270cf03` の上。

| test | 番号 | 一時的に壊した rule（mutation） | 結果 |
|---|---|---|---|
| `consensus/core/tests/t02b_rt2_is_s0_prime_on_every_class.rs::t02b_rt2_is_s0_prime_on_the_floor_the_8k_row_and_the_2m_row` | T02b（regenesis） | `sweep_deadlines` の RT#2 の `void_and_slash` を floor（`base_class_id`）だけにした | red: "8k row: S0′ — the producer forfeits exactly its commitment"（left 0） |
| `…::t02_bind_timeout_and_no_capable_panel_charge_nothing` | T02 | `sweep_deadlines` の `Provisional` の void を `void_claim` → `void_and_slash` | red: "floor: S0 — no bond is charged" |
| `consensus/core/tests/t37_an_fp_only_stretch_is_a_halt.rs::t37_f18_a_stretch_in_which_only_free_prompts_license_is_a_halt` | T37（F18） | `license_claim` の `ticks` から `attempt &&` を外した（FP の licence も anchor を刻む） | red: "F18: a free prompt's licence settles no anchor"（left `(2, [1005, 1129])`） |
| `consensus/core/tests/t90_admission_jury_is_unweighted.rs::t90_the_admission_jury_is_the_operator_ticket_order_whatever_the_stake` | T90 | `palw_admission_jury_v1` を「担保の多い operator から」並べた | red: "member 0: the jury is the lowest operator tickets" |
| `…::t90_the_folds_jury_reads_neither_the_stake_nor_the_fence` | T90（fold） | 同上（jury の本文が `collateral` を読む）／`admission_jury_seated` に `PalwPanelStakeDrawV1` を置いた | red: "palw_admission_jury_v1 reads `collateral`…"／red: "admission_jury_seated reads `stake`…" |
| `consensus/src/pipeline/virtual_processor/tests/t28_a_retired_claims_row_burns.rs::t28_a_conviction_after_retirement_under_a_held_second_clock_burns_the_row` | T28 | (1) `post_final_producer_leg_v1` が retired claim（claim 記録なし）を飛ばす（行を burn しない）／(2) `palw_claim_g_v1` の liability 記録の `basis_k` を 1 に | (1) red: "the conviction burned the row"／(2) red: "with the claim record gone, G and basis_k are the liability record's"（left `basis_k` 1、right 2） |
| `…/t18m_forged_output_tiled.rs::t18m_forged_output_tiled_convicts_the_full_mask_signer_before_final_and_never_an_honest_decode`、`::t18m_forged_output_tiled_after_final_reverses_it_by_kind_3_and_by_kind_4` | T18m | `palw_forged_output_tiled_fault_v1` の `NotSelected` と `OutOfVocab` を常に `TokenHolds` | 両方 red: 前者は partial の拒否理由が `TokenHolds`（"the committed token is its row's selection…"）に変わり、後者は gate が有罪の object を拒否 |

mutation を戻した後、同じ test は全部 GREEN（core: 本 lane の 3 file ＋ `rcore_s4_conviction_funnel`・`t12_class_verify_deadline`・`palw_rcore_plus_is_t12_only` が ok。consensus: `t46_false_valid_real_claim` の全体（子の t47・t18m・t28・t54d・t54f・p2_8・p2_t29、IA-14 9-3 の `p2_7_…` を含む）84 passed / 0 failed。stranger script の `selftest` も PASSED（21 checks、IA-14 9-5））。数値（test の出力）: T02b の RT#2 は floor 320,095,402,740 / 8k 369,516,654,680 /
2M 6,294,378,856,900 sompi（= commitment）。T28 は F 123 → retire 3,124 → 有罪 4,123、行 320,084,650,080 sompi（`basis_k` 2）burn、S3 960,289,208,220、
S4 984,302,020,940、X 5,876,330（`basis_k` 1 なら 11,752,660）。T37/F18 は anchor 1,005、FP licence 1,129、halt 7,005（window_court 3,000）。
T90 は 256 母集団で陪審不変、同じ再配置で stake race は 142 母集団で動いた（対照）。

**2 回目（review 対応、同じ branch の次の commit）** — 足した test / 強めた test と mutation。各 mutation は production の 1 行を一時的に
変え、red を見て、元の file に戻した（`git status` で production の変更が無いことを確認）:

| test | 番号 | 一時的に壊した rule（mutation） | 結果 |
|---|---|---|---|
| `consensus/core/tests/t06_stake_draw_ev_grid.rs::t06_the_real_race_follows_the_successive_sampling_law` | T06 | `palw_panel_stake_entries_under_v1` の `weight_msk` を 1 に（stake race を一様に） | red: "design point: P(A = 1): measured 0.0000, the law says 0.0168 (4σ = 0.0066)" |
| `…::t06_the_worst_admitted_state_under_sw10` | T06（SW-10） | `palw_panel_stake_floor_v1` の `>=` を `>`（7/8 = 875‰ ちょうどが bind しない） | red: "the Sybils the floor needs, k = 8 … 0"（left `[0, 1, 58, …]`、right `[0, 0, 58, …]`）。※ 最初の版は executor 項 0 を `palw_draw_operator_weight_msk_v1(0)`（= 1、重みは 1 未満にならない）で足していて、この mutation を見逃した — executor の bond の list（無ければ空）を `palw_panel_stake_weight_v1` で量る形に直してから red を確認 |
| `…::t06_the_ev_grid_reproduces_section_4_3` | T06 | （純粋な model の再計算。preset の入力は `net()` が pin: genesis 8 × 939,063 MSK、panel の床 130,000 MSK、`E` 3,200.85 MSK、上限 1,000,000、875‰） | 数値は §4.3 と一致（表 16 行の全 cell、閾値 15 個 ＋ `stake: None` 5 個、最悪状態 15 個） |
| `consensus/src/pipeline/virtual_processor/tests/t62_the_unique_path.rs::t62_after_final_and_retirement_every_source_resolves_the_same_path` | T62（N8） | `finalize_claim` の vesting 行の写しで `trace_root` を 0 に | red: "kind 3 after Final: the vesting row's copies (N8)"（trace root が 0 と実値） |
| 同上 | T62（J-2 の第 3 源） | `palw_offence_target_v1` の vesting 行の分岐で `trace_root` を 0 に | red: "the vesting row alone: the committed root, the recorded job identity (J-1) and the trace root (R9)" |
| `::t62_before_final_every_conviction_kind_resolves_one_path`、`::t62_after_final_…`、`::t62_a_court_default_lands_on_the_resolved_path` | T62（liability 記録） | `persist_panel_liability` の `job_identity` の写しを 0 に | 3 本とも red: "DA default, voided: the liability record (F1's appended fields)"、"kind 3 after Final: the liability record …"、"court default: the liability record …" |
| `…/t28_a_retired_claims_row_burns.rs::t28_…`（probe を足した） | T28 | `palw_claim_g_v1` が vesting 行の `basis_k` を読む（liability 記録の代わりに） | red: "the row's diverged copy moves nothing: the funnel reads the liability record" |
| `consensus/core/tests/t90_admission_jury_is_unweighted.rs::t90_the_folds_jury_reads_neither_the_stake_nor_the_fence` | T90 | (1) fold の関数を Pool の名前 `admission_jury_v1` に改名（呼び出しも）、(2) `palw_admission_jury_quorum_v1` に `weight` を読ませた | (1) は通り（名前の解決と本文の検査を抜けた）、(2) で red: "`pub fn palw_admission_jury_quorum_v1(` reads `weight`" |

mutation を戻した後: core の `t06_stake_draw_ev_grid`（3）・`t90_…`（2）・`t02b_…`（2）・`t37_…`（1）・`adr0152_stake_draw_sw10_split`（2）・
`rcore_s4_conviction_funnel`（9）が ok。consensus の `t46_false_valid_real_claim` 全体（子の t62・t28 を含む）は下の再実行の 2 行目。
数値: T06 の実 race（6,000 panel）の P2 は design point 0.5050（法則 0.5003）、P3 閾値の母集団 0.2385（0.2408）、最悪の許容状態
0.5063（0.5006）、`stake: None` 0.5133（一様の法則 0.5026）。T28 の probe は行の写しを `basis_k` 1 にしても報酬 194,458,535,283 sompi のまま。
**int-3 `f7350af91`（A-held）を merge した後の tree でも**: core の上の 6 target（19 test）が ok、consensus の `t46_false_valid_real_claim`
全体（t62 の 3・t28・t18m・t47・t54d・t54f・p2_8・p2_t29 を含む）が 87 passed / 0 failed（merge 前も 87 / 0）。

再実行:

```bash
export CARGO_TARGET_DIR=<target> CARGO_BUILD_JOBS=4 CARGO_INCREMENTAL=0
cargo test --locked -p kaspa-consensus-core --test t02b_rt2_is_s0_prime_on_every_class --test t37_an_fp_only_stretch_is_a_halt --test t90_admission_jury_is_unweighted --test t06_stake_draw_ev_grid
cargo test --locked -p kaspa-consensus --lib -- t46_false_valid_real_claim   # t62・t28・t18m を含む子の suite 全体
```


---

## 2. IA-14 の ship 条件（各項目を出荷 commit で）

「8270cf03」列は lane 3 が 2026-09-25 に code と `git merge-base --is-ancestor` で確かめた判定（**満** / **未**）と根拠。「a0af3c92」列は
release-prep の読みで、履歴として残す。判定は出荷 commit でもう一度取る（merge で動く）。

| # | 条件 | 8270cf03（lane 3） | a0af3c92（release-prep） | 添付する証拠 |
|---|---|---|---|---|
| 9-1 | F2 と F1 の fence（`palw_offence_attribution`）は SEAT-S1・SEAT-S2・SEAT-R と一緒にだけ出す。SEAT-S2 の kaspad 半分は `palw_seat_replay_step_v1` で `CoreV1` の `output_root` を比べる | **満**。SEAT-R: `palw_seat_r_in_force_v1`（`kaspad/src/palw_panel.rs`、test `seat_r_is_in_force_on_testnet_12_from_genesis_and_nowhere_else` :15153）。SEAT-S1: base0 `seat_s1_whole_job.rs`（3 本）。SEAT-S2: base0 `seat_s2_output_root.rs`（3 本）と kaspad の `palw_replay_answer_v1`（`Some(root) if root == claimed_output_root => Reproduces`）、test `past_seat_r_the_replay_step_refutes_a_claim_whose_answer_is_not_the_core_v1_root`（:15736、`449fd892` は祖先） | あり | 上の test の PASS |
| 9-2 | `AnyValid` fence の H-1（S3 の layer-sample は `verify_material` の前に Valid を署名しない）と H-2（FP S1 resume は `palw_fp_job_pin_of_context_v1(&ctx) == duty.fp_job_pin_v1()`） | **満**。H-1: `1e20edd50` は祖先、test `seat_s_tests::c1_s3_never_attests_past_the_fence_without_verify_material`（:16583）。H-2: `kaspad/src/palw_panel.rs:13158` の `palw_fp_job_pin_of_context_v1(&ctx) != pin` → `None`（source test :16972）、`d675423d` は祖先。T18p-M は base0 `seat_material_duty.rs`。※ ADR §8.3 item 9 の「H-2 is pending」と IA-14 の同文は古い | あり | 両 test と T18p-M の PASS。入っていなければ identity site は `Whole` |
| 9-3 | producer の V2 DA responder が `palw_rcore_plus` を武装する全 build にある（honest producer が告発に答え、課金されない processor e2e） | **満**。kaspad は `session.palw_disclosure_duties_v1(vec![bond_key])`（:7607）で自分の claim の session も読む（`palw_da_duties_v2` は court 用に残る）。processor e2e: `t46::p2_7_an_honest_producer_answers_two_accusers_with_the_nodes_builder_and_is_not_charged`（:4428）、`p2_7_a_held_session_on_an_attempt_claim_is_answered_from_its_capture`。※ ADR の「the responder's end-to-end test is to be written」は古い | あり | 両 test の PASS。運用上の飢え（PLAN §2.5 R-2b）は §7 |
| 9-4 | `PALW_RCORE_SEAT_DA_ANSWER_LANDED_V1` は seat の DA 自動応答（P2-7）と同じ commit でだけ `true` | **満**。`true`（`consensus/core/src/palw_da_rcore_v1.rs:67`）、P2-7（`b34df2ff`・`fc5e30d8`）は祖先。covering signer の答え: `t46::t32_t54b_…`、core `rcore_m3::t32_c7_…`（`landed` の両側） | あり | flag の値と上の test |
| 9-5 | `misaka-palw-derive` と stranger script は network の attempt rule（t12 は `CoreV1`）で `output_root` を再計算。FP worker も | **満**。`a4682a8d`・`d675423d` は祖先。`misaka-palw-derive/tests/output_root_rules.rs`（4 本）、`scripts/misaka-palw-derive-stranger.py` | あり | `cargo test --locked -p misaka-palw-derive --test output_root_rules`、`python3 scripts/misaka-palw-derive-stranger.py selftest` |
| 9-6 | `PALW_RCORE_VESTING_ROWS_LANDED_V1 = true` と S-4 の funnel（kaspad は false なら起動しない） | **満**。`true`（`consensus/core/src/palw_state_v2.rs:2961`）、S-4 の merge `68f0d672` は祖先、kaspad `palw_rcore_build_can_run_v1`・test `n1_rcore_plus_needs_the_vesting_rows_at_startup`（`kaspad/src/daemon.rs:2216`） | あり | flag の値、`rcore_s4_conviction_funnel.rs` と n1 の PASS |
| 9-7 | 4-quater の consensus 半分と P-1（`palw_class_verify_deadline`、pruning depth ≈ 74,920 DAA、D_cap 16,000）が genesis params にある — 2M は規則で閉じる | **満**（a0af3c92 から変わった）。merge `a66509f9`（`a4323997`）。`Params::palw_class_verify_deadline`、`params.rs` の `T12_PRUNING_DEPTH = 74_920`（T-D7 の test）、2M の拒否は `core/tests/t12_class_verify_deadline.rs::td2_the_2m_row_is_refused_at_launch_attempt_and_free_prompt`。t12 の params と fingerprint は a0af3c92 から動いている（§3 の値は古い） | 無い | 出荷 commit の t12 params、`probe-identity-local.sh` の fingerprint、T-D2 の PASS |
| 9-8 | A-held と shard court（`feat/t12-aheld`、8be0f661 の上）が監査の review 後に merge、B の routing・object 57 の自動応答・N4 | 8270cf03 では **未**。**int-3 `f7350af91`（15:01）で merge 済み**: `2f92228f`（`1ee0e08d4` を含む）と `8be0f661` は祖先（本 lane はこれを merge した）。残りは review の結論の添付と item 7 の test の PASS | 無い | 監査 review の結論、merge commit、item 7 の test |
| — | 公開条件ではない（IA-13）: SEAT-S4 の forged-sibling residual（2M の flag day 項目） | — | — | — |

**IA-14 のうち 8270cf03 で「未」は 9-8 だけ**（item 8 の shard court（8be0f661）も同じ merge で入る）。**これは item 9 だけの判定で、
gate 全体は未達**: item 1（T57 の規則欠と一部の cell）、2、4、7、8 も未 — §1.0。

---

## 3. a0af3c92 での identity（**古い値。使わない**）

2026-09-25、Mac、`contrib/t12-deploy-kit/probe-identity-local.sh`（dev build の kaspad、sha256 `2a7dd6e4…`、隔離 node）で読んだ値。
§1 item 7・§2 9-7/9-8 の merge（deadline＋P-1、A-held、Activation Pool）と P2-8 がこれを動かす。
**lane 3 の注記（8270cf03）**: deadline ＋ P-1（9-7）と P2-8 は既に merge されたので、下の値は 8270cf03 の t12 とは一致しない
（lane 3 は identity を読み直していない。再 pin は §5 で出荷 commit に対して一度だけ）。

**`fleet.env` にはここから何も貼らない。** `EXPECT_FP`・`EXPECT_GENESIS`・`PREMINE_TXID` の唯一の出所は、出荷 commit の上で §5 step 1 の
`contrib/t12-deploy-kit/probe-identity-local.sh --build --layout` が出す値（exit 0）。下は a0af3c92 の記録で、貼れない形にしてある
（8270cf03 で既に動いている。step 1 の merge — readiness-horizon の新 param と Pool — でさらに動く）。

```
# DO NOT USE — a0af3c92 (2026-09-25), stale on 8270cf03 and on every later commit
# a0af3c92 fp         8a4810231e4f54e7…b2d5ee6
# a0af3c92 genesis    a27f8f44fe4d91a5…a8ca1f23
# a0af3c92 premine    5e0d5f1b37a71288…e55e2669
# a0af3c92 schedule   8b0ee13cd93ed0ca…c5077e284c8 (fence schedule "1000")
# a0af3c92 manifest   9def81a1c56c02d5…adb4201d8d
```

fingerprint `8a481023…` は EVM bridge ledger（fad522c6、int-3 への merge dee8b595）の時点でメモリに記録された t12 の値と同じ（その後の merge は t12 の params を動かしていない）。genesis は c8652a97 の候補
`a27f8f44…` のまま（R-core+ の genesis 行はまだ動いていない）。drill salt `5353…53`（例示用）の drill genesis は
`1678f353…70349`（`--drill-salt` の出力。実際の drill の salt ではない）。

---

## 4. 順序（出荷 commit から公開後まで）

| step | 何を | 誰 | 確認 |
|---|---|---|---|
| 1 | ~~**deadline ＋ P-1**（`feat/t12-class-verify-deadline`）を merge~~ — **済（8270cf03）**: `a66509f9` が `a4323997` を入れた（`git merge-base --is-ancestor` で確認） | — | |
| 2 | ~~**A-held の node 半分**を merge~~ — **済（int-3 `f7350af91`、15:01）**: `feat/t12-aheld-node` @ `2f92228f`（`1ee0e08d4`、`8be0f661` を含む）。item 7・8・9-8 の証拠は出荷 commit で | — | |
| 3 | **Activation Pool と readiness-horizon**（local `feat/t12-activation-pool` @ `f92f34a7f`（15:10 時点。P1/P2/P4 と P4 の review）、`feat/t12-readiness-horizon` @ `a9d9f8320`（15:09、pool `f92f34a7` と int-3 `f7350af9` を merge 済みで t12 の pin を取り直した。新しい t12 param `palw_readiness_v2_max_age_spans`、genesis-only、t12 だけ 24 span、他の preset は None）を merge — readiness-horizon を merge すれば pool も入る）。**§5 の再 pin の前に**（さもなければ fingerprint を 2 度 pin し直す）。T90 の source 検査は Pool の改名を読む（§1a）。T06 は Pool の P1 が足す `PALW_T12_GENESIS_CLAIM_ESCROW_SOMPI` を `E` に使える | 実装 | |
| 3b | **P2-8e**（`rcore/p2-8e` @ `ade4b4581`、T54g。A-held node の上なので step 2 の後）。**S-5**（T57）はユーザーが入れると決めた場合だけ | 実装（S-5 はユーザー判断） | S-5 は **【確認】** |
| 4 | ~~**B の lane**（`rcore/p2-file`、`rcore/m5b-tests`、`rcore/readiness-escalation`）を merge~~ — **済（8270cf03）**: `ec657a445`・`bad93f808`・`2b356156c` は祖先 | — | |
| 5 | §1・§2 の各 test を出荷候補で確認し、残りの一部 cell（§1.0）を監査と詰める。merge 後に監査 branch の番号の外の test も集める | 実装・監査 | |
| 6 | **再 pin**（§5）。t12 の identity と kit の写しを読み直し（`probe-identity-local.sh --build --layout`、exit 0 が条件）、resource profile を再評価して PLAN §2 の表と install-*.sh の share / memmax を直す: `cargo test --locked -p kaspad --test t12_role_memory_figures -- --ignored --nocapture`（全行が `OK`） | 実装 | |
| 7 | genesis が a27f8f44 から動いたら、a27f8f44 を `fleet.env.example` の `FORBIDDEN_GENESIS` と `t12_regenesis.rs` の superseded 接頭辞リストに足し、docs（`docs/testnet12-join-mining.md`、`docs/testnet-12-regenesis-2026-09-23.md`）と explorer の `PANEL_BOND_TX` を直す。class id・premine 配置・kit の写しは §5 の 7〜9 | 実装 | |
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
値は **計算して貼る**（手で打たない）。

**8270cf03 から出荷 commit までに t12 の identity を動かすもの**（step 1 の merge。どれも t11 / devnet / mainnet は動かさないはず — 3 で確認）:
* readiness-horizon（`f6e970e98`・`694012721`）: 新しい param `Params::palw_readiness_v2_max_age_spans`（t12 だけ `Some`、24 span、
  `consensus_params_id` に Some-only で hash）。branch 上で既に `palw_offence_attribution_is_t12_only.rs`・`evm_bridge_ledger_is_t12_only.rs`・
  `palw_readiness_horizon_is_t12_only.rs` の pin を解いている（WIP）— merge で入る値を出荷 commit でもう一度確かめる。
* Activation Pool: R1 の genesis-only fence `palw_activation_pool`、P1 の terms（`bonus_cap_sompi` などが params の hash に入る）と
  `PALW_T12_GENESIS_CLAIM_ESCROW_SOMPI`、P2 の jury の床と seed v2、P4 の登録 sponsor（consensus なら identity に入る）。
* A-held（int-3 `f7350af91` で merge 済み）: `Params::validate_palw_held_answerability_v1` と、genesis の held 行から導く bundle の mirror
  `held_unanswerable_classes`（`sync_palw_held_answerability`）。mirror が identity に入るかは再 pin の時に確かめる。P2-8e、S-5（入れる場合）
  — genesis の params に何が入るかを merge の後に `git diff 8270cf03 -- consensus/core/src/config/params.rs` で列挙する。

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
7. **genesis の class id・artifact root**（A-held や Activation Pool が genesis の行や graph-v7 profile を動かしたら動く）
   - `consensus/core/src/config/class_manifest_const_v1.rs` の `the_committed_manifest_parses_to_what_it_says`（2M: inventory root
     `f63af2c4…`、class id `74c67e63…`、artifact digest `b5baca63…`）と `the_committed_8k_manifest_parses_to_what_it_says`（8k: `88096dc1…`、
     `ebf44d0a…`、`f4af38d9…`）。committed sidecar（`consensus/core/src/config/class-manifests/*.palwmanifest`）を差し替えたら、その sha256 を
     kit の `MANIFEST_8K_SHA256` に。
   - `consensus/core/tests/t12_regenesis.rs` の `the_held_rows_are_the_fleets_classes`（2M `74c67e63…`・8k `ebf44d0a…` の shape profile id と、
     genesis に登録される model class が「8k、2M の順にこの 2 本だけ」）、`t12_genesis_reads_its_root_from_the_committed_manifest`・
     `t12_genesis_roots_are_all_read_from_committed_manifests`（root は committed manifest から）、`t12_certifies_both_lanes_of_the_classes_it_registers`。
   - `consensus/core/src/config/params.rs` の `PALW_T12_RCORE_CONSERVATIVE_CLASSES`（`the_t12_c7_list_is_the_2m_row`）。
   - `t12_regenesis.rs` の `t12_shares_no_premine_outpoint_or_genesis_with_the_sentinel_chains` にある **superseded genesis の接頭辞リスト**
     （`f6cc9576`・`a8cabac4`・`d73dbf44`）: genesis が a27f8f44 から動いたら `a27f8f44` を足す（`FORBIDDEN_GENESIS` と同時に）。
8. **premine の index 配置**（Activation Pool の行や card の並べ替えで動き得る）
   - `consensus/core/src/config/premine.rs`: `MAIN_PREMINE_INDEX = 40`、genesis bond の collateral は card が **宣言した** index
     （`premine_index`）、fee float は `MAIN_PREMINE_INDEX + 1 + i`（i は `PALW_T12_GENESIS_BONDS` の **並び順**。`bonded_genesis_utxos_on`）。
     kit は card N を bond `PREMINE_TXID:N`・fee float `PREMINE_TXID:(FEE_FLOAT_BASE + N)` と一つの番号で呼ぶので、両者が一致している必要がある。
   - `t12_regenesis.rs` の `t12_bond_collateral_matches_the_card`・`the_fleets_premine_indices_are_unchanged_on_t12s_own_txid`・
     `t12_genesis_mints_exactly_the_cap`。
9. **kit と explorer の写し**（node は起動時にこれらを確かめない。`--check` は flag と sha だけ、switch の旧 gate は fp と genesis だけだった）
   - `contrib/t12-deploy-kit/fleet.env.example`: `FEE_FLOAT_BASE`（41）、`CLASS_8K`、`ART_8K_BYTES`、`ART_8K_SHA256`、`MANIFEST_8K_SHA256`、
     `FORBIDDEN_GENESIS`、`HB_ADDR`（card 0 の payout address）。
   - `contrib/t12-deploy-kit/lib.sh`: `CLASS_2M_PREFIX`（74c67e63）。
   - `contrib/misakascan-t12/app.js`: `LLM_CLASSES`（floor・8k・2M の id）、`PANEL_BOND_TX`。
   - **pin**: `consensus/core/tests/t12_deploy_kit_constants.rs` がこれらをこの build の genesis と突き合わせる（上の 8 の配置、class id、
     artifact bytes、explorer の表、`FORBIDDEN_GENESIS` に自分の genesis が無いこと）。`MANIFEST_8K_SHA256` は `probe-identity-local.sh --layout`
     が committed sidecar の sha と比べる。
     ```bash
     cargo test --locked -p kaspa-consensus-core --test t12_deploy_kit_constants -- --nocapture   # LAYOUT card N … の表を印字
     contrib/t12-deploy-kit/probe-identity-local.sh --build --layout                               # binary の genesis の class と bond も比べる（exit 0）
     ```
   - 公開時の gate: `install-<host>.sh switch` は node ごとに bond 8 本が `PREMINE_TXID:0..7`、`CLASS_8K` が登録済み、
     `CLASS_2M_PREFIX` の class が登録済みであることを RPC で確かめ、違えばその node を止める（`lib.sh` `wait_genesis`、`t12check.py --expect-*`）。
10. **文書と kit**: `docs/testnet12-join-mining.md`・`docs/testnet-12-regenesis-2026-09-23.md`（genesis `a27f8f44…`、premine `5e0d5f1b…`）、
   `contrib/t12-deploy-kit/PLAN.md` §4 の暫定値と §2 の表（`kaspad/tests/t12_role_memory_figures.rs` の印字）、`contrib/misakascan-t12/app.js`
   の `PANEL_BOND_TX`、この文書の §3。
11. 再 pin の commit の後に step 8 の battery を 2 回（上の 2 本の test も含まれる: `-p kaspa-consensus-core --tests` と `-p kaspad`）。

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
- **ユーザー（gate の判断）**: **T57 / S-5** — S-5（object 56 `PanelUnavailableQuorum` と P2-6 の filer）を公開前に入れるか、RT#2（S0′）を
  唯一の課金として出し §7.1 S の done-when と §8.3 item 1 の T57 を gate から外すか（安全側だが D-8 の SR-9 と §3.7 の SR-9 付き H_f は
  成り立たない）。**2M の cell** — T18m と T39 の 2M cell を 2M の flag day の gate に移すことの了承（下の ADR amendment）。
- **監査**: ~~所在不明の test 番号（T02b、T06、T28、T31、T57、T62、T90、T54c〜T54g、T55）の所在か追加~~ → lane 3 が §1a で所在を示し、
  T02b（regenesis の半分）・T02 の 0 の cell・T28・T90・T18m の残りの cell・T37 の F18 を書き、2 回目で **T06・T62** を書いた（§1b）。
  **残りの一部 cell**（item 1 の未の理由、§1.0）: T16/T82 の V3S-02 条項（A/B）、T42 の引き出し完了の端から端（A）、T26 の F21 twin（A）、
  T18p の kaspad 層（B）、T37 の「3 本の genesis retirement」「`NoCapablePanel`」から halt に至る場面（A）、T66 の「3 round で F + 4,000」の
  S4 経路（A、M3）、T54e の node e2e（B）／**T55 は規則欠**（ledger の `named`・`minted`・`burned_by_conviction` の列。item 5 の外）／
  A-held の review と merge（9-8、item 7・8）と、merge 後の A-held・Pool・readiness-horizon の番号の外の test の収集／`fix/t12-live-seat0` が
  公開 line に要るか／t11 の休眠 parity の digest が動いた場合の確認。
- **ADR の amendment（B か A が本文へ。code は ADR を編集しない）**: (1) 古い記述の訂正 — §8.3 item 9 と IA-14 の「H-2 pending」
  「the responder's end-to-end test is to be written」、IA-1c と T94 の「reservation release の test は未」（どれも 8270cf03 で満たされている）。
  (2) T31 の「fence 未満は v3 の場合」と T04 の ConflictingPermit 1 share — R-core+ は attribution を前提に要求するので起こらない。
  (3) **T18m と T39 の 2M cell を 2M の flag day の gate へ**（2M は U-D1 で公開時に閉じている）— 了承が要る。(4) **T90 と SW-A4 の
  「the admission jury stays ADR-0147's」** — Pool の P2 の後は「無加重、`palw_activation_pool` 以降は panel の床（130,000 MSK）から
  seed v2 で引く」。(5) T28 の「`basis_k` is read from the row」— code は liability 記録から読む（`palw_claim_g_v1`。行の写しと一致、X29）。
  (6) T06 の「reproduces or replaces」— 本 lane の port は §4.3 の値をそのまま再現した（置き換えは無い）。§4.3 が IA-1b で「not re-run
  (UNVERIFIED)」とした行は T06 の model で走らせた: executor の bond が上限にあるとき動くのは free redraw だけ（floor 7.15M → **6.63M**
  （51、k = 6）、8k と 2M 7.02M → **6.63M**（51、k = 6））。P2 with filing（12.74M / 12.61M）、30% offline（9.88M / 9.75M）、P3（5.72M）、
  P5（25.61M / 25.35M）は動かない（どれも race が縛る）。ADR の表の該当 cell を埋める。
