# RFC-0001 serving surface (node only, no consensus) — operator page

Everything here changes what a gateway/worker serves and nothing a block contains. No flag below arms a fence.

| need | flag | notes |
|---|---|---|
| KV prefix cache (§2.6 stage 1) | worker `--kv-cache-budget-mib N`, `--kv-cache-verify-every K` (via the gateway) | LRU by bytes; keyed by class, tokenizer and the exact prefix ids. A boot self-check and a sampled re-prefill (every K-th hit, logits and K/V) disable the cache for good on any difference. Serves the **answer-only** path (`--no-answer-fast-path` turns that path off): the committed run still folds every prefill tile under a ctx-bound leaf hash, so it is not faster — an inherited-leaf form (stage 2b) is not built. |
| concurrency (§2.7) | gateway `--worker-processes N`, `--max-connections-per-source`, `--max-jobs-per-source` | N resident workers per class (each holds the artifact: mind RAM); the decode scheduler batches the `n` candidates and concurrent requests, batch-invariant (a sequence's ids are what it selects alone — golden tested at engine, scheduler, backend and frame level). |
| `n > 1` (§2.4) | request `n` (≤ 8) | candidate `i` runs under seed `H(base_seed ‖ i)`; each is an ordinary job. |
| local embeddings (§2.8) | `POST /v1/embeddings`, `misaka.pool` mean/last, `misaka.normalize` | local only: no job, no claim, no reward. `misaka.claim: true` is refused by name (the claim form is RFC-0003's `Embedding` profile, `tensor::embedding_claim_job_v1`). |
| artifact sidecar v2 (§2.9) | gateway/worker `--sidecar FILE` | tokenizer.json, chat template spec, generation defaults, each with a digest; read side only; a template or default the model does not declare is refused by name. |
| images (§2.11) | request content part `palw_image_tensor` `{h, w, format:"u8_hwc_rgb", data:<hex>}` | decoded integer tensors only; URLs and encoded images are refused by name. Executed only behind a class with image slots (none behind this worker): otherwise refused by name at the run. |

## Dormant fences this lane added (all `None` on every preset, in no t12 flag-day list)

`palw_fp_prefix_state` (FP job version 11, prefix-state claim; credit side only), `palw_fp_tokenizer_match` (a job's tokenizer must be the class's
listed one), `palw_fp_constraint_v2` (the second constraint form: bigger bounds, JSON Schema `$ref` / `anyOf` / integer ranges compiled by
`misaka-palw-constraint::compile_v2`; **cannot be armed**: its prerequisite `palw_fp_decode_constraint` is not buildable), `palw_adapter_class_v1`
(ADR-0163, object tag 94). Salted drills arm them with `--palw-drill-fp-prefix-at`, `--palw-drill-fp-tokenizer-at`, `--palw-drill-fp-constraint2-at`,
`--palw-drill-adapter-at` (each over its prerequisites, refused by name otherwise).
