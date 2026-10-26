# Lane TNODE report (successor) — 2026-10-26

Branch `claude/g14-tnode2` = predecessor c8f02297b rebased on d8aded6de + c335f4e4e (12 GB RSS watchdog in `scripts/drills/g14_tnode_slice.py`).

## T01–T15 on real processes (driver `scripts/drills/g14_tnode_slice.py`, over `g14_t12_16000_fence_drill.run` with `crossing=kernel-slice`)

| T | scenario / roles | how shown | result at c335f4e4e |
|---|---|---|---|
| T01 | honest P + Panel=1 seat + outsider O as wrong challenger | real process needed (node KernelClaim producer + wrong-challenge filer missing); in-process canonical/spine.rs, cert.rs | NOT-IMPLEMENTED |
| T02 | P silent forever, O/relay see default | real process + CLOCK deadlines | NOT-IMPLEMENTED (and CLOCK) |
| T03 | forged seat signature via wRPC | in-process canonical/cert.rs; no node injection flag | NOT-IMPLEMENTED |
| T04 | flood driver vs P and relay | no native-intake flood driver | NOT-IMPLEMENTED |
| T05 | colluding P+seat, one outsider convicts | collusion drill covers legacy route only | NOT-IMPLEMENTED |
| T06 | last notice / latest response vs hard deadline | CLOCK lane | WAITING-INTEGRATION |
| T07 | -1/=/+1 at native deadlines | CLOCK lane | WAITING-INTEGRATION |
| T08 | P offline, distinct signer, relay | no kernel-route court-signer flag | NOT-IMPLEMENTED |
| T09 | leases, shared bond | CLOCK (`palw_provider_court_v1`) + bond budget values | WAITING-INTEGRATION |
| T10 | fold-only lie | in-process only | NOT-IMPLEMENTED (node) |
| T11 | 1..10,000 pursuits | needs claim volume on node | NOT-IMPLEMENTED |
| T12 | hard-boundary concentration | CLOCK + F02 | WAITING-INTEGRATION |
| T13 | oldest claim starves | needs kernel claims on node | NOT-IMPLEMENTED |
| T14 | partition/reorg across H, restart, fresh relay IBD (fence F2/F3/F4/F7) | real processes | BLOCKED: slice ruleset refused |
| T15 | boundary F0/F1/F5/F6, shipped refusal | real processes on release build | BLOCKED: slice ruleset refused |

Blocker: `PALW_T12_KERNEL_SLICE_FENCES_V1` arms `palw_settlement_once_v1` without `palw_bond_budget_v1`; V2 validation refuses
("palw_settlement_once_v1 requires the ADR-0176 bond budget fence at or below it"). No node started. Evidence:
`docs/evidence/palw/g14-17000/tnode/`.

Binary sha256: private-node 143b7f2a…c867, kaspad d4f6d9c5…bed8, misaka 6be1a4fa…a343 (full values in evidence README).

## Re-run after integration (one command)
```
BUILDSLOT_SINCE=1760000000 CARGO_INCREMENTAL=0 TMPDIR=/private/tmp ~/Downloads/MISAKA-wt-b/buildslot.sh env CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo build --release -p kaspad -p misaka-cli --features kaspad/private-integration-config \
 && python3 scripts/drills/g14_tnode_slice.py --salt $(openssl rand -hex 32) --at 60 --work <scratch>/work --out-dir <scratch>/out
```

## Requests to the coordinator
1. CLOCK: add `palw_bond_budget_v1` (at or below settlement-once) to the slice list, or drop settlement-once from the slice.
2. Node-side kernel-route producer/challenger/forgery hooks are needed before T01–T05, T08, T10, T11, T13 can be real-process PASS; requirement #1 cannot pass until then.
