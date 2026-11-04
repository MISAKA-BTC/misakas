# PALW は入力と出力をどこまで見るか

## 結論

chain に prompt 本文や生成本文を直接保存する設計ではありません。chain が扱うのは commitment、root、receipt、signature、claim/court state です。ただし、実際に inference を実行する provider と、再実行が必要な panel/verifier は、その仕事に必要な入力と出力を扱います。

| 利用形態 | provider に本文が見える | panel/verifier に本文が渡る可能性 |
|---|---|---|
| 自分の host だけで実行 | 自分のみ | claim の検証方式に依存 |
| remote provider | はい | はい |
| verified / PALW receipt | はい | 再実行・監査に必要な範囲で、はい |

「chain に本文がない」と「実行者が本文を見ない」は同じ意味ではありません。

## Public chain data

公開されるのは主に次の情報です。

- class / model identity
- artifact、trace、output、schedule 等の commitments
- executor Bond
- panel receipt と signature
- claim phase、court move、settlement
- metering / reward に必要な値

これらから本文をそのまま取得するフィールドはありませんが、長さ、class、時刻、実行関係などの metadata は観測できます。

## Local files

gateway / runtime が作る private bundle、state database、audit material には入力・出力または再検証素材が含まれる場合があります。権限を制限し、暗号化鍵と保存期間を運用ポリシーとして管理してください。

自動削除を前提にしません。不要になった private data の削除、backup、log rotation は運用者の責任です。

## Before sending sensitive data

- remote provider と panel を信頼できるか確認する。
- secret、seed phrase、API key、個人情報を prompt に入れない。
- local-only 実行で済む仕事は local に保つ。
- bundle、database、logs の保存先と権限を確認する。
- 「receipt がある」ことを TEE や zero-knowledge privacy と解釈しない。

## 正本

- [Private prompts design](https://github.com/MISAKA-BTC/misakas/blob/main/docs/palw-private-prompts-design-2026-09-05.md)
- [PALW extension envelope](https://github.com/MISAKA-BTC/misakas/blob/main/docs/palw-extension-envelope.md)
- [PALW registry map](https://github.com/MISAKA-BTC/misakas/blob/main/docs/palw-registry-map.md)
