# testnet-10 → testnet-200 migration

As of 2026-07-29, `testnet-200` is the only publicly operated MISAKA testnet.
`testnet-10` remains parseable as a compatibility preset, but it has no public DNS seeds,
explorer, MTP accrual, or operator-supported bootstrap node.

## What moved

- CLI, miner, desktop setup, Web UI, probe, and MTP defaults now select `testnet-200`.
- The two verified authoritative MISAKA DNS seed names are attached exclusively to
  `STAGING_MAINNET_PALW_PARAMS`; stale/unreachable names are no longer shipped.
- Seed nodes listen on the testnet-200 default P2P port, `26511`.
- PALW is genesis-active, `palw_algo4_accept = true`, and the peer allowlist is open.
- DNS finality retains the production validator-count and bond floors while using
  rehearsal thresholds `WorkDepth = 100` and `StakeDepth = 5000`.
- MTP rules version 3 accepts new epochs for `testnet-200` only.

## Operator cutover

Build current `main`, then start:

```sh
kaspad --testnet --netsuffix=200 --utxoindex --rpclisten-borsh=default
```

DNS discovery is automatic. If it is unavailable:

```sh
kaspad --testnet --netsuffix=200 \
  --addpeer=95.111.236.186:26511 --utxoindex --rpclisten-borsh=default
```

Use `testnet-200` anywhere a tool asks for the full network id. Testnet RPC defaults remain
gRPC `26210`, wRPC Borsh `27210`, and wRPC JSON `28210`; only the P2P port is suffix-specific.

Do not point a testnet-200 binary at a testnet-10 app directory. They have different genesis and
consensus identities. Use a fresh app directory and a fresh validator anti-equivocation state file.

## What does not migrate

This is a network cutover, not a ledger-state import. Testnet-10 blocks, UTXOs, bonds, validator
epochs, and balances do not become testnet-200 state. ML-DSA-87 seed material can derive the same
testnet address prefix, but funds and registrations must exist independently on testnet-200.

Historical testnet-10 MTP ledgers remain verifiable under their original signed rules. They are not
rewritten, and new testnet-10 facts fail the rules-v3 network-scope check.

## Verification

```sh
misaka bootstrap seeds --network testnet-200
misaka bootstrap resolve --network testnet-200
misaka node doctor --network testnet-200
```

At least one returned P2P endpoint must accept TCP on `26511`; the operator anchor is
`95.111.236.186:26511`.
The complete receipt-v3/algo-4 proof is in
[`artifacts/testnet-200-real-qwen-20260729`](../artifacts/testnet-200-real-qwen-20260729/README.md).
