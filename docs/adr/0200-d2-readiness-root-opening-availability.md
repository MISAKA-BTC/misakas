# ADR-0200 — D2 readiness rootの認証path供給

Status: **Proposed / Review Required**。ADR-0188 Bの詳細codec選択は承認済み。本ADRはBの選択をやり直すものではなく、rootに隠れた偽digestを第三者が実際に開ける経路を決める。[RFC-0033](../rfc/0033-d2-readiness-root-opening-availability.md)に規約・検証対象を具体化した。

## 問題

root単独からseatがcommitしたdigest/pathは復元できず、検証者自身の正しいモデルcopyだけでもその欠落は埋まらない。root/pathのpure認証器をnativeへ配線しただけでは、seatがopeningを一切渡さない反証不能な経路が残る。

## 選択肢

| 候補 | 内容 | 必須費用・残件 |
| --- | --- | --- |
| C1（推奨候補） | 承認済みF02契約へ、モデルbyteを含まないdigest/path開示courtを接続 | 元draw全許容分岐の予約・原資・保持とliteral seat authority、実供給、no-slash経路、公開要求/応答とqualified効果 |
| C2 | 全drawの補助digest materialが完成公開され、root一致と完全性を確認してからrootを受理 | 全digestの有料転送・保持・再取得、有限・非独占なmaterial公開とnative完全性検査 |

C1はBのtop-level rootの小ささと必要座標だけのopeningを維持し、F02の未供給規則を再利用する。一方でF02の原子的受付・実供給を完成させるまでnativeのready効果を許可できない。C2は全体転送を要するが、受理前の欠落を明確に拒否できる。どちらも未決のdraw上限や料金を決めた扱いにしない。

## 採択境界

利用者へ求める判断は、C1/C2の詳細仕様化・隔離実装だけである。新しい本番義務/料金/保持期間の採択、pre統合、武装、配備、モデル単位の審査/係数変更は含まない。未回答ならcodecの純粋な検査と、方式に依存しないoriginal coordinatorの作業を続ける。rootだけのready/slash入口を暫定的に開かない。
