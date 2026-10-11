# RFC12 private process/P2P driver

`scripts/misaka-rfc12-multiprocess.py` executes a pinned private plan through actual
wRPC JSON and Ethereum HTTP reads. It owns three processes and distinct initially
empty app directories. Fork and partition traffic uses opaque loopback TCP links:
the production P2P protocol exchanges all headers, bodies, proofs and sidecars.
The driver never copies DBs/state between nodes, rewrites a parameter file, or
permits a dormant fence to open. A compatible private test binary and genuine
signed-work fixture commands are external prerequisites. Current shipping presets
keep retirement `None`; observing them produces **BLOCKED**, not acceptance.

Run with Python 3.9+ (no third-party dependencies):

```sh
python3 scripts/misaka-rfc12-multiprocess.py --plan private-plan.json --out evidence.json
```

The plan must be prepared and fixed **before** the run. Its SHA-256 is recorded.
Required fields:

| Field | Contract |
| --- | --- |
| `ruleset_id`, `policy_id` | Actual private binary fingerprints; unapproved production D/W values must not be guessed. |
| `minimum_depth`, `minimum_work` | Integer depth and decimal integer unique work; both positive, declared before measurement. |
| `minimum_isolated_daa_span` | At least 5,400 genuine DAA per isolated branch. A clock-stopped short fixture cannot pass. |
| `minimum_claims` | At least 100,000 actual state claims, observed through `getPalwCapacityShadow`; declared synthetic counts are not accepted. |
| `maximum_probe_ms` | Fixed latency acceptance bound. It includes actual public state/roots/readiness/capacity reads. |
| `fork_winner`, `partition_winner` | `A` or `B`, fixed before the corresponding run; the harness cannot pick a winner after seeing a result. |
| `nodes` | Exactly `A`, `B`, `JOIN`; each has `argv` (array), distinct loopback `wrpc`/`p2p` addresses, and loopback `eth` HTTP URL. |
| `fixtures` | Arrays of finite executable argv commands for `bootstrap`, `fork`, `partition`, `prune`. They create legitimate signed work and quiesce production before assertions. No shell is used. |
| Timeouts | Optional `command_timeout_s`, `probe_timeout_s`, `scenario_timeout_s`; evidence preserves timeout failures. |

Node argv must include `--nodnsseed`, `--outpeers=0`, and an appdir using the
role's `{A_APPDIR}`, `{B_APPDIR}` or `{JOIN_APPDIR}`. No `--connect`/`--addpeer`
is permitted: only driver-owned links can carry traffic. Fixture argv can use
`{ROOT}`, `{A_WRPC}`, `{B_WRPC}`, `{JOIN_WRPC}`, and (for the pruning phase)
`{OLD_BLOCK}`. A role's optional `grpc` field also supplies `{A_GRPC}` etc.
Ports and private fixture keys belong to the test plan; they are not production
network recommendations. Existing salted drill key/domain protections remain.
The driver supplies no activation argument and no model-acquisition gate.

The run performs isolated, mature fork selection; transport partition/heal;
actual old-block-body garbage collection; an empty-DB pruned P2P join;
and forced owned-child kill plus cold restart. It compares complete L1 header
commitments (UTXO, PALW, overlay, EVM), native generation/policy/readiness and
ETH latest/safe/finalized state roots across nodes. ETH block IDs use the actual
adapter's prescribed first 32 bytes of the native 64-byte block hash; complete
native identifiers are still compared through wRPC. No missing data, null
certificate, empty history, merely high DAA, or heartbeat/floor count substitutes
for native mature-work coverage. Capacity queries use their own connection and
a final generation check, because the expensive query is terminal on a stream.

Artifacts and node logs persist under the reported temporary directory for
review, including failures. Only processes/sockets created by the driver are
killed or closed. `SCENARIOS_PASS` means those measured mechanics passed under
that plan. **`fullRfc12SecurityApproval` remains false**: common-prefix analysis,
class soundness/economics, actor independence, D/W/concentration justification,
full bridge fill/withdrawal/backing matrix, in-flight IBD crash fault injection,
and authorization of live monetary/legacy-horizon policy are separate conditions.
This driver is not yet a measured full private network result.

Five oracle/socket controls are verified with:

```sh
python3 -m unittest discover -s scripts/tests -p test_rfc12_multiprocess.py -v
```

They reject dormant retirement, live DNS authority, stale generations/readiness,
and a one-unit-short u128 work floor. The pruning oracle recognizes successful
header-only RPC replies as missing bodies and refuses to call transport errors
GC evidence; the socket control checks established
transport cut and subsequent reconnection. These five tests are not node,
pruning, P2P-protocol, release100k, or soundness evidence.
