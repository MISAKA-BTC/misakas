# DA-real algo-4 mint — live runbook

Goal: drive the single-host devnet-111 harness to an **ACCEPTED** algo-4 PALW block
(certificate blob present → `CertPresent`), not just a PoW'd **candidate** block
that admission rejects with `CertAbsent`.

> **Status: DONE — executed live on 2026-07-25 (devnet-111-soak, batch `5152a9f4`).**
> `certificate_blob_present=true`; both DA obligations `Pending → Challenged →
> Satisfied`; batch `active` on both nodes; **zero** `CertAbsent`; algo-4 block
> `d8046f7c…` merged **BLUE** by chain block `ac893d54…`, confirmed independently
> on node A and node B via `find-reward-settlement`. Open caveat: the merging
> block's coinbase has **0 outputs**, so provider rewards were NOT observed paid —
> expected values were derived (A 79299440 / B 79299441 sompi) but the exact-SPK
> assertion has not been run. See "Known gotchas" for the five defects the live
> run exposed (all fixed in commit `a9f576d`).

## Why this is now possible

The old mock set each leaf's `receipt_da_root` to a random value and published no
DA object, so `register_leaf_obligations` created obligations that could never be
satisfied → `PalwDaStateV1::certificate_allowed` always failed (an empty/unsatisfied
obligation set is NOT success) → the certificate carrier was never UTXO-accepted →
the certificate **blob** was never written → algo-4 admission failed with
`CertAbsent`. Three landed changes close this:

1. **create-lifecycle** builds a REAL DA object per leaf (`palw-payload
   da-object-build`, author-time `batch_id=0`) and sets the leaf's
   `receipt_da_root` / `_object_len` / `_chunk_count` / `private_match_commitment`
   to the object's DERIVED commitment. Objects are saved at
   `<lifecycle>/da/<object_root>.palwobj`. State records `PALW_DA_REAL=1`.
2. **getPalwState wire v5** exposes `da_obligations` (id, provider_bond,
   object_root, chunk_index, status, …) so a challenger can find what to open.
3. **submit-lifecycle step 3.5** (`run_da_challenge_response`) drives every
   obligation `Pending → Challenged → Satisfied` BEFORE the certificate carrier,
   so `certificate_allowed` passes at the carrier's acceptance and the blob is
   written.

## Prerequisite: the new binary must be running on BOTH nodes

The DA step calls `getPalwState` and expects the **wire-v5** `da_obligation` lines.
A node built before these changes will not emit them. The running `devnet-111-soak`
nodes (ports 266xx) were started from an older binary — they MUST be restarted onto
`target/release/kaspad` built from this commit. The change is DB-compatible (RPC
wire + a read-only probe field only; no store schema change), so a restart resumes
the existing chain, providers, funding and beacon.

## One-command path (recommended)

The soak uses `PALW_ROLE=all` single-host config in `soak.env`
(`PALW_DATA_ROOT=$HOME/.palw-testnet/devnet-111-soak`). `run-all.sh` is idempotent:
it restarts the nodes onto the freshly-built binary, re-warms the beacon, keeps the
already-registered providers, then builds a FRESH DA-real batch and mints it.

```sh
cd scripts/palw-shared-testnet

# 0. stop the currently-running (old-binary) soak processes first
./stop.sh                     # or: pkill -f 'palw-testnet/devnet-111-soak/node[-]'; pkill -f 'misaminer .*devnet-111'

# 1. build the new binaries (kaspad + kaspa-pq-validator + misaminer + mock-ticket)
./build-and-hash.sh           # add HASH_FORCE=1 to re-record binary-hashes.txt after a rebuild

# 2. full idempotent bring-up + DA-real mint, single host, mock ticket
PALW_ENV_FILE="$PWD/soak.env" TICKET_MODE=mock LEAF_COUNT=1 \
  GATE_TIMEOUT_SECS=900 \
  PALW_MINE_ADDRESS="misakadev:<algo-4 coinbase payout addr>" \
  EXTRA_EXCLUDE_OUTPOINTS="<any retired/slashed bond outpoint>" \
  ./run-all.sh
```

The three extra env vars are NOT optional in practice — the live run needed all
of them (see "Known gotchas"):

* `GATE_TIMEOUT_SECS=900` — the batch goes `certified` immediately but only flips
  to `active` at `activation_not_before` (here `registration_epoch + 8` ≈ 800 DAA
  ≈ 13 min). The default 180 s gate times out mid-wait and looks like a failure.
* `PALW_MINE_ADDRESS` — `start-palw-miner.sh` fail-closes without it. Only the
  network prefix is checked, so any address you control works (the harness's own
  `SUPPORTING_ADDR` is convenient).
* `EXTRA_EXCLUDE_OUTPOINTS` — mandatory if ANY identity has a retired bond.

`LEAF_COUNT=1` keeps the DA game within the challenger rate limit: 1 leaf → 2
obligations (1 sample × 2 providers), and `auditor-c` may open ≤ 4 challenges per
epoch (`STRICT_TESTNET.max_challenges_per_bond_per_epoch = 4`). `LEAF_COUNT=2` (4
obligations) is the max for a single challenger in one epoch.

## Stage-by-stage (equivalent, for control / debugging)

```sh
export PALW_ENV_FILE="$PWD/soak.env" TICKET_MODE=mock LEAF_COUNT=1

./node-b.sh                    # restart node B onto the new binary
./restart-a-synced.sh          # restart node A into in-process validator/beacon mode
./supporting-miner.sh start    # the ONLY block producer (in-process validator emits txs, not blocks)
./dns-validator.sh             # (idempotent) confirm beacon healthy; step 6b warms it to sustained Healthy

# sanity: the new wire exposes obligations (empty until a batch registers, but the field must parse)
kaspa-pq-validator palw-status --node-wrpc-borsh 127.0.0.1:27610 --network devnet-111 --batch-id <id>
#   → look for `da_obligations.count:` and `da_obligation:` lines

./register-providers.sh        # idempotent; providers/auditor-c already bonded on the soak
./create-lifecycle.sh          # builds the REAL DA objects; records PALW_DA_REAL=1 + PALW_DA_OBJ_DIR
./submit-lifecycle.sh          # step 3.5 runs the DA challenge/response automatically
./start-palw-miner.sh          # mint the algo-4 block and assert acceptance on BOTH nodes
```

## What the DA step does (submit-lifecycle step 3.5)

Runs only when `PALW_DA_REAL=1`. For each sampled obligation:

- **Freeze DAA**: stop the supporting miner (the in-process validator emits
  transactions, not blocks, so the external `misaminer` is the only producer;
  stopping it freezes the sink at a stable `D`).
- **Challenge (exact-daa)**: a block never accepts its own txs, so a carrier in
  block X (`daa D+1`) is accepted by X's child Y (`daa D+2`). Set
  `opened_daa_score = D+2`, inject with `palw-submit --no-wait` (mempool only),
  mine EXACTLY 2 blocks (`misaminer --blocks 2`; devnet skip-pow → instant).
  Challenger = the independent, active `AUD_C_BOND` (≠ the obligation's
  provider_bond). A mistimed challenge is inert (never rejected, rate-limit not
  consumed) → a fresh-`D` retry is always safe.
- **Response**: signed by the CHALLENGED provider's owner seed
  (`provider-a/-b.seed`), chunk proof from `<lifecycle>/da/<object_root>.palwobj`.
  No exact-daa rule (just `current_daa ≤ deadline = D+2+200`); inject `--no-wait`,
  mine 2.
- Poll each obligation `Pending → Challenged → Satisfied`; resume the miner and
  proceed to the certificate carrier only once all are `Satisfied`.

Tuning knobs (env, all optional): `DA_CH_MAX_ATTEMPTS` (5), `DA_SINK_STABLE_TRIES`
(10), `DA_STATUS_POLL_SECS` (12), `DA_STATUS_EXTRA_BLOCKS` (2),
`DA_INJECT_SETTLE_SECS` (2), `DA_MINE_TIMEOUT_SECS` (60).

## Success criteria (accepted, not candidate)

```sh
# 1. certificate BLOB present (was always false under the random-root mock):
kaspa-pq-validator palw-status --node-wrpc-borsh 127.0.0.1:27610 --network devnet-111 --batch-id <id> \
  | grep -E 'certificate_blob_present|status'
#   → certificate_blob_present: true   AND   status: active

# 2. all obligations satisfied:
#   → every `da_obligation:` line shows status=satisfied

# 3. the algo-4 block is ADMITTED, not rejected:
#   node A log NO LONGER shows `algo-4 PALW ticket invalid: CertAbsent`; the minted
#   block enters the selected chain and settles a coinbase. start-palw-miner.sh
#   asserts acceptance on BOTH nodes (STN-012) and verify-consensus.sh cross-checks.
grep -c 'CertAbsent' "$HOME/.palw-testnet/devnet-111-soak/node-a/logs/"*.log   # → should stop growing
```

## Known gotchas (every one of these bit the first live run)

**A retired bond silently eats the carrier.** A slashed (or unbonded-and-lapsed)
provider bond's locked output-0 is plain P2PKH at the SAME funding address, so
`kaspa-pq-validator balance` counts it and funding-input selection picks it. But
`ProviderBondSpendFilter` (ADR-0040 ECON-03 leg 4) locks a non-releasable bond's
output-0 forever and acceptance **SKIPS** the spending tx — it is NOT a block
reject, so the block stays valid, the node logs nothing, and the only symptom is
`palw-submit` timing out on its change outpoint 120 s later. Tell-tale: node A's
`Tx throughput stats` shows `in: N via RPC` but `out: N-1 via accepted blocks`.
Fix: list the retired outpoint in `EXTRA_EXCLUDE_OUTPOINTS`. This applies to the
DA step too, which funds carriers from the PROVIDER/AUDITOR owner keys.

**Re-bonding a slashed identity.** Clear only that identity's `PROV_*_BOND` line
from `artifacts/state.env` and re-run `register-providers.sh`; it rebuilds and
resubmits. Re-registering with the SAME owner seed is fine (the registry is keyed
by bond outpoint). Keep the old outpoint in `EXTRA_EXCLUDE_OUTPOINTS` forever.

**A local `*.env` can clobber your override.** `soak.env` is gitignored and was
hand-written with a bare `export TICKET_MODE=skip`, which silently defeats
`TICKET_MODE=mock ./run-all.sh`. Every override in a local env file must use the
`${VAR:-default}` form that `env.example` uses. Check yours before blaming the
pipeline.

**The disk SLO gate is a PERCENTAGE.** `create-lifecycle` refuses to register a
new batch below 20 % free on the filesystem holding `PALW_DATA_ROOT`. On a large
workstation that can trip with tens of GiB actually free. The testnet itself is
tiny (~750 MB); the usual culprit is `target/debug`. `cargo clean --profile dev`
reclaims it without touching the `release/` binaries the harness runs.

**`palw-payload --out` refuses to overwrite** — every payload needs its own path.

**Bonded DA carriers must be submitted by their payload owner** — `palw-submit`
rejects a 0x3a/0x3b whose owner differs from `--validator-key`.

**Do not trust the mint gate's log scrape alone.** `_wait_algo4` historically
grepped for `pow_algo_id` / `replica` / `StatusUTXOValid`, tokens this kaspad
build does not emit, so it timed out on a genuinely accepted block. It now also
confirms over RPC. To check by hand, ignore the logs and ask consensus:

```sh
kaspa-pq-validator find-reward-settlement --node-wrpc-borsh 127.0.0.1:27610 \
  --network devnet-111 --source-block <algo-4 block hash>
#   → settlement.classification: blue      (admitted; red = weight-0 fork)
#   → settlement.source_batch_id: <your batch>   (proves it carries YOUR leaf)
# Run it against BOTH nodes (27610 / 27620) for cross-node parity.
```

## Notes / honesty

- The minted block is a WIRING-ONLY, NON-inference **mock-ticket** block; the DA
  object carries mock content. This proves the full PALW carrier + DA-availability
  + certificate + algo-4 admission plumbing end to end — NOT real inference and NOT
  PALW chain security (the algo-4 lane has fork-choice weight 0 in the closed net).
- `da-object-build` / `da-challenge` / `da-response` were validated offline
  (36 062-byte 3-chunk object; `da-inspect` round-trips `object_root ==
  receipt_da_root`); the wire-v5 round-trip has a unit test. The live drive above
  turned the 32+ prior **candidate** blocks into an **accepted** one on
  2026-07-25.
- **Provider rewards are NOT yet demonstrated.** The merging chain block
  `ac893d54…` has `coinbase_output_count: 0`, so no provider payout was observed.
  `find-reward-settlement` derived the expected split (inclusion 16477805,
  validator 30895885, base 158598881 → provider A 79299440 / B 79299441 sompi)
  but the verdict is `PARTIAL`: the exact-SPK assertion needs
  `--provider-a-spk/--provider-b-spk` and a merging block that actually pays.
  Whether that is the PALW-014 weight-0 lane behaving as designed, a
  settlement-lands-later effect, or a real gap is **open** — do not cite this run
  as evidence that provider payout works.
- The exact-daa challenge recipe is confirmed in practice, first attempt, twice:
  freeze DAA by stopping the supporting miner (the in-process validator emits
  transactions, not blocks), set `opened_daa = D + 2`, inject with
  `palw-submit --no-wait`, then mine EXACTLY 2 blocks. Sampled chunk indices were
  2 and 0 — real sampling, not a fixed chunk 0.
