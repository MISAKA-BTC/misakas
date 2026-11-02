# 全機能の実装完了計画 — 2026-10-31

状態: **作業中・完了未証明**。ユーザーが添付評価文書の実装完了をgoalとして明示した。

完了条件は、採択済み・初期mainnet対象の全機能について正常系まで実行できる経路を完成させ、同一候補で全体ビルドと必要な対象試験を通すことである。公開配備と長時間の運用キャンペーンは今回の完了条件外。画像・動画、モデル改善、未採択RFC-0027は対象に戻さない。

[恒久原則](../../../../PRINCIPLES.md)を維持する。モデル計算の暗号学的証明は使用しない。モデル係数はgenesis/年次指定1.0、後日登録0.3で固定し、モデル単位の審査・昇格を追加しない。全モデルでclaim単位の認証とCourtを維持する。

## 候補と保全

既存全機能作業ツリーの425ファイルをハッシュ付きで保全し、ローカルcheckpoint `7da6d70ae`へ保存した。作業ブランチは`codex/full-implementation-completion`。最新mainのCI修正と実装はmerge commit `cfaa62b6d`で保全済み実装へ統合した。その後の担当別差分を検証中である。元の未コミット実装、過去の失敗ログと未検証状態を消して成功扱いにしない。

G14-onlyのバイナリ、過去の19 suite、新たな限定候補の試験は、それぞれ記録されたソース・範囲の証拠である。全機能候補の成功証拠に付け替えない。G14の既存予定はDAA18400、FULLは`None`を維持する。このgoalは公開ネットワークの追加有効化を宣言しない。

RFC-0037の採択とversion割当、およびRFC-0042/ADR-0210の新双方署名供給契約の採択は、会話内のユーザー承認済みとして引き継ぐ。旧署名・wire・IDの意味を新契約へ遡及変更しない。

## 実装と受入の担当

| 対象 | 担当 | 必要な正常系・境界 |
| --- | --- | --- |
| RFC-0039 verifier/provider報酬 | 支払担当 | 実署名、全額保留、期限内default/conviction返金、期限後支払、物理queue、一回性、復元・reorg |
| 有料Courtと材料・保存料 | 外部A（②③） | 実payer/UTXO、正常schedule、実free principalからの正料金徴収、予約済み資金の非侵食、escrow精算・burn・refund |
| 全Court分岐と四役割供給 | 外部A（④） | TFO2、認証済み実inventory、全分岐の資源・資金・供給・保持の原子的予約、実材料による裁定、処理機会を保つ時計 |
| RFC-0023/0040依存・Prefix | 内部依存担当（⑤）・外部B（⑥） | 独立Bの進行、A依存Cの保留、A有罪のA/Cへの限定効果、実birth identity、材料保持・GC、pruning/IBD/restart/reorg |
| RFC-0037 IBD/P2P | 外部B（⑧） | opt-in version109、実peer転送、完全inventoryとselected-chain witness、通常full import、資源不足とpeer不正の区別 |
| RFC-0038回答会計 | 外部B（⑨） | 実accepted first carrierの永続化、正しいtxへの結合、破損拒否、欠落時の一意fallback、復元・reorg |
| RFC-0012 native39→EVM | 決済担当 | 実issuance/spend、固有W/PWUとanchor/成熟/帰属、詐欺取消、EVM Safe/Finalizedまでのconsumer |
| RFC-0041無genesis bond起動 | 外部C（⑩） | 実BASE回復、支出可能資金、通常bond登録・成熟、最初のPALWとclaim認証、再起動・reorg |
| 同一候補の全体検証 | 統合担当 | 本番node/CLI/SDK/P2P/EVMを含むビルド、厳密な対象試験、ソースとbinaryのSHA、前後一致 |

詳細な既存受入条件は[完了条件台帳](../../../../evidence/palw/mainnet-adopted-connections-2026-10-30/completion-requirements.json)にある。台帳の過去の診断結果を最終同一候補PASSへ自動昇格しない。

## 現時点の確認

- 旧ZIP時点の不足には、既存作業ツリーで既に実装されたものがある。schedule取得やescrow接続の古いコメントだけを根拠に再実装しない。
- 有料受付・保存料の一部に、総担保のみを見て既予約額を差し引かない徴収経路を確認した。実free principalの検査と、正料金成功・不足時の原子的拒否を修正対象とする。
- main統合の64競合を解消済み。coreおよびEVM有効consensusの診断用試験ビルドは成功し、報酬・escrow・Prefix・回答会計等の対象試験を実行している。失敗からProviderのqueue対応保存順序、Prefix正常入口、TFO2期限の未接続を修正中である。後続差分を含む全体の同一候補PASSは未証明であり、実装完了とは判定しない。
- 測定・設計承認・コード・試験・有効化・配備を区別する。新しい測定値やwaiverを捏造せず、認証・責任・予約の条件を弱めて試験を通さない。

## 初期mainnetと隔離実装の境界

[RFC-0033](../../../../rfc/0033-d2-readiness-root-opening-availability.md)と[ADR-0200](../../../../adr/0200-d2-readiness-root-opening-availability.md)のC1は詳細仕様化・隔離実装の承認であり、公開金融契約・pre統合の採択とは区別する。後続のdraw/fee/slash数値決定だけで未接続の資金移動を完成扱いにしない。隔離codec・literal署名・開示・期限・no-supply・undoの検査を記録し、公開ready/信用を迂回して許可しない。RFC-0034も採択済み隔離S1範囲と未採択の公開有料dispatchを区別する。

RFC-0016のレポートは既存の受付用容量計算へ接続し、認証済み入力が揃う場合のcapacity/shortfallと、欠測時のNoneを検査する。診断用試験の成功を全体の最終候補の証拠へ付け替えない。

## 最終判定

全対象について実行可能な正常系と不正・不足の拒否が成立し、同じ固定候補から作った実行ファイルで必要な試験が成功した時点で判定する。ゼロ件一致、失敗実行、後続の未検証差分、テスト用注入だけの成功は完了証拠にしない。未完の対象が一つでも残る間、goalをcompleteにしない。
