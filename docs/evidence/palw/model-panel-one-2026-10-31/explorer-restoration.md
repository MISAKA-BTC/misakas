# misakascan backend restoration — 2026-10-31

At 12:51 UTC the public `/kaspa` WebSocket read the local b6 node on
169.58.232.113. It returned `isSynced=false`, DAA 18,576 and sink
`c1d33ff2…c4b8a4`, matching the user's stalled overview. The node's headers were
newer than its virtual chain. Changing displayed counts would not repair that
state.

The replacement relay on 95.111.236.186 was verified through a private tunnel:
`isSynced=true`, the same testnet-12 genesis, current params `edf47229…`, schedule
`4024a6e7…`, and DAA 18,639 then 18,640. The public explorer's `/kaspa` and
`/kaspa-seed` upstream was changed at 12:55:54 UTC to that node. No node binary,
node database, node configuration or UI source content was changed by this
explorer repair. The public `/kaspa-hub` retains b6 as a separate peer vantage.

The dedicated key was generated on the explorer host. The user's SSH key was
not copied. On relay95 the new key is limited to the explorer's source IP,
port forwarding, and `permitopen` for loopback JSON 26314 and gRPC 26312; command
execution is disabled. The tunnel binds only to explorer-host loopback ports
38314 and 38312. Unit:
`misakascan-relay95-g14-tunnel.service`.

The existing DB filler and REST server now use gRPC 38312 via their new
`zzzzz-g14-relay95-18640.conf` drop-ins. Their SQL connection settings and DB
were retained. The filler had repeatedly failed because its unsynced backend
could not supply a usable client. It resumed with PID 1211272, automatically
re-anchored the old c1d33ff2 checkpoint at the genuine common ancestor c70c05e2,
and indexed two current DAG tips. These are ordinary indexer updates; no DB
reset, deletion or manual checkpoint fabrication was used. Public REST health
reports `isSynced=true` and the network endpoint reports DAA 18,640.

The app's existing 60-second ETag watcher reconnects long-open browser sessions.
Only app.js's timestamp was advanced to trigger it: its SHA-256 remained
`3d3b15b582c59e027d5e870a640285080198b48c1bb30da7133e92aa4b8948b5`.

## Observations and limit

Public WebSocket probes at 12:56:02 and 12:57:12 UTC both returned the new synced
DAA 18,640 / sink d7cb4a13 chain. The actual public browser view was also
verified to show Block count / DAA 18,640, headers 18,640 and node connected. The stale 18,576 connection is repaired.
The same public WebSocket then advanced to DAA **18,641 at 13:11:15 UTC**
and **18,642 at 13:13:21 UTC**, preserving synced status, params, schedule
and genesis. The already-open browser view independently updated to Block count /
DAA / headers 18,642, sink 7273a524…fec2be; its lane window showed 599 work
blocks and one ordinary heartbeat. Thus the actual public chain and view advanced
twice after the migration fence; the earlier two equal probes alone were not
evidence of that progress.

The public arming read at next DAA 18,641 reports `g14ModelPanelOne` armed,
all models, one seat, whole-claim scope, and no model review. The historical
`legacyPanelAttempt` row has `newClaims=false`. This establishes the backend's
active route policy; it does not establish a real model claim's acceptance,
receipt, Final, or SecurityCertified credit.

Raw compact observations: [explorer-relay95-restoration.json](explorer-relay95-restoration.json).

## Reversible configuration

Remote backups and timestamp metadata:
`/root/misaka-incident-20261031/explorer-relay95-18640-backup`.
The switch report is `/root/misaka-incident-20261031/explorer-relay95-switch.json`.
Relay95 retains `/root/.ssh/authorized_keys.before-explorer-g14-18640`.

Rollback restores the saved nginx file, validates it with `nginx -t`, reloads,
moves the two new environment drop-ins aside, reloads systemd and restarts the
filler/REST. Decommissioning the tunnel also disables its unit and removes only
its uniquely named authorized-key line, preserving later unrelated key changes.
Do not switch back to b6 until its real chain and synchronization are verified.
