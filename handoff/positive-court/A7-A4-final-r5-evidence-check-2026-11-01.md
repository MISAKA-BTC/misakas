# A7 — A4 final-r5証拠の照合（2026-11-01）

対象source `c1292bc085cbfc1ca24a8ac0a962ab1096a88f23`、tree `b5b2ea1f5a103f513779d4aba9eb82106df59fdf`、証拠commit `34728f1b9`。親と独立担当によるread-only照合であり、A7によるCargo再実行は行っていない。

`final-r5/ci.log` の29 summaryは300成功・失敗0・ignore0と一致。manifestの選択118 source SHAは固定sourceとすべて一致し、raw log SHAは `445130a4a679a8aa1480ddba91d83536c6968cba6bc9edeb2733d94a919072f5` と一致した。CI scriptのfail-fast設定、consensus全testsの型検査と後続処理の成功記録は、manifestの型検査exit0と整合する。証拠commitはsource以後の文書・証拠だけの変更。

qualified試験名300は一意だが、leaf名18個はNode/coreの2driverで再利用される。文書も300の独立scenarioとは扱わない。P1/P2を修正した `coordinator/contract.rs` と `carrier.rs` は、A7が先に静的レビューした `f6c7d9f4d` と同一。

**このA4 sourceはA5 ready `30f3fd23e` の8用途APIを含まない。** `ports.rs` / `credit.rs` は異なり、`credit/consumers.rs` もこのsourceには存在しない。300件はA7 source-ready `70e28e07f` のentitlement/WorkerPayment変更のruntime証拠にはならない。A7の歴史的243件とも合算しない。

actual176 one-writer・実UTXO lock・native完全case journal・具体readonly store・T12・DB reopen・reorg・IBDは未完了と文書に明示されている。構造的Full-provenance反例4件をlive受付成功と扱わない。選択sourceとowner生ログの整合確認であり、A7の全source集合・lock・binary・group終了を固定した統合候補検証の代用ではない。

A7の実装source・既存raw/digest・公開activationは変更なし。次の全candidate指定後に、ready8用途と専用fence8試験を含めた同一候補を検証する。
