# t10 SHADOW release — deploy card

Everything below is copy-paste. The decisions are already made and the numbers are already
measured; what is left is running them on the fleet.

**Fence: DAA 30_200_000.** Measured against a live tip of 29_981_862 (2026-08-11). t10 runs at
1 bps, so this is ~2.5 days of margin — twice the end-to-end duration of the 2026-08-10 flag day.

**Consensus fingerprint after this release: `d07cb673…`** (was `0e3914b0…` on the fleet). An
un-updated peer is rejected at the handshake, not silently forked — which is why the rollout can
be verified rather than hoped for.

---

## ⛔ BLOCKER — and a READING ERROR in this document's fingerprints

**Correction first, because everything below inherited it.** The line these numbers came from is
`P2P, got reject message: Consensus params mismatch … local: X, remote: Y`. That is a message
this node RECEIVED. `flow_context.rs:1293` shows the sender fills `local` with
`self.config.params.consensus_params_id()` — **its own** — so in a received reject, `local:` is
the PEER's fingerprint and `remote:` is ours. Every attribution in this document and in
`testnet10-vlt-shadow-fork-runbook.md` was made the other way round and must be re-read:

* `5fabb683…` is most likely the FLEET's fingerprint, not our build's.
* `0e3914b0…` is what the fleet saw from OUR build — which is this repository's pre-merge pin,
  and that does not match a build of the release commit either.

Neither reading is yet reconciled with the probe below, which is decisive about the source:

```
const                : 62e299b6…      (TESTNET_PARAMS, table empty)
Params::from(net)    : d07cb673…      (materialized — what a node runs)
Params::from(id{10}) : d07cb673…      (identical; the daemon's path reaches the same arm)
```

So the source is self-consistent and the pin is right for the materialized path. What is NOT
established is which binary announced what. **Do not cut a release, and do not trust any
fingerprint claim in these documents, until a node's own value is read from its own side** —
`grep "local_params_id"` at a debug log, or a one-line print after `build()` — rather than from
a peer's rejection.



A kaspad built from a CLEAN checkout of the release commit, in an isolated target dir, with
`--features evm`, announces `5fabb683…` at the handshake. The same source computes `d07cb673…`
for the materialized testnet preset (`Params::from(NetworkId)` → `with_registered_models`), under
default features and under `evm` alike. **Three explanations were tested and eliminated:** a
stale artifact (clean rebuild — same), a concurrent session editing the tree (clean worktree at
the release commit — same), and feature-flag skew (test green under `--features evm` — same).

So the node is not building its `Params` through the path the pin now measures. That is the same
class of miss as attaching the model table in kaspad's `apply_to_config`: an install point that
is not on the node's actual path. Until it is explained, the number this card tells an operator
to verify is not the number their node will print, and a flag day whose verification step is
wrong is worse than one with no verification step.

**Narrowed, 2026-08-11.** `kaspad/src/daemon.rs:410` is `let params: Params = network.into();`
— so it DOES go through `From<NetworkId> for Params`, and `with_registered_models` does run. The
release commit's preset genuinely carries `vlt_shadow_activation_daa_score:
TESTNET_VLT_SHADOW_FORK_DAA_SCORE` = 30_200_000 (verified in the clean worktree). The divergence
must therefore be introduced AFTER that, in the chain at `daemon.rs:433`:

```rust
ConfigBuilder::new(params).adjust_perf_params_to_consensus_params().apply_args(|c| args.apply_to_config(c)).build()
```

`apply_args` is eliminated too: every `config.params` mutation in `apply_to_config` sits inside
the `--vlt-devnet` / `--tkn-devnet` guards, which a testnet run never enters. That leaves
`adjust_perf_params_to_consensus_params()` and `build()`.

`adjust_perf_params_to_consensus_params` is eliminated as well — it writes only
`self.config.perf` (`consensus/core/src/config/mod.rs:218`), never `params`. So NOTHING in the
daemon's chain mutates `params`, and the node's announced value should already equal
`Params::from(network).consensus_params_id()`. It does not. The remaining candidates are
therefore `ConfigBuilder::build()` itself, or a difference between the `NetworkId` the daemon
passes (`--testnet --netsuffix=10`) and the one this test passes (`TESTNET_PARAMS.net`) — check
that `TESTNET_PARAMS.net.suffix` is `Some(10)` and that both reach the same match arm.

**Next step:** run the probe rather than reason further, then either stop it doing so or pin
the post-`build()` value (which is what the node announces and therefore what peers compare).
A one-line probe settles it: print `config.params.consensus_params_id()` right after `build()`
and compare with `Params::from(network).consensus_params_id()` before the builder.

The check below has now caught two real defects. It stays first in this document.

## 0. Staleness check — 10 seconds, do it first

```bash
curl -s https://misakascan.com/info/blockdag | python3 -c "
import json,sys; tip=int(json.load(sys.stdin)['virtualDaaScore']); H=30_200_000
print(f'tip {tip}  H {H}  margin {(H-tip)/86400:.2f} days')
print('OK' if H-tip > 86400 else 'TOO TIGHT — recompute H and re-cut')"
```

Under one day of margin means the fleet cannot finish updating before the fence arrives. Raise
`TESTNET_VLT_SHADOW_FORK_DAA_SCORE` in `consensus/core/src/config/params.rs`, re-run
`cargo test -p kaspa-consensus-core --lib`, re-pin the fingerprint it reports, and re-cut.

## 1. Calibrate the compute class — once per machine, before anything is deployed

```bash
MISAKA_PALW_CPU=1 MISAKA_LLAMA_SRC=<llama.cpp built -DGGML_METAL=OFF -DGGML_BLAS=OFF -DGGML_ACCELERATE=OFF -DGGML_NATIVE=OFF> \
  cargo build --release -p misaka-palw-worker
MISAKA_PALW_GGUF=<Qwen3.5-2B-Q4_K_M.gguf> scripts/misaka-palw-cpu-calibrate.sh \
  --worker ./target/release/palw-worker
```

Compare the printed line across machines **of the same architecture**. Identical ⇒ that class is
calibrated. Different ⇒ **stop**: an executor and its committee would refute each other while
both were honest, and the fence must not be scheduled for that class.

An `x86_64` line differing from an `aarch64` line is expected and fine — different classes,
different `ggml/src/ggml-cpu/arch/` kernels. Compare like with like.

Reference (Apple M4 Pro, aarch64):
```
MISAKA-PALW-CPU-CALIBRATION-v1 arch=arm64 os=Darwin class=8825d03e4da7faa1 runtime=f561dd30b7b69d31 cu=414 output=a78c75a364c074799261b9f2776639c4 trace=f96abaeee120a3e5f5f444528d4681ae
```

## 2. Verify the release binary is not stale — and build it in a PRIVATE target dir

**Build with `CARGO_TARGET_DIR` set to a directory nothing else writes.** A release binary built
into a shared `target/` cannot be trusted: on 2026-08-11 a freshly built kaspad reported
`5fabb683…` at the handshake — a value matching NEITHER the const-derived pin nor the
materialized one, because a concurrent session was editing `params.rs` while the build ran. A
release cannot be cut from a tree another session is editing; take a clean checkout of the
release commit. The check below is what caught it, twice, which is the argument for running it
every time.

```bash
CARGO_TARGET_DIR=/tmp/misaka-release cargo build --release --features evm   --bin kaspad --bin misaminer --bin kaspa-pq-validator --bin misaka
```



```bash
./kaspad --testnet --netsuffix=10 --appdir=/tmp/probe --addpeer=<any fleet IP>:26211 2>&1 | grep -m1 "params mismatch"
```

The `local:` value must be `d07cb673…`. If it is anything else the binary predates the release —
rebuild. (This is exactly how the fleet's current ruleset was identified.)

## 3. Roll out — A2 pattern, `docs/testnet10-transition.md`

Every validator, miner and seeder binary inside the fleet **before** DAA 30_200_000. The
collector's per-host version table is the go/no-go gate. A host that crosses the fence on the old
build is rejected by its peers at the handshake, so the failure is visible and recoverable — but
it is still an outage for that host.

Each validator additionally needs, to participate in compute (optional — a node can run the
overlay without executing):

```bash
--enable-compute --compute-worker=<palw-worker> --compute-prompt=<file> --compute-max-tokens=128
# and in its environment: MISAKA_PALW_GGUF=<the pinned GGUF>
```

Without `--compute-prompt` a node audits peers and originates nothing, which is a legitimate and
useful configuration — auditing is what the network is short of.

## 4. Watch the fence land

```bash
grep -E "vlt-state|vlt-shadow|validator-compute|vlt-credit" kaspad.log | tail
```

Expect, in order: `[vlt-state] none -> pre_shadow`, then at 30_200_000 `pre_shadow -> shadow`,
then `[validator-compute] enabled: runtime=…` on every compute node, then jobs committing and
`compute: confirmed certificate … — our replay reproduced R_j` as committees audit each other.

`[vlt-shadow]` is the line the soak is FOR: it reports `W(E)`, how much of it signed, and how
many epochs *would* have reached quorum. Step 4 (the weight fork) is scheduled only when that
line has been healthy and boring across the whole fleet for at least one full credit window.

## What this release does NOT do

The weight fence stays `u64::MAX`. Finality still runs on bonded stake; the overlay runs beside
it, credited and policed, with nothing depending on the answer. What becomes real is the audit
fee (coinbase value) and challenge slashing (bonds) — see the runbook's blast-radius section.

Rollback is a point release moving the fence forward, not a chain rollback: below the weight
fence the overlay cannot stall finality.
