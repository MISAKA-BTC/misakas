# PALW-TIR — ModelSpec V1, the HF schema reader and model adapters as data

| Field | Value |
| --- | --- |
| Status | implemented in `misaka-palw-tir-lower` (branch `tir/generic`); the format is `misaka.palw.model-adapter.v1` |
| Crate | `misaka-palw-tir-lower`: `model`, `hf_schema`, `adapter`, `adapters/*.json` |
| Non-goals | consensus: nothing here is on a validation path; no primitive, runtime or court kernel is added |

## 1. The point

A model is registered by lowering it to PALW-TIR. Until now *which* models could be lowered was a
list of Rust parsers, one per `architectures[0]`: one more supported model meant a core developer
writing one more function. The aim here is the opposite: **no model needs a core developer.**

```text
HF config / tensor names ──hf_schema──▶ ModelSpec ──features()──▶ FeatureId set
        ▲                                   │
        │ adapter (a DATA file, optional)   └─ hl::build ─▶ HL ─▶ lower ─▶ PALW-TIR ─▶ admission, executor, court
```

* `model_type` is **informational**. Nothing is selected by it; two models with equal specs compute
  the same function whatever they are called.
* A model is a *combination* of **features** from a finite, versioned vocabulary (`model::REGISTRY`).
  A feature is lowered once, by a generic lowerer, to the existing primitives.
* Where a model uses keys, defaults or tensor names of its own, an **adapter** — a JSON file — maps
  them onto features. Rust code is allowed only for the features and the generic reader/lowerers.
* Where a model needs something the vocabulary lacks, that is a **missing feature (Level C)**, named,
  with the smallest *general* primitive that would close a protocol gap — never a per-model primitive.

## 2. Support levels

| Level | Meaning | Reported adapter |
| --- | --- | --- |
| **A** | the standard keys and tensor names suffice: the reader's own template (`standard-decoder`, itself data) reads the configuration; applied only to a `…ForCausalLM` class no adapter claims | `none` |
| **B** | an adapter file maps the class's keys onto features; config → ModelSpec only, no protocol change | built-in data file, user-supplied data file |
| **C** | a capability is missing: a feature that cannot be lowered (named), or a protocol gap (named, with its general closing primitive) | — |

`ModelRead::adapter` (`hf_schema::AdapterSource`) says which. A configuration read without tensor
names is Level A *unconfirmed*: what a config alone cannot say (does a projection carry a bias? is there
a q/k norm?) is read from the tensor names when they are given and otherwise assumed
(`ModelRead::assumed_defaults` lists every assumption and every class default used).

## 3. The adapter file

```jsonc
{
  "format": "misaka.palw.model-adapter.v1",
  "id": "my-model",                          // 1..=128 chars
  "doc": "…",
  "extends": ["decoder-core"],               // built-in adapters merged first, later overriding earlier
  "match": { "architectures": ["MyForCausalLM"], "model_types": ["my_model"] },
  "remote_code": ["modeling_my.MyForCausalLM"],   // auto_map modules this adapter models
  "remote_code_required": false,             // true, or a list of architectures that exist only as remote code
  "config": {
    "decoder": "text_config",                // the decoder's config lives in this nested object (VLMs)
    "defaults": { "rms_norm_eps": 1e-5 },    // the HF config class's own defaults
    "inert": ["some_training_only_key"],     // keys known not to change the forward pass
    "root_inert": []                         // the same for the wrapper's own keys
  },
  "vars": [ { "name": "heads", "value": { "$cfg": "num_attention_heads" } } ],
  "spec": { /* a ModelSpec (§5) whose values may be operator nodes */ },
  "refuse": [ { "architectures": ["X"], "missing": ["ATTN_CROSS_V1"], "why": "…" } ]   // a refusal, by named feature
}
```

* **Variables** are evaluated in dependency order (a variable may use one defined later). Every
  variable is evaluated — its checks run and the keys it reads are marked used — unless it says
  `"lazy": true`. `"layer": true` makes a variable per-layer (evaluated for each layer of a `$layers`,
  with `i` bound). Redefining a name in an extending adapter replaces it in place.
* **Merging** (`extends`): objects merge key by key (later wins); arrays and scalars are replaced; the
  `inert` / `root_inert` lists accumulate; `vars` merge by `name`; `{"$unset": true}` deletes a key.
* **Identity**: `BLAKE2b-512`, keyed `misaka-palw/model-adapter/v1`, over the *canonical JSON* of the
  *effective* adapter (after `extends`): keys sorted, no whitespace, integral floats written as
  integers. The hash pins behaviour, including every built-in the file extends. The built-in pack's
  hash (`adapter::builtin::pack_hash`, over the sorted `(id, hash)` pairs) is what the runtime pack pins.
* **Safety**: an adapter is untrusted data. Evaluation is pure and bounded (4 M nodes, depth 96, lists
  of 2²⁰, 4096 layers); a key no rule reads is refused (`NOT_LOWERABLE`), never ignored; a file over
  1 MiB is refused.

## 4. The expression language

An object with **exactly one key that starts with `$`** is an operator node; every other object,
array and scalar is a literal (objects and arrays are evaluated element-wise), so a template for a
`ModelSpec` is a `ModelSpec` with operator nodes where a value depends on the configuration.

| group | operators |
| --- | --- |
| configuration | `$cfg` (`"key"` or `["key", default]`; class defaults from `config.defaults`), `$cfg?` (the configuration's own value or null), `$cfgn` (`usize_or_null`: absent → default, explicit `null` stays null), `$alias` (`[["k1","k2"], default]`), `$has`, `$root` (a VLM wrapper's key), `$forbid` (`["key", why]`), `$require_eq` (`["key", value, why]`), `$get` (`[object, "key", default?]`), `$scope` (`{key, inert, body}`: evaluate with the configuration narrowed to a nested object, with its own key tracking) |
| tensors (when a tensor index is given) | `$has_tensor` (true / false / null if unknown), `$tensor_flag` (`[name, default]`: present?, else the default, recorded as assumed), `$tensor_shape`, `$tensor_prefix` (`{candidates, probe, default}`), `$tensor_prefix_scan` (`{probe, exclude?, default}`: the one prefix the index stores `probe` under; two is a refusal) |
| variables | `$var`, `$let` (`["name", value, body]`) |
| arithmetic | `$add $sub $mul $div $idiv $mod $neg $abs $min $max $pow $sqrt $ln $exp $floor $ceil $round $int $float` (integers stay integers; `$div` is a float division) |
| logic | `$eq $ne $lt $le $gt $ge $and $or $not $if` (`[c, a, b]`) `$switch` (`[value, {case: result}, default]`) |
| lists, objects, strings | `$list $range $len $index $contains $concat $flatten $repeat $map` (`[list, "name", body]`) `$sum $cat $starts_with $ends_with $merge $omit $set` |
| checks | `$check` (`[cond, message]`: NOT_LOWERABLE if false), `$bad` (`[cond, message]`: bad config) |
| generic features | `$act` (an HF activation name → `Act`), `$rope` / `$rope_temp` (a rope spec / query temperature from the config's rope fields), `$rope_plain`, `$partial_rotary`, `$alibi` (slopes), `$layers` (`{count, each}`), `$layer_types` (`{n, allowed, each}`: the config's `layer_types`, validated, or the class's own rule) |

The generic features (`$act`, `$rope`, `$alibi`, …) are the places where a feature has Rust semantics
(an activation table, a rope-scaling formula, a slope rule). They are part of the vocabulary: adding
one is adding a feature, once, for everyone.

## 5. What an adapter produces: `ModelSpec` V1

The JSON schema of `spec::ModelSpec` (serde; additive-only changes with defaults, so a V1 adapter
keeps working). Its layers are fully expanded (`layers[i]`); the HL builder groups equal layers into
block kinds. `ModelSpec::features()` lists the features it uses; each [`FeatureInfo`](../../../../misaka-palw-tir-lower/src/model/features.rs)
states its lowering status, the primitives it emits (checked against every lowered fixture), its protocol
requirement (none, or a named capability with the general primitive that would close it) and its tests.

## 6. The built-in pack

`adapters/*.json`: `decoder-core` (the strict Llama-lineage decoder every family adapter extends: it reads
only the keys a Llama reads, so an adapter is exactly as permissive as its class), `standard-decoder` (the
Level A template: core + sliding window, partial rotary, tensor-driven biases and q/k norm, multipliers,
soft-caps), `spec-frame` (the shape of every decoder-shaped spec), the mixins (`mixin-*`), the family
adapters — **every architecture this crate lowers is one**: the Llama lineage and its relatives, the
MoEs (Mixtral, Qwen-MoE, OLMoE, Granite-MoE, DeepSeek-V2/V3 with MLA, GLM-4.5, Phi-3.5-MoE, Llama-4,
gpt-oss, Gemma-4 with per-layer inputs and KV sharing), the hybrids (Qwen3-Next/3.5, Jamba), the SSMs
(Mamba, Mamba-2, Falcon-Mamba, RWKV-4), the encoders (BERT, RoBERTa/XLM-R, MPNet, DistilBERT, CLIP's
text tower) and the vision-language wrappers — and `refusals` (architectures refused on purpose, by the
features they lack, RWKV-5/6/7 among them).

Every family adapter is held to the `ModelSpec` the per-architecture Rust parser it replaced produced,
by the differential oracle `tests/adapters.rs` (cargo feature `legacy-oracle`): on 224 fixtures and
published configs and on ~15 000 single-key *mutants* of them (every key deleted, nulled, flipped,
nudged, rewritten or truncated, at the top level and one object down — plus an unknown key), the adapter
and the Rust parser either read the same spec or both refuse. Where an adapter refuses a mutant the
parser read, the refusal comes from the generic checks of the evaluator (a head count that does not
divide the width, an odd rotary dimension, a division by zero, a non-finite number, a decoder with no
layer): stricter, never looser. `tests/golden_lowering.rs` holds the lowered programs and artifacts
(92 fixtures, 63 real configs) byte-identical to the baseline recorded before the refactor; it is the
permanent gate, and the Rust parsers (`src/hf_config/legacy_oracle`, compiled only with the feature) can
be deleted once the corpus lane has validated the pack.

## 7. Writing an adapter

1. `palw-class check-architecture <hf dir>`: if it says Level A you are done.
2. Otherwise copy the closest built-in, set `match`, the class defaults (`config.defaults`), the inert keys,
   and override the variables your class departs in (`norm`, `residual`, `names`, `l_mixer`, `l_ffn`, …).
3. If a key changes the math and no variable can express it, the gap is a **feature**: name it, add it
   generically (a `FeatureId`, a spec field with a default, a lowerer) — never family code.
