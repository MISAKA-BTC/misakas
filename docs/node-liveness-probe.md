# Node liveness probe

`systemctl is-active` and `Restart=` watch whether the PROCESS exists. On 2026-09-02 the
testnet-11 public node on .113 hung four minutes after its 5d start — every runtime worker
blocked, no epoll thread, every socket CLOSE-WAIT with unread bytes, 0% CPU — and stayed
"active" for 46 minutes while the explorer, the faucet and the public P2P entry were dead. Nothing
inside a wedged process can report that it is wedged; the check has to come from outside.

## What the probe measures

`misaka node liveness` (misaka-cli) asks the node's wRPC for `getBlockDagInfo` under a timeout and
keeps the answer in a state file:

| Verdict | Exit | Meaning |
|---|---|---|
| `ALIVE` | 0 | the RPC answered and the virtual DAA or block count advanced (or the stall window has not elapsed) |
| `WEDGED` | 11 | the connect or the first RPC did not answer within `--timeout` — the hang's shape |
| `STALLED` | 12 | the RPC answers but nothing has moved for `--stall-secs` |

`STALLED` cannot distinguish a stalled node from an idle chain by itself; the detail carries the
sink's past-median age so an operator can. The watchdog script therefore restarts on `WEDGED`
immediately and on `STALLED` only after two full windows, never twice within a cooldown.

## Install

```bash
# the CLI on the host
cp target/release/misaka /usr/local/bin/misaka
cp scripts/misaka-node-liveness-probe.sh /usr/local/bin/misaka-node-liveness-probe
```

`/etc/systemd/system/misaka-liveness@.service`:

```ini
[Unit]
Description=MISAKA node liveness probe for %i

[Service]
Type=oneshot
Environment=NETWORK=testnet-11
Environment=UNIT=%i
Environment=STATE_DIR=/var/lib/misaka/%i
# Environment=RESTART_ON_STALL=0   # first hours of a fresh chain: warn on STALLED, restart on WEDGED
# RPC=127.0.0.1:27311   # set when the unit's wRPC port is not the network default
ExecStart=/usr/local/bin/misaka-node-liveness-probe
```

`/etc/systemd/system/misaka-liveness@.timer`:

```ini
[Unit]
Description=Probe %i every minute

[Timer]
OnBootSec=3min
OnUnitActiveSec=1min
AccuracySec=10s

[Install]
WantedBy=timers.target
```

```bash
systemctl daemon-reload
systemctl enable --now misaka-liveness@misaka-t11-node.timer
journalctl -t misaka-liveness -f
```

`RESTART_ON_STALL=0` keeps `STALLED` a warning (still logged) while `WEDGED` restarts — use it
for the first hours of a fresh chain, where silence is designed (nobody produces during the
first artifact map; a floor at genesis bits is hours per block), then remove it.

Tune `STALL_SECS` to several block intervals of the network (testnet-11 at 120 s: 900 s), and
`TIMEOUT` above the node's normal RPC latency under load (15 s). A node that is syncing (IBD)
advances its DAA and reads as alive.

## What it does not do

It does not diagnose the hang. Keep an unstripped copy of every deployed binary
(`[profile.release] debug = 1`, `strip = false`) so the next thread dump has symbols, and read
`docs/adr/0075-certification-is-a-consensus-object.md` §7 for why the public entry node should
carry no PALW duties.
