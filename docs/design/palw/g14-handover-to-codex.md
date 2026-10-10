# G14 — codex への引継ぎ（2026-10-10, Lead）

ユーザーの指示により、G14（RFC-0014/0015 の公開告発の完成）は codex が担当する。この文書は、Claude 側 lane の到達点と未完了項目の一覧である。
判定の上位基準は [PESG](probabilistic-economic-security-gate.md) の §4 C、そして [g14-completion-matrix.md](g14-completion-matrix.md) である。

## Codex側の更新（2026-10-10）

以下のLead引継ぎは当時のsnapshotである。Codex branch `codex/g14-prosecution-bounds`の現在の修正と検証は[監査記録](../../audit/g14-review-2026-10-10/implementation.md)、残る受入条件は[レビュー](../../audit/g14-review-2026-10-10/feedback.md)を参照する。

- `f89431d82`までにK2S `4a6f20aaf`を統合し、small-value whole-value court、認証済みmaterialからの独立再実行、RAM/state/filingの上限を修正した。64要素hiding tileと累積scope、最大chunk数を含む期限は残る。shape上限計算は実modelのpeak測定ではない。
- `74d59907f`で固定報酬jobの生成数を束縛し、短い正しい計算が満額を得るF-B1を修正した。kernel回帰と実nodeの受入拒否・完全実行Finalを検証済み。
- `e9eee4911`でLG14-Aを統合し、`a4a62ec21`で単独の不正leafを見逃すmidpoint探索と未認証responseのcache汚染を修正した。core 10件、fold 7件、kaspad filer 5件、実nodeの最終batch 7件と修正後reorg 1件、A2U 3件がPASS。shipping repinは差分なし。実node8件が未解決のままという下記記述は旧snapshotの状態である。
- LG14-Bのbase0-codec step descentとtag-159／CKW fused openingを共通filerへ接続した。fresh verifier自身のnode startupから公開read・包含までの完走、checkpoint/trace/job/count不一致、reservation全枠の占有対策、期限内包含は未達。unsupported-codecの小型fallbackは34-session枠を超えるclaimをUnjudgedとする。
- G14Rのverifier-payとPESGはintegration `0b73fd33f`から統合済み。F-MEAS-07のnet徴収計算も現行`required_reservation`では自己還流分を控除する項を含み、下記の総額だけを数えるという記述は現HEADに当てはまらない。
- integration `7d8c31270`のbeacon source-set freezeを`b6a90a0d0`で統合した。検証結果とscopeは監査記録へ記載する。
- legacyの公開履歴readerを件数制限付きpaginationへ変更し、開始tipの固定、新しいMismatchのbackfill、reorg時の読み直し、認証済みの利用対象unitだけのcacheを実装した。filer 7件とConsensus API adapter reader 4件、計11 V-unit PASS。恒久的なprune後の取得・再要求とfresh-node追及のV-node試験は残る。
- LG14-B producer応答を既存DA workerへ接続した。検証済みbase0-codecのdense/fold保持からstep node・checkpoint node・CKWを作り、認証・scope・close ceiling・署名・carrier検査を通るtag 158をqueueする。DA policy 8 V-unit、LG14-B 6 V-fold、IR回帰19件がPASS。outsiderの大型descent/terminalとproduction-serviceのV-nodeは残る。
- 上記producer接続後、outsiderの共通filerをLG14-Bへ接続した。独立したown replayからstep treeを読み、認証済み公開frontierでdescentし、court自身のpredicateを通ったtag 159又はCKW fused accusationを署名・queueする。公開tag-158履歴は選択unitだけをcacheし、後から選ぶ既回答unitもbackfillする。LG14-B 7 V-fold、filer 8 V-unit、公開reader 5 V-unit PASS。Final前後のgather/matmulとfused conviction、frontier/CKWのDA defaultをactual controllerで確認した。full serviceのV-nodeや最大profileの測定は未達で、step treeのみの一致からHonestとは判定しない。held-court後半の旧readerも次の更新でpaginationへ変更した。

- held-court後半のpublic readerもpaginationへ変更した。部分suffixからroot filingを採用せず、要求floorまでの最古の認証済みfilingと選択chunkだけを保持する。unit選択変更でbackfillし、cached filingのbranch確認をtickで行う。reorgではchallenger evidenceとqueued moveを破棄し、branch確認が読めない場合はmoveを待つ。reader 6 V-unit、held policy 6 V-unit、held controller 20 V-fold、LG14-B 7 V-foldを検証した。full service／最大profile／恒久pruneの完成ではない。

- X8R統合 `cc4757d04`を`123d8254e`でmergeし、EXEC v2のkind fence 6とLG14-Aの7を両立した。Final deadlineではwork-session holdとdispute holdの両方を維持し、retirementでも両recordを処理する。lifecycle/A2U core 31件と公開reader 6件がPASS。
- 公開bindingからのIdentityMismatch、shape/count違反、公開token pinからのOutputMismatchを共通filerのreporter doorへ接続した。output-only mismatchをHonestとする抜けを修正し、DA回答済みflagがなくても完全な直接証拠を受け付ける。最新の試験範囲と残条件は監査記録の「公開bindingの直接証拠」を参照。

- 4096 idsを超えるcanonical attempt入力も、公開bindingと記録済みanchorからPromptNotAnchoredのWhole証明を作り、共通reporter doorへ渡す。メモリ予約を取り、直接証拠の判定をblocking workerへ移した。65k／2M contextのsynthetic inputでhonest/unbound拒否と、全Panel Valid後のFinal前後のactual fold convictionを確認。LG14-B 9 V-fold＋2 V-unit、filer 10 V-unit、reporter 21 V-unit PASS。最大modelの実行・通常eligibility・fresh-node full-serviceを立証したものではない。

- integration `589d9aa3e`のremote-miner drill adapterを`7bd999b17`でmergeし、RPC binaryのcheckを通した。共通filerでは受理block/job/role変更時に旧判定・探索・queueを破棄し、起動時snapshotと違う旧replay結果を適用しない。blocking workerは予約を保持してdrainし、book clear後も終わるまで次のworkerを開始しない。filer 14 V-unit、LG14-B 9 V-fold＋2 V-unit、public reader 6 V-unit PASS。full nodeのreorg完走は未検証。

- free-promptの公開accepted commitmentからjob/inputを認証し、fresh modelの独立replayへ接続した。FPにvesting rowがなくFinal後のreservationとDA admissionが止まる経路も、既存public-filer fence内で共通predicateとliability保持へ修正した。PanelDA User inputのbootstrapとordinary V3/V4 canonical FP count/rootのreplay前の直接証明を追加済み。公開prefix-state／別job意味論、full service／全family・最大profileは残る。検証結果とfixtureの範囲は[FP修正記録](../../audit/g14-review-2026-10-10/implementation.md#公開fp-commitmentinputからの独立再実行とpost-final-fpの追及)を参照。

- PanelDA User入力の公開court bootstrapを追加した。producer側のinput retentionだけを回答に使い、verifierは公開認証済みchunkを学習後、自身の登録modelで再計算する。worker handoffで入力accumulatorを失わないよう修正し、reorgでinputを破棄する。最大contextの34-session適合、FP commitment自体の再取得、full-service包含は残る。検証結果と範囲は監査記録末尾を参照。

G14全域の完成判定はしていない。ADR-0177の登録モデル保有前提はユーザー確認済みであり、モデル配布義務の追加やactivation fenceの変更は行っていない。

共謀者による予約人数の先取り対策として、claim共通の64-live／256-lifetime capを除き、closed replay guardを含む各bond自身のretained record 64件へ上限を移した。旧閾値を超えた新bondのsession/defaultと、小型PanelDAの公開input→own replay→conviction/defaultをV-foldで確認した。133件の関連回帰とshipping pin 361件は通過。owner数に比例する最大state／response負荷、fresh-node包含と全familyは未完。詳細は[監査記録](../../audit/g14-review-2026-10-10/implementation.md)末尾を参照。

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

その後、canonical harness への移設と、LG14-B の descent の配線が要る。

**番号の注意（10-10 Lead）:** 統合では lifecycle kind fence の 6 を X8R の `ExecPayloadV2` が使った（`66597500a`）。LG14-A を merge するときは `PalwLifecycleKindFenceV1` の 7 を使い、`ALL` を 8 要素にすること（discriminant は連番必須）。詳細は `docs/design/palw/legacy-route-g14-filer.md` §12。

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

## Codex追記: 回答済み公開DAの再取得

`codex/g14-prosecution-bounds`の`20c746436`以後、回答済みunitの公開carrierが失われても追及できるよう、休眠LG14-Aへ署名付き予約DAA・次session番号・期限・unitのtag 156を追加した。historical answeredは保持し、古いaccusationと旧番号を再送して担保や枠を消費することを拒否する。Event／Held／LG14-Bのみ対象で、同じunitの認証済み再公開又はDA defaultへ進める。公開viewのFlat回答範囲も反映する。readerは古いsuffixの欠落を完了とせず、新tipの再公開を読む。最終検証結果は[実装修正記録](../../audit/g14-review-2026-10-10/implementation.md)を参照。

これは全G14達成ではない。次はPanelDA inputを持たないverifierのbootstrap、pruneされたFP jobの認証済み再取得、TIR／pipelineへの接続、全familyの正規registrationとfresh-node実service／包含、累積scopeと最大profile・期限・枠飽和を閉じる。有効化は変更していない。
