# PR-9.5 — Hash → Hash64 Consensus Identity Cascade — Migration Inventory

Planning artifact for the multi-session [ADR-0008](adr/0008-hash64-consensus-identity.md)
identity cascade. See [PR-9.1](adr/0008-hash64-consensus-identity.md)
for the design rationale and §"Public-claim discipline (binding)" for
the security framing (`512-bit commitment domain`, `256-bit quantum
preimage margin`, **not** `256-bit quantum collision`).

Status: PR-9.5a landed (this doc + primitive consolidation); PR-9.5b
through PR-9.5g pending across follow-on sessions.

## Why PR-8.4 is folded into PR-9.5d (not its own PR)

`Header.pow_algo_id: u8` is part of `Header`'s serialised layout, so it
contributes to:
- Header hashing (the byte ordering documented below);
- Header serialisation (Borsh + proto + RPC JSON);
- Genesis block hash (the genesis header is the first hash this
  byte ordering applies to);
- All call sites that construct a `Header` positionally (36 files —
  see §"Header construction sites" below).

Hash64 cascade *also* changes Header layout (the 5 hash fields widen),
all parent references, the pruning point, and the genesis. Doing
PR-8.4 as a standalone first would force a **double** genesis
recompute and break review continuity in every Header construction
site twice. The conservative answer is to land both changes in
PR-9.5d, recompute genesis exactly once in PR-9.5g.

## PR-9.5 sub-PR plan

| Sub-PR | Title | Scope | Status |
|---|---|---|---|
| 9.5a | Hash32 / Hash64 primitive + this inventory | crypto/hashes/src/{lib.rs, hash64.rs, hashers.rs} confirmed; no breaking changes | ✅ landed |
| 9.5b | Semantic aliases in consensus/core | `BlockHash` / `TransactionId` / `TransactionHash` / `MerkleHash` / `MerkleRoot` / `AcceptedIdMerkleRoot` / `UtxoCommitment` / `LegacyHash32` aliases, initially pointing at `Hash32` (no width change yet) | next |
| 9.5c | Transaction / Outpoint / Merkle aliases → Hash64 | txid / `TransactionOutpoint.transaction_id` / merkle leaf+branch+root widen to 64 B; mempool, utxo store, wallet storage cascade | deferred |
| 9.5d | Header → Hash64 + `pow_algo_id` (was PR-8.4) | 5 Header hash fields + `CompressedParents` + `pruning_point` widen; new `pow_algo_id: u8` field; hash-byte-ordering rule pinned; 36 construction sites repaired | deferred |
| 9.5e | GHOSTDAG / pruning / reachability / consensus stores → BlockHash64 | `consensus/src/processes/{ghostdag, pruning, pruning_proof, reachability, relations, headers_selected_tip, parents_builder}.rs` + `consensus/src/model/stores/*.rs` (23 files) keyed on `BlockHash64`; `BlockHashMap` / `BlockHashSet` doc comments + tests updated for Hash64's 8-u64 layout | deferred |
| 9.5f | P2P / RPC / database / wallet / SDK call sites → Hash64 | `protocol/p2p/proto/p2p.proto` + `rpc/grpc/core/proto/rpc.proto` hex-length validation 64→128; `wallet/core/src/storage/*` tx record / outpoint width; PSKT format; WASM bindings (`Uint8Array(64)`, 128-hex `fromHex`); DB schema version bump (`kaspa-pq-*` namespace; old Kaspa DB explicitly rejected at open) | deferred |
| 9.5g | Genesis re-compute + devnet boot + integration smoke | 5 genesis constants in `consensus/core/src/config/genesis.rs` (mainnet, testnet, testnet11, simnet, devnet) regenerated; ~22 fixture files (~100 hex literals) regen; devnet boot acceptance + 2-node IBD + RPC round-trip + tx round-trip | deferred (final) |

## Call-site inventory (workspace-wide ripgrep, 2026-05-28)

| Area | Files touched | Ref count | Most painful single file | Notes |
|---|---:|---:|---|---|
| `consensus/core` | 31 | 168 | `src/header.rs` (404 LoC, 5 hash fields, the central type) | `BlockHashMap` / `BlockHashSet` doc rewrite needed |
| `consensus/src/processes` | 24 | 126 | `ghostdag/protocol.rs`, `pruning_proof/{apply,build,mod,validate}.rs`, `reachability/tree.rs` | Pruning-proof format may need explicit version bump |
| `consensus/pow` | 4 | 12 | `matrix.rs`, `wasm.rs`, `xoshiro.rs`, `benches/bench.rs` | PR-9.3 already migrated `pow_layer0.rs`; these are residual cache-key / debug call sites |
| `protocol/p2p` + `protocol/flows` | 17 | 47 | `protocol/p2p/proto/p2p.proto` (305 LoC, 33 hash refs) | Proto stays `bytes` — width change is mechanical; converter files (`hash.rs`, `header.rs`, `tx.rs`, `messages.rs`) carry the brunt |
| `rpc` (`core` + `grpc` + `wrpc`) | 16 | 76 | `rpc/grpc/core/proto/rpc.proto` (1115 LoC, 50 hash refs); `rpc/core/src/model/header.rs` (339 LoC, 17 Header constructions) | RPC proto fields stay `string`; hex-length validation flips 64→128 chars (already covered for `RpcUtxoCommitment64` in PR-7.6) |
| `database` | 3 | ~5 | `database/src/access.rs` | Mostly abstract; schema-version field if present must bump |
| `wallet` (core + pq-cli + native + wasm + pskt) | 32 | 123 | `wallet/core/src/storage/transaction/{data,record,utxo}.rs`; `wallet/pskt/src/pskt.rs` | PSKT borsh format will widen; check if any wire-compat constraint |
| `wasm` / `cli` / SDK examples | 2 Rust + ~10 JS | ~5 | `cli/src/modules/history.rs` | JS examples may pin hex strings — regen with PR-9.5g |
| `mining` / `mempool` | 22 | 132 | `mining/src/manager.rs` (1113 LoC) | Mempool `BlockHashMap` keyed on `TransactionId`; fee/priority logic should be width-agnostic (verify) |
| `testing/integration` | 22 fixture files | ~100 hex literals | `consensus/core/src/sign.rs`, `hashing/{tx,sighash}.rs`, `mass/mod.rs`, `tx.rs`; `rpc/core/src/model/optional/tx_serde_tests.rs`; `wallet/pskt/src/{bundle,examples/multisig}.rs`; JS examples | All regenerated in PR-9.5g via the existing test-vector harness pattern |

**Header construction sites** (PR-9.5d blocker — adding `pow_algo_id`
breaks every positional constructor): **36 files**, top 5 by count:

| File | `Header::new_finalized` count |
|---|---:|
| `rpc/core/src/model/header.rs` | 17 |
| `rpc/core/src/model/optional/header.rs` | 10 |
| `consensus/client/src/header.rs` | 10 |
| `rpc/core/src/model/tests.rs` | 6 |
| `consensus/src/processes/parents_builder.rs` | 6 |

**`BlockHasher` / `BlockHashMap` compatibility**: 54 files use
`BlockHashMap` / `BlockHashSet`. `Hash64`'s `StdHash` impl already
writes 8 little-endian `u64` words ([crypto/hashes/src/hash64.rs:189-196](../crypto/hashes/src/hash64.rs)),
so the existing `BlockHasher` (which keeps the last `u64` per the
[`blockhash.rs:135-143`](../consensus/core/src/lib.rs) comment) is
**source-compatible** with the wider hash — the implicit "last 8 bytes
of a Hash" property becomes "last 8 bytes of a Hash64". Update the
doc comment when PR-9.5e lands; no code change.

**Genesis values**: 5 constants in [consensus/core/src/config/genesis.rs](../consensus/core/src/config/genesis.rs)
(`GENESIS`, `TESTNET_GENESIS`, `TESTNET11_GENESIS`, `SIMNET_GENESIS`,
`DEVNET_GENESIS`). Regeneration helper already lives in the same file
(test `gen_kaspa_pq_genesis_hashes` per PR-2 / Phase 2 pattern); add a
`--nocapture` run in PR-9.5g and paste the output into the constants.

## Header hashing byte order (binding for PR-9.5d)

The byte order fed into `BlockHash64::new()` is **frozen by PR-9.5d**
and a hard fork to change. The order is:

```text
version              (2 B, le)
parents_by_level     (write_len + write_var_array per level; BlockHash64 each)
hash_merkle_root     (64 B)
accepted_id_merkle_root (64 B)
utxo_commitment      (64 B)
timestamp            (8 B, le)
bits                 (4 B, le)
nonce                (8 B, le)
pow_algo_id          (1 B)         ← new in PR-9.5d
daa_score            (8 B, le)
blue_score           (8 B, le)
blue_work            (write_blue_work — Uint576 LE)
pruning_point        (64 B)
```

Notes:
- `pow_algo_id` is inserted **after** `nonce` and **before**
  `daa_score`. Inserting at this position keeps the
  `(timestamp, bits, nonce)` PoW triple contiguous (matching the
  existing miner expectations) and puts the algo discriminator
  immediately adjacent.
- `pow_algo_id` participates in the header hash on purpose — without
  it, the same header body could be interpreted under different L1
  algorithms with no on-chain disambiguation.
- `blue_work` uses `write_blue_work` which is width-invariant per the
  `kaspa_hashes::HasherExtensions` blanket impl (`BlueWorkType =
  Uint576` since PR-8.5).

## 10-step implementation order (per the user's plan)

For PR-9.5c onwards, follow this exact order to keep compile errors
local and reviewable:

```text
Step 0  inventory                                    (this document)
Step 1  Hash32/Hash64 primitive                       (PR-9.5a — landed)
Step 2  semantic aliases (still 32B)                  (PR-9.5b)
Step 3  raw `Hash` removed from consensus core types  (PR-9.5b tail)
Step 4  flip aliases to Hash64; cascade in:
          crypto/hashes → consensus/core/tx → core/merkle →
          core/header → core/block → consensus/src/model/stores →
          protocol → rpc → wallet → sdk/wasm                (PR-9.5c/d/e/f)
Step 5  add pow_algo_id to Header constructor          (PR-9.5d)
Step 6  add pow_algo_id to header hash input           (PR-9.5d)
Step 7  PoW validate_pow_algo_id: u8 (algo=1 only)     (PR-9.5d tail)
Step 8  DB schema version bump + old-DB-rejection      (PR-9.5f)
Step 9  RPC / P2P / SDK hex-length flip 64→128         (PR-9.5f)
Step 10 genesis regen + devnet boot acceptance         (PR-9.5g)
```

## Review gates (binding for PR-9.5g merge)

PR-9.5g does not merge until **all six** gates pass on the
`kaspa-pq-devnet` config:

| Gate | Command / acceptance |
|---|---|
| 1. Type safety | `cargo check --workspace --exclude muhash-fuzz`; `cargo test -p kaspa-hashes`; `cargo test -p kaspa-consensus-core --lib` |
| 2. Hash vectors | Hash64 hex round-trip, BlockHash64 / TransactionId64 / MerkleRoot64 / HeaderHash64 / GenesisHash64 vector pinned per network |
| 3. Genesis stability | `genesis.cached_hash == hashing::header::hash(&genesis.header)`; `genesis.merkle_root == recompute_merkle(&genesis.coinbase_only_tx_list)`; `genesis.pow_algo_id == POW_ALGO_KHEAVYHASH_V1` |
| 4. Devnet boot | `kaspa-pq-node --network kaspa-pq-devnet --reset-db` boots; genesis accepted; virtual-selected-parent moves at least one block; `getBlock(genesis_hash)` returns a 128-char hex string |
| 5. Two-node P2P / RPC smoke | Two nodes connect; headers + blocks sync; `getBlock`, `getBlockTemplate`, `submitBlock`, `getMempoolEntry` all work |
| 6. Wallet / tx smoke | Wallet creates a tx with a 128-hex `txid`; outpoint `transaction_id` is 64 B on the wire; mempool accepts; subsequent block accepts |

## Hard rules across the cascade

These are tripwires that the implementation in PR-9.5c through
PR-9.5g **must not** violate. Any future PR breaking one is a hard
fork and needs its own ADR.

1. **No `pub type Hash = Hash64;` global flip.** The cascade goes
   through semantic aliases (`BlockHash`, `TransactionId`,
   `MerkleRoot`, …) so each call site is reviewed under the name
   that describes its consensus meaning. A "rename `Hash` →
   `Hash64`" search-and-replace breaks every legacy 32-B seed
   (`Hash::from_u64_word`, RNG seeds, debug fingerprints) that
   should stay 32 B.

2. **`algo_id = 1` kHeavyHash seed stays 32 B.** PR-8.5 / PR-9.3
   already defined `l1_seed32 = BLAKE2b-256(SEED_KEY ||
   pre_pow_hash64)`. The L1 inner-loop seed remains 32 B for
   kHeavyHash compatibility; only the outer Header hash and the
   pre-PoW hash widen.

3. **Genesis is recomputed exactly once.** PR-9.5c/d/e/f do **not**
   touch genesis constants. PR-9.5g does it once, after every
   upstream Header-layout-changing PR has merged.

4. **RPC fields stay `string`.** The proto on-wire type does not
   change from `string`/`bytes`. Width-handling moves into
   validation (`64 → 128 hex chars`) and into the converter layer.
   Changing field types would cascade into every SDK consumer.

5. **No DB migration.** kaspa-pq is a new network ([ADR-0001](adr/0001-network-isolation.md));
   the new schema version starts fresh. Opening an old Kaspa DB
   rejects with a clear error message.

## Coordination with the DNS overlay (ADR-0009/0010/0011/0012/0013/0014/0015)

The Phase 10 DNS overlay design relies on a stable consensus
identity for the WorkScore side of its two-resource dominance rule
(ADR-0009 §"Public-claim discipline (binding)" reorg framing). The
overlay's stake-side data (`validator_set_commitment`,
`SortitionCommitPayload`, etc.) is **already Hash64-native** per
ADR-0010 §"Validator-set commitment derivation" / ADR-0012
§"Payload types" — those types ship in `consensus/core/src/dns_finality.rs`
already and do not need any of the PR-9.5 cascade.

The cascade's effect on the DNS overlay is one-way: once `BlockHash`
flips to Hash64 in PR-9.5d, every existing DNS overlay reference to
`StakeAttestation.target_hash: Hash64` (already correct) round-trips
against the now-actually-64-B block hash without further code change.

## References

- [ADR-0008 — Full Hash64 consensus identity](adr/0008-hash64-consensus-identity.md)
  (the design this cascade implements).
- [ADR-0007 — Layered PoW](adr/0007-layered-pow.md)
  (`pow_algo_id` is part of the Layer 0/Layer 1 split this ADR
  defined; PR-8.4 was its standalone placeholder, now folded into
  PR-9.5d).
- [crypto/hashes/src/hash64.rs](../crypto/hashes/src/hash64.rs)
  (the 64-byte type, with `StdHash` impl that writes 8 u64s — the
  property `BlockHasher` relies on).
- [crypto/hashes/src/hashers.rs:40-50](../crypto/hashes/src/hashers.rs)
  (the 9 keyed BLAKE2b-512 hashers that produce Hash64 digests).
- [docs/kaspa-pq-spec.md §10](kaspa-pq-spec.md) (Phase plan; this
  cascade lives under "Other deferred work (outside the Phase 10
  slot range)" in the spec's refined PR table).
