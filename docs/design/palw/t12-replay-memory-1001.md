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
against every seat's `MemoryMax`. A host pinner of its own (a unit that pins before the seats start, so the charge leaves the seats'
cgroups) is weighed in §5.

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
where the attempt would otherwise HOLD — an attempt that holds produces nothing, one at 16 positions takes ~1.2× the time.

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

## 6. Kit, and the host's arithmetic

* Units: `LimitMEMLOCK=infinity` (`lib.sh unit_body_service`). The kit's units run as root (`CAP_IPC_LOCK`), so the pin works on the
  live units unchanged; the line is for units without it, and documents what the node needs.
* `move-host.sh preflight add`: Σ shares + the moved seat's share + **each distinct pinned artifact once for the host** (the 8k
  `.palwart`; with Phase H the IR container) + reserve ≤ MemTotal — counted only when the staged kaspad pins (its `--help` knows
  `--palw-no-artifact-pin`).
* `t12check.py`: prints `pinned_mib`/`pinned_files`, and a NOTE when a node pins nothing.
* Live kit (`deploy-int10`): `contrib/t12-deploy-kit/patches/p2-pin-live-kit.patch` (after `p1-f4-live-kit.patch`).
* Join guide (`docs/testnet12-join-mining.md` §6): the pin and `RLIMIT_MEMLOCK`.
