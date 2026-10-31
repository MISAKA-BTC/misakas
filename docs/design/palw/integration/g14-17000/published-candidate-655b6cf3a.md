# testnet-12 G14-only verified candidate — DAA 17,400

Published at the user's explicit request. This is a G14-only candidate; full-feature implementation remains in progress. The recorded fleet remains on the old release after the IBM rollback. Publication does not deploy this package.

Linux x86-64 testnet release built from `655b6cf3a8f61ebdce920af867f6bedd87e717d6`. G14 activation is scheduled at **DAA 17,400**. Publication and installation before that DAA do not mean activation has occurred. Genesis and premine are unchanged.

Run `kaspad --testnet --netsuffix=12`. Expected params fingerprint:
`73e67678a432effb4a982386d67383b95cf172babdde3e6f86c31cf3268d5bd5`

Schedule id: `fd42c16706195ec0a6fa63902e1bd612303136b342bd29424e28be22599a5f2e`.

The binary bundle contains kaspad, misaka, palw-class and misaka-dnsseeder, with SHA256SUMS, source REV, build details, ELF checks and identity probe output. Use the adjacent `.sha256` file to verify the archive. kaspad/misaka/palw-class require GLIBC 2.39 or newer; the seeder requires GLIBC 2.34 or newer. The actual release node passed an isolated Linux x86-64 identity probe on GLIBC 2.41. This package is a Linux fleet build, not a claim that all platform CI checks passed.

The evidence bundle records **19 passing arming suites** on clean source `da46131a41fe7c90ec263b0f26ab9b444e4c74a2`, 324 test executions with overlapping filters, and **19 passing re-arm validation tests**. LG14-B, R-DETERM and material-intake fixture corrections preserve the consensus behavior they test. Empty filters and incomplete command results are rejected by the evidence runner.

Existing [typed waivers and explicit decisions](https://github.com/MISAKA-BTC/misakas/blob/655b6cf3a8f61ebdce920af867f6bedd87e717d6/consensus/core/src/palw_t12_17000_records_v1.rs) remain in force; no new waiver was introduced. A waiver is not a passing test or a guarantee of detection. Features without an armed fence remain disabled.

Claim credit requires claim-level selected-validator approval under the stated honest-fraction assumption. MISAKA uses probabilistic detection, public evidence, deterministic adjudication and economic deterrence; it does not use cryptographic proofs of model computation.

See the [release evidence and current deployment status](https://github.com/MISAKA-BTC/misakas/blob/main/docs/design/palw/integration/g14-17000/release-17400-2026-10-28.md) and [rollout procedure](https://github.com/MISAKA-BTC/misakas/blob/main/contrib/t12-deploy-kit/G14-17400-ROLLOUT.md).

Automated repository CI for this source [reported failure](https://github.com/MISAKA-BTC/misakas/actions/runs/40648597106); the attached local evidence and isolated Linux binary checks do not replace or relabel that result.
