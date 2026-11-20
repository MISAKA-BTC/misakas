# ADR-0223 — 全モデル共通の受付難易度・資源予約・総搬送1 BPS

- 日付: 2026-11-17（Asia/Tokyo）
- 状態: ADOPTED_DESIGN_NOT_ACTIVATED
- RFC: [0055](../rfc/0055-global-admission-court-calendar-and-one-bps.md)
- 設計責任者の指示: 現会話での統合制御のRFC作成・設計閉鎖。後続指示により、一件・三件・五件等は説明例とし、件数を固定せず計算負荷からNodeを保護する。添付補足に従い、未知modelごとの測定ではなく共通bounded checkerを用いる。
- source基準: pre@c3da7c6eaf5bf87f8e032d5b122c603d720a8398
- 実装・実機性能・公開有効化: この文書では完了としない。

## 背景と採用理由

旧実装にはAttemptの共通W、Final後のFreePrompt receipt抽選、courtのblock予算、classの未完了上限がある。これらの存在を、全ての新規routeから生じる将来の検証責任を共通容量内に収める仕組みの完成と扱えない。Studio/TFO3を含む本番接続の完成状態は[実装台帳](../design/palw/integration/public-one-honest-implementation-ledger.md)と別途照合する。

EXEC専用1 BPSは、他のcarrierや一carrier内の証拠処理、decode、保存、復旧の総負荷を制限しない。court一件の費用はshape、bytes、認証、局所演算、保持、合法な継続によって異なるため、一律の件数では、重い一件による過負荷と軽い処理の過剰な制限の双方が残る。

ユーザーの最後の指示を採用し、120秒は計画・集計・difficulty更新のepochとして維持する。通常30秒、original一件、反証三件等は規範上の割当から撤去する。共通形式の費用上界と全体の残資源から受付量と実行予定を導く。本書はRFC-0055との対でこの限定変更を記録し、過去の採択・署名・実測記録を遡及変更しない。

## 決定

### 1. 共通checkerと未知model

CommonCheckerProfileV1は、整数意味、対応kernelと分解、WellFormed / Init / Step / End、codec、認証、費用上界、plannerの有限性を定める。Minerは初回にtrace、分解位置、前後値、source、状態接続、終端へcommitする。異議後に作った記録で元の計算を置き換えない。

共通profileの資格はchecker/schema/cost-meter/binary/hardware_class/capacity/measurement/policyへ束縛し、model IDを必須キーにしない。同じ規則で表せる未知modelは、認証されたprogram構成とjob shapeからClaimResourceDescriptorを導き、資金・資格・全工程予約が成立すれば自動受付する。個別benchmark、人手審査、model名allowlistを要求しない。未対応kernel・意味を保つ分解がない数値規則・上界不明shapeは受付不能とし、Nodeの全推論やGPU fallbackへ切り替えない。

### 2. 固定件数を使わない全体予約

受付可能量は、qualification済み共通供給からsystem/control/EXEC/復旧と既受理責任を差し引いて決める。deterministic work、署名、hash、parse/展開、wire、I/O、control、carrier spaceをflowとして、RAM、queue、parts、claim/case、保持、journal、WAL/IBD/compaction、GC backlogを占有期間付きstockとして計上する。

全資源・必要な全期限区間について、先行予約＋既受理claimの未消化最悪継続＋候補の全責任が供給以下であり、stockも全時点で上限内でなければ受理しない。120秒合計だけで最終時刻への需要集中を許さない。空packet、重複、wrapper、control、索引、候補走査にも非ゼロの費用を置く。

全選出主体と有限の適格第三者、正当な防御、最後のnotice、最遅応答、InspectionPending、終端・精算、保持・GC、復旧、carrier/controlを予約する。nativeに相互排他性を検査できる枝以外は共有して減額しない。有限の決定的plannerは保守的な拒否を許し、後発案件のために既存の保護予約を弱めない。

### 3. 全新規routeの共通Dと原子的受付

新profileのAttempt、FreePrompt、TFO3、atomic batch等はUnifiedAdmissionV1を通る。Dは新しい検証責任の候補資格であり、安全上限そのものでも経済権利でもない。旧WとFinal後receipt抽選の履歴・意味は維持する。

target epoch eのpresealはe−2中に受け付け、Start(e−1)で集合を固定し、固定後の既存beaconから未知seedを得る。Start(e)までにseedが成立しなければ新規受付を行わず、既受理courtを継続する。Dはpreseal開始前に固定する。epoch b開始時の更新はtarget b+2へ適用し、b/b+1を変更しない。整数更新、初期値、利用率・混雑・停止の判定はRFC-0055 §6.5に従う。十epochごとの固定更新や固定候補件数を目標にしない。

予算化した有限preseal集合について、共通Dを通った候補をseed由来の順位で一巡する。保存cursorから処理可能なprefixを進め、候補ごとの実vectorと全将来予約を同じcanonical prefixへ順に適用する。受理件数は残資源から決まり、one-winner制約を置かない。原claim・実資金・予約・actor義務・仕事一回性・cursor/counterを同じcheckpointで保存する。quoteは予約権ではない。

### 4. 受理済み救済とNode内部の過負荷防止

適格notice、MaterialRequest、Q holdは全時間帯に保護control経路で受け付ける。既受理の反証・応答・終端へ新しいDやExecCreditを要求しない。他人の無効packetや先着占有によって本人の保護予約を消費させない。

Nodeは固定headerのchecked検査、process全体のparse/認証/decode budgetの確保、有界な対応認証、費用導出、全階層予算の確認を経て、leaseとmeter内で実行する。巨大allocation、展開、全state cloneの後に上限を調べない。planner、失敗、ログ、undoも有界にする。leaseはbuffer/worker/I/O等の所有権とともに移し、cancel/reorg/timeout通知だけで返却しない。実際の終了後に一回だけ返却する。

上界とmeterの不整合は未commit処理の停止・rollbackと新規受付停止へ進め、Minerの算術有罪にしない。未認証流入、candidate、queue、同期、保持、GCをそれぞれ制限し、既受理serviceと復旧を先に保護する。任意の機械・OS・ディスク故障でも絶対に停止しないという保証ではない。

### 5. 全carrier合計1 BPSと経済資格の分離

UnifiedCarrierV1の外側permitは全kind共通で1,000msに一本releaseする。live期間L、fallback、候補保持、包含条件は共通capacity/clock profileの較正済み有限欄とし、Lを説明用件数から固定しない。同じpermitの二重消費、lane別の追加発行、失効枠の再発行、無制限catch-upは禁止する。

異なるlive PermitIdは同parentから独立生成でき、唯一のglobal proposerを必須にしない。実到着burst、fork候補、悪意packet、IBDには別のingress budgetを適用する。通常時の持続目標は全kind合計1 BPSであり、空slotや供給不足時の達成を保証しない。

外側の搬送資格・service予約は内側のModelBlockRight/ExecCreditと分離する。既受理court/controlは新しいclaim成功や将来EXEC feeに依存させない。Final後の既存P_m、CCU、モデル生成権、有限credit/receiptの配分・消費規則を維持し、一claim一blockや生成権一件/epochを追加しない。共通D、予約、搬送だけで未認証weightや報酬を作らない。

モデル別の実受理率、Final到達率、実ブロック数は候補供給・資源要求・既存予約にも依存する。入口を含む成功確率のP_m比例や均等配分は保証しない。

### 6. 時計・Final・有限終了・復元

120秒epoch、carrierの一秒、原claimのDAAは別の時計である。ClockBridgeで最遅releaseと最早deadlineを保守的に導き、a、notice a+3,120、hard a+10,000と保持期限を元の意味で維持する。橋渡し不能なら新規受付しない。

局所positiveはNormalCheckedであり、独立全体承認、必要機会、Q解消、依存・資金・原期限を満たしてFinalとする。同batchのnotice/Q/反証・認証取消をFinal判定と権利発行・消費より先に適用する。Final待ちや再予約で元expiryを延長しない。

全工程を実資金で支え、service・金融責任・保持・journal・GCの有限release条件と一回性を別に管理する。原期限到達、Final、Refuted、AbortedUnverifiedだけで全資源を無条件に解放しない。予約bookと仕事一回性をsnapshot/journalへ認証し、復元不能なら新規受付しない。reorg/IBDの費用とworker世代も管理する。

## 規範の優先範囲

新版に限り、PRINCIPLES §14、RFC-v2-0010、RFC-0048/ADR-0216のEXEC専用外側1 BPSを、全kind共通carrierの1 BPSへ改定する。creditの認証先行・有限性、既存経済配分、court独立搬送は維持する。

共通形式はRFC02/16/17/19、全体資源とqualificationはRFC05/19、新規D・preseal・受付はRFC07/09/16、救済・実行・精算はRFC14/18/20、carrier/clock/DAGはRFC10、既存P_mと有限権利はRFC07/11、状態/移行はRFC12/13が所有する。主題別RFC20冊・ADR統合版50冊の数を維持し、本書を統合後の限定変更記録とする。

計算の暗号学的証明禁止、model tier、モデル審査不採用、追加model burn停止、一つの合意authority、延期機能、旧wire・署名・Final・既受理責任は変更しない。

## 棄却した構成

- court費用にかかわらず一律の件数で安全性を判断する構成、固定時間帯に通常処理と反証を閉じ込める構成。
- 各modelへ独立予算や1 BPSを与え、登録数に比例してglobal供給を増やす構成。
- 全claimを先に検証し、Final後の抽選だけでcourt流入を制御する構成。
- 未知modelごとのbenchmark・allowlist・人手承認を共通形式への適合条件に追加する構成。
- 算術workだけを課金し、parse、空packet、索引、RAM、保持、失敗・復旧の費用を無視する構成。
- 120秒の合計だけを見て、短いdeadline区間・因果順序・carrier包含を検査しない構成。
- 受理済み救済への再難易度、共通先着枠、無効packetによる他人の保護容量消費。
- 局所positiveだけでFinalを作る構成、搬送permitを未認証の経済権利へ変える構成。
- cancel通知だけのlease解放、空snapshotからの予約再開、GC未完了diskの再利用。
- 未測定容量を旧推定・fixtureから補い、設定や文書検査だけで有効化する構成。

## トレードオフと完成条件

多資源の保守的な予約は、実際には同時発生しない負荷まで保護し、実受理率を下げ得る。決定的なgreedy plannerは最適配置を保証しない。この利用率の取りこぼしを許容して、既受理の一人の適格反証、期限、資金、保持を守る。Dの追従遅れや上限到達時も、hard admissionとNode内部予算が超過を拒否する。

共通profileはDISABLED / QUALIFIED / RETIREDを区別する。最大合法入力の構造上界、最低機器での同時混合負荷、一万/五万modelと最大未完了状態、復旧、未知modelの自動受付、実LLMの正常Final・不正反証・精算・既存生成権消費を同じ版で検証する。共通checker等の変更時は再qualificationするが、同じ規格内のmodel追加だけでは個別測定を要求しない。全拒否/Abortだけで実用完成にしない。

レビュー・文書検査・抽象モデルは[変更記録](../maintenance/revisions/2026-11-17-global-admission-one-bps/README.md)、接続と受入証拠は[実装計画](../design/palw/integration/unified-admission-implementation-plan-2026-11-17.md)へ分ける。旧責任を原期限内に保つ移行bookと別のactivation recordが必要である。本決定は設計方向の採択であり、Rust実装済み、最低Node性能達成、公開稼働・有効化を意味しない。
