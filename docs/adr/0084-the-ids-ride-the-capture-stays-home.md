# ADR-0084: The ids ride, the capture stays home — a model-class claim serves its answer, never its history

**Status:** PROPOSED (2026-09-04), implemented on `palw-adr0084-served-answer` (see §7 for what
landed and what did not). **Consensus-inert**: no fence, no state field, no object variant; the
fingerprint does not move. Ships by a rolling rebuild of the seats and of every node that answers
material pulls; an old seat and a new executor interoperate (an old seat still pulls the whole
material and still gets nothing over the cap, exactly as today).
**Builds on:** ADR-0042 Decision 7 (the transport half: broadcast + pull), ADR-0073 Phase ① 1a/1e
(`FPC1` "the answer beside the question"; a seat checks, it does not re-run), ADR-0077 Decision 8
and SA-2 (the seat opens `k` intervals; "the executor never ships the capture whole"), ADR-0078 X6
(`output_root` is recomputable from the answer's ids, the job's context hash and the family's
rendered-hash rule), ADR-0082 Decisions 7 and 9 (the capture is a fold; a seat recomputes the
cache from the prompt it holds and "the committed output ids"), ADR-0065 Decision 4 (an
`Unavailable` quorum abstains — a separate fence, not touched here).
**Amends:** ADR-0073 Phase ① 1a, on the served-payload side only — what a seat is served for a
claim whose capture does not fit the transport is the question and the answer's ids, not the
capture. ADR-0077 Decision 8's last sentence — the attempt lane's seats "keep hashing their
canonical-job captures until Decision 14 makes those captures large, at which point the same arm
serves both lanes": the captures are large now, and this is that point.

## 1. What was measured (testnet-11 Relaunch 5f, genesis `ad30b5cb…`, 2026-09-03/04)

Read from the 5f genesis card (§6k–§6m, §7b, step 9) and from the code on the 5f head `b52502a6`:

| quantity | value | where |
|---|---|---|
| material cap, one broadcast / one served pull | `PALW_MATERIAL_MAX_BYTES` = 16 MiB | `protocol/flows/src/palw_gossip.rs:45` |
| interval-opening cap, one served opening | `PALW_INTERVAL_OPENING_MAX_BYTES` = 4 MiB | same file, `:718` |
| openings a seat draws per claim | `PALW_FP_SEAT_INTERVAL_SAMPLES_V1` = 4 | `consensus/core/src/palw_fp_interval_v1.rs:28` |
| floor material (attempt lane) | 2.27 MB | card §6m |
| QWEN36 attempt material (dense tuple) | 253 MB | card §6l |
| QWEN25-A16 graph-v5 attempt material (dense tuple) | 748 MB per 512-context job | card §6k/§6l |
| graph-v5 free-prompt material (the fold, ADR-0082 D7) | ~700 MB — 696.5 MB of checkpoint chunks per 512-context job | ADR-0082 §10.3 |
| serve allowance | 48 MiB per peer per minute, 256 MiB node-wide | `palw_gossip.rs` `SERVE_BUDGET_*` |

**What happened on the attempt lane.** `palw_producer.rs` broadcasts the whole material and only
then submits the block; `broadcast_palw_material` has no size check on the push path; the
receiver's `admit_material` refuses `> cap` only after the bytes have arrived (gRPC decode cap 1
GiB); the serve path refuses `> cap` before sending ("never serve what the transport would
refuse"). On seat2's links 748 MB × five peers did not finish inside the 120 s flow window: the
peers' ping and relay flows timed out, the routers were torn down, and the block — announced once,
queued behind the material — was gone (card §6l). QWEN36's 253 MB crossed ibm's DC links, and
still **no seat could pull it**: at 21:34 UTC node1 and `.113` filed `Unavailable` for QWEN36's
first claim `e78441c7…` at the half-window. Every model-class attempt claim on this chain reaches
its half-window as Unavailable × quorum, is redrawn once, and voids by `ReceiptTimeout` with nobody
slashed on the shipped path (card §6m, correction). seat2's v5 production was paused for it.

**What happened on the free-prompt lane, read in code.** ADR-0077 Decision 8 and ADR-0082
Decision 9 are implemented: `kaspad/src/palw_fp_seat.rs` draws the intervals, `palw_panel.rs`
`fp_interval_seat_outcome_v1` recomputes the checkpoint state and replays them, and the openings
travel on their own authenticated lane. **The arm's two inputs, though, come off the whole-capture
pull it was built to retire**: the job and the prompt off an `FPM1`/`FPC1` payload
(`fp_job_material_for_claim`), and the committed output ids off the `FPC1` payload's CAPTURE
(`fp_committed_output_ids_v1`). The latter names `base0_material_decode_v1` — the DENSE tuple's
decoder — and a graph-v5 free-prompt capture is the FOLD (`base0_fp_material_encode_v2`,
`qwen25_a16_backend.rs:1162`), so even an `FPC1` that had arrived would yield no ids for a v5
claim. The panel's own comment on that function says where the ids belong: "on the seam … or in a
small served object of their own, so a seat that fetches no capture does not have to hold one".
This ADR is that object.

**What happened to the two public free-prompt claims (STL `b15ef21c…`, MIDI `d575928f…`).** The
rail was run without `--capture` and with `--retention-dir /root/fp-5f/outbox/traces` (card, step
9). Without `--capture` the submitter stages the question-only `FPM1`; the node's panel reads
`<appdir>/<network>/palw-retention/` (`kaspad/src/daemon.rs:1358`) and never that directory. The
executor's node therefore held nothing for either claim: every pull answered `NotHeld`, no interval
could be opened, and the seats abstained. The reading "their material exceeds the cap" was wrong
for those two claims; the cap failure is the attempt lane's, and any free-prompt claim staged
correctly would have met it next.

## 2. The requirement

**R1 — a seat's bytes are bounded by the answer.** What a seat needs to conclude on a model-class
claim, on either lane, is the job (fixed size), the prompt's ids (`4 × prompt_tokens`) and the
answer's ids (`4 × decode_tokens_executed`). Everything else it recomputes (ADR-0082 D9) or fetches
one interval at a time (ADR-0077 D8). Nothing a seat fetches may grow with the history.

**R2 — a cap never overrules an admission.** The constant's own doc states it; the shipped tree
violated it twice (8 → 16 MiB on 2026-08-28; 16 MiB against 253 and 748 MB on 2026-09-03). The
cure is not a third number: it is that nothing the transport would refuse is ever handed to it.

**R3 — "retained" means the serving node has it.** The data-availability obligation is discharged
by the node that answers pulls and opens intervals. A capture on a disk that node does not read is
not retained, whatever the rail printed.

## 3. Decisions

**Decision 1 — a third free-prompt payload: the answer envelope `FPA1`.**
`PalwFpAnswerV1 { material: PalwFpMaterialV1, output_token_ids: Vec<u32> }`, wire magic `FPA1`,
beside `FPM1` (the question) and `FPC1` (the question and the capture). Its decoder re-checks the
one binding it can prove itself — the prompt ids hash to the job's `prompt_token_ids_hash`, as
`FPM1` does — and nothing else. **The answer's ids are bound by the seat, to the chain**:
`output_commitment_v2(job_context_hash, ids, rendered_output_hash_for_family(ids))` must equal the
claim's `output_root` (ADR-0078 X6's recompute, through a new backend seam
`fp_output_root_v1(job, ids)` that builds the context exactly as `fp_recompute_checkpoint_root`
does). `PalwSeatDutyV2` carries `output_root`, read off the state's claim record where
`execution_root` and `trace_root` already are — a read view, not a state field. An envelope whose
ids do not recompute the root is refused BY NAME before any forward pass is spent, so a forged
envelope cannot make an honest claim's seat file nothing: the pool is iterated and the honest one
binds. Size: the job plus `4 × (prompt_tokens + decode_tokens_executed)` — under 3 KB at 512, 512
KB at 131,072, inside the material cap at every ladder the ruleset admits.

**Decision 2 — the resolver serves the envelope whenever the capture does not fit.** A node answers
a whole-capture pull for a claim with its retained material when that material is within
`PALW_MATERIAL_MAX_BYTES`, exactly as today, and otherwise with the claim's answer envelope: the
`<claim>.answer` file staged beside `<claim>.material` at submission (Decision 5), or, when there is
none, an envelope derived once from the retained capture through the seam and cached under that
name. This is a property of the RESOLVER the panel registers, not of the transport: the gossip
module, its caps, its budgets and its message types are unchanged, and no payload the transport
would refuse is ever handed to it. The envelope arrives at the asker as the same
`PalwTraceMaterialBroadcast` a served material does, is pooled the same way, and is read by the
seat's free-prompt arm as the job material it already accepts (`palw_fp_job_material_decode_v1`
reads all three magics) plus the ids it lacked.

**Decision 3 — nothing over the cap is pushed, and the block goes first.** `broadcast_palw_material`
refuses bytes over the cap before `hub().broadcast` — one warning naming the claim and the cap, no
message — and the attempt-lane producer submits its block BEFORE it announces material. Since
protocol 104 the announcement is a courtesy and the pull is the obligation ("announced, not
pushed", `palw_producer.rs`), so a skipped announcement costs a seat one pull it makes anyway,
while a block queued behind 748 MB cost the block. The panel's canonical-claim path already
submits the transaction first; it gains the same refusal. The cap stays 16 MiB: it now bounds what
a seat NEEDS (an envelope, a floor capture, a graph-v2 A16 capture), and a class's capture size
stops being a transport concern at all.

**Decision 4 — the interval arm serves both lanes.** For an attempt-lane claim of a class that
registers a checkpoint geometry, the seat runs the same arm the free-prompt lane runs, with the
lane's own sources: the job context and the prompt from the anchor (`job_for_anchor`, as
`job_anchor_for_claim` derives it today — never from the material), the answer's ids from the
attempt-lane envelope `ATA1` (`PalwAttemptAnswerV1 { anchor, prompt_token_ids, output_token_ids }`,
bound to the claim's `output_root` through the anchor-derived context by the same X6 recompute),
the interval count from the context's `declared_prefill_tokens` and `exact_decode_tokens`, the
checkpoint state recomputed from the prompt (D9) through a context-taking seam the free-prompt
seam now delegates to, and `k` intervals opened on the interval lane and replayed exactly. The
executor's resolver opens intervals of a raw family capture by taking the prompt from the anchor
the capture's own binding names, which the seat re-derives independently and the opening's ids
bind to. The whole-capture arm (`verify_material`) stays for every material that fits — the floor
and the graph-v2 class — and remains the fallback where a class has no geometry.

**Decision 5 — one retention directory, and the node names it.** The node reports the directory
its panel serves from in `getPalwProducerFacts` (a version-5 suffix field, node-local exactly like
`lockedBondOutpoints`; a version-4 reader sees nothing). `misaka-palw-fp-rail --submit` and
`misaka palw fp-submit` take `--retention-dir` when given; otherwise they use the node's own
directory when the node is on this host, and refuse to stage anywhere else, saying why. At staging
the submitter writes `<claim>.answer` (`FPA1`, from the worker result's `output_token_ids`) beside
`<claim>.material`, staged `.partial` and renamed under the same ordering as the material — so the
serving node never decodes a 700 MB capture to answer a pull, and a claim's answer is servable the
moment the claim is on chain. The rail's summary prints the directory the files went to and whether
it is the node's.

**Decision 6 — the ids read through the seam, on either retention form.**
`PalwExecutionBackendV1::fp_committed_output_ids` is implemented on the base0 backends over
`base0_material_decode_any_v1` — the fold and the dense tuple alike — and the panel reads a
capture's ids only through it. The panel names no family decoder; the fold gap in §1 closes with
it.

## 4. What this costs, stated before it is measured

* **Bytes per seat, per claim**: the envelope (≤ 3 KB at 512, ≤ 512 KB at 131,072) plus `k` = 4
  openings (≤ 4 MiB each by the transport's backstop, `O(interval × row + log₂ leaves)` by
  construction). Independent of the capture's size, on both lanes. Before: 748 MB the transport
  refused, so zero bytes and no verdict.
* **Compute per seat**: unchanged from ADR-0082 D9 — one forward pass of the job plus `k`
  interval replays; the X6 recompute is one hash over the ids.
* **Executor disk**: one small file beside each material. A node that derives an envelope for a
  claim staged before this change pays one decode of the capture, once, and caches it.
* **The court**: unchanged and named. A terminal close still needs the ACCUSED capture on the
  closer's side (`refutation_for_free_prompt_index(&accused_bytes, …)`); the executor holds its own,
  a challenger of a claim whose capture exceeds the cap does not, and its pull answers with the
  envelope, which a close cannot be assembled from. The court then stalls at "the close needs the
  ACCUSED capture and this node holds none" — the standing behaviour on this chain, now bounded to
  the court rather than reaching certification. Assembling the close from an interval opening
  (the opening that contains the disputed step carries the operands the refutation names) is the
  follow-up, listed in §8.

## 5. Invariants the tests must hold

```
Y1   R1: a seat judging a model-class claim on either lane fetches at most the envelope plus k
     openings, whatever the capture's size; the envelope is under PALW_MATERIAL_MAX_BYTES at
     every ladder the ruleset admits.
Y2   An envelope whose ids do not recompute the claim's output_root is refused by name before
     any forward pass; with a forged and an honest envelope pooled in either order the seat
     concludes on the honest one.
Y3   No payload over PALW_MATERIAL_MAX_BYTES leaves this node: the push path refuses before
     the broadcast, the resolver answers an over-cap material with the envelope. The constant
     and the gossip module's behaviour on what it receives are unchanged (their tests pass
     untouched).
Y4   A block is submitted before its material is announced; an over-cap announcement is
     skipped with one warning naming the cap; the block propagates.
Y5   The submitter stages <claim>.answer beside <claim>.material under the same
     partial-then-rename ordering; the resolver serves the answer for an over-cap material and
     the material otherwise; with no .answer file it derives one from an FPC1 capture and caches it.
Y6   Attempt lane: the envelope binds to output_root through the anchor-derived context; the
     draw's counts come from that context; a claim whose material exceeds the cap reaches a
     Valid receipt on a seat that fetched no capture.
Y7   fp_committed_output_ids reads a fold and a dense tuple alike; the panel names no family
     decoder.
Y8   getPalwProducerFacts version 5 is read by a version-4 reader with the new field absent;
     an explicit --retention-dir always wins; a submitter given no directory and a node on
     another host refuses to stage, by name.
```

## 6. Order of work

1. Decision 3 (transport guards): `broadcast_palw_material` refusal; the producer's block-first
   order. Independent of everything below and safe alone.
2. Decision 1 (the envelope, the seam, the duty field) and Decision 6 (the ids seam on the base0
   backends).
3. Decision 2 (the resolver) and the seat's free-prompt arm reading the envelope.
4. Decision 5 (the submitter's `.answer`, the facts field, the rail/CLI default).
5. Decision 4 (the attempt-lane arm): the context-taking recompute seams, `ATA1`, the producer
   staging its answer, the resolver opening a raw capture, the seat's attempt arm.
6. The card's §6m fleet decision is reversible on the day 1–4 are deployed on ibm, `.113`, node1
   and seat2: seat2's v5 production resumes; QWEN36 claims start reaching Final.

## 7. Implementation record (2026-09-04, `palw-adr0084-served-answer`, from 5f head `b52502a6`)

**Landed — §6 items 1–5, all consensus-inert; `cargo check` and the crates' unit suites green
on the branch (see the commits for the exact test names):**

| decision | where | what |
|---|---|---|
| D1 `FPA1` | `consensus/core/src/palw_freeprompt_v3.rs` | `PalwFpAnswerV1`, encoder, decoder (prompt binding; no empty answer), `palw_fp_committed_output_ids_decode_v1` (`FPA1` direct, `FPC1` through the seam); `palw_fp_job_material_decode_v1` reads all three magics |
| D1 binding | `palw_backend.rs`; `palw_producer_v2.rs` | `fp_output_root_v1` (ADR-0078 X6); `PalwSeatDutyV2.output_root` off the claim record |
| D2 resolver | `kaspad/src/palw_panel.rs` `serve_material_or_answer` | the material when ≤ `PALW_MATERIAL_MAX_BYTES`, else `<claim>.answer`, else derived once from the capture (`FPC1` → `FPA1`, raw capture → `ATA1`) and cached |
| D2 seat | `palw_panel.rs` `fp_committed_output_ids_v1` | ids off `FPA1` or `FPC1`-through-the-seam, bound to `output_root` by name before any forward pass; a bound envelope retained under `foreign/` |
| D3 | `protocol/flows/src/flow_context.rs`; `kaspad/src/palw_producer.rs` | `broadcast_palw_material` refuses over the cap before the broadcast; the producer submits its block before it announces |
| D4 `ATA1` | `consensus/core/src/palw_attempt_v2.rs` | `PalwAttemptAnswerV1 { anchor, prompt_token_ids, output_token_ids }`, encoder, decoder |
| D4 seams | `palw_backend.rs`; base0 floor / A16 / Qwen3.6 backends | `fp_job_context_v1`, `checkpoint_root_for_context_v1`, `checkpoint_covered_bound_for_context_v1`, `output_root_for_context_v1`; the free-prompt seams delegate to them, so there is one context per claim on either lane |
| D4 seat | `palw_panel.rs` `interval_seat_outcome_v1` (was `fp_interval_seat_outcome_v1`), `attempt_committed_output_ids_v1` | the interval arm keyed on the context; on the attempt lane it runs after the whole-capture arms and before the pull, with the job and prompt from the anchor and the ids from `ATA1` or a held capture, bound under the anchor-derived context |
| D4 executor | `palw_producer.rs`; `palw_panel.rs` `open_retained_interval`, `backend_for_raw_capture_v1` | the producer stages `<attempt>.answer` beside its material; the resolver opens intervals of a raw capture with the prompt the anchor derives |
| D5 | `rpc/core` v5 `palw_retention_dir`, `rpc/grpc` field 26, `rpc/service`, `flow_context::palw_declare_retention_dir`; `misaka-palw-fp-submit` `FpStaging.output_token_ids` / `<claim>.answer`; the rail; `misaka palw fp-submit --capture` | the node names its directory; the submitter stages the envelope under the material's ordering; the rail defaults to the node's directory and refuses one not on this host |
| D6 | the three base0 backends | `fp_committed_output_ids` over `base0_material_decode_any_v1`; the panel names no family decoder |

**Not landed here, by name:**

* **W7-shaped end-to-end evidence.** No devnet drill in this record certifies a claim whose
  capture exceeds the cap through the interval arm (Y1, Y6). The unit suites pin the payloads, the
  bindings, the staging ordering and the RPC round trip; the loopback devnet (card §6a's three
  kaspads and two v5 producers) is where Y1/Y6 are measured, and that run is the next step before
  the fleet rebuild.
* **The opening cost of a raw capture.** `open_retained_interval` on an attempt-lane capture
  decodes the whole retention twice per request (`capture_shape`, then `open_fp_interval`) — the
  free-prompt path already does — ~0.15 s and 575 MB peak for a 253 MB QWEN36 tuple (the exporter's
  measurement), more for 748 MB. Bounded by the serve throttles; a class-tagged retention file
  would remove the first decode.
* **The exporter** (`tools/palw-jobs-export`) still reads the answer's ids off `<claim>.material`;
  `<claim>.answer` is a few kilobytes and is the better source now.
* §8's list: the court's close over the cap (U-07c), ADR-0065 D4's activation, the cap itself.

## 8. What is deliberately not decided

* **A close assembled from an opening (U-07c).** §4 names the stall. The refutation of a step
  inside interval `j` needs the operands the interval's opening already carries; making
  `refutation_for_free_prompt_index` take an opening instead of a capture is a court change with
  its own invariants (the challenger's operands must be the executor's, proven against the leg
  roots), and its own ADR.
* **Raising or deriving `PALW_MATERIAL_MAX_BYTES`.** After Decision 2 nothing a seat needs
  approaches it; the whole-capture pull for the court is the only consumer that could want more,
  and U-07c removes that consumer. A cap that follows the largest class is the 2026-08-28 rule
  restated, and it was wrong twice.
* **Arming ADR-0065 Decision 4 on testnet-11 — already the case, and the card's item (b) was a
  misreading.** `palw_rc_shipped_params()` (what `--netsuffix=11` resolves to) runs
  `palw_rc_arm_phase1`, which sets `palw_unavailable_abstains = Some(ForkActivation::always())`
  from genesis; the test `the_rc_ruleset_arms_the_unavailable_abstains_rule_from_genesis` pins it.
  The "None on every shipped preset" reading (card §6m) came from the pin on the RAW preset
  (`MAINNET_PARAMS.palw_unavailable_abstains.is_none()`), which is the assembly's input, not the
  ruleset the fleet runs. So an `Unavailable` quorum already abstains on this chain; nothing here
  to activate, and §6m's correction ("nobody is slashed on the shipped path") stands for a second,
  independent reason.
* **A retention directory flag on kaspad.** The directory is derived from the app dir and reported
  over RPC; a flag would be a second place the answer lives.

## 9. Number hygiene

This is ADR-0084. The README's index records 0080–0083 as resident (0083 on
`palw-daa-bits-priced-rows`); its "next free number" line still said 0080 and is corrected with
this row. A concurrent claimant renumbers the later writer, per ADR-0036 Decision 5.
