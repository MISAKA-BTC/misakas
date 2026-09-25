# misaka-next

A reference implementation of Misaka's **Proof-of-LLM (PoL)** consensus, written so that a third
party can follow its safety argument from the specification to the code to the tests.

testnet-12 is frozen as the historical reference implementation (see [PROVENANCE.md](PROVENANCE.md)).
misaka-next does not keep compatibility with it. It takes t12's specification, its test vectors and
its attack regressions — never its code line by line — and rebuilds the consensus core around three
disciplines:

1. **Invariants before code.** Every safety property has an ID (`INV-CLAIM-01`, …) in
   [docs/consensus/09-invariants.md](docs/consensus/09-invariants.md), and the test that checks it
   carries the same ID (`inv_claim_01_…`).
2. **Wrong code should be hard to write.** Distinct clocks are distinct types (`LocalDaa`,
   `SafeDaa`, `FinalizedDaa`); a claim's authority is its type (`UnverifiedClaim` →
   `VerifiedClaim` → `FinalClaim`). A rule that must use the safe clock takes `SafeDaa`, so the
   compiler refuses the local one.
3. **Consensus is pure.** The consensus crates never touch storage, the network or RPC. Fork choice
   is a function of its inputs: `compare_chains(a, b, ctx) -> Ordering`.

## Order of work

```
Consensus Book + invariants  →  primitives  →  consensus model  →  adversarial simulator
    →  consensus tests (every t12 attack reproduced as a regression)  →  state / storage / network / node / rpc
```

Nothing below the line is started until every attack in
[docs/consensus/10-attack-model.md](docs/consensus/10-attack-model.md) is reproduced by the simulator
and refused by the model.

## Layout

```
docs/consensus/   the Consensus Book — the normative specification (00–10)
crates/           primitives, crypto, consensus/*, pol/*  (state, storage, network, node, rpc later)
tests/            consensus-vectors, adversarial, differential, simulation
fuzz/  benches/
```
