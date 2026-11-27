# MISAKA documentation

PALWの実行・検証・合意・経済・運用を、**RFC20本の設計**と**ADR20本の判断理由**に整理した文書群です。

## 読み始める場所

- [設計原則](PRINCIPLES.md)
- [RFC — 何をどう実装するか](rfc/README.md)
- [ADR — なぜその設計を選んだか](adr/README.md)
- [目的別入口](INDEX.md)
- [採択・提案・有効化の区別](STATUS.md)
- [文書検査・差替え手順](maintenance/README.md)

RFCには数式、データ形式、状態遷移、制約、拒否条件、移行、受入条件を記します。ADRには背景、簡潔な決定、判断理由、代替案、帰結を記します。同じ番号の文書が対応し、相互に参照できます。

モデル追加はBFT YES/NOだけです。kernelは実行・技術互換のために使い、単独でモデル採用の権限を持ちません。個別claimのG14 Panelと公開Courtは別の責務です。

旧71RFCと235ADRの本文は削除し、archiveやv1/v2別体系を作りません。[RFC参照対応表](rfc/MAPPING.md)と[ADR参照対応表](adr/MAPPING.md)だけで旧番号を解決します。

文書は設計と判断を定めます。実装・実測・公開有効化の完了は別の証拠を要します。wire識別子・台帳・公開設定・支払いを文書番号の整理で変更しません。
