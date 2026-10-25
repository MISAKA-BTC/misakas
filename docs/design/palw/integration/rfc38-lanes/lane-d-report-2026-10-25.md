# Lane D report — 全 stage 検査・4 adapter・外部検証者(2026-10-25)

開始 SHA `5487efe91`(統合 branch `claude/rfc38-integration`)、branch `claude/rfc38-lane-d`。
計算の暗号学的証明は使わない(PRINCIPLES §8)。モデル二階層・claim 単位認証は変更しない。activation は None のまま。

## 1. 4 adapter と stage の対応表

4 adapter = paid claim target の Program / Pipeline / TypedSpec / Segmented(RFC07 §V.7、remaining-work 2026-10-15 の「全 4 種類の対象の実 adapter」)。
stage = RFC14 §6.1 の Discover → ResolvePlan → Check → Mismatch → ReservePursuit → Localize → RequestCommittedWitness → JudgeExact → Submit/Court → Convicted/Dismissed/DA default/Expired。

| adapter / stage | 現実装 | 実 caller | typed 検査結果 | 費用報告(変更前 → 変更後) |
|---|---|---|---|---|
| Program・Pipeline の Check | `provider_reader_v4.rs` `check.verify_program_or_pipeline` | `kernel_paid_checker_v4.rs` の production loop | `ProviderCheckedResultV4` / `ProviderReaderFailureV4`(Unavailable / InvalidMaterial / LocalCapacity / Unknown) | なし → `ProviderAcquisitionReportV4`(request 数・取得 byte・workspace byte) |
| TypedSpec の Check | `verify_typed`(memory snapshot reader) | 同上 | 同上 | 同上 |
| Segmented の Check | `verify_segmented`(SegPart stage) | 同上 | 同上 | 同上 |
| Mismatch → 単位の局所化 | `provider_reader_v4_failure.rs`(exact invalid unit)、`failed_query.rs` | checker | 不正 material なら invalid_unit。availability なら transport query だけ | — |
| Submit(material) | `monetary_material_challenge_object_v4` → ChallengeCurrentV4 | checker WAL → mempool → native fold | 回答義務。conviction ではない | 手数料は実 UTXO の差額 |
| Submit(計算) | `objective_action_from_provider_check_v4` → FileProof / FileDemand | checker | OutsiderFindingV1 | — |
| JudgeExact / court | native court、monetary material fold | consensus | Convicted / Dismissed / default | court 費用の報告なし(残件) |
| CPU work・RSS | — | — | — | 未測定(残件) |

## 2. この差分で閉じた要件

**(a) RFC07/RFC14 §2–5:availability と計算誤りを分ける陰性対照。** Codex の実経路(`a7c499dd7`)をそのまま使い、擬似 checker は作っていない。
- provider は別 process。ML-DSA 署名付きの `Refused` を返す。
- 実 SDK はこれを `Unavailable` に分類する。invalid-unit 座標は出さない。
- production checker は funded の ChallengeCurrentV4(回答義務)だけを提出する。FileProof や conviction は出さない。
- native rehearsal → mempool → block 適用 → cold reopen で checkpoint が残る。approved Final は作られない。
- 陽性対照(malformed bytes → InvalidMaterial、exact unit)は同じ経路のまま残し、env で 2 つの mode を分けた。
- 新しい test: `rfc07_signed_socket_refusal_is_availability_obligation_not_conviction`(ignored。pin した kaspad test binary が必要)。

**(b) RFC07「実 adapter ごとに同じ typed 費用報告を実 caller へ返す」(byte/IO 部分)。**
- `ProviderAcquisitionReportV4 { target, unit_requests, fetched_bytes, workspace_bytes_charged }` を追加した。
- 成功時と reader 失敗時の両方で、4 target に同じ型で返す。
- checker は失敗時にこの報告を log に出す。
- SDK 試験: 報告値が観測した IO と一致すること、Unavailable では 1 request・0 byte、malformed では 1 request・1 byte を確認する。

## 3. 変更ファイル

- `misaka/palw/sdk/src/claim_paid_v2/provider_reader_v4.rs`、`provider_reader_v4_failure.rs`、`provider_reader_v4_located_tests.rs`
- `kaspad/src/private_integration/kernel_paid_checker_v4.rs`(debug log 1 か所)、`kernel_paid_checker_v4/signed_socket_native.rs`
- `consensus/src/pipeline/virtual_processor/tests/g14_kernel_route_e2e/signed_provider_challenge_v4.rs`

中央ファイル・wire・DB は変更していない。Codex の編集対象 file には触れていない。

## 4. 試験

結果は §7 に記す。陽性・陰性の native parent は、ビルドした kaspad lib test binary を `MISAKA_SIGNED_PROVIDER_CHECKER_BIN_V4` に指定して実行する。

## 5. 残件

- CODE: CPU work・RSS・court 費用の報告。Program / Pipeline / Segmented の実 socket 陰性対照(今回の native 対照は Program だけ)。ChallengeCurrentV4 が無回答のまま期限切れになった後の DA default 帰結(slash の種類が計算不正と別になること)の native 試験。
- TEST: 実 Llama、ctx2M、全形状、全 stage の中断・資源枯渇、reorg / prune / IBD。
- EXTERNAL: 承認済みの独立 source。fixture の別 process や別鍵は、独立性や抽選承認の実証にならない。
- POLICY: 回答期限・手数料の値(Lane C の D-C1..3)。

## 6. 他 lane・統括への依頼

- F: 4 target 共通の material identity と retention 中の provider 責任を確定してほしい。DA default 試験の前提になる。
- A: checker の CPU / RSS 予約 vector を、報告型へ追加する欄として出してほしい。
- C: ChallengeCurrentV4 の fee と default slash の出典を示してほしい。
- 統括: 本 branch の統合と、pin した binary での ignored parent 2 本の再実行を頼みたい。

## 7. 実行結果

Build: `buildslot.sh cargo test --offline --no-run --lib -p kaspad -p kaspa-consensus -p misaka-palw-sdk --features kaspad/private-integration-config,kaspa-consensus/private-integration-config`(CARGO_INCREMENTAL=0)exit 0。最初の起動は SDK 変更を含めるため途中停止(exit 143、受入に数えない)。

- SDK `provider_reader_v4`: exit 0、15 + 1 PASS(新 report 試験を含む)。
- 陰性 parent `rfc07_signed_socket_refusal_is_availability_obligation_not_conviction`(`--ignored --exact`、TMPDIR=/private/tmp、RUST_MIN_STACK=67108864): **1 PASS**、15.4 s。SDK report `{Program, 1 request, 0 byte, 0 workspace}`、Unavailable、unit=(0,2) は checker 自身の失敗 query、funded ChallengeCurrentV4 accepted、fee 289,712 sompi、cold checkpoint sequence 1、Final なし。
- 陽性 parent `rfc07_signed_socket_unit_failure_paid_checker_native_funded_challenge`: **1 PASS**、5.6 s(InvalidMaterial、exact unit (0,2)、同 fee/sequence)。
- 既存 checker 群 `private_integration::kernel_paid_checker_v4:: --skip signed_socket_native`: **19 PASS**。
- 子 process の結果は親の内訳であり、追加の受入件数に数えない。full suite・clippy は実行していない。
