# 専用TFP1の保持期限・資金消費者

状態: **IMPLEMENTATION_IN_PROGRESS / WRITTEN_NOT_RUN / 公開有効化なし**。

前回の候補予約・実payer検査の後、本番受付へ繋ぐため既存consumerを監査した。旧escrow解放処理は旧TIR frameだけを参照し、専用frameのhard deadline後の資料保持期限を読まなかった。さらに旧qualified spendは、全額を決済する場合、liabilityがopenでもsettledにできた。この二経路を専用profileの受付前に分離する。

[共通資料期限reader](../../../../consensus/core/src/palw_staged_court_fold_v1.rs)は旧source indexの最大期限と専用frameの固定pinの最大を返す。これにより、実claim退役、settlement liabilityの退役下限、prefix export資金保持が同じ資料期限を読む。FPの再serialize/Source再構築や新しい派生indexは追加しない。専用keyに不正なrowがある未検証候補は無期限holdとし、正式なstate importは既存frame/link検査で拒否する。

[escrow fold](../../../../consensus/core/src/palw_state_v2/funded_escrow.rs)の専用frame経路は、hardと資料保持期限、frame identity/Source/original DAAが一致したlive bindingまたはcanonical release、全packetの決着、資料case、court/clock、terminal dispositionを検査する。資料応答が来ただけでaux199のpending caseは解決した扱いにしない。現在は専用PublicOriginalのterminal/dispositionから決済権限を作るwriterが未接続なので、これら必要条件が成立してもholdを維持する。旧claimのFinal、Voided、時刻経過、refutation locatorを専用決済の代行にしない。direct clearと旧settlement factの適用も専用frameでは拒否し、状態・journalを変更しない。有限discharge/releaseは未完成であり、このholdを完成した決済と呼ばない。

[Node共通spend filter](../../../../consensus/src/pipeline/virtual_processor/utxo_validation.rs)は、専用frameの未決済outpointを旧qualifierより先に拒否する。[親stateからの投影](../../../../consensus/src/pipeline/virtual_processor/processor/palw.rs)をblock walk、virtual/template、mempoolへ接続した。専用frameのrooted lockは旧TIR activation fenceと独立し、TKRの手動preflightも同じ集合を読む。旧返金・fee scheduleが専用資金を消費せず、拒否された旧spendにburn/fee creditを計上しない。正規の専用決済を有効にする際は、この保護集合からの移行をnative settlement ownerに明示的に接続する必要がある。

回帰コードは同じ実paired-frame保存fixtureの保持期限をhard+10にし、delta apply/revert、snapshot復元、退役下限、破損rowでのhold、service binding欠落/未決着/別scope、資料caseが読めない状態、旧Finalだけでの解放拒否、旧全額決済factでのstate/journal不変を追加した。Node側は既存のmempool/block共通判定fixtureで同じspendを専用資金に分類すると拒否する。正のburnが旧経路では計上され、専用資金では0となる対照と、旧row投影が空でもlockが効くコードを追加した。これらはコード追加であり未実行。公開Final・実決済の成功fixtureではない。

本経路の残件を具体化した:

- `objects.rs` の旧prepare/admit/installとretirementは旧Frame依存。専用planner成功ownerからの分岐、claim/book/escrow/専用frame/198/204/206の同じcheckpoint内保存が必要。
- complete profileのnative branch、全工程の費用・時間、payee/settlement、全rankのcausal bookをまだ導いていない。
- TFO3 literalはinner funded original・prompt・companionを持つが、HF descriptorのpreimageは含まない。native受付で必要な固定参照metadataを、認証済み入力として取得する経路が必要。これはモデル取得可能性の証明ではない。
- Node minimum-rentは旧quote、候補rehearsalはsingleton。専用quoteと全prefix再演への接続が必要。
- 独立Panel approval、PublicOriginal terminal/dispositionと有限決済・release、FpTokenizer公開routeは未完。

Rust build/check/testは[採択計画](../../../design/palw/integration/public-one-honest-vertical-implementation-plan-2026-11-10.md)の順序どおり未実行。整形・静的レビュー・文書検査のみ。モデル取得証明・モデル重みのMiner全量配信は追加しない。公開配備/送信/probeなし。同一Studio claimの公開quote/受付→Panel→Final→当選権利消費・対応blockは引き続き **NOT_PROVEN**。
