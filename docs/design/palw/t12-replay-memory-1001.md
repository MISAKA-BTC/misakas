# testnet-12 replay memory, 2026-10-01 — what a PALW replay reserves, and int-10.2

Status: node-only release `rcore/int-10-p2` (lane M1), off int-10.1 (`8712ef00d`). The t12 consensus fingerprint does not move
(`scripts/t12-repin.sh --drift-only`). Companion to `t12-panel-backlog-1001.md` (lane P's root cause; F1–F4 are int-10.1).

## 1. What was measured (5.104, 10-01 ~21:25 JST)

5.104 = 8 cores / 24 GiB, five seats (b2 b3 b4 b5 b7), each `--palw-host-memory-share` 3.5 GiB, int-10.1 with
`--palw-seat-replay-slots=1`. A live deferral line on seat4 (the per-duty pre-check, `replay_memory_budget_v1`):

    a replay needs 3.37 GiB as full-seat (artifact 1.68 GiB + K/V 0.03 GiB at 1024 rows + attention scratch 0.00 GiB
    + trace scratch 1.67 GiB + capture 0.00 GiB (Fold { retain_level: 12 }) + checkpoint leg 0.00 GiB) under A16-KV-i16
    and the host memory ledger cannot cover 3.37 GiB: 0.09 GiB available (declared share 3.50 GiB, host headroom 5.29 GiB,
    less the readiness proof lane's 0.03 GiB, …

and the floor replays behind it: `a replay needs 0.50 GiB … less 3.37 GiB already reserved by full-seat of class ebf44d0a…`.
`smaps_rollup` per seat: Rss 5.0–6.2 G, Pss 3.6–4.8 G, **Shared_Clean 1.68 G in every seat** (the mapped `.palwart`: one
page-cache copy shared by all five), Private_Dirty 2.7–3.5 G, Anonymous 3.3–4.5 G, LazyFree 0.6–1.4 G, Swap 0.6–1.8 G; seat4's
largest mapping is the `.palwart` (1,733,304 KiB Rss), then the mimalloc arenas.

So the 3.37 GiB an 8k replay reserved is two terms, neither of which is the replay's own state (K/V is 0.03 GiB):

| term | GiB | what it is | why it is wrong to reserve per replay |
| --- | --- | --- | --- |
| artifact | 1.68 | the file's size (`holding_replay_bytes_v1`) | the pages are ONE shared page-cache copy, already resident, and charged five times on 5.104 (once per seat per replay) |
| trace scratch | 1.67 | `prefill_run_positions` (64) × one position's committed trace at the widest history (≈ 9.3 MB at 1,024 rows) × `PALW_PREFILL_RUN_COPIES_V1` (3) | a node-local choice of width; every width commits the same bits |

And the shared copy was not safe where it sat: under the host's pressure the kernel evicted it and replays refaulted it 4 KiB at a
time at 6–11 MB/s (ADR-0112 §1, `misaka-palw-base0/src/mmap.rs`; 845 MB/s for large reads) — the minutes 5.104's replays spent.

## 2. How each holding is held (CP1)

| holding | how kaspad opens it | file-backed (shared on the host) | anonymous (per process) | `incremental_replay_bytes_v1` before int-10.2 |
| --- | --- | --- | --- | --- |
| dense A16 `.palwart` (the 8k class) | `DenseLineageV1::load` → `ReadOnlyMap` (`PROT_READ`, `MAP_PRIVATE`, whole file) → `decode_artifact_file_mapped_v1` | the int8 slabs, `Int8SlabV1::Mapped`: **1,694.6 of 1,716.0 MiB** (walked on the real 8k file) | the a16 store 17.35 MiB (`.to_vec()` at decode), the rotary table 4.0 MiB, small tables; ~20 MiB of `A16Engine` parameter tables per execution; a per-resolve clone of the owned part (`dense_artifacts`) | the file (1.68 GiB) |
| dense, platform that cannot map | `decode_artifact_file_v1` (owned) | — | the whole file | the file |
| IR container (`PALWTIR1`, Phase H) | `TirArtifactV1::open`: `MappedFile` (`PROT_READ`, `MAP_PRIVATE`), params `Cow::Borrowed` in place (64-byte aligned, LE); root streamed at load | all params | the plan, the param index; the inventory tree on the first court opening | the file (no resource profile: + the 0.5 GiB estimate) |
| Qwen3.6 `.palwq36` under ADR-0112 | `open_artifact_with_residency`: `ReadOnlyMap` for the header and embedding rows; the always-set and the routed-expert LRU `pread` into OWNED buffers | the embedding rows touched | the residency budget (always-set ~1.86 GiB + experts), per process, not shared | 0 (the budget is counted at load) |
| the floor (derived) | minted from the seed | — | the derived artifact | 0 |

The comment at `qwen25_a16_backend.rs` (`artifact_read_probe_v1`, ADR-0077 SA-6: "this tier's artifact is owned memory … no mapped
page can fault under a job") and `lineage.rs` ("owned whole") predate "Mapped, not read" in `DenseLineageV1::load`: on a POSIX node
the weights are a mapping, they are refaulted under pressure, and they are pinned when the policy allows (int-10.2). Both comments
now say so; the probe still answers `Ok`, and its doc names the remaining gap (a file truncated in place under the mapping).

**Who reserves what** (all through `role_memory_need_for_backend_or_chain_v1` on the very backend instance that then executes, so
the figure is the instance's): producer (`producer`), SEAT-R full seat (`full-seat`: attempt, free prompt, the legacy path), SEAT-S4
partial seat (`partial-seat`, streamed fold), court close (`court`), interval seat (`interval-seat`), DA answer (`da-answer`), S3
sampler (`s3-sampler`), whole-capture sampling (`full-seat capture`), ADR-0160 audit and operator DA (`operator-da`), J1 probe
(`j1-probe`), replay filer (`replay-filer`, `replay-bisect`, `held-dissection`), held evidence (`held-evidence`, bytes) — each the
holding's incremental bytes + the role's working set. The readiness proof lane reserves its 32 MiB carve and asks
`capacity_admits(full-seat need)`. The pre-check before a duty (`replay_memory_budget_v1`) is a dry run of the full seat.

## 3. A1 — shared weights pinned, counted once

**Decision.** Each class artifact whose replay reads a whole-file mapping in place — a dense `.palwart` the lineage mapped, an IR
container — is pinned at load: its own `PROT_READ | MAP_SHARED` mapping, the file read into the page cache by 8 MiB reads, then
`mlock` (`misaka_palw_base0::mmap::PinnedFileMapV1`; `kaspad/src/palw_artifact_pin.rs`). A pinned holding is already resident:
`incremental_replay_bytes_v1` answers 0 for it (as for a Qwen3.6 residency), so no replay of its class reserves its bytes and no
duty's share is charged for them; they are reported beside every need (`PalwRoleMemoryNeedV1::pinned_bytes`, "1.68 GiB pinned:
resident once on this host, not reserved").

**Why a mapping of its own, and why `MAP_SHARED`.** The lineages' mappings are `MAP_PRIVATE`. For a mapping nobody writes, private
and shared map the same page-cache pages on Linux — but only the shared one says so: a private page that is ever written, or locked
while writable, becomes the process's anonymous copy, the per-seat copy the pin exists to avoid. Measured on macOS (this Mac,
2026-10-01): a locked 256 MB private view costs the process 257 MB of footprint (the kernel resolves the copy at wire time), the
same file locked through a shared view 1 MB. Its own mapping also makes the pin's life the node's policy (flag, cap, eviction) and
leaves the lineages, their decoders and their mappings untouched. A page is unevictable while ANY mapping of it is locked, so the
holding's private view of the same pages is protected too.

**Why before the lineage maps the file.** (1) The cold start: the dense decoder's digest pass faults the private mapping 4 KiB at a
time (1.7 GiB at 6–11 MB/s on the fleet's disks is minutes); the pin's large reads bring the file in first and the decoder reads a
resident page cache. (2) macOS refuses `mlock` of a shared view with `EPERM` once any process has faulted the file's pages through a
private mapping — in the same process and across processes (measured) — while a shared view locked first stays locked and lets
private views read beside it. So `load_class_holdings_v1` calls `prepin_file_v1` (the lineage is read from the file's magic,
`PalwClassSdk::lineage_id_for_file_v1`) before `load_artifact_with`, and `settle_prepin_v1` hands the lock to the holding the lineage
built — only when it loaded, is a whole-file mapping read in place (`palw_pin_eligibility_v1`: a dense holding whose slabs are
`Mapped`, an IR container), and the file at the path is still, by identity (device, inode, size, mtime), the one locked. Anything
else releases the lock.

**Never pinned.** A holding with its own residency: Qwen3.6 now (its policy holds the always-set and an expert LRU in owned memory
and leaves the rest of a 33 GiB file to the page cache), and a later IR residency (lane M2) answers in `palw_pin_lineage_v1` the same
way. A derived class (no file). A dense artifact a platform decoded into owned memory.

**Every refusal keeps today's behaviour** (the replay reserves the file) and is logged with its numbers:

| refusal | rule |
| --- | --- |
| off | `--palw-no-artifact-pin` |
| cap | Σ pinned by this process + the file > `--palw-artifact-pin-max-bytes`; default **¼ of MemTotal** (24 GiB host: 6 GiB, which holds the 8k `.palwart` 1.68 + the IR container 1.74; an 8 GiB desktop: 2 GiB, the first). Per process: several seats locking one file pay for it once, so a per-process cap can be generous; the spare check below is what refuses a pin the host cannot hold now |
| no room | the bytes not yet resident (`mincore`) > what the host and this process's cgroup can spare (`host_available_bytes_v1`, the ledger's own reading, F1 credit included) − the node's 1 GiB reserve |
| replaced | the file at the path is not the one locked (identity) |
| kernel | `mlock` refused: the warning names `RLIMIT_MEMLOCK` soft/hard and the bytes already locked (raise it: `LimitMEMLOCK=infinity`, `ulimit -l unlimited`, or `CAP_IPC_LOCK`; macOS `EPERM` is the private-view refusal above) |

**Whose cgroup pays.** A page-cache page is charged to the memory cgroup of the process that first faulted it in — on a host of seats,
the first seat to load the artifact. Pinned, it stays there: in that seat's `memory.current` for good and so in its ledger's cgroup
term (`cgroup_headroom_from_v1`), lowered by the file (1.68 GiB, + 1.74 with the IR container); the other seats are charged nothing.
The periodic `[palw-host] memory` line says it beside `pinned_mib`; the kit's `check_memmax` floor already budgets every artifact
against every seat's `MemoryMax`. On a multi-seat host the charge goes to a unit of its own instead — the host pinner, which faults
the files in before any seat starts (§5): that is the configuration for 5.104 and ibm; a single-seat host (b6) and a Mac pin in
process as above.

**Reported.** The load line (`pinned class artifact …: 1.68 GiB locked in RAM …`), the periodic `[palw-host] memory` line (`class
artifacts pinned by this process: … (pinned_mib=…)`), and `getPalwNodeStatus.verification` (`pinned_mib=<MiB> pinned_files=<n>` —
new keys in the existing `key=value` string: the wire does not move). A host's arithmetic counts each distinct file ONCE, however
many seats report it (`t12check.py` says so).

**Invariants.**
1. A pinned file's pages are the page-cache pages every process on the host maps (`MAP_SHARED`, `PROT_READ`, `O_RDONLY`): no lock makes a copy.
2. A holding counts as pinned only through its own payload (`holding_is_pinned_v1`, a `Weak` to it): a file re-minted under the same name is another holding, unpinned until settled; the need memo keys each holding on its pinned bytes, so no figure computed before a pin answers after it.
3. A pin and its holding are the same file by identity, or there is no pin.
4. Eviction (`evict_held_artifacts_v1`) releases the pin with the holding.
5. Pinning is node policy: an unarmed process (a tool, a test) pins nothing; nothing here is read by consensus.

## 4. A2 — the prefill run width chosen from the budget

**Decision.** The dense engine's prefill run width (`A16_PREFILL_RUN_POSITIONS` = 64, the fastest measured: stepped 36.3 ms a
position, runs of 32 16.3 ms, of 64 14.7 ms) is a node-local runtime choice. When a duty reserves memory for a replay or an attempt,
the node takes the WIDEST width in `{cap, 32, 16, 8, 4, 2, 1}` — `cap` = `--palw-prefill-run-max` (1–64, default 64) — whose derived
need the ledger grants now (`kaspad/src/palw_prefill_run.rs`, `reserve_at_widest_run_v1`), and the replay runs at exactly that width.
Narrower widths are tried only while they lower the need, so a family whose memory does not move with the width (the floor, the
hybrid, an IR class) is priced once and reserved once, as before.

**One value, from the reservation to the run.** The width is a field of the backend INSTANCE (`Qwen25A16Backend::prefill_run_positions`):
the resource profile reads it (`runtime_limits_v1(&self)` → `PalwRuntimeLimitsV1::prefill_run_positions`, the term consensus core's
profile already took as a node-local input — no derivation moved), and so do the capture loop (`a16_execute_in_storage_at_run_v1`),
the interval/segment replay engine (`A16ReplayEngineV1::run_positions`) and every other execution of the instance. The node sets it
through two defaulted trait methods (`PalwExecutionBackendV1::{prefill_run_positions_v1, set_prefill_run_positions_v1}`, the
`set_attempt_rules_v1` pattern — the only consensus-core edit), derives the need FROM the instance, reserves, and runs that same
instance. The SDK starts every backend it resolves at the node's cap (`PalwClassSdk::with_prefill_run_cap_v1`, set by
`PalwBackendRegistry::for_node_v1`).

*Race check (the coordinator's condition 4).* Every duty that reserves resolves an instance of its own — `resolve_backend` (seat
replays, the court, the interval seat, the S3 sampler, the capture sampler, SEAT-S4), the producer's, the audit's, the operator DA's,
the J1 probe's: a fresh `Box<dyn PalwExecutionBackendV1>` each, moved into its blocking task. The narrowing helper takes `&mut`, which
the one shared instance — the executor's kept backend (`executor_backend_v1`, an `Arc`) — cannot give: the DA answer, the V2 partial
resume on a borrowed backend, the replay filer and the held court keep reserving at the instance's own width (the cap) and run at it.
So the reservation and the run can never disagree.

**Where the width is chosen.** SEAT-R full seat (attempt, free prompt, the legacy path), SEAT-S4 partial seat, the court's close, the
interval seat, the S3 sampler, the whole-capture sampler, the ADR-0160 audit and operator DA, the J1 probe, and the producer. **The
producer** takes the same rule: 64 whenever its need fits (the attempt is a race and the widest width is the fastest), narrower only
where the attempt would otherwise HOLD — an attempt that holds produces nothing, one at 16 positions takes a few percent longer
(timed below).

**The pre-check and the readiness proof.** The per-duty pre-check (`replay_memory_budget_v1`) asks the NARROWEST width's need (one
position at a time): the replay's own reservation takes the widest the ledger grants when it starts, so a duty is deferred only when
not even that fits. The readiness proof's capacity (`replay_memory_capacity_v1`) asks each width from the cap down; a seat that can
meet a class only below 16 still proves it, and is logged and counted (`capacity_run_min`, `capacity_narrow_classes`) — its receipts
are the class's slow tail. The status's `full_capable` reads the narrowest width; its `working_set_bytes` stays the cap's.

**The 8k class's need per seat** (`the_8k_full_seat_need_falls_from_3_37_gib_with_the_pin_and_the_width`, derived from the t12 row):

| width | unpinned (artifact 1.68 GiB reserved) | pinned (A1) |
| --- | --- | --- |
| 64 | **3.374 GiB** (the 10-01 line) | 1.699 GiB |
| 32 | 2.541 GiB | **0.865 GiB** |
| 16 | 2.124 GiB | **0.449 GiB** |
| 8 | 1.916 GiB | 0.240 GiB |
| 4 | 1.812 GiB | 0.136 GiB |
| 2 | 1.760 GiB | 0.084 GiB |
| 1 | 1.734 GiB | 0.058 GiB |

**The real 8k seat replay, timed** (CP2's request; `a_real_8k_replay_at_every_width_commits_one_set_of_roots`, env-gated): the
converted `qwen25-1.5b-a16-8k.palwart` mapped as the dense lineage maps it, testnet-12's `graph-v7@8192` row at its canonical (1023, 2),
the anchor's verdict replay (`execute_for_verdict`, SEAT-R's call) at each width in turn in one process, `RAYON_NUM_THREADS=4`, on this
Mac (10 cores) while other lanes built — load average 30 at the start, 75 at the end:

| width (order run) | time | a prefill position | working set (trace scratch) |
| --- | --- | --- | --- |
| 64 (1st) | 214.7 s | 209.9 ms | 1.698 GiB (1.667) |
| 32 (2nd) | 222.8 s | 217.8 ms | 0.865 GiB (0.833) |
| 16 (3rd) | 227.1 s | 222.0 ms | 0.448 GiB (0.417) |
| 8 (4th) | 271.5 s | 265.4 ms | 0.240 GiB (0.208) |
| 64 (5th) | 345.2 s | 337.5 ms | 1.698 GiB (1.667) |

Every width committed the same execution root, trace root, work leaves and output root (asserted); peak RSS 2.6 GiB. The host's
load is the largest term here — the same width took 61 % longer at the end than at the start — so the table bounds the width's own
cost rather than measuring it: 32 and 16 cost a few percent against the first 64 and 8 a quarter, never the 2.45× of stepping one
position at a time. ADR-0117's idle measurement of the prefill alone (graph-v5, 508 positions, 12 cores) is the other bound: 64 →
32 +11 %. So **narrowing an 8k replay from 64 to 32 costs 4–11 % of its time and halves its trace scratch**; 16 a little more.

**Reported.** A reservation the ledger narrowed is logged (once a minute per role: the need at the width taken and what the cap's
would have needed); SEAT-R's start lines print the need reserved (`… trace scratch 0.42 GiB at a prefill run of 16 …`);
`getPalwNodeStatus.verification` carries `run_cap`, `run_last`, `run_narrowed`, `capacity_run_min`, `capacity_narrow_classes`.

**Identity.** Every width commits the same bits: `the_one_pass_prefill_is_the_position_by_position_one` now walks every width a node
may choose (and the odd ones between) on v2/v5/v7, both engines; `the_backend_commits_the_same_roots_at_every_prefill_run_width`
runs the producer's attempt (roots and material byte for byte), the verdict replay, a free prompt, and every segment of the
producer's capture resumed by a seat, at 64…1; `the_instance_reserved_at_a_width_runs_at_it_and_commits_the_caps_roots` takes the
reservation through the ledger and runs the instance it left.

**Invariants.**
1. The need a duty reserves is derived from the instance that runs, at the width it runs.
2. Only an owned instance (`&mut`) is narrowed; a shared one runs and is priced at the cap.
3. No consensus figure reads the width (`the_economic_derivations_do_not_read_the_resource_profile`); every width commits the same rows and roots.

## 5. D1 — host-level coordination

**The failure.** Each node's ledger grants when its need fits its own share and `MemAvailable − 1 GiB` less ITS OWN reservations.
Five seats on 5.104 each read the same `MemAvailable` and each subtracted only their own grants: together they promised it five
times. Lane P: per-seat cgroups cannot fix that — five caps that each fit the host sum past it.

**The configuration for a multi-seat host** (the coordinator's decision on CP2). Two parts, both node policy, both set per host by
the kit: the **host pinner** — a unit and memory cgroup of its own, started before the seats, so the class artifacts' page cache
(3.41 GiB with Phase H: the 8k `.palwart` 1.68 + the IR container 1.74) is charged there and to no seat — and the **host ledger**,
so every seat's grant also needs the host's free memory less EVERY seat's reservations. The seats keep `MemoryMax=9G` and their 3.5
GiB share: F1's live axis (each seat's cgroup term, LazyFree credited) bounds each seat, the host ledger bounds their sum.
`install-5104.sh` and `install-ibm.sh` set `HOST_PINNER=1` and `HOST_LEDGER_DIR=/run/misaka-palw`. A single-seat host (b6 on .113)
and a Mac pin in process (§3) with no host ledger; `install-113.sh` says to add both lines when b7 arrives (two seats).

**The host ledger.** `--palw-host-ledger-dir=<dir>` names a ledger every kaspad on the host shares (`kaspad/src/palw_host_ledger.rs`):
a text file `host-ledger-v1` on a tmpfs, rewritten whole by rename under an exclusive `flock(2)` on `host-ledger-v1.lock` (released
by the kernel with the descriptor, so a node killed mid-write leaves no lock), never fsynced (it describes live processes only; a
writer waiting on a swapping disk would hold its node's ledger).
Its lines: `n <pid> <start> <instance> node|pinner` (who takes part), `r <pid> <start> <instance> <id> <bytes> <since> <role>` (a
grant), `p <pid> <start> <instance> <dev> <ino> <size> <mtime>` (a pinned file). The host pool's ledger
(`palw_memory_ledger::arm_host_ledger_v1`) registers every grant there under its own lock — so the two ledgers cannot disagree about
which grants exist — and a grant needs BOTH the node's own bounds (share, its live headroom, the proof carve: unchanged) AND

    need ≤ host_bound − Σ reservations of every live node on the host
    host_bound = MemAvailable − 1 GiB (declared share) | 70 % × (MemAvailable − 1 GiB) (no share) — the node ledger's own policy

`MemAvailable` alone (`palw_backends::host_wide_headroom_v1`): the cgroup term is the node's, the host is everyone's. It keeps the
node ledger's deliberate double count host-wide (a grant's touched pages have left `MemAvailable` and are subtracted again): the
error holds a duty that would have fitted, never starts one that would not. A host refusal names the host's bound, what is reserved
on it and by how many nodes (`… this node's own bounds admit it, and the host ledger cannot cover 2.50 GiB: the host's bound is
6.00 GiB and 5.00 GiB of it is reserved by 2 node(s) on this host`); the per-duty pre-check sees it as a dry run, and A2's widest-width
search narrows a replay to what the HOST can grant as well as the seat. **Readiness never reads it**: a seat's capacity
(`capacity_admits`) stays its own — a seat is a seat for the class while its host is busy — and the readiness proof's 32 MiB is
registered on the host but never refused there (it is the node's standing carve; a refused proof lapses the seat's readiness row,
and on 2026-09-25 that HELD a class and voided its claims). Pinned pages are already outside `MemAvailable` (unevictable), so the
aggregate needs no term for them; the `p` lines are the host's report, each distinct identity counted once.

**The host pinner** (the coordinator's condition 5). `kaspad --palw-host-pinner --palw-class-artifact=<8k> --palw-class-artifact=<IR>
--palw-host-ledger-dir=<dir>` holds the pins and nothing else (no chain, no appdir, no bond: `palw_artifact_pin::run_host_pinner_v1`),
by a node's own rules (never a residency's container, the cap, the spare memory), registered in the host ledger as `pinner`. Once every
file is locked it sends systemd `READY=1` (`sd_notify` over `NOTIFY_SOCKET`, `palw_artifact_pin::sd_notify_v1`); one that locked
nothing exits 1 and the unit retries. The kit's `misaka-palw-pinner.service` (`contrib/t12-deploy-kit/pinner-lib.sh`): `Type=notify`,
`LimitMEMLOCK=infinity`, no `MemoryMax`, `Restart=on-failure`; every seat's unit on the host `Wants=` it and is `After=` it, so at boot
and at every seat start the pages are the pinner's before a seat can fault one. A page is charged to the cgroup that FIRST faults it in:
with the pinner first, no seat's ledger carries 1.7–3.4 GiB of artifact in its cgroup term. Each seat still pins the same files itself
(its own lock on resident pages — free: `mincore` reads them resident, so the seat's pin reads nothing and always fits), so the pinner is
never a dependency (`Wants=`, not `Requires=`): a seat starts without one, and stopping it changes nothing for a running seat. On a host
already running, the pages stay charged to the seat that faulted them until that seat restarts: its old cgroup's charge then passes to
the parent slice, and the pinner's lock keeps the pages resident, so the restarted seat faults none of them — the kit's `upgrade`
starts the pinner BEFORE the first seat restarts for exactly this.

**The host's arithmetic: 5.104** (24 GiB; the coordinator's question on CP2). RAM 24,033 MiB (`install-5104.sh`), the reserve for
everything that is not a t12 kaspad 4,096 MiB (`RESERVE_MIB`, the t11 fixture node included), the pinned artifacts once 3,497 MiB
(1,716 + 1,781), and each seat's live set 3.2 GiB = 3,277 MiB (lane P, `t12-panel-backlog-1001.md` §7: 2.7–3.2 GiB `Private_Dirty`;
the high end). What is left is what replays can use; the host ledger keeps its 1 GiB below `MemAvailable` on top. An 8k full seat
reserves 1,740 MiB at W = 64, 886 at 32, 460 at 16 (§4, pinned). The host ledger keeps the node ledger's deliberate double count: a
running replay's touched pages leave `MemAvailable` AND stay reserved, so replays that start together get `⌊bound / need⌋`, while
one that starts beside running replays whose pages are touched needs `bound − 2 × their reservations ≥ need` — the sustained count:

| 5.104 | Σ live | RAM − reserve − pinned once − Σ live | the host ledger's bound (− 1 GiB) | concurrent 8k replays at W = 64 / 32 / 16: started together · sustained |
| --- | --- | --- | --- | --- |
| five seats (now) | 16,385 MiB | **55 MiB** | 0 | **0 / 0 / 0 · 0 / 0 / 0** |
| three seats (b7 → .113, b3 → ibm) | 9,831 MiB | **6,609 MiB** (6.45 GiB) | 5,585 MiB (5.45 GiB) | **3 / 6 / 12 · 2 / 3 / 6** |

*Five seats.* There is no room: the five live sets, the pins and the reserve fill the host before a replay starts, which is the
10-01 night measured from the other side. The host ledger cannot make room; it turns the swap storm into a queue — a seat's duty
waits (`… the host ledger cannot cover …`) until `MemAvailable` has the room, which on this host is the page cache and LazyFree the
kernel can still drop: replays run one or two at a time, host-wide, at whatever width fits. The 5.104 seats stay slow, and so do the
claims they gate, until two of them move. The figure moves with the live set (each 0.5 GiB less per seat is 2.5 GiB more at five
seats: one replay at W = 64 and one at W = 32); lane P's 3.2 is the planning figure, and an idle seat's `Private_Dirty − LazyFree`
from the host would firm the row up.

*Three seats.* 5.45 GiB on the host ledger admits every seat's one 8k replay at full width when they start together (3 × 1,740 =
5,220 MiB), but sustains two at W = 64: with two running and touched, `5,585 − 2 × 3,480 < 0` holds the third whatever its width
until one returns. At W = 32 it sustains three — one per seat (each seat's own share would admit four: 4 × 886 ≤ 3,584 − 32) — and
at 16 six. 32 costs 4–11 % of a replay's time against 64 (§4, timed): on a host whose ledger is the limit,
**`--palw-prefill-run-max=32` buys a third concurrent replay for 4–11 % of each one's time**. The kit carries the knob (`PREFILL_RUN_MAX` in
an `install-<host>.sh`) and sets it nowhere: the coordinator's call, after the move. Beyond the host, a seat's own limits: its
replay slots, and its cgroup — with the pinner, 9,216 − 3,277 live − 1,024 − its page cache ≈ 2.9–4.9 GiB of working sets, two W =
64 replays when its page cache is under ~1.4 GiB. The rule in `t12-panel-backlog-1001.md` §7 (`seats ≤ ⌊(RAM − 4 GiB − P) / (3.2 +
R_W)⌋`: 3 at W = 64, 4 at W ≤ 32) is the same arithmetic without the double count.

**What the worksheet then says** (`kaspad/tests/t12_role_memory_figures.rs`, W = 64, pinned; on a host whose `install-<host>.sh`
sets `HOST_PINNER=1` the artifact leaves the `MemoryMax` term, the pinner's cgroup holding it). With its own estimate of a seat's base
(437 MiB: the caches it declares at ram-scale 0.131, + 256), every 5.104 seat is **OK** at 9 GiB: it needs ≥ 8,877 MiB = share 3,584
+ base 437 + artifact 0 + ΣW 1,784 (one seat duty running — the worst partial segment — while a second asks) + reserve 1,024 + cache
2,048. In-process, the artifact in the term, the same seat needed 10,594 (TOO LOW, CP2). With lane P's measured live set in place
of the estimate (`T12_BASE_MIB=3277`) it says **TOO LOW: 11,717 MiB**. That is the cgroup term binding before the share, not a crash
guard crossed: at 3.2 GiB live a seat's cgroup admits 9,216 − 3,277 − 2,048 − 1,024 = 2,867 MiB of working sets — one 8k replay at
W = 64 and a floor, or two at 32 — A2 narrows to it, and on this host the host ledger binds first anyway (at three seats it sustains
two W = 64 replays or three W = 32 across the host; at five, none). ibm's rows are TOO LOW even on the estimate (b0 21,400 against 20,480; b1 16,524 against 16,384):
with the artifact out of every need, a share holds more duties at once (b0: two seat duties, an attempt and a DA answer, ΣW 7,048),
and the term counts their working sets; .113's b6, pinning in process, 18,753 against 17,408. MemoryMax and the shares stay as they
are (the coordinator's call on CP2): on those rows the cgroup term, LazyFree credited (F1), is the bound when the worst mix runs, and
A2 narrows within it.

**Default: OFF in the binary; ON per host in the kit.** The release's kaspad runs neither unless told: a node-only release should not
let a host's shared file decide what a node starts by default — a bug in that path (a lock never released, a corrupt file, a
permission change on `/run`) would hold every seat on the host at once, where the per-node ledger's failures stay per node — and a
host mid-way through a rolling upgrade runs int-10.1 seats that register nothing, so its aggregate is partial until the whole host
runs int-10.2 (`host_nodes` says how many take part). The kit turns both on where the coordinator decided they belong: the
multi-seat hosts. Watch on each after the rollout: `host_nodes` = the host's seats and `host_pinner=1` (`t12check.py` prints the
host ledger's line and a NOTE without a pinner); `host_reserved_mib` back to 0 when the seats are idle; no `host ledger cannot
cover` lines on a host that has room (three seats); on five seats, those lines instead of swap-in (`vmstat 1` si = 0).

**Staleness and failure.** A line whose process is gone (`kill(pid, 0)` = ESRCH) or whose pid was reused (Linux: `/proc/<pid>/stat`
start time ≠ recorded) is pruned by the next writer. A directory that cannot be created or written leaves the host ledger off with
one warning; an I/O failure at a grant is logged once and the node's own bounds decide (fail OPEN to int-10.1 — failing closed would
hold every duty on the host). F4: `host_ledger=on host_nodes host_pinner host_reserved_mib host_reserving host_pinned_mib
host_pinned_files` (or `host_ledger=off`).

**Invariants.**
1. A grant registered on the host is a grant held by a live node; a node that is gone counts nothing from the next write.
2. The host aggregate only ever refuses; it never admits what a node's own bounds refuse — and it never refuses a readiness proof.
3. Off, or broken, the node is exactly int-10.1's; a seat never needs the pinner to start or to run.
4. The pinner holds nothing a seat does not also hold: stopping it releases no page a running seat reads.

**Tests** (several ledgers in one test process over a temp directory): `three_seats_on_one_host_cannot_promise_its_memory_three_times`
(the third seat refused by the host while its share admits it, its readiness proof and capacity untouched; admitted when the first
releases; the share still binds),
`a_dead_process_stops_counting_a_shared_pin_counts_once_and_a_bad_directory_holds_nothing` (a reaped child's 5 GiB line pruned;
the 8k file pinned by two seats and the IR container by one count two files, each once; an unwritable directory refuses at open;
a directory that vanishes fails open), `the_host_ledger_file_round_trips_and_counts_a_shared_pin_once` (members by kind, the
reserving ledgers, a pin counted once), `the_pinners_magic_is_the_containers` (the pinner reads a file's lineage from its magic as
the SDK dispatches it; a Qwen3.6 container is never pinned), `the_pinners_readiness_reaches_the_notify_socket`.

## 6. Kit

* Units: `LimitMEMLOCK=infinity` (`lib.sh unit_body_service`). The kit's units run as root (`CAP_IPC_LOCK`), so the pin works on the
  live units unchanged; the line is for units without it, and documents what the node needs.
* The host pinner (`pinner-lib.sh`, sourced by `lib.sh`): on a host with `HOST_PINNER=1`, `stage` writes `$REL/launch/pinner.sh` (the
  binary's sha256, every flag known to its `--help` — an int-10.1 kaspad is refused with exit 78 —, every artifact present, no
  `KASPAD_*` environment) and `$REL/units/misaka-palw-pinner.service`; `upgrade` installs and (re)starts it BEFORE the first seat and
  saves what was there (`upgrade-<rid>/misaka-palw-pinner.unit.before`, or `.absent`), which `upgrade-rollback` puts back after the
  seats (or stops, disables and removes); `switch` / `rollback` install / remove it; `check` prints its state, what is charged to its
  cgroup, its `holding … GiB` line, and the host ledger as its file says it. A pinner that does not come up is a warning everywhere —
  never a stop: the seats pin the files themselves. Every seat unit on such a host carries `Wants=` / `After=misaka-palw-pinner.service`;
  a plain host's units are byte for byte what they were.
* `--palw-host-ledger-dir=$HOST_LEDGER_DIR` on every node of a host that names one (`build_args`). Turning it on is an ARGS change for
  `upgrade` (`UPGRADE_ARGS_CHANGE_OK=1` on 5.104 and ibm). `--palw-prefill-run-max=$PREFILL_RUN_MAX` where a host caps the prefill
  run — set nowhere; §5's three-seat arithmetic is the case for 32.
* `move-host.sh` / `move-nodes.sh`: a moved seat takes its target host's configuration (`move_host_memory_env`: 5.104 and ibm on, .113
  off until b7's spec moves there), and only under a staged kaspad that knows the flags. `preflight add` counts each distinct pinned
  artifact once for the host (the 8k `.palwart`; with Phase H the IR container) — when the staged kaspad pins.
* `t12check.py`: `pinned_mib`/`pinned_files` (A1), the prefill run (A2), the host ledger's line and a NOTE without a pinner (D1).
* Live kit (`deploy-int10`): `contrib/t12-deploy-kit/patches/` — `p1-f4`, `p2-pin`, `p3-run-width`, `p4-host-ledger-pinner`, in that
  order (`patch -p3`); p4 reproduces, byte for byte, the copy the kit's functions were exercised on (the pinner's launch script and
  unit, the seats' units on a pinner host and a plain one, `upgrade` → re-run → `check` → `upgrade` over an older pinner →
  `upgrade-rollback` twice → `DRY_RUN`, against stub `systemctl`/`journalctl`).
* Join guide (`docs/testnet12-join-mining.md` §6): the pin and `RLIMIT_MEMLOCK`; several nodes on one host (the host ledger, the
  pinner as a unit of its own); what narrowing the prefill run costs.
