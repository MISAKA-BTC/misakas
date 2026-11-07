# VPS regression fixture

Two mined chains on live infrastructure, used to run the IBD recovery regression against a real
267 ms intercontinental link instead of loopback. This file records what the fixture is, how to
verify it has not drifted, and — as importantly — the boundary between it and the production
services that share those hosts.

## Boundary

Both hosts run production testnet-10 services. The regression is confined to:

- directory `/tmp/misaka-regress` on each host,
- simnet with `--override-params-file` (never testnet params),
- ports `412xx`.

Nothing in the regression touches production ports (26211/26610/27610/28610/8545), production data
directories, or production units. **Never** `pkill -f` on these hosts — the pattern matches
production binaries. Stop a regression process only after `readlink /proc/<pid>/exe` confirms it
points inside `/tmp/misaka-regress`.

Production units, confirmed `active` at capture time:

| Host | Units |
|---|---|
| 95.111.236.186 | `kaspad-tn`, `misaka-dnsseeder`, `misapool` |
| 160.16.131.119 | `kaspad-tn`, `kaspa-pq-miner-tn`, `kaspa-pq-validator-tn`, `kaspa-db-filler`, `kaspa-rest-server`, `misaka-dnsseeder`, `misaka-mtp`, `kaspa-seed-tunnel` |

## What the fixture is

| | Light branch | Heavy branch |
|---|---|---|
| Host | 95.111.236.186 (VPS1) | 160.16.131.119 (VPS2) |
| Blocks mined | 3500 | 5200 |
| Role | the branch the follower races onto | the branch it must end up on |

The follower runs on VPS1, so the light peer is loopback and the heavy peer is 267 ms away — the
asymmetry is deliberate. It reproduces the condition that decided testnet-22: the *worse* chain is
the *closer* one.

Mining is deliberately not repeated per round. The two histories are expensive and fixed; what
varies between runs is the follower's experience of the network, which is the thing under test.

## Snapshot, 2026-08-09T12:11Z

Read-only capture. Nothing was written to either host.

```
shallow_preset.json  df4161582810d12723a623669514f8d6a376e2fa03bc434dcfc18f9c2d6f336f
regress_node.sh      e86e35ebf3b4f76ae0a289538b1c5332f5431abc22ce55c605271e9439d90e19
regress_mine.sh      8ce6c0d0c5d133266ce6b3d332be0c63913263aaf43ef11d75695985cf080a8c
fixture-metadata.txt 27641079b42812b14d7309fab265106e7805f8b64679e14acec131e94b76d320
```

Both preset hashes match, so the two branches were mined under identical rules — the precondition
for the comparison meaning anything.

Chain data, as a rollup of the per-file digests, at 2026-08-09T12:11Z:

```
light + follower (VPS1)  16792b1effc156ecad404ee83bfd189cf97e9d1b6039290d61c898bb74e20b69
heavy (VPS2)             67637b95b9546baf3b35bd5a90d0ec683f7cb6fbe10d782d9822d35d1843a999
```

**That digest is provenance, not an invariant, and an earlier version of this file said otherwise.**
Merely starting a node rewrites its RocksDB — compaction, WAL, manifest — so the digest changes
without a single block changing. Checking it before a run would fail every time and teach whoever
runs this to ignore it.

What identifies a branch is what the chain says about itself, and it is what the round script
compares anyway:

```bash
ssh -i ~/.ssh/claude_key root@95.111.236.186 '/tmp/misaka-regress/src/target/release/regress-rpc 127.0.0.1:41610'
ssh -i ~/.ssh/claude_key ubuntu@160.16.131.119 '/tmp/misaka-regress/src/target/release/regress-rpc 127.0.0.1:41610'
```

At the time of the runs recorded here:

| | pruning point | virtual DAA score |
|---|---|---|
| light (VPS1) | `9d9d9940db34b378…021b7b96` | 7000 |
| heavy (VPS2) | `466a060561bff43e…bfcc76f5` | 8302 |

Distinct pruning points and the heavy branch ahead: if either stops being true, the fixture has
drifted and a green round means nothing. Check these, not the file digests.

The file digests remain useful for the one thing they do describe: whether the fixture *inputs* —
preset and scripts — are the ones these results were produced with.

## Known property of this fixture

`fixture-metadata.txt` records that pruning-point blue work measured **equal** across the two
branches (80289507 vs 80289507). That is not a defect in the fixture — it is why the comparison
cannot be made at the pruning point alone, and why adoption compares verified tip work with the
canonical `(blue_work, hash)` fork-choice order. A fixture where the pruning points differed in
work would have hidden that.
