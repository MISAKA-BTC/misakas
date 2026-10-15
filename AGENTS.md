# MISAKA Working Rules

## Permanent Decision: Model Computation Verification

User decision, 2026-10-15: MISAKA does not use, and will not use,
cryptographic proofs of correctness of LLM or model computation.

- Do not introduce or propose zero-knowledge proofs, zk-SNARKs, zk-STARKs,
  zkVMs, zkML, polynomial-commitment-based succinct arguments or validity
  proofs, recursive proofs, or proof aggregation. This applies to designs,
  RFCs, ADRs, implementations, roadmaps, and research candidates.
- Use probabilistic detection (sampled re-execution, deterministic
  recomputation, and algebraic spot checks), localization using public
  evidence, deterministic adjudication, and economic deterrence.
- Authenticate consensus credit through approval by selected verifiers,
  stating the assumed honest proportion explicitly.
- Hashes, signatures, Merkle commitments, and VRF/beacons may authenticate
  data identity or provide randomness. They do not establish correctness of
  model computation. A local adjudication witness is not a succinct
  cryptographic validity proof.

See [docs/PRINCIPLES.md](docs/PRINCIPLES.md), especially section 8. Do not
reinterpret legacy terminology as permission to violate this decision.
