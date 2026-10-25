# Lane E report (2026-10-25): output path, winning carrier, worker connection

Base be6671afd, branch claude/rfc38-lane-e. Scope actually delivered is narrow; most of RFC01/03/18/20/38 stays open (see Remaining).

## 1. Winning-carrier source of truth (findings)
- Rooted state records the claim at `PalwClaimStateV2.accepted_block` (objects.rs `FreePromptCommitted` arm) but NOT the carrier txid. A second same-claim registration is refused as `DuplicateClaim`, and a refused carrier is never mined, so a valid chain has one fold-registered carrier per claim.
- The fold only sees a carrier if `palw_fp_answer_for_carrier_v1(tx, carrier, decode_tokens_executed)` succeeds (palw_accept.rs, `fp_answer` construction; a Decode error drops the object). Hence a same-claim tx failing that check cannot be the winner.
- Rollback/checkpoint: a state-derived index follows state rollback and checkpoint automatically. A separate side index would need its own rollback rule and is not introduced.
- Exact index (CODE, central, consensus-affecting): add carrier txid to the claim record (or a rooted claim->carrier map) written by the fold at registration. Changes state encoding, so needs a fence and version; not done here (central, statuent decision).

## 2. Output path to carrier map
| Emitter | Context source | Carrier consumer | State |
|---|---|---|---|
| node producer (kaspad/src/palw_producer.rs) | node native fence/context | FPA1 explicit carriage | emits |
| rail (gateway/src/bin/rail.rs, `accounting_context`) | quoted public binding header + native price quote; dies for plain worker results ("carry no context") | `wrap_fp_answer_accounting_tx_explicit_with_v1` (validator-core) | emits, requires public binding |
| fp-submit planner | staged FPA1 ids vs literal tx | `AccountingAnswerMismatch` pre-write | checked |
| plain worker / SDK | none: no authenticated native fence/context handshake | n/a | OPEN (CODE; needs node RPC context + policy for context identity) |
| seat check (kaspad/src/palw_panel/answer_accounting.rs) | duty.accepted_block acceptance data | `palw_fp_answer_bind_duty_v1` | see below |

## 3. Closed by this diff
- Seat accounting check no longer lets a same-claim carrier the fold would drop (bad framing/count/identity) make the real carrier "ambiguous". It applies the fold's own `palw_fp_answer_for_carrier_v1`; Decode failure skips, Resource failure stays typed Resource. Two fold-admissible same-claim carriers still fail closed (no exact index yet). Sound because skipped carriers cannot be the fold winner; carriers failing other fold gates still cause conservative ambiguity.
- Commit: see git log (`kaspad: ignore fold-dropped same-claim decoys ...`).

## 4. Tests
- `CARGO_INCREMENTAL=0 buildslot.sh cargo test -p kaspad --lib accounting_seat_` exit 0, 6 passed / 0 failed / 0 ignored (new: `accounting_seat_ignores_a_same_claim_decoy_the_fold_would_drop`, both orders plus decoy-alone refusal). Fixture BASE0; zero signature is transport only, not native approval. No real worker/gateway process was started; no campaign evidence.

## Remaining
- CODE: exact carrier id in rooted state (central, fenced); plain worker/SDK native fence+context handshake; completion/cancel/retry/dup-output across processes; multi-gateway consistency.
- TEST: real worker+gateway in scratch dir (wrong-context refusal, cancel/restart/reorg); RFC18 T0f/T1e/T3c/T3d top-level runs; re-execution cost/grinding measurement (conditions to state: model, ctx, answer length, number of grinding attempts per claim, backend, warm/cold) - not measured.
- POLICY: context identity for plain worker; RFC20 KV diskspill not introduced (no 32K need shown); deferred media stays closed.
- Not claimed: no RFC closed; SDK-only success not counted.

## For G (release generation)
Signature, provenance and artifact must be recorded per release generation: node binary SHA (seat/ingress), rail/gateway SHA, validator-core wrap signer, artifact root (`palw_rc_base0_artifact_root_v1` / class artifact_root in duty), fence set. Do not sum test counts across binaries; this run used one kaspad lib test binary built from this branch only.
