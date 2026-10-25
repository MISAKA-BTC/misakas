# RFC12 公開履歴の再取得と一時資料喪失記録 — 2026-10-25

ローカルhostは21:11:19 JSTに再起動した。`/tmp` のビルド・固定binary・旧公開履歴の生データとgzipが失われた。過去のcommitに保存した試験ログ・digestは履歴として残るが、旧segmentの生資料が現在も保存されているとは扱わない。詳細・baseline・実行ログは[JSON記録](0012-history-reacquisition-after-reboot-2026-10-25.json)に保存した。

collectorに `--baseline` を追加し、元のidentityとanchorを検査したうえでgenesisから取得し直す。旧cursor・page数・coverageは引き継がない。identity不一致はpage取得前に拒否し、resumeとの併用、cursor混入、入力過大、不完全なidentity/headerを拒否する。既存resumeの動作は維持した。

既存14件と新規2件、計16親テストPASS。新規対照は実collector remote bodyをfake RPCで実行し、fresh scanの開始点と不一致時のgetBlocks未呼出を検査した。公開nodeへの再取得は進行中であり、終端・after stamp・祖先閉包の確認前に受入済みcoverageを宣言しない。

今後のsource snapshot・binary pin・build log・生履歴は `/Users/wata/.codex/artifacts/rfc06-12-20261025` に保存する。これは一時資料喪失からの復旧であり、EVM互換性、履歴署名、OS crash、実campaign 15 gateの合格を意味しない。各gateはUNKNOWNのまま。
