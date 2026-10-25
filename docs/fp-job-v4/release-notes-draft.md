# Release notes (draft) — FP Job V4: Deterministic Decode Pipeline

**Status:** draft for the testnet-12 release that arms `palw_fp_decode_rules` at the DAA-1,500 flag
day together with the capacity work. Code lane: `rcore/fp-sampler` (RFC-0001 §A, Implementation
Frozen).

## Highlights

* **Deterministic decode controls on the mining lane**: repeat, frequency and presence penalties over
  a window of generated tokens, `logit_bias` (including bans), stop token sequences — all integer,
  all inside the job id, all replayed by the panel.
* **Sampling (ADR-0082 D11)**: temperature and seed, the seeded Gumbel-max argmax.
* **Pay for decode (ADR-0082 D10)**: claims earn on their executed decode work; the prompt's prefill
  is priced at zero.
* **FP Job V4 wire**: the V3 job plus `DecodeConfigV4`, hashed under its own domain. V3 bytes are
  unchanged.

## Consensus changes (armed only at the flag day)

| rule | before the fence | from the fence |
|---|---|---|
| new FP job version | V3 only (greedy) | V4 only |
| committed token | argmax of the raw logits | §A.3 pipeline + D11 key |
| FP claim credit | whole run (derived) | decode work only (D10) |
| decode-token court close | argmax arms | refused for V4-era FP claims |

While the fence is dormant every network's consensus params fingerprint is unchanged (testnet-12
`24e1aec3…`, schedule `d263d7f2…`); arming it at the flag day is a one-entry addition
(`PALW_FP_DECODE_RULES_POST_LAUNCH_FENCE_V1`) to the flag day's list, and moves the fingerprint and
schedule id as every flag day does. The V2 bundle's ruleset id does NOT move at deploy (the height
rides a borsh-skipped mirror), so un-upgraded nodes keep peering until the fence.

## Node, worker and gateway changes (ship with the release)

* All three FP engines (floor, A16 dense tiers, Qwen3.6 hybrid) decode through one consensus-core
  decoder; early stops re-run the capture at the stop count.
* The worker spells stop strings with the class's tokenizer.
* Seats verify V4 claims under the claim's rule and check the stop; executors open their own
  retentions by feeding the committed ids back.
* The gateway normalizes OpenAI-style requests into the canonical V4 config past the fence, cuts stop
  sequences from the displayed answer, and reports what was applied.

## Compatibility

* A V4 no-op job decodes the same tokens with the same work as the same V3 job (checked on 1,716
  engine runs across the floor and A16 tiers, greedy and sampled, plus 164 cross-family pairs).
* Mixed fleets are safe before the fence: isolation refuses the V4 shape on a ruleset without the
  fence, and the header-context door refuses it below the fence on one with it.

## Known limitations

* The V4 two-tile decode-token court arm is not in this release (V4 selection is policed by the
  seats' replay; arithmetic refutations apply as before).
* The Qwen3.6 fixture's anchored intervals are not covered by the V4 seat test at fixture scale (its
  V3 claims fail the same way there); its genesis interval is.
* A stop string that the model generates with a different token split does not stop generation.
