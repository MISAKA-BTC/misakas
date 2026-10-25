# RFC12 EVM round gas with market inactive

Status: baseline and empty-path defects reproduced; final native and full/fast runtime PASS. See [final verification](0006-0007-0008-0012-court-aux-provider-round-verification-2026-10-25.md).

Before repair, the retained template parent helper returned None when model-market
projection is inactive. Execution-lane round gas is independently active and
also needs that exact PALW parent. The producer then computes zero permitted
rounds, while validation derives actual permits from the native mergeset. Gas
limit is committed even for an empty EVM user transaction list. This is a source
finding until the actual native test reaches and observes the mismatch.

The new `evm_round_gas_actual_permitted_merge_with_market_inactive_matches_producer_and_validator`
parent extends the existing t12 round-lane harness. It retains real premine
UTXOs, actual signed panel receipts, an accepted floor Final, its actual
schedule/ticket and a signed accepted round block. It restores EVM activation
from genesis while leaving model market None, validates that private config,
and opts into the instance-local validation/template clock before building.
The harness never rewrites an EVM-active template timestamp or commitment.

Before testing the merging chain block, it requires the native round verdict
to contain that accepted round block and its distinct permitted count to be
positive. It builds through the real producer API, submits the untouched block
through FullUTXO validation, and checks the stored execution gas limit against
base plus the derived round budget. It does not substitute a hand-built gas
header or fabricated permitted-round facts for this path.

Harness keys and floor execution/trace roots remain explicit fixture inputs;
the signatures certify those claims under the existing selected-seat rules.
This is not K3 inference, an independent deployment, native39 settlement
mapping, Safe/Finalized mature-work acceptance, OS crash or the 15-gate campaign.
Public activation remains None and all real campaign gates remain UNKNOWN.

## Actual RED and repair

The initial five-package no-run build passed in 249.97s with frozen identities.
The actual native parent ran in 2.15s (2.20s controller). It earned a Final at
DAA142, snapshotted it for span264, accepted its signed round63843, and required
one distinct permitted round before building the merging candidate. That
candidate reached StatusUTXOValid, but its stored execution gas_limit was
30,000,000 rather than the derived 33,000,000. This is not an observed
commitment-mismatch rejection: both the producer and worker used the base cap,
so their shared omission masked the missing round budget. Log:
`/tmp/misaka-rfc-court-aux-provider-round-round-runtime-red-20261025.log`;
result `/tmp/misaka-rfc-court-aux-provider-round-round-results-red-20261025.json`.
The immutable Consensus executable is
`/tmp/misaka-rfc-court-aux-provider-round-kaspa_consensus-red-20261025`, SHA-256
`c5151db41b987706f9b530aaeb06c75f39d050ad7ca30751609d2a38d2a0f483`.

The template parent helper now retains the exact native parent when either
market projection or execution-round gas needs it. Market projection itself
keeps its independent existing gate. Validation computes the actual round cap
before consulting the speculative worker. A worker success is reusable only
with that exact gas limit; a worker error has no cap identity and is rerun
inline under the actual context before classification. The worker's existing
cached-header/base fallback cannot substitute for native round authority.
The actual native regression is unchanged for the repaired run.

This is consensus-visible repair under the existing ADR0139 round-gas rule.
The RED fixture proves an older binary accepted the lower committed cap; it
does not prove public history contains such a block or that all historical
blocks can be replayed interchangeably across these versions. Public-history
compatibility, release/upgrade review and deployment remain unverified. No
public binary replacement or new activation height is performed here.

## First repair failure and empty-path reproduction

The first repair build passed in146.83s and all13 new court/aux/provider parents
passed, but the unchanged native EVM parent still FAILED in2.27s: stored30M
versus expected33M. Its actual signed permit and FullUTXO acceptance both
completed. This remains a failed attempt, not GREEN acceptance. Result/log:
`/tmp/misaka-rfc-court-aux-provider-round-round-results-green-20261025.json`,
`/tmp/misaka-rfc-court-aux-provider-round-round-runtime-green-20261025.log`.

The full executor already replaces the derived environment gas limit with
`max(input.user_gas_cap, base)`. Its empty fast path did not. The existing
full/fast equivalence test previously covered only the base cap. It now also
compares four caps (zero/minimum, base, one permitted round, ceiling), both
receipt-fence states, implicit-genesis and real-transfer stateful parents,
and the snapshot execution entry point. These are16 parent/cap/fence cases
within one existing test, not16 independent native campaigns.

Before changing that production path, the extended actual EVM lib test
FAILED0/1 at cap33M (0.01s harness). Its no-run build passed77.87s. Immutable
binary `/tmp/misaka-rfc-court-aux-provider-round-kaspa_evm-fast-path-red-20261025`
has SHA-256 `58e36fefa344ee33bff41738119f575b9b0594806c36d942cb97ceefa16ec1b5`;
log `/tmp/misaka-rfc-court-aux-provider-round-fast-path-red-runtime-20261025.log`.
The empty path now applies the same existing gas-cap clamp as the full path;
the extended test and actual native test are unchanged for the final retry.
Final runtime passed: actual native parent1/1 (2.14s), extended equivalence1/1, full round-lane8/8 and executor31/31. The six-package build passed179.37s with31 matching source/config identities. The final verification record preserves the exact binaries, hashes, source manifest and logs.
