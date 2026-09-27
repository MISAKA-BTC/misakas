# FP Job V4 — wire specification

Normative source: RFC-0001 §A (Implementation Frozen). This document states the bytes; where it and
the RFC differ, the RFC wins and this document is wrong. The executable form of every rule below is
`consensus-vectors/fp-v4/` (see "Vectors").

## 1. `DecodeConfigV4`

Borsh, fields in this order (`consensus/core/src/palw_decode_pipeline_v4.rs`):

| # | field | type | unit | admissible |
|---|---|---|---|---|
| 1 | `repeat_penalty_q` | `u32` | Q16 (`65536` = 1.0 = off) | `[65536, 262144]` |
| 2 | `penalty_window` | `u16` | generated tokens | `0` iff every penalty is off, else `1..=256` |
| 3 | `frequency_penalty_q` | `i32` | Q24 logit | `[-2·2^24, 2·2^24]` |
| 4 | `presence_penalty_q` | `i32` | Q24 logit | `[-2·2^24, 2·2^24]` |
| 5 | `logit_bias` | `Vec<(u32, i32)>` | (token id, Q24 logit) | ≤ 300, ids strictly ascending, bias in `[-100·2^24, 100·2^24] ∖ {0}`; `-100·2^24` bans |
| 6 | `stop_sequences` | `Vec<Vec<u32>>` | token ids | ≤ 4, each `1..=16` ids, strictly ascending lexicographically |

**One canonical form per behaviour.** The no-op is exactly `{65536, 0, 0, 0, [], []}`
(`DecodeConfigV4::NOOP`). Anything else that is out of range, out of order, duplicated, empty or a
zero bias is refused **by name** (`PalwDecodeConfigV4Error`, checked by
`DecodeConfigV4::validate_canonical` in this order: penalties and window, `logit_bias`,
`stop_sequences`).

## 2. `PalwFreePromptJobV4`

A `PalwFreePromptJobV3` at **version 7** (`PALW_FP_V4_VERSION`): every V3 field in V3 order,
then the `DecodeConfigV4`. In Rust it is the same type (`PalwFreePromptJobV4 = PalwFreePromptJobV3`)
with a `decode: Option<DecodeConfigV4>` tail whose presence the version decides:

* version 5 (V3): the bytes end at `temperature_q` — byte for byte the V3 job as it always was;
* version 7 (V4): the bytes continue with the `DecodeConfigV4`;
* version 6 is ADR-0096 Decision 8's constraint job, reserved and unbuilt; versions are not reused.

A version/tail mismatch (V4 without a config, V3 with one) is refused as
`DecodeConfigVersionMismatch`. Borsh serialization never fails on it (a hash is computed over
whatever the struct holds); validation refuses it.

**Job id.** `fp_job_id_v4(job) = BLAKE2b-512_key("misaka-palw/fp-v4/job-id/v1")(len_le64(bytes) ‖ bytes)`
over the whole borsh of the V4 job. `fp_job_id_v3` — the one job-id function every path calls (the
trace binding, the seat's anchor, the job context, the court) — returns `fp_job_id_v4` for a version-7
job, so a V4 job is named by its V4 id everywhere. The V4 domain differs from the V3 one
(`misaka-palw/fp-v3/job-id/v1`), so no V4 job can share a V3 job's id.

**Everything else is unchanged.** The commitment, the envelope, the transaction payload
(`PalwFpCommitmentTxPayloadV3`, version 5) and the worker result keep their layouts; each carries the
job, and the job says which job it is. The claim id (`fp_claim_id_v3`) hashes the whole commitment,
so it covers the decode config.

## 3. Admission — one job version per side of the fence

`Params::palw_fp_decode_rules` (height `H`, dormant on every shipped preset) is resolved to
`PalwFpDecodeRulesV1`:

| rule | where | V3 job | V4 job |
|---|---|---|---|
| `Dormant` | below `H`, or no fence | admitted (greedy only) | refused `DecodeRulesNotArmed` |
| `Scheduled` | the height-free isolation door of a ruleset that carries the fence | shape admitted | shape admitted |
| `Active` | at or past `H` | refused `V3JobPastDecodeRules` | admitted |

* **Isolation** (`validate_palw_fp_commitment_tx_under_v5`) takes the door rule, so a ruleset without
  the fence refuses the V4 shape exactly as a pre-V4 build refuses its bytes.
* **Header context** (`check_palw_fp_job_version_in_context`) refuses, at the containing block's DAA,
  a V4 commitment below `H` and a new V3 commitment at or past `H` (`PalwFpJobVersionAtHeight`). With
  the isolation answer this keeps a build that schedules the fence and one that does not in
  agreement on every transaction below `H`.
* **Extraction walk** re-checks at the accepting block's DAA (the authoritative rule).
* A V3 claim accepted before `H` is state: it is verified to the end by the V3 verifier.
* Sampling: a V3 job is always greedy (`SamplingNotArmed` otherwise); a V4 job carries ADR-0082
  Decision 11 (`temperature_q`, `sampling_seed`), with a non-zero seed at temperature 0 refused as a
  second encoding.

## 4. Worker request (gateway ↔ worker, not consensus)

`PalwFpWorkerRequestV3` at version 7 carries, after every V3 field, the `DecodeConfigV4` and then
`stop_texts: Vec<Vec<u8>>` — the API's stop strings, which the worker spells into stop sequences with
the class's tokenizer (each string alone, special tokens off; empty or > 16 ids refused by position).
A version-5 request is byte for byte the V3 request. The worker result must return the request's
version, sampler inputs and decode config (stop sequences may only be the request's plus the
spelled strings).

## 5. Execution facts

* A V4 run stops where its pipeline stops: a completed stop sequence (the stop tokens are part of the
  committed answer), or an empty admitted set; otherwise at `decode_token_limit`. The commitment's
  `decode_tokens_executed` is that count and `stop_reason` is canonical for it
  (`ExactBudgetReached` iff the budget was used).
* `decode_answer_stop_v4(config, limit, vocab, answer)` decides from the job and the committed answer
  alone whether the answer ends where the rule ends it; seats and the gateway ask it.
* D10: past `H` the transition credits a free-prompt claim for its decode work only (the whole prompt
  accounted as paid: `total − prefill(prompt)` leaves, or the generated positions' compute).

## 6. Vectors

`consensus-vectors/fp-v4/` — generated by `scripts/palw-fp-v4-vectors.py`, an independent
implementation of §A.3 (`--check` proves the files are its output); `job_v4_encoding.json` is the
Rust wire's own pin (`cargo test -p kaspa-consensus-core --lib -- --ignored
regenerate_job_v4_encoding_vector`). Read by the sampler (consensus-core), the worker
(misaka-palw-base0) and the panel (kaspad) tests.

| file | pins |
|---|---|
| `repeat_penalty.json` | step 1 over values, counts and penalties |
| `frequency_penalty.json`, `presence_penalty.json` | step 2 |
| `logit_bias.json` | the bias list's canonical form (errors by name) and steps 3–5 per lane |
| `stop_sequences.json` | the stop list's canonical form, the tail matcher, a finished answer's first stop |
| `processor_order.json` | steps 1–7 in order, greedy and three sampled temperatures, the constraint mask with bans, whole decode loops |
| `job_v4_encoding.json` | input fields → borsh bytes → job hash (V3 reference and four V4 jobs) |
| `v4_noop_equals_v3.json` | the no-op V4 selects what the V3 rule selects |
