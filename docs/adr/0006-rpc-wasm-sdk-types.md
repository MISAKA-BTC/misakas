# ADR-0006: RPC / WASM / SDK types for kaspa-pq

Status: Accepted (Phase 7 scope freeze)
Date: 2026-05-28
Supersedes: —
Depends on: [ADR-0002](0002-mldsa65-p2pkh.md), [ADR-0003](0003-lthash-utxo-accumulator.md), [ADR-0004](0004-utxo-commitment64.md)

## Context

Phases 1–6 changed the kaspa-pq consensus layer in three ways that the
RPC / WASM / SDK surface has to follow:

1. **Address payload version** — a new `Version::PubKeyHashMlDsa65 = 2`
   replaces the legacy `PubKey` / `PubKeyECDSA` variants as the only
   standard send template.
2. **Signature primitive** — ML-DSA-65 (FIPS 204) byte-blob fields
   (1952-byte public key, 3309-byte signature) replace 32 / 64-byte
   secp256k1 fields on the input side.
3. **UTXO commitment** — `UtxoCommitment64` is the production header
   field (PoC PR shipped 32 bytes for diff control; ADR-0004 commits
   to widening this to 64 bytes for the production launch).

The on-disk consensus encoding for these is now finalized
(crate `kaspa-muhash`, `kaspa-txscript`, `kaspa-addresses`). The RPC
side still encodes everything as if upstream Kaspa fields apply, which
is wrong at the byte-length level: an RPC client that asks the node for
a transaction's input scripts will receive ML-DSA-65-sized pushes but
the proto field is still typed as `bytes` (untagged), so language
bindings emit no compile-time signal that this is a new wire format.

Phase 7's job is to make that signal explicit at the RPC, WASM, and TS
layers, without re-litigating the on-chain decisions of Phases 1–6.

## Decision

### 1. Scope

In scope for Phase 7:

- **gRPC proto** updates (new field options / variants where the
  meaning changed, new top-level fields where new info is exposed).
- **wRPC** model + Borsh/JSON encoding additions for the same.
- **WASM** bindings: kaspa-pq address class, ML-DSA-65 type wrappers,
  `UtxoCommitment64` type.
- **TypeScript** type declarations matching the WASM bindings.
- **kaspa-rpc-core** model types.
- One example app per language flavour (`rust`, `wasm/js`,
  `wasm/python` if applicable) demonstrating: build → sign with
  ML-DSA-65 → submit.

Out of scope for Phase 7 (deferred):

- gRPC service backward compatibility with upstream Kaspa nodes —
  kaspa-pq is a separate network (see ADR-0001), so the gRPC service
  name and version are explicitly different.
- Header-version bump to expose `UtxoCommitment64` as a 64-byte field
  on the RPC. Phase 7 starts by RPC-exposing the 32-byte PoC width;
  ADR-0004 already commits to widening to 64 bytes, and that switch
  will be a single point-update to the proto + wRPC + WASM in a
  follow-up.
- A `kaspa-pq-wallet` GUI integration. The minimal CLI in
  `wallet/pq-cli/` (Phase 5') is the supported entry point.

### 2. gRPC proto changes

The kaspa-pq gRPC service is **renamed** to make the chain identity
explicit on the wire:

```proto
package protowire.kaspapq;

service KaspaPqRpcService {
    ...
}
```

(upstream Kaspa uses `package protowire;` `service RPC`.) Renaming
guarantees no mixed-chain client will accidentally connect.

For each existing RPC message that carried a script blob, version, or
hash, the kaspa-pq proto adds **per-field comments** documenting the
new lengths and a new `kaspa_pq` extension subfield where the
semantics differ:

| Upstream proto field | kaspa-pq treatment |
|---|---|
| `bytes script_public_key` | Encoded as **kaspa-pq scriptPubKey**. For the standard send template this is a 37-byte ML-DSA-65 P2PKH template (see ADR-0002 §"scriptPubKey (output)"). |
| `bytes signature_script` | Encoded as kaspa-pq signatureScript. For a standard spend this is `push(sig\|\|sighash_type)` followed by `push(public_key)` — approximately 5.3 KB. |
| `bytes utxo_commitment` | 32 bytes in Phase 7 (matches PoC final commitment), to be widened to a separate `bytes utxo_commitment_64` field when ADR-0004's production switch flips. |
| `string address` (RPC display) | Carries the `kaspapq*` prefix family. RPC servers must not emit `kaspa*`-prefixed strings. |

New top-level enum (Phase 7 only — not in upstream proto):

```proto
enum AddressVersion {
    PUB_KEY                  = 0;  // legacy, parse-only in kaspa-pq
    PUB_KEY_ECDSA            = 1;  // legacy, parse-only in kaspa-pq
    PUB_KEY_HASH_ML_DSA_65   = 2;  // kaspa-pq standard
    SCRIPT_HASH              = 8;
}
```

`reserved` blocks around the deleted upstream message-type numbers
where the kaspa-pq proto removes legacy messages, so future field
additions can't accidentally re-use a tag.

### 3. wRPC (Borsh + JSON)

`kaspa-rpc-core` adds three new types:

```rust
/// 1952-byte ML-DSA-65 public key. Borsh: fixed-size byte array.
/// JSON: hex string of length 3904.
pub struct RpcMlDsa65PublicKey(pub [u8; 1952]);

/// 3309-byte ML-DSA-65 signature.
pub struct RpcMlDsa65Signature(pub [u8; 3309]);

/// 32-byte kaspa-pq PoC UTXO commitment. Will be replaced by
/// RpcUtxoCommitment64 when the production switch happens.
pub struct RpcUtxoCommitment(pub [u8; 32]);
```

For Borsh, all three derive `BorshSerialize` + `BorshDeserialize`
natively (Borsh supports primitive byte arrays of any length).

For JSON (wRPC), each type implements `Serialize` / `Deserialize` as
hex-encoded strings with a fixed length validated at deserialize time.
The serde encoding **must** be hex strings — not `serde_bytes` — so
that JSON clients can read them with no special framing.

### 4. WASM bindings

`crypto/addresses/src/wasm.rs` Phase 2 TODO is resolved: the
`#[ignore]`-marked tests get re-enabled with regenerated `kaspapq:`
test vectors. The `prefix` JS-side field accepts the kaspa-pq prefix
strings (`"kaspapq"`, `"kaspapqtest"`, `"kaspapqsim"`, `"kaspapqdev"`).

A new module `wasm/src/kaspa_pq.rs` exposes:

```rust
#[wasm_bindgen]
pub struct MlDsa65PublicKey(...);

#[wasm_bindgen]
pub struct MlDsa65Signature(...);

#[wasm_bindgen]
pub struct UtxoCommitment(...); // 32-byte PoC width
```

Each with `from_hex`, `to_hex`, `to_bytes`, and `from_bytes`
constructors / accessors. Length checks are enforced and surfaced as
`JsValue`-typed errors.

`KaspaPqKeyPair` is the WASM-side mirror of
`kaspa_wallet_keys::kaspa_pq::KaspaPqMlDsa65KeyPair`. Exposed
methods: `from_seed`, `from_mnemonic`, `address`,
`public_key_bytes`, `sign`.

### 5. TypeScript declarations

`wasm/types/kaspa-pq.d.ts` is auto-generated from the WASM bindings.
Notable additions:

```typescript
export type KaspaPqNetwork =
    | "mainnet"
    | "testnet-10"
    | "simnet"
    | "devnet";

export type KaspaPqAddressPrefix =
    | "kaspapq"
    | "kaspapqtest"
    | "kaspapqsim"
    | "kaspapqdev";

export class MlDsa65PublicKey {
    static fromHex(hex: string): MlDsa65PublicKey;
    toHex(): string;
    toBytes(): Uint8Array;   // length 1952
}

export class MlDsa65Signature {
    static fromHex(hex: string): MlDsa65Signature;
    toHex(): string;
    toBytes(): Uint8Array;   // length 3309
}

export class KaspaPqKeyPair {
    static fromMnemonic(
        phrase: string,
        passphrase: string,
        network: KaspaPqNetwork,
        account: number,
        change: number,
        index: number,
    ): KaspaPqKeyPair;
    address(): Address;
    publicKeyBytes(): Uint8Array; // length 1952
    sign(message: Uint8Array, randomness: Uint8Array): Uint8Array; // length 3309
}
```

The on-chain RPC types (`RpcMlDsa65PublicKey`, etc.) are emitted with
`Uint8Array` typing plus a comment naming the required length.

### 6. SDK examples

One example per supported language:

- `rpc/wrpc/examples/kaspa_pq_send.rs` — sign + submit a tx on simnet.
- `rpc/wrpc/examples/vcc_v2_kaspa_pq.ts` — same as above from
  TypeScript (assumes a node available at the kaspa-pq default ports
  per ADR-0001).
- `wasm/examples/kaspa-pq-address-derivation.html` — minimal HTML +
  WASM demo of mnemonic → kaspa-pq address.

Each example carries a `// kaspa-pq Phase 7` comment at the top
pointing back at this ADR.

## Consequences

### Positive

- Cross-language clients can compile-time-check the kaspa-pq wire
  format. A TypeScript caller that tries to put a 64-byte Schnorr
  signature into an `MlDsa65Signature` slot fails at the type layer.
- The gRPC service-name change makes mixed-chain client errors loud
  rather than silent.
- The 32-byte → 64-byte UtxoCommitment switch becomes a single,
  trackable proto change in the future instead of a cross-cutting
  rewrite.

### Negative

- Phase 7 touches many files (rpc/core model, rpc/grpc proto, all
  three wRPC encoders, WASM bindings, TS declarations, examples).
  Realistic delivery is one focused PR per surface, gated on the
  preceding one merging.
- `protoc` and the wasm-bindgen toolchain become hard build deps for
  the kaspa-pq node release artefact. The upstream README already
  documents `protoc` for gRPC build; the wasm-bindgen path is new for
  some downstream packagers.

### Neutral

- Removing legacy `PubKey` / `PubKeyECDSA` from the kaspa-pq RPC
  serialization entirely is **out of scope** for Phase 7. They remain
  representable for parsers, but no RPC method emits them as standard
  output addresses.

## Implementation order (Phase 7 PR sequence)

1. **PR-7.1: rpc/core types.** Add `RpcMlDsa65PublicKey`,
   `RpcMlDsa65Signature`, `RpcUtxoCommitment`; thread them through the
   relevant model structs. Tests: roundtrip Borsh + JSON encoding.
2. **PR-7.2: gRPC proto rename + AddressVersion.** Rename service to
   `KaspaPqRpcService`, package to `protowire.kaspapq`, add the new
   `AddressVersion` enum, add `reserved` blocks. Tests: proto compile +
   one gRPC roundtrip.
3. **PR-7.3: wRPC encoders.** Plumb the new core types through Borsh
   and JSON wRPC paths; add hex (de)serialization with length
   validation.
4. **PR-7.4: WASM bindings + TS declarations.** New
   `wasm/src/kaspa_pq.rs`, regenerate `kaspa_pq.d.ts`. Re-enable the
   wasm-bindgen tests in `crypto/addresses/src/wasm.rs` with
   `kaspapq:`-prefixed vectors.
5. **PR-7.5: SDK examples.** Rust, TypeScript, HTML.
6. **PR-7.6 (separate, gated): UtxoCommitment 32 → 64 switch.** Bump
   header version, add `RpcUtxoCommitment64`. This is the production
   commitment cut per ADR-0004 §"Decision".

## References

- [ADR-0001 — Network isolation](0001-network-isolation.md)
- [ADR-0002 — ML-DSA-65 P2PKH](0002-mldsa65-p2pkh.md)
- [ADR-0004 — UtxoCommitment64](0004-utxo-commitment64.md)
- [kaspa-pq-spec.md](../kaspa-pq-spec.md) §12 (ADR index, kept in sync with this ADR.)
