# Private retained semantic material candidate

Unapplied `/tmp` candidate. `git apply --check` succeeds against current frozen worktree. No compile/test/node/RPC performed. Independent read review by D2 found no concrete authentication/heap-underquote blocker; compile/runtime remain unverified.

## Changed files

- New `consensus/core/src/palw_court_shared_semantic_material_v1.rs`: private Store child, opaque retained all-ID rendering/EOG/raw owner.
- `palw_court_shared_tir_builder_store_v1.rs`: one child declaration at EOF.
- `palw_court_shared_tokenizer_native_tests.rs`: actual existing signed custody, native original frame, immutable claim semantics, rooted inventory fixture tests immediately after real Store creation.

## Data/ownership

Actual current c.source/service/frame validation and equality with Store's Source; actual current immutable claim semantics, original acceptance clock, actual rooted EOF/hash/length. Tokenizer-only originals legitimately have no Constraint tail: an actual Constraint tail root, when present, must equal immutable table root; absent tail is not interpreted as absent registered tokenizer table.

All actual vocab IDs 0..V-1 must be present exactly once and in canonical ascending order. Some(empty) and None remain distinct authenticated values. Every old rendering opening verifies actual root/vocab. EOG uses actual PalwEogStream new/push/finish (including ID range/order and full keyed hash); no clone/sort-only `Eog::verifies` shortcut.

Every old standalone Borsh body is allocation-free scanned unconditionally before output grow/decode. Native Source construction/actual frame clone/hashwriter are reserved before current-source reconstruction; extra Source is dropped before output decoding. Additional reservation covers all decoded body scanner shapes, outer Vec, owned raw copy, inline output and EOG primitive. Single ticket conservatively retains source prelude peak through output lifetime; this over-retention is intentional, not RSS or full CPU/hash/IO pricing. Input carrier arrays/raw and existing Store/source remain caller-owned, separately admitted. Scanner timeout/unsupported/OOM and decoder I/O OOM preserve Resource; malformed bytes remain Decode.

No public codec/tag or new material retention/supply duty. No JSON validity/rendering correctness, original admission, paid authority, Complete flag, fee/profile/cap/fence change, state writer or custody guarantee. Existing original tail refusal remains unchanged. Post-admission snapshot binding here is not a pre-admission immutable-context coordinator.

## Tests for parent build

New filters:
- `semantic_material_retained_all_id_authentication_and_quota_release`
- `semantic_material_rejects_missing_duplicate_tampered_and_range_invalid_eog`

Expanded existing integration:
- `shared_tokenizer_builder_uses_native_original_eof_and_rechecks_authenticated_material`

Local tests authenticate actual generated Merkle table/body/EOG and flat raw commitment, distinguish absent/empty, test root-consistent out-of-range EOG counterexample, missing/duplicate/reorder/tamper/wrong raw/truncated schema, checked overflow, Resource-vs-Decode, reservation minus-one, growth refusal with unchanged ledger, exact total hold and drop release. Their Scope is a private local fixture, not native-state authentication. The expanded existing real original fixture exercises current native scope/EOF path, wrong claim/source tuple, missing ID and changed raw, without claiming funded/mined original acceptance.
