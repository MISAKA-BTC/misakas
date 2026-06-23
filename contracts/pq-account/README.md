# MISAKA PQ-Rooted EVM Smart Account (PREA P0-2)

`MisakaPqSmartAccount` — an EVM account whose **unrestricted authority is a
post-quantum ML-DSA-87 key**, not secp256k1. A root operation is authorized by an
ML-DSA-87 signature verified **on-chain** by the MISAKA **F003 `MLDSA87_VERIFY`
precompile** (`0x…F003`, version `0x02`). See `docs/misaka-prea-design-v1.1.md`
§13 for the full design.

This is the **P0-2 MVP**: only the root path (`executeRoot`). Deferred to later
slices: operational-root rotation, the offline Vault Owner, freeze/recovery, the
restricted secp256k1 session path, ERC-1271, the deterministic Factory, and the
relayed EntryPoint (design §7 / §12 / §13.5 / §14 / §15 / §16).

## How authorization works (option B — full PQ, no BLAKE2b-in-EVM)

`executeRoot` packs a **canonical op preimage** (`OP_DOMAIN ‖ chainId ‖ account ‖
version ‖ nonce ‖ validAfter ‖ validUntil ‖ target ‖ value ‖ callData`), builds
the F003 v0x02 input (`0x02 ‖ rootPayload(64) ‖ pubkey(2592) ‖ sig(4627) ‖
preimage`), and `staticcall`s `0x…F003`. **F003 itself** binds the public key to
the stored 64-byte address payload (`blake2b_512(address_ctx, pubkey) ==
rootPayload`), computes `message_hash64 = keyed_blake2b_512(op_ctx, preimage)`,
and verifies the ML-DSA-87 signature over it. So the on-chain account does **not**
need keyed-BLAKE2b-512 in Solidity — it just passes the exact operation bytes, and
the signature is bound to those bytes with full post-quantum strength.

The off-chain signer MUST reproduce `_opPreimage(...)` byte-for-byte (fixed widths:
chainId 32B, account 20B, the `uint64`s 8B, value 32B), then sign
`keyed_blake2b_512("misaka-pq-evm-v1/op/mldsa87", preimage)` under
`"misaka-pq-evm-v1/root/mldsa87"`.

## ⚠️ F003 is consensus-FENCED INERT today

`evm_f003_mldsa_verify_activation_daa_score = u64::MAX` on every MISAKA network, so
a call to `0x…F003` returns empty data and `executeRoot` **reverts**
(`"PQ: ml-dsa root auth failed"`). This account becomes operable only once F003 is
activated by governance (a coordinated deploy with frozen gas/caps). The contract
+ tests exist now so the consumer is ready and reviewed.

## Build & test

> NOTE: this repository snapshot was authored without a local Foundry toolchain,
> so the Solidity here is **reviewed but not compiled in-tree**. Run:

```bash
./build.sh          # installs forge-std v1.9.4, builds (solc 0.8.28), runs tests
# or
forge test -vvv
```

`test/MisakaPqSmartAccount.t.sol` exercises the `executeRoot` LOGIC with F003
**mocked** (happy path + value forward + nonce bump, replay, wrong nonce, validity
window, ML-DSA-false, inert-F003, target-revert). Foundry cannot run the lattice
precompile, so the **real F003 verify + a real ML-DSA-87 signature** are proven by
the Rust end-to-end test against an F003-activated harness (P0-2 next step).

## Security notes

- Replay + intra-call reentrancy are guarded by the strictly-increasing
  `rootNonce` (a reentrant call would need a signature over `nonce+1`); effects
  (nonce bump) precede the external interaction.
- Cross-chain / cross-account replay is prevented by binding `chainId` +
  `address(this)` + `accountVersion` into the op preimage.
- Ownership is ML-DSA-87 (post-quantum). The account accepts no secp256k1/ECDSA
  authority; on a PQ-active network the consensus rule (PREA I-6) additionally
  class-2-skips a direct ECDSA tx whose sender is a registered PQ account.
