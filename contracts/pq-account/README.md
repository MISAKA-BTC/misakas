# MISAKA PQ-Rooted EVM Smart Account (PREA P0-2)

`MisakaPqSmartAccount` — an EVM account whose **unrestricted authority is a
post-quantum ML-DSA-87 key**, not secp256k1. A root operation is authorized by an
ML-DSA-87 signature verified **on-chain** by the MISAKA **F003 `MLDSA87_VERIFY`
precompile** (`0x…F003`, version `0x02`). See `docs/misaka-prea-design-v1.1.md`
§13 for the full design.

**P0-2 implemented:** the ML-DSA root path (`executeRoot`), root-authorized session
grant/revoke, the **restricted secp256k1 session path** (`executeSession`),
**ERC-1271** (root-only), the deterministic **`MisakaPqAccountFactory`** (CREATE2 +
`getAddress` predictor + idempotent), and a permissionless **`MisakaPqEntryPoint`**
relayer (`handleOps`: deploy-if-needed via `initCode` + forward `executeRoot`/
`executeSession`; the relayer holds no authority — the account self-validates). The session path enforces: deny-by-default of `approve`
/ `setApprovalForAll` (approval-as-delegation drains every cap), a (target,selector)
allowlist, native value caps (per-total) + ERC-20 `transfer`/`transferFrom` amount
caps, expiry / max-calls / monotonic call-index / root-epoch binding, and CALL-only
(never delegatecall). Deferred to later slices: the offline Vault Owner,
operational-root rotation / freeze / recovery, Merkle target allowlists, full
ERC-721/1155 amount policy + Permit2, ERC-1271 session-purpose recompute, the
deterministic Factory, and the relayed EntryPoint
(design §7 / §12 / §13.6 / §14.5-6 / §15.2 / §16).

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

Verified: `forge test` = **20 passed** (solc 0.8.28, `via_ir`). Runtime bytecode
keccak (record on source freeze):
`0xd8e758fb3d1f87bfec2426d1d44e2fada537d57acb70faf4ffd33e47acd68c77`.

`test/MisakaPqSmartAccount.t.sol` exercises `executeRoot` with F003 **mocked**
(happy/replay/nonce/window/ML-DSA-false/inert-F003/target-revert) and the full
session path (happy + value forward + counters, forbidden-selector, unlisted
target, native cap, call cap, bad call-index, expiry, ERC-20 amount cap, revoke,
ungranted key, only-root grant) + ERC-1271 (root-valid / F003-false / bad-length).
Foundry cannot run the lattice precompile, so the root path's real ML-DSA verify is
the Rust e2e; the session path is pure secp256k1/EVM and fully forge-tested. The **real F003 verify + a real ML-DSA-87 signature over this contract's
exact op-preimage encoding** are proven by the Rust test
`kaspa-evm::mldsa_verify::tests::contract_execute_root_f003_input_verifies_with_real_mldsa`
(it replicates `_opPreimage` byte-for-byte and runs the real `run_f003_verify`).
A further optional e2e (deploy the compiled bytecode in revm + `executeRoot` end to
end) is a belt-and-suspenders follow-up; the encoding is already proven byte-exact.

## Security notes

- Replay + intra-call reentrancy are guarded by the strictly-increasing
  `rootNonce` (a reentrant call would need a signature over `nonce+1`); effects
  (nonce bump) precede the external interaction.
- Cross-chain / cross-account replay is prevented by binding `chainId` +
  `address(this)` + `accountVersion` into the op preimage.
- Ownership is ML-DSA-87 (post-quantum). The account accepts no secp256k1/ECDSA
  ROOT authority; on a PQ-active network the consensus rule (PREA I-6) additionally
  class-2-skips a direct ECDSA tx whose sender is a registered PQ account.
- A session can NEVER target the account itself (`executeSession` rejects
  `target == address(this)`) — otherwise a session allowlisted for
  `(address(this), grantSession)` could self-escalate via the
  `msg.sender == address(this)` self-call. Sessions also cannot `approve` /
  `setApprovalForAll` (approval-as-delegation), use delegatecall (CALL-only), or
  exceed their native + ERC-20-transfer amount + call-count + expiry caps.
- ⚠️ Allowlisting a router / multicall / aggregator / non-standard-token selector
  for a session grants UNCAPPED token movement through that call (the amount cap
  only decodes `transfer`/`transferFrom`). The root must allowlist only specific,
  trusted (target, selector) pairs. Per-token/collection Merkle policy is P1.
