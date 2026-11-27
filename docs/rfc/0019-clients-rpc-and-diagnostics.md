# RFC-0019 — Studio・Miner・RPC・原因診断

- 文書日付：2026-11-26（Asia/Tokyo）
- 文書区分：**DESIGN_SPECIFICATION**。採択済みの規則を統合する。提案に限る部分は該当節で明示する。
- 判断理由：[ADR-0019](../adr/0019-clients-and-cause-preserving-diagnostics.md)
- 規範入口：[設計原則](../PRINCIPLES.md)／[RFC一覧](README.md)
- 本文は実装の設計を定める。実装・実測・公開有効化の完了を示すものではない。

## 1. clientとNodeの分担

Studio/Minerは認証済み実行profileで推論・claim署名・資料供給を行う。Gateway/RPCは見積もり・提出・状態照会を提供するが、合意上の受理・認証・決済の代わりにならない。

remote clientがNodeを持たない構成でも、帰属、署名対象、受取先、原資、期限、送信先networkを固定し、秘密鍵をNodeの保管へ渡す前提にしない。Nodeの応答は選択親・状態rootと結び付けて照合する。

## 2. 一件のclaimの診断

次の段階を独立に返す：model採用、技術互換、資金/役務予約、実行、claim受付、Panel、資料/Court、Final、公開Court条件、Ticket、権利、受賞block、報酬出力、決済frontier。

各段階は状態、理由code、対象claim、選択親、state root、受付位置、元期限、次条件、再試行可否、根拠recordを持つ。`found=false`、診断API非対応、未登録、未同期、ローカル故障を同じ空応答にしない。

## 3. 型付き原因

外部境界では共通のerror envelopeを使い、内部の下位原因をsource chainとして保持する。message文字列は表示用であり、`contains("halted")`等で制御を分岐しない。

| 区分例 | 意味 | 原則の再試行 |
| --- | --- | --- |
| NotFinalized / DependencyPending | 正規の前提未成立 | 対象状態が変化したとき |
| OutsideWindow / Expired | 元期間外 | 同じ要求の反復不可 |
| Halted / PolicyStopped | 意図された安全停止 | 正規解除・明示復旧後 |
| Corrupt / ChecksumMismatch | 保存内容の破損 | 整合した復元後 |
| Conflict / AlreadyConsumed | scope競合・一回性違反 | 原因に応じ再計画。無条件再送不可 |
| NativeUnavailable / SupplierUnavailable | 必要な状態・資料にアクセス不能 | 取得可能性が変化したとき |
| LocalResourceExhausted | RAM/disk/IO予約拒否 | 資源回復後 |
| ArtifactRootMismatch / FundingMismatch / CurveMismatch | 特定の決済条件違反 | 元入力・設定の修正後 |

`UnknownClaimContext`や`ReleaseContext`一つで異なる失敗を潰さない。`filter_map(... .ok()?)`で拒否理由を捨てる場合は、成功集合とは別に有界な拒否診断を保持する。

## 4. TB1・Court・市場の診断

TB1はround/purpose/roster/freeze/公開gate/必要share/停止又は破損を区別する。Courtは未裁定、資料不足、期限、包含機会、convictionを区別する。MOE3はquote anchor、reserve root、curve、署名済み原資、既消費slot、dust、期限を分ける。

機密share、秘密鍵、認証tokenをerrorや診断RPCへ出さない。公開root、理由、record参照、非秘密parameterで再現可能にする。

## 5. 冪等な提出と復旧

署名済み資金tx・claim carrier・受賞候補のbytesを保存し、再起動後は同じ内容を再提出できるようにする。txidだけを保存して再送不能にしない。

RPC受理、mempool、block包含、選択鎖、決済を別に観測する。署名済み予約DAAを時計の高速化で飛び越さず、親変更で失効したcandidateは元文脈へ照合して作り直す。

## 6. 3peerの照合

同じ選択tipと状態snapshotで、各peerから独立にregistry、claim、Panel、Court、資料義務、権利、UTXOを読む。tipがずれた応答を単純比較して不一致と断定しない。

初回submitBlock、他peerへのP2P伝播、重複再提出を別記録にする。RPC成功だけを受賞成功と表示しない。実報酬が選択枝から外れた場合はcheckpointを再監査する。

## 7. 受入条件

型のvariantとscopeに対してテストする。未知errorを無期限retryにせず、retry budget・次条件・deadlineを持つ。同一条件の反復、ログ割込み、起動競合、欠落stderr、未対応RPC、破損履歴を検証する。

## 9. 型・値・診断の公開契約

CLI、RPC、SDK、Studioはnativeと同じhash幅、address checksum、asset単位、model/profile、署名domainを使用する。base-unit整数をfloatへ変換して予算・残高・Ticketを比較しない。JavaScriptで安全整数を超える値は整数文字列又は規定のbig integer codecで搬送する。

親状態に依存する見積もりは`selected_parent`、policy identity、予約分解、必要額と利用可能額、元署名slotを束縛する。stale見積もりを別の親で受理済みと表示しない。受付・mempool滞留・選択履歴包含・Final・公開Court条件・当選・消費・決済を別段階にする。

原因を保持するerrorは下位原因、再試行分類、claim/case、parent/rootを持つ。transportで文字列へ変換しても機械判定用codeを失わない。`Halted`と`Corrupt`、`NotFinalized`と`OutsideWindow`、`Conflict`と`NativeUnavailable`を同じ文字列照合へ縮約しない。未対応RPCを正常な空結果又は不存在としてcacheしない。

web資産操作は実network、asset、受取人、額、fee、署名対象を確認できる形で表示する。EVM/Nativeそれぞれの同一asset対応、Positionの直接移転と通常AMM売買、claim役務feeとcoinbaseを混同しない。

## 10. 後発監視者の再開と診断

監視サービスは選択履歴のclaimと承認を列挙し、model、tokenizer、実行profile、入力とtraceのrootを照合して独立再実行する。保存する位置には選択blockを含め、再起動時に未処理分へ追いつき、reorgでは外れた枝の判断・送信状態を照合して元期限から再開する。署名、notice包含、証拠、個別裁定、回収、支払いまで追跡する。

診断は初回期限超過、元証拠との不一致、資料取得不能、Court容量不足、責任拘束中、既回収、裁定待ち、回収待ち、決済待ちを型付きの原因・対象・選択枝・期限とともに返す。再試行を表示文言の部分一致で決定しない。
