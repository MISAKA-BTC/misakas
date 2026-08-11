# t10 SHADOW release — deploy card

Everything below is copy-paste. The decisions are already made and the numbers are already
measured; what is left is running them on the fleet.

**Fence: DAA 30_200_000.** Measured against a live tip of 29_981_862 (2026-08-11). t10 runs at
1 bps, so this is ~2.5 days of margin — twice the end-to-end duration of the 2026-08-10 flag day.

**Consensus fingerprint after this release: `8bf48730…`** (the fleet runs `5fabb683…` today). An
un-updated peer is rejected at the handshake, not silently forked — which is why the rollout can
be verified rather than hoped for.

---

## Fingerprints — RESOLVED 2026-08-11, and the misreading that produced the scare

A node built from the release commit now prints its own value at startup:

```
Consensus params fingerprint: 8bf487309c1371da52725b501a581600c8fbc98887071476f066d2ecdb6fe377
```

That matches the pin exactly, and matches `Params::from(NetworkId)` — source, pin and running
binary agree. There was never a broken build.

**What went wrong was the reading.** The numbers used earlier came from lines prefixed
`P2P, got reject message: … local: X, remote: Y` — messages this node RECEIVED. The sender fills
`local` with its own `consensus_params_id()` (`flow_context.rs:1293`), so in a received reject
`local:` is the PEER's fingerprint. Attributing it to our build inverted every conclusion and
sent two rounds of debugging after a clean binary.

So, correctly attributed:

| | fingerprint |
|---|---|
| the live t10 fleet today | `5fabb683…` |
| this release | `8bf48730…` |

The startup line above exists so this is never inferred from a peer again: it answers "is this
binary the release?" with no network at all.

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

Read the binary's own startup line instead — `Consensus params fingerprint:` — which must be
`8bf48730…`. Do NOT read it from a peer's rejection: in a received reject, `local:` is the
PEER's value, and reading it the other way is what cost two rounds of debugging here.

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
