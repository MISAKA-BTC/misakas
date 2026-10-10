# G14 — codex への引継ぎ（2026-10-10, Lead）

ユーザーの指示により、G14（RFC-0014/0015 の公開告発の完成）は codex が担当する。この文書は、Claude 側 lane の到達点と未完了項目の一覧である。
判定の上位基準は [PESG](probabilistic-economic-security-gate.md) の §4 C、そして [g14-completion-matrix.md](g14-completion-matrix.md) である。

## Codex側の更新（2026-10-10）

以下のLead引継ぎは当時のsnapshotである。Codex branch `codex/g14-prosecution-bounds`の現在の修正と検証は[監査記録](../../audit/g14-review-2026-10-10/implementation.md)、残る受入条件は[レビュー](../../audit/g14-review-2026-10-10/feedback.md)を参照する。

- `f89431d82`までにK2S `4a6f20aaf`を統合し、small-value whole-value court、認証済みmaterialからの独立再実行、RAM/state/filingの上限を修正した。64要素hiding tileと累積scope、最大chunk数を含む期限は残る。shape上限計算は実modelのpeak測定ではない。
- `74d59907f`で固定報酬jobの生成数を束縛し、短い正しい計算が満額を得るF-B1を修正した。kernel回帰と実nodeの受入拒否・完全実行Finalを検証済み。
- `e9eee4911`でLG14-Aを統合し、`a4a62ec21`で単独の不正leafを見逃すmidpoint探索と未認証responseのcache汚染を修正した。core 10件、fold 7件、kaspad filer 5件、実nodeの最終batch 7件と修正後reorg 1件、A2U 3件がPASS。shipping repinは差分なし。実node8件が未解決のままという下記記述は旧snapshotの状態である。
- LG14-Bの大型tree/fused court本番接続、fresh verifier自身の公開readからの完走、reservation全枠の占有対策、期限内包含は未達。小型fallbackは34-session枠を超えるclaimをUnjudgedとする。
- G14Rのverifier-payとPESGはintegration `0b73fd33f`から統合済み。F-MEAS-07のnet徴収計算も現行`required_reservation`では自己還流分を控除する項を含み、下記の総額だけを数えるという記述は現HEADに当てはまらない。
- integration `7d8c31270`のbeacon source-set freezeを`b6a90a0d0`で統合した。検証結果とscopeは監査記録へ記載する。

G14全域の完成判定はしていない。ADR-0177の登録モデル保有前提はユーザー確認済みであり、モデル配布義務の追加やactivation fenceの変更は行っていない。

## 統合ブランチ

`claude/g14-public-prosecution-integration-9bee39`（origin）。

この head には次が merge 済みである。
- **codex 側:** `codex/g14-prosecution-bounds`（`a3eb2e36c`）。merge の際、K2S の v4 dispatch との整合を取り、`ledger_cache` を復元した。
- **G14 関連 lane:** G14R round 2、G14C m1–m4、K2S e6/e7、LG14-B（fold レベル）、DA16b、OPVB、BUDGET。
- **経済・測定:** ECON、MEAS。

## lane 別の到達点（branch はすべて origin に push 済み）

### K2S（`k2/real-scale` @ `4a6f20aaf`）

**検証済み（`8fae728a2` まで。統合に merge 済み）:**
- GAP-30：v4 の各種の嘘、Final 後の conviction、filing、reorg・restart・pruned import。
- GAP-31：history・routing を持つ 8k class。
- GAP-40：v5 の tiny BERT。
- GAP-32：merge 済み。

**未完了（`4a6f20aaf` は未 compile の WIP）:**
1. option (c)：64 要素未満の masked value に対する whole-value recompute court。test の配線と、wide128 d=16、dense-MoE rows of 4 での試験が残っている。
2. option (a)：64 要素の hiding tile。
3. tag-113 の chunk carriage（1 MiB の Respond、v5 registration）、chunk 数を数える deadline、最大 Respond の terminal 試験。

詳細は `docs/design/palw/k2-real-scale.md` §13 にある。

### LG14-A（`g14/legacy-filer` @ `51d026bc9`）

**検証済み:** fold レベル（`lg14a_legacy_dispute_fold` 7/7）。

**未完了:** 実ノード E2E の 8 件が失敗している。症状は次の 3 つ。
- 5 回の demand の後に追及が止まる。
- dismissal の後に Final へ届かない。
- reorg 後に branch A へ戻らない。

その後、canonical harness への移設と、LG14-B の descent の配線が要る。詳細は `docs/design/palw/legacy-route-g14-filer.md` §12。

### LG14-B（`g14/legacy-held-da` @ `19522bc01`）

**検証済み:** fold レベル（[C12] の 3 つの gap を塞いだ）。

**未完了:** 実ノード、D2 part 2–3（dissection root claim、readiness）、`palw_court_scope_v1` への置き換え。

### G14C（`g14/completion` @ `aa533a6e6`）

canonical node harness と、F1・F6・F8 の実ノード検証。matrix の §2a に family ごとの現況がある。

### G14R（`g14/r4-fixes`）

M\*-49 と default 時の取り分の保留を、新しい休眠 fence `palw_verifier_pay_v1` で実装中（ユーザー採用済み）。

## 測定と攻撃から出た未解決項目

- **gather lie の filing が wire 上限を超える（MEAS、実測）:** filing は 272 MB で、wire の上限は 67 MB。そのため `p_evidence = 0`、`p_min = 0` で、K2-TIR v1/v2 は FAIL である。実 class 5 つすべてで構造的に起きる（gate の filing bound は 0.54–2.15 GB）。v4 の per-element court への移行か、class の拒否が必要。
- **F-MEAS-07:** `OpvPolicyV1::required_reservation` が slash を総額で数えており、49% の自己還流を差し引いていない。
- **v4 の資源見積り:** v4 の `max_retained_state` と `max_verifier_ram` は、codex の保守的な計上（served response、prosecution metadata、decode 後 16 B/element）でまだ導出し直していない。`gate.rs` の `public_prosecution_complete_v4` にある。
- **cumulative scope（F-C4R4-17）:** kernel ledger の hook（`cumulative_scope_allows_v1`）は、現状すべてを受け入れる。DA16b の predicate（`palw_court_scope_v1`、`misaka_palw_kernel::scope`）への接続が残っている。
- **16 unit の範囲内で、任意の不正位置を局所化できることの証明**（codex のレビュー指摘）。
- **期限内の包含:** junk な FileProof が court budget を消費する（arming list の CODE 14）。
- **F-B1（PESG-B、以下は発見時の記録）:** 生成長がどの規則にも束縛されていない。`job.rs` は 1..`max_new_tokens` の任意の長さを受け入れ、報酬は claim 単位で固定なので、1 token の claim が満額を得て、どの court も有罪にできない。v1–v4 と pipeline の T1 FAIL。job で長さを固定するか、検証済み位置ごとの支払いにする必要がある。詳細は `pesg-b-detection-bounds.md`。Codex review branchでは、生成型jobの`max_new_tokens`を必須の生成数として受入時に照合する修正と回帰試験を追加した。非生成型のzero-generation規則は別扱い。wire名を維持した意味の変更であり、activation前のconsensus reviewは必要。
- **v4 route A（m/P 標本）は FAIL、`q` 標本の route B は draw が未配線で UNKNOWN（PESG-B）。**
- **A2U の pin:** 解決済み（`d9f7cbd6c` で tag 109 再 pin・TakeoverToken を NotCarried に分類、`824e6da15` で kernel route の mempool gate を fence 未満では判定せず通すよう修正）。

## 判定の扱い

PESG に従い、PASS / FAIL / UNKNOWN の 3 種類だけを使う。
fixture の PASS や `cfg(test)` の seam は、production 経路が完成した証拠にしない。
ADR-0177 に従い、G14 は model を取得できた verifier を前提とする。閉鎖 model の `p = 0` は独立した未解決 gate である。
