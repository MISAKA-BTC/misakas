# TFO3専用frameと四行の原子的保存

状態: **IMPLEMENTATION_IN_PROGRESS / WRITTEN_NOT_RUN / 公開有効化なし**。

[専用frame](../../../../consensus/core/src/palw_positive_contract_v2/tfp1_original/native_frame.rs)を追加した。旧FP frameのallocation・モデルDA・custodyを新profileへ持ち込まず、literal FP、Source header、受付origin、profile/resource、obligation、予約identity、claim資料の保持期限を固定する。新しいstaged key 39 / row 46を末尾追加し、旧tagと旧frameの意味は変更しない。aux 204は明示version 2でこのframeを参照し、旧version 1は従来のallocationを参照する。

frameのFP/headerはそれぞれ既存の48 KiB / 2 KiB上限を持つ。decode前に固定stack領域で内部codecをpreflightし、unknown version、過大長、末尾余剰を拒否する。正常なframe検査のhashは既存domainとcanonical bytesを維持したstreaming計算とし、一時Vecを作らない。保存用clone・shared handle・map/journal・派生indexはnative資源ownerに予約する。拒否時の診断文字列の割り当ては残る。

[専用writer](../../../../consensus/core/src/palw_staged_court_fold_v1.rs)はframeを先に置き、その状態で資金escrowと履歴を照合して198/204/206を保存する。失敗時は三行writerの局所inverseの後、frame・期限index・producer holdを照合して削除する。一般writerからの挿入は拒否する。既存の外側checkpointがclaim・book・escrowを所有する構成であり、この四行writerだけで完全な受付・費用確保が成立するわけではない。

順方向・逆方向のjournalは四行の隣接、claim/Source/予約/保持期限を照合し、履歴境界に当該frame自身を含めない。snapshotでもframeの孤立を拒否する。静的レビューで判明したclaim/class/旧frame単独変更時の再検査漏れも修正した。claimと旧frameは対象claimだけをlookupし、class変更時は専用frame範囲をdelta当たり一度検査する。

回帰コードには、実署名pairing fixtureからの四行保存、escrow不足・prefix不足の正確な失敗箇所と完全巻戻し、tracked owner必須、旧新wire tag、全行の一行欠落、履歴境界差替え、claim/root/保持期限/classの差替え、旧frame併存、snapshot復元、streaming hashのV3/V4既存値との一致を追加した。**実行済みではない**。Rust build/check/testは[採択計画](../../../design/palw/integration/public-one-honest-vertical-implementation-plan-2026-11-10.md)の通し実装後に行う。

残件は完全なG14費用・資源・期限planner、本番profile resolver、claim/book/escrowと四行writerの受付本経路への接続、court service consumer、有限retirement/release。現frameは単独TTL削除を禁止しており、期限indexに残るframeの掃引費用を有界化するreleaseも公開有効化前に必要。既存の永続holdを完成した保持制度とは扱わない。

FpTokenizer公開routeの不足は未解消。同一Studio claimの公開quote/受付→Panel→Final→当選権利消費と対応blockは **NOT_PROVEN**。モデル取得証明・Minerによるモデル全量配信は追加していない。配備・公開送信・公開probeは行っていない。静的・文書検査はvalidation.jsonに記録する。
