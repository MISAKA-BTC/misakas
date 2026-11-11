# TFO3受付に向けた全prefix再演

状態: **IMPLEMENTATION_IN_PROGRESS / Rust build・test NOT_RUN / 公開有効化なし**。

前回 `a099f82dd` は実UTXOと履歴prefixのhandoffを接続したprogress。今回は、そのprefixをsingleton rehearsalのjournalで代用できない問題への実装を進めた。Studio由来の同一claimの公開受理・Final・当選時権利消費・対応blockは引き続き **NOT_PROVEN**。

## 共通の実行経路

[ordered prefix](../../../../consensus/core/src/palw_state_v2/ordered_prefix.rs)は、actual v7 foldの初期化・事前処理とnative ordered loopを共有する。親状態・全object・own/merged work・execution key・extrasから入力計上を導き、queue drain、sweep、bootstrapを一度だけ実行する。ordinary/BCT/TKR/raw Assignmentを同じbuilder・journal・資源予約の下で適用する。初期化の順序、actual foldの後段処理、旧singleton入口の意味は変更しない。

新しいread-only入口 `palw_v2_replay_accepted_prefix_v1` は全候補を再演し、状態とjournalを一体保持する。公開するのは借用したprefix stateとjournal長だけで、partial deltaや確定済みstateへ変換するAPIは設けない。後段のGC・LastPoint・Final・ブロック処理を実行済みとはしない。失敗した候補は全体を破棄する。候補が変わった場合はraw位置・入力計上を含む全入力で親から再演し、singleton deltaの長さを足してglobal位置にしない。

仮想ORIGINとzeroの包含点を拒否し、court telemetryはnested guardで確定foldの観測から分離する。資源予約は返されたownerの寿命まで保持する。再演の利用者が候補を反復除外する場合の回数・累積work上限は、これとは別に必要である。

## 確認と未接続部分

独立静的レビューで、初期処理は元v7と同じ内容、ordered loopも借用表記以外は同じ順序と確認した。[回帰コード](../../../../consensus/core/src/palw_state_v2/tests/ordered_prefix_v1.rs)は、非空のqueue drain、複数object、途中の重複拒否からの全再演、actual deltaの先頭とのbytes一致、LastPointを更新しない境界、仮想点と非単調点の拒否を記述する。これはledger fixtureであり、公開受付・署名・raw/TKR混在の統合検証ではない。元計画に従いRust build/testは未実行。

本適用は共通関数へ接続したが、nodeの候補filterはまだ旧singleton経路である。新しい全再演を使うfilterには、拒否位置・carrier種類の識別、物理ordinalの一致、明示的な累積work上限が必要。TKR単独fenceへの依存、PMR dispatcher、完全なTFO3共通plannerと原子的installは未解決である。

並行読解により、次の受付接続も必要と確認した。

- TFO3は旧 `prepare_paired_original_v1` のallocation経路より前に分岐し、allocation非依存のnative projectionへ進める。
- 公開quoteとintakeで同じprofile・上限・予約処理を使う。現op248はprompt/manifestを受け取らないため、正確なcompanion IDによる全予約の成功を返せない。
- 有限な署名対象budgetを先に固定し、そのIDで予約した実slot/費用が収まることを確認する。予約結果から署名IDを再計算する循環を作らない。
- 現予約はdrawn rankと初回要求まで。claim固有の初期供給・outsider・防御・回復・保持を共同予約へ接続する。モデル重み全量配信やHF取得可否認定は追加しない。

配備・送信・新しい公開probeは行っていない。`PublicOriginalProfile`拒否を維持する。
