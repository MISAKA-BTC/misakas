# RFC-0007 saved checker receipt and native-view retry — 2026-10-24

## Production path and repaired behavior

The private checker obtains its own native assignment/source, performs the actual provider check,
builds the selected receipt through `objective_action_from_provider_check_v4`, signs it, and retains
its exact object and funded transaction in the checker WAL. Before relaying any saved carrier it
rechecks native authority and rehearses that original object.

Previously, `validate_saved_object` returned a string error for both fixed WAL identity mismatches
and changes in the current native assignment/context/seed. The `?` in the worker loop terminated
the checker on either error. Although the message said the old receipt was retained without
rebinding, the worker no longer monitored a later rollback restoring the original native binding.

The implementation now distinguishes:

- A receipt for another configured enrollment, round or actor remains a hard outbox refusal.
- A different current native assignment, source context or seed returns `NativeView`; the worker
  keeps the exact WAL and polls again, without building a replacement transaction, funding another
  action or changing the saved receipt.

The saved-object signature check runs before this retry classification. A bad signature is still a
hard refusal; a native-view change does not excuse it. Existing namespace checks, ordinary bootstrap
activation refusals, provider generation checks and native rehearsal are retained.

The new binding helper takes immutable references. Only the original native binding can make the
original saved statement match again. Matching this local guard is not receipt acceptance or
aggregate selected approval: native rehearsal and carrier submission remain required.

## Verification scope

The new controls use synthetic receipt/lock statements with real kernel encoding and ML-DSA signing.
They cover ordinary, staged-round and existing private V6 receipt namespaces; independent changes in
assignment/context/seed; restoration of the original snapshot; unchanged signed object bytes; and
hard refusal of enrollment/round/actor mismatch even when native binding also differs.

These are local checker/WAL controls, not a live reorg or four-adapter campaign. They create no native
source, assignment, accepted receipt, approval, funding or chain credit. Full stage coverage, real
Program/Pipeline/Typed/Segmented campaigns, source independence/economics and K3 inputs remain open.
`PALW_T12_FULL_ACTIVATION_DAA` remains `None`; public presets are unchanged.

## Actual crate results

The actual `kaspad` library test binary compiled with `private-integration-config`, and all
**17 checker tests passed** (0.22 seconds runtime), including the two new binding controls and
15 existing cold receipt, canonical demand, immutable transaction/funding, challenge-history,
finality and rollback controls:

```sh
TMPDIR=/private/tmp RUST_MIN_STACK=67108864 \
CARGO_TARGET_DIR=/tmp/misaka-rfc06-12-build-20261024 \
CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 \
cargo --config 'profile.test.package.kaspa-consensus-core.opt-level=0' \
  test -p kaspad --lib --features private-integration-config \
  private_integration::kernel_paid_checker_v4 --message-format=json -- --nocapture
```

The first test run used macOS's default temporary directory beneath `/var`, whose symlink was
rejected by the existing `owned_dir` check: the two new tests passed, but 11 existing filesystem
fixtures failed. The successful rerun uses the real `/private/tmp` path and leaves production
symlink refusal unchanged. The stack size is a test-runner setting, not a production memory bound.

Final local output is `/tmp/misaka-rfc07-checker-source-retry-verified.jsonl` and its `.stderr` sibling.
The actual binary is `/tmp/misaka-rfc06-12-build-20261024/debug/deps/kaspad_lib-5c1f30073b2241b4`.
Document checks report 43 known reference issues and zero new issues; these checks are not semantic
acceptance evidence. No live node, network campaign or native selected-approval acceptance was run.
