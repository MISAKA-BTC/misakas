# DNS-finality validator runbook

Verified against current `main` / Testnet-11 Relaunch 5f on 2026-09-13. The precommit section describes the build that schedules Testnet-11's DAA 6,701 flag day (ADR-0128).

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
- from the BFT gate's height, a `precommit: LOCKED epoch …` line for the epochs the duty names
- anti-equivocation state (both logs) writable
- DNS-confirmed anchor advances

Testnet-11 has a 120-second PALW block cadence and a DNS attestation epoch of 2 blue-score. The sidecar polls every 3 seconds by default; old testnet-21 heartbeat advice does not apply.

## Anti-equivocation state

`signed-epoch-db` is safety-critical, and so is the precommit log beside it (`<signed-epoch-db>.precommits.json`).

- back both up with the seed
- use a separate file for each network
- never run the same seed concurrently on two hosts
- do not replace lost state with an empty database and immediately resume

## Precommits (round two, ADR-0128)

From the height a network schedules ADR-0128's BFT gate (Testnet-11: **DAA 6,701**), an epoch's anchor is DNS-final only when validators holding more than two thirds of the counted bonded stake have both **attested** to it and **precommitted** to it, and the DNS stake reorg gate then refuses any chain that abandons that anchor until it goes stale. Voting power is the bond amount.

- `kaspa-pq-validator run` precommits by itself on such a network, after each poll's attestations; the in-node validator does the same. Neither needs a flag. Both read what to sign from the node (`getPrecommitDuty`), so the node must be a build that has it; against an older node the sidecar logs a warning and asks again every 10 minutes.
- Each precommit declares the lock the chain shows for your bond and is signed only after it is written to the **precommit safety log**, `<signed-epoch-db>.precommits.json` beside the attestation log (`validator.state` → `validator.precommits.json`). The in-node validator uses the same name next to its state file, so moving a validator between the two keeps it.
- A `refusing to sign` line means signing would contradict a precommit this key already released (a reorg moved the anchor or the lock). That is the log protecting the bond: do not delete the log to silence it.
- A bond silent for 5,040 DAA (about seven days at 120 seconds per block) stops counting in the denominator; it counts again once an attestation of it is buried 200 DAA. No leak takes the counted set below four validators.
- `--dry-run` reads and checks the duty and signs nothing.

## Unbonding

Stopping the process does not unbond. Unbonding has a waiting/evidence period during which protocol obligations can remain relevant. Inspect current `kaspa-pq-validator unbond --help` before submitting the transaction.

## Relationship to PALW

Validators attest and precommit for the DNS stake reorg gate and are paid 20 % of the block subsidy from the height the network schedules (ADR-0126, revised; Testnet-11: DAA 6,701, previously 30 %). PALW block production and settlement do not depend on validators: a PALW payment's confirmations are settled PALW anchors (`misaka palw settlement`, ADR-0127/0129), and the gate is a veto layered on top.

Validator downtime affects DNS confirmation and its consumers such as the EVM bridge. It is not the same service as `misaka mining` or `misaka verifier`, and its Bond outpoint cannot substitute for a PALW producer Bond.
