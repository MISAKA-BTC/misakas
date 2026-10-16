# STATUS — 決定 → 実装 → 検証 → armable → activated

**この表は、機能ごとに 5 つの状態を分けて書いた現在地です。** 「決定された(Accepted)」は「実装された」でも「検証された」でも「有効化された」でもありません。読者が混同しないよう、列を分けています。

- **作り方:** コード(`consensus/core/src/config/params.rs` の fence と preset)と既存の記録から写しただけです。**何も実行していません**(build・test・実ノードは未実施)。記録に無いセルは、推測せず `unknown — check …` と書きました。
- **基準日:** 2026-10-16。基準 commit は `25320cbc4`(branch `integ/rfc02-rfc06-12-g14` の先端)。記録が新しい commit で動いた場合は、下の「出典」の文書が正で、この表は古くなります。**更新するときは、出典の文書を読んでから該当行だけを書き換えてください。**
- **出典(正):** [remaining-work-2026-10-16](design/palw/remaining-work-2026-10-16.md) · [g14-completeness-proof-2026-10-16](design/palw/g14-completeness-proof-2026-10-16.md) · [consolidated-validation-2026-10-15](design/palw/consolidated-validation-2026-10-15.md) · [t12-daa14000-release-plan-2026-10-15](design/palw/t12-daa14000-release-plan-2026-10-15.md) · [activation-readiness-matrix](design/palw/activation-readiness-matrix.md) · [rfc-0015](design/palw/rfc-0015-implementation-status-2026-10-15.md) / [rfc-0016](design/palw/rfc-0016-implementation-status-2026-10-15.md) / [rfc-0018](design/palw/rfc-0018-implementation-status-2026-10-15.md) の実装状況 · [adr-0162-0178-implementation-status](design/palw/adr-0162-0178-implementation-status-2026-10-15.md) · ユーザー決定の記録 [user-decisions-2026-10-15-adr-rfc16-rfc17](design/palw/user-decisions-2026-10-15-adr-rfc16-rfc17.md)

## 列の意味

| 列 | 意味(ユーザーの 3 段階語彙 2026-10-09 に Decision と Activated を足したもの) |
| --- | --- |
| **Decision** | ADR/RFC/ユーザー決定として採用されたか。Draft・Accepted・Withdrawn |
| **Implemented** | コードが存在するか。fence の下で休眠(`None`)か、有効かを書く |
| **Verified** | 対象 branch で build でき、単体試験・real-node E2E・必要な攻撃試験が通ったか。**通った範囲だけ**を書く(PASS と書かれていない項目は PASS ではない) |
| **Armable** | Verified に加えて、経済 parameter・外部 review・activation 条件が揃い、有効化の判断に出せるか |
| **Activated** | 稼働中の network(testnet-12)の preset で、実際に有効になっているか |

「fence が `None`」= 休眠。`consensus/core/src/config/params.rs` の 4 つの preset(mainnet・testnet・simnet・devnet)の既定は全て `None` で、testnet-12 だけが `PALW_T12_*` の一覧で高さを与えて有効にします。

## 1. testnet-12 が実際に走らせているもの

| 項目 | 現在地 | 出典 |
| --- | --- | --- |
| 識別子 | params `5ee7fd8e…`、identity `5de80e64…`、schedule `1678e073…`(出荷 int-12)。genesis `a27f8f44fe4d91a5…`。新しい規則を休眠で足しても、この 3 つは動いていない(`g14_close_gates::testnet12_identity_did_not_move` は PASS) | [docs/README.md](README.md)、[consolidated-validation](design/palw/consolidated-validation-2026-10-15.md) |
| 有効な flag day(`params.rs`) | DAA 750(13 fences、[ADR-0154](adr/0154-testnet-12-flag-day-daa-750.md))→ 1,300([ADR-0155](adr/0155-testnet-12-flag-day-daa-1300.md))→ 1,700(容量 ρ=10)→ 2,000(`palw_tir_v1`)→ 3,600(`palw_tir_fence2`)→ **5,300(int-11/12 一覧 `PALW_T12_INT11_FENCES_V1`)**、続いて ρ=100(5,395)・250(5,490)・1,000(5,585) | `params.rs` の `PALW_T12_POST_LAUNCH_FENCE*_DAA`、`PALW_T12_TIR_*_DAA`、`PALW_T12_INT11_*` |
| DAA 5,300 の一覧に入っているもの | RFC-0001 の decode constraint・prefix・tokenizer・adapter class、RFC-0003 `palw_gen_v1`、RFC-0004 `palw_improvement_v1`、RFC-0006 `palw_tir_shard_v1`、RFC-0007 の vertex・witness manifest・audit mesh・capped onboarding、class seating、[ADR-0165](adr/0165-the-floor-is-a-reserve-and-the-work-carries-the-clock.md)(floor reserve・real clock tick)・[ADR-0166](adr/0166-verifier-unavailability-is-not-producer-fraud.md)・[ADR-0170](adr/0170-the-seed-anchor-is-a-window-not-a-span.md)(anchor window)・[ADR-0167](adr/0167-the-x1000-capacity-package-a-fixed-per-daa-reward-budget-riders-and-a-lower-only-breaker.md)(×1000 容量) | `PALW_T12_INT11_FENCES_V1`(`params.rs`)、[adr-0162-0178-implementation-status](design/palw/adr-0162-0178-implementation-status-2026-10-15.md) |
| 休眠のままの一覧 | int-13 の 4 本(`palw_audit_1004_v1`・`palw_gen_range_twin_v1`・`palw_model_court_window`・`palw_receipt_spend_v4`)は `PALW_T12_INT13_DAA = None`。`PALW_T12_DECODE_RULES_DAA`・`PALW_T12_MODEL_COURT_WINDOW_DAA`・`PALW_T12_POST_LAUNCH_FENCE_V4_DAA` も `None` | `params.rs` |
| 現在の DAA | 約 10,750(2026-10-16 11:00 JST、約 21 DAA/h の実測ペース)。**稼働 fleet が実際にどの binary か、DAA がいくつかは `unknown — check` chain RPC と fleet の binary id** | release plan |
| DNS-BFT・validator | 旧 DNS 機構は出荷 ruleset の一部のまま(`PALW_T12_DNS_PARAMS`)。退役の fence `palw_dns_retirement` は全 preset で `None`。新しい規則は DNS に依存しない(release plan)。**finality はブロック数・blue score・DAA で数えず、finality 深さか `Final` の PALW anchor で判断する** | `params.rs`、[docs/README.md](README.md) |
| 次の fence | **DAA 14,000 では行わない(2026-10-16 ユーザー決定)。** 16,000 は目安で確定値ではない。G14 を全部武装したまま PALW の報酬・ブロック生成・Final が正常に進むことを、出荷 binary の drill で確かめてから fence の高さを決める。順序: 全部完了 → 一括検証 → 実ノードでの検証・測定 → 出荷 binary で fence を跨ぐ drill → ユーザー承認 → 配備 | [release plan](design/palw/t12-daa14000-release-plan-2026-10-15.md) の最終 3 節、[remaining-work §5](design/palw/remaining-work-2026-10-16.md) |

## 2. 機能別の現在地

読み方: **Activated が「しない」で Armable も「しない」の行は、今の testnet-12 には何も入っていません。** 「全 preset `None`」は 2026-10-16 時点の `params.rs` の確認結果です。

### 2.1 既に有効な RFC(testnet-12)

| 機能 | Decision | Implemented | Verified | Armable | Activated | 備考・出典 |
| --- | --- | --- | --- | --- | --- | --- |
| RFC-0002 PALW-TIR(IR・class 登録・seating) | RFC Draft(Phase A–C が Gate 1 通過 2026-09-28)。[ADR-0172](adr/0172-model-extensibility-uses-versioned-kernels-not-a-universal-vm.md): VM でなく versioned kernel | 実装済み。`palw_tir_v1`・`palw_tir_fence2`・class seating。IR class の Free Prompt lane(Phase H)は branch `integ/rfc02-ir-free-prompt` で実装 | 一部。focused run 2,481 件中 2,453 PASS。tir-lower の 4 件(three_way・corpus_v2・admission・quant_decode_pins)は旧 base でも失敗 | `palw_tir_only_v1`(mainnet step)はユーザー未決定 | **する(DAA 2,000 と 3,600、class seating は 5,300)。** `palw_tir_only_v1` は全 preset `None` | genesis モデルの変換 artifact は `target/` 削除で失われ、evidence の sha256 で再生成・照合が必要([remaining-work §6](design/palw/remaining-work-2026-10-16.md)) |
| RFC-0001/0003/0004(推論 surface・生成 class・モデル改善) | 各 RFC は Draft / Revised(0004 は 2026-10-09) | 実装済み(`palw_fp_*`・`palw_gen_v1`・`palw_improvement_v1`) | unknown — check [consolidated-validation](design/palw/consolidated-validation-2026-10-15.md)(個別の PASS 記録は無い) | — | **する(DAA 5,300)** | 一覧は §1 |
| RFC-0006 layer-sharded panels | Revised Draft(2026-10-10 後続改定、ADR-0177 の経済・配分・実装 gate は未完) | 実装済み。native typed inventory など後続の追加は休眠 | 生成 source の再現性は PASS(Step 0)。native 部分は unknown — check `rfc06-native-typed-inventory-source-2026-10-14.md` | `palw_tir_shard_v1` は有効。追加分は armable でない | **一部する:** `palw_tir_shard_v1` @5,300。後続の追加は `None` | RFC-0006 の Status update 2026-10-08 |
| RFC-0007 検出・認証 receipt・代数 check | Revised Draft(Part VI が post-commit の唯一の protocol) | Part I・II・IV.1・IV.2 は実装済み。Part VI の beacon に使う **selected-approval adapter は 2026-10-16 の最新 commit で実装**(PaidClaimV2 Final で割り当て quorum がある場合だけ event を出す) | adapter は未検証。**kernel route の beacon が lock できず約 45 試験が red**(`g14_kernel_route_e2e::{conformance,opv_bootstrap}`、core の `rfc0010_production_fold::paid_panel_sources…`)。旧 OPV fixture を adapter 前提へ移す作業が残る | いいえ | **一部する:** Part I・II・IV.1・IV.2 @5,300。Part VI の経路は休眠 | [remaining-work §1](design/palw/remaining-work-2026-10-16.md) |

### 2.2 休眠で実装された RFC(testnet-12 には入っていない)

| 機能 | Decision | Implemented | Verified | Armable | Activated | 備考・出典 |
| --- | --- | --- | --- | --- | --- | --- |
| RFC-0008 EXEC_TX / EXEC_SLICE(`palw_exec_payload_v2`) | Revised Draft / deferred(2026-10-10 後続改定)。[ADR-0168](adr/0168-an-execution-block-is-a-third-class-and-the-chain-reaches-it-through-an-anchor.md) は RFC-0008 v2 に置き換え | 実装済み・休眠。試験用 seam `exec_v2_test_admit_class_v1`(`cfg(test)` のみ) | 旧 branch `rfc8/x8r-review` @6246e7942 の上では verified(Lead 記録)。**現在の integ では unknown — check**。EXEC の試験 seam は G-K2 / GAP-81 として未解決 | いいえ。`PALW_EXEC_PAYLOAD_V2_ARMABLE = false`、validation が arm を拒否 | しない(全 preset `None`) | [activation-readiness-matrix](design/palw/activation-readiness-matrix.md) |
| RFC-0009 node-less 登録・remote claim(`palw_receipt_spend_v4`) | Revised Draft(node-less は維持) | `misaka/palw/remote` は reference 実装。receipt spend v4 は int-13 一覧で休眠 | reference の単体試験のみ。real node は無し(2026-10-08 の matrix)。`rfc9_v4_chain_e2e` offline miner が hash 依存で 1 件 red | いいえ(arming plan と drill が無い) | しない(`PALW_T12_INT13_DAA = None`) | [remaining-rfc-integration-matrix](design/palw/remaining-rfc-integration-matrix.md) |
| RFC-0010 permissionless Panel(`palw_permissionless_panel_v1`) | Revised Draft / deferred | 実装済み・休眠(production fold・shard draw・RPC/CLI) | **red あり:** core の `rfc0010_production_fold::paid_panel_sources…`(kernel-route beacon の件と同根) | いいえ。承認済みの production beacon が無く、validation が arm を拒否 | しない | matrix、consolidated-validation |
| RFC-0011 permissionless モデル onboarding・K2 route(`palw_probabilistic_constraints_v1`) | Revised Draft。固定 identity・条件付き G14 | 部分的・休眠(arm は拒否) | 同上(kernel route の約 45 試験が red)。2026-10-08 時点で `g14_` 試験 63 本 | いいえ | しない | [adr-0162-0178-implementation-status](design/palw/adr-0162-0178-implementation-status-2026-10-15.md) (ADR-0171 の行) |
| RFC-0012 PALW-only・DNS 退役(`palw_dns_retirement`) | RFC-0012 は Revised Draft。**[ADR-0180](adr/0180-palw-only-consensus-no-bft-validators.md) Accepted(ユーザー決定 2026-10-16)**: BFT の validator を使わない | 実装済み・休眠(zero-DNS matrix x0–x15) | **red:** rfc12 の 4 試験(旧 base `64699e960` でも失敗)。X12b の running 記録は unknown — check | いいえ(D1・D・W・operator/class cap・horizon は未決、wallet・runbook・drill が無い) | しない。旧 DNS 機構は出荷 ruleset に残っている | D1(最初の safe ≥ 5,400 DAA)は短縮も最終扱いもしない(ユーザー 2026-10-08) |
| RFC-0013 resource-bounded 道具 | Revised Draft(2026-10-10) | 既存道具の remediation と、提案段階の post-commit conformance record。RFC の Status 行は「runtime 実装・consensus 有効化なし」 | unknown — check [model-onboarding-implementation-matrix](design/palw/model-onboarding-implementation-matrix.md) | fence なし | 該当なし | — |
| RFC-0014 Panel 非依存の fraud prosecution(G14 の主設計) | Revised Draft | → G14 の行(§2.4) | → §2.4 | → §2.4 | → §2.4 | — |
| RFC-0015 Panel=0(`palw_panel_free_v1`) | Revised Draft / **deferred**(2026-10-12 N5: Panel=0 は有効化しない) | 実装済み・休眠 | 未検証(記録に PASS なし) | いいえ。validation は arm を拒否(認証付き activation gate 記録の pin 一覧が空) | しない | [rfc-0015-implementation-status](design/palw/rfc-0015-implementation-status-2026-10-15.md) |
| RFC-0016 bond 加重のブロック機会・弾力的 Final 容量(V+) | **Accepted design**(V+、ユーザー判断 2026-10-15)。値は測定待ち。7 日平均・hold は両 mode で 1 日・slash 損失 > 不正利得に改訂(2026-10-16) | 実装済み・休眠(`palw_model_block_rights_v1`・`palw_elastic_final_capacity_v1`)。値は placeholder(`PALW_RFC0016_VALUES_V1`) | 一部。consolidated-validation で red は 1 件(`rfc0016_e2e::t12_rfc0016_zero_rights_heartbeat_keeps_the_cadence`、floor-class harness で設計上 exempt)。その他の PASS は項目別の記録が無く unknown — check | いいえ。`PALW_RFC0016_MEASURED_RECORDS_V1` が空で、隔離した calibration network 以外の arm は拒否 | しない(全 preset `None`) | [rfc-0016-implementation-status](design/palw/rfc-0016-implementation-status-2026-10-15.md) |
| RFC-0017 / [ADR-0179](adr/0179-model-tiers-genesis-annual-1-0-registered-0-3.md) モデル階層(genesis・年次フォーク 1.0、登録 0.3) | **Accepted(ユーザー決定 2026-10-15)。凍結前提**([PRINCIPLES §9](PRINCIPLES.md))。モデル単位の審査は廃止。claim 単位の検証は全 tier で維持 | 実装済み・休眠(`palw_model_tier_v1`・`palw_model_eligibility_v1`) | unknown — check 検証した head に `integ/rfc0017-model-eligibility` の内容が入っているか。この行専用の PASS 記録は無い(record は「build・試験は未実施」) | いいえ | しない(全 preset `None`) | [user-decisions-2026-10-15-adr-rfc16-rfc17 §7b・§7c](design/palw/user-decisions-2026-10-15-adr-rfc16-rfc17.md) |
| RFC-0018 DAG 並行 multi-model 合意・特権なき Panel anchor・rule E v2(`palw_dag_sibling_attribution_v1` ほか) | Draft。ユーザー方針として採用 | 実装済み・休眠 | **red:** `rfc0018_dag_siblings` ×3(lane source が build されておらず、armed fence を binary が拒否)。その他は未実行 | いいえ | しない | [rfc-0018-implementation-status](design/palw/rfc-0018-implementation-status-2026-10-15.md) |
| **Gate C**(`Final` と `SecurityCertified` の分離、`palw_independently_work_certified_v1`) | RFC-0018 で採用(ユーザー方針)。**U-1〜U-8 はユーザー判断待ち**([remaining-work §7](design/palw/remaining-work-2026-10-16.md)) | 実装済み・休眠 | unknown — check。300M 閉鎖モデル攻撃試験(`attack_tests.rs`)は未実行 | **いいえ(G-04)。** m・k の測定と判断が待ち | しない | 未認証 claim は frontier・settlement・rule E・anchor に寄与しない(PRINCIPLES) |
| RFC-0019 年次モデル追加フォーク | Draft(ユーザー採用 2026-10-15)。**U-1 / U-3 / U-5 がユーザー判断待ち** | 休眠実装(新 fence なし。catalog は `palw_model_tier_v1` の値)、RPC `getPalwModelRegistry` wire v4、release 道具 | release 道具 2 本の Python self-test と署名往復は lane が実行。Rust は未検証。drill の R-7 / R-8 は未実施 | いいえ | しない | [rfc-0019](rfc/0019-annual-model-upgrade.md) |
| RFC-0020 長文脈の較正・変換メモリ | Draft(2026-10-16) | 道具のみ(consensus surface なし)。RAM 計上・carry・lazy 重み・disk 退避は実装、tiled attention と KV spill は未実装 | 未検証(cargo を実行していない) | 該当なし | 該当なし(道具) | genesis モデル #2 は fit の長文チェックで停止、#8 の streaming 変換は未実行 ([remaining-work §6](design/palw/remaining-work-2026-10-16.md)) |

### 2.3 ADR の経済・登録規則

| 機能 | Decision | Implemented | Verified | Armable | Activated | 備考・出典 |
| --- | --- | --- | --- | --- | --- | --- |
| [ADR-0175](adr/0175-registered-models-are-permanently-immutable.md) 登録モデルは永久不変(`palw_model_immutable_v1`) | Accepted(2026-10-09) | 実装済み・休眠(拒否コードは完結) | 2026-10-10 の[検証記録](adr/evidence/0175-immutable-model-registration-2026-10-10.md)は文書側。コード試験の PASS は unknown — check | いいえ(既存 network の有効化高さは設定していない) | しない | — |
| [ADR-0176](adr/0176-bond-bounds-palw-production-rewards-and-final-weight.md) bond が claim 容量・ブロック・報酬・Final weight の上限を決める | **Accepted(設計前提)**。2026-10-16 に PRINCIPLES §10 を追加 | 実装済み・休眠(mode 別 hold・日ごとの cap・burst と同時 liability の cap・`palw_bond_formula_v2`・rule E への接続) | unknown — check。記録は「build・試験は未実施」 | いいえ。値は user 判断と測定待ち(Q/B/R/F、σ、k) | しない | 実装状況は [adr-0162-0178-implementation-status](design/palw/adr-0162-0178-implementation-status-2026-10-15.md)。×1000 容量(ADR-0167)は別で、t12 で有効 |
| [ADR-0177](adr/0177-model-bond-allocation-without-availability-consensus.md) model-bond 配分(α = 1.5、miner+verifier の 30 日平均) | Accepted direction(2026-10-10 後続改定、10-14 の決定を反映) | 実装済み・休眠(V3 allocation policy) | unknown — check | いいえ。「公開参加の経済優位」は倍率式と敵対的評価で立証する未完の目標 | しない | — |
| [ADR-0178](adr/0178-fork-choice-heals-a-partition-on-bonded-participation.md) rule E(分断の自己修復、`palw_fork_choice_rule_e_v1`) | Accepted(ユーザーが rule E を採用 2026-10-09) | 実装済み・休眠(arm は拒否) | unknown — check | いいえ(G-11: 測定値と判断待ち) | しない | — |
| ADR-0179 | → 2.2 の RFC-0017 の行 | | | | | |

### 2.4 G14(「1 つの正直な検証者で客観的 conviction」)

**主張(ユーザー、PRINCIPLES §10):** producer と Panel 全員が共謀しても、Panel 外の public bonded verifier が 1 人いれば、公開 material だけから客観的に有罪まで進められる。**この主張は、まだ証明されていない**([g14-completeness-proof §0 と §6](design/palw/g14-completeness-proof-2026-10-16.md)。`f244ed92d` の source では成り立たない)。付録 B・C・D と §7・§8・§9 で gap を一部閉じたが、node 水準の試験と drill が無い。

| lane | Decision | Implemented | Verified | Armable | Activated | 備考 |
| --- | --- | --- | --- | --- | --- | --- |
| G14 全体 | 最優先(ユーザー 2026-10-10)。2026-10-16 の定義: 正しい Panel が 1 つあれば計算との不一致を客観的に証明し、slash・ブロック生成の拒否・報酬の拒否まで進める | 部分的(下の各 lane) | **主張は未証明。** 未解決 gap: G-07(court の 42 枠)・G-13 の効率(Panel 担保の Active/Resolved 分離)・TIR frame の価格と期限・G-N1/G-N2/G-K2・G-K3・G-11・G-12・G-04・X3 | いいえ | しない。**報酬を出しているのは G14 を満たさない legacy Panel route** | drill 18 場面(D-01〜D-18)の driver は未完([remaining-work §2](design/palw/remaining-work-2026-10-16.md)) |
| TIR 公開 frame(`palw_tir_public_conviction_v1`、tag 170/171) | 採用。frame 必須化は G-03 | 実装済み・休眠。frame の allocation の u64 溢れは修正(`integ/g14-tir-alloc`) | **fold 水準のみ PASS**(`palw_tir_public` 69 本など)。node E2E と drill は無し。T-N1(正直な IR FP claim が Final・支払いまで届く node 試験)は書けていない | **いいえ。** t12 の現行価格では賃料 2e7 MSK 以上・告発を開けるのが 0.9e6 DAA 後・Final が 6e7 DAA 後で実用にならない。arm-readiness record(`PALW_G14_ARMING_RECORDS_V1`)は空 | しない | WIP: `integ/g14-tir-pricing`(`61ab3a9a8`、harness の途中) |
| legacy LG14-A / LG14-B(`palw_legacy_public_filer_v1`・`palw_legacy_held_da_v2`) | 採用 | 実装済み・休眠 | **fold / E2E 水準の PASS:** `lg14a_legacy_dispute_fold` 9、`palw_legacy_public_filer_v1` 12、`lg14a_legacy_filer_e2e` 14、`g14_close_legacy` 8 | いいえ。court の 42 枠(G-07)と Panel 担保(G-13)が未解決 | しない | WIP: `integ/g14-court-capacity`(`5b64cac9c`) |
| kernel route / EXEC / native(v11・v29・v39) | 採用 | 実装済み・休眠 | **red:** kernel-route の beacon が lock できず約 45 試験。RFC-0007 の adapter(最新 commit)で fixture を移す必要がある | いいえ(arm は拒否) | しない | gap: G-N1(v11/v29 の告発枠)、G-N2(v39 mining の Final 後の告発、veto は RFC-0018 U-6)、G-K2 |
| option (b) `palw_g14_route_required_v1`(G-01) | **ユーザー決定 2026-10-16:** G14 経路が武装されていない family の新規 claim は報酬・weight・consensus credit を得ない。正直な seat 1 人でも裁けない Free Prompt の選択・constraint・prefix・tokenizer の job は受理時に拒否(G-05) | 実装済み・休眠 | fold 水準 PASS(T-N8 `legacy_route_claim_without_a_g14_path_earns_nothing_past_14000`、T-N10) | **いいえ。** 武装すると、G14 経路を持たない family の報酬が止まる。G14 経路を実用にしてから | しない。DAA 14,000 の entry は予定から外れた(fence 延期) | 将来の network では G14 は genesis から有効(fence 0 で動く形で実装を進める) |
| court の全体容量・Panel 担保(G-07・G-13) | ユーザー方針 2026-10-16(別々の問題として扱う。Final+1,000 の無条件解放へは戻さない) | 途中(WIP branch、未 build・未試験) | 無し | いいえ | しない | 42 枠飽和+継続的な埋め直し攻撃の試験、占有時間の課金、Active/Resolved 分離 |

### 2.5 リリースと検証の全体

| 項目 | 現在地 |
| --- | --- |
| 一括検証(`integ/validation` `37cf3fa89`) | green: check ladder 20 段、t12 identity、G14 対象 150 試験。red: kaspad service E2E 16 件、RFC-0018 sibling 3 件、`rfc9_v4`、rfc0016 の 1 件、rfc12 ×4、`t12_a2u_mixed` ×2、SDK ×3、tir-lower ×4、base0 ×3(一部は旧 base でも失敗)。clippy は警告 557 件で green でない。**最新の integ head では全 suite を再実行していない** |
| 新 fence の武装 | **しない。** 報酬・consensus weight・Panel=0 の新 fence は、経済が閉じるまで武装しない(ユーザー判定 2026-10-09)。今日 armable な fence は無い |
| ユーザー判断待ち | RFC-0019 U-1/U-3/U-5、G14 の Lead 既定値(owner cap 8、V3/V4 拒否、idle grace 256 DAA)、legacySettlementWindowDaa と reserve drip、RFC-0018 U-1〜U-8、#3 のモデル選定、Downloads 削除の A/B/C 区分([remaining-work §7](design/palw/remaining-work-2026-10-16.md)) |

## 3. この表の限界

- **Verified と Armable の多くは `unknown — check …` です。** 項目別の PASS 記録が無い項目を、PASS と書いていません。
- 「しない / しません」は 2026-10-16 時点の `params.rs` の確認に基づきます。**稼働 fleet の binary と chain の状態は、この表では確認していません。**
- 古い記録(2026-10-08 の matrix など)に基づくセルは、そう書いてあります。新しい commit で変わっている可能性があります。
- 暗号学的な計算証明は MISAKA の検証方式ではありません([PRINCIPLES §8](PRINCIPLES.md))。ここでの「検証」は、試験・drill・確率的検出・公開証拠による局所化・決定論的な裁定を指します。
