# MISAKA EVM Wallet Profile v1 (`misaka-evm-hd-v1`)

**Status:** normative for wallets / SDKs / CLIs. **NOT a consensus rule.** Status 2026‑06‑19.

## Scope and rationale

The consensus layer is intentionally agnostic to how an EVM address is derived: the EVM
lane is an independent secp256k1 / ECDSA domain, and a deposit‑lock simply records a
destination `EvmAddress` ([u8; 20]) — see `docs/misaka-evm-design-v0.4.md` and
`consensus/core/src/evm/mod.rs` (`EvmAddress`, `EVM_CHAIN_ID = 0x4D534B`). Consensus
**MUST NOT** require that an address be derived from any particular mnemonic or path —
the destination can be an EOA, a contract, a smart‑account, a system predeploy, or a
precompile.

But "the wallet may choose freely" must **not** be read as "every wallet may pick a
different, undocumented scheme." Without a fixed profile, the same mnemonic restored in
a different client shows a zero balance. This document fixes the canonical scheme at the
**wallet level** so backups are portable and Ethereum tooling (MetaMask, Foundry, viem)
interoperates. This is a P0 item before mainnet; it is **not** a consensus change.

## Canonical HD derivation — `misaka-evm-hd-v1`

| Field | Value |
|---|---|
| Profile ID | `misaka-evm-hd-v1` |
| Mnemonic | BIP‑39, UTF‑8 **NFKD**, optional BIP‑39 passphrase |
| Seed | BIP‑39 (mnemonic + passphrase → 64‑byte seed) |
| Master/child | BIP‑32 over **secp256k1** |
| Default path | `m/44'/60'/0'/0/i` (account index `i`, starting 0) |
| First account | `m/44'/60'/0'/0/0` |
| Address | rightmost 20 bytes of `Keccak‑256(uncompressed secp256k1 pubkey, without the 0x04 prefix)` |
| Display | `0x`‑prefixed **EIP‑55** mixed‑case checksum |
| Tx signing | **EIP‑155** chain‑id binding, mandatory |
| MISAKA EVM chain id | `0x4D534B` (`EVM_CHAIN_ID`) |

Notes:
- **coin_type `60'`** (SLIP‑44 Ethereum) is adopted as an **EVM‑interoperability profile**,
  *not* a claim that MISAKA's SLIP‑44 identity is Ethereum. Using `60'` means the same
  mnemonic imported into MetaMask (default base path `m/44'/60'/0'/0`, incrementing the
  trailing index) derives the **same** addresses. A MISAKA‑specific coin_type would break
  that interop and is therefore *not* used for the EVM lane.
- **`network_id` is NOT mixed into the derivation path.** (The PQ/UTXO derivation domain‑separates
  by `network_id`; the EVM profile deliberately does **not** copy that.) testnet and mainnet
  therefore share the same EVM address; cross‑chain replay protection is handled by **EIP‑155**
  (the signed transaction binds `chain_id`). EIP‑155 does *not* prevent mistakenly depositing
  to the right address on the wrong network, so the UI must always show network name + chain id
  together with the recipient (see "Deposit safety").
- The verified reference wallet (chrome‑extension) already derives the EVM address via standard
  BIP44 `m/44'/60'/0'/0/0` (keccak(secp pubkey)[‑20:]); test mnemonic → Foundry account #0
  `0xf39Fd6…2266` matches. This profile documents that as normative.

## PQ ↔ EVM mnemonic separation (security)

The quantum‑resistance guarantee covers the **UTXO/ML‑DSA‑87 lane only**; the EVM lane carries
ordinary secp256k1 risk (same as Ethereum). The PQ key derivation also takes a BIP‑39 master
seed as input (`wallet/keys/src/kaspa_pq.rs`). Domain separation means one lane's key cannot be
trivially reversed from the other, but **operationally** entering a shared mnemonic into a
third‑party EVM wallet hands that software the master secret that *also* restores the PQ/UTXO
assets.

Therefore the **default** MUST be two separate mnemonics:

| Lane | Default mnemonic |
|---|---|
| UTXO / PQ (`misaka:` / ML‑DSA‑87) | dedicated mnemonic |
| EVM (`0x` / secp256k1) | **separate** dedicated mnemonic |

A `shared-root` mode (one BIP‑39 seed for both, EVM at `m/44'/60'/0'/0/i`) MAY be offered as an
explicit **opt‑in**, with this warning surfaced at enable time:

> Entering this mnemonic into a third‑party EVM wallet also discloses the master secret for your
> UTXO/PQ (ML‑DSA‑87) assets.

In `shared-root` mode, do **not** insert a custom XOF, network salt, or coin_type — any of those
break MetaMask address recovery for the same mnemonic.

## `EvmAddress` is not necessarily an HD EOA

A destination may be an EOA, a contract, a smart/multisig account, a system predeploy, a
precompile, or the zero address. Consensus accepts any 20 bytes — it MUST NOT require a derivation
proof. By design a deposit **credits a balance**; it is **not** a contract call, so a deposit to a
contract does **not** run `receive()`/`fallback()` (`docs/misaka-evm-design-v0.4.md`). Wallets
should resolve the destination kind over RPC and confirm; the CLI applies the static guards below.

## Deposit safety (P0/P1 — funds are unrecoverable after a claim)

A deposit‑lock, once **claimed**, consumes the lock UTXO and credits the recorded 20‑byte address;
there is no timeout‑refund after that. A one‑character address typo is therefore worse than an
ordinary mis‑send. Required at the CLI / wallet / JSON‑RPC boundaries (no consensus serialization
change to `EvmAddress`):

- **EIP‑55**: a mixed‑case `--evm-address` MUST pass EIP‑55 checksum or be rejected (typo guard).
  An all‑one‑case address has no checksum → accept but **warn**, and echo the checksummed form.
- **Zero address** `0x0000…0000`: reject by default.
- **System / precompile** addresses (MISAKA `0x…F001/F002/F003`; EVM precompiles `0x01..0x09`):
  strong warning.
- **deposit‑to‑self** as the default; a raw `--evm-address` is advanced mode.
- Always show recipient + **network name + chain id** + amount in the final confirmation; recommend
  a small test deposit first.
- Status as of this writing: the `kaspa-pq-validator deposit-lock` CLI now performs the EIP‑55 +
  zero/system‑address checks (this profile's enforcement at the CLI boundary). Wallet + JSON‑RPC
  boundaries SHOULD mirror them.

EIP‑1191 (chain‑aware checksum) exists but is not used: keep the **EIP‑55** representation for
Ethereum‑tooling compatibility and show network/chain id separately.

## Wallet persistence (derivation metadata)

A wallet DB MUST record, per account:

```
profile_id        # e.g. misaka-evm-hd-v1
seed_scope        # separate | shared-root
derivation_path   # e.g. m/44'/60'/0'/0/0
account_index
address           # EIP-55
origin            # hd-derived | imported-key
```

Accounts created from a raw private key or a legacy hard‑coded key MUST stay `imported-key`; do
**not** silently migrate them onto the canonical path.

## Test vectors (to fix as normative)

The current `kaspa-evm/examples/evm_tx_gen.rs` (`PrivateKeySigner::from_bytes([b; 32])`) is a
*signing* smoke test, not a *derivation* test. Add fixed normative vectors:

```
mnemonic, BIP-39 passphrase (non-empty), path index 0 and 1,
private key, compressed + uncompressed pubkey, EIP-55 address,
a signed EIP-1559 transaction's bytes, the transaction hash
```

## Verdict

- **Consensus:** unchanged and correct — no mnemonic/path belongs in consensus.
- **Wallet/SDK spec:** this profile fixes it (P0 before mainnet).
- **Deposit address validation:** EIP‑55 + zero/system guards close the mis‑send risk (P0/P1).
- **PQ security:** separate EVM mnemonic by default; `shared-root` is explicit opt‑in.
