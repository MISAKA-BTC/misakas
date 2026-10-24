# RFC-0007 provider checker V6 receipt signing — 2026-10-24

## Repaired connection

The private checker already routes a successful provider check through
`objective_action_from_provider_check_v4` and `sign_provider_objective_action_v4`.
`VerifiedPaidClaimSourcesV2::receipt` selects the existing
`ReceiptApprovedBootstrapV6` namespace when its authenticated route contains a V6 binding.
The action signer accepted only ordinary `Receipt` and `RoundReceipt` variants, so this
private clean-result path failed with a receipt/court-role refusal before signing.

The signer now includes the existing V6 receipt variant in the same signer-bond check.
It signs the original exact kernel frame without converting the receipt to another namespace.
Native enrollment, selected assignment, source identity, signer authority, reorg/WAL namespace
checks and acceptance remain separate. No new receipt, source adapter or activation policy is added.

## Verification

The actual SDK crate test command passed, using the dedicated build target and command-only
unoptimized consensus-core test profile:

```sh
CARGO_TARGET_DIR=/tmp/misaka-rfc06-12-build-20261024 \
CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 \
cargo --config 'profile.test.package.kaspa-consensus-core.opt-level=0' \
  test -p misaka-palw-sdk --lib claim_paid_v2::provider_actions_v4 \
  --message-format=json -- --nocapture
```

**5 tests passed** (2m10s build, 0.13s runtime). Three existing action-selection controls
cover canonical demand order, reorg re-selection, clean/fault separation and refusal of
unknown/unsupported acquisition observations. Two new signing controls cover:

- Ordinary, staged-round and V6 receipt carriers retain their original encoded frame and
  round-trip through the real kernel decoder. Each ML-DSA signature verifies under the
  existing native message/context and fails under a different network or another receipt frame.
- Every receipt namespace refuses a different signing actor before invoking the signing key.

The synthetic receipt payloads test the signing/codec connection only. They do not create
native assignments, source independence evidence, accepted receipts or claim credit. Local
output is `/tmp/misaka-rfc07-provider-signing-tests.jsonl`, with build diagnostics in the
same-prefix `.stderr` file.

## Boundaries

`PALW_T12_FULL_ACTIVATION_DAA` remains `None`. Ordinary Params validation still refuses
activation of selected-source bootstrap and paid bootstrap V6. No shipped preset or
private source policy changed. This does not complete all-stage direct checking, real
Program/Pipeline/Typed/Segmented node acceptance, source economics/independence or the
uncreated K3 campaign inputs.
