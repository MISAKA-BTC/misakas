# A3 — Hello2 prepared CPU: Root integration evidence

Hello2 source sequence: `31739101d` (codec/quota), `33f01a1c8` (CPU),
`30b22359f` (session), `451f50338` (tests), `1b14d1ecc` (admission-owner
documentation), then test-only fix `c99125eb8`. Source-review evidence is
`9c4e21f56`. This remains the prepared resident CPU subset described in
[the API/ownership contract](A3-hello2-prepared-cpu.md).

## Executed scope

Root candidate **`7a6935de6add7fe66deb324090a7144b974a8730`** successfully ran this
37-test selection (30 plus producer7) with feature `node`, default stack, two jobs, debug0, no incremental
compilation, Core dev/test opt0 and the existing static RocksDB archive:

| Target | Passed |
|---|---:|
| `cell_ipc_v2` | 6 |
| `cell` | 12 |
| `capture_codec` | 4 |
| `wide_capture_v2` | 7 |
| `run_wide_memory` | 1 |
| `run_wide_q24_v2` | 7 |
| Total | 37 |

The one ignored OS-child helper is not another test success. The actual-child
test is among the six executed Hello2 tests. It checks exact i64 rows/committed
values/carry/state/history data and both Hello1/Hello2 rejection directions.
The selected wide producer target `run_wide_q24_v2` has seven separate tests;
those also passed on this same Root candidate and are not confused with the one
host-memory test. [Its separate raw log and binary pin](A3-r1-evidence/hello2-root7a693-corrected/root-producer.json)
record that additional target. Earlier 18/49-test runs and other roles are not added.

[Copied raw batch and selected Root metadata](A3-r1-evidence/hello2-root7a693-corrected/root-batch.json)
pin the Root source, command, original metadata snapshot hash, source-file hashes,
five actual test-binary hashes, raw log, archive identity and stable source/archive
checks. A3 kept its Cargo hold; this is **Root integration execution**, not an
A3 standalone frozen-branch run. No strict Clippy result for this source is yet
asserted here. The source-only documentation checker reported 43 known missing
references and zero new issues; that is not a strict clean audit.

## Initial failure and correction

The initial Root `9795a6038` batch passed old capture4, old cell12 and five Hello2
tests, then failed the actual-child test. The parent immediately returned
`Resource temporarily unavailable (os error35)` with accepted read0/write13 in
0.02 seconds. Its nonblocking accept listener had not explicitly reset the
accepted stream's blocking mode. Parent Drop then caused the child's secondary
EOF/steps0-versus7 assertion.

The four-line test-only `c99125eb8` adds `set_nonblocking(false)` before the
existing ten-second read/write timeouts. It preserves the fifteen-second accept/
finish limits and ChildGuard cleanup, and changes no production protocol, CPU,
wire byte, quota or deadline policy. Independent source review found no further
blocker; the corrected Root batch above passed the actual-child test.
[The original raw failure is retained](A3-r1-evidence/hello2-root9795-initial/failure.json).
Root retains the original failed binary and its own complete evidence.

## Completion boundary

This result covers explicit Hello2 against an independently prepared resident
CPU plan. Direct CPU methods require caller work/step admission; session owns
the cumulative connection budget. Trusted unmodified compiled-plan fields and
original resident-source identity remain caller preconditions. Returned raw
data supplies no arithmetic acceptance, native role, funding, Gate C,
SecurityCertified or computation credit.

Generic artifact startup/program decoding, startup owner reservations, verified
source lifetime and generic process/SDK/native integration remain required.
Separate branch `codex/a3-wide-artifact-startup-v2` contains the new bounded
header source `e49e5628a`, still uncompiled/unexecuted. Its tests and draft child
owner do not receive success credit from this Hello2 batch.
