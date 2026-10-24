# RFC07/08 private source economics entry points — 2026-10-24

Implementation record with the verification results recorded below. This is
not approval of economic terms, a real-node campaign, or release acceptance.

The source checker economics implementation uses new aux146 families. It does
not adopt the unfinished aux139 files from a separate dirty worktree; aux139
continues to mean router binding. Public presets leave the new option `None`.
Normal startup still refuses selected-source bootstrap, V6 paid bootstrap and
source checker economics. `PALW_T12_FULL_ACTIVATION_DAA` remains `None`.

## Explicit private inputs

The feature-gated private node accepts an optional
`--private-probe-source-verifier-economics-policy=ABSOLUTE_BORSH` only with an
explicit source profile. The fixed file is canonical Borsh of
`(u64 activation, PalwSourceVerifierEconomicsPolicyV2)`. No policy values or fee
terms are filled in. The source factory checks the source-policy ID, network,
schedule, finite work bound and native mirror, and still compares the stripped
base with the exact existing private factory fingerprint. Its manifest reports
the economics policy identity, aux146 and campaign acceptance `UNKNOWN`.

The producer JSON optionally supplies `verifier_funding` with
`fee_per_checker_sompi`, `carrier_funding_outpoint` and
`maximum_carrier_fee_sompi`. It is required exactly when that producer's private
Params supplies economics. Its carrier input must differ from the bond,
opening, seal and claim inputs. The source producer signs a separate
`FundVerifierEconomicsV2` action after observing its own accepted opening. Its
principal comes from the registered sponsor's native collateral through the
native fold; the separate transaction input pays carrier fees. The terms and
carrier have separate immutable WAL files. Restarts and branch changes recheck
the native work, exact terms, current funding, payer, input and fee authority.

A checker under the economics epoch waits for native funding and signs
`ApproveWithLiabilityV2` with that exact funding ID. Missing funding waits;
malformed or unavailable state returns an error. An existing WAL cannot change
between the historical approval namespace and the funded approval namespace.
The old action tags 0/1 keep their meaning; the two new variants append tags
2/3. The absent-option path continues using the old producer/checker actions.

## Retired compact-fence guard correction

Three private source-paid guards incorrectly required the retired compact
computation fence, while the paid-v4 factory requires that fence to be absent.
The factory, combined constructor and exporter now share a guard requiring the
actual active paid-claim challenge and release-recovery base, with the retired
fence absent. The source relation validator retains its exact stripped-base
fingerprint check. This repairs private construction; it does not enable normal
activation or introduce a computation proof.

## Verification and limits

On 2026-10-25 the real private factory group passed **3/3** (5.65 seconds):
source-paid/V6 construction with the retired fence absent, rejection of source
and consumer drift, and the explicit optional economics input. The last control
checks the policy/mirror and fingerprints and rejects network/bootstrap ID,
zero fee, late activation and truncated input. All normal arming refusals stay
in place. Command: `cargo --config 'profile.test.package.kaspa-consensus-core.opt-level=0' test -p kaspad --lib --features private-integration-config source_bootstrap_factory_v1 -- --nocapture`, with the shared target, debug=0,
incremental=0 and 64 MiB test-thread stack used by this continuation.

Log: `/tmp/misaka-rfc-source-factory-tests-final-20261025.log`.
Binary: `/tmp/misaka-rfc06-12-build-20261024/debug/deps/kaspad_lib-5c1f30073b2241b4`,
SHA256 `d8a7fa7264af8d1ba81c14ce79b021a350ae61128180085b0cd4f3674a91a603`.
This is a mutable local build-cache path, not a published release artifact.

The aux registry group passed **9/9** in
`/tmp/misaka-rfc-parallel-aux_registry_tests-20261025.log`, covering all nine
exact aux146 key families, truncation, unallocated keys and preservation of the
action discriminants. The economics namespace also passed 9/9; its aux146
scanner overlaps the registry group and is not an additional distinct test.
See the separate economics record for lifecycle and full-carriage controls.
These are codec and mechanism regressions, not claim correctness or economic
acceptance evidence. Final node/private-bin and gateway compilation checks
also passed; their exact scopes and logs are recorded by the remaining-work
ledger. The private producer pre-reserves both phase paths and their exact
capacity before copying, avoiding intermediate `join` growth buffers.

Signed multi-process funding, approval, objective court, release, real coinbase,
restart/reorg and pruning campaigns remain required. The formal K3 specification,
manifest, weights and signed execution inputs are uncreated. Existing small
fixtures do not replace them. The 15 RFC12 campaign gates remain unexecuted;
`UNKNOWN` is not promoted to `PASS`.
