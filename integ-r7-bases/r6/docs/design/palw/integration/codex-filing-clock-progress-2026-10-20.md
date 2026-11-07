# RFC14/G14 — sequential witness filing clock candidate

Status: Proposed / Review Required, branch `codex/rfc14-x3-reorg-clock`, based on `4f41081ee`. [ADR-0193](../../../adr/0193-sequential-witness-filing-clock.md) separates this unadopted clock choice from the approved ADR-0192 A/A detailed-spec work. No arming, integration or deployment.

## Scope from the real pipeline

X3 already de-stacks preparation WorkDebits by whole pass and owner/response work by turn. The main worktree's conditional 12,420 DAA lower bound for the legacy `network_v1` quote does not establish the same bound for this clock. All shipped frame-cost presets remain None and the pinned record list remains empty. There is no measured production door.

The node's `kaspad/src/palw_tir_public_filer_scheduler.rs` builds each staged part using accepted `TirWitness.received.len()` as its offset. It waits for that cursor before sending the next part. V1's filing allowance of two inclusion margins plus aggregate byte inclusion does not explicitly count each sequential wait. The actual plan uses `stream + max(collection_daa, filing_daa)`; a long collection window can already cover the difference. This is not a finding that every existing plan is too short.

## Candidate and compatibility

Only explicit clock version 2 uses `max(2, maximum_moves) * inclusion_margin + aggregate_byte_inclusion`. It conservatively uses the existing terminal envelope maximum_moves; no maximum, retry count, supported model scope or preset changes. V1 calculation and serialization remain unchanged, including the original fixture's byte-for-byte plan derivation.

The existing real allocation helper propagates filing to collect/material/bond exit. A required door above the unchanged maximum is refused with `DoorBeyondLiabilityHorizon`. If explicitly configured, V2 has different params/schedule/resource identities; a V1 record cannot authorize it. No V2 record is added and lifecycle tag 230 is not resolved by this candidate.

## Validation and remaining work

Raw logs, commands and SHA-256 are recorded in the [evidence record](../../../evidence/palw/filing-clock-v2-2026-10-20/README.md). Final release checks: 2 native plan tests plus 29 identity/arming regressions passed (31 total, zero failed/ignored). `cargo check --release -p kaspa-consensus` succeeded. X3 document validation passed with zero new issues and 43 known baseline references. These are correctness checks, not T01-T15 closure or capacity measurements. Existing synthetic throughput, margins and collection windows are not proposed production values.

Actual carrier costs, slowest supported node CPU/IO/memory, physical reserved supply, fair inclusion assumptions, all-owner collusion/shared pursuit, signed dependency horizons, atomic clocks and duty/default rules, end effects, retention/recovery and restart/pruned IBD remain open. The 3,120 / 10,000 requirements and arming refusal remain.
