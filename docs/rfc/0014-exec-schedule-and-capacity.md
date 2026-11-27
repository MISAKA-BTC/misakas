# RFC-0014 — EXEC credit・schedule・共通容量・難易度

- 文書日付：2026-11-26（Asia/Tokyo）
- 文書区分：**DESIGN_SPECIFICATION**。EXECの採択済み責務を統合する。負荷連動受付の詳細とMWA1の仕事量配分・600秒目標は提案として扱う。
- 判断理由：[ADR-0014](../adr/0014-exec-capacity-and-consensus-clock.md)
- 規範入口：[設計原則](../PRINCIPLES.md)／[RFC一覧](README.md)
- 本文は実装の設計を定める。実装・実測・公開有効化の完了を示すものではない。

## 1. EXECの責務

認証され成熟した有限の仕事からExecCreditを一度だけ発行し、未来期間のExecScheduleへ割り当て、ExecBlockが正規permitを消費する。creditは譲渡可能通貨でも追加報酬原資でもない。

claim処理、モデル推論、資料取得、次scheduleの準備を通常の1slot producer処理へ押し込まない。認証・権利の準備と、許可された取引搬送を分ける。

## 2. 発行と用途

発行には成功Final、公開Court条件、成熟、依存、停止・取消なし、資本・bond予算、一回会計を必要とする。同じroot work、snapshot、sliceを別仕事として発行しない。EXEC_TXとEXEC_SLICEは用途・消費・予約を分ける。

MWA1の仕事量Wからcreditを導く部分は**提案**である。採用する場合は同じ認証済み当選仕事を使い、量子Q、端数処理、上限を事前固定する。端数を確率配分する場合は独立SCHEDULEの固定規則を使い、剰余偏り・有限拒否回数を定義する。permit数を増やして採掘Ticketを増やさない。

## 3. 事前schedule

未失効・未消費・未予約のcredit集合、予約、期間、policyを乱数公開前に固定する。SCHEDULEの正規certificateから一度だけ割り当て、結果を見て集合や期間を選び直さない。

ScheduleIdは対象集合、authority、seed、割当、未来slotを束縛する。署名されたpermit/slotと取引の接続先を別に結合する。通常tip伸長では割当を変更せず、個別取消は対象permitだけを無効にする。

予定表の公開先行期間、生成期間、包含猶予、closeを別に定める。validatorは到着した壁時計秒だけで有効性を決めず、署名slot・canonical authority・包含境界を検査する。

## 4. 共通carrierと1 BPS

共通carrierの設計目標はslot幅1000ms、global slotあたり最大1本とする。EXEC、BASE/heartbeat、Court/controlを同じ搬送容量へ含め、model数やproducer数を掛けたBPSにしない。

重複候補、再送、不正複数署名は正規BPSへ数えない。空slot、credit不足、producer停止を、無認証permitや過去slotへの新規生成で埋めない。1 BPSは一秒決済の保証ではない。

共通carrierへの実際の切替は採択済みの対象profileとactivationに従う。旧ネットワークのDAA換算やCourtの3120/10000境界を1000msで一括換算しない。

## 5. Court・controlの保護

既受理反証、資料応答、case終端、beacon進行、回復の容量を先に確保する。これらを運ぶために、まだ発行されていないcreditや同じ処理から生成するseedを要求しない。

CPU、RAM、IO、帯域、包含bytes、保持stock、検査役務を合算する。共有資料があっても、認証・参照・再開費用を二重予約又は省略しない。有限global calendarとcursorで複数claimの最悪分岐を収める。

## 6. 受付難易度と資源制御

FOW1のtargetは試行前に固定する。正規Ticketで並べるprofileでは`(ticket,slot_id)`等の固定順位を用い、受付のためだけの追加Ticket抽選を重ねない。既受理救済へ新しいDやnonce探索を要求しない。

global容量・保護calendarの採択済み制約は維持する。負荷連動の全claim受付難易度・指数更新等の未採択詳細は提案として扱う。対象profileの更新式、上下限、空epoch、初期値、観測対象、閉鎖タイミングを未定義のまま有効化しない。

MWA1の初期案は共通固定targetと共通仕事単価を使う。winnerだけの仕事量を全試行供給として測らず、target低下と報酬単価の無限増加を連動させない。資本/経済floorによる拒否と単なる落選を区別する。

## 7. 生成・消費・失効

同一permitの複数block、競合取引、遅延候補、closed slotは共通の順序関数で扱う。選択済みprefixへ後から挿入しない。署名journalは合意状態とは別に保持し、restart/reorgで同一permitへ別payloadを署名しない。

authorityのreorgはcanonical branchに従って戻す。曝露済み乱数、署名済み事実、残存責任をundoで消さない。失効creditを別scheduleへ無料移送して復活させない。

## 8. 600秒決済目標

600秒はMWA1が提案する台帳確定frontierの運用目標であり、claimを600秒で強制Finalにする期限ではない。120秒計画epoch、原claim責任期間、公開異議期間、slot、報酬成熟を別々に定義する。

必要仕事供給、認証速度、schedule先行、通信・検証容量が揃わなければ目標を達成したと表示しない。600slotのうちCourt/controlが占める分を引かずに毎分60取引blockを保証しない。

## 9. 受入条件

事前集合の差替え、seed再利用、二重permit、close後生成、未来authorityへの循環、公開Court条件未達の発行、複数modelでのglobal枠超過、Court飢餓、ゼロ供給epoch、restart/reorgを試験する。BPS、充足率、credit在庫、frontier遅延を別に計測する。

## 9. 時計・heartbeat・massの境界

blockの受付回数、選択DAA、slot cursor、経済的仕事量を別の値として扱う。carrier受理ごとにDAAが必ず増えるという前提を置かない。各遷移はparent状態の正規clock/slotと対象profileの更新則から次状態を導く。

heartbeatは復旧・時計進行用の条件付きcarrierであり、PALW実推論、認証済み仕事、EXEC消費又は受賞を表さない。非前進riderを繰り返しても必要な時計前進を永久に延期できないよう、対象slotと受付容量に帰属させる。通常attempt、heartbeat、EXECの難易度・有効時刻・成熟・配分枠を取り違えない。

block massはbyte、script、署名検査、native state read/write、Court資料の上限を対象profileで固定する。decode、巨大allocation、暗号検査の前に長さ・個数・境界を検査する。ML-DSAのsignature幅に対応するscript上限は当該native profileへ束縛し、旧署名方式の上限で新profileを切り捨てない。

明示的なBASE復旧やbootstrap条件は元profileの無活動判定・期間・上限に従う。モデル採用BFTやdevnet時計から、通常仕事なしの恒常的発行経路を作らない。devnet用の短期値はgenesis生成前に固定し、既存履歴の途中で適用しない。
