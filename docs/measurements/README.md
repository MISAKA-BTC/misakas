# 測定結果

実ノード・出荷ビルドで実際に測った結果だけを置く場所です(2026-10-19、ユーザー指示)。式から導いた値・設計・提案は置きません(それらは `docs/evidence/`・`docs/rfc/`・`docs/adr/`)。

## 書き方
- 1 回の測定につき 1 ファイル:`YYYY-MM-DD-<対象>.md`。生ログは同名の `.jsonl` / `.json` を横に置くか、置き場所を明記する。
- 冒頭に必ず:測定日時、source SHA とビルド条件、測った host(稼働中の t12 か隔離 devnet か)、負荷の条件、試行回数。
- 値は observed_max・中央値・p99 を分けて書く。**観測最大値を安全上限と呼ばない。**
- 測れなかった項目・中断した理由・測定が当てはまらない範囲も書く。

## 一覧
| ファイル | 内容 |
|---|---|
| [2026-10-19-t12-passive-daa-blocks.md](2026-10-19-t12-passive-daa-blocks.md) | 稼働中の t12 の DAA の進み(中央値 184 秒/DAA)とブロックの tx bytes(観測最大 111,949 B)、受動的観測 |
| [2026-10-19-devnet-payload-carrier.md](2026-10-19-devnet-payload-carrier.md) | 隔離 devnet で 17,408 B payload の ML-DSA-87 tx 7 本が 0〜1 DAA で取り込まれた。意図した負荷はかからず、混雑下は未測定 |
| [2026-10-19-court-verify-cpu.md](2026-10-19-court-verify-cpu.md) | court の証拠検証 CPU(fixture)、1,000 件で合計約 3 秒 |
| (測定後に追加) | |
