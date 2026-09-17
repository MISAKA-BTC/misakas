# ADR-0136: An artifact is mapped, not read, and a host holds one copy of it

Status: PROPOSED 2026-09-17 on `feat/palw-exec-lane-and-validator-retirement`, with its first two decisions
implemented the same day. Node-side only: no consensus rule changes, no fingerprint moves.

## 0. The sentence this ADR is

A model's weights live in the kernel's page cache once per host; every producer and every seat on that host
reads the same pages and keeps only its own scratch, a replay starts only when the host has the memory to
finish it, and a seat that cannot replay a class proves nothing for it.

## 1. What was measured (2026-09-17, ADR-0135 drill on a 12 GiB VPS)

Seven fixture nodes, each holding the same 1.67 GiB Qwen2.5 A16 artifact (`--palw-class-artifact`), the
registry armed at DAA 20:

| reading | value |
|---|---|
| anonymous memory per node at rest, one node | 1.80 GiB (`RssAnon`; `RssFile` 3 MiB) |
| anonymous per node under the drill's first claims | 2.6–2.8 GiB (`RssAnon` + `VmSwap`) |
| the registering node (`--palw-register-class`) | 3.9 GiB |
| host after four minutes | `MemAvailable` 330 MiB, swap 8.4 GiB of 8.4 GiB, the OOM killer took `node-6` |

The cause is the dense container's loader: `std::fs::read` of the whole file, then a decoder that copied every
int8 tensor into its own `Vec` (`Base0ArtifactV1 { embed, unembed, layers[].wq .. w_down }`), so an artifact
was a private copy per process. The registering node paid twice more: the registration derives the A16
operand-inventory root by materializing every row's bytes (`a16_inventory_v1`) — a second 1.7 GiB — and the
allocator kept the freed arenas.

The same structure is why two 24 GiB Macs rebooted earlier the same day under seven and eight artifact nodes
(`eight-artifact-nodes-materialize-the-inventory-and-reboot-the-mac`): the streaming inventory (ADR-0106) had
removed one copy; the container's own copy remained.

## 2. Decisions

**D1 — the container's int8 slabs point into a read-only mapping of the file.** `Int8SlabV1` (owned or
`Mapped { map, offset, len }`) derefs to `[i8]`, so every reader keeps its slice and only the loader decides
where the bytes live. `decode_artifact_file_mapped_v1` decodes over `ReadOnlyMap` (the POSIX `mmap` already
written for Qwen3.6, ADR-0112) with zero copies of the weights; the digest is still recomputed over the whole
file once, which is the pass that pages it in. The dense lineage's `load` maps where the platform maps and
reads whole where it cannot (a non-POSIX host, as before). `MAP_PRIVATE` read-only: the pages are the page
cache's, shared by every process that maps the same file, and reclaimable under pressure instead of swapped.

**D2 — the registration root is streamed.** The A16 operand-inventory root at registration, in the class
context vector and in the e2e drill is `a16_inventory_digest_v1(...).root()` — one row's bytes alive at a
time, the same root (ADR-0106's streamed build is held to the materialized one by test).

**D3 — a replay respects the host's memory budget (node-local, never consensus).** Before a seat starts a
replay: `need = largest held artifact file + 512 MiB of scratch ≤ 70 % × (MemAvailable − 1 GiB)`; otherwise
the duty waits for a later tick, logged once a minute. Swap is not capacity: a host in swap finishes no replay.
The deadline still runs — a seat that never fits answers nothing, which the quorum prices as silence.

**D4 — readiness follows the budget.** A seat without the budget for a class proves no possession of it; the
standing proof expires by itself (ADR-0135's readiness age) and the class counts one seat fewer. The chain judges
the proof; the budget is the node's. Nothing about RAM enters consensus.

**D5 — the node reports what it holds.** After loading its artifacts a node logs PSS split into anonymous and
file-backed pages and what is swapped (`/proc/self/smaps_rollup`); `scripts/palw-memory-census.sh` prints the
same split for every kaspad on a host, with `MemAvailable`. RSS alone misleads once files are mapped: a
mapped page appears in every sharer's RSS and in none of their anonymous memory. The reading that matters is
`Pss_Anon` per process and `Pss_File` summed over the host.

## 3. Results

Four nodes at rest, the same artifact, before and after D1 (`smaps_rollup`, MiB):

| | before: anon | before: file | after: anon | after: file (PSS share) |
|---|---|---|---|---|
| node | 1,830 | 6 | 178–211 | 437 |
| the registering node | 3,890 | 8 | 2,325 (D2 not yet in this binary) | 438 |
| host, summed | 11,255 + 8,300 swapped (7 nodes) | — | 2,923 | 1,752 = the file, once |

Seven nodes at rest after D1: anonymous 52–66 MiB per non-registering node, the file 1,752 MiB once for the
host, `MemAvailable` 9.3 GiB of 12 (before: 0.3 GiB and the OOM killer). **Under the drill's activity**
(phase 2, eight nodes, the class in PROBATION with eight ready seats, every seat replaying the floor's claims
and proving possession each span): 800–1,030 MiB anonymous per node — the scratch this ADR §4 names: retained
materials, KV caches, logits rows, the per-process compile — and the file still 1,753 MiB once (`Pss_File`
summed over eight processes), `MemAvailable` 9.4 GiB of 12. Before D1 the same eight nodes would have held
8 × 1.8 GiB of copies before any scratch. **D2 matters to every node, not only
the registrant**: on the D1-only binary, the moment the class registration landed, each node that resolved the
new class derived its operand-inventory root by materializing the rows — four nodes at 2.0–2.1 GiB anonymous
within a minute, 9.2 GiB on the host, swap in use — because the resolve runs on every producer tick and the
allocator keeps the freed arenas. With D2 the root is streamed on that path too. The completion condition the operator
set — four concurrent jobs do not multiply the artifact's physical memory by four — holds at rest by
construction (one file, one page cache); the reading under four concurrent replays is taken from the same drill
as its claims license (§7 records it).

## 4. What this ADR does not build, and the final form

* **One runtime per model per host** (the operator's design: a `misaka-model-runtime` holding Qwen2.5 ×1 and
  Qwen3.6 ×1, producer and seats asking it for jobs over a Unix socket). With D1 the weights are already one
  copy per host; what a shared runtime would still save is each process's own scratch (KV caches, retained
  logits rows, traces) and the per-process compile of the class program, and what it would add is an IPC
  boundary in the replay path and a second process to supervise. It stays the final form for a Kimi-class
  artifact whose scratch is itself large; it is not built here.
* **A host-global artifact cache manager** (content-addressed, pin the active classes, LRU-evict the rest).
  The page cache is the cache today; `ReadOnlyMap` carries `will_need`/`dont_need` for a pin policy when
  more than one large class shares a host.
* **Per-tensor scratch reuse inside a replay** and a measured (not estimated) per-class scratch for D3's
  `need`: the 512 MiB estimate is replaced by a measurement once the drill's replays report it.

## 5. Rollout

No consensus change: nothing in the fold, the carriage or the fingerprint moves. Every fleet node that holds a
dense artifact benefits on its next restart (the `.113` host's memory budget, ADR-0112's page-in counts). The
permissionless registry (ADR-0135) is armed only after this, because its readiness evidence assumes a host can
hold what it proves.

## 6. Number hygiene

0136 was free when written; the next free number is 0137.
