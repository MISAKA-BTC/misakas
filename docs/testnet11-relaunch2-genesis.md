# testnet-11 Relaunch 2 — the public re-genesis and its community allocation

> **Historical (superseded 2026-08-30 by Relaunch 3, the 10B premine cap —
> [ADR-0059](adr/0059-the-10b-premine-cap.md)).** The community table itself lives on (11
> entrants / 547M as of the cap re-genesis), but it is now carved OUT of the single 10B main
> wallet; the 40-vault premine this document assumes is deleted, and every network's genesis
> mints exactly 10B.

- **Cut:** 2026-08-20
- **Why:** the PALW public testnet opens with the collected community allocation baked into
  genesis, so participants hold spendable MSK at block 0 rather than waiting on a faucet.
- **Scope:** `testnet-11` only. testnet-10 (running), devnet, simnet and mainnet are untouched —
  their genesis constants and consensus fingerprints are byte-identical to before this change,
  which the pinned-fingerprint test proves row by row.

## The new network identity

| | |
|---|---|
| genesis hash | `3564ea39d83e6f107eeab8142e36f83bccb71574178a210509c55444de31c3fc c5e6a3f26bd75965bb4930c1c2e50447c4a6cd86f5f6b912d396565d9e41e128` |
| `utxo_commitment` | `80fad3c3d9051a8f5c1d828bd4012b903972a0f15e0423f8ecbe658aaf6673f6 796821686 16bc3b809ff33e7156f0284d69fb94e2527618f1ec5a37824105385` |
| consensus fingerprint | `49ff962891445eeef0411f6499561e8f00640339ad3cdb0ac306ad07ccf299db` |
| relaunch marker | coinbase payload `11, 2` (Relaunch 1 was `11, 1`) |

The marker bump is what makes this chain cryptographically distinct from the Relaunch-1 soak: an
un-wiped soak node hits the startup genesis-mismatch guard instead of silently resuming on a
chain whose genesis it does not share. **Operators must wipe the t11 datadir before joining.**

`palw_consensus_mode` is `LegacyTn11` on this preset — the ADR-0042 mode enum's first live
network. It changes no rule (the legacy `pow_palw_activation` etc. still carry the values); it
makes the lineage a handshake fact, and it rode this flag day rather than needing its own.

## The genesis UTXO set

`config::premine::genesis_premine_utxos_for(testnet-11)` = the shared 13B premine (40 vaults ×
0.1B + 1 main × 9B, unchanged) **plus** 9 community UTXOs totalling **347,000,000 MSK**.

Community UTXOs sit on their own sentinel txid — ASCII `misaka-t11-community`, zero-padded to 64
bytes — at indices `0..9` in the table's fixed order. Each is a single-key ML-DSA-87 P2PKH,
`is_coinbase: false`, spendable from block 0 with no maturity delay.

| # | recipient | MSK | collected |
|---|---|---|---|
| 0 | operator | 100,000,000 | 2026-08-11 |
| 1 | tetsu31 | 5,000,000 | 2026-08-18 (address change) |
| 2 | Kurenai | 30,000,000 | 2026-08-11 |
| 3 | タケヤマ #1 | 100,000,000 | 2026-08-12 |
| 4 | タケヤマ #2 | 100,000,000 | 2026-08-12 |
| 5 | コタヌキM | 1,000,000 | 2026-08-12 |
| 6 | uki | 5,000,000 | 2026-08-19 (address change) |
| 7 | あかぼね | 5,000,000 | 2026-08-17 |
| 8 | kamil | 1,000,000 | 2026-08-17 |

The exact addresses live in `consensus/core/src/config/premine.rs`
(`TESTNET11_COMMUNITY_ALLOCATIONS`) as text, not opaque hashes, so the allocation is auditable
from source.

### Superseded addresses

Two participants changed address before the cut. The superseded ones are recorded in the table's
doc comment and allocated **nothing** — each participant appears exactly once:

- tetsu31, posted 2026-08-11 as `qfdqr02rx…` (no `misakatest:` prefix), replaced 2026-08-18.
- uki, posted 2026-08-13 as `misakatest:qfa2z97ys…`, replaced 2026-08-19 after installing the
  wallet.

## What guards this

- **Address validity.** `owner_payload` bech32-decodes every address at build; a transcription
  slip is a build failure, never a silently mis-locked allocation. The test additionally asserts
  every community address carries the `misakatest:` (Testnet) prefix — a mainnet-prefixed address
  in this table would be rejected rather than paid.
- **Amounts and count.** `t11_community_allocation_is_the_collected_list` pins 9 entries, the
  per-index amounts in table order, and the 347M total.
- **No collisions.** All 9 community owners are distinct from each other *and* from all 41
  premine owners.
- **Confinement.** The same test asserts t11 = 41 + 9 UTXOs while testnet-10, devnet, simnet and
  mainnet stay at 41 — the keying is by full `NetworkId`, so t10 and t11 sharing a `NetworkType`
  cannot share a UTXO set.
- **Round-trip.** `all_networks_genesis_constants_match_premine` now includes t11: the pinned
  `hash` / `utxo_commitment` must re-derive from the UTXO set at every node start (audit M-07),
  so a divergent genesis cannot be run.

## Re-deriving the constants

```bash
cargo test -p kaspa-consensus-core --lib config::premine::tests::print_premine_commitment -- --nocapture
```

prints `TESTNET11_UTXO_COMMITMENT`; paste it into `TESTNET11_GENESIS`, then

```bash
cargo test -p kaspa-consensus-core --lib config::genesis::tests::gen_kaspa_pq_genesis_hashes -- --nocapture
```

prints the resulting `hash_merkle_root` and `hash`. The fingerprint pins in `params.rs` fail with
the new value in the message, so the last step is mechanical.
