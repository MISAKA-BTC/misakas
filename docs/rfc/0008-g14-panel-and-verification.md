# RFC-0008 — G14 Panel・完全再実行・肯定承認

- 文書日付：2026-11-26（Asia/Tokyo）
- 文書区分：**DESIGN_SPECIFICATION**。採択済みの規則を統合する。提案に限る部分は該当節で明示する。
- 判断理由：[ADR-0008](../adr/0008-one-panel-independent-reexecution.md)
- 規範入口：[設計原則](../PRINCIPLES.md)／[RFC一覧](README.md)
- 本文は実装の設計を定める。実装・実測・公開有効化の完了を示すものではない。

## 1. G14の役割

標準のG14はproducerを除外した正規Panel 1席の承認をclaimの成功条件に含める。一席方式は他の公開検証者を排除せず、有効な反証は承認に優先する。

G14は1席とする。適格poolと除外条件は固定DrawPolicyで定める。選出外の監視者は登録や当番割当を要せず公開反証に参加できる。

## 2. 抽選と担当

元claim・対象集合・bond pool・重み・actor除外・原期限・担当義務をPANEL乱数公開前に固定する。抽選後の候補変更や有利なPanelへの引き直しを認めない。

抽選されたbondの役務、対応model、必要資料、検査範囲、報酬、責任を同じassignmentへ束縛する。別operator除外が必要なprofileで、別addressだけを独立主体の証拠にしない。

## 3. 完全再実行

Panelは不変artifact、tokenizer、入力、数値profile、challenge、prefix origin、Stopを認証し、元claimと同じ全実行を再現する。回答y、採掘値z、四Root、実処理範囲、Ticket入力を照合する。

正常時はMinerとの追加対話を挟まず承認できる。任意の対話による局所化は実装してよいが、直接反証の前提にしない。任意質問への無回答だけで義務違反を作らない。

## 4. 応答の区分

| 検査結果 | 提出と効果 |
| --- | --- |
| 全必須関係が一致 | 元assignmentに署名した承認。なおFinal・公開Court条件・当選は別条件 |
| 計算不一致 | 適格noticeと認証された局所反証。元期限・元予約でCourtへ |
| 資料不足 | 資料case。未検証を承認に変換しない |
| local資源故障 | 原因を保持して復旧又は担当規則による失敗処理。Miner有罪と混同しない |

検証者が反証を発見したことと、その検証者の全量再実行費用をNodeが観測できることは別である。署名だけで全量再実行をした事実やコストを暗号学的に証明したと表現しない。

## 5. 肯定証拠とコピー対策

肯定資料は元claimの実行commitment、位置、model root、assignmentへ結合する。他claimの局所Courtだけを提出して元claimなしに採掘資格を作る経路を禁止する。

**追加のactor固有challenge（旧CPN1の提案部分）**を採用するprofileは元claim、actor、義務、PANEL値、generation、nonceを束縛し、同じ義務の再送は一回だけ受理する。再署名した同じ局所計算は新しい実仕事として計上しない。

局所証拠のコピーを防ぐだけでは全量再実行費用の保証にならない。反証の内容はコピー元にかかわらず客観的に裁定し、発見報酬の資格・先行性・一回性は別の正規planで決める。特定のnonce方式の新規有効化は採択されたcodec・policyを要する。

## 6. 部分検査・分割検証の扱い

認証境界からcell/shardを検証する部品、Freivalds等の直接スポットチェックは、当該profileの検出範囲と前提を明示して使う。FOW1の全量再実行義務を部分一致だけで置換しない。

新しいcell profileは、全依存境界、position範囲、weight参照、反証の構成・搬送資源を覆う。全体coverageが不足したまま正常承認を作らない。共有資料・並列検証を理由に単独の適格検証者の入口を閉じない。

## 7. 公開反証とFinal

Panel承認後も通知期間、未解決case、資料義務、依存取消、現在の停止を確認する。選出外の参加者は同じclaimの全資料を要求し、元期限内に単独で反証できる。Nodeは有効な反証を虚偽承認に優先させる。合意creditはRFC-0011に従う。

## 8. 検証項目

producer自身の承認、別claimの証拠、無資料承認、二重報酬、偽Readoutを拒否する。選出席がMinerと共謀した場合に、第三者の反証で両者の違反を個別に裁定できることを確認する。

## 9. G14 assignmentと返答責務

新規producer claimのG14一席経路では、元claimの受付profile、対象root、producer除外、必要capability、bond状態と原通知期限を同じassignmentへ固定する。受付後にPanel人数だけを切り替えず、旧claimの通知・保持を元規則で処理する。

Minerは結果commitと規定の有界応答を担う。Panelが元資料から正常・不正の局所証拠を構成するprofileでも、Minerに任意の全proof構築サービスを追加要求しない。先に反証を発見した席は、その同じ不一致と両立しない正常承認を出さない。

Panelのtrace保持・最小担保・役務期間を対象policyで固定する。shard/cell部品を利用する場合も全実行のcoverageと元境界認証を要求する。
