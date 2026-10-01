# RFC-0003 step 8 — the generative drill's inputs (for the int-11 combined drill)

Owner: lane D. Lane A lists the flags below in `docs/design/palw/int-11-drill-flags.md`; lane C's harness (`audit-tir/`,
`dm.sh`) runs the scenarios. Nothing here is armed on any shipped ruleset: every fence is the drill's own mover
(`config::drill::*`), dormant elsewhere. t12 identity must not move (params `3db42ea638f3c427…`, schedule `9d6b83fe744b6374…`
— `scripts/t12-repin.sh --drift-only` reported no drift on the tip this document ships with).

## 1. The classes

Written by `cargo test -p misaka-palw-sdk --test gen_drill_classes -- --ignored write_the_drill_classes` (Plan B) and
`… --test gen_sd3_class -- --ignored write_the_sd3_class` (Plan A), both with `PALW_GEN_DRILL_OUT=<dir>`; then
`python3 audit-gen/merge-sd3-manifest.py <dir>` puts the SD3 entry into `drill-classes.json`. Current artifacts:
`/Users/wata/Downloads/MISAKA-wt-b/gen-drill-classes/`.

| class | role | container | step leaves (canonical) | a claim's material (FPG1) | largest cone close |
| --- | --- | ---: | ---: | ---: | ---: |
| `toy-image` (Plan B) | image claim, one-move court (DG-3/4/5) | 6,112 B | 120 | 2,628 B | 1,657 B |
| `toy-embed` | embedding claim, readiness (DG-2/3) | 326,785 B | 750 | 51,784 B | 8,213 B |
| `wide-embed` | held leaf challenge (DG-6/7a/7b) | 329,600 B | 4 | 8,736 B | 605,362 B (over one carrier: tag 90 only) |
| `sd3-tiny` (Plan A) | the reduced SD3 pipeline replaces `toy-image` | 2,301,184 B | 15,738 | 1,065,616 B | 73,676 B (every cone-closable kind under one carrier) |

Plan A is selected with `IMAGE_CLASS=sd3-tiny DENOISE_STAGE=2` (audit-gen/dg.sh). Plan B stays the default; Plan A replaces it
only on the coordinator's word. A `sd3-tiny` claim is ~1 MB of material and 13 stages (text rows, text pooled, denoise,
ten VAE): a seat's replay and the capture fetch are the first things the drill exercises beyond the toy classes.

## 2. Fences and flags the generative drill needs

One flag per fence (lane A's list), in this order on the command line, at lane C's heights:

| fence | flag | DAA | what the drill checks at it |
| --- | --- | ---: | --- |
| IR (`palw_tir_v1`) | `--palw-drill-tir-at` | per lane C (before 28) | prerequisite of every pipeline fence |
| `palw_gen_v1` | `--palw-drill-gen-at` | 28 | DG-1: a generative registration is dropped by name below it, accepted from it |
| `palw_fp_decode_rules` | `--palw-drill-decode-rules-at` | 32 | prerequisite of the v10 claim |
| `palw_fp_job_v5` | `--palw-drill-fp-v5-at` | 104 | DG-1: a v10 tensor claim is skipped below it, accepted from it; DG-2 readiness |
| `palw_held_close_chunks_v1` | `--palw-drill-held-chunks-at` | 140 | DG-1: the wide class is refused (PALW-GEN-21) below it, registrable from it; DG-6/7a/7b |

`--palw-drill-held-chunks-at` is NEW in this layer: the mover is `config::drill::palw_drill_held_close_chunks_at_v1`
(consensus/core, tested: `palw_held_close_chunks_fence.rs`, 6 tests), the entry `PALW_HELD_CLOSE_CHUNKS_ENTRY_V1`. It needs
`palw_tir_v1` and `palw_held_context` in force at or below it, refuses the same height as `--palw-drill-tir-at`, and is
command-line only (never in a release). Lane A owns `kaspad/src/args.rs` and wires it at integration.

CLI used (all in `misaka-cli`, compiled): `palw gen-registration`, `palw gen-claim` (`--plant step:<global leaf>:<lane>:<delta>`
or `output:<lane>:<delta>`, `--list-leaves <file>`, `--retention-dir`), `palw submit-object`, `palw fp-submit`. A claim's
retention dir is `<appdir>/<network>/palw-retention`.

## 3. Scenarios (audit-gen/dg.sh, functions over lane C's lib; `dgwatch.py` reads the chain)

| id | what | expected |
| --- | --- | --- |
| DG-1 | the three crossings (gen at 28, fp-v5 at 104, held at 140) | dropped/refused below, accepted from |
| DG-2 | embedding class registered late; a claim at 104 | `GenClassNotReady` until ≥ 5 operators hold the class, then taken |
| DG-3 | honest image and embedding claims | both reach Final (licence + the 120-DAA window) |
| DG-4 | bond 10 plants a cone lie at the denoiser's first commit leaf, bond 11 an output lie | both convicted in one move |
| DG-5 | nine unlicensed claims from one bond | the ninth refused `BondClassShareExceeded` |
| DG-6 | bond 12 lies in the wide class | the refuting seat declares the close, delivers its chunks, the claim is convicted |
| DG-7a | DG-6 with the accusing seat stopped between the declaration and the last chunk | it resumes inside the assembly clock; the close completes |
| DG-7b | bond 13 lies in the wide class; the accusing seat is kept down past `4 · count` DAA | the declarer is charged; the claim is NOT convicted |

Liar bonds 10..13 and their keyring are lane C's (`$WORK_DIR/liars/`).

## 4. What was verified before this ships (compiled, not drilled)

* consensus-core: `palw_held_close_chunks` 7/7, `…_fence` 6/6, `palw_gen_one_move` 6/6, `palw_gen_claim_wire` 11/11,
  `palw_gen_claim_fold` 8/8, `palw_gen_dissect` 3/3, in-crate `held_close_chunks_clock` 6/6, goldens pin tests 6/6.
* kaspad: `palw_panel::held_chunks` 5/5 and the verdict-block pin; processor gate test for tag 90 1/1.
* SDK: `gen_drill_classes` 2/2 (each Plan B class passes the gate; a planted lie is convicted with the close the lane expects);
  `gen_sd3_class`: the class passes the gate, 61 of 63 commit-point kinds are cone-closable and every one fits one carrier;
  the 2 others are the CLIP stages' fused-attention outputs (dissected: ADR-0103's held dissection, like every LLM class).
* NOT exercised: `gen-claim --plant`/`--list-leaves` against a node, the filer's chunk delivery against a live chain, the
  capture fetch of a 1 MB sd3 claim. Those are what the drill is for.
