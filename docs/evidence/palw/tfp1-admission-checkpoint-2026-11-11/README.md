# 通常受付の保存境界と一時資源所有

状態: **IMPLEMENTATION_IN_PROGRESS / Rust build・test NOT_RUN / 公開有効化なし**。

前の変更`17d0f13e4`はclaim固有Traceのnative量を予約下限と供給側の照合へ接続した。今回は最優先の原子的受付について、実際のFreePrompt dispatcherの保存境界を修正する。モデル取得証明は実装しない。

## 本経路の変更

[objects.rs](../../../../consensus/core/src/palw_state_v2/objects.rs)のFreePrompt受付で、public frame、prefix依存、funded pairingのいずれかを持つ候補を一つの保存境界で囲む。funded pairingがあれば、legacy frame mapが欠けていても保護対象になる。欠けたframeや未完成profileを受理可能にする変更ではない。

従来は、既存court serviceがある場合に外側の保存とpaired originalの保存が重なっていた。paired候補では一つに統合し、claim・frame・service book・escrow等の書込み後に失敗してもstateとjournalを戻す。court観測とprefixの一時値も元の所有者へ戻す。nested dispatchが別objectを処理する場合は、そのobject自身の保存境界を持つ。courtの失敗時課金規則や累積workを払戻しへ変更しない。

[資源所有者](../../../../consensus/core/src/palw_native_resource_v1/transition.rs)は、実際の型付きstateのコピー量を保存前に計上し、既存builderと同じcheckerから一時予約する。既存checkerがなければ、通常processの予約経路を使う。コピーの予約は成功時の破棄または失敗時の復元が済むまで保持する。builderの長期予約へ同じコピー量を累積しない。これは全stateコピー自体を局所undoに置換する変更ではない。

[198/204/206の局所undo](../tfp1-local-insertion-undo-2026-11-11/README.md)は別に維持する。三行writerだけでclaim全体の原子性が完成したとは扱わない。

## 検証と残作業

回帰コードは、既存court serviceがある状態での受付後半のdeposit不足、コピー予約が1 byte不足する早期拒否、保存用予約の反復解放と同時二重予約拒否を扱う。いずれも **WRITTEN_NOT_RUN**。静的レビュー、整形・文書検査の結果はvalidation.jsonに記録する。[採択計画](../../../design/palw/integration/public-one-honest-vertical-implementation-plan-2026-11-10.md)の順序に従い、Rust実行は通し実装後とする。今回の静的確認を実行成功として扱わない。

TFO3の新profileには、完全なnative plannerからのclaim/frame/book/escrowと198/204/206の一括接続が残る。G14の費用・資源・期限の全枝計算、FpTokenizer経路、本番候補filterも未完成。typed profile refusalは維持する。

公開T12で同一claimのquote成功・署名・受付、Panel検証・Final、当選権利消費・対応blockは **NOT_PROVEN**。今回の実装変更を配備や公開確認として扱わない。
