# ADR-0216 — EXEC credit・事前公開schedule・1 BPSとcourt容量

Status: **Accepted design / ADOPTED_DESIGN_NOT_ACTIVATED**
Date: 2026-11-08
RFC: [RFC-0048](../rfc/0048-exec-credit-schedule-and-one-bps.md)
Amends: 新claim／新EXEC profileに限る。[ADR-0215](0215-model-free-node-noninteractive-positive-and-adversarial-availability.md)の検証先行FinalとNode非保有を維持。

## 背景と明示的な採択根拠

ユーザーは、PALWが生成権を供給しEXECが事前に決めたslotで取引を処理する二層構成を提示し、
その内容と追加の1 BPS目標に基づいて該当RFCを修正するよう指示した。
また、添付分析のcourt待機・搬送予算の膨張を解消する条件もRFCへ記述するよう指示した。

本決定は、その設計と必要な検証条件の採択である。
実装の完成、性能の達成、ネットワーク稼働状態、全モデルでの期限内完了を宣言しない。
本番号は提供workspace内で未使用を確認した範囲で割り当て、外部branchの未使用を保証しない。

## 決定

### 1. EXECを常設の取引処理経路として維持する

認証済みの仕事から期限付きExecCreditを発行し、
future ExecScheduleのslotをScheduleIdで明示的に参照する。
producerは現在slotのpermitを読んで取引を構築する。
本体親のDAA spanの変化で期間内の割当を引き直す正常経路を廃止する。

起動時はサービスを動かし、有効な予定表がなければ待機する。
最初のFinalだけを扱う特別なmodeや無償permitを追加しない。
claim・証拠・認証・work anchor・予定表commit・回復はEXEC creditの有無から独立させる。

### 2. 1 BPSを明示した目標にする

EXEC_TXの通常時目標を1 BPS、slot幅を1,000 ms、global slotあたり正規EXEC_TXを最大1ブロックとする。
対象条件は、有効なcreditとproducer、取得済みschedule、指定負荷と必要資源が成立する範囲である。
生成、validation、暫定実行、本体受理、決済を別に測定する。

この採択は1秒の不可逆決済、BASE／Heartbeatを含む全クラス合算の1 BPS、
1 DAA＝1秒、現行presetの即時変更を意味しない。
具体的なlead・horizon・grace・信用寿命と合意上の時計接続は、有効化前に定義・測定する。

### 3. 生成権と取引履歴を分ける

credit台帳が発行・予約・消費・失効の正本となる。
scheduleのauthorityと、各EXECが接続する許可済みattachmentを署名で別々に束縛する。
通常tipの伸長では割当を維持し、取り消されたpermitだけを失効させる。
authorityのreorgは明示的なundo／replayで処理する。

受理前の競合と遅延候補は、暫定実行と本体受理で共通の順序関数から導出する。
受理済みprefixへの通常枝での後挿入は認めない。
同じpermitの別payloadへの再署名を、restart／reorgで許可しない。

### 4. 途中計算の会計を独立させる

EXEC_SLICEはroot配下の計算記録とし、slice数の増加をTxPermit発行へ換算しない。
未検証claim、CertificationReadyだけのclaim、失効仕事はcredit発行の根拠にしない。
既存の認証・成熟・bond・資本上限と一回性を、単一のcredit発行gateで確認する。

### 5. courtと高速取引の容量を一体で検証する

owner固有の排他的待機を必要経路にせず、認証付き共通資料、先読み、
独立処理の並列化、全合法分岐・複数claimの予約を採用する。
真の逐次依存、cold取得、搬送・包含・reorg・再起動は予算から取り除かない。

最長経路、資源別の総需要と保証供給、具体的に実行可能な予約scheduleを同時に確認する。
EXECの目標rateを、実供給未確認のcourt予約容量として計上しない。
原時計a+3,120／a+10,000と資料保持の区別を維持し、供給停止を自動有罪・成功へ変換しない。

超過拒否の成功と、対象実モデルの受理・公開裁定完了の成功を別に証拠化する。
単一の短い経路やfixtureのPASSを、全ブロッカー解消の判断に使わない。

## 文書の担当と20冊の維持

| 規範 | 担当 |
| --- | --- |
| ExecCredit・root work・EXEC_TX／SLICE会計 | [RFC07](../rfc-v2/0007-claims-exec-and-answer-accounting.md) |
| ScheduleId・slot・attachment・順序・1 BPS | [RFC10](../rfc-v2/0010-dag-consensus-and-native-settlement.md) |
| 依存DAG・全分岐資源計画・測定 | [RFC05](../rfc-v2/0005-bounded-resources-and-long-context.md) |
| 公開negative・owner非依存・供給失敗 | [RFC18](../rfc-v2/0018-interactive-negative-court-and-defaults.md) |
| Node非保有・局所証拠・合算資源上限 | [RFC19](../rfc-v2/0019-model-free-node-and-bounded-evidence.md) |
| 成功Finalとcreditの境界 | [RFC20](../rfc-v2/0020-verification-gated-final-and-adversarial-models.md) |

RFC-v2の現行20冊とADR-v2の統合50冊を維持する。
RFC-0048と本ADRは後続採択記録として登録する。
以前の原文・historical source-map・付録・2026-11-08の先行改訂記録を保持する。

## 変更しない判断と有効化条件

Nodeのモデル非保有、秘匿モデルとモデル内全員結託を想定する脅威モデル、
global承認の条件付き保証、計算validity proofの禁止、一つの鎖、非BFT、モデル二階層、
media／モデル改善の延期、既存の経済・資金権利は維持する。

旧scheduleと既発行permitはその版の意味を保つ。
新profileのnative接続、anti-equivocation保存、重複予約・reorg・決済、
実モデルとEXEC混合負荷での供給、最大対象profile、cold Nodeを確認してからactivationを別に判断する。

