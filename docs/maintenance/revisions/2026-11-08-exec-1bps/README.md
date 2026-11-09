# 2026-11-08追加改訂 — EXECの1 BPSとcourt容量

入力は直前のRFC20本更新版と、今回のユーザー指示・添付court予算分析である。
状態は **ADOPTED_DESIGN_NOT_ACTIVATED**。
[現行RFC20本](../../../rfc-v2/README.md) /
[追加採択RFC-0048](../../../rfc/0048-exec-credit-schedule-and-one-bps.md) /
[ADR-0216](../../../adr/0216-exec-credit-schedule-one-bps-and-court-capacity.md) /
[検査結果](VALIDATION.md) / [変更ファイル台帳](changes.json)。

## 1. 完了した文書改訂

EXECを常設の高速取引経路として維持し、
**認められたPALW仕事 → 期限付きExecCredit → 未来期間ExecSchedule → 現在slotのExecBlock**
という責務分担に整理した。生成資格はScheduleIdで直接参照する。
通常の本体tip伸長で期間内の割当を引き直す構造と、過去round回収を通常producer経路から外す。

| 主題 | 現行の担当 |
| --- | --- |
| credit発行gate、数量保存、予約、期限、carry、消費、失効 | [RFC07](../../../rfc-v2/0007-claims-exec-and-answer-accounting.md) |
| ScheduleId、slot、authorityとattachment、順序、暫定実行、1 BPS | [RFC10](../../../rfc-v2/0010-dag-consensus-and-native-settlement.md) |
| courtの依存DAG、全合法分岐・複数claimの容量予約、測定 | [RFC05](../../../rfc-v2/0005-bounded-resources-and-long-context.md) |
| 公開取得、negative、期限、提供機会と失敗終了 | [RFC18](../../../rfc-v2/0018-interactive-negative-court-and-defaults.md) |
| Node非保有、有界証拠、EXEC・court・BASEの共用資源 | [RFC19](../../../rfc-v2/0019-model-free-node-and-bounded-evidence.md) |
| 成功Final、認証、credit発行の境界 | [RFC20](../../../rfc-v2/0020-verification-gated-final-and-adversarial-models.md) |

API、共通会計、保存・IBD、bootstrap・release、共通court、変更管理の冊子にも接続条件を追加した。
PRINCIPLES §14と設計パラメーター台帳へ明示採択を記録した。
現行RFC20冊と統合ADR50冊を維持し、0048と0216は後続採択記録として登録する。

## 2. 1 BPSの意味

| 項目 | 採択した条件 |
| --- | --- |
| 対象 | EXEC_TX全体の通常時生成目標 |
| 目標 | 1 BPS |
| slot | 1,000 ms、global slot |
| 正規枠 | 全モデル・producer・scheduleを通じ、1slot最大1ブロック |
| 前提 | 有効credit、適格producer、事前取得可能なschedule、指定負荷での検証・伝播・本体受理能力 |
| 分離する指標 | generated / valid / BASE accepted / settled BPS、TPS、gas、遅延、未完了件数 |

1秒slotは本体の新しいblockやDAA更新を毎秒待たない。
合意で定義されたepoch／clock profileが絶対slot区間を固定し、local timerはproducerの起床に使う。
恒久的な受理判定はlocal受信秒から独立させ、署名だけで実生成時刻を証明できるとは扱わない。

EXECへの収録、暫定実行、本体受理、決済条件成立を別状態にする。
1 BPSを1秒の不可逆決済、全クラス合算BPS、1 DAA/秒、courtの実供給率とは扱わない。
旧preset、blockrate、DAA換算、報酬周期は変更していない。

## 3. 生成権の正本と一回性

成功Finalだけでcreditを発行しない。
有効な認証、成熟、canonical仕事量、依存、bond・資本上限、失効、一回性を同じ発行gateで確認する。
同じrootについてNを一度だけ固定し、N=0や端数抽選の結果も保存する。
EXEC_SLICE、再送、別Final、後のschedule seedから同じ仕事を追いmintしない。

数量台帳は、available、reserved、consumed、expired、revokedをunitごとに相互排他として保持する。
reservationとconsumeは合意stateへ原子的に反映し、生成や送信成功だけで消費しない。
選択済み未使用枠は包含・閉鎖を終えるまで返さず、返却後も元のunit ID・expiry・試行上限を保つ。

1permit/1blockの場合、持続1 BPSには長期の有効credit純供給、有限在庫、
成熟遅延・失効損失、本体受理容量を同時に確認する。
在庫を短時間消費しただけの1 BPSを、継続能力の実証にしない。

## 4. 固定した予定表と動く取引履歴

候補集合とScheduleProposalIdをseed前に固定・予約し、
後続seedから一回割り当ててScheduleIdをcommitし、十分前に公開して未来slotから開始する。
生成権のauthorityと、EXECが接続する取引履歴のattachmentを別々に署名へ束縛する。

識別子は次の順に導出する。

~~~text
先行canonical cut
  → ScheduleProposalId
  → 後続seedと、ScheduleId非依存のallocation root
  → ScheduleId
  → PermitId
~~~

allocation leafはglobal slot・permit位置・producer・credit unitから作る。
ScheduleIdやPermitIdをallocation rootへ含めない。
予約／schedule commit自身のblock hashやpost-stateを、そのID材料へ戻さない。
実際の収録根拠は別のnative受理record・包含証拠で認証する。

通常tipの伸長では割当を維持する。取消は該当permitへ反映し、他のslotを再抽選しない。
authorityのreorgはcanonical stateのundo／replayで扱う。
durableな二重署名防止journalはundoで消さず、同じPermitIdで異なるpayloadを署名しない。

## 5. 遅延、欠落、競合

producerは現在slotを処理し、欠けたslotを待ち続けない。
validatorは遅延候補を、合意上の包含期限とslot閉鎖条件で判定する。
暫定実行と本体受理は同じ順序関数を使い、UTXO／nonce競合、同じpermitの候補競合、
closed slotへの後着を一貫して処理する。

通常枝の受理済みprefixへ古いEXECを後挿入しない。
canonical reorgは別の履歴変更として、台帳と取引stateを一緒にundoする。
元workの取消だけで、すでに受理した全取引を自動rollbackする新しい規則は作らない。

## 6. 添付のcourt問題に対する改訂

[提供された分析](sources/README.md)が指摘するowner排他待ち、逐次取得、
重複搬送、分割包含、保持／収集の混同を解消する規範を追加した。
添付の過去数値を今回の再測定値や達成保証へ転用していない。

- 公開取得・訴追をowner枠の解放待ちへ依存させない。
- 認証済みの共通資料、先読み、一括取得、独立workerの並列化を使い、真の逐次依存を残す。
- 最長経路に加え、資源別の合算需要・保証供給、releaseと締切を満たす因果的な予約scheduleを要求する。
- 全合法分岐、複数claim、異なる合法要求、cold outsider、reorg／再起動まで含める。
- EXEC・court・BASE／Heartbeatが共用するCPU、メモリ、IO、帯域、包含bytesを二重予約しない。
- claim・証拠・認証・work anchor・schedule commit・回復を、既にあるEXEC creditへ循環依存させない。
- a+3,120／a+10,000と資料保持を分離し、receipt・資料受領・再起動で時計をresetしない。

供給前提が成立する場合の公開裁定完了と、
資料が永久に隠される場合の成功停止・未検証終了・適格なdefault・残責任を別に試験する。
資料不足のfallbackとしてNodeにモデル本体や全推論を要求しない。

## 7. 解消完了の確認条件

超過計画を拒否することは受付の安全性である。
目的の実モデル・文脈長・jobを受理して処理できることは別の実用上のgateである。

同一のbinary、設定、最低要件機材で、EXEC 1 BPS負荷とcourt負荷、
owner共謀・混雑・資料不供給、cold取得、restart／reorg、pruned IBDを確認する。
未完了・未確認件を残してBPS・slot充足率・先行時間・在庫・遅延分布を報告する。
局所debug PASSや成功例だけの中央値を全経路の供給保証にしない。

この改訂には受入ケースを記述したが、runtimeや性能の実証を今回実施したものではない。
合意clock、lead／horizon／grace、credit寿命・carry上限、具体的な順序fold・移行、
資源profileを確定し、実装・測定・公開有効化を別に判断する。

## 8. 保全と反映

入力版の全ファイル、旧RFC48原文、旧ADR211原文、統合ADR50冊、89のRFC原付録、
前回改訂フォルダを保持する。今回追加後の旧番号系列はRFC49・ADR212となる。
旧schema・wire・署名・fence・既発行permitの意味を遡及変更しない。

ZIP内のdocsは更新済み一式である。別作業があるrepositoryへ反映する場合は、
変更台帳の新旧digestを照合してmergeする。
[新しい保全検査](check_revision.py)は今回入力との差分を評価し、
以前の検査レポートはその時点の履歴として保持する。

