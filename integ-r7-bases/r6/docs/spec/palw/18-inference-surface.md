# 18. The inference surface rules (RFC-0001) — normative text

Status: implemented 2026-10-02..03 on `rfc1/serve`; every rule below is behind its own fence, `None` on every preset except testnet-12, whose release arms
all six (`palw_fp_decode_constraint`, `palw_fp_constraint_v2`, `palw_fp_prefix_state`, `palw_fp_prefix_inherit`, `palw_fp_tokenizer_match`,
`palw_adapter_class_v1`) at DAA 5,300 (the int-12 flag day, `PALW_T12_INT11_FENCES_V1`, in prerequisite order). Numbers in
brackets are the RFC-0001 section.

## 18.1 FP job versions

| version | job | fence | tail |
|---|---|---|---|
| 5 | V3 (greedy) | — | none |
| 6 | constrained | `palw_fp_decode_constraint` | the decode constraint's canonical bytes |
| 7 | V4 (decode rules) | `palw_fp_decode_rules` | none |
| 8 / 9 / 10 | V5 images / evaluation / tensor | their own | their own |
| 11 | prefix-state | `palw_fp_prefix_state` | `PalwFpPrefixStateV1 { state_root, prefix_tokens, class_id }` |

Each version has its own `fp_job_id` domain. The isolation door admits a version only where the ruleset carries its fence at all;
the header-context door refuses it below its height; the extraction walk judges it on its stand-in (version 6: the V3 job, version
11: the V4 job) and skips by name otherwise.

## 18.2 The constrained job (ADR-0096 D6-8, [§2.5])

1. The job is a V3 job (`decode = None`, greedy) whose tail is a decode constraint: the byte automaton of
   `palw_decode_constraint_v1`, header version 1 (`PALW_CONSTRAINT_MAX_BYTES_V1`) or, past `palw_fp_constraint_v2`, version 2
   (`PALW_CONSTRAINT_MAX_BYTES_V2`).
2. The class's token table renders every id to bytes and names its end-of-generation ids; a host may judge a constrained job only
   when its table was derived under the job's `tokenizer_id`.
3. Selection at position `t` with state `s` = the automaton after the rendered committed prefix: the argmax of the
   `decode_lane_key_v2` over the lanes `j` whose rendering is non-empty, is not an end-of-generation id, and keeps the automaton
   alive from `s`; ties to the lowest index. No lane admitted: the committed token is the lowest end-of-generation id and the run
   ends there with `EndOfGeneration`. A seat replays through the same function (`PalwFpReplayRuleV1`); a host without the table
   files `Unverifiable`.
4. The court's third arm — the committed token at `p` is not admitted from `s`, proven from the ids, the constraint and the table —
   is `check_tiled_decode_token_refutation_v3`. (Not yet a proof kind on the wire; see ADR-0096's note.)

## 18.3 The prefix-state job ([§2.6] stage 2)

A version-11 job is a V4 job naming the KV prefix state its run consumed. The state root is
`H("misaka-palw/fp-prefix/state-root/v1", class ‖ prefix_tokens ‖ kv_digest)`; a seat derives it from the claim's own prompt prefix
(from its cache or by recomputation) and disagreement is a fault in the claim. The fold's `KvReused` credit prices the new positions
only. With the decode rules armed D10 already credits decode work only, so the version moves no price there. **Stage 2b (inherited
leaves) is not specified here**: leaves are bound to the whole job context, so inheriting a cached prefix's committed leaves needs a
job-independent prefix context for the first `k` positions in every leaf-binding function and a segment-aware court binding; caching
the per-position rows instead costs gigabytes per position at model sizes.

## 18.4 Tokenizer match ([§2.9]) and the adapter class listing ([§2.10], ADR-0163)

`palw_fp_tokenizer_match`: a commitment's job `tokenizer_id` must equal the class's listed one (a class listing none is not asked).
`palw_adapter_class_v1`: object tag 94 lists a registered composite IR class in the composite registry; its acceptance is RFC-0004's
composite admission without a governed line.

## 18.5 Images and embeddings ([§2.8], [§2.11])

Images enter as decoded `u8` HWC RGB only; an image becomes an FP Job V5 slot reference (`input_root` at the slot's tile length).
An embedding claim is an RFC-0003 `Embedding`-profile tensor job. Neither adds a rule to consensus.

## 18.x Stage 2b: inherited prefix leaves (FP job version 12, fence `palw_fp_prefix_inherit`, dormant)

A version-12 job is a version-11 prefix-state job whose step leaves over the prefix positions are hashed under a job-independent
prefix context (version-3 `PalwJobContextV3`-style context: `job_nullifier` carries k, `assignment_id` the prefix state root;
`step_tile_leaf_hash_ctx_v1`). Leaves over a shared prefix are therefore byte-identical across jobs, equal the recomputed root,
and a lie in an inherited range is convictable through the earlier claim that committed the honest leaf. Version-2 contexts and
leaves are unchanged bit for bit. Limits: checkpoint leaves, KV aux chunks and fused-attention (attn court) paths are not
inheritance-aware (the admission rule restricting version 12 to non-fused, non-aux classes is not yet written); no executor
compute saving or seat prefix-leaf cache yet. Drill flag: `--palw-drill-fp-prefix-inherit-at`.
