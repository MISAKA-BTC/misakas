# RFC-0031 — 共有courtのinitial notice境界

Status: **Draft / Review Required** (2026-10-21)。正本は[RFC-0026 §2.2](0026-court-policy-master-contract.md#22-境界表f09値は未決形式だけ固定)と[ADR-0192](../adr/0192-court-service-opportunity-and-shared-pursuit.md)。判断は[ADR-0198](../adr/0198-shared-court-initial-notice-boundary.md)。旧claim・t12・既存tag/clockを変更せず、本番採択・有効化は含まない。

## 1. 固定値と未決の一点

`a`はoriginalの実accepting-chain DAA。通常申立ての終端値は`a + 3,120`、hardは`a + 10,000`で固定する。両方をchecked arithmeticで求め、overflowは受付拒否とし、飽和値や別の起点に置換しない。

未決なのはinitial noticeが終端DAAと等しい場合の受理である。RFC-0026は等号を未決としており、既存`collect_until`の排他的境界や旧courtのinclusiveな期限をそのまま流用する根拠にはしない。

| 候補 | initial noticeのDAA区間 | 最後のnotice DAA |
| --- | --- | --- |
| A（詳細仕様化の推奨） | `a <= d <= a + 3,120` | `a + 3,120` |
| B | `a <= d < a + 3,120` | `a + 3,119` |

Aは「3,120まで」という終端を含める明示方式である。3,200へ変更する案やhardを後ろへ動かす案ではない。Bは新profileの全受付窓をhalf-openで表しやすいが、Aより最後のnotice DAAが一つ早い。どちらも実供給・適格性・全残作業の予約を別途要求する。

## 2. 機械的な境界表

| notice DAA | A | B |
| --- | --- | --- |
| `d < a` | 拒否 | 拒否 |
| `a` | DAA条件を満たす | DAA条件を満たす |
| `a + 3,119` | DAA条件を満たす | DAA条件を満たす |
| `a + 3,120` | DAA条件を満たす | 拒否 |
| `a + 3,121` | 拒否 | 拒否 |
| `a + 10,000` | 拒否 | 拒否 |

「DAA条件を満たす」は受付成功や処理機会の供給を意味しない。元claimが実際に先に受理済みであること、現在の署名/資格、canonical object順、原資・全分岐予約を満たす必要がある。同一DAA内の前後関係をDAAだけで推測せず、実block/受理順を使う。原則としてservice機会は必要な全前提の実成立DAAより後から計算し、noticeそのものを実供給にしない。

## 3. adaptive要求と原子受付への接続

initial noticeと後続adaptive要求を別のnative状態として検査する。後続要求は元の適時notice・同一original/source/rules/profileと全分岐予約に束縛し、後続要求や再投稿でnotice起点やhardをresetしない。ownerや最初のrequesterを共有経路の独占権にしない。

claim共通noticeの重複統合、後続actorの資格と費用、signer義務、全分岐・最遅提出の処理余裕はcoordinatorの別条件である。本書の等号選択だけではその実装・採択・保証を満たさない。現在の共有data候補のhard排他的検査を変更したり、旧court規則を上書きしたりしない。

## 4. 次の実装・検証

選んだ候補でpredicateと境界試験を作る。最短/最大u64、overflow、同一DAA別block、originalより前のnotice、重複notice、後続要求の起点差替え、reorgでのnotice消失/復元を対象にする。最遅notice後の全作業がhard内に収まることは別の全graph検査で示す。deadlineの等号から供給やdefault許可を作らない。

本書は性能測定、数値fee、turn duration、配送遅延、公開wireの選択を行わない。F02/F03、全original有料受付、T01–T15と実doorの完了前は武装を拒否する。
