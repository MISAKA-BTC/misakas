# testnet-10 PALW rollout runbook — the LLM-PoW re-genesis

The release that makes the public testnet's proof-of-work one deterministic Qwen3.5-2B
inference per attempt (ADR-0021), at one block per 10 seconds. This is a **re-genesis**, the
"-bs3" precedent: the new chain is cryptographically distinct, an un-wiped node hits the
startup genesis-mismatch guard, and nothing of the old chain (balances included) carries over.

## Identity of the new network

| what | value |
|---|---|
| genesis hash | `477f85fc a51674f5 …` (`TESTNET_GENESIS`, "-palw" payload marker) |
| consensus fingerprint | `2d2258cc51a3b2216bab6d93b0aec2332322903e5e7414db15ad8112adced671` (pin it MATERIALIZED — `Params::from(net)` — per the 8208cd6 lesson) |
| PoW | `algo_id = 5` (PALW via **Ollama**) from genesis; fixture env **refused** on this network |
| block rate | 0.1 bps (`target_time_per_block = 10 s`, ghostdag k = 4) |
| genesis difficulty | `0x207fffff` (p ≈ ½ per inference; DAA converges within the 150-block min window) |
| block subsidy (year 1) | 37_046_834_500 sompi ≈ 370.47 MSK per block — the same 37.047 MSK/s rate |
| emission fork | none: `crescendo_activation = always` (the 88_657_000 score belonged to the old chain) |
| VLT windows | wall-clock preserved (÷100 table in `TESTNET_DNS_PARAMS`): 14-day evidence/unbond = 120_960/120_990 blocks, epochs 10 blocks (100 s), gate horizon 1_800 / veto TTL 600 |
| DNS seeders | unchanged records (`seeder1-4.misakascan.com`, `seeder1-3.misakachain.com`, `seeder1.misakastake.com`) — the daemons must run this build to crawl the new chain |

## Per-host prerequisites — the Ollama runtime (user decision 2026-08-11)

Every **validating** node replays one inference per header against a **host-local Ollama server**
running the pinned Qwen model — the runtime an Ubuntu VPS fleet actually operates (the
Metal-pinned worker stays devnet's algo-4 runtime). Per host:

1. **Provision with `scripts/misaka-palw-ollama-setup.sh`** (run ON the VPS):
   installs Ollama (systemd unit, 127.0.0.1:11434), pulls the model, prints the **model digest**
   and the **calibration line**. Every fleet host must print the same digest, and hosts of the
   same architecture must print identical calibration lines — different ⇒ STOP (version/blob
   skew; an executor would refute honest verifiers).
2. **Determinism class = (Ollama version, model digest, architecture).** Greedy decoding is
   reproducible within one class; ACROSS architectures (NEON vs AVX2 reduction order) it is not
   promised — the same arch-scoping the VLT CPU compute class documents. The public chain is
   therefore mined AND validated by the fleet's class (x86-64 Ubuntu). An arm64 machine (the
   M4 Pro dev box) forms its own class: fine for local E2E, not a validator of the public chain
   unless its calibration line happens to match — verify, never assume.
3. **Environment for kaspad AND misaminer**:
   ```
   export MISAKA_PALW_OLLAMA_MODEL=qwen3.5:2b        # the fleet's pinned ref — one value everywhere
   # export MISAKA_PALW_OLLAMA_URL=http://127.0.0.1:11434   # default; set only if changed
   ```
   kaspad checks at startup on PALW networks that the model env is set and the server is
   reachable, and exits with instructions otherwise — no first-header panic.
   `MISAKA_PALW_POW_FIXTURE=1` is refused outside devnet.
4. **Tag semantics under Ollama** (ADR-0021 addendum): the API exposes no per-decode logits, so
   the algo-5 tag commits to the greedy response bytes + token counts — weaker binding than the
   worker's `gemm_trace_root`, still model-work-priced. Devnet keeps the stronger algo-4 worker
   tag; a future Ollama fork exposing logits (or a logits-serving shim) can restore full-trace
   binding as algo 6.

## Rollout order (single coordinated window — this is a flag day AND a re-genesis)

0. **Decide the deploy that day** — two mutually exclusive t10 plans exist on this branch:
   * **Plan A** (`docs/testnet10-shadow-release-DEPLOY.md`): VLT shadow release on the EXISTING
     1-bps chain, fence DAA 30_200_000. No re-genesis; PoW stays algo 3.
   * **Plan B** (THIS runbook): the PALW re-genesis — new chain, algo 5, 0.1 bps. The shadow
     card's fence value is meaningless here; on the fresh chain set
     `TESTNET_VLT_SHADOW_FORK_DAA_SCORE` to a small height (e.g. 20_000 ≈ 2.3 days at 0.1 bps)
     or run the soak first and schedule later — the one-constant machinery works unchanged.
   Deploying A then B later re-genesises twice; deploying B first obsoletes A's fence math.
   Decide, then re-pin the materialized fingerprint for whichever constants ship.
1. Build this branch on every host: `cargo build --release -p kaspad --features evm -p misaminer`.
   The `--features evm` requirement is unchanged from the current fleet. Cut the release from a
   clean checkout of the release commit (a tree another session is editing poisons the build —
   the 8208cd6 lesson), into a private target dir.
2. Rehearse locally first (any host with Ollama + the model pulled):
   ```
   NET=testnet-10 IBD=1 MISAKA_PALW_OLLAMA_MODEL=qwen3.5:2b BLOCKS=6 \
     bash scripts/misaka-palw-pow-e2e.sh
   ```
   PASS = mine → independent replay validation → fail-fast probe → fresh-node from-genesis IBD.
3. Stop the old fleet (nodes, miners, seeder daemons). Back up, then remove the old datadirs —
   the genesis-mismatch guard would refuse them anyway.
4. Start kaspad on the seeder-backed hosts first (with the env), then the seeder daemons from
   this build, then the remaining nodes.
5. Start the miner(s) (same `MISAKA_PALW_OLLAMA_MODEL` env; one inference per nonce against the
   local Ollama): the first template on a fresh chain reports `is_synced=false`, so the cold
   start needs the explicit consent flag, exactly like devnet bootstrap:
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
* **Miners**: one attempt = one `/api/generate` call; Ollama keeps the model RESIDENT between
  calls (its keep_alive default), so there is no per-attempt model-load cost — on VPS CPUs
  expect ~5-15 s/attempt for prefill 70 + decode 48 at 2B-Q4. Size the fleet's miner count for
  the 10 s target accordingly; the DAA absorbs whatever the real rate is.
* **VLT**: bonding floors are unchanged (10 KAS); epochs are now 100 s wall-clock (10 blocks),
  and the 14-day evidence/unbonding windows are the same 14 days they always were.
* **Rollback**: the old chain's datadir backups + the previous build restore the pre-PALW
  testnet exactly (its fingerprint pins are in git history); the two chains can never confuse
  each other (distinct genesis + fingerprint + the "-palw" marker).
