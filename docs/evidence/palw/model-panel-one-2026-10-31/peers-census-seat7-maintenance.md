# Census maintenance observer — 2026-11-01

At16:28Z the public census correctly reported unavailable: b6's systemd MainPID was0
while its old process was draining. This agent did not operate that process or guardian.
B7 remained the running unit `misaka-t12-seat7`, PID1274445, with actual appdir
`/root/.t12r-b7` and owned JSON listener127.0.0.1:26354.

The initial b7 dryrun validated its runtime/log binding but exposed a hardcoded b6 label.
It was not deployed. After parent approval, the minimum Python change generates only the
113 unit label from the same CENSUS_UNIT used for actual binding validation. The default
b6 string remains byte-identical. `source.unit` is additive when that binding is verified;
it is not an RPC-supplied revision or unit-name assertion. No host generalization was added.

Python7/7 and existing JS20/20 passed. The added cases use actual b6 and b7 runtime payloads
and check label/unit alignment, preserved false sync status, default-label compatibility,
and unavailable output for an unbound or invalid source. Actual b7 temporary OUT then
confirmed its unit, identity and fresh log with available=true.

At16:35:13Z, exact-SHA guards and backups preceded atomic Python replacement and installation
of only `95-maintenance-seat7-observer.conf`. Its three ENV overrides select b7 unit, log
and RPC. The original90 drop-in remained SHA238a01f5. The census oneshot succeeded and the
existing timer resumed active/enabled. No frontend, node, DB, key, hold, guardian, nginx or
REST changes were made. The backup is under
`/root/misaka-incident-20261031/census-seat7-maintenance-20261101/backup`.

The external check at16:35:55Z found available=true, testnet-12, source unitmisaka-t12-seat7,
P2P ID944ecf45-6198-4f0d-8141-af1756851db6, DAA18936, EDF/4024, and isSynced=false. The exact
unchanged public app SHA862c49ff rendered the b7 label and “No rejection events observed in
the available log tail.” The observed count was0 for this source's tail. It is not comparable
to historical b6 event groups and does not imply sync, chain recovery, Qwen admission or Final.

[Manifest](peers-census-seat7-maintenance.json) contains full hashes and runtime facts.
A manual failback candidate is saved at `/tmp/misaka-chain-20261101/failback-census-to-b6.py`
and has not been executed. It refuses Python/base90/own95 drift, then validates live b6
PID/listener/log via temporary OUT before removing only the exact-SHA own95 override.
It keeps the new Python, whose default b6 label is unchanged, and keeps the original90 config.
After reload/oneshot it verifies the b6 source identity before restoring the active timer.
Its failure path restores only its saved95 override/feed. It never operates a node unit.
