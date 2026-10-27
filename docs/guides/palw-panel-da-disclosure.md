# PanelDa: 期限までにプロンプトを公開する(ADR-0206)

PanelDa(privacy mode 2)のテキストプロンプトは**期限まで非公開**です。紛争がなくても、受理 DAA `a` から `a + 1,040` までにプロンプト ids を公開しなければなりません。これは confidentiality ではありません。

- **誰が公開するか**: 義務者は producer。ids を持つ投稿者(poster)も公開できます(claim の hash が中身を固定するので、誰が出しても同じ)。
- **公開しないと**: claim は void(`PanelDaDisclosureDefault`)、報酬なし、開示 deposit(目安 13,107,200 sompi)を burn。計算の有罪や計算 bond の slash にはなりません。
- **公開すると**: deposit は返却。プロンプトは公開 material となり、外部者がそれを使って告発できます。報酬は期限が過ぎてから出ます(Final は `deadline + 1` 以降)。

## 状態を見る

```
misaka palw panel-da status <CLAIM_ID>          # 期限、残り DAA、公開済みか、deposit(locked/returned/burned)、void 理由
misaka palw panel-da status <CLAIM_ID> --json   # RPC getPalwFenceSchedule の view `panelDaDisclosure/v1`(op 227)と同じ文書(version 1)
```

## 公開する

```
misaka palw panel-da disclose --claim <CLAIM_ID> --ids-file prompt-ids.txt --key-file <KEY>            # dry-run
misaka palw panel-da disclose --claim <CLAIM_ID> --ids-file prompt-ids.txt --key-file <KEY> --submit   # 送信
```

- `prompt-ids.txt` は 10 進の token id を空白・カンマ区切り(JSON 配列も可)で並べたもの。
- 送信の前に node の行と突き合わせます: 件数・hash が一致し、未公開で、期限内であること。合わなければ手数料を払う前に拒否します。
- carrier の手数料は `--key-file` の鍵の UTXO から払います。期限の数百 DAA 前までに送ってください(包含の遅れに余裕を持たせる)。
- node の自動公開(producer node が ids を持つ場合に自分で公開する)は未実装です。当面はこの CLI で公開してください。
