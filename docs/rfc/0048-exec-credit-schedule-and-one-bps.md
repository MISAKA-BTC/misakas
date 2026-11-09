# RFC-0048 — EXEC credit・事前公開schedule・1 BPSとcourt容量の統合契約

Status: **Accepted design / ADOPTED_DESIGN_NOT_ACTIVATED**
Date: 2026-11-08
Decision: [ADR-0216](../adr/0216-exec-credit-schedule-one-bps-and-court-capacity.md)
Current owners: [RFC-v2-0007](../rfc-v2/0007-claims-exec-and-answer-accounting.md) / [RFC-v2-0010](../rfc-v2/0010-dag-consensus-and-native-settlement.md)

本書は、ユーザーが提示したEXEC二層構成をRFCへ反映し、ブロック生成の1 BPS目標と、
添付分析が指摘したcourtの待機・搬送・予約問題の解消条件を追加する採択記録である。
現行の閲覧・通常編集先は引き続きRFC-v2の20冊とし、本書を21冊目の独立した通常編集先にしない。
番号0048は提供workspace内で未使用だった範囲の割当であり、未取得branchの予約を保証しない。

## 1. 解決する問題

旧方式は、親のDAA spanで生成資格を選ぶ処理と、1秒roundでEXECを生成する処理が結び付いていた。
遅れて見える予定表、過去roundの回収、span切替時の残権利処理を、通常のproducer経路から取り除く。
現行実装の完成を宣言する文書ではなく、参照元は提供workspaceにある旧実装snapshotである。

court側には、ownerごとの排他待ち、真の依存以上の直列取得、重複搬送、分割収録、
複数claimの合算予約、保持と収集の時計の混同を残さない。
共有・並列化の採用だけで全ケースの期限内完了を保証したことにはしない。

## 2. 採択した二層構成

認められたPALWの仕事から期限付きExecCreditを一度だけ発行し、
合意状態へ事前commitした未来期間のExecScheduleが、global slotごとの生成資格を割り当てる。
ExecBlockはScheduleIdとpermitを明示し、生成権のauthorityと取引履歴へのattachmentを別々に束縛する。

| 概念 | 主な責務 | 規範の担当 |
| --- | --- | --- |
| Claim | 仕事、元trace、肯定検証、裁定、依存、成功Final | RFC16〜20 |
| ExecCredit | 認証・成熟・資本上限を満たす有限生成権、予約・消費・失効 | RFC07 |
| ExecSchedule | 対象集合固定、post-commit seed、一度の割当、未来期間、公開・包含条件 | RFC10 |
| ExecBlock | permit、slot、接続先、取引内容を署名し、共通順序で実行・受理 | RFC10 |

ExecCreditは譲渡可能な通貨、別報酬、無担保の発行権ではない。
同じroot work、snapshot、Final、sliceを別々の仕事として二重発行しない。
producerは通常のslot処理でClaimの詳細、LLM、モデル資料、証拠取得、次期schedule作成を実行しない。

## 3. 1 BPSの採択範囲

高速取引経路EXEC_TXの通常時目標を **1 BPS** とし、設計上のslot幅を **1,000 ms**、
global slot当たりの正規EXEC_TXを **最大1ブロック** とする。
モデルごと・producerごとの1 BPSを合算した値ではない。重複候補・再送・不正な複数署名を正規BPSに数えない。

この目標は、有効なcredit、事前に取得できるschedule、適格producer、
対象負荷に対する検証・伝播・本体受理容量が揃う運用条件で評価する。
credit不足、生成者停止、受理不能を、無認証creditや過去slotへの新規生成で埋めない。
空ブロックを含む生成数と、実取引数・gas・有効な本体受理数を別々に報告する。

1 BPSは1秒で不可逆決済する保証ではない。EXEC収録、暫定実行、本体受理、決済条件成立を分ける。
BASE／Heartbeatを含む総block rate、既存DAA換算、報酬・裁定時計をこの数値へ一括置換しない。
公開preset、wire、署名domain、fence、高さは今回変更しない。

## 4. 新profileの主要な不変条件

1. creditの発行条件は一箇所に集約し、成功Finalだけでなく有効な認証、成熟、依存、bond・資本予算、取消状態を読む。
2. 対象credit集合をseedより前に固定し、future scheduleへ一度だけ割り当てる。scheduleを見て対象集合を引き直さない。
3. ScheduleIdは期間・規則・authority・対象集合・seed・割当を束縛する。生成資格を最新親のspanから逆引きしない。
4. 公開先行期間、生成期間、収録・閉鎖の猶予を別々に定義し、予定表は合意で検査できる未来slotから開始する。
5. 通常tip伸長では割当を変更しない。個別失効は該当permitを無効にし、他者のslotを再抽選しない。authorityのreorgはcanonical branchに従う。
6. 通常producerは現在slotだけを処理する。validatorは自分への到着秒ではなく、署名されたslot、資格、合意上の包含・閉鎖境界を検査する。
7. 同一permitの複数block、取引競合、遅延候補、closed slotを、暫定実行と本体受理で共通の順序関数に従って扱う。正常枝の受理済みprefixへ後から挿入しない。
8. ローカルの署名journalは合意上のcredit消費と別に永続化する。restart／reorg後に同じPermitIdで別payloadを署名しない。
9. EXEC_TXとEXEC_SLICEの資格・会計・キューを分離し、sliceの追加だけでTxPermitを増やさない。
10. claim、証拠、認証、work anchor、scheduleのcommit、回復は、creditを既に持つEXECだけへ依存させない。

## 5. court予算の膨張を解消する条件

[05](../rfc-v2/0005-bounded-resources-and-long-context.md)を資源計画、
[18](../rfc-v2/0018-interactive-negative-court-and-defaults.md)を公開対話・帰責、
[19](../rfc-v2/0019-model-free-node-and-bounded-evidence.md)をNode境界、
[20](../rfc-v2/0020-verification-gated-final-and-adversarial-models.md)を成功条件の正本とする。

依存DAGの最長経路が期限以内であることに加えて、各資源の全必要量が保証供給内であり、
応答の開始可能時刻・期限・既存予約を満たす具体的な予約scheduleが存在することを受理条件にする。
共有資料のbytesは一度の搬送を再利用できても、各claimへの認証・参照・責任と再開費用を省略しない。
全合法分岐、複数claim、全owner枠の共謀占有、cold outsiderを対象に含める。

原点a、noticeのa+3,120、係争hardのa+10,000、最遅notice後の6,880 DAAを維持する。
資料保持をclock resetや排他owner待ちに転用しない。
EXEC 1 BPSを、そのままcourtへの1回/秒の包含能力や1 DAA/秒に換算しない。
EXEC・court・BASEが共有するCPU、メモリ、IO、帯域、包含bytesは合算し、同じ容量を二重予約しない。

供給機会不足は、自動的なMiner有罪や成功Finalへ変換しない。
期限内未成立なら成功を止め、未検証終了と未解決liability・資料保持・適格な回復を分ける。
元時計の外で成功に変える隠れたfallbackを設けない。

## 6. 解消完了の判定

次の二つを別々に満たす。

- **受付の安全性:** 全必要経路の時間・資源を引き受けられないclaimを受理しない。超過を拒否できることを確認する。
- **対象profileの実用上の成立:** 明示した資料供給・配送・包含・復旧前提が成立する範囲で、対象実モデル・文脈長・jobを実際に受理し、資料を持たない適格な第三者が取得から裁定まで完遂できることを示す。

全保有者が永久に資料を隠して供給前提が成立しない場合は、算術反証の無条件な完成を保証しない。成功停止・未検証終了、客観的に成立する提供義務違反と残存責任を、別の失敗試験で確認する。

同一のbinary・設定・資源上限で、最低要件のNode、対象最大profile、混雑、
EXEC 1 BPS負荷、owner共謀、資料不供給、restart、reorg、pruned IBDを検証する。
BPS、slot充足率、credit在庫と補充率、schedule先行時間、暫定実行・本体受理・決済の遅延を分けて測る。
未完了・未確認のサンプルも分母に残し、成功例だけで供給上限や遅延保証を作らない。

添付の分析は[保存した提供資料](../maintenance/revisions/2026-11-08-exec-1bps/sources/README.md)として参照する。
過去の短いdebug経路、fixtureのCPU時間、条件付きDAA算術を、今回の実モデル・全合法経路のPASSへ転用しない。
本採択時点の実装・実測・公開有効化は未完了である。

## 7. 互換性と移行

責務分離、明示ScheduleIdへの新版移行、混合負荷の性能確認を順に行う。
旧schedule、既発行permit、過去の取引・会計・署名・fenceは旧規則で再検査可能にする。
新旧permitが同じ仕事量を重複して消費できる変換は認めない。
新version、署名domain、clock profile、lead/horizon/grace、credit寿命・carry上限は有効化前に確定する。
単なるwrapper追加で新しい生成権契約を実装済みとはしない。

一つの鎖、非BFT、Nodeモデル非保有、正常非対話positive、不一致時の対話negative、
global承認の誠実性前提、旧仕事の権利・責任、既存経済上限を維持する。

