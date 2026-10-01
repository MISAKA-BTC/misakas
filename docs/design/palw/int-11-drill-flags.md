# int-11 drill flags — the frozen set

Status: **frozen 2026-10-01 by lane A** for lane C's drill scripts. Everything here is in the tree on the int-11 line
(`kaspad/src/args.rs`, `kaspad/src/palw_drill.rs`, `consensus/core/src/config/drill.rs`); a flag that is not in this file is
not part of the int-11 drill. Lane F's class-seating flag is **reserved** (its row is below) and is added to the tree by lane A
when lane F's mover lands.

The rule of the set is **one flag per fence**: a drill arms or moves exactly one fence (or, for the three flag-day flags, the
whole fence list of that flag day), through that fence's own `set`, so every fold mirror follows, and moves nothing else.

## 1. Rules every flag shares

* **Every flag needs `--palw-drill-genesis-salt`** (a drill is a salted testnet-12 chain). Without it the node refuses at
  start-up and names the flag. On any network but testnet-12, and on public testnet-12's own genesis, no flag applies.
* **A height is refused, by name and never by a panic,** when it is `0` or `never()`; when it equals the height of any other
  fence on the ruleset (the fork id names heights, not fences, so a node started without the flag would peer across the height
  and fork silently); when the move would move nothing; and when `validate_palw_v2` refuses the result (a prerequisite that is
  not in force at or below the height — the table's "needs" column).
* **Heights must therefore be distinct.** The layout in §3 gives every flag a height of its own.
* **A stored drill chain is never reopened under another set.** The datadir marker (the file `palw-drill-genesis` in the node's app directory, one `key=value`
  line per flag group) records the set; a start under another set is refused and names the flag that differs. A marker written
  before a flag existed reads `none` for it, so a chain a release line's binary started reopens under this build.
* **`--palw-drill-write-keyring=DIR`** writes the keyring and a `manifest.json` (the seeds, the addresses, the seat outpoints, the
  drill genesis, `consensus_params_id`) computed under the same flags, so the fingerprint the manifest announces is the one the
  node on the same command line announces. Pass the same fence flags to the keyring export and to every node.
* The flags are **command line only** (`#[serde(skip)]`, no environment variable): a unit file or an env file shared with a
  public node can never arm one by accident.

## 2. The set, in the order the node applies it

The flag days first, then the extras in this order. Each row's "needs" are fences that must be in force **at or below** the
flag's own height; arm them first (a lower height) or the node refuses the flag by name.

| # | Flag | Arms or moves | Fence | Needs | Marker line | Manifest key |
| --- | --- | --- | --- | --- | --- | --- |
| 1 | `--palw-drill-fence-at=H` | moves the DAA-750 flag day's list | `PALW_T12_POST_LAUNCH_FENCES_V1` | — | `fence_at=` | `fence_at` |
| 2 | `--palw-drill-fence2-at=H` | moves the second post-launch flag day | `PALW_T12_POST_LAUNCH_FENCES_V2` | a height of its own | `fence2_at=` | `fence2_at` |
| 3 | `--palw-drill-fence3-at=H` | moves the capacity architecture at ρ = 10 — the whole capacity list, F-N included | `PALW_T12_POST_LAUNCH_FENCES_V3` | a height of its own | `fence3_at=` | `fence3_at` |
| 4 | `--palw-drill-tir-at=H` | moves the IR fence | `palw_tir_v1` | `palw_audit_2026_09_11` declared; `palw_kary_court` and `palw_rcore_plus` at or below (combine with `fence-at` below it) | `tir_at=` | `tir_at` |
| 5 | `--palw-drill-tir2-at=H` | moves the DAA-3,600 list (the second IR fence) | `palw_tir_fence2` | `palw_tir_v1` | `tir2_at=` | `tir2_at` |
| 6 | `--palw-drill-model-court-at=H` | moves the per-model finite court window | `palw_model_court_window` | — | `model_court_at=` | `model_court_at` |
| 7 | `--palw-drill-capacity-network-room-at=H` | moves F-N alone (the network level and the work-conserving fair share) | `palw_capacity_network_room` | F-R (verify room) and F-S (issuance slots) at or below | `capacity_at=room:` | `capacity_network_room_at` |
| 8 | `--palw-drill-capacity-network-verify-at=H` | **arms** F-N's static verification term, `L_ver` = 435 at the shipped 600 / 20 | `palw_capacity_network_verify` | F-N at or below | `capacity_at=…verify:` | `capacity_network_verify_at` |
| 9 | `--palw-drill-capacity-step2-at=H` | appends F-L's ρ = 25 step | `palw_capacity_aggregate_liability` | the ρ = 10 step below it (`fence3-at`) | `capacity_at=…step2:` | `capacity_step2_at` |
| 10 | `--palw-drill-capacity-rho100-at=H` | appends ρ = 100 straight after ρ = 10 — **instead of** 9 and 11, never with them | same | the ρ = 10 step below it | `capacity_at=…rho100:` | `capacity_rho100_at` |
| 11 | `--palw-drill-capacity-step3-at=H` | appends F-L's ρ = 100 step after ρ = 25 | same | step 2 below it | `capacity_at=…step3:` | `capacity_step3_at` |
| 12 | `--palw-drill-gen-at=H` | arms RFC-0003's generative fence | `palw_gen_v1` | `palw_tir_v1` | `extra_at=gen:` | `gen_at` |
| 13 | `--palw-drill-decode-rules-at=H` | arms the decode rules | `palw_fp_decode_rules` | — | `extra_at=…decode_rules:` | `decode_rules_at` |
| 14 | `--palw-drill-fp-v5-at=H` | arms FP Job V5 (and with it the tensor claim's door, FP job version 10) | `palw_fp_job_v5` | `palw_gen_v1`, `palw_fp_decode_rules` | `extra_at=…fp_v5:` | `fp_v5_at` |
| 15 | `--palw-drill-held-chunks-at=H` | arms RFC-0003's held leaf challenge (object tag 90) | `palw_held_close_chunks_v1` | `palw_tir_v1`, `palw_held_context` | `held_chunks_at=` | `held_chunks_at` |
| 16 | `--palw-drill-class-seating-at=H` | **reserved** — lane F's class-seating fence (the possession floor and the independence read) | `palw_class_seating` (name fixed by lane F) | lane F states them | `class_seating_at=` | `class_seating_at` |
| 17 | `--palw-drill-improve-at=H` | arms RFC-0004's improvement fence (pipeline-claim data availability, spec 17 §17.14, rides with it) | `palw_improvement_v1` | `palw_tir_v1`, `palw_tir_fence2`, `palw_gen_v1`, `palw_kary_court`, `palw_fp_decode_rules` | `extra_at=…improve:` | `improve_at` |

Marker lines. The release line's own `tir2_at=` and `model_court_at=` are unchanged. The capacity flags share `capacity_at=`
(`none` when none stands, else `room:…,verify:…,step2:…,step3:…,rho100:…`). The four RFC-0003 / RFC-0004 flags of 12–14 and 17
share `extra_at=` (`gen:…,decode_rules:…,fp_v5:…,improve:…`). Every flag added after `extra_at=` has a line of its own
(`held_chunks_at=`, and `class_seating_at=` when it lands), so no earlier line changes its text.

## 3. The height layout (lane C's drill D, frozen)

All heights are distinct (the fork-id rule above) and respect the "needs" column:

| Height | Flag |
| --- | --- |
| 6 | `--palw-drill-fence-at` |
| 10 | `--palw-drill-fence2-at` |
| 14 | `--palw-drill-fence3-at` (F-N is armed here with the capacity list) |
| 20 | `--palw-drill-tir-at` |
| 24 | `--palw-drill-tir2-at` |
| 28 | `--palw-drill-gen-at` |
| 32 | `--palw-drill-decode-rules-at` |
| 36 | `--palw-drill-model-court-at` |
| 40 | `--palw-drill-improve-at` |
| 104 | `--palw-drill-fp-v5-at` |
| 140 | `--palw-drill-held-chunks-at` |
| 150 or 300 | `--palw-drill-class-seating-at` (reserved; lane C proposes 150, lane F 300 — they settle it) |
| 400 | `--palw-drill-capacity-network-verify-at` (proposed: after F-N at 14, before the ρ = 25 step, so blocks fold both below and above the cap) |
| 560 | `--palw-drill-capacity-step2-at` (ρ = 25) |
| 655 | `--palw-drill-capacity-step3-at` (ρ = 100) |

`--palw-drill-capacity-network-room-at` is not in the layout: F-N is armed at 14 by the flag day; use the flag only to time it
apart (any height at or above F-R and F-S, and at or below 400 when the verify flag is used).

## 4. What the int-11 flag day arms, and what has no drill flag

The production flag day (a height the coordinator names) arms, each through the fence's own `set`: `palw_improvement_v1`,
`palw_gen_v1`, `palw_fp_decode_rules`, `palw_fp_job_v5`, `palw_held_close_chunks_v1`, `palw_class_seating` (when it lands),
`palw_capacity_network_verify`, and the capacity steps the release names. Rows 12–17 drill exactly those, one flag each. The
sub-features of `palw_improvement_v1` (the evaluation court, the pipeline-claim DA units and objects 83–86, 89) are not fences of
their own and have no flag: they are in force when row 17 is.

## 5. Fault injection (not fences)

Flags that make a node lie or withhold, to exercise the courts; they apply on a salted drill chain (and devnet/simnet) only.

| Flag | Effect |
| --- | --- |
| `--palw-drill-tamper-leaf=N` | corrupts one lane of step leaf N in every block this node produces |
| `--palw-drill-tamper-fp-leaf=N` | this node's canonical free-prompt claims commit a capture with one lane of leaf N corrupted (the one-move court's drill) |
| `--palw-drill-tamper-eval=leaf:N\|output\|score[@LINEHEX][/parent\|candidate]` | commits one evaluation with a fault: a self-consistent lie an honest replay disputes, told again on the next job until one lands (RFC-0004 D-M3) |
| `--palw-drill-challenge-all` | opens a court against every licensed claim, reproduced or not |
| `--palw-drill-answer-only` | free-prompt claims are broadcast and served as their answer envelope, never the capture (ADR-0111's drill) |
| `--palw-drill-refuse-leaf-evidence` | refuses every leaf-evidence request on the interval lane, so a seat that named a leaf must demand its evidence on chain |

## 6. Scenario inputs that use these flags

* Lane D's generative drill (DG-1 … DG-7b): `docs/design/palw/rfc3-gen-drill-inputs.md` — the classes (`toy-image`, `toy-embed`,
  `wide-embed`, `sd3-tiny`), the flags it needs (`--palw-drill-tir-at`, `-gen-at` 28, `-decode-rules-at` 32, `-fp-v5-at` 104,
  `-held-chunks-at` 140, in that order) and what each crossing checks; the harness is lane C's (`audit-tir/`, `audit-gen/dg.sh`).
* Lane C's improvement drills (the evaluation court, D-M3 with `--palw-drill-tamper-eval`) use rows 4–6 and 12–14 and 17.

## 7. A command line, end to end

```
kaspad --testnet --netsuffix=12 --nodnsseed --palw-drill-genesis-salt=<64 hex> \
  --palw-drill-fence-at=6 --palw-drill-fence2-at=10 --palw-drill-fence3-at=14 \
  --palw-drill-tir-at=20 --palw-drill-tir2-at=24 --palw-drill-gen-at=28 --palw-drill-decode-rules-at=32 \
  --palw-drill-model-court-at=36 --palw-drill-improve-at=40 --palw-drill-fp-v5-at=104 \
  --palw-drill-held-chunks-at=140 --palw-drill-capacity-network-verify-at=400 \
  --palw-drill-capacity-step2-at=560 --palw-drill-capacity-step3-at=655
```

The keyring export takes the same fence flags: `kaspad … --palw-drill-write-keyring=DIR` (add them all, or the manifest
announces another ruleset's fingerprint).
