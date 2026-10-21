# ADR-0202 — shared court originalの実原資と役割別同意

Status: **Proposed / Review Required** (2026-10-21)。[RFC-0035](../rfc/0035-court-original-funded-contract-and-role-consent.md)に具体化。選択待ち。

## 問題

既存rentはburnされ、既存depositは担保への名目exposureである。いずれも将来のPanel/relay作業料の前払原資へ流用できない。旧TFP1/custody署名も、新しい全分岐graphと新役割の金融/保持義務に同意していない。原子的な全分岐受付を成功させるには、実原資と役割別同意を新契約へ接続する必要がある。

## 選択肢

| 案 | 実資金の確保 | 影響 |
| --- | --- | --- |
| A（推奨） | originalの実入力で全額裏付けた独立運用escrow。実outpoint/script/spendとrooted once-only accountingを原子的に照合 | 元bond/予約資本・旧rent/feeの意味を維持。新しい専用資金/支出規則・wallet・UTXO検査と会計の実装が必要 |
| B | 新original carrier feeの実徴収額を運用口へ分配し、実資金と将来支払を追跡 | 新fee/collectible/coinbase/burnの会計経路が必要。旧rentの遡及再配分は不可 |

Aで詳細仕様化・隔離実装を進めることを推奨する。どちらも元claimに束縛された新署名と役割別同意、canonical相対graph、実accepting DAAでの期限導出が必要。producerが未同意の将来actorへ義務を課す経路を作らない。

## 承認の範囲

求めるのは資金方式の詳細仕様化・隔離実装の選択である。金額・draw上限・新料金・保持期間・配送前提・本番採択・旧claim移行・pre統合・武装・配備を含まない。数値や実profileが未決なら公開入口を拒否し、仮資金で合格にしない。RFC-0027のAccount Bondを本書だけで採択/実装済みにしない。

既存のF02/F03・RFC-0032 A・archive S1の承認は、ここで新しく必要になった資金保持/支払方式の採択を兼ねない。未回答の間も、方式に依存しないcanonical graph/依存閉包/署名/原子性と試験を続ける。実装・検証結果は未記録。

## 2026-10-21 ユーザー選択 — Aの詳細仕様化・隔離実装

ユーザーは「A：実UTXOで裏付けた独立escrow」を選択した。上の選択待ち表記は選択前の履歴である。実outpoint・支出・保存則と役割別同意を束縛する詳細仕様化・隔離実装に進む。金額・新料金・draw上限・保持期間・本番採択・pre統合・武装・配備を含まない。実UTXO/決済・全費用/資本・原子的original受付の接続と検証は未完であり、この選択だけから資金確保済み・G14完了を導かない。
