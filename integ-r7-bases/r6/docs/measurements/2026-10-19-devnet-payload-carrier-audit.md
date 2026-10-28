# devnet payload log: import and reaggregation audit — 2026-10-20

This is a reaggregation of historical observations, not a new node run or saturation experiment. Source: `release/t12-16000`, commit `799a058fa97d5a1c270b98b4bdbfed731dd84fb3`. The [source report](2026-10-19-devnet-payload-carrier.md) and [1,180-line raw log](2026-10-19-devnet-payload-carrier.jsonl) are imported verbatim. [Import hashes](2026-10-19-devnet-payload-carrier-import.json) prove byte equality to those Git blobs. The source report describes a 72ad80463 build and sender 67bc4d4ca; it is not a measurement of the current G14 integration candidate.

The operator reported that the previous agent stopped before writing the summary; the report has now been pushed, and the four devnet nodes and temporary directories/keys have been removed. This is an operator report, not an independent process/cleanup audit. No replacement devnet or sender is started by this work.

## Directly reproduced observations

[Derived JSON](2026-10-19-devnet-payload-carrier-reaggregation.json) records event counts, pairing, block identifiers, stage observations and limits. There are 1,131 tick rows, 35 block-observation rows, seven first-mempool observations and seven included records. Every included txid pairs with its first mempool observation and an observed carrying header; all seven observed mempool txids are included. The logged header-DAA differences are 0 or 1.

| Stage | First mempool observations / included records | Header-DAA differences | Recorded latency seconds | Maximum measured-filter pending count |
| --- | --- | --- | --- | --- |
| duty005 | 1 / 1 | 0 | 8.1 | 1 |
| duty020 | 2 / 2 | 0, 1 | 12.1, 13.1 | 2 |
| sat100 | 4 / 4 | 0, 0, 0, 0 | 3.0, 2.0, 1.0, 2.0 | 2 |

The nominal 17,408-byte payload/ML-DSA setting comes from the source report; the JSONL contains neither raw transactions nor per-tx mass/serialized bytes to independently recheck each payload or signature. With seven observed txs and discarded sender output, these counts are not proof of the intended offered load, the number of rejected attempts, or a sender submission timestamp distribution. Intended saturation was not reached. Funding-confirmation waits and sender restarts remain hypotheses for the shortfall, not established causes.

## Corrections and definitions for downstream use

- **35 observations, 33 distinct recorded block identifiers.** Two stage-boundary identifiers recur: `e42a4e602a9197ec` and `97ca1911b3580959`. Their block fields agree. Identifiers in this log are 16-hex prefixes; full block hashes were not fetched here. The median total tx bytes is 49,752 using either the 35 rows or the 33 recorded IDs.
- **111,927 B is not isolated floor-tx bytes.** Both maximum rows (`a8206aa1a44f36e5`, header DAA 17; `f6fe4960fe062cc7`, header DAA 30) have `txs=6` and `meas_txs=1`. They include a measured transaction. `kind=LEGACY_FLOOR` labels the block lane, not the class of each transaction. The largest `tx_bytes` observation with `meas_txs=0` is 87,095 B. Even that is a whole-block tx sum, not a per-floor-tx measurement. Without per-tx serialized bytes, the floor-only maximum cannot be isolated. The source report's “floor tx alone 111,927 B” statement is therefore not established by the supplied log and is not used to calibrate reservations.
- **Header DAA, observation DAA and native accepting clock differ.** `submit_daa` equals the first-mempool event's DAA for every tx. Matching carrying rows have `sample_daa = include_daa + 1`; the recorded observation clock is 1 or 2 DAA after the first mempool event, while the reported header difference is 0 or 1. Neither is a measured native selected-chain court-acceptance timestamp. Fair inclusion/service assumptions cannot be inferred from the header difference.
- **DAA-second summaries require an interval definition.** Taking the first tick of each new observed DAA across the concatenated log yields 32 unit-DAA intervals, median 26.670169 s and maximum 124.245595 s. The source report's 31 intervals / median 28 s is not reproduced by this definition; a different interval selection may explain it. Per-stage first-to-last tick spans are 359.555243, 299.818656 and 479.644725 s; these are not controller process-start/stop durations. No conversion to a guaranteed service clock is made.
- **A byte ceiling is not reserved court supply.** The constants comment explains the usual 500,000 mass and transient byte factor 4 as a 125,000-byte body ceiling. It does not establish a 125,000 B/DAA court service lower bound, nor a subtraction-based safe residual after ordinary/floor traffic. Payload overhead, mass, contention, accepted-chain order, reorg, signer cost and actual reserved supply remain separate.

## Reproduce and scope

```sh
python3 scripts/measurements/audit_devnet_payload_carrier.py \
  docs/measurements/2026-10-19-devnet-payload-carrier.jsonl \
  --output /tmp/devnet-payload-reaggregation.json
```

The script pins the input SHA-256, checks txid uniqueness, mempool/included/header pairing, DAA latency arithmetic and repeated block-row agreement. Two final executions produced byte-identical derived JSON. This verifies reaggregation and observed-record consistency; it does not validate the original recording script, complete chain history, sender success, saturation, mass accounting, p99 or an adopted capacity profile.

The next load run needs multiple independently spendable funded UTXOs, continuous sender execution with retained stdout/stderr, per-tx bytes/mass and accepted-chain timestamps. Preparation must diagnose actual funding/maturity and offered-load delivery before interpreting any target-rate label as load achieved. This is a future test requirement, not a command here to start or deploy nodes. Real fences stay dormant; no cost, bandwidth, deadline or bond parameter is adopted from this small quiet-network sample.
