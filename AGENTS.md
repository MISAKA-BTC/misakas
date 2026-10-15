# MISAKA instructions

## Permanent user decision — 2026-10-15

MISAKA does not use, and will never use, cryptographic proofs of the correctness of LLM or model computation. Do not propose or implement zero-knowledge proofs, zk-SNARKs, zk-STARKs, zkVM, zkML, PCS-based succinct arguments or validity proofs, recursive proofs, or proof aggregation for this purpose. This restriction applies to RFCs, ADRs, designs, implementation, future plans, and research items.

Use probabilistic detection through sampled deterministic re-execution and algebraic spot checks, public evidence localization, deterministic adjudication, and economic deterrence. Consensus credit authentication uses approvals from randomly selected validators under an explicitly stated honest-fraction assumption. An optimistic Final or matching local replay does not replace that approval.

Hashes, signatures, Merkle commitments, and VRF/beacons may establish data identity, authentication, and randomness. They do not prove computation correctness. See [the governing principles](docs/PRINCIPLES.md#8-llm計算の暗号学的証明を使用しない--2026-10-15ユーザー決定).
