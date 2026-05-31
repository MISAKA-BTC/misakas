# ADR-0002: ML-DSA-65 P2PKH as the only standard script

Status: Accepted (Phase 1). **Signature scheme superseded by [ADR-0019](0019-mldsa87-migration.md)** — ML-DSA-65 → **ML-DSA-87**, full PQ-only. The P2PKH structure and acceptance criteria here still hold; what changed: the scheme name/sizes (pk 2592 / sig 4627), the tx context `b"kaspa-pq-v1/tx/mldsa87"`, and the address payload (64-byte BLAKE2b-512). Source identifiers still read `MlDsa65` pending a cosmetic rename (design doc §J / ADR-0019 P8, deferred).
Date: 2026-05-28
Supersedes: —
Superseded-by: ADR-0019 (signature scheme)

## Context

The mainline Kaspa transaction script uses Schnorr (secp256k1) or ECDSA
over secp256k1. Both schemes are vulnerable to a sufficiently large
quantum computer running Shor's algorithm against the discrete-log
problem. To make the chain useful for long-term value storage in the
PQ-transition era, we replace the signature scheme with a NIST FIPS 204
standardized scheme.

ML-DSA-65 keys and signatures are large:

- public key: 1952 bytes
- signature: 3309 bytes (+1 byte sighash type when scripted)
- private key: 4032 bytes

This rules out P2PK (pay-to-public-key) as the standard output form,
because P2PK would store the full 1952-byte public key inside every
UTXO, exploding UTXO set size. P2PKH (pay-to-public-key-hash) keeps
outputs at ~36 bytes and pushes the public key to the spending input
where it disappears after confirmation.

## Decision

The **only** standard output script in kaspa-pq is ML-DSA-65 P2PKH:

```
scriptPubKey:
    OP_DUP
    OP_BLAKE2B_256
    OP_DATA32 <BLAKE2b-256(public_key)>
    OP_EQUALVERIFY
    OP_CHECKSIG_MLDSA65
```

A new address `Version::PubKeyHashMlDsa65 = 2` is introduced. Its
32-byte payload is `BLAKE2b-256(public_key)`. The existing
`PAYLOAD_VECTOR_SIZE = 36` SmallVec capacity already accommodates
a 32-byte payload, so the address-bech32 layer does not need a layout
change.

A new opcode `OP_CHECKSIG_MLDSA65` is added. It pops `<public_key>`,
then `<signature || sighash_type>`, length-checks both **before** any
allocation-heavy work, computes the sighash with
`calc_mldsa65_signature_hash`, and calls
`libcrux_ml_dsa::ml_dsa_65::verify` with the fixed context string
`"kaspa-pq-v1/tx/mldsa65"`.

`MAX_SCRIPT_ELEMENT_SIZE` is widened from `520` to `4096` to admit
the 3310-byte signature item and 1952-byte public-key item.

ML-DSA P2PK, legacy `Version::PubKey`, legacy `Version::PubKeyECDSA`,
legacy multisig, and `Version::ScriptHash` wrapping any of the above
are explicitly **non-standard** in kaspa-pq. Whether to remove them
from the consensus opcode table outright or to keep them disabled at
mempool admission is a Phase 4 decision; the design rule is that
no PoC wallet can produce or accept them.

The `libcrux-ml-dsa` dependency is pinned exactly:

```toml
libcrux-ml-dsa = "=0.0.9"
```

with `Cargo.lock` committed and `cargo build --locked` in CI. RustSec
advisory monitoring is mandatory.

## Consequences

### Positive

- Standard outputs become quantum-resistant.
- UTXO entries stay small (~36 bytes scriptPubKey) — the giant key
  material only appears on the input side.
- One single output template means simpler wallets, simpler mass
  policy, and simpler test surface.

### Negative

- Inputs are large (~5.3 KB per spend) — transaction byte mass and
  signature-op mass both have to be retuned (see
  [ADR-0005](0005-mass-policy.md)).
- We lose multisig at PoC time. Reintroducing it requires either
  threshold ML-DSA (not yet standardized) or a script-hash style
  composition, deferred to a follow-up ADR.
- We are temporarily dependent on a `<0.1` library
  (`libcrux-ml-dsa 0.0.9`). The library is high-assurance but
  pre-release; mitigations are exact pinning, lockfile commitment,
  differential KAT testing against a second implementation, and
  RustSec monitoring.

### Neutral

- We deliberately use BLAKE2b-256 for the public-key hash because the
  consensus opcode table already contains `OP_BLAKE2B_256`. Switching
  to BLAKE3 would require adding a new consensus opcode; that is a
  larger change than the PoC needs.

## Alternatives considered

1. **SLH-DSA (FIPS 205) instead of ML-DSA-65.** Rejected: signatures
   are an order of magnitude larger again, and verify cost is
   higher. ML-DSA is the right operating point for an L2-style
   chain where verify throughput matters.
2. **Falcon / FN-DSA.** Rejected for the PoC: less mature
   constant-time implementations in Rust, floating-point
   verification subtleties.
3. **Hybrid Schnorr + ML-DSA.** Rejected for the PoC: doubles the
   signature footprint with little PQ benefit if the Schnorr half
   is required to verify.
4. **P2PK ML-DSA-65 (no hash).** Rejected for UTXO-set size reasons.
5. **Custom BLAKE3-based P2PKH.** Rejected for PoC; revisit in Phase 4
   if benchmarks justify a new consensus opcode.

## Implementation notes for Phase 4

Files expected to change:

- `crypto/addresses/src/lib.rs` — add `Version::PubKeyHashMlDsa65`.
- `crypto/txscript/src/opcodes/mod.rs` — add `OP_CHECKSIG_MLDSA65`.
- `crypto/txscript/src/standard.rs` — add
  `pay_to_pub_key_hash_mldsa65`; route from `Version::PubKeyHashMlDsa65`.
- `crypto/txscript/src/script_class.rs` — new `ScriptClass` variant.
- `crypto/txscript/src/lib.rs` — bump `MAX_SCRIPT_ELEMENT_SIZE` to
  `4096`.
- New module `crypto/txscript/src/mldsa65.rs` — verify wrapper,
  length pre-checks, context constant.
- `consensus/core/src/sig_hash.rs` (or equivalent) —
  `calc_mldsa65_signature_hash`.
- `consensus/core/src/sigcache.rs` (or equivalent) —
  `Mldsa65SigCacheKey`.
- `Cargo.toml` of the workspace — `libcrux-ml-dsa = "=0.0.9"`.

## Acceptance criteria (Phase 4)

1. A well-formed ML-DSA-65 P2PKH spend on simnet verifies and the
   tx is accepted into a block.
2. Each of the following malformations is rejected **without**
   reaching `verify`:
   - wrong public-key length,
   - wrong signature length,
   - wrong sighash type byte,
   - non-canonical push,
   - script element > `MAX_SCRIPT_ELEMENT_SIZE`.
3. A spend with a public key whose `BLAKE2b-256` does not match the
   address payload is rejected.
4. Context-mismatch (e.g. a signature made with a different `ctx`)
   is rejected.
5. The signature-cache stores `Mldsa65SigCacheKey` (96 bytes plus
   tag) and not the raw signature/public key.

## References

- FIPS 204 (ML-DSA).
- `libcrux-ml-dsa` 0.0.9 on docs.rs.
- RustSec advisory regarding pre-0.0.9 AVX2 verify acceptance of
  invalid signatures.
