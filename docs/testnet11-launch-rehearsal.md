# testnet-11 launch rehearsal — two x86-64 Linux hosts

The dress rehearsal that runs before the public Relaunch-2 announcement. It exists because of a
specific failure shape this project keeps meeting: **a configuration that starts fine, syncs fine,
and fails minutes later**. The `--features evm` gap found on 2026-08-20 is the latest one — the
node booted, synced, and panicked the instant it built a template, and no amount of reading the
code surfaced it. Only booting did.

So the rehearsal is not "does it work". It is **"do the four failures we can predict actually look
the way we told operators they would, and does the happy path hold across two machines"**.

Budget half a day. One person can drive both hosts.

---

## 0. What this proves, and what it cannot

**Proves:**

- two independently-built-and-shipped hosts compute **bit-identical** PALW tags on real blocks;
- a second host syncs from genesis by replaying every header's inference;
- the community allocation is present and spendable at block 0;
- a miner is paid, and the emission matches §8 of the operator doc;
- each predicted failure mode produces the message the operator doc promises.

**Cannot prove:** behaviour at N>2 hosts, over WAN latency, or under adversarial load. Those are
what the public net is for. It also cannot prove the arithmetic is right in general — the
class probe is a fixed four-job corpus, so `ONE-CLASS` is evidence, not proof.

## 0.1 Exit criteria — decide these BEFORE you start

A rehearsal without a written pass mark is just "we ran it". Every line must be checked off:

| # | criterion | where |
|---|---|---|
| 1 | Both hosts report the same `runtime_class_id` **and** the same `palw-worker` sha256 | §2 |
| 2 | `misaka-palw-v2-class-compare.py` says `ONE-CLASS` | §2 |
| 3 | Both nodes log consensus fingerprint `49ff9628…` and genesis `3564ea39…` | §4 |
| 4 | Host B reaches host A's tip from an empty datadir, replaying every header | §6 |
| 5 | Every block A mines is accepted by B (and vice-versa) — zero rejects in either log | §6 |
| 6 | `misaka wallet utxo list` shows the right amount for ≥3 community addresses | §7 |
| 7 | The miner's coinbase matches the §8 emission figure | §7 |
| 8 | Headroom ≥ **2×** on both hosts (below 1× the network cannot admit newcomers) | §8 |
| 9 | All four failure drills produce their documented message | §9 |
| 10 | Neither node enters `quarantined` during the run | §9.5 |

If 8 lands between 1× and 2×, **do not launch** — publish a lower block rate or a higher-spec
requirement first. A network that cannot admit newcomers is closed, whatever its uptime.

---

## 1. Hosts

| | host A | host B |
|---|---|---|
| role | genesis node + miner | joiner (from-genesis IBD) |
| CPU | x86-64, ≥4 free cores | x86-64, ≥4 free cores |
| RAM | 8 GiB | 8 GiB |
| disk | 4 GiB free | 4 GiB free |
| OS | Ubuntu 24.04 (the fleet's glibc 2.39) | same |

They must reach each other on the p2p port. **Check the clock on both** —
`timedatectl` should show NTP synchronised. The PALW seed binds the header timestamp, and a host
minutes out of step produces headers its peer rejects as time-too-far-into-the-future; that reads
as "PALW is broken" and is not.

Ports used below (pick your own, but keep them consistent):

```
37711  p2p          37710  gRPC (miner)          27210  wRPC Borsh (wallet CLI)
```

---

## 2. Build once, ship the bytes, then prove one class

**Build the worker on ONE host and copy the binary to the other.** Two independent builds of the
same source can differ (toolchain, cmake cache), and the golden-set gate will reject the mismatch
later with a confusing message. One build, two copies, one sha256.

```bash
# build host only, against the pinned CPU-profile llama.cpp tree
#   commit 030ebb558a5820b444a8f836ed5cdd46c9b4bd7a
#   NATIVE=OFF F16C=ON AVX2=ON FMA=ON SSE42=ON CPU_ALL_VARIANTS=OFF OPENMP=OFF BLAS=OFF METAL=OFF
MISAKA_PALW_CPU=1 MISAKA_LLAMA_SRC=$HOME/llama.cpp-cpu \
  cargo build --release -p misaka-palw-worker

sha256sum target/release/palw-worker      # record it; both hosts must match
scp target/release/palw-worker  hostB:/opt/misaka/
```

The node and miner may be built per host, **but the node needs the feature flag**:

```bash
cargo build --release -p kaspad    --bin kaspad --features evm   # ← MANDATORY, see §9.1
cargo build --release -p misaminer --bin misaminer
cargo build --release -p misaka-cli --bin misaka
```

Then, on **each** host:

```bash
sha256sum /opt/misaka/palw-worker                                    # criterion 1a
sha256sum /opt/misaka/Qwen3.5-2B-Q4_K_M.gguf
#   want aaf42c8b7c3cab2bf3d69c355048d4a0ee9973d48f16c731c0520ee914699223
#   size 1280835840 bytes exactly

MISAKA_PALW_GGUF=/opt/misaka/Qwen3.5-2B-Q4_K_M.gguf \
  /opt/misaka/palw-worker --mode manifest                           # criterion 1b
#   want runtime_class_id: misaka-palw-lite-cpu/x86_64/v1
```

Cross-host class check, **before any chain exists**:

```bash
# on each host
MISAKA_PALW_GGUF=/opt/misaka/Qwen3.5-2B-Q4_K_M.gguf \
  bash scripts/misaka-palw-v2-class-probe.sh /opt/misaka/palw-worker $(hostname) > class-$(hostname).json
# collect both files on one host
python3 scripts/misaka-palw-v2-class-compare.py class-*.json         # criterion 2: want ONE-CLASS
```

> **Stop here if this is not `ONE-CLASS`.** Everything downstream assumes it. A `BUILD-MISMATCH`
> verdict means "determinism untested", not "hosts disagree" — usually it means the two hosts did
> not get the same binary, which §2's ship-the-bytes rule exists to prevent.

---

## 3. Wipe

Relaunch 2 changed the genesis. Any Relaunch-1 state stops the node at the genesis-mismatch guard.

```bash
rm -rf /var/lib/misaka-t11        # both hosts
```

---

## 4. Host A — genesis node

```bash
PALW_WORKER=/opt/misaka/palw-worker \
MISAKA_PALW_GGUF=/opt/misaka/Qwen3.5-2B-Q4_K_M.gguf \
  ./kaspad --testnet --netsuffix=11 --appdir=/var/lib/misaka-t11 \
           --listen=0.0.0.0:37711 --rpclisten=127.0.0.1:37710 \
           --rpclisten-borsh=127.0.0.1:27210 --utxoindex \
           2>&1 | tee /var/log/misaka-t11-A.log
```

`--utxoindex` is what makes §7's balance check possible. Turning it on later means a reindex, so
turn it on now.

At startup the node logs a class-calibration line (one probe inference, a few seconds) and then
its own fingerprint. **Check both against criterion 3:**

```bash
grep -E "class calibration|Consensus params fingerprint" /var/log/misaka-t11-A.log
#   want: "PALW worker runtime verified in the pinned determinism class of testnet-11"
#   want: "Consensus params fingerprint: 49ff9628…  (network testnet-11)"
```

The fingerprint line is **this node's own** announced value. Never read a fingerprint out of a
peer's rejection message and attribute it to yourself — that misreading cost a release cut on
2026-08-11.

---

## 5. Host A — mine

```bash
./misaminer --rpc=127.0.0.1:37710 --network-id=testnet-11 \
            --wallet=<a misakatest: address you hold the key for> \
            --worker=rehearsal-A --blocks=0 \
            2>&1 | tee /var/log/misaka-t11-A-miner.log
```

`--network-id=testnet-11` is the Layer-0 domain separator, not a label: a wrong value mines tags
for a different network and nothing is ever accepted.

Let it produce **at least 20 blocks** before starting host B. One attempt costs seconds, so this is
minutes, not hours. Confirm from the miner log that blocks are landing and note the coinbase
figure for criterion 7.

---

## 6. Host B — join from genesis

This is the criterion-4/5 test, and the one that only exists with two hosts.

```bash
PALW_WORKER=/opt/misaka/palw-worker \
MISAKA_PALW_GGUF=/opt/misaka/Qwen3.5-2B-Q4_K_M.gguf \
  ./kaspad --testnet --netsuffix=11 --appdir=/var/lib/misaka-t11 \
           --listen=0.0.0.0:37711 --rpclisten=127.0.0.1:37710 \
           --rpclisten-borsh=127.0.0.1:27210 --utxoindex \
           --addpeer=<host A ip>:37711 \
           2>&1 | tee /var/log/misaka-t11-B.log
```

There are no DNS seeds for testnet-11 yet, so `--addpeer` is the only way in. That is also what the
public announcement will have to carry.

Watch B replay every header — each accepted header is one inference on B's own CPU:

```bash
grep -c "Accepted block" /var/log/misaka-t11-B.log        # climbs toward A's height
grep -iE "reject|invalid|mismatch" /var/log/misaka-t11-B.log   # criterion 5: must stay EMPTY
```

**Criterion 5 is the heart of the rehearsal.** B accepting A's blocks means two separately-running
machines computed the same 200-byte tag for every header. One reject line here and the class is not
one class, whatever §2 said.

Then reverse it: keep both nodes up and mine a few blocks **from host B** (same miner command,
`--worker=rehearsal-B`). A must accept those too. A one-directional test would pass with one host
silently in a permissive mode.

---

## 7. The promises to participants

**Criterion 6 — the community allocation is really there and really spendable.** Pick at least
three addresses from `TESTNET11_COMMUNITY_ALLOCATIONS`, including one that changed address
(tetsu31 or uki) so the supersession is confirmed to have taken effect:

```bash
./misaka --network=testnet-11 --rpc=127.0.0.1:27210 \
  wallet utxo list --address misakatest:qtjw605sgh0uha25crcxy4sp8hl644x4ddl3msrtnurv3c4prz6cnag9hle8a5vyqkxgw54cl6tzyuap7j47yajf4wq3cl0tqdgup50rkdm9r4k3
#   want exactly 30,000,000 MSK in one UTXO (Kurenai)
```

Also confirm a **superseded** address shows nothing:

```bash
./misaka --network=testnet-11 --rpc=127.0.0.1:27210 \
  wallet utxo list --address misakatest:qfa2z97yspcra7pel80h06jg4a6mg0669fj5qx63e4v5y8geddd8hvyvy75rqaejgrq69e8yv4nd66rzlt5tqepw95q7q3k55qev84g6ey5yj8x8
#   want: empty  (uki's OLD address — the allocation went to the 2026-08-19 one)
```

If you hold a key for one of the test addresses, do a real send to prove spendability end to end.
If you do not, the UTXO listing plus the M-07 round-trip (which every node runs at start) is the
available evidence — say so in the report rather than implying a spend was tested.

**Criterion 7 — emission.** Compare the miner's coinbase against operator-doc §8: 2756.28 MSK/block
to the miner while no validators are bonded (62 % of the 4445.62 MSK/block schedule). A different
number means the carve or the schedule moved and the announcement text is wrong.

---

## 8. Record the numbers (criterion 8)

On **both** hosts, after ≥30 minutes of steady operation:

```bash
bash scripts/misaka-palw-headroom.sh /var/lib/misaka-t11 60
```

Record `headroom`, the median per-header validation time, and the observed seconds-per-block. The
reference host measured 3.0–5.4×. **Below 1× a node can never finish syncing**; between 1× and 2×
there is no margin for a slower participant machine, and the public net will quietly become
un-joinable for exactly the people you are inviting.

Also record: peak RSS of node + worker, and whether `vmstat` shows any steal.

---

## 9. Failure drills — do these deliberately

Every one of these is something a participant will hit in week one. The operator answering support
should have seen each message once, on purpose, in a controlled setting.

### 9.1 Node built without `--features evm`

```bash
cargo build --release -p kaspad --bin kaspad        # NO feature flag
# run it, then mine against it
```

Expect: the node starts, syncs, and then **panics** with

```
the EVM lane is active at DAA 0 but this kaspad was built without the `evm` feature
 — cannot build a valid template (rebuild with --features evm)
```

This is the 2026-08-20 discovery. It fails on the *miner*, minutes after a start that looked
perfect. Rebuild with the flag afterwards.

### 9.2 Node with no worker configured

```bash
./kaspad --testnet --netsuffix=11 --appdir=/tmp/t11-drill   # no PALW_WORKER
```

Expect refusal **before any peer is dialed**, naming both `PALW_WORKER` and `MISAKA_PALW_GGUF`.

### 9.3 Out-of-class worker

Point `PALW_WORKER` at a worker built without the pinned flags (or from an arm host). Expect the
class gate to run one probe inference and refuse, printing `expected calibration` vs `got`. Note
that the two hex strings **share their head and tail and differ in the middle**: same output
commitment, different GEMM trace root — the same text, different arithmetic. That signature is how
you recognise a class problem versus a model problem in a support thread.

### 9.4 Stale (Relaunch-1) datadir

Point a node at a pre-wipe appdir. Expect the startup genesis-mismatch guard, not a silent resume.
This is the message every existing t11 operator will meet at the announcement, so have the exact
text ready to paste.

### 9.5 Quarantine (criterion 10)

Throughout the run, watch for:

```bash
grep -i "quarantin\|switched chains" /var/log/misaka-t11-*.log
```

Neither node should quarantine in a two-host rehearsal. If one does, capture the full log before
restarting — the switch-counter runaway was fixed (`a refused chain switch no longer feeds the
counter that refused it`), so a quarantine here is a **new** finding and worth stopping the
rehearsal for.

---

## 10. Go / no-go

Write the result as the ten criteria with a value beside each, not as a paragraph. Then:

- **All ten pass** → publish: the binaries (with the worker sha256), the datadir-wipe notice, host
  A's `<ip>:37711` as the bootstrap peer, and the operator doc link.
- **8 between 1× and 2×** → no-go until the block interval or the host requirement changes.
- **Any of 1, 2, 3, 5 fails** → no-go, and it is a code/build problem, not an operations one. Bring
  the two `class-*.json` files and both node logs.
- **6 or 7 fails** → the genesis or the emission text is wrong. Do not announce numbers you have
  not seen a node produce.

## 11. Tear down, and what to keep

Keep, for the launch record: both `class-*.json`, the headroom output from both hosts, the
fingerprint lines, the first 20 blocks' miner log, and the four drill messages. They are the
evidence for the announcement's claims, and the baseline any later "it got slower" is measured
against.
