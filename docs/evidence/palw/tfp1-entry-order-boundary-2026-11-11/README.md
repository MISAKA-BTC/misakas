# TFO3入口の型付き拒否と実受理順の境界

状態: **IMPLEMENTATION_IN_PROGRESS / Rust build・test NOT_RUN / 公開有効化なし**。

前ターンの `42d7bb534` はHF参照資料を接続したprogress。今回もコードを変更したが、Studio同一claimの公開G14受付・Final・当選時権利消費・対応blockは **NOT_PROVEN**。

## 今回の修正

[旧native準備入口](../../../../consensus/core/src/palw_tir_public_fold_v1.rs)は、認識済みTFO3 companionを旧frame/source/資料配信見積もりに流す前に `PublicOriginalProfileUnavailable { claim }` を返す。preclaim quoteではbuilder生成前、paired preparationではlegacy envelope lookup前に同じguardを使う。[coordinator](../../../../consensus/core/src/palw_funded_original_coordinator_fold_v1.rs)にも二重のguardを維持する。完全profile未完成という既存の拒否条件を開放した変更ではない。旧準備を完了させてから拒否する無駄と、legacy view欠落時の旧経路への降格を防ぐ。

[全prefix再演](../../../../consensus/core/src/palw_state_v2/ordered_prefix.rs)はこの型がordinary object位置で発生した場合のみ、除外候補indexを返せる。任意のCarriageInconsistent、順序不整合、Overflow、local capacity/context failureを除外可能に広げない。実際に候補を除外するNodeの新filter loopはまだ未接続。

[Nodeのliteral順序処理](../../../../consensus/src/pipeline/virtual_processor/processor/palw_accept.rs)は、ordinary carrierが完全AcceptanceDataに存在することとordinal+1の非overflowを並べ替え前に検査する。未知carrierをusize::MAXへ置くfallbackを廃止し、localなcontext/resource faultとして返す。派生objectの先頭配置、重複IDの初出順、decoderから消えた取引によるordinalの隙間、旧非TKR経路は維持する。Batch/TKR挿入位置とstable sortは同じ検査済みkeyを使う。activationは変更しない。

回帰コードはTFO3の準備前拒否・legacy view欠落時の拒否・旧frame持込時のcoordinator拒否・state/journal無変更、拒否位置の分類、unknown/overflow ordinal、派生先頭・duplicate・ordinary除外後のnative位置を記述。Rust実行は[通し実装後](../../../design/palw/integration/public-one-honest-vertical-implementation-plan-2026-11-10.md)。検証結果はvalidation.jsonに記録する。

## 次に接続する実経路

独立読解により、実処理の入口は `processor.rs` の `palw_v2_objects_of_block` → `palw_v2_accepted_objects_and_raw_assignments_v2` → work/merged work/own key/transition_extras組立 → v8 foldと確認した。palw.rsの単体advisory rehearsalを実filterの代用にしない。

- 現filterはnode署名・admission、rent、refund、court枠、raw Assignment、Batch/TKR fee projectionを含む。core replayだけで置換するとこれらが抜ける。
- own attemptのbootstrapは旧filterのfolded stateを読み、確定workはその後に決まる。生headerから取った未認証attemptを確定workへ昇格しない。新prefixから同じbootstrap結果を再照合する必要がある。
- 同一候補を再試行するとき、ordinaryだけを削るのではなくcompanion/answer/prefix/refund等の候補依存mapとBatch/TKR/PMR/raw挿入位置を再構築する。physical ordinal・carrier identity・UTXOで既に計上したfeeを付け替えない。
- TKRの意味上の拒否でもattempt消費等が残る経路を、handoff削除として扱わない。fee projectionの変化は実UTXO計上と一致しなければlocal fault。discardする再試行でclass-carriage store/観測の副作用を確定しない。
- 完全profile predicate、全枝の資源/費用/期限、quote、共同予約、原子的TFP1/companion install、PMR dispatcher以後は未完。今回の整備を正常受付やブロック生成の達成と数えない。

配備・署名送信・公開probeは実行していない。
