# Native TFP1受付条件と予約保存費用の接続

状態: **IMPLEMENTATION_IN_PROGRESS / WRITTEN_NOT_RUN / 公開有効化なし**。

[受付adapter](../../../../consensus/core/src/palw_positive_contract_v2/tfp1_original.rs)に共通constructorを追加した。実FP・Source・登録classからjob/artifact/minerを導き、literal UTF-8 promptとcold initial/dependenciesのsubjectを既存codecで生成する。profile/resourceを束縛したobligationとの同一性、FPのnetworkと明示policyのnetworkを照合する。署名受付・unsigned照合にもnative networkの検査を適用する。

ruleset・branch・manifest上限はnative ownerが明示する入力として残す。FP anchorをbranchへ暗黙に割り当てたり、fixtureの上限を本番規則へ採用したりしない。今回のconstructorは本経路の完全profile resolverではなく、現呼出しは回帰fixtureである。署名・ancestry・本番profile選択・資金確保はそのownerに必要。

[共通予約処理](../../../../consensus/core/src/palw_positive_contract_v2/tfp1_original/preclaim.rs)では、全選出rankのcarrier storage rentとNode生成fault storage rentをchecked加算し、署名budgetのfeeから既使用feeを引いた残額へ照合する。PMRを含む場合も、そのcarrier料金は同じ合計に入る。unsigned quoteとsigned preparationの両方が同じ判定を通り、不足なら候補bookを確定しない。signed preparationは同じ受付入力の既使用feeを保持し、再予約でゼロに戻さない。これは費用上限の検査であり、burn・実支払・escrow確保を実行しない。

回帰コードはnative graphから求めた保存費用ちょうどの成功、1単位不足、既使用1単位による不足、各失敗後のbook不変、unsigned/signedの同一結果を扱う。cold資料・promptの導出一致、異network・異class・不正UTF-8・不正manifest上限の拒否も追加した。整形・静的レビュー・文書検査の結果はvalidation.jsonに記録する。Rust検証は[採択計画](../../../design/palw/integration/public-one-honest-vertical-implementation-plan-2026-11-10.md)に従い通し実装後。

これは選出Panel向け保存費用の下限であり、全工程のG14見積もりではない。初回供給・outsider・defense・非保存費用を含む完全planner、本番policy resolver、受付filterへの接続、claim/frame/book/escrow/198/204/206の一体適用が残る。FpTokenizerの公開route、同一claimの公開quote/受付→Panel→Final→当選権利消費/blockは **NOT_PROVEN**。モデル取得証明は追加せず、配備・公開送信・公開probeは行っていない。
