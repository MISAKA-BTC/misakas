# PALW-AN1 脅威・反証・保証境界

状態: DRAFT_NOT_ACTIVATED。対策の記述と本番検証済み状態を区別する。

| ID | 経路 | 規定と残る条件 |
| --- | --- | --- |
| AN-T01 | owner/nonce変更後の旧claim転用 | C/s/正規input/元Root/署名を照合。新しい正しい再生成の費用は別問題 |
| AN-T02 | 同一Cを別funding/slotで再包装 | RegKey→slot一意。slot/s/実仕事の未消費検査と発行を原子化 |
| AN-T03 | Cに自己を含むclosed rootやslotを戻す | 非循環collection IDとnative登録envelopeを分離 |
| AN-T04 | nonce文字列が同じtokenへ正規化 | 実tokenの固定単射。UNK、切詰め、template削除を拒否 |
| AN-T05 | seedを見てから候補を選び直す | 全C候補を最初のbeaconアクセス前にfreeze。事前候補からのsubset選択は費用分析へ残す |
| AN-T06 | Panel再実行を見て元Rootを埋める | 元claim/対象集合/poolを未来Panelイベント前にfreeze |
| AN-T07 | 任意のinput rootを正規値として申告 | 参照profileはnative直接再計算可能な長さへ限定。未実装adapterでpayloadを信用しない |
| AN-T08 | modelのRoot属性と実weight不一致 | 実bytes再認証。Node Courtはnative model rootの期待indexへopeningを確認 |
| AN-T09 | 同model別index/別programの値を提示 | 命令・token・座標からindexをNodeが導出。任意indexのmembershipだけでは不十分 |
| AN-T10 | 正しい局所Courtのコピーで全体承認 | LocalHoldsと完全再実行承認を分離。本人労働を証明したとはしない |
| AN-T11 | 算術/型/range/padding/Endの不正 | 元Rootに認証された違反なら反証。全local関係の完全性が前提 |
| AN-T12 | 偽pathでMinerを誣告 | InvalidEvidenceとして拒否。認証不成立を元claim不正と混同しない |
| AN-T13 | 資料隠蔽・cold Panel排除 | 有限資料caseと保護容量、原期限、未充足の成功停止。実サービス・包含が必要 |
| AN-T14 | 同batchのnoticeより先にFinal | hold/反証/取消を先行適用し、現在stateを再検査 |
| AN-T15 | cache/import/repair/別APIの未検証発行 | 全consumerに用途別gateと同じ消費journalを接続。native実装は未完 |
| AN-T16 | reorgで消費だけ戻す/発行だけ残す | claim/消費/発行/予約/依存を整合undo。DAG/外部settlementは別証明 |
| AN-T17 | 安い偽Rootで大量の当選payload | Dだけでは防げない。資金付き登録・候補/byte上限・全工程予約を別に適用 |
| AN-T18 | 定数/零model・cache・外注による正当高速化 | 不正と扱わない。c_minの費用定理は未取得、通常LLM実費を強制しない |
| AN-T19 | 全Panel悪意・正直者検閲 | 一人正直の条件外。独立実装には条件を外すと偽Finalが通る境界例を含める |
| AN-T21 | nativeとCourtで件数型が不一致 | float/boolの暗黙変換を受付で拒否。認証できない受理済みclaimを作らない |
| AN-T22 | 過大leafがbounded Courtを無効化 | 期待indexの資料/構造caseでhold。InvalidEvidenceを捨てただけで成功にしない |
| AN-T20 | 昔のFULL1下界や元AN1テスト数を流用 | sigma依存入力へ固定入力定理を適用しない。元報告値と今回の独立結果を分離 |

この表は全攻撃の網羅性を証明しない。具体的なhash・署名・beacon・wire・整数型・資料case・全native経路の検証が有効化条件である。
