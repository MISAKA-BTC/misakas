# T12 handshake observations

Connected peers and rejection log events are different data sources. The Peers page continues
to union its actual RPC peer lists by node identity, with IP grouping for lookup. It adds no
connected IP and does not change a fork-id gate. One IP/NAT can have successful and refused
sessions; rejection rows therefore retain direction, source port and mismatch category even
when that IP is currently connected at another vantage.

The 113 census unit had no override and read the stopped t11 path
`/root/.t11/misaka-testnet-11/logs/rusty-kaspa.log`. Its freshly generated empty JSON was not
an observation of current t12 handshakes. `peer-census.py` instead defaults to the verified
running b6 log `/root/.t12r-b6/misaka-testnet-12/logs/rusty-kaspa.log` and unit
`misaka-t12-node`. It validates the running PID's appdir and ownership of the local RPC
listener, RPC testnet-12/genesis/params/fences, and the log's timestamps. Missing, stale,
unparsed or mismatched sources publish `available=false`, not an authoritative zero.
A valid source can report no observed events in its tail, with incomplete window coverage
explicitly marked. A 32 MiB read cap prevents an unbounded history scan.

`peer-census-rpc.mjs` uses Node20 `--experimental-websocket` to read getInfo,
getBlockDagInfo and getPalwNodeStatus from localhost26314 in a bounded10s snapshot. It reads
no headers, transaction contents or private files. The Python log reader reads only the
configured log and runtime proc metadata; it never reads keys, answers, materials or DBs.

Actual current next-fence text and unknown-fired-set text are parsed independently of
which side rejected. In a received reject, the quoted DAA/crossed-fence belongs to the
remote rejecting node. Neither mismatch is automatically labelled an outdated binary.
The UI shows current RPC network/DAA/params/schedule/fences and identity separately; a
semver label or a successful session is not an attestation of a git revision or majority.

## Tests and deployment

```sh
python3 contrib/misakascan-t12/test/peer_census_test.py
node --test contrib/misakascan-t12/test/peer-census.test.mjs
```

The log fixture retains actual2026-11-01 t12 error text with its IP replaced by a TEST-NET
address. It covers nextMAX versus18640 and a remote19019/18400 unknown-fired-set reject.
The named CurrentNextFenceMismatch case is a synthetic diagnostic compatibility control.
Other fixtures cover invalid/stale sources, empty valid tails and mixed sessions behind an IP.

Parent review/deployment is separate. Back up current script, service/drop-ins and public
JSON. Install both scripts beside each other under `/usr/local/libexec/`, then review/install
`peer-census-t12.conf` as a drop-in for the existing census oneshot. Its existing two-minute
timer can remain. Validate first with CENSUS_OUT pointed to a temporary JSON, compare the
source P2P identity and runtime values, then publish atomically. The frontend candidate must
be merged against the actual LIVE baseline; never overwrite unrelated site fixes.
Rollback only this drop-in/scripts/public JSON; no node restart, hold release, DB mutation,
handshake relaxation or new peer connection is involved.
