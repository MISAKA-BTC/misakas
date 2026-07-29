# systemd wrappers for the PALW closed-testnet harness (§5.3)

These units wrap the **host-local agent** (`palw-node-agent.sh`) so a node host
gets boot ordering and `systemctl` ergonomics without changing who supervises
the process (the agent's pid/argv/start-time records under `$PALW_DATA_ROOT`).

| Unit | What it runs |
|---|---|
| `palw-node@<a\|b>-<bootstrap\|validator\|miner>.service` | `palw-node-agent.sh start/stop <node> <mode>` |
| `palw-roll-lifecycle.timer` | Keeps a successor batch in flight, switches node A after it becomes active, and verifies a new algo-4 block |

On a host that also mines for another validator, set
`PALW_AUX_MINER_UNITS` and `PALW_POST_DA_MINER_UNITS` in `env.local` to a
space-separated list of its miner units. The roller stops them while it needs
an exact DAA for DA challenge/response, then resumes them to carry each
validator's attestation and beacon transactions during the activation wait.

## Install (on each node host)

```sh
sudo install -d /opt/palw
sudo cp -R /path/to/MisakaLLM-palw-shared /opt/palw/repo       # or a release tarball
# host config: /opt/palw/repo/scripts/palw-shared-testnet/env.local (PALW_DATA_ROOT etc.)
sudo cp palw-node@.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now palw-node@a-validator
```

If the harness lives elsewhere, override `PALW_DIR`:

```sh
sudo systemctl edit palw-node@a-validator
# [Service]
# Environment=PALW_DIR=/your/path/scripts/palw-shared-testnet
```

## Honest limitations (read before relying on these)

* `Type=oneshot + RemainAfterExit` — systemd tracks the UNIT, not the kaspad
  process. If kaspad crashes, systemd will NOT restart it; health lives in
  `palw-node-agent.sh status` (poll it from a timer or your monitoring, and
  `restart <node> <mode> --force` recovers). A native foreground `Type=exec`
  unit needs the kaspad argv builder extracted from `node-a.sh` (review doc
  §10.2's `build_node_a_common_args`) — a Phase-B follow-up.
* macOS has no systemd; on the current single-host dev box the agent alone is
  the supported path (these units are for Linux node hosts in a shared net).
* Secrets stay host-local (`$PALW_DATA_ROOT/keys`, 0600) exactly as with the
  bare agent; the units add no new secret surface.
