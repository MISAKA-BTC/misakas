# RFC08 provider replacement / material lifecycle — HANDOFF (2026-10-26)

Branch `claude/heavy-rfc08-lifecycle`, base `2de0a526b41559994e9894e6e13a268bb4dc2247`. No push/merge/deploy/activation change. Public activation None; no new price/payer/TTL; no wire ID / DB schema / consensus meaning change; no cryptographic computation proof.

## 1. 既存実装の確認（再作成していない）
- 実 provider 交換・受入・旧世代保持・cold reopen: `g14_kernel_route_e2e/material_typed_ingress_v4.rs`（replacement child）、証拠 `docs/evidence/rfc/0008-typed-replacement-request-binding-2026-10-25.md`。
- reorg 中の旧/新世代保持と Denied 分類: `material_typed_reorg_v4.rs`、証拠 `0008-typed-replacement-reorg-retention-2026-10-25.md`。
- Release→finality→GC（native/op242 janitor・WAL replay）: `material_typed_terminal_v4.rs`、`material_gc_v4.rs`。
- 実 pruning（Run6）と retained-source pruned import（`material_typed_pruned_import_v1.rs`, `--ignored`）: 別 run が所有、PENDING。
- native authority の分類（Resource / CapacityUnavailable / Denied / Unavailable、import 未完了は Unavailable）: `consensus/src/consensus/mod.rs::palw_material_generation_authority_checked_v4` と store gate 試験 `monetary_material_current_tests.rs` で既に閉じている。

## 2. 変更要件（今回閉じた範囲）
store gate の分類は正しいが、公開 answer API が `Result<_, String>` で、実 caller（court answer tick）は local capacity・native 不可用・全世代 Denied・local 組立拒否を同じ「material unavailable」warn として扱い、同じ pass で残りの demand に資源を使い続けていた。Resource を semantic 拒否へ潰さない要件に合わせて:
- `misaka/palw/remote/src/monetary_material.rs`: `MaterialAnswerErrorV4 { Authority(native class), NoCurrentGeneration, Local }` と `is_local_capacity()`。`answer_material_current_classified_v4` / `answer_material_exact_current_classified_v4` を追加。既存 String API は同じ値の display 形（文字列は不変、既存 caller・試験の挙動不変）。
- 実 caller `kaspad/src/private_integration/native_recovery_runtime_v4.rs::answer_and_release_tick_v4`: local capacity なら warn して当該 pass を終了し次 tick に再試行（deadline 不変、missing と記録しない）。その他は従来どおり continue。
- 所有権: 返す answer owner（NativeRetainedV1 と allocation ticket）は従来どおり postcheck まで保持し、失敗時は bytes を出さず drop。refusal はどの class でも provider store の保持世代を変更しない（試験で確認）。

## 3. 試験
- 新規 `monetary_material::current_generation_tests::classified_answer_keeps_local_capacity_unavailable_denied_and_local_apart`（実 admitted + cold reopen した provider store、native authority は注入）: 各 class の保持、非 Denied で他世代へ fallback しない、全 Denied は NoCurrentGeneration、local 組立拒否は Local、両保持世代が残る、fallback 世代は positive authority で answer 可能。
- 検証 command と結果は §5。

## 4. 未完（担当外・依頼）
- 実 pruning 後の provider read と pruned import 後の再取得: retained source（Run6/7）完成後に `material_typed_pruned_import_v1` の cold reopen 後へ provider store の current/withdrawn 読み出しを加える。PENDING（source 未完成）。
- kernel query 応答（`native_kernel_material_query_v4.rs`）は wire 上 `Refused{reason}` のみで class を運べない。class を運ぶには wire 採択が要る（POLICY）。今回は変更していない。
- typed material の P2P serve/request（lane F 報告 §1）は wire 未採択。node-local 直接 import を P2P 受入に数えない。
- RFC37 row stream 成果（統合 branch `85ba75733`）は不要のため依存なし。

## 5. 検証
Code commit `84776b83c`（source = その tree）。既定 feature、`CARGO_INCREMENTAL=0 TMPDIR=/private/tmp ~/Downloads/MISAKA-wt-b/buildslot.sh env CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo …`。
- `cargo test --offline -p misaka-palw-remote --lib -- monetary_material::current_generation_tests`: exit 0、親 **8 passed**、0 failed、169 filtered（新規 1 + 既存 7。子 process 行は親に含む）。binary `target/debug/deps/misaka_palw_remote-8b3db3fd64b53a1c` SHA256 `d06e9442d898615e9769a5da1a907c6032a5ea5190c7a96239d5d3855a2e1e20`（target は終了時に削除）。log `/private/tmp/claude-501/rfc08lc/remote.log`。
- `cargo check --offline -p kaspad --lib`: exit 0（4m22s、既存 warning のみ）。log `/private/tmp/claude-501/rfc08lc/kaspad.log`。court tick の新 branch を実行する試験は無い: **PENDING**（実 node で local capacity を起こす fixture が要る）。
- full suite・clippy・一括 fmt は未実行。fixture 上の PASS で campaign/15 gate は更新しない。
