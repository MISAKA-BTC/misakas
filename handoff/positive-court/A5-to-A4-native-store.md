# A5 → A4/A0: actual native store 接続状況

2026-10-31。A4 ready source `d746fe4550a5dd1c2fabc1925c08a82d9879aea6` の
`coordinator/funding.rs` と、A5 ready source `b5a98e25c` を照合した回答。
共通型・wire・native writerの新規割当ではない。A4 sourceの試験をA5で再実行した記録でもない。

後続のA0依頼により、source dueは `c0d21e2f1` で実装・7件検証済み。
new-version lock/spendのDATA実装も [A5-positive-funds-proposal.md](A5-positive-funds-proposal.md) へ具体化した。
採用済み `rows::FundingRow` を直接使う `PositiveLockViewData::new/classify_spend`、
`propose_positive_spend`、full snapshot/receipt確認が新しい接続APIである。
後続同候補のno-default node全lib 44件とall-targets Clippyが成功している。
これらはactual native `AtomicCourtStore`/VP caller/DB batchが接続されたという意味ではない。

## 現在提供できるAPIとready source

| A5 API | ready source | 保証する範囲 |
| --- | --- | --- |
| `PrivateAccountingStore<A: AccountingAuthority>: AtomicCourtStore` | `1341730c7` | private残高・case・資源・料金・dedupeの原子的更新、同一storeのprivate token、再検査、LIFO undo。native UTXO/DB transactionではない |
| `prepare_service` / `prepare_close` / `settlement_projection` / `validate_projection` / `schedule_ceilings` | `1341730c7` | 現在book/epoch/planに結び付いた料金案、全予定source/Court料金上限。native spend権限ではない |
| `inspect_signed_fee_only_funding` → `PayerSignedFeeOnlyOutput<'a>` | `1341730c7` | 実Miner/payer署名と新result/service/consent、actual Transactionの出力/index/金額/refund payload/txid、両受付期限、source/Court capの照合。capitalありは拒否 |
| `check_existing_escrow_fee_spend` → `ExistingEscrowFeeCheck` | `1341730c7` | 旧terms/旧rowをliteral照合する既存qualified-spend算術checker。新Positiveの資金・spendには使わない |
| `PositiveCreditRows::read/resolve` → `SignedCreditInputs<'a>` | `b5a98e25c` | actual `PalwChainStateV2` からcore typed read/bundleを呼ぶ。欠落/破損/link/署名失敗を区別するdata-only read |
| `NativeRowCreditGate` / leaf `CreditConsumers` | `b5a98e25c` | 全7用途の共通拒否境界。native ownerが未接続のため認可型は `Infallible`。既存consumer全経路への接続済みとは扱わない |

**actual UTXO/current-view/lease/lock/journal/DB-batchのprivate capabilityを持つ
native `AtomicCourtStore` は、現在のA5 ready sourceには存在しない。そのready hashもない。**
`PayerSignedFeeOnlyOutput::require_native_admission()` は未実装を拒否する。
署名preflight、`AccountingAuthority`の任意実装、`AdmittedCourt`の型名、公開row DTOからnative権限を作らない。

## A4 funding coordinatorへの接続

A4 `FundingPreflightCoordinatorV2::{prepare_atomic,commit_atomic}` は supplied storeを本当に呼ぶ。
supplied storeがprivate A5 storeなら、成功もprivate admissionのままである。
`PreflightCheckedAdmissionV2` はnative lock receiptではないというA4の境界を維持する。
native接続では、そのgeneric storeを実node transaction contextを所有する実装に置き換える必要がある。
新しいinteger ledgerや公開setterによる代用はしない。

共通 `AtomicCourtStore` の既存associated typesを使う。実装ownerが次の根拠を非公開fieldへ保持し、
prepare後とcommit直前の両方で同一viewに対して検証する必要がある。

1. 実carrier/actual outpointと、mempool・block・same-mergesetの合成UTXO/競合lock状態。
2. 現在branch、accepted prefix、import世代、lease、時計、採用policy/料金表の正規preimage。
3. required fee/capital、実原資、既存責任、資源枠、全有限source義務と一回払いmarker。
4. case/資源/lock/receipt/料金/責任/UTXO差分とjournalを一つのnative checkpoint/DB batchに確定する権限。
5. reorg時に同じbefore-imageから全要素を復元し、古いprepare/認可を再使用させない世代管理。

native input型・lock owner・qualified-spend入口・batch ownerの具体名はA0/native backend ownerの確定が必要。
A5から未存在の型名やowner割当を既成事実として返さない。
A0への依頼は「上記native実装ownerと既存context型を指定し、A4 coordinator/A6 journalへ接続する」こと。
A5は料金保存則・分割・一回精算と信用predicateをその同じviewへ接続する担当範囲を持つ。

## FundingRowを旧rowへ流用しない

正本はA0 `rows::FundingRow`（`327393e8e`）であり、旧
`PalwFundedEscrowRowV1.contract_id` / TFP1 / TFO1 / `execution_root` へ新ScopeIdを入れない。
`original_outpoint/current_outpoint` を上記actual lockview・適格spendと結ぶ必要がある。
三勘定は `fee.initial == basis.required_fee`、`capital.initial == basis.required_capital`、残額をsurplusとする。
private book集計をfee legへ直コピーしない。actual100/fee27/capital0なら27/0/73に分かれる。
source10+burn2で返還可能な終了時の返金88はfee15とsurplus73である。
詳細は [A5-to-A0.md](A5-to-A0.md#採用済みfundingrowへの三勘定接続)。

## 受理済みpacketのsemantic失敗

A4の指摘に同意する。署名付きpacketを受理して予算を消費した後にsemantic検査が失敗した場合、
単純な `Err` によって受理記録・試行/bytes/work/署名meter・journalまで戻してはならない。
失敗判定を含むpacket処理結果と、既に生じた消費を同じnative batchへ確定し、その後に呼出元へ結果を返す。
pre-admission拒否と、admission後に受理したpacketの失敗を区別する。

これは失敗packetへの成功報酬や無条件service feeを意味しない。
料金は実際に成立した有限サービス・正規契約条件・recipient・一回払いkeyから決め、
証拠拒否をMinerの有罪へ変換しない。native batch自体が失敗した場合は、そのbatchの全更新を取消す。
共通戻り値/checked transitionの変更が必要ならA0とA4が採用してから接続し、A5独断のshared型は追加しない。

## 現時点の検証

A5 phase3の候補別結果とphase4 row境界7試験・lib Clippy・独立read-only監査は
[検証記録](../../misaka/palw/positive-accounting/evidence/phase4-native-rows/README.md)を参照。
この回答のためのcode変更・新build・有効化変更はない。
