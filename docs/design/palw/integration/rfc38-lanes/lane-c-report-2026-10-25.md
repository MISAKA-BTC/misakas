# Lane C 報告（C3、2026-10-25）

開始 SHA `fdd468a5b`（`claude/rfc38-integration`）、branch `claude/rfc38-lane-c3`。前任の対応表と決定票は `lane-c-handoff-2026-10-25.md` にある。統括の指示で、RFC16 の容量 report（前任 handoff の候補 1）は Lane A に移したので、この差分では扱っていない。

## 1. RFC35 A：escrow の実台帳試験（試験専用 schedule）

- **RFC 要件**：RFC-0035 §7.1/§7.4/§7.5、ADR-0202 で承認された範囲は A の隔離実装だけ。対象は徴収→実行→決済/返還の保存則、exactly-once、資金不足のとき無変更になること、restart/reorg。
- **変更前の実 caller**：`utxo_validation.rs` の `PalwEscrowSpendLocksV1::refusal_with_bond_burn` と block filter が `palw_escrow_qualified_spend_v1(.., palw_escrow_fee_schedule_v1(row))` を呼ぶ。production の schedule は `None`。fold 側は `palw_state_v2/funded_escrow.rs` の `settle_funded_escrow_v1` / `sweep_funded_escrows_v1`。既存の試験は state delta の revert までしか見ておらず、UTXO 台帳との突き合わせはしていなかった。
- **実装**：試験を 1 件追加した（leaf ファイル `consensus/core/src/palw_state_v2/funded_escrow.rs` の既存 tests mod 内）。中央ファイルの変更はない。
  - `rfc35a_test_only_schedule_escrow_conserves_on_native_ledger_exactly_once_restart_reorg`
  - schedule は `PalwEscrowFeeScheduleV1::test_only_v1`（`#[cfg(test)]`）で、**試験専用**。値（work 300 / burn 100）は保存則を見るための任意の数で、提案値ではない。`palw_escrow_fee_schedule_v1` が `None` を返すことを試験内で assert している。本番経路は `Err(Schedule)` / `SettlementScheduleMissing` のまま。
  - 台帳は native の `UtxoCollection` で、`UtxoDiff::add_transaction` と `PopulatedTransaction` で動かす。processor と同じ代数を使う。
  - 確認した項目：
    - 各段で `amount = paid + burned + returned + unreleased` が成り立つ。
    - 未精算の row の unreleased は、その時点の outpoint にある UTXO 額と一致する。
    - 台帳は `before = after + fee`、かつ `fee ≥ burn` を満たす。
    - 最終状態は paid 300 / burned 100 / returned 600 で、台帳から消えたのは burn と relay 自身の fee だけ。
    - 同じ tx を再実行すると、台帳側は入力欠落で拒否し、fold 側も fact を拒否する（root は不変）。
    - 継続後の row に旧 tx を当てると `NotEscrowInput` になる。
    - relay の資金不足（`EscrowFundsOtherOutputs`）、escrow を超える dues、paid を水増しした偽造 fact は、どれも拒否され、builder の entries は空のまま、root と台帳は不変。
    - carriage の round trip（`into_state` で root を照合）から再開しても、同じように進む。
    - state delta の revert と UTXO diff の逆適用を一緒に行うと、row・lock・台帳が段ごとに元に戻る。
  - **限界**：virtual processor の spend filter・coinbase・実ブロックは通していない。production の schedule が存在せず、`test_only_v1` は consensus-core の cfg(test) なので、consensus crate からは注入できない。実 coinbase には到達しない。
- **保存則の検査で見つかった実装欠陥**：なし（修正なし）。

## 2. B の `AllBranchTermsSourceV1` への接続案（文書のみ）

- 現状：`adopted_terms_v1(original_claim) -> Result<Infallible, UnresolvedSupplyContractV1>`。唯一の実装は `NoAdoptedAllBranchTermsV1` で、`NativeAllBranchPacketAndFundingTerms` を返す。
- **Ok 側になり得る採択済みの型**：方式として採択済みなのは RFC35 A の escrow row（`PalwFundedEscrowRowV1`、実 outpoint・額・payer・hard deadline）と、`PalwFundedOriginalScheduleV1::admission_v1()` が返す `FundedAdmissionScheduleV1`（service profile、packet cost、branch あたりの capital/heap/retention）。案としては、Ok 型を「その claim の rooted escrow row」と「admission schedule による全 branch 予約の導出結果（`FundedAdmissionDerivationV1`）」の組にし、実装 `FundedEscrowAllBranchTermsV1<'a> { state: &'a PalwChainStateV2, schedule: &'a FundedAdmissionScheduleV1 }` が claim から row を引く形にする。
- **未採択のため塞がれている項目**：
  - `FundedAdmissionScheduleV1` と `PalwFundedOriginalScheduleV1` に production の値がない（D-C1）。このため実装は作れても、production の caller は `Err(Schedule)` で止まる。
  - 精算 dues（`PalwEscrowFeeScheduleV1`）に production の導出がない（D-C1/D-C2）。
  - source custody / material provider の契約は F の担当。
  - Ok 型の差し替えは中央（統括）が行う。
- **差し替えの条件**：D-C1 の値が採択され、production の constructor が入ったあと、統括が Ok 型を中央で差し替える。それまで `Infallible` を維持する。

## 3. 試験証拠

- command：`CARGO_INCREMENTAL=0 ~/Downloads/MISAKA-wt-b/buildslot.sh cargo test --offline -p kaspa-consensus-core --lib -- palw_state_v2::funded_escrow`
- 結果：exit 0、7 passed / 0 failed（新規 1 件と既存 6 件）、5210 filtered。

## 4. 残件

| 区分 | 項目 | 解除条件・依存先 |
|---|---|---|
| POLICY | D-C1〜D-C6（下記） | ユーザーの採択 |
| CODE | production の fee schedule の導出、`AllBranchTermsSourceV1` の Ok 型 | D-C1・D-C2 の採択後。中央の差し替えは統括 |
| TEST | processor 経由の escrow 精算（spend filter・burn withholding・coinbase） | production の schedule、または consensus crate から使える試験注入口（中央の変更で、統括の判断） |
| EXTERNAL | なし | — |

**この差分で閉じた要件**：RFC35 A の保存則・exactly-once・資金不足のとき無変更・restart/reorg を、試験専用 schedule と native UTXO 代数の水準で確認した。**まだ閉じないもの**：RFC35 全体、RFC08 の一回決済、RFC26/32/33/34/27。

## 5. 決定票（創作しない。採択待ち）

- **D-C1 RFC35 schedule の値**：fee/capital の額、retention 期間、settlement policy ID、requirement budgets、role の契約 4 bit。
- **D-C2 payee settlement**：payee の決め方、void/refund のときの返還先、reward の一回精算（RFC08 `palw_settlement_once_v1` と統合するか）。補足：現行の規則では、hard deadline 後に liability が閉じると payer の返還経路が未払いの dues より優先される。これが意図どおりかも、この票で決めてほしい。
- **D-C3 RFC33**：1.3M fee の徴収 movement と返還条件、300M slash の適用条件と責任者。
- **D-C4 RFC34 S1**：~~詳細仕様化と隔離実装の承認。~~ 訂正（2026-10-26 レビュー `3589fab5f`）：詳細仕様化・隔離実装は ADR-0201 と RFC34 末尾で 2026-10-21 にユーザー選択済み。再承認待ちではない。本番採択・料金・有効化は別。
- **D-C5 RFC27**：AccountID と lot の採択。
- **D-C6 native39→RFC12 写像**：W/PWU 変換、operator、anchor、retention、成熟、void。

private economics の値は公開 preset に流用していない。activation は変更していない（None のまま、DAA 16000 で武装しない）。計算証明は使っていない。モデル二階層は変えていない。
