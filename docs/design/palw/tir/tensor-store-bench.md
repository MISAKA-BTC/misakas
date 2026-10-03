# Tensor-store benchmark — the conditions, fixed before any implementation

> **Status: FIXED 2026-10-03, nothing has run** (lane TS, branch `tir/tensor-store-design` off `rcore/int-12` @ `4d5f97d58`).
> Companion of [`tensor-store.md`](tensor-store.md): this file says what is measured, on what, how, against what, and what counts as
> "M2 improved it", so that the answer is decidable when the code exists. Nothing here runs tonight; nothing here runs on a host
> that runs a public node; no weight is downloaded. §7's numbers are **arithmetic predictions**; §8's thresholds are **proposals
> for the operator**. A change to any condition after the first measurement is a new revision of this file with its reason, and
> the results under the old one are kept.

## 概要(日本語)

- **ワークロード 4+1 種:** ① Qwen3-30B-A3B 形の MoE ② 実物の 8k replay(`qwen25-1.5b-a16-8k.palwart`、A16 エンジン)③ 密 7B
  (Qwen2.5-7B 形)と 14B(Phi-4 形)④ ≥100 GB の密(Mistral-Large-2 形 123B、114 GiB)— 任意で ④b 405B 形 64 層(194 GiB)。
  重みは実形状の合成値(`tir-exec-bench` の決定的な xorshift、instance ごとの seed)。HF からは何も落とさない。
- **指標:** peak RSS、SSD から読んだバイト(Linux `/proc/self/io`、macOS `proc_pid_rusage`)、実効 GB/s、producer の
  positions/s・tokens/s、検証 s/position(seat・cell)、prefetch hit 率、bit-exact(roots と全葉 hash の digest 一致)。
- **ハード:** この Mac Studio(M1 Max・32 GiB・APFS 内蔵 SSD・空き 36 GiB)と、fleet に似た Linux virtio VM(公開ノードを
  動かしていない専用機、cgroup v2 で上限)。④ は ≥150 GB の空きが要るので Mac では不可・今夜は実行しない。
- **基準線:** page cache、int-10.2 の pin、今の TirResidency(全保持・floor・1/5)、今の A16 8k 経路。
- **M2 合格案:** bit-exact 100 %、上限内で OOM なし、読みバイトが算術予測の ±5 %、ストリーム中の読み速度 ≥ 0.7×デバイス、
  prefill run の prefetch hit ≥ 95 %、7B・14B を 1/5 予算で prefill 速度 ≥ 0.9×(全保持比)、何もストリームしない
  場合は ±5 % で無劣化、123B が 16/32 GiB cgroup で 1 draw 完走、MoE の 255 位置 draw の読みが今の ≈1/10。

## 0. Rules

1. **Hosts.** The maintainer's Mac and a dedicated Linux VM (§3). Never ibm, C, .113, 5.104 or any b-host, or any machine that runs
   a public node or a seat; never while a node, a drill or a cargo build runs on the bench host.
2. **Weights.** Synthetic, deterministic, at real shapes (§1.3); the only real artifact is ②'s, already on the Mac.
3. **Builds.** `--release`, the features the node ships with, the toolchain of `rust-toolchain.toml`; the build's git sha is
   recorded with every result.
4. **Disk.** The bench deletes what it generated when a workload's cells are done; a container is regenerated, not kept, when
   disk is short — its inventory root proves the regeneration is the same file (§1.3).

## 1. Workloads

### 1.1 The five workloads

| id | what | program | geometry | weights (arithmetic) | file |
| --- | --- | --- | --- | --- | --- |
| **W1** | Qwen3-30B-A3B shape, MoE | S1-MoE (§1.2); cross-checked against the lowered real config | 48 layers, d 2,048, 32 / 4 heads × 128, 128 experts, 8 a token, expert ff 768, vocab 151,936, untied head | 28.96 GiB in the real lowering (`runtime-residency.md` §7: 1.28 pinned, 27.09 routed, 1.69 a token, floor 3.01, fifth 5.79) | generated |
| **W2** | the 8k replay | the real class `Qwen/Qwen2.5-1.5B/graph-v7@8192` on base0's A16 engine | Qwen2.5-1.5B | 1,799,359,436 B, manifest digest `f4af38d9…`, class `ebf44d0a…`, inventory root `88096dc1…` (`consensus/core/src/config/class-manifests/qwen25-1.5b-a16-8k.palwmanifest`) | `/Users/wata/pret12/art/qwen25-1.5b-a16-8k.palwart` (Mac); copied with its manifest to the Linux host, digest checked |
| **W3a** | dense 7B | S1 (`qwen2_program`) | Qwen2.5-7B: 28 layers, d 3,584, 28 / 4 × 128, ff 18,944, vocab 152,064 | 7.12 GiB | generated |
| **W3b** | dense 14B | S1 | Phi-4: 40 layers, d 5,120, 40 / 10 × 128, ff 17,920, vocab 100,352 | 13.69 GiB | generated |
| **W4** | dense, ≥ 100 GB | S1 | Mistral-Large-2: 88 layers, d 12,288, 96 / 8 × 128, ff 28,672, vocab 32,768 | 114.32 GiB (≈ 122.8 GB) | generated, Linux only |
| W4-t4 | W4's reference | S1 | W4 at 4 layers | ≈ 5.9 GiB | generated |
| W4b | dense, the "200 GB" claim (optional) | S1 | Llama-3.1-405B shape cut to 64 layers: d 16,384, 128 / 8 × 128, ff 53,248, vocab 128,256 | 194.06 GiB | generated, Linux only, ≥ 260 GB free |

### 1.2 The programs

- **S1** — the bench's own programs, written in the dense lowering's conventions (`misaka-palw-tir-exec/src/bin/tir-exec-bench.rs:117-241`:
  `i32` residual carry, `i8` weights `[out, in]` read by `MatMul → i64` and narrowed per channel `(m, s, z)`, the wide RMS norm,
  two-level RoPE tables, GQA over `k`/`v` histories, SiLU as a 65,536-entry `i16` table, the LM head over the whole vocabulary).
  The bench already runs it at three geometries (`:52-57`, `--geo 1.5b|32b|70b`); W3a, W3b, W4 and W4b are four more.
- **S1-MoE** (new, harness change H1) — the same conventions plus a mixture as the lowering writes one (`runtime-residency.md` §2):
  a router `MatMul` → `TopK` of 8, the expert stacks `[E, out, in]` per layer gathered by the route group's index, the
  per-(expert, row) scales `[E·rows]` reshaped to `[E, rows]` and gathered by the same index, `moe_combine_q36` for the weighted
  sum, q/k norms. `TirTiersV1::of` must classify the stacks and their scales `Routed`.
- **S2 cross-check** (H5) — the real configs in the repo, lowered at real shapes with no weight read, as
  `misaka-palw-tir-lower/tests/residency_tiers.rs:100-157` does: `tests/configs/real/qwen3-30b-a3b.json` (W1),
  `qwen2.5-7b-instruct.json` (W3a), `phi-4.json` (W3b). S1's tier arithmetic (weights, pinned, routed, routed a token, in flight,
  the per-occurrence slab bytes) must be within ±3 % of S2's, or the report uses S2's numbers for sizing and says so. W4 and W4b
  have no config in the repo; their arithmetic is S1's own.

### 1.3 Synthetic weights, deterministically

- **The generator is the bench's container writer** (`tir-exec-bench.rs:424-458`): the PALWTIR1 container is written one instance
  at a time, never all in memory; each instance is filled by an xorshift64* (`Rng`, `:243-255`) seeded
  `0x9e37_79b9_7f4a_7c15 ^ (j << 32) ^ layer.map_or(0xFFFF, |l| l)` (`:431-432`), by the param's fill kind (`:257-284`):
  `W8` — weight codes uniform in [−127, 127]; `M(lo, hi)` — narrowing multipliers; `S` — a constant shift; `Z` — small biases in
  [−64, 64]; `Rope` — Q24 angles; `Table` — the activation table.
- **The activation table becomes integer-only** (H1). Today it is computed with `f64` `exp` (`:273-281`), which macOS libm and glibc
  need not round alike (`tensor-store.md` §10, F13). The bench replaces it by an integer formula, so the bytes are the same on every
  host.
- **Mixture fills**: router weights `W8`, stacks `W8`, scales `M`. Routing is then close to uniform: the pessimistic case of the
  expected union (`TirTiersV1::routed_union_bytes`, `tiers.rs:530-545`). One informative cell biases the router toward 16 experts a
  layer (a skewed fill), to show the sensitivity; it decides nothing.
- **Identity.** After writing, the harness prints the container's inventory root (the open pass) and its file digest; both go in
  the results, and a regeneration on any host must reproduce both. A mismatch voids every cross-host comparison of that workload.
- **Layout.** A declared layout is written into the container (H1; today the bench writes none, `:439`), so a container can run
  jobs: `max_context` 2,048 (W4b: 1,024), checkpoint interval 64, `h_tile` 64, commit tiles of 64 lanes, state tiles of 64 lanes.

### 1.4 Where the files live, and the disk each needs

| host | directory | can hold |
| --- | --- | --- |
| H-MAC | `~/Downloads/MISAKA-wt-b/bench-tensor-store/` (internal APFS) | 36 GiB free on 2026-10-03: W2 in place; W3a (7.1 GiB) and W3b (13.7 GiB), one at a time; W1 (≈ 29 GiB) only after freeing ≥ 45 GiB (stale build targets, per the standing 09-28 rule); **never W4 / W4b** |
| H-VIRTIO | `/data/tsb/` on a dedicated volume | ≥ 200 GB free: W4 (≈ 123 GB) + W4-t4 + results; ≥ 260 GB for W4b |

Writing W4 at ~0.5 GB/s is about 4 minutes, W4b about 7.

### 1.5 The jobs

| id | job | how it runs | used by |
| --- | --- | --- | --- |
| **J-P(P)** | a producer attempt: `P` prompt positions, the last one's logits choosing the one token (the prefill draw, ADR-0117 Decision 1) | `TirBackendV1::execute` on the anchor `Hash64::from_u64_word(0x8000_1001)` (the 8k test's anchor) — the leaves hashed, the roots formed, the capture kept as the node keeps it (dense while it fits the dense-capture bytes, a fold beyond) | W1 P=255; W3a/W3b P=255 and P=1023; W4 P=255; W4-t4 P=255; W4b P=64 |
| **J-R** | a full-seat replay of J-P's job | `execute_for_verdict` (`backend.rs:1156`) | W3a, W3b (P=255) |
| **J-D(P+D)** | a generating job: `P` prompt positions then `D` generated tokens | the runner over a job of `D` decode tokens | W1 64+8; W3a/W3b 64+8; W4 16+4; W4b 8+2 |
| **J-C** | RFC-0006 cells: `S_L` = 4 shards, `S_P` = 1 segment, over J-P's dense capture | `tir_verify_capture_cells_over_v1` (`node/cell.rs:796-828`) per shard | W3a P=63 (informative in M2; M3's gate) |
| **J-X(P+D)** | executor-only, no commitment: today's bench loop | `TirExecutor::step` with tokens `(i·7919 + 1013) mod vocab` (`tir-exec-bench.rs:380, 498`) | the M1 regression cells |
| **J-8k** | the 8k seat replay | canonical job (1023, 2), anchor `0x8000_1001`, `execute_for_verdict` at widths 64 / 32 / 16 / 8 — the existing harness `a_real_8k_replay_at_every_width_commits_one_set_of_roots` (`misaka-palw-base0/src/qwen25_a16_backend.rs:4694-4751`); also the attempt job (1023, 1) | W2 |

## 2. Metrics

| metric | definition | Linux | macOS |
| --- | --- | --- | --- |
| **peak RSS** | the process's peak resident memory over the measured phase, split anon / file where the OS says | `VmHWM` (`/proc/self/status`), `RssAnon` / `RssFile` sampled at 1 Hz; under a cap, the cgroup's `memory.peak` (kernel ≥ 5.19) — the number the cap is judged by | `ri_lifetime_max_phys_footprint` (`proc_pid_rusage`, `RUSAGE_INFO_V4`) and `ps -o rss` sampled at 1 Hz. A footprint excludes clean file-backed pages, so the page-cache baseline is compared by device bytes and wall, not footprint |
| **bytes read from SSD** | bytes the process caused the device to read, per phase (open, run) | `read_bytes` deltas of `/proc/self/io`; major faults from `/proc/self/stat` | `ri_diskio_bytesread` deltas; page-ins `ri_pageins` |
| — beside it | the store's own count (`bytes_read`, residency.rs:197-221), always printed | app ≥ device when the page cache serves | same |
| **effective GB/s** | (a) device bytes ÷ the phase's wall; (b) streaming efficiency = (a) ÷ `R_dev`; (c) the store's bytes ÷ its I/O threads' busy time | | |
| **producer throughput** | J-P: positions/s = P ÷ the run's wall (open excluded) and the draw's seconds; J-D: tokens/s over the decode positions, median / min / max per token | | |
| **verification** | J-R: seconds a position; J-C: seconds a position per cell (the cell's wall ÷ its segment's positions), per shard | | |
| **prefetch** | hit rate = streamed occurrences whose slab was resident or fully read at `begin` ÷ streamed `begin`s; stall seconds = Σ waits in `begin`; bytes prefetched and evicted unused | | |
| **bit-exact** | J-P / J-R: `Hash64(execution_root ‖ trace_root ‖ output_root ‖ step_merkle_root ‖ leaf_count ‖ H(every leaf hash in order))`; J-D / J-X: a hash over every committed value in sink order and every logits row (a digest sink, as the GPU bench digests per position, `gpu-integer-backend.md` §11); J-8k: `(execution_root, trace_root, work_leaves, output_root)` — equal to the reference configuration's (§5) | | |
| **open** | the open pass: seconds, bytes, and the root (also the identity check of §1.3) | | |
| context | user + sys CPU ÷ wall ÷ cores; rayon threads; I/O threads; the store's mode and every flag | | |

## 3. Hardware profiles

| profile | what | memory cap |
| --- | --- | --- |
| **H-MAC** | this Mac Studio: Apple M1 Max, 10 cores, 32 GiB, internal SSD (APFS), Darwin 25.6.0 | no cgroup on macOS: a cell's cap is checked against the footprint, not enforced; nothing over 16 GiB (32 GiB is the whole machine) |
| **H-VIRTIO** | a Linux x86-64 VM like the fleet's hosts: 8 vCPU, 23–24 GiB (ADR-0112 §1, §10.5), virtio disk, kernel ≥ 5.19 with cgroup v2, swap present but off for the bench scope, a dedicated volume ≥ 200 GB, `dd iflag=direct` ≥ 500 MB/s | `systemd-run --scope -p MemoryMax=<cap> -p MemorySwapMax=0 --`; the 16 GiB profile |
| **H-VIRTIO-L** | the same instance type with ≥ 48 GiB | the 32 GiB profile (W4, W4b) |
| H-NVME (informative) | a Linux host with local NVMe, to show the ceiling | as H-VIRTIO |

**Who provides H-VIRTIO** is the operator's open question (`tensor-store.md` §12, 4): a separate VM of the fleet's instance type,
running nothing else.

**The device's own rate, `R_dev`, measured per host per day** before the cells and recorded with them:
- Linux: `fio --name=seq --filename=<a cold container> --rw=read --bs=8m --direct=1 --ioengine=psync --numjobs=4 --size=8g`,
  cross-checked by ADR-0112 §1's method (`dd if=<file> of=/dev/null bs=4M iflag=direct skip=2048 count=128`: 512 MiB at 8 GiB).
- macOS: after `sudo purge`, the harness's `--calibrate` (8 MiB positional reads on 4 threads over 8 GiB of a container, with
  `F_NOCACHE` on the descriptor).

## 4. Cache state, repeats and hygiene

- **Cold.** Linux: `sync; echo 1 | sudo tee /proc/sys/vm/drop_caches` before each cold run (without root: `vmtouch -e <file>`), and
  `fincore <file>` must show no page resident. macOS: `sudo purge`. Either way a cold run whose device bytes come under 0.9× its
  predicted cold bytes (§7) was warm and is discarded.
- **Warm.** The same command again with no eviction. On Linux inside the cap — "capped-warm": the page cache the process reads is
  charged to its cgroup, so a warm run cannot keep more of the file than the cap allows. On the Mac a warm run can be helped by up
  to 32 GiB of page cache; Mac warm numbers are reported and never decide a threshold.
- **The pin baseline** is cold only at its load (locking is its point); its runs are warm by construction.
- **Repeats.** One warm-up run discarded (thread pools, allocator); then 5 measured warm runs or 3 cold ones; W4 and W4b one
  measured run per cell (each is long) plus its digest. Reported: median, min and max. A cell whose coefficient of variation passes
  10 % is run once more and both sets are kept.
- **Hygiene.** Before a run: `pgrep -fl 'kaspad|cargo|rustc'` empty, load average under 1.0 for 60 s, no slot held in
  `lanes/cargo-slot.sh`, no other bench running; the Mac on power with `caffeinate -dims`; on Linux the CPU governor recorded (and
  `performance` where allowed).
- **Recorded with every result**: git sha, `rustc -V`, the CPU model and core count, RAM, kernel or macOS version, file system and
  mount options, `R_dev` of the day, `RAYON_NUM_THREADS` (default: all cores), the store's I/O threads.

## 5. Baselines — measured with today's code

| id | what | for |
| --- | --- | --- |
| **B-PC** | the page cache: `TirArtifactV1::open`, mapped and unpinned (`--resident-bytes 0` in the bench today) | every TIR workload; on W4/W4b once, with a 2-hour wall cap and its fault rate recorded — at the fleet's 6–11 MB/s a J-P is not expected to finish (ADR-0112 §1), and that is the result |
| **B-PIN** | mapped and pinned, int-10.2 A1 (`MAP_SHARED`, populated, `mlock`) | W2 (R3); W3a on H-MAC as an informative cell (7.1 GiB within ¼ of 32 GiB); never W3b and up (over the cap) |
| **B-ALL** | the TIR residency holding every byte (budget = the weights), uncapped | W1, W3a, W3b: the speed reference |
| **B-FLOOR** | the TIR residency at its floor (`Bytes(floor)`) | W1 (3.01 GiB); for a dense class it is B-ALL |
| **B-FIFTH** | the TIR residency at a fifth | W1 (it holds); a dense class: a stated fifth is refused by name and a default fifth declines to the page cache — the message is the result |
| **B-A16** | the current A16 8k path, widths 64 / 32 / 16 / 8 | W2 |

Under a 16 or 32 GiB cap, B-ALL and B-FLOOR of W4/W4b are refused by name; the refusal is recorded.

**The reference configuration of each bit-exact check:** B-ALL for W1, W3a, W3b; for W4 and W4b, (i) W4-t4 under B-ALL against
every mode at 4 layers, and (ii) at full size the M2 streamed mode at prefetch depth 0, position-major, with no resident slab — the
simplest path — against every optimised mode; for W2, width 64's roots (the existing test's rule).

## 6. The matrix

Modes of the M2 build (each a flag set of `tensor-store.md` §9): **S-sf** — the streamed floor; **S-5** — a fifth; **S-13** — 13 GiB
of weights in a 16 GiB cap; **S-28** — 28 GiB of weights in a 32 GiB cap. A streamed mode's cgroup cap, where none is named, is
⌈weights budget + state + working set + 1 GiB⌉, which the harness computes and prints. Defaults unless a cell varies them: depth 1,
occurrence-major runs with W = P for J-P and W = the prompt for J-D, page cache `drop`, resident slabs evenly spaced, 4 I/O threads.

**M1 cells — the trait moves no byte** (M1 build against `legacy`, same host, same day):

| cell | workload / job | modes | hosts | cache × repeats |
| --- | --- | --- | --- | --- |
| R1 | W1 J-P(255) | B-FLOOR, B-FIFTH | H-MAC, H-VIRTIO | warm × 5 |
| R2 | W3a J-X(64+32) | B-PC, B-ALL | H-MAC, H-VIRTIO | cold × 3, warm × 5 |
| R3 | W2 J-8k widths 64, 16 | B-PIN, B-PC | H-MAC, H-VIRTIO | warm × 5 (+ one cold load) |
| R4 | a small IR container under a stated budget | v1 | H-MAC | once: `pinned_mib` must be 0 (F2) |

**M2 cells — mandatory for acceptance:**

| cell | workload / job | modes | hosts | cache × repeats |
| --- | --- | --- | --- | --- |
| A1 | W3a J-P(255), J-P(1023), J-D(64+8); and J-R(255) on H-VIRTIO | B-ALL, S-5, S-sf | H-MAC, H-VIRTIO | cold × 3; capped-warm × 5 on H-VIRTIO (not J-R) |
| A2 | W3b, the same | B-ALL, S-5, S-sf | H-MAC, H-VIRTIO | the same |
| A3 | W1 J-P(255), J-D(64+8) | B-FLOOR, B-FIFTH, S-5 (runs, nothing streamed), S-sf | H-MAC (after freeing disk), H-VIRTIO | cold × 3 |
| A4 | W4 J-P(255), J-D(16+4) | S-13 (H-VIRTIO), S-28 (H-VIRTIO-L); B-PC once, J-P only, under a 2-hour wall cap | — | cold × 1 + digest |
| A5 | W4-t4 J-P(255) | B-ALL, S-sf, S-13 | H-VIRTIO | cold × 1, digests |
| A6 | the floor | W3b and W1 at `Bytes(streamed floor − 1)` → refused, the message checked; at `Bytes(streamed floor)` → J-P(63) runs | H-MAC | once |
| A7 | the run order alone | W3b J-P(63) and W1 J-P(255) at S-5, runs off (position-major) against on | H-VIRTIO | cold × 1 off (≈ 0.7 TiB and ≈ 363 GiB of reads, §7), cold × 3 on |

**Cells that decide the defaults** (mandatory, but pass or fail no threshold): W3b J-D(64+8) at S-5 on H-VIRTIO, cold × 3, over
prefetch depth {0, 1, 3} × page cache {`drop`, `keep`} × resident slabs {evenly spaced, prefix} — 12 cells. Their winner becomes the
default of `--palw-tensor-prefetch-depth`, `--palw-tensor-page-cache` and the placement (`tensor-store.md` §12, 2–3).

**Informative**: W4b J-P(64), J-D(8+2) at S-28; J-C on W3a at B-ALL and S-5 (M3's gate later); H-NVME repeats of A1/A2; the skewed
router on W1; fused kernels on (`--palw-tir-fused-kernels`) for A1; I/O threads {2, 8}.

The mandatory set is about 150 runs for M1 (both builds) and about 320 for M2: A1 and A2 ≈ 108 each (54 cold, 45 capped-warm, 9
J-R), A3 48, A4–A6 twelve, A7 eight, the defaults 36. Most are seconds to minutes; A4's four W4 runs and A7's two position-major
runs dominate the wall time (§7).

## 7. Predictions — what each cell should read

**Arithmetic, not measurement** — from the shapes in the bench program's conventions and `R = 0.845 GB/s` (the fleet's `dd`,
ADR-0112 §1; the bench substitutes its own `R_dev`). A measured cell is compared with its prediction (§8, T3).

| workload, mode | weights budget | streamed floor | resident slabs | streamed bytes / run or decode position | J-P device bytes (run, W = P) | J-D device bytes | at R |
| --- | --- | --- | --- | --- | --- | --- | --- |
| W3a S-5 | 1.42 GiB | 0.95 GiB | 2 / 28 | 5.66 GiB | 5.7 GiB | 51.0 GiB (64+8) | 7 s / 65 s |
| W3a S-sf | 0.95 GiB | 0.95 GiB | 0 / 28 | 6.10 GiB | 6.1 GiB | 54.9 GiB | 8 s / 70 s |
| W3b S-5 | 2.74 GiB | 1.12 GiB | 5 / 40 | 11.14 GiB | 11.1 GiB | 100.3 GiB | 14 s / 127 s |
| W3b S-sf | 1.12 GiB | 1.12 GiB | 0 / 40 | 12.73 GiB | 12.7 GiB | 114.6 GiB | 16 s / 146 s |
| W4 S-13 | 13 GiB | 2.96 GiB | 7 / 88 | 104.5 GiB | 104.5 GiB | 522.7 GiB (16+4) | 133 s / 664 s |
| W4 S-28 | 28 GiB | 2.96 GiB | 19 / 88 | 89.0 GiB | 89.0 GiB | 445.2 GiB | 113 s / 566 s |
| W4b S-28 | 28 GiB | 7.90 GiB | 6 / 64 | 172.3 GiB | 172.3 GiB | 517.0 GiB (8+2) | 219 s / 657 s |

**W1, J-P(255), routed rows read under uniform routing** (the dense per-layer weights are pinned in every row but the last):

| mode | order | routed capacity | expected routed bytes | at R |
| --- | --- | --- | --- | --- |
| B-FLOOR | position-major | 1 token | 0.94 × 1.69 GiB × 255 ≈ 403 GiB | ≈ 8.5 min |
| B-FIFTH | position-major | ≈ 2.65 tokens | 0.84 × 1.69 GiB × 255 ≈ 363 GiB | ≈ 7.7 min |
| S-5 | occurrence-major, W = 255 | one layer's union over the run (≤ 0.56 GiB) held | every chosen expert once ≈ 27.0 GiB | ≈ 34 s |
| S-sf | occurrence-major, but W = 1 (the capacity holds one position's rows) | 36 MiB | (1.69 + 0.86) GiB × 255 ≈ 650 GiB | ≈ 14 min |

Excluded from every column: the open pass, which reads each byte once (the weights' size) whatever the mode. Compute is not
predicted here; the B-ALL cells measure it, and T5–T6 are stated against them.

## 8. M2 acceptance — PROPOSALS for the operator

| | threshold | why this number |
| --- | --- | --- |
| **T1** | every digest equals its reference, in every cell, on every host | tiers decide where bytes are, never which (`tensor-store.md` §7); one mismatch stops M2, whatever else passes |
| **T2** | under every cap: no OOM kill, no swap, the cgroup's `memory.peak` ≤ the cap; the manager's held bytes never pass its budget (printed by the harness); `VmHWM` ≤ budget + windows × cursors + state + working set + 0.5 GiB | the floor and budget are the contract the ledger reserves by; 0.5 GiB covers the plan, the program and the allocator |
| **T3** | device bytes of each run within [0.95, 1.05] × §7's prediction (the open pass measured apart, equal to the file ±1 %) | the read volume is the cost model operators size hosts by; > 5 % over means re-reads (an eviction bug, a thrashing LRU, a pinned page refaulted); < 95 % means the page cache served and the run was not cold |
| **T4** | while streaming and I/O-bound (J-D at S-5, S-13, S-28): device bytes ÷ wall ≥ 0.7 × `R_dev` | ADR-0112's whole premise is reads at the device's rate (845 against 11 MB/s); 0.7 leaves room for a slab's several extents (7 in the dense shapes) and the queue's ramp — under it, the I/O pool or the chunking is wrong |
| **T5** | J-P runs (W ≥ 64): prefetch hit rate ≥ 95 % and stalls ≤ 5 % of the wall; J-D: a decode position's wall ≤ 1.15 × max(its B-ALL compute time, its streamed bytes ÷ `R_dev`) | at depth 1 the read of N + 1 hides behind N wherever compute ≥ read, which a run of 64 positions makes true for every dense shape here; 15 % is the per-position bubble (each position's first slab, uneven layers) |
| **T6** | W3a and W3b J-P(255) and J-P(1023) at S-5: positions/s ≥ 0.9 × B-ALL on both hosts | at W = P a run reads the streamed slabs once for all its positions (W3b: 11.1 GiB, ≈ 14 s at 845 MB/s), while its compute reads every weight from RAM once a position (≈ 13.7 GB × 255) — minutes on 8 vCPU; with N + 1's read behind N's compute, streaming should cost under 10 % |
| **T7** | where nothing streams (W1 at B-FLOOR and B-FIFTH, W2, W3a at B-ALL): wall ±5 %, device bytes ±2 %, peak memory at most +5 % against the M1 build | M2 must cost nothing when it is not used |
| **T8** | W4 at S-13 and S-28: J-P(255) completes with T1; device bytes ≤ 1.05 × 104.5 / 89.0 GiB; wall ≤ 1.3 × max(compute, bytes ÷ `R_dev`), compute extrapolated from W4-t4's B-ALL time, its layers' share × 22 | the "200 GB ≠ 200 GB of RAM" claim at half size on fleet-like hardware; 30 % for extrapolating compute from 4 layers to 88 |
| **T9** | A6: one byte under the streamed floor is refused with every term named; the floor runs | ADR-0112 I-3's rule, for the new floor |
| **T10** | W1 J-P(255) at S-5 (A3, A7): device bytes ≤ 0.15 × B-FIFTH's measured bytes, and within T3 of §7's ≈ 27 GiB | the occurrence-major run's point for a mixture: ≈ 1/13 of the position-major reads under uniform routing |

**M2 is accepted** when T1–T10 pass on H-VIRTIO (and H-VIRTIO-L for T8) and H-MAC shows no T1 or T2 failure. A failure of T4–T6
or T10 alone, with T1–T3 passing, is a performance defect to fix before the fleet, not a correctness stop.

## 9. What the harness needs — described, not written

**H1 — `tir-exec-bench`** (`misaka-palw-tir-exec/src/bin/tir-exec-bench.rs`):
1. `--geo 7b | 14b | 123b | 405b-64 | 30b-a3b` beside `1.5b | 32b | 70b` (the geometries of §1.1), and the S1-MoE builder for
   `30b-a3b`; `--layers N` already truncates (W4-t4).
2. The integer-only activation table (§1.3) and a declared layout written into the container (`--layout max_context=2048,ckpt=64,h_tile=64,tile=64`).
3. `--write-only`: generate, then print the inventory root, the file digest, the tier arithmetic and every occurrence's slab bytes
   (S1's half of H5).
4. `--mode exec | producer | replay | decode | cells` — `exec` is today's loop (J-X); `producer`, `replay` and `cells` drive
   `TirBackendV1` as `tests/residency_node.rs:90-128` does (J-P, J-R, J-C); `decode` runs J-D.
5. `--store pagecache | pinned | resident:<all|floor|fifth|BYTES> | stream:<floor|fifth|BYTES>` — `pinned` locks the container as
   kaspad does, `PinnedFileMapV1` (`MAP_SHARED`, populated by 8 MiB reads, `mlock`) before the artifact is mapped — with `--prefetch-depth`,
   `--run-positions auto|W|off`, `--io-threads`, `--page-cache drop|keep`, `--placement spaced|prefix`.
6. `--cold` (Linux: `posix_fadvise(DONTNEED)` on the container, or `drop_caches` when root; macOS: refused unless
   `--assume-purged` follows a `sudo purge`), `--repeat N --warmup 1`, `--calibrate` (`R_dev`).
7. The OS readings of §2 (Linux `/proc/self/{io,status,stat}` and the cgroup's `memory.peak` / `memory.current`; macOS
   `proc_pid_rusage(RUSAGE_INFO_V4)`), a 1 Hz sampler thread, and the store's and the manager's counters. Today the bench reads RSS by
   spawning `ps` (`:290-293`), which is neither a peak nor cheap.
8. `--json <file>`: one line per run with every metric, the predicted figures of §7 beside the measured ones, the digest, and the
   facts of §4.

**H2 — `palw-a16-replay-bench`** (new binary in `misaka-palw-base0/src/bin/`): the body of the env-gated 8k test
(`qwen25_a16_backend.rs:4694-4751`) with `--hold mapped | pinned | owned`, `--widths`, `--job canonical | attempt`, `--cold`,
`--repeat`, the readings of H1.7 and `--json`. The test stays as the correctness gate; the binary is the measurement.

**H3 — `scripts/tensor-store-bench.sh`**: the matrix of §6 as a checked-in list of cell ids; per cell it checks the host is quiet
(§4), makes the cache cold or warm, wraps Linux runs in `systemd-run --scope -p MemoryMax=<cap> -p MemorySwapMax=0`, runs the
repeats, appends the JSON lines, and refuses to start on a host whose hostname is on a deny list of fleet hosts.

**H4 — `scripts/tensor-store-bench-report.py`**: JSON lines → the result tables (§10), measured beside predicted, pass or fail per
threshold.

**H5 — the S2 cross-check**: a test beside `misaka-palw-tir-lower/tests/residency_tiers.rs` that lowers the three configs of §1.2
and compares their arithmetic with S1's `--write-only` output (±3 %).

**Command lines, once H1–H3 exist** (paths illustrative):

```text
# build, on the bench host
cargo build --release -p misaka-palw-tir-exec --features node --bin tir-exec-bench
cargo build --release -p misaka-palw-base0 --bin palw-a16-replay-bench

# W3b: generate, then print root, digest, tiers, slabs
tir-exec-bench --geo 14b --container /data/tsb/w3b.palwtir --write-only --layout max_context=2048,ckpt=64,h_tile=64,tile=64

# A2, one cell: a fifth, streamed, a cold 255-position producer draw, in its cap
sudo systemd-run --scope -p MemoryMax=5G -p MemorySwapMax=0 -- \
  tir-exec-bench --geo 14b --container /data/tsb/w3b.palwtir --reuse --mode producer --prefill 255 \
  --store stream:fifth --prefetch-depth 1 --run-positions auto --page-cache drop --cold --repeat 3 --warmup 1 \
  --json /data/tsb/out/A2.jsonl

# A4: W4 in a 16 GiB cgroup, 13 GiB of weights
sudo systemd-run --scope -p MemoryMax=16G -p MemorySwapMax=0 -- \
  tir-exec-bench --geo 123b --container /data/tsb/w4.palwtir --reuse --mode producer --prefill 255 \
  --store stream:13958643712 --cold --repeat 1 --json /data/tsb/out/A4.jsonl

# W2 on the Mac, after `sudo purge`
palw-a16-replay-bench --artifact /Users/wata/pret12/art/qwen25-1.5b-a16-8k.palwart --hold pinned --widths 64,16 \
  --job canonical --assume-purged --repeat 5 --json ~/Downloads/MISAKA-wt-b/bench-tensor-store/out/R3.jsonl

# the whole matrix, on a host that passes the checks
scripts/tensor-store-bench.sh --host-profile H-VIRTIO --cells A1,A2,A3,A7 --out /data/tsb/out
```

**What runs before M1 lands.** B-PC, B-PIN, B-ALL, B-FLOOR, B-FIFTH and B-A16 measure today's code: once H1 (items 1–4 and 6–8)
and H2 exist, the baselines can be taken on H-MAC and H-VIRTIO before M1, and M1's cells reuse them.

## 10. Reporting

- Raw JSON lines: `~/Downloads/MISAKA-wt-b/lanes/evidence/tensor-store/` (Mac) and `/data/tsb/out/` (Linux), kept.
- The summary: `docs/design/palw/tir/tensor-store-bench-results-<yyyy-mm-dd>-<host>.md` — per cell the median / min / max, the
  prediction, the thresholds passed or failed, the digest, the container roots of §1.3 and the facts of §4.
- A result is quoted with its host and its date (ADR-0110's rule: a figure is a measurement with a host and a time).
