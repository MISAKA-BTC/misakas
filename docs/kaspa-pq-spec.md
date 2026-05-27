# kaspa-pq Specification (v0.1, draft)

Status: Draft — Phase 1 deliverable. Frozen values listed here are the contract
that Phase 2 onward must implement. Any change must go through an ADR update
under `docs/adr/`.

## 0. Scope and non-goals

This document specifies a quantum-resistant Kaspa-based network ("kaspa-pq")
forked from rusty-kaspa. It is **not** a compatibility layer with the
mainline Kaspa network.

### In scope (PoC + production roadmap)

1. Signature scheme replacement: ML-DSA-65 (FIPS 204) P2PKH only.
2. UTXO accumulator replacement: LtHash16_1024.
3. Network-level isolation (NetworkId, genesis, address prefix, ports).
4. UTXO commitment field widened to 64 bytes in the production design
   (PoC may keep 32-byte commitment if explicitly noted).

### Out of scope (Phase 1)

- Mainline Kaspa interoperability (wallet, RPC, P2P, address).
- Widening `kaspa_hashes::Hash` (txid / block hash / merkle root) past 32 bytes.
- PQ-strengthening the PoW hash, block hash, txid, or merkle root.
- ML-DSA multisig, script-hash composite scripts, smart contracts.
- Hardware-wallet support, BIP32-style hierarchical key derivation that requires
  a discrete-log-friendly curve.

## 1. Base version

- Upstream: `rusty-kaspa` workspace package version `1.1.0`
  (see [Cargo.toml](../Cargo.toml) → `[workspace.package].version`).
- Vendoring commit recorded at the repository root as the initial git commit
  `vendor: rusty-kaspa v1.1.0 base`.
- The vendored snapshot is treated as a hard pin. Upstream merges must be
  reviewed against this specification before being accepted.

## 2. Frozen constants

These constants are normative. Implementations must use exactly these values
unless a follow-up ADR amends them.

| Constant | Value | Where it appears |
|---|---|---|
| `MLDSA65_PK_LEN`  | `1952`  | ML-DSA-65 public key length (bytes) |
| `MLDSA65_SIG_LEN` | `3309`  | ML-DSA-65 signature length (bytes) |
| `MLDSA65_SIG_ITEM_MAX_LEN` | `3310` | signature item incl. 1-byte sighash type |
| `LTHASH_LANES`    | `1024`  | Number of 16-bit lanes in LtHash state |
| `LTHASH_LANE_BYTES` | `2`   | Bytes per lane |
| `LTHASH_STATE_BYTES` | `2048` | Serialized accumulator state size |
| `UTXO_COMMITMENT_BYTES` (production) | `64` | Header UTXO commitment field |
| `UTXO_COMMITMENT_BYTES` (PoC, optional) | `32` | PoC-only shortcut, must be flagged |
| `MAX_SCRIPT_ELEMENT_SIZE` (kaspa-pq) | `4096` | up from upstream `520` |
| `MAX_SCRIPTS_SIZE` (kaspa-pq) | `10_000` | initial value, unchanged from upstream |
| `MAX_STACK_SIZE` (kaspa-pq) | `244` | initial value, unchanged from upstream |
| Signature context (default) | `"kaspa-pq-v1/tx/mldsa65"` | ML-DSA `ctx` parameter |
| Wallet keygen domain | `"kaspa-pq-wallet-v1/mldsa65/keygen"` | XOF domain separator |

These are pre-implementation freezes — they are the spec, not derived from
running code.

## 3. Cryptographic decisions

### 3.1 Signature

- Algorithm: ML-DSA-65 (FIPS 204), pure mode, with a fixed `ctx` value
  (see §2 and [ADR-0002](adr/0002-mldsa65-p2pkh.md)).
- Library: `libcrux-ml-dsa = "=0.0.9"` (exact pin).
- Verify-time pre-checks: signature length and public-key length must be
  validated **before** calling into `verify`.
- See [ADR-0002](adr/0002-mldsa65-p2pkh.md).

### 3.2 UTXO accumulator

- Algorithm: LtHash16_1024 (Meta).
- State: 1024 lanes × 16 bits = 2048 bytes.
- Element serialization includes the spending outpoint to defeat the
  2^16 duplication wrap-around.
- See [ADR-0003](adr/0003-lthash-utxo-accumulator.md).

### 3.3 UTXO commitment

- Production: BLAKE2b-512 (or BLAKE3 XOF) of LtHash state → 64-byte
  `UtxoCommitment64` type.
- PoC: 32-byte commitment is permitted but must not be claimed as
  the 200-bit-security finalization of LtHash.
- See [ADR-0004](adr/0004-utxo-commitment64.md).

### 3.4 Hashes that stay 32 bytes

- `txid`, block hash, merkle root, accepted-id merkle root, RPC `Hash`:
  unchanged.
- Only the UTXO commitment is widened, via a dedicated `UtxoCommitment64`
  type. The general-purpose `kaspa_hashes::Hash` remains 32 bytes.

## 4. Network identity

- New `NetworkId`, genesis block, address prefix (`kaspapq`), P2P port,
  RPC port, DNS seed list, protocol handshake magic, initial UTXO commitment.
- See [ADR-0001](adr/0001-network-isolation.md).

## 5. Standard transaction format

### 5.1 Address

- New `Version::PubKeyHashMlDsa65 = 2`.
- Payload: `BLAKE2b-256(public_key)` = 32 bytes.
- The existing `PAYLOAD_VECTOR_SIZE = 36` SmallVec accommodates 32-byte
  payloads without resizing.
- The `payload` field of an address is **never** a raw ML-DSA-65 public key.

### 5.2 scriptPubKey (output)

```
OP_DUP
OP_BLAKE2B_256
OP_DATA32 <BLAKE2b-256(public_key)>
OP_EQUALVERIFY
OP_CHECKSIG_MLDSA65
```

Approximately 36–37 bytes per output.

### 5.3 signatureScript (input)

```
PUSH <signature || sighash_type>     ; 3309 + 1 = 3310 bytes payload
PUSH <ML-DSA-65 public key>          ; 1952 bytes payload
```

Approximately 5267 bytes including push opcodes. The full input
(outpoint + length + script + sequence) is approximately 5319 bytes.

### 5.4 sighash

`calc_mldsa65_signature_hash` is added as a new function alongside
the existing `calc_schnorr_signature_hash` / `calc_ecdsa_signature_hash`.
The ML-DSA `ctx` parameter binds the signature to the network and scheme
(see §2).

## 6. Mass / DoS policy (initial)

These values are placeholders for PoC. They will be replaced by
benchmarked values in Phase 6 — see [ADR-0005](adr/0005-mass-policy.md).

| Parameter | PoC value | Notes |
|---|---|---|
| `mass_per_tx_byte` | `1` | unchanged |
| `mass_per_script_pub_key_byte` | `10` | unchanged |
| `mass_per_sig_op` | TBD (Phase 6) | scale from upstream `1000` by measured ML-DSA verify cost × safety factor ≥ 1.5 |
| `max_block_mass` | `500_000` | unchanged, may be tightened in Phase 6 |
| `max_signature_script_len` | `10_000` | unchanged, fits one ML-DSA P2PKH input |
| `max_script_public_key_len` | `10_000` | unchanged |
| `MAX_SCRIPT_ELEMENT_SIZE` | `4096` | widened from `520` to accommodate 3310-byte signature item |

## 7. SigCache shape

ML-DSA-65 public keys and signatures are far too large to keep verbatim
in a hot signature-verification cache. The cache key shape is:

```
struct Mldsa65SigCacheKey {
    sig_alg: SigAlg,           // tag = ML-DSA-65
    pubkey_hash: [u8; 32],     // BLAKE2b-256 of public key bytes
    signature_hash: [u8; 32],  // BLAKE2b-256 of signature bytes
    message_hash: [u8; 32],    // sighash digest
}
```

The signature-verification cache must not hold full public keys or
signatures by value. This is both a memory-DoS mitigation and an
allocation policy decision.

## 8. Wallet key derivation

- BIP39 mnemonic → 64-byte master seed: reused unchanged.
- BIP32-style hierarchical derivation (secp256k1): **not used**.
- Per-account / per-index seed:

```
keygen_seed =
    XOF(
        "kaspa-pq-wallet-v1/mldsa65/keygen" ||
        network_id ||
        account ||
        change ||
        index ||
        master_seed
    )[0..32]
keypair = MLDSA65.KeyGen(keygen_seed)
```

The PoC PRF is BLAKE3 XOF; the spec admits BLAKE2b-512 as an
alternative. Once chosen, the choice is normative.

## 9. Compatibility and migration

There is **no** migration path between mainline Kaspa and kaspa-pq.
This is by design: the address format, accumulator, and signature scheme
are all different. A separate one-shot migration tool is out of scope
for the PoC.

## 10. Test plan summary

Full test plan lives in §7 of the project plan; this spec carries the
mandatory acceptance criteria for each phase:

- **Phase 2** simnet launches with kaspa-pq genesis; existing Kaspa
  mainnet/testnet nodes are rejected at handshake.
- **Phase 3** add-then-remove on LtHash returns the empty-state
  commitment; serialized state is exactly 2048 bytes; invalid-block
  rollback leaves the accumulator consistent with a slow recompute.
- **Phase 4** a well-formed ML-DSA-65 P2PKH spend is accepted; any
  length/context/hash mismatch is rejected before `verify` is called.
- **Phase 5** wallet round-trip on simnet: create, receive coinbase,
  spend.
- **Phase 6** `mass_per_sig_op` set from measured median verify cost
  × safety factor ≥ 1.5; mempool survives a malformed-signature flood.

## 11. ADR index

- [ADR-0001 — Network isolation](adr/0001-network-isolation.md)
- [ADR-0002 — ML-DSA-65 P2PKH as the only standard script](adr/0002-mldsa65-p2pkh.md)
- [ADR-0003 — LtHash16_1024 UTXO accumulator](adr/0003-lthash-utxo-accumulator.md)
- [ADR-0004 — 64-byte UTXO commitment](adr/0004-utxo-commitment64.md)
- [ADR-0005 — Mass / DoS policy](adr/0005-mass-policy.md)
- [ADR-0006 — RPC / WASM / SDK types](adr/0006-rpc-wasm-sdk-types.md) (Phase 7 scope freeze)
