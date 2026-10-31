# DAA 18,660 pruning stall — 2026-10-31

The public relay (95.111.236.186, process 1305257, immutable source
8cac52ec4ed15fb4ea982d46235a5c3b5f78e4cc) remained at virtual DAA 18,660
while node b7 continued generating ordinary heartbeats privately. Connected
peer pings and the cached DAG RPC continued to answer; PALW RPCs and getInfo
timed out. This was not evidence that the heartbeat worker had stopped.

The relay advanced its pruning point and finished its UTXO commitment check
at 13:51:06.512 UTC. Its exact-point PALW snapshot finished only at
14:37:30.570 UTC: 46 minutes 24 seconds later. The process was not restarted.
After the snapshot and pruning completed, the relay and b6 both reported
DAA 18,668 with sink 649650c6f0fb07d1dfa4e44af0b4d9782fabef54fd104ea11719a952acc351784f45461f3c1a05aa92843e943b60aab2b79e921d1235ad08f7ff0744cf7afb60
at 14:39 UTC, and DAA 18,669 at 14:40 UTC. getInfo then answered isSynced=true.
The attached raw log uses the host's +01:00 timestamp.

A two-second /proc thread CPU sample found the pruning-process thread
1305436 consuming one CPU continuously. Two short perf samples contained
allocation/copy-heavy work. The deployed binary was stripped, so these
profiles do not identify a Rust function or prove an infinite loop. The
natural completion demonstrates a long finite operation instead.

The capture implementation held palw_state_v2_store.read() throughout
materializing the tip and checked replay of every historical delta back to
the pruning point. Each delta retains its native resource reservation and
uses the existing checked state transition. A queued virtual-state writer
waits behind that long read, and subsequent PALW readers can wait behind the
writer. This explains why a small RPC request can be held by historical
snapshot reconstruction.

The targeted repair detaches the database reader before reconstruction,
retains the existing checked replay and exact-serving requirements, checks
shutdown/import/pruning context, and serializes fresh snapshot publication.
It does not shorten liability windows, change claim approval, alter G14
seat selection, delete an operator database, or replace verification with
local replay. Each checked delta retains its original computation cost. When a validated
prior checkpoint exists, the shorter actual path is chosen before loading
state; an absent checkpoint keeps the tip fallback. Live checkpoint-row
availability and wall-clock speedup have not yet been measured.

Restored heartbeat progress is separate from the real-model completion
criterion. At the time of this incident record, a new public Qwen claim's
acceptance, panel=1, selected-seat approval and Final have not been verified.

## Startup recovery and Qwen submission

At 14:49 UTC, /proc task inventories on IBM b0 (PID 3525795) and b1
(PID 3522496) contained the allocated block/virtual pools but none of the
header, body, virtual or pruning processor workers. The same inventory on
b3 (PID 3484183) contained all four. Consensus::run_processors performs
interrupted-pruning recovery synchronously before spawning those workers.
A Qwen producer can consequently finish execution and await a submitted
block that no worker can yet validate. It is incorrect to diagnose this as
an incomplete inference merely from producerState=drawing.

The existing transitional-state API now keeps local participation and RPC
sync reporting closed until the actual workers have been spawned. Existing
import flags remain authoritative afterward, and exiting closes readiness.
This introduces no consensus-core API, schema or wire change. The cold
recovery is still completed in its required order before workers start.
The current live b0/b1 recovery was not restarted for this change.

The producer also logs each non-floor block hash, claim ID and template DAA
before its submission wait, explicitly identifying validation and public
acceptance as pending. A local submit log is not public acceptance or Final.

## Verification

- Snapshot readiness and detached/shorter-path reconstruction: 9 passed.
- Actual worker startup, incomplete-import hold and shutdown readiness: 1 passed.
- Owned-state caller/materialization guard: 1 passed.
- Documentation check: 0 new reference issues, 43 known baseline issues retained.

The command record and validated source hashes accompany the raw logs. The
snapshot tests exercise exact state/root equality, concurrent writer access,
mid-walk cancellation, same-point import-epoch replacement, missing/corrupt
source refusal and cold reuse after partial pruning. The shorter-path test
removes an irrelevant far-tip delta and still derives the exact target state
from a verified retained checkpoint. No public-model success is inferred
from those fixtures. Runtime distribution and real-model acceptance are
recorded separately after the new immutable build exists.
