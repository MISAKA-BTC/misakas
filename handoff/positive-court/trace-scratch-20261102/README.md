# Reuse empty bounded producer walk capacity — 2026-11-02

Implementation: `ac0699fde`. The
[actual CPU sample](../source-profile-20261102/README.md) identified repeated
table growth inside the producer's dependency walk. This increment retains
the pending-vector and membership-table capacity in the existing builder,
instead of allocating/growing them again for every material. The
[full seven-stage goal](../COMPLETION-REQUIREMENTS.md) remains required.

## Owner and semantic boundaries

The builder acquires its unchanged simultaneous owner **before** constructing
the scratch. Its existing 1,024-byte/frame request already co-reserves frames,
retained model/capture/record, source authentication, provider scratch and all
material stages. Pending storage is bounded by `max_trace_frames`; membership
still has the same unique/pending limits and conservative table-growth margin.
Static scratch headers fit the existing metadata overhead. Neither the real
8-GiB shared logical cap nor any request declaration was reduced or enlarged.
Capacity now stays live through the builder's lifetime, including while later
material/authentication stages execute; the existing owner covers that overlap.
It is not a measured allocator bound or kernel RSS quota.

Every entry clears pending and membership, and the walk's destructor clears
both on success **and each early return**. Only empty capacity persists.
No computed value, read list, canonical-closure result or verdict is cached.
The membership table remains private and is never iterated. Pending traversal,
ordered source/parameter construction, charge-before-lookup, all finite caps
and refusal ordering remain. The scratch borrow cannot escape; the existing
single-live-material gate still refuses concurrent material construction.
Reconstructing a builder uses the same shared spent-work session.

Node authentication, direct arithmetic, canonical closure, all required
relations, state/trace bridges, output comparison and EOF/signature acceptance
are unchanged. There is no native/wire/domain/credit/activation addition. A
complete local replay would still not replace selected-validator certification.

## Executed regression checks

[Runtime validation](runtime-first.json) passes **36 tests**: 11 existing
full-relation, four record-authentication and 21 wide Panel tests. Two opt-in
cases remain ignored in the default run; this count is not a trained-model pass.
[Selected strict Clippy](clippy-first.json) passes with `--no-deps -D warnings`.
These are selected package/target checks, not workspace-wide cleanliness.

Normal, false-Panel, false-Miner and dynamic/state fixtures compare material
bytes and every material-meter delta for a reused scratch and for a freshly
constructed builder with another membership seed. Fresh builder preparation
charges are kept separate from the per-material comparison and are not
refunded. Both serialized test exports are prepaid and remain bounded; the
second export is dropped before allocating the fresh-builder comparison.
The test tuple is local, not a new peer/native codec.

A new actual one-frame-cap refusal leaves a partial walk, then repeats the same
request: both fail at the same frame bound with identical nonzero spent-work
deltas, without granting a material. The existing unknown-provider test also
continues to refuse both attempts after a completed discovery walk. Actual
Node tests retain false-witness responsibility separation, delayed Miner
output judgment, final-relation coverage and detached/cold synthetic source
handling. Synthetic fixture owners are not native resource/funding authority.

The same private opt2/debug0 package overrides retain debug assertions and
overflow checks. Model, arithmetic, source limits, finite provider and tape
caps, 240-s CPU / 360-s wall and sampled RSS/disk stops are unchanged.

## Actual original-model result

[The actual process](pythia-first.json) exits **-24 / SIGXCPU**, with no outer
stop and no final libtest summary. Its last logged numeric prefix is
**49,152 / 213,298 relations**, versus 40,960 in the preceding hash-membership
run. This is a lower bound on completed numeric checks, not an exact terminal
relation count. No full Node verdict, record EOF/signature acceptance or native
Court result is produced. The model run is **not a pass**.

At the common 40,960 numeric prefix, producer material time falls from
150.4721 s to **115.0595 s**; Node submit is 87.1526 s versus **87.5357 s**.
[Every common logged prefix](common-prefix-comparison.json) has identical
provider usage, observed evaluation count, evaluator peaks and logical
live/peak requests. These are actual prefix measurements, not complete-job
extrapolations or a universal speedup. Fixture comparisons establish repeated
material bytes; the actual trained run does not make a separate complete
before/after material-byte comparison.

At the last new prefix, producer preparation is 136.8292 s and Node submit
104.3689 s. Provider usage is 1,170,576,257 bytes, 2,313,705 reads, 38,693 openings
and one attempt. Evaluator peaks are 8,714 elements, 164,864 terms, 170,720 source
reads and 808,025,472 workspace request bytes. Logical live/peak requests remain
8,053,697,455 bytes under the same real ledger. The same original trained
artifact/registration SHA values and current compiled binary match before and
after ([input pins](input-sha.json)). The process group is empty after exit;
no observation timeout triggered another run.

The original model, arithmetic, input IDs, class/artifact/tokenizer/context,
token 273 and output commitment remain unchanged. This remains the original
**noncanonical 16/1** job, with max context 2,048, one Panel capture, 14,157 leaves,
213,298 relations / 13,225,922 target lanes and 305,797,608 aggregate tape bytes.
It does not qualify canonical 255/2 or full-context execution. Producer and
Node still coexist and retain producer owners, while input SHA reads warm OS
caches. No isolated cold-role physical-cost or production quota claim follows.
[Model summary](model-run-summary.json) retains all phase and monitor results.

## Provenance and remaining work

[Validation summary](validation-summary.json) binds the three changed source
files to runtime, strict Clippy, model execution and the current checkout,
with unchanged source throughout each run. It also records changed-file
formatting/whitespace checks and the actual compiler profiles. Commands,
environment, full source inventories, raw-log SHA values, exits, sampled target
RSS and free-disk minima are preserved separately. Raw logs are audited without
rewriting their whitespace. Final operator-file versions and SHA inventory
are under `operator-scripts/`.

A normal Cargo build replaced the prior generated test executable at the same
artifact filename. Prior raw records identify the SHA actually executed then;
they are historical execution evidence, not a current-availability promise
for old generated-cache bytes. No archive/native/model/source data was retired
in this increment. Baseline and changed timings come from their individually
pinned executions, not a later rerun of an overwritten binary.

Complete checking of the unchanged trained short job, separate cold/warm role
CPU/RAM/IO/storage/recovery measurements, canonical/full-context costs, actual
native Assignment/Result/source/funding/P, every credit consumer and real-node
restart/reorg/IBD/pruning/dependency handling remain required, along with
explicitly approved finite activation. Native-assignment RFC/ADR drafts remain
unadopted. Pure checker fixtures, request guards and measured prefixes cannot
stand in for that native accepted flow or authorize production activation.
