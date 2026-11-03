# A3 owner-local Source program preparation — V1

Status: source-only; Cargo/rustc and all new runtime tests remain HOLD under A0's shared build scheduling. This component is not complete generic artifact/process startup. The branch is `codex/a3-source-program-local-v1`, based on `89b1d6346` (public Vec policy dependency). No native flag, checker, host reservation, ledger pricing, grammar, wire ID, registry route or default backend is activated.

## Concrete API and ownership

`scan_source_program_local_v1(bytes, limits, &mut work)` returns a private-field `ScannedSourceProgramLocalV1` borrowing exactly the immutable scanned bytes. Its consuming `decode()` invokes the existing `TirProgramV1::decode_canonical` exactly once on those bytes and returns `LocalSourceProgramOwnerV1`. There is no replacement-byte argument, public unchecked constructor, supplied shape/count, decoder callback, deep Clone or mutable program access. The owner retains its complete logical preparation quote; the shared program getter can be explicitly copied only under the caller's separate export reservation.

Four positive explicit limits cover program bytes, scanner steps, NF structural items and logical native requests. Persistent `LocalScanWorkBudgetV1` has no reset/refund/Clone: accepted attempts precharge work atomically, including attempts that later fail. Program ownership here is not `NativeRetainedV1`, a `NativeDemandCheckerV1` lease, native certification or a host concurrency reservation. Input backing, simultaneous owners, formatting/copies and source/plan/map/weights/verified-space coexistence belong to the caller's separate inventory.

## Admission and diagnostic phases

The fixed no-allocation order is four zero-limit checks, byte ceilings, scan-step upper ceiling, private supported-build gate, typed diagnostic bootstrap ceiling, cumulative work precharge, unchanged memory-only scanner through observed thin ports, NF-item/diagnostic census, and full native-request ceiling. Only then may consuming decode allocate. Insufficient limits, unsupported profile and arithmetic failures escape as fixed enums without an owned diagnostic String.

The scanner bootstrap enumerates the finite borrowed-memory scanner vocabulary and actual Rust types. It uses checked shared Vec growth envelopes, concrete `io::Error`, padded pinned std `Custom`, boxed `StringError` String header, `NativeDecodeErrorV1<String>`, wrapper and observation headers, compile-time type-name lengths and MAX across alternative Resource/IO error branches. It does not cover arbitrary `Read` or custom provider errors.

The existing Source q remains unchanged. Its four growing diagnostic byte-writer envelopes use `D = 60*n + 1024`. A separate finite NF/Borsh Display census checks that maximum shape/perm/UTF8 strings and pinned format initial estimates fit these slots. Additional typed decode requests cover actual error/String/UTF8 box headers, same-bytes slice coordinate and Result framing, without charging the four existing byte writers again. The admitted envelope is `MAX(scan bootstrap, Source q + diagnostic headers + owner root delta)`, because scanning and decode are sequential. Backend errors are classified and dropped locally; only fixed `Encoding`, `NormalForm`, `Shape`, arithmetic or coverage kinds escape. The canonical decoder, validation order and old error text remain intact for legacy callers.

## Scanner observations and work

Private optional ports preserve the old wrappers with `None`. Their grammar, q operations, preliminary wider ledger arithmetic, first-error order and component strings are unchanged. Four fixed scalar slots record outer, nested, StepSpace and raw Source readers before success/error propagation. They count successful reader-boundary bytes, including repeated traversal and framing, rather than unique source bytes or physical IO. Refused skip lengths consume zero; partial EOF records the actual consumed bytes. Duplicate passes, negative consumption and overflow refuse observation coverage.

With `L(x)=8*x+4096`, the accepted per-attempt precharge is `L(n+4)+3*L(n)+4` scanner steps and `4*n+4` reader-boundary bytes. The four additions include each reader's increment-before-limit refusal. Actual observations do not debit again or refund precharged work. Scan steps and NF item counts are not all-NF validation CPU, compile CPU, elapsed time or physical RSS guarantees.

## Audited build profile and trust

A private Core build script emits both a cfg and a build-generated profile environment value; the new API requires both. Supported scope is exact Rust release 1.93.0, registry Borsh and Borsh-derive 1.5.1 with the audited Cargo.lock source/checksum, 64-bit little-endian aarch64/x86_64 Darwin or GNU Linux, and known compiler flags. Unknown profiles, wrappers, custom sysroots/targets and unsupported flags keep building the crate but the new entry point refuses before the diagnostic backend. The script itself has not yet run in this stage.

This checks Cargo resolution and compiler metadata under a trusted local toolchain/sysroot and installed dependency-source premise. It does not hash every installed `.rs` file, detect every same-version local source replacement, authenticate a compiler binary, or add a supply-chain attestation mechanism. A standalone externally injected cfg cannot authorize decode without the generated supported environment value. Existing legacy/native behavior is independent of this gate.

## Source regressions and pending validation

On an audited native build the facade declares eight active tests plus one exact ignored child helper; the helper runs inside one parent case and is not another outer test. Two additional thin-port tests cover failed-step accounting, refused skip/partial EOF and differential legacy errors. Unknown native builds compile three facade active tests plus the helper; wasm32 compiles two active cases and no process tests. Unsupported-profile cases explicitly identify refusal rather than claim successful decode; audited success cases require both private build outputs.

Coverage includes narrow/wide same-bytes identity and old Source q, exact/one-below limits, zero-global native/logical policy in an isolated child, repeated failed attempts without refunds, unknown versions/truncation/hostile name lengths, UTF8, malformed ranks and canonical error kinds. These are source regressions, not allocator instrumentation. No Cargo, rustc, successful runtime, Clippy or measured allocation result is claimed for this commit. A0 owns subsequent integration/validation scheduling.

The preserved diagnostic audits are evidence of source reasoning, not runtime evidence. Whole startup compilation, StepSpace, rooted artifact values, generic child startup, provider mutation hooks, SDK/native registration and full-process acceptance still require their own implementation and integration. Canonical bytes and authenticated data identity never establish model computation correctness, model qualification, claim certification or credit.
