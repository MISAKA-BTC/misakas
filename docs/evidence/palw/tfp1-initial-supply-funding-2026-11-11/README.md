# TFP1初回claim資料量と保護予約・実支払の接続

状態: **IMPLEMENTATION_IN_PROGRESS / WRITTEN_NOT_RUN / 公開有効化なし**。

[初回供給量](../../../../consensus/core/src/palw_positive_contract_v2/tfp1_original/initial_supply.rs)は、実FP/Source、登録program/layout、先行して受理されたtokenizer、固定したHF参照descriptor、literal promptからclaim固有資料のbyte下限を導く。全Trace値にopeningとtileのwire prefix・Merkle siblings・indexを加え、manifest本文も計上する。InputTokensは一組のraw vectorを保守的に加え、別courtのinput opening搬送とは区別する。scope codec/chunk geometry、manifest各上限と総量、初回送信/保持枠を検査する。保持byte×DAAはoverflowで拒否する。HFモデル重みの取得・全量配信・取得可否証明は行わない。Panelの全モデル再計算はNode workへ換算しない。

[保護予約](../../../../consensus/core/src/palw_positive_contract_v2/tfp1_original/preclaim.rs)の新APIは、成功した予約のOriginal ID・obligation・受付DAA/blue score・quotes/graphのnative所有者をprivateな一つの結果にする。callerがquoteのrent列を差し替えて資金検査へ渡す経路をなくす。旧選出枠だけのAPIと識別子は維持する。時刻一致だけで同じparent/bookを認証したとは扱わない。

[資金検査](../../../../consensus/core/src/palw_positive_contract_v2/tfp1_original/funding.rs)はnative factsとsealed bookingを照合し、選出・第三者・防御の全quoteからNode保存料と後続carrier料をchecked sumで導く。署名fee上限は既使用料＋元claim費用＋Node保存料＋後続carrier料を覆う必要がある。実funding出力は元claim費用＋Node保存料＋capitalを覆い、後続carrier料を二重に取らない。実pairingのcompanion、UTXO handoff、origin、txid、outpoint、funding条件、payer署名を照合する。privateなimmutable pairingのfactoryがliteral transportを照合済みなので、ここでは全transportを再encode/decodeせず、実出力とpayer署名を再検査する。legacy hashの一時メモリは別途事前予約し、local資源不足を不正claimのrefusalと分ける。

[予約と支払の一括準備](../../../../consensus/core/src/palw_positive_contract_v2/tfp1_original/funding/admission.rs)は、署名済みprepared claimの予約を現候補bookの複製で再計算し、その結果から資金を検査する。成功した場合だけ呼出し元のbookを置換する。候補bookの既存量とpacket上限から増分を事前予約し、確定後も`NativeRetainedV1`で所有する。資金不足・署名不正・UTXO handoff欠落・重複予約では元bookを変更しない。これはbook単位の準備であり、claim/frame/escrow/履歴をDBへ一括保存するproduction coordinatorの完成ではない。

回帰コードは既存の実tokenizer登録fixtureと同じnative FP/Sourceを利用する。Trace openingの実Borsh長、上限/codec/保持overflow、署名前後の予約一致、実payer署名・出力、額/期限不足、scope差替え、handoff欠落、支払失敗と二重予約でのbook不変を追加した。FP署名、単価、資本額、UTXO入力値はfixtureであり、公開支払・quote・受付の証拠にしない。Rust build/check/testは[採択計画](../../../design/palw/integration/public-one-honest-vertical-implementation-plan-2026-11-10.md)に従い未実行。整形、静的レビュー、文書検査の結果はvalidation.jsonに記録する。

次は初回供給とnotice/全体検査・回復/終端の完全graph、実tariff/capitalとprofile resolver、native受付coordinator・RPC・Node全prefix再演への接続が必要。モデル取得証明は対象外。FpTokenizer公開route、公開同一claimのquote/署名/受付→Panel→Final→当選権利消費・対応blockは未完・**NOT_PROVEN**。公開配備・送信・probeは行っていない。
