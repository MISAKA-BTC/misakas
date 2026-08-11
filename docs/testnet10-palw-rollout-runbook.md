# testnet-10 PALW rollout runbook — the LLM-PoW re-genesis

The release that makes the public testnet's proof-of-work one deterministic Qwen3.5-2B
inference per attempt (ADR-0021), at one block per 10 seconds. This is a **re-genesis**, the
"-bs3" precedent: the new chain is cryptographically distinct, an un-wiped node hits the
startup genesis-mismatch guard, and nothing of the old chain (balances included) carries over.

## Identity of the new network

| what | value |
|---|---|
| genesis hash | `477f85fc a51674f5 …` (`TESTNET_GENESIS`, "-palw" payload marker) |
| consensus fingerprint | `32cbf80f4264dd1336b3d02664baf2595fbdf21d37fa6f83f324b241306ad158` |
| PoW | `algo_id = 4` (PALW LLM) from genesis; fixture env **refused** on this network |
| block rate | 0.1 bps (`target_time_per_block = 10 s`, ghostdag k = 4) |
| genesis difficulty | `0x207fffff` (p ≈ ½ per inference; DAA converges within the 150-block min window) |
| block subsidy (year 1) | 37_046_834_500 sompi ≈ 370.47 MSK per block — the same 37.047 MSK/s rate |
| emission fork | none: `crescendo_activation = always` (the 88_657_000 score belonged to the old chain) |
| VLT windows | wall-clock preserved (÷100 table in `TESTNET_DNS_PARAMS`): 14-day evidence/unbond = 120_960/120_990 blocks, epochs 10 blocks (100 s), gate horizon 1_800 / veto TTL 600 |
| DNS seeders | unchanged records (`seeder1-4.misakascan.com`, `seeder1-3.misakachain.com`, `seeder1.misakastake.com`) — the daemons must run this build to crawl the new chain |

## Per-host prerequisites

Every **validating** node replays one pinned inference per header. Per host:

1. **Hardware class: Apple Silicon (Metal)** — the v1 tag is pinned to the Metal runtime class
   (`qwen35_pins`, the class the 78-replay devnet verification ran on). The portable CPU class
   (8d31521) exists for VLT compute; adopting it for PoW is a *different tag* = its own
   coordinated fork. Until then, only Apple-Silicon hosts can validate.
2. **Worker binary** — `cargo build --release -p misaka-palw-worker` with the pinned llama.cpp
   build present (`MISAKA_LLAMA_SRC`, default `/Users/wata/Downloads/misaka-palw-runtime/llama.cpp`,
   commit `030ebb55…` build 10358, Metal profile).
3. **Model artifact** — `Qwen3.5-2B-Q4_K_M.gguf`, exactly 1_280_835_840 bytes,
   SHA-256 `aaf42c8b…` (the worker refuses anything else; distribute out-of-band, verify the sha
   after copy).
4. **Environment for kaspad AND misaminer**:
   ```
   export PALW_WORKER=/path/to/target/release/palw-worker
   export MISAKA_PALW_GGUF=/path/to/Qwen3.5-2B-Q4_K_M.gguf
   ```
   kaspad now checks this at startup on PALW networks and exits with instructions if missing —
   no more first-header panic. `MISAKA_PALW_POW_FIXTURE=1` is refused outside devnet.

## Rollout order (single coordinated window — this is a flag day AND a re-genesis)

1. Build this branch on every host: `cargo build --release -p kaspad --features evm -p misaminer`
   (plus the worker, above). The `--features evm` requirement is unchanged from the current fleet.
2. Rehearse locally first (any one Apple-Silicon host):
   ```
   NET=testnet-10 IBD=1 PALW_REAL=1 PALW_WORKER=… MISAKA_PALW_GGUF=… BLOCKS=6 \
     bash scripts/misaka-palw-pow-e2e.sh
   ```
   PASS = mine → independent replay validation → fail-fast probe → fresh-node from-genesis IBD.
3. Stop the old fleet (nodes, miners, seeder daemons). Back up, then remove the old datadirs —
   the genesis-mismatch guard would refuse them anyway.
4. Start kaspad on the seeder-backed hosts first (with the env), then the seeder daemons from
   this build, then the remaining nodes.
5. Start the miner(s): the first template on a fresh chain reports `is_synced=false`, so the
   cold start needs the explicit consent flag, exactly like devnet bootstrap:
   ```
   misaminer --rpc=127.0.0.1:26210 --network-id=testnet-10 --wallet=<payout> --mine-when-not-synced
   ```
   Drop `--mine-when-not-synced` once the chain is moving (F3: it must never run in normal
   operation). Expect sub-10 s blocks for the first ~150 blocks (fixed genesis difficulty
   window), then the DAA walks onto the 10 s cadence.
6. Verify the public path from a machine that is NOT in the fleet: start a kaspad with only the
   env set (no `--addpeer`) and watch it discover via DNS, IBD from genesis (one inference per
   header — ~1 s/header hot), and report the fleet's sink. That is the full public flow.

## Operating notes

* **IBD cost grows with the chain**: a day of chain is 8_640 headers ≈ 2-3 h of replay on one
  host. Before the chain is months old, ship either the Open-then-Audit sampled-audit tier or
  trusted-checkpoint IBD (ADR-0021 consequences).
* **Miners**: one attempt = one worker process = one model load. Throughput comes from the OS
  page cache (~0.5-1 s/attempt hot); a resident-worker protocol is the known optimization and
  changes no consensus rule.
* **VLT**: bonding floors are unchanged (10 KAS); epochs are now 100 s wall-clock (10 blocks),
  and the 14-day evidence/unbonding windows are the same 14 days they always were.
* **Rollback**: the old chain's datadir backups + the previous build restore the pre-PALW
  testnet exactly (its fingerprint pins are in git history); the two chains can never confuse
  each other (distinct genesis + fingerprint + the "-palw" marker).
