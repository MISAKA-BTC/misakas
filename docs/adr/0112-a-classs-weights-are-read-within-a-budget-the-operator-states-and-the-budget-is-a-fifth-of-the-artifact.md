# ADR-0112 — A class's weights are read within a budget the operator states, and the budget is a fifth of the artifact

* Status: PROPOSED 2026-09-11 on `feat/adr-0103-held-context`, from the operator's decision of the
  same day after the testnet-11 producers were measured (§1): "the same model and the same weights,
  with the way they are run changed from resident-whole to explicit streaming; the formal target is
  a model five times the memory available to it". **Decisions 1–5 and 7–8 IMPLEMENTED the same day
  (§10)**; Decision 6's fleet measurement waits on the operator's go, because it runs a new binary
  on a fleet host. Consensus-inert: no object, acceptance rule, fence, parameter or fingerprint
  moves. A fleet takes it by an ordinary rebuild, and a node that states no budget takes the ratio.
* Builds on: [0052](0052-palw-qwen36-hybrid-class.md) (the mapped container: a 33 GiB file the
  runtime maps rather than reads, and the note that residency was left to the page cache),
  [0067](0067-a-registered-class-is-served-by-any-node-that-holds-its-artifact.md) (a node holds
  what it chooses; `--palw-class-cache-bytes` bounds which artifacts load — this ADR bounds what
  of one artifact is in memory), [0103](0103-the-context-is-held-off-the-chain-and-the-chain-carries-a-root-an-opening-and-a-logarithm.md)
  (every chain term at 2M is constant or logarithmic; the model's cost is the host's), and
  [0110](0110-a-context-limit-is-activated-from-reproducible-public-vectors-not-the-maintainers-workstation.md)
  (a figure is a measurement with a host and a time, never a guess).
* Supersedes: the residency paragraph of ADR-0052 §"what remains" and the `MADV_WILLNEED` expert
  cache it described (§7). Amends nothing in consensus.

## 0. The sentence this ADR is

**A class's weights are the same bytes on every machine that holds them. Which of those bytes are
in memory at any moment is a decision, and it is the runtime's, made under a budget the operator
states, never the kernel's. The always-set is read once and pinned; the routed experts are read as
the router chooses them, together and through the file descriptor, and held under what the budget
leaves; nothing the arithmetic reads arrives through a page fault. The budget's default is a fifth
of the artifact's weight bytes, and the class's floor — the always-set and one token's experts — is
the least it may be. A budget below the floor is refused at startup, by name and with the
numbers.**

## 1. What was measured

On 2026-09-11, on the three testnet-11 producers that run the Qwen3.6 class, with nothing changed
on any of them. The class is `PALW-QWEN36` (Qwen3.6-35B-A3B, `graph-v3`), a 33.27 GiB artifact
mapped by `Qwen36ArtifactV1::open_artifact`; the hosts are 8-core virtual machines with 23 GB of
memory and virtio disks.

**The draw.** One draw of the class's canonical job — 7 prefill positions and 2 decode calls, nine
forward passes — takes 17 to 20 minutes on `ibm node0` and on `C seat2` (their own logs: 16
draws of 17–19 minutes on node0, 16 of 17–18 on seat2, between 09:07 and 13:54 local). The
class ticket's probability is 1.0 (the per-class retarget has eased it to the floor), and the two
producers won 16 of 32 network draws against bits. The chain advanced 101 DAA in 22 hours: one
block every 13 minutes against a 2-minute cadence. **The lottery is the protocol's; the 17
minutes are the draw's.**

**What a draw reads.** `/proc/<pid>/io` sampled every 5 s across one whole draw on node0 (block
`#3` at 13:33:57 to `#4` at 13:54:00, 20 min 3 s):

| | one draw |
|---|---|
| `read_bytes` (bytes the process caused to be read from storage) | **12.8 GiB** (13,759,242,240) |
| major page faults | **3,012,123** |
| average storage rate | 11 MB/s |
| the artifact's pages resident in the process, sampled | 28 MiB to 1.2 GiB, oscillating |

Nine forward passes read 12.8 GiB: the always-set (§1's next paragraph) is evicted between tokens
and re-read, and the routed experts are read as 4 KiB faults. The device is not the limit:

| the same file, the same day, `dd iflag=direct bs=4M`, 512 MiB from offset 8 GiB | |
|---|---|
| `ibm` | **845 MB/s** |
| `C` | **563 MB/s** (its producer running) |

`misaka-palw-base0/src/mmap.rs` already recorded the mechanism when the root pass was moved off
the mapping: a cold page through a mapping is one synchronous 4 KiB fault, and on these disks
fault readahead never engages — "not under `MADV_SEQUENTIAL`, not under `MADV_WILLNEED`, not
with the device's readahead window raised" — 6 MB/s against 1.3 GB/s through reads sized to a
tensor. The root pass took that lesson. The forward pass did not: every weight it reads is
`map.i8_slice`, a fault.

**What the artifact is.** Read off its header by `scripts/misaka-qwen36-artifact-directory.py`
(the directory of 62,393 tensors and 52,623 parameter rows; the script prints this table and the
budget arithmetic for any `.palwq36`, so a host can be sized before a node starts):

| | bytes | share |
|---|---|---|
| routed experts (256 a layer, 8 routed a token, 40 layers) | 30.94 GiB | 93.0 % |
| attention and recurrence projections, norms | 1.23 GiB | 3.7 % |
| unembedding (248,320 × 2,048) | 0.49 GiB | 1.5 % |
| embedding (the same) | 0.47 GiB | 1.4 % |
| shared experts and routers | 0.14 GiB | 0.4 % |
| **the always-set** (everything but routed experts) | **2.34 GiB**; **1.86 GiB** without the embedding | |
| one routed expert (gate, up, down, exponents, five rows) | 3.09 MiB | |
| **one token's routed experts** (8 × 40) | **0.97 GiB** | |
| parameter rows, in the header, all read into memory at open | 0.71 GiB | |

Nine tokens route to about 64 of each layer's 256 experts (eight draws of 256, nine times: the
expected union), so a draw's experts are about 7.7 GiB of the file whatever holds them.

**The other term.** The node's own working set, which this ADR does not touch and states because
it is the rest of the host: `ibm node0` (the class's producer, a panel seat, two owned dense
artifacts) holds 8.9 GB of anonymous memory with 3.5 GB more swapped; `ibm node1` (a floor
producer and a seat, the same two dense artifacts, no mapped class) 13.6 GB with 1.8 GB swapped;
`C seat2` 14.2 GB. Two of them share `ibm`'s 23 GB. The class's residency cache existed
(`Qwen36Residency`, ADR-0052's note) and was never wired into the producer; it prefetched with
`MADV_WILLNEED`, which on these disks prefetches nothing.

**What was NOT the cause.** The operator's first reading named two more suspects, checked and
cleared: the Qwen3.6 runtime allocates no context-length cache at start (`Qwen36Cache::new` holds
the recurrence's fixed state and empty key/value vectors that grow by the token, 60 MiB at most on
this class), and the draw is nine forward passes, not thirty-two. A 128K context vector was running
on the maintainer's Mac at the time (ADR-0110 §9.5); it shares nothing with the fleet.

## 2. The requirement

A class artifact of `S` bytes runs — produces, replays, is judged — on a host whose memory for it
is `B ≥ S / 5`, with the same bytes and the same class id it has anywhere else, at a wall time
bounded by what it must read: `(routed experts per forward − hits) × forwards / storage bandwidth
+ compute`, never by page faults. **R-5**: the ratio the class is certified at is five. The floor
below which a class does not run at all is stated by the class (§3 Decision 3), and it is far
under a fifth for this one: 2.83 GiB against 6.65.

Three things the requirement does not ask for, said so nobody re-solves them:

* **A smaller model.** The weights are the artifact's, byte for byte; residency changes no bit of
  arithmetic, and the artifact root, the class id and every committed row are unchanged (§5 I-1).
* **A different job.** The canonical job stays `(7, 2)`; changing it changes the class id, which
  is a registration, not a runtime (§8).
* **Determinism the tree does not already have.** Every kernel a forward pass runs is exact
  integer arithmetic under ADR-0040 Decision E, so two hosts agree bit for bit whatever their
  residency; the model's identity is `artifact_root` (ADR-0052), a digest over every weight byte,
  every parameter row and the shape. Both already exist; this ADR relies on them.

## 3. Decisions

**Decision 1 — the forward pass reads weights through the file descriptor into memory the runtime
owns, never through page faults on the mapping.** `Qwen36ArtifactV1::tensor` returns
`TensorBytes`, a handle: a slice of an owned store or the mapping, or an `Arc` on bytes the
residency holds. Under a residency every weight a projection reads is a held handle. The mapping
stays: it is where the header and the embedding table are read, and it is the whole path when the
operator asks for the page cache (`PageCache`, the behaviour before this ADR, kept so the two can
be measured against each other).

**Decision 2 — the budget is one number, stated by the operator, and its default is a fifth of the
artifact's weights.** `--palw-class-resident-bytes=<bytes>` on `kaspad`; `Qwen36ResidencyPolicyV1`
in the runtime: `FifthOfTheWeights` (nothing said), `Bytes(b)`, `PageCache` (`0`). The ratio is
`QWEN36_RESIDENT_FRACTION_DENOMINATOR_V1 = 5`, one spelling. For the Qwen3.6 class that is
7,145,529,856 bytes: 6.65 GiB.

**Decision 3 — two tiers, told apart by name, and a floor.** Every tensor that is not a routed
expert's (`blk.N.ffn_expert.K_*`) and not the embedding table is the always-set: read once at open
into owned memory, in parallel through the file descriptor, and never given back (1.86 GiB here).
Routed experts are an LRU of owned buffers under what the budget leaves (4.79 GiB here, about five
tokens of them). The embedding table is neither: a token reads one row of it,
`embedding_row` reads that row (2 KiB) directly, and the 0.47 GiB table is never in memory. An
expert's five parameter rows ride with its tensors — read when it is admitted, given back when it
is evicted — so the 0.71 GiB of rows the old path copied into memory at open is not copied
(the non-expert rows, kilobytes, still are). The floor is the always-set plus one token's routed
experts: `1.86 + 0.97 = 2.83 GiB`, below which a forward pass would re-read what it just read. A
budget below the floor is refused at open, by name, with both terms and the floor in the message.

**Decision 4 — a layer's experts are read together, the moment its router commits.**
`admit_experts(layer, chosen)`: the chosen experts not yet held are read in parallel (one expert
is 3.09 MiB in three tensors, their exponents and five rows; eight of them land in tens of
milliseconds at the measured device rates) before the first is computed. Nothing depends on the
prefetch: an expert read outside an admission — a test, the inventory pass, a court replaying a
tile — is admitted on the way. Prefetch across layers is impossible and not attempted: layer
`L+1`'s routing is a function of layer `L`'s output.

**Decision 5 — the class's identity and its arithmetic are untouched, and a test says so at the
ratio.** `a_budgeted_artifact_computes_what_an_owned_one_does_at_a_fifth_of_its_size`: a fixture
with the class's expert count, held within a fifth of its bytes, computes the owned store's logit
rows token for token while never holding more than its budget; at the floor too; and through the
page cache too. `the_budgeted_paths_read_what_the_owned_ones_hold`: the root is one digest whether
the expert rows are owned or in the file, the embedding row is the table's row, and a walk over
every expert through a budget that holds a sixth of them evicts and re-reads them all correctly.

**Decision 6 — the fleet measurement, and the numbers this ADR is judged by.** The same host, the
same file, the same tokens, `qwen36-run` (the bench tool, ADR-0052) under `--resident page-cache`
and under the default: bytes read from the file per token, time per token, hits and evictions.
Then a producer restarted with the default: its draw's storage line (Decision 8) and its draw
time. The arithmetic says a draw reads about `9 × 0.97 GiB` less the hits — 8 GiB at 563–845 MB/s
is 10–15 s — plus compute; the measurement is what §10 records. This decision is the one not
taken by the maintainer's session, because it runs a new binary on a fleet host.

**Decision 7 — the operator's arithmetic, printed.** At load, for every mapped holding: the
budget, its two tiers, how many tokens of experts the second tier holds, and the host's
`MemAvailable`; a warning when the budget exceeds what the host has, because a budget the kernel
reclaims is the page cache with extra steps. The node's own working set (§1, 9–14 GB on these
hosts) is outside this ADR and inside that arithmetic: two kaspads on one 23 GB host leave no
budget, and that is the operator's to change.

**Decision 8 — what one draw reads from storage is a log line, not a sampler.** After every draw
the producer prints the process's `read_bytes` delta (Linux; "not counted" elsewhere) and, per
mapped holding, what the loader read, in how many misses of how many expert lookups, what it
evicted and what it holds. The number that explained the fleet's draws was found with a sampler on
the host; it is a line an operator watches now.

## 4. What this costs

* **Memory:** the budget, plus one layer's experts in flight (a handle a projection holds
  outlives an eviction of its entry; the floor is sized for it), plus the mapping's header pages.
  Against today: today the process held 8.9 GB anonymous and paid 3.5 GB of swap for the 1.2 GiB
  of file pages it could keep; under the default it holds 6.65 GiB of weights it chose.
* **Open:** the always-set's 1.86 GiB through the file descriptor in parallel — seconds — beside
  the root pass the node already makes (a minute cold).
* **A draw:** its experts, once each, at the device's sequential rate; its always-set, never
  again. The hit rate between tokens of one draw is small (§1: 64 of 256 a layer over nine
  tokens) and the ADR does not count on it.
* **Code:** one accessor type, one residency struct, one open path, one flag, two log lines. No
  consensus crate changes.

## 5. Invariants the tests must hold

1. **I-1, identity.** A budgeted mapping's `artifact_root`, every tensor and every parameter row
   equal the owned store's; a forward pass over it produces the owned store's rows, at a fifth, at
   the floor, and through the page cache. (`a_budgeted_artifact_computes_what_an_owned_one_does_at_a_fifth_of_its_size`,
   `the_budgeted_paths_read_what_the_owned_ones_hold`, and the existing
   `a_mapped_artifact_runs_identically_to_an_owned_one`, `the_mapped_root_equals_the_owned_root`.)
2. **I-2, the budget.** Between admissions the residency never holds more routed-expert bytes than
   the budget leaves after the always-set; at the floor it holds at most one token's.
3. **I-3, the floor.** A budget below the always-set plus one token's routed experts is refused at
   open with both terms and the floor in the message; the floor itself opens.
   (`a_budget_below_the_floor_is_refused_by_name`.)
4. **I-4, no faults.** Under a residency no weight a projection reads is a slice of the mapping:
   pinned tensors and expert parts are held handles, the embedding row is read directly. Held by
   construction (`tensor`'s residency arm returns `Held` for both tiers) and by the storage line's
   major-fault count in the fleet measurement (Decision 6).
5. **I-5, the policy's arithmetic.** A fifth rounds up; `0` is the page cache; the expert-name
   rule classifies routed experts and their rows and nothing else.
   (`the_residency_policy_arithmetic`.)
6. **I-6, the seam.** The lineage contract carries the policy (`PalwModelLineageV1::load`), the
   dense lineage ignores it, `kaspad`'s two duties pass what the operator stated, and a holding
   reports its residency in its summary line and its stats through `residency_stats_of`.

## 6. Order of work

1. The accessor and the residency in `misaka-palw-base0` (`qwen36.rs`, `mmap.rs`): Decisions 1,
   3, 4, 5 and their tests. **Done.**
2. The seam: the policy on the lineage contract; `kaspad`'s flag, both duties, the load-time
   arithmetic and the draw's storage line (Decisions 2, 7, 8). **Done.**
3. The bench tool under the policy (`qwen36-run --resident-gib | --resident page-cache`). **Done.**
4. Decision 6 on `C` or `ibm`, with the operator's go: the bench tool under both policies, then
   one producer restarted on the default, its storage line and draw time recorded in §10.
5. The unarmed integration build and the `main` merge carry it (consensus-inert; fingerprint
   unchanged).

## 7. Supersession

| what | by |
|---|---|
| ADR-0052's note that expert residency "remains the page cache's, with a measured note" and the `Qwen36Residency` LRU (`MADV_WILLNEED` admission, `MADV_DONTNEED` eviction, on the engine, used only by the bench tool) | Decisions 1, 3, 4: residency is the artifact's, owned buffers read through the file descriptor, on every engine over the artifact. The `MADV` advice stays in `mmap.rs` as a utility nobody calls. |
| the root pass's rule "per-token expert access stays on the map, whose resident-set behavior is the reason the map exists" (`mmap.rs`) | withdrawn: the resident-set behaviour was the reason the draw took twenty minutes. The map's reason is now the header and the embedding row. |
| `docs/palw-practical-runtime-plan-2026-08-26.md`'s residency line ("always-set は pin, routed expert 以外を open 時に MADV_WILLNEED") | Decision 3, with the pin an owned read. |

Nothing in consensus moves. `PalwExecutionBackendV1` is unchanged; the lineage contract gains one
parameter.

## 8. What is deliberately not decided

* **A single-forward ticket.** The operator's sketch proposed that a draw be one prefill pass
  rather than the canonical job's nine, so a low-memory host reads its experts once a draw rather
  than nine times. It would cut a draw's storage traffic by about nine and its compute likewise —
  and it changes the canonical job, hence `pwu_per_inference`, hence the class id: on testnet-11
  the class is minted in genesis, so it is a re-mint, and on a live registry it is a new class.
  Decision 6's numbers decide whether it is worth a ruleset move: at the device rates measured, a
  nine-pass draw reads for 10–15 s, which is the order of its compute, so the loader alone brings
  the draw from twenty minutes to under a minute. Recorded, with the arithmetic, and not taken.
* **Streaming the always-set.** Below the floor (2.83 GiB here, a ratio of 11.8) the always-set
  would have to be read per layer per token — 1.86 GiB a token, 17 GiB a draw. The dial exists in
  the design; the ratio this ADR certifies does not need it. **The ratio is a property of the
  mixture, and a class that is not one has a floor near its size**: the maintainer's
  `qwen35-2b.palwq36` (2.28 GiB, one routed expert a token of 37 MiB in each of 24 layers) has a
  floor of 1.81 GiB — a ratio of 1.3 — and the default fifth is refused on it, by name, as
  Decision 3 says. Such a class runs with a stated budget at or above its floor and gains
  Decision 1's read path all the same (§10.3).
* **The dense tier's owned artifacts.** `Base0ArtifactV1` files (1.67 GiB each, two on every
  fleet node) are read whole into anonymous memory. The same loader would fit them; they are small
  against the hybrid and are not this ADR's problem.
* **The node's own working set.** 9–14 GB of anonymous memory per `kaspad` on the fleet (§1),
  3.3 GiB of which are the two owned dense artifacts. What the rest is has not been measured; it
  is the other term of every host's arithmetic, and it is a measurement to make, not a guess to
  write here.
* **Reading and computing in one layer overlapped.** Decision 4 reads a layer's eight experts,
  then computes them. Computing the first while the eighth lands is a later refinement of the
  same design, worth at most a layer's read per layer.

## 9. Number hygiene

Written as 0112 on `feat/adr-0103-held-context`. Every local branch and `origin/main` were listed
before the number was taken (0108 and 0109 are `origin/main`'s, 0110 and 0111 this branch's); no
branch holds an 0112. **The next free number is 0113.**

## 10. Implementation record (2026-09-11)

### 10.1 Where each Decision lives

| Decision | where | what pins it |
|---|---|---|
| **1** the handle | `qwen36.rs`: `TensorBytes`, `Qwen36ArtifactV1::tensor` / `tensor_sized` / `group_exponents`; `mmap.rs`: `read_i8_at`, `read_u8_at` | I-1's tests; every kernel call site takes `&TensorBytes` |
| **2** the policy | `Qwen36ResidencyPolicyV1`, `QWEN36_RESIDENT_FRACTION_DENOMINATOR_V1`; `kaspad --palw-class-resident-bytes`, `palw_class_residency_v1` | `the_residency_policy_arithmetic` |
| **3** the tiers and the floor | `Qwen36ResidencyV1` (pinned map, expert LRU, `expert_param_extents`), `open_artifact_with_residency`, `embedding_row`, `param_rows`' expert arm, `artifact_root`'s merged walk | `a_budget_below_the_floor_is_refused_by_name`, `the_budgeted_paths_read_what_the_owned_ones_hold` |
| **4** the admission | `Qwen36ResidencyV1::admit` (rayon over the misses), called from `moe()` and the planned walk's `RouterTopk` | the fifth test's lookup count |
| **5** identity at the ratio | — | `a_budgeted_artifact_computes_what_an_owned_one_does_at_a_fifth_of_its_size` |
| **7** the arithmetic | `kaspad::palw_backends::load_class_holdings_v1` (the info line, the `MemAvailable` warning) | — |
| **8** the storage line | `storage_snapshot_v1`, `log_draw_storage_v1`, `process_storage_read_bytes_v1`; the producer's `produce_one` | — |
| the seam | `PalwWeightResidencyV1` on `PalwModelLineageV1::load`; `PalwClassSdk::load_artifact_with`; `residency_stats_of`; the holding's summary | the SDK suite; `kaspad`'s holdings tests |

### 10.2 What was run

The `misaka-palw-base0` suite, the SDK suite, `kaspad`'s `palw_backends` tests, clippy on the three
crates — §10.3 has the counts. The fleet measurement (Decision 6) has not run.

### 10.3 Numbers

**The suites** (2026-09-11, debug): `misaka-palw-base0` 426 passed, 0 failed, 10 ignored;
`misaka-palw-sdk` 25 passed; `kaspad`'s `palw_backends` 12 passed, the budgeted holding's among them.

**A real mapping on the maintainer's Mac** (12 cores, 24 GB; `qwen35-2b.palwq36`, 2.28 GiB, one
routed expert a token in 24 layers, so nothing to evict; the file warm in the page cache, which is
the case that flatters the old path): six prefill positions and eight decode calls, the same
tokens through both policies.

| | the page cache | a budget of 1.90 GiB |
|---|---|---|
| open | 4.6 ms | 71 ms (0.94 GiB pinned) |
| prefill, six positions | 2.14 s (357 ms a token) | **0.54 s (91 ms a token)** |
| decode | 13.2 tok/s | 13.0 tok/s |
| produced ids | `[11, 1879, 0, 3555, 830, 11, 1879, 0]` | the same |
| nonzero logits | 248,196 of 248,320 | the same |
| peak resident | 2.23 GB | 2.26 GB |
| read from the file | (faults) | 1.82 GiB: 0.94 pinned, 0.88 of experts, 99.4 % hits after |

The rows are identical (I-1 on a real artifact). Even with every page already cached, the prefill
is four times faster through owned buffers than through the mapping, which says something about
faults and copies before it says anything about disks. What it says about disks is §1's 845 MB/s
against 11 MB/s, and that is Decision 6's measurement to make.

**The fleet** (Decision 6): not run.
