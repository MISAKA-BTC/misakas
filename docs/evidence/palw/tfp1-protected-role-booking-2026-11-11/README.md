# TFP1役割別の共通予約と専用frameのサービス接続

状態: **IMPLEMENTATION_IN_PROGRESS / WRITTEN_NOT_RUN / 公開有効化なし**。

[共通preclaim予約](../../../../consensus/core/src/palw_positive_contract_v2/tfp1_original/preclaim.rs)に明示的な新resource/domainを追加し、選出rank、第三者rank、防御枠を一つの候補bookへ予約する。各役割のbytes/work/保持量をその役割のallowanceへ照合し、packet上限とcarrier/fault保存料は全枠で合算する。不足なら候補全体を破棄する。既存の選出枠だけのAPI・識別子・署名の意味は維持する。

第三者と防御も現在の資料搬送・局所検査graph全体で保守的に予約する。防御枠は`ProtectedRescueDemand.defensive`の一つのaggregate枠であり、そのgraphを予約しただけでは、任意数の独立した防御caseの並行実行を保証しない。初回要求を指定した場合は各枠のgraphに包含・保存費用を加えるが、防御枠の要求packetは容量予約であって、選出者・第三者の申告資格を付与しない。初回資料供給、抽選、notice、Panelの全体検査、全回復・終端の追加工程と実資金確保を含む完全profileはまだ未完。

[サービス状態](../../../../consensus/core/src/palw_court_service_state_v2.rs)は専用TFP1 frameを認識し、claim、frame identity、Source、元受理時刻、hard期限を照合する。旧allocation/custodyへフォールバックしない。旧bindingは旧意味のままで、専用frameの新claimだけがdomainの異なるframe identityを参照する。原子的受付内でLastPointが進む前もnative claimを照合でき、時点の認証は四行writerと最終state検査が担う。bond holdは当該producerを参照し、存在しない旧モデルcustodianを理由に全bondを保持しない。他actorの独立した責任・holdは変更しない。bond照会は検査済みstateの固定pinを読み、FP本文の再hashを行わない。

[有料資料要求](../../../../consensus/core/src/palw_positive_contract_v2/tfp1_original/manifest_request.rs)と[継続の復元](../../../../consensus/core/src/palw_positive_contract_v2/tfp1_original/native_link.rs)にも、明示protected termsから選出rankのscopeを導く入口を追加した。署名companionのobligation/rescue、実抽選rank、予約packet、実UTXO fee、paid rowの照合を維持する。第三者・防御の容量を選出rankの権限へ転用しない。frame/linkの版は保存形式を指定するものであり、予約domainの自動選択ではない。新domainの本番resolverとdispatchは未接続で、既存domainへの暗黙変換はしない。

回帰コードは2選出＋1第三者＋1防御のscope分離、各役割だけの容量不足、packet/費用不足時のbook不変、保存料ちょうど、署名前後の共通予約一致、第三者0枠を扱う。専用frameには既存の実署名draw/request fixtureを通し、service scopeの差替え拒否、native service writerの保存・apply/undo、有料要求writerと継続を接続した。このfixtureのサービス単価・packet・fundingはfixtureであり、完全plannerで生成したprotected TFO3の通し受付ではない。新role-domainのpaid入口と継続を同じ完全候補で通す回帰は、そのcoordinatorとの接続時に必要。

Rust build/check/testは[採択計画](../../../design/palw/integration/public-one-honest-vertical-implementation-plan-2026-11-10.md)に従い通し実装後にまとめる。整形・静的レビュー・文書検査のみ実施し、結果はvalidation.jsonに記録する。公開T12へ配備・送信・probeは行っていない。

次の本経路接続には、完全plannerと実fee/capital、受付前の同一候補book、frame/claim/escrow/履歴の一括install、Nodeのminimum-rent/全prefix再演、RPC quoteを同じprofileで結ぶ必要がある。有限retirement/release、FpTokenizer route、公開同一claimのquote/受付→Panel→Final→当選権利消費・対応blockは未完・**NOT_PROVEN**。モデル取得証明やMinerのモデル全量配信は追加していない。
