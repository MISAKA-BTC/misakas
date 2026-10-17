# FP Job V4 — migration guide

For gateway operators, free-prompt (FP) producers and API clients of the MINING lane
(`misaka-palw-gateway`'s FP lane). Normative spec: RFC-0001 §A; bytes: `job-v4-wire-spec.md`.

## What changes, and when

Nothing changes until the flag day that arms `Params::palw_fp_decode_rules` (height `H`, set by the
release; dormant on every preset until then). At `H`, three rules move together (one fence):

1. **Only FP Job V4 is admitted for new jobs.** A new V3 commitment at or past `H` is refused
   (header context and extraction walk). V3 claims accepted before `H` finish under the V3 verifier.
2. **ADR-0082 D11 — sampling.** A V4 job may carry `temperature_q` and `sampling_seed`; the committed
   token is the seeded argmax (Gumbel-max) of the processed logits. `temperature 0` is greedy.
3. **ADR-0082 D10 — pay for decode.** A claim's quanta are counted on its executed DECODE work; the
   prefill of the prompt is priced at zero; an answer a stop sequence ended early earns for what it
   decoded.

## For API clients (OpenAI-compatible surface)

Past `H` these request fields are rules (they change the committed tokens, the seats replay them, and
they are inside the job id). Below `H` they are refused by name as before.

| request field | V4 meaning | range / normalization |
|---|---|---|
| `temperature` | D11 temperature | `0..=MAX_TEMPERATURE`, Q24 |
| `seed` | D11 seed | 64 hex chars (absent = zero seed) |
| `repeat_penalty` | multiplicative repeat penalty | `1.0..=4.0`, Q16 (round to nearest) |
| `frequency_penalty`, `presence_penalty` | additive penalties | `-2.0..=2.0`, Q24 |
| `repeat_last_n` | the penalties' window `W` (generated tokens only) | `1..=256`; default 64 when a penalty is active; dropped (listed in `misaka.ignored_fields`) when none is |
| `logit_bias` | `{token_id: bias}` | `-100..=100`, Q24; `-100` BANS the token; `0` entries dropped; ≤ 300 |
| `stop` | string or array of strings | ≤ 4, non-empty; each is encoded ALONE by the class's tokenizer into ≤ 16 ids |

Still refused by name past `H`: `top_p`, `top_k`, `min_p` (except their identity values), `n > 1`,
`logprobs`, `top_logprobs`, and every field the surface does not name.

**Stop semantics.** A stop string becomes the token sequence it encodes to on its own. Generation
ends when the committed answer's tail equals that sequence; the stop tokens are part of the committed
answer and are cut from the displayed answer (`finish_reason: "stop"`). If the model produces the
same characters split into different tokens, the sequence does not match and generation continues —
the response says so in `misaka.sampling.decode_v4.stop_note`.

**Penalty window.** Penalties count only the last `W` GENERATED tokens (never the prompt), and the
frequency/presence window is the same `W` (unlike OpenAI's whole-answer count).

## For producers (workers) and gateway operators

* Run the release's `misaka-palw-base0` worker and `misaka-palw-gateway` together; the gateway sends
  V4 requests (version 7, with the decode config and stop strings) once the node reports
  `fp_decode_rules_armed`, and a V3 request below `H`.
* Every job past `H` is V4, including a request that asks for nothing (the no-op config): a V4 no-op
  job decodes exactly what the same V3 job decodes, with the same work (G7), but it is a different
  job with a different id.
* Stop V3 FP submissions a few blocks before `H`: a V3 commitment accepted at `H` or later is refused,
  and its inference is lost.
* Early stops cost the producer one extra capture: the engine runs to the budget and then once more at
  exactly the stop (a step leaf binds the executed count). Only jobs with stop sequences can stop early.

## For seat operators

Nothing to configure. The panel replays a V4 claim's intervals under the claim's rule (the job's
pipeline over its committed answer) and checks the answer's stop before signing; V3 claims keep the
V3 verifier.

## For court participants

The argmax decode-token closes (`DecodeToken`, `DecodeTokenTiled`) are refused for a free-prompt
claim accepted at or past `H` (`DecodeCloseIsNotTheClaimsRule`); arithmetic closes are unchanged. A
V4 two-tile decode arm (RFC-0001 §A.3 I-2: the committed and the beating lane's tiles, the window of
generated ids, the bias entries) is a follow-up; until then a V4 claim's token selection is policed by
the seats' replay.
