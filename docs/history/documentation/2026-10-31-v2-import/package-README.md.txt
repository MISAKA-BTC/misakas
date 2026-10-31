# MISAKA 文書整理 — RFC 15本／ADR 50本

**[ADR 50本の入口](docs/adr-v2/README.md)** · **[204→50の完全対応表](docs/adr-v2/MAPPING.md)** · **[凍結・延期・不採用の台帳](docs/adr-v2/0050-freeze-deferred-rejected-and-governance.md)**

**[既存RFC 15本](docs/rfc-v2/README.md)** · **[RFCとADRの対応](docs/adr-v2/RFC-CROSSWALK.md)** · **[導入手順](docs/adr-v2/MIGRATION.md)** · **[文書検査](docs/adr-v2/VALIDATION.md)**

`misakas-pre(5).zip`を基準とする文書overlayです。204の旧ADRを50の主題別文書にまとめました。旧番号を再利用せず`ADR-v2-0001`〜`ADR-v2-0050`を用います。旧204 ADR、旧41 RFCは原文のbytesを保持し、先に作成したRFC-v2の15本も変更していません。

各新ADRには統合した判断・境界・関連RFCと、担当原文の完全収録欄があります。採択／未採択／撤回／延期を分離し、実装・有効化・配備済みという新しい判断をしていません。runtimeや合意設定は変更していません。

全repositoryではないため、旧文書から参照される全ての実行ログ・runtimeコードは重複収録していません。同じsnapshotへ適用する際は導入手順に従って文書だけをmergeしてください。
