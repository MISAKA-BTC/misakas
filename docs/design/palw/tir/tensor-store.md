# PALW tensor store — one interface over every weight read, and dense layers streamed in the order the program already runs them

> **Status: DESIGN — nothing here is implemented** (lane TS, branch `tir/tensor-store-design` off `rcore/int-12` @ `4d5f97d58`,
> 2026-10-03). The implementation is deferred until after the DAA 5,300 deployment, as int-12.x node releases. **Node software
> only: no object, rule, fence, parameter, class id or fingerprint moves**, and `scripts/t12-repin.sh --drift-only` must print no
> row at any step. The benchmark that decides each step is fixed before any code, in [`tensor-store-bench.md`](tensor-store-bench.md).
> Builds on ADR-0112 (a class's weights are read within a stated budget), [`runtime-residency.md`](runtime-residency.md) (the IR
> tiers), ADR-0117 (a draw is one forward), RFC-0006 (cells, shard-only fetch), [`gpu-integer-backend.md`](gpu-integer-backend.md)
> and `docs/design/palw/t12-replay-memory-1001.md` (the 8k pin). Every `file:line` below is at `4d5f97d58`.

## 概要(日本語)

- **一つの trait。** 重みの読み出しはすべて `PalwTensorStoreV1`(`read_range` / `pin` / `prefetch` / `evict` / `stats`)を通る。
  backend は Memory(プロセス所有、またはホスト共有の mlock 済み mapping)・Pread(ローカルファイルの位置指定読み)・Shard
  (holder から取得し登録 root で証明した行をローカルに spill)の 3 つ。demand paging の mmap は計測の基準線だけに残す。
- **予算は manager に一つ。** `TirResidencyManagerV1` が RAM 予算(と M5 用に予約した VRAM 欄)の下で「どのバイトをどこに置くか」
  だけを決める。既存の pinned / routed LRU / gathered、Qwen3.6 の LRU、shard 行取得、8k pinner をこの一か所に集める。
- **欠けている部品 = 密層ストリーミング。** 実行中の occurrence の層を常駐、次の層を先読み、一つ前の層を返す。順序は
  `plan.occurrences`(静的スケジュール)そのものなので推測ではない。常駐させる層の集合は予算から静的に決める(周期アクセスに
  LRU は全滅する)。MoE の routed 行は router が決めるので先読みしない(ADR-0112 Decision 4 のまま)。
- **prefill/replay は occurrence-major の run で実行**し、ストリーム層を run あたり 1 回だけ読む。decode は位置ごとに全層を読み直す
  (遅いが動く)。t12 の attempt は prefill だけ(prefill draw は genesis から必須)なので、producer は 1 回の抽選で artifact を 1 回読む。
  MoE ではこの実行順の方が効く:今の位置優先 executor は 1/5 予算で 255 位置の draw に expert を ≈363 GiB 読むが、run なら ≈27 GiB
  (一様 routing の計算値、§5.6)。
- **floor** = 固定 global + 連続する 2 層(N と N+1)+ 1 occurrence の routed 行 + admission 1 つ。stated 予算が下回れば名前と
  項つきで拒否、既定値が下回れば基準線へ。KV/state は常に常駐(ストリームしない)。
- **不変条件:** 層はバイトの「場所」を決め、「どのバイトか」は決めない。既存の同一性テスト(roots・葉・opening・court close・
  6,000 本の random program)を全モード・全 run 幅・故障注入 store に拡張する。
- **段階(node のみ、fence なし):** M1 trait 統合(挙動不変)→ M2 密層 prefetch/evict + occurrence-major run → M3 verifier
  (cell・shard の disk spill・court)→ M4 producer → M5 GPU/VRAM。各段は 1 台で flag による A/B → fleet、flag で即戻せる。
- **既存コードとスケッチの食い違い**は §10 にそのまま書いた(共有 mlock の mmap は残す、pinner が IR residency と二重保持、
  executor/cell 検証が位置優先、floor の routed 項が 1 token 分、shard 保持が RAM に二重、など)。

## 0. The sentence

**A class's weights are the same bytes on every host; where they are — in host memory, on a device, on local SSD, at a holder —
is the runtime's decision under a budget the operator states, and it is made in one place. Every weight read goes through one
interface (`PalwTensorStoreV1`: read a range, pin, prefetch, evict, count), served by one of three backends — memory, positional
reads of the local file, or a shard's rows fetched from a holder and proven against the registered root — with the demand-paged
mapping kept only as the baseline it is measured against, and as the last resort ADR-0112 already makes it for a default the host
cannot fit. Above it a residency manager decides placement: what every forward
reads whole and small stays pinned; routed and gathered rows are served as they are today; and the per-layer dense weights that
do not fit are STREAMED — the occurrence that runs is resident, the next one is being read, the one before is given back — in the
order the program's schedule already fixes, so the prefetch is a schedule, not a guess. Prefill and replay run occurrence-major
over runs of positions, so a run reads each streamed layer once. The floor — the pinned globals, two consecutive layers, one
occurrence's routed rows and one admission in flight — is the least a budget may be, and a budget below it is refused by name.
Where a byte is never changes which byte it is.**

## 1. What exists, and what is missing

### 1.1 Every weight path at `rcore/int-12`

| path | code | how bytes are read | what holds memory | how the node accounts it |
| --- | --- | --- | --- | --- |
| IR class, page cache | `TirArtifactV1::open` → `MappedFile` (`misaka-palw-tir-exec/src/node/artifact.rs:199-211`, `node/mapped.rs:18-48`); params bound in place and extended to `'static` (`artifact.rs:103-130`) | demand faults on a `MAP_PRIVATE` mapping | the page cache | the file, per replay (`kaspad/src/palw_backends.rs:1930-1939`), 0 when pinned (`:1946-1962`) |
| IR class under a residency | `TirWeightStoreV1` (`node/residency.rs:433-692`) over `TirWeightFileV1` (`:170-234`); tiers read off the program (`tiers.rs:398-493`) | positional reads: the pinned set in parallel at open (`residency.rs:506-518`), one pass over every byte for the root and the ranges (`:519-545`), a route group's rows admitted together (`:701-740`), gathered rows per gather (`:742-785`) | pinned set + routed LRU under `budget − pinned − in flight` (`tiers.rs:575-581`) | the budget (`palw_backends.rs:1934-1937`) |
| shard-only seat (RFC-0006 §4.2) | `fetch_shard_params_v1`, `TirRowPursuitV1` (`node/shardrows.rs:63-119, 267-361`) | the shard's inventory leaves from a holder, each opening checked against `artifact_root` (`:88-100, 332-344`) | the assembled params AND every opening with its bytes (`:44-58, 99-117`) | nothing |
| Qwen3.6 (base0, hand-written) | `Qwen36ResidencyV1` (`misaka-palw-base0/src/qwen36.rs:305-534`) | always-set pinned at open; experts told apart by name (`expert_key_v1`, `:378-383`) and admitted per layer (`:460-489`); embedding rows read directly (`:693-706`); with no residency every tensor is read per use through the descriptor (`:633-638`) | always-set + expert LRU | the budget (`palw_backends.rs:1931-1933`) |
| dense A16 `.palwart` (the 8k class) | `Int8SlabV1::Mapped` (`misaka-palw-base0/src/artifact.rs:261-286`); pinned by int-10.2 A1 (`kaspad/src/palw_artifact_pin.rs`) and by the host pinner (`kaspad/src/main.rs:27-43`, `palw_artifact_pin.rs:488-547`) | in place, from a `MAP_SHARED` mapping populated by 8 MiB reads and `mlock`ed before the lineage maps the file | the locked page cache, one copy per host | reported, never reserved (`palw_backends.rs:914-931`) |
| GPU (prototype) | `misaka-palw-tir-gpu` (its own cargo workspace); seams `TirDeviceV1` (`src/cellstep.rs:115-128`) and `KernelBackendV1` (`node/cell.rs:86-102`); device pool `kaspad/src/palw_memory_ledger.rs:107-110, 631-639` | params uploaded per cell | VRAM, or host on unified memory | the device pool the backend arms |

What each duty runs on top of these: the producer and a full-seat replay step **one position through every occurrence**
(`TirClassRunnerV1::drive`, `node/run.rs:317-337`); so does the RFC-0006 cell verifier (`node/cell.rs:274-313`); RFC-0004's
evaluation steps a parent's candidates **occurrence at a time across members** (`src/lockstep.rs:95-101`); the A16 engine walks a
prefill **a layer at a time over runs of 64 positions** (`misaka-palw-base0/src/qwen25_a16_backend.rs:127-146, 680-700`;
`engine_a16.rs:2005`); the GPU prototype replays a job **a layer at a time over a chunk of positions** (`BatchReplay`,
`misaka-palw-tir-gpu/src/batch.rs:184`).

### 1.2 What is missing

1. **Dense layers cannot stream.** A per-layer weight every forward reads densely is tier `Pinned(Dense)` (`tiers.rs:448-449`), so a
   dense class's floor is its size: `runtime-residency.md` §7 has Qwen2.5-7B at 6.70 GiB of floor for 7.72 GiB of weights, "SHORT"
   at a fifth, and it stays on the page cache by default (`residency.rs:91-93`). ADR-0112 §8 named the dial — "Streaming the
   always-set … The dial exists in the design; the ratio this ADR certifies does not need it" — and left it. A producer must run
   every layer, so this is the hardware barrier that remains once RFC-0006 lets a verifier hold one shard.
2. **No prefetch.** An admission reads, then the gathers compute (`runtime-residency.md` §9; ADR-0112 §8, last bullet).
3. **No occurrence-major run on the CPU executor** (§10, F3): streaming without it re-reads every streamed layer every position.
4. **Five accountings** — the IR residency's stats, the Qwen3.6 stats, the pin summary, the shard holding's byte count, the device
   pool — and two decisions that can disagree about the same file (§10, F2).
5. **Size-blind holdings**: a shard holding and the inventory tree live in RAM whatever their size (§10, F8 and F9).

## 2. Requirements (the user's decisions, 2026-10-03) and non-goals

| | requirement | where |
| --- | --- | --- |
| R1 | One trait over every weight read — `read_range`, `pin`, `prefetch`, `evict`, plus stats, budget and schedule hints; backends Memory, Pread, Shard first; mmap only as a measured baseline (fleet virtio disks: 6–11 MB/s through faults against ~845 MB/s for sized reads, ADR-0112 §1) | §3 |
| R2 | A residency manager with a RAM budget and a VRAM budget field reserved for M5, unifying pinned / routed LRU / gathered, the Qwen3.6 LRU, the shard-row fetcher and the A16 8k pinner — how each maps, what changes, what does not | §3.6, §4 |
| R3 | Dense-layer streaming driven by the static schedule: the streamed tier, its floor, its interaction with the other tiers, prefill against decode, KV/state resident, refusal by name | §5 |
| R4 | Producer and panel in one plan; "a 200 GB artifact is not 200 GB of RAM": 200 GB on SSD, 16–32 GB of RAM (+ 12–24 GB of VRAM at M5), slow but runnable | §6 |
| R5 | Bit-exact: tiers decide where bytes are, never which; every result identical to the reference interpreter and the current executor; how it is tested | §7 |
| R6 | Node-only integration M1–M5; each step A/B on one node behind a flag, then the fleet; what is measured to decide; rollback | §8 |
| R7 | Out of scope stated plainly | §11 |

**Non-goals.** No consensus change of any kind. No container format change: instances stay in inventory order, which is
param-major (`plan.rs:341-349`), and a layer's slab is gathered from per-param extents. No offloading of KV or state. No
re-quantization or compression of weights. No remote streaming for a producer: a producer keeps its artifact on local SSD (a
shard's rows come from a holder only for a verifier, §3.4).

## 3. The interface

### 3.1 Keys, spans and leases

```rust
/// An instance of a param as the inventory keys it: (param, layer) — `None` for a global.
pub type TirInstanceKeyV1 = (u16, Option<u16>);

/// A byte range of one instance: a whole instance, one row of a row-addressed one, or a piece (an
/// opening's ≤ 32 KiB leaf, a pass's chunk). The spans a manager holds never overlap: each instance
/// has exactly one placement (§3.6).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TirSpanV1 {
    pub key: TirInstanceKeyV1,
    pub at: u64,
    pub len: u64,
}

/// Bytes a store holds for a reader, alive while the lease lives whatever the store evicts meanwhile
/// (the floor's in-flight term counts exactly this). 8-aligned when owned (`TirHeldBytesV1`,
/// residency.rs:140-168), at the container's 64-byte alignment when locked (mapped.rs:1-2).
pub enum TirLeaseV1 {
    Owned(Arc<TirHeldBytesV1>),
    Locked { region: Arc<dyn TirLockedRegionV1>, at: usize, len: usize },
}
```

### 3.2 The trait

```rust
/// **Every weight read a node makes.** Mechanism only: a store reads, holds and gives back what it is
/// told; WHICH bytes are held and WHEN is the manager's (§3.6).
pub trait PalwTensorStoreV1: Send + Sync {
    /// "memory:owned", "memory:locked <path>", "pread <path>", "shard <root> i/S_L", "page-cache <path>".
    fn kind(&self) -> PalwStoreKindV1;
    /// The bytes of `key`, if this store serves it (a shard store serves its shard's instances only).
    fn extent(&self, key: TirInstanceKeyV1) -> Option<u64>;
    /// **Exactly `out.len()` bytes of `key` from `at`, now** — synchronous, counted, refused by name
    /// past the extent; never a fault on a mapping (the baseline excepted).
    fn read_range(&self, key: TirInstanceKeyV1, at: u64, out: &mut [u8]) -> Result<(), PalwStoreErrorV1>;
    /// **Hold `span` and lease it.** A span already held, or being read by a prefetch, is joined —
    /// never read twice. The hold lasts until `evict`.
    fn pin(&self, span: TirSpanV1) -> Result<TirLeaseV1, PalwStoreErrorV1>;
    /// **Start holding these spans, without waiting**, in the order given, on the store's own I/O
    /// threads (never the compute pool). A later `pin` joins the read.
    fn prefetch(&self, spans: &[TirSpanV1], hint: TirScheduleHintV1);
    /// **Give back the hold on these spans.** A lease outlives it; the bytes go with the last lease.
    /// A prefetch not yet started is cancelled.
    fn evict(&self, spans: &[TirSpanV1]);
    /// Bytes and reads, holds, joins, waits and their time, evictions, wasted prefetches — the
    /// store's own counters (the process's OS counters are the node's, §9).
    fn stats(&self) -> PalwStoreStatsV1;
}

/// Why a span is wanted — for ordering, cancellation and the log.
pub struct TirScheduleHintV1 {
    pub cursor: u32,
    pub occurrence: u16,
    pub position: u32,
    pub want: TirWantV1, // Slab | RouteAdmission | Pinned | Pass | Opening
}
```

A blanket `impl<S: PalwTensorStoreV1 + ?Sized> TirByteSourceV1 for S` makes every store an inventory byte source
(`node/inventory.rs:264-267` is already `read_range`'s shape), so the open pass, court openings and the inventory tree read
through it with no change of their own.

**Where the sketch's "budget" and "schedule hints" went.** The budget is the manager's, not a backend's: a composite candidate
already reads two stores — its parent's shared residency and its own adapter, pinned beside it (`artifact.rs:340-404`) — and the
node prices the two together (`palw_backends.rs:1934-1937`). The schedule enters a store only as `TirScheduleHintV1` on a
prefetch; the schedule itself is the manager's (§5.2).

### 3.3 The contract every backend keeps

1. **Exactness.** `read_range` and a lease return exactly the file's bytes — or, for a shard store, bytes whose openings were
   proven against the registered root before they were written to its spill.
2. **Every byte counted once** where it is read (`bytes_read`), as `TirWeightFileV1::read_at` counts today
   (`residency.rs:197-221`).
3. **Refusals by name**: a range past the extent (`residency.rs:205-207`'s message), a byte the device could not read, a shard
   leaf not held, a closed I/O pool.
4. **No policy.** A store never evicts on its own and never refuses for memory; the manager does both.
5. **No I/O on the compute pool.** Prefetches run on a dedicated I/O pool (§5.9); a `pin` on the forward's path may read
   synchronously (that is a stall, and is counted).

### 3.4 The backends

| backend | `read_range` | `pin` | `prefetch` | `evict` | used for |
| --- | --- | --- | --- | --- | --- |
| **Memory, owned** | copy | lease on the existing `Arc` | nothing to do | nothing to do | tests (`TirRowsInMemoryV1`, `rows.rs:154-212`), a composite's adapter (`artifact.rs:363-372`), a shard small enough to hold |
| **Memory, locked** | copy from the locked mapping | lease on the region, zero copy | nothing (the file is locked whole) | nothing | an artifact read in place by several processes on one host: the 8k `.palwart`, a small IR container — today's pin (`palw_artifact_pin.rs:271-331`): `MAP_SHARED`, populated by 8 MiB reads, `mlock`, one copy per host |
| **Pread** | positional read (`read_exact_at`), split into 8 MiB parallel reads (`residency.rs:182, 224-233`) | read into owned memory and hold | queue on the I/O pool | drop the hold | everything that is not locked: today's residency, and streaming |
| **Shard** | from the local spill, through Pread; a leaf not in the spill is an error, never a fetch inside a step | as Pread | as Pread | as Pread | a shard-only seat: the fill (`TirRowPursuitV1`, unchanged) proves each opening against the root and appends it to a local spill file; then the shard's instances are served like any file |
| **Page-cache baseline** | copy from a `MAP_PRIVATE` mapping (faults) | a borrowed view (faults when read) | `MADV_WILLNEED` (which ADR-0112 §1 measured to do nothing on the fleet's disks) | `MADV_DONTNEED` | what neither the lock nor a budget can hold — today's decline, and `0` where the lock is refused — logged as such; and the bench |

**Why the locked mapping stays.** The sketch puts mmap on the measured baseline only. That holds for a **demand-paged** mapping,
the one ADR-0112 measured at 6–11 MB/s. The pin's mapping is something else: populated by large reads and locked before anything
reads it (`palw_artifact_pin.rs:28-46`), so no read of it ever faults, and it is the one way several processes on a host share one
copy — 5.104 measured five seats at 1.68 GiB of `Shared_Clean` each, one physical copy (`t12-replay-memory-1001.md` §1); five
owned copies would be 8.4 GiB. It is the Memory backend's shared form, not a fourth way of reading.

### 3.5 Which backend a holding gets — one decision, taken before the file is mapped

Today two decisions are taken about the same file: the pinner's before the lineage maps it (`prepin_file_v1`,
`palw_backends.rs:1284`) and the residency's inside the load (`load_artifact_with(p, policy)`, `:1285`). For an IR container they
can disagree (§10, F2). M1 makes them one function, `palw_store_choice_v1`, called where `prepin_file_v1` is called now — before
the lineage maps the file, as macOS requires of a shared lock (`palw_artifact_pin.rs:37-46`) — from the file's magic, an IR
container's header (its program gives the tier arithmetic without reading a weight: `TirTiersV1::of`, `tiers.rs:398`), the
policy and the host:

1. A Qwen3.6 `.palwq36` → **its own residency**, reported through the manager's accounting (§4).
2. A shard-only seat → **Shard**.
3. A stated budget (`--palw-class-resident-bytes N`, `N > 0`) for a lineage that takes one (an IR class) → **Pread + manager** at
   that budget, and never locked: the operator's number is the budget. (Today such a holding is also locked whole — F2.)
4. Otherwise, a file that would not be resident under today's rules — the dense lineage, which takes no residency policy
   (ADR-0112 I-6); an IR class at `0`, or whose default residency declines — and that fits the pin rules (`--palw-no-artifact-pin`
   off; within the cap, default ¼ of MemTotal; the bytes not yet resident within the host's spare past the 1 GiB reserve,
   `palw_artifact_pin.rs:304-318`) → **Memory, locked**: today's pin, unchanged. A file that fits the lock is not streamed, even
   with streaming on — on a host of seats one shared copy beats a stream per process.
5. Otherwise, an IR class whose default residency does not decline → **Pread + manager** under the default (`FifthWithin(spare)`,
   `palw_backends.rs:1666-1671`), as today, without the lock.
6. Otherwise — a file the lock cannot hold and today's default cannot run — with streaming on and the default at or over the
   streamed floor → **Pread + manager, streaming** (§5.8); else the **page-cache baseline**, as today's decline, logged.

So the dense A16 lineage takes 4 or the page cache, exactly as today: its engine reads `Int8SlabV1` slices and has no streamed path
in this plan (§10, F11). Streaming is for what does not fit, and it is the IR classes too large to lock — the 7B and up of §5.10 on
the fleet's 23 GiB hosts — that take it.

### 3.6 The residency manager

```rust
/// **The budget**: one number for host memory, as ADR-0112's; the device's reserved for M5.
pub struct TirBudgetV1 {
    /// Bytes of weights the manager may hold: the pinned set, the resident layers, the routed rows,
    /// every cursor's window and one admission in flight (§5.8).
    pub ram_bytes: u64,
    /// Device memory for weights (M5). `None` until a device backend leaves the prototype, and `None`
    /// on unified memory, where device bytes are host bytes (gpu-integer-backend.md §9). A `Some`
    /// before M5 is refused at open, by name.
    pub vram_bytes: Option<u64>,
}

/// **Where one instance's bytes are**, decided at open from the tiers — which stay a pure function
/// of the program (tiers.rs) — and the budget.
pub enum TirPlacementV1 {
    /// Read at open, held for the manager's life (today's pinned tier).
    Pinned,
    /// A per-layer dense instance the budget keeps for the manager's life (§5.3).
    Resident { occurrence: u16 },
    /// A per-layer dense instance held while its occurrence runs, and while it is being prefetched.
    Streamed { occurrence: u16 },
    /// Rows a route selects, under the routed capacity (today's LRU, residency.rs:376-420, unchanged).
    Routed,
    /// Rows an input selects, read per gather, never held (unchanged).
    Gathered,
}

/// What an executor asks of the manager for the instances it streams.
pub trait TirSlabSourceV1: Send + Sync {
    /// Is (j, layer) streamed or resident — read through an occurrence's lease rather than bound?
    fn streams(&self, key: TirInstanceKeyV1) -> bool;
    /// A cursor for one forward (an executor, or one lockstep batch): reserves one window (§5.8),
    /// refused by name when the budget has none left — the duty then waits, as the ledger holds it.
    fn cursor(&self, hint: TirCursorHintV1) -> Result<TirCursorV1, PalwStoreErrorV1>;
    /// Occurrence `occ`'s slab, resident and leased to the cursor: waits for what is not in yet (a
    /// stall, counted) and prefetches the next `depth` occurrences of the schedule, cyclically —
    /// after `post` comes the next position's `pre` — while the job has positions left.
    fn begin(&self, cursor: &mut TirCursorV1, occ: usize) -> Result<TirOccLeaseV1, PalwStoreErrorV1>;
    /// The cursor is done with `occ`: its leases drop, and a slab no cursor holds that is not
    /// resident is evicted.
    fn end(&self, cursor: &mut TirCursorV1, occ: usize);
}
```

`TirResidencyManagerV1` owns one store, the budget, the tiers (`TirTiersV1`, unchanged), the placements, the schedule
(`TirPlan::occurrences`, `plan.rs:112`), each occurrence's slab (its spans and bytes), the routed LRU (moved as is from
`TirWeightStoreV1`), the cursors, and the root and ranges of the open pass. It implements `TirRowSourceV1` exactly as the store
does today (`rows.rs:35-57`: admit, gather rows, the counted whole read, ranges, counts) and the new `TirSlabSourceV1`. One
manager per artifact root per process, as today's store registry (`residency.rs:452-460`), so a parent's candidates share slabs
as they share rows.

Its numbers extend `TirResidencyStatsV1` (`residency.rs:104-131`, kept as a view): `resident_slab_bytes`, `window_bytes`,
`cursors`, `slab_begins`, `slab_hits` (the slab was resident or fully read at `begin`), `slab_waits`, `stall_nanos`,
`streamed_bytes_read`, `prefetch_wasted` (read, then evicted unused), and in M5 the device half.

### 3.7 What the executor changes (M2)

The store alone cannot stream: the executor binds params for its whole life (`TirExecutor<'a> { params: &'a TirParams<'a> }`,
`exec.rs:437-440`), and a resident artifact's pinned bytes are extended to `'static` for it (`artifact.rs:103-111, 135-159`). So:

1. `TirParams` (`params.rs:106-124`) gains a third kind of instance beside bound and row-served: **streamed**, answered by a
   `TirSlabSourceV1`. `has` is true for it (so `check_complete`, `params.rs:280-290`, and `new_cell`'s check, `exec.rs:492-510`,
   pass), `range` comes from the source's open pass (as for served rows, `params.rs:295-307`), and `get` does not answer it.
2. `Reader` (`exec.rs:194-225`) carries the occurrence's lease; `Reader::data(Src::Param(j))` reads a bound instance, else the
   lease's.
3. `run_occurrences` (`exec.rs:888-1132`) brackets each occurrence with `begin`/`end`. `end` is called after the carry-out and
   the logits are materialised (`exec.rs:1100-1124`): from there no `Val` of the occurrence is read again — a block's stale
   `vals` are overwritten in node order before they are read (`Ref::Node` names earlier nodes only) — so the slab may go.
4. A fused region (`exec.rs:643-668`) that reads a streamed param reads it through the same lease, whole.
5. The counted whole-read fallback (`params.rs:118-124, 314-325`) keeps an instance in a `OnceLock` for the executor's life; it
   becomes an occurrence lease, so even an unplanned dense read stays inside the budget. It is still counted and still asserted
   zero on every planned path.
6. A lockstep batch (`lockstep.rs:78-112`) takes ONE cursor: `begin` before the members run an occurrence, `end` after the last —
   so a batch reads a slab once a position, as it reads a route's rows once today.
7. The occurrence-major run (§5.4): `TirPlan::run_eligible()` and `TirExecutor::step_run`.

## 4. How each existing piece maps onto the interface

| piece | today | in the store | what changes | what does not |
| --- | --- | --- | --- | --- |
| IR pinned tier | read in parallel at open into a `BTreeMap`, bound `'static` (`residency.rs:506-518`, `artifact.rs:135-159`) | placement `Pinned`: `store.pin(whole instance)` at open; the manager holds the leases for its life | the bytes sit behind a lease's `Arc`; the `held_forever` contract holds on it | which instances are pinned (`TirTiersV1::of`), the parallel read at open, I-1 … I-8 of `runtime-residency.md` §6 |
| IR routed LRU | `TirRowCacheV1` keyed `(j, layer, row)`, stamp-ordered (`residency.rs:376-420`); admission per route group (`:701-740`); late reads (`:757-768`) | placement `Routed`: a miss is `store.pin(row span)`; the LRU's accounting stays in the manager | the bytes are the store's holds | admission at the group's first gather (`rows.rs:110-132`), the capacity arithmetic, hits / misses / evictions |
| IR gathered rows | a positional read per row (`residency.rs:769-772`) | placement `Gathered`: `store.read_range` | nothing | never held |
| the open pass | every byte once, leaves hashed in chunks of whole leaves (`residency.rs:236-349, 519-545`) | through `read_range`, in inventory order | the reads go through the trait; with spare budget the manager keeps slabs the pass read (a warm open) | the root, the ranges, `bytes_read` after open = the weights' bytes |
| `TirWeightFileV1` | positional reads, rayon-parallel above 8 MiB (`residency.rs:170-234`) | the Pread backend's file | prefetches move to a dedicated I/O pool | the reads themselves, the refusal past the file |
| page cache | `MappedFile` (`mapped.rs`), `TirWeightsV1::Mapped` (`artifact.rs:47-52`) | the page-cache baseline | reachable only by `0` or a declined default | identity with every other placement |
| Qwen3.6 residency | `Qwen36ResidencyV1` (`qwen36.rs:305-534`), placement by name (`:378-383`), `TensorBytes::Held` handles (`:180-183`) | M1: an adapter — its always-set reported as `Pinned`, its experts as `Routed`, its embedding as `Gathered`, over its own reads | it reports through the manager's one accounting line | its LRU, its admission, its floor, its tests (`a_budgeted_artifact_computes_what_an_owned_one_does_at_a_fifth_of_its_size`) — it is the hand-written runtime of the Qwen3.6 rows (`.palwq36`), and it moves only if those rows become IR classes |
| shard-row fetcher | `TirRowPursuitV1` → `fetch_shard_params_v1` → `MapParams` → `TirParams` in memory, openings kept with their bytes (`shardrows.rs:55-57, 99-117`) | the Shard backend: fill → spill file → Pread | the shard on disk, not in RAM; openings' paths on disk (M3) | what is fetched (`palw_tir_shard_inventory_ranges_v1`), what is believed (each opening against the root, `shardrows.rs:88-100`), the refusals |
| A16 8k pinner | `prepin_file_v1` / `settle_prepin_v1` (`palw_artifact_pin.rs:271-398`), the host pinner (`:488-547`) | Memory, locked; the host ledger still counts each file once (`register_pin`) | the decision moves into `palw_store_choice_v1` (§3.5), which never locks a file a residency decides | the A16 engine's in-place `Int8SlabV1` reads, `MAP_SHARED` + populate + `mlock`, the pinner unit, the run width (`palw_prefill_run.rs:112`) |
| GPU | `TirDeviceV1::cell_stepper` uploads a cell's params (`cellstep.rs:115-128`) | M5: placement widened to "resident where" | M5 only | — |

The Qwen3.6 runtime's own comment already says where this goes: "the same policy is what a GPU tier would use, with the
placement decision widened from 'resident or not' to 'resident where'" (`qwen36.rs:300-304`).

## 5. Dense-layer streaming

### 5.1 What is streamed

- **Streamable**: a per-layer instance `(j, Some(l))` whose tier is `Pinned(Dense | EveryRow | UnevenRows)` and whose bytes are
  at least `TIR_PIN_BELOW_BYTES_V1` (1 MiB, `tiers.rs:43`). Smaller per-layer instances — narrowing vectors, the 128 KiB
  activation tables, a mixture's router — stay `Pinned`: ≈ 0.8 MiB a layer at the 7B shape, ≈ 2.4 MiB at the 405B shape
  (≈ 23 and ≈ 151 MiB in all).
- **A slab** is every streamable instance of one occurrence. A per-layer instance is read by exactly the occurrence of its layer
  (`tir_param_instances_v1`, `tiers.rs:359-383`; the schedule is `pre`, the layers, `post`, `misaka-palw-tir/src/program.rs:165-173`).
- **Globals stay as they are**: the embedding is gathered, the RoPE tables are gathered or small, and `post`'s head — one
  occurrence, read every position — stays pinned (0.29–1.96 GiB in the shapes of §5.10).
- **A slab is read from per-param extents** — the container is param-major, so a slab is a few extents, each located by the
  container's directory (`container.locate`): 7 in the dense shapes of §5.10 (q, k, v, o, gate, up, down), 4 in the mixture's
  (its attention). No format change.

### 5.2 The schedule is the prefetch

A cursor walks `plan.occurrences`. At `begin(o)` the slab of `o` is resident (waited for if its read has not finished) and the
slabs of `o + 1 … o + depth` are queued; at `end(o)` the slab of `o` is given back unless it is resident or another cursor holds
it. With the default `depth = 1` that is the sketch exactly: **N resident, N + 1 prefetched, N − 1 evicted.** The order is the
compiled plan's, fixed per program (`plan.rs:112`), so nothing is predicted; after `post` the cursor continues at the next
position's `pre` while the job has positions, and stops prefetching at its end.

**Routed rows are not in this schedule.** Which experts a layer reads is the router's output at that layer, so they are admitted
at the route group's first gather, as now (`rows.rs:110-132`) — ADR-0112 Decision 4: prefetch of experts across layers "is
impossible and not attempted". Determinism of the prefetch is a property of the dense slabs only.

### 5.3 Which slabs stay resident

The budget left over after the floor (§5.8) holds `k` of the `n` slabs for the manager's life (placement `Resident`), chosen at
open, deterministically from the program and the budget, **evenly spaced** in schedule order.

- **Not an LRU.** The access sequence is a cycle of `n` slabs, every position. An LRU holding `k < n` slabs of a cycle never hits
  (each slab is evicted just before it comes round again). Every policy must miss at least `n − k` slabs a cycle — Belady's MIN does
  no better on a cycle — and a static set of `k` misses exactly `n − k`.
- **Evenly spaced.** At depth 1 a streamed slab's read hides behind the previous occurrence's compute, whoever that is; spreading
  the resident slabs keeps the I/O queue steady instead of bunching the misses. Prefix against interleaved is an informative bench
  cell (`tensor-store-bench.md` §6).
- **Leftover to resident slabs, not to depth.** A resident slab saves its read every position; a deeper prefetch only hides
  latency where compute exceeds the read. Depth stays 1 unless the bench shows stalls with compute ≥ read.

### 5.4 Prefill and replay: occurrence-major runs

When the positions are known before the run — the producer's attempt (on t12 a prefill and nothing else, §6), a seat's replay, a
cell's segment, a court's re-derivation from a resume point — the executor runs **occurrence-major over a run of W positions**:
occurrence 0 at every position of the run, then occurrence 1, and so on. A streamed slab is then read once a run, not once a
position. This is what base0 already does for the dense and hybrid engines (`forward_prefill_planned`, `engine_a16.rs:2005`,
`qwen36_plan.rs:485`; ADR-0117 Decision 2: "the order is not the protocol"), what the GPU prototype does (`BatchReplay`), and what
RFC-0006 §1.4 specifies for a cell ("one layer at a time over every position of the segment").

- **API (M2).** `TirExecutor::step_run(tokens, run_post, sink) -> TirResult<RunOut>` over positions `pos .. pos + tokens.len()`.
- **Eligibility.** `TirPlan::run_eligible()`: every state instance is read and written by one occurrence only — per-layer K/V
  histories and per-layer recurrences, a decoder's usual form; which corpus programs qualify is the M2 tests' first report.
  Otherwise the run is position-major, as today, and says so. (A history appended by one occurrence and read by others could join later through positional views, as
  `BatchReplay` handles a state `post` reads, `gpu-integer-backend.md` §11.)
- **Effects per (occurrence, position).** A `Fixed` instance's swap, a history's commit and its kept tail apply after its
  occurrence runs at a position — the same values `end_step` applies after the whole position (`exec.rs:853-885`), because no
  other occurrence reads the instance.
- **Leaves by index.** The producer already keeps every leaf hash of the job (`run.rs:60-61, 297`) and a dense capture keeps every
  preimage (`backend.rs:341-353`). A run writes each at its index — F4's enumeration gives it (`leaves_of_position`, used at
  `run.rs:565`) — instead of pushing; `on_leaf` consumers get the run's leaves in F4 order when it ends. Checkpoint and history-tile
  leaves are snapshotted per instance as its occurrence passes the position.
- **Failures.** The reference's error is the first failing `(position, slot)` in position order; an occurrence-major run may meet
  a later one first. A run that fails restores its start (the `Fixed` instances it wrote, the history lengths) and re-runs
  position-major, so the error, its class and its position are the reference's.
- **Width.** The widest W whose window, routed union over W positions and run buffers fit — chosen as int-10.2 A2 chooses the A16
  width (`reserve_at_widest_run_v1`, `kaspad/src/palw_prefill_run.rs:112`); for a dense class W is the whole prefill when it fits.
- **Compute is unchanged in M2.** A run calls the per-position kernels, so each position still reads the resident slab from RAM;
  a batched GEMM over W positions (what `forward_prefill_planned` and `BatchReplay` do) is a later CPU optimisation, outside the
  store.

### 5.5 Decode

A generated position reads the token the previous position's logits chose, so decode is position-major: each decode position
reads every streamed slab. The time of a position is about `max(Σ compute, streamed bytes / device rate)` at depth 1. A lockstep
batch amortises it (one cursor, §3.7 item 6): a slab is read once a position for every member. Duties that decode: free-prompt
answers, RFC-0004's generating evaluations, and jobs with more than one generated token where the prefill draw is not armed.

### 5.6 With the other placements

- **Pinned globals**: as today.
- **Routed**: admission unchanged; the routed capacity must hold one occurrence's routed rows, and in a run their union over its
  W positions to read each once (ADR-0117 Decision 2: "the same order admits a layer's routed union while that layer runs").
- **Gathered**: unchanged.
- **A mixture.** Qwen3-30B-A3B today: 1.28 GiB pinned, 1.69 GiB of routed rows a token, floor 3.01 GiB (`runtime-residency.md`
  §7). Streamed, its 0.86 GiB of per-layer attention and routers become 48 slabs of ≈ 18 MiB and its floor ≈ 0.4 GiB (§5.10).
- **For a mixture the run order matters more than the streaming.** `runtime-residency.md` §7 estimates a replay at "the cold
  expected union of its routes" — every expert read once (27.1 GiB for Qwen3-30B-A3B). The position-major executor reads that only
  when the routed capacity holds the whole union. At a fifth the LRU holds ≈ 2.65 tokens of rows, so under uniform routing a
  chosen expert is held with probability `1 − (1 − 8/128)^2.65 ≈ 0.16`, and a 255-position draw reads ≈ 0.84 × 1.69 GiB a position
  ≈ 363 GiB (the 64-expert test mixture measured 76 % misses at a fifth, the same section). An occurrence-major run holds one
  layer's union over the run (≤ 0.56 GiB) and reads each chosen expert once: ≈ 27 GiB, about 13× less, with the placement
  unchanged (a fifth is over today's floor, so nothing streams). Real routing is skewed and does better than uniform in both
  orders; the bench measures the uniform case (`tensor-store-bench.md` §7).
- **Composites**: one manager per parent root (`residency.rs:452-460`); candidates share slabs as they share rows; an adapter is a
  Memory-owned store under the same budget.

### 5.7 KV and state stay resident

`Fixed` and `Hist` instances are the executor's (`RunBufs`, `exec.rs:376-417`): never placed by the manager, never streamed. The
manager reports their bytes for the duty's need, the way `kv_resident_bytes` is reported today (`palw_backends.rs:960-965`):
every `Fixed` instance twice (current and pending), every history's rows at the job's positions (capped at its window), and the
kept tails. The executor's histories grow by doubling (`exec.rs:128-141`), up to twice their rows; M2 reserves them at the job's
positions instead (as the A16 cache does, `qwen25_a16_backend.rs:2517`), and until then they are charged at twice. At 8,192
positions the K/V of the shapes of §5.10 is 0.44 GiB (7B) to 2.75 GiB (123B) in `i16`; `PositionV1::state_bytes`
(`misaka-palw-tir/src/admit.rs:124-138, 780-788`) is the worst-case bound admission already computes.

### 5.8 The floor, and refusal by name

| term | today (ADR-0112 Decision 3, `tiers.rs:500-528`) | streamed (proposal) |
| --- | --- | --- |
| pinned | every pinned instance, the dense per-layer weights included | the pinned globals and the per-layer instances under 1 MiB |
| dense per-layer weights | — (inside pinned) | **the window**: the largest `depth + 1` consecutive slabs of the cyclic schedule — at depth 1 the widest two consecutive layers (N and N + 1) — times the cursors the node runs |
| routed | **one token's routed rows over every layer** | **one occurrence's routed rows** (all its route groups) |
| in flight | the largest single admission | unchanged |
| **floor** | pinned + routed a token + in flight | pinned globals + window × cursors + routed an occurrence + in flight |

Reported beside it, outside the weights' budget and reserved per duty as now: the state (§5.7) and the working set (the peak live
bytes, `admit.rs:788-822`, plus a run's carries).

**Refusal.** A stated budget under the streamed floor is refused at open with every term, as `residency.rs:485-490` refuses today:
*"a resident budget of B bytes is below this class's streamed floor of F: the pinned globals are G, the widest window is W
(occurrences o … o + d at prefetch depth d, for c cursors), one occurrence's routed rows are R and one admission in flight is I;
the smallest budget this class runs in is F (tensor-store.md §5.8)"*. A default under it declines to the page-cache baseline with
`TirResidencyDeclinedV1` extended by the streamed floor — never a refusal, because nobody stated a number (ADR-0112 Decision 2,
amended). A budget at or over today's floor streams nothing: placement is exactly today's.

**The routed term is a decision for the operator.** ADR-0112's floor holds one token's routed rows "so a forward never re-reads
what it read earlier in the same forward" (`runtime-residency.md` §3), and I-2/I-3 assert it. Under streaming a layer's rows are
needed only while that layer runs, so one occurrence's rows suffice to run; keeping the token term would make a mixture with
~20 GiB of active weights a token need ~20 GiB of floor for that term alone, against the 16–32 GB target. The per-occurrence term
applies in streamed placement only, amends ADR-0112 Decision 3 for it (node software), and needs the operator's OK before M2.

### 5.9 I/O mechanics

- **A dedicated I/O pool** (default 4 threads, `--palw-tensor-io-threads`). Today's residency reads on rayon
  (`residency.rs:225-233, 649-655, 729-734`), which is right when the forward waits for the read anyway; a prefetch on the compute
  pool would take workers from the kernels and could queue behind them.
- **Reads of 8 MiB** (`PARALLEL_READ_BYTES`, `residency.rs:182`), in extent order, a slab's extents queued together.
- **The page cache's second copy.** A slab read through the page cache leaves a copy there, and on Linux that page cache is charged
  to the reading cgroup. Two modes: `keep`, and `drop` (`posix_fadvise(POSIX_FADV_DONTNEED)` after the read on Linux; `F_NOCACHE`
  on the descriptor on macOS) for streamed slabs only. The default is the bench's to decide (`tensor-store-bench.md` §6).
- **Cursors are the concurrency.** One cursor per running forward — each replay slot (`--palw-seat-replay-slots`,
  `kaspad/src/args.rs:3187`), the producer, a lockstep batch. The manager is opened for `--palw-tensor-cursors` windows (default:
  the replay slots plus one); a further cursor waits. The ledger's role need (`PalwRoleMemoryNeedV1`, `palw_backends.rs:914-931`)
  gains `stream_window_bytes`, so a duty that would need a window the host cannot give is held by name, as today.
- **The open pass is unchanged**: every byte once, for the root and the ranges. For 200 GB it is ~4 min at the fleet's 845 MB/s.
  A sidecar of root and ranges keyed by file identity would skip it, and is an open question (§12), not part of this design.

### 5.10 What it comes to, by arithmetic

Computed from the shapes (`tensor-store-bench.md` §1 fixes them), `i8` projections plus `i64`/`i8` narrowing vectors and `i16`
activation tables in the bench program's conventions (`misaka-palw-tir-exec/src/bin/tir-exec-bench.rs:117-241`); a slab here
counts its layer's small vectors too, which §5.1 pins instead (≤ 2.4 MiB a layer moves between the columns). **Arithmetic, not
measurement**: the bench prints the measured terms beside these.

| shape | weights | slab | window (2 slabs) | pinned globals | today's floor | streamed floor (weights) | K/V at 8k |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Qwen2.5-1.5B | 1.67 GiB | 45 MiB | 0.09 GiB | 0.22 GiB | 1.45 GiB | 0.31 GiB | 0.22 GiB |
| Qwen2.5-7B | 7.12 GiB | 223 MiB | 0.44 GiB | 0.51 GiB | 6.61 GiB | 0.95 GiB | 0.44 GiB |
| Phi-4 (14.7B) | 13.69 GiB | 326 MiB | 0.64 GiB | 0.48 GiB | 13.21 GiB | 1.12 GiB | 1.56 GiB |
| Llama-3-70B shape | 65.81 GiB | 817 MiB | 1.60 GiB | 0.98 GiB | 64.83 GiB | 2.58 GiB | 2.50 GiB |
| Mistral-Large-2 shape (123B) | 114.32 GiB | 1,322 MiB | 2.58 GiB | 0.38 GiB | 113.94 GiB | 2.96 GiB | 2.75 GiB |
| Llama-3.1-405B shape, 64 layers | 194.06 GiB | 3,042 MiB | 5.94 GiB | 1.96 GiB | 192.11 GiB | 7.90 GiB | 2.00 GiB |
| Qwen3-30B-A3B (MoE) | 28.96 GiB (lowered) | ≈ 18 MiB | ≈ 0.04 GiB | 0.29 GiB | 3.01 GiB (lowered) | ≈ 0.40 GiB (+ 36 MiB routed, + 36 MiB in flight) | 0.75 GiB |

What a decode position (or a prefill run) then reads from SSD — the streamed slabs, at the budget named:

| shape, budget for weights | resident slabs | streamed bytes | at 845 MB/s (fleet `dd`, ADR-0112 §1) | at 3 GB/s (an NVMe, illustrative) |
| --- | --- | --- | --- | --- |
| 7B, a fifth (1.42 GiB) | 2 of 28 | 5.66 GiB | 7.2 s | 2.0 s |
| 14B, a fifth (2.74 GiB) | 5 of 40 | 11.14 GiB | 14.2 s | 4.0 s |
| 123B, 13 GiB (a 16 GiB cgroup) | 7 of 88 | 104.5 GiB | 133 s | 37 s |
| 123B, 28 GiB (a 32 GiB cgroup) | 19 of 88 | 89.1 GiB | 113 s | 32 s |
| 405B@64L, 28 GiB (a 32 GiB cgroup) | 6 of 64 | 172.3 GiB | 219 s | 62 s |

A prefill run of W positions reads the same bytes once for all W: a producer's 255-position attempt on the 123B shape in a
16 GiB cgroup reads ≈ 105 GiB once (about 2 min of I/O at 845 MB/s) against ~31 TMAC of compute — CPU-bound on an 8-vCPU host.

## 6. Producer and panel in one plan

| duty | code | order under the store | reads from SSD per job, streamed |
| --- | --- | --- | --- |
| producer attempt | `TirBackendV1::execute` → `capture_run` → `TirClassRunnerV1::run` (`backend.rs:1070-1073, 329-375`; `run.rs:168-178`) | occurrence-major over the prefill (M4) | the streamed slabs once (W = P), plus routed misses |
| producer, generating jobs | the same runner, decode positions | position-major; lockstep where several jobs share a store | the streamed slabs once a position |
| full-seat replay | `execute_for_verdict` (`backend.rs:1156`) | occurrence-major over the prefill (M3) | once a run |
| cell (RFC-0006) | `verify_cell_v1` (`cell.rs:206-216`) over the shard's params | occurrence-major over the segment (M3), as RFC-0006 §1.4 specifies | the shard's slabs once a run |
| shard-only outsider | `TirRowPursuitV1` fill → Shard spill → cells | as a cell | its shard, once fetched and spilled; then as a cell |
| court close, openings | `TirParamOpenerV1` on the artifact (`artifact.rs:663-686`), pieces through `TirByteSourceV1` | `read_range` of ≤ 32 KiB pieces | the pieces |
| court re-derivation | `TirEvidenceV1` replays from resume points (`node/evidence.rs`) | occurrence-major from the resume point | once a run |
| readiness | `artifact_readiness_material` (drawn leaves) | `read_range` | the drawn leaves |
| RFC-0004 evaluation | `TirStageStepperV1`, `TirLockstepHubV1` (`stage.rs:65, 192`) | one cursor per hub batch | once a position for the batch |

**Why the producer is the case that matters.** RFC-0006 already lets a verifier hold one shard. A producer runs every layer, and
until now that meant every layer in memory — or, for a dense class, the page cache. On t12 an attempt is a prefill and nothing
else: wherever `palw_offence_attribution` is armed, `palw_prefill_draw` must be armed at genesis (`consensus/core/src/config/params.rs:4136-4146`),
and the drawn job is the canonical one at one decode step (ADR-0117 Decision 1), i.e. its `P` prompt positions, the last one's
logits choosing the token. So an occurrence-major producer reads each streamed slab **once per draw**: the 123B shape's 105 GiB is
~2 min of I/O on the fleet's disks and ~40 s on a 3 GB/s NVMe, beside its compute.

**"A 200 GB artifact is not 200 GB of RAM."** The Llama-3.1-405B shape cut to 64 layers is 194 GiB on SSD. In a 32 GiB cgroup:
28 GiB for weights (2 GiB of globals, a 5.94 GiB window, 6 resident slabs), 2 GiB of K/V at 8k, the rest working set — it runs. A
draw of 255 positions reads ~172 GiB once (≈ 3.6 min at 845 MB/s); a decode position reads the same again. Slow, and runnable. At
M5 a 12–24 GB device holds the window and some resident slabs, and the host's RAM becomes the staging tier between SSD and device.

## 7. Bit-exactness: tiers decide where bytes are, never which

| | invariant | how it is held |
| --- | --- | --- |
| S-1 | every placement, budget, depth, run width, cursor count and backend computes the same roots, leaves, captures, readiness material, openings and court closes as today's executor over the mapped artifact | `residency_node.rs`'s `Seen` (`tests/residency_node.rs:77-128`: inventory root, execution / trace / output roots, material, readiness, openings, closes) over every corpus program and the 64-expert mixture, at: the page cache, locked, everything resident, today's floor, a fifth, the streamed floor, the streamed floor + one slab, the streamed floor at depth 0, 1 and 3 |
| S-2 | the executor equals the reference interpreter with every streamable instance streamed | the 6,000 random programs, the corpus and the goldens (`tests/residency.rs:252-312`) under a "stream everything" rule (streamable at 0 bytes, as `pin_below_bytes: 0` serves every row-addressed param today, `tiers.rs:49-51`) |
| S-3 | an occurrence-major run equals the position-major steps | every program above at W ∈ {1, 2, 3, 7, 64, the whole job}, both sinks (commit only and every node), fused on and off; a failing program reports the same error class at the same position (the random generator's failing programs) |
| S-4 | where the bytes come from cannot matter | a fault-injecting store: prefetch completions delayed and reordered, transient read failures (the pin re-reads and names the failure), evictions at random points that honour leases — every result equal to S-1's |
| S-5 | the budget holds | between admissions and between occurrences the manager's held bytes never exceed the budget (I-2's analogue); no served or streamed instance is read whole on a planned path (`whole_reads == 0`, as I-4) |
| S-6 | the floor is refused by name and runs at itself | a stated budget one byte under the streamed floor is refused with every term; the floor itself opens and runs (I-3's analogue) |
| S-7 | lockstep and cells | a lockstep batch over one cursor equals each member alone (`residency_node.rs::candidates_stepped_in_lockstep_*`); a cell verified occurrence-major gives the verdict the position-major cell verifier gives — the lowest faulting leaf, `Unavailable`, `TokenFault` — on the RFC-0006 drill's lies (`tests/cell.rs`) |
| S-8 | a shard spill serves what the holder proved | a shard-only seat's params, openings and closes, rebuilt from its spill, equal the in-memory holding's and the full holder's; the lying-holder refusals stand (the shard fetch tests, `tests/cell.rs:899-1040`) |
| S-9 | M5 | a device placement equals the CPU (the F-4 gate, `gpu-integer-backend.md` §7) |

The bench repeats the check at full size: every run's digest — the roots, the leaf count and a hash over every leaf hash in order,
or for executor-only runs a hash over every committed value in sink order — equals the reference configuration's
(`tensor-store-bench.md` §2).

## 8. Integration plan (node-only, no consensus fence)

Every step is a node release in the int-12.x line after the DAA 5,300 deployment, behind a flag, A/B on one node first (a host
the operator chooses that holds an IR class), then the fleet. A step that changes a byte any commitment sees is a bug, not a
step: the drift check prints no row, receipts and verdicts are identical between the A and B nodes. Rollback is the flag; the
previous path stays in the binary until the next step is stable on the fleet, and a binary is rolled back with the kit's
`upgrade-rollback`, never `switch`.

| step | scope | flag | decided by | rollback |
| --- | --- | --- | --- | --- |
| **M1** the trait, existing tiers behind it | `PalwTensorStoreV1` + Memory (owned, locked) / Pread / page-cache baseline; `TirResidencyManagerV1` wrapping today's store with today's placements; `palw_store_choice_v1` (§3.5), which removes the pin of an IR container a residency decides (F2); the Qwen3.6 adapter; one accounting line and status keys; the Shard backend as an adapter over today's in-memory holding | `--palw-tensor-store=v1` (default `legacy`) | bench M1 cells: digests equal, bytes read ±2 %, peak memory +5 % at most, times ±5 %; on the A/B node 24 h: identical receipts, the storage line's bytes per replay equal, RSS ±5 %, `pinned_mib` 0 for an IR holding under a residency | `--palw-tensor-store=legacy` |
| **M2** dense streaming | streamed and resident placement, cursors, the I/O pool, the page-cache mode, the streamed floor and its refusal; the executor's lease, `step_run`, `run_eligible`; history reservation | `--palw-tensor-stream=on` (default `off`), `--palw-tensor-prefetch-depth`, `--palw-tensor-run-positions`, `--palw-tensor-io-threads`, `--palw-tensor-page-cache`, `--palw-tensor-cursors` | the M2 thresholds (`tensor-store-bench.md` §8); then one node: a dense class at a fifth replays and verdicts identically to its twin, its storage line's bytes per replay within 5 % of the arithmetic | `--palw-tensor-stream=off` |
| **M3** verifiers | cells occurrence-major over the segment (RFC-0006 §1.4); the Shard backend's spill (fill → verify → spill → Pread) and openings on disk; the inventory tree's lower levels recomputed from the file instead of held (F9); court re-derivation as runs | rides `--palw-tensor-stream` | cell seconds a position ≤ the position-major verifier's at the same budget; verdict identity on the RFC-0006 drill; a shard-only seat's RSS bounded by its window, not its shard; `Unavailable`/`Refused` rates unchanged on the A/B seat | the same flag; cells fall back to position-major |
| **M4** producer | `drive` occurrence-major over the prefill with leaves by index; decode streamed; a lockstep cursor for jobs sharing a store | `--palw-tensor-producer-runs=on` | draw time, bytes read per draw within 5 % of the arithmetic, peak memory under the budget; on one producer: claims Final, no court loss, no `Mismatch` | the flag; the producer steps position-major |
| **M5** device residency | `vram_bytes` live; placement "resident where" (host or device; pinned, resident, streamed); host → device slab streaming; the device pool armed from it (`arm_device_share_v1`); occurrence-major runs on `BatchReplay`'s form | `--palw-tensor-vram-bytes` | only after the GPU backend leaves the prototype: in the node build behind a feature, the F-4 gate on every device class, the startup self-test (B-5); then S-9 and the bench's device cells | the flag; the CPU runs everything |

Order inside the line: M1 alone first (it moves no byte and fixes F2); M2 next, on the bench before any fleet host; M3 and M4 may
swap — M4 first if producers are the bottleneck when M2 lands, M3 first if seats are.

## 9. Operator surface

- **New flags** (all resource choices, never duties): `--palw-tensor-store=legacy|v1`, `--palw-tensor-stream=off|on`,
  `--palw-tensor-prefetch-depth=N` (1), `--palw-tensor-run-positions=auto|W`, `--palw-tensor-io-threads=N` (4),
  `--palw-tensor-page-cache=drop|keep`, `--palw-tensor-cursors=N` (replay slots + 1), `--palw-tensor-producer-runs=off|on` (M4),
  `--palw-tensor-vram-bytes=N` (M5; refused before).
- **Kept, unchanged in meaning**: `--palw-class-resident-bytes` (`0` = no residency, as today), `--palw-no-artifact-pin`,
  `--palw-artifact-pin-max-bytes`, `--palw-host-pinner`, `--palw-prefill-run-max`, `--palw-seat-replay-slots`.
- **The load line**, one per holding: `[palw-store] <class> backend=<kind> budget=… pinned=… resident_slabs=k/n window=…
  cursors=c streamed_floor=… (today's floor …) routed_capacity=… state@positions=…` — or the refusal / the decline with its terms.
- **The per-duty line** (beside the storage line of ADR-0112 Decision 8): bytes read, slab hits / waits and stall seconds, routed
  hits / misses / evictions, gathered rows, the process's device bytes (`/proc/self/io` on Linux).
- **`getPalwNodeStatus.verification`** gains `store_kind`, `store_budget_mib`, `store_resident_mib`, `store_streamed_mib`,
  `store_stall_ms`, `store_cursors` — new keys in the existing `key=value` string, so the wire does not move (as int-10.2 did).

## 10. Where the code today disagrees with the sketch

- **F1 — mmap is not only a baseline today.** The 8k A16 path reads its weights in place from a mapping (`Int8SlabV1::Mapped`,
  `misaka-palw-base0/src/artifact.rs:261-272`), and int-10.2 made that mapping `MAP_SHARED` + populated + `mlock`ed on purpose
  (`palw_artifact_pin.rs:1-60`; the host pinner, `main.rs:27-43`) so a host's seats share one copy. A demand-paged mapping goes to
  the baseline; the locked shared mapping stays, as the Memory backend's shared form (§3.4).
- **F2 — the pinner locks an IR container that a residency decides.** `palw_pin_lineage_v1` answers `Ok` for the IR lineage with no
  condition (`palw_artifact_pin.rs:141-152`), `palw_pin_eligibility_v1` asks nothing about a residency (`:157-169`), and the pin is
  taken before the policy is applied (`palw_backends.rs:1284-1286`) — against the module's own doc ("any later residency of the IR
  tier — decides its own resident set", `:47-51`) and `t12-replay-memory-1001.md` §3. An IR class opened under a residency would
  then hold its budget in owned memory AND its whole file locked. Dormant on today's t12 — the IR container its seats hold (1.74 GiB,
  `t12-replay-memory-1001.md` §3) is dense, so its default residency declines to the page cache, where the pin is right — and live
  the day a routed IR class is held or a budget is stated. M1 removes it by making the choice once (§3.5).
- **F3 — the executor and the cell verifier are position-major.** `drive` steps one position through every occurrence
  (`run.rs:317-337`) and so does the cell verifier (`cell.rs:274-313`), although RFC-0006 §1.4 specifies a cell "one layer at a time
  over every position of the segment". Only base0 (`engine_a16.rs:2005`, `qwen36_plan.rs:485`) and the GPU prototype
  (`misaka-palw-tir-gpu/src/batch.rs:184`) run layer-major. With "N resident, N + 1 prefetched, N − 1 evicted" alone, every position
  re-reads every streamed layer — the sketch needs the occurrence-major run (§5.4), an executor change, not only a store. The same
  order is why `runtime-residency.md` §7's replay estimate (every expert read once) does not hold on today's TIR path at a fifth:
  ≈ 363 GiB against ≈ 27 GiB for a 255-position Qwen3-30B-A3B draw under uniform routing (§5.6).
- **F4 — the prefetch is deterministic for dense slabs only.** Routed rows are the router's choice at run time; ADR-0112 Decision 4
  calls prefetch of experts across layers impossible, and the code admits them at the route group's first gather (`rows.rs:110-132`).
- **F5 — the floor's routed term is one token over every layer** (`tiers.rs:525`; refused at `residency.rs:485-490`; I-2/I-3). For a
  large mixture that term alone outgrows a 16–32 GB host. Streaming needs a per-occurrence term (§5.8) — an amendment of ADR-0112
  Decision 3 for streamed placement, the operator's to approve.
- **F6 — params are bound for the executor's life** (`exec.rs:437-440`; `'static` by `held_forever`, `artifact.rs:103-111`), so a
  streamed instance needs a lease through `Reader` (§3.7).
- **F7 — the unplanned whole read is held forever** (`params.rs:118-124, 314-325`): unbounded under a budget; it becomes a lease.
- **F8 — a shard-only seat holds its shard in RAM twice**: the assembled params (`shardrows.rs:102-117`) and every opening with its
  operand bytes, kept for the court (`:55-57, 99`). At 200 GB and four shards that is 2 × 50 GB; the Shard backend spills (M3).
- **F9 — the inventory tree holds every level** (`inventory.rs:28-31`): about 2 × leaves × 64 B (`tir-exec-bench.rs:489-493`), ≈ 0.78 GiB
  for 200 GB of 32 KiB pieces, built on the first opening and outside any budget.
- **F10 — histories grow by doubling** (`exec.rs:128-141`): up to twice their rows; charged at twice until M2 reserves them.
- **F11 — the 8k replay is not on the TIR executor.** It runs base0's A16 engine over `Int8SlabV1`, which this plan does not move
  onto a streamed path (one locked copy per host costs less than a stream per seat). In M1–M4 it is the regression guard and the
  measurement of the locked Memory backend, not a workload streaming speeds up.
- **F12 — two traits are called `KernelBackendV1`**: the node-grain seam specified in `gpu-integer-backend.md` §8 (hold / view /
  record / …) and the cell-grain trait implemented in `node/cell.rs:86-102`. M5 integrates against one of them and renames the other.
- **F13 — the bench's synthetic activation table uses `f64` `exp`** (`tir-exec-bench.rs:273-281`), which macOS libm and glibc need
  not round alike, so a container written on the Mac and on Linux may root differently; the bench pins an integer-only table
  (`tensor-store-bench.md` §9, H1).
- **F14 — the page cache is a runtime fallback, not only a baseline.** ADR-0112 Decision 2 (amended) sends a default budget below
  the floor to the page cache — "never a refusal" (`residency.rs:476-484`) — and today that is how a dense IR container runs
  wherever the lock is refused. The design keeps that fallback, logged and counted; streaming makes it rare (the 7B streamed floor
  is ≈ 1 GiB against today's 6.6). Removing it would refuse a node whose host can spare nothing, which ADR-0112 rejected.

## 11. Out of scope

- **The llama.cpp worker classes.** `misaka-palw-worker` (the pinned palw-lite runtime behind the subprocess contract) and
  `misaka-palw-agent` (its supervisor) load weights through llama.cpp; nothing here touches them.
- **Anything consensus**: objects, acceptance, fences, parameters, class ids, fingerprints, the container and inventory formats,
  the step space, the court. The cost formulas of RFC-0002 §5.3 are not used to admit anything here; the manager computes its own
  node-side arithmetic.
- KV/state offload, weight compression, remote streaming for producers, a host-wide residency service shared by processes
  (the locked mapping covers the shared case that exists today), the open pass's sidecar.
- The base0 engines' internals (A16, Qwen3.6): adapters and accounting only.

## 12. Open questions for the operator

1. **The routed term** (§5.8): approve the per-occurrence term for streamed placement (an ADR-0112 Decision 3 amendment)?
2. **The page-cache mode** for streamed slabs (`drop` or `keep`) — the bench measures both; the default follows it.
3. **The resident subset** (evenly spaced or a prefix) — default evenly spaced unless the bench says otherwise.
4. **The hosts.** The A/B node of each step is a fleet node the operator picks (a seat that holds an IR class; a producer for
   M4), running the flag after the bench has passed. The bench's own Linux host (`tensor-store-bench.md` §3) is a separate machine
   that runs no public node; who provides it is open.
5. **A root-and-ranges sidecar** keyed by file identity, to skip the open pass of a large artifact at restart: worth an ADR of its
   own (it changes what a node proves at startup), not part of this one.
6. **An ADR number** for the M2 amendment, taken when M2 is implemented (number hygiene: the next free one then).
