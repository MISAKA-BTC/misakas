# PALW Stage-0 shadow drill runbook — carriage on a live chain, telemetry only

**Normative:** ADR-0029 §5 · **Binary:** `palw-shadow` (`misaka-palw-shadow`) · **Stage:** 0
**Nature:** consensus-inert. Every object rides the native subnetwork; no node change, no
offense evidence against anyone, no credit. The output is the §12 artifact set: observed check
rate, no-show, mismatch, attestation inclusion latency — measured from real chain events.

## 0. Rules before commands

1. **Drill identities only.** `palw-shadow keygen` makes them. Never point any mode at a
   production validator seed — Stage-0 validates nothing statefully, so namespace discipline is
   a rule, not a mechanism.
2. **Drill namespace only.** Every submitted job's envelope carries the drill `network_id`
   (default `misaka-palw-drill/v1`). A signed root mismatch on the production namespace is
   `ClassContradictionCertificateV1` material and must never be manufactured; the attester
   therefore never auto-files anything on mismatch — it logs and stops.
3. **Fees are real.** Each carriage tx spends a real fee from the drill wallet. Budget:
   commitment ≈ 7 KB, attestation ≈ 5 KB; at `--fee 100_000` sompi and the drill cadence below,
   a session of 20 jobs × (1 commitment + 2 attestations) ≈ 6 M sompi plus dust-free change.
4. **The node needs `--utxoindex`** (funding discovery) and a reachable borsh wRPC port.
5. Windows: `--params two-minute` against the 120 s PALW net; `deci` against a 10 s devnet.
   Reports label their parameter set; never compare across sets.

## 1. Build

```bash
cargo build --release -p misaka-palw-shadow -p misaka-palw-worker
```

The worker needs its usual environment on every host that executes or attests:
`MISAKA_PALW_GGUF` (pinned artifact) and `MISAKA_PALW_GOLDEN` (this class's golden set).

## 2. Identities and funding

Per participating host:

```bash
palw-shadow keygen --out ~/palw-drill/drill.seed --prefix testnet
```

Collect every printed `validator_id` into one roster file, shared by all hosts:

```json
{ "q": 2, "delta_bind": 10,
  "candidates": [
    {"validator_id": "<A>", "class": "<runtime_class_id hex>"},
    {"validator_id": "<B>", "class": "<runtime_class_id hex>"},
    {"validator_id": "<C>", "class": "<runtime_class_id hex>"},
    {"validator_id": "<D>", "class": "<runtime_class_id hex>"}
  ] }
```

`class` is the worker's own `runtime_class_id` (`palw-worker --mode v2-manifest`); `delta_bind`
matches the parameter set (10 on the 120 s net, 120 on deci). Fund each printed address with a
**non-coinbase** transfer (instant spendability; coinbase outputs sit behind maturity).
~10 M sompi (0.1 MSK) per host covers a long session.

**Funding source — confirmed 2026-08-16, read-only, key untouched:**

* The operator wallet is the t10 premine / miner-payout / 9B-validator funding address
  `misakatest:qtpflz03z576h02mtpn2vtwg5npj8fhlau3fgmsjl2a2uw0venj3573l07uahcs4gnsl8eqc7nlq5phakthxy606q2jyuxh2a08weduxa2yqlxuz`
  — measured via `misaka wallet utxo list --address …` over an SSH tunnel to B's RPC:
  **491,432 mature UTXOs, 9,001,069,939.21 MSK** (plus 82 immature). The whole drill budget
  (4 × 0.1 MSK) is ~4×10⁻⁹ of it.
* Its key is `/home/ubuntu/kpq-9b-validator.seed` on host A (0600, 32-byte hex — exactly the
  `--key-file` format `misaka` expects). **The key stays on A**; the CLI runs there and points
  at B's RPC, the same pattern the 2026-08-15 bond used (A's own RPC is degraded under load).
* Recommended one-hop isolation: fund a dedicated **drill treasury** (itself a
  `palw-shadow keygen` identity) once from the operator wallet, then fund the per-host drill
  identities from the treasury — repeated drill operations never touch the premine key again.

```bash
# on host A (key locality), operator-run; DRY-RUN first — broadcast only with --yes:
misaka wallet send \
  --to <drill-treasury-or-host-address> --amount 0.1 \
  --key-file /home/ubuntu/kpq-9b-validator.seed \
  --network testnet-10 --rpc 95.111.236.186:27610
# review the preview, then re-run with --yes
```

The `misaka` binary must be an x86-64 build on A (build on host B per the fleet recipe —
default features only; the heavy EVM deps sit behind `evm-send` and are not needed). Note the
wallet's fragmentation (491 k UTXOs) is harmless here: sends select from the giant non-coinbase
UTXO; the ~2.5 min address scan the bond flow paid applies to each send from this address —
one more reason the treasury hop is worth it.

## 3. The three loops

Watcher (every attesting host runs one; host B's is the reporting copy):

```bash
palw-shadow watch --state-dir ~/palw-drill/state --rpc 127.0.0.1:<borsh-port>
```

Attester (all hosts; the drill flags differ per host — §4):

```bash
palw-shadow attest --key ~/palw-drill/drill.seed --worker <palw-worker> \
  --state-dir ~/palw-drill/state --roster ~/palw-drill/roster.json \
  --rpc 127.0.0.1:<borsh-port> --prefix testnet
```

Submitter (one host, cron/loop — one job per interval):

```bash
palw-shadow submit-commitment --key ~/palw-drill/drill.seed --worker <palw-worker> \
  --name golden-probe-12tok-d16 --decode 512 \
  --rpc 127.0.0.1:<borsh-port> --prefix testnet
```

`--decode 512` runs at the credited ceiling; omit it for fast smoke jobs. Verify plumbing
before spending anything: `--offline-out /tmp/drill-check` builds, signs and stateless-validates
the whole commitment without touching a node.

## 4. Induced negatives — scheduled, bounded, labeled

The report's no-show and late columns must be exercised, not vacuously zero (ADR-0029 §5).
Per session:

* ONE host runs its attester with `--noshow-nth 3` — its third duty is skipped silently.
* ONE other host runs `--late-nth 5 --late-secs <past W_replay for the parameter set>` — its
  fifth duty answers late (on the 120 s net, `w_replay` = 30 blocks ⇒ > 3 600 s).
* Record which host carried which flag in the session notes; the report cannot (and should
  not) distinguish drill negatives from real ones — that is what the notes are for.
* Mismatch drills are NOT automated. If ever run, they are a manual, explicitly-labeled
  procedure on a dedicated `--network-id misaka-palw-drill/mismatch/vN` namespace.

## 5. Report

After `challenge_close` has passed for the session's jobs (24 h wall-clock at either
parameter set):

```bash
palw-shadow report --state-dir ~/palw-drill/state --roster ~/palw-drill/roster.json \
  --params two-minute > stage0-report.$(date +%Y%m%d).json
```

Jobs still inside their window appear under `jobs_pending`, never in the ledger columns.
Archive the report AND the raw `events.jsonl` — the log is the replayable artifact; two
watchers over the same chain must produce identical reports (diff them across hosts; a
divergence is a watcher bug or a node disagreement, both worth knowing).

## 6. Verified offline (2026-08-16, this increment)

* `keygen` → drill identity + funding address.
* `submit-commitment --offline-out`: golden envelope → drill-namespace patch → real legs
  execution → composite carriage (6 756 B) → ML-DSA-87 signature → stateless self-validation.
* `report` over a synthetic event log: 4 jobs → creditable 2 / on-time-match jobs 3 /
  refuted-in-window 1 / duties 12 = 9 on-time + 3 late + 2 no-shows across jobs, attestation
  latency p50 exact — every ledger column reached through the binary path.

## 7. Not in this increment

* **Opening call/answer drill flows** (`submit-call` / `answer-call`): the carriage kinds and
  the worker modes exist; the shadow binary gains the two thin modes in the next increment,
  which is when `W_answer` latency joins the report.
* Live-fleet execution of this runbook (an operator action: funding + systemd units + session
  notes). The binaries and the offline verification above are the deliverable here.
* Replay-cost rows in the ledger (`replay_durations_ms`) — fed from `v2-replay-bench` runs,
  merged at report time in a later increment; the fleet numbers already exist in
  `docs/palw-stage0-fleet-replay-bench-2026-08-16.md`.
