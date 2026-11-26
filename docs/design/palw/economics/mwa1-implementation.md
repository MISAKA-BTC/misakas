# MWA1 — 実装接続・API・検証計画（RFC0070 companion）

状態: **PROPOSED_DESIGN_NOT_ACTIVATED**。ここでのRust型名・RPC名は案であり、native API、wire number、fenceを割当済みではない。[RFC0070](../../../rfc/0013-model-work-accounting.md)／[ADR0238](../../../rfc/0013-model-work-accounting.md)が規範案。

## 1. 実装単位と依存関係

| ID | 主担当（既存パスまたは追加候補） | 具体的な作業／短時間受入 |
|---|---|---|
| M01 | `consensus/core/src/palw_canonical_work_v1.rs`、`palw_tir_work_v1.rs` | 既存shape分類、work vector、dtype表を使う純粋な新版meter。旧 `provisional_scalar_v1` の値を変更しない |
| M02 | FOW1/QF1のcanonical stage・trace codec | P/D/Stop、PrefillBody/DecodeDecision、LM head/Readout、active branchとoriginを束縛。P+D-1、EOS、context境界をfixtureで検査 |
| M03 | `palw_model_registry_v1.rs`、`palw_state_v2/objects.rs` | ModelWorkProfile、標準job、W_max、resource upper、profile hash、model generationを保存。旧fieldの読み取り互換 |
| M04 | MAD2採用record、BFT ballot検証 | WorkProfileIdを署名対象へ追加する新版。AUTOも同じderiver。genesis含む全モデル、MOE3管理・creatorなしを継承 |
| M05 | FOW1受付・TB1 freeze | 前確定target/price/profile、job bound、最大報酬・検証・credit予約を元試行へ結合。勝者を見てから再予約しない |
| M06 | Panel/Gate C、QF1/Court | 完全再実行でmeter factsを照合。有界な局所work count/origin反証。未知coverage拒否 |
| M07 | 共通発行budget・claim診断 | `W_est/W_max` とnative支払上限、model cap、bond liability、P/F role予約、deadline/retarget理由を検算 |
| M08 | FOW1成功/award consumer | 一回の認定work row。Final/Gate C/当選/適格性/成熟を同じreaderで検査。落選・未認証の発行拒否 |
| M09 | `palw_execution_quanta_v1.rs`、execution lane/schedule | CCU単位の新版有限credit、future seed freeze、purpose別ID、予約・取消・消費。旧100000 PWUをそのままCCUにしない |
| M10 | MOE3 native settlement | 共通Cの一回分割、結合按分、実予算比LP、POSITION atoms、P/F分離、quote/dust/期限、reward journal |
| M11 | BMR1 native、Court slash | 用途別bond、実回収49/51、恒久廃止の実reserve burn、依存credit取消、成立済検証役務保持 |
| M12 | journal・snapshot・pruning/IBD | 予約/credit/market/role/aliasのatomic undo、TB1曝露・署名journal、live責任pinと安全GC。pruned/archival一致 |
| M13 | Studio/Gateway/Miner/Panel、PGM1等 | 全経路で同一見積もり関数を呼ぶ。CPU/GPU/低RAM/streamedの成果一致。見積もりを合意値と混同しない |
| M14 | RPC/CLI、診断・経済評価 | model estimate、claim meter/quote/eligibility、epoch capacity、allocation、retarget、credit、settlementを選択親へ束縛 |
| M15 | isolated devnet | 短時間fixtureでM01–14を先に接続。次に実LLM3ピア、Pruned IBD、通常carrier、長時間1 BPS/600秒測定。公開activationとは別 |

旧型・旧経済を直接意味変更しない。新moduleを作る場合は既存のnative consumerへ接続し、テスト専用の成功pathを通常採掘の完成証拠にしない。

## 2. 型と純粋関数の境界（擬似Rust）

```rust
struct ModelWorkProfileV1 {
    descriptor: DescriptorId,
    meter: MeterId,
    shape: ShapeDigest,
    generation: u64,
    max_context: u32,
    max_output: u32,
    max_economic_work: u128,
    verification_upper: WorkVector,
    resources_upper: ResourceVector,
    reference_suite: SuiteDigest,
    calibration: Option<MeasurementBundleId>,
}
struct WorkTermsV1 {
    work_profile: WorkProfileId,
    difficulty_snapshot: DifficultySnapshotId,
    target: Uint257, // 0..=2^256, not u256 with wraparound
    price_num: u128,
    price_den: NonZeroU128,
    exec_quantum: NonZeroU128,
    source: IssuanceSourceId,
    allocation: ModelAllocationId,
    role_plan: RewardPlanId,
    job_bound: JobBound,
    max_work: u128,
    reward_reservation: ReservationId,
    credit_reservation: ReservationId,
    deadline: CanonicalDeadline,
}
struct CertifiedWorkV1 {
    work_key: ComputationalWorkKey,
    original: OriginalId,
    terms: WorkTermsId,
    facts_root: FactsDigest,
    vector: WorkVector,
    eligible_ccu: u128,
    final_id: FinalId,
    gate_c: CertificationId,
    ticket: Ticket256,
}
fn derive_profile(descriptor: &Descriptor, meter: &MeterPolicy) -> Result<Profile, MeterError>;
fn estimate(profile: &Profile, job_bound: &JobBound) -> Result<Estimate, MeterError>;
fn derive_work(profile: &Profile, facts: &CanonicalFacts, origins: &OriginView) -> Result<WorkVector, MeterError>;
fn reserve_terms(parent: &NativeView, trial: &Trial, bound: &JobBound) -> Result<Terms, Refusal>;
fn certify_work(parent: &NativeView, original: &Claim, evidence: &VerifiedFacts) -> Result<CertifiedWork, Refusal>;
fn reward_quote(terms: &Terms, ccu: u128) -> Result<u64, Refusal>;
fn reserve_exec(terms: &Terms, certified: &CertifiedWork) -> Result<PendingCredit, Refusal>;
fn commit_source_close(tx: &mut NativeJournal, source: &Source, plan: &AuthorizedPlan) -> Result<(), Refusal>;
```

型は既存hash/amount/codecに合わせる。`Uint257` は論理範囲。積はchecked u256/u512又は厳密有理数、格納幅への変換はchecked。固定長の整数表現・順序・domain・最大数は登録profileと同時に決める。未知版、分母0、超過、飽和・float・wraparoundを拒否する。

## 3. 状態遷移・native transactionの境界

| 状態 | 更新する項目 | 禁止する更新 |
|---|---|---|
| REGISTERED / REVIEW | descriptor、market、profile候補、資料 | 報酬・EXEC発行 |
| ADMITTED | admission generation、work profile | claimを正当と確定すること |
| RESERVED | 固定terms、W_max、budget/bond/role/credit予約、元期限 | 結果を見た後のtargetや価格変更 |
| SUBMITTED / CHECKING | 元claim、四Root、work facts、Panel/Court | coinbase市場・EXEC利用可能残高 |
| FINAL_UNCERTIFIED | Final記録 | 合意credit・受賞決済 |
| CERTIFIED_WIN | 同じwork key、Final/Gate C/Ticket照合、実CCU、予算差額、成熟待ち | 別Ticket、二重予約・二重credit |
| SOURCE_CLOSED | 元sourceの共通C、allocation/plan、MOE3予約消費、独立P/F権利 | sourceの二度目の分割、残高上書き |
| CREDIT_SCHEDULED | 固定集合、TB1 seed、ScheduleId、PermitId | 過去slot追加・再抽選 |
| SETTLED / CONSUMED | 一回消費、native journal、receipt、依存index、所定frontier | 同じsource/credit再利用 |
| REJECTED / EXPIRED / RETIRED | 元期限・責任保持、正規失効/解放/取消 | 別family・別model名で再受付して同一work消費を回避 |

FinalとGate Cの間は資料・反証責任を保持。正規noticeや負の裁定は同batchの成功・発行より先に処理する。source close、市場処理、P/F役務確定、credit発行、slot消費は別の正規遷移とpurposeを持つが、各遷移の該当budget・状態・journalは原子的。市場失敗で成立済みP/Fを失わせない。必要な後続attempt・frontierは既存規則を使用する。

## 4. 診断と経済preflight

Native readerを一箇所で共有する。RPC例（名称は案）:

- `getPalwModelWorkProfile`：model/root/profile/meter、標準jobのCCUと上界、測定UNKNOWN、admission/stop状態。
- `getPalwWorkQuote`：選択親、state root、snapshot、W_est/W_max、target、price、expected gross/CCU、R_max、実支払上限、role/credit予約、cap残、必要資源、期限。これは見積もりであって認証ではない。
- `getPalwClaimDiagnostic` への新版項目：実Wとorigin、Final、Gate C、当落、source、予約/消費、market quote、credit/slot、一回会計、決済、現在の再試行条件。
- epoch統計：共通target・価格・上限、認定仕事の供給/受理/拒否/落選/認証率、供給不足とcapacity拒否の区別、予定/受理/空/失効carrier、frontier lag、Pruned Nodeの物理容量。

型付き拒否: UnsupportedMeter, ProfileMismatch, InvalidWorkFacts, InvalidOrigin, DuplicateWork, WorkBoundExceeded, BudgetInsufficient, LiabilityInsufficient, VerificationCapacityInsufficient, PendingFinal, PendingGateC, TicketLost, Revoked, Expired, ScheduleCapacityExhausted, ViewChanged, LocalIo。ローカル故障は不正ではない。空結果を落選と決めつけない。読出しは同じ選択親/状態世代を固定し、変化したら新viewを取得する。秘密鍵やTB1秘密shareをログへ出さない。

## 5. 短時間fixture一覧

| ID | 入力・ケース | 期待結果 |
|---|---|---|
| T01 | 2×3×4 dense積、同一dtype | 手計算24 MAC相当（単価1の仮想表）。tileサイズに依存しない |
| T02 | MoE total 80、active 3、共有expert | 稼働分と共通経路だけ。inactive expertを追加しても増額しない |
| T03 | 同一P,Dの大小モデル | 共通targetの下でW比と期待原資比が一致（floor/cap条件内） |
| T04 | P=1,D=1、P=4096,D=1、EOS早期終了 | stage別position数、LM head位置、末尾token重複なし |
| T05 | KV prefix origin正当/偽造/重複 | 正規再利用分除外、偽origin・別root・別profile拒否 |
| T06 | Select branch、MoE tie、重複expert | 規範branch以外の水増し拒否、予約上界を維持 |
| T07 | weight width、CPU/GPU/backend/cache差 | 同一descriptorのCCU同一。異なる実行契約をaliasにしない |
| T08 | BFT票のWorkProfileId改変、UNKNOWN測定 | 改変拒否。UNKNOWN測定はBFT採否と分離 |
| T09 | theta 0、1、2^256、境界T==theta | strict less、overflowなし、事後変更拒否 |
| T10 | Final前/未認証/落選/正規当選 | 正規資格成立以外のreward/credit拒否 |
| T11 | R_max>budget、旧ADR-0176上限不足 | 受付前に拒否。winner時の追加資金・clipなし |
| T12 | 複数modelとC端数、model zero | MOE3結合按分、D残差、qJ/B、zero no-op、dust |
| T13 | 49/51の分割回収、自己受給、廃止burn | 累積差分、二重報酬拒否、検証bond保護 |
| T14 | EXEC W=0,Q-1,Q,Q+1、整数最大幅 | 有限予約、正規丸め、0除算なし、上限超過拒否 |
| T15 | TB1 seed再曝露、候補集合差替え | 再抽選拒否、署名journal保持 |
| T16 | 共通slot競合、Court保護枠 | 外側carrier 1/slot、保護枠侵害・失効permit拒否 |
| T17 | budget/market/credit途中の障害、reorg | 原子的巻き戻し、3ピアroot/UTXO/一回状態一致 |
| T18 | Pruned IBD・古いorigin/consumed tombstone | 履歴の安全な回収、未決責任・二重消費拒否維持 |
| T19 | 空epoch、認証停滞、経済上限 | 理由を保持、target/価格の無限変更なし |
| T20 | 同一実LLM試行のnative経路 | 同一claim、正規当選、Final/Gate C、reward/EXEC、決済を証跡化 |

T01–T19は短時間fixture優先。T20、GPU実機比較、1 BPS・600秒・長時間pruningを最後に行う。T01等の仮想数値は公開パラメーターではない。以下の参考スクリプトは算術例のみでnative検証の代替ではない。

## 6. 未設定パラメーターと評価

実装時に共通meter係数、W_max、price/target retargetの上下限と更新率、source closeの時計、Q_exec/credit horizon、予約上限、役務plan、Node capacity・Pruning budgetを版付きで決定する。具体値未設定のまま有効化しない。600秒は目標であり、現在の120秒計画epochを自動置換しない。比較にはモデル固有の正確なartifact/descriptorを必要とし、Qwen2BやFlash Nextの名称だけから数値を作らない。
