# RFC-0018 実装状況 — 2026-10-15

[RFC-0018](../../rfc/0018-dag-parallel-multi-model-consensus.md) の各項目を、規則・fence・コード位置・試験sourceへ対応させる。branch `integ/rfc0018-dag`。**build・test・実node試験はいずれも未実施であり、どの行もPASSではない。** 「実装(休眠)」は規則のコードが書かれ、fenceの下で既定では動かないこと、「試験source」は試験が書かれて未実行であることを意味する。

## 1. ユーザーの完了定義(6項目)

| # | 項目 | 状態 | 規則(file:line) | fence | 試験 |
| --- | --- | --- | --- | --- | --- |
| 1 | `Final`と`SecurityCertified`を別状態にし、合意creditは認証済みだけ。FPとAttemptはfrontier・settlementで一つの規則 | 実装(休眠) | 述語 `consensus/core/src/palw_rfc0018_v1.rs:147`、単一規則 `consensus/core/src/palw_security_certification_v1.rs:367`、frontier `consensus/core/src/palw_state_v2.rs:14696-14712`、settlement `consensus/core/src/palw_settlement_v1.rs:89-104`、provisional `palw_rfc0018_v1.rs:166-174`、加算 `consensus/core/src/palw_security_certification_fold_v1.rs:196`、報酬保留 `:91`、全site一覧はRFC §13.3 | `palw_independently_work_certified_v1` | `palw_rfc0018_v1/tests.rs`(identity、`Final ≠ SecurityCertified`、単一規則とFP-only)、`palw_rfc0018_v1/attack_tests.rs` |
| 2 | 認証述語: commit-then-draw、global pool、m-of-k、期限、G14 window、閉鎖モデルは認証されない、試行の有界化と計数、前提の明記 | 実装(休眠) | commit `palw_security_certification_fold_v1.rs:45`、母集団 `palw_security_certification_v1.rs:280`、抽選 `:296`、verdict `:320`、承認 `palw_security_certification_fold_v1.rs:111`、sweep `:234`、計数(class row、epoch上限)fold `:234-330`、前提文言 `palw_security_certification_v1.rs:50`、module doc冒頭 | 同上(+`palw_model_eligibility_v1`・`palw_probabilistic_constraints_v1`を同じ高さ以下に要する、`palw_rfc0018_v1.rs:520`) | verdict・母集団・policy試験(`palw_rfc0018_v1/tests.rs`、`palw_security_certification_v1.rs`内) |
| 3 | rule E v2: 資本加重・分割耐性・認証済みのみ・順序修正 | 実装(休眠) | accumulator `palw_rfc0018_v1.rs:219`、`palw_fork_choice_rule_e_v1.rs:520`、record(未認証は`attempt = false`)`:357-383`、order/decide v2 `:794`/`:804`、creditなし同士のtie `:828`、processor `consensus/src/pipeline/virtual_processor/palw_rule_e.rs:167-226`、IBD `palw_fork_choice_rule_e_v1.rs`(`palw_rule_e_ibd_commit_v1`) | `palw_rule_e_participation_v2`(rule E v1を同じ高さ以下に要する) | `palw_rfc0018_v1/tests.rs`(分割中立、除外、v1同値)、`attack_tests.rs`(T-0d/e) |
| 4 | liveness・時計・pruning・RPCの正直な表示 | 実装(休眠) | DAG・取引・heartbeat・DAAは変更なし(creditだけがGate Cを読む)、pruning progress `palw_security_certification_v1.rs:379` と `consensus/src/pipeline/virtual_processor/processor/palw.rs:719-750`、RPC `rpc/core/src/model/message/palw_operator_reads.rs`(`uncertified_final_anchors`、`security_certification_premise`、wire v4)、`rpc/service/src/service.rs:2120-2128`、native settlement `consensus/core/src/palw_native_settlement_v1.rs:161`/`:338`/`:404` | `palw_independently_work_certified_v1` | `attack_tests.rs`(pruning、settlement)、driver `attack`(pruning progress) |
| 5 | 文書改定(追記) | 済 | PRINCIPLES(§0の改定節)、ADR-0173/0176/0177/0178、RFC-0012/0016/0017の末尾「ユーザー方針 2026-10-15 による改定」 | — | — |
| 6 | 最上位受入試験「300M閉鎖モデル/自前Panel攻撃」 | 試験source(Rust)+driver | `consensus/core/src/palw_rfc0018_v1/attack_tests.rs`、`scripts/drills/rfc0018_dag_drill.py attack` | 全fence | 未実行 |

## 2. その他の設計変更

| 項目 | 状態 | file:line | fence | 試験 |
| --- | --- | --- | --- | --- |
| canonical sibling順(モデル間round robin) | 実装(休眠) | `palw_rfc0018_v1.rs:283`、fold `palw_state_v2.rs:14481-14496` | `palw_dag_sibling_attribution_v1` | `palw_rfc0018_v1/tests.rs`、E2E source `consensus/src/pipeline/virtual_processor/tests/rfc0018_dag_siblings.rs`、driver `siblings` |
| permissionless Panel anchor | 実装(休眠) | seed `palw_rfc0018_v1.rs:363`、reach `:391`、processor `processor/palw.rs:6531-6640` | `palw_panel_anchor_permissionless_v1` | `palw_rfc0018_v1/tests.rs`(T-2) |
| DAG幅schedule | 仕様とstub(readerなし) | `palw_rfc0018_v1.rs:407-475` | `palw_dag_width_schedule_v1` | `palw_rfc0018_v1/tests.rs`(T-4) |
| fence配管 | 実装 | `config/params.rs:3186-3206`、`config/params/identity.rs`、`fork_id_v1.rs`、`config/params/validation.rs`、mirror `palw_state_v2/state_params.rs:689-697` | — | `palw_rfc0018_v1/tests.rs`(休眠、Some-only、arm拒否) |
| aux 136 scanner | 生成 | `palw_native_resource_v1/wire_kernel_aux.rs`、`wire_generated.rs`、`generated.rs` | — | — |

## 2b. 2026-10-15 追加(残りgapの解消)

| 項目 | 状態 | file | 試験 |
| --- | --- | --- | --- |
| frontierの巻き戻し(conviction後に再計算、floor、reorg/IBD一貫) | 実装(休眠、Gate C fence) | `palw_security_certification_v1.rs`(`palw_gate_c_frontier_v1`、`palw_frontier_rollback_bound_v1`、floor row `'F'`)、fold `ensure_frontier_floor_v1`/`note_retirement_frontier_floor_v1`、`palw_state_v2.rs`の frontier step、`transition_builder.rs` `retire_claim` | `palw_rfc0018_v1/tests.rs` `rfc18_frontier_rolls_back_on_conviction_and_pruning_never_passes_a_rollback_target` |
| pruningの巻き戻し上限 | 実装(休眠) | `processor/palw.rs` `palw_pruning_point_allowed_v2` | 同上 |
| permissionless anchorの厳密化(確定fold出力、claimごとのproducer除外) | 実装(休眠、anchor fence) | `palw_rfc0018_v1.rs`(`palw_permissionless_contributions_from_state_v1`、`palw_permissionless_anchors_claim_v1`)、`processor/palw.rs`(walk `_for_v1`、binding導出)、`palw_state_v2.rs` step 4c | `rfc18_anchor_seed_excludes_the_producer…`、`rfc18_anchor_contributions_are_committed_fold_output` |
| leaf v2 / gRPC / RPCへの配線 | 実装 | record `certification`、`palw_rule_e_certification_summary_v1`、`rpc.proto`(settlement 13/14、claim row 39)、`convert/message.rs`、claim row wire v6 | `rfc18_a_client_reads_gate_c_from_the_window` |
| siblingの順序非依存(上限・seed anchor) | 実装(休眠、sibling fence) | mergeset内はcanonical順、seed anchorはspan内最小priority(`record_round_seed_anchor_of`)。selected parentのsiblingはGHOSTDAGの決定的規則で先に採用(RFC §4.1 I-S6) | E2E `rfc0018_dag_siblings.rs`(到着順全6通りでstate root・seed anchor一致) |

## 3. 完了できなかった点と理由

* **build・試験の実行:** 本laneの規則(cargo build/check/test禁止)により未実施。validation agentが実行する。
* **anchor:** GAP-A1/A2は解消。認識は採用の1 chain block後になる。
* **GAP-C3:** 認証前のfree-prompt spendのcoinbase carveは止めていない(weightは止める)。processorのproducer pre-checkまで変える必要がある。
* **GAP-C4:** 保留した発行由来報酬は認証時にproducerへ1 legで払う。Panel share・buyback・vestingへの分配は未設計(U-2)。
* **GAP-R4:** 2026-10-15に解消(rule-E leaf v3: `palw_fork_choice_rule_e_leaf_v3.rs`、node `palw_committed_root_v1` → `palw_committed_state_root_v2`、v39 anchor・L2 client・carriage RPCの共通検証、native roots chain。試験 `palw_fork_choice_rule_e_leaf_v3/tests.rs`)。
* **selected parentのsibling:** GHOSTDAGがselected parentに選んだsiblingは自分のblockで先に採用される。決定的(blue work、block hash)で到着順に依存しないが、mergeset内の他siblingと同じ土俵では比べない。
* **DAG幅のreader:** live経路(header処理、GHOSTDAG、difficulty、pruning)に触れるため実装しない(RFC §8.4)。
* **DNS-BFTの物理除去:** 別lane(`integ/rfc12-dns-removal`)。本branchには取り込まない。
