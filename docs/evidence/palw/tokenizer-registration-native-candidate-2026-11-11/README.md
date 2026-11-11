# M=780 native candidate の機能検査と一時予約の修正

`ebefd0152` の exporter と runner で、同じ Finish payload を異なる outpoint/受理順に置いた780 carrierを模した。source partition と候補 scannerを通し、fee試行2回・巻き戻し2回・observed処理2回・確定fold1回を検査する。初期Open/Partは従来の2パスで再生する。

最大8 MiB入力は修正前、論理RAM 4 GiBで停止した。共通writerが状態を変えないFinish再送の一時予約を返さず、予約済み4,290,166,219 bytesへ5,179,808 bytesを追加しようとした。物理RSSは148,078,592 bytesであり、実メモリ枯渇とは異なる。RAM上限だけ8 GiBにした比較は成功したが、上限引上げを修正として採用していない。

`098da5e04` はjournalの長さと容量が変わらず、fee照合にも成功した場合に一時予約を返す処理を共通writerへ移した。実際の行変更、失敗Finishの試行記録、journal容量増加は予約を保持する。修正後、同じ4 GiB上限で実入力24.08秒・最大入力70.48秒で成功した。確定foldではFinish成立1件・消費slot1件・状態を変えない再送779件で、費用とrootも期待値に一致した。

通常コア回帰は25件成功、exporter1件ignored。追加の分離プロセス検査では、不正署名と完了後再送の予約返却、成功Finishと失敗試行記録の予約保持を検査した。子プロセスの1件は25件に含め、別の成功数として加算しない。

[集計・入力digest・制約](summary.json)と修正前後の原文ログを保存する。macOS opt-level 0・64 MiB worker stackの機能検査であり、実DAG/Round、公開T12の障害原因特定、性能・料金の採択やTFP1受付成功を示さない。修正後のLinux release測定は別途必要である。
