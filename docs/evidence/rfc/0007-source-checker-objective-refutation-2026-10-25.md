# RFC-0007 source checker objective refutation — 2026-10-25

## Production connection

The private direct Program source checker previously withheld its approval when exact replay
returned a fault but submitted no objective refutation. A `KernelGoalV1::Prove` now reaches the
existing authenticated native `KernelRouteV1` / `FileProof` path. `Demand` and local replay errors
still withhold approval without accusing a producer of unavailable material or non-response.

Prosecution requires an explicit optional checker spec field:

```json
"objective_refutation": {
  "funding_outpoint": "<private transaction id>:<index>",
  "maximum_fee_sompi": 100000
}
```

The fee value above illustrates the field only; it is neither a policy default nor an approved
parameter. The separate input may not equal the approval input, the checker bond, or a public
genesis outpoint. The checker uses its existing explicitly configured payer and input/workspace
ceilings. Without this optional authority it withholds approval and submits no refutation.

A retained native ledger clone applies the exact filing with admitted mutations before the WAL
is published. Publication requires an actual `Convicted` event for the same claim. Accepted
`ProofDismissed`, duplicates, insufficient collateral and other semantic refusals do not authorize
publication. Projection/decoding/resource failures remain errors. Collateral uses the existing
public committed-bond reader, the configured audit fence/depth and the same own anchor ring.
No new court rule or core API is introduced.

`objective-refutation.borsh` and `objective-refutation-carrier.borsh` retain the exact signed object
and transaction. The object binds native network, checker actor, target claim and objective
refutation bytes under the existing ML-DSA kernel-route context. The carrier must spend the
explicit refutation input, return one change output to the payer, and remain within its fee cap.
Cold reads revalidate these bindings. Mempool presence is only a wait condition. A fresh own
observation, reserved exact-conviction preview and native rehearsal are checked after the async
pool lookup before relay. These checks and submission use the same consensus session, including
when the manager replaced its instance; a branch change retains the original WAL. WAL path
copies are reserved before construction. Only the native court fold establishes conviction.

## Verification status

Runtime verification passed **3/3** on 2026-10-25 in 0.12 seconds. The tests cover an actual small
OPV Program with a deliberately false committed MatMul value, exact direct replay producing its
objective finding, and native ledger conviction preview; a malformed filing cannot authorize
publication. Other controls cover cold signed object/carrier decoding, signer/domain/proof and
signature substitutions, explicit input/fee bindings, and preservation of the input ledger root.
These are small mechanism tests, not a multi-process native submission campaign.

The coordinated node filter was `source_bootstrap_checker_v1::refutation::tests`. Its fresh binary
was `/tmp/misaka-rfc06-12-build-20261024/debug/deps/kaspad_lib-5c1f30073b2241b4`, SHA-256
`d99ff3055ce45e90244775fd1a1db26415043882807c19a1252ae6bbbb1a7377`.
The final log is `/tmp/misaka-rfc-source-refutation-tests-final-20261025.log`.

The first build failed with test-only E0624 (private ledger projection API) and a claim comparison
type error, recorded in `/tmp/misaka-rfc-source-refutation-tests-20261025.log`. The fixture now
uses the existing public retained `ledger_checked_v1()` API and compares claim slices; no
production visibility was widened. The corrected retry passed 3/3 in 0.15 seconds in
`/tmp/misaka-rfc-source-refutation-tests-retry1-20261025.log`, followed by the fresh final 3/3
above. These repeat runs cover the same three tests, not six distinct tests.
No independent Cargo build was started while other source owners were editing shared core code.

## Remaining boundaries

Whole-carrier filings must fit the existing explicit byte cap. This connection adds no chunked
refutation funding/seal campaign and grants no local success marker. Actual native dispatch from
multiple processes and accepted court-conviction campaigns remain unexecuted. Native source campaigns,
funded FS StageMessage, independent sources and K3 remain unexecuted or uncreated; campaign gates
remain UNKNOWN. Public activation remains `None`, ordinary selected-source/V6 startup remains
refused, and all model-tier and claim-selected-approval requirements are unchanged. Objective
refutations use deterministic local adjudication, never cryptographic computation validity.
