# RFC-0034 — archived closeの実受付と金融責任の適格解消

Status: **Proposed / Review Required** (2026-10-21)。[ADR-0201](../adr/0201-archived-close-qualified-financial-discharge.md)のS1を推奨する。詳細仕様化・隔離実装の判断待ち。本番採択、旧claim移行、料金・保持期間、有効化、pre統合・武装・配備を含まない。

正本は[RFC-0026](0026-court-policy-master-contract.md)。[RFC-0030](0030-court-end-effects-and-pending-liability-archive.md)の裁定と未決責任の分離、および[ADR-0192](../adr/0192-court-service-opportunity-and-shared-pursuit.md)の実供給とno-slashを維持する。本RFCはRFC-0030 §12が保留した、終了後materialの実受付と義務履行の区別を具体化する。

## 1. 問題と現在の根拠

現private `append_archived_close_chunk_v1`は、宣言済みdigestに一致する不足chunkを保存する。引数はsession/side/index/bytesであり、accepting blockのpointを受け取らず、各chunkの受理時刻も保存しない。`last_point`はブロック全体commit前には親pointであるため、これを完成時刻と見なせない。後日完成したbitmapや現在tipから、元assembly期限内の完成を逆算しない。

元期限は既に確定できる。署名済み`CourtCloseReceiptV1`は宣言object、宣言point、元session/phase/native v5入力を持つ。対応groupの`declared_daa`と宣言pointを照合し、既存`palw_close_assembly_daa_v1(count)`による`assembly_deadline_daa`、元session deadline、original hardを検査する。新しい返還期限や延長値は作らない。既存active chunk受付と同じく、assembly期限の等号を含む。

`verify_archived_close_v1`は、元署名と原契約・既受理prefix・全chunk/whole digestを照合し、保存native v5入力によって裁定する。返す非serialized tokenはstate rootに束縛されるが、現在は金融状態を変えない。妥当な計算裁定資料であることだけでは、元の時限義務を履行した証拠にはならない。

## 2. S1の区別

| 根拠 | 効果 |
| --- | --- |
| 正しい全bytesが元assembly期限内の実native accepting pointで受理され、元署名/native v5に適合 | 当該sideの提出義務についてpositive fulfillmentを認証できる。未投稿txの配送・収録を仮定する必要はない |
| 元期限後に正しい全bytesを取得し、private v5で一致 | 裁定資料の妥当性だけを確認。自動履行・自動返還・absence違反にはしない |
| 期限までのmaterialが欠け、F02の実供給・元義務・配送/収録前提を全て確認 | 別の適格absence判定を要する。計算不正と混同しない |
| 供給不足、前提崩壊、受理時刻不明、署名/native入力欠落 | no-slash・資料/未決責任の保持。hard到達だけで返還・pruningしない |

positive fulfillmentは当該declarationに限る。claim credit、計算不正に基づく既確定徴収、別side、同bondの他claim、別のcourt stakeや提供/保持義務を消さない。別判決でclaimが既にvoidとなっていても正しい裁定を取り消さない。

## 3. 実受付と権限の構成

1. 新originalの全合法graphで、終了後の不足material受付、署名・digest・v5、金融writer、journal/root、reorg、retentionまで予約する。既存depositを前払escrowと数えない。
2. 公開入口はnative accepting contextからpointを得る。callerが渡した`paid`、`on_time`、`fulfilled`、現在tip、私有cacheは根拠にしない。公開受付のdispatchと実fee決済が未接続なら入口を開かない。
3. 元declarationに束縛した不足chunkだけを受理する。継続prefixを削除・変更せず、既受理のduplicateを新完成として再使用しない。body完成を理由に新しいsession/clock/priorityを得ない。
4. 実native遷移内で、元assembly期限以下の完成point、original/allocation/source/profile/session/side、declaration digest、完成raw bytesのdigestを束縛する。rootedな受理記録、または同一原子遷移だけで消費できるprivate capabilityを用いる。保存済み旧appendから時刻記録を捏造しない。
5. 元署名、元native v5、完全group、同一state rootを再照合する。proof妥当性tokenとpositive completion tokenのどちらか一つでは金融writerを許可しない。途中でstateが変わったtokenは再検証する。
6. 当該sideのpending declarationだけを一度履行済みに移す。raw bytes、元契約、実受理point、根拠を残す。元court終了causeの証拠とは別記録とし、異なる妥当な裁定資料を一つのcauseへ上書きしない。
7. 金融marker/名目hold解消/資料保存を同一checkpointで適用する。失敗時はbytes追加も含めrollback。undo/replay・snapshot読込時にも同じ根拠を検査する。

新しいrooted記録を導入する場合、既存archive/receiptのBorsh意味を変更せず、別version/namespaceと末尾ordinalを使用する。具体ordinalは実装時に競合を確認して記録する。本RFCはordinalを予約しない。

## 4. 金銭と保持

既存assembly depositは将来の名目徴収額である。positive fulfillmentによる当該名目責任の解消は、預かり金の送金や新規Panel収入ではない。実徴収済み額を戻さず、徴収・fee・払戻を仮残高で作らない。

公開material受付・native検証・署名・保存・伝送・金融/root更新には実費と実在原資が必要である。新originalの署名済み責任範囲と資本拘束、実native transactionの通常fee、profile内の作業料/retention原資を区別する。未採択の料金・期間・全分岐費用を0で埋めず、資本不足は受付時に拒否する。既受理の正しいfraudを、後から資本不足で妨げる実装にはしない。

一つのdeclaration履行後もarchive自体を即削除しない。全side、元stake、F02/提供義務、他claim責任、保持契約、金融once-only markerを確認するまではbond exit/pruningを許可しない。全解消後の公開pruning方式は別途完全な契約と検証を必要とする。有限の保持完了を元hardから仮定しない。

## 5. 隔離実装範囲と受入条件

最初の実装は、上記positive fulfillmentを実際の私有遷移で一度適用し、名目pending/exit判定へ接続する。公開有料coordinatorが未完なら、その私有遷移の検証結果を公開受付完了と呼ばない。absence徴収、期限後completionの返還、旧資料の時刻補完は実装しない。

- 元assembly期限の直前・等号でのみpositive completionを認証し、直後を拒否する。元session/original hardの外へ延長しない。
- 元署名/native v5、scope/root/side/contract、prefix、全digest、実受理pointの改竄・欠落を拒否する。
- 期限不明の旧append、late valid body、未完成bitmap、cache一致だけではholdを解消しない。
- positive completionは追加された当該sideの名目pendingだけを解消する。既確定徴収・反対side・他claim・元stake・claim creditはそのまま。
- 二重適用、state変更後token、途中失敗を拒否し、complete undo/replayとcarriage復元で同じroot/金銭/holdとなる。
- 実原資、全合法graph、署名/IO/retention費用、公開native受付・実node試験、E01–E10/T01–T15が未完なら、G14完了とはしない。

実装・試験結果は未記録。数値・費用・測定の成功は本RFCによって追加されない。

## 6. 採択非依存のprivate native event検査候補

`palw_court_archive_verify_v1.rs`の`inspect_archived_native_chunk_v1`は、既存native wrapperが捕捉するpoint/objectの組からだけ検査入力を得る。caller引数の時刻を許可せず、元group・署名receipt・不足chunk・元assembly/session/original hardの境界を検査し、state rootに束縛した非serialized/非Cloneのinspectionを返す。親tipを実受付pointに読み替えない。これは現在native eventの検査であり、block全体commit済みのreceiptや金融権限ではない。

公開dispatcherは引き続きactive groupを必要とし、終了後archiveへの公開受付は開かない。既存private appendも変更せず、資料の妥当性・fulfilled status・名目/実残高・exit・保持を変更しない。S1採択後の実原資・v5・once-only金融transitionへの接続は未実装である。既存の実署名8fixtureへ境界直前/等号/直後、native event欠落/別object、state-root変化、公開入口拒否、状態不変の検査を追加する候補であり、試験結果はまだ未記録。

## 2026-10-21 ユーザー選択 — S1の詳細仕様化・隔離実装

ユーザーは「S1：期限内の実履行だけ責任解消」を選択した。元の期限内に実際に受理・完成した正しい応答だけ当該sideの未決責任を一度解消し、期限後のproof-valid資料の妥当性と金融上の履行を分離する。上の未選択表記は選択前の履歴である。

この選択は詳細仕様化・隔離実装の承認であり、新料金・本番採択・pre統合・武装・配備を含まない。公開有料入口・実原資・同一rootのnative v5・全必要materialの時限資格・once-only処理・他責任保持の接続と試験は別途必要である。実装・試験の成功をこの選択だけから導かない。
