# PALW ConsensusV2 — mainnet readiness audit (2026-08-22)

**Method.** 88 agents: 5 re-verifying the standing gate ledger, 10 hunting by dimension
(emission, bond enforcement, consensus safety, liveness, cryptography, adjudication, DoS,
IBD/pruning, activation, the mainnet preset), then every surviving finding refuted from three
independent angles — *you misread the code*, *it is unreachable*, *the consequence is wrong* —
with refutation as the default. 88 raw findings, 87 after dedup, 24 verified in depth,
**14 survived**, 10 refuted. Branch `palw-base0-depth` at `0ba34ed2`.

**Scope.** Mainnet today is PALW-free (`palw_consensus_mode: Disabled`, every fence `None`,
`pow_palw_activation: never()`). Nothing below is a live mainnet incident. The question asked
was: *if mainnet were switched to ConsensusV2, what breaks, what is exploitable, and what mints
value that should not exist?*

---

## Fixed since the audit ran (2026-08-22)

Phase 0 — the items the audit called code-sized — is **done**. Each fix carries a regression test
that was checked against the unfixed code, because a gate that does not fire is worse than no gate:
one of these tests passed on first writing and had to be rebuilt until it reproduced the failure.

| audit item | fix | the test, against the OLD code |
|---|---|---|
| 2 — one ~5 KB class registration halts every node forever | `611edef0` — bound `n_ctx × layer_count`, validate before enumerating in both the sibling and the admission gate, test the leaf cap inside the loops | timed: refusal must take under 200 ms, because a regression hangs rather than fails |
| 5 — a stranger charges claims to anybody's bond | `cbce765c` — the object carries the signing key, and the transition compares it to the bond's, as the attempt lane has since P0-2 | a commitment signed by a foreign key is refused, and the victim's reserved exposure stays 0 |
| 3 — a court session outlives its claim and halts the chain | `cbce765c` — sessions retire with their claim; a sweep that meets an orphan drops it | `block at daa 610 must apply, got MissingClaim(...)` |
| 10 — merged blues paid without lottery, budget or dedup | `410bbd90` — the payment predicate now asks the two questions the parent state can answer: did it win its class lottery, and has this identity already been paid here | — |
| 11 — the gossip map grows forever on unauthenticated claim ids | `410bbd90` — the map became a derived index of the digest FIFO | `the map held 12288 claims against a 4096-digest window` |

**Items 1, 4, 6, 7, 8, 9 are untouched.** They are the `[A]` builds — pruning-point carriage, the
court responder, bond registration and retirement, and the adjudication layer's three bindings —
and the audit's own phasing says they are weeks to months, not an afternoon. The verdict below is
therefore unchanged: **NO-GO**.

---

**結論: NO-GO。** ConsensusV2 を mainnet で有効化すると、攻撃者がいなくても新規ノードが1台も参加できず既存ノードが壊れ、攻撃者がいれば1トランザクションで全ノードが永久停止し、bond は約15日で全部ゼロにできる。しかも「不正は court で slash される」という前提そのものが実運用パスで一度も成立しない。値を載せられる状態から複数の**build**（コード修正ではなく未実装機能）ぶん離れている。

以下、**先に詰まる順**。`[A]`=mainnet が値を運ぶ前に作らねばならないもの、`[B]`=欠陥だがコードサイズ、`[C]`=欠陥ではないが明言すべき事実。

---

## 1. `[A]` pruning-point の PALW carriage を誰も配れない — 攻撃者不要、初日から詰む
`consensus/src/pipeline/virtual_processor/processor.rs:10073`
`pruning_point_palw_state` は singleton tip 行が pruning point と一致する時だけ carriage を返すが、稼働中のノードの tip は常に sink（`processor.rs:1432` が `diff_point`=sink を書く）なので、この等号は永久に偽。DNS/PoS overlay にある `capture_overlay_snapshot` 相当が PALW には無い。
- 帰結A: headers-proof IBD は `protocol/flows/src/ibd/flow.rs:2331` で必ず hard abort → 新規ノードは genesis からの全再生（全ヘッダが LLM 推論検証）以外に参加手段が無い。
- 帰結B: `sync_new_utxo_set` は `async_clear_pruning_utxo_set()` を**先に**実行してから `sync_pruning_point_palw_state`（flow.rs:2106）を呼ぶので、既存の健全ノードが `is_utxo_stable=false` のまま永久ループに落ちる。datadir 全消し以外の復旧が無い。
- 今日から着手できる修正: pruning point 前進時（`advance_pruning_point_if_possible`）に、既存の `palw_state_walk::walk_chain_path` で tip から巻き戻して pruning point 時点の state を materialize し、専用 `pruning_point_palw_snapshot` に保存。`pruning_point_palw_state` はそちらを配る。実 exporter/importer を2ノードで跨がせる統合テストを必ず同時に作る（現行 `tests.rs:1728` は tip を配って selected parent で import しており、実経路を一度も通っていない）。

## 2. `[B]` クラス登録の `n_ctx`/`layer_count` が無制限ループを駆動 — 1件の ~5KB tx で全ノード永久停止
`consensus/core/src/palw_step.rs:731`
`worst_case_step_leaf_count_v1` は `n_ctx-1` 回 × `layer_count` 回を回してから `PALW_STEP_MAX_LEAVES` を判定する（判定は :739、ループの**後**、しかも `saturating_add` で早期脱出なし）。`validate_shape` はこの経路で呼ばれず、そもそも両フィールドを縛っていない。0x4b の `ClassRegistered{admission: Some(_)}` は ride 可（`palw_lifecycle_objects_v2.rs:135`）で、`processor.rs:4241` から全 chain-candidate block について走る。`n_ctx=0xFFFF_FFFF, layer_count=65535` で ~2.8e14 反復。ブロックは isolation validation を通って保存済みなので再起動でも同期でも再現し、hard fork 以外に復旧が無い。
- 修正: `validate_shape` に両フィールドの上限を入れ、`verify_class_admission_v2` の**先頭**（`shape_profile_id` より前）で呼ぶ。ループは `total > PALW_STEP_MAX_LEAVES` で即 break。恒久的には閉じた式で計算（leaves/position は kv_len の1次式）。1日仕事。

## 3. `[B]` claim 退役後も court session が生き残り、次の sweep が全ブロックで失敗 — 永久停止
`consensus/core/src/palw_state_v2.rs:2591`
`retire_claim`（:2364-2377）は claim と panel を消すが `court_sessions`/`court_deadlines` を消さない。孤児 session の backstop が発火すると `sweep_court_deadlines` が `claims.get(...).ok_or(MissingClaim)?` で transition ごと `Err`。`apply_palw_transition_v3` は `sweep_deadlines` → `sweep_court_deadlines` の順（:2446-2447）なので、claim を退役させたブロック自身が孤児を踏む。RC は `CLAIM_RETIREMENT = WINDOW_COURT = 2400`（`palw_fp_devnet_v3.rs:171,49`）なので DAA ジャンプすら不要。エラーは (parent state, daa) の純関数なので全ノードが同一に失格判定 → 以降の全ブロックが失格、復旧オブジェクトは存在しない。
- 修正: claim が terminal になる時点（`void_claim`/`finalize_claim`、遅くとも `retire_claim`）で当該 claim の全 session を `write_court(None)`。`sweep_court_deadlines` は claim 不在なら drop（エラーにしない）。`assert_*_consistency` に「全 session は生きた claim を指す」不変条件を追加（現行の carriage check :1736-1740 は index と session の整合しか見ていない）。

## 4. `[A]` court は無料で開けて、応答するソフトウェアがこのツリーに存在しない — 全 claim が無条件に有罪化できる
`consensus/core/src/palw_state_v2.rs:2615`
`validate_court_opened_v2`（`palw_court_v2.rs:164-213`）は challenger 側に何も予約させない（bond の存在だけ確認、`Active` すら見ない）。challenger 敗北側の全経路は `rearm_after_challenger_side_close` のみで `slash_bond` は一度も呼ばれない。一方 responder の沈黙は `declare_no_show`→`void_and_slash(..., CourtFraud)`（:2615-2621）で producer の bond を debit。そして `CourtDisclosed` を構築するコードは**どこにも無い**（定義/遷移/検証の3ファイルのみ、`kaspad/src/palw_panel.rs:388-389` は `ReceiptLicensed`/`ProducerDefaulted` だけを出す）。`COURT_TURN_DEADLINE=60` は live。
- 実害: bond を1つ持つ誰でも、1ブロック1txで 61 DAA 後に producer を有罪化。RC 定数で 853,358,000 ÷ 79,000 ≒ 10,802 ブロック（約15日）で1 bond がゼロ→`ExposureCeilingExceeded` で永久に採掘不能、bond の補充も新規登録も不可（項目6参照）なのでチェーンが終わる。
- 修正（3つ全部必要）: (a) `CourtOpened` で challenger 自身の exposure に担保を予約し、`ChallengerDefeated`/rung 無応答/backstop で没収する。(b) responder 実装が無い間は開幕 rung の沈黙を fraud として扱わない（`turn_deadline_daa < window_court` を無効化するか、無 slash で session を閉じる）。(c) court responder（`CourtDisclosed`/`CourtVerdictPosted` の構築と送信）を実装し、ConsensusV2 でブロックを作るノードがそれを持つことを起動時に assert する（`daemon.rs` の evm feature と同じやり方）。

## 5. `[A→B]` free-prompt commitment が他人の bond に紐づく — 署名は提出者が選んだ鍵で検証される
`consensus/core/src/palw_fp_objects_v3.rs:99` / `consensus/core/src/palw_state_v2.rs:3406`
**（元の findings 5 と 6 は同一欠陥の別角度。統合した。）** `validate_signature_v3` は payload 内の `commitment.job.executor_pubkey` で検証する（`palw_freeprompt_v3.rs:743-748`）だけで、その鍵が名指しされた bond の鍵かを誰も見ない。acceptance 側は `Obj::FreePromptCommitted { .. } => {}`（`processor.rs:4323`、確認済み）、transition 側は bond の存在と Retiring/Frozen/quanta だけ（`palw_state_v2.rs:3406-3418`）。attempt レーンには存在する `bond.pubkey != attempt.executor_pubkey`（`palw_admission_v2.rs:155`）がここだけ欠けている。bond outpoint は `params.rs:2386-2414` の公開定数。
- 実害: 無担保の第三者が1txで被害者の bond に claim を作れる。チェーンが自動で panel を derive（`processor.rs:4551-4581`）→ material が無いので `Unavailable`→`ProducerDefaulted`→`void_and_slash` が**被害者の**担保を焼く。または `ReceiptTimeout` で panel 5席全部が焼ける（`palw_state_v2.rs:3042`）。同時に被害者の exposure ceiling を埋めて採掘不能にできる。
- 修正: `PalwConsensusObjectV2::FreePromptCommitted`（`palw_state_v2.rs:1213-1228`）に `executor_pubkey` を持たせ（現状オブジェクトに鍵が無く、下流でチェックが**表現不能**）、`apply_object` で `bond.pubkey` と比較。加えて `processor.rs:4323` を空アームから実アームにして acceptance でも state から bond を引いて再検証。オブジェクト形が変わる＝ruleset id が変わるので t12 再 mint が必要（[C]参照）。

## 6. `[A]` bond の登録も退役もできない — 担保は永久ロック、生産者/panel 集合は genesis で凍結
`consensus/core/src/palw_lifecycle_objects_v2.rs:118`（および :139）
0x4b band が `BondRetireRequested` と `BondRegistered` を拒否し、acceptance でも再度拒否（`processor.rs:4312`）。`BondRetireRequested` は `PalwBondStatusV2::Retiring` の**唯一の**書き手（`palw_state_v2.rs:3123-3131`）で、`palw_bond_collateral_is_locked_v2`（:606-615）は `Active` に対して無条件 true。よって全 genesis bond は永久 Active、担保 outpoint は永久 unspendable。C-08 の burn 義務（`palw_bond_burn_obligation_v2`, :634）は解放済み bond の spend でしか履行されないので、slash された sompi は焼かれず凍るだけ。
- 実害: 出資が没収に化ける（stake ではない）。validator 集合が永久閉鎖。上の項目4・5で bond が枯れても補充手段が無い。
- 修正: 両オブジェクトを、コード自身が要求と書いている認可を持たせて再許可する。`BondRetireRequested` には bond 鍵に対する所有者 ML-DSA-87 署名（`ClassRegistered` が `processor.rs:4258-4276` で既にやっている分割）。`BondRegistered` には `verify_palw_genesis_v2`（`palw_genesis_v2.rs:88-96`）と同じ方法で、宣言された担保が実在の未使用 outpoint であることの証明。

## 7. `[A]` court close の bind が実物と食い違う — BASE-0 の実 claim では有罪判定に一度も到達しない
`consensus/core/src/palw_court_v2.rs:369`（確認済み: Arithmetic/DecodeToken の両アームが先頭で `check_arithmetic_close_binding(claim.trace_root, ...step_merkle_root)`）
producer は `kaspad/src/palw_producer.rs:365` で `trace_root = run.trace_root` を入れ、その実体は `misaka-palw-base0/src/produce.rs:267` の `base0_logits_trace_root_v1`（logits の平坦ハッシュ）で、step leg の Merkle root ではない。`palw_state_v2.rs:3577` がそれをそのまま claim にコピー。つまり実運用の close は証拠を読む前に必ず `TraceRootMismatch`。赤くならない理由も特定済み: `palw_court_v2.rs:1020` のテストが `env.attempt.trace_root = binding.step_merkle_root` と**本番と逆の代入を自分で作っている**。
- 実害: 嘘の実行を出した producer を構造上一度も有罪にできない。bond は名目だけの担保になり、exposure ceiling も Sybil 上限も fork-choice weight も無根拠になる。
- 修正: `trace_root` の意味を1つに決めて両側同時に直す。(a) court 側を正とし `Base0ExecutionV1` に `step_merkle_root` を出させて producer.rs:365 を差し替える（logits root は binding の `full_logits_trace_root` に既にある）、または (b) producer 側を正とし close の pin を `binding.full_logits_trace_root` との比較に変える。併せて :1007 のテストを、本物の `base0_execute_for_attempt_v1` の attempt から claim を立てる形に書き直す。

## 8. `[A]` close が dispute に紐づいていない — 執行者が自分で選んだ正しい1ステップを再計算して自分を無罪にできる
`consensus/core/src/palw_court_v2.rs:366`（確認済み: `let (_session, claim) = ...` で session を捨てている）
どちらのアームも `session.ladder` を読まない。refute された leaf index と `session.ladder.terminal_index()` の一致も、ladder が `Terminal` に達していることも、closer が challenger であることも要求しない。`CourtClosed` には署名も権限フィールドも無い（`palw_state_v2.rs:1164`）。`map_refutation_outcome` は `NoFaultFound` を `ChallengerDefeated` に変える（:440-450）。トレースは最大 2^22 leaf で1枚だけ嘘なら十分なので、執行者は常に正しい leaf を選んで無罪を買える。1件の tx で session が消え、claim は Final へ再武装する。
- 併走する同族欠陥 `[A]`（元 finding 14, `consensus/core/src/palw_step_refute.rs:984`）: decode 裁定は pin 内部の自己整合しか見ない。`check_base0_decode_pin` は `base0_logits_trace_root_v1(ctx, pin.logits_rows, generated)` を `binding.full_logits_trace_root` と比べるだけで、その logits 行が step leg の post-logits タイルと一致するかを court も panel（`produce.rs:762-798` は2本を別々に再計算して突き合わせない）も見ない。→ 生成テキストを任意に選んだ claim が Final に到達する。
- 修正: close を「session 内の一手」にする。(a) ladder が `Terminal`/`Abandoned` でない close を拒否、(b) `refutation.output_opening.leaf_index == session.ladder.terminal_index()` を要求（decode も同様の position pin）、(c) `CourtClosed` に ML-DSA-87 署名を要求し `ChallengerDefeated` は session の challenger bond からしか受けない、(d) 6番目の evidence 種として decode 位置 p の post-logits タイル（座標は `canonical_step_leaf_index` で決まる）を開いて `pin.logits_rows[p]` と bit 比較する経路を追加、恒久的には `full_logits_trace_root` を step leg から導出させて独立スロットにしない。

> **項目 7・8 は同じ層の3つの穴（bind が繋がらない／close が session に縛られない／logits が leg に縛られない）で、別々に直すと意味が無い。「裁定層の作り直し」1本として扱うべき。**

## 9. `[A]` IBD commit の PALW fork-choice ゲートが構造的に空振り — headers-proof IBD は blue work だけで決まる
`protocol/flows/src/ibd/flow.rs:1819`
**（元の findings 7 と 13 は同一欠陥。統合した。）** `validate_staging_palw_order` は `let (Some(incumbent), Some(challenger)) = ... else { return Ok(()) }` と fail-OPEN（確認済み）。challenger は**常に** `None`: staging は `.skip_adding_genesis()` で作られ（`factory.rs:389`）、`PalwChainStateV2` の genesis 書き手が走らず、唯一の他の書き手 `import_pruning_point_palw_state` は commit barrier の**後**（flow.rs:1564 vs 1521）。兄弟サイトの deep-reorg ゲート（`processor.rs:8005-8030`）は同じ形を明示的に fail-closed に直してある。
- 実害: 私有フォークで blue work を積んだ攻撃者が、relay 経路なら `decide_deep_reorg_v2` に `DominanceViolation` で拒否される chain を、IBD 経路では丸ごと victim に載せ替えられる。「PALW work がチェーンを決める」が「blue work が決める」に退化する。
- 修正: `processor.rs:8012-8020` と同じ三分岐にする（challenger が `None` なら `KeepIncumbent`、incumbent が `None` なら hard error）。そのためには staged chain が判定時点で weigh 可能である必要があり、`sync_pruning_point_palw_state` を **staging セッションに対して** commit 前に走らせる。**項目1と同じ継ぎ目**なので、1と9は1本の作業にすること。

## 10. `[B]` merged blue の subsidy が entitlement だけで払われる — lottery も epoch 予算も dedup も効かない
`consensus/src/pipeline/virtual_processor/processor.rs:3820`
`palw_v2_unentitled_blues` は `check_palw_producer_entitlement_v2`（bond 存在/非 Retiring/pubkey/operator_id/class Active の5点、`palw_admission_v2.rs:143-185`）しか見ない。発行量を計量する項目——DerivedV1 pwu 等式、per-epoch 予算、per-class lottery `class_ticket_v2 <= target`、exposure ceiling、`DuplicateAttempt`——は全て `check_palw_attempt_admission_v2`（:191-330）にあり、selected-chain walk でしか走らない。しかも署名は `attempt_id` と PoW digest の外なので（`consensus/pow/src/lib.rs:883-895` で pin 済み）、1つ解いた attempt を hedging randomness を変えて再署名するだけで別 block id の valid block が任意個作れる。ツリー内テスト `tests.rs:2139-2202` は claim が1つであることは assert するが**支払いが1つ**であることは assert しない。
- 実害: worker base は subsidy の 6200bps（`params.rs:1346`）。chain block の worker base は escrow されて void リスクを負うのに、兄弟 blue のそれは無条件即払い。合理的 producer は1チケットあたり K ブロックを出し class lottery を完全に無視する。正直な採掘者は K 倍希釈され、per-class DAA は支払われたブロックが入っていない census を測ることになる。
- 修正: coinbase の支払い述語を、発行量を計量する項目について chain admission と同じ述語にする。`palw_v2_unentitled_blues` の中で `class_ticket_v2 <= class_target` と mergeset 内 + selected chain の attempt_id dedup（state は attempt_id をキーにしているので O(1)）を評価する。正直な代替は、merge blue には何も払わず worker carve 全額を chain block の escrow に通すこと。

## 11. `[B]` gossip の `materials_per_claim` が永久に増える + 8MiB blob が無認証で全ピアに中継される
`protocol/flows/src/palw_gossip.rs:87`
`materials_per_claim` は claim id ごとに entry が入り（:125、確認済み）、削除も TTL も上限も無い。claim id は 2^512 通りで、`v8/palw_gossip_flow.rs:53-56` でワイヤからそのまま取られ、on-chain 存在確認も署名も bond 紐付けもレート制限も無い。`Fresh` は全ピアへの再中継を起こす。`seen` は 4096 件 FIFO なので flush してから 8MiB material を再注入すれば増幅器にもなる。
- 修正: `materials_per_claim` に LRU 上限（または `seen` の FIFO に畳んで evict 時に減算）、admit/relay の前に claim が `PalwChainStateV2` の生きた claim か確認、ピアごとの token bucket、`seen` に時間ベース失効。**consensus 外なので fork 不要**、いつでも直せる。

---

## `[C]` 欠陥ではないが明言すべき事実

- **今日の mainnet は無傷。** `MAINNET_PARAMS` は `palw_consensus_mode: Disabled`、credit/fork_choice/schedule/ramp/block_commitment 全て None、`pow_palw_activation: never()`、`evm_activation_daa_score: u64::MAX`。上記は全て「ConsensusV2 に切り替えたら」の話であり、現行 mainnet の緊急事態ではない。
- **testnet-12 が緑であることは裁定層について何も証明しない。** court responder のバイナリが存在しないので、t12 は court を一度も通っていない。ブロックが出ていることは、生産経路が動く証拠であって、adjudication・pruned IBD・bond 退役の証拠ではない。
- **ドキュメントの gate ledger は信用できない。** `docs/palw-road-to-mainnet-2026-08-21.md` の Gate 0「closed」の根拠テストは `palw_court_v2.rs:1020` で本番と逆の代入を自作しており、最後の継ぎ目を通っていない（項目7）。ledger は再検証するまで参照しないこと。
- **上記の修正のうち複数（FP オブジェクトへの pubkey 追加、`CourtClosed` への署名追加、shape の上限追加）は `palw_ruleset_id_v2` を変える。** t12 は再 mint / 再 genesis が必要になる。修正をまとめてから1回で切ること。
- **bond 集合は genesis 固定の公開定数**（`params.rs:2386-2414`）。上記の攻撃は全て既知の outpoint を狙える。「6 operator だから内輪」は緩和にならない（項目5は無担保の第三者が実行者）。
- **部分的な有効化は不可。** credit レーンを ON にして court を OFF のまま、という中間状態は「不正が slash されない発行」そのもの。ConsensusV2 は裁定層と一緒にしか入れられない。
- claim 序盤の weight=0（最初の約40時間）は設計どおりで欠陥ではない。

---

## GO への最短経路

**Phase 0（数日、コードサイズ／並行可）** — 項目2（step ループの上限と早期脱出）、項目3（terminal 時の session 掃除 + 寛容な sweep + carriage 不変条件）、項目11（gossip の eviction とレート制限）、項目10（`unentitled_blues` に lottery と dedup、または merge blue 無払い）、項目5（FP object に pubkey を載せ bond と比較 + acceptance 実アーム）。

**Phase 1（数週）** — 項目1と項目9を**1本の作業**として: pruning point 前進時の PALW snapshot store を作り、exporter をそれに向け、`sync_pruning_point_palw_state` を staging セッションに対して commit 前に走らせ、`validate_staging_palw_order` を fail-closed 三分岐にする。実 exporter/実 importer の2ノード統合テストと、pruning point を実際に跨がせた fleet drill を受け入れ条件にする。

**Phase 2（数週〜数ヶ月、これが本丸）** — 裁定層を通す: 項目7（trace_root の意味を一本化 + テストを本物の attempt から立てる）、項目8（close を session の一手に縛る + 署名 + terminal index 一致 + decode logits を step leg に縛る）、項目4（court responder の実装と起動時 assert、challenger 側に担保と没収）。受け入れ条件は「実 producer が嘘を1タイル入れ、第三者 challenger が実際に有罪化し、かつ正直な producer が実際に無罪を証明する」E2E を live fleet で往復させること。片道だけの green は証拠にならない（今回2件の欠陥がまさにそれで隠れていた）。

**Phase 3** — 項目6: 所有者署名つき `BondRetireRequested` と、担保 outpoint の実在証明つき `BondRegistered` の再許可。ここが入るまで「stake」と呼んではいけない。

**そのあと** — 新 ruleset id で t12 を再 mint し、**最低1 pruning サイクル**を跨ぐ公開 testnet を回して（項目1と項目6が実機で証明される唯一の方法）、その上で再監査。この4フェーズを通過するまで、ConsensusV2 の mainnet 有効化は NO-GO。
