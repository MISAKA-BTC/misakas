# M-X3 — measured deadline budget status

**NOT_MET.** No V10 term is promoted to an arming value. The existing passive t12 observation [2026-10-19-t12-passive-daa-blocks.md](../../../../../measurements/2026-10-19-t12-passive-daa-blocks.md) reports a median 184 s per DAA and observed maximum 111,949 transaction bytes per block. These are conversion inputs, not measurements of a reserved court carrier lane or a worst-case inclusion delay.

The SPINE checkpoint `ebb8b911f` identifies the slice as card 1's testnet-12 genesis class and the single-prompt decode job from `canonical::claimed`. This selects the fixture family, but it does not pin a real artifact or maximum legal profile. Required run: fix those inputs, then perform the complete cold material fetch, deterministic check, every localization round, disclosure/court, carrier submission and observed inclusion, and reorg replay on the supported minimum-specification node. Record each stage's warm-up and at least 20 repetitions, traffic conditions, byte totals, load, and a nonzero retention price from the actual storage economics. Convert each duration to DAA with the observed 184 s/DAA only as an explicitly labeled estimate; the accepted bound must cover observed tails and congestion. The five `B_*` terms and `c_retain` remain blank until those measurements exist. Do not fill any term with zero or carry forward the old candidate 5,520 DAA as a measurement.

The arming gate in `consensus/core/src/palw_g14_gates_v1.rs` additionally requires X3 fold evidence, node E2E, and a shipped-binary fence crossing with restart/reorg. This document supplies none of those gate digests.

## Scope and inclusion review (continuation)

At inspected source `2119c943b`, `PALW_T12_KERNEL_SLICE_FENCES_V1` explicitly includes `PALW_T12_FULL_CLAIM_HARD_DEADLINE_ENTRY` (`full_activation_v1.rs:309`). Therefore the VAL report's statement that X3 can be postponed does **not** describe the current slice list. M-X3 remains required for that list. Neither the older 5,520-DAA scenario nor the later provisional Δ=2,016 is a measurement.

The [inclusion calculation review](../approval-material.md) also rejects treating the 4-DAA fluid-byte estimate as a demonstrated default-liability bound. Measure actual carrier serialization, priority/fragmentation, backlog and reorg before adopting the premise.

## Explicit interim parameter judgment

Retain X3 in the current slice and retain its existing **provisional** policy candidate: cold/check/localize/carrier/reorg = **512/512/512/120/360 DAA**, Δ = **2,016 DAA**, retention = **36,058 sompi/DAA**. This is the choice to keep an estimated candidate, not to remove the deadline fence or to mark the measured requirement met. Product policy and evidence pins remain ACT's integration step; this branch changes neither.

The candidate is not established for the newly available Dolphin 32k profile. Its check-term comment assumes 4,096 tokens at 60 s per 16 tokens (128 DAA at the 120 s target), then multiplies by 4 to get 512. Substituting 8,192 or 32,768 tokens at that same assumed rate gives 256 or 1,024 DAA **before** the 4× factor. Do not silently reuse the 4k coverage for those profiles. The forthcoming real execution measures the rate; it still cannot establish minimum-node performance. Likewise 2,250,000 B in the retention-price formula is one maximum close, not an observed entire 32k claim's retained material. Recompute on the selected legal profile and actual retention obligation.

The arithmetic T06 margin (303 DAA under the old candidate) remains conditional. Any enlarged measured term must be propagated through the complete deadline/retention composition before ACT pins it.
