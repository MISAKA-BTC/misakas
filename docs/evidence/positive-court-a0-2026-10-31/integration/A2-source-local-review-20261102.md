# A2 independent Source local facade review — 2026-11-02

対象 `74bd239321771f47cd2a6eabdb338c63649292ba`、親 `89b1d63465e95a57cec990390cbc4d24d00abea0`。指定A3原本からすべて `git show` で取得した7 Rust filesとhandoff、および関連する旧reader/q・TIR decoder/validator・manifestを確認した。追加agent、Cargo、rustc、テスト、モデル実行、source変更、Root/target/cache変更は行っていない。

**結論: 記載されたaudited build / trusted installed source前提の範囲では、適用前に止める具体的blocker、public native escape、typed resourceの具体的過小見積り、明白なcfg/可視性/type障害は見つからなかった。** これはソースレビュー結果であり、compile成功、実allocation上限、全startupのRAM/CPU上限またはnative admission成功の承認ではない。追加8 active＋helper1・薄port2は全未実行のまま扱う。

**ownership_same_bytes — no concrete escape found**

参照: source_local.rs:208-249。Private borrowed slice and private quote; consuming decode calls existing canonical decoder once; no replacement bytes, callback, Clone or mutable program access; public borrowed program export is caller-owned and is not NativeRetained or a native lease.

**pre_reader_order — source checked**

参照: source_local.rs:269-297; source_local/bootstrap.rs:43-88。Positive limits, byte ceilings, scalar step upper, private build gate and typed bootstrap ceiling precede scanner entry; fixed enum refusals require no owned diagnostic. Cumulative work charge occurs atomically before the observed scanner.

**four_reader_accounting — source checked**

参照: source_local.rs:134-164; source_local.rs:252-259; ledger_expansion.rs:234-301; ledger_expansion.rs:646-741; wire.rs:48-105。L(n+4)+3L(n)+4 includes increment-before-limit refusal; reader byte envelope is 4n+4. Observers run before success/error propagation, count actual remaining-byte deltas, reject duplicate/overflowed coverage and do not refund prior charges.

**diagnostic_envelope — no concrete underquote identified within documented assumptions; not measured**

参照: source_local/bootstrap.rs:17-88; source_local/bootstrap.rs:96-149; source_local.rs:299-323; ledger_expansion.rs:737-789; misaka/palw/tir/core/src/validate.rs:36-43,140-159,374-411,519-566。Bootstrap enumerates fixed memory-reader IO/Resource error families and type-name backing. Decode extra adds typed headers/custom-error/UTF8/frame/owner differences while retaining the existing four diagnostic byte writers. NF text census uses structural items, bounded prevalidated names, fixed Dim/perm/scalar/UTF8 display branches. Admission takes MAX of sequential scan/bootstrap and decode phases; backend owned errors are dropped before public fixed enums return.

**legacy_compatibility — source checked, two byte comparisons recorded**

参照: ledger_expansion.rs; wire.rs。Existing wrappers pass None to private observation ports. Source and wider q arithmetic/error tails are byte-identical to parent. Grammar, old messages, checked ordering, wire IDs, flags and fees have no identified semantic change; no differential execution performed.

**build_and_cfg — no obvious source-level visibility/type/cfg blocker; uncompiled**

参照: build.rs:12-151; source_local.rs:80-89; source_local/tests.rs:22-137; consensus/core/Cargo.toml; Cargo.toml:251-260。Core auto-discovers build.rs, which is included by workspace packaging. Gate requires supported cfg AND build-generated profile env, exact release, target width/endian/triple and Borsh/derive registry lock versions/checksums; wrappers/bootstrap/custom or unknown flags refuse. Unsupported builds keep the API but refuse before backend. Cargo/rustc was not run.

**静的比較**

- `source_q_and_error_tail`: parentとbyte一致、SHA-256 `2f9a343f07b7e31de8d25b2bc624ed8f35bd8ad922ed016aad3e4b3d6516dd22`。
- `wider_program_q_tail`: parentとbyte一致、SHA-256 `668762c40427ea172c96d2e79a94accb82439dc22e50458d98227cfc686af466`。

既存None経路への観測portの薄い転送を確認した。first-error順序・qの動的実行同値を試験で確認したとは述べない。

**条件・未検証範囲**

- **trusted_build_premise (scope boundary, not a newly demonstrated defect)**: Gate does not hash all installed dependency .rs files, compiler executable or sysroot; same-version trusted installed sources/toolchain remain a prerequisite. No supply-chain attestation or complete build-integrity guarantee is established.
- **runtime_profile_counting (validation caveat)**: All added tests are unexecuted: supported native scope declares eight facade active tests plus one ignored child helper and two thin-port tests. Unsupported normal native scope has three facade active tests, wasm two. An externally injected cfg with unsupported build env can compile audited test bodies that return early; successful harness counts alone must not be labeled successful audited decode. Preserve audited/unsupported child markers and effective profile.
- **local_scope_only (scope boundary)**: Logical allocation requests, scalar scanner work and NF item ceilings do not establish total NF/compile CPU, stack/RSS, host leases, simultaneous-owner/input/export budgets, full artifact/model/process startup, native admission, model qualification or consensus credit.
- **pending_validation (unverified, not a demonstrated code blocker)**: Normal intended supported build, unsupported/refusal profile matrix, cfg-selected test compilation, old/new q/errors differential and diagnostic/allocation observations still require separately scheduled execution. No Cargo, rustc, test, model, target/cache mutation or source modification was performed by this review.

**ファイル固定**

- `consensus/core/build.rs` — `4bebbb8e4d33c2da4601b3782eb89fe95cc790a35ae83b12d4312bfdece91460` (5760 bytes)
- `consensus/core/src/palw_native_resource_v1.rs` — `ae50cb4be9442e207145eea7cc94bd174d6be8a89a12808acf7b2f4322e77267` (7208 bytes)
- `consensus/core/src/palw_native_resource_v1/ledger_expansion.rs` — `0f8690ccf277ce2e0b14912326d46ee3e6b5892567a5dc6d75175f2050ad774f` (62958 bytes)
- `consensus/core/src/palw_native_resource_v1/source_local.rs` — `fb82981273d1d274bac32459558d0b55a1481b2bf5b1c0cafac2844a3681e7bd` (13380 bytes)
- `consensus/core/src/palw_native_resource_v1/source_local/bootstrap.rs` — `7ccbd2bfd0a3d415a8df7833232b4f8beee6d76a29f8d239cf815f7fa2542bf6` (7657 bytes)
- `consensus/core/src/palw_native_resource_v1/source_local/tests.rs` — `0115daac6c89c8214377b5a89a2f3336d227fce035ff58e99706e0375fef120e` (17330 bytes)
- `consensus/core/src/palw_native_resource_v1/wire.rs` — `a81d241c6911f03f7d259a94a0f7cc0c434adc13f87af6bf84f19ddbc14823fd` (29697 bytes)
- `handoff/positive-court/A3-source-program-local-v1.md` — `34694d2d0f61ec6e99339fbe2f47b01553ff915ef41d1bbd917a83099a712143` (7054 bytes)

補助資料hash、確認項目、実行しなかった操作は対のJSONに記載。修正提案を必須とする新規findingは無し。A0が予定するRoot適用・実行検証のcapacity/grant判断は別途必要。
