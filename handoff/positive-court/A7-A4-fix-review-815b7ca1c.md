# A7 — A4修正の追跡レビュー（2026-10-31）

固定source `815b7ca1c` を親と2担当で静的レビューした。Cargo実行・production編集・追加target作成なし。前回の[d746レビュー](A7-A4-static-review.md)を上書きせず、修正の到達点を記録する。A7の歴史的243件は `bd9623914` の結果であり、この統合baseの実行結果ではない。

## P1 — 静的に解消、実行確認は次の固定候補で行う

`contract.rs:290–377` は非変更のclock検査、witness認証、`authorize_candidate`、context/plan/同一context確認、全receipt容量のchecked計算と確保、checker初期化を済ませてからattempt/candidateを更新する。`carrier.rs:63–81` の認可はrooted prover opportunity/roleを解決し、使用不能・無制限のpacket上限も拒否する。Positive後のGate C用P drawとは別の認可である。

追加試験 `positive_court_a4_integration.rs:462–519` は、seed93の有効な自己署名と実A1 contextを使って、rooted roleを持たない者を拒否する。attempt・counter・既存候補の不変と、その後の正規Panel完了を確認する内容になっている。準備失敗時・opaque context差替え時も状態を変更しない分岐を照合した。

## P2 — 受理済み再送は修正、未受理future frameの再送が残存

受理済みのcanonical frameは、現在のauthorityと署名を検査した後、`EvidencePartId` を `first_relation` で検索し、Case課金より先にACKする。receiptは全plan分を上限付きで先に確保し、payload全体を保存しない。canonical one-relation/順次受理によりreceipt indexとcursorが対応する。

ただし `contract.rs:403–428` には次の残存経路がある。

1. cursor=0で、正規Panelが署名した後続 `frames[1]` を取得した第三者relayが同じpacketを送る。
2. authority・署名検査は成功するが、receiptは空なので既知ACKにならない。
3. 412–418行でwire予算を加算する。
4. `require_next` は `first_relation=1` とcursor=0の不一致から `ContractError::NonCanonical` を返す（`ports.rs:143–144`）。
5. cursor・receiptは更新されず、同じ時刻・同じ署名コピーで反復できる。毎回の加算で有限Case予算を埋め、正しいframe0やsuffixの受理も妨げる。

悪意あるPanel、新しいpacket、署名偽造は不要。`relation_count=2` の同一署名packetも課金後の構文拒否を繰り返せる。受理済みframeだけを対象とする646–686行の再送試験では、この経路は検査されない。

A0はこの残存経路をA4へ修正依頼済み。native認証と既知accepted receipt照合の後、順序・受付条件をCase課金より前に確認し、future/order delivery拒否ではCase状態・Case journalを変更しない方針。現在relationの署名済み不正算術に対する費用保持とは区別する。

未受理future frameを一度渡した後に同じ署名コピーを反復し、wire/cursor等が増えず、後で同じframeを正しい順序で受理できるnegativeが必要。[補助試験案](../../testing/positive-court-a7/evidence/a4-fix-review-815b7ca1c/future_frame_replay.rs)を保存した。これは既存integration test module内へ追加するための案であり、実行済み結果ではない。

追加の確認範囲: accepted ACK前の認証、receipt確保失敗のatomicity、distinct signed bad packetの課金保持、rooted branch/clock cutoff。既存の履歴再実行試験はclosureからの手動replayであり、common Courtの永続journal roundtripとは扱わない。厳密なwire cap到達後ACKの専用実行試験とnative復旧も未確認。

A0の型検査と次の統合候補固定を待つ。実176 writer/header/profileは未接続、public設定はNone。過去の成功件数・raw・digestは変更しない。
