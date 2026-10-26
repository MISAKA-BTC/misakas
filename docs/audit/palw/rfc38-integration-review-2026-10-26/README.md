# RFC38 7レーン統合レビュー・Q1〜Q9判断 — 2026-10-26

対象 `dd4582bc877ad9f2eed43e591e42b6f1fbbdc05d`、origin `studio/rfc38-integration` の同一SHAを確認した。7レーンの差分は存在する。第1巡の部分成果であり、RFC全体や実node campaignの完了ではない。全15 gateはUNKNOWN、full activationはNoneを維持する。

## 受入前の修正事項

1. P2 — `scripts/drills/rfc_gate_inputs.py:83–86`：不正JSONを末尾以外でも無視し、重複scenario IDを後勝ちにする。FAIL→壊れた中間行→PASSからPASSになることを実関数で再現した。壊れた末尾だけを許し、重複は拒否または過去FAILを保持する。attemptを区別するならrun/attempt identityを正式に分ける。
2. P2 — `scripts/drills/misaka-rfc-integration-campaign.py:70–79`：aggregate-rootの保存freeze/onceと現在plan・candidateの照合がない。別実行のledgerを現在candidate名義で再出力できる。全体UNKNOWNでも個別gateの証拠帰属が崩れるため、再集計前に同一実行を照合し、不一致は拒否する。
3. P2 — `consensus/core/src/palw_row_stream_v1/resume.rs:159–163`：PermissionDenied/TimedOut等のlocal I/OをResumeSpillへ分類する。ADR-0204の容量・I/Oはlocalという境界と一致させる。現状のnode-local部品の問題であり、実P2Pでpeerが罰された証拠はない。

報告も2点訂正する。Lane Eの「refused carrier is never mined」は不正確で、fold登録から落ちたcarrier TXがaccepted dataに残り得る。RFC34 S1の詳細仕様化・隔離実装はADR-0201とRFC34末尾で2026-10-21ユーザー選択済みであり、再承認待ちへ戻さない。本番採択・料金・有効化とは別である。

## 試験と保存

既存横断ログから91、19、47 PASSを確認した。これらは既存実行のログ確認で、今回全Rust suiteを独立再実行した件数ではない。Python gate-inputの4テスト、diff check、docs checkは今回実行しPASS。docs既存不足43・新規0。

統括run2.shのbinary検索はtarget名kaspadだが実名はkaspad_libである。空のBINでshasumが標準入力待ちとなり、元parent試験は始まっていなかった。元processは変更せず、正しいtarget名・source manifest・SHA固定binaryで2親を再実行する別harnessを開始した。独立再実行はbuild exit 0、親2件PASS、試験前後source SHA一致。binary 3本のSHAと親の全ログをreview.jsonへ保存した。子processを別テストとして加算しない。

元一時ログは永続artifactへ複製済み。source SHA、保存ログdigest、再現結果は[review.json](review.json)。fixture/in-process/親試験を実campaign PASSへ換算しない。

再確認の結果、Lane Dの2親はこの統合sourceで通る。ただし下記P2を直して第1巡の部分成果として統合することを勧める。RFC全体の完了件数は更新しない。

## 判断

以下は実装方針の推奨であり、未採択の料金・wire番号・新合意設計を採択済みへ変更するものではない。設計凍結と既存の明示的な承認範囲を維持する。

| ID | 判断・次に進める範囲 |
|---|---|
| Q1 | exact carrier記録は必要。canonical foldの受付時にversioned rooted claim→carrier mapへ記録する案を選ぶ。旧claim構造への無条件field追加はしない。fence・旧履歴の扱い・rollback/checkpoint/import・実spent-input依存辺を一緒に仕様化する。 |
| Q2 | worker独自の自己申告identityは採用しない。own-nodeのnative descriptor/quoteにjob、network/genesis、artifact/profile、ruleset/fence、import世代を束縛する既存契約へ接続する。completion時に同じcontextを再照合し、FP actual countは既存規則で扱う。新しい信頼policyを待つ理由にはしない。 |
| Q3 | 金額は実測なしに決めない。実原資・履行・payee・未払duesをrootedにし、既に成立した支払債務を消して期限だけで全額返還する挙動は採らない。責任・保持を解消し、決済後の残額だけpayerへ返す。RFC35 §2 Aの保存則に沿って整合させ、本番scheduleは導出と承認まで拒否を維持する。 |
| Q4 | RFC34 S1は既承認の隔離実装を続行する。RFC33 fee/slashやRFC27全体は一括採択しない。客観的違反、責任者、支払原資、返還条件、単位、非二重計上を個別に確定する。取得不能を計算不正にしない。 |
| Q5 | native39写像は新しい明示仕様が必要。W/PWU、anchor、operator、成熟・retention、void/reorgを写像表と不変条件として先に固定し、仮の換算値やoperator推定を使わない。決済陽性gateは未策定の間UNKNOWN。 |
| Q6 | providerの探索とL2検証を分離する。既存P2Pを使った取得先探索を優先し、探索先は信用しない。checkpoint以降のtransitionを既存claim承認/court規則で独立検証する範囲、鮮度・競合・欠落処理をRFC09へ明記する。計算正当性の暗号学的証明は使用しない。 |
| Q7 | node-local row streamをそのままwire採択したことにしない。message/codec表、長さ・件数上限、resume認証、互換性を仕様化し、既存registryとの衝突を確認してprotocol番号を予約する。本番配備は別段階。 |
| Q8 | calendar・原子的予約更新は先行実装する。混雑extensionは署名済みpolicy、費用・保持資金、全依存termsの原子更新を要する。a+10,000内、E_max≤6,800という既存上限を値の採択に読み替えず、未採択中は自動延長しない。 |
| Q9 | 再集計の2欠陥を修正後、新候補source・binary SHA・feature・入力digest・15 gate別要求証拠を凍結する。実入力のあるシナリオから実複数process試験を進め、K3未作成・写像未策定や未実行scale/OS crash等はUNKNOWNを維持する。今回のfixture件数をcampaignへ転用しない。 |

branch統合とdeploy/activationは別であり、このレビューで共有branchへのmerge・push・deployは行っていない。
