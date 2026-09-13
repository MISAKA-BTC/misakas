# DNS-finality validator runbook

Verified against current `main` / Testnet-11 Relaunch 5f on 2026-09-13.

This validator is separate from a PALW producer/panel Bond. It signs DNS-finality attestations through `kaspa-pq-validator`.

## Requirements

- synced Testnet-11 node
- `--utxoindex`
- wRPC Borsh enabled with `--rpclisten-borsh=default`
- validator seed stored on one host
- mature funds at the seed's funding address

Testnet-11's DNS stake minimum is **10 MSK = 1,000,000,000 sompi**. This is not the PALW class collateral amount.

## Recommended setup

```bash
misaka --network testnet-11 validator setup
```

The wizard checks the node, creates or loads the key, checks funds, creates/fetches the stake Bond and writes validator configuration. Re-running resumes.

## Direct sidecar flow

```bash
kaspa-pq-validator keygen --out validator.seed --network testnet

kaspa-pq-validator bond \
  --node-rpc 127.0.0.1:27210 \
  --validator-key validator.seed \
  --amount 1000000000 \
  --network testnet-11

kaspa-pq-validator run \
  --node-rpc 127.0.0.1:27210 \
  --validator-key validator.seed \
  --stake-bond <txid>:<index> \
  --signed-epoch-db validator.state \
  --network testnet-11 \
  --attest-poll-secs 3
```

Omit a manually guessed fee when the command supports automatic mass-based sizing.

## Status

```bash
misaka --network testnet-11 validator status
misaka --network testnet-11 validator bonds --all
```

Healthy operation means:

- node synced to the current fingerprint
- stake Bond active
- current canonical-ready epoch attested
- anti-equivocation state writable
- DNS-confirmed anchor advances

Testnet-11 has a 120-second PALW block cadence and a DNS attestation epoch of 2 blue-score. The sidecar polls every 3 seconds by default; old testnet-21 heartbeat advice does not apply.

## Anti-equivocation state

`signed-epoch-db` is safety-critical.

- back it up with the seed
- use a separate file for each network
- never run the same seed concurrently on two hosts
- do not replace lost state with an empty database and immediately resume

## Unbonding

Stopping the process does not unbond. Unbonding has a waiting/evidence period during which protocol obligations can remain relevant. Inspect current `kaspa-pq-validator unbond --help` before submitting the transaction.

## Relationship to PALW

Validator downtime affects DNS confirmation and consumers such as settlement/EVM bridge. It is not the same service as `misaka mining` or `misaka verifier`, and its Bond outpoint cannot substitute for a PALW producer Bond.
