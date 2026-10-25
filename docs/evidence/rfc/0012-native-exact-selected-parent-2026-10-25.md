# RFC0012 exact immutable EVM parent identity

Status: coordinated build and dedicated runtime PASS; see [verification record](0006-0012-capacity-parent-head-verification-2026-10-25.md).

The node renderer formerly resolved an EVM block's parent through the current canonical number N−1. A retained side block, or a notification queued across a reorg, could therefore expose another branch's parentHash. A failed parent read also silently produced zero. The latter consumer is handled by the checked renderer/head continuation; this record covers its native identity source.

`ConsensusApi::get_evm_parent_l1_hash` now reads the existing immutable target EVM header and its Ghostdag selected parent directly. It never uses the mutable EVM number index, receipts, raw transaction store or optional trace bodies. A present parent EVM header must have number+1 equal to the child and state root equal to the child's committed parent_state_root. Checked arithmetic refuses overflow. Malformed/missing referenced history returns an error rather than a canonical replacement or fabricated zero.

The API requires an existing target EVM header. Absence there is an error; ordinary unknown block RPC requests return None at their block reader before calling this API. None from this API means only the existing zero-number boundary or the first execution (number1, no parent EVM header, native selected parent at genesis or strictly before activation, and the existing EVM genesis state root). Missing post-activation history is an error. Unimplemented readers default to unavailable, never a successful absent parent. Existing commitments, EVM execution, activation and wire layouts are unchanged.

Four prepared native parents use the actual reader:

- Real native accepted sibling branches and their children keep distinct selected parents while the number index is rebound to the other branch or corrupted. EVM headers are explicit storage fixtures, not EVM-executed headers. Malformed receipts demonstrate that parent identity does not read irrelevant transaction bodies.
- Actual cold malformed target EVM header, compact Ghostdag, parent EVM header and native compact-header bytes propagate typed read failures.
- Unknown target is unavailable; zero-number/genesis/pre-activation first-execution boundaries return None, while a missing parent exactly at activation cannot do so.
- Parent number mismatch, state-root mismatch, overflow and absent non-first history refuse.

These controls do not establish EVM execution/certification, a full process restart, Safe/Finalized positive settlement, physical pruning/reorg campaign or cryptographic computation correctness. They test stored immutable identity and actual native branch selection, separately from the node's injected-API renderer/retry controls. Concurrent history pruning requires the caller's existing consensus-session discipline; no new cross-read transaction or durable WebSocket delivery is claimed.

Activation None, normal selected-source/V6 bootstrap refusal, dormant unapproved economics, frozen model tiers and claim-selected approval remain unchanged. K3 inputs remain uncreated, native39 settlement mapping unformulated and 15 real campaign gates UNKNOWN.
